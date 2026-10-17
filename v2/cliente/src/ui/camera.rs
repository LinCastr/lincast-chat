use crate::{Msg, PreviaCamera, fraco, negrito, tema};
use iced::widget::{Column, button, center, container, image, row, text};
use iced::{Alignment, Element, Length};

pub fn painel_previa_camera<'a>(prev: &'a PreviaCamera) -> Element<'a, Msg> {
    let mut corpo = Column::new().spacing(10);
    corpo = corpo.push(text("Prévia da câmera").size(14).font(negrito()));
    corpo = corpo.push(
        text("Verifique a imagem antes de transmitir para a sala.")
            .size(13)
            .style(fraco),
    );

    if prev.cameras.len() > 1 {
        let mut seletor = row![text("Câmera:").size(13).style(fraco)]
            .spacing(6)
            .align_y(Alignment::Center);
        for cam in &prev.cameras {
            let ativo = cam.indice == prev.indice_selecionado;
            seletor = seletor.push(
                button(text(&cam.nome).size(12))
                    .padding([4, 8])
                    .style(if ativo {
                        tema::botao_primario
                    } else {
                        tema::botao_secundario
                    })
                    .on_press(Msg::TrocaCameraPrevia(cam.indice)),
            );
        }
        corpo = corpo.push(seletor);
    }

    let area_video = if let Some(img) = &prev.imagem {
        container(
            image(img.clone())
                .width(Length::Fixed(320.0))
                .height(Length::Fixed(240.0))
                .content_fit(iced::ContentFit::Cover),
        )
        .style(tema::moldura_video)
    } else {
        container(center(
            text("Iniciando prévia da câmera…").size(13).style(fraco),
        ))
        .width(Length::Fixed(320.0))
        .height(Length::Fixed(240.0))
        .style(tema::moldura_video)
    };
    corpo = corpo.push(area_video);

    let botoes = row![
        button(text("Ligar câmera").size(13))
            .padding([6, 14])
            .style(tema::botao_primario)
            .on_press(Msg::ConfirmaCamera),
        button(text("Cancelar").size(13))
            .padding([6, 14])
            .style(tema::botao_secundario)
            .on_press(Msg::CancelaPreviaCamera),
    ]
    .spacing(8);
    corpo = corpo.push(botoes);

    container(corpo)
        .padding(12)
        .style(tema::painel_lateral)
        .into()
}
