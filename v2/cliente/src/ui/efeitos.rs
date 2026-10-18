//! Efeitos de hover e entrada de janelas (ver `anim`). Só desenho: o estado
//! do tempo vive em `anim`, as telas leem o progresso ao desenhar.

use iced::widget::{button, container, mouse_area, svg, text, tooltip};
use iced::{Alignment, Background, Color, Element, Length, Padding};

use crate::{Msg, anim, tema};

/// Qual movimento o ícone faz quando o mouse entra.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Efeito {
    Nenhum,
    /// Balança de um lado para o outro (mic, fone, GIF, atividades).
    Balanco,
    /// Dá uma volta (engrenagem).
    Giro,
    /// Troca por um emoji colorido sorteado a cada hover.
    EmojiAleatorio,
}

/// Avisa o relógio de animação quando o mouse entra/sai de `conteudo`.
pub fn com_hover<'a>(id: &str, conteudo: impl Into<Element<'a, Msg>>) -> Element<'a, Msg> {
    mouse_area(conteudo)
        .on_enter(Msg::Hover(id.to_string(), true))
        .on_exit(Msg::Hover(id.to_string(), false))
        .into()
}

/// Linha do chat que não é mensagem de texto (aviso de chamada, cartão de app)
/// com o mesmo destaque de hover das mensagens, sem a barra de ações.
pub fn linha_com_hover<'a>(id: &str, conteudo: Element<'a, Msg>) -> Element<'a, Msg> {
    let c = tema::cores();
    let t = anim::t(id);
    com_hover(
        id,
        container(conteudo)
            .width(Length::Fill)
            .style(move |_| container::Style {
                background: (t > 0.01).then_some(Background::Color(Color {
                    a: 0.05 * t,
                    ..c.texto
                })),
                ..Default::default()
            }),
    )
}

/// Botão de ícone da barra de controles, com efeito de hover.
pub fn botao_icone_anim<'a>(
    id: &'static str,
    icone: svg::Handle,
    ligado: bool,
    perigoso: bool,
    msg: Option<Msg>,
    efeito: Efeito,
) -> Element<'a, Msg> {
    let c = tema::cores();
    let cor = if perigoso {
        c.perigo
    } else if ligado {
        c.destaque
    } else {
        c.texto_fraco
    };
    let miolo: Element<'a, Msg> = if efeito == Efeito::EmojiAleatorio && anim::dentro(id) {
        // Colorido enquanto o mouse está em cima; volta ao ícone ao sair.
        container(
            text(anim::emoji(id))
                .size(16)
                .font(crate::tema::FONTE_EMOJI),
        )
        .width(18)
        .height(18)
        .center_x(18)
        .center_y(18)
        .into()
    } else {
        let angulo = match efeito {
            Efeito::Balanco => anim::balanco(id),
            Efeito::Giro => anim::giro(id),
            _ => 0.0,
        };
        svg(icone)
            .width(18)
            .height(18)
            .rotation(iced::Radians(angulo))
            .style(tema::icone_cor(cor))
            .into()
    };
    let b = button(miolo)
        .padding(7)
        .style(tema::botao_icone(ligado, perigoso))
        .on_press_maybe(msg);
    com_hover(id, b)
}

/// Janela por cima do app: fundo escurece e o cartão sobe suavemente ao abrir.
/// `id` identifica a janela para a animação reiniciar a cada abertura.
pub fn janela<'a>(id: &str, conteudo: Element<'a, Msg>, escuridao: f32) -> Element<'a, Msg> {
    let p = anim::abertura(id);
    container(container(conteudo).padding(Padding {
        top: (1.0 - p) * 24.0,
        ..Default::default()
    }))
    .width(Length::Fill)
    .height(Length::Fill)
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .style(move |_| container::Style {
        background: Some(Background::Color(Color::from_rgba(
            0.0,
            0.0,
            0.0,
            escuridao * p,
        ))),
        ..Default::default()
    })
    .into()
}

/// Item de lista com fundo que faz transição suave ao passar o mouse.
pub fn item_suave(
    id: String,
    selecionado: bool,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_, status| {
        let c = tema::cores();
        let mut t = anim::t(&id);
        if matches!(status, button::Status::Pressed) {
            t = 1.0;
        }
        let base = if selecionado { c.elevado } else { c.lateral };
        let alvo = if selecionado { c.forte } else { c.elevado };
        let fundo = tema::mistura(base, alvo, t);
        let fundo = if matches!(status, button::Status::Pressed) {
            tema::mistura(fundo, Color::BLACK, 0.12)
        } else {
            fundo
        };
        button::Style {
            background: (selecionado || t > 0.01).then_some(Background::Color(fundo)),
            text_color: c.texto,
            border: iced::Border {
                radius: tema::RAIO.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

/// Ícone Lucide solto (cor sólida), para quem não é botão.
pub fn ico<'a, M: 'a>(icone: svg::Handle, tam: f32, cor: Color) -> Element<'a, M> {
    svg(icone)
        .width(tam)
        .height(tam)
        .style(tema::icone_cor(cor))
        .into()
}

/// Aviso curto no topo da janela ("Mensagem encaminhada!").
pub fn toast<'a>(texto: &'a str) -> Element<'a, Msg> {
    let c = tema::cores();
    container(
        container(
            iced::widget::row![
                ico(crate::icones::encaminhar(), 16.0, c.texto_medio),
                text(texto).size(14),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .padding([10, 16])
        .style(move |_| container::Style {
            background: Some(Background::Color(c.lateral)),
            border: iced::Border {
                color: c.borda,
                width: 1.0,
                radius: tema::RAIO_GRANDE.into(),
            },
            ..Default::default()
        }),
    )
    .width(Length::Fill)
    .padding(Padding {
        top: 56.0,
        ..Default::default()
    })
    .align_x(Alignment::Center)
    .into()
}

/// Envolve `conteudo` numa dica arredondada que aparece ao passar o mouse.
pub fn com_dica<'a>(
    conteudo: impl Into<Element<'a, Msg>>,
    texto: &'static str,
    posicao: tooltip::Position,
) -> Element<'a, Msg> {
    tooltip(
        conteudo,
        container(text(texto).size(12))
            .padding([5, 9])
            .style(tema::dica),
        posicao,
    )
    .gap(6)
    .into()
}

/// Escurece `conteudo` e mostra um lápis no centro quando o mouse está em cima
/// (foto e banner do editor de perfil). `raio` arredonda o escurecimento.
pub fn com_lapis<'a>(
    id: &str,
    conteudo: impl Into<Element<'a, Msg>>,
    raio: f32,
) -> Element<'a, Msg> {
    let t = anim::t(id);
    let sobre: Element<'a, Msg> = if t > 0.01 {
        container(ico(
            crate::icones::lapis(),
            22.0,
            Color::from_rgba(1.0, 1.0, 1.0, t),
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_| container::Style {
            background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.55 * t))),
            border: iced::Border {
                radius: raio.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
    } else {
        iced::widget::space().into()
    };
    com_hover(id, iced::widget::stack![conteudo.into(), sobre])
}

/// Entrada de popup: sobe uns pixels com suavidade ao abrir. `id` identifica o
/// popup (a animação recomeça a cada abertura). Respeita "reduzir animações".
pub fn entrada<'a>(id: &str, conteudo: impl Into<Element<'a, Msg>>) -> Element<'a, Msg> {
    let p = anim::abertura(id);
    container(conteudo)
        .padding(Padding {
            top: (1.0 - p) * 10.0,
            ..Default::default()
        })
        .into()
}

/// Como `janela`, e clicar no fundo escurecido (fora do cartão) fecha com `fecha`.
pub fn janela_fechavel<'a>(
    id: &str,
    conteudo: Element<'a, Msg>,
    escuridao: f32,
    fecha: Msg,
) -> Element<'a, Msg> {
    // O cartão captura o clique (Nenhum) para o clique nele não subir até o fundo.
    let cartao = mouse_area(conteudo).on_press(Msg::Nenhum);
    mouse_area(janela(id, cartao.into(), escuridao))
        .on_press(fecha)
        .into()
}
