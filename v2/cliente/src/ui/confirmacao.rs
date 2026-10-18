//! Diálogo modal de confirmação antes de sair de uma chamada ou fechar o app.
//!
//! Evita desconexões acidentais quando o usuário clica no botão de voltar ou tenta
//! fechar a janela durante uma chamada ativa de voz/vídeo.

use iced::widget::{button, column, container, row, space, text};
use iced::{Alignment, Border, Color, Element, Length, Shadow, Vector};

use crate::tema::Cores;

/// Tipos de ação crítica que exigem confirmação do usuário.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TipoConfirmacao {
    #[allow(dead_code)]
    SairChamada {
        sala: String,
    },
    FecharAplicativo {
        sala: Option<String>,
    },
    ApagarGrupo {
        id: String,
        nome: String,
    },
}

impl TipoConfirmacao {
    pub fn titulo(&self) -> &'static str {
        match self {
            Self::SairChamada { .. } => "Sair da chamada?",
            Self::FecharAplicativo { sala: Some(_) } => "Sair da chamada e fechar o app?",
            Self::FecharAplicativo { sala: None } => "Fechar o aplicativo?",
            Self::ApagarGrupo { .. } => "Apagar o grupo?",
        }
    }

    pub fn mensagem(&self) -> String {
        match self {
            Self::SairChamada { sala } => {
                format!(
                    "Você está ativo na chamada em #{sala}. Ao sair, seu áudio, vídeo e compartilhamento de tela serão encerrados."
                )
            }
            Self::FecharAplicativo { sala: Some(sala) } => {
                format!(
                    "Você está conectado na chamada em #{sala}. Ao fechar, você será desconectado da chamada e o aplicativo será encerrado."
                )
            }
            Self::FecharAplicativo { sala: None } => {
                "Deseja realmente encerrar o Lincast Chat?".into()
            }
            Self::ApagarGrupo { nome, .. } => format!(
                "O grupo \"{nome}\" e todo o chat dele (mensagens, imagens e arquivos) somem para sempre, para todo mundo. Isso não tem volta."
            ),
        }
    }

    pub fn texto_confirmar(&self) -> &'static str {
        match self {
            Self::SairChamada { .. } => "Sair da chamada",
            Self::FecharAplicativo { .. } => "Fechar app",
            Self::ApagarGrupo { .. } => "Apagar grupo",
        }
    }
}

/// Renderiza o diálogo modal de confirmação.
pub fn modal_confirmacao<'a, M: 'a + Clone>(
    tipo: &TipoConfirmacao,
    p: Cores,
    ao_confirmar: M,
    ao_cancelar: M,
) -> Element<'a, M> {
    let titulo = text(tipo.titulo()).size(16).color(p.texto);

    let icone = container(crate::ui::efeitos::ico(
        crate::icones::aviso_triangulo(),
        22.0,
        p.alerta,
    ))
    .padding(8);

    let topo = row![icone, titulo].spacing(8).align_y(Alignment::Center);

    let corpo = text(tipo.mensagem()).size(13).color(p.texto_fraco);

    let btn_cancelar = button(text("Cancelar").size(13).color(p.texto))
        .padding([8, 16])
        .style(move |_, status| {
            let bg = if status == button::Status::Hovered {
                p.borda
            } else {
                p.lateral
            };
            button::Style {
                background: Some(bg.into()),
                text_color: p.texto,
                border: Border {
                    color: p.borda,
                    width: 1.0,
                    radius: 6.0.into(),
                },
                ..Default::default()
            }
        })
        .on_press(ao_cancelar);

    let btn_confirmar = button(text(tipo.texto_confirmar()).size(13).color(Color::WHITE))
        .padding([8, 16])
        .style(move |_, status| {
            let mut bg = p.perigo;
            if status == button::Status::Hovered {
                bg.a = 0.85;
            }
            button::Style {
                background: Some(bg.into()),
                text_color: Color::WHITE,
                border: Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .on_press(ao_confirmar);

    let rodape = row![space().width(Length::Fill), btn_cancelar, btn_confirmar,]
        .spacing(10)
        .align_y(Alignment::Center);

    let painel = container(
        column![topo, corpo, space().height(Length::Fixed(6.0)), rodape]
            .spacing(12)
            .width(Length::Fixed(440.0)),
    )
    .padding(18)
    .style(move |_| container::Style {
        background: Some(p.elevado.into()),
        border: Border {
            color: p.borda,
            width: 1.5,
            radius: 10.0.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.6),
            offset: Vector::new(0.0, 6.0),
            blur_radius: 18.0,
        },
        ..Default::default()
    });

    container(painel)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(|_| container::Style {
            background: Some(Color::from_rgba(0.0, 0.0, 0.0, 0.65).into()),
            ..Default::default()
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn testa_textos_confirmacao_sair_chamada() {
        let conf = TipoConfirmacao::SairChamada {
            sala: "amigos".into(),
        };
        assert_eq!(conf.titulo(), "Sair da chamada?");
        assert!(conf.mensagem().contains("amigos"));
        assert!(conf.mensagem().contains("áudio, vídeo e compartilhamento"));
        assert_eq!(conf.texto_confirmar(), "Sair da chamada");
    }

    #[test]
    fn testa_textos_confirmacao_fechar_app_em_chamada() {
        let conf = TipoConfirmacao::FecharAplicativo {
            sala: Some("devs".into()),
        };
        assert_eq!(conf.titulo(), "Sair da chamada e fechar o app?");
        assert!(conf.mensagem().contains("devs"));
        assert_eq!(conf.texto_confirmar(), "Fechar app");
    }

    #[test]
    fn testa_instanciacao_modal_confirmacao_sem_panico() {
        let conf = TipoConfirmacao::SairChamada {
            sala: "sala-teste".into(),
        };
        let cores = crate::tema::GRAFITE;
        let _elem = modal_confirmacao::<()>(&conf, cores, (), ());
    }
}
