//! A partida de Truco Paulista. Puro: nenhum I/O, nenhum async, nenhuma rede.
//!
//! Mantido sem dependência de runtime de propósito — é o que deixa a porta aberta para
//! compilar este crate para WASM se algum dia o cliente precisar decidir algo sozinho
//! (ver decisão D06 no repo de pesquisa).
//!
//! Spec: https://github.com/yryapu/poliorketikos-truco-zap/blob/main/REGRAS.md

use crate::card::{Card, Rank, Suit};
use rand::seq::SliceRandom;
use rand::Rng;
use serde::{Deserialize, Serialize};

pub const PONTOS_PARA_VENCER: u8 = 12;
pub const PONTOS_MAO_DE_ONZE: u8 = 11;
/// A escada de R9. Índice = quantos pedidos já foram aceitos.
pub const ESCADA: [u8; 5] = [1, 3, 6, 9, 12];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TipoMao {
    /// Mão comum: vale 1 e aceita truco.
    Normal,
    /// R10: uma dupla tem 11. Vale 3, sem truco, e a dupla de 11 decide se joga.
    MaoDeOnze { time: u8 },
    /// R11: as duas duplas têm 11. Vale 1, sem truco, quem vencer vence a partida.
    MaoDeFerro,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fase {
    /// R10: esperando a dupla de 11 dizer se joga ou corre.
    DecisaoMaoDeOnze { time: u8 },
    /// Alguém tem a vez de jogar uma carta.
    Jogando,
    /// R9: houve pedido, o time `respondendo` deve correr, aceitar ou aumentar.
    AguardandoResposta {
        pedinte: u8,
        respondendo: u8,
        proposto: u8,
    },
    /// A mão acabou; o servidor chama [`Match::nova_mao`] quando quiser dar a próxima.
    MaoEncerrada,
    PartidaEncerrada { vencedor: u8 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Jogada {
    pub assento: usize,
    /// `None` quando encoberta (R7) — a carta existe mas não é revelada nem comparada.
    pub carta: Option<Card>,
    pub encoberta: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "acao")]
pub enum Acao {
    Jogar { carta: Card, encoberta: bool },
    /// Truco, 6, 9 ou 12 — a escada decide qual, não o cliente (R9).
    Pedir,
    Aceitar,
    Correr,
    Aumentar,
    MaoDeOnzeJogar,
    MaoDeOnzeCorrer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "evento")]
pub enum Evento {
    MaoComecou {
        vira: Card,
        tipo: TipoMao,
        valor: u8,
        mao: usize,
    },
    CartaJogada(Jogada),
    RodadaTerminou {
        indice: usize,
        /// `None` = rodada empatada (R6).
        vencedor: Option<u8>,
        proximo_a_puxar: usize,
    },
    Pedido {
        assento: usize,
        time: u8,
        proposto: u8,
    },
    PedidoAceito {
        valor: u8,
    },
    Correu {
        time_que_correu: u8,
        pontos: u8,
    },
    MaoDeOnzeRecusada {
        time: u8,
        pontos_para: u8,
    },
    MaoTerminou {
        /// `None` = mão empatada, ninguém pontua (R8).
        vencedor: Option<u8>,
        pontos: u8,
        placar: [u8; 2],
    },
    PartidaTerminou {
        vencedor: u8,
        placar: [u8; 2],
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Erro {
    NaoEhSuaVez,
    CartaNaoEstaNaSuaMao,
    EncobertaNaPrimeiraRodada,
    AcaoForaDeFase,
    TrucoProibidoMaoDeOnze,
    JaPediuEsperaResposta,
    SeuTimeFezOUltimoPedido,
    ValorMaximoAtingido,
    PartidaEncerrada,
}

/// Estado de uma mão em andamento.
#[derive(Debug, Clone)]
pub struct Mao {
    pub vira: Card,
    pub cartas: Vec<Vec<Card>>,
    pub tipo: TipoMao,
    pub valor: u8,
    /// Resultado de cada rodada já concluída: `Some(time)` ou `None` se empatou.
    pub rodadas: Vec<Option<u8>>,
    pub jogadas: Vec<Jogada>,
    pub puxador: usize,
    pub vez: usize,
    /// R9: o time que fez o último pedido aceito não pode pedir de novo em seguida.
    pub ultimo_pedinte: Option<u8>,
    /// Índice na [`ESCADA`] do valor corrente.
    degrau: usize,
}

#[derive(Debug, Clone)]
pub struct Match {
    pub jogadores: usize,
    pub placar: [u8; 2],
    pub mao: Mao,
    pub fase: Fase,
    /// Assento que puxa a primeira rodada da mão corrente (R13).
    pub mao_de_quem: usize,
    pub numero_da_mao: usize,
}

/// `time` de um assento: assentos alternam times, então partidas ficam opostas no 2x2.
#[inline]
pub fn time_do_assento(assento: usize) -> u8 {
    (assento % 2) as u8
}

impl Match {
    /// Cria a partida e já distribui a primeira mão.
    ///
    /// `jogadores` deve ser 2 (1x1) ou 4 (2x2). Nada no motor assume 4 — o 1x1 é
    /// simplesmente um time de tamanho 1 (decisão D11).
    pub fn novo<R: Rng>(jogadores: usize, rng: &mut R) -> Match {
        assert!(jogadores == 2 || jogadores == 4, "mesa e 1x1 ou 2x2");
        let mao_de_quem = rng.gen_range(0..jogadores);
        let mut m = Match {
            jogadores,
            placar: [0, 0],
            mao: Mao::vazia(),
            fase: Fase::Jogando,
            mao_de_quem,
            numero_da_mao: 0,
        };
        m.distribuir(rng);
        m
    }

    /// Distribui a próxima mão. R13: o "mão" rotaciona um assento.
    pub fn nova_mao<R: Rng>(&mut self, rng: &mut R) -> Vec<Evento> {
        if let Fase::PartidaEncerrada { .. } = self.fase {
            return vec![];
        }
        self.mao_de_quem = (self.mao_de_quem + 1) % self.jogadores;
        self.distribuir(rng)
    }

    fn distribuir<R: Rng>(&mut self, rng: &mut R) -> Vec<Evento> {
        let mut baralho = Card::baralho();
        baralho.shuffle(rng);
        let vira = baralho.pop().expect("baralho de 40 nao esvazia");
        let cartas: Vec<Vec<Card>> = (0..self.jogadores)
            .map(|_| {
                (0..3)
                    .map(|_| baralho.pop().expect("40 cartas cobrem 4x3 + vira"))
                    .collect()
            })
            .collect();

        let onze = [
            self.placar[0] >= PONTOS_MAO_DE_ONZE,
            self.placar[1] >= PONTOS_MAO_DE_ONZE,
        ];
        let (tipo, valor, fase) = match onze {
            [true, true] => (TipoMao::MaoDeFerro, 1, Fase::Jogando),
            [true, false] => (
                TipoMao::MaoDeOnze { time: 0 },
                3,
                Fase::DecisaoMaoDeOnze { time: 0 },
            ),
            [false, true] => (
                TipoMao::MaoDeOnze { time: 1 },
                3,
                Fase::DecisaoMaoDeOnze { time: 1 },
            ),
            [false, false] => (TipoMao::Normal, 1, Fase::Jogando),
        };

        self.numero_da_mao += 1;
        self.mao = Mao {
            vira,
            cartas,
            tipo,
            valor,
            rodadas: vec![],
            jogadas: vec![],
            puxador: self.mao_de_quem,
            vez: self.mao_de_quem,
            ultimo_pedinte: None,
            degrau: if valor == 3 { 1 } else { 0 },
        };
        self.fase = fase;
        vec![Evento::MaoComecou {
            vira,
            tipo,
            valor,
            mao: self.numero_da_mao,
        }]
    }

    /// Quem pode agir agora. Em resposta a pedido, qualquer jogador do time que responde
    /// (F2: "Qualquer um dos jogadores pode responder o pedido e vale a primeira resposta").
    pub fn pode_agir(&self, assento: usize) -> bool {
        match self.fase {
            Fase::Jogando => self.mao.vez == assento,
            Fase::AguardandoResposta { respondendo, .. } => {
                time_do_assento(assento) == respondendo
            }
            Fase::DecisaoMaoDeOnze { time } => time_do_assento(assento) == time,
            Fase::MaoEncerrada | Fase::PartidaEncerrada { .. } => false,
        }
    }

    pub fn aplicar(&mut self, assento: usize, acao: Acao) -> Result<Vec<Evento>, Erro> {
        if let Fase::PartidaEncerrada { .. } = self.fase {
            return Err(Erro::PartidaEncerrada);
        }
        if !self.pode_agir(assento) {
            return Err(Erro::NaoEhSuaVez);
        }
        match acao {
            Acao::Jogar { carta, encoberta } => self.jogar(assento, carta, encoberta),
            Acao::Pedir => self.pedir(assento),
            Acao::Aceitar => self.aceitar(),
            Acao::Correr => self.correr(assento),
            Acao::Aumentar => self.aumentar(assento),
            Acao::MaoDeOnzeJogar => self.mao_de_onze(true),
            Acao::MaoDeOnzeCorrer => self.mao_de_onze(false),
        }
    }

    fn mao_de_onze(&mut self, jogar: bool) -> Result<Vec<Evento>, Erro> {
        let time = match self.fase {
            Fase::DecisaoMaoDeOnze { time } => time,
            _ => return Err(Erro::AcaoForaDeFase),
        };
        if jogar {
            self.fase = Fase::Jogando;
            return Ok(vec![Evento::PedidoAceito {
                valor: self.mao.valor,
            }]);
        }
        // R10: recusou. O adversário leva 1 ponto e a mão não é jogada.
        let adversario = 1 - time;
        let mut ev = vec![Evento::MaoDeOnzeRecusada {
            time,
            pontos_para: adversario,
        }];
        ev.extend(self.pontuar(Some(adversario), 1));
        Ok(ev)
    }

    fn jogar(&mut self, assento: usize, carta: Card, encoberta: bool) -> Result<Vec<Evento>, Erro> {
        if self.fase != Fase::Jogando {
            return Err(Erro::AcaoForaDeFase);
        }
        if encoberta && self.mao.rodadas.is_empty() {
            return Err(Erro::EncobertaNaPrimeiraRodada); // R7
        }
        let pos = self.mao.cartas[assento]
            .iter()
            .position(|c| *c == carta)
            .ok_or(Erro::CartaNaoEstaNaSuaMao)?;
        self.mao.cartas[assento].remove(pos);

        let jogada = Jogada {
            assento,
            carta: if encoberta { None } else { Some(carta) },
            encoberta,
        };
        self.mao.jogadas.push(jogada);
        let mut ev = vec![Evento::CartaJogada(jogada)];

        if self.mao.jogadas.len() < self.jogadores {
            self.mao.vez = (self.mao.vez + 1) % self.jogadores;
            return Ok(ev);
        }

        // Rodada completa: resolve.
        let (vencedor, proximo) = self.resolver_rodada();
        self.mao.rodadas.push(vencedor);
        let indice = self.mao.rodadas.len() - 1;
        self.mao.jogadas.clear();
        self.mao.puxador = proximo;
        self.mao.vez = proximo;
        ev.push(Evento::RodadaTerminou {
            indice,
            vencedor,
            proximo_a_puxar: proximo,
        });

        if let Some(resultado) = resultado_da_mao(&self.mao.rodadas) {
            ev.extend(self.pontuar(resultado, self.mao.valor));
        }
        Ok(ev)
    }

    /// R6 + R4 + D07. Devolve (vencedor da rodada, quem puxa a próxima).
    fn resolver_rodada(&self) -> (Option<u8>, usize) {
        let vira = self.mao.vira;
        let visiveis: Vec<(usize, u8)> = self
            .mao
            .jogadas
            .iter()
            .filter_map(|j| j.carta.map(|c| (j.assento, c.forca(vira))))
            .collect();

        // Caso de borda: todos jogaram encoberto (só possível na 2ª/3ª rodada).
        // Ninguém vence; quem puxou a rodada puxa a próxima.
        let Some(&(_, maxf)) = visiveis.iter().max_by_key(|(_, f)| *f) else {
            return (None, self.mao.puxador);
        };

        // Na ordem em que foram jogadas, os assentos que empataram no topo.
        let topo: Vec<usize> = self
            .mao
            .jogadas
            .iter()
            .filter(|j| j.carta.map(|c| c.forca(vira)) == Some(maxf))
            .map(|j| j.assento)
            .collect();

        let times: Vec<u8> = topo.iter().map(|a| time_do_assento(*a)).collect();
        let primeiro_do_topo = topo[0];
        if times.iter().all(|t| *t == times[0]) {
            // Um único time no topo — vence, mesmo se foram dois parceiros com cartas iguais
            // (F1: "the trick is not tied, but is won by the team that played the highest").
            (Some(times[0]), primeiro_do_topo)
        } else {
            // Empate (R6). D07: puxa quem pôs a primeira carta que empatou.
            (None, primeiro_do_topo)
        }
    }

    fn pedir(&mut self, assento: usize) -> Result<Vec<Evento>, Erro> {
        if self.mao.tipo != TipoMao::Normal {
            return Err(Erro::TrucoProibidoMaoDeOnze); // R15 / D10
        }
        if self.fase != Fase::Jogando {
            return Err(Erro::JaPediuEsperaResposta);
        }
        let time = time_do_assento(assento);
        if self.mao.ultimo_pedinte == Some(time) {
            return Err(Erro::SeuTimeFezOUltimoPedido); // R9
        }
        self.propor(assento, time)
    }

    fn aumentar(&mut self, assento: usize) -> Result<Vec<Evento>, Erro> {
        let (pedinte, proposto) = match self.fase {
            Fase::AguardandoResposta {
                pedinte, proposto, ..
            } => (pedinte, proposto),
            _ => return Err(Erro::AcaoForaDeFase),
        };
        if proposto >= 12 {
            return Err(Erro::ValorMaximoAtingido); // R9: no 12 só aceita ou corre
        }
        // Aumentar = aceitar o degrau proposto e propor o seguinte.
        self.mao.degrau += 1;
        self.mao.valor = ESCADA[self.mao.degrau];
        self.mao.ultimo_pedinte = Some(pedinte);
        let time = time_do_assento(assento);
        let mut ev = vec![Evento::PedidoAceito {
            valor: self.mao.valor,
        }];
        ev.extend(self.propor(assento, time)?);
        Ok(ev)
    }

    fn propor(&mut self, assento: usize, time: u8) -> Result<Vec<Evento>, Erro> {
        if self.mao.degrau + 1 >= ESCADA.len() {
            return Err(Erro::ValorMaximoAtingido);
        }
        let proposto = ESCADA[self.mao.degrau + 1];
        self.fase = Fase::AguardandoResposta {
            pedinte: time,
            respondendo: 1 - time,
            proposto,
        };
        Ok(vec![Evento::Pedido {
            assento,
            time,
            proposto,
        }])
    }

    fn aceitar(&mut self) -> Result<Vec<Evento>, Erro> {
        let pedinte = match self.fase {
            Fase::AguardandoResposta { pedinte, .. } => pedinte,
            _ => return Err(Erro::AcaoForaDeFase),
        };
        self.mao.degrau += 1;
        self.mao.valor = ESCADA[self.mao.degrau];
        self.mao.ultimo_pedinte = Some(pedinte);
        self.fase = Fase::Jogando;
        Ok(vec![Evento::PedidoAceito {
            valor: self.mao.valor,
        }])
    }

    fn correr(&mut self, assento: usize) -> Result<Vec<Evento>, Erro> {
        let pedinte = match self.fase {
            Fase::AguardandoResposta { pedinte, .. } => pedinte,
            _ => return Err(Erro::AcaoForaDeFase),
        };
        // R9: quem pediu leva o valor **anterior** ao pedido, não o proposto.
        let pontos = self.mao.valor;
        let mut ev = vec![Evento::Correu {
            time_que_correu: time_do_assento(assento),
            pontos,
        }];
        ev.extend(self.pontuar(Some(pedinte), pontos));
        Ok(ev)
    }

    fn pontuar(&mut self, vencedor: Option<u8>, pontos: u8) -> Vec<Evento> {
        if let Some(t) = vencedor {
            self.placar[t as usize] = self.placar[t as usize].saturating_add(pontos);
        }
        let mut ev = vec![Evento::MaoTerminou {
            vencedor,
            pontos: if vencedor.is_some() { pontos } else { 0 },
            placar: self.placar,
        }];
        self.fase = Fase::MaoEncerrada;

        // R11: na mão de ferro, quem vence a mão vence a partida (sem depender do placar).
        let campeao = match (self.mao.tipo, vencedor) {
            (TipoMao::MaoDeFerro, Some(t)) => Some(t),
            _ => (0..2u8).find(|t| self.placar[*t as usize] >= PONTOS_PARA_VENCER),
        };
        if let Some(t) = campeao {
            self.fase = Fase::PartidaEncerrada { vencedor: t };
            ev.push(Evento::PartidaTerminou {
                vencedor: t,
                placar: self.placar,
            });
        }
        ev
    }

    /// R10: na mão de onze a dupla de 11 vê as cartas do parceiro. Em nenhum outro momento.
    pub fn pode_ver_cartas_do_parceiro(&self, assento: usize) -> bool {
        matches!(self.fase, Fase::DecisaoMaoDeOnze { time } if time_do_assento(assento) == time)
    }
}

impl Mao {
    fn vazia() -> Mao {
        Mao {
            vira: Card::new(Rank::Quatro, Suit::Ouros),
            cartas: vec![],
            tipo: TipoMao::Normal,
            valor: 1,
            rodadas: vec![],
            jogadas: vec![],
            puxador: 0,
            vez: 0,
            ultimo_pedinte: None,
            degrau: 0,
        }
    }
}

/// R8 — quem vence a mão.
///
/// `None` = a mão continua. `Some(None)` = mão empatada, ninguém pontua.
/// `Some(Some(t))` = o time `t` vence a mão.
///
/// Esta função é a transcrição direta da tabela de 15 linhas de F1 (ver REGRAS.md R8) e
/// está coberta exaustivamente por `tests/regras.rs::tabela_de_empates_de_f1`.
pub fn resultado_da_mao(rodadas: &[Option<u8>]) -> Option<Option<u8>> {
    let ganhas = |t: u8| rodadas.iter().filter(|r| **r == Some(t)).count();
    let (g0, g1) = (ganhas(0), ganhas(1));
    if g0 >= 2 {
        return Some(Some(0));
    }
    if g1 >= 2 {
        return Some(Some(1));
    }
    match rodadas.len() {
        // "vencer uma e empatar outra" ganha a mão (F2).
        2 if rodadas.contains(&None) && g0 == 1 => Some(Some(0)),
        2 if rodadas.contains(&None) && g1 == 1 => Some(Some(1)),
        3 => {
            // Aqui g0 <= 1 e g1 <= 1.
            match (g0, g1) {
                // Empate na 3ª com 1x1: vence quem ganhou a primeira rodada não empatada.
                (1, 1) => Some(rodadas.iter().flatten().copied().next()),
                (1, 0) => Some(Some(0)),
                (0, 1) => Some(Some(1)),
                // Três empates: ninguém pontua (R8).
                _ => Some(None),
            }
        }
        _ => None,
    }
}
