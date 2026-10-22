//! Desenha um elemento maior que o espaço que ele ocupa no layout, centrado.
//! É o que a decoração de avatar precisa: ela passa da borda da foto sem
//! empurrar o resto da linha. O `float` do iced faz parecido, mas desenha numa
//! camada por cima de tudo (inclusive das janelas abertas); este fica na ordem
//! normal do desenho. Não recebe eventos: é só imagem.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::widget::{self, Tree, Widget};
use iced::advanced::{Clipboard, Shell, mouse};
use iced::{Element, Event, Length, Point, Rectangle, Size};

use crate::Msg;

pub struct Transborda<'a> {
    conteudo: Element<'a, Msg>,
    lado: Size,
    escala: f32,
}

/// `conteudo` ocupa `lado` no layout e é desenhado `escala` vezes maior.
pub fn transborda<'a>(
    conteudo: impl Into<Element<'a, Msg>>,
    lado: Size,
    escala: f32,
) -> Element<'a, Msg> {
    Element::new(Transborda {
        conteudo: conteudo.into(),
        lado,
        escala,
    })
}

impl Widget<Msg, iced::Theme, iced::Renderer> for Transborda<'_> {
    fn tag(&self) -> widget::tree::Tag {
        self.conteudo.as_widget().tag()
    }

    fn state(&self) -> widget::tree::State {
        self.conteudo.as_widget().state()
    }

    fn children(&self) -> Vec<Tree> {
        self.conteudo.as_widget().children()
    }

    fn diff(&self, tree: &mut Tree) {
        self.conteudo.as_widget().diff(tree);
    }

    fn size(&self) -> Size<Length> {
        Size::new(
            Length::Fixed(self.lado.width),
            Length::Fixed(self.lado.height),
        )
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        _limits: &layout::Limits,
    ) -> layout::Node {
        let grande = Size::new(
            self.lado.width * self.escala,
            self.lado.height * self.escala,
        );
        let filho = self
            .conteudo
            .as_widget_mut()
            .layout(tree, renderer, &layout::Limits::new(grande, grande))
            .move_to(Point::new(
                (self.lado.width - grande.width) / 2.0,
                (self.lado.height - grande.height) / 2.0,
            ));
        layout::Node::with_children(self.lado, vec![filho])
    }

    fn update(
        &mut self,
        _tree: &mut Tree,
        _event: &Event,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _renderer: &iced::Renderer,
        _clipboard: &mut dyn Clipboard,
        _shell: &mut Shell<'_, Msg>,
        _viewport: &Rectangle,
    ) {
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        if let Some(filho) = layout.children().next() {
            self.conteudo
                .as_widget()
                .draw(tree, renderer, theme, style, filho, cursor, viewport);
        }
    }
}

/// Moldura de perfil por cima de um cartão: camada de um `stack` cujo tamanho
/// é o do cartão. "Topo": a faixa vai na largura do cartão, toda acima da
/// borda de cima, com a base apoiada nela (por cima da interface, fora do
/// recorte do cartão); "inteira": esticada sobre o cartão todo.
/// Quanto a base da faixa entra no cartão (fração da altura dela).
const SOBREPOE: f32 = 0.25;
/// O máximo que fica acima da borda (fração da largura do cartão): faixa alta
/// (olhos, velas, rosas) desce por cima do banner, como a arte foi feita.
const ACIMA_MAX: f32 = 0.15;

pub struct MolduraSobre<'a> {
    conteudo: Element<'a, Msg>,
    inteira: bool,
    /// Altura / largura da imagem (para a faixa ir na largura toda).
    proporcao: f32,
}

pub fn moldura_sobre<'a>(
    conteudo: impl Into<Element<'a, Msg>>,
    inteira: bool,
    proporcao: f32,
) -> Element<'a, Msg> {
    Element::new(MolduraSobre {
        conteudo: conteudo.into(),
        inteira,
        proporcao,
    })
}

impl Widget<Msg, iced::Theme, iced::Renderer> for MolduraSobre<'_> {
    fn tag(&self) -> widget::tree::Tag {
        self.conteudo.as_widget().tag()
    }

    fn state(&self) -> widget::tree::State {
        self.conteudo.as_widget().state()
    }

    fn children(&self) -> Vec<Tree> {
        self.conteudo.as_widget().children()
    }

    fn diff(&self, tree: &mut Tree) {
        self.conteudo.as_widget().diff(tree);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let cartao = limits.max();
        // A arte do Discord já vem desenhada para a largura do cartão, presa
        // na borda de cima (com as margens transparentes no lugar certo).
        let alto = if self.inteira {
            cartao.height
        } else {
            cartao.width * self.proporcao
        };
        let tam = Size::new(cartao.width, alto);
        let filho =
            self.conteudo
                .as_widget_mut()
                .layout(tree, renderer, &layout::Limits::new(tam, tam));
        // A faixa fica toda acima do cartão, com a base apoiada na borda.
        let filho = if self.inteira {
            filho
        } else {
            filho.move_to(Point::new(
                0.0,
                -(alto * (1.0 - SOBREPOE)).min(cartao.width * ACIMA_MAX),
            ))
        };
        layout::Node::with_children(cartao, vec![filho])
    }

    fn update(
        &mut self,
        _tree: &mut Tree,
        _event: &Event,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _renderer: &iced::Renderer,
        _clipboard: &mut dyn Clipboard,
        _shell: &mut Shell<'_, Msg>,
        _viewport: &Rectangle,
    ) {
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        if let Some(filho) = layout.children().next() {
            // A moldura sai um pouco do cartão: desenha sem o recorte da
            // janela de rolagem em volta.
            let area = filho.bounds().union(viewport);
            // Com a janela baixa a faixa sairia pelo topo: desce o
            // necessário para aparecer inteira.
            let desce = (-filho.bounds().y).max(0.0);
            renderer.with_translation(iced::Vector::new(0.0, desce), |r| {
                self.conteudo
                    .as_widget()
                    .draw(tree, r, theme, style, filho, cursor, &area);
            });
        }
    }
}
