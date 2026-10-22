//! Módulo de layouts e densidade da interface gráfica do Lincast Chat.
//!
//! Implementa os 5 arranjos de chamada da v1 (Clássico, Chat ao lado, Vídeo cheio,
//! Lista à direita e Só chat) e 2 níveis de densidade visual (Confortável e Compacta).

use iced::widget::{Column, Row, button, column, text};
use iced::{Alignment, Element, Length};
use serde::{Deserialize, Serialize};

use crate::{Msg, fraco, negrito, rotulo, tema};

/// Densidade de espaçamento visual da interface do aplicativo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Densidade {
    #[default]
    Confortavel,
    Compacta,
}

impl Densidade {
    pub const TODAS: [Self; 2] = [Self::Confortavel, Self::Compacta];

    pub fn rotulo(self) -> &'static str {
        match self {
            Self::Confortavel => "Confortável",
            Self::Compacta => "Compacta",
        }
    }

    pub fn descricao(self) -> &'static str {
        match self {
            Self::Confortavel => "Espaçamentos normais, avatares 32px e barra lateral de 248px",
            Self::Compacta => "Mais mensagens e pessoas na tela: barra 208px e avatares 26px",
        }
    }

    /// Largura da barra lateral de participantes da sala em pixels.
    #[allow(dead_code)]
    pub fn largura_sidebar(self) -> f32 {
        match self {
            Self::Confortavel => 248.0,
            Self::Compacta => 208.0,
        }
    }

    /// Dimensão do diâmetro do avatar ou bolinha de status dos participantes.
    #[allow(dead_code)]
    pub fn tamanho_avatar_pessoa(self) -> f32 {
        match self {
            Self::Confortavel => 32.0,
            Self::Compacta => 26.0,
        }
    }

    /// Espaçamento vertical entre mensagens no histórico do chat.
    pub fn espacamento_mensagens(self) -> u32 {
        match self {
            // Mensagens seguidas quase coladas, como no Discord; o respiro
            // entre pessoas diferentes vem do padding do cabeçalho.
            Self::Confortavel => 2,
            Self::Compacta => 0,
        }
    }

    /// Padding interno de cada balão de mensagem do chat.
    #[allow(dead_code)]
    pub fn padding_mensagem(self) -> [u16; 2] {
        match self {
            Self::Confortavel => [4, 8],
            Self::Compacta => [2, 6],
        }
    }
}

/// Renderiza o seletor de densidade em formato de cartões interativos.
pub fn seletor_densidade<'a>(densidade_atual: Densidade) -> Element<'a, Msg> {
    let mut col = Column::new().spacing(6);
    col = col.push(rotulo("DENSIDADE VISUAL DA INTERFACE"));
    col = col.push(
        text("Ajusta o tamanho dos avatares, espaçamento de mensagens e largura da barra lateral.")
            .size(12)
            .style(fraco),
    );

    let mut linha = Row::new().spacing(8);
    for d in Densidade::TODAS {
        let selecionada = d == densidade_atual;
        linha = linha.push(
            button(
                column![
                    // Selecionado: sem estilo próprio, o texto herda a cor do botão
                    // primário (antes sumia: `destaque` sobre fundo de destaque).
                    {
                        let t = text(d.rotulo()).size(13).font(if selecionada {
                            negrito()
                        } else {
                            crate::tema::FONTE
                        });
                        if selecionada { t } else { t.style(fraco) }
                    },
                    {
                        let t = text(d.descricao()).size(11);
                        if selecionada { t } else { t.style(fraco) }
                    },
                ]
                .spacing(3)
                .align_x(Alignment::Start),
            )
            .width(Length::FillPortion(1))
            .padding([8, 12])
            .style(if selecionada {
                tema::botao_primario
            } else {
                tema::botao_secundario
            })
            .on_press(Msg::SelecionaDensidade(d)),
        );
    }

    col = col.push(linha);
    col.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn testa_larguras_e_dimensoes_densidade() {
        assert_eq!(Densidade::Confortavel.largura_sidebar(), 248.0);
        assert_eq!(Densidade::Compacta.largura_sidebar(), 208.0);
        assert!(
            Densidade::Compacta.tamanho_avatar_pessoa()
                < Densidade::Confortavel.tamanho_avatar_pessoa()
        );
        assert!(
            Densidade::Compacta.espacamento_mensagens()
                < Densidade::Confortavel.espacamento_mensagens()
        );
    }
}
