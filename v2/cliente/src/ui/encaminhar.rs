//! Janela "Encaminhar para": escolhe uma ou várias conversas, mensagem
//! opcional e envia. O servidor copia a mensagem original (o cliente só diz qual).

use iced::widget::{
    Column, button, checkbox, column, container, row, scrollable, space, text, text_input,
};
use iced::{Alignment, Background, Border, Element, Length};

use crate::avisos::Conversa;
use crate::ui::conversa::{InfoConversa, icone_da_conversa};
use crate::ui::efeitos::ico;
use crate::{Msg, fraco, icones, negrito, nome_da_conversa, tema, texto_perigo};

#[derive(Debug, Clone)]
pub struct EstadoEncaminhar {
    /// Conversa onde a mensagem está e id dela.
    pub origem: String,
    pub mensagem: String,
    /// Resumo mostrado na janela ("1 imagem", trecho do texto…).
    pub resumo: String,
    pub busca: String,
    pub marcadas: Vec<String>,
    pub extra: String,
    pub enviando: bool,
    pub erro: Option<String>,
}

impl EstadoEncaminhar {
    pub fn novo(origem: String, mensagem: String, resumo: String) -> Self {
        Self {
            origem,
            mensagem,
            resumo,
            busca: String::new(),
            marcadas: Vec::new(),
            extra: String::new(),
            enviando: false,
            erro: None,
        }
    }

    pub fn alterna(&mut self, id: &str) {
        if let Some(i) = self.marcadas.iter().position(|c| c == id) {
            self.marcadas.remove(i);
        } else {
            self.marcadas.push(id.to_string());
        }
    }
}

/// Conversas que casam com a busca (nome da conversa ou de quem está nela).
pub fn filtra<'a>(
    conversas: &'a [Conversa],
    busca: &str,
    meu_usuario: Option<&str>,
) -> Vec<&'a Conversa> {
    let b = busca.trim().to_lowercase();
    conversas
        .iter()
        .filter(|c| {
            b.is_empty()
                || nome_da_conversa(c, meu_usuario).to_lowercase().contains(&b)
                || c.membros.iter().any(|m| m.name.to_lowercase().contains(&b))
        })
        .collect()
}

fn subtitulo(c: &Conversa, meu_usuario: Option<&str>) -> String {
    if c.tipo == "grupo" {
        let nomes: Vec<&str> = c
            .membros
            .iter()
            .filter(|m| m.usuario.as_deref() != meu_usuario)
            .map(|m| m.name.as_str())
            .collect();
        match nomes.len() {
            0..=3 => nomes.join(", "),
            n => format!("{}, {} e {} outros", nomes[0], nomes[1], n - 2),
        }
    } else {
        c.membros
            .iter()
            .find(|m| m.usuario.as_deref() != meu_usuario)
            .and_then(|m| m.usuario.clone())
            .unwrap_or_default()
    }
}

pub fn modal<'a>(
    est: &'a EstadoEncaminhar,
    conversas: &'a [Conversa],
    meu_usuario: Option<&'a str>,
) -> Element<'a, Msg> {
    let c = tema::cores();
    let lista_filtrada = filtra(conversas, &est.busca, meu_usuario);
    let mut lista = Column::new().spacing(2);
    if lista_filtrada.is_empty() {
        lista = lista.push(
            container(text("Nenhuma conversa encontrada.").size(13).style(fraco)).padding([12, 8]),
        );
    }
    for conv in lista_filtrada {
        let marcada = est.marcadas.contains(&conv.id);
        let nome = nome_da_conversa(conv, meu_usuario);
        let info = InfoConversa {
            conversa: conv,
            meu_usuario,
        };
        let id = conv.id.clone();
        lista = lista.push(
            button(
                row![
                    icone_da_conversa(Some(&info), &nome, 32.0, c.lateral),
                    column![
                        text(nome).size(14).font(negrito()),
                        text(subtitulo(conv, meu_usuario)).size(12).style(fraco),
                    ]
                    .width(Length::Fill),
                    checkbox(marcada).size(18),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .padding([6, 8])
            .style(move |_, st| button::Style {
                background: matches!(st, button::Status::Hovered)
                    .then_some(Background::Color(c.forte)),
                text_color: c.texto,
                border: Border {
                    radius: tema::RAIO.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .on_press(Msg::EncaminhaAlterna(id)),
        );
    }
    let n = est.marcadas.len();
    let enviar = button(
        row![
            text(if n > 1 {
                format!("Enviar ({n})")
            } else {
                "Enviar".to_string()
            })
            .size(14),
            ico(icones::enviar(), 14.0, c.sobre_destaque),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .padding([8, 16])
    .style(tema::botao_primario)
    .on_press_maybe((n > 0 && !est.enviando).then_some(Msg::EncaminhaEnvia));

    let mut corpo = column![
        row![
            column![
                text("Encaminhar para").size(20).font(negrito()),
                text("Selecione onde você quer compartilhar esta mensagem.")
                    .size(13)
                    .style(fraco),
            ]
            .spacing(2)
            .width(Length::Fill),
            button(ico(icones::fechar(), 16.0, c.texto_fraco))
                .padding(4)
                .style(tema::botao_icone(false, false))
                .on_press(Msg::FechaEncaminhar),
        ],
        text_input("Buscar", &est.busca)
            .on_input(Msg::EncaminhaBusca)
            .padding([8, 10])
            .size(14)
            .style(tema::campo),
        container(scrollable(lista).style(tema::rolagem)).height(300),
        container(
            row![
                ico(icones::imagem(), 16.0, c.texto_medio),
                text(&est.resumo).size(13).style(fraco),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
        )
        .padding([6, 10])
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(c.elevado)),
            border: Border {
                radius: tema::RAIO.into(),
                ..Default::default()
            },
            ..Default::default()
        }),
        row![
            text_input("Adicione uma mensagem opcional…", &est.extra)
                .on_input(Msg::EncaminhaExtra)
                .on_submit(Msg::EncaminhaEnvia)
                .padding([8, 10])
                .size(14)
                .style(tema::campo),
            enviar,
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    ]
    .spacing(12);
    if let Some(e) = &est.erro {
        corpo = corpo.push(text(e.clone()).size(13).style(texto_perigo));
    }
    let _ = space();
    container(corpo)
        .padding(20)
        .width(460)
        .style(move |_| container::Style {
            background: Some(Background::Color(c.lateral)),
            border: Border {
                color: c.borda,
                width: 1.0,
                radius: tema::RAIO_GRANDE.into(),
            },
            ..Default::default()
        })
        .into()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn alterna_marca_e_desmarca() {
        let mut e = EstadoEncaminhar::novo("a".into(), "m".into(), "x".into());
        e.alterna("c1");
        e.alterna("c2");
        e.alterna("c1");
        assert_eq!(e.marcadas, vec!["c2".to_string()]);
    }
}
