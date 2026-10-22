//! Popup do avatar (rodapé da coluna): banner, foto, status personalizado,
//! nome, atividade e as ações da conta (print 3 do Discord).

use iced::widget::{Column, button, column, container, image, row, space, stack, text};
use iced::{Alignment, Background, Border, ContentFit, Element, Length, Padding};

use crate::api::{Atividade, StatusCustom};
use crate::ui::efeitos::ico;
use crate::ui::status::{STATUS_OPCOES, Sub, bolinha_status, rotulo_status, sub_widget};
use crate::{Msg, avatar, fraco, icones, negrito, tema};

pub struct DadosPopup<'a> {
    pub nome: &'a str,
    pub usuario: Option<&'a str>,
    pub uid: &'a str,
    pub avatar: Option<&'a str>,
    pub banner: Option<&'a str>,
    pub status: &'a str,
    pub custom: Option<&'a StatusCustom>,
    pub atividade: Option<Atividade>,
    /// A lista de presenças (Online/Ausente/…) está aberta?
    pub presenca_aberta: bool,
}

const LARGURA: f32 = 320.0;

fn linha<'a>(
    icone: iced::widget::svg::Handle,
    rotulo: &'a str,
    msg: Msg,
    cor_icone: iced::Color,
) -> Element<'a, Msg> {
    let c = tema::cores();
    button(
        row![ico(icone, 16.0, cor_icone), text(rotulo).size(14)]
            .spacing(12)
            .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding([9, 12])
    .style(move |_, st| button::Style {
        background: matches!(st, button::Status::Hovered | button::Status::Pressed)
            .then_some(Background::Color(c.forte)),
        text_color: c.texto,
        border: Border {
            radius: tema::RAIO.into(),
            ..Default::default()
        },
        ..Default::default()
    })
    .on_press(msg)
    .into()
}

fn bloco<'a>(itens: Vec<Element<'a, Msg>>) -> Element<'a, Msg> {
    let c = tema::cores();
    container(Column::with_children(itens).spacing(1))
        .padding(4)
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(c.elevado)),
            border: Border {
                radius: tema::RAIO_GRANDE.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

pub fn popup<'a>(d: DadosPopup<'a>) -> Element<'a, Msg> {
    let c = tema::cores();
    let faz = |m: Msg| Msg::MenuFaz(Box::new(m));
    let cor_banner = tema::cor_de_avatar(d.uid);
    let banner: Element<'a, Msg> = match d.banner.and_then(avatar::handle_exibicao) {
        Some(h) => image(h)
            .width(Length::Fill)
            .height(84)
            .content_fit(ContentFit::Cover)
            .into(),
        None => container(space())
            .width(Length::Fill)
            .height(84)
            .style(move |_| container::Style {
                background: Some(Background::Color(cor_banner)),
                ..Default::default()
            })
            .into(),
    };
    // Banner e foto abrem o editor de perfil; o mouse em cima mostra o lápis.
    let banner: Element<'a, Msg> = button(crate::ui::efeitos::com_lapis("pp-banner", banner, 0.0))
        .padding(0)
        .width(Length::Fill)
        .style(tema::botao_transparente)
        .on_press(faz(Msg::AbreEditarPerfil))
        .into();
    // Bolha de status personalizado ao lado da foto: "+" quando não há.
    let custom = d.custom.filter(|s| !s.vencido());
    let conteudo_bolha: Element<'a, Msg> = match custom {
        Some(sc) => row![
            text(sc.emoji.clone())
                .size(13)
                .font(crate::tema::FONTE_EMOJI),
            text(sc.texto.clone()).size(12)
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .into(),
        None => row![
            ico(icones::mais(), 12.0, c.texto_medio),
            text("Definir status").size(12).style(fraco),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .into(),
    };
    let bolha = button(conteudo_bolha)
        .padding([6, 10])
        .style(move |_, st| button::Style {
            background: Some(Background::Color(
                if matches!(st, button::Status::Hovered) {
                    c.forte
                } else {
                    c.elevado
                },
            )),
            text_color: c.texto,
            border: Border {
                radius: 14.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .on_press(faz(Msg::AbreStatusCustom));
    let topo = stack![
        column![banner, space().height(44)],
        container(
            button(crate::ui::efeitos::com_lapis(
                "pp-foto",
                crate::ui::avatar::avatar_com(
                    d.avatar,
                    d.uid,
                    d.nome,
                    72.0,
                    Some(d.status),
                    c.lateral,
                    crate::cosmeticos::de(d.uid)
                        .decoracao
                        .as_ref()
                        .map(|x| x.arquivo.as_str()),
                    true
                ),
                36.0,
            ))
            .padding(0)
            .style(tema::botao_transparente)
            .on_press(faz(Msg::AbreEditarPerfil))
        )
        .padding(Padding {
            top: 48.0,
            left: 16.0,
            ..Default::default()
        }),
        container(bolha).padding(Padding {
            top: 96.0,
            left: 96.0,
            ..Default::default()
        }),
    ];
    let atv = Sub::de_pessoa(None, d.atividade.as_ref(), "");
    let escolha = crate::cosmeticos::de(d.uid);
    let mut cabeca = column![crate::ui::nome::nome(
        d.nome,
        escolha.nome.as_ref(),
        20.0,
        c.texto,
        negrito()
    )]
    .spacing(2);
    if let Some(u) = d.usuario {
        cabeca = cabeca.push(text(format!("@{u}")).size(13).style(fraco));
    }
    if !atv.texto.is_empty() {
        cabeca = cabeca.push(sub_widget(&atv, 12.0, c.texto_medio));
    }

    // Presença: uma linha que abre a lista dos quatro estados.
    let presenca: Vec<Element<'a, Msg>> = vec![
        button(
            row![
                bolinha_status(d.status, 10.0),
                text(rotulo_status(d.status)).size(14).width(Length::Fill),
                // O submenu abre à direita: a seta aponta para ele, aberto ou
                // não; aberto, ela só acende.
                ico(
                    icones::seta_direita(),
                    14.0,
                    if d.presenca_aberta {
                        c.texto
                    } else {
                        c.texto_fraco
                    },
                ),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding([9, 12])
        .style(tema::item_de_lista)
        .on_press(Msg::AlternaPresenca)
        .into(),
    ];
    // Passar o mouse na linha abre a lista à direita (ver `submenu_presenca`).
    let presenca: Vec<Element<'a, Msg>> = presenca
        .into_iter()
        .map(|e| {
            iced::widget::mouse_area(e)
                .on_enter(Msg::PresencaHover(true))
                .on_exit(Msg::PresencaHover(false))
                .into()
        })
        .collect();
    let conta = bloco(vec![
        linha(
            icones::lapis(),
            "Editar perfil",
            faz(Msg::AbreEditarPerfil),
            c.texto_medio,
        ),
        linha(
            icones::brilho(),
            "Personalizar perfil",
            faz(Msg::AbreLoja(None)),
            c.texto_medio,
        ),
        bloco_presenca(presenca),
        linha(
            icones::favorito(),
            "Mensagens favoritas",
            faz(Msg::AbreFavoritos),
            c.texto_medio,
        ),
        linha(
            icones::configuracoes(),
            "Configurações",
            faz(Msg::AbreConfiguracoes(None)),
            c.texto_medio,
        ),
    ]);
    let itens_conta: Vec<Element<'a, Msg>> = vec![
        linha(
            icones::pessoa_redonda(),
            "Mudar de conta",
            faz(Msg::Deslogar),
            c.texto_medio,
        ),
        linha(
            icones::copiar(),
            "Copiar ID do usuário",
            faz(Msg::CopiaTexto(d.uid.to_string())),
            c.texto_medio,
        ),
    ];
    let conta2 = bloco(itens_conta);

    let fundo = crate::ui::loja::fundo_do_cartao(&escolha);
    let cartao = container(crate::ui::loja::com_efeito(
        column![
            topo,
            container(column![cabeca, conta, conta2].spacing(10)).padding(Padding {
                top: 0.0,
                right: 12.0,
                bottom: 12.0,
                left: 12.0,
            }),
        ]
        .width(LARGURA)
        .into(),
        &escolha,
    ))
    .clip(true)
    .style(move |_| container::Style {
        background: Some(fundo),
        border: Border {
            color: c.borda,
            width: 1.0,
            radius: tema::RAIO_GRANDE.into(),
        },
        ..Default::default()
    })
    .into();
    crate::ui::loja::com_moldura(cartao, &escolha)
}

/// A lista de estados que abre à direita do popup, com o estado atual marcado.
pub fn submenu_presenca<'a>(atual: &str) -> Element<'a, Msg> {
    let c = tema::cores();
    let mut col = Column::new().spacing(1);
    for (id, rotulo) in STATUS_OPCOES {
        let ativo = *id == atual;
        col = col.push(
            button(
                row![
                    bolinha_status(id, 10.0),
                    text(*rotulo).size(14).width(Length::Fill),
                    if ativo {
                        ico(icones::check(), 14.0, c.destaque)
                    } else {
                        iced::widget::space().width(14).into()
                    },
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .padding([9, 12])
            .style(tema::item_de_lista)
            .on_press(Msg::DefineStatus((*id).to_string())),
        );
    }
    iced::widget::mouse_area(container(col).padding(6).width(210).style(move |_| {
        container::Style {
            background: Some(Background::Color(c.lateral)),
            border: Border {
                color: c.borda,
                width: 1.0,
                radius: tema::RAIO_GRANDE.into(),
            },
            shadow: iced::Shadow {
                color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.4),
                offset: iced::Vector::new(0.0, 4.0),
                blur_radius: 14.0,
            },
            ..Default::default()
        }
    }))
    .on_enter(Msg::PresencaHover(true))
    .on_exit(Msg::PresencaHover(false))
    .into()
}

/// Presença não tem fundo próprio: ela vive dentro do bloco de ações.
fn bloco_presenca<'a>(itens: Vec<Element<'a, Msg>>) -> Element<'a, Msg> {
    Column::with_children(itens).spacing(1).into()
}
