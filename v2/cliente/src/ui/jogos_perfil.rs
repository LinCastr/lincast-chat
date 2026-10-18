//! Listas de jogos do perfil: favorito, "jogos que eu gosto" e "quero jogar".
//! Escolhidas à mão (busca no SteamGridDB feita pelo servidor), com capa.

use iced::widget::{Column, Row, button, column, container, row, space, stack, text, text_input};
use iced::{Alignment, Background, Border, ContentFit, Element, Length};

use crate::api::{JogoLista, ListasJogos, MAX_JOGOS_POR_LISTA, capa_url};
use crate::ui::efeitos::ico;
use crate::{Msg, fraco, icones, negrito, tema};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListaJogo {
    Favorito,
    Gosto,
    Quero,
}

impl ListaJogo {
    pub fn titulo(self) -> &'static str {
        match self {
            Self::Favorito => "Jogo favorito",
            Self::Gosto => "Jogos que eu gosto",
            Self::Quero => "Quero jogar",
        }
    }
}

/// Estado do editor (busca e resultados) dentro da janela de editar perfil.
#[derive(Debug, Clone, Default)]
pub struct EdicaoJogos {
    pub listas: ListasJogos,
    pub busca: String,
    pub resultados: Vec<JogoLista>,
    pub buscando: bool,
    pub erro: Option<String>,
}

impl EdicaoJogos {
    /// Põe `jogo` em `lista`, respeitando o limite e sem repetir.
    pub fn adiciona(&mut self, jogo: JogoLista, lista: ListaJogo) -> Result<(), &'static str> {
        match lista {
            ListaJogo::Favorito => self.listas.favorito = Some(jogo),
            ListaJogo::Gosto | ListaJogo::Quero => {
                let alvo = if lista == ListaJogo::Gosto {
                    &mut self.listas.gosto
                } else {
                    &mut self.listas.quero
                };
                if alvo.iter().any(|j| j.id == jogo.id) {
                    return Err("Esse jogo já está na lista.");
                }
                if alvo.len() >= MAX_JOGOS_POR_LISTA {
                    return Err("A lista já tem 20 jogos.");
                }
                alvo.push(jogo);
            }
        }
        Ok(())
    }

    pub fn remove(&mut self, lista: ListaJogo, id: u64) {
        match lista {
            ListaJogo::Favorito => self.listas.favorito = None,
            ListaJogo::Gosto => self.listas.gosto.retain(|j| j.id != id),
            ListaJogo::Quero => self.listas.quero.retain(|j| j.id != id),
        }
    }
}

const LARG: f32 = 72.0;
const ALT: f32 = 108.0;

fn capa<'a>(j: &'a JogoLista) -> Element<'a, Msg> {
    let c = tema::cores();
    match crate::avatar::handle_exibicao(&capa_url(j.id)) {
        Some(h) => iced::widget::image(h)
            .width(LARG)
            .height(ALT)
            .content_fit(ContentFit::Cover)
            .border_radius(tema::RAIO_GRANDE)
            .into(),
        None => container(text(j.nome.as_str()).size(11).font(negrito()).center())
            .width(LARG)
            .height(ALT)
            .padding(6)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .style(move |_| container::Style {
                background: Some(Background::Color(c.elevado)),
                border: Border {
                    color: c.borda,
                    width: 1.0,
                    radius: tema::RAIO_GRANDE.into(),
                },
                ..Default::default()
            })
            .into(),
    }
}

fn tile<'a>(j: &'a JogoLista, lista: ListaJogo, editavel: bool) -> Element<'a, Msg> {
    let c = tema::cores();
    let capa_el = capa(j);
    let capa_com_dica: Element<'a, Msg> = iced::widget::tooltip(
        capa_el,
        container(text(j.nome.as_str()).size(12).font(negrito()))
            .padding([5, 9])
            .style(tema::dica),
        iced::widget::tooltip::Position::Top,
    )
    .gap(6)
    .into();

    if editavel {
        let x = button(ico(icones::fechar(), 12.0, iced::Color::WHITE))
            .padding(3)
            .style(move |_, st| button::Style {
                background: Some(Background::Color(
                    if matches!(st, button::Status::Hovered) {
                        c.perigo
                    } else {
                        iced::Color::from_rgba(0.0, 0.0, 0.0, 0.75)
                    },
                )),
                border: Border {
                    radius: 10.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .on_press(Msg::JogoRemove(lista, j.id));
        stack![
            capa_com_dica,
            container(x).width(LARG).align_x(Alignment::End).padding(3)
        ]
        .into()
    } else {
        capa_com_dica
    }
}

fn secao<'a>(
    lista: ListaJogo,
    jogos: &'a [JogoLista],
    editavel: bool,
    vazio: &'static str,
) -> Element<'a, Msg> {
    let mut cab = row![text(lista.titulo()).size(13).font(negrito())]
        .spacing(8)
        .align_y(Alignment::Center);
    if lista != ListaJogo::Favorito {
        cab = cab.push(
            text(format!("{}/{}", jogos.len(), MAX_JOGOS_POR_LISTA))
                .size(11)
                .style(fraco),
        );
    }
    let mut col = column![cab].spacing(10);
    if jogos.is_empty() {
        col = col.push(text(vazio).size(12).style(fraco));
    } else {
        let mut linha = Row::new().spacing(10);
        for j in jogos {
            linha = linha.push(tile(j, lista, editavel));
        }
        col = col.push(linha.wrap().vertical_spacing(12));
    }
    col.into()
}

/// As três listas, para ver (perfil de alguém) ou editar (o meu).
pub fn blocos<'a>(listas: &'a ListasJogos, editavel: bool) -> Element<'a, Msg> {
    let c = tema::cores();
    let mut col = Column::new().spacing(18);
    let fav_el: Element<'a, Msg> = match &listas.favorito {
        Some(j) => {
            let mut info = column![
                text("Jogo favorito").size(12).style(fraco),
                text(j.nome.as_str()).size(16).font(negrito()),
            ]
            .spacing(4)
            .width(Length::Fill);
            if editavel {
                info = info.push(
                    button(text("Remover dos favoritos").size(11))
                        .padding([4, 8])
                        .style(tema::botao_secundario)
                        .on_press(Msg::JogoRemove(ListaJogo::Favorito, j.id)),
                );
            }
            container(row![capa(j), info,].spacing(14).align_y(Alignment::Center))
                .padding(12)
                .width(Length::Fill)
                .style(move |_| container::Style {
                    background: Some(Background::Color(c.elevado)),
                    border: Border {
                        color: c.borda,
                        width: 1.0,
                        radius: tema::RAIO_GRANDE.into(),
                    },
                    ..Default::default()
                })
                .into()
        }
        None => column![
            text(ListaJogo::Favorito.titulo()).size(13).font(negrito()),
            text(if editavel {
                "Escolha 1 jogo na busca abaixo."
            } else {
                "Nenhum jogo favorito."
            })
            .size(12)
            .style(fraco),
        ]
        .spacing(8)
        .into(),
    };
    col = col.push(fav_el);
    col = col.push(secao(
        ListaJogo::Gosto,
        &listas.gosto,
        editavel,
        if editavel {
            "Adicione jogos pela busca abaixo."
        } else {
            "Nenhum jogo ainda."
        },
    ));
    col = col.push(secao(
        ListaJogo::Quero,
        &listas.quero,
        editavel,
        if editavel {
            "Adicione jogos pela busca abaixo."
        } else {
            "Nenhum jogo ainda."
        },
    ));
    col.into()
}

/// Editor: as listas com "×" e a busca por nome com botões para escolher a lista.
pub fn editor<'a>(ed: &'a EdicaoJogos) -> Element<'a, Msg> {
    let c = tema::cores();
    let busca = row![
        text_input("Buscar um jogo pelo nome…", &ed.busca)
            .on_input(Msg::JogosBuscaInput)
            .on_submit(Msg::JogosBusca)
            .padding([7, 10])
            .size(13)
            .style(tema::campo),
        button(text(if ed.buscando { "Buscando…" } else { "Buscar" }).size(13))
            .padding([7, 14])
            .style(tema::botao_primario)
            .on_press_maybe(
                (!ed.buscando && ed.busca.trim().len() >= 2).then_some(Msg::JogosBusca)
            ),
    ]
    .spacing(8);
    let mut col = column![
        text("JOGOS NO PERFIL")
            .size(tema::TEXTO_ROTULO)
            .font(negrito())
            .style(fraco),
        blocos(&ed.listas, true),
        space().height(4),
        busca,
    ]
    .spacing(12);
    if let Some(e) = &ed.erro {
        col = col.push(text(e.as_str()).size(12).style(crate::texto_perigo));
    }
    for j in &ed.resultados {
        let botao = |rotulo: &'static str, l: ListaJogo| {
            button(text(rotulo).size(11))
                .padding([4, 8])
                .style(tema::botao_secundario)
                .on_press(Msg::JogoAdiciona(j.clone(), l))
        };
        col = col.push(
            container(
                row![
                    text(j.nome.as_str()).size(13).width(Length::Fill),
                    botao("Favorito", ListaJogo::Favorito),
                    botao("Gosto", ListaJogo::Gosto),
                    botao("Quero jogar", ListaJogo::Quero),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .padding([6, 10])
            .style(move |_| container::Style {
                background: Some(Background::Color(c.lateral)),
                border: Border {
                    color: c.borda,
                    width: 1.0,
                    radius: tema::RAIO.into(),
                },
                ..Default::default()
            }),
        );
    }
    col.into()
}

#[cfg(test)]
mod testes {
    use super::*;

    fn j(id: u64) -> JogoLista {
        JogoLista {
            id,
            nome: format!("Jogo {id}"),
        }
    }

    #[test]
    fn adiciona_sem_repetir_e_respeita_o_limite() {
        let mut e = EdicaoJogos::default();
        assert!(e.adiciona(j(1), ListaJogo::Gosto).is_ok());
        assert!(e.adiciona(j(1), ListaJogo::Gosto).is_err());
        for i in 2..=20 {
            e.adiciona(j(i), ListaJogo::Gosto).unwrap();
        }
        assert!(e.adiciona(j(99), ListaJogo::Gosto).is_err());
        assert_eq!(e.listas.gosto.len(), 20);
    }

    #[test]
    fn favorito_troca_e_remove() {
        let mut e = EdicaoJogos::default();
        e.adiciona(j(1), ListaJogo::Favorito).unwrap();
        e.adiciona(j(2), ListaJogo::Favorito).unwrap();
        assert_eq!(e.listas.favorito.as_ref().map(|x| x.id), Some(2));
        e.remove(ListaJogo::Favorito, 2);
        assert!(e.listas.favorito.is_none());
    }
}
