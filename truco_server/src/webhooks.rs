//! Webhooks: registro, guarda contra SSRF e entrega em background (decisão D12).
//!
//! Garantia: a entrega é enfileirada na **mesma transação** que grava o resultado da partida.
//! Se o resultado existe, o evento vai sair. At-least-once — o receptor precisa ser idempotente
//! pelo `evento_id` que vai no corpo.

use crate::db::{self, Pool};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

pub const EVENTOS: [&str; 3] = ["partida.comecou", "partida.terminou", "partida.resultado"];

pub fn assinar(segredo: &str, corpo: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(segredo.as_bytes())
        .expect("HMAC aceita chave de qualquer tamanho");
    mac.update(corpo.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Um webhook é "o servidor faz request para uma URL que o usuário escolheu" — SSRF por
/// construção. Esta função é a mitigação, e ela roda **na entrega**, não só no registro,
/// porque DNS pode mudar entre um e outro.
///
/// Devolve o endereço validado, e **não** apenas `Ok(())`, de propósito: validar o DNS e
/// depois deixar o cliente HTTP resolver de novo é um TOCTOU clássico (DNS rebinding) — um
/// resolvedor hostil devolve um IP público para a checagem e um privado para a conexão.
/// Quem chama tem de **fixar** este endereço na conexão. Ver `entregar_uma`.
pub async fn endereco_permitido(url: &str, cfg: &Config) -> Result<Option<(String, SocketAddr)>, String> {
    let u = reqwest::Url::parse(url).map_err(|e| format!("url invalida: {e}"))?;
    match u.scheme() {
        "https" => {}
        "http" if cfg.permite_http => {}
        s => return Err(format!("esquema nao permitido: {s}")),
    }
    let host = u.host_str().ok_or("url sem host")?.to_string();
    if cfg.permite_privado {
        // Pilha local de teste: sem fixação e sem checagem. Nunca em produção.
        return Ok(None);
    }
    let porta = u.port_or_known_default().unwrap_or(443);
    let enderecos: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), porta))
        .await
        .map_err(|e| format!("dns falhou para {host}: {e}"))?
        .collect();
    if enderecos.is_empty() {
        return Err(format!("dns nao resolveu {host}"));
    }
    // Recusa se QUALQUER resposta for interna: um host que mistura IP público e privado é
    // exatamente o perfil de um ataque, não de um webhook legítimo.
    for sa in &enderecos {
        if ip_interno(sa.ip()) {
            return Err(format!("{host} resolve para endereco interno ({})", sa.ip()));
        }
    }
    Ok(Some((host, enderecos[0])))
}

/// Compatível com o uso de validação-só (registro), onde não há conexão a fixar.
pub async fn url_permitida(url: &str, cfg: &Config) -> Result<(), String> {
    endereco_permitido(url, cfg).await.map(|_| ())
}

fn ip_interno(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            v.is_loopback()
                || v.is_private()
                || v.is_link_local()
                || v.is_broadcast()
                || v.is_unspecified()
                || v.is_documentation()
                || v.octets()[0] == 0
                // 100.64.0.0/10 (CGNAT) e 169.254 ja coberto por link_local
                || (v.octets()[0] == 100 && (64..128).contains(&v.octets()[1]))
        }
        IpAddr::V6(v) => {
            v.is_loopback()
                || v.is_unspecified()
                // fc00::/7 (unique local) e fe80::/10 (link local)
                || (v.segments()[0] & 0xfe00) == 0xfc00
                || (v.segments()[0] & 0xffc0) == 0xfe80
                || v.to_ipv4_mapped().is_some_and(ip_interno_v4)
        }
    }
}

fn ip_interno_v4(v: std::net::Ipv4Addr) -> bool {
    ip_interno(IpAddr::V4(v))
}

#[derive(Clone, Debug)]
pub struct Config {
    pub permite_http: bool,
    pub permite_privado: bool,
    pub max_tentativas: i64,
}

impl Config {
    pub fn do_ambiente() -> Config {
        let liga = |k: &str| std::env::var(k).map(|v| v == "1" || v == "true").unwrap_or(false);
        Config {
            permite_http: liga("TRUCO_WEBHOOK_ALLOW_HTTP"),
            permite_privado: liga("TRUCO_WEBHOOK_ALLOW_PRIVATE"),
            max_tentativas: 3,
        }
    }
}

/// Entrega as pendentes. Chamada em loop por [`trabalhador`], e também diretamente nos
/// testes — por isso devolve quantas tentou, em vez de só logar.
pub async fn drenar(pool: &Pool, http: &reqwest::Client, cfg: &Config) -> usize {
    let pendentes: Vec<(String, String, String, String, String, i64)> = match sqlx::query_as(
        "SELECT e.id, e.evento, e.corpo, w.url, w.segredo, e.tentativas
         FROM entrega e JOIN webhook w ON w.id = e.webhook_id
         WHERE e.status = 'pendente' AND w.ativo = 1 AND e.tentativas < ?
         ORDER BY e.criada_em LIMIT 50",
    )
    .bind(cfg.max_tentativas)
    .fetch_all(pool)
    .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("entrega: falha ao ler fila: {e}");
            return 0;
        }
    };

    let n = pendentes.len();
    for (id, evento, corpo, url, segredo, tentativas) in pendentes {
        let resultado = entregar_uma(http, cfg, &url, &segredo, &evento, &corpo).await;
        let tentativas = tentativas + 1;
        match resultado {
            Ok(()) => {
                let _ = sqlx::query(
                    "UPDATE entrega SET status='entregue', tentativas=?, atualizada_em=? WHERE id=?",
                )
                .bind(tentativas)
                .bind(db::agora())
                .bind(&id)
                .execute(pool)
                .await;
            }
            Err(err) => {
                let final_ = tentativas >= cfg.max_tentativas;
                tracing::warn!("entrega {id} falhou (tentativa {tentativas}): {err}");
                let _ = sqlx::query(
                    "UPDATE entrega SET status=?, tentativas=?, ultimo_erro=?, atualizada_em=?
                     WHERE id=?",
                )
                .bind(if final_ { "falhou" } else { "pendente" })
                .bind(tentativas)
                .bind(err)
                .bind(db::agora())
                .bind(&id)
                .execute(pool)
                .await;
            }
        }
    }
    n
}

async fn entregar_uma(
    http: &reqwest::Client,
    cfg: &Config,
    url: &str,
    segredo: &str,
    evento: &str,
    corpo: &str,
) -> Result<(), String> {
    let fixado = endereco_permitido(url, cfg).await?;

    // Fecha o TOCTOU: a conexão vai para o endereço que acabou de ser validado, não para o
    // que o DNS devolver de novo na hora do connect. `resolve` mantém o SNI e o header Host
    // do domínio original, então TLS continua sendo verificado contra o nome certo.
    let cliente_fixado = match &fixado {
        Some((host, addr)) => Some(
            reqwest::Client::builder()
                .user_agent("truco-zap/1.0")
                .redirect(reqwest::redirect::Policy::none())
                .resolve(host, *addr)
                .build()
                .map_err(|e| format!("cliente: {e}"))?,
        ),
        None => None,
    };
    let http = cliente_fixado.as_ref().unwrap_or(http);

    let assinatura = assinar(segredo, corpo);
    let r = http
        .post(url)
        .header("content-type", "application/json")
        .header("x-truco-evento", evento)
        .header("x-truco-signature", format!("sha256={assinatura}"))
        .body(corpo.to_string())
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| format!("http: {e}"))?;
    if r.status().is_success() {
        Ok(())
    } else {
        Err(format!("http {}", r.status()))
    }
}

pub async fn trabalhador(pool: Pool, http: reqwest::Client, cfg: Config) {
    loop {
        drenar(&pool, &http, &cfg).await;
        tokio::time::sleep(Duration::from_millis(1500)).await;
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn cfg() -> Config {
        Config { permite_http: false, permite_privado: false, max_tentativas: 3 }
    }

    #[test]
    fn hmac_e_estavel_e_sensivel_ao_corpo() {
        assert_eq!(assinar("s", "a"), assinar("s", "a"));
        assert_ne!(assinar("s", "a"), assinar("s", "b"));
        assert_ne!(assinar("s", "a"), assinar("t", "a"));
        assert_eq!(assinar("s", "a").len(), 64, "sha256 hex");
    }

    #[test]
    fn reconhece_enderecos_internos() {
        for s in ["127.0.0.1", "10.1.2.3", "192.168.0.1", "172.16.0.1", "169.254.169.254",
                  "0.0.0.0", "100.64.0.1", "::1", "fd00::1", "fe80::1", "::ffff:127.0.0.1"] {
            assert!(ip_interno(s.parse().unwrap()), "{s} deveria ser interno");
        }
        for s in ["8.8.8.8", "1.1.1.1", "2606:4700::1111"] {
            assert!(!ip_interno(s.parse().unwrap()), "{s} e publico");
        }
    }

    #[tokio::test]
    async fn recusa_esquema_e_loopback() {
        assert!(url_permitida("http://exemplo.com/h", &cfg()).await.is_err());
        assert!(url_permitida("file:///etc/passwd", &cfg()).await.is_err());
        assert!(url_permitida("https://127.0.0.1/h", &cfg()).await.is_err());
        assert!(url_permitida("https://[::1]/h", &cfg()).await.is_err());
        // 169.254.169.254 e o metadata de nuvem — o alvo classico de SSRF.
        assert!(url_permitida("https://169.254.169.254/latest/meta-data/", &cfg()).await.is_err());
    }

    #[tokio::test]
    async fn permite_http_e_privado_quando_explicitamente_ligado() {
        let aberto = Config { permite_http: true, permite_privado: true, max_tentativas: 3 };
        assert!(url_permitida("http://127.0.0.1:9/h", &aberto).await.is_ok());
        // Com privado liberado nao ha endereco a fixar — a pilha de teste resolve normalmente.
        assert_eq!(endereco_permitido("http://127.0.0.1:9/h", &aberto).await.unwrap(), None);
    }

    /// O endereco validado tem de voltar para quem chama, senao a conexao resolve de novo
    /// e o rebinding passa. Este teste existe por causa de E3 (ver ERROS.md da pesquisa).
    #[tokio::test]
    async fn devolve_o_endereco_validado_para_ser_fixado_na_conexao() {
        let r = endereco_permitido("https://one.one.one.one/h", &cfg()).await;
        match r {
            Ok(Some((host, addr))) => {
                assert_eq!(host, "one.one.one.one");
                assert!(!ip_interno(addr.ip()));
                assert_eq!(addr.port(), 443);
            }
            // Sem rede no ambiente de teste, o DNS falha — e falhar fechado tambem e correto.
            Ok(None) => panic!("com permite_privado=false o endereco tem de voltar"),
            Err(e) => eprintln!("sem DNS neste ambiente ({e}); o caminho de erro e fechado"),
        }
    }
}
