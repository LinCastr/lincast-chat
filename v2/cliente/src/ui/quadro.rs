use crate::tema;
use iced::mouse::{self, Cursor};
use iced::widget::canvas::{self, Action, Canvas, Event, Frame, Geometry, Path, Program, Stroke};
use iced::widget::{button, column, container, row, slider, space, text};
use iced::{Color, Element, Length, Point, Rectangle, Size, Theme};
use serde::{Deserialize, Serialize};

pub const CORES_DO_QUADRO: [&str; 7] = [
    "#ffffff", "#ff5f6d", "#ffd166", "#4ade80", "#60a5fa", "#c084fc", "#111111",
];

pub const MAX_PONTOS_DO_QUADRO: usize = 12000;

/// Traço do quadro branco compatível com a sinalização da v1.
/// Coordenadas de 0 a 10000 para manter proporção idêntica independente da resolução.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Traco {
    pub c: String,
    pub l: u32,
    pub p: Vec<u32>,
    #[serde(default)]
    pub b: u8,
}

#[derive(Debug, Clone)]
pub enum QuadroMsg {
    NovoPonto(u32, u32),
    FinalizarTraco,
    MudarCor(String),
    MudarEspessura(u32),
    AlternarBorracha,
    LimparQuadro,
    Fechar,
}

#[derive(Debug)]
pub struct EstadoQuadro {
    pub aberto: bool,
    pub tracos: Vec<Traco>,
    pub total_pontos: usize,
    pub cor_atual: String,
    pub espessura_atual: u32,
    pub borracha: bool,
    pub traco_em_andamento: Option<Vec<u32>>,
    pub cache: canvas::Cache,
}

impl Default for EstadoQuadro {
    fn default() -> Self {
        Self {
            aberto: false,
            tracos: Vec::new(),
            total_pontos: 0,
            cor_atual: CORES_DO_QUADRO[0].to_string(),
            espessura_atual: 3,
            borracha: false,
            traco_em_andamento: None,
            cache: canvas::Cache::default(),
        }
    }
}

impl EstadoQuadro {
    pub fn adicionar_traco(&mut self, traco: Traco) {
        let pontos = traco.p.len() / 2;
        self.total_pontos += pontos;
        self.tracos.push(traco);
        while self.total_pontos > MAX_PONTOS_DO_QUADRO && !self.tracos.is_empty() {
            let primeiro = self.tracos.remove(0);
            self.total_pontos = self.total_pontos.saturating_sub(primeiro.p.len() / 2);
        }
        self.cache.clear();
    }

    pub fn carregar_estado(&mut self, tracos: Vec<Traco>) {
        self.tracos = tracos;
        self.total_pontos = self.tracos.iter().map(|t| t.p.len() / 2).sum();
        self.cache.clear();
    }

    pub fn limpar(&mut self) {
        self.tracos.clear();
        self.total_pontos = 0;
        self.traco_em_andamento = None;
        self.cache.clear();
    }
}

/// Canvas Program para desenhar e interagir com o quadro branco.
pub struct QuadroCanvasProgram<'a> {
    pub tracos: &'a [Traco],
    pub traco_ativo: Option<&'a [u32]>,
    pub cor_ativa: &'a str,
    pub espessura_ativa: u32,
    pub borracha: bool,
    pub cache: &'a canvas::Cache,
}

#[derive(Default)]
pub struct CanvasState {
    desenhando: bool,
}

fn parse_hex_color(hex: &str) -> Color {
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

impl<'a> Program<QuadroMsg> for QuadroCanvasProgram<'a> {
    type State = CanvasState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> Option<Action<QuadroMsg>> {
        if bounds.width <= 0.0 || bounds.height <= 0.0 {
            return None;
        }

        if let Event::Mouse(mouse_ev) = event {
            match mouse_ev {
                mouse::Event::ButtonPressed(mouse::Button::Left) => {
                    if let Some(pos) = cursor.position_in(bounds) {
                        state.desenhando = true;
                        let x = ((pos.x / bounds.width) * 10000.0).clamp(0.0, 10000.0) as u32;
                        let y = ((pos.y / bounds.height) * 10000.0).clamp(0.0, 10000.0) as u32;
                        return Some(Action::publish(QuadroMsg::NovoPonto(x, y)));
                    }
                }
                mouse::Event::CursorMoved { .. } => {
                    if state.desenhando
                        && let Some(pos) = cursor.position_in(bounds)
                    {
                        let x = ((pos.x / bounds.width) * 10000.0).clamp(0.0, 10000.0) as u32;
                        let y = ((pos.y / bounds.height) * 10000.0).clamp(0.0, 10000.0) as u32;
                        return Some(Action::publish(QuadroMsg::NovoPonto(x, y)));
                    }
                }
                mouse::Event::ButtonReleased(mouse::Button::Left) if state.desenhando => {
                    state.desenhando = false;
                    return Some(Action::publish(QuadroMsg::FinalizarTraco));
                }
                _ => {}
            }
        }

        None
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry> {
        if bounds.width <= 0.0 || bounds.height <= 0.0 {
            return Vec::new();
        }

        let geom_historico = self.cache.draw(renderer, bounds.size(), |frame| {
            // Fundo escuro do quadro
            frame.fill_rectangle(Point::ORIGIN, bounds.size(), tema::cores().palco);

            for traco in self.tracos {
                desenha_traco_no_frame(frame, traco, bounds.size());
            }
        });

        // Se houver traço em andamento desenhado pelo usuário local
        if let Some(pontos) = self.traco_ativo
            && pontos.len() >= 2
        {
            let mut frame_ativo = Frame::new(renderer, bounds.size());
            let traco_temp = Traco {
                c: self.cor_ativa.to_string(),
                l: self.espessura_ativa,
                p: pontos.to_vec(),
                b: if self.borracha { 1 } else { 0 },
            };
            desenha_traco_no_frame(&mut frame_ativo, &traco_temp, bounds.size());
            return vec![geom_historico, frame_ativo.into_geometry()];
        }

        vec![geom_historico]
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> mouse::Interaction {
        if cursor.is_over(bounds) {
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::default()
        }
    }
}

fn desenha_traco_no_frame(frame: &mut Frame, traco: &Traco, size: Size) {
    let p = &traco.p;
    if p.len() < 2 {
        return;
    }

    let scale_x = size.width / 10000.0;
    let scale_y = size.height / 10000.0;
    let scale_w = (size.width / 1000.0).max(1.0);

    let cor = if traco.b != 0 {
        // Borracha usa a cor de fundo do quadro no canvas
        tema::cores().palco
    } else {
        parse_hex_color(&traco.c)
    };

    let stroke_width = ((traco.l as f32) * scale_w * 0.4).max(1.5);

    let path = Path::new(|builder| {
        let p0 = Point::new(p[0] as f32 * scale_x, p[1] as f32 * scale_y);
        builder.move_to(p0);
        for chunk in p[2..].chunks(2) {
            if chunk.len() == 2 {
                let pt = Point::new(chunk[0] as f32 * scale_x, chunk[1] as f32 * scale_y);
                builder.line_to(pt);
            }
        }
        if p.len() == 2 {
            builder.line_to(Point::new(p0.x + 0.5, p0.y + 0.5));
        }
    });

    let stroke = Stroke::default()
        .with_color(cor)
        .with_width(stroke_width)
        .with_line_cap(canvas::LineCap::Round)
        .with_line_join(canvas::LineJoin::Round);

    frame.stroke(&path, stroke);
}

/// Renderiza o modal completo do quadro branco
pub fn modal_quadro<'a>(estado: &'a EstadoQuadro) -> Element<'a, QuadroMsg> {
    let topo = row![
        text("Quadro branco da sala")
            .size(16)
            .color(tema::cores().texto),
        text("Todo mundo da sala desenha junto.")
            .size(12)
            .color(tema::cores().texto_fraco),
        space().width(Length::Fill),
        button(crate::ui::efeitos::ico(
            crate::icones::fechar(),
            14.0,
            tema::cores().texto_medio
        ))
        .on_press(QuadroMsg::Fechar)
        .style(|_t, _s| button::Style {
            background: None,
            text_color: tema::cores().texto_medio,
            ..Default::default()
        }),
    ]
    .spacing(12)
    .align_y(iced::Alignment::Center);

    let canvas_widget = Canvas::new(QuadroCanvasProgram {
        tracos: &estado.tracos,
        traco_ativo: estado.traco_em_andamento.as_deref(),
        cor_ativa: &estado.cor_atual,
        espessura_ativa: estado.espessura_atual,
        borracha: estado.borracha,
        cache: &estado.cache,
    })
    .width(Length::Fill)
    .height(Length::Fill);

    let container_canvas = container(canvas_widget)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_t| container::Style {
            background: Some(tema::cores().palco.into()),
            border: iced::Border {
                color: tema::cores().borda,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        });

    let botoes_cores = row(CORES_DO_QUADRO
        .iter()
        .map(|cor_hex| {
            let eh_ativa = *cor_hex == estado.cor_atual && !estado.borracha;
            let c = parse_hex_color(cor_hex);
            button(text("").size(1))
                .width(Length::Fixed(24.0))
                .height(Length::Fixed(24.0))
                .style(move |_t, _s| button::Style {
                    background: Some(c.into()),
                    border: iced::Border {
                        color: if eh_ativa {
                            Color::WHITE
                        } else {
                            Color::TRANSPARENT
                        },
                        width: 2.0,
                        radius: 12.0.into(),
                    },
                    ..Default::default()
                })
                .on_press(QuadroMsg::MudarCor(cor_hex.to_string()))
                .into()
        })
        .collect::<Vec<_>>())
    .spacing(6);

    let botao_borracha = button(
        text(if estado.borracha {
            "Borracha (ativa)"
        } else {
            "Borracha"
        })
        .size(13),
    )
    .on_press(QuadroMsg::AlternarBorracha)
    .style(move |_t, _s| button::Style {
        background: Some(if estado.borracha {
            tema::cores().destaque.into()
        } else {
            tema::cores().elevado.into()
        }),
        text_color: Color::WHITE,
        border: iced::Border {
            radius: 6.0.into(),
            ..Default::default()
        },
        ..Default::default()
    });

    let controle_espessura = row![
        text(format!("Espessura: {}px", estado.espessura_atual))
            .size(12)
            .color(tema::cores().texto_medio),
        slider(1..=24, estado.espessura_atual, QuadroMsg::MudarEspessura)
            .width(Length::Fixed(120.0)),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);

    let botao_limpar = button(text("Limpar tudo").size(13))
        .on_press(QuadroMsg::LimparQuadro)
        .style(|_t, _s| button::Style {
            background: Some(tema::cores().perigo.into()),
            text_color: Color::WHITE,
            border: iced::Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

    let barra_ferramentas = row![
        botoes_cores,
        botao_borracha,
        controle_espessura,
        space().width(Length::Fill),
        botao_limpar,
    ]
    .spacing(14)
    .align_y(iced::Alignment::Center);

    let conteudo = column![topo, container_canvas, barra_ferramentas,]
        .spacing(12)
        .padding(16)
        .width(Length::Fixed(960.0))
        .height(Length::Fixed(600.0));

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
    fn testa_serializacao_e_desserializacao_traco() {
        let traco = Traco {
            c: "#ff5f6d".to_string(),
            l: 4,
            p: vec![1000, 2000, 1500, 2500, 2000, 3000],
            b: 0,
        };
        let json = serde_json::to_string(&traco).expect("serializa traco");
        let parsed: Traco = serde_json::from_str(&json).expect("desserializa traco");
        assert_eq!(parsed, traco);
    }

    #[test]
    fn testa_limite_de_pontos_no_estado_quadro() {
        let mut estado = EstadoQuadro::default();
        // Adiciona 20 traços de 800 pontos cada (16000 pontos no total)
        for i in 0..20 {
            let pontos = vec![i as u32; 1600]; // 800 pontos (x, y)
            estado.adicionar_traco(Traco {
                c: "#ffffff".to_string(),
                l: 3,
                p: pontos,
                b: 0,
            });
        }
        assert!(estado.total_pontos <= MAX_PONTOS_DO_QUADRO);
        assert!(!estado.tracos.is_empty());
    }

    #[test]
    fn testa_limpar_quadro() {
        let mut estado = EstadoQuadro::default();
        estado.adicionar_traco(Traco {
            c: "#ffffff".to_string(),
            l: 3,
            p: vec![10, 20, 30, 40],
            b: 0,
        });
        assert_eq!(estado.tracos.len(), 1);
        estado.limpar();
        assert_eq!(estado.tracos.len(), 0);
        assert_eq!(estado.total_pontos, 0);
    }
}
