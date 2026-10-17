//! Indicador visual de qualidade da conexão por pessoa (M10).

use iced::border::Radius;
use iced::widget::{container, space, text, tooltip};
use iced::{Background, Border, Color, Element};

use crate::sala::{NivelConexao, QualidadeConexao};
use crate::tema;

// Sem consumidor até o menu de contexto (C5) e o palco da chamada (C8).
#[allow(dead_code)]
pub fn cor_da_qualidade(nivel: NivelConexao) -> Color {
    match nivel {
        NivelConexao::Boa => Color::from_rgb8(0x23, 0xa5, 0x5a), // Verde
        NivelConexao::Media => Color::from_rgb8(0xf0, 0xb2, 0x32), // Amarelo
        NivelConexao::Ruim => Color::from_rgb8(0xf2, 0x3f, 0x43), // Vermelho
        NivelConexao::Indefinido => Color::from_rgb8(0x72, 0x76, 0x7d), // Neutro/Cinza
    }
}

// Sem consumidor até o menu de contexto (C5) e o palco da chamada (C8).
#[allow(dead_code)]
pub fn bolinha_qualidade<'a, M: 'a>(qualidade: Option<&QualidadeConexao>) -> Element<'a, M> {
    let (cor, txt) = match qualidade {
        Some(q) => (cor_da_qualidade(q.nivel), q.texto()),
        None => (
            cor_da_qualidade(NivelConexao::Indefinido),
            "Medindo conexão...".to_string(),
        ),
    };

    let bolinha = container(space().width(8).height(8))
        .width(8)
        .height(8)
        .style(move |_| container::Style {
            background: Some(Background::Color(cor)),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: Radius::from(4.0),
            },
            ..Default::default()
        });

    tooltip(
        bolinha,
        container(text(txt).size(12))
            .padding([4, 8])
            .style(tema::painel_lateral),
        tooltip::Position::Top,
    )
    .gap(4)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sala::NivelConexao;

    #[test]
    fn texto_e_cores_qualidade() {
        let q_boa = QualidadeConexao {
            nivel: NivelConexao::Boa,
            perda: Some(0.005),
            rtt_ms: Some(45),
            ..Default::default()
        };
        assert_eq!(q_boa.texto(), "Conexão boa (perda 0.5%, ida e volta 45ms)");
        assert_eq!(
            cor_da_qualidade(NivelConexao::Boa),
            Color::from_rgb8(0x23, 0xa5, 0x5a)
        );

        let q_media = QualidadeConexao {
            nivel: NivelConexao::Media,
            perda: Some(0.03),
            rtt_ms: Some(180),
            ..Default::default()
        };
        assert_eq!(
            q_media.texto(),
            "Conexão instável (perda 3.0%, ida e volta 180ms)"
        );
        assert_eq!(
            cor_da_qualidade(NivelConexao::Media),
            Color::from_rgb8(0xf0, 0xb2, 0x32)
        );

        let q_ruim = QualidadeConexao {
            nivel: NivelConexao::Ruim,
            perda: Some(0.12),
            rtt_ms: Some(400),
            ..Default::default()
        };
        assert_eq!(
            q_ruim.texto(),
            "Conexão ruim (perda 12.0%, ida e volta 400ms)"
        );
        assert_eq!(
            cor_da_qualidade(NivelConexao::Ruim),
            Color::from_rgb8(0xf2, 0x3f, 0x43)
        );

        let q_indef = QualidadeConexao {
            nivel: NivelConexao::Indefinido,
            perda: None,
            rtt_ms: None,
            ..Default::default()
        };
        assert_eq!(q_indef.texto(), "Medindo conexão...");
        assert_eq!(
            cor_da_qualidade(NivelConexao::Indefinido),
            Color::from_rgb8(0x72, 0x76, 0x7d)
        );
    }
}
