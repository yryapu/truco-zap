//! Motor de regras do **Truco Paulista**.
//!
//! Crate puro de propósito: sem I/O, sem async, sem banco, sem rede. Isso o torna
//! testável de forma exaustiva e mantém aberta a opção de compilá-lo para WASM
//! (decisão D06 — hoje o cliente não precisa dele).
//!
//! Especificação com fonte por regra:
//! <https://github.com/yryapu/poliorketikos-truco-zap/blob/main/REGRAS.md>

pub mod card;
pub mod engine;

pub use card::{Card, Rank, Suit, CARTA_DE_COSTAS};
pub use engine::{
    resultado_da_mao, time_do_assento, Acao, Erro, Evento, Fase, Jogada, Mao, Match, TipoMao,
    ESCADA, PONTOS_MAO_DE_ONZE, PONTOS_PARA_VENCER,
};
