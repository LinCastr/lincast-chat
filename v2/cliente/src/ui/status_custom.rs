//! Janela "Definir seu status" (print 6): texto + emoji + "Limpar depois de".

use chrono::{Duration, Local};
use iced::widget::{Column, button, column, container, row, space, text, text_input};
use iced::{Alignment, Background, Border, Element, Length};

use crate::ui::efeitos::ico;
use crate::{Msg, icones, negrito, tema, texto_perigo};

/// Prazos oferecidos, em minutos (`None` = não limpar).
pub const PRAZOS: [Option<u32>; 5] = [Some(1440), Some(240), Some(60), Some(30), None];

pub const EMOJIS: [&str; 20] = [
    "😀", "😂", "😎", "🥳", "😍", "🤩", "😴", "🤔", "🎮", "🎧", "💻", "📚", "☕", "🍕", "🌧️", "🔥",
    "✨", "💀", "🏠", "🚀",
];

#[derive(Debug, Clone)]
pub struct EstadoStatusCustom {
    pub texto: String,
    pub emoji: String,
    pub minutos: Option<u32>,
    pub prazos_abertos: bool,
    pub emojis_abertos: bool,
    pub salvando: bool,
    pub erro: Option<String>,
}

impl EstadoStatusCustom {
    pub fn novo(atual: Option<&crate::api::StatusCustom>) -> Self {
        Self {
            texto: atual.map(|s| s.texto.clone()).unwrap_or_default(),
            emoji: atual.map(|s| s.emoji.clone()).unwrap_or_default(),
            minutos: Some(1440),
            prazos_abertos: false,
            emojis_abertos: false,
            salvando: false,
            erro: None,
        }
    }
}

/// "24 horas (amanhã às 22:00)", "30 minutos (22:30)", "Não limpar".
pub fn rotulo_prazo(minutos: Option<u32>) -> String {
    let Some(m) = minutos else {
        return "Não limpar".into();
    };
    let agora = Local::now();
    let fim = agora + Duration::minutes(i64::from(m));
    let hora = fim.format("%H:%M");
    let nome = match m {
        1440 => "24 horas".to_string(),
        240 => "4 horas".to_string(),
        60 => "1 hora".to_string(),
        n => format!("{n} minutos"),
    };
    if fim.date_naive() != agora.date_naive() {
        format!("{nome} (amanhã às {hora})")
    } else {
        format!("{nome} ({hora})")
    }
}

pub fn modal<'a>(est: &'a EstadoStatusCustom) -> Element<'a, Msg> {
    let c = tema::cores();
    let botao_emoji = button(if est.emoji.is_empty() {
        ico(icones::smile(), 18.0, c.texto_medio)
    } else {
        text(est.emoji.clone())
            .size(18)
            .font(crate::tema::FONTE_EMOJI)
            .into()
    })
    .padding([8, 10])
    .style(tema::botao_icone(false, false))
    .on_press(Msg::StatusCustomAlternaEmojis);

    let mut corpo = column![
        row![
            text("Definir seu status")
                .size(20)
                .font(negrito())
                .width(Length::Fill),
            button(ico(icones::fechar(), 16.0, c.texto_fraco))
                .padding(4)
                .style(tema::botao_icone(false, false))
                .on_press(Msg::FechaStatusCustom),
        ]
        .align_y(Alignment::Center),
        text("Status").size(13).font(negrito()),
        row![
            botao_emoji,
            text_input("Passatempo recente?", &est.texto)
                .on_input(Msg::StatusCustomTexto)
                .on_submit(Msg::SalvaStatusCustom)
                .padding([9, 10])
                .size(14)
                .style(tema::campo),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    ]
    .spacing(10);

    if est.emojis_abertos {
        let grade = row(EMOJIS.iter().map(|e| -> Element<'a, Msg> {
            button(text(*e).size(20).font(crate::tema::FONTE_EMOJI))
                .padding([4, 6])
                .style(tema::botao_icone(false, false))
                .on_press(Msg::StatusCustomEmoji((*e).to_string()))
                .into()
        }))
        .spacing(2)
        .wrap();
        corpo = corpo.push(grade);
    }

    let prazo = button(
        row![
            text(rotulo_prazo(est.minutos)).size(13).width(Length::Fill),
            ico(
                if est.prazos_abertos {
                    icones::seta_cima()
                } else {
                    icones::seta_baixo()
                },
                14.0,
                c.texto_medio
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding([8, 10])
    .width(Length::Fill)
    .style(tema::botao_secundario)
    .on_press(Msg::StatusCustomAlternaPrazos);
    corpo = corpo.push(text("Limpar depois de").size(13).font(negrito()));
    corpo = corpo.push(prazo);
    if est.prazos_abertos {
        let mut lista = Column::new().spacing(1);
        for p in PRAZOS {
            let sel = p == est.minutos;
            lista = lista.push(
                button(
                    row![
                        text(rotulo_prazo(p)).size(13).width(Length::Fill),
                        ico(
                            if sel {
                                icones::check()
                            } else {
                                icones::menos()
                            },
                            14.0,
                            if sel { c.destaque } else { c.fundo }
                        ),
                    ]
                    .align_y(Alignment::Center),
                )
                .width(Length::Fill)
                .padding([8, 10])
                .style(tema::item_de_lista)
                .on_press(Msg::StatusCustomPrazo(p)),
            );
        }
        corpo = corpo.push(
            container(lista)
                .padding(4)
                .style(move |_| container::Style {
                    background: Some(Background::Color(c.elevado)),
                    border: Border {
                        radius: tema::RAIO.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
        );
    }
    if let Some(e) = &est.erro {
        corpo = corpo.push(text(e.clone()).size(13).style(texto_perigo));
    }
    corpo = corpo.push(
        row![
            button(text("Limpar status").size(13))
                .padding([8, 14])
                .style(tema::botao_secundario)
                .on_press(Msg::LimpaStatusCustom),
            space().width(Length::Fill),
            button(text("Cancelar").size(13))
                .padding([8, 14])
                .style(tema::botao_secundario)
                .on_press(Msg::FechaStatusCustom),
            button(
                text(if est.salvando {
                    "Salvando…"
                } else {
                    "Salvar"
                })
                .size(13)
            )
            .padding([8, 16])
            .style(tema::botao_primario)
            .on_press_maybe(
                (!est.salvando && (!est.texto.trim().is_empty() || !est.emoji.is_empty()))
                    .then_some(Msg::SalvaStatusCustom)
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    );
    container(corpo)
        .padding(20)
        .width(420)
        .style(move |_| container::Style {
            background: Some(Background::Color(c.lateral)),
            border: Border {
                color: c.borda,
                width: 1.0,
                radius: tema::RAIO_GRANDE.into(),
            },
            ..Default::default()
        })
        .into()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn rotulos_de_prazo() {
        assert_eq!(rotulo_prazo(None), "Não limpar");
        assert!(rotulo_prazo(Some(30)).starts_with("30 minutos ("));
        assert!(rotulo_prazo(Some(1440)).starts_with("24 horas (amanhã às "));
    }
}
