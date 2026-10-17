//! Janela "Selecionar amigos" (C6): criar grupo ou adicionar pessoas a um.

use iced::widget::{
    Column, button, checkbox, column, container, row, scrollable, space, text, text_input,
};
use iced::{Alignment, Background, Border, Color, Element, Length};

use crate::api::CartaoPerfil;
use crate::{Msg, fraco, negrito, tema, texto_perigo, ui};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModoGrupo {
    Criar,
    /// Adicionar a este grupo (id da conversa).
    Adicionar(String),
}

#[derive(Debug, Clone)]
pub struct EstadoModalGrupo {
    pub modo: ModoGrupo,
    pub busca: String,
    /// Marcados, na ordem em que foram marcados.
    pub marcados: Vec<String>,
    pub enviando: bool,
    pub erro: Option<String>,
}

impl EstadoModalGrupo {
    pub fn novo(modo: ModoGrupo, marcado: Option<String>) -> Self {
        Self {
            modo,
            busca: String::new(),
            marcados: marcado.into_iter().collect(),
            enviando: false,
            erro: None,
        }
    }

    pub fn alterna(&mut self, uid: &str) {
        if let Some(i) = self.marcados.iter().position(|u| u == uid) {
            self.marcados.remove(i);
        } else {
            self.marcados.push(uid.to_string());
        }
    }
}

/// Amigos que aparecem na lista: os que casam com a busca e (ao adicionar)
/// ainda não estão no grupo.
pub fn candidatos<'a>(
    amigos: &'a [CartaoPerfil],
    busca: &str,
    ja_no_grupo: &[String],
) -> Vec<&'a CartaoPerfil> {
    let b = busca.trim().to_lowercase();
    amigos
        .iter()
        .filter(|a| !ja_no_grupo.contains(&a.uid))
        .filter(|a| {
            b.is_empty()
                || a.name.to_lowercase().contains(&b)
                || a.usuario.to_lowercase().contains(&b)
        })
        .collect()
}

pub fn modal<'a>(
    est: &'a EstadoModalGrupo,
    candidatos: &[&'a CartaoPerfil],
    carregando: bool,
) -> Element<'a, Msg> {
    let c = tema::cores();
    let titulo = match est.modo {
        ModoGrupo::Criar => "Selecionar amigos",
        ModoGrupo::Adicionar(_) => "Adicionar pessoas",
    };
    let n = est.marcados.len();
    let rotulo_ok = match (&est.modo, n) {
        (ModoGrupo::Criar, 0) => "Criar grupo".to_string(),
        (ModoGrupo::Criar, n) => format!("Criar grupo ({n})"),
        (ModoGrupo::Adicionar(_), 0) => "Adicionar".to_string(),
        (ModoGrupo::Adicionar(_), n) => format!("Adicionar ({n})"),
    };
    let mut lista = Column::new().spacing(2);
    if candidatos.is_empty() {
        lista = lista.push(
            container(
                text(if carregando {
                    "Carregando amigos…"
                } else if est.busca.trim().is_empty() {
                    "Nenhum amigo para escolher."
                } else {
                    "Nenhum amigo com esse nome."
                })
                .size(13)
                .style(fraco),
            )
            .padding([12, 8]),
        );
    }
    for a in candidatos {
        let marcado = est.marcados.contains(&a.uid);
        let uid = a.uid.clone();
        lista = lista.push(
            button(
                row![
                    ui::avatar::avatar(
                        a.avatar.as_deref(),
                        &a.uid,
                        &a.name,
                        32.0,
                        a.status.as_deref(),
                        c.elevado
                    ),
                    column![
                        text(&a.name).size(14).font(negrito()),
                        text(format!("@{}", a.usuario)).size(12).style(fraco),
                    ]
                    .width(Length::Fill),
                    checkbox(marcado).size(18),
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
            .on_press(Msg::GrupoAlterna(uid)),
        );
    }

    let mut corpo = column![
        text(titulo).size(20).font(negrito()),
        text(match est.modo {
            ModoGrupo::Criar => "Escolha quem entra no grupo. Não há limite de pessoas.",
            ModoGrupo::Adicionar(_) => "Escolha quem entra no grupo.",
        })
        .size(13)
        .style(fraco),
        text_input("Buscar amigos", &est.busca)
            .on_input(Msg::GrupoBusca)
            .padding([8, 10])
            .size(14)
            .style(tema::campo),
        container(scrollable(lista).style(tema::rolagem)).height(300),
    ]
    .spacing(12);
    if let Some(e) = &est.erro {
        corpo = corpo.push(text(e.clone()).size(13).style(texto_perigo));
    }
    let ok = button(text(rotulo_ok).size(14))
        .padding([8, 16])
        .style(tema::botao_primario)
        .on_press_maybe((n > 0 && !est.enviando).then_some(Msg::GrupoConfirma));
    let rodape = row![
        space().width(Length::Fill),
        button(text("Cancelar").size(14))
            .padding([8, 16])
            .style(tema::botao_secundario)
            .on_press(Msg::GrupoCancela),
        ok,
    ]
    .spacing(8);
    let cartao = container(column![corpo, rodape].spacing(16))
        .padding(20)
        .width(440)
        .style(move |_| container::Style {
            background: Some(Background::Color(c.lateral)),
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

#[cfg(test)]
mod testes {
    use super::*;

    fn amigo(uid: &str, nome: &str, usuario: &str) -> CartaoPerfil {
        serde_json::from_value(serde_json::json!({
            "uid": uid, "usuario": usuario, "name": nome
        }))
        .unwrap()
    }

    #[test]
    fn candidatos_filtram_por_nome_usuario_e_grupo() {
        let todos = vec![
            amigo("u1", "Ana", "ana"),
            amigo("u2", "Bruno", "br_9"),
            amigo("u3", "Carla", "carla"),
        ];
        assert_eq!(candidatos(&todos, "", &[]).len(), 3);
        assert_eq!(candidatos(&todos, "BRU", &[]).len(), 1);
        assert_eq!(candidatos(&todos, "br_", &[]).len(), 1);
        let fora = candidatos(&todos, "", &["u2".to_string()]);
        assert!(fora.iter().all(|a| a.uid != "u2"));
    }

    #[test]
    fn alterna_marca_e_desmarca_em_ordem() {
        let mut e = EstadoModalGrupo::novo(ModoGrupo::Criar, Some("u1".into()));
        e.alterna("u2");
        e.alterna("u3");
        assert_eq!(e.marcados, vec!["u1", "u2", "u3"]);
        e.alterna("u2");
        assert_eq!(e.marcados, vec!["u1", "u3"]);
    }
}
