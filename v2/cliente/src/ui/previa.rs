//! Prévia de perfil: o popup que abre ao clicar na foto ou no nome de quem
//! mandou uma mensagem (banner, foto, nome, status, bio e atalhos).

use iced::widget::{button, column, container, image, row, space, stack, text};
use iced::{Alignment, Background, Border, ContentFit, Element, Length, Padding};

use crate::ui::efeitos::ico;
use crate::ui::status::{bolinha_status, rotulo_status};
use crate::{Msg, avatar, fraco, icones, negrito, tema};

pub const LARGURA: f32 = 300.0;
pub const ALTURA: f32 = 300.0;

pub struct DadosPrevia<'a> {
    pub uid: &'a str,
    pub nome: &'a str,
    pub usuario: Option<&'a str>,
    pub avatar: Option<&'a str>,
    pub banner: Option<&'a str>,
    pub cor: Option<iced::Color>,
    pub status: Option<&'a str>,
    pub bio: Option<&'a str>,
    /// É a minha própria conta? (some o botão de mensagem)
    pub sou_eu: bool,
}

fn cartao<'a>(corpo: Element<'a, Msg>) -> Element<'a, Msg> {
    let c = tema::cores();
    container(corpo)
        .width(LARGURA)
        .clip(true)
        .style(move |_| container::Style {
            background: Some(Background::Color(c.lateral)),
            text_color: Some(c.texto),
            border: Border {
                color: c.borda,
                width: 1.0,
                radius: tema::RAIO_GRANDE.into(),
            },
            shadow: iced::Shadow {
                color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.45),
                offset: iced::Vector::new(0.0, 4.0),
                blur_radius: 16.0,
            },
            ..Default::default()
        })
        .into()
}

fn topo_redondo() -> iced::border::Radius {
    iced::border::Radius {
        top_left: tema::RAIO_GRANDE,
        top_right: tema::RAIO_GRANDE,
        ..Default::default()
    }
}

fn faixa<'a>(
    banner: Option<&str>,
    cor: iced::Color,
    altura: f32,
    interno: Element<'a, Msg>,
) -> Element<'a, Msg> {
    let fundo: Element<'a, Msg> = match banner.and_then(avatar::handle_exibicao) {
        Some(h) => image(h)
            .width(Length::Fill)
            .height(altura)
            .content_fit(ContentFit::Cover)
            .border_radius(topo_redondo())
            .into(),
        None => container(space())
            .width(Length::Fill)
            .height(altura)
            .style(move |_| container::Style {
                background: Some(Background::Color(cor)),
                border: Border {
                    radius: topo_redondo(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into(),
    };
    stack![fundo, interno].into()
}

fn botao_acao<'a>(rotulo: &'static str, msg: Msg, primario: bool) -> Element<'a, Msg> {
    button(text(rotulo).size(13).center())
        .width(Length::Fill)
        .padding([7, 10])
        .style(if primario {
            tema::botao_primario
        } else {
            tema::botao_secundario
        })
        .on_press(msg)
        .into()
}

/// Prévia de uma pessoa.
pub fn previa<'a>(d: DadosPrevia<'a>) -> Element<'a, Msg> {
    let c = tema::cores();
    let cor = d.cor.unwrap_or_else(|| tema::cor_de_avatar(d.uid));
    let topo = column![
        faixa(d.banner, cor, 72.0, space().into()),
        space().height(36)
    ];
    let foto = container(crate::ui::avatar::avatar(
        d.avatar, d.uid, d.nome, 64.0, d.status, c.lateral,
    ))
    .padding(Padding {
        top: 40.0,
        left: 14.0,
        ..Default::default()
    });
    let mut corpo = column![stack![topo, foto]].spacing(0);

    let mut info = column![text(d.nome).size(17).font(negrito())].spacing(2);
    if let Some(u) = d.usuario {
        info = info.push(text(format!("@{u}")).size(12).style(fraco));
    }
    if let Some(s) = d.status {
        info = info.push(
            row![
                bolinha_status(s, 8.0),
                text(rotulo_status(s)).size(12).style(fraco)
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        );
    }
    if let Some(b) = d.bio.filter(|b| !b.trim().is_empty()) {
        info = info.push(space().height(6));
        info = info.push(text(b.trim().to_string()).size(13));
    }
    corpo = corpo.push(container(info).padding([6, 14]));

    let alvo = d.usuario.unwrap_or(d.nome).to_string();
    let mut acoes = row![botao_acao(
        "Ver perfil",
        Msg::MenuFaz(Box::new(Msg::AbrePerfilUsuario(alvo))),
        false,
    )]
    .spacing(8);
    if !d.sou_eu {
        acoes = acoes.push(botao_acao(
            "Mensagem",
            Msg::MenuFaz(Box::new(Msg::AbreDm {
                uid: d.uid.to_string(),
                nome: d.nome.to_string(),
                ligar: false,
            })),
            true,
        ));
    }
    corpo = corpo.push(container(acoes).padding([8, 14]));
    cartao(corpo.into())
}

/// Prévia de um app (o bot do Wordle).
pub fn previa_app<'a>(nome: &'a str) -> Element<'a, Msg> {
    let c = tema::cores();
    let faixa_app = faixa(None, c.palco, 64.0, space().into());
    let logo = container(ico(icones::grade(), 30.0, c.texto))
        .padding(12)
        .style(move |_| container::Style {
            background: Some(Background::Color(c.elevado)),
            border: Border {
                color: c.lateral,
                width: 4.0,
                radius: 40.0.into(),
            },
            ..Default::default()
        });
    let topo = column![faixa_app, space().height(30)];
    let foto = container(logo).padding(Padding {
        top: 24.0,
        left: 14.0,
        ..Default::default()
    });
    let selo = container(
        text("APP")
            .size(10)
            .font(negrito())
            .color(iced::Color::WHITE),
    )
    .padding([1, 5])
    .style(move |_| container::Style {
        background: Some(Background::Color(c.destaque)),
        border: Border {
            radius: 3.0.into(),
            ..Default::default()
        },
        ..Default::default()
    });
    let info = column![
        row![text(nome).size(17).font(negrito()), selo]
            .spacing(8)
            .align_y(Alignment::Center),
        text("Aplicativo do chat").size(12).style(fraco),
        space().height(6),
        text("Adivinhe a palavra de 5 letras em 6 tentativas. Uma palavra nova por dia, igual para todo mundo.")
            .size(13),
    ]
    .spacing(2);
    cartao(
        column![
            stack![topo, foto],
            container(info).padding([6, 14]),
            space().height(10)
        ]
        .into(),
    )
}
