//! Integração com a bandeja do sistema (StatusNotifierItem no Linux/Wayland/X11).
//!
//! Permite que o Lincast Chat permaneça rodando em segundo plano quando a janela
//! for fechada (minimizando para a bandeja) e oferece controles rápidos via
//! menu de contexto (como alternar o microfone mudo e restaurar a janela).

use std::ops::Deref;
use std::path::PathBuf;

use ksni::menu::{CheckmarkItem, MenuItem, StandardItem};

/// Wrapper em torno do handle do ksni que implementa Debug para integração com o enum Msg do Iced.
#[derive(Clone)]
pub struct HandleBandeja(pub ksni::Handle<BandejaApp>);

impl std::fmt::Debug for HandleBandeja {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HandleBandeja")
    }
}

impl Deref for HandleBandeja {
    type Target = ksni::Handle<BandejaApp>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Ações disparadas a partir de cliques no ícone ou no menu da bandeja.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcaoBandeja {
    RestaurarJanela,
    AlternarMudo,
    AlternarMinimizarParaBandeja,
    Sair,
}

/// Configuração local persistida para o comportamento da bandeja.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ConfigBandeja {
    pub minimizar_para_bandeja: bool,
}

fn arquivo_config() -> Option<PathBuf> {
    let dir = directories::ProjectDirs::from("", "", "lincast-chat")?;
    Some(dir.config_dir().join("bandeja.json"))
}

pub fn carrega_config() -> ConfigBandeja {
    let Some(arq) = arquivo_config() else {
        return ConfigBandeja::default();
    };
    let Ok(conteudo) = std::fs::read_to_string(arq) else {
        return ConfigBandeja::default();
    };
    serde_json::from_str(&conteudo).unwrap_or_default()
}

pub fn salva_config(config: &ConfigBandeja) {
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

/// Converte o ícone da aplicação em formato binário ARGB32 de 32x32 pixels
/// exigido pela especificação freedesktop / KDE StatusNotifierItem.
pub fn gera_pixmap_icone() -> Vec<ksni::Icon> {
    if let Ok(img) = image::load_from_memory(include_bytes!("../../assets/icone-256.png")) {
        let redimensionado = img
            .resize_exact(32, 32, image::imageops::FilterType::Lanczos3)
            .to_rgba8();
        let mut argb = Vec::with_capacity(32 * 32 * 4);
        for pixel in redimensionado.pixels() {
            let [r, g, b, a] = pixel.0;
            argb.push(a);
            argb.push(r);
            argb.push(g);
            argb.push(b);
        }
        vec![ksni::Icon {
            width: 32,
            height: 32,
            data: argb,
        }]
    } else {
        Vec::new()
    }
}

/// Estado do item da bandeja do Lincast Chat.
pub struct BandejaApp {
    pub tx: tokio::sync::mpsc::UnboundedSender<AcaoBandeja>,
    pub mutado: bool,
    pub em_chamada: bool,
    pub sala: Option<String>,
    pub minimizar_para_bandeja: bool,
    pub icon_pixmap: Vec<ksni::Icon>,
}

impl BandejaApp {
    pub fn novo(
        tx: tokio::sync::mpsc::UnboundedSender<AcaoBandeja>,
        minimizar_para_bandeja: bool,
    ) -> Self {
        Self {
            tx,
            mutado: false,
            em_chamada: false,
            sala: None,
            minimizar_para_bandeja,
            icon_pixmap: gera_pixmap_icone(),
        }
    }
}

impl ksni::Tray for BandejaApp {
    fn id(&self) -> String {
        "lincast-chat".into()
    }

    fn title(&self) -> String {
        "Lincast Chat".into()
    }

    fn icon_name(&self) -> String {
        if self.mutado {
            "microphone-sensitivity-muted".into()
        } else {
            "lincast-chat".into()
        }
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        self.icon_pixmap.clone()
    }

    fn status(&self) -> ksni::Status {
        if self.em_chamada {
            ksni::Status::Active
        } else {
            ksni::Status::Passive
        }
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        let descricao = if self.em_chamada {
            let s = self.sala.as_deref().unwrap_or("chamada");
            let st = if self.mutado {
                "Mudo ativo"
            } else {
                "Microfone aberto"
            };
            format!("Em #{s} — {st}")
        } else {
            "Lincast Chat — Pronto".into()
        };
        ksni::ToolTip {
            title: "Lincast Chat".into(),
            description: descricao,
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.tx.send(AcaoBandeja::RestaurarJanela);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            StandardItem {
                label: "Abrir Lincast Chat".into(),
                icon_name: "window-maximize".into(),
                activate: Box::new(|this: &mut Self| {
                    let _ = this.tx.send(AcaoBandeja::RestaurarJanela);
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            CheckmarkItem {
                label: if self.mutado {
                    "Microfone Mudo (Ativado)".into()
                } else {
                    "Microfone Mudo".into()
                },
                checked: self.mutado,
                enabled: self.em_chamada,
                activate: Box::new(|this: &mut Self| {
                    this.mutado = !this.mutado;
                    let _ = this.tx.send(AcaoBandeja::AlternarMudo);
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            CheckmarkItem {
                label: "Minimizar para a bandeja ao fechar".into(),
                checked: self.minimizar_para_bandeja,
                activate: Box::new(|this: &mut Self| {
                    this.minimizar_para_bandeja = !this.minimizar_para_bandeja;
                    let _ = this.tx.send(AcaoBandeja::AlternarMinimizarParaBandeja);
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Sair do Lincast Chat".into(),
                icon_name: "application-exit".into(),
                activate: Box::new(|this: &mut Self| {
                    let _ = this.tx.send(AcaoBandeja::Sair);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ksni::Tray;

    #[test]
    fn testa_config_bandeja_padrao() {
        let cfg = ConfigBandeja::default();
        assert!(!cfg.minimizar_para_bandeja);
        let json = serde_json::to_string(&cfg).expect("serializa");
        let des: ConfigBandeja = serde_json::from_str(&json).expect("desserializa");
        assert_eq!(cfg, des);
    }

    #[test]
    fn testa_pixmap_icone_bandeja() {
        let icons = gera_pixmap_icone();
        assert_eq!(icons.len(), 1);
        let icone = &icons[0];
        assert_eq!(icone.width, 32);
        assert_eq!(icone.height, 32);
        assert_eq!(icone.data.len(), 32 * 32 * 4);
    }

    #[test]
    fn testa_tooltip_com_e_sem_chamada() {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mut b = BandejaApp::novo(tx, true);
        let t1 = b.tool_tip();
        assert_eq!(t1.title, "Lincast Chat");
        assert_eq!(t1.description, "Lincast Chat — Pronto");

        b.em_chamada = true;
        b.sala = Some("amigos".into());
        b.mutado = true;
        let t2 = b.tool_tip();
        assert_eq!(t2.description, "Em #amigos — Mudo ativo");
    }

    #[test]
    fn testa_menu_da_bandeja() {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let b = BandejaApp::novo(tx, true);
        let menu = ksni::Tray::menu(&b);
        assert_eq!(menu.len(), 7); // Abrir, Sep, Mudo, Sep, Minimizar, Sep, Sair
    }
}
