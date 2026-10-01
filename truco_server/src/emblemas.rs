//! Emblemas de reputação, por vitórias e por histórico.
//!
//! Função pura sobre o perfil — o cliente não calcula emblema nenhum, só desenha o que vem.

use crate::db::Jogador;
use serde::Serialize;

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Emblema {
    pub chave: &'static str,
    pub nome: &'static str,
    pub icone: &'static str,
    pub motivo: String,
}

pub fn de(j: &Jogador) -> Vec<Emblema> {
    let mut v = Vec::new();
    let em = |chave, nome, icone, motivo: String| Emblema {
        chave,
        nome,
        icone,
        motivo,
    };

    // Trilha por vitórias.
    let (chave, nome, icone) = match j.vitorias {
        0 => ("estreante", "Estreante", "🪶"),
        1..=4 => ("pe_de_meia", "Pé-de-meia", "🌱"),
        5..=14 => ("bom_de_truco", "Bom de truco", "🎴"),
        15..=39 => ("zap", "Zap", "🃑"),
        _ => ("lenda", "Lenda do truco", "👑"),
    };
    v.push(em(
        chave,
        nome,
        icone,
        match j.vitorias {
            0 => "ainda sem vitórias".to_string(),
            1 => "1 vitória".to_string(),
            n => format!("{n} vitórias"),
        },
    ));

    // Trilha por histórico.
    if j.partidas() >= 25 {
        v.push(em(
            "veterano",
            "Veterano",
            "🏛️",
            format!("{} partidas jogadas", j.partidas()),
        ));
    }
    if j.vitorias >= 5 && j.derrotas == 0 {
        v.push(em(
            "invicto",
            "Invicto",
            "🛡️",
            format!("{} vitórias, nenhuma derrota", j.vitorias),
        ));
    }
    if j.derrotas >= 10 && j.vitorias >= j.derrotas {
        v.push(em(
            "ressurgido",
            "Ressurgido",
            "🔥",
            format!(
                "virou o jogo: {} vitórias contra {} derrotas",
                j.vitorias, j.derrotas
            ),
        ));
    }
    if j.saldo >= 5000 {
        v.push(em(
            "fortuna",
            "Fortuna",
            "💰",
            format!("{} moedas em caixa", j.saldo),
        ));
    }
    if j.saldo == 0 && j.partidas() > 0 {
        v.push(em("quebrado", "Quebrado", "🫙", "sem uma moeda".into()));
    }
    v
}

#[cfg(test)]
mod testes {
    use super::*;

    fn jog(vitorias: i64, derrotas: i64, saldo: i64) -> Jogador {
        Jogador {
            id: "x".into(),
            apelido: "x".into(),
            senha_hash: String::new(),
            saldo,
            vitorias,
            derrotas,
            convidado: 0,
        }
    }

    #[test]
    fn todo_jogador_tem_exatamente_um_emblema_de_trilha() {
        for v in [0, 1, 4, 5, 14, 15, 39, 40, 1000] {
            let e = de(&jog(v, 0, 1000));
            let trilha = ["estreante", "pe_de_meia", "bom_de_truco", "zap", "lenda"];
            assert_eq!(
                e.iter().filter(|x| trilha.contains(&x.chave)).count(),
                1,
                "{v} vitorias"
            );
        }
    }

    #[test]
    fn invicto_exige_cinco_vitorias_e_zero_derrotas() {
        assert!(de(&jog(5, 0, 1000)).iter().any(|e| e.chave == "invicto"));
        assert!(!de(&jog(4, 0, 1000)).iter().any(|e| e.chave == "invicto"));
        assert!(!de(&jog(9, 1, 1000)).iter().any(|e| e.chave == "invicto"));
    }

    #[test]
    fn historico_e_vitorias_sao_trilhas_independentes() {
        let e = de(&jog(20, 20, 9000));
        assert!(e.iter().any(|x| x.chave == "zap"));
        assert!(e.iter().any(|x| x.chave == "veterano"));
        assert!(e.iter().any(|x| x.chave == "ressurgido"));
        assert!(e.iter().any(|x| x.chave == "fortuna"));
    }

    #[test]
    fn quebrado_so_aparece_depois_de_jogar() {
        assert!(!de(&jog(0, 0, 0)).iter().any(|e| e.chave == "quebrado"));
        assert!(de(&jog(0, 1, 0)).iter().any(|e| e.chave == "quebrado"));
    }
}
