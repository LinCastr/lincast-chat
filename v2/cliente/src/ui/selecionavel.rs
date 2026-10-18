//! Texto de mensagem que dá para selecionar e copiar (Ctrl+C), como no
//! Discord. O iced não tem texto selecionável: é um `text_editor` só de
//! leitura (as ações de edição são descartadas em `SalaAberta`), com links e
//! emoji realçados por um `Highlighter` próprio e sem roubar a roda do mouse
//! da rolagem do chat (`SemRolagem`).

use std::ops::Range;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Tree, Widget};
use iced::advanced::{Clipboard, Shell, mouse, overlay};
use iced::widget::text_editor::{self, Content};
use iced::{Color, Element, Event, Font, Length, Rectangle, Size, Theme, Vector};

use crate::ui::chat::{SegmentoTexto, separa_links};
use crate::{Msg, tema};

/// Um trecho de texto de mensagem já pronto para seleção.
pub struct TextoSelecionavel {
    pub texto: String,
    pub conteudo: Content,
}

impl TextoSelecionavel {
    pub fn novo(texto: &str) -> Self {
        Self {
            texto: texto.to_string(),
            conteudo: Content::with_text(texto),
        }
    }

    /// Link sob o cursor (depois de um clique sem seleção).
    pub fn link_no_cursor(&self) -> Option<String> {
        let c = self.conteudo.cursor();
        if c.selection.is_some_and(|s| s != c.position) {
            return None;
        }
        let linha = self.conteudo.line(c.position.line)?;
        links_da_linha(&linha.text)
            .into_iter()
            .find(|(r, _)| r.contains(&c.position.column))
            .map(|(_, url)| url)
    }
}

fn links_da_linha(linha: &str) -> Vec<(Range<usize>, String)> {
    let base = linha.as_ptr() as usize;
    separa_links(linha)
        .into_iter()
        .filter_map(|s| match s {
            SegmentoTexto::Link(u) => {
                let ini = u.as_ptr() as usize - base;
                Some((ini..ini + u.len(), u.to_string()))
            }
            SegmentoTexto::Texto(_) => None,
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
pub enum Realce {
    Link,
    Emoji,
}

/// Pinta links com a cor de destaque e põe emoji na fonte colorida.
pub struct RealceChat {
    linha: usize,
}

impl iced::advanced::text::Highlighter for RealceChat {
    type Settings = ();
    type Highlight = Realce;
    type Iterator<'a> = std::vec::IntoIter<(Range<usize>, Realce)>;

    fn new(_: &()) -> Self {
        Self { linha: 0 }
    }

    fn update(&mut self, _: &()) {}

    fn change_line(&mut self, linha: usize) {
        self.linha = linha;
    }

    fn highlight_line(&mut self, linha: &str) -> Self::Iterator<'_> {
        self.linha += 1;
        let base = linha.as_ptr() as usize;
        let mut v: Vec<(Range<usize>, Realce)> = links_da_linha(linha)
            .into_iter()
            .map(|(r, _)| (r, Realce::Link))
            .collect();
        for (e, pedaco) in tema::trechos_de_emoji(linha) {
            if e {
                let ini = pedaco.as_ptr() as usize - base;
                v.push((ini..ini + pedaco.len(), Realce::Emoji));
            }
        }
        v.into_iter()
    }

    fn current_line(&self) -> usize {
        self.linha
    }
}

fn formato(r: &Realce, _: &Theme) -> iced::advanced::text::highlighter::Format<Font> {
    match r {
        Realce::Link => iced::advanced::text::highlighter::Format {
            color: Some(tema::cores().destaque),
            font: None,
        },
        Realce::Emoji => iced::advanced::text::highlighter::Format {
            color: None,
            font: Some(tema::FONTE_EMOJI),
        },
    }
}

/// O texto selecionável da chave `chave` (mensagem + trecho).
pub fn vista<'a>(chave: &str, sel: &'a TextoSelecionavel, tamanho: f32) -> Element<'a, Msg> {
    let chave = chave.to_string();
    let editor = iced::widget::text_editor(&sel.conteudo)
        .on_action(move |a| Msg::SelecaoTexto(chave.clone(), a))
        .highlight_with::<RealceChat>((), formato)
        .size(tamanho)
        .padding(0)
        .height(Length::Shrink)
        .style(|_, _| {
            let c = tema::cores();
            text_editor::Style {
                background: iced::Background::Color(Color::TRANSPARENT),
                border: iced::Border::default(),
                placeholder: c.texto_fraco,
                value: c.texto,
                selection: Color {
                    a: 0.35,
                    ..c.destaque
                },
            }
        });
    Element::new(SemRolagem {
        conteudo: editor.into(),
    })
}

/// Repassa tudo ao filho, menos a roda do mouse: assim a rolagem do chat
/// continua funcionando com o mouse em cima do texto.
struct SemRolagem<'a> {
    conteudo: Element<'a, Msg>,
}

impl Widget<Msg, Theme, iced::Renderer> for SemRolagem<'_> {
    fn size(&self) -> Size<Length> {
        self.conteudo.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.conteudo.as_widget().size_hint()
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.conteudo)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.conteudo));
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.conteudo
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        self.conteudo
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Msg>,
        viewport: &Rectangle,
    ) {
        if matches!(event, Event::Mouse(mouse::Event::WheelScrolled { .. })) {
            return;
        }
        self.conteudo.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.conteudo.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.conteudo.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Msg, Theme, iced::Renderer>> {
        self.conteudo.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn acha_link_pela_posicao_na_linha() {
        let l = links_da_linha("veja https://x.com/a agora");
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].0, 5..20);
        assert_eq!(l[0].1, "https://x.com/a");
    }
}
