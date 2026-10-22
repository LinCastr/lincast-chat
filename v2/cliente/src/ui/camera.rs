use crate::{Msg, PreviaCamera, fraco, icones, negrito, tema, ui};
use iced::widget::{
    Column, button, center, column, container, image, pick_list, row, space, stack, svg, text,
};
use iced::{Alignment, Background, Border, Element, Length};

/// Camera preview as a floating window (like "Compartilhar tela"): it opens
/// over the call and never pushes the chat.
pub fn painel_previa_camera<'a>(prev: &'a PreviaCamera) -> Element<'a, Msg> {
    let c = tema::cores();
    let fechar = ui::efeitos::com_dica(
        crate::botao_icone(
            icones::fechar(),
            false,
            false,
            Some(Msg::CancelaPreviaCamera),
        ),
        "Fechar",
        iced::widget::tooltip::Position::Bottom,
    );
    let cabecalho = row![
        svg(icones::camera())
            .width(22)
            .height(22)
            .style(tema::icone_cor(c.destaque)),
        column![
            text("Ligar câmera").size(20).font(negrito()),
            text("Confira a imagem antes de mostrar para a chamada.")
                .size(13)
                .style(fraco),
        ]
        .spacing(2)
        .width(Length::Fill),
        fechar,
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    let mut corpo = Column::new().spacing(12).push(cabecalho);
    if prev.cameras.len() > 1 {
        let nomes: Vec<String> = prev.cameras.iter().map(|cam| cam.nome.clone()).collect();
        let atual = prev
            .cameras
            .iter()
            .find(|cam| cam.indice == prev.indice_selecionado)
            .map(|cam| cam.nome.clone());
        let cameras = prev.cameras.clone();
        corpo = corpo.push(
            pick_list(nomes, atual, move |nome: String| {
                let indice = cameras
                    .iter()
                    .find(|cam| cam.nome == nome)
                    .map(|cam| cam.indice)
                    .unwrap_or(0);
                Msg::TrocaCameraPrevia(indice)
            })
            .width(Length::Fill)
            .padding([8, 12])
            .style(tema::lista_suspensa)
            .menu_style(tema::menu_suspenso),
        );
    }

    let area_video = if let Some(img) = &prev.imagem {
        container(stack![
            image(img.clone())
                .width(Length::Fill)
                .height(Length::Fixed(270.0))
                .content_fit(iced::ContentFit::Cover),
            ui::video::marca_desenho(&prev.desenhado),
        ])
        .style(tema::moldura_video)
    } else {
        container(center(
            text("Iniciando prévia da câmera…").size(13).style(fraco),
        ))
        .width(Length::Fill)
        .height(Length::Fixed(270.0))
        .style(tema::moldura_video)
    };
    corpo = corpo.push(area_video);
    corpo = corpo.push(
        row![
            space().width(Length::Fill),
            button(text("Cancelar").size(14))
                .padding([8, 16])
                .style(tema::botao_secundario)
                .on_press(Msg::CancelaPreviaCamera),
            button(text("Ligar câmera").size(14))
                .padding([8, 16])
                .style(tema::botao_primario)
                .on_press(Msg::ConfirmaCamera),
        ]
        .spacing(8),
    );

    container(corpo)
        .padding(20)
        .width(520)
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
