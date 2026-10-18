//! Janela "Editar grupo" (C7): nome e ícone, em três passos.

use iced::widget::{
    Column, button, column, container, image, row, scrollable, space, stack, svg, text, text_input,
};
use iced::{Alignment, Background, Border, Color, ContentFit, Element, Length};

use crate::api::ItemGif;
use crate::avisos::Conversa;
use crate::{Msg, avatar, fraco, icones, negrito, tema, texto_perigo, ui};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Passo {
    Principal,
    MudarIcone,
    BuscaGif,
}

/// O que a pessoa escolheu para o ícone, ainda não salvo.
#[derive(Debug, Clone)]
pub enum IconeNovo {
    Imagem(Vec<u8>),
    Gif(String),
    /// Voltar para as fotos dos membros.
    Remover,
}

#[derive(Debug, Clone)]
pub struct EstadoEditarGrupo {
    pub conversa: String,
    pub nome: String,
    pub nome_original: String,
    pub icone_original: Option<String>,
    pub novo_icone: Option<IconeNovo>,
    /// A imagem escolhida já como `Handle`, para não recalcular a cada quadro.
    pub previa_imagem: Option<iced::widget::image::Handle>,
    pub passo: Passo,
    pub busca_gif: String,
    pub resultados_gif: Vec<ItemGif>,
    pub buscando_gif: bool,
    pub enviando: bool,
    pub erro: Option<String>,
}

impl EstadoEditarGrupo {
    pub fn novo(c: &Conversa) -> Self {
        Self {
            conversa: c.id.clone(),
            nome: c.nome.clone(),
            nome_original: c.nome.clone(),
            icone_original: c.icone.clone(),
            novo_icone: None,
            previa_imagem: None,
            passo: Passo::Principal,
            busca_gif: String::new(),
            resultados_gif: Vec::new(),
            buscando_gif: false,
            enviando: false,
            erro: None,
        }
    }

    /// "Salvar" só liga com mudança de verdade.
    pub fn mudou(&self) -> bool {
        let nome = self.nome.trim();
        (!nome.is_empty() && nome != self.nome_original) || self.novo_icone.is_some()
    }

    /// Há um ícone escolhido (atual ou novo) que o botão "voltar para as fotos" remove.
    pub fn tem_icone(&self) -> bool {
        match &self.novo_icone {
            Some(IconeNovo::Remover) => false,
            Some(_) => true,
            None => self.icone_original.is_some(),
        }
    }
}

fn cartao_modal<'a>(conteudo: Element<'a, Msg>) -> Element<'a, Msg> {
    let c = tema::cores();
    let cartao = container(conteudo)
        .padding(20)
        .width(440)
        .style(move |_| container::Style {
            background: Some(Background::Color(c.fundo)),
            border: Border {
                color: c.borda,
                width: 1.0,
                radius: tema::RAIO_GRANDE.into(),
            },
            ..Default::default()
        });
    container(cartao)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(|_| container::Style {
            background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.65))),
            ..Default::default()
        })
        .into()
}

fn cabecalho<'a>(titulo: &'a str, voltar: Option<Msg>) -> Element<'a, Msg> {
    let mut r = row![].spacing(8).align_y(Alignment::Center);
    if let Some(v) = voltar {
        r = r.push(crate::ui::efeitos::com_dica(
            crate::botao_icone(icones::esquerda(), false, false, Some(v)),
            "Voltar",
            iced::widget::tooltip::Position::Bottom,
        ));
    }
    r = r.push(text(titulo).size(20).font(negrito()));
    r = r.push(space().width(Length::Fill));
    r = r.push(crate::ui::efeitos::com_dica(
        crate::botao_icone(icones::fechar(), false, false, Some(Msg::EditaGrupoCancela)),
        "Fechar",
        iced::widget::tooltip::Position::Bottom,
    ));
    r.into()
}

/// O ícone grande: imagem/GIF escolhido, o atual, ou as fotos dos membros.
fn icone_grande<'a>(
    est: &'a EstadoEditarGrupo,
    conv: Option<&'a Conversa>,
    meu: Option<&str>,
) -> Element<'a, Msg> {
    let c = tema::cores();
    let tam = 112.0;
    let redondo = |h: iced::widget::image::Handle| -> Element<'a, Msg> {
        image(h)
            .width(tam)
            .height(tam)
            .content_fit(ContentFit::Cover)
            .border_radius(tam / 2.0)
            .into()
    };
    match &est.novo_icone {
        Some(IconeNovo::Imagem(_)) => {
            if let Some(h) = &est.previa_imagem {
                return redondo(h.clone());
            }
        }
        Some(IconeNovo::Gif(u)) => {
            if let Some(h) = avatar::handle(u) {
                return redondo(h);
            }
        }
        Some(IconeNovo::Remover) => {}
        None => {
            if let Some(h) = est.icone_original.as_deref().and_then(avatar::handle) {
                return redondo(h);
            }
        }
    }
    let membros: Vec<ui::avatar::MembroDoIcone<'_>> = conv
        .map(|c| {
            c.membros
                .iter()
                .filter(|m| m.usuario.as_deref() != meu)
                .map(|m| ui::avatar::MembroDoIcone {
                    chave: &m.uid,
                    nome: &m.name,
                    url: m.avatar.as_deref(),
                })
                .collect()
        })
        .unwrap_or_default();
    ui::avatar::icone_grupo(None, &membros, tam, c.fundo)
}

pub fn modal<'a>(
    est: &'a EstadoEditarGrupo,
    conv: Option<&'a Conversa>,
    meu_usuario: Option<&'a str>,
    gif_ativo: bool,
) -> Element<'a, Msg> {
    let c = tema::cores();
    let corpo: Element<'a, Msg> = match est.passo {
        Passo::Principal => {
            let icone = button(stack![
                container(icone_grande(est, conv, meu_usuario)).padding(6),
                container(
                    container(
                        svg(icones::lapis())
                            .width(16)
                            .height(16)
                            .style(tema::icone_cor(c.texto))
                    )
                    .padding(9)
                    .style(move |_| container::Style {
                        background: Some(Background::Color(c.forte)),
                        border: Border {
                            color: c.fundo,
                            width: 3.0,
                            radius: 20.0.into(),
                        },
                        ..Default::default()
                    })
                )
                .width(Length::Fill)
                .align_x(Alignment::End),
            ])
            .padding(0)
            .style(tema::botao_transparente)
            .on_press(Msg::EditaGrupoPasso(Passo::MudarIcone));
            let mut col = column![
                cabecalho("Editar grupo", None),
                container(icone).center_x(Length::Fill),
            ]
            .spacing(14);
            if est.tem_icone() {
                col = col.push(
                    container(
                        button(text("Voltar para as fotos dos membros").size(13))
                            .padding([4, 8])
                            .style(tema::botao_secundario)
                            .on_press(Msg::EditaGrupoRemoveIcone),
                    )
                    .center_x(Length::Fill),
                );
            }
            col = col.push(
                text_input("Nome do grupo", &est.nome)
                    .on_input(Msg::EditaGrupoNome)
                    .padding([8, 10])
                    .size(14)
                    .style(tema::campo),
            );
            if let Some(e) = &est.erro {
                col = col.push(text(e.clone()).size(13).style(texto_perigo));
            }
            let salvar = button(text("Salvar").size(14))
                .padding([8, 16])
                .width(Length::FillPortion(1))
                .style(tema::botao_primario)
                .on_press_maybe((est.mudou() && !est.enviando).then_some(Msg::EditaGrupoSalva));
            col = col.push(
                row![
                    button(text("Cancelar").size(14))
                        .padding([8, 16])
                        .width(Length::FillPortion(1))
                        .style(tema::botao_secundario)
                        .on_press(Msg::EditaGrupoCancela),
                    salvar,
                ]
                .spacing(8),
            );
            col.into()
        }
        Passo::MudarIcone => {
            let tile =
                |icone: iced::widget::svg::Handle, rotulo: &'static str, msg: Option<Msg>| {
                    button(
                        column![
                            svg(icone)
                                .width(32)
                                .height(32)
                                .style(tema::icone_cor(c.texto)),
                            text(rotulo).size(14).font(negrito()),
                        ]
                        .spacing(10)
                        .align_x(Alignment::Center),
                    )
                    .width(Length::FillPortion(1))
                    .padding([28, 12])
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
                            radius: tema::RAIO_GRANDE.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .on_press_maybe(msg)
                };
            column![
                cabecalho("Mudar ícone", Some(Msg::EditaGrupoPasso(Passo::Principal))),
                row![
                    tile(
                        icones::arquivo(),
                        "Enviar imagem",
                        Some(Msg::EditaGrupoEscolheImagem)
                    ),
                    tile(
                        icones::grade(),
                        "Escolher GIF",
                        gif_ativo.then_some(Msg::EditaGrupoPasso(Passo::BuscaGif))
                    ),
                ]
                .spacing(10),
            ]
            .spacing(16)
            .into()
        }
        Passo::BuscaGif => {
            let mut grade = Column::new().spacing(6);
            let mut linha = row![].spacing(6);
            let mut n = 0;
            for g in &est.resultados_gif {
                let miolo: Element<'a, Msg> = match avatar::handle(&g.previa) {
                    Some(h) => image(h)
                        .width(Length::Fill)
                        .height(96)
                        .content_fit(ContentFit::Cover)
                        .into(),
                    None => container(text("…").size(12).style(fraco))
                        .width(Length::Fill)
                        .height(96)
                        .center_x(Length::Fill)
                        .center_y(96)
                        .into(),
                };
                linha = linha.push(
                    button(miolo)
                        .width(Length::FillPortion(1))
                        .padding(0)
                        .style(tema::botao_secundario)
                        .on_press(Msg::EditaGrupoEscolheGif(g.url.clone())),
                );
                n += 1;
                if n % 3 == 0 {
                    grade = grade.push(linha);
                    linha = row![].spacing(6);
                }
            }
            if n % 3 != 0 {
                for _ in 0..(3 - n % 3) {
                    linha = linha.push(space().width(Length::FillPortion(1)));
                }
                grade = grade.push(linha);
            }
            let estado: Element<'a, Msg> = if est.buscando_gif {
                text("Buscando GIFs…").size(13).style(fraco).into()
            } else if let Some(e) = &est.erro {
                text(e.clone()).size(13).style(texto_perigo).into()
            } else if est.resultados_gif.is_empty() {
                text("Digite algo e aperte Enter.")
                    .size(13)
                    .style(fraco)
                    .into()
            } else {
                space().height(0).into()
            };
            column![
                cabecalho(
                    "Escolher GIF",
                    Some(Msg::EditaGrupoPasso(Passo::MudarIcone))
                ),
                text_input("Buscar GIF", &est.busca_gif)
                    .on_input(Msg::EditaGrupoBuscaInput)
                    .on_submit(Msg::EditaGrupoBusca)
                    .padding([8, 10])
                    .size(14)
                    .style(tema::campo),
                estado,
                container(scrollable(grade).style(tema::rolagem)).height(280),
            ]
            .spacing(12)
            .into()
        }
    };
    cartao_modal(corpo)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn conv() -> Conversa {
        Conversa {
            id: "g1".into(),
            tipo: "grupo".into(),
            nome: "Os Cria".into(),
            icone: Some("/api/grupos/g1/icone?v=ab".into()),
            ..Default::default()
        }
    }

    #[test]
    fn salvar_so_com_mudanca() {
        let mut e = EstadoEditarGrupo::novo(&conv());
        assert!(!e.mudou());
        e.nome = "  Os Cria  ".into();
        assert!(!e.mudou(), "espaços não contam como mudança");
        e.nome = "Novo".into();
        assert!(e.mudou());
        e.nome = "   ".into();
        assert!(!e.mudou(), "nome vazio não habilita salvar");
        e.novo_icone = Some(IconeNovo::Remover);
        assert!(e.mudou());
    }

    #[test]
    fn tem_icone_segue_a_escolha() {
        let mut e = EstadoEditarGrupo::novo(&conv());
        assert!(e.tem_icone());
        e.novo_icone = Some(IconeNovo::Remover);
        assert!(!e.tem_icone());
        e.novo_icone = Some(IconeNovo::Gif("https://media.giphy.com/x.gif".into()));
        assert!(e.tem_icone());
    }
}
