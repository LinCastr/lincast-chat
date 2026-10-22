//! Short selectable text (message time, system lines, the Wordle card): drag or
//! double-click to select, Ctrl+C to copy. Unlike `selecionavel` it keeps the
//! selection in the widget tree, so it needs no state in `SalaAberta`.
//! Each character is its own span, which makes hit testing and painting the
//! selection a matter of `hit_span`/`span_bounds` (fine for short texts).

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::text::{self as atext, Paragraph as _, Renderer as _};
use iced::advanced::widget::{Tree, Widget, tree};
use iced::advanced::{Clipboard, Shell, clipboard, mouse};
use iced::{Color, Element, Event, Font, Length, Point, Rectangle, Size, Theme, keyboard};

use crate::{Msg, tema};

type Paragrafo = <iced::Renderer as atext::Renderer>::Paragraph;

pub struct TextoSel {
    texto: String,
    tamanho: f32,
    cor: Color,
    fonte: Option<Font>,
}

/// Selectable text with the given size and color.
pub fn texto_sel(texto: impl Into<String>, tamanho: f32, cor: Color) -> TextoSel {
    TextoSel {
        texto: texto.into(),
        tamanho,
        cor,
        fonte: None,
    }
}

impl TextoSel {
    pub fn fonte(mut self, f: Font) -> Self {
        self.fonte = Some(f);
        self
    }
}

#[derive(Default)]
struct Estado {
    paragrafo: Option<Paragrafo>,
    chave: (String, f32, f32),
    /// Byte offset of each span in the text.
    inicios: Vec<usize>,
    ancora: Option<usize>,
    ponta: Option<usize>,
    arrastando: bool,
    ultimo_clique: Option<std::time::Instant>,
}

impl Estado {
    fn intervalo(&self) -> Option<(usize, usize)> {
        let (a, b) = (self.ancora?, self.ponta?);
        Some((a.min(b), a.max(b)))
    }

    /// Span under (or nearest to) `p`, in local coordinates.
    fn span_em(&self, p: Point) -> Option<usize> {
        let par = self.paragrafo.as_ref()?;
        if let Some(i) = par.hit_span(p) {
            return Some(i);
        }
        let mut melhor: Option<(f32, usize)> = None;
        for i in 0..self.inicios.len() {
            for r in par.span_bounds(i) {
                let dy = if p.y < r.y {
                    r.y - p.y
                } else {
                    (p.y - (r.y + r.height)).max(0.0)
                };
                let dx = if p.x < r.x {
                    r.x - p.x
                } else {
                    (p.x - (r.x + r.width)).max(0.0)
                };
                let d = dy * 1000.0 + dx;
                if melhor.is_none_or(|(m, _)| d < m) {
                    melhor = Some((d, i));
                }
            }
        }
        melhor.map(|(_, i)| i)
    }

    fn selecionado<'t>(&self, texto: &'t str) -> Option<&'t str> {
        let (a, b) = self.intervalo()?;
        let ini = *self.inicios.get(a)?;
        let fim = self.inicios.get(b + 1).copied().unwrap_or(texto.len());
        Some(&texto[ini..fim])
    }
}

/// One span per character; emoji runs stay whole (and in the emoji font).
fn spans(texto: &str) -> (Vec<atext::Span<'_, (), Font>>, Vec<usize>) {
    let base = texto.as_ptr() as usize;
    let mut v = Vec::new();
    let mut inicios = Vec::new();
    for (e, pedaco) in tema::trechos_de_emoji(texto) {
        let ini = pedaco.as_ptr() as usize - base;
        if e {
            inicios.push(ini);
            let mut sp = atext::Span::new(pedaco);
            sp.font = Some(tema::FONTE_EMOJI);
            v.push(sp);
        } else {
            for (i, ch) in pedaco.char_indices() {
                inicios.push(ini + i);
                v.push(atext::Span::new(&pedaco[i..i + ch.len_utf8()]));
            }
        }
    }
    (v, inicios)
}

impl Widget<Msg, Theme, iced::Renderer> for TextoSel {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Estado>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Estado::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Shrink, Length::Shrink)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let est = tree.state.downcast_mut::<Estado>();
        let largura = limits.max().width;
        let chave = (self.texto.clone(), self.tamanho, largura);
        if est.paragrafo.is_none() || est.chave != chave {
            let (sp, inicios) = spans(&self.texto);
            est.paragrafo = Some(Paragrafo::with_spans(atext::Text {
                content: sp.as_slice(),
                bounds: Size::new(largura, f32::INFINITY),
                size: self.tamanho.into(),
                line_height: atext::LineHeight::default(),
                font: self.fonte.unwrap_or_else(|| renderer.default_font()),
                align_x: atext::Alignment::Default,
                align_y: iced::alignment::Vertical::Top,
                shaping: atext::Shaping::Advanced,
                wrapping: atext::Wrapping::default(),
            }));
            if est.chave.0 != chave.0 {
                est.ancora = None;
                est.ponta = None;
            }
            est.chave = chave;
            est.inicios = inicios;
        }
        let tam = est
            .paragrafo
            .as_ref()
            .map(|p| p.min_bounds())
            .unwrap_or(Size::ZERO);
        layout::Node::new(limits.resolve(Length::Shrink, Length::Shrink, tam))
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Msg>,
        _viewport: &Rectangle,
    ) {
        let est = tree.state.downcast_mut::<Estado>();
        let b = layout.bounds();
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(p) = cursor.position_in(b) {
                    let agora = std::time::Instant::now();
                    let duplo = est
                        .ultimo_clique
                        .is_some_and(|t| agora.duration_since(t).as_millis() < 400);
                    est.ultimo_clique = Some(agora);
                    if duplo && !est.inicios.is_empty() {
                        est.ancora = Some(0);
                        est.ponta = Some(est.inicios.len() - 1);
                        est.arrastando = false;
                    } else {
                        let i = est.span_em(p);
                        est.ancora = i;
                        est.ponta = None;
                        est.arrastando = true;
                    }
                    // Only one selection at a time: drop the message texts' one.
                    shell.publish(Msg::LimpaSelecoes);
                    shell.request_redraw();
                } else if est.ancora.is_some() {
                    est.ancora = None;
                    est.ponta = None;
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) if est.arrastando => {
                if let Some(p) = cursor.position() {
                    let local = Point::new(p.x - b.x, p.y - b.y);
                    let i = est.span_em(local);
                    if i != est.ponta {
                        est.ponta = i;
                        shell.request_redraw();
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                est.arrastando = false;
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Character(c),
                modifiers,
                ..
            }) if c.as_str() == "c" && modifiers.command() => {
                if let Some(t) = est.selecionado(&self.texto) {
                    clipboard.write(clipboard::Kind::Standard, t.to_string());
                    shell.capture_event();
                }
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::None
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let est = tree.state.downcast_ref::<Estado>();
        let Some(par) = est.paragrafo.as_ref() else {
            return;
        };
        let b = layout.bounds();
        if let Some((a, z)) = est.intervalo() {
            let cor = Color {
                a: 0.35,
                ..tema::cores().destaque
            };
            for i in a..=z {
                for r in par.span_bounds(i) {
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: Rectangle {
                                x: b.x + r.x,
                                y: b.y + r.y,
                                width: r.width,
                                height: r.height,
                            },
                            ..Default::default()
                        },
                        cor,
                    );
                }
            }
        }
        renderer.fill_paragraph(par, b.position(), self.cor, *viewport);
    }
}

impl<'a> From<TextoSel> for Element<'a, Msg> {
    fn from(t: TextoSel) -> Self {
        Element::new(t)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn um_span_por_letra_e_emoji_inteiro() {
        let (sp, ini) = spans("oi 👍🏽!");
        assert_eq!(sp.len(), 5);
        assert_eq!(ini, vec![0, 1, 2, 3, 3 + "👍🏽".len()]);
    }

    #[test]
    fn selecao_vira_texto() {
        let est = Estado {
            inicios: vec![0, 1, 2, 3],
            ancora: Some(2),
            ponta: Some(1),
            ..Default::default()
        };
        assert_eq!(est.selecionado("abcd"), Some("bc"));
    }
}
