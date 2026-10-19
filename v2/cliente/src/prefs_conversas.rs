//! Preferências locais por conversa: silenciadas (sem notificação) e fechadas
//! (somem da lista até chegar mensagem nova). Em `conversas.json`, ao lado das
//! outras configurações. É só do app deste computador; o servidor não sabe.

use std::collections::HashSet;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrefsConversas {
    #[serde(default)]
    pub silenciadas: HashSet<String>,
    #[serde(default)]
    pub fechadas: HashSet<String>,
    /// Aviso do sistema para mensagem de conversa com a janela fora de foco.
    #[serde(default = "verdadeiro")]
    pub notificar_mensagens: bool,
    /// Aviso quando alguém cita você com @, mesmo em conversa silenciada.
    #[serde(default = "verdadeiro")]
    pub notificar_mencoes: bool,
    /// Tocar o som de chamada recebida.
    #[serde(default = "verdadeiro")]
    pub som_do_toque: bool,
    /// Volume do toque de chamada (0–100).
    #[serde(default = "volume_toque_padrao")]
    pub volume_toque: u8,
    /// Sons de retorno da chamada (mudo, fone, tela, quem assiste), só para você.
    #[serde(default = "verdadeiro")]
    pub sons_de_retorno: bool,
    /// Volume dos efeitos sonoros e soundboard (0–100).
    #[serde(default = "volume_efeitos_padrao")]
    pub volume_efeitos: u8,
}

fn verdadeiro() -> bool {
    true
}

fn volume_toque_padrao() -> u8 {
    80
}

fn volume_efeitos_padrao() -> u8 {
    crate::som::VOLUME_PADRAO
}

fn arquivo() -> Option<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "lincast-chat")?;
    Some(dirs.config_dir().join("conversas.json"))
}

impl Default for PrefsConversas {
    fn default() -> Self {
        Self {
            silenciadas: HashSet::new(),
            fechadas: HashSet::new(),
            notificar_mensagens: true,
            notificar_mencoes: true,
            som_do_toque: true,
            sons_de_retorno: true,
            volume_toque: 80,
            volume_efeitos: crate::som::VOLUME_PADRAO,
        }
    }
}

impl PrefsConversas {
    pub fn carrega() -> Self {
        arquivo()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn salva(&self) {
        let Some(p) = arquivo() else {
            return;
        };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(t) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(p, t);
        }
    }

    /// Liga/desliga o silêncio e devolve o novo estado.
    pub fn alterna_silencio(&mut self, id: &str) -> bool {
        if !self.silenciadas.remove(id) {
            self.silenciadas.insert(id.to_string());
            return true;
        }
        false
    }

    pub fn fecha(&mut self, id: &str) {
        self.fechadas.insert(id.to_string());
    }

    /// Chegou coisa nova: a conversa volta para a lista.
    pub fn reabre(&mut self, id: &str) -> bool {
        self.fechadas.remove(id)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn silenciar_alterna_e_fechar_reabre() {
        let mut p = PrefsConversas::default();
        assert!(p.alterna_silencio("c1"));
        assert!(p.silenciadas.contains("c1"));
        assert!(!p.alterna_silencio("c1"));
        assert!(p.silenciadas.is_empty());
        p.fecha("c2");
        assert!(p.fechadas.contains("c2"));
        assert!(p.reabre("c2"));
        assert!(!p.reabre("c2"));
    }

    #[test]
    fn json_antigo_sem_campos_carrega() {
        let p: PrefsConversas = serde_json::from_str("{}").unwrap();
        assert!(p.silenciadas.is_empty() && p.fechadas.is_empty());
        assert!(p.notificar_mensagens && p.notificar_mencoes && p.som_do_toque);
    }
}
