//! Módulo de interface da lista e gerenciamento de amigos (S8).

use iced::widget::{
    Column, button, column, container, row, scrollable, space, svg, text, text_input,
};
use iced::{Alignment, Border, Color, Element, Length};

use crate::api::{Atividade, CartaoPerfil, RespostaAmigos};
use crate::{Msg, botao_icone, destaque, fraco, icones, negrito, tema, texto_perigo};

/// Sub-aba ativa dentro do painel de amigos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SubAbaAmigos {
    #[default]
    Disponiveis,
    Todos,
    Pendentes,
    Adicionar,
}

/// Estado da tela/painel de amigos.
#[derive(Debug, Clone, Default)]
pub struct EstadoAmigos {
    pub sub_aba: SubAbaAmigos,
    pub input_usuario: String,
    pub enviando_pedido: bool,
    pub erro: Option<String>,
    pub sucesso: Option<String>,
    pub amigo_para_remover: Option<String>,
}

/// Formata a descrição da atividade atual de um usuário (jogo ou música).
pub fn formata_atividade(atividade: Option<&Atividade>, em_chamada: bool) -> Option<String> {
    if let Some(atv) = atividade {
        if let Some(jogo) = &atv.jogo {
            let origem_str = match jogo.origem.as_deref() {
                Some("steam") => " (Steam)",
                Some("wine") => " (Wine)",
                _ => "",
            };
            return Some(format!("Jogando {}{origem_str}", jogo.nome));
        }
        if let Some(musica) = &atv.musica {
            let artista = musica.artista.as_deref().unwrap_or("");
            if !artista.is_empty() {
                return Some(format!("Ouvindo {} — {}", musica.titulo, artista));
            }
            return Some(format!("Ouvindo {}", musica.titulo));
        }
    }
    if em_chamada {
        return Some("Em chamada".to_string());
    }
    None
}

/// Filtra a lista de amigos com base na sub-aba.
pub fn filtra_amigos(lista: &[CartaoPerfil], sub_aba: SubAbaAmigos) -> Vec<&CartaoPerfil> {
    match sub_aba {
        SubAbaAmigos::Disponiveis => lista
            .iter()
            .filter(|a| a.status.as_deref().unwrap_or("offline") != "offline")
            .collect(),
        SubAbaAmigos::Todos => lista.iter().collect(),
        _ => Vec::new(),
    }
}

/// Renderiza o painel completo de amigos na tela principal.
pub fn painel_amigos<'a>(
    dados: Option<&'a RespostaAmigos>,
    estado: &'a EstadoAmigos,
    carregando: bool,
) -> Element<'a, Msg> {
    let c = tema::cores();

    let total_amigos = dados.map(|d| d.amigos.len()).unwrap_or(0);
    let total_online = dados
        .map(|d| {
            d.amigos
                .iter()
                .filter(|a| a.status.as_deref().unwrap_or("offline") != "offline")
                .count()
        })
        .unwrap_or(0);
    let total_recebidos = dados.map(|d| d.recebidos.len()).unwrap_or(0);
    let total_enviados = dados.map(|d| d.enviados.len()).unwrap_or(0);
    let total_pendentes = total_recebidos + total_enviados;

    let botao_aba = |sub: SubAbaAmigos, rotulo: String, eh_destaque: bool| {
        let selecionado = estado.sub_aba == sub;
        button(text(rotulo).size(13).font(negrito()))
            .padding([6, 12])
            .style(move |t, s| {
                if eh_destaque {
                    if selecionado {
                        button::Style {
                            background: Some(iced::Background::Color(c.sucesso)),
                            text_color: c.sobre_destaque,
                            border: Border {
                                radius: 6.0.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        }
                    } else {
                        tema::botao_primario(t, s)
                    }
                } else if selecionado {
                    button::Style {
                        background: Some(iced::Background::Color(c.elevado)),
                        text_color: c.texto,
                        border: Border {
                            radius: 6.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                } else {
                    tema::botao_secundario(t, s)
                }
            })
            .on_press(Msg::TrocaSubAbaAmigos(sub))
    };

    let barra_abas = row![
        svg(icones::amigos())
            .width(18)
            .height(18)
            .style(tema::icone_cor(c.destaque)),
        text("Amigos").size(15).font(negrito()),
        container(space().width(1))
            .height(18)
            .style(move |_| container::Style {
                background: Some(iced::Background::Color(c.borda)),
                ..Default::default()
            }),
        botao_aba(
            SubAbaAmigos::Disponiveis,
            format!("Online ({total_online})"),
            false
        ),
        botao_aba(
            SubAbaAmigos::Todos,
            format!("Todos ({total_amigos})"),
            false
        ),
        botao_aba(
            SubAbaAmigos::Pendentes,
            if total_recebidos > 0 {
                format!("Pendentes ({total_recebidos})")
            } else {
                format!("Pendentes ({total_pendentes})")
            },
            false
        ),
        botao_aba(SubAbaAmigos::Adicionar, "Adicionar Amigo".to_string(), true),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    let conteudo: Element<'a, Msg> = match estado.sub_aba {
        SubAbaAmigos::Adicionar => {
            let mut col = column![
                text("ADICIONAR AMIGO").size(12).font(negrito()).style(fraco),
                text("Você pode adicionar um amigo usando o nome de usuário da conta dele no Lincast Chat.")
                    .size(13)
                    .style(fraco),
                row![
                    text_input("Digite o nome de usuário (ex: fulano)", &estado.input_usuario)
                        .on_input(Msg::InputUsuarioAmigo)
                        .on_submit_maybe((!estado.enviando_pedido && !estado.input_usuario.trim().is_empty()).then_some(Msg::EnviaPedidoAmigo))
                        .padding(10)
                        .size(14)
                        .style(tema::campo)
                        .width(Length::Fill),
                    button(text(if estado.enviando_pedido { "Enviando..." } else { "Enviar pedido de amizade" }).size(13))
                        .padding([10, 16])
                        .style(tema::botao_primario)
                        .on_press_maybe((!estado.enviando_pedido && !estado.input_usuario.trim().is_empty()).then_some(Msg::EnviaPedidoAmigo)),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            ]
            .spacing(12)
            .width(520);

            if let Some(err) = &estado.erro {
                col = col.push(
                    container(text(err).size(13).style(texto_perigo))
                        .padding([8, 12])
                        .style(move |_| container::Style {
                            background: Some(iced::Background::Color(Color::from_rgba(
                                0.9, 0.2, 0.2, 0.1,
                            ))),
                            border: Border {
                                color: c.perigo,
                                width: 1.0,
                                radius: 6.0.into(),
                            },
                            ..Default::default()
                        }),
                );
            }
            if let Some(suc) = &estado.sucesso {
                col = col.push(
                    container(text(suc).size(13).style(destaque))
                        .padding([8, 12])
                        .style(move |_| container::Style {
                            background: Some(iced::Background::Color(Color::from_rgba(
                                0.1, 0.7, 0.3, 0.1,
                            ))),
                            border: Border {
                                color: c.sucesso,
                                width: 1.0,
                                radius: 6.0.into(),
                            },
                            ..Default::default()
                        }),
                );
            }

            container(col).padding(20).into()
        }
        SubAbaAmigos::Pendentes => {
            let mut col = column![].spacing(16);

            let recebidos = dados.map(|d| d.recebidos.as_slice()).unwrap_or(&[]);
            let enviados = dados.map(|d| d.enviados.as_slice()).unwrap_or(&[]);

            col = col.push(
                text(format!("PEDIDOS RECEBIDOS — {}", recebidos.len()))
                    .size(12)
                    .font(negrito())
                    .style(fraco),
            );
            if recebidos.is_empty() {
                col = col.push(
                    container(
                        text("Nenhum pedido de amizade recebido.")
                            .size(13)
                            .style(fraco),
                    )
                    .padding([4, 8]),
                );
            } else {
                for r in recebidos {
                    col = col.push(card_pedido_recebido(r));
                }
            }

            col = col.push(space().height(10));
            col = col.push(
                text(format!("PEDIDOS ENVIADOS — {}", enviados.len()))
                    .size(12)
                    .font(negrito())
                    .style(fraco),
            );
            if enviados.is_empty() {
                col = col.push(
                    container(
                        text("Nenhum pedido de amizade enviado pendente.")
                            .size(13)
                            .style(fraco),
                    )
                    .padding([4, 8]),
                );
            } else {
                for e in enviados {
                    col = col.push(card_pedido_enviado(e));
                }
            }

            scrollable(container(col).padding(16))
                .style(tema::rolagem)
                .into()
        }
        SubAbaAmigos::Disponiveis | SubAbaAmigos::Todos => {
            if carregando && dados.is_none() {
                container(text("Carregando amigos…").size(14).style(fraco))
                    .padding(20)
                    .into()
            } else {
                let lista = dados.map(|d| d.amigos.as_slice()).unwrap_or(&[]);
                let amigos_filtrados = filtra_amigos(lista, estado.sub_aba);

                let rotulo = if estado.sub_aba == SubAbaAmigos::Disponiveis {
                    format!("ONLINE — {}", amigos_filtrados.len())
                } else {
                    format!("TODOS OS AMIGOS — {}", amigos_filtrados.len())
                };

                let mut col =
                    column![text(rotulo).size(12).font(negrito()).style(fraco)].spacing(6);

                if amigos_filtrados.is_empty() {
                    col = col.push(
                        container(
                            text(if estado.sub_aba == SubAbaAmigos::Disponiveis {
                                "Nenhum amigo online no momento."
                            } else {
                                "Você ainda não tem amigos adicionados. Clique em 'Adicionar Amigo' para começar!"
                            })
                            .size(13)
                            .style(fraco),
                        )
                        .padding([12, 8]),
                    );
                } else {
                    for amigo in amigos_filtrados {
                        col = col.push(card_amigo(
                            amigo,
                            estado.amigo_para_remover.as_deref() == Some(&amigo.uid),
                        ));
                    }
                }

                scrollable(container(col).padding(16))
                    .style(tema::rolagem)
                    .into()
            }
        }
    };

    let principal = column![
        container(barra_abas)
            .padding([12, 16])
            .style(move |_| container::Style {
                background: Some(iced::Background::Color(c.lateral)),
                border: Border {
                    color: c.borda,
                    width: 1.0,
                    ..Default::default()
                },
                ..Default::default()
            }),
        conteudo,
    ]
    .width(Length::Fill)
    .height(Length::Fill);
    let ativos: Vec<&CartaoPerfil> = dados
        .map(|d| {
            d.amigos
                .iter()
                .filter(|a| a.em_chamada || a.atividade.is_some())
                .collect()
        })
        .unwrap_or_default();
    row![
        principal,
        container(space().width(1))
            .height(Length::Fill)
            .style(move |_| container::Style {
                background: Some(iced::Background::Color(c.borda)),
                ..Default::default()
            }),
        coluna_ativo_agora(&ativos),
    ]
    .into()
}

/// A coluna "Ativo agora": amigos em chamada ou fazendo alguma coisa.
fn coluna_ativo_agora<'a>(ativos: &[&'a CartaoPerfil]) -> Element<'a, Msg> {
    let c = tema::cores();
    let mut col = Column::new().spacing(8).padding(16);
    col = col.push(
        text("ATIVO AGORA")
            .size(tema::TEXTO_ROTULO)
            .font(negrito())
            .style(fraco),
    );
    if ativos.is_empty() {
        col = col.push(
            text("Quando seus amigos estiverem em chamada ou jogando, aparecem aqui.")
                .size(13)
                .style(fraco),
        );
    }
    for a in ativos {
        let st = a.status.as_deref().unwrap_or("offline");
        let sub = formata_atividade(a.atividade.as_ref(), a.em_chamada).unwrap_or_default();
        col = col.push(
            container(
                row![
                    crate::ui::avatar::avatar(
                        a.avatar.as_deref(),
                        &a.uid,
                        &a.name,
                        36.0,
                        Some(st),
                        c.elevado,
                    ),
                    column![
                        text(&a.name).size(14).font(negrito()),
                        text(sub).size(12).style(fraco),
                    ]
                    .spacing(2),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            )
            .padding(10)
            .width(Length::Fill)
            .style(move |_| container::Style {
                background: Some(iced::Background::Color(c.elevado)),
                border: Border {
                    radius: tema::RAIO_GRANDE.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
        );
    }
    container(scrollable(col).style(tema::rolagem))
        .width(300)
        .height(Length::Fill)
        .style(tema::painel_principal)
        .into()
}

/// Renderiza o cartão de um amigo na lista de amigos.
fn card_amigo<'a>(amigo: &'a CartaoPerfil, confirmando_remocao: bool) -> Element<'a, Msg> {
    let c = tema::cores();
    let status_str = amigo.status.as_deref().unwrap_or("offline");
    let rotulo_status = crate::ui::status::rotulo_status(status_str);

    let atividade_opt = formata_atividade(amigo.atividade.as_ref(), amigo.em_chamada);

    let avatar_circulo = crate::ui::avatar::avatar(
        amigo.avatar.as_deref(),
        &amigo.uid,
        &amigo.name,
        36.0,
        Some(status_str),
        c.fundo,
    );

    let linha_nome = row![
        text(&amigo.name).size(14).font(negrito()),
        text(format!("@{}", amigo.usuario)).size(12).style(fraco),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let linha_subtexto = if let Some(atv) = atividade_opt {
        row![
            crate::ui::status::bolinha_status(status_str, 8.0),
            text(atv).size(12).style(destaque),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
    } else {
        row![
            crate::ui::status::bolinha_status(status_str, 8.0),
            text(rotulo_status).size(12).style(fraco),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
    };

    let info_col = column![linha_nome, linha_subtexto].spacing(2);

    let acoes: Element<'a, Msg> = if confirmando_remocao {
        row![
            text("Remover?").size(12).style(texto_perigo),
            button(text("Sim").size(12))
                .padding([4, 8])
                .style(tema::botao_perigo)
                .on_press(Msg::ConfirmaRemocaoAmigo(amigo.uid.clone())),
            button(text("Não").size(12))
                .padding([4, 8])
                .style(tema::botao_secundario)
                .on_press(Msg::CancelaRemocaoAmigo),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .into()
    } else {
        row![
            crate::ui::efeitos::com_dica(
                botao_icone(
                    icones::mensagem(),
                    false,
                    false,
                    Some(Msg::AbreDm {
                        uid: amigo.uid.clone(),
                        nome: amigo.name.clone(),
                        ligar: false,
                    }),
                ),
                "Enviar mensagem",
                iced::widget::tooltip::Position::Top
            ),
            crate::ui::efeitos::com_dica(
                botao_icone(
                    icones::chamada(),
                    false,
                    false,
                    Some(Msg::AbreDm {
                        uid: amigo.uid.clone(),
                        nome: amigo.name.clone(),
                        ligar: true,
                    }),
                ),
                "Iniciar chamada de voz",
                iced::widget::tooltip::Position::Top
            ),
            button(text("Perfil").size(12))
                .padding([4, 10])
                .style(tema::botao_secundario)
                .on_press(Msg::AbrePerfilUsuario(amigo.usuario.clone())),
            button(
                svg(icones::fechar())
                    .width(14)
                    .height(14)
                    .style(tema::icone_cor(c.texto_fraco))
            )
            .padding([6, 8])
            .style(tema::botao_secundario)
            .on_press(Msg::PedeConfirmacaoRemocaoAmigo(amigo.uid.clone())),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .into()
    };

    container(
        row![avatar_circulo, info_col, space().width(Length::Fill), acoes,]
            .spacing(12)
            .align_y(Alignment::Center),
    )
    .padding([8, 12])
    .style(move |_| container::Style {
        background: Some(iced::Background::Color(c.fundo)),
        border: Border {
            color: c.borda,
            width: 1.0,
            radius: 6.0.into(),
        },
        ..Default::default()
    })
    .into()
}

/// Renderiza o cartão de um pedido de amizade recebido com botões de Aceitar e Recusar.
fn card_pedido_recebido<'a>(pedido: &'a CartaoPerfil) -> Element<'a, Msg> {
    let c = tema::cores();
    let uid = pedido.uid.clone();
    let uid_recusar = pedido.uid.clone();

    container(
        row![
            column![
                text(&pedido.name).size(14).font(negrito()),
                text(format!("@{}", pedido.usuario)).size(12).style(fraco),
            ]
            .spacing(2),
            space().width(Length::Fill),
            button(text("Aceitar").size(12))
                .padding([6, 12])
                .style(tema::botao_primario)
                .on_press(Msg::RespondePedidoAmigo { uid, aceita: true }),
            button(text("Recusar").size(12))
                .padding([6, 12])
                .style(tema::botao_perigo)
                .on_press(Msg::RespondePedidoAmigo {
                    uid: uid_recusar,
                    aceita: false
                }),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding([8, 12])
    .style(move |_| container::Style {
        background: Some(iced::Background::Color(c.fundo)),
        border: Border {
            color: c.borda,
            width: 1.0,
            radius: 6.0.into(),
        },
        ..Default::default()
    })
    .into()
}

/// Renderiza o cartão de um pedido de amizade enviado pendente com botão de Cancelar.
fn card_pedido_enviado<'a>(pedido: &'a CartaoPerfil) -> Element<'a, Msg> {
    let c = tema::cores();
    let uid = pedido.uid.clone();

    container(
        row![
            column![
                text(&pedido.name).size(14).font(negrito()),
                text(format!("@{} • Pedido enviado pendente", pedido.usuario))
                    .size(12)
                    .style(fraco),
            ]
            .spacing(2),
            space().width(Length::Fill),
            button(text("Cancelar").size(12))
                .padding([6, 12])
                .style(tema::botao_secundario)
                .on_press(Msg::RemoveAmigo(uid)),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding([8, 12])
    .style(move |_| container::Style {
        background: Some(iced::Background::Color(c.fundo)),
        border: Border {
            color: c.borda,
            width: 1.0,
            radius: 6.0.into(),
        },
        ..Default::default()
    })
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::JogoAtividade;

    #[test]
    fn testa_formata_atividade_jogo() {
        let atv = Atividade {
            jogo: Some(JogoAtividade {
                nome: "Half-Life 2".to_string(),
                origem: Some("steam".to_string()),
                desde: None,
                capa: None,
            }),
            musica: None,
        };
        let texto = formata_atividade(Some(&atv), false);
        assert_eq!(texto.as_deref(), Some("Jogando Half-Life 2 (Steam)"));
    }

    #[test]
    fn testa_filtra_amigos_disponiveis() {
        let a1 = CartaoPerfil {
            uid: "local:1".into(),
            usuario: "u1".into(),
            name: "U1".into(),
            avatar: None,
            bio: None,
            cor: None,
            banner: None,
            banner_url: None,
            status: Some("online".into()),
            em_chamada: false,
            atividade: None,
            status_custom: None,
        };
        let mut a2 = a1.clone();
        a2.uid = "local:2".into();
        a2.status = Some("offline".into());

        let lista = vec![a1, a2];
        let disponiveis = filtra_amigos(&lista, SubAbaAmigos::Disponiveis);
        assert_eq!(disponiveis.len(), 1);
        assert_eq!(disponiveis[0].uid, "local:1");

        let todos = filtra_amigos(&lista, SubAbaAmigos::Todos);
        assert_eq!(todos.len(), 2);
    }
}
