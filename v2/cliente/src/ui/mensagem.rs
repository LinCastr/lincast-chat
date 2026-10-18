//! Barra que aparece ao passar o mouse numa mensagem (reações rápidas,
//! emoji, responder, encaminhar e "⋯").

use iced::widget::{button, container, row, text};
use iced::{Alignment, Background, Border, Element, Length};

use crate::ui::efeitos::ico;
use crate::{Msg, icones, tema};

/// Reações rápidas da barra (todas da lista fechada do servidor).
pub const REACOES_RAPIDAS: [&str; 3] = ["✅", "🔥", "👍"];

pub fn barra_hover<'a>(id: &str, autor: &str, texto: &str) -> Element<'a, Msg> {
    let c = tema::cores();
    let botao = |conteudo: Element<'a, Msg>, msg: Msg| -> Element<'a, Msg> {
        button(conteudo)
            .padding([3, 5])
            .style(tema::botao_icone(false, false))
            .on_press(msg)
            .into()
    };
    let mut r = row![].spacing(0).align_y(Alignment::Center);
    for e in REACOES_RAPIDAS {
        r = r.push(botao(
            text(e).size(14).font(crate::tema::FONTE_EMOJI).into(),
            Msg::ReagirChat {
                id: id.to_string(),
                emoji: e.to_string(),
            },
        ));
    }
    r = r
        .push(botao(
            ico(icones::smile(), 14.0, c.texto_medio),
            Msg::AlternaSeletorReacao(id.to_string()),
        ))
        .push(botao(
            ico(icones::responder(), 14.0, c.texto_medio),
            Msg::IniciaRespostaChat {
                id: id.to_string(),
                autor: autor.to_string(),
                texto: crate::ui::chat::trunca_trecho(texto, 120),
            },
        ))
        .push(botao(
            ico(icones::encaminhar(), 14.0, c.texto_medio),
            Msg::AbreEncaminhar(id.to_string()),
        ))
        .push(botao(
            ico(icones::reticencias(), 14.0, c.texto_medio),
            Msg::AbreMenuMensagem(id.to_string()),
        ));
    container(r)
        .padding(1)
        .width(Length::Shrink)
        .style(move |_| container::Style {
            background: Some(Background::Color(c.lateral)),
            border: Border {
                color: c.borda,
                width: 1.0,
                radius: tema::RAIO.into(),
            },
            ..Default::default()
        })
        .into()
}
