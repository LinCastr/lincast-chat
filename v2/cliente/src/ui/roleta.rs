use crate::tema;
use iced::mouse::Cursor;
use iced::widget::canvas::{self, Canvas, Frame, Geometry, Path, Program, Stroke};
use iced::widget::{button, column, container, row, scrollable, space, text, text_input};
use iced::{Color, Element, Length, Point, Rectangle, Theme};
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

pub const CORES_DA_ROLETA: [&str; 8] = [
    "#4a6cf7", "#ff5f6d", "#ffd166", "#4ade80", "#c084fc", "#22d3ee", "#fb923c", "#f472b6",
];

#[allow(dead_code)]
pub const MAX_ROLETAS_POR_SALA: usize = 12;
#[allow(dead_code)]
pub const MAX_OPCOES_DA_ROLETA: usize = 24;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Roleta {
    #[serde(default)]
    pub id: String,
    pub nome: String,
    pub opcoes: Vec<String>,
    #[serde(default, rename = "criadaPor")]
    pub criada_por: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AnimacaoRoleta {
    pub indice_alvo: usize,
    pub girada_por: Option<String>,
    pub angulo_inicial: f32,
    pub angulo_alvo: f32,
    pub inicio: std::time::Instant,
    pub duracao: std::time::Duration,
}

#[derive(Debug, Clone)]
pub enum RoletaMsg {
    Selecionar(String),
    NovaRoleta,
    RascunhoNome(String),
    RascunhoOpcoes(String),
    Salvar,
    Apagar(String),
    Girar(String),
    TickAnimacao,
    Fechar,
}

#[derive(Debug, Clone)]
pub struct EstadoRoletas {
    pub aberto: bool,
    pub roletas: Vec<Roleta>,
    pub roleta_selecionada_id: Option<String>,
    pub rascunho_nome: String,
    pub rascunho_opcoes: String,
    pub erro: Option<String>,
    pub angulo: f32,
    pub animacao: Option<AnimacaoRoleta>,
    pub resultado_recente: Option<String>,
}

impl Default for EstadoRoletas {
    fn default() -> Self {
        Self {
            aberto: false,
            roletas: Vec::new(),
            roleta_selecionada_id: None,
            rascunho_nome: String::new(),
            rascunho_opcoes: String::new(),
            erro: None,
            angulo: 0.0,
            animacao: None,
            resultado_recente: None,
        }
    }
}

impl EstadoRoletas {
    pub fn roleta_atual(&self) -> Option<&Roleta> {
        self.roleta_selecionada_id
            .as_ref()
            .and_then(|id| self.roletas.iter().find(|r| &r.id == id))
            .or_else(|| self.roletas.first())
    }

    pub fn selecionar(&mut self, id: String) {
        self.roleta_selecionada_id = Some(id.clone());
        self.resultado_recente = None;
        if let Some(r) = self.roletas.iter().find(|r| r.id == id) {
            self.rascunho_nome = r.nome.clone();
            self.rascunho_opcoes = r.opcoes.join("\n");
        }
        self.erro = None;
    }

    pub fn nova_roleta(&mut self) {
        self.roleta_selecionada_id = None;
        self.rascunho_nome = String::new();
        self.rascunho_opcoes = String::new();
        self.erro = None;
        self.resultado_recente = None;
    }

    pub fn atualizar_roletas(&mut self, roletas: Vec<Roleta>) {
        self.roletas = roletas;
        if let Some(id) = &self.roleta_selecionada_id {
            if let Some(r) = self.roletas.iter().find(|r| &r.id == id) {
                self.rascunho_nome = r.nome.clone();
                self.rascunho_opcoes = r.opcoes.join("\n");
            } else if let Some(primeira) = self.roletas.first() {
                self.roleta_selecionada_id = Some(primeira.id.clone());
                self.rascunho_nome = primeira.nome.clone();
                self.rascunho_opcoes = primeira.opcoes.join("\n");
            } else {
                self.nova_roleta();
            }
        } else if let Some(primeira) = self.roletas.first() {
            self.roleta_selecionada_id = Some(primeira.id.clone());
            self.rascunho_nome = primeira.nome.clone();
            self.rascunho_opcoes = primeira.opcoes.join("\n");
        }
    }

    pub fn iniciar_giro(&mut self, id: &str, indice: usize, por: Option<String>) {
        self.aberto = true;
        self.roleta_selecionada_id = Some(id.to_string());
        if let Some(r) = self.roletas.iter().find(|r| r.id == id) {
            self.rascunho_nome = r.nome.clone();
            self.rascunho_opcoes = r.opcoes.join("\n");
            let total_opcoes = r.opcoes.len().max(1);
            let fatia = (2.0 * PI) / total_opcoes as f32;
            // O ponteiro fica no topo (-PI / 2.0). Queremos que o centro da fatia `indice` pare no topo.
            // Centro da fatia `indice` está em angulo + indice * fatia + fatia / 2.
            let voltas = 6.0;
            let angulo_alvo =
                (-PI / 2.0) - (indice as f32 * fatia + fatia / 2.0) + (2.0 * PI * voltas);

            self.resultado_recente = None;
            self.animacao = Some(AnimacaoRoleta {
                indice_alvo: indice,
                girada_por: por,
                angulo_inicial: self.angulo,
                angulo_alvo,
                inicio: std::time::Instant::now(),
                duracao: std::time::Duration::from_millis(3200),
            });
        }
    }

    pub fn atualizar_animacao(&mut self) -> bool {
        if let Some(anim) = &self.animacao {
            let decorrido = anim.inicio.elapsed();
            let duracao = anim.duracao.as_secs_f32().max(0.001);
            let t = (decorrido.as_secs_f32() / duracao).clamp(0.0, 1.0);
            // Curva suave quartica: 1.0 - (1.0 - t)^4
            let suave = 1.0 - (1.0 - t).powi(4);
            self.angulo = anim.angulo_inicial + (anim.angulo_alvo - anim.angulo_inicial) * suave;

            if t >= 1.0 {
                let indice = anim.indice_alvo;
                let por = anim.girada_por.clone();
                let resultado = self
                    .roleta_atual()
                    .and_then(|r| r.opcoes.get(indice))
                    .cloned()
                    .unwrap_or_else(|| "Opção".to_string());

                let texto = if let Some(p) = por {
                    format!("🎉 {resultado} — girada por {p}")
                } else {
                    format!("🎉 {resultado}")
                };
                self.resultado_recente = Some(texto);
                self.animacao = None;
                return false;
            }
            return true;
        }
        false
    }
}

fn parse_hex(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');
    if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255);
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255);
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255);
        Color::from_rgb8(r, g, b)
    } else {
        Color::WHITE
    }
}

pub struct RoletaCanvasProgram<'a> {
    pub opcoes: &'a [String],
    pub angulo: f32,
    pub destaque: Option<usize>,
}

#[derive(Default)]
pub struct RoletaCanvasState;

impl<'a> Program<RoletaMsg> for RoletaCanvasProgram<'a> {
    type State = RoletaCanvasState;

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let cx = bounds.width / 2.0;
        let cy = bounds.height / 2.0;
        let raio = (cx.min(cy) - 20.0).max(10.0);

        if self.opcoes.is_empty() {
            frame.fill_text(canvas::Text {
                content: "Sem roleta escolhida".to_string(),
                position: Point::new(cx, cy),
                color: tema::cores().texto_fraco,
                size: 16.0.into(),
                align_x: iced::alignment::Horizontal::Center.into(),
                align_y: iced::alignment::Vertical::Center,
                ..Default::default()
            });
            return vec![frame.into_geometry()];
        }

        let num_opcoes = self.opcoes.len();
        let fatia = (2.0 * PI) / num_opcoes as f32;

        for (i, op) in self.opcoes.iter().enumerate() {
            let a0 = self.angulo + (i as f32) * fatia;
            let a1 = a0 + fatia;

            let path = Path::new(|builder| {
                builder.move_to(Point::new(cx, cy));
                builder.arc(canvas::path::Arc {
                    center: Point::new(cx, cy),
                    radius: raio,
                    start_angle: iced::Radians(a0),
                    end_angle: iced::Radians(a1),
                });
                builder.close();
            });

            let cor_base = parse_hex(CORES_DA_ROLETA[i % CORES_DA_ROLETA.len()]);
            let alpha = if self.destaque == Some(i) { 1.0 } else { 0.88 };
            let cor = Color {
                a: alpha,
                ..cor_base
            };

            frame.fill(&path, cor);
            frame.stroke(
                &path,
                Stroke::default()
                    .with_color(Color::from_rgba8(0, 0, 0, 0.3))
                    .with_width(1.5),
            );

            // Texto da opção no centro da fatia
            let angulo_texto = a0 + fatia / 2.0;
            let dist_texto = raio * 0.65;
            let tx = cx + angulo_texto.cos() * dist_texto;
            let ty = cy + angulo_texto.sin() * dist_texto;

            let texto_curto = if op.len() > 14 {
                format!("{}…", &op[..12])
            } else {
                op.clone()
            };

            frame.fill_text(canvas::Text {
                content: texto_curto,
                position: Point::new(tx, ty),
                color: Color::from_rgb8(18, 18, 18),
                size: 13.0.into(),
                align_x: iced::alignment::Horizontal::Center.into(),
                align_y: iced::alignment::Vertical::Center,
                ..Default::default()
            });
        }

        // Ponteiro de agulha no topo (apontando para baixo)
        let ponteiro = Path::new(|builder| {
            builder.move_to(Point::new(cx - 12.0, 6.0));
            builder.line_to(Point::new(cx + 12.0, 6.0));
            builder.line_to(Point::new(cx, 34.0));
            builder.close();
        });

        frame.fill(&ponteiro, Color::WHITE);
        frame.stroke(
            &ponteiro,
            Stroke::default()
                .with_color(Color::from_rgb8(30, 30, 30))
                .with_width(2.0),
        );

        vec![frame.into_geometry()]
    }
}

/// Renderiza o modal completo de roletas da sala
pub fn modal_roletas<'a>(estado: &'a EstadoRoletas) -> Element<'a, RoletaMsg> {
    let topo = row![
        text("Roletas da sala").size(16).color(tema::cores().texto),
        text("Ficam salvas na sala. O servidor sorteia o resultado sem forja.")
            .size(12)
            .color(tema::cores().texto_fraco),
        space().width(Length::Fill),
        button(crate::ui::efeitos::ico(
            crate::icones::fechar(),
            14.0,
            tema::cores().texto_medio
        ))
        .on_press(RoletaMsg::Fechar)
        .style(|_t, _s| button::Style {
            background: None,
            text_color: tema::cores().texto_medio,
            ..Default::default()
        }),
    ]
    .spacing(12)
    .align_y(iced::Alignment::Center);

    // Coluna esquerda: Lista de roletas
    let mut lista_items = column![].spacing(6);
    if estado.roletas.is_empty() {
        lista_items = lista_items.push(
            text("Nenhuma roleta ainda. Crie uma abaixo!")
                .size(12)
                .color(tema::cores().texto_fraco),
        );
    } else {
        for r in &estado.roletas {
            let eh_selecionada = estado.roleta_selecionada_id.as_deref() == Some(&r.id);
            let btn = button(
                column![
                    text(&r.nome).size(13).color(Color::WHITE),
                    text(format!(
                        "{} opções{}",
                        r.opcoes.len(),
                        r.criada_por
                            .as_ref()
                            .map(|p| format!(" · de {p}"))
                            .unwrap_or_default()
                    ))
                    .size(11)
                    .color(tema::cores().texto_medio),
                ]
                .spacing(2),
            )
            .width(Length::Fill)
            .padding([8, 10])
            .style(move |_t, _s| button::Style {
                background: Some(if eh_selecionada {
                    tema::cores().destaque.into()
                } else {
                    tema::cores().elevado.into()
                }),
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .on_press(RoletaMsg::Selecionar(r.id.clone()));

            lista_items = lista_items.push(btn);
        }
    }

    let botao_nova = button(text("+ Nova roleta").size(13))
        .width(Length::Fill)
        .padding([8, 12])
        .style(|_t, _s| button::Style {
            background: Some(tema::cores().borda.into()),
            text_color: Color::WHITE,
            border: iced::Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .on_press(RoletaMsg::NovaRoleta);

    let coluna_esquerda = column![scrollable(lista_items).height(Length::Fill), botao_nova,]
        .spacing(10)
        .width(Length::Fixed(220.0));

    // Coluna central: Palco com a Roleta gráfica e botão Girar
    let r_atual = estado.roleta_atual();
    let opcoes_atuais = r_atual.map(|r| r.opcoes.as_slice()).unwrap_or(&[]);

    let canvas_roleta = Canvas::new(RoletaCanvasProgram {
        opcoes: opcoes_atuais,
        angulo: estado.angulo,
        destaque: estado.animacao.as_ref().map(|a| a.indice_alvo),
    })
    .width(Length::Fixed(360.0))
    .height(Length::Fixed(360.0));

    let banner_resultado = text(estado.resultado_recente.as_deref().unwrap_or(""))
        .size(15)
        .color(Color::from_rgb8(245, 190, 70));

    let esta_girando = estado.animacao.is_some();
    let roleta_id_opt = r_atual.map(|r| r.id.clone());

    let botao_girar = button(text(if esta_girando { "Girando..." } else { "Girar!" }).size(15))
        .padding([10, 28])
        .style(move |_t, _s| button::Style {
            background: Some(if esta_girando {
                tema::cores().borda.into()
            } else {
                tema::cores().sucesso_forte.into()
            }),
            text_color: Color::WHITE,
            border: iced::Border {
                radius: 8.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

    let botao_girar = if !esta_girando && let Some(id) = roleta_id_opt {
        botao_girar.on_press(RoletaMsg::Girar(id))
    } else {
        botao_girar
    };

    let coluna_central = column![canvas_roleta, banner_resultado, botao_girar,]
        .spacing(12)
        .align_x(iced::Alignment::Center)
        .width(Length::Fixed(380.0));

    // Coluna direita: Editor de opções da roleta
    let campo_nome = text_input(
        "Nome da roleta (ex: Quem lava a louça)",
        &estado.rascunho_nome,
    )
    .on_input(RoletaMsg::RascunhoNome)
    .padding([8, 10])
    .size(13);

    let campo_opcoes = text_input("Opções separadas por linha...", &estado.rascunho_opcoes)
        .on_input(RoletaMsg::RascunhoOpcoes)
        .padding([8, 10])
        .size(13);

    let botao_salvar = button(text("Salvar").size(13))
        .padding([8, 14])
        .style(|_t, _s| button::Style {
            background: Some(tema::cores().destaque.into()),
            text_color: Color::WHITE,
            border: iced::Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .on_press(RoletaMsg::Salvar);

    let mut acoes_editor = row![botao_salvar].spacing(8);

    if let Some(r) = r_atual {
        let id_del = r.id.clone();
        let botao_apagar = button(text("Apagar").size(13))
            .padding([8, 14])
            .style(|_t, _s| button::Style {
                background: Some(tema::cores().perigo.into()),
                text_color: Color::WHITE,
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .on_press(RoletaMsg::Apagar(id_del));
        acoes_editor = acoes_editor.push(botao_apagar);
    }

    let mut coluna_direita = column![
        text("Editar roleta").size(14).color(Color::WHITE),
        text("Nome").size(12).color(tema::cores().texto_medio),
        campo_nome,
        text("Opções (uma por linha ou separadas por vírgula)")
            .size(12)
            .color(tema::cores().texto_medio),
        campo_opcoes,
        acoes_editor,
    ]
    .spacing(8)
    .width(Length::Fixed(260.0));

    if let Some(err) = &estado.erro {
        coluna_direita = coluna_direita.push(text(err).size(12).color(tema::cores().perigo));
    }

    let corpo = row![
        coluna_esquerda,
        container(space().width(1))
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(tema::cores().borda.into()),
                ..Default::default()
            }),
        coluna_central,
        container(space().width(1))
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(tema::cores().borda.into()),
                ..Default::default()
            }),
        coluna_direita,
    ]
    .spacing(16)
    .height(Length::Fill);

    let conteudo = column![topo, corpo,]
        .spacing(14)
        .padding(18)
        .width(Length::Fixed(940.0))
        .height(Length::Fixed(560.0));

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
    fn testa_serializacao_roleta() {
        let r = Roleta {
            id: "12345".to_string(),
            nome: "Pizza ou Hamburguer".to_string(),
            opcoes: vec!["Pizza".to_string(), "Hamburguer".to_string()],
            criada_por: Some("Val".to_string()),
        };
        let json = serde_json::to_string(&r).expect("serializa");
        let back: Roleta = serde_json::from_str(&json).expect("desserializa");
        assert_eq!(back, r);
    }

    #[test]
    fn testa_selecao_e_atualizacao_de_roletas() {
        let mut estado = EstadoRoletas::default();
        let roletas = vec![
            Roleta {
                id: "r1".to_string(),
                nome: "Roleta 1".to_string(),
                opcoes: vec!["A".to_string(), "B".to_string()],
                criada_por: None,
            },
            Roleta {
                id: "r2".to_string(),
                nome: "Roleta 2".to_string(),
                opcoes: vec!["X".to_string(), "Y".to_string(), "Z".to_string()],
                criada_por: None,
            },
        ];
        estado.atualizar_roletas(roletas);
        assert_eq!(estado.roleta_selecionada_id.as_deref(), Some("r1"));
        assert_eq!(estado.rascunho_nome, "Roleta 1");

        estado.selecionar("r2".to_string());
        assert_eq!(estado.roleta_selecionada_id.as_deref(), Some("r2"));
        assert_eq!(estado.rascunho_nome, "Roleta 2");
    }

    #[test]
    fn testa_animacao_calcula_e_encerra() {
        let mut estado = EstadoRoletas::default();
        let roleta = Roleta {
            id: "r1".to_string(),
            nome: "Sorteio".to_string(),
            opcoes: vec!["A".to_string(), "B".to_string(), "C".to_string()],
            criada_por: None,
        };
        estado.atualizar_roletas(vec![roleta]);
        estado.iniciar_giro("r1", 1, Some("Amigo".to_string()));
        assert!(estado.animacao.is_some());

        // Força expiração do tempo de animação
        if let Some(anim) = &mut estado.animacao {
            anim.inicio = std::time::Instant::now() - std::time::Duration::from_millis(4000);
        }
        let continua = estado.atualizar_animacao();
        assert!(!continua);
        assert!(estado.animacao.is_none());
        assert!(estado.resultado_recente.unwrap().contains("B"));
    }
}
