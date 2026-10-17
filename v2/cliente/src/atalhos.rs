//! Atalhos de teclado configuráveis (C12): push-to-talk, mudo, surdo, atender
//! e desligar. O app os registra no portal `GlobalShortcuts` (o compositor é
//! quem de fato liga a tecla, mesmo com o app fora de foco) e também os
//! reconhece com a janela em foco.
//!
//! O push-to-talk tem a configuração própria de sempre (`ptt.json`); aqui
//! ficam as outras quatro ações, em `atalhos.json`.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// O identificador do app no portal e no Hyprland (`lincast-chat:mute`).
pub const ID_DO_APP: &str = "lincast-chat";

/// `(id, rótulo, combinação padrão)`. A ordem é a da tela.
pub const ACOES: [(&str, &str, &str); 5] = [
    ("ptt", "Aperte para falar", "V"),
    ("mute", "Silenciar microfone", "Ctrl + Shift + M"),
    // Ctrl+Shift+D já é o diagnóstico de rede, dentro do app.
    ("deaf", "Desativar áudio", "Ctrl + Shift + O"),
    ("answer", "Atender chamada", "Ctrl + Enter"),
    ("hangup", "Desligar", "Ctrl + Shift + E"),
];

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Atalhos {
    #[serde(default)]
    teclas: HashMap<String, String>,
}

fn arquivo() -> Option<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "lincast-chat")?;
    Some(dirs.config_dir().join("atalhos.json"))
}

impl Atalhos {
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

    /// A combinação da ação (a escolhida, ou a padrão).
    pub fn combo(&self, acao: &str) -> String {
        self.teclas
            .get(acao)
            .cloned()
            .or_else(|| {
                ACOES
                    .iter()
                    .find(|(id, _, _)| *id == acao)
                    .map(|(_, _, p)| (*p).to_string())
            })
            .unwrap_or_default()
    }

    pub fn define(&mut self, acao: &str, combo: String) {
        self.teclas.insert(acao.to_string(), combo);
    }

    pub fn restaura(&mut self, acao: &str) {
        self.teclas.remove(acao);
    }

    /// A ação que tem esta combinação, se alguma (para aviso de conflito).
    pub fn acao_de(&self, combo: &str, exceto: &str) -> Option<&'static str> {
        ACOES
            .iter()
            .find(|(id, _, _)| *id != exceto && self.combo(id).eq_ignore_ascii_case(combo))
            .map(|(id, _, _)| *id)
    }
}

/// "Ctrl + Shift + M" a partir de uma tecla com modificadores. `None` para
/// tecla que sozinha não serve (só um modificador apertado).
pub fn combo_de_evento(
    key: &iced::keyboard::Key,
    mods: iced::keyboard::Modifiers,
) -> Option<String> {
    use iced::keyboard::Key;
    use iced::keyboard::key::Named;
    let nome = match key {
        Key::Character(c) => {
            let c = c.as_str();
            if c.chars().count() != 1 {
                return None;
            }
            c.to_uppercase()
        }
        Key::Named(n) => match n {
            Named::Space => "Espaço".to_string(),
            Named::Enter => "Enter".to_string(),
            Named::Tab => "Tab".to_string(),
            Named::Backspace => "Backspace".to_string(),
            Named::Delete => "Delete".to_string(),
            Named::Home => "Home".to_string(),
            Named::End => "End".to_string(),
            Named::PageUp => "PageUp".to_string(),
            Named::PageDown => "PageDown".to_string(),
            Named::F1 => "F1".into(),
            Named::F2 => "F2".into(),
            Named::F3 => "F3".into(),
            Named::F4 => "F4".into(),
            Named::F5 => "F5".into(),
            Named::F6 => "F6".into(),
            Named::F7 => "F7".into(),
            Named::F8 => "F8".into(),
            Named::F9 => "F9".into(),
            Named::F10 => "F10".into(),
            Named::F11 => "F11".into(),
            Named::F12 => "F12".into(),
            _ => return None,
        },
        _ => return None,
    };
    let mut partes: Vec<String> = Vec::new();
    if mods.control() {
        partes.push("Ctrl".into());
    }
    if mods.alt() {
        partes.push("Alt".into());
    }
    if mods.shift() {
        partes.push("Shift".into());
    }
    if mods.logo() {
        partes.push("Super".into());
    }
    partes.push(nome);
    Some(partes.join(" + "))
}

/// "Ctrl + Shift + M" → "CTRL+SHIFT+m" (o gatilho preferido do portal).
pub fn para_portal(combo: &str) -> String {
    combo
        .split(" + ")
        .map(|p| match p {
            "Ctrl" => "CTRL".to_string(),
            "Alt" => "ALT".to_string(),
            "Shift" => "SHIFT".to_string(),
            "Super" => "LOGO".to_string(),
            "Espaço" => "space".to_string(),
            "Enter" => "Return".to_string(),
            x if x.chars().count() == 1 => x.to_lowercase(),
            x => x.to_string(),
        })
        .collect::<Vec<_>>()
        .join("+")
}

/// "Ctrl + Shift + M" → "CTRL SHIFT, M" (as duas partes do `bind` do Hyprland).
pub fn para_hyprland(combo: &str) -> String {
    let partes: Vec<&str> = combo.split(" + ").collect();
    let (teclas, mods) = partes.split_last().map_or((&"", &[][..]), |(t, m)| (t, m));
    let mods = mods
        .iter()
        .map(|m| match *m {
            "Ctrl" => "CTRL",
            "Alt" => "ALT",
            "Shift" => "SHIFT",
            "Super" => "SUPER",
            outro => outro,
        })
        .collect::<Vec<_>>()
        .join(" ");
    let tecla = match *teclas {
        "Espaço" => "SPACE".to_string(),
        "Enter" => "RETURN".to_string(),
        t if t.chars().count() == 1 => t.to_uppercase(),
        t => t.to_string(),
    };
    format!("{mods}, {tecla}")
}

/// A linha pronta para colar no `hyprland.conf`.
pub fn linha_hyprland(acao: &str, combo: &str) -> String {
    format!(
        "bind = {}, global, {ID_DO_APP}:{acao}",
        para_hyprland(combo)
    )
}

#[cfg(test)]
mod testes {
    use super::*;
    use iced::keyboard::{Key, Modifiers};

    #[test]
    fn combo_do_teclado() {
        let m = Modifiers::CTRL | Modifiers::SHIFT;
        assert_eq!(
            combo_de_evento(&Key::Character("m".into()), m).as_deref(),
            Some("Ctrl + Shift + M")
        );
        assert_eq!(
            combo_de_evento(
                &Key::Named(iced::keyboard::key::Named::F5),
                Modifiers::empty()
            )
            .as_deref(),
            Some("F5")
        );
        // Só um modificador apertado não é atalho.
        assert!(
            combo_de_evento(
                &Key::Named(iced::keyboard::key::Named::Control),
                Modifiers::CTRL
            )
            .is_none()
        );
    }

    #[test]
    fn formatos_do_portal_e_do_hyprland() {
        assert_eq!(para_portal("Ctrl + Shift + M"), "CTRL+SHIFT+m");
        assert_eq!(para_portal("Ctrl + Enter"), "CTRL+Return");
        assert_eq!(para_hyprland("Ctrl + Shift + M"), "CTRL SHIFT, M");
        assert_eq!(para_hyprland("V"), ", V");
        assert_eq!(
            linha_hyprland("mute", "Ctrl + Shift + M"),
            "bind = CTRL SHIFT, M, global, lincast-chat:mute"
        );
    }

    #[test]
    fn padrao_edicao_e_conflito() {
        let mut a = Atalhos::default();
        assert_eq!(a.combo("mute"), "Ctrl + Shift + M");
        a.define("mute", "Ctrl + Alt + K".into());
        assert_eq!(a.combo("mute"), "Ctrl + Alt + K");
        assert_eq!(a.acao_de("Ctrl + Shift + E", "mute"), Some("hangup"));
        assert_eq!(
            a.acao_de("Ctrl + Alt + K", "mute"),
            None,
            "ela mesma não conta"
        );
        a.restaura("mute");
        assert_eq!(a.combo("mute"), "Ctrl + Shift + M");
    }
}
