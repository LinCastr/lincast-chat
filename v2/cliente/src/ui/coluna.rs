//! Coluna da esquerda (C1): busca, Amigos, conversas, painel de voz e o rodapé
//! do usuário. É a mesma nas telas de fora e de dentro de uma conversa.

use std::time::Instant;

use iced::widget::{
    Column, button, column, container, row, scrollable, space, svg, text, text_input,
};
use iced::{Alignment, Background, Border, Element, Length};

use crate::api::{Atividade, Eu};
use crate::avisos::{Conversa, EstadoAvisos};
use crate::ui::efeitos;
use crate::{Msg, icones, tema, ui};
use crate::{botao_icone, fraco, linha_horizontal, negrito, nome_da_conversa};

/// O que a coluna precisa saber da chamada em que a pessoa está.
pub struct ChamadaLocal<'a> {
    pub titulo: &'a str,
    pub desde: Option<Instant>,
    pub com_camera: bool,
    pub apresentando: bool,
    pub som_aberto: bool,
}

pub struct ContextoColuna<'a> {
    pub eu: Option<&'a Eu>,
    pub avisos: &'a EstadoAvisos,
    pub filtro: &'a str,
    /// Id da conversa aberta, se houver.
    pub ativa: Option<&'a str>,
    pub amigos_aberto: bool,
    pub pedidos_pendentes: usize,
    pub status: &'a str,
    pub chamada: Option<ChamadaLocal<'a>>,
    /// Wordle aberto: título da conversa e modo (cartão na coluna).
    pub wordle: Option<(&'a str, crate::ui::wordle::ModoWordle)>,
    /// Estado do microfone/áudio. `None` quando não há sala aberta: os botões
    /// ficam apagados.
    pub mudo: Option<bool>,
    pub surdo: Option<bool>,
    /// Conversas fechadas/silenciadas localmente.
    pub prefs: &'a crate::prefs_conversas::PrefsConversas,
}

/// Conversas cujo nome casa com o texto da busca (sem distinguir maiúsculas
/// nem acentos simples). Busca vazia mostra todas.
pub fn filtra_conversas<'a>(
    todas: &'a [Conversa],
    filtro: &str,
    meu_usuario: Option<&str>,
) -> Vec<&'a Conversa> {
    let f = filtro.trim().to_lowercase();
    todas
        .iter()
        .filter(|c| f.is_empty() || nome_da_conversa(c, meu_usuario).to_lowercase().contains(&f))
        .collect()
}

/// `mm:ss` (ou `h:mm:ss` passando de uma hora).
pub fn formata_duracao(seg: u64) -> String {
    let (h, m, s) = (seg / 3600, (seg / 60) % 60, seg % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

fn subtitulo(c: &Conversa, avisos: &EstadoAvisos, meu_usuario: Option<&str>) -> ui::status::Sub {
    if c.tipo == "grupo" {
        return ui::status::Sub::texto(format!("{} membros", c.membros.len()));
    }
    let outro = c
        .membros
        .iter()
        .find(|m| m.usuario.as_deref() != meu_usuario);
    let Some(o) = outro else {
        return ui::status::Sub::default();
    };
    let atv = avisos
        .atividade_amigos
        .get(&o.uid)
        .or(o.atividade.as_ref())
        .and_then(|v| serde_json::from_value::<Atividade>(v.clone()).ok());
    let st = avisos
        .status_amigos
        .get(&o.uid)
        .map(String::as_str)
        .or(o.status.as_deref())
        .unwrap_or("offline");
    ui::status::Sub::de_pessoa(
        o.status_custom.as_ref(),
        atv.as_ref(),
        ui::status::rotulo_status(st),
    )
}

fn linha_de_conversa<'a>(
    c: &'a Conversa,
    ctx: &ContextoColuna<'a>,
    meu_usuario: Option<&str>,
) -> Element<'a, Msg> {
    let cores = tema::cores();
    let ativa = ctx.ativa == Some(c.id.as_str());
    let nome = nome_da_conversa(c, meu_usuario);
    let outros: Vec<&crate::avisos::Membro> = c
        .membros
        .iter()
        .filter(|m| m.usuario.as_deref() != meu_usuario)
        .collect();
    let icone: Element<'a, Msg> = if c.tipo == "grupo" {
        let membros: Vec<ui::avatar::MembroDoIcone<'_>> = outros
            .iter()
            .map(|m| ui::avatar::MembroDoIcone {
                chave: &m.uid,
                nome: &m.name,
                url: m.avatar.as_deref(),
            })
            .collect();
        ui::avatar::icone_grupo(c.icone.as_deref(), &membros, 32.0, cores.lateral)
    } else {
        let m = outros.first();
        let st = m.map(|m| {
            ctx.avisos
                .status_amigos
                .get(&m.uid)
                .map(String::as_str)
                .or(m.status.as_deref())
                .unwrap_or("offline")
        });
        ui::avatar::avatar(
            m.and_then(|m| m.avatar.as_deref()),
            m.map_or("", |m| m.uid.as_str()),
            &nome,
            32.0,
            st,
            cores.lateral,
        )
    };
    let em_chamada = ctx.avisos.chamadas.contains_key(&c.id);
    let sub = if em_chamada {
        text("Chamada em andamento")
            .size(12)
            .color(cores.sucesso_forte)
            .into()
    } else {
        ui::status::sub_widget(
            &subtitulo(c, ctx.avisos, meu_usuario),
            12.0,
            cores.texto_fraco,
        )
    };
    let mut linha = row![
        icone,
        column![text(nome).size(14).font(negrito()), sub]
            .spacing(1)
            .width(Length::Fill),
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    if em_chamada {
        linha = linha.push(
            svg(icones::volume())
                .width(16)
                .height(16)
                .style(tema::icone_cor(cores.sucesso_forte)),
        );
    }
    if let Some(n) = ctx.avisos.nao_lidas.get(&c.id).filter(|n| **n > 0) {
        linha = linha.push(
            container(text(n.to_string()).size(11).font(negrito()))
                .padding([1, 7])
                .style(move |_| container::Style {
                    background: Some(Background::Color(cores.destaque)),
                    text_color: Some(cores.sobre_destaque),
                    border: Border {
                        radius: 10.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
        );
    }
    let id_hover = format!("conv-{}", c.id);
    let btn = button(linha)
        .width(Length::Fill)
        .padding([6, 8])
        .style(efeitos::item_suave(id_hover.clone(), ativa))
        .on_press(Msg::AbreSala(c.id.clone()));
    iced::widget::mouse_area(btn)
        .on_enter(Msg::Hover(id_hover.clone(), true))
        .on_exit(Msg::Hover(id_hover, false))
        .on_right_press(Msg::AbreMenuConversa(c.id.clone()))
        .into()
}

fn nav_amigos<'a>(ctx: &ContextoColuna<'a>) -> Element<'a, Msg> {
    let cores = tema::cores();
    let ativo = ctx.amigos_aberto;
    let mut r = row![
        svg(icones::amigos())
            .width(20)
            .height(20)
            .style(tema::icone_cor(if ativo {
                cores.texto
            } else {
                cores.texto_medio
            })),
        text("Amigos").size(14).font(negrito()),
        space().width(Length::Fill),
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    if ctx.pedidos_pendentes > 0 {
        r = r.push(
            container(
                text(ctx.pedidos_pendentes.to_string())
                    .size(11)
                    .font(negrito()),
            )
            .padding([1, 7])
            .style(move |_| container::Style {
                background: Some(Background::Color(cores.perigo)),
                text_color: Some(cores.sobre_destaque),
                border: Border {
                    radius: 10.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
        );
    }
    button(r)
        .width(Length::Fill)
        .padding([9, 10])
        .style(move |_, st| {
            let hover = matches!(st, button::Status::Hovered);
            button::Style {
                background: Some(Background::Color(if ativo {
                    cores.forte
                } else if hover {
                    cores.elevado
                } else {
                    cores.lateral
                })),
                text_color: if ativo || hover {
                    cores.texto
                } else {
                    cores.texto_medio
                },
                border: Border {
                    radius: tema::RAIO.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .on_press(Msg::AbreAmigos)
        .into()
}

fn painel_de_voz<'a>(ch: &ChamadaLocal<'a>) -> Element<'a, Msg> {
    let cores = tema::cores();
    let tempo = ch
        .desde
        .map(|t| formata_duracao(t.elapsed().as_secs()))
        .unwrap_or_default();
    let topo = row![
        svg(icones::sinal())
            .width(20)
            .height(20)
            .style(tema::icone_cor(cores.sucesso_forte)),
        column![
            text("Voz conectada")
                .size(13)
                .font(negrito())
                .color(cores.sucesso_forte),
            text(format!("{} · {}", ch.titulo, tempo))
                .size(12)
                .style(fraco),
        ]
        .width(Length::Fill),
        crate::ui::efeitos::com_dica(
            botao_icone(icones::desligar(), false, true, Some(Msg::SaiDaChamada)),
            "Desconectar da chamada",
            iced::widget::tooltip::Position::Top
        ),
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    let botoes = row![
        crate::ui::efeitos::com_dica(
            botao_icone(
                if ch.com_camera {
                    icones::camera()
                } else {
                    icones::camera_off()
                },
                ch.com_camera,
                false,
                Some(if ch.com_camera {
                    Msg::ParaCamera
                } else {
                    Msg::PedeCamera
                }),
            ),
            "Câmera",
            iced::widget::tooltip::Position::Top
        ),
        crate::ui::efeitos::com_dica(
            botao_icone(
                icones::tela(),
                ch.apresentando,
                false,
                Some(if ch.apresentando {
                    Msg::ParaTela
                } else {
                    Msg::PedeTela
                }),
            ),
            "Compartilhar tela",
            iced::widget::tooltip::Position::Top
        ),
        crate::ui::efeitos::com_dica(
            efeitos::botao_icone_anim(
                "voz-atividades",
                icones::grade(),
                false,
                false,
                Some(Msg::AlternaMenuAtividades),
                efeitos::Efeito::Balanco,
            ),
            "Atividades",
            iced::widget::tooltip::Position::Top
        ),
        crate::ui::efeitos::com_dica(
            botao_icone(
                icones::musica(),
                ch.som_aberto,
                false,
                Some(Msg::AlternaPainelSoundboard),
            ),
            "Soundboard",
            iced::widget::tooltip::Position::Top
        ),
    ]
    .spacing(6);
    container(column![topo, botoes].spacing(8))
        .padding(10)
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(cores.palco)),
            border: Border {
                color: cores.borda,
                width: 1.0,
                radius: tema::RAIO_GRANDE.into(),
            },
            ..Default::default()
        })
        .into()
}

fn rodape_do_usuario<'a>(ctx: &ContextoColuna<'a>) -> Element<'a, Msg> {
    let cores = tema::cores();
    let nome = ctx.eu.map(|e| e.name.as_str()).unwrap_or("");
    let usuario = ctx.eu.and_then(|e| e.usuario.as_deref());
    let id = row![
        ui::avatar::avatar(
            ctx.eu.and_then(|e| e.avatar.as_deref()),
            usuario.unwrap_or(nome),
            nome,
            32.0,
            Some(ctx.status),
            cores.palco,
        ),
        column![
            text(nome).size(14).font(negrito()),
            text(ui::status::rotulo_status(ctx.status))
                .size(12)
                .style(fraco),
        ]
        .width(Length::Fill),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    container(
        row![
            button(id)
                .padding([2, 4])
                .width(Length::Fill)
                .style(move |_, st| button::Style {
                    background: matches!(st, button::Status::Hovered)
                        .then_some(Background::Color(cores.elevado)),
                    text_color: cores.texto,
                    border: Border {
                        radius: tema::RAIO.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .on_press(Msg::AlternaMenuStatus),
            crate::ui::efeitos::com_dica(
                efeitos::botao_icone_anim(
                    "rodape-mic",
                    if ctx.mudo == Some(true) {
                        icones::mic_off()
                    } else {
                        icones::mic()
                    },
                    ctx.mudo == Some(true),
                    false,
                    ctx.mudo.map(|_| Msg::AlternaMudo),
                    efeitos::Efeito::Balanco,
                ),
                "Silenciar microfone",
                iced::widget::tooltip::Position::Top
            ),
            crate::ui::efeitos::com_dica(
                efeitos::botao_icone_anim(
                    "rodape-fone",
                    if ctx.surdo == Some(true) {
                        icones::fone_off()
                    } else {
                        icones::fone()
                    },
                    ctx.surdo == Some(true),
                    false,
                    ctx.surdo.map(|_| Msg::AlternaSurdo),
                    efeitos::Efeito::Balanco,
                ),
                "Desativar áudio",
                iced::widget::tooltip::Position::Top
            ),
            crate::ui::efeitos::com_dica(
                efeitos::botao_icone_anim(
                    "rodape-engrenagem",
                    icones::configuracoes(),
                    false,
                    false,
                    Some(Msg::AbreConfiguracoes(None)),
                    efeitos::Efeito::Giro,
                ),
                "Configurações",
                iced::widget::tooltip::Position::Top
            ),
        ]
        .spacing(2)
        .align_y(Alignment::Center),
    )
    .padding([8, 8])
    .width(Length::Fill)
    .style(move |_| container::Style {
        background: Some(Background::Color(cores.palco)),
        ..Default::default()
    })
    .into()
}

pub fn coluna<'a>(ctx: &ContextoColuna<'a>) -> Element<'a, Msg> {
    let cores = tema::cores();
    let meu_usuario = ctx.eu.and_then(|e| e.usuario.as_deref());

    let busca = container(
        text_input("Encontre ou comece uma conversa", ctx.filtro)
            .on_input(Msg::FiltraConversas)
            .padding([7, 10])
            .size(13)
            .style(tema::campo_busca),
    )
    .padding(10);

    let mut lista = Column::new().spacing(2).padding(8);
    lista = lista.push(nav_amigos(ctx));
    lista = lista.push(
        row![
            text("Mensagens diretas".to_uppercase())
                .size(tema::TEXTO_ROTULO)
                .font(negrito())
                .style(fraco),
            space().width(Length::Fill),
            button(
                svg(icones::mais())
                    .width(16)
                    .height(16)
                    .style(tema::icone_cor(cores.texto_fraco))
            )
            .padding(2)
            .style(|_, _| button::Style::default())
            .on_press(Msg::AbreCriarGrupo(None)),
        ]
        .align_y(Alignment::Center)
        .padding([14, 8]),
    );
    let visiveis: Vec<&Conversa> = filtra_conversas(&ctx.avisos.conversas, ctx.filtro, meu_usuario)
        .into_iter()
        .filter(|c| !ctx.prefs.fechadas.contains(&c.id) || ctx.ativa == Some(c.id.as_str()))
        .collect();
    if visiveis.is_empty() {
        let t = if !ctx.filtro.trim().is_empty() {
            "Nenhuma conversa com esse nome."
        } else if ctx.avisos.conectado {
            "Nenhuma conversa ainda. Comece uma pela aba Amigos."
        } else {
            "Conectando…"
        };
        lista = lista.push(container(text(t).size(13).style(fraco)).padding([6, 10]));
    }
    for c in visiveis {
        lista = lista.push(linha_de_conversa(c, ctx, meu_usuario));
    }

    let mut base = column![
        busca,
        linha_horizontal(),
        scrollable(lista).height(Length::Fill).style(tema::rolagem),
    ];
    if let Some((titulo, modo)) = ctx.wordle {
        base =
            base.push(container(crate::ui::wordle::cartao_da_coluna(titulo, modo)).padding([0, 8]));
    }
    if let Some(ch) = &ctx.chamada {
        base = base.push(container(painel_de_voz(ch)).padding([0, 8]));
    }
    base = base.push(rodape_do_usuario(ctx));

    container(base)
        .width(252)
        .height(Length::Fill)
        .style(tema::painel_lateral_reto)
        .into()
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::avisos::Membro;

    fn grupo(id: &str, nome: &str) -> Conversa {
        Conversa {
            id: id.into(),
            tipo: "grupo".into(),
            nome: nome.into(),
            ..Default::default()
        }
    }

    fn dm(id: &str, outro: &str) -> Conversa {
        Conversa {
            id: id.into(),
            tipo: "dm".into(),
            membros: vec![
                Membro {
                    uid: "local:1".into(),
                    usuario: Some("eu".into()),
                    name: "Eu".into(),
                    ..Default::default()
                },
                Membro {
                    uid: "local:2".into(),
                    usuario: Some("outro".into()),
                    name: outro.into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn filtra_pelo_nome_mostrado() {
        let todas = vec![
            grupo("g1", "Os Cria"),
            dm("d1", "Davi"),
            grupo("g2", "Trabalho"),
        ];
        let nomes = |f: &str| -> Vec<String> {
            filtra_conversas(&todas, f, Some("eu"))
                .iter()
                .map(|c| c.id.clone())
                .collect()
        };
        assert_eq!(nomes("").len(), 3);
        assert_eq!(nomes("cria"), vec!["g1"]);
        assert_eq!(nomes("  DAV "), vec!["d1"]);
        assert!(nomes("zzz").is_empty());
    }

    #[test]
    fn duracao_formatada() {
        assert_eq!(formata_duracao(5), "00:05");
        assert_eq!(formata_duracao(61), "01:01");
        assert_eq!(formata_duracao(3661), "1:01:01");
    }
}
