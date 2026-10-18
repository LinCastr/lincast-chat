//! Palco da chamada (C8): no topo da conversa, quem está dentro (ou sendo
//! chamado) e a barra de controles.

use iced::widget::{Column, button, column, container, row, scrollable, space, stack, svg, text};
use iced::{Alignment, Background, Border, Color, Element, Length};

use crate::{Msg, fraco, icones, negrito, tema, ui};

pub struct PessoaNoPalco<'a> {
    pub uid: &'a str,
    pub nome: &'a str,
    pub usuario: Option<&'a str>,
    pub avatar: Option<&'a str>,
    pub falando: bool,
    pub mudo: bool,
    /// Está tocando: ainda não entrou.
    pub chamando: bool,
}

pub struct BarraDoPalco {
    pub mudo: bool,
    pub mudo_forcado: bool,
    pub camera: bool,
    pub apresentando: bool,
    pub menu_dispositivos_aberto: bool,
    pub menu_atividades_aberto: bool,
    pub menu_mais_aberto: bool,
    pub sons_abertos: bool,
}

const TAMANHO: f32 = 80.0;

/// Quem ainda está tocando: o avatar inteiro apagado (véu da cor do palco).
/// Antes só a letra ficava transparente e o círculo seguia com cor cheia.
fn apagado<'a>(foto: Element<'a, Msg>, lado: f32, fundo: Color) -> Element<'a, Msg> {
    let veu = container(space())
        .width(lado)
        .height(lado)
        .style(move |_| container::Style {
            background: Some(Background::Color(Color { a: 0.6, ..fundo })),
            border: Border {
                radius: (lado / 2.0).into(),
                ..Default::default()
            },
            ..Default::default()
        });
    iced::widget::stack![foto, veu].into()
}

fn tile<'a>(p: &PessoaNoPalco<'a>, pulso: bool, fundo: Color) -> Element<'a, Msg> {
    let c = tema::cores();
    let anel_cor = if p.falando {
        c.sucesso_forte
    } else if p.chamando {
        // Pulsa entre dois tons de cinza (o iced não anima; o cronômetro da
        // chamada redesenha a cada segundo).
        if pulso { c.texto_fraco } else { c.borda }
    } else {
        Color::TRANSPARENT
    };
    let foto = ui::avatar::avatar(p.avatar, p.uid, p.nome, TAMANHO, None, fundo);
    let foto = if p.chamando {
        apagado(foto, TAMANHO, fundo)
    } else {
        foto
    };
    let com_anel = container(foto).padding(4).style(move |_| container::Style {
        border: Border {
            color: anel_cor,
            width: 3.0,
            radius: ((TAMANHO + 8.0) / 2.0).into(),
        },
        ..Default::default()
    });
    let mut camadas: Vec<Element<'a, Msg>> = vec![com_anel.into()];
    if p.mudo && !p.chamando {
        camadas.push(
            container(
                container(
                    svg(icones::mic_off())
                        .width(14)
                        .height(14)
                        .style(tema::icone_cor(c.texto)),
                )
                .padding(5)
                .style(move |_| container::Style {
                    background: Some(Background::Color(c.forte)),
                    border: Border {
                        color: fundo,
                        width: 2.0,
                        radius: 14.0.into(),
                    },
                    ..Default::default()
                }),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::End)
            .align_y(Alignment::End)
            .into(),
        );
    }
    let nome = text(p.nome.to_string())
        .size(12)
        .style(fraco)
        .wrapping(iced::widget::text::Wrapping::None);
    let bloco = column![
        stack(camadas).width(TAMANHO + 8.0).height(TAMANHO + 8.0),
        nome
    ]
    .spacing(4)
    .align_x(Alignment::Center)
    .width(TAMANHO + 24.0);

    iced::widget::mouse_area(bloco)
        .interaction(iced::mouse::Interaction::Pointer)
        .on_press(Msg::AbrePrevia {
            uid: Some(p.uid.to_string()),
            nome: p.nome.to_string(),
            app: false,
        })
        .on_right_press(Msg::AbreMenuPessoa {
            uid: p.uid.to_string(),
            usuario: p.usuario.map(str::to_string),
            nome: p.nome.to_string(),
        })
        .into()
}

/// Avatar pequeno da coluna da direita quando uma transmissão está em foco.
fn tile_mini<'a>(p: &PessoaNoPalco<'a>, fundo: Color) -> Element<'a, Msg> {
    let c = tema::cores();
    let anel = if p.falando {
        c.sucesso_forte
    } else {
        Color::TRANSPARENT
    };
    let foto = ui::avatar::avatar(p.avatar, p.uid, p.nome, 40.0, None, fundo);
    let foto = if p.chamando {
        apagado(foto, 40.0, fundo)
    } else {
        foto
    };
    let bloco = container(foto).padding(2).style(move |_| container::Style {
        border: Border {
            color: anel,
            width: 2.0,
            radius: 24.0.into(),
        },
        ..Default::default()
    });

    iced::widget::mouse_area(bloco)
        .interaction(iced::mouse::Interaction::Pointer)
        .on_press(Msg::AbrePrevia {
            uid: Some(p.uid.to_string()),
            nome: p.nome.to_string(),
            app: false,
        })
        .on_right_press(Msg::AbreMenuPessoa {
            uid: p.uid.to_string(),
            usuario: p.usuario.map(str::to_string),
            nome: p.nome.to_string(),
        })
        .into()
}

fn botao_da_barra<'a>(
    icone: iced::widget::svg::Handle,
    titulo: &'static str,
    ligado: bool,
    perigo: bool,
    msg: Option<Msg>,
) -> Element<'a, Msg> {
    let efeito = match titulo {
        "Microfone" | "Atividades" => crate::ui::efeitos::Efeito::Balanco,
        _ => crate::ui::efeitos::Efeito::Nenhum,
    };
    let botao = crate::ui::efeitos::botao_icone_anim(titulo, icone, ligado, perigo, msg, efeito);
    // Com o popup aberto (botão "ligado"), a dica sairia em cima do popup.
    if ligado
        && matches!(
            titulo,
            "Atividades" | "Sons" | "Mais" | "Dispositivos de voz"
        )
    {
        return botao;
    }
    crate::ui::efeitos::com_dica(botao, titulo, iced::widget::tooltip::Position::Top)
}

fn barra<'a>(b: &BarraDoPalco) -> Element<'a, Msg> {
    let c = tema::cores();
    let grupo = |itens: Vec<Element<'a, Msg>>| -> Element<'a, Msg> {
        let mut r = row![].spacing(2).align_y(Alignment::Center);
        for i in itens {
            r = r.push(i);
        }
        container(r)
            .padding(4)
            .style(move |_| container::Style {
                background: Some(Background::Color(c.lateral)),
                border: Border {
                    radius: tema::RAIO_GRANDE.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
    };
    let mic = grupo(vec![
        botao_da_barra(
            if b.mudo {
                icones::mic_off()
            } else {
                icones::mic()
            },
            "Microfone",
            b.mudo,
            false,
            (!b.mudo_forcado).then_some(Msg::AlternaMudo),
        ),
        botao_da_barra(
            icones::seta_baixo(),
            "Dispositivos de voz",
            b.menu_dispositivos_aberto,
            false,
            Some(if b.menu_dispositivos_aberto {
                Msg::FechaConfigAudio
            } else {
                Msg::AbreConfigAudio
            }),
        ),
    ]);
    let midia = grupo(vec![
        botao_da_barra(
            if b.camera {
                icones::camera()
            } else {
                icones::camera_off()
            },
            "Câmera",
            b.camera,
            false,
            Some(if b.camera {
                Msg::ParaCamera
            } else {
                Msg::PedeCamera
            }),
        ),
        botao_da_barra(
            icones::tela(),
            "Compartilhar tela",
            b.apresentando,
            false,
            Some(if b.apresentando {
                Msg::ParaTela
            } else {
                Msg::PedeTela
            }),
        ),
    ]);
    let extras = grupo(vec![
        botao_da_barra(
            icones::grade(),
            "Atividades",
            b.menu_atividades_aberto,
            false,
            Some(Msg::AlternaMenuAtividades),
        ),
        botao_da_barra(
            icones::musica(),
            "Sons",
            b.sons_abertos,
            false,
            Some(Msg::AlternaPainelSoundboard),
        ),
        botao_da_barra(
            icones::reticencias(),
            "Mais",
            b.menu_mais_aberto,
            false,
            Some(Msg::AlternaMenuMaisPalco),
        ),
    ]);
    let desligar = button(
        svg(icones::desligar())
            .width(22)
            .height(22)
            .style(tema::icone_cor(Color::WHITE)),
    )
    .padding([10, 18])
    .style(move |_, st| button::Style {
        background: Some(Background::Color(
            if matches!(st, button::Status::Hovered) {
                Color {
                    a: 0.85,
                    ..c.perigo
                }
            } else {
                c.perigo
            },
        )),
        text_color: Color::WHITE,
        border: Border {
            radius: tema::RAIO_GRANDE.into(),
            ..Default::default()
        },
        ..Default::default()
    })
    .on_press(Msg::SaiDaChamada);
    container(
        row![mic, midia, extras, desligar]
            .spacing(8)
            .align_y(Alignment::Center)
            .wrap(),
    )
    .center_x(Length::Fill)
    .into()
}

/// Cartão flutuante de menu (lista vertical de ações), como o do Discord.
fn cartao_de_menu<'a>(itens: Vec<Element<'a, Msg>>) -> Element<'a, Msg> {
    let c = tema::cores();
    container(ui::menu::coluna(itens).spacing(2))
        .padding(6)
        .width(250)
        .style(move |_| container::Style {
            background: Some(Background::Color(c.lateral)),
            border: Border {
                color: c.borda,
                width: 1.0,
                radius: tema::RAIO_GRANDE.into(),
            },
            shadow: iced::Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.4),
                offset: iced::Vector::new(0.0, 4.0),
                blur_radius: 14.0,
            },
            ..Default::default()
        })
        .into()
}

/// Menu "Atividades" (popup acima do botão, ver `SalaAberta::camada_painel`).
pub fn menu_atividades<'a>(eh_grupo: bool) -> Element<'a, Msg> {
    let mut itens = vec![
        ui::menu::item(icones::lapis(), "Quadro branco", Msg::AlternarQuadro, false),
        ui::menu::item(icones::roda(), "Roleta", Msg::AlternarRoletas, false),
        ui::menu::item(
            icones::grafico(),
            "Enquete",
            Msg::AlternarCriadorEnquete,
            false,
        ),
    ];
    if eh_grupo {
        itens.push(ui::menu::item(
            icones::grade_3x3(),
            "Wordle",
            Msg::Wordle(crate::ui::wordle::WordleMsg::Inicia),
            false,
        ));
    }
    cartao_de_menu(itens)
}

pub fn menu_mais_palco<'a>() -> Element<'a, Msg> {
    cartao_de_menu(vec![
        ui::menu::item(
            icones::smile(),
            "Reações na tela",
            Msg::AlternaMenuReacoesFlutuantes,
            false,
        ),
        ui::menu::item(icones::fone(), "Sussurrar", Msg::AlternaMenuSussurro, false),
        ui::menu::item(
            icones::sinal(),
            "Diagnóstico de rede",
            Msg::TogglePainelDiagnostico,
            false,
        ),
        ui::menu::item(
            icones::configuracoes(),
            "Configurações de voz",
            Msg::AbreConfiguracoes(Some(ui::configuracoes::AbaConfig::VozVideo)),
            false,
        ),
    ])
}

pub fn palco<'a>(
    pessoas: &[PessoaNoPalco<'a>],
    na_chamada: bool,
    b: &BarraDoPalco,
    pulso: bool,
    transmissoes: Vec<Element<'a, Msg>>,
    foco: Option<Element<'a, Msg>>,
) -> Element<'a, Msg> {
    let c = tema::cores();
    let em_foco = foco.is_some();
    let mut col: Column<'a, Msg> = Column::new().spacing(18).align_x(Alignment::Center);
    if let Some(grande) = foco {
        // Transmissão em foco: ela ocupa o palco e as pessoas viram uma coluna
        // de avatares pequenos à direita.
        let lateral = Column::with_children(pessoas.iter().map(|p| tile_mini(p, c.palco)))
            .spacing(8)
            .align_x(Alignment::Center);
        col = col.push(
            row![
                container(grande).width(Length::Fill).height(Length::Fill),
                scrollable(lateral),
            ]
            .spacing(12)
            .height(Length::Fill),
        );
    } else {
        // Quebra de linha quando a janela é estreita: ninguém fica esmagado.
        // As transmissões entram como retângulos ao lado dos avatares.
        let tiles = row(pessoas
            .iter()
            .map(|p| tile(p, pulso, c.palco))
            .chain(transmissoes))
        .spacing(12)
        .align_y(Alignment::Start)
        .wrap();
        col = col.push(container(tiles).center_x(Length::Fill));
    }
    if na_chamada {
        col = col.push(barra(b));
    } else {
        let entrar = button(
            svg(icones::chamada())
                .width(24)
                .height(24)
                .style(tema::icone_cor(Color::WHITE)),
        )
        .padding(16)
        .style(move |_, st| button::Style {
            background: Some(Background::Color(
                if matches!(st, button::Status::Hovered) {
                    c.sucesso_forte
                } else {
                    c.sucesso
                },
            )),
            text_color: Color::WHITE,
            border: Border {
                radius: 28.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .on_press(Msg::LigarConversa);
        col = col.push(container(entrar).center_x(Length::Fill));
        col = col.push(
            text("Chamada em andamento. Toque para entrar.")
                .size(12)
                .style(fraco),
        );
    }
    let _ = (space(), negrito());
    container(col)
        .padding([24, 20])
        .width(Length::Fill)
        .height(if em_foco {
            Length::FillPortion(3)
        } else {
            Length::Shrink
        })
        .style(move |_| container::Style {
            background: Some(Background::Color(c.palco)),
            ..Default::default()
        })
        .into()
}
