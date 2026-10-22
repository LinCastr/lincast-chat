//! Painel da direita (C4): no grupo, os membros; na conversa direta, o perfil
//! da outra pessoa.

use iced::widget::{
    Column, button, column, container, image, row, scrollable, space, stack, svg, text,
};
use iced::{Alignment, Background, Border, ContentFit, Element, Length};

use crate::api::Atividade;
use crate::avisos::{EstadoAvisos, Membro};
use crate::ui::conversa::InfoConversa;
use crate::{Msg, avatar, fraco, icones, negrito, tema, ui};

fn atividade_de(m: &Membro, avisos: &EstadoAvisos) -> Option<Atividade> {
    let v = avisos
        .atividade_amigos
        .get(&m.uid)
        .or(m.atividade.as_ref())?;
    serde_json::from_value(v.clone()).ok()
}

/// Há quanto tempo (mm:ss ou h:mm:ss) o jogo está aberto.
pub(crate) fn tempo_de_jogo(desde_ms: u64) -> String {
    let agora = chrono::Utc::now().timestamp_millis().max(0) as u64;
    ui::coluna::formata_duracao(agora.saturating_sub(desde_ms) / 1000)
}

/// mm:ss (ou h:mm:ss) de uma duração em ms.
fn mmss(ms: u64) -> String {
    ui::coluna::formata_duracao(ms / 1000)
}

/// Bloco "Jogando" / "Ouvindo" do perfil, no formato do Discord: música com
/// capa, título, artista, álbum e barra de progresso.
pub(crate) fn bloco_atividade<'a>(a: &Atividade) -> Option<Element<'a, Msg>> {
    let c = tema::cores();
    let (rotulo, corpo): (&'static str, Element<'a, Msg>) = if let Some(j) = &a.jogo {
        let mut linha = column![text(j.nome.clone()).size(14).font(negrito())].spacing(2);
        if let Some(d) = j.desde.map(tempo_de_jogo) {
            linha = linha.push(
                row![
                    ui::efeitos::ico(icones::controle(), 13.0, c.sucesso_forte),
                    text(format!("há {d}")).size(12).style(fraco),
                ]
                .spacing(5)
                .align_y(Alignment::Center),
            );
        }
        // Jogo registrado: mostra a capa ao lado do nome.
        let capa = j
            .capa
            .and_then(|id| avatar::handle_exibicao(&crate::api::capa_url(id)));
        let corpo: Element<'a, Msg> = match capa {
            Some(h) => row![
                iced::widget::image(h)
                    .width(48)
                    .height(68)
                    .content_fit(iced::ContentFit::Cover)
                    .border_radius(tema::RAIO),
                container(linha).width(Length::Fill),
            ]
            .spacing(12)
            .align_y(Alignment::Center)
            .into(),
            None => container(linha).width(Length::Fill).into(),
        };
        ("Jogando", corpo)
    } else if let Some(m) = &a.musica {
        let capa: Element<'a, Msg> = match m.arte.as_deref().and_then(avatar::handle_exibicao) {
            Some(h) => iced::widget::image(h)
                .width(64)
                .height(64)
                .content_fit(iced::ContentFit::Cover)
                .border_radius(tema::RAIO)
                .into(),
            None => container(ui::efeitos::ico(icones::musica(), 26.0, c.texto_fraco))
                .width(64)
                .height(64)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center)
                .style(move |_| container::Style {
                    background: Some(Background::Color(c.elevado)),
                    border: Border {
                        radius: tema::RAIO.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .into(),
        };
        let mut info = column![text(m.titulo.clone()).size(14).font(negrito())].spacing(2);
        if let Some(ar) = m.artista.as_deref().filter(|x| !x.is_empty()) {
            info = info.push(text(ar.to_string()).size(12));
        }
        if let Some(al) = m.album.as_deref().filter(|x| !x.is_empty()) {
            info = info.push(text(al.to_string()).size(12).style(fraco));
        }
        // Progresso: só quando o player informou posição e duração.
        let progresso: Option<Element<'a, Msg>> = match (m.inicio, m.duracao) {
            (Some(ini), Some(dur)) => {
                let agora = chrono::Utc::now().timestamp_millis().max(0) as u64;
                let pos = agora.saturating_sub(ini).min(dur);
                Some(
                    row![
                        text(mmss(pos)).size(11).style(fraco),
                        iced::widget::progress_bar(0.0..=dur as f32, pos as f32)
                            .girth(4)
                            .style(move |_| iced::widget::progress_bar::Style {
                                background: Background::Color(c.elevado),
                                bar: Background::Color(c.destaque),
                                border: Border {
                                    radius: 2.0.into(),
                                    ..Default::default()
                                },
                            }),
                        text(mmss(dur)).size(11).style(fraco),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .into(),
                )
            }
            _ => None,
        };
        let mut corpo_musica = column![
            row![capa, container(info).width(Length::Fill)]
                .spacing(12)
                .align_y(Alignment::Start)
        ]
        .spacing(10);
        if let Some(p) = progresso {
            corpo_musica = corpo_musica.push(p);
        }
        ("Ouvindo", corpo_musica.into())
    } else {
        return None;
    };
    Some(
        container(column![text(rotulo).size(12).style(fraco), corpo].spacing(10))
            .padding(12)
            .width(Length::Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(c.fundo)),
                border: Border {
                    radius: tema::RAIO_GRANDE.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into(),
    )
}

fn status_de<'a>(m: &'a Membro, avisos: &'a EstadoAvisos) -> &'a str {
    avisos
        .status_amigos
        .get(&m.uid)
        .map(String::as_str)
        .or(m.status.as_deref())
        .unwrap_or("offline")
}

/// Membros ordenados: quem está por aqui primeiro, offline no fim; a ordem
/// original (de entrada no grupo) se mantém dentro de cada bloco.
pub fn ordena_membros<'a>(membros: &'a [Membro], avisos: &EstadoAvisos) -> Vec<&'a Membro> {
    let mut v: Vec<&Membro> = membros.iter().collect();
    v.sort_by_key(|m| status_de(m, avisos) == "offline");
    v
}

fn linha_de_membro<'a>(
    m: &'a Membro,
    dono: bool,
    em_chamada: bool,
    avisos: &'a EstadoAvisos,
) -> Element<'a, Msg> {
    let c = tema::cores();
    let st = status_de(m, avisos);
    let offline = st == "offline";
    // Offline bem apagado, como no Discord (lá a linha inteira fica a 30%).
    let fundo_painel = tema::mistura(c.lateral, c.fundo, 0.45);
    let apagado = tema::mistura(c.texto_fraco, fundo_painel, 0.45);
    let sub: Element<'a, Msg> = if em_chamada {
        text("Em chamada").size(12).color(c.sucesso_forte).into()
    } else {
        ui::status::sub_widget(
            &ui::status::Sub::de_pessoa(
                m.status_custom.as_ref(),
                atividade_de(m, avisos).as_ref(),
                ui::status::rotulo_status(st),
            ),
            12.0,
            if offline { apagado } else { c.texto_fraco },
        )
    };
    let mut nome = row![ui::nome::nome_de(
        &m.uid,
        &m.name,
        14.0,
        if offline { apagado } else { c.texto },
        negrito()
    )]
    .spacing(6)
    .align_y(Alignment::Center);
    if dono {
        nome = nome.push(ui::moderacao::badge_dono());
    }
    let alvo = m.usuario.clone().unwrap_or_else(|| m.name.clone());
    let foto = ui::avatar::avatar(
        m.avatar.as_deref(),
        &m.uid,
        &m.name,
        32.0,
        Some(st),
        fundo_painel,
    );
    let foto: Element<'a, Msg> = if offline {
        let veu = container(space().width(32).height(32)).style(move |_| container::Style {
            background: Some(Background::Color(iced::Color {
                a: 0.6,
                ..fundo_painel
            })),
            border: iced::Border {
                radius: 16.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });
        iced::widget::stack![foto, veu].into()
    } else {
        foto
    };
    let conteudo = row![foto, column![nome, sub].spacing(1).width(Length::Fill),]
        .spacing(10)
        .align_y(Alignment::Center);
    let id_hover = format!("memb-{}", m.uid);
    let corpo: Element<'a, Msg> = crate::ui::loja::com_placa(
        container(conteudo).padding([6, 8]).into(),
        &m.uid,
        crate::anim::dentro_desde(&id_hover),
        offline,
        fundo_painel,
    );
    let btn = button(corpo)
        .width(Length::Fill)
        .padding(0)
        .style(crate::ui::efeitos::item_suave(id_hover.clone(), false))
        .on_press(Msg::AbrePerfilUsuario(alvo.clone()));
    iced::widget::mouse_area(btn)
        .on_press(Msg::AbrePerfilUsuario(alvo))
        .on_enter(Msg::Hover(id_hover.clone(), true))
        .on_exit(Msg::Hover(id_hover, false))
        .on_right_press(Msg::AbreMenuPessoa {
            uid: m.uid.clone(),
            usuario: m.usuario.clone(),
            nome: m.name.clone(),
        })
        .into()
}

fn lista_de_membros<'a>(info: &InfoConversa<'a>, avisos: &'a EstadoAvisos) -> Element<'a, Msg> {
    let c = tema::cores();
    let conv = info.conversa;
    let dentro = avisos.chamadas.get(&conv.id);
    let mut col = Column::new().spacing(2).padding([16, 8]);
    col = col.push(
        container(
            text(format!("MEMBROS — {}", conv.membros.len()))
                .size(tema::TEXTO_ROTULO)
                .font(negrito())
                .style(fraco),
        )
        .padding([4, 8]),
    );
    for m in ordena_membros(&conv.membros, avisos) {
        let dono = conv.dono.as_deref() == Some(m.uid.as_str());
        let em_chamada = dentro.is_some_and(|d| d.contains(&m.uid));
        col = col.push(linha_de_membro(m, dono, em_chamada, avisos));
    }
    col = col.push(
        button(
            row![
                svg(icones::usuario_mais())
                    .width(16)
                    .height(16)
                    .style(tema::icone_cor(c.texto)),
                text("Convidar para grupo privado").size(14),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding([8, 12])
        .style(tema::botao_secundario)
        .on_press(Msg::AbreAdicionarPessoas(conv.id.clone())),
    );
    scrollable(col).style(tema::rolagem).into()
}

fn cartao<'a>(titulo: &'a str, corpo: String) -> Element<'a, Msg> {
    let c = tema::cores();
    container(
        column![
            text(titulo.to_uppercase())
                .size(tema::TEXTO_ROTULO)
                .font(negrito())
                .style(fraco),
            text(corpo).size(14),
        ]
        .spacing(4),
    )
    .padding(12)
    .width(Length::Fill)
    .style(move |_| container::Style {
        background: Some(Background::Color(c.fundo)),
        border: Border {
            radius: tema::RAIO_GRANDE.into(),
            ..Default::default()
        },
        ..Default::default()
    })
    .into()
}

fn perfil_da_conversa_direta<'a>(m: &'a Membro, avisos: &'a EstadoAvisos) -> Element<'a, Msg> {
    let c = tema::cores();
    let st = status_de(m, avisos);
    let cor_banner = tema::cor_de_avatar(&m.uid);
    let banner: Element<'a, Msg> = match m.banner_url.as_deref().and_then(avatar::handle_exibicao) {
        Some(h) => image(h)
            .width(Length::Fill)
            .height(100)
            .content_fit(ContentFit::Cover)
            .into(),
        None => container(space())
            .width(Length::Fill)
            .height(100)
            .style(move |_| container::Style {
                background: Some(Background::Color(cor_banner)),
                ..Default::default()
            })
            .into(),
    };
    let alvo = m.usuario.clone().unwrap_or_else(|| m.name.clone());
    let escolha = crate::cosmeticos::de(&m.uid);
    let avatar_btn = button(ui::avatar::avatar_com(
        m.avatar.as_deref(),
        &m.uid,
        &m.name,
        80.0,
        Some(st),
        c.lateral,
        escolha.decoracao.as_ref().map(|x| x.arquivo.as_str()),
        true,
    ))
    .padding(0)
    .style(tema::botao_transparente)
    .on_press(Msg::AbrePerfilUsuario(alvo.clone()));

    // A foto de 80px sobrepõe a base do banner.
    let topo = stack![
        column![banner, space().height(40)],
        container(avatar_btn).padding(iced::Padding {
            top: 60.0,
            left: 16.0,
            ..Default::default()
        }),
    ];
    let mut corpo = Column::new().spacing(12).padding([8, 16]);
    let nome_btn = button(ui::nome::nome(
        &m.name,
        escolha.nome.as_ref(),
        20.0,
        c.texto,
        negrito(),
    ))
    .padding(0)
    .style(move |_, st| button::Style {
        text_color: if matches!(st, button::Status::Hovered) {
            c.destaque
        } else {
            c.texto
        },
        ..Default::default()
    })
    .on_press(Msg::AbrePerfilUsuario(alvo.clone()));
    corpo = corpo.push(nome_btn);
    if let Some(u) = &m.usuario {
        corpo = corpo.push(text(format!("@{u}")).size(14).style(fraco));
    }
    if let Some(sc) = m.status_custom.as_ref().filter(|s| !s.vencido()) {
        corpo = corpo.push(
            row![
                text(sc.emoji.clone())
                    .size(14)
                    .font(crate::tema::FONTE_EMOJI),
                text(sc.texto.clone()).size(13).style(fraco),
            ]
            .spacing(6),
        );
    }
    if let Some(bloco) = atividade_de(m, avisos).as_ref().and_then(bloco_atividade) {
        corpo = corpo.push(bloco);
    }
    corpo = corpo.push(cartao(
        "Sobre mim",
        m.bio
            .clone()
            .filter(|b| !b.trim().is_empty())
            .unwrap_or_else(|| "Sem descrição.".into()),
    ));
    let fundo = crate::ui::loja::fundo_do_cartao(&escolha);
    let com_tema = escolha.tema.is_some();
    let perfil: Element<'a, Msg> = container(crate::ui::loja::com_efeito(
        column![topo, corpo].into(),
        &escolha,
    ))
    .style(move |_| container::Style {
        background: com_tema.then_some(fundo),
        ..Default::default()
    })
    .into();
    let perfil = crate::ui::loja::com_moldura(perfil, &escolha);
    column![
        scrollable(perfil).height(Length::Fill).style(tema::rolagem),
        container(
            button(
                text("Ver Perfil Completo")
                    .size(14)
                    .width(Length::Fill)
                    .align_x(Alignment::Center)
            )
            .width(Length::Fill)
            .padding([10, 12])
            .style(tema::botao_secundario)
            .on_press(Msg::AbrePerfilUsuario(alvo))
        )
        .padding(12),
    ]
    .into()
}

/// O painel completo, já com largura e fundo.
pub fn painel_direito<'a>(info: &InfoConversa<'a>, avisos: &'a EstadoAvisos) -> Element<'a, Msg> {
    let conteudo: Element<'a, Msg> = if info.conversa.tipo == "grupo" {
        lista_de_membros(info, avisos)
    } else {
        let outro = info
            .conversa
            .membros
            .iter()
            .find(|m| m.usuario.as_deref() != info.meu_usuario);
        match outro {
            Some(m) => perfil_da_conversa_direta(m, avisos),
            None => space().into(),
        }
    };
    container(conteudo)
        .width(248)
        .height(Length::Fill)
        .style(tema::painel_membros)
        .into()
}

#[cfg(test)]
mod testes {
    #[test]
    fn atividade_limpa_nao_volta_do_retrato_da_conversa() {
        use crate::avisos::{EstadoAvisos, Evento, Membro};
        let mut st = EstadoAvisos::default();
        let musica = serde_json::json!({ "musica": { "titulo": "Sonora" } });
        let membro = Membro {
            uid: "local:1".into(),
            atividade: Some(musica.clone()),
            ..Default::default()
        };
        // Sem evento nenhum, vale o retrato da conversa.
        assert!(atividade_de(&membro, &st).is_some());
        // Avisaram que parou: o retrato velho não pode ressuscitar a música.
        st.aplica(&Evento::AtividadeAmigo {
            uid: "local:1".into(),
            atividade: None,
        });
        assert!(atividade_de(&membro, &st).is_none());
        // E uma atividade nova volta a valer.
        st.aplica(&Evento::AtividadeAmigo {
            uid: "local:1".into(),
            atividade: Some(musica),
        });
        assert!(atividade_de(&membro, &st).is_some());
    }

    use super::*;

    fn m(uid: &str, st: &str) -> Membro {
        Membro {
            uid: uid.into(),
            status: Some(st.into()),
            ..Default::default()
        }
    }

    #[test]
    fn offline_vai_para_o_fim_mantendo_a_ordem() {
        let membros = vec![
            m("a", "offline"),
            m("b", "online"),
            m("c", "offline"),
            m("d", "ausente"),
        ];
        let av = EstadoAvisos::default();
        let uids: Vec<&str> = ordena_membros(&membros, &av)
            .iter()
            .map(|x| x.uid.as_str())
            .collect();
        assert_eq!(uids, vec!["b", "d", "a", "c"]);
    }

    #[test]
    fn status_do_canal_vale_mais_que_o_da_lista() {
        let membros = vec![m("a", "offline"), m("b", "online")];
        let mut av = EstadoAvisos::default();
        av.status_amigos.insert("a".into(), "online".into());
        av.status_amigos.insert("b".into(), "offline".into());
        let uids: Vec<&str> = ordena_membros(&membros, &av)
            .iter()
            .map(|x| x.uid.as_str())
            .collect();
        assert_eq!(uids, vec!["a", "b"]);
    }
}
