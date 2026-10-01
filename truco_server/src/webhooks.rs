//! Webhooks: registro, guarda contra SSRF e entrega em background (decisão D12).
//!
//! Garantia: a entrega é enfileirada na **mesma transação** que grava o resultado da partida.
//! Se o resultado existe, o evento vai sair. At-least-once — o receptor precisa ser idempotente
//! pelo `evento_id` que vai no corpo.

use crate::db::{self, Pool};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::net::IpAddr;
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
/// porque DNS pode mudar entre um e outro (rebinding).
pub async fn url_permitida(url: &str, cfg: &Config) -> Result<(), String> {
    let u = reqwest::Url::parse(url).map_err(|e| format!("url invalida: {e}"))?;
    match u.scheme() {
        "https" => {}
        "http" if cfg.permite_http => {}
        s => return Err(format!("esquema nao permitido: {s}")),
    }
    let host = u.host_str().ok_or("url sem host")?;
    if cfg.permite_privado {
        return Ok(());
    }
    let porta = u.port_or_known_default().unwrap_or(443);
    let ips: Vec<IpAddr> = tokio::net::lookup_host((host, porta))
        .await
        .map_err(|e| format!("dns falhou para {host}: {e}"))?
        .map(|sa| sa.ip())
        .collect();
    if ips.is_empty() {
        return Err(format!("dns nao resolveu {host}"));
    }
    for ip in ips {
        if ip_interno(ip) {
            return Err(format!("{host} resolve para endereco interno ({ip})"));
        }
    }
    Ok(())
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
    url_permitida(url, cfg).await?;
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
    }
}
