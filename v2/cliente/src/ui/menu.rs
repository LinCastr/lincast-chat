//! Menu de contexto (botão direito, C5): em pessoa e em conversa da lista.

use iced::widget::{Column, button, column, container, mouse_area, row, space, svg, text};
use iced::{Alignment, Background, Border, Element, Length, Padding};

use crate::{Msg, tema};

const LARGURA: f32 = 240.0;

/// Um item do menu. `perigo` pinta de vermelho (sair, remover).
pub fn item<'a>(
    icone: iced::widget::svg::Handle,
    rotulo: impl Into<String>,
    msg: Msg,
    perigo: bool,
) -> Element<'a, Msg> {
    let c = tema::cores();
    let cor = if perigo { c.perigo } else { c.texto };
    button(
        row![
            svg(icone)
                .width(16)
                .height(16)
                .style(tema::icone_cor(if perigo {
                    c.perigo
                } else {
                    c.texto_medio
                })),
            text(rotulo.into()).size(14).color(cor),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding([7, 10])
    .style(move |_, st| button::Style {
        background: matches!(st, button::Status::Hovered).then_some(Background::Color(c.forte)),
        text_color: cor,
        border: Border {
            radius: tema::RAIO.into(),
            ..Default::default()
        },
        ..Default::default()
    })
    .on_press(msg)
    .into()
}

pub fn separador<'a>() -> Element<'a, Msg> {
    let c = tema::cores();
    container(space().height(1))
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(c.borda)),
            ..Default::default()
        })
        .padding([0, 0])
        .into()
}

/// Cabeçalho pequeno do menu (nome da pessoa ou da conversa).
pub fn titulo<'a>(t: &str) -> Element<'a, Msg> {
    container(
        text(t.to_string())
            .size(tema::TEXTO_ROTULO)
            .font(crate::negrito())
            .style(crate::fraco),
    )
    .padding([6, 10])
    .into()
}

/// Posição do canto superior esquerdo do menu: onde o mouse estava, sem
/// deixar o menu sair da janela.
pub fn posiciona(x: f32, y: f32, janela: iced::Size, altura_estimada: f32) -> Padding {
    let esq = x.min((janela.width - LARGURA - 8.0).max(0.0));
    let topo = y.min((janela.height - altura_estimada - 8.0).max(0.0));
    Padding {
        top: topo,
        left: esq,
        ..Default::default()
    }
}

/// Coloca o menu sobre a base, com um fundo invisível que fecha ao clicar fora.
pub fn sobre<'a>(
    base: Element<'a, Msg>,
    itens: Column<'a, Msg>,
    padding: Padding,
) -> Element<'a, Msg> {
    let c = tema::cores();
    let caixa = container(itens.spacing(1))
        .padding(6)
        .width(LARGURA)
        .style(move |_| container::Style {
            background: Some(Background::Color(c.elevado)),
            border: Border {
                color: c.borda,
                width: 1.0,
                radius: tema::RAIO_GRANDE.into(),
            },
            ..Default::default()
        });
    let fundo = mouse_area(container(space()).width(Length::Fill).height(Length::Fill))
        .on_press(Msg::FechaMenuContexto)
        .on_right_press(Msg::FechaMenuContexto);
    let posicionado = container(crate::ui::efeitos::entrada("menu-ctx", caixa))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(padding);
    iced::widget::stack([base, fundo.into(), posicionado.into()]).into()
}

/// Uma coluna com `n` itens, só para quem monta o menu não repetir `column!`.
pub fn coluna<'a>(itens: Vec<Element<'a, Msg>>) -> Column<'a, Msg> {
    let mut c = column![];
    for i in itens {
        c = c.push(i);
    }
    c
}

/// Coloca um cartão pronto (prévia de perfil) perto do mouse, com fundo
/// invisível que fecha ao clicar fora.
pub fn sobre_cartao<'a>(
    base: Element<'a, Msg>,
    cartao: Element<'a, Msg>,
    x: f32,
    y: f32,
    janela: iced::Size,
    largura: f32,
    altura: f32,
) -> Element<'a, Msg> {
    let esq = (x + 10.0).min((janela.width - largura - 8.0).max(0.0));
    let topo = (y - 20.0)
        .min((janela.height - altura - 8.0).max(0.0))
        .max(8.0);
    let fundo = mouse_area(container(space()).width(Length::Fill).height(Length::Fill))
        .on_press(Msg::FechaMenuContexto)
        .on_right_press(Msg::FechaMenuContexto);
    let posicionado = container(crate::ui::efeitos::entrada("previa-perfil", cartao))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding {
            top: topo,
            left: esq,
            ..Default::default()
        });
    iced::widget::stack([base, fundo.into(), posicionado.into()]).into()
}
