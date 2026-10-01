//! Acesso a dados. SQLite via sqlx (decisão D02).
//!
//! Usa a API dinâmica (`query_as` + `bind`) em vez de `query!`, de propósito: `query!` exige
//! banco disponível em tempo de compilação ou um cache `.sqlx` versionado, e eu não quero
//! acoplar o build a isso. Perco checagem estática e pago com testes.

use serde::Serialize;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{FromRow, SqlitePool};
use std::str::FromStr;

pub type Pool = SqlitePool;

pub async fn abrir(url: &str) -> anyhow::Result<Pool> {
    let opts = SqliteConnectOptions::from_str(url)?
        .create_if_missing(true)
        // WAL: leitores não bloqueiam o escritor. Importa porque o loop de jogo lê muito.
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .busy_timeout(std::time::Duration::from_secs(5))
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new().max_connections(8).connect_with(opts).await?;
    sqlx::migrate!("../migrations").run(&pool).await?;
    Ok(pool)
}

pub fn agora() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Jogador {
    pub id: String,
    pub apelido: String,
    #[serde(skip)]
    pub senha_hash: String,
    pub saldo: i64,
    pub vitorias: i64,
    pub derrotas: i64,
    pub convidado: i64,
}

impl Jogador {
    pub fn partidas(&self) -> i64 {
        self.vitorias + self.derrotas
    }
}

const COLS: &str = "id, apelido, senha_hash, saldo, vitorias, derrotas, convidado";

pub async fn criar_jogador(
    p: &Pool,
    id: &str,
    apelido: &str,
    senha_hash: &str,
    convidado: bool,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO jogador (id, apelido, apelido_lc, senha_hash, saldo, convidado, criado_em)
         VALUES (?, ?, ?, ?, 1000, ?, ?)",
    )
    .bind(id)
    .bind(apelido)
    .bind(apelido.to_lowercase())
    .bind(senha_hash)
    .bind(i64::from(convidado))
    .bind(agora())
    .execute(p)
    .await
    .map(|_| ())
}

pub async fn jogador_por_apelido(p: &Pool, apelido: &str) -> sqlx::Result<Option<Jogador>> {
    sqlx::query_as::<_, Jogador>(&format!("SELECT {COLS} FROM jogador WHERE apelido_lc = ?"))
        .bind(apelido.to_lowercase())
        .fetch_optional(p)
        .await
}

pub async fn jogador_por_id(p: &Pool, id: &str) -> sqlx::Result<Option<Jogador>> {
    sqlx::query_as::<_, Jogador>(&format!("SELECT {COLS} FROM jogador WHERE id = ?"))
        .bind(id)
        .fetch_optional(p)
        .await
}

pub async fn criar_sessao(p: &Pool, token_hash: &str, jogador_id: &str) -> sqlx::Result<()> {
    sqlx::query("INSERT INTO sessao (token_hash, jogador_id, criada_em) VALUES (?, ?, ?)")
        .bind(token_hash)
        .bind(jogador_id)
        .bind(agora())
        .execute(p)
        .await
        .map(|_| ())
}

pub async fn jogador_por_token_hash(p: &Pool, h: &str) -> sqlx::Result<Option<Jogador>> {
    sqlx::query_as::<_, Jogador>(&format!(
        "SELECT j.id, j.apelido, j.senha_hash, j.saldo, j.vitorias, j.derrotas, j.convidado
         FROM sessao s JOIN jogador j ON j.id = s.jogador_id WHERE s.token_hash = ?"
    ))
    .bind(h)
    .fetch_optional(p)
    .await
}

pub async fn apagar_sessao(p: &Pool, h: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM sessao WHERE token_hash = ?").bind(h).execute(p).await.map(|_| ())
}

/// Debita a aposta de cada jogador humano e abre a partida, tudo numa transação.
///
/// Se qualquer debito falhar (saldo insuficiente — o `CHECK (saldo >= 0)` do esquema pega),
/// nada acontece. É isso que garante que não existe meia-aposta.
pub async fn abrir_partida(
    p: &Pool,
    partida_id: &str,
    modo: &str,
    aposta: i64,
    assentos: &[(usize, u8, Option<String>)],
) -> anyhow::Result<i64> {
    let mut tx = p.begin().await?;
    let mut bolo = 0i64;
    sqlx::query(
        "INSERT INTO partida (id, modo, aposta, estado, bolo, comecou_em)
         VALUES (?, ?, ?, 'em_andamento', 0, ?)",
    )
    .bind(partida_id)
    .bind(modo)
    .bind(aposta)
    .bind(agora())
    .execute(&mut *tx)
    .await?;

    for (assento, time, jogador_id) in assentos {
        let Some(jid) = jogador_id else { continue }; // robô não aposta (D15)
        if aposta > 0 {
            let r = sqlx::query("UPDATE jogador SET saldo = saldo - ? WHERE id = ? AND saldo >= ?")
                .bind(aposta)
                .bind(jid)
                .bind(aposta)
                .execute(&mut *tx)
                .await?;
            if r.rows_affected() != 1 {
                anyhow::bail!("saldo insuficiente para {jid}");
            }
            bolo += aposta;
        }
        sqlx::query(
            "INSERT INTO participacao (partida_id, jogador_id, assento, time, aposta)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(partida_id)
        .bind(jid)
        .bind(*assento as i64)
        .bind(i64::from(*time))
        .bind(aposta)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("UPDATE partida SET bolo = ? WHERE id = ?")
        .bind(bolo)
        .bind(partida_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(bolo)
}

pub struct Premiacao {
    pub jogador_id: String,
    pub premio: i64,
    pub venceu: bool,
}

/// Fecha a partida: placar, prêmios, vitórias/derrotas e as entregas de webhook — tudo
/// na mesma transação (D12). Devolve o saldo final de cada jogador.
pub async fn fechar_partida(
    p: &Pool,
    partida_id: &str,
    placar: [u8; 2],
    time_vencedor: u8,
    premiacoes: &[Premiacao],
    entregas: &[(String, String, String, String)], // (id, webhook_id, evento, corpo)
) -> anyhow::Result<Vec<(String, i64)>> {
    let mut tx = p.begin().await?;
    sqlx::query(
        "UPDATE partida SET estado='terminada', placar_time0=?, placar_time1=?,
         time_vencedor=?, terminou_em=? WHERE id=? AND estado='em_andamento'",
    )
    .bind(i64::from(placar[0]))
    .bind(i64::from(placar[1]))
    .bind(i64::from(time_vencedor))
    .bind(agora())
    .bind(partida_id)
    .execute(&mut *tx)
    .await?;

    let mut saldos = Vec::new();
    for pr in premiacoes {
        sqlx::query(
            "UPDATE jogador SET saldo = saldo + ?, vitorias = vitorias + ?, derrotas = derrotas + ?
             WHERE id = ?",
        )
        .bind(pr.premio)
        .bind(i64::from(pr.venceu))
        .bind(i64::from(!pr.venceu))
        .bind(&pr.jogador_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE participacao SET premio = ? WHERE partida_id = ? AND jogador_id = ?")
            .bind(pr.premio)
            .bind(partida_id)
            .bind(&pr.jogador_id)
            .execute(&mut *tx)
            .await?;
        let saldo: (i64,) = sqlx::query_as("SELECT saldo FROM jogador WHERE id = ?")
            .bind(&pr.jogador_id)
            .fetch_one(&mut *tx)
            .await?;
        saldos.push((pr.jogador_id.clone(), saldo.0));
    }

    for (id, wid, evento, corpo) in entregas {
        sqlx::query(
            "INSERT INTO entrega (id, webhook_id, evento, corpo, criada_em) VALUES (?,?,?,?,?)",
        )
        .bind(id)
        .bind(wid)
        .bind(evento)
        .bind(corpo)
        .bind(agora())
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(saldos)
}

pub async fn enfileirar_entregas(
    p: &Pool,
    entregas: &[(String, String, String, String)],
) -> anyhow::Result<()> {
    for (id, wid, evento, corpo) in entregas {
        sqlx::query(
            "INSERT INTO entrega (id, webhook_id, evento, corpo, criada_em) VALUES (?,?,?,?,?)",
        )
        .bind(id)
        .bind(wid)
        .bind(evento)
        .bind(corpo)
        .bind(agora())
        .execute(p)
        .await?;
    }
    Ok(())
}

/// Estorna apostas de partidas que ficaram `em_andamento` (o processo caiu no meio).
/// Chamado no startup — é a mitigação do risco registrado em D05.
pub async fn estornar_partidas_orfas(p: &Pool) -> anyhow::Result<u64> {
    let mut tx = p.begin().await?;
    let orfas: Vec<(String,)> =
        sqlx::query_as("SELECT id FROM partida WHERE estado='em_andamento'")
            .fetch_all(&mut *tx)
            .await?;
    for (id,) in &orfas {
        sqlx::query(
            "UPDATE jogador SET saldo = saldo + (
                 SELECT aposta FROM participacao WHERE partida_id = ? AND jogador_id = jogador.id
             ) WHERE id IN (SELECT jogador_id FROM participacao WHERE partida_id = ?)",
        )
        .bind(id)
        .bind(id)
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE partida SET estado='cancelada', terminou_em=? WHERE id=?")
            .bind(agora())
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(orfas.len() as u64)
}

#[derive(Debug, FromRow, Serialize)]
pub struct LinhaRanking {
    pub posicao: i64,
    pub apelido: String,
    pub vitorias: i64,
    pub derrotas: i64,
    pub saldo: i64,
}

pub async fn ranking(p: &Pool, limite: i64) -> sqlx::Result<Vec<LinhaRanking>> {
    sqlx::query_as::<_, LinhaRanking>(
        "SELECT ROW_NUMBER() OVER (
                   ORDER BY vitorias DESC, (vitorias - derrotas) DESC, saldo DESC, apelido ASC
                 ) AS posicao,
                apelido, vitorias, derrotas, saldo
         FROM jogador
         ORDER BY posicao LIMIT ?",
    )
    .bind(limite)
    .fetch_all(p)
    .await
}

#[derive(Debug, FromRow, Serialize)]
pub struct LinhaHistorico {
    pub partida_id: String,
    pub modo: String,
    pub aposta: i64,
    pub premio: i64,
    pub placar_time0: Option<i64>,
    pub placar_time1: Option<i64>,
    pub venceu: i64,
    pub terminou_em: Option<String>,
}

pub async fn historico(p: &Pool, jogador_id: &str, limite: i64) -> sqlx::Result<Vec<LinhaHistorico>> {
    sqlx::query_as::<_, LinhaHistorico>(
        "SELECT pa.id AS partida_id, pa.modo, pt.aposta, pt.premio,
                pa.placar_time0, pa.placar_time1, pa.terminou_em,
                CASE WHEN pa.time_vencedor = pt.time THEN 1 ELSE 0 END AS venceu
         FROM participacao pt JOIN partida pa ON pa.id = pt.partida_id
         WHERE pt.jogador_id = ? AND pa.estado = 'terminada'
         ORDER BY pa.terminou_em DESC LIMIT ?",
    )
    .bind(jogador_id)
    .bind(limite)
    .fetch_all(p)
    .await
}

#[derive(Debug, FromRow)]
pub struct WebhookRow {
    pub id: String,
    pub url: String,
}

pub async fn webhooks_de(p: &Pool, jogador_id: &str) -> sqlx::Result<Vec<WebhookRow>> {
    sqlx::query_as::<_, WebhookRow>(
        "SELECT id, url FROM webhook WHERE jogador_id = ? AND ativo = 1",
    )
    .bind(jogador_id)
    .fetch_all(p)
    .await
}

pub async fn registrar_webhook(
    p: &Pool,
    id: &str,
    jogador_id: &str,
    url: &str,
    segredo: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO webhook (id, jogador_id, url, segredo, criado_em) VALUES (?,?,?,?,?)",
    )
    .bind(id)
    .bind(jogador_id)
    .bind(url)
    .bind(segredo)
    .bind(agora())
    .execute(p)
    .await
    .map(|_| ())
}

pub async fn remover_webhook(p: &Pool, id: &str, jogador_id: &str) -> sqlx::Result<u64> {
    sqlx::query("UPDATE webhook SET ativo = 0 WHERE id = ? AND jogador_id = ?")
        .bind(id)
        .bind(jogador_id)
        .execute(p)
        .await
        .map(|r| r.rows_affected())
}
