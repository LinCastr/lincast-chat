//! Seletor e modal de temas e aparência (U1).
//!
//! Direção visual: sóbrio, denso, hierarquia pelo texto, 3 chips de cores por tema,
//! sem gradientes berrantes ou sombras coloridas.

use iced::alignment::Alignment;
use iced::widget::{Column, Row, button, column, container, row, scrollable, space, svg, text};
use iced::{Color, Element, Length};

use crate::icones;
use crate::tema::{self, InfoTema};

#[derive(Debug, Clone, PartialEq)]
pub enum TemaMsg {
    Selecionar(String),
    #[allow(dead_code)]
    Abrir,
    #[allow(dead_code)]
    Fechar,
    RecarregarWallust,
}

/// Renderiza um chip circular/arredondado de amostra de cor.
pub fn chip_cor(cor: Color, tamanho: f32) -> Element<'static, TemaMsg> {
    container(space().width(tamanho).height(tamanho))
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(cor)),
            border: iced::Border {
                color: iced::Color::from_rgba(1.0, 1.0, 1.0, 0.12),
                width: 1.0,
                radius: iced::border::Radius::from(3.0),
            },
            ..Default::default()
        })
        .into()
}

/// Renderiza a trinca de chips de um tema (fundo, elevado/superfície, destaque).
pub fn amostra_chips(chips: &[Color; 3]) -> Element<'static, TemaMsg> {
    row![
        chip_cor(chips[0], 14.0),
        chip_cor(chips[1], 14.0),
        chip_cor(chips[2], 14.0),
    ]
    .spacing(4)
    .align_y(Alignment::Center)
    .into()
}

/// Linha de cartão de tema com nome, amostras e indicação de ativo.
pub fn cartao_tema(info: &InfoTema, eh_ativo: bool) -> Element<'static, TemaMsg> {
    let c = tema::cores();
    let id = info.id.clone();
    let nome = info.nome.clone();
    let eh_wallust = info.eh_wallust;
    let chips = info.chips;

    let mut info_col = Column::new().spacing(2);
    info_col = info_col.push(text(nome).size(13).font(if eh_ativo {
        crate::negrito()
    } else {
        crate::tema::FONTE
    }));

    if eh_wallust {
        info_col = info_col.push(
            text("Cores extraídas dinamicamente do wallpaper atual")
                .size(11)
                .style(crate::fraco),
        );
    }

    let conteudo = row![
        amostra_chips(&chips),
        info_col.width(Length::Fill),
        if eh_ativo {
            text("Ativo").size(12).font(crate::negrito())
        } else {
            text("").size(12)
        }
    ]
    .spacing(12)
    .align_y(Alignment::Center)
    .padding([8, 12]);

    button(conteudo)
        .width(Length::Fill)
        .style(move |_, status| {
            let fundo = if eh_ativo {
                Some(iced::Background::Color(tema::mistura(
                    c.fundo, c.destaque, 0.12,
                )))
            } else if status == button::Status::Hovered {
                Some(iced::Background::Color(c.elevado))
            } else {
                Some(iced::Background::Color(c.fundo))
            };

            let cor_borda = if eh_ativo { c.destaque } else { c.borda };

            button::Style {
                background: fundo,
                text_color: c.texto,
                border: iced::Border {
                    color: cor_borda,
                    width: 1.0,
                    radius: iced::border::Radius::from(6.0),
                },
                shadow: iced::Shadow::default(),
                snap: true,
            }
        })
        .on_press(TemaMsg::Selecionar(id))
        .into()
}

/// Modal flutuante de seleção de temas.
fn divisoria_horizontal<'a>() -> Element<'a, TemaMsg> {
    container(space().height(1)).style(tema::divisoria).into()
}

pub fn modal_seletor_temas(tema_atual: &str) -> Element<'static, TemaMsg> {
    let c = tema::cores();
    let temas = tema::lista_temas();

    let cabecalho = row![
        svg(icones::paleta())
            .width(18)
            .height(18)
            .style(tema::icone_cor(c.destaque)),
        column![
            text("Aparência e Temas").size(16).font(crate::negrito()),
            text("Escolha uma paleta de cores ou use as cores do seu sistema via Wallust.")
                .size(12)
                .style(crate::fraco),
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
        .on_press(TemaMsg::Fechar),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    let mut lista_cartoes = Column::new().spacing(6);

    for t in &temas {
        let ativo = t.id == tema_atual;
        lista_cartoes = lista_cartoes.push(cartao_tema(t, ativo));
    }

    let mut rodape = Row::new().spacing(8).align_y(Alignment::Center);

    if tema::wallust_disponivel() {
        rodape = rodape.push(
            button(text("Recarregar cores do Wallust").size(12))
                .padding([6, 12])
                .style(tema::botao_secundario)
                .on_press(TemaMsg::RecarregarWallust),
        );
    }

    rodape = rodape.push(space().width(Length::Fill));
    rodape = rodape.push(
        button(text("Fechar").size(13))
            .padding([6, 16])
            .style(tema::botao_primario)
            .on_press(TemaMsg::Fechar),
    );

    let corpo = column![
        cabecalho,
        divisoria_horizontal(),
        scrollable(lista_cartoes)
            .height(Length::Fixed(340.0))
            .style(tema::rolagem),
        divisoria_horizontal(),
        rodape,
    ]
    .spacing(12)
    .padding(16);

    container(corpo)
        .width(Length::Fixed(500.0))
        .style(tema::painel_lateral)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn testa_instanciacao_modal_temas() {
        let _modal = modal_seletor_temas("padrao");
    }

    #[test]
    fn testa_chips_amostra() {
        let cores = [
            tema::rgb(0x111111),
            tema::rgb(0x222222),
            tema::rgb(0x333333),
        ];
        let _elem = amostra_chips(&cores);
    }
}
