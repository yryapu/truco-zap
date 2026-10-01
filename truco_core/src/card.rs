//! A carta do truco paulista e sua representação Unicode.
//!
//! Regras implementadas aqui: R1 (baralho sujo de 40), R2 (ordem base), R3 (manilha pela
//! vira em ordem cíclica), R4 (naipe só desempata entre manilhas).
//! Spec: https://github.com/yryapu/poliorketikos-truco-zap/blob/main/REGRAS.md

use serde::{Deserialize, Serialize};

/// Naipes em ordem **crescente de força de manilha** (R4): ♦ < ♠ < ♥ < ♣.
/// O valor numérico é a força, então `Suit as u8` já é comparável.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Suit {
    /// Ouros — a manilha mais fraca ("pica-fumo").
    Ouros = 0,
    /// Espadas — "espadilha".
    Espadas = 1,
    /// Copas — "copeta" / "escopeta".
    Copas = 2,
    /// Paus — "zap", a manilha mais forte.
    Paus = 3,
}

impl Suit {
    pub const TODOS: [Suit; 4] = [Suit::Ouros, Suit::Espadas, Suit::Copas, Suit::Paus];

    /// Base do bloco Unicode *Playing Cards* para o naipe.
    /// Espadas U+1F0A0, Copas U+1F0B0, Ouros U+1F0C0, Paus U+1F0D0.
    const fn unicode_base(self) -> u32 {
        match self {
            Suit::Espadas => 0x1F0A0,
            Suit::Copas => 0x1F0B0,
            Suit::Ouros => 0x1F0C0,
            Suit::Paus => 0x1F0D0,
        }
    }

    /// Apelido tradicional da manilha deste naipe.
    pub const fn apelido_manilha(self) -> &'static str {
        match self {
            Suit::Paus => "zap",
            Suit::Copas => "copeta",
            Suit::Espadas => "espadilha",
            Suit::Ouros => "pica-fumo",
        }
    }
}

/// Ranks em ordem **crescente de força base** (R2): 4 < 5 < 6 < 7 < Q < J < K < A < 2 < 3.
///
/// Atenção: Dama abaixo de Valete. Não é erro de digitação — é a regra
/// ("as in many games of Portuguese ancestry, the Queens rank lower than the Jacks", F1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Rank {
    Quatro = 0,
    Cinco = 1,
    Seis = 2,
    Sete = 3,
    Dama = 4,
    Valete = 5,
    Rei = 6,
    As = 7,
    Dois = 8,
    Tres = 9,
}

impl Rank {
    /// A ordem cíclica de R3. O índice no array é `Rank as usize`.
    pub const TODOS: [Rank; 10] = [
        Rank::Quatro,
        Rank::Cinco,
        Rank::Seis,
        Rank::Sete,
        Rank::Dama,
        Rank::Valete,
        Rank::Rei,
        Rank::As,
        Rank::Dois,
        Rank::Tres,
    ];

    /// Offset do rank dentro do bloco Unicode do naipe.
    ///
    /// Cuidado: os ranks **não** são contíguos. U+1F0xC é o *Cavaleiro* (Knight), que não
    /// existe no baralho francês nem no truco, então Valete=+0xB, Dama=+0xD, Rei=+0xE.
    /// Assumir contiguidade daria um baralho de 44 cartas.
    const fn unicode_offset(self) -> u32 {
        match self {
            Rank::As => 0x1,
            Rank::Dois => 0x2,
            Rank::Tres => 0x3,
            Rank::Quatro => 0x4,
            Rank::Cinco => 0x5,
            Rank::Seis => 0x6,
            Rank::Sete => 0x7,
            Rank::Valete => 0xB,
            Rank::Dama => 0xD,
            Rank::Rei => 0xE,
        }
    }

    fn from_unicode_offset(off: u32) -> Option<Rank> {
        Some(match off {
            0x1 => Rank::As,
            0x2 => Rank::Dois,
            0x3 => Rank::Tres,
            0x4 => Rank::Quatro,
            0x5 => Rank::Cinco,
            0x6 => Rank::Seis,
            0x7 => Rank::Sete,
            0xB => Rank::Valete,
            0xD => Rank::Dama,
            0xE => Rank::Rei,
            _ => return None,
        })
    }

    /// R3: o rank imediatamente **acima** deste na ordem cíclica — 3 volta para 4.
    pub fn proximo_ciclico(self) -> Rank {
        Rank::TODOS[(self as usize + 1) % 10]
    }
}

/// Carta de costas: U+1F0A0 PLAYING CARD BACK. É o caractere Unicode correto para
/// "encoberta" (R7), não um símbolo inventado.
pub const CARTA_DE_COSTAS: char = '\u{1F0A0}';

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Card {
    pub rank: Rank,
    pub suit: Suit,
}

impl Card {
    pub const fn new(rank: Rank, suit: Suit) -> Self {
        Card { rank, suit }
    }

    /// O baralho sujo: 40 cartas, sem 8, 9, 10 e curingas (R1).
    pub fn baralho() -> Vec<Card> {
        let mut v = Vec::with_capacity(40);
        for s in Suit::TODOS {
            for r in Rank::TODOS {
                v.push(Card::new(r, s));
            }
        }
        v
    }

    /// O caractere Unicode da carta — a representação que trafega no WebSocket (D04).
    pub fn unicode(self) -> char {
        let cp = self.suit.unicode_base() + self.rank.unicode_offset();
        // Todo valor construído aqui está no bloco U+1F0A1..U+1F0DE, que é válido.
        char::from_u32(cp).expect("code point de carta sempre valido")
    }

    /// Volta do caractere Unicode para a carta. Rejeita qualquer coisa fora do baralho —
    /// incluindo o Cavaleiro (U+1F0xC) e os naipes de Trunfo (U+1F0E0+).
    pub fn from_unicode(c: char) -> Option<Card> {
        let cp = c as u32;
        let (suit, base) = match cp & 0xFFFF_FFF0 {
            0x1F0A0 => (Suit::Espadas, 0x1F0A0),
            0x1F0B0 => (Suit::Copas, 0x1F0B0),
            0x1F0C0 => (Suit::Ouros, 0x1F0C0),
            0x1F0D0 => (Suit::Paus, 0x1F0D0),
            _ => return None,
        };
        Rank::from_unicode_offset(cp - base).map(|rank| Card::new(rank, suit))
    }

    /// R3/R4: a força da carta nesta mão, dada a vira.
    ///
    /// Comuns ficam em 0..=9 (a ordem base). Manilhas ficam em 10..=13, acima de qualquer
    /// comum, ordenadas pelo naipe. Como cada naipe aparece uma vez, **duas manilhas nunca
    /// empatam** — o que é exatamente o que R4 diz.
    pub fn forca(self, vira: Card) -> u8 {
        if self.rank == vira.rank.proximo_ciclico() {
            10 + self.suit as u8
        } else {
            self.rank as u8
        }
    }

    pub fn eh_manilha(self, vira: Card) -> bool {
        self.rank == vira.rank.proximo_ciclico()
    }
}
