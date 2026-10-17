//! Módulo de interface para reações flutuantes e cutucadas (A1).
//!
//! Reações flutuantes sobem e desaparecem suavemente sobre a sala.
//! Cutucadas exibem um banner de destaque alertando o destinatário e
//! fornecem feedback visual para quem enviou.

use iced::widget::{button, column, container, row, space, text};
use iced::{Alignment, Border, Color, Element, Length};

use crate::ui::chat::EMOJIS_PERMITIDOS;
use crate::{Msg, fraco, negrito, tema};

/// Representa uma reação flutuante ativa em exibição na sala.
#[derive(Debug, Clone, PartialEq)]
pub struct ReacaoAtiva {
    pub id: u64,
    pub emoji: String,
    pub quem: String,
    /// Posição horizontal fracionária relativa (ex: 0.15 a 0.85).
    pub x_frac: f32,
    /// Duração restante em milissegundos (inicia em 2600ms).
    pub tempo_restante_ms: u32,
}

/// Representa um aviso visual temporário de cutucada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvisoCutucada {
    pub texto: String,
    /// Duração restante em milissegundos (ex: 2500ms).
    pub tempo_restante_ms: u32,
}

/// Atualiza o temporizador das reações e remove as que expiraram.
pub fn atualiza_reacoes(reacoes: &mut Vec<ReacaoAtiva>, delta_ms: u32) {
    reacoes.retain_mut(|r| {
        if r.tempo_restante_ms <= delta_ms {
            false
        } else {
            r.tempo_restante_ms -= delta_ms;
            true
        }
    });
}

/// Atualiza o temporizador da cutucada e remove o aviso se expirou.
pub fn atualiza_cutucada(aviso: &mut Option<AvisoCutucada>, delta_ms: u32) {
    if let Some(a) = aviso {
        if a.tempo_restante_ms <= delta_ms {
            *aviso = None;
        } else {
            a.tempo_restante_ms -= delta_ms;
        }
    }
}

/// Renderiza o seletor rápido flutuante de emojis para enviar reação.
pub fn seletor_reacoes<'a>() -> Element<'a, Msg> {
    let mut grade = row![].spacing(6).align_y(Alignment::Center);

    for &emoji in EMOJIS_PERMITIDOS {
        let em_str = emoji.to_string();
        grade = grade.push(
            button(text(emoji).size(18))
                .padding([4, 6])
                .style(tema::botao_secundario)
                .on_press(Msg::EnviarReacao(em_str)),
        );
    }

    container(grade)
        .padding([6, 10])
        .style(tema::painel_lateral)
        .into()
}

/// Renderiza o banner central superior de aviso de cutucada.
pub fn banner_cutucada<'a>(aviso: &'a AvisoCutucada) -> Element<'a, Msg> {
    let c = tema::cores();

    let conteudo = row![
        crate::ui::efeitos::ico(crate::icones::dedo(), 16.0, c.sobre_destaque),
        text(&aviso.texto)
            .size(14)
            .font(negrito())
            .style(move |_| text::Style {
                color: Some(c.sobre_destaque),
            }),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let badge = container(conteudo)
        .padding([8, 16])
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(c.destaque)),
            border: Border {
                radius: 20.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

    container(badge)
        .width(Length::Fill)
        .align_x(Alignment::Center)
        .padding([14, 0])
        .into()
}

/// Renderiza a camada de reações flutuantes.
pub fn camada_reacoes<'a>(reacoes: &'a [ReacaoAtiva]) -> Element<'a, Msg> {
    if reacoes.is_empty() {
        return space().width(0).height(0).into();
    }

    let mut col_itens = row![].spacing(16).align_y(Alignment::End);

    for r_item in reacoes {
        // Balão com emoji grande e o nome de quem reagiu
        let balao = container(
            column![
                text(&r_item.emoji).size(28),
                text(&r_item.quem).size(11).style(fraco),
            ]
            .spacing(2)
            .align_x(Alignment::Center),
        )
        .padding([4, 8])
        .style(|_| container::Style {
            background: Some(iced::Background::Color(Color::from_rgba(
                0.0, 0.0, 0.0, 0.65,
            ))),
            border: Border {
                radius: 8.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

        col_itens = col_itens.push(balao);
    }

    container(col_itens)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::End)
        .padding(iced::Padding {
            top: 0.0,
            right: 20.0,
            bottom: 80.0,
            left: 20.0,
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn testa_atualiza_reacoes_decremento_e_expiracao() {
        let mut reacoes = vec![
            ReacaoAtiva {
                id: 1,
                emoji: "🔥".into(),
                quem: "Val".into(),
                x_frac: 0.5,
                tempo_restante_ms: 1000,
            },
            ReacaoAtiva {
                id: 2,
                emoji: "🎉".into(),
                quem: "Alice".into(),
                x_frac: 0.3,
                tempo_restante_ms: 400,
            },
        ];

        atualiza_reacoes(&mut reacoes, 500);
        assert_eq!(reacoes.len(), 1);
        assert_eq!(reacoes[0].id, 1);
        assert_eq!(reacoes[0].tempo_restante_ms, 500);

        atualiza_reacoes(&mut reacoes, 500);
        assert!(reacoes.is_empty());
    }

    #[test]
    fn testa_atualiza_cutucada_expiracao() {
        let mut aviso = Some(AvisoCutucada {
            texto: "Alice te cutucou".into(),
            tempo_restante_ms: 1000,
        });

        atualiza_cutucada(&mut aviso, 600);
        assert!(aviso.is_some());
        assert_eq!(aviso.as_ref().unwrap().tempo_restante_ms, 400);

        atualiza_cutucada(&mut aviso, 500);
        assert!(aviso.is_none());
    }
}
