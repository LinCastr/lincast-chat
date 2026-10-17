//! Módulo de perfil próprio e visualização de perfis alheios (S5).

use iced::widget::{button, column, container, image, row, space, stack, svg, text, text_input};
use iced::{Alignment, Color, ContentFit, Element, Length};

use crate::api::{CartaoPerfil, PerfilCompleto};
use crate::{Msg, destaque, fraco, icones, negrito, tema, texto_perigo};

/// Paleta de cores predefinidas para nome e perfil.
pub const CORES_PALETA: &[(&str, &str)] = &[
    ("Padrão", ""),
    ("Azul", "#3b82f6"),
    ("Verde", "#10b981"),
    ("Roxo", "#8b5cf6"),
    ("Dourado", "#f59e0b"),
    ("Vermelho", "#ef4444"),
    ("Rosa", "#ec4899"),
    ("Ciano", "#06b6d4"),
];

/// Converte string hexadecimal (#RRGGBB) em `Color`.
pub fn parse_hex_color(hex: &str) -> Option<Color> {
    let limpo = hex.trim().strip_prefix('#').unwrap_or(hex.trim());
    if limpo.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&limpo[0..2], 16).ok()?;
    let g = u8::from_str_radix(&limpo[2..4], 16).ok()?;
    let b = u8::from_str_radix(&limpo[4..6], 16).ok()?;
    Some(Color::from_rgb8(r, g, b))
}

/// Aba selecionada no gerenciamento de conta/perfil.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AbaPerfil {
    #[default]
    Perfil,
    Seguranca,
}

/// Estado do formulário de edição do próprio perfil e segurança.
#[derive(Debug, Clone, Default)]
pub struct EdicaoPerfil {
    pub aba: AbaPerfil,
    pub nome: String,
    pub bio: String,
    pub cor: Option<String>,
    pub status: String,
    /// A prévia mostra a aba "Atividade" em vez de "Mural".
    pub previa_atividade: bool,
    pub salvando: bool,
    pub erro: Option<String>,
    pub sucesso: Option<String>,
    // Troca de senha
    pub senha_atual: String,
    pub senha_nova: String,
    pub senha_nova_confirma: String,
    pub trocando_senha: bool,
    // Apagar conta
    pub confirma_apagar: bool,
    pub senha_apagar: String,
    pub apagando: bool,
    /// Listas de jogos do perfil (favorito, gosto, quero jogar) e a busca.
    pub jogos: crate::ui::jogos_perfil::EdicaoJogos,
}

/// Renderiza o cartão público de perfil de outro usuário.
/// Como o perfil aberto se relaciona comigo.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Relacao {
    Eu,
    Amigo,
    PedidoEnviado,
    PedidoRecebido,
    Nenhuma,
}

/// Estado da janela de perfil aberta (mural carregado, rascunho do recado).
#[derive(Debug, Clone, Default)]
pub struct EstadoVerPerfil {
    pub recados: Vec<crate::api::Recado>,
    pub carregando: bool,
    pub rascunho: String,
    pub erro: Option<String>,
}

fn data_curta(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|d| {
            d.with_timezone(&chrono::Local)
                .format("%d/%m/%Y")
                .to_string()
        })
        .unwrap_or_default()
}

fn aviso_so_amigos<'a>(texto: &'static str) -> Element<'a, Msg> {
    container(text(texto).size(12).style(fraco))
        .padding([10, 16])
        .into()
}

/// Cartão de seção da tela de perfil (fundo elevado, título pequeno).
fn secao_perfil<'a>(titulo: String, corpo: Element<'a, Msg>) -> Element<'a, Msg> {
    let c = tema::cores();
    container(column![text(titulo).size(12).font(negrito()).style(fraco), corpo].spacing(10))
        .padding(14)
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(c.elevado)),
            border: iced::Border {
                radius: tema::RAIO_GRANDE.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

/// Perfil de outra pessoa (ou o meu): uma tela grande só, com tudo junto —
/// à esquerda identidade, bio e amigos em comum; à direita atividade, jogos e mural.
pub fn modal_perfil_usuario<'a>(
    perfil: &'a PerfilCompleto,
    relacao: Relacao,
    ver: &'a EstadoVerPerfil,
    meu_uid: &'a str,
) -> Element<'a, Msg> {
    let cartao: &CartaoPerfil = perfil;
    let c = tema::cores();

    let cor_perfil = cartao
        .cor
        .as_deref()
        .and_then(parse_hex_color)
        .unwrap_or_else(|| tema::cor_de_avatar(&cartao.uid));
    let status_str = cartao.status.as_deref().unwrap_or("online");
    let status_rotulo = crate::ui::status::rotulo_status(status_str);
    let cor_status = crate::ui::status::cor_status(status_str, &c);

    // --- Coluna da esquerda -------------------------------------------------
    let fundo_banner: Element<'a, Msg> = match cartao
        .banner_url
        .as_deref()
        .and_then(crate::avatar::handle_exibicao)
    {
        Some(h) => image(h)
            .width(Length::Fill)
            .height(150)
            .content_fit(ContentFit::Cover)
            .into(),
        None => container(space())
            .width(Length::Fill)
            .height(150)
            .style(move |_| container::Style {
                background: Some(iced::Background::Color(cor_perfil)),
                ..Default::default()
            })
            .into(),
    };
    let avatar_grande = container(crate::ui::avatar::avatar(
        cartao.avatar.as_deref(),
        &cartao.uid,
        &cartao.name,
        96.0,
        Some(status_str),
        c.lateral,
    ))
    .padding(5)
    .style(move |_| container::Style {
        background: Some(iced::Background::Color(c.lateral)),
        border: iced::Border {
            radius: 56.0.into(),
            ..Default::default()
        },
        ..Default::default()
    });
    let topo = stack![
        column![fundo_banner, space().height(52)],
        container(avatar_grande).padding(iced::Padding {
            top: 98.0,
            left: 20.0,
            ..Default::default()
        }),
    ];

    let mut identidade = column![
        row![
            text(&cartao.name).size(24).font(negrito()),
            if cartao.em_chamada {
                container(text("Na chamada").size(11).style(destaque)).padding([2, 6])
            } else {
                container(space().width(0))
            }
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        text(format!("@{}", cartao.usuario)).size(14).style(fraco),
        row![
            container(space().width(10).height(10)).style(move |_| container::Style {
                background: Some(iced::Background::Color(cor_status)),
                border: iced::Border {
                    radius: 5.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
            text(status_rotulo).size(12).style(fraco),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    ]
    .spacing(4);

    if !perfil.insignias.is_empty() {
        let mut chips = row![].spacing(6).align_y(Alignment::Center);
        for i in &perfil.insignias {
            chips = chips.push(
                container(text(&i.nome).size(11).style(destaque))
                    .padding([2, 8])
                    .style(move |_| container::Style {
                        background: Some(iced::Background::Color(c.elevado)),
                        border: iced::Border {
                            color: c.borda,
                            width: 1.0,
                            radius: 10.0.into(),
                        },
                        ..Default::default()
                    }),
            );
        }
        identidade = identidade.push(space().height(4)).push(chips.wrap());
    }

    let botao = |rotulo: &'static str, msg: Option<Msg>| {
        button(text(rotulo).size(13))
            .padding([7, 14])
            .style(tema::botao_secundario)
            .on_press_maybe(msg)
    };
    let mensagem = Msg::AbreDm {
        uid: cartao.uid.clone(),
        nome: cartao.name.clone(),
        ligar: false,
    };
    let acoes = match relacao {
        Relacao::Eu => row![
            button(text("Editar perfil").size(13))
                .padding([7, 14])
                .style(tema::botao_primario)
                .on_press(Msg::AbreEditarPerfil)
        ],
        Relacao::Amigo => row![botao("Mensagem", Some(mensagem))],
        Relacao::PedidoEnviado => row![
            botao("Mensagem", Some(mensagem)),
            botao("Pedido enviado", None),
        ],
        Relacao::PedidoRecebido => row![
            botao("Mensagem", Some(mensagem)),
            botao("Pedido recebido: veja em Amigos", None),
        ],
        Relacao::Nenhuma => row![
            botao("Mensagem", Some(mensagem)),
            botao(
                "Adicionar aos amigos",
                Some(Msg::EnviaPedidoAmigoPorNome(cartao.usuario.clone())),
            ),
        ],
    }
    .spacing(8)
    .align_y(Alignment::Center);

    let mut esquerda = column![
        container(identidade).padding([0, 20]),
        container(acoes).padding([0, 20])
    ]
    .spacing(12);

    let bio = cartao
        .bio
        .as_deref()
        .filter(|b| !b.trim().is_empty())
        .unwrap_or("Sem descrição.");
    let mut sobre = column![
        text("Sobre mim").size(12).font(negrito()).style(fraco),
        text(bio).size(13),
    ]
    .spacing(4);
    if let Some(ms) = perfil.criada_em {
        sobre = sobre
            .push(space().height(6))
            .push(text("Membro desde").size(12).font(negrito()).style(fraco))
            .push(text(data_curta(ms)).size(13));
    }
    for i in &perfil.insignias {
        sobre = sobre.push(
            text(format!("{}: {}", i.nome, i.detalhe))
                .size(11)
                .style(fraco),
        );
    }
    esquerda = esquerda.push(container(sobre).padding([0, 20]));

    if perfil.amigo_de_mim && relacao != Relacao::Eu {
        let corpo: Element<'a, Msg> = if perfil.mutuos.is_empty() {
            text("Vocês não têm amigos em comum.")
                .size(12)
                .style(fraco)
                .into()
        } else {
            let mut col = column![].spacing(4);
            for m in &perfil.mutuos {
                col = col.push(
                    button(
                        row![
                            text(&m.name).size(13).font(negrito()),
                            text(format!("@{}", m.usuario)).size(11).style(fraco),
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center),
                    )
                    .padding([4, 8])
                    .width(Length::Fill)
                    .style(tema::botao_secundario)
                    .on_press(Msg::AbrePerfilUsuario(m.usuario.clone())),
                );
            }
            col.into()
        };
        esquerda = esquerda.push(
            container(secao_perfil(
                format!("Amigos em comum · {}", perfil.mutuos.len()),
                corpo,
            ))
            .padding([0, 20]),
        );
    }

    let esquerda = column![topo, esquerda].spacing(0).width(380);

    // --- Coluna da direita ----------------------------------------------------
    let mut direita = column![].spacing(12).width(Length::Fill);
    if let Some(bloco) = cartao
        .atividade
        .as_ref()
        .and_then(crate::ui::painel::bloco_atividade)
    {
        direita = direita.push(bloco);
    }
    if perfil.amigo_de_mim {
        direita = direita.push(secao_perfil(
            "Jogos".into(),
            crate::ui::jogos_perfil::blocos(&perfil.listas, false),
        ));
        if !perfil.jogos.is_empty() {
            let mut col = column![].spacing(6);
            for j in perfil.jogos.iter().take(30) {
                col = col.push(
                    row![
                        svg(icones::controle()).width(14).height(14),
                        text(&j.nome).size(13).width(Length::Fill),
                        text(format!(
                            "{} · {} dia{}",
                            data_curta(j.ultima),
                            j.dias,
                            if j.dias == 1 { "" } else { "s" }
                        ))
                        .size(11)
                        .style(fraco),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                );
            }
            direita = direita.push(secao_perfil(
                format!("Jogados recentemente · {}", perfil.jogos.len()),
                col.into(),
            ));
        }
        // Mural
        let sou_dono = relacao == Relacao::Eu;
        let mut col = column![].spacing(8);
        let enviar = (!ver.rascunho.trim().is_empty()).then_some(Msg::EnviaRecado);
        col = col.push(
            row![
                text_input("Deixe um recado (até 200 letras)", &ver.rascunho)
                    .on_input(Msg::RascunhoRecado)
                    .on_submit_maybe(enviar.clone())
                    .padding(8)
                    .size(13)
                    .style(tema::campo),
                button(text("Enviar").size(12))
                    .padding([8, 14])
                    .style(tema::botao_primario)
                    .on_press_maybe(enviar),
            ]
            .spacing(6),
        );
        if let Some(e) = &ver.erro {
            col = col.push(text(e).size(11).style(texto_perigo));
        }
        if ver.carregando {
            col = col.push(text("Carregando...").size(11).style(fraco));
        } else if ver.recados.is_empty() {
            col = col.push(text("Nenhum recado ainda.").size(12).style(fraco));
        }
        for r in &ver.recados {
            let pode_apagar = sou_dono || r.de == meu_uid;
            let mut linha = row![
                column![
                    text(format!("{} · {}", r.de_nome, data_curta(r.ts)))
                        .size(11)
                        .style(fraco),
                    text(&r.texto).size(13),
                ]
                .spacing(2)
                .width(Length::Fill),
            ]
            .align_y(Alignment::Center);
            if pode_apagar {
                linha = linha.push(
                    button(svg(icones::fechar()).width(12).height(12))
                        .padding([3, 5])
                        .style(tema::botao_secundario)
                        .on_press(Msg::ApagaRecado(r.id.clone())),
                );
            }
            col = col.push(linha);
        }
        direita = direita.push(secao_perfil("Mural".into(), col.into()));
    } else {
        direita = direita.push(aviso_so_amigos(
            "Jogos, mural e amigos em comum são só para amigos.",
        ));
    }

    let fechar = container(
        button(svg(icones::fechar()).width(14).height(14))
            .padding([6, 8])
            .style(tema::botao_secundario)
            .on_press(Msg::FechaModalPerfil),
    )
    .width(Length::Fill)
    .align_x(Alignment::End)
    .padding(10);

    let corpo = row![
        esquerda,
        container(
            iced::widget::scrollable(container(direita).padding([16, 20])).height(Length::Fill)
        )
        .width(Length::Fill)
        .height(Length::Fill),
    ];
    container(stack![corpo, fechar])
        .width(1000)
        .height(640)
        .clip(true)
        .style(tema::painel_lateral)
        .into()
}

/// Renderiza o modal de edição do próprio perfil e configurações de segurança da conta.
#[allow(dead_code)]
pub fn modal_editar_meu_perfil<'a>(
    estado: &'a EdicaoPerfil,
    usuario: &'a str,
    eh_local: bool,
    tem_avatar: bool,
    tem_banner: bool,
) -> Element<'a, Msg> {
    let c = tema::cores();

    let titulo_linha = row![
        svg(icones::configuracoes())
            .width(16)
            .height(16)
            .style(tema::icone_cor(c.destaque)),
        text(if estado.aba == AbaPerfil::Seguranca {
            "Configurações da Conta"
        } else {
            "Editar Perfil"
        })
        .size(16)
        .font(negrito()),
        space().width(Length::Fill),
        button(svg(icones::fechar()).width(14).height(14))
            .padding([4, 6])
            .style(tema::botao_secundario)
            .on_press(Msg::FechaModalPerfil),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let divisor = || {
        container(space().height(1))
            .width(Length::Fill)
            .style(move |_| container::Style {
                background: Some(iced::Background::Color(c.borda)),
                ..Default::default()
            })
    };

    let seletor_abas = if eh_local {
        row![
            button(text("Perfil").size(13).font(negrito()))
                .padding([6, 12])
                .style(if estado.aba == AbaPerfil::Perfil {
                    tema::botao_primario
                } else {
                    tema::botao_secundario
                })
                .on_press(Msg::TrocaAbaPerfil(AbaPerfil::Perfil)),
            button(text("Segurança e Senha").size(13).font(negrito()))
                .padding([6, 12])
                .style(if estado.aba == AbaPerfil::Seguranca {
                    tema::botao_primario
                } else {
                    tema::botao_secundario
                })
                .on_press(Msg::TrocaAbaPerfil(AbaPerfil::Seguranca)),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
    } else {
        row![]
    };

    let mut col = column![titulo_linha].spacing(12).width(420);
    if eh_local {
        col = col.push(seletor_abas);
    }

    match estado.aba {
        AbaPerfil::Perfil => {
            let mut botoes_cores = row![].spacing(6).align_y(Alignment::Center);
            for (nome_cor, hex) in CORES_PALETA {
                let selecionado = estado.cor.as_deref().unwrap_or("") == *hex;
                let cor_bg = parse_hex_color(hex).unwrap_or(c.destaque);
                let btn = button(text(*nome_cor).size(11))
                    .padding([4, 8])
                    .style(move |t, s| {
                        let mut est = tema::botao_secundario(t, s);
                        if selecionado {
                            est.background = Some(iced::Background::Color(cor_bg));
                            est.text_color = c.sobre_destaque;
                        }
                        est
                    })
                    .on_press(Msg::EscolheCorPerfil(hex.to_string()));
                botoes_cores = botoes_cores.push(btn);
            }

            let acoes_avatar = row![
                button(
                    text(if tem_avatar {
                        "Trocar foto de avatar"
                    } else {
                        "Adicionar avatar"
                    })
                    .size(12)
                )
                .padding([6, 10])
                .style(tema::botao_secundario)
                .on_press(Msg::EscolheArquivoAvatar),
                if tem_avatar {
                    button(text("Remover avatar").size(12))
                        .padding([6, 10])
                        .style(tema::botao_perigo)
                        .on_press(Msg::RemoveAvatar)
                } else {
                    button(text("").size(0))
                        .padding(0)
                        .style(|_, _| button::Style::default())
                }
            ]
            .spacing(8)
            .align_y(Alignment::Center);

            let acoes_banner = row![
                button(
                    text(if tem_banner {
                        "Trocar banner"
                    } else {
                        "Adicionar banner"
                    })
                    .size(12)
                )
                .padding([6, 10])
                .style(tema::botao_secundario)
                .on_press(Msg::EscolheArquivoBanner),
                if tem_banner {
                    button(text("Remover banner").size(12))
                        .padding([6, 10])
                        .style(tema::botao_perigo)
                        .on_press(Msg::RemoveBanner)
                } else {
                    button(text("").size(0))
                        .padding(0)
                        .style(|_, _| button::Style::default())
                }
            ]
            .spacing(8)
            .align_y(Alignment::Center);

            let rodape_acoes = row![
                space().width(Length::Fill),
                button(text("Cancelar").size(13))
                    .padding([8, 14])
                    .style(tema::botao_secundario)
                    .on_press(Msg::FechaModalPerfil),
                button(
                    text(if estado.salvando {
                        "Salvando..."
                    } else {
                        "Salvar alterações"
                    })
                    .size(13)
                )
                .padding([8, 14])
                .style(tema::botao_primario)
                .on_press_maybe(
                    (!estado.salvando && !estado.nome.trim().is_empty())
                        .then_some(Msg::SalvaMeuPerfil)
                ),
            ]
            .spacing(8)
            .align_y(Alignment::Center);

            col = col
                .push(
                    column![
                        text("Nome de exibição")
                            .size(12)
                            .font(negrito())
                            .style(fraco),
                        text_input("Seu nome de exibição", &estado.nome)
                            .on_input(Msg::RascunhoNomePerfil)
                            .padding(8)
                            .size(13)
                            .style(tema::campo),
                        text(format!("Nome de usuário: @{usuario} (imutável)"))
                            .size(11)
                            .style(fraco),
                    ]
                    .spacing(4),
                )
                .push(
                    column![
                        text("Biografia / Recado")
                            .size(12)
                            .font(negrito())
                            .style(fraco),
                        text_input("Uma frase curta sobre você (máx. 140 letras)", &estado.bio)
                            .on_input(Msg::RascunhoBioPerfil)
                            .padding(8)
                            .size(13)
                            .style(tema::campo),
                    ]
                    .spacing(4),
                )
                .push(
                    column![
                        text("Cor do Perfil e Nome")
                            .size(12)
                            .font(negrito())
                            .style(fraco),
                        botoes_cores,
                    ]
                    .spacing(4),
                )
                .push(
                    column![
                        text("Avatar").size(12).font(negrito()).style(fraco),
                        acoes_avatar,
                    ]
                    .spacing(4),
                )
                .push(
                    column![
                        text("Banner").size(12).font(negrito()).style(fraco),
                        acoes_banner,
                    ]
                    .spacing(4),
                )
                .push(space().height(6))
                .push(rodape_acoes);
        }
        AbaPerfil::Seguranca => {
            let pode_trocar_senha = !estado.trocando_senha
                && !estado.senha_atual.is_empty()
                && estado.senha_nova.len() >= 8
                && estado.senha_nova == estado.senha_nova_confirma;

            let secao_senha = column![
                text("Alterar Senha").size(14).font(negrito()),
                text("A senha atual é exigida. Ao trocar, as outras sessões da sua conta serão desconectadas.")
                    .size(12)
                    .style(fraco),
                column![
                    text("Senha atual").size(12).font(negrito()).style(fraco),
                    text_input("Sua senha atual", &estado.senha_atual)
                        .secure(true)
                        .on_input(Msg::RascunhoSenhaAtual)
                        .padding(8)
                        .size(13)
                        .style(tema::campo),
                ].spacing(4),
                column![
                    text("Nova senha").size(12).font(negrito()).style(fraco),
                    text_input("Mínimo 8 caracteres", &estado.senha_nova)
                        .secure(true)
                        .on_input(Msg::RascunhoSenhaNova)
                        .padding(8)
                        .size(13)
                        .style(tema::campo),
                ].spacing(4),
                column![
                    text("Confirmar nova senha").size(12).font(negrito()).style(fraco),
                    text_input("Repita a nova senha", &estado.senha_nova_confirma)
                        .secure(true)
                        .on_input(Msg::RascunhoSenhaNovaConfirma)
                        .padding(8)
                        .size(13)
                        .style(tema::campo),
                ].spacing(4),
                row![
                    space().width(Length::Fill),
                    button(text(if estado.trocando_senha { "Alterando..." } else { "Atualizar senha" }).size(13))
                        .padding([8, 14])
                        .style(tema::botao_primario)
                        .on_press_maybe(pode_trocar_senha.then_some(Msg::SubmeteTrocaSenha)),
                ]
                .align_y(Alignment::Center),
            ]
            .spacing(10);

            let secao_apagar = if !estado.confirma_apagar {
                column![
                    text("Zona de Perigo — Excluir Conta").size(14).font(negrito()).style(texto_perigo),
                    text("Excluir sua conta é uma ação definitiva e irreversível. Seu avatar, banner e lista de amizades serão removidos. As mensagens que você enviou nas conversas serão mantidas.")
                        .size(12)
                        .style(fraco),
                    button(text("Quero excluir minha conta").size(12))
                        .padding([8, 12])
                        .style(tema::botao_perigo)
                        .on_press(Msg::IniciaConfirmacaoApagarConta),
                ]
                .spacing(8)
            } else {
                column![
                    text("Tem certeza que deseja excluir sua conta?").size(14).font(negrito()).style(texto_perigo),
                    text("Para confirmar a exclusão permanente de todos os seus dados e sessões, digite sua senha atual abaixo:")
                        .size(12)
                        .style(fraco),
                    text_input("Digite sua senha atual", &estado.senha_apagar)
                        .secure(true)
                        .on_input(Msg::RascunhoSenhaApagarConta)
                        .padding(8)
                        .size(13)
                        .style(tema::campo),
                    row![
                        button(text("Cancelar").size(12))
                            .padding([6, 12])
                            .style(tema::botao_secundario)
                            .on_press(Msg::CancelaConfirmacaoApagarConta),
                        space().width(Length::Fill),
                        button(text(if estado.apagando { "Excluindo..." } else { "Confirmar exclusão definitiva" }).size(12))
                            .padding([6, 12])
                            .style(tema::botao_perigo)
                            .on_press_maybe((!estado.apagando && !estado.senha_apagar.is_empty()).then_some(Msg::SubmeteApagarConta)),
                    ]
                    .align_y(Alignment::Center),
                ]
                .spacing(8)
            };

            col = col
                .push(secao_senha)
                .push(space().height(4))
                .push(divisor())
                .push(space().height(4))
                .push(secao_apagar);
        }
    }

    if let Some(s) = &estado.sucesso {
        col = col.push(text(s).size(12).style(move |_| text::Style {
            color: Some(c.sucesso),
        }));
    }
    if let Some(e) = &estado.erro {
        col = col.push(text(e).size(12).style(texto_perigo));
    }

    container(col)
        .padding([16, 20])
        .style(tema::painel_lateral)
        .into()
}

/// Editor do próprio perfil (print 4): controles à esquerda e, à direita, a
/// prévia do cartão — foto e banner clicáveis (menu mudar/remover), status
/// personalizado e as abas Mural/Atividade.
pub fn editor_perfil<'a>(
    ctx: &crate::ui::configuracoes::ContextoConfiguracoes<'a>,
    ed: &'a EdicaoPerfil,
) -> Element<'a, Msg> {
    use crate::ui::efeitos::ico;
    let c = tema::cores();
    let cor_banner = tema::cor_de_avatar(ctx.uid);
    let banner_el: Element<'a, Msg> = match ctx.banner_url.and_then(crate::avatar::handle_exibicao)
    {
        Some(h) => iced::widget::image(h)
            .width(Length::Fill)
            .height(110)
            .content_fit(iced::ContentFit::Cover)
            .into(),
        None => container(space())
            .width(Length::Fill)
            .height(110)
            .style(move |_| container::Style {
                background: Some(iced::Background::Color(cor_banner)),
                ..Default::default()
            })
            .into(),
    };
    let banner = button(crate::ui::efeitos::com_lapis("ep-banner", banner_el, 0.0))
        .padding(0)
        .width(Length::Fill)
        .style(|_, _| button::Style::default())
        .on_press(Msg::AbreMenuMidia(false));
    let foto = button(
        container(crate::ui::efeitos::com_lapis(
            "ep-foto",
            crate::ui::avatar::avatar(
                ctx.avatar_url,
                ctx.uid,
                ctx.nome_atual,
                88.0,
                None,
                c.lateral,
            ),
            44.0,
        ))
        .padding(4)
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(c.lateral)),
            border: iced::Border {
                radius: 48.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }),
    )
    .padding(0)
    .style(|_, _| button::Style::default())
    .on_press(Msg::AbreMenuMidia(true));
    let bolha = button(match ctx.status_custom.filter(|s| !s.vencido()) {
        Some(sc) => row![
            text(sc.emoji.clone()).size(13),
            text(sc.texto.clone()).size(12)
        ]
        .spacing(6)
        .align_y(Alignment::Center),
        None => row![
            ico(icones::mais(), 12.0, c.texto_medio),
            text("Definir status").size(12).style(fraco),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    })
    .padding([6, 10])
    .style(move |_, st| button::Style {
        background: Some(iced::Background::Color(
            if matches!(st, button::Status::Hovered) {
                c.forte
            } else {
                c.elevado
            },
        )),
        text_color: c.texto,
        border: iced::Border {
            radius: 14.0.into(),
            ..Default::default()
        },
        ..Default::default()
    })
    .on_press(Msg::AbreStatusCustom);
    let topo = iced::widget::stack![
        column![banner, space().height(48)],
        container(foto).padding(iced::Padding {
            top: 62.0,
            left: 16.0,
            ..Default::default()
        }),
        container(bolha).padding(iced::Padding {
            top: 124.0,
            left: 112.0,
            ..Default::default()
        }),
    ];
    let aba = |rotulo: &'static str, ativa: bool, atividade: bool| {
        button(text(rotulo).size(13).font(negrito()))
            .padding([6, 10])
            .style(move |_, st| button::Style {
                background: None,
                text_color: if ativa || matches!(st, button::Status::Hovered) {
                    c.texto
                } else {
                    c.texto_fraco
                },
                border: iced::Border {
                    color: if ativa {
                        c.destaque
                    } else {
                        iced::Color::TRANSPARENT
                    },
                    width: 0.0,
                    radius: tema::RAIO.into(),
                },
                ..Default::default()
            })
            .on_press(Msg::TrocaPreviaPerfil(atividade))
    };
    let conteudo_aba: Element<'a, Msg> = if ed.previa_atividade {
        match ctx.atividade.as_ref() {
            Some(a) => {
                text(crate::ui::amigos::formata_atividade(Some(a), false).unwrap_or_default())
                    .size(14)
                    .into()
            }
            None => text("Nada rolando agora. Abra um jogo ou toque uma música.")
                .size(13)
                .style(fraco)
                .into(),
        }
    } else {
        column![
            text("SOBRE MIM")
                .size(tema::TEXTO_ROTULO)
                .font(negrito())
                .style(fraco),
            text_input("Uma frase sobre você…", &ed.bio)
                .on_input(Msg::RascunhoBioPerfil)
                .padding([6, 10])
                .size(13)
                .style(tema::campo),
        ]
        .spacing(6)
        .into()
    };
    let cartao = container(
        column![
            topo,
            container(
                column![
                    text(ed.nome.clone()).size(22).font(negrito()),
                    text(format!("@{}", ctx.usuario)).size(13).style(fraco),
                    row![
                        aba("Mural", !ed.previa_atividade, false),
                        aba("Atividade", ed.previa_atividade, true)
                    ]
                    .spacing(4),
                    conteudo_aba,
                ]
                .spacing(8)
            )
            .padding([0, 16]),
            space().height(16),
        ]
        .width(360),
    )
    .clip(true)
    .style(move |_| container::Style {
        background: Some(iced::Background::Color(c.elevado)),
        border: iced::Border {
            color: c.borda,
            width: 1.0,
            radius: tema::RAIO_GRANDE.into(),
        },
        ..Default::default()
    });

    let mut paleta = row![].spacing(6).align_y(Alignment::Center);
    for &(_nome, hex) in CORES_PALETA {
        let selecionada = ed.cor.as_deref().unwrap_or("") == hex;
        let cor_obj = parse_hex_color(hex).unwrap_or(c.texto);
        paleta = paleta.push(
            button(
                container(space().width(18).height(18)).style(move |_| container::Style {
                    background: Some(iced::Background::Color(cor_obj)),
                    border: iced::Border {
                        color: if selecionada {
                            c.destaque
                        } else {
                            iced::Color::TRANSPARENT
                        },
                        width: if selecionada { 2.0 } else { 0.0 },
                        radius: 9.0.into(),
                    },
                    ..Default::default()
                }),
            )
            .padding(2)
            .style(|_, _| button::Style::default())
            .on_press(Msg::EscolheCorPerfil(hex.to_string())),
        );
    }
    let mut controles = column![
        text("Conectado como @".to_string() + ctx.usuario)
            .size(12)
            .style(fraco),
        text("NOME DE EXIBIÇÃO")
            .size(tema::TEXTO_ROTULO)
            .font(negrito())
            .style(fraco),
        text_input("Seu nome visível na chamada", &ed.nome)
            .on_input(Msg::RascunhoNomePerfil)
            .padding([6, 10])
            .size(13)
            .style(tema::campo),
        text("COR DE DESTAQUE DO NOME")
            .size(tema::TEXTO_ROTULO)
            .font(negrito())
            .style(fraco),
        paleta,
        text("Clique na foto ou no banner da prévia para trocar ou remover.")
            .size(12)
            .style(fraco),
        button(
            text(if ed.salvando {
                "Salvando…"
            } else {
                "Salvar alterações"
            })
            .size(13)
        )
        .padding([8, 16])
        .style(tema::botao_primario)
        .on_press_maybe(
            (!ed.salvando && !ed.nome.trim().is_empty()).then_some(Msg::SalvaMeuPerfil)
        ),
    ]
    .spacing(10)
    .width(300);
    if let Some(s) = &ed.sucesso {
        controles = controles.push(text(s.clone()).size(12).style(crate::destaque));
    }
    if let Some(e) = &ed.erro {
        controles = controles.push(text(e.clone()).size(12).style(crate::texto_perigo));
    }
    row![controles, cartao].spacing(32).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn testa_parse_hex_color() {
        assert_eq!(
            parse_hex_color("#ff0000"),
            Some(Color::from_rgb8(255, 0, 0))
        );
        assert_eq!(parse_hex_color("00ff00"), Some(Color::from_rgb8(0, 255, 0)));
        assert_eq!(parse_hex_color("invalido"), None);
    }
}
