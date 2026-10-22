//! Avatar de pessoa e ícone de grupo, no desenho do protótipo: círculo com a
//! foto, ou a inicial sobre uma cor fixa por uid; bolinha de status no canto;
//! grupo sem imagem mostra as fotos de dois membros sobrepostas.

use iced::widget::{container, image, stack, text};
use iced::{Alignment, Background, Border, Color, ContentFit, Element, Length};

use crate::{Msg, avatar, tema, ui};

/// Um membro, o que o ícone de grupo precisa saber dele.
// Sem uso até a lista de conversas (C1), que é quem desenha o ícone de grupo.
#[allow(dead_code)]
pub struct MembroDoIcone<'a> {
    pub chave: &'a str,
    pub nome: &'a str,
    pub url: Option<&'a str>,
}

fn inicial(nome: &str) -> String {
    nome.chars()
        .find(|c| c.is_alphanumeric())
        .or_else(|| nome.chars().next())
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".into())
}

/// Avisa o app quando o mouse está sobre um GIF animado (modo "só com o mouse
/// em cima"). Imagem que não é GIF animado passa sem custo.
pub fn hover_gif<'a>(elemento: impl Into<Element<'a, Msg>>, url: Option<&str>) -> Element<'a, Msg> {
    let elemento = elemento.into();
    match url.filter(|u| crate::gifanim::animada(u)) {
        Some(u) => iced::widget::mouse_area(elemento)
            .on_enter(Msg::HoverGif(Some(u.to_string())))
            .on_exit(Msg::HoverGif(None))
            .into(),
        None => elemento,
    }
}

fn circulo<'a>(url: Option<&str>, chave: &str, nome: &str, tamanho: f32) -> Element<'a, Msg> {
    if let Some(h) = url.and_then(avatar::handle_exibicao) {
        return hover_gif(
            image(h)
                .width(tamanho)
                .height(tamanho)
                .content_fit(ContentFit::Cover)
                .border_radius(tamanho / 2.0),
            url,
        );
    }
    let fundo = tema::cor_de_avatar(chave);
    container(
        text(inicial(nome))
            .size((tamanho * 0.42).max(10.0))
            .color(Color::WHITE),
    )
    .width(tamanho)
    .height(tamanho)
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .style(move |_| container::Style {
        background: Some(Background::Color(fundo)),
        border: Border {
            radius: (tamanho / 2.0).into(),
            ..Default::default()
        },
        ..Default::default()
    })
    .into()
}

/// Avatar redondo. `status` desenha a bolinha no canto inferior direito, com
/// um aro da cor de `fundo_do_aro` (a cor do painel onde o avatar está).
/// Leva a decoração que a pessoa (`chave` = uid) escolheu, animada com o mouse
/// em cima.
pub fn avatar<'a>(
    url: Option<&str>,
    chave: &str,
    nome: &str,
    tamanho: f32,
    status: Option<&str>,
    fundo_do_aro: Color,
) -> Element<'a, Msg> {
    let deco = crate::cosmeticos::de(chave)
        .decoracao
        .as_ref()
        .map(|d| d.arquivo.clone());
    avatar_com(
        url,
        chave,
        nome,
        tamanho,
        status,
        fundo_do_aro,
        deco.as_deref(),
        false,
    )
}

/// Escala da decoração em relação à foto (a do Discord passa 20% da borda).
pub const ESCALA_DECORACAO: f32 = 1.2;

/// Como `avatar`, com a decoração dada (prévias, perfil). `animar` faz ela
/// andar sempre; senão, só com o mouse em cima.
#[allow(clippy::too_many_arguments)]
pub fn avatar_com<'a>(
    url: Option<&str>,
    chave: &str,
    nome: &str,
    tamanho: f32,
    status: Option<&str>,
    fundo_do_aro: Color,
    decoracao: Option<&str>,
    animar: bool,
) -> Element<'a, Msg> {
    let base = circulo(url, chave, nome, tamanho);
    let deco: Option<Element<'a, Msg>> = decoracao.and_then(|d| {
        let anda = animar || crate::gifanim::em_hover(d);
        crate::cosmeticos::quadro(d, anda).map(|q| {
            crate::ui::transborda::transborda(
                crate::ui::loja::img_q(q, Length::Fill, Length::Fill, ContentFit::Contain),
                iced::Size::new(tamanho, tamanho),
                ESCALA_DECORACAO,
            )
        })
    });
    let base: Element<'a, Msg> = match deco {
        Some(d) => {
            let pilha = stack![base, d].width(tamanho).height(tamanho);
            match decoracao.filter(|_| !animar) {
                Some(u) => iced::widget::mouse_area(pilha)
                    .on_enter(Msg::HoverGif(Some(u.to_string())))
                    .on_exit(Msg::HoverGif(None))
                    .into(),
                None => pilha.into(),
            }
        }
        None => base,
    };
    let Some(st) = status else {
        return base;
    };
    let d = (tamanho * 0.36).clamp(8.0, 16.0);
    let aro = 3.0_f32.min(d / 3.0);
    let bolinha = container(ui::status::bolinha_status(st, d - aro * 2.0))
        .padding(aro)
        .style(move |_| container::Style {
            background: Some(Background::Color(fundo_do_aro)),
            border: Border {
                radius: (d / 2.0).into(),
                ..Default::default()
            },
            ..Default::default()
        });
    stack![
        base,
        container(bolinha)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::End)
            .align_y(Alignment::End),
    ]
    .width(tamanho)
    .height(tamanho)
    .into()
}

/// Ícone de grupo: a imagem escolhida (`icone`), ou as fotos de dois membros
/// sobrepostas (o primeiro em cima à esquerda, o segundo embaixo à direita).
#[allow(dead_code)]
pub fn icone_grupo<'a>(
    icone: Option<&str>,
    membros: &[MembroDoIcone<'_>],
    tamanho: f32,
    fundo_do_aro: Color,
) -> Element<'a, Msg> {
    if let Some(h) = icone.and_then(avatar::handle_exibicao) {
        return image(h)
            .width(tamanho)
            .height(tamanho)
            .content_fit(ContentFit::Cover)
            .border_radius(tamanho / 2.0)
            .into();
    }
    let lado = (tamanho * 0.69).round();
    let aro = 2.0;
    let um = |m: Option<&MembroDoIcone<'_>>| -> Element<'a, Msg> {
        let (chave, nome, url) = m.map_or(("", "?", None), |m| (m.chave, m.nome, m.url));
        container(circulo(url, chave, nome, lado - aro * 2.0))
            .padding(aro)
            .style(move |_| container::Style {
                background: Some(Background::Color(fundo_do_aro)),
                border: Border {
                    radius: (lado / 2.0).into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
    };
    let canto = |e: Element<'a, Msg>, dir: bool| -> Element<'a, Msg> {
        container(e)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(if dir {
                Alignment::End
            } else {
                Alignment::Start
            })
            .align_y(if dir {
                Alignment::End
            } else {
                Alignment::Start
            })
            .into()
    };
    stack![
        canto(um(membros.first()), false),
        canto(um(membros.get(1).or(membros.first())), true),
    ]
    .width(tamanho)
    .height(tamanho)
    .into()
}
