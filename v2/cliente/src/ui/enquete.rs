use crate::tema;
use iced::widget::{button, column, container, row, scrollable, space, text, text_input};
use iced::{Color, Element, Length};

pub const MAX_OPCOES_ENQUETE: usize = 6;
pub const MIN_OPCOES_ENQUETE: usize = 2;

#[derive(Debug, Clone)]
pub struct EstadoCriadorEnquete {
    pub aberto: bool,
    pub pergunta: String,
    pub opcoes: Vec<String>,
    pub erro: Option<String>,
}

impl Default for EstadoCriadorEnquete {
    fn default() -> Self {
        Self {
            aberto: false,
            pergunta: String::new(),
            opcoes: vec![String::new(), String::new()],
            erro: None,
        }
    }
}

impl EstadoCriadorEnquete {
    pub fn abrir(&mut self) {
        self.aberto = true;
        self.pergunta.clear();
        self.opcoes = vec![String::new(), String::new()];
        self.erro = None;
    }

    pub fn fechar(&mut self) {
        self.aberto = false;
        self.erro = None;
    }

    pub fn adicionar_opcao(&mut self) {
        if self.opcoes.len() < MAX_OPCOES_ENQUETE {
            self.opcoes.push(String::new());
        }
    }

    pub fn remover_opcao(&mut self, indice: usize) {
        if self.opcoes.len() > MIN_OPCOES_ENQUETE && indice < self.opcoes.len() {
            self.opcoes.remove(indice);
        }
    }

    pub fn validar(&self) -> Result<(String, Vec<String>), String> {
        let p = self.pergunta.trim().to_string();
        if p.is_empty() {
            return Err("Digite a pergunta da enquete.".to_string());
        }
        let ops: Vec<String> = self
            .opcoes
            .iter()
            .map(|o| o.trim().to_string())
            .filter(|o| !o.is_empty())
            .collect();

        if ops.len() < MIN_OPCOES_ENQUETE {
            return Err("A enquete precisa de pelo menos duas opções preenchidas.".to_string());
        }

        Ok((p, ops))
    }
}

#[derive(Debug, Clone)]
pub enum EnqueteMsg {
    Pergunta(String),
    Opcao(usize, String),
    AdicionarOpcao,
    RemoverOpcao(usize),
    Criar,
    Fechar,
}

/// Renderiza o modal de criação de enquetes
pub fn modal_criador_enquete<'a>(estado: &'a EstadoCriadorEnquete) -> Element<'a, EnqueteMsg> {
    let topo = row![
        text("Nova enquete").size(16).color(tema::cores().texto),
        text("Aparece no chat para todos votarem.")
            .size(12)
            .color(tema::cores().texto_fraco),
        space().width(Length::Fill),
        button(crate::ui::efeitos::ico(
            crate::icones::fechar(),
            14.0,
            tema::cores().texto_medio
        ))
        .on_press(EnqueteMsg::Fechar)
        .style(|_t, _s| button::Style {
            background: None,
            text_color: tema::cores().texto_medio,
            ..Default::default()
        }),
    ]
    .spacing(12)
    .align_y(iced::Alignment::Center);

    let campo_pergunta = text_input("Qual é a pergunta da enquete?", &estado.pergunta)
        .on_input(EnqueteMsg::Pergunta)
        .padding([10, 12])
        .size(14);

    let mut lista_opcoes = column![].spacing(8);
    for (i, op) in estado.opcoes.iter().enumerate() {
        let placeholder = format!("Opção {}", i + 1);
        let campo_op = text_input(&placeholder, op)
            .on_input(move |val| EnqueteMsg::Opcao(i, val))
            .padding([8, 10])
            .size(13);

        let mut linha_op = row![campo_op].spacing(8).align_y(iced::Alignment::Center);

        if estado.opcoes.len() > MIN_OPCOES_ENQUETE {
            let btn_del = button(crate::ui::efeitos::ico(
                crate::icones::fechar(),
                12.0,
                tema::cores().texto_medio,
            ))
            .padding([6, 10])
            .style(|_t, _s| button::Style {
                background: Some(Color::from_rgb8(60, 30, 35).into()),
                text_color: Color::from_rgb8(240, 120, 120),
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .on_press(EnqueteMsg::RemoverOpcao(i));
            linha_op = linha_op.push(btn_del);
        }

        lista_opcoes = lista_opcoes.push(linha_op);
    }

    let mut acoes = row![].spacing(10).align_y(iced::Alignment::Center);

    if estado.opcoes.len() < MAX_OPCOES_ENQUETE {
        let btn_add = button(text("+ Adicionar opção").size(12))
            .padding([8, 12])
            .style(|_t, _s| button::Style {
                background: Some(tema::cores().elevado.into()),
                text_color: Color::WHITE,
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .on_press(EnqueteMsg::AdicionarOpcao);
        acoes = acoes.push(btn_add);
    }

    acoes = acoes.push(space().width(Length::Fill));

    let btn_criar = button(text("Criar enquete").size(13))
        .padding([10, 20])
        .style(|_t, _s| button::Style {
            background: Some(tema::cores().destaque.into()),
            text_color: Color::WHITE,
            border: iced::Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .on_press(EnqueteMsg::Criar);

    acoes = acoes.push(btn_criar);

    let mut conteudo = column![
        topo,
        text("Pergunta").size(13).color(Color::WHITE),
        campo_pergunta,
        text("Opções de voto").size(13).color(Color::WHITE),
        scrollable(lista_opcoes).height(Length::Fixed(180.0)),
        acoes,
    ]
    .spacing(12)
    .padding(18)
    .width(Length::Fixed(520.0));

    if let Some(err) = &estado.erro {
        conteudo = conteudo.push(text(err).size(12).color(tema::cores().perigo));
    }

    container(conteudo)
        .style(|_t| container::Style {
            background: Some(tema::cores().lateral.into()),
            border: iced::Border {
                color: tema::cores().borda,
                width: 1.0,
                radius: 12.0.into(),
            },
            shadow: iced::Shadow {
                color: Color::from_rgba8(0, 0, 0, 0.6),
                offset: iced::Vector::new(0.0, 8.0),
                blur_radius: 24.0,
            },
            ..Default::default()
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn testa_validacao_criador_enquete() {
        let mut estado = EstadoCriadorEnquete::default();
        assert!(estado.validar().is_err());

        estado.pergunta = "Qual seu editor favorito?".to_string();
        assert!(estado.validar().is_err());

        estado.opcoes[0] = "Neovim".to_string();
        assert!(estado.validar().is_err());

        estado.opcoes[1] = "VS Code".to_string();
        let ok = estado.validar().expect("validacao");
        assert_eq!(ok.0, "Qual seu editor favorito?");
        assert_eq!(ok.1, vec!["Neovim", "VS Code"]);
    }

    #[test]
    fn testa_limites_de_opcoes() {
        let mut estado = EstadoCriadorEnquete::default();
        assert_eq!(estado.opcoes.len(), 2);
        estado.remover_opcao(0);
        assert_eq!(estado.opcoes.len(), 2); // Não remove abaixo de 2

        for _ in 0..10 {
            estado.adicionar_opcao();
        }
        assert_eq!(estado.opcoes.len(), MAX_OPCOES_ENQUETE); // Teto de 6

        estado.remover_opcao(1);
        assert_eq!(estado.opcoes.len(), 5);
    }
}
