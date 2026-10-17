//! Janela "Mensagens favoritas": retratos guardados no servidor, por pessoa.

use iced::widget::{Column, button, column, container, row, scrollable, text};
use iced::{Alignment, Background, Border, Element, Length};

use crate::api::Favorito;
use crate::avisos::Conversa;
use crate::ui::conversa::quando;
use crate::ui::efeitos::ico;
use crate::{Msg, fraco, icones, negrito, nome_da_conversa, tema, texto_perigo};

#[derive(Debug, Clone, Default)]
pub struct EstadoFavoritos {
    /// `None` enquanto carrega.
    pub itens: Option<Vec<Favorito>>,
    pub erro: Option<String>,
}

fn resumo(f: &Favorito) -> String {
    if !f.texto.is_empty() {
        return f.texto.clone();
    }
    match f.tipo.as_str() {
        "imagem" => "Imagem".into(),
        "gif" => "GIF".into(),
        "enquete" => "Enquete".into(),
        _ => "Mensagem".into(),
    }
}

pub fn modal<'a>(
    est: &'a EstadoFavoritos,
    conversas: &'a [Conversa],
    meu_usuario: Option<&'a str>,
) -> Element<'a, Msg> {
    let c = tema::cores();
    let mut lista = Column::new().spacing(6);
    match &est.itens {
        None => {
            lista = lista.push(text("Carregando…").size(13).style(fraco));
        }
        Some(v) if v.is_empty() => {
            lista = lista.push(
                text("Nenhuma mensagem favorita ainda. Use o menu “⋯” de uma mensagem para favoritar.")
                    .size(13)
                    .style(fraco),
            );
        }
        Some(v) => {
            for f in v {
                let onde = conversas
                    .iter()
                    .find(|x| x.id == f.conversa)
                    .map(|x| nome_da_conversa(x, meu_usuario))
                    .unwrap_or_else(|| "Conversa".into());
                lista = lista.push(
                    container(
                        row![
                            button(
                                column![
                                    row![
                                        text(&f.autor).size(14).font(negrito()),
                                        text(format!("em {onde} · {}", quando(f.ts)))
                                            .size(12)
                                            .style(fraco),
                                    ]
                                    .spacing(8),
                                    text(resumo(f)).size(13),
                                ]
                                .spacing(2)
                                .width(Length::Fill),
                            )
                            .width(Length::Fill)
                            .padding([6, 8])
                            .style(tema::item_de_lista)
                            .on_press(Msg::AbreSala(f.conversa.clone())),
                            button(ico(icones::favorito(), 16.0, c.destaque))
                                .padding(6)
                                .style(tema::botao_icone(false, false))
                                .on_press(Msg::Desfavorita {
                                    conversa: f.conversa.clone(),
                                    id: f.id.clone(),
                                }),
                        ]
                        .spacing(4)
                        .align_y(Alignment::Center),
                    )
                    .padding(2),
                );
            }
        }
    }
    let mut corpo = column![
        row![
            text("Mensagens favoritas")
                .size(20)
                .font(negrito())
                .width(Length::Fill),
            button(ico(icones::fechar(), 16.0, c.texto_fraco))
                .padding(4)
                .style(tema::botao_icone(false, false))
                .on_press(Msg::FechaFavoritos),
        ]
        .align_y(Alignment::Center),
        container(scrollable(lista).style(tema::rolagem)).height(340),
    ]
    .spacing(12);
    if let Some(e) = &est.erro {
        corpo = corpo.push(text(e.clone()).size(13).style(texto_perigo));
    }
    container(corpo)
        .padding(20)
        .width(480)
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
