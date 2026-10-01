//! Cadastro, login e sessão. Token opaco em cookie HttpOnly (decisão D03).
//!
//! Propriedade que isto garante: o servidor nunca acredita no cliente sobre quem ele é.
//! Toda rota autenticada resolve `cookie -> sha256 -> sessao -> jogador` no banco.

use crate::db;
use argon2::password_hash::{rand_core::OsRng as ArgonRng, PasswordHasher, SaltString};
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use rand::RngCore;
use sha2::{Digest, Sha256};

pub const COOKIE: &str = "truco_sessao";

pub fn token_novo() -> String {
    let mut b = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut b);
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
}

pub fn hash_token(t: &str) -> String {
    hex::encode(Sha256::digest(t.as_bytes()))
}

pub fn hash_senha(s: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut ArgonRng);
    Ok(Argon2::default()
        .hash_password(s.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("argon2: {e}"))?
        .to_string())
}

pub fn confere_senha(s: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|h| Argon2::default().verify_password(s.as_bytes(), &h).is_ok())
        .unwrap_or(false)
}

/// Monta o cookie de sessão. `HttpOnly` tira o token do alcance de XSS; `SameSite=Lax`
/// porque o app é same-origin e Lax não quebra a navegação inicial.
pub fn cookie_de_sessao(token: &str, seguro: bool) -> String {
    let mut c = format!("{COOKIE}={token}; HttpOnly; SameSite=Lax; Path=/; Max-Age=2592000");
    if seguro {
        c.push_str("; Secure");
    }
    c
}

pub fn cookie_vazio() -> String {
    format!("{COOKIE}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0")
}

pub fn token_do_header(parts: &Parts) -> Option<String> {
    let raw = parts
        .headers
        .get(axum::http::header::COOKIE)?
        .to_str()
        .ok()?;
    raw.split(';')
        .filter_map(|kv| kv.split_once('='))
        .find(|(k, _)| k.trim() == COOKIE)
        .map(|(_, v)| v.trim().to_string())
}

/// Extractor: a presença dele numa assinatura de handler é o que torna a rota autenticada.
/// Não há caminho alternativo para obter um `Autenticado` — é isso que dá o isolamento.
pub struct Autenticado(pub db::Jogador);

impl FromRequestParts<crate::AppState> for Autenticado {
    type Rejection = (StatusCode, axum::Json<serde_json::Value>);

    async fn from_request_parts(
        parts: &mut Parts,
        st: &crate::AppState,
    ) -> Result<Self, Self::Rejection> {
        let nao = || {
            (
                StatusCode::UNAUTHORIZED,
                axum::Json(serde_json::json!({"erro": "sessao_invalida"})),
            )
        };
        let token = token_do_header(parts).ok_or_else(nao)?;
        let j = db::jogador_por_token_hash(&st.pool, &hash_token(&token))
            .await
            .map_err(|_| nao())?
            .ok_or_else(nao)?;
        Ok(Autenticado(j))
    }
}

/// Apelido: 2..=20 caracteres, só letras/dígitos/`_`/`-`/espaço simples. Rejeitar aqui
/// evita apelido com caractere de controle vazando para o HTML do ranking.
pub fn apelido_valido(a: &str) -> bool {
    let n = a.chars().count();
    (2..=20).contains(&n)
        && a.trim() == a
        && a.chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == ' ')
}

pub fn senha_valida(s: &str) -> bool {
    (6..=200).contains(&s.chars().count())
}
