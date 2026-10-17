use crate::{Msg, icones, tema};
use iced::widget::{button, container, row, svg, text};
use iced::{Alignment, Element, Length};

pub fn aviso_falando_no_mudo<'a>(mudo_forcado: bool) -> Element<'a, Msg> {
    let c = tema::cores();
    let mut conteudo = row![
        svg(icones::mic_off())
            .width(16)
            .height(16)
            .style(tema::icone_cor(c.perigo)),
        text(if mudo_forcado {
            "Você foi silenciado pelo dono da sala."
        } else {
            "Você está falando com o microfone mutado."
        })
        .size(13)
        .style(move |_| text::Style {
            color: Some(c.texto),
        }),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    if !mudo_forcado {
        conteudo = conteudo.push(
            button(text("Desmutar").size(12))
                .padding([3, 8])
                .style(tema::botao_primario)
                .on_press(Msg::AlternaMudo),
        );
    }

    container(conteudo)
        .padding([8, 14])
        .style(tema::aviso_erro)
        .width(Length::Fill)
        .into()
}

pub fn aviso_eco<'a>() -> Element<'a, Msg> {
    let c = tema::cores();
    let conteudo = row![
        text("O som da chamada parece estar voltando pelo seu microfone. Use fone de ouvido, ou abaixe o volume.")
            .size(13)
            .style(move |_| text::Style {
                color: Some(c.texto),
            }),
        button(text("Entendi").size(12))
            .padding([3, 8])
            .style(tema::botao_secundario)
            .on_press(Msg::DispensaAvisoEco),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    container(conteudo)
        .padding([8, 14])
        .style(tema::aviso_alerta)
        .width(Length::Fill)
        .into()
}
