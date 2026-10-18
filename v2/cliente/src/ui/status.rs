//! Módulo de status de presença (S7).

use iced::widget::{container, row, space, svg, text};
use iced::{Alignment, Border, Color, Element};

use crate::api::{Atividade, StatusCustom};
use crate::{Msg, icones, tema};

/// A linha pequena sob o nome de uma pessoa: status personalizado, jogo,
/// música ou o rótulo da presença — nessa ordem de prioridade.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Sub {
    pub icone: Option<Icone>,
    pub emoji: String,
    pub texto: String,
}

/// Qual ícone verde acompanha a atividade.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icone {
    Jogo,
    Musica,
}

impl Sub {
    pub fn texto(t: impl Into<String>) -> Self {
        Self {
            texto: t.into(),
            ..Default::default()
        }
    }

    pub fn de_pessoa(
        custom: Option<&StatusCustom>,
        atividade: Option<&Atividade>,
        rotulo: &str,
    ) -> Self {
        if let Some(sc) = custom.filter(|s| !s.vencido()) {
            return Self {
                icone: None,
                emoji: sc.emoji.clone(),
                texto: sc.texto.clone(),
            };
        }
        if let Some(a) = atividade {
            if let Some(j) = &a.jogo {
                let texto = if let Some(d) = j.desde.map(crate::ui::painel::tempo_de_jogo) {
                    format!("{} · há {d}", j.nome)
                } else {
                    j.nome.clone()
                };
                return Self {
                    icone: Some(Icone::Jogo),
                    emoji: String::new(),
                    texto,
                };
            }
            if let Some(m) = &a.musica {
                let texto = match m.artista.as_deref().filter(|x| !x.is_empty()) {
                    Some(art) => format!("{} — {art}", m.titulo),
                    None => m.titulo.clone(),
                };
                return Self {
                    icone: Some(Icone::Musica),
                    emoji: String::new(),
                    texto,
                };
            }
        }
        Self::texto(rotulo)
    }
}

/// Desenha a linha (ícone verde ou emoji, depois o texto numa linha só).
pub fn sub_widget<'a>(sub: &Sub, tamanho: f32, cor: Color) -> Element<'a, Msg> {
    let c = tema::cores();
    let mut r = row![].spacing(5).align_y(Alignment::Center);
    match sub.icone {
        Some(Icone::Jogo) => {
            r = r.push(
                svg(icones::controle())
                    .width(tamanho + 2.0)
                    .height(tamanho + 2.0)
                    .style(tema::icone_cor(c.sucesso_forte)),
            );
        }
        Some(Icone::Musica) => {
            r = r.push(
                svg(icones::musica())
                    .width(tamanho + 2.0)
                    .height(tamanho + 2.0)
                    .style(tema::icone_cor(c.sucesso_forte)),
            );
        }
        None if !sub.emoji.is_empty() => {
            r = r.push(
                text(sub.emoji.clone())
                    .size(tamanho)
                    .font(crate::tema::FONTE_EMOJI),
            );
        }
        None => {}
    }
    r.push(
        text(sub.texto.clone())
            .size(tamanho)
            .color(cor)
            .wrapping(iced::widget::text::Wrapping::None),
    )
    .into()
}

pub const STATUS_OPCOES: &[(&str, &str)] = &[
    ("online", "Online"),
    ("ausente", "Ausente"),
    ("ocupado", "Não perturbe"),
    ("invisivel", "Invisível"),
];

/// Retorna a cor associada a cada status.
pub fn cor_status(status: &str, tema_cores: &tema::Cores) -> Color {
    match status {
        "online" => tema_cores.sucesso,
        "ausente" | "idle" => Color::from_rgb8(245, 158, 11),
        "ocupado" | "dnd" | "nao-perturbe" => tema_cores.perigo,
        _ => tema_cores.texto_fraco,
    }
}

/// Retorna o rótulo amigável do status.
pub fn rotulo_status(status: &str) -> &'static str {
    match status {
        "online" => "Online",
        "ausente" | "idle" => "Ausente",
        "invisivel" => "Invisível",
        "ocupado" | "dnd" | "nao-perturbe" => "Não perturbe",
        _ => "Offline",
    }
}

/// Renderiza uma bolinha colorida indicando o status.
pub fn bolinha_status(status: &str, tamanho: f32) -> Element<'static, Msg> {
    let c = tema::cores();
    let cor = cor_status(status, &c);
    container(space().width(tamanho).height(tamanho))
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(cor)),
            border: Border {
                radius: (tamanho / 2.0).into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn sub_prioriza_custom_depois_atividade_depois_rotulo() {
        let custom = StatusCustom {
            texto: "codando".into(),
            emoji: "💻".into(),
            ate: None,
        };
        let atv = Atividade {
            jogo: Some(crate::api::JogoAtividade {
                nome: "Death Stranding".into(),
                origem: None,
                desde: None,
                capa: None,
            }),
            musica: None,
        };
        let s = Sub::de_pessoa(Some(&custom), Some(&atv), "Online");
        assert_eq!(s.texto, "codando");
        let s = Sub::de_pessoa(None, Some(&atv), "Online");
        assert_eq!(s.icone, Some(Icone::Jogo));
        assert_eq!(s.texto, "Death Stranding");
        assert_eq!(Sub::de_pessoa(None, None, "Online").texto, "Online");
    }

    #[test]
    fn sub_ignora_status_vencido() {
        let vencido = StatusCustom {
            texto: "x".into(),
            emoji: String::new(),
            ate: Some(1),
        };
        assert_eq!(
            Sub::de_pessoa(Some(&vencido), None, "Online").texto,
            "Online"
        );
    }

    #[test]
    fn testa_mapeamento_status() {
        let c = tema::cores();
        assert_eq!(rotulo_status("online"), "Online");
        assert_eq!(rotulo_status("ausente"), "Ausente");
        assert_eq!(rotulo_status("ocupado"), "Não perturbe");
        assert_eq!(rotulo_status("desconhecido"), "Offline");

        assert_eq!(cor_status("online", &c), c.sucesso);
        assert_eq!(cor_status("ocupado", &c), c.perigo);
    }
}
