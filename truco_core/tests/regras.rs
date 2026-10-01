//! Testes do motor contra as fontes citadas.
//!
//! Cada teste nomeia a regra (R1..R16) de
//! https://github.com/yryapu/poliorketikos-truco-zap/blob/main/REGRAS.md
//! Se um destes falha, a regra está errada — e regra errada invalida o resto.

use rand::rngs::StdRng;
use rand::SeedableRng;
use truco_core::card::CARTA_DE_COSTAS;
use truco_core::*;

const A: Option<u8> = Some(0);
const B: Option<u8> = Some(1);
const T: Option<u8> = None; // empate

// ---------------------------------------------------------------- R1, R2, D04

#[test]
fn r1_baralho_sujo_tem_40_cartas_distintas() {
    let b = Card::baralho();
    assert_eq!(b.len(), 40);
    let mut u: Vec<char> = b.iter().map(|c| c.unicode()).collect();
    u.sort_unstable();
    u.dedup();
    assert_eq!(u.len(), 40, "40 cartas => 40 code points distintos");
}

#[test]
fn d04_unicode_roundtrip_e_os_quatro_ases_do_enunciado() {
    for c in Card::baralho() {
        assert_eq!(Card::from_unicode(c.unicode()), Some(c), "roundtrip de {c:?}");
    }
    // Exatamente os caracteres que o enunciado mostra: 🂡 🂱 🃁 🃑
    assert_eq!(Card::new(Rank::As, Suit::Espadas).unicode(), '\u{1F0A1}');
    assert_eq!(Card::new(Rank::As, Suit::Copas).unicode(), '\u{1F0B1}');
    assert_eq!(Card::new(Rank::As, Suit::Ouros).unicode(), '\u{1F0C1}');
    assert_eq!(Card::new(Rank::As, Suit::Paus).unicode(), '\u{1F0D1}');
}

#[test]
fn d04_rejeita_o_que_nao_e_carta_do_truco() {
    // Cavaleiro (U+1F0xC) existe no Unicode e NAO existe no truco.
    for cp in [0x1F0AC, 0x1F0BC, 0x1F0CC, 0x1F0DC] {
        assert_eq!(Card::from_unicode(char::from_u32(cp).unwrap()), None, "{cp:X}");
    }
    // 8, 9, 10 foram removidos do baralho sujo (R1).
    for cp in [0x1F0A8, 0x1F0A9, 0x1F0AA, 0x1F0D8] {
        assert_eq!(Card::from_unicode(char::from_u32(cp).unwrap()), None, "{cp:X}");
    }
    // Costas, trunfos e lixo.
    assert_eq!(Card::from_unicode(CARTA_DE_COSTAS), None);
    assert_eq!(Card::from_unicode('\u{1F0E0}'), None);
    assert_eq!(Card::from_unicode('A'), None);
    assert_eq!(Card::from_unicode('🂠'), None);
}

#[test]
fn r2_dama_vale_menos_que_valete() {
    let vira = Card::new(Rank::Quatro, Suit::Ouros); // manilha = 5, nao interfere
    let q = Card::new(Rank::Dama, Suit::Paus).forca(vira);
    let j = Card::new(Rank::Valete, Suit::Ouros).forca(vira);
    assert!(q < j, "Dama ({q}) tem de valer menos que Valete ({j})");
}

#[test]
fn r2_ordem_base_crescente() {
    let vira = Card::new(Rank::Tres, Suit::Ouros); // manilha = 4, sai da ordem base
    let ordem = [
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
    for par in ordem.windows(2) {
        let a = Card::new(par[0], Suit::Paus).forca(vira);
        let b = Card::new(par[1], Suit::Ouros).forca(vira);
        assert!(a < b, "{:?} deve valer menos que {:?}", par[0], par[1]);
    }
}

// -------------------------------------------------------------------- R3, R4

#[test]
fn r3_manilha_e_o_rank_imediatamente_acima_da_vira_em_ordem_ciclica() {
    // Exemplos literais de F1: vira 5 -> manilha 6; vira 7 -> manilha Q; vira 3 -> manilha 4.
    let casos = [
        (Rank::Cinco, Rank::Seis),
        (Rank::Sete, Rank::Dama),
        (Rank::Tres, Rank::Quatro),
        (Rank::Valete, Rank::Rei), // exemplo "vira = Jack" de F1
        (Rank::Dama, Rank::Valete),
        (Rank::Dois, Rank::Tres),
    ];
    for (vira, manilha) in casos {
        assert_eq!(vira.proximo_ciclico(), manilha, "vira {vira:?}");
    }
}

#[test]
fn r3_exemplo_de_controle_de_f1_vira_valete() {
    // F1: "if the vira is a Jack, the cards rank from high to low:
    //      K-K-K-K - 3's - 2's - A's - J's - Q's - 7's - 6's - 5's - 4's"
    let vira = Card::new(Rank::Valete, Suit::Ouros);
    let esperado_desc = [
        Card::new(Rank::Rei, Suit::Paus),
        Card::new(Rank::Rei, Suit::Copas),
        Card::new(Rank::Rei, Suit::Espadas),
        Card::new(Rank::Rei, Suit::Ouros),
        Card::new(Rank::Tres, Suit::Ouros),
        Card::new(Rank::Dois, Suit::Ouros),
        Card::new(Rank::As, Suit::Ouros),
        Card::new(Rank::Valete, Suit::Ouros),
        Card::new(Rank::Dama, Suit::Ouros),
        Card::new(Rank::Sete, Suit::Ouros),
        Card::new(Rank::Seis, Suit::Ouros),
        Card::new(Rank::Cinco, Suit::Ouros),
        Card::new(Rank::Quatro, Suit::Ouros),
    ];
    for par in esperado_desc.windows(2) {
        assert!(
            par[0].forca(vira) > par[1].forca(vira),
            "{:?} deve ser mais forte que {:?}",
            par[0],
            par[1]
        );
    }
    assert!(esperado_desc[0].eh_manilha(vira));
    assert!(!esperado_desc[4].eh_manilha(vira), "o 3 nao e manilha aqui");
}

#[test]
fn r4_naipe_das_manilhas_ouros_espadas_copas_paus() {
    let vira = Card::new(Rank::Cinco, Suit::Ouros); // manilhas = os 6
    let f = |s| Card::new(Rank::Seis, s).forca(vira);
    assert!(f(Suit::Ouros) < f(Suit::Espadas));
    assert!(f(Suit::Espadas) < f(Suit::Copas));
    assert!(f(Suit::Copas) < f(Suit::Paus));
    assert_eq!(Suit::Paus.apelido_manilha(), "zap");
}

#[test]
fn r4_duas_manilhas_nunca_empatam_e_duas_comuns_de_mesmo_rank_sempre_empatam() {
    let vira = Card::new(Rank::Cinco, Suit::Ouros);
    let mut forcas: Vec<u8> = Suit::TODOS
        .iter()
        .map(|s| Card::new(Rank::Seis, *s).forca(vira))
        .collect();
    forcas.sort_unstable();
    forcas.dedup();
    assert_eq!(forcas.len(), 4, "as 4 manilhas tem forcas distintas");

    for s in Suit::TODOS {
        assert_eq!(
            Card::new(Rank::Tres, s).forca(vira),
            Card::new(Rank::Tres, Suit::Ouros).forca(vira),
            "cartas comuns de mesmo rank empatam independente do naipe"
        );
    }
}

// ------------------------------------------------------------------------ R8

/// A tabela exaustiva de F1, transcrita linha por linha. É o teste de ouro.
#[test]
fn r8_tabela_de_empates_de_f1() {
    // (rodadas, resultado esperado): Some(t) = time t vence, None = mao empatada.
    let tabela: &[(&[Option<u8>], Option<u8>)] = &[
        (&[A, A], Some(0)),
        (&[A, T], Some(0)),
        (&[A, B, A], Some(0)),
        (&[A, B, T], Some(0)),
        (&[A, B, B], Some(1)),
        (&[T, A], Some(0)),
        (&[T, T, A], Some(0)),
        (&[T, T, T], None),
        (&[T, T, B], Some(1)),
        (&[T, B], Some(1)),
        (&[B, A, A], Some(0)),
        (&[B, A, T], Some(1)),
        (&[B, A, B], Some(1)),
        (&[B, T], Some(1)),
        (&[B, B], Some(1)),
    ];
    assert_eq!(tabela.len(), 15, "F1 lista 15 linhas (ver ERROS.md E1)");
    for (rodadas, esperado) in tabela {
        assert_eq!(
            resultado_da_mao(rodadas),
            Some(*esperado),
            "linha {rodadas:?} de F1"
        );
    }
}

#[test]
fn r8_prefixos_incompletos_nao_decidem_a_mao() {
    for prefixo in [vec![A], vec![B], vec![T], vec![A, B], vec![B, A], vec![T, T]] {
        assert_eq!(
            resultado_da_mao(&prefixo),
            None,
            "{prefixo:?} ainda nao decide"
        );
    }
}

/// Fecha o espaço: toda sequência de 3 rodadas decide no instante correto e nunca depois.
#[test]
fn r8_espaco_completo_decide_no_momento_mais_cedo_possivel() {
    for r1 in [A, B, T] {
        for r2 in [A, B, T] {
            if let Some(res) = resultado_da_mao(&[r1, r2]) {
                // Decidiu em 2 rodadas: a 3ª não pode mudar nada.
                for r3 in [A, B, T] {
                    assert_eq!(
                        resultado_da_mao(&[r1, r2, r3]),
                        Some(res),
                        "{r1:?},{r2:?} ja decidiu {res:?}; {r3:?} nao pode mudar"
                    );
                }
            } else {
                // Não decidiu: a 3ª rodada tem de decidir sempre.
                for r3 in [A, B, T] {
                    assert!(
                        resultado_da_mao(&[r1, r2, r3]).is_some(),
                        "{r1:?},{r2:?},{r3:?} tem de decidir"
                    );
                }
            }
        }
    }
}

// ----------------------------------------------------------- motor: R5..R15

/// Mesa determinística para teste: o assento que puxa é sempre o 0.
///
/// `Match::novo` sorteia o "mão" (R13), o que é correto em produção e péssimo em teste —
/// foi exatamente o que me derrubou 5 testes na primeira execução (ver ERROS.md E2).
fn mesa(n: usize) -> Match {
    let mut m = Match::novo(n, &mut StdRng::seed_from_u64(42));
    m.mao_de_quem = 0;
    m.mao.puxador = 0;
    m.mao.vez = 0;
    m
}

/// Força uma mão conhecida: cartas de cada assento e a vira.
fn montar(m: &mut Match, vira: Card, maos: &[[Card; 3]]) {
    m.mao.vira = vira;
    m.mao.cartas = maos.iter().map(|h| h.to_vec()).collect();
    m.mao.jogadas.clear();
    m.mao.rodadas.clear();
    m.mao.puxador = 0;
    m.mao.vez = 0;
}

#[test]
fn r5_1x1_e_2x2_distribuem_3_cartas_e_uma_vira() {
    for n in [2, 4] {
        let m = mesa(n);
        assert_eq!(m.mao.cartas.len(), n);
        for h in &m.mao.cartas {
            assert_eq!(h.len(), 3);
        }
        // Nenhuma carta repetida entre as mãos e a vira.
        let mut todas: Vec<Card> = m.mao.cartas.iter().flatten().copied().collect();
        todas.push(m.mao.vira);
        todas.sort_unstable();
        let antes = todas.len();
        todas.dedup();
        assert_eq!(todas.len(), antes, "cartas nao se repetem");
    }
}

#[test]
fn r7_encoberta_proibida_na_primeira_rodada_e_permitida_depois() {
    let mut m = mesa(2);
    let c = m.mao.cartas[0][0];
    assert_eq!(
        m.aplicar(0, Acao::Jogar { carta: c, encoberta: true }),
        Err(Erro::EncobertaNaPrimeiraRodada)
    );
    // joga a 1a rodada normalmente
    m.aplicar(0, Acao::Jogar { carta: c, encoberta: false }).unwrap();
    let c1 = m.mao.cartas[1][0];
    m.aplicar(1, Acao::Jogar { carta: c1, encoberta: false }).unwrap();
    // agora encoberta vale
    let vez = m.mao.vez;
    let cx = m.mao.cartas[vez][0];
    let ev = m.aplicar(vez, Acao::Jogar { carta: cx, encoberta: true }).unwrap();
    match &ev[0] {
        Evento::CartaJogada(j) => {
            assert!(j.encoberta);
            assert_eq!(j.carta, None, "encoberta nao revela a carta");
        }
        outro => panic!("esperava CartaJogada, veio {outro:?}"),
    }
}

#[test]
fn r7_encoberta_nunca_vence_a_rodada() {
    let mut m = mesa(2);
    let vira = Card::new(Rank::Quatro, Suit::Ouros); // manilha = 5
    montar(
        &mut m,
        vira,
        &[
            [Card::new(Rank::Quatro, Suit::Paus), Card::new(Rank::Cinco, Suit::Paus), Card::new(Rank::Seis, Suit::Paus)],
            [Card::new(Rank::Seis, Suit::Ouros), Card::new(Rank::Sete, Suit::Ouros), Card::new(Rank::Dama, Suit::Ouros)],
        ],
    );
    // rodada 1: assento 1 ganha com 6 contra 4
    m.aplicar(0, Acao::Jogar { carta: Card::new(Rank::Quatro, Suit::Paus), encoberta: false }).unwrap();
    m.aplicar(1, Acao::Jogar { carta: Card::new(Rank::Seis, Suit::Ouros), encoberta: false }).unwrap();
    assert_eq!(m.mao.rodadas, vec![Some(1)]);
    // rodada 2: assento 1 puxa e joga encoberto o 7; assento 0 joga o 4... perdao, o 5 (manilha)
    m.aplicar(1, Acao::Jogar { carta: Card::new(Rank::Sete, Suit::Ouros), encoberta: true }).unwrap();
    m.aplicar(0, Acao::Jogar { carta: Card::new(Rank::Cinco, Suit::Paus), encoberta: false }).unwrap();
    assert_eq!(m.mao.rodadas, vec![Some(1), Some(0)], "encoberta nao ganha nem empata");
}

#[test]
fn r6_parceiros_com_cartas_iguais_vencem_a_rodada_em_vez_de_empatar() {
    // F1: "the trick is not tied, but is won by the team that played the highest cards".
    let mut m = mesa(4);
    let vira = Card::new(Rank::Quatro, Suit::Ouros); // manilha = 5
    let tres = |s| Card::new(Rank::Tres, s);
    let seis = |s| Card::new(Rank::Seis, s);
    montar(
        &mut m,
        vira,
        &[
            [tres(Suit::Paus), seis(Suit::Paus), seis(Suit::Copas)],
            [seis(Suit::Ouros), tres(Suit::Copas), tres(Suit::Espadas)],
            [tres(Suit::Ouros), seis(Suit::Espadas), Card::new(Rank::Sete, Suit::Paus)],
            [Card::new(Rank::Sete, Suit::Ouros), Card::new(Rank::Sete, Suit::Copas), Card::new(Rank::Sete, Suit::Espadas)],
        ],
    );
    m.aplicar(0, Acao::Jogar { carta: tres(Suit::Paus), encoberta: false }).unwrap();
    m.aplicar(1, Acao::Jogar { carta: seis(Suit::Ouros), encoberta: false }).unwrap();
    m.aplicar(2, Acao::Jogar { carta: tres(Suit::Ouros), encoberta: false }).unwrap();
    m.aplicar(3, Acao::Jogar { carta: Card::new(Rank::Sete, Suit::Ouros), encoberta: false }).unwrap();
    assert_eq!(
        m.mao.rodadas,
        vec![Some(0)],
        "assentos 0 e 2 (mesmo time) empataram no topo => o time vence"
    );
    assert_eq!(m.mao.puxador, 0, "puxa quem jogou a primeira das cartas mais altas");
}

#[test]
fn d07_apos_empate_puxa_quem_jogou_a_primeira_carta_que_empatou() {
    let mut m = mesa(4);
    let vira = Card::new(Rank::Quatro, Suit::Ouros);
    let tres = |s| Card::new(Rank::Tres, s);
    let dois = |s| Card::new(Rank::Dois, s);
    montar(
        &mut m,
        vira,
        &[
            [dois(Suit::Paus), dois(Suit::Copas), dois(Suit::Espadas)],
            [tres(Suit::Ouros), tres(Suit::Copas), tres(Suit::Espadas)],
            [tres(Suit::Paus), dois(Suit::Ouros), Card::new(Rank::Sete, Suit::Paus)],
            [Card::new(Rank::Sete, Suit::Ouros), Card::new(Rank::Sete, Suit::Copas), Card::new(Rank::Sete, Suit::Espadas)],
        ],
    );
    m.aplicar(0, Acao::Jogar { carta: dois(Suit::Paus), encoberta: false }).unwrap();
    m.aplicar(1, Acao::Jogar { carta: tres(Suit::Ouros), encoberta: false }).unwrap(); // 1o do topo
    m.aplicar(2, Acao::Jogar { carta: tres(Suit::Paus), encoberta: false }).unwrap(); // empata
    m.aplicar(3, Acao::Jogar { carta: Card::new(Rank::Sete, Suit::Ouros), encoberta: false }).unwrap();
    assert_eq!(m.mao.rodadas, vec![None], "3 contra 3 de times opostos = empate");
    assert_eq!(m.mao.puxador, 1, "assento 1 jogou a primeira carta que empatou");
}

#[test]
fn r9_escada_1_3_6_9_12_e_quem_corre_paga_o_valor_anterior() {
    let mut m = mesa(2);
    assert_eq!(m.mao.valor, 1);
    m.aplicar(0, Acao::Pedir).unwrap(); // truco: propoe 3
    assert_eq!(m.fase, Fase::AguardandoResposta { pedinte: 0, respondendo: 1, proposto: 3 });
    m.aplicar(1, Acao::Aumentar).unwrap(); // aceita 3 e propoe 6
    assert_eq!(m.mao.valor, 3);
    assert_eq!(m.fase, Fase::AguardandoResposta { pedinte: 1, respondendo: 0, proposto: 6 });
    m.aplicar(0, Acao::Aumentar).unwrap(); // aceita 6 e propoe 9
    assert_eq!(m.mao.valor, 6);
    m.aplicar(1, Acao::Aumentar).unwrap(); // aceita 9 e propoe 12
    assert_eq!(m.mao.valor, 9);
    assert_eq!(m.fase, Fase::AguardandoResposta { pedinte: 1, respondendo: 0, proposto: 12 });
    // no 12 nao se aumenta mais (R9)
    assert_eq!(m.aplicar(0, Acao::Aumentar), Err(Erro::ValorMaximoAtingido));
    // correndo do 12, quem pediu leva 9 — o valor anterior
    let ev = m.aplicar(0, Acao::Correr).unwrap();
    assert!(ev.contains(&Evento::Correu { time_que_correu: 0, pontos: 9 }), "{ev:?}");
    assert_eq!(m.placar, [0, 9]);
}

#[test]
fn r9_mesmo_time_nao_pode_pedir_duas_vezes_seguidas() {
    let mut m = mesa(2);
    m.aplicar(0, Acao::Pedir).unwrap();
    m.aplicar(1, Acao::Aceitar).unwrap();
    assert_eq!(m.mao.valor, 3);
    assert_eq!(m.aplicar(0, Acao::Pedir), Err(Erro::SeuTimeFezOUltimoPedido));
    // o adversario pode
    assert!(m.aplicar(1, Acao::Pedir).is_err(), "nao e a vez do assento 1 jogar/pedir");
    m.aplicar(0, Acao::Jogar { carta: m.mao.cartas[0][0], encoberta: false }).unwrap();
    m.aplicar(1, Acao::Pedir).unwrap();
    assert_eq!(m.fase, Fase::AguardandoResposta { pedinte: 1, respondendo: 0, proposto: 6 });
}

#[test]
fn r9_correr_do_truco_da_1_ponto() {
    let mut m = mesa(2);
    m.aplicar(0, Acao::Pedir).unwrap();
    m.aplicar(1, Acao::Correr).unwrap();
    assert_eq!(m.placar, [1, 0], "correr do truco paga 1, nao 3");
    assert_eq!(m.fase, Fase::MaoEncerrada);
}

#[test]
fn r10_mao_de_onze_vale_3_recusar_da_1_ao_adversario_e_truco_e_proibido() {
    let mut m = mesa(2);
    m.placar = [11, 5];
    m.nova_mao(&mut StdRng::seed_from_u64(7));
    m.mao.puxador = 0;
    m.mao.vez = 0;
    assert_eq!(m.mao.tipo, TipoMao::MaoDeOnze { time: 0 });
    assert_eq!(m.mao.valor, 3);
    assert_eq!(m.fase, Fase::DecisaoMaoDeOnze { time: 0 });
    assert!(m.pode_ver_cartas_do_parceiro(0));
    assert!(!m.pode_ver_cartas_do_parceiro(1));
    assert_eq!(m.aplicar(0, Acao::Pedir), Err(Erro::TrucoProibidoMaoDeOnze));
    // o time que nao tem 11 nao decide nada
    assert_eq!(m.aplicar(1, Acao::MaoDeOnzeCorrer), Err(Erro::NaoEhSuaVez));
    m.aplicar(0, Acao::MaoDeOnzeCorrer).unwrap();
    assert_eq!(m.placar, [11, 6], "recusar da 1 ponto ao adversario");
}

#[test]
fn r10_mao_de_onze_aceita_e_ganha_vence_a_partida_com_3() {
    let mut m = mesa(2);
    m.placar = [11, 0];
    m.nova_mao(&mut StdRng::seed_from_u64(7));
    m.mao.puxador = 0;
    m.mao.vez = 0;
    m.aplicar(0, Acao::MaoDeOnzeJogar).unwrap();
    assert_eq!(m.fase, Fase::Jogando);
    assert_eq!(m.aplicar(0, Acao::Pedir), Err(Erro::TrucoProibidoMaoDeOnze), "sem truco na mao de onze");
    // forca o time 0 a ganhar as duas primeiras rodadas
    let vira = Card::new(Rank::Quatro, Suit::Ouros);
    montar(&mut m, vira, &[
        [Card::new(Rank::Cinco, Suit::Paus), Card::new(Rank::Cinco, Suit::Copas), Card::new(Rank::Quatro, Suit::Paus)],
        [Card::new(Rank::Sete, Suit::Ouros), Card::new(Rank::Seis, Suit::Ouros), Card::new(Rank::Cinco, Suit::Ouros)],
    ]);
    m.aplicar(0, Acao::Jogar { carta: Card::new(Rank::Cinco, Suit::Paus), encoberta: false }).unwrap();
    m.aplicar(1, Acao::Jogar { carta: Card::new(Rank::Sete, Suit::Ouros), encoberta: false }).unwrap();
    let ev = m.aplicar(0, Acao::Jogar { carta: Card::new(Rank::Cinco, Suit::Copas), encoberta: false })
        .and_then(|mut e| { e.extend(m.aplicar(1, Acao::Jogar { carta: Card::new(Rank::Seis, Suit::Ouros), encoberta: false })?); Ok(e) })
        .unwrap();
    assert_eq!(m.placar, [14, 0]);
    assert!(matches!(m.fase, Fase::PartidaEncerrada { vencedor: 0 }), "{:?}", m.fase);
    assert!(ev.iter().any(|e| matches!(e, Evento::PartidaTerminou { vencedor: 0, .. })), "{ev:?}");
}

#[test]
fn r11_mao_de_ferro_vale_1_sem_truco_e_quem_ganha_a_mao_ganha_a_partida() {
    let mut m = mesa(2);
    m.placar = [11, 11];
    m.nova_mao(&mut StdRng::seed_from_u64(9));
    m.mao.puxador = 0;
    m.mao.vez = 0;
    assert_eq!(m.mao.tipo, TipoMao::MaoDeFerro);
    assert_eq!(m.mao.valor, 1);
    assert_eq!(m.fase, Fase::Jogando, "ninguem decide na mao de ferro");
    assert_eq!(m.aplicar(0, Acao::Pedir), Err(Erro::TrucoProibidoMaoDeOnze));
    assert!(!m.pode_ver_cartas_do_parceiro(0));
    let vira = Card::new(Rank::Quatro, Suit::Ouros);
    montar(&mut m, vira, &[
        [Card::new(Rank::Cinco, Suit::Paus), Card::new(Rank::Cinco, Suit::Copas), Card::new(Rank::Quatro, Suit::Paus)],
        [Card::new(Rank::Sete, Suit::Ouros), Card::new(Rank::Seis, Suit::Ouros), Card::new(Rank::Cinco, Suit::Ouros)],
    ]);
    m.aplicar(0, Acao::Jogar { carta: Card::new(Rank::Cinco, Suit::Paus), encoberta: false }).unwrap();
    m.aplicar(1, Acao::Jogar { carta: Card::new(Rank::Sete, Suit::Ouros), encoberta: false }).unwrap();
    m.aplicar(0, Acao::Jogar { carta: Card::new(Rank::Cinco, Suit::Copas), encoberta: false }).unwrap();
    m.aplicar(1, Acao::Jogar { carta: Card::new(Rank::Seis, Suit::Ouros), encoberta: false }).unwrap();
    assert!(matches!(m.fase, Fase::PartidaEncerrada { vencedor: 0 }), "{:?}", m.fase);
    assert_eq!(m.placar, [12, 11]);
}

#[test]
fn r12_partida_acaba_ao_chegar_ou_passar_de_12() {
    // Exemplo literal de F2: 9x9, uma mao trucada de 3 => 12x9.
    let mut m = mesa(2);
    m.placar = [9, 9];
    m.nova_mao(&mut StdRng::seed_from_u64(3));
    m.mao.puxador = 0;
    m.mao.vez = 0;
    m.aplicar(0, Acao::Pedir).unwrap();
    m.aplicar(1, Acao::Aceitar).unwrap();
    let vira = Card::new(Rank::Quatro, Suit::Ouros);
    montar(&mut m, vira, &[
        [Card::new(Rank::Cinco, Suit::Paus), Card::new(Rank::Cinco, Suit::Copas), Card::new(Rank::Quatro, Suit::Paus)],
        [Card::new(Rank::Sete, Suit::Ouros), Card::new(Rank::Seis, Suit::Ouros), Card::new(Rank::Cinco, Suit::Ouros)],
    ]);
    for (c0, c1) in [
        (Card::new(Rank::Cinco, Suit::Paus), Card::new(Rank::Sete, Suit::Ouros)),
        (Card::new(Rank::Cinco, Suit::Copas), Card::new(Rank::Seis, Suit::Ouros)),
    ] {
        m.aplicar(0, Acao::Jogar { carta: c0, encoberta: false }).unwrap();
        m.aplicar(1, Acao::Jogar { carta: c1, encoberta: false }).unwrap();
    }
    assert_eq!(m.placar, [12, 9]);
    assert!(matches!(m.fase, Fase::PartidaEncerrada { vencedor: 0 }));
}

#[test]
fn r13_d16_o_mao_rotaciona_um_assento_contra_a_ordem_de_jogo() {
    // F2: "o jogador que começa a primeira rodada é sempre o da esquerda ao que começou a mão
    // anterior", e a ordem de jogo é anti-horária (o próximo é o da direita, +1). Logo o mão
    // anda -1. F1 diz +1; a divergência está resolvida em D16 a favor de F2.
    //
    // O teste antigo afirmava `+1` — isto é, testava a implementação, não a fonte. Foi assim
    // que ele passou enquanto o código discordava da spec (ver ERROS.md E8).
    let mut m = mesa(4);
    let primeiro = m.mao_de_quem;
    for i in 1..=4usize {
        m.nova_mao(&mut StdRng::seed_from_u64(i as u64));
        assert_eq!(
            m.mao_de_quem,
            (primeiro + 4 - (i % 4)) % 4,
            "mao {i}: o mao anda contra a ordem de jogo"
        );
    }
    // Depois de 4 mãos numa mesa de 4, todo mundo foi mão exatamente uma vez.
    assert_eq!(m.mao_de_quem, primeiro);
}

#[test]
fn seguranca_nao_da_para_jogar_carta_que_nao_esta_na_mao() {
    let mut m = mesa(2);
    let minha = m.mao.cartas[0].clone();
    let alheia = Card::baralho()
        .into_iter()
        .find(|c| !minha.contains(c) && !m.mao.cartas[1].contains(c))
        .unwrap();
    assert_eq!(
        m.aplicar(0, Acao::Jogar { carta: alheia, encoberta: false }),
        Err(Erro::CartaNaoEstaNaSuaMao)
    );
    // e nem jogar a mesma carta duas vezes
    m.aplicar(0, Acao::Jogar { carta: minha[0], encoberta: false }).unwrap();
    m.aplicar(1, Acao::Jogar { carta: m.mao.cartas[1][0], encoberta: false }).unwrap();
    let vez = m.mao.vez;
    assert_eq!(
        m.aplicar(vez, Acao::Jogar { carta: minha[0], encoberta: false }),
        Err(Erro::CartaNaoEstaNaSuaMao)
    );
}

#[test]
fn seguranca_so_quem_tem_a_vez_joga() {
    let mut m = mesa(4);
    let vez = m.mao.vez;
    for a in 0..4 {
        if a != vez {
            assert_eq!(
                m.aplicar(a, Acao::Jogar { carta: m.mao.cartas[a][0], encoberta: false }),
                Err(Erro::NaoEhSuaVez),
                "assento {a} nao tem a vez"
            );
        }
    }
}

/// Fuzz: 2000 partidas inteiras com jogadas aleatórias legais. Nenhuma deve travar,
/// panicar, ou terminar com placar impossível.
#[test]
fn fuzz_partidas_completas_nunca_travam_nem_produzem_placar_impossivel() {
    use rand::Rng;
    for semente in 0..2000u64 {
        let mut rng = StdRng::seed_from_u64(semente);
        let n = if semente % 2 == 0 { 2 } else { 4 };
        let mut m = Match::novo(n, &mut rng);
        let mut passos = 0;
        let mut placar_anterior = m.placar;
        loop {
            passos += 1;
            // R9: só 0, 1, 3, 6, 9 ou 12 podem ser creditados por mão. Checar o DELTA é o
            // invariante certo; checar só o total deixaria passar um crédito de 2.
            let delta = [
                m.placar[0] as i32 - placar_anterior[0] as i32,
                m.placar[1] as i32 - placar_anterior[1] as i32,
            ];
            for d in delta {
                assert!(
                    [0, 1, 3, 6, 9, 12].contains(&d),
                    "semente {semente}: creditou {d} pontos numa mao"
                );
            }
            assert!(
                delta[0] == 0 || delta[1] == 0,
                "semente {semente}: as duas duplas pontuaram na mesma mao"
            );
            placar_anterior = m.placar;
            assert!(passos < 10_000, "semente {semente} travou");
            match m.fase {
                Fase::PartidaEncerrada { vencedor } => {
                    assert!(
                        m.placar[vencedor as usize] >= PONTOS_PARA_VENCER
                            || m.mao.tipo == TipoMao::MaoDeFerro,
                        "semente {semente}: venceu com placar {:?}", m.placar
                    );
                    break;
                }
                Fase::MaoEncerrada => {
                    m.nova_mao(&mut rng);
                }
                Fase::DecisaoMaoDeOnze { time } => {
                    let a = (0..n).find(|a| time_do_assento(*a) == time).unwrap();
                    let acao = if rng.gen_bool(0.7) { Acao::MaoDeOnzeJogar } else { Acao::MaoDeOnzeCorrer };
                    m.aplicar(a, acao).unwrap();
                }
                Fase::AguardandoResposta { respondendo, proposto, .. } => {
                    let a = (0..n).find(|a| time_do_assento(*a) == respondendo).unwrap();
                    // R18: aumentar é ilegal se aceitar já venceria. O fuzz respeita as regras;
                    // quem testa a recusa é `r18_...` abaixo.
                    let pode_aumentar = proposto < 12
                        && m.placar[respondendo as usize] + proposto < PONTOS_PARA_VENCER;
                    let acao = match rng.gen_range(0..3) {
                        0 => Acao::Correr,
                        _ if pode_aumentar && rng.gen_bool(0.5) => Acao::Aumentar,
                        _ => Acao::Aceitar,
                    };
                    m.aplicar(a, acao).unwrap();
                }
                Fase::Jogando => {
                    let a = m.mao.vez;
                    if m.mao.tipo == TipoMao::Normal
                        && m.mao.ultimo_pedinte != Some(time_do_assento(a))
                        && m.mao.valor < 12
                        && rng.gen_bool(0.15)
                    {
                        m.aplicar(a, Acao::Pedir).unwrap();
                        continue;
                    }
                    let mao = m.mao.cartas[a].clone();
                    assert!(!mao.is_empty(), "semente {semente}: assento {a} sem cartas na fase Jogando");
                    let carta = mao[rng.gen_range(0..mao.len())];
                    let encoberta = !m.mao.rodadas.is_empty() && rng.gen_bool(0.2);
                    m.aplicar(a, Acao::Jogar { carta, encoberta }).unwrap();
                }
            }
        }
        // Só 0, 1, 3, 6, 9 ou 12 podem ter sido somados; o placar nunca passa de 23.
        assert!(m.placar[0] <= 23 && m.placar[1] <= 23, "semente {semente}: {:?}", m.placar);
    }
}

// ─────────────────────── lacunas apontadas pela revisão adversarial ───────────────────────
// Os testes abaixo cobrem regras que estavam implementadas (ou ausentes) e sem teste. A
// revisão que as encontrou está creditada em ERROS.md E8.

#[test]
fn r2_a_ordem_base_inclui_o_par_quatro_menor_que_cinco() {
    // Os dois testes antigos de R2 usavam vira 3 (tira o 4) e vira 4 (tira o 5), então a
    // relação 4 < 5 da ordem base nunca era afirmada por ninguém.
    let vira = Card::new(Rank::Valete, Suit::Ouros); // manilha = K, nem 4 nem 5 saem
    assert!(
        Card::new(Rank::Quatro, Suit::Paus).forca(vira) < Card::new(Rank::Cinco, Suit::Ouros).forca(vira),
        "4 tem de valer menos que 5"
    );
    // E a cadeia inteira, numa mao em que nenhum dos dez ranks e manilha... impossivel:
    // a manilha sempre tira um rank. Entao confiro a cadeia com duas viras complementares.
    for vira_rank in [Rank::Valete, Rank::Tres] {
        let v = Card::new(vira_rank, Suit::Ouros);
        let manilha = vira_rank.proximo_ciclico();
        let base: Vec<Rank> = Rank::TODOS.into_iter().filter(|r| *r != manilha).collect();
        for par in base.windows(2) {
            assert!(
                Card::new(par[0], Suit::Paus).forca(v) < Card::new(par[1], Suit::Ouros).forca(v),
                "vira {vira_rank:?}: {:?} < {:?}", par[0], par[1]
            );
        }
    }
}

#[test]
fn r7_rodada_inteira_encoberta_empata_e_nao_trava() {
    let mut m = mesa(2);
    let vira = Card::new(Rank::Quatro, Suit::Ouros);
    montar(&mut m, vira, &[
        [Card::new(Rank::Tres, Suit::Paus), Card::new(Rank::Dois, Suit::Paus), Card::new(Rank::Sete, Suit::Paus)],
        [Card::new(Rank::Tres, Suit::Ouros), Card::new(Rank::Dois, Suit::Ouros), Card::new(Rank::Sete, Suit::Ouros)],
    ]);
    // Rodada 1 empatada (3 contra 3, nenhum e manilha).
    m.aplicar(0, Acao::Jogar { carta: Card::new(Rank::Tres, Suit::Paus), encoberta: false }).unwrap();
    m.aplicar(1, Acao::Jogar { carta: Card::new(Rank::Tres, Suit::Ouros), encoberta: false }).unwrap();
    assert_eq!(m.mao.rodadas, vec![None]);
    let puxador = m.mao.puxador;
    // Rodada 2: os DOIS jogam encoberto. Ninguem vence; quem puxou a rodada puxa a proxima.
    let a = m.mao.vez;
    m.aplicar(a, Acao::Jogar { carta: m.mao.cartas[a][0], encoberta: true }).unwrap();
    let b = m.mao.vez;
    m.aplicar(b, Acao::Jogar { carta: m.mao.cartas[b][0], encoberta: true }).unwrap();
    assert_eq!(m.mao.rodadas, vec![None, None], "tudo encoberto nao da vencedor");
    assert_eq!(m.mao.puxador, puxador, "quem puxou a rodada toda encoberta puxa a proxima");
    assert_eq!(m.fase, Fase::Jogando, "a mao continua para a terceira rodada");
}

#[test]
fn r8_tres_rodadas_empatadas_pelo_match_nao_creditam_ponto_a_ninguem() {
    // A tabela de R8 e testada em isolamento; este teste exercita o caminho pelo `Match`,
    // que e onde `pontuar` poderia creditar pontos numa mao empatada.
    let mut m = mesa(2);
    let vira = Card::new(Rank::Quatro, Suit::Ouros); // manilha = 5, nenhuma em jogo
    montar(&mut m, vira, &[
        [Card::new(Rank::Tres, Suit::Paus), Card::new(Rank::Dois, Suit::Paus), Card::new(Rank::Sete, Suit::Paus)],
        [Card::new(Rank::Tres, Suit::Ouros), Card::new(Rank::Dois, Suit::Ouros), Card::new(Rank::Sete, Suit::Ouros)],
    ]);
    for (c0, c1) in [
        (Card::new(Rank::Tres, Suit::Paus), Card::new(Rank::Tres, Suit::Ouros)),
        (Card::new(Rank::Dois, Suit::Paus), Card::new(Rank::Dois, Suit::Ouros)),
        (Card::new(Rank::Sete, Suit::Paus), Card::new(Rank::Sete, Suit::Ouros)),
    ] {
        let a = m.mao.vez;
        let (primeira, segunda) = if a == 0 { (c0, c1) } else { (c1, c0) };
        m.aplicar(a, Acao::Jogar { carta: primeira, encoberta: false }).unwrap();
        let b = m.mao.vez;
        m.aplicar(b, Acao::Jogar { carta: segunda, encoberta: false }).unwrap();
    }
    assert_eq!(m.mao.rodadas, vec![None, None, None]);
    assert_eq!(m.placar, [0, 0], "mao empatada nao credita ponto a ninguem");
    assert_eq!(m.fase, Fase::MaoEncerrada);
}

#[test]
fn r11_mao_de_ferro_empatada_leva_a_outra_mao_de_ferro() {
    let mut m = mesa(2);
    m.placar = [11, 11];
    m.nova_mao(&mut StdRng::seed_from_u64(5));
    assert_eq!(m.mao.tipo, TipoMao::MaoDeFerro);
    m.mao.puxador = 0;
    m.mao.vez = 0;
    let vira = Card::new(Rank::Quatro, Suit::Ouros);
    montar(&mut m, vira, &[
        [Card::new(Rank::Tres, Suit::Paus), Card::new(Rank::Dois, Suit::Paus), Card::new(Rank::Sete, Suit::Paus)],
        [Card::new(Rank::Tres, Suit::Ouros), Card::new(Rank::Dois, Suit::Ouros), Card::new(Rank::Sete, Suit::Ouros)],
    ]);
    for (c0, c1) in [
        (Card::new(Rank::Tres, Suit::Paus), Card::new(Rank::Tres, Suit::Ouros)),
        (Card::new(Rank::Dois, Suit::Paus), Card::new(Rank::Dois, Suit::Ouros)),
        (Card::new(Rank::Sete, Suit::Paus), Card::new(Rank::Sete, Suit::Ouros)),
    ] {
        let a = m.mao.vez;
        let (p, q) = if a == 0 { (c0, c1) } else { (c1, c0) };
        m.aplicar(a, Acao::Jogar { carta: p, encoberta: false }).unwrap();
        let b = m.mao.vez;
        m.aplicar(b, Acao::Jogar { carta: q, encoberta: false }).unwrap();
    }
    assert_eq!(m.placar, [11, 11], "ferro empatada nao decide nada");
    assert_eq!(m.fase, Fase::MaoEncerrada, "a partida NAO acabou");
    m.nova_mao(&mut StdRng::seed_from_u64(6));
    assert_eq!(m.mao.tipo, TipoMao::MaoDeFerro, "joga-se outra mao de ferro");
}

#[test]
fn r9_r10_qualquer_um_da_dupla_responde_em_2x2() {
    // F2: "Qualquer um dos jogadores pode responder o pedido e vale a primeira resposta dada."
    // Todos os testes de truco usavam mesa(2), onde "qualquer um do time" e um so.
    let mut m = mesa(4);
    m.aplicar(0, Acao::Pedir).unwrap();
    assert_eq!(m.fase, Fase::AguardandoResposta { pedinte: 0, respondendo: 1, proposto: 3 });
    assert!(m.pode_agir(1) && m.pode_agir(3), "os dois do time que responde podem");
    assert!(!m.pode_agir(0) && !m.pode_agir(2), "o time que pediu nao responde");
    // O assento 3 (parceiro do 1, e nao o proximo a jogar) responde, e vale.
    m.aplicar(3, Acao::Aceitar).unwrap();
    assert_eq!(m.mao.valor, 3);

    // Mesmo na mao de onze.
    let mut m = mesa(4);
    m.placar = [11, 4];
    m.nova_mao(&mut StdRng::seed_from_u64(2));
    assert_eq!(m.fase, Fase::DecisaoMaoDeOnze { time: 0 });
    assert!(m.pode_agir(0) && m.pode_agir(2));
    assert!(!m.pode_agir(1) && !m.pode_agir(3));
    m.aplicar(2, Acao::MaoDeOnzeCorrer).unwrap();
    assert_eq!(m.placar, [11, 5]);
}

#[test]
fn r10_r17_quem_decide_ve_a_mao_do_parceiro_e_so_quem_decide() {
    // R10 em 2x2: a dupla de 11 ve as cartas um do outro.
    let mut m = mesa(4);
    m.placar = [4, 11];
    m.nova_mao(&mut StdRng::seed_from_u64(8));
    assert_eq!(m.mao.tipo, TipoMao::MaoDeOnze { time: 1 });
    assert!(m.pode_ver_cartas_do_parceiro(1) && m.pode_ver_cartas_do_parceiro(3));
    assert!(!m.pode_ver_cartas_do_parceiro(0) && !m.pode_ver_cartas_do_parceiro(2));

    // R17: ao responder um pedido, a dupla que responde ve as cartas um do outro.
    let mut m = mesa(4);
    for a in 0..4 {
        assert!(!m.pode_ver_cartas_do_parceiro(a), "fora de decisao ninguem ve nada");
    }
    m.aplicar(0, Acao::Pedir).unwrap();
    assert!(m.pode_ver_cartas_do_parceiro(1) && m.pode_ver_cartas_do_parceiro(3),
            "o time que responde ao truco pode ver (R17)");
    assert!(!m.pode_ver_cartas_do_parceiro(0) && !m.pode_ver_cartas_do_parceiro(2),
            "quem pediu nao ve — ainda");
    // F1: ao pedirem 6, o time que pediu truco passa a poder ver antes de responder.
    m.aplicar(1, Acao::Aumentar).unwrap();
    assert!(m.pode_ver_cartas_do_parceiro(0) && m.pode_ver_cartas_do_parceiro(2));
    assert!(!m.pode_ver_cartas_do_parceiro(1) && !m.pode_ver_cartas_do_parceiro(3));
    m.aplicar(0, Acao::Aceitar).unwrap();
    for a in 0..4 {
        assert!(!m.pode_ver_cartas_do_parceiro(a), "resolvido o pedido, ninguem mais ve");
    }
}

#[test]
fn r18_ilegal_aumentar_se_aceitar_ja_vencesse_a_partida() {
    // Exemplo literal de F1: A tem 7, B tem 5. A pede truco, B pede 6. A nao pode pedir 9,
    // porque aceitar 6 ja lhe daria 13.
    let mut m = mesa(2);
    m.placar = [7, 5];
    m.aplicar(0, Acao::Pedir).unwrap();            // A propoe 3
    m.aplicar(1, Acao::Aumentar).unwrap();         // B aceita 3 e propoe 6
    assert_eq!(m.mao.valor, 3);
    assert_eq!(m.fase, Fase::AguardandoResposta { pedinte: 1, respondendo: 0, proposto: 6 });
    assert_eq!(m.aplicar(0, Acao::Aumentar), Err(Erro::AumentoDesnecessario));
    // Aceitar e correr continuam legais.
    m.aplicar(0, Acao::Aceitar).unwrap();
    assert_eq!(m.mao.valor, 6);

    // Com placar baixo, aumentar segue permitido.
    let mut m = mesa(2);
    m.placar = [0, 0];
    m.aplicar(0, Acao::Pedir).unwrap();
    m.aplicar(1, Acao::Aumentar).unwrap();
    assert!(m.aplicar(0, Acao::Aumentar).is_ok(), "0x0: aumentar para 9 e legal");
}

#[test]
fn seguranca_assento_inexistente_nao_age_nem_pela_paridade() {
    // Sem validacao de assento, `pode_agir` nas fases de resposta so olhava `assento % 2`,
    // entao o assento 99 (impar) podia correr pelo time 1 numa mesa de 2.
    let mut m = mesa(2);
    m.aplicar(0, Acao::Pedir).unwrap();
    assert_eq!(m.aplicar(99, Acao::Correr), Err(Erro::AssentoInexistente));
    assert_eq!(m.aplicar(3, Acao::Aceitar), Err(Erro::AssentoInexistente));
    assert_eq!(m.placar, [0, 0], "nada aconteceu");
    assert_eq!(m.fase, Fase::AguardandoResposta { pedinte: 0, respondendo: 1, proposto: 3 });

    let mut m = mesa(2);
    m.placar = [11, 0];
    m.nova_mao(&mut StdRng::seed_from_u64(1));
    assert_eq!(m.aplicar(2, Acao::MaoDeOnzeCorrer), Err(Erro::AssentoInexistente));
    assert_eq!(m.aplicar(4, Acao::MaoDeOnzeJogar), Err(Erro::AssentoInexistente));
}

#[test]
fn r16_d11_o_1x1_e_a_mesma_regra_com_time_de_um() {
    // D11 declara o 1x1 como extrapolacao: nada no motor assume 2 jogadores por time.
    let mut m = mesa(2);
    assert_eq!(m.jogadores, 2);
    assert_eq!(time_do_assento(0), 0);
    assert_eq!(time_do_assento(1), 1);
    // Mao de onze no 1x1: o proprio jogador decide, e "ver o parceiro" e ver a propria mao.
    m.placar = [11, 2];
    m.nova_mao(&mut StdRng::seed_from_u64(4));
    assert_eq!(m.fase, Fase::DecisaoMaoDeOnze { time: 0 });
    assert!(m.pode_agir(0) && !m.pode_agir(1));
    assert_eq!(m.mao.valor, 3);
    // Mao de ferro no 1x1.
    let mut m = mesa(2);
    m.placar = [11, 11];
    m.nova_mao(&mut StdRng::seed_from_u64(4));
    assert_eq!(m.mao.tipo, TipoMao::MaoDeFerro);
    assert_eq!(m.fase, Fase::Jogando, "ninguem decide na mao de ferro, nem no 1x1");
}
