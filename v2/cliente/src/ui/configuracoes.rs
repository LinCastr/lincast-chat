//! Central unificada de configurações em abas (U2).
//!
//! Abas: Áudio, Vídeo, Atalhos (PTT), Aparência (Temas/Wallust), Conta (Perfil/Segurança).
//! Direção visual: sóbrio, denso, hierarquia pelo texto (AGENTS §9).

use iced::alignment::Alignment;
use iced::widget::{
    Column, button, checkbox, column, container, pick_list, row, scrollable, slider, space, svg,
    text, text_input,
};
use iced::{Color, Element, Length};

use crate::captura_camera::DispositivoCamera;
use crate::captura_tela::PerfilTela;
use crate::icones;
use crate::ptt::PttConfig;
use crate::tema::{self, InfoTema};
use crate::ui::perfil::{self, AbaPerfil, EdicaoPerfil};
use crate::ui::tema::cartao_tema;
use crate::{Msg, PainelAudio, destaque, fraco, negrito, rotulo, texto_perigo};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AbaConfig {
    Conta,
    Perfil,
    #[default]
    VozVideo,
    Atalhos,
    Aparencia,
    Notificacoes,
    Sistema,
    JogosRegistrados,
}

impl AbaConfig {
    pub const TODAS: [Self; 7] = [
        Self::Conta,
        Self::VozVideo,
        Self::Atalhos,
        Self::Aparencia,
        Self::Notificacoes,
        Self::Sistema,
        Self::JogosRegistrados,
    ];

    pub fn rotulo(self) -> &'static str {
        match self {
            Self::Conta => "Minha conta",
            Self::Perfil => "Perfil",
            Self::VozVideo => "Voz e vídeo",
            Self::Atalhos => "Atalhos",
            Self::Aparencia => "Aparência",
            Self::Notificacoes => "Notificações",
            Self::Sistema => "Sistema",
            Self::JogosRegistrados => "Jogos registrados",
        }
    }

    pub fn icone(self) -> iced::widget::svg::Handle {
        match self {
            Self::Conta => icones::amigos(),
            Self::Perfil => icones::lapis(),
            Self::VozVideo => icones::fone(),
            Self::Atalhos => icones::configuracoes(),
            Self::Aparencia => icones::paleta(),
            Self::Notificacoes => icones::sino(),
            Self::Sistema => icones::ajustes(),
            Self::JogosRegistrados => icones::controle(),
        }
    }
}

pub struct ContextoConfiguracoes<'a> {
    /// Mostra só o editor de perfil, numa janela própria (sem a coluna de abas).
    pub so_perfil: bool,
    pub aba_ativa: AbaConfig,
    pub prefs: &'a crate::prefs_conversas::PrefsConversas,
    pub atalhos: &'a crate::atalhos::Atalhos,
    pub capturando_atalho: Option<&'a str>,
    pub aviso_atalho: Option<&'a str>,
    // Áudio
    pub painel_audio: &'a PainelAudio,
    pub soundboard_mudo: bool,
    // Vídeo
    pub cameras: &'a [DispositivoCamera],
    pub camera_selecionada: Option<usize>,
    pub espelhar_camera: bool,
    pub perfil_tela: PerfilTela,
    // Atalhos
    pub ptt: &'a PttConfig,
    pub capturando_ptt: bool,
    pub portal_global_ativo: bool,
    pub minimizar_para_bandeja: bool,
    // Aparência
    pub tema_atual: String,
    pub temas: Vec<InfoTema>,
    pub densidade: crate::ui::layout::Densidade,
    // Conta
    pub edicao_perfil: Option<&'a EdicaoPerfil>,
    pub usuario: &'a str,
    pub avatar_url: Option<&'a str>,
    pub banner_url: Option<&'a str>,
    pub uid: &'a str,
    pub nome_atual: &'a str,
    pub janela: iced::Size,
    pub volume_mic: Option<f32>,
    pub volume_saida: Option<f32>,
    pub status_custom: Option<&'a crate::api::StatusCustom>,
    pub atividade: Option<crate::api::Atividade>,
    // Jogos registrados
    pub jogos_reg: &'a [crate::jogos_registrados::JogoRegistrado],
    pub jogos_reg_filtro: &'a str,
    pub processos_abertos: &'a [String],
    pub jogos_reg_renomeando: Option<(&'a str, &'a str)>,
    pub jogos_reg_seletor_aberto: bool,
    // Sistema → atualização
    pub atualizacao_status: Option<&'a str>,
    pub atualizando: bool,
}

fn divisoria<'a>() -> Element<'a, Msg> {
    container(space().height(1)).style(tema::divisoria).into()
}

fn janela_editar_perfil<'a>(ctx: ContextoConfiguracoes<'a>) -> Element<'a, Msg> {
    let c = tema::cores();
    let Some(ed) = ctx.edicao_perfil else {
        return text("Não foi possível carregar os dados da conta.")
            .size(13)
            .style(fraco)
            .into();
    };
    let cabecalho = row![
        text("Editar perfil").size(18).font(negrito()),
        space().width(Length::Fill),
        button(
            svg(icones::fechar())
                .width(16)
                .height(16)
                .style(tema::icone_cor(c.texto_fraco))
        )
        .padding(4)
        .style(tema::botao_secundario)
        .on_press(Msg::FechaConfiguracoes),
    ]
    .align_y(Alignment::Center);
    let largura = (ctx.janela.width - 80.0).clamp(560.0, 820.0);
    container(
        iced::widget::scrollable(
            column![
                cabecalho,
                divisoria(),
                perfil::editor_perfil(&ctx, ed),
                divisoria(),
                crate::ui::jogos_perfil::editor(&ed.jogos),
            ]
            .spacing(14)
            .padding(22),
        )
        .height(Length::Fixed(
            (ctx.janela.height - 90.0).clamp(300.0, 760.0),
        ))
        .style(tema::rolagem),
    )
    .width(largura)
    .style(move |_| container::Style {
        background: Some(iced::Background::Color(c.fundo)),
        text_color: Some(c.texto),
        border: iced::Border {
            color: c.borda,
            width: 1.0,
            radius: (tema::RAIO_GRANDE + 4.0).into(),
        },
        ..Default::default()
    })
    .into()
}

pub fn modal_configuracoes<'a>(ctx: ContextoConfiguracoes<'a>) -> Element<'a, Msg> {
    if ctx.so_perfil {
        return janela_editar_perfil(ctx);
    }
    let c = tema::cores();

    // 1. Barra lateral de abas (esquerda)
    let mut menu_lateral = Column::new().spacing(4).padding(14).width(260);
    // Cabeçalho do usuário (print 12): foto, nome e atalho para editar o perfil.
    menu_lateral = menu_lateral.push(crate::ui::efeitos::com_hover(
        "cfg-cabecalho",
        button(
            row![
                crate::ui::avatar::avatar(
                    ctx.avatar_url,
                    ctx.uid,
                    ctx.nome_atual,
                    44.0,
                    None,
                    c.lateral
                ),
                column![
                    text(ctx.nome_atual).size(15).font(negrito()),
                    // Com o mouse em cima, o @ dá lugar ao atalho (print 12).
                    if crate::anim::dentro("cfg-cabecalho") {
                        Element::from(
                            row![
                                crate::ui::efeitos::ico(icones::lapis(), 12.0, c.texto),
                                text("Editar perfil").size(12),
                            ]
                            .spacing(5)
                            .align_y(Alignment::Center),
                        )
                    } else {
                        text(format!("@{}", ctx.usuario))
                            .size(12)
                            .style(fraco)
                            .into()
                    },
                ]
                .spacing(1),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding([6, 8])
        .style(tema::item_de_lista_hover)
        .on_press(Msg::AbreEditarPerfil),
    ));
    menu_lateral = menu_lateral.push(space().height(10));

    for aba in AbaConfig::TODAS {
        let ativa = ctx.aba_ativa == aba;
        let botao_aba = button(
            row![
                svg(aba.icone())
                    .width(15)
                    .height(15)
                    .style(tema::icone_cor(if ativa {
                        c.sobre_destaque
                    } else {
                        c.texto_fraco
                    })),
                text(aba.rotulo()).size(13).font(if ativa {
                    negrito()
                } else {
                    crate::tema::FONTE
                }),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding([7, 10])
        .style(move |t, s| {
            if ativa {
                tema::botao_primario(t, s)
            } else {
                let mut b = tema::botao_secundario(t, s);
                b.border = iced::Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: iced::border::Radius::from(6.0),
                };
                b
            }
        })
        .on_press(Msg::TrocaAbaConfig(aba));

        menu_lateral = menu_lateral.push(botao_aba);
    }

    menu_lateral = menu_lateral.push(space().height(Length::Fill));
    menu_lateral = menu_lateral.push(
        button(
            row![
                svg(icones::sair())
                    .width(15)
                    .height(15)
                    .style(tema::icone_cor(c.perigo)),
                text("Sair").size(13).color(c.perigo),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding([7, 10])
        .style(tema::botao_secundario)
        .on_press(Msg::Deslogar),
    );
    menu_lateral = menu_lateral.push(
        button(
            row![
                svg(icones::voltar()).width(14).height(14),
                text("Fechar (Esc)").size(13),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding([6, 10])
        .style(tema::botao_secundario)
        .on_press(Msg::FechaConfiguracoes),
    );

    let painel_lateral = container(menu_lateral)
        .height(Length::Fill)
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(c.lateral)),
            text_color: Some(c.texto),
            border: iced::Border {
                radius: iced::border::Radius {
                    top_left: tema::RAIO_GRANDE,
                    bottom_left: tema::RAIO_GRANDE,
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        });

    // 2. Área de conteúdo da aba ativa (direita)
    let conteudo_aba: Element<'a, Msg> = match ctx.aba_ativa {
        AbaConfig::VozVideo => column![
            renderiza_aba_audio(&ctx),
            divisoria(),
            renderiza_aba_video(&ctx)
        ]
        .spacing(14)
        .into(),
        AbaConfig::Atalhos => renderiza_aba_atalhos(&ctx),
        AbaConfig::Aparencia => renderiza_aba_aparencia(&ctx),
        AbaConfig::Conta => renderiza_aba_conta(&ctx, AbaPerfil::Seguranca),
        AbaConfig::Perfil => renderiza_aba_conta(&ctx, AbaPerfil::Perfil),
        AbaConfig::Notificacoes => renderiza_aba_notificacoes(&ctx),
        AbaConfig::Sistema => renderiza_aba_sistema(&ctx),
        AbaConfig::JogosRegistrados => renderiza_aba_jogos(&ctx),
    };

    let cabecalho_conteudo = row![
        column![
            text(ctx.aba_ativa.rotulo()).size(18).font(negrito()),
            text(descricao_aba(ctx.aba_ativa)).size(12).style(fraco),
        ]
        .spacing(2)
        .width(Length::Fill),
        button(
            svg(icones::fechar())
                .width(16)
                .height(16)
                .style(tema::icone_cor(c.texto_fraco))
        )
        .padding(4)
        .style(tema::botao_secundario)
        .on_press(Msg::FechaConfiguracoes),
    ]
    .align_y(Alignment::Center);

    let painel_conteudo = container(
        column![
            cabecalho_conteudo,
            divisoria(),
            scrollable(container(conteudo_aba).max_width(640))
                .id(ID_ROLAGEM)
                .height(Length::Fill)
                .style(tema::rolagem),
        ]
        .spacing(12)
        .padding([14, 18]),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(move |_| container::Style {
        background: Some(iced::Background::Color(c.fundo)),
        text_color: Some(c.texto),
        border: iced::Border {
            radius: iced::border::Radius {
                top_right: tema::RAIO_GRANDE,
                bottom_right: tema::RAIO_GRANDE,
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    });

    // Overlay grande no meio da janela (o app continua visível atrás); Esc fecha.
    let largura = (ctx.janela.width - 80.0).clamp(640.0, 1120.0);
    let altura = (ctx.janela.height - 80.0).clamp(420.0, 760.0);
    container(row![
        painel_lateral,
        crate::linha_vertical(),
        painel_conteudo
    ])
    .width(largura)
    .height(altura)
    .clip(true)
    .style(move |_| container::Style {
        background: Some(iced::Background::Color(c.fundo)),
        border: iced::Border {
            color: c.borda,
            width: 1.0,
            radius: tema::RAIO_GRANDE.into(),
        },
        ..Default::default()
    })
    .into()
}

fn descricao_aba(aba: AbaConfig) -> &'static str {
    match aba {
        AbaConfig::Conta => "Senha, segurança e encerramento da conta.",
        AbaConfig::Perfil => "Nome, biografia, cores, avatar e banner que os outros veem.",
        AbaConfig::VozVideo => "Microfone, saída de áudio, câmera e qualidade da tela.",
        AbaConfig::Atalhos => "Teclas de atalho, inclusive com o app fora de foco.",
        AbaConfig::Aparencia => "Temas, paletas de cores e Wallust do sistema.",
        AbaConfig::Notificacoes => "Quando o app avisa de mensagens, menções e chamadas.",
        AbaConfig::Sistema => "Início com o sistema, bandeja e informações do app.",
        AbaConfig::JogosRegistrados => {
            "Jogos que você já jogou e programas abertos que contam como jogo."
        }
    }
}

/// Rolagem do conteúdo da aba: volta ao topo ao trocar de aba.
pub const ID_ROLAGEM: &str = "config-rolagem";

// --- Conteúdo da Aba Áudio ---

/// Um dispositivo na lista suspensa.
#[derive(Debug, Clone, PartialEq, Eq)]
struct OpcaoDisp {
    guid: String,
    nome: String,
}

impl std::fmt::Display for OpcaoDisp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.nome)
    }
}

fn opcoes(lista: &[crate::dispositivos::DispositivoAudio]) -> Vec<OpcaoDisp> {
    lista
        .iter()
        .map(|d| OpcaoDisp {
            guid: d.guid.clone(),
            nome: nome_de_dispositivo(&d.nome),
        })
        .collect()
}

/// "default: USB PnP Audio Device Mono" → "USB PnP Audio Device Mono (padrão)".
/// O libwebrtc cola o prefixo cru no dispositivo padrão do sistema.
pub(crate) fn nome_de_dispositivo(nome: &str) -> String {
    match nome.strip_prefix("default:") {
        Some(resto) if !resto.trim().is_empty() => format!("{} (padrão)", resto.trim()),
        _ => nome.to_string(),
    }
}

/// Linha "texto ........ [interruptor]" das configurações.
fn linha_interruptor<'a>(rotulo_txt: &'a str, ativo: bool, msg: Msg) -> Element<'a, Msg> {
    row![
        text(rotulo_txt).size(13).width(Length::Fill),
        interruptor(ativo, msg)
    ]
    .spacing(12)
    .align_y(Alignment::Center)
    .into()
}

fn titulo_secao<'a>(t: &'a str) -> Element<'a, Msg> {
    text(t).size(20).font(negrito()).into()
}

fn campo_rotulado<'a>(rotulo_txt: &'a str, corpo: Element<'a, Msg>) -> Column<'a, Msg> {
    column![text(rotulo_txt).size(13).style(fraco), corpo].spacing(6)
}

/// Slider 0–100% do volume do sistema (aplica ao soltar).
pub(crate) fn slider_volume<'a>(mic: bool, valor: Option<f32>) -> Element<'a, Msg> {
    let c = tema::cores();
    match valor {
        Some(v) => row![
            slider(0.0..=1.0, v, move |x| Msg::VolumeSistema(mic, x))
                .step(0.01_f32)
                .on_release(Msg::AplicaVolumeSistema(mic))
                .style(tema::estilo_slider)
                .width(Length::Fill),
            text(format!("{:.0}%", v * 100.0))
                .size(12)
                .width(40)
                .color(c.texto_medio),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .into(),
        None => text("Volume do sistema indisponível (precisa do wpctl/PipeWire).")
            .size(12)
            .style(fraco)
            .into(),
    }
}

fn renderiza_aba_audio<'a>(ctx: &ContextoConfiguracoes<'a>) -> Element<'a, Msg> {
    let c = tema::cores();
    let pa = ctx.painel_audio;
    let entradas = opcoes(&pa.entradas);
    let saidas = opcoes(&pa.saidas);
    let sel_e = pa
        .entrada_selecionada
        .as_ref()
        .and_then(|g| entradas.iter().find(|o| &o.guid == g).cloned());
    let sel_s = pa
        .saida_selecionada
        .as_ref()
        .and_then(|g| saidas.iter().find(|o| &o.guid == g).cloned());
    let lista_e: Element<'a, Msg> = if entradas.is_empty() {
        text("Nenhum microfone detectado.")
            .size(13)
            .style(fraco)
            .into()
    } else {
        pick_list(entradas, sel_e, |o| Msg::SelecionaEntradaAudio(o.guid))
            .placeholder("Escolha o microfone")
            .width(Length::Fill)
            .padding([8, 12])
            .text_size(13)
            .style(tema::lista_suspensa)
            .menu_style(tema::menu_suspenso)
            .into()
    };
    let lista_s: Element<'a, Msg> = if saidas.is_empty() {
        text("Nenhuma saída de som detectada.")
            .size(13)
            .style(fraco)
            .into()
    } else {
        pick_list(saidas, sel_s, |o| Msg::SelecionaSaidaAudio(o.guid))
            .placeholder("Escolha a saída")
            .width(Length::Fill)
            .padding([8, 12])
            .text_size(13)
            .style(tema::lista_suspensa)
            .menu_style(tema::menu_suspenso)
            .into()
    };

    let mut col = Column::new().spacing(18);
    col = col.push(titulo_secao("Voz"));
    col = col.push(
        row![
            campo_rotulado("Microfone", lista_e).width(Length::FillPortion(1)),
            campo_rotulado("Alto-falante", lista_s).width(Length::FillPortion(1)),
        ]
        .spacing(24),
    );
    col = col.push(
        row![
            campo_rotulado("Volume do microfone", slider_volume(true, ctx.volume_mic))
                .width(Length::FillPortion(1)),
            campo_rotulado(
                "Volume do alto-falante",
                slider_volume(false, ctx.volume_saida)
            )
            .width(Length::FillPortion(1)),
        ]
        .spacing(24),
    );
    col = col.push(
        text("Os volumes são os do sistema (PipeWire); o app não aplica ganho próprio.")
            .size(12)
            .style(fraco),
    );

    // Teste de microfone com medidor em segmentos.
    let btn_teste = button(
        text(if pa.testando_mic {
            "Parar teste"
        } else {
            "Teste do microfone"
        })
        .size(14),
    )
    .padding([9, 16])
    .style(tema::botao_primario)
    .on_press(Msg::AlternaTesteMic);
    const SEGMENTOS: usize = 40;
    // O ator manda o nível de 0 a 100 (raiz do pico × 100); tratado como 0–1,
    // qualquer som acendia a barra inteira.
    let acesos = ((pa.nivel_mic / 100.0).clamp(0.0, 1.0) * SEGMENTOS as f32).round() as usize;
    let medidor = row((0..SEGMENTOS).map(|i| -> Element<'a, Msg> {
        let ligado = pa.testando_mic && i < acesos;
        container(space().width(5).height(26))
            .style(move |_| container::Style {
                background: Some(iced::Background::Color(if ligado {
                    c.sucesso_forte
                } else {
                    c.forte
                })),
                border: iced::Border {
                    radius: 2.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
    }))
    .spacing(3);
    col = col.push(
        row![btn_teste, medidor]
            .spacing(16)
            .align_y(Alignment::Center),
    );

    col = col.push(linha_interruptor(
        "Supressão de ruído avançada (RNNoise)",
        pa.supressao_ruido,
        Msg::AlternaSupressaoRuido,
    ));
    col = col.push(
        text(
            "Tira teclado, ventilador e barulho de fundo do microfone. Sem o \
             cancelamento de eco do sistema: use com fone de ouvido.",
        )
        .size(12)
        .style(fraco),
    );

    col = col.push(divisoria());
    col = col.push(titulo_secao("Sons do aplicativo"));

    // Mesma grade de duas colunas da seção "Voz", para os sliders terem a
    // mesma largura; liga/desliga sempre como interruptor.
    let slider_pct = |valor: u8, msg: fn(u8) -> Msg| -> Element<'a, Msg> {
        row![
            slider(0..=100, valor, msg)
                .step(1)
                .style(tema::estilo_slider)
                .width(Length::Fill),
            text(format!("{valor}%"))
                .size(12)
                .width(40)
                .color(c.texto_medio),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .into()
    };
    col = col.push(
        row![
            campo_rotulado(
                "Volume do toque de chamada",
                slider_pct(ctx.prefs.volume_toque, Msg::DefineVolumeToque)
            )
            .width(Length::FillPortion(1)),
            campo_rotulado(
                "Volume dos efeitos e do soundboard",
                slider_pct(ctx.prefs.volume_efeitos, Msg::DefineVolumeEfeitos)
            )
            .width(Length::FillPortion(1)),
        ]
        .spacing(24),
    );
    col = col.push(
        button(
            row![
                svg(icones::volume())
                    .width(14)
                    .height(14)
                    .style(tema::icone_cor(c.texto)),
                text("Testar toque").size(13),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .padding([7, 14])
        .style(tema::botao_secundario)
        .on_press(Msg::TestaToque),
    );
    col = col.push(linha_interruptor(
        "Tocar som quando alguém ligar",
        ctx.prefs.som_do_toque,
        Msg::AlternaSomDoToque,
    ));
    col = col.push(linha_interruptor(
        "Ouvir o soundboard dos outros",
        !ctx.soundboard_mudo,
        Msg::AlternaMuteSoundboardGlobal,
    ));
    col.into()
}

// --- Conteúdo da Aba Vídeo ---
fn renderiza_aba_video<'a>(ctx: &ContextoConfiguracoes<'a>) -> Element<'a, Msg> {
    let mut col = Column::new().spacing(14);

    // Câmera
    let mut col_cam = Column::new().spacing(6);
    col_cam = col_cam.push(rotulo("DISPOSITIVO DE CÂMERA (WEBCAM)"));

    if ctx.cameras.is_empty() {
        col_cam = col_cam.push(
            text("Nenhuma câmera detectada em /dev/video*.")
                .size(12)
                .style(fraco),
        );
    } else {
        let opcoes: Vec<(usize, String)> = ctx
            .cameras
            .iter()
            .map(|c| (c.indice, c.nome.clone()))
            .collect();
        let atual = ctx
            .camera_selecionada
            .or_else(|| ctx.cameras.first().map(|c| c.indice))
            .unwrap_or(0);
        col_cam = col_cam.push(crate::ui::tela::segmentos(
            &opcoes,
            atual,
            Msg::SelecionaCameraConfig,
        ));
    }
    col = col.push(col_cam);

    // Espelhar vídeo
    col = col.push(
        row![
            checkbox(ctx.espelhar_camera)
                .on_toggle(|_| Msg::AlternaEspelharCamera)
                .size(16),
            text("Espelhar imagem da minha câmera localmente").size(13),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    );

    col = col.push(divisoria());

    // Transmissão de Tela
    let mut col_tela = Column::new().spacing(6);
    col_tela = col_tela.push(rotulo("PERFIL DE TRANSMISSÃO DE TELA"));
    col_tela = col_tela.push(
        text("Ajuste a resolução e taxa de quadros padrão ao apresentar a tela.")
            .size(12)
            .style(fraco),
    );

    col_tela = col_tela.push(
        text(format!("Atual: {}", ctx.perfil_tela.rotulo()))
            .size(12)
            .style(fraco),
    );
    col_tela = col_tela.push(crate::ui::tela::seletor_de_qualidade(
        ctx.perfil_tela,
        Msg::SelecionaPerfilTelaConfig,
    ));
    col = col.push(col_tela);

    col.into()
}

// --- Conteúdo da Aba Atalhos ---
fn renderiza_aba_atalhos<'a>(ctx: &ContextoConfiguracoes<'a>) -> Element<'a, Msg> {
    let c = tema::cores();
    let mut col = Column::new().spacing(14);

    let mut col_ptt = Column::new().spacing(8);
    col_ptt = col_ptt.push(rotulo("PUSH-TO-TALK (PRESSIONE PARA FALAR)"));

    col_ptt = col_ptt.push(
        row![
            checkbox(ctx.ptt.ativo)
                .on_toggle(|_| Msg::AlternaPttAtivo)
                .size(16),
            text(
                "Ativar Push-to-Talk (microfone só transmite enquanto a tecla estiver pressionada)"
            )
            .size(13),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    );

    if ctx.ptt.ativo {
        let texto_btn = if ctx.capturando_ptt {
            "Pressione qualquer tecla ou Esc para cancelar...".to_string()
        } else {
            format!("Tecla atual: {}", ctx.ptt.rotulo)
        };

        let linha_tecla = row![
            text("Tecla de atalho:").size(13),
            button(text(texto_btn).size(12).font(negrito()))
                .padding([6, 12])
                .style(if ctx.capturando_ptt {
                    tema::botao_primario
                } else {
                    tema::botao_secundario
                })
                .on_press(if ctx.capturando_ptt {
                    Msg::CancelaCapturaPtt
                } else {
                    Msg::IniciaCapturaPtt
                }),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        col_ptt = col_ptt.push(linha_tecla);
    }

    col = col.push(col_ptt);
    col = col.push(divisoria());

    // Mudo, surdo, atender e desligar: clique e aperte a tecla nova.
    col = col.push(rotulo("OUTROS ATALHOS"));
    col = col.push(
        text("Clique na tecla e aperte a combinação nova (Esc cancela). Valem com o app em foco; fora dele, o compositor precisa da linha da seção abaixo.")
            .size(12)
            .style(fraco),
    );
    for (id, nome, _) in crate::atalhos::ACOES
        .iter()
        .filter(|(id, _, _)| *id != "ptt")
    {
        let capturando = ctx.capturando_atalho == Some(*id);
        let combo = ctx.atalhos.combo(id);
        let tecla = button(
            text(if capturando {
                "Aperte a tecla…".to_string()
            } else {
                combo
            })
            .size(12)
            .font(negrito()),
        )
        .padding([5, 12])
        .style(if capturando {
            tema::botao_primario
        } else {
            tema::botao_secundario
        })
        .on_press(Msg::CapturaAtalho((*id).to_string()));
        col = col.push(
            row![
                text(*nome).size(13).width(Length::Fill),
                tecla,
                button(text("Padrão").size(11))
                    .padding([4, 8])
                    .style(tema::botao_secundario)
                    .on_press(Msg::RestauraAtalho((*id).to_string())),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );
    }
    if let Some(a) = ctx.aviso_atalho {
        col = col.push(text(a.to_string()).size(12).style(texto_perigo));
    }
    col = col.push(divisoria());

    // Hyprland: as linhas prontas para o hyprland.conf.
    let mut linhas = String::new();
    for (id, _, _) in crate::atalhos::ACOES.iter() {
        let combo = if *id == "ptt" {
            ctx.ptt.rotulo.clone()
        } else {
            ctx.atalhos.combo(id)
        };
        linhas.push_str(&crate::atalhos::linha_hyprland(id, &combo));
        linhas.push('\n');
    }
    col = col.push(rotulo("NO HYPRLAND"));
    col = col.push(
        text("O Hyprland é quem liga a tecla (por isso funciona com o app fora de foco). Cole no hyprland.conf e recarregue:")
            .size(12)
            .style(fraco),
    );
    col = col.push(
        container(text(linhas.clone()).size(12).font(iced::Font::MONOSPACE))
            .padding(10)
            .width(Length::Fill)
            .style(move |_| container::Style {
                background: Some(iced::Background::Color(c.palco)),
                border: iced::Border {
                    radius: tema::RAIO.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
    );
    col = col.push(
        button(text("Copiar linhas").size(12))
            .padding([6, 12])
            .style(tema::botao_secundario)
            .on_press(Msg::CopiaTexto(linhas)),
    );
    col = col.push(divisoria());

    // Portal global do Wayland
    let mut col_portal = Column::new().spacing(4);
    col_portal = col_portal.push(rotulo(
        "CAPTURA GLOBAL DE ATALHOS NO LINUX (WAYLAND / HYPRLAND)",
    ));
    if ctx.portal_global_ativo {
        col_portal = col_portal.push(
            text("O portal global do Wayland (ashpd / GlobalShortcuts) está ativo. O PTT funciona mesmo com o app em segundo plano.")
                .size(12)
                .style(destaque),
        );
    } else {
        col_portal = col_portal.push(
            text("Portal global não inicializado. O atalho de PTT será detectado quando a janela estiver focada.")
                .size(12)
                .style(fraco),
        );
    }
    col = col.push(col_portal);
    col = col.push(divisoria());

    // Bandeja do Sistema
    let mut col_bandeja = Column::new().spacing(6);
    col_bandeja = col_bandeja.push(rotulo("BANDEJA DO SISTEMA (STATUSNOTIFIERITEM)"));
    col_bandeja = col_bandeja.push(
        row![
            checkbox(ctx.minimizar_para_bandeja)
                .on_toggle(|_| Msg::AlternaMinimizarParaBandeja)
                .size(16),
            column![
                text("Minimizar para a bandeja ao fechar")
                    .size(13)
                    .font(negrito()),
                text("Fechar a janela esconde o app na bandeja em vez de desconectar a chamada.")
                    .size(11)
                    .style(fraco),
            ]
            .spacing(2),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    );
    col = col.push(col_bandeja);
    col = col.push(divisoria());

    // Ajuda e diagnóstico: os botões do topo da sala saíram.
    col = col.push(rotulo("AJUDA E DIAGNÓSTICO"));
    col = col.push(
        row![
            button(text("Manual (F1)").size(13))
                .padding([6, 12])
                .style(tema::botao_secundario)
                .on_press(Msg::AbreManual),
            button(text("Diagnóstico de rede (Ctrl+Shift+D)").size(13))
                .padding([6, 12])
                .style(tema::botao_secundario)
                .on_press(Msg::TogglePainelDiagnostico),
        ]
        .spacing(8),
    );

    col.into()
}

// --- Conteúdo da Aba Aparência ---
fn renderiza_aba_aparencia<'a>(ctx: &ContextoConfiguracoes<'a>) -> Element<'a, Msg> {
    let mut col = Column::new().spacing(12);

    // 1. Densidade visual
    col = col.push(crate::ui::layout::seletor_densidade(ctx.densidade));
    col = col.push(divisoria());

    // Desempenho (como no Discord): menos movimento e GIF sob demanda.
    let desemp = crate::desempenho::atual();
    col = col.push(rotulo("DESEMPENHO"));
    col = col.push(
        row![
            checkbox(desemp.reduz_animacoes)
                .on_toggle(|_| Msg::AlternaReduzAnimacoes)
                .size(16),
            column![
                text("Reduzir animações").size(13),
                text("Desliga transições, balanços e entradas de janela. Também para os GIFs, menos o que está sob o mouse.")
                    .size(11)
                    .style(fraco),
            ]
            .spacing(2),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    );
    col = col.push(
        row![
            checkbox(desemp.gif_so_hover)
                .on_toggle(|_| Msg::AlternaGifSoHover)
                .size(16),
            column![
                text("Animar GIFs só com o mouse em cima").size(13),
                text("Avatares, banners e figurinhas ficam parados até você passar o mouse.")
                    .size(11)
                    .style(fraco),
            ]
            .spacing(2),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    );
    col = col.push(divisoria());

    // 2. Tema e cores
    col = col.push(rotulo("TEMA E PALETA DE CORES"));
    col = col.push(
        text("Selecione uma paleta sóbria da v1 ou sincronize automaticamente com o wallpaper via Wallust.")
            .size(12)
            .style(fraco),
    );

    if tema::wallust_disponivel() {
        col = col.push(
            button(text("Recarregar cores do Wallust agora").size(12))
                .padding([6, 12])
                .style(tema::botao_secundario)
                .on_press(Msg::Tema(crate::ui::tema::TemaMsg::RecarregarWallust)),
        );
    }

    let mut lista_temas = Column::new().spacing(6);
    for t in &ctx.temas {
        let eh_ativo = t.id == ctx.tema_atual;
        let c_tema = cartao_tema(t, eh_ativo).map(Msg::Tema);
        lista_temas = lista_temas.push(c_tema);
    }

    col = col.push(lista_temas);
    col.into()
}

// --- Conteúdo da Aba Conta ---
fn renderiza_aba_conta<'a>(ctx: &ContextoConfiguracoes<'a>, secao: AbaPerfil) -> Element<'a, Msg> {
    let Some(ed) = ctx.edicao_perfil else {
        return text("Não foi possível carregar os dados da conta.")
            .size(13)
            .style(fraco)
            .into();
    };

    let mut col = Column::new().spacing(12);

    if !ctx.usuario.is_empty() {
        col = col.push(
            text(format!("Conectado como: @{}", ctx.usuario))
                .size(12)
                .style(fraco),
        );
    }

    if secao == AbaPerfil::Perfil {
        return perfil::editor_perfil(ctx, ed);
    } else {
        // Aba de Segurança (Troca de senha e Apagar conta)
        col = col.push(rotulo("ALTERAÇÃO DE SENHA"));
        col = col.push(
            text_input("Senha atual", &ed.senha_atual)
                .secure(true)
                .on_input(Msg::RascunhoSenhaAtual)
                .padding([6, 10])
                .size(13)
                .style(tema::campo),
        );
        col = col.push(
            text_input("Nova senha (mínimo 8 caracteres)", &ed.senha_nova)
                .secure(true)
                .on_input(Msg::RascunhoSenhaNova)
                .padding([6, 10])
                .size(13)
                .style(tema::campo),
        );
        col = col.push(
            text_input("Confirme a nova senha", &ed.senha_nova_confirma)
                .secure(true)
                .on_input(Msg::RascunhoSenhaNovaConfirma)
                .padding([6, 10])
                .size(13)
                .style(tema::campo),
        );
        col = col.push(
            button(
                text(if ed.trocando_senha {
                    "Alterando..."
                } else {
                    "Alterar Senha"
                })
                .size(13),
            )
            .padding([7, 14])
            .style(tema::botao_primario)
            .on_press_maybe((!ed.trocando_senha).then_some(Msg::SubmeteTrocaSenha)),
        );

        col = col.push(divisoria());

        col = col.push(rotulo("ZONA DE PERIGO: APAGAR CONTA"));
        if ed.confirma_apagar {
            let col_confirma = column![
                text("Atenção: sua conta, dados e conexões serão apagados permanentemente.")
                    .size(12)
                    .style(texto_perigo),
                text_input(
                    "Digite sua senha para confirmar a exclusão",
                    &ed.senha_apagar
                )
                .secure(true)
                .on_input(Msg::RascunhoSenhaApagarConta)
                .padding([6, 10])
                .size(13)
                .style(tema::campo),
                row![
                    button(
                        text(if ed.apagando {
                            "Apagando..."
                        } else {
                            "Confirmar e Apagar Definitivamente"
                        })
                        .size(12)
                    )
                    .padding([6, 12])
                    .style(tema::botao_perigo)
                    .on_press_maybe((!ed.apagando).then_some(Msg::SubmeteApagarConta)),
                    button(text("Cancelar").size(12))
                        .padding([6, 12])
                        .style(tema::botao_secundario)
                        .on_press(Msg::CancelaConfirmacaoApagarConta),
                ]
                .spacing(8),
            ]
            .spacing(8);
            col = col.push(col_confirma);
        } else {
            col = col.push(
                button(text("Apagar Minha Conta...").size(12))
                    .padding([6, 12])
                    .style(tema::botao_perigo)
                    .on_press(Msg::IniciaConfirmacaoApagarConta),
            );
        }
    }

    col.into()
}

// --- Conteúdo da Aba Notificações ---
fn renderiza_aba_notificacoes<'a>(ctx: &ContextoConfiguracoes<'a>) -> Element<'a, Msg> {
    let linha = |ligado: bool, titulo: &'static str, ajuda: &'static str, msg: Msg| {
        row![
            checkbox(ligado).on_toggle(move |_| msg.clone()).size(16),
            column![
                text(titulo).size(13).font(negrito()),
                text(ajuda).size(11).style(fraco),
            ]
            .spacing(2),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
    };
    let mut col = Column::new().spacing(14);
    col = col.push(rotulo("NOTIFICAÇÕES DO SISTEMA"));
    col = col.push(linha(
        ctx.prefs.notificar_mensagens,
        "Mensagens",
        "Avisar de mensagem nova quando a janela não está à vista. Conversas silenciadas ficam de fora.",
        Msg::AlternaNotificaMensagens,
    ));
    col = col.push(linha(
        ctx.prefs.notificar_mencoes,
        "Menções",
        "Avisar quando alguém citar você com @, mesmo em conversa silenciada.",
        Msg::AlternaNotificaMencoes,
    ));
    col = col.push(divisoria());
    col = col.push(rotulo("CHAMADAS"));
    col = col.push(linha(
        ctx.prefs.som_do_toque,
        "Som de chamada",
        "Tocar o toque quando alguém ligar. O aviso do sistema vem sempre que a janela está fora de foco.",
        Msg::AlternaSomDoToque,
    ));
    col = col.push(linha(
        ctx.prefs.sons_de_retorno,
        "Sons de retorno",
        "Tons só para você ao mutar o microfone, desligar o fone, entrar ou sair da chamada, e ao começar, parar ou assistir transmissões.",
        Msg::AlternaSonsDeRetorno,
    ));
    col.into()
}

fn arquivo_autostart() -> Option<std::path::PathBuf> {
    let d = directories::BaseDirs::new()?;
    Some(
        d.config_dir()
            .join("autostart")
            .join("lincast-chat.desktop"),
    )
}

fn autostart_ligado() -> bool {
    arquivo_autostart().is_some_and(|p| p.exists())
}

/// Liga/desliga a abertura com a sessão (entrada em `~/.config/autostart`).
pub fn alterna_autostart() {
    let Some(p) = arquivo_autostart() else { return };
    if p.exists() {
        let _ = std::fs::remove_file(&p);
        return;
    }
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let exe = std::env::current_exe()
        .ok()
        .and_then(|e| e.to_str().map(str::to_string))
        .unwrap_or_else(|| "lincast-chat".into());
    let conteudo = format!(
        "[Desktop Entry]\nType=Application\nName=Lincast Chat\nExec={exe}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"
    );
    let _ = std::fs::write(p, conteudo);
}

// --- Conteúdo da aba Sistema ---
fn renderiza_aba_sistema<'a>(ctx: &ContextoConfiguracoes<'a>) -> Element<'a, Msg> {
    let linha = |ligado: bool, titulo: &'static str, ajuda: &'static str, msg: Msg| {
        row![
            checkbox(ligado).on_toggle(move |_| msg.clone()).size(16),
            column![
                text(titulo).size(13).font(negrito()),
                text(ajuda).size(11).style(fraco),
            ]
            .spacing(2),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
    };
    let mut col = Column::new().spacing(14);
    col = col.push(rotulo("INICIALIZAÇÃO"));
    col = col.push(linha(
        autostart_ligado(),
        "Abrir o Lincast Chat ao entrar no sistema",
        "Cria uma entrada em ~/.config/autostart. Desligar remove a entrada.",
        Msg::AlternaAutostart,
    ));
    col = col.push(divisoria());
    col = col.push(rotulo("JANELA"));
    col = col.push(linha(
        ctx.minimizar_para_bandeja,
        "Minimizar para a bandeja ao fechar",
        "Fechar a janela deixa o app rodando na bandeja do sistema.",
        Msg::AlternaMinimizarParaBandeja,
    ));
    col = col.push(divisoria());
    col = col.push(rotulo("RELATÓRIO DE PROBLEMAS"));
    col = col.push(linha(
        crate::relatorio::enviar_ligado(),
        "Enviar o log do app aos desenvolvedores ao fechar",
        "Ajuda a achar defeitos. Tokens e senhas de convite são removidos antes do envio.",
        Msg::AlternaEnviarRelatorio,
    ));
    col = col.push(divisoria());
    col = col.push(rotulo("ATUALIZAÇÃO"));
    col = col.push(
        text("Ao abrir, o app procura uma versão nova e se atualiza sozinho.")
            .size(11)
            .style(fraco),
    );
    col = col.push(
        row![
            button(text("Verificar atualização").size(13))
                .padding([7, 14])
                .style(tema::botao_primario)
                .on_press_maybe((!ctx.atualizando).then_some(Msg::VerificaAtualizacaoManual)),
            text(ctx.atualizacao_status.unwrap_or(""))
                .size(12)
                .style(fraco),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    );
    col = col.push(divisoria());
    col = col.push(rotulo("SOBRE"));
    col = col.push(text(format!("Lincast Chat {}", env!("CARGO_PKG_VERSION"))).size(13));
    col.into()
}

// --- Conteúdo da aba Jogos registrados ---
fn miniatura_de_jogo<'a>(capa: Option<u64>) -> Element<'a, Msg> {
    let c = tema::cores();
    match capa.and_then(|id| crate::avatar::handle_exibicao(&crate::api::capa_url(id))) {
        Some(h) => iced::widget::image(h)
            .width(36)
            .height(50)
            .content_fit(iced::ContentFit::Cover)
            .border_radius(tema::RAIO)
            .into(),
        None => container(crate::ui::efeitos::ico(
            icones::controle(),
            18.0,
            c.texto_fraco,
        ))
        .width(36)
        .height(50)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(c.elevado)),
            border: iced::Border {
                radius: tema::RAIO.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into(),
    }
}

/// Modal de visão geral com detalhes do jogo (capa grande, processo, última atividade).
pub fn modal_visao_jogo<'a>(j: &'a crate::estado::ModalVisaoJogo) -> Element<'a, Msg> {
    let c = tema::cores();
    let capa_elem: Element<'a, Msg> = match j
        .capa
        .and_then(|id| crate::avatar::handle_exibicao(&crate::api::capa_url(id)))
    {
        Some(h) => iced::widget::image(h)
            .width(130)
            .height(180)
            .content_fit(iced::ContentFit::Cover)
            .border_radius(tema::RAIO)
            .into(),
        None => container(crate::ui::efeitos::ico(
            icones::controle(),
            48.0,
            c.texto_fraco,
        ))
        .width(130)
        .height(180)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(c.elevado)),
            border: iced::Border {
                radius: tema::RAIO.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into(),
    };

    let cabecalho = row![
        text("Visão Geral do Jogo").size(16).font(negrito()),
        space().width(Length::Fill),
        button(
            svg(icones::fechar())
                .width(16)
                .height(16)
                .style(tema::icone_cor(c.texto_fraco)),
        )
        .padding(4)
        .style(tema::botao_secundario)
        .on_press(Msg::FechaVisaoJogo),
    ]
    .align_y(Alignment::Center);

    let mut detalhes = column![
        text(&j.nome).size(18).font(negrito()),
        row![
            container(
                text(if j.manual {
                    "Adicionado à mão"
                } else {
                    "Detectado automaticamente"
                })
                .size(11)
                .style(destaque)
            )
            .padding([2, 6])
            .style(move |_| container::Style {
                background: Some(iced::Background::Color(c.elevado)),
                border: iced::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
        space().height(4),
        text(format!(
            "Última atividade: {}",
            crate::jogos_registrados::ha_quanto_tempo(j.ultimo)
        ))
        .size(12)
        .style(fraco),
    ]
    .spacing(4)
    .width(Length::Fill);

    if let Some(exec) = &j.exec {
        detalhes = detalhes.push(
            text(format!("Executável / Processo: {exec}"))
                .size(12)
                .style(fraco),
        );
    }

    let corpo = row![capa_elem, detalhes]
        .spacing(16)
        .align_y(Alignment::Start);

    let rodape = row![
        space().width(Length::Fill),
        button(text("Fechar").size(13))
            .padding([8, 16])
            .style(tema::botao_primario)
            .on_press(Msg::FechaVisaoJogo),
    ]
    .align_y(Alignment::Center);

    let painel = container(
        column![cabecalho, divisoria(), corpo, space().height(8), rodape]
            .spacing(12)
            .width(480),
    )
    .padding(20)
    .style(move |_| container::Style {
        background: Some(iced::Background::Color(c.fundo)),
        border: iced::Border {
            color: c.borda,
            width: 1.0,
            radius: tema::RAIO_GRANDE.into(),
        },
        shadow: iced::Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.4),
            offset: iced::Vector::new(0.0, 6.0),
            blur_radius: 20.0,
        },
        ..Default::default()
    });

    crate::ui::efeitos::janela_fechavel("visao-jogo", painel.into(), 0.65, Msg::FechaVisaoJogo)
}

pub(crate) fn interruptor<'a>(ativo: bool, msg: Msg) -> Element<'a, Msg> {
    let c = tema::cores();
    let bg_cor = if ativo { c.destaque } else { c.elevado };
    let dot_cor = if ativo {
        c.sobre_destaque
    } else {
        c.texto_fraco
    };
    let bolinha = container(space().width(12).height(12)).style(move |_| container::Style {
        background: Some(iced::Background::Color(dot_cor)),
        border: iced::Border {
            radius: 6.0.into(),
            ..Default::default()
        },
        ..Default::default()
    });
    let switch_layout = if ativo {
        row![space().width(12), bolinha].align_y(Alignment::Center)
    } else {
        row![bolinha, space().width(12)].align_y(Alignment::Center)
    };
    button(
        container(switch_layout)
            .width(34)
            .height(20)
            .padding([3, 4])
            .style(move |_| container::Style {
                background: Some(iced::Background::Color(bg_cor)),
                border: iced::Border {
                    radius: 10.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
    )
    .padding(0)
    .style(tema::botao_transparente)
    .on_press(msg)
    .into()
}

fn renderiza_aba_jogos<'a>(ctx: &ContextoConfiguracoes<'a>) -> Element<'a, Msg> {
    let c = tema::cores();
    let cartao = move |corpo: Element<'a, Msg>| {
        container(corpo)
            .padding(12)
            .width(Length::Fill)
            .style(move |_| container::Style {
                background: Some(iced::Background::Color(c.elevado)),
                border: iced::Border {
                    radius: tema::RAIO_GRANDE.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
    };
    let mut col = Column::new().spacing(14);

    col = col.push(rotulo("JOGO ATUAL"));
    let atual = ctx.atividade.as_ref().and_then(|a| a.jogo.as_ref());
    col = col.push(match atual {
        Some(j) => {
            let visao = crate::estado::ModalVisaoJogo {
                nome: j.nome.clone(),
                exec: None,
                capa: j.capa,
                manual: false,
                ultimo: chrono::Utc::now().timestamp_millis() as u64,
            };
            cartao(
                row![
                    button(miniatura_de_jogo(j.capa))
                        .padding(0)
                        .style(tema::botao_transparente)
                        .on_press(Msg::AbreVisaoJogo(visao.clone())),
                    column![
                        button(text(j.nome.clone()).size(14).font(negrito()))
                            .padding(0)
                            .style(tema::botao_transparente)
                            .on_press(Msg::AbreVisaoJogo(visao)),
                        row![
                            container(space().width(8).height(8)).style(move |_| container::Style {
                                background: Some(iced::Background::Color(c.destaque)),
                                border: iced::Border {
                                    radius: 4.0.into(),
                                    ..Default::default()
                                },
                                ..Default::default()
                            }),
                            text("Jogando agora!").size(12).style(destaque),
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center),
                    ]
                    .spacing(3)
                    .width(Length::Fill),
                    interruptor(true, Msg::JogosRegAlternaAtivo(j.nome.clone())),
                ]
                .spacing(12)
                .align_y(Alignment::Center)
                .into(),
            )
        }
        None => cartao(
            text("Nenhum jogo detectado agora. Abra um jogo suportado ou adicione o programa abaixo.")
                .size(12)
                .style(fraco)
                .into(),
        ),
    });

    let link_adicionar = row![
        text("Não está vendo seu jogo?").size(12).style(fraco),
        button(
            text(if ctx.jogos_reg_seletor_aberto {
                "Ocultar seletor"
            } else {
                "Adicione-o!"
            })
            .size(12)
            .style(destaque)
        )
        .padding(0)
        .style(tema::botao_transparente)
        .on_press(Msg::JogosRegAlternaSeletor),
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    col = col.push(link_adicionar);

    if ctx.jogos_reg_seletor_aberto {
        let mut caixa_seletor = column![
            row![
                text_input("Filtrar programas abertos…", ctx.jogos_reg_filtro)
                    .on_input(Msg::JogosRegFiltro)
                    .padding([6, 10])
                    .size(13)
                    .style(tema::campo),
                crate::ui::efeitos::com_dica(
                    button(crate::ui::efeitos::ico(
                        icones::atualizar(),
                        15.0,
                        c.texto_fraco
                    ))
                    .padding(8)
                    .style(tema::botao_secundario)
                    .on_press(Msg::JogosRegAtualiza),
                    "Atualizar a lista de programas",
                    iced::widget::tooltip::Position::Left,
                ),
            ]
            .spacing(6)
            .align_y(Alignment::Center)
        ]
        .spacing(8);

        let filtro = ctx.jogos_reg_filtro.trim().to_lowercase();
        let ja: std::collections::HashSet<&str> = ctx
            .jogos_reg
            .iter()
            .filter_map(|j| j.exec.as_deref())
            .collect();
        let mut mostrados = 0;
        let mut lista_procs = Column::new().spacing(4);
        for p in ctx.processos_abertos {
            if mostrados >= 14 {
                break;
            }
            if ja.contains(p.as_str()) || (!filtro.is_empty() && !p.contains(&filtro)) {
                continue;
            }
            mostrados += 1;
            lista_procs = lista_procs.push(
                row![
                    text(p.clone()).size(12).width(Length::Fill),
                    button(text("Adicionar").size(11))
                        .padding([4, 10])
                        .style(tema::botao_primario)
                        .on_press(Msg::JogosRegAdiciona(p.clone())),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );
        }
        if mostrados == 0 {
            lista_procs = lista_procs.push(
                text("Nenhum programa novo encontrado.")
                    .size(12)
                    .style(fraco),
            );
        }
        caixa_seletor = caixa_seletor.push(lista_procs);
        col = col.push(
            container(caixa_seletor)
                .padding(12)
                .width(Length::Fill)
                .style(move |_| container::Style {
                    background: Some(iced::Background::Color(c.elevado)),
                    border: iced::Border {
                        color: c.borda,
                        width: 1.0,
                        radius: tema::RAIO.into(),
                    },
                    ..Default::default()
                }),
        );
    }

    col = col.push(divisoria());

    col = col.push(rotulo("JOGOS ADICIONADOS"));
    col = col.push(
        text(
            "Jogos detectados e adicionados à sua biblioteca. Clique no nome para ver a visão geral ou no lápis para renomear.",
        )
        .size(12)
        .style(fraco),
    );
    if ctx.jogos_reg.is_empty() {
        col = col.push(text("Nenhum jogo registrado ainda.").size(12).style(fraco));
    }
    let mut ordenados: Vec<&crate::jogos_registrados::JogoRegistrado> =
        ctx.jogos_reg.iter().collect();
    ordenados.sort_by(|a, b| b.ultimo.cmp(&a.ultimo).then(a.nome.cmp(&b.nome)));
    for j in ordenados {
        let chave = j.exec.as_deref().unwrap_or(j.nome.as_str());
        let renomeando_este = ctx
            .jogos_reg_renomeando
            .as_ref()
            .map(|(k, _)| *k == chave || *k == j.nome.as_str())
            .unwrap_or(false);

        let visao = crate::estado::ModalVisaoJogo {
            nome: j.nome.clone(),
            exec: j.exec.clone(),
            capa: j.capa,
            manual: j.manual,
            ultimo: j.ultimo,
        };

        let miniatura = button(miniatura_de_jogo(j.capa))
            .padding(0)
            .style(tema::botao_transparente)
            .on_press(Msg::AbreVisaoJogo(visao.clone()));

        let info_col: Element<'a, Msg> = if renomeando_este {
            let rascunho = ctx
                .jogos_reg_renomeando
                .as_ref()
                .map(|(_, r)| *r)
                .unwrap_or(&j.nome);
            row![
                text_input("Novo nome do jogo…", rascunho)
                    .on_input(Msg::JogosRegRascunhoNome)
                    .padding([4, 8])
                    .size(12)
                    .style(tema::campo)
                    .width(Length::Fill),
                button(text("Salvar").size(11))
                    .padding([4, 8])
                    .style(tema::botao_primario)
                    .on_press(Msg::JogosRegSalvaNome),
                button(text("Cancelar").size(11))
                    .padding([4, 8])
                    .style(tema::botao_secundario)
                    .on_press(Msg::JogosRegCancelaRenomear),
            ]
            .spacing(6)
            .align_y(Alignment::Center)
            .width(Length::Fill)
            .into()
        } else {
            let mut info = column![
                button(text(j.nome.clone()).size(13).font(negrito()))
                    .padding(0)
                    .style(tema::botao_transparente)
                    .on_press(Msg::AbreVisaoJogo(visao)),
                text(crate::jogos_registrados::ha_quanto_tempo(j.ultimo))
                    .size(11)
                    .style(fraco),
            ]
            .spacing(2)
            .width(Length::Fill);
            if let (true, Some(e)) = (j.manual, j.exec.as_deref()) {
                info = info.push(
                    text(format!("Adicionado à mão · {e}"))
                        .size(11)
                        .style(fraco),
                );
            }
            info.into()
        };

        col = col.push(
            row![
                miniatura,
                info_col,
                crate::ui::efeitos::com_dica(
                    button(crate::ui::efeitos::ico(
                        icones::lapis(),
                        14.0,
                        c.texto_fraco
                    ))
                    .padding(6)
                    .style(tema::botao_secundario)
                    .on_press(Msg::JogosRegIniciaRenomear {
                        exec: chave.to_string(),
                        nome_atual: j.nome.clone(),
                    }),
                    "Renomear jogo",
                    iced::widget::tooltip::Position::Left,
                ),
                interruptor(j.ativo, Msg::JogosRegAlternaAtivo(chave.to_string()),),
                crate::ui::efeitos::com_dica(
                    button(crate::ui::efeitos::ico(
                        icones::lixeira(),
                        14.0,
                        c.texto_fraco
                    ))
                    .padding(6)
                    .style(tema::botao_secundario)
                    .on_press(Msg::JogosRegRemove(j.nome.clone())),
                    "Remover da lista",
                    iced::widget::tooltip::Position::Left,
                ),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        );
    }

    col.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nome_do_dispositivo_padrao_fica_legivel() {
        assert_eq!(
            nome_de_dispositivo("default: USB PnP Audio Device Mono"),
            "USB PnP Audio Device Mono (padrão)"
        );
        assert_eq!(nome_de_dispositivo("Fone Bluetooth"), "Fone Bluetooth");
        assert_eq!(nome_de_dispositivo("default:"), "default:");
    }

    #[test]
    fn testa_todas_as_abas_config() {
        // O perfil tem janela própria (Editar perfil): não é uma aba das configurações.
        assert_eq!(AbaConfig::TODAS.len(), 7);
        assert!(!AbaConfig::TODAS.contains(&AbaConfig::Perfil));
        assert_eq!(AbaConfig::Conta.rotulo(), "Minha conta");
        assert_eq!(AbaConfig::VozVideo.rotulo(), "Voz e vídeo");
        assert_eq!(AbaConfig::Atalhos.rotulo(), "Atalhos");
        assert_eq!(AbaConfig::Aparencia.rotulo(), "Aparência");
        assert_eq!(AbaConfig::Notificacoes.rotulo(), "Notificações");
        assert_eq!(AbaConfig::default(), AbaConfig::VozVideo);
    }
}
