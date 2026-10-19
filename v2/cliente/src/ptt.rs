//! Módulo de Push-to-talk (PTT).
//!
//! Gerencia a configuração do atalho (tecla, estado ativo), a persistência
//! em `~/.config/lincast-chat/ptt.json`, o mapeamento para nomes amigáveis
//! e a tentativa de registro global via portal `GlobalShortcuts` do Wayland (ashpd).

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PttConfig {
    pub ativo: bool,
    pub tecla: String,
    pub rotulo: String,
}

impl Default for PttConfig {
    fn default() -> Self {
        Self {
            ativo: false,
            tecla: "Space".to_string(),
            rotulo: "Espaço".to_string(),
        }
    }
}

fn arquivo_config() -> Option<PathBuf> {
    let dir = directories::ProjectDirs::from("", "", "lincast-chat")?;
    Some(dir.config_dir().join("ptt.json"))
}

pub fn carrega_config() -> PttConfig {
    let Some(arq) = arquivo_config() else {
        return PttConfig::default();
    };
    let Ok(conteudo) = std::fs::read_to_string(arq) else {
        return PttConfig::default();
    };
    serde_json::from_str(&conteudo).unwrap_or_default()
}

pub fn salva_config(config: &PttConfig) {
    let Some(arq) = arquivo_config() else {
        return;
    };
    if let Some(parent) = arq.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(config) {
        let _ = std::fs::write(arq, json);
    }
}

/// Converte uma tecla do iced em um identificador estável e um rótulo legível em português.
pub fn descreve_tecla_completa(
    key: &iced::keyboard::Key,
    physical: Option<&iced::keyboard::key::Physical>,
) -> Option<(String, String)> {
    use iced::keyboard::key::{Code, Named, Physical};
    if let Some(Physical::Code(code)) = physical {
        match code {
            Code::NumLock => return Some(("NumLock".to_string(), "Num Lock".to_string())),
            Code::ScrollLock => {
                return Some(("ScrollLock".to_string(), "Scroll Lock".to_string()));
            }
            Code::Numpad0 => return Some(("Numpad0".to_string(), "Numpad 0".to_string())),
            Code::Numpad1 => return Some(("Numpad1".to_string(), "Numpad 1".to_string())),
            Code::Numpad2 => return Some(("Numpad2".to_string(), "Numpad 2".to_string())),
            Code::Numpad3 => return Some(("Numpad3".to_string(), "Numpad 3".to_string())),
            Code::Numpad4 => return Some(("Numpad4".to_string(), "Numpad 4".to_string())),
            Code::Numpad5 => return Some(("Numpad5".to_string(), "Numpad 5".to_string())),
            Code::Numpad6 => return Some(("Numpad6".to_string(), "Numpad 6".to_string())),
            Code::Numpad7 => return Some(("Numpad7".to_string(), "Numpad 7".to_string())),
            Code::Numpad8 => return Some(("Numpad8".to_string(), "Numpad 8".to_string())),
            Code::Numpad9 => return Some(("Numpad9".to_string(), "Numpad 9".to_string())),
            Code::NumpadDecimal => {
                return Some(("NumpadDecimal".to_string(), "Numpad .".to_string()));
            }
            Code::NumpadEnter => {
                return Some(("NumpadEnter".to_string(), "Numpad Enter".to_string()));
            }
            _ => {}
        }
    }
    match key {
        iced::keyboard::Key::Named(named) => {
            let (id, rotulo) = match named {
                Named::Space => ("Space", "Espaço"),
                Named::Control => ("Control", "Ctrl"),
                Named::Alt => ("Alt", "Alt"),
                Named::Shift => ("Shift", "Shift"),
                Named::CapsLock => ("CapsLock", "Caps Lock"),
                Named::Tab => ("Tab", "Tab"),
                Named::Enter => ("Enter", "Enter"),
                Named::Backspace => ("Backspace", "Backspace"),
                Named::Insert => ("Insert", "Insert"),
                Named::Delete => ("Delete", "Delete"),
                Named::Home => ("Home", "Home"),
                Named::End => ("End", "End"),
                Named::PageUp => ("PageUp", "Page Up"),
                Named::PageDown => ("PageDown", "Page Down"),
                Named::F1 => ("F1", "F1"),
                Named::F2 => ("F2", "F2"),
                Named::F3 => ("F3", "F3"),
                Named::F4 => ("F4", "F4"),
                Named::F5 => ("F5", "F5"),
                Named::F6 => ("F6", "F6"),
                Named::F7 => ("F7", "F7"),
                Named::F8 => ("F8", "F8"),
                Named::F9 => ("F9", "F9"),
                Named::F10 => ("F10", "F10"),
                Named::F11 => ("F11", "F11"),
                Named::F12 => ("F12", "F12"),
                Named::NumLock | Named::Clear => ("NumLock", "Num Lock"),
                Named::ScrollLock => ("ScrollLock", "Scroll Lock"),
                _ => return None,
            };
            Some((id.to_string(), rotulo.to_string()))
        }
        iced::keyboard::Key::Character(c) => {
            let ch = c.as_str();
            if ch.len() == 1 {
                let maiuscula = ch.to_uppercase();
                Some((format!("Char:{maiuscula}"), maiuscula))
            } else {
                None
            }
        }
        _ => None,
    }
}

#[allow(dead_code)]
pub fn descreve_tecla(key: &iced::keyboard::Key) -> Option<(String, String)> {
    descreve_tecla_completa(key, None)
}

/// Verifica se a tecla pressionada bate com o identificador configurado.
pub fn corresponde_completo(
    id_config: &str,
    key: &iced::keyboard::Key,
    physical: Option<&iced::keyboard::key::Physical>,
) -> bool {
    if let Some((id, _)) = descreve_tecla_completa(key, physical) {
        id == id_config
    } else {
        false
    }
}

#[allow(dead_code)]
pub fn corresponde(id_config: &str, key: &iced::keyboard::Key) -> bool {
    corresponde_completo(id_config, key, None)
}

/// Inicia o listener global no Wayland via portal GlobalShortcuts.
/// Emite `true` quando ativado (pressionado) e `false` quando desativado (solto).
pub async fn inicia_portal_global(
    tx: tokio::sync::mpsc::UnboundedSender<(String, bool)>,
    gatilhos: Vec<(String, String, Option<String>)>,
) -> Result<(), String> {
    use ashpd::desktop::CreateSessionOptions;
    use ashpd::desktop::global_shortcuts::{BindShortcutsOptions, GlobalShortcuts, NewShortcut};

    let portal = GlobalShortcuts::new()
        .await
        .map_err(|e| format!("Não foi possível conectar ao portal GlobalShortcuts: {e}"))?;

    let session = portal
        .create_session(CreateSessionOptions::default())
        .await
        .map_err(|e| format!("Falha ao criar sessão no portal GlobalShortcuts: {e}"))?;

    // (id, descrição, gatilho preferido). No Hyprland o gatilho de verdade vem
    // do `hyprland.conf`; a tela de atalhos mostra a linha pronta.
    let shortcuts: Vec<NewShortcut> = gatilhos
        .iter()
        .map(|(id, descricao, gatilho)| {
            NewShortcut::new(id.as_str(), descricao.as_str()).preferred_trigger(gatilho.as_deref())
        })
        .collect();

    let request = portal
        .bind_shortcuts(&session, &shortcuts, None, BindShortcutsOptions::default())
        .await
        .map_err(|e| format!("Falha ao registrar atalho no portal GlobalShortcuts: {e}"))?;

    let _res = request
        .response()
        .map_err(|e| format!("Portal GlobalShortcuts recusou atalho: {e}"))?;

    let mut ativado = portal
        .receive_activated()
        .await
        .map_err(|e| format!("Falha ao assinar sinal Activated: {e}"))?;

    let mut desativado = portal
        .receive_deactivated()
        .await
        .map_err(|e| format!("Falha ao assinar sinal Deactivated: {e}"))?;

    tokio::spawn(async move {
        loop {
            tokio::select! {
                Some(sig) = ativado.next() => {
                    let _ = tx.send((sig.shortcut_id().to_string(), true));
                }
                Some(sig) = desativado.next() => {
                    {
                        let _ = tx.send((sig.shortcut_id().to_string(), false));
                    }
                }
                else => break,
            }
        }
    });

    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn config_padrao_e_serializacao() {
        let p = PttConfig::default();
        assert!(!p.ativo);
        assert_eq!(p.tecla, "Space");
        assert_eq!(p.rotulo, "Espaço");

        let json = serde_json::to_string(&p).expect("serializa");
        let recuperado: PttConfig = serde_json::from_str(&json).expect("deserializa");
        assert_eq!(recuperado.tecla, "Space");
        assert_eq!(recuperado.rotulo, "Espaço");
    }

    #[test]
    fn descreve_e_corresponde_tecla() {
        use iced::keyboard::key::Named;

        let key_space = iced::keyboard::Key::Named(Named::Space);
        let (id, rotulo) = descreve_tecla(&key_space).expect("descreve Space");
        assert_eq!(id, "Space");
        assert_eq!(rotulo, "Espaço");
        assert!(corresponde("Space", &key_space));
        assert!(!corresponde("Control", &key_space));

        let key_f1 = iced::keyboard::Key::Named(Named::F1);
        let (id_f1, rotulo_f1) = descreve_tecla(&key_f1).expect("descreve F1");
        assert_eq!(id_f1, "F1");
        assert_eq!(rotulo_f1, "F1");
        assert!(corresponde("F1", &key_f1));

        let key_char = iced::keyboard::Key::Character("t".into());
        let (id_char, rotulo_char) = descreve_tecla(&key_char).expect("descreve char");
        assert_eq!(id_char, "Char:T");
        assert_eq!(rotulo_char, "T");
        assert!(corresponde("Char:T", &key_char));

        let key_numlock = iced::keyboard::Key::Named(Named::NumLock);
        let (id_nl, rotulo_nl) = descreve_tecla(&key_numlock).expect("descreve NumLock");
        assert_eq!(id_nl, "NumLock");
        assert_eq!(rotulo_nl, "Num Lock");
        assert!(corresponde("NumLock", &key_numlock));
    }
}
