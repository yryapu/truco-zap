//! Lobby, mesas vivas e o protocolo WebSocket.
//!
//! Estado de mesa em memória sob um `Mutex` (decisão D05): o motor é CPU puro e o lock
//! nunca é mantido através de um `await`. Envio em canal é síncrono e não bloqueia, então
//! broadcast dentro do lock é seguro.
//!
//! No protocolo, **a carta é o caractere Unicode** (decisão D04).

use crate::db;
use crate::emblemas;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::UnboundedSender;
use truco_core::card::CARTA_DE_COSTAS;
use truco_core::{time_do_assento, Acao, Card, Evento, Fase, Match, TipoMao};

pub type ConnId = u64;

/// Quanto tempo um jogador espera na fila antes de a mesa ser completada com robôs.
/// Existe para que "começa a jogar em menos de um minuto" seja verdade com a casa vazia
/// (decisão D15).
pub const ESPERA_ATE_ROBO: Duration = Duration::from_secs(8);
const PAUSA_ENTRE_MAOS: Duration = Duration::from_millis(1200);
const PAUSA_DO_ROBO: Duration = Duration::from_millis(900);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum Modo {
    #[serde(rename = "1x1")]
    UmXUm,
    #[serde(rename = "2x2")]
    DoisXDois,
}

impl Modo {
    pub fn jogadores(self) -> usize {
        match self {
            Modo::UmXUm => 2,
            Modo::DoisXDois => 4,
        }
    }
    pub fn texto(self) -> &'static str {
        match self {
            Modo::UmXUm => "1x1",
            Modo::DoisXDois => "2x2",
        }
    }
}

pub struct Conn {
    pub jogador_id: String,
    pub apelido: String,
    pub tx: UnboundedSender<String>,
}

pub struct Assentado {
    /// `None` = robô.
    pub jogador_id: Option<String>,
    pub apelido: String,
    pub conn: Option<ConnId>,
}

pub struct Mesa {
    pub id: String,
    pub modo: Modo,
    pub aposta: i64,
    /// Soma do que os humanos apostaram. Robô não aposta (D15), então o bolo é o teto do
    /// que pode ser pago — não existe caminho para imprimir moeda.
    pub bolo: i64,
    pub assentados: Vec<Assentado>,
    pub m: Match,
    /// Marcada quando a finalização (banco + webhook) já foi disparada, para não rodar duas vezes.
    pub finalizando: bool,
    /// O ticker só age nesta mesa depois deste instante — é o que dá ritmo humano ao jogo.
    pub agir_em: Instant,
}

pub struct Espera {
    pub conn: ConnId,
    pub jogador_id: String,
    pub apelido: String,
    pub desde: Instant,
}

#[derive(Default)]
pub struct Hub {
    pub conns: HashMap<ConnId, Conn>,
    pub por_jogador: HashMap<String, ConnId>,
    pub filas: HashMap<(Modo, i64), Vec<Espera>>,
    pub mesas: HashMap<String, Mesa>,
    pub mesa_do_jogador: HashMap<String, String>,
    prox: ConnId,
}

impl Hub {
    pub fn registrar(&mut self, jogador_id: String, apelido: String, tx: UnboundedSender<String>) -> ConnId {
        self.prox += 1;
        let id = self.prox;
        // Uma conexão por jogador: a nova derruba a antiga. Evita duas abas disputando a vez.
        if let Some(antiga) = self.por_jogador.insert(jogador_id.clone(), id) {
            if let Some(c) = self.conns.remove(&antiga) {
                let _ = c.tx.send(json!({"t":"derrubado","motivo":"outra_conexao"}).to_string());
            }
        }
        self.conns.insert(id, Conn { jogador_id: jogador_id.clone(), apelido, tx });
        // Reconexão: se o jogador já está numa mesa, religa o assento a esta conexão.
        if let Some(mid) = self.mesa_do_jogador.get(&jogador_id).cloned() {
            if let Some(mesa) = self.mesas.get_mut(&mid) {
                for a in mesa.assentados.iter_mut() {
                    if a.jogador_id.as_deref() == Some(jogador_id.as_str()) {
                        a.conn = Some(id);
                    }
                }
            }
        }
        id
    }

    pub fn desregistrar(&mut self, conn: ConnId) {
        let Some(c) = self.conns.remove(&conn) else { return };
        if self.por_jogador.get(&c.jogador_id) == Some(&conn) {
            self.por_jogador.remove(&c.jogador_id);
        }
        for fila in self.filas.values_mut() {
            fila.retain(|e| e.conn != conn);
        }
        for mesa in self.mesas.values_mut() {
            for a in mesa.assentados.iter_mut() {
                if a.conn == Some(conn) {
                    a.conn = None;
                }
            }
        }
    }

    pub fn enviar(&self, conn: ConnId, v: Value) {
        if let Some(c) = self.conns.get(&conn) {
            let _ = c.tx.send(v.to_string());
        }
    }

    pub fn erro(&self, conn: ConnId, codigo: &str) {
        self.enviar(conn, json!({"t":"erro","codigo":codigo}));
    }

    /// Manda a cada assento humano a visão dele da mesa. Cada jogador vê só a própria mão.
    pub fn transmitir_estado(&self, mesa: &Mesa) {
        for (assento, a) in mesa.assentados.iter().enumerate() {
            if let Some(c) = a.conn {
                self.enviar(c, estado_view(mesa, assento));
            }
        }
    }

    pub fn transmitir(&self, mesa: &Mesa, v: Value) {
        for a in &mesa.assentados {
            if let Some(c) = a.conn {
                self.enviar(c, v.clone());
            }
        }
    }
}

// ----------------------------------------------------------------- serialização

fn carta_str(c: Card) -> String {
    c.unicode().to_string()
}

/// Um evento do motor em JSON, com as cartas como caractere Unicode.
/// A encoberta sai como U+1F0A0 (PLAYING CARD BACK) — o caractere certo para "carta de costas".
pub fn evento_json(ev: &Evento) -> Value {
    match ev {
        Evento::MaoComecou { vira, tipo, valor, mao } => json!({
            "evento":"mao_comecou","vira":carta_str(*vira),
            "manilha_rank": carta_str(Card::new(vira.rank.proximo_ciclico(), truco_core::Suit::Paus)),
            "tipo":tipo,"valor":valor,"mao":mao
        }),
        Evento::CartaJogada(j) => json!({
            "evento":"carta_jogada","assento":j.assento,
            "carta": j.carta.map(carta_str).unwrap_or_else(|| CARTA_DE_COSTAS.to_string()),
            "encoberta":j.encoberta
        }),
        Evento::RodadaTerminou { indice, vencedor, proximo_a_puxar } => json!({
            "evento":"rodada_terminou","indice":indice,"vencedor":vencedor,
            "proximo_a_puxar":proximo_a_puxar
        }),
        Evento::Pedido { assento, time, proposto } => json!({
            "evento":"pedido","assento":assento,"time":time,"proposto":proposto
        }),
        Evento::PedidoAceito { valor } => json!({"evento":"pedido_aceito","valor":valor}),
        Evento::Correu { time_que_correu, pontos } => json!({
            "evento":"correu","time_que_correu":time_que_correu,"pontos":pontos
        }),
        Evento::MaoDeOnzeRecusada { time, pontos_para } => json!({
            "evento":"mao_de_onze_recusada","time":time,"pontos_para":pontos_para
        }),
        Evento::MaoTerminou { vencedor, pontos, placar } => json!({
            "evento":"mao_terminou","vencedor":vencedor,"pontos":pontos,"placar":placar
        }),
        Evento::PartidaTerminou { vencedor, placar } => json!({
            "evento":"partida_terminou","vencedor":vencedor,"placar":placar
        }),
    }
}

pub fn estado_view(mesa: &Mesa, assento: usize) -> Value {
    let m = &mesa.m;
    let minha_mao: Vec<String> = m.mao.cartas[assento].iter().copied().map(carta_str).collect();

    // R10: só na decisão da mão de onze a dupla vê a mão do parceiro. Em nenhum outro momento.
    let mao_do_parceiro = if m.pode_ver_cartas_do_parceiro(assento) && mesa.modo == Modo::DoisXDois {
        let parceiro = (assento + 2) % 4;
        Some(json!({
            "assento": parceiro,
            "cartas": m.mao.cartas[parceiro].iter().copied().map(carta_str).collect::<Vec<_>>()
        }))
    } else {
        None
    };

    let mesa_cartas: Vec<Value> = m
        .mao
        .jogadas
        .iter()
        .map(|j| {
            json!({
                "assento": j.assento,
                "carta": j.carta.map(carta_str).unwrap_or_else(|| CARTA_DE_COSTAS.to_string()),
                "encoberta": j.encoberta
            })
        })
        .collect();

    let opcoes: Vec<&str> = match m.fase {
        Fase::AguardandoResposta { proposto, .. } if m.pode_agir(assento) => {
            if proposto >= 12 {
                vec!["correr", "aceitar"]
            } else {
                vec!["correr", "aceitar", "aumentar"]
            }
        }
        Fase::DecisaoMaoDeOnze { .. } if m.pode_agir(assento) => {
            vec!["mao_de_onze_jogar", "mao_de_onze_correr"]
        }
        Fase::Jogando if m.pode_agir(assento) => {
            // R15/D10: sem truco em mão de onze ou de ferro; e sem pedir duas vezes seguidas.
            if m.mao.tipo == TipoMao::Normal
                && m.mao.ultimo_pedinte != Some(time_do_assento(assento))
                && m.mao.valor < 12
            {
                vec!["jogar", "pedir"]
            } else {
                vec!["jogar"]
            }
        }
        _ => vec![],
    };

    json!({
        "t": "estado",
        "partida_id": mesa.id,
        "modo": mesa.modo.texto(),
        "aposta": mesa.aposta,
        "assento": assento,
        "time": time_do_assento(assento),
        "jogadores": mesa.assentados.iter().enumerate().map(|(i, a)| json!({
            "assento": i, "time": time_do_assento(i), "apelido": a.apelido,
            "robo": a.jogador_id.is_none(), "online": a.conn.is_some() || a.jogador_id.is_none(),
            "cartas_na_mao": m.mao.cartas[i].len()
        })).collect::<Vec<_>>(),
        "placar": m.placar,
        "vira": carta_str(m.mao.vira),
        "tipo_mao": m.mao.tipo,
        "valor": m.mao.valor,
        "numero_da_mao": m.numero_da_mao,
        "rodadas": m.mao.rodadas,
        "mesa": mesa_cartas,
        "minha_mao": minha_mao,
        "mao_do_parceiro": mao_do_parceiro,
        "fase": m.fase,
        "vez": match m.fase { Fase::Jogando => Some(m.mao.vez), _ => None },
        "pode_agir": m.pode_agir(assento),
        "opcoes": opcoes,
        "encoberta_permitida": !m.mao.rodadas.is_empty(),
    })
}

// ----------------------------------------------------------------------- robô

/// Política do robô. Simples de propósito: ele é parceiro de treino, não adversário de
/// verdade. Nada aqui tenta ser bom — tenta ser legal (no sentido de "jogada válida").
pub fn acao_do_robo<R: rand::Rng>(m: &Match, assento: usize, rng: &mut R) -> Acao {
    let vira = m.mao.vira;
    let minha: Vec<Card> = m.mao.cartas[assento].clone();
    let forca_max = minha.iter().map(|c| c.forca(vira)).max().unwrap_or(0);
    let soma: u32 = minha.iter().map(|c| c.forca(vira) as u32).sum();
    let mao_boa = forca_max >= 10 || soma >= 24;

    match m.fase {
        Fase::DecisaoMaoDeOnze { .. } => {
            if mao_boa || rng.gen_bool(0.35) {
                Acao::MaoDeOnzeJogar
            } else {
                Acao::MaoDeOnzeCorrer
            }
        }
        Fase::AguardandoResposta { proposto, .. } => {
            if mao_boa && proposto < 12 && rng.gen_bool(0.3) {
                Acao::Aumentar
            } else if mao_boa || rng.gen_bool(0.4) {
                Acao::Aceitar
            } else {
                Acao::Correr
            }
        }
        _ => {
            // Pede truco de vez em quando, quando pode e quando a mão ajuda.
            if m.mao.tipo == TipoMao::Normal
                && m.mao.valor < 12
                && m.mao.ultimo_pedinte != Some(time_do_assento(assento))
                && mao_boa
                && rng.gen_bool(0.25)
            {
                return Acao::Pedir;
            }
            // Menor carta que ganha do que está na mesa; se não ganha de nada, a menor de todas.
            let melhor_na_mesa = m
                .mao
                .jogadas
                .iter()
                .filter_map(|j| j.carta.map(|c| c.forca(vira)))
                .max();
            let escolha = match melhor_na_mesa {
                Some(alvo) => minha
                    .iter()
                    .filter(|c| c.forca(vira) > alvo)
                    .min_by_key(|c| c.forca(vira))
                    .or_else(|| minha.iter().min_by_key(|c| c.forca(vira))),
                None => minha.iter().min_by_key(|c| c.forca(vira)),
            };
            Acao::Jogar {
                carta: *escolha.expect("robo sempre tem carta na fase de jogar"),
                encoberta: false,
            }
        }
    }
}

// --------------------------------------------------------- comandos do cliente

#[derive(Debug, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Comando {
    EntrarFila { modo: Modo, aposta: i64 },
    SairFila,
    /// A carta vem como o caractere Unicode. Qualquer coisa fora do baralho é rejeitada.
    Jogar { carta: String, #[serde(default)] encoberta: bool },
    Pedir,
    Aceitar,
    Correr,
    Aumentar,
    MaoDeOnze { jogar: bool },
    Ping,
}

impl Comando {
    /// Converte para a ação do motor. Devolve `None` para comandos que não são ação de jogo.
    pub fn para_acao(&self) -> Option<Result<Acao, &'static str>> {
        Some(match self {
            Comando::Jogar { carta, encoberta } => {
                let mut cs = carta.chars();
                match (cs.next(), cs.next()) {
                    (Some(c), None) => match Card::from_unicode(c) {
                        Some(carta) => Ok(Acao::Jogar { carta, encoberta: *encoberta }),
                        None => Err("carta_desconhecida"),
                    },
                    _ => Err("carta_desconhecida"),
                }
            }
            Comando::Pedir => Ok(Acao::Pedir),
            Comando::Aceitar => Ok(Acao::Aceitar),
            Comando::Correr => Ok(Acao::Correr),
            Comando::Aumentar => Ok(Acao::Aumentar),
            Comando::MaoDeOnze { jogar: true } => Ok(Acao::MaoDeOnzeJogar),
            Comando::MaoDeOnze { jogar: false } => Ok(Acao::MaoDeOnzeCorrer),
            _ => return None,
        })
    }
}

pub fn codigo_do_erro(e: truco_core::Erro) -> &'static str {
    use truco_core::Erro::*;
    match e {
        NaoEhSuaVez => "nao_eh_sua_vez",
        CartaNaoEstaNaSuaMao => "carta_nao_esta_na_sua_mao",
        EncobertaNaPrimeiraRodada => "encoberta_na_primeira_rodada",
        AcaoForaDeFase => "acao_fora_de_fase",
        TrucoProibidoMaoDeOnze => "truco_proibido_mao_de_onze",
        JaPediuEsperaResposta => "ja_pediu_espera_resposta",
        SeuTimeFezOUltimoPedido => "seu_time_fez_o_ultimo_pedido",
        ValorMaximoAtingido => "valor_maximo_atingido",
        PartidaEncerrada => "partida_encerrada",
    }
}

pub fn perfil_json(j: &db::Jogador) -> Value {
    json!({
        "id": j.id, "apelido": j.apelido, "saldo": j.saldo,
        "vitorias": j.vitorias, "derrotas": j.derrotas, "convidado": j.convidado == 1,
        "emblemas": emblemas::de(j),
    })
}

impl Mesa {
    pub fn pausa_do_robo() -> Duration {
        PAUSA_DO_ROBO
    }

    /// Dá ritmo ao jogo. Só importa quando o próximo a agir é automático (robô ou humano
    /// desconectado) ou quando a mão acabou e a próxima precisa ser distribuída — uma jogada
    /// humana nunca espera por isto.
    pub fn reagendar(&mut self) {
        let d = match self.m.fase {
            Fase::MaoEncerrada => PAUSA_ENTRE_MAOS,
            Fase::PartidaEncerrada { .. } => Duration::ZERO,
            _ => PAUSA_DO_ROBO,
        };
        self.agir_em = Instant::now() + d;
    }
}
