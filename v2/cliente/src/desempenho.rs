//! Opções de desempenho (como no Discord): reduzir animações e animar GIF só
//! com o mouse em cima. Guardadas em `~/.config/lincast-chat/desempenho.json`
//! e lidas de qualquer lugar do desenho por átomos globais, igual ao tema.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

static REDUZ: AtomicBool = AtomicBool::new(false);
static GIF_SO_HOVER: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConfigDesempenho {
    #[serde(default)]
    pub reduz_animacoes: bool,
    #[serde(default)]
    pub gif_so_hover: bool,
}

pub fn reduz_animacoes() -> bool {
    REDUZ.load(Ordering::Relaxed)
}

pub fn gif_so_hover() -> bool {
    GIF_SO_HOVER.load(Ordering::Relaxed)
}

pub fn atual() -> ConfigDesempenho {
    ConfigDesempenho {
        reduz_animacoes: reduz_animacoes(),
        gif_so_hover: gif_so_hover(),
    }
}

pub fn aplica(c: ConfigDesempenho) {
    REDUZ.store(c.reduz_animacoes, Ordering::Relaxed);
    GIF_SO_HOVER.store(c.gif_so_hover, Ordering::Relaxed);
}

fn arquivo() -> Option<PathBuf> {
    let dir = directories::ProjectDirs::from("", "", "lincast-chat")?;
    Some(dir.config_dir().join("desempenho.json"))
}

/// Lê do disco e aplica. Chamado uma vez na abertura.
pub fn carrega() {
    let Some(p) = arquivo() else { return };
    if let Ok(t) = std::fs::read_to_string(p)
        && let Ok(c) = serde_json::from_str::<ConfigDesempenho>(&t)
    {
        aplica(c);
    }
}

/// Aplica e grava.
pub fn salva(c: ConfigDesempenho) {
    aplica(c);
    let Some(p) = arquivo() else { return };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(j) = serde_json::to_string_pretty(&c) {
        let _ = std::fs::write(p, j);
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn aplica_e_le_de_volta() {
        let antes = atual();
        aplica(ConfigDesempenho {
            reduz_animacoes: true,
            gif_so_hover: true,
        });
        assert!(reduz_animacoes() && gif_so_hover());
        aplica(antes);
    }

    #[test]
    fn json_antigo_sem_campos_vira_padrao() {
        let c: ConfigDesempenho = serde_json::from_str("{}").unwrap();
        assert_eq!(c, ConfigDesempenho::default());
    }
}
