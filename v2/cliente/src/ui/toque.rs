//! Janela de toque recebido (C11): fixa no centro, por cima de tudo.

use std::time::Instant;

use iced::widget::{button, column, container, row, space, svg, text};
use iced::{Alignment, Background, Border, Color, Element, Length};

use crate::avisos::ToqueRecebido;
use crate::{Msg, fraco, icones, negrito, tema, ui};

/// Fração do tempo que ainda resta (1.0 = acabou de começar).
pub fn fracao_restante(t: &ToqueRecebido, agora: Instant) -> f32 {
    if t.duracao_ms == 0 {
        return 0.0;
    }
    let resta = t.termina_em.saturating_duration_since(agora).as_millis() as f32;
    (resta / t.duracao_ms as f32).clamp(0.0, 1.0)
}

pub fn modal<'a>(
    t: &'a ToqueRecebido,
    destino: String,
    foto: Option<&'a str>,
    pulso: bool,
) -> Element<'a, Msg> {
    let c = tema::cores();
    let anel = if pulso { c.sucesso_forte } else { c.sucesso };
    let avatar = container(ui::avatar::avatar(
        foto,
        &t.quem_uid,
        &t.quem_nome,
        96.0,
        None,
        c.fundo,
    ))
    .padding(6)
    .style(move |_| container::Style {
        border: Border {
            color: anel,
            width: 3.0,
            radius: 54.0.into(),
        },
        ..Default::default()
    });
    let frac = fracao_restante(t, Instant::now());
    // Barra de 30s: a parte colorida encolhe até acabar o toque.
    let cheio = ((frac * 1000.0) as u16).max(1);
    let vazio = (((1.0 - frac) * 1000.0) as u16).max(1);
    let barra = row![
        container(space())
            .width(Length::FillPortion(cheio))
            .height(4)
            .style(move |_| container::Style {
                background: Some(Background::Color(c.sucesso_forte)),
                ..Default::default()
            }),
        space().width(Length::FillPortion(vazio)),
    ]
    .width(Length::Fill);
    let botao = |icone: iced::widget::svg::Handle, cor: Color, msg: Msg| {
        button(
            svg(icone)
                .width(26)
                .height(26)
                .style(tema::icone_cor(Color::WHITE)),
        )
        .padding(16)
        .style(move |_, st| button::Style {
            background: Some(Background::Color(
                if matches!(st, button::Status::Hovered) {
                    Color { a: 0.85, ..cor }
                } else {
                    cor
                },
            )),
            text_color: Color::WHITE,
            border: Border {
                radius: 32.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .on_press(msg)
    };
    let cartao = container(
        column![
            avatar,
            text(destino).size(20).font(negrito()),
            text(format!("{} está chamando…", t.quem_nome))
                .size(14)
                .style(fraco),
            row![
                column![
                    botao(
                        icones::desligar(),
                        c.perigo,
                        Msg::RecusaToque(t.conversa.clone())
                    ),
                    text("Recusar").size(12).style(fraco),
                ]
                .spacing(6)
                .align_x(Alignment::Center),
                column![
                    botao(
                        icones::chamada(),
                        c.sucesso,
                        Msg::AtendeToque(t.conversa.clone())
                    ),
                    text("Atender").size(12).style(fraco),
                ]
                .spacing(6)
                .align_x(Alignment::Center),
            ]
            .spacing(40),
            barra,
        ]
        .spacing(14)
        .align_x(Alignment::Center),
    )
    .padding(28)
    .width(340)
    .style(move |_| container::Style {
        background: Some(Background::Color(c.lateral)),
        border: Border {
            color: c.borda,
            width: 1.0,
            radius: tema::RAIO_GRANDE.into(),
        },
        ..Default::default()
    });
    container(cartao)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(|_| container::Style {
            background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.55))),
            ..Default::default()
        })
        .into()
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::time::Duration;

    fn toque(restante_ms: u64, duracao_ms: u64) -> ToqueRecebido {
        ToqueRecebido {
            conversa: "c".into(),
            quem_uid: "u".into(),
            quem_nome: "Ana".into(),
            termina_em: Instant::now() + Duration::from_millis(restante_ms),
            duracao_ms,
        }
    }

    #[test]
    fn barra_encolhe_com_o_tempo() {
        let agora = Instant::now();
        let t = toque(30_000, 30_000);
        assert!(fracao_restante(&t, agora) > 0.95);
        let t = toque(15_000, 30_000);
        let f = fracao_restante(&t, agora);
        assert!((0.45..=0.55).contains(&f));
        let t = toque(0, 30_000);
        assert_eq!(fracao_restante(&t, agora + Duration::from_secs(1)), 0.0);
    }
}
