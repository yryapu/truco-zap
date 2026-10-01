//! truco-zap — Truco Paulista jogável na web.
//!
//! Servidor autoritativo: o cliente nunca decide nada de regra. Um processo, uma porta,
//! HTTP e WebSocket juntos (decisão D01).

mod auth;
mod db;
mod emblemas;
mod hub;
mod webhooks;

use auth::Autenticado;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use futures::{SinkExt, StreamExt};
use hub::{Hub, Mesa, Modo};
use rand::rngs::StdRng;
use rand::SeedableRng;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use truco_core::{time_do_assento, Fase, Match};

#[derive(Clone)]
pub struct AppState {
    pub pool: db::Pool,
    pub hub: Arc<Mutex<Hub>>,
    pub http: reqwest::Client,
    pub wcfg: webhooks::Config,
    /// Marca o cookie como `Secure`. Fica desligado no Docker local (sem TLS).
    pub cookie_seguro: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,truco_server=debug".into()),
        )
        .init();

    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://data/truco.db".into());
    if let Some(dir) = url
        .strip_prefix("sqlite://")
        .and_then(|p| std::path::Path::new(p).parent())
    {
        let _ = std::fs::create_dir_all(dir);
    }
    let pool = db::abrir(&url).await?;

    // Mitigação do risco de D05: aposta travada por crash volta para o jogador.
    match db::estornar_partidas_orfas(&pool).await {
        Ok(0) => {}
        Ok(n) => tracing::warn!("estornei {n} partida(s) orfa(s) do restart anterior"),
        Err(e) => tracing::error!("estorno de orfas falhou: {e}"),
    }

    let st = AppState {
        pool,
        hub: Arc::new(Mutex::new(Hub::default())),
        http: reqwest::Client::builder()
            .user_agent("truco-zap/1.0")
            .redirect(reqwest::redirect::Policy::none()) // redirect e rota de fuga de SSRF
            .build()?,
        wcfg: webhooks::Config::do_ambiente(),
        cookie_seguro: std::env::var("TRUCO_COOKIE_SECURE")
            .map(|v| v == "1")
            .unwrap_or(false),
    };

    tokio::spawn(webhooks::trabalhador(
        st.pool.clone(),
        st.http.clone(),
        st.wcfg.clone(),
    ));
    tokio::spawn(ticker(st.clone()));

    let estaticos = std::env::var("TRUCO_STATIC").unwrap_or_else(|_| "static".into());
    let app = Router::new()
        .route("/api/saude", get(saude))
        .route("/api/cadastro", post(cadastro))
        .route("/api/convidado", post(convidado))
        .route("/api/entrar", post(entrar))
        .route("/api/sair", post(sair))
        .route("/api/eu", get(eu))
        .route("/api/ranking", get(ranking))
        .route("/api/historico", get(historico))
        .route("/api/webhooks", get(listar_webhooks).post(criar_webhook))
        .route("/api/webhooks/{id}", delete(apagar_webhook))
        .route("/ws", get(ws_entrada))
        .fallback_service(
            tower_http::services::ServeDir::new(&estaticos).append_index_html_on_directories(true),
        )
        .layer(tower_http::limit::RequestBodyLimitLayer::new(32 * 1024))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(st);

    let porta: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", porta)).await?;
    tracing::info!("truco-zap ouvindo em http://0.0.0.0:{porta}");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("encerrando");
        })
        .await?;
    Ok(())
}

// ------------------------------------------------------------------ rotas HTTP

type Resposta = Result<Response, (StatusCode, Json<Value>)>;

fn ruim(codigo: &str) -> (StatusCode, Json<Value>) {
    (StatusCode::BAD_REQUEST, Json(json!({"erro": codigo})))
}

async fn saude(State(st): State<AppState>) -> Resposta {
    let mesas = st.hub.lock().await.mesas.len();
    Ok(
        Json(json!({"ok": true, "mesas_ativas": mesas, "versao": env!("CARGO_PKG_VERSION")}))
            .into_response(),
    )
}

#[derive(Deserialize)]
struct Credenciais {
    apelido: String,
    senha: String,
}

fn com_cookie(st: &AppState, token: &str, corpo: Value) -> Response {
    let mut h = HeaderMap::new();
    h.insert(
        header::SET_COOKIE,
        auth::cookie_de_sessao(token, st.cookie_seguro)
            .parse()
            .expect("cookie ascii"),
    );
    (h, Json(corpo)).into_response()
}

async fn abrir_sessao(
    st: &AppState,
    j: &db::Jogador,
) -> Result<Response, (StatusCode, Json<Value>)> {
    let token = auth::token_novo();
    db::criar_sessao(&st.pool, &auth::hash_token(&token), &j.id)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"erro":"banco"})),
            )
        })?;
    Ok(com_cookie(
        st,
        &token,
        json!({"jogador": hub::perfil_json(j)}),
    ))
}

async fn cadastro(State(st): State<AppState>, Json(c): Json<Credenciais>) -> Resposta {
    if !auth::apelido_valido(&c.apelido) {
        return Err(ruim("apelido_invalido"));
    }
    if !auth::senha_valida(&c.senha) {
        return Err(ruim("senha_curta"));
    }
    criar_e_entrar(&st, &c.apelido, &c.senha, false).await
}

/// O caminho de "jogando em menos de um minuto": um clique, sem formulário.
async fn convidado(State(st): State<AppState>) -> Resposta {
    for _ in 0..5 {
        let n: u32 = rand::Rng::gen_range(&mut rand::thread_rng(), 1000..10000);
        let apelido = format!("Visitante {n}");
        let senha = auth::token_novo();
        match criar_e_entrar(&st, &apelido, &senha, true).await {
            Ok(r) => return Ok(r),
            Err(_) => continue, // colisão de apelido; tenta outro
        }
    }
    Err(ruim("nao_consegui_criar_convidado"))
}

async fn criar_e_entrar(st: &AppState, apelido: &str, senha: &str, convidado: bool) -> Resposta {
    let hash = auth::hash_senha(senha).map_err(|_| ruim("hash"))?;
    let id = uuid::Uuid::new_v4().to_string();
    db::criar_jogador(&st.pool, &id, apelido, &hash, convidado)
        .await
        .map_err(|_| ruim("apelido_em_uso"))?;
    let j = db::jogador_por_id(&st.pool, &id)
        .await
        .map_err(|_| ruim("banco"))?
        .ok_or_else(|| ruim("banco"))?;
    abrir_sessao(st, &j).await
}

async fn entrar(State(st): State<AppState>, Json(c): Json<Credenciais>) -> Resposta {
    let j = db::jogador_por_apelido(&st.pool, &c.apelido)
        .await
        .map_err(|_| ruim("banco"))?;
    // Mesma resposta para apelido inexistente e senha errada: não entrego enumeração de contas.
    let erro = || {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({"erro": "credenciais_invalidas"})),
        )
    };
    let j = j.ok_or_else(erro)?;
    if !auth::confere_senha(&c.senha, &j.senha_hash) {
        return Err(erro());
    }
    abrir_sessao(&st, &j).await
}

async fn sair(State(st): State<AppState>, headers: HeaderMap) -> Resposta {
    if let Some(raw) = headers.get(header::COOKIE).and_then(|v| v.to_str().ok()) {
        if let Some(tok) = raw
            .split(';')
            .filter_map(|kv| kv.split_once('='))
            .find(|(k, _)| k.trim() == auth::COOKIE)
            .map(|(_, v)| v.trim())
        {
            let _ = db::apagar_sessao(&st.pool, &auth::hash_token(tok)).await;
        }
    }
    let mut h = HeaderMap::new();
    h.insert(
        header::SET_COOKIE,
        auth::cookie_vazio().parse().expect("cookie ascii"),
    );
    Ok((h, Json(json!({"ok": true}))).into_response())
}

async fn eu(Autenticado(j): Autenticado) -> Resposta {
    Ok(Json(json!({"jogador": hub::perfil_json(&j)})).into_response())
}

/// Ranking público, mais a linha de quem está pedindo — ela vem sempre, esteja ele no topo
/// ou em 300º. Sem isso o jogador não se acha na própria classificação (ver ERROS.md E16).
async fn ranking(State(st): State<AppState>, headers: HeaderMap) -> Resposta {
    let linhas = db::ranking(&st.pool, 25).await.map_err(|_| ruim("banco"))?;
    let eu = match jogador_do_cookie(&st, &headers).await {
        Some(j) => db::posicao_de(&st.pool, &j.id).await.ok().flatten(),
        None => None,
    };
    Ok(Json(json!({"ranking": linhas, "eu": eu, "total_mostrado": linhas.len()})).into_response())
}

/// Resolve o jogador pelo cookie **sem** falhar quando não há sessão. Usado por rotas
/// públicas que ficam melhores quando sabem quem está olhando.
async fn jogador_do_cookie(st: &AppState, headers: &HeaderMap) -> Option<db::Jogador> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    let tok = raw
        .split(';')
        .filter_map(|kv| kv.split_once('='))
        .find(|(k, _)| k.trim() == auth::COOKIE)
        .map(|(_, v)| v.trim())?;
    db::jogador_por_token_hash(&st.pool, &auth::hash_token(tok))
        .await
        .ok()
        .flatten()
}

async fn historico(State(st): State<AppState>, Autenticado(j): Autenticado) -> Resposta {
    let h = db::historico(&st.pool, &j.id, 25)
        .await
        .map_err(|_| ruim("banco"))?;
    Ok(Json(json!({"historico": h})).into_response())
}

async fn listar_webhooks(State(st): State<AppState>, Autenticado(j): Autenticado) -> Resposta {
    let w = db::webhooks_de(&st.pool, &j.id)
        .await
        .map_err(|_| ruim("banco"))?;
    // O segredo não volta na listagem: ele é mostrado uma vez, no registro.
    Ok(Json(json!({
        "webhooks": w.iter().map(|x| json!({"id": x.id, "url": x.url})).collect::<Vec<_>>(),
        "eventos": webhooks::EVENTOS,
    }))
    .into_response())
}

#[derive(Deserialize)]
struct NovoWebhook {
    url: String,
}

async fn criar_webhook(
    State(st): State<AppState>,
    Autenticado(j): Autenticado,
    Json(n): Json<NovoWebhook>,
) -> Resposta {
    // Valida já no registro para dar erro útil, e de novo na entrega (DNS pode mudar).
    if let Err(motivo) = webhooks::url_permitida(&n.url, &st.wcfg).await {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"erro":"url_recusada","motivo":motivo})),
        ));
    }
    let existentes = db::webhooks_de(&st.pool, &j.id)
        .await
        .map_err(|_| ruim("banco"))?;
    if existentes.len() >= 5 {
        return Err(ruim("limite_de_webhooks"));
    }
    let id = uuid::Uuid::new_v4().to_string();
    let segredo = auth::token_novo();
    db::registrar_webhook(&st.pool, &id, &j.id, &n.url, &segredo)
        .await
        .map_err(|_| ruim("banco"))?;
    Ok(Json(json!({
        "id": id, "url": n.url, "segredo": segredo,
        "assinatura": "HMAC-SHA256 do corpo, header X-Truco-Signature: sha256=<hex>",
        "eventos": webhooks::EVENTOS,
    }))
    .into_response())
}

async fn apagar_webhook(
    State(st): State<AppState>,
    Autenticado(j): Autenticado,
    Path(id): Path<String>,
) -> Resposta {
    let n = db::remover_webhook(&st.pool, &id, &j.id)
        .await
        .map_err(|_| ruim("banco"))?;
    if n == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({"erro":"nao_encontrado"})),
        ));
    }
    Ok(Json(json!({"ok": true})).into_response())
}

// -------------------------------------------------------------------- WebSocket

async fn ws_entrada(
    ws: WebSocketUpgrade,
    State(st): State<AppState>,
    Autenticado(j): Autenticado,
) -> Response {
    ws.on_upgrade(move |socket| conexao(socket, st, j))
}

async fn conexao(socket: WebSocket, st: AppState, jogador: db::Jogador) {
    let (mut saida, mut entrada) = socket.split();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    let escritor = tokio::spawn(async move {
        while let Some(txt) = rx.recv().await {
            if saida.send(Message::Text(txt.into())).await.is_err() {
                break;
            }
        }
    });

    let conn = {
        let mut h = st.hub.lock().await;
        let conn = h.registrar(jogador.id.clone(), jogador.apelido.clone(), tx);
        h.enviar(
            conn,
            json!({"t":"ola","jogador": hub::perfil_json(&jogador)}),
        );
        if let Some(mid) = h.mesa_do_jogador.get(&jogador.id).cloned() {
            if let Some(mesa) = h.mesas.get(&mid) {
                if let Some(assento) = assento_de(mesa, &jogador.id) {
                    h.enviar(conn, hub::estado_view(mesa, assento));
                }
            }
        }
        conn
    };

    while let Some(Ok(msg)) = entrada.next().await {
        let txt = match msg {
            Message::Text(t) => t.to_string(),
            Message::Close(_) => break,
            _ => continue,
        };
        match serde_json::from_str::<hub::Comando>(&txt) {
            Ok(cmd) => tratar(&st, conn, &jogador.id, cmd).await,
            Err(_) => st.hub.lock().await.erro(conn, "comando_invalido"),
        }
    }

    st.hub.lock().await.desregistrar(conn);
    escritor.abort();
}

fn assento_de(mesa: &Mesa, jogador_id: &str) -> Option<usize> {
    mesa.assentados
        .iter()
        .position(|a| a.jogador_id.as_deref() == Some(jogador_id))
}

async fn tratar(st: &AppState, conn: hub::ConnId, jogador_id: &str, cmd: hub::Comando) {
    match cmd {
        hub::Comando::Ping => st.hub.lock().await.enviar(conn, json!({"t":"pong"})),
        hub::Comando::SairFila => {
            let mut h = st.hub.lock().await;
            for fila in h.filas.values_mut() {
                fila.retain(|e| e.conn != conn);
            }
            h.enviar(conn, json!({"t":"fila_saiu"}));
        }
        hub::Comando::EntrarFila { modo, aposta } => {
            // Saldo é lido do banco, nunca do cliente.
            let Ok(Some(j)) = db::jogador_por_id(&st.pool, jogador_id).await else {
                st.hub.lock().await.erro(conn, "jogador_desconhecido");
                return;
            };
            if aposta < 0 || aposta > j.saldo {
                st.hub.lock().await.erro(conn, "aposta_invalida");
                return;
            }
            let mut h = st.hub.lock().await;
            if h.mesa_do_jogador.contains_key(jogador_id) {
                h.erro(conn, "ja_esta_em_partida");
                return;
            }
            for fila in h.filas.values_mut() {
                fila.retain(|e| e.conn != conn);
            }
            let fila = h.filas.entry((modo, aposta)).or_default();
            fila.push(hub::Espera {
                conn,
                jogador_id: jogador_id.to_string(),
                apelido: j.apelido.clone(),
                desde: Instant::now(),
            });
            let n = fila.len();
            h.enviar(
                conn,
                json!({"t":"fila","modo":modo.texto(),"aposta":aposta,"na_fila":n,
                       "faltam":modo.jogadores().saturating_sub(n),
                       "robos_em_segundos": hub::ESPERA_ATE_ROBO.as_secs()}),
            );
        }
        outro => {
            let acao = match outro.para_acao() {
                Some(Ok(a)) => a,
                Some(Err(codigo)) => return st.hub.lock().await.erro(conn, codigo),
                None => return st.hub.lock().await.erro(conn, "comando_invalido"),
            };
            let mut h = st.hub.lock().await;
            let Some(mid) = h.mesa_do_jogador.get(jogador_id).cloned() else {
                return h.erro(conn, "nao_esta_em_partida");
            };
            let Some(mesa) = h.mesas.get_mut(&mid) else {
                return h.erro(conn, "nao_esta_em_partida");
            };
            let Some(assento) = assento_de(mesa, jogador_id) else {
                return h.erro(conn, "nao_esta_em_partida");
            };
            match mesa.m.aplicar(assento, acao) {
                Ok(eventos) => {
                    let eventos: Vec<Value> = eventos.iter().map(hub::evento_json).collect();
                    mesa.reagendar();
                    let mesa = h.mesas.get(&mid).expect("acabei de pegar essa mesa");
                    for ev in eventos {
                        h.transmitir(mesa, json!({"t":"evento","evento":ev}));
                    }
                    h.transmitir_estado(mesa);
                }
                Err(e) => h.erro(conn, hub::codigo_do_erro(e)),
            }
        }
    }
}

// ----------------------------------------------------------------------- ticker

struct Abertura {
    modo: Modo,
    aposta: i64,
    humanos: Vec<(hub::ConnId, String, String)>,
}

async fn ticker(st: AppState) {
    let mut rng = StdRng::from_entropy();
    loop {
        tokio::time::sleep(Duration::from_millis(250)).await;

        let aberturas = {
            let mut h = st.hub.lock().await;
            coletar_aberturas(&mut h)
        };
        for ab in aberturas {
            abrir_mesa(&st, ab).await;
        }

        {
            let mut h = st.hub.lock().await;
            passo_das_mesas(&mut h, &mut rng);
        }

        let fins = {
            let mut h = st.hub.lock().await;
            coletar_fins(&mut h)
        };
        for f in fins {
            finalizar(&st, f).await;
        }
    }
}

fn coletar_aberturas(h: &mut Hub) -> Vec<Abertura> {
    let mut out = Vec::new();
    for ((modo, aposta), fila) in h.filas.iter_mut() {
        let n = modo.jogadores();
        while fila.len() >= n {
            let humanos = fila
                .drain(..n)
                .map(|e| (e.conn, e.jogador_id, e.apelido))
                .collect();
            out.push(Abertura {
                modo: *modo,
                aposta: *aposta,
                humanos,
            });
        }
        // Casa vazia: completa com robôs para que ninguém espere indefinidamente (D15).
        if let Some(mais_antigo) = fila.first() {
            if mais_antigo.desde.elapsed() >= hub::ESPERA_ATE_ROBO {
                let humanos = fila
                    .drain(..)
                    .map(|e| (e.conn, e.jogador_id, e.apelido))
                    .collect();
                out.push(Abertura {
                    modo: *modo,
                    aposta: *aposta,
                    humanos,
                });
            }
        }
    }
    h.filas.retain(|_, f| !f.is_empty());
    out
}

async fn abrir_mesa(st: &AppState, ab: Abertura) {
    let n = ab.modo.jogadores();
    let partida_id = uuid::Uuid::new_v4().to_string();
    let assentos: Vec<(usize, u8, Option<String>)> = (0..n)
        .map(|i| {
            (
                i,
                time_do_assento(i),
                ab.humanos.get(i).map(|(_, id, _)| id.clone()),
            )
        })
        .collect();

    let bolo = match db::abrir_partida(&st.pool, &partida_id, ab.modo.texto(), ab.aposta, &assentos)
        .await
    {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("nao abri partida: {e}");
            let h = st.hub.lock().await;
            for (c, _, _) in &ab.humanos {
                h.erro(*c, "saldo_insuficiente");
            }
            return;
        }
    };

    let mut rng = StdRng::from_entropy();
    let m = Match::novo(n, &mut rng);
    let assentados: Vec<hub::Assentado> = (0..n)
        .map(|i| match ab.humanos.get(i) {
            Some((c, id, apelido)) => hub::Assentado {
                jogador_id: Some(id.clone()),
                apelido: apelido.clone(),
                conn: Some(*c),
            },
            None => hub::Assentado {
                jogador_id: None,
                apelido: format!("Robô {}", i + 1),
                conn: None,
            },
        })
        .collect();

    let mut mesa = Mesa {
        id: partida_id.clone(),
        modo: ab.modo,
        aposta: ab.aposta,
        bolo,
        assentados,
        m,
        finalizando: false,
        agir_em: Instant::now() + Mesa::pausa_do_robo(),
    };
    mesa.reagendar();

    let evento_inicial = {
        let mut h = st.hub.lock().await;
        for (_, id, _) in &ab.humanos {
            h.mesa_do_jogador.insert(id.clone(), partida_id.clone());
        }
        let corpo = json!({
            "evento": "partida.comecou",
            "partida_id": partida_id,
            "modo": ab.modo.texto(),
            "aposta": ab.aposta,
            "jogadores": mesa.assentados.iter().enumerate().map(|(i, a)| json!({
                "assento": i, "time": time_do_assento(i), "apelido": a.apelido,
                "robo": a.jogador_id.is_none()
            })).collect::<Vec<_>>(),
        });
        h.transmitir(
            &mesa,
            json!({"t":"mesa","partida_id":partida_id,"modo":ab.modo.texto(),
                                  "aposta":ab.aposta,"bolo":bolo}),
        );
        h.transmitir_estado(&mesa);
        h.mesas.insert(partida_id.clone(), mesa);
        corpo
    };

    let ids: Vec<String> = ab.humanos.iter().map(|(_, id, _)| id.clone()).collect();
    enfileirar(st, &ids, "partida.comecou", evento_inicial).await;
}

/// Quem o servidor joga por conta própria: robô, ou humano desconectado (senão a mesa travava).
fn ator_automatico(mesa: &Mesa) -> Option<usize> {
    let automatico =
        |i: usize| mesa.assentados[i].jogador_id.is_none() || mesa.assentados[i].conn.is_none();
    match mesa.m.fase {
        Fase::Jogando => {
            let v = mesa.m.mao.vez;
            automatico(v).then_some(v)
        }
        Fase::AguardandoResposta { respondendo, .. }
        | Fase::DecisaoMaoDeOnze { time: respondendo } => {
            let do_time: Vec<usize> = (0..mesa.assentados.len())
                .filter(|i| time_do_assento(*i) == respondendo)
                .collect();
            // Só age sozinho se NENHUM humano online pode responder.
            do_time.iter().all(|i| automatico(*i)).then(|| do_time[0])
        }
        _ => None,
    }
}

fn passo_das_mesas(h: &mut Hub, rng: &mut StdRng) {
    let agora = Instant::now();
    let ids: Vec<String> = h
        .mesas
        .iter()
        .filter(|(_, m)| agora >= m.agir_em && !m.finalizando)
        .map(|(k, _)| k.clone())
        .collect();

    for id in ids {
        let Some(mesa) = h.mesas.get_mut(&id) else {
            continue;
        };
        let eventos: Vec<Value> = match mesa.m.fase {
            Fase::MaoEncerrada => {
                let evs = mesa.m.nova_mao(rng);
                evs.iter().map(hub::evento_json).collect()
            }
            Fase::PartidaEncerrada { .. } => continue, // coletar_fins trata
            _ => {
                let Some(assento) = ator_automatico(mesa) else {
                    continue;
                };
                let acao = hub::acao_do_robo(&mesa.m, assento, rng);
                match mesa.m.aplicar(assento, acao) {
                    Ok(evs) => evs.iter().map(hub::evento_json).collect(),
                    Err(e) => {
                        tracing::error!("robo fez jogada invalida ({e:?}) na mesa {id}");
                        continue;
                    }
                }
            }
        };
        mesa.reagendar();
        let mesa = h.mesas.get(&id).expect("mesa existe");
        for ev in eventos {
            h.transmitir(mesa, json!({"t":"evento","evento":ev}));
        }
        h.transmitir_estado(mesa);
    }
}

struct Fim {
    partida_id: String,
    vencedor: u8,
    placar: [u8; 2],
    bolo: i64,
    modo: String,
    aposta: i64,
    /// (jogador_id, time, apelido)
    humanos: Vec<(String, u8, String)>,
}

fn coletar_fins(h: &mut Hub) -> Vec<Fim> {
    let mut out = Vec::new();
    for mesa in h.mesas.values_mut() {
        if mesa.finalizando {
            continue;
        }
        if let Fase::PartidaEncerrada { vencedor } = mesa.m.fase {
            mesa.finalizando = true;
            out.push(Fim {
                partida_id: mesa.id.clone(),
                vencedor,
                placar: mesa.m.placar,
                bolo: mesa.bolo,
                modo: mesa.modo.texto().to_string(),
                aposta: mesa.aposta,
                humanos: mesa
                    .assentados
                    .iter()
                    .enumerate()
                    .filter_map(|(i, a)| {
                        a.jogador_id
                            .clone()
                            .map(|id| (id, time_do_assento(i), a.apelido.clone()))
                    })
                    .collect(),
            });
        }
    }
    out
}

async fn finalizar(st: &AppState, f: Fim) {
    // D15: o bolo é só o que humanos apostaram, e é dividido entre os humanos vencedores.
    // Robô não aposta, então não há como imprimir moeda ganhando de robô.
    let vencedores: Vec<&(String, u8, String)> = f
        .humanos
        .iter()
        .filter(|(_, t, _)| *t == f.vencedor)
        .collect();
    let por_cabeca = if vencedores.is_empty() {
        0
    } else {
        f.bolo / vencedores.len() as i64
    };

    let premiacoes: Vec<db::Premiacao> = f
        .humanos
        .iter()
        .map(|(id, t, _)| db::Premiacao {
            jogador_id: id.clone(),
            premio: if *t == f.vencedor { por_cabeca } else { 0 },
            venceu: *t == f.vencedor,
        })
        .collect();

    let corpo_resultado = json!({
        "partida_id": f.partida_id, "modo": f.modo, "aposta": f.aposta, "bolo": f.bolo,
        "placar": f.placar, "time_vencedor": f.vencedor,
        "jogadores": f.humanos.iter().map(|(_, t, ap)| json!({
            "apelido": ap, "time": t, "venceu": *t == f.vencedor,
            "premio": if *t == f.vencedor { por_cabeca } else { 0 }
        })).collect::<Vec<_>>(),
    });

    let ids: Vec<String> = f.humanos.iter().map(|(id, _, _)| id.clone()).collect();
    let mut entregas = Vec::new();
    for id in &ids {
        if let Ok(ws) = db::webhooks_de(&st.pool, id).await {
            for w in ws {
                for evento in ["partida.terminou", "partida.resultado"] {
                    let eid = uuid::Uuid::new_v4().to_string();
                    let mut corpo = corpo_resultado.clone();
                    corpo["evento"] = json!(evento);
                    corpo["evento_id"] = json!(eid);
                    entregas.push((eid, w.id.clone(), evento.to_string(), corpo.to_string()));
                }
            }
        }
    }

    let saldos = match db::fechar_partida(
        &st.pool,
        &f.partida_id,
        f.placar,
        f.vencedor,
        &premiacoes,
        &entregas,
    )
    .await
    {
        Ok(s) => s.into_iter().collect::<HashMap<_, _>>(),
        Err(e) => {
            tracing::error!("falha ao fechar partida {}: {e}", f.partida_id);
            HashMap::new()
        }
    };

    let mut h = st.hub.lock().await;
    if let Some(mesa) = h.mesas.remove(&f.partida_id) {
        for (i, a) in mesa.assentados.iter().enumerate() {
            let Some(id) = &a.jogador_id else { continue };
            h.mesa_do_jogador.remove(id);
            if let Some(c) = a.conn {
                h.enviar(
                    c,
                    json!({
                        "t": "fim",
                        "partida_id": f.partida_id,
                        "vencedor": f.vencedor,
                        "venceu": time_do_assento(i) == f.vencedor,
                        "placar": f.placar,
                        "premio": if time_do_assento(i) == f.vencedor { por_cabeca } else { 0 },
                        "saldo": saldos.get(id).copied(),
                    }),
                );
            }
        }
    }
}

async fn enfileirar(st: &AppState, jogadores: &[String], evento: &str, mut corpo: Value) {
    let mut entregas = Vec::new();
    for id in jogadores {
        if let Ok(ws) = db::webhooks_de(&st.pool, id).await {
            for w in ws {
                let eid = uuid::Uuid::new_v4().to_string();
                corpo["evento_id"] = json!(eid);
                entregas.push((eid, w.id, evento.to_string(), corpo.to_string()));
            }
        }
    }
    if !entregas.is_empty() {
        if let Err(e) = db::enfileirar_entregas(&st.pool, &entregas).await {
            tracing::error!("nao enfileirei {evento}: {e}");
        }
    }
}
