use crate::sala::Pessoa;
use crate::{Msg, icones, tema};
use iced::widget::{button, row, slider, svg, text};
use iced::{Alignment, Element, Length};

// Sem consumidor até o menu de contexto (C5) e o palco da chamada (C8).
#[allow(dead_code)]
pub fn controle_volume_participante<'a>(p: &'a Pessoa) -> Element<'a, Msg> {
    let c = tema::cores();
    let mudo = p.volume <= 0.0;
    let id = p.id.clone();
    let botao_mute = button(
        svg(if mudo {
            icones::volume_mudo()
        } else {
            icones::volume()
        })
        .width(13)
        .height(13)
        .style(tema::icone_cor(if mudo { c.perigo } else { c.texto_fraco })),
    )
    .padding(2)
    .style(tema::botao_secundario)
    .on_press(Msg::AlternaMute(id.clone()));

    let barra = slider(0.0..=2.0, p.volume, move |v| {
        Msg::AjustaVolume(id.clone(), v)
    })
    .step(0.05_f32)
    .style(tema::estilo_slider)
    .width(Length::Fill);

    let rotulo = text(format!("{:.0}%", p.volume * 100.0))
        .size(11)
        .style(move |_| text::Style {
            color: Some(c.texto_fraco),
        })
        .width(36);

    row![botao_mute, barra, rotulo]
        .spacing(4)
        .align_y(Alignment::Center)
        .into()
}

pub fn controle_volume_video<'a>(
    dono_id: String,
    de_tela: bool,
    vol: f32,
    mudo: bool,
) -> Element<'a, Msg> {
    let c = tema::cores();
    let id_mute = dono_id.clone();
    let id_vol = dono_id;

    let btn_mute = button(
        svg(if mudo {
            icones::volume_mudo()
        } else {
            icones::volume()
        })
        .width(14)
        .height(14)
        .style(tema::icone_cor(if mudo { c.perigo } else { c.texto_fraco })),
    )
    .padding(3)
    .style(tema::botao_secundario)
    .on_press(if de_tela {
        Msg::AlternaMuteTela(id_mute)
    } else {
        Msg::AlternaMute(id_mute)
    });

    let barra = slider(0.0..=2.0, vol, move |val| {
        if de_tela {
            Msg::AjustaVolumeTela(id_vol.clone(), val)
        } else {
            Msg::AjustaVolume(id_vol.clone(), val)
        }
    })
    .step(0.05_f32)
    .style(tema::estilo_slider)
    .width(100);

    let pct = text(format!("{:.0}%", vol * 100.0))
        .size(11)
        .style(move |_| text::Style {
            color: Some(c.texto_fraco),
        });

    row![btn_mute, barra, pct]
        .spacing(6)
        .align_y(Alignment::Center)
        .into()
}
