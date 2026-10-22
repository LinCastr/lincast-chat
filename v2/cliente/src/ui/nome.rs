//! Nome exibido com estilo (fonte, efeito e cores), como o "Estilo do nome
//! exibido" do Nitro. O iced não tem sombra nem degradê em texto, então os
//! efeitos são feitos com camadas do mesmo texto deslocadas (contorno, brilho,
//! sombra) e com uma cor por letra (degradê, prisma).

use iced::widget::text::Span;
use iced::widget::{container, span, stack, text};
use iced::{Color, Element, Font, Padding, font};

use crate::Msg;
use crate::cosmeticos::EstiloNome;

pub struct Fonte {
    pub id: &'static str,
    pub rotulo: &'static str,
    pub fonte: Font,
    /// Algumas fontes são bem maiores que outras no mesmo tamanho.
    pub escala: f32,
}

const fn f(nome: &'static str) -> Font {
    Font::with_name(nome)
}

pub const FONTES: [Fonte; 12] = [
    Fonte {
        id: "bangers",
        rotulo: "Bangers",
        fonte: f("Bangers"),
        escala: 1.05,
    },
    Fonte {
        id: "cherry",
        rotulo: "Cherry Bomb",
        fonte: f("Cherry Bomb One"),
        escala: 0.95,
    },
    Fonte {
        id: "chicle",
        rotulo: "Chicle",
        fonte: f("Chicle"),
        escala: 1.05,
    },
    Fonte {
        id: "pixel",
        rotulo: "Pixelify",
        fonte: f("Pixelify Sans"),
        escala: 1.0,
    },
    Fonte {
        id: "zilla",
        rotulo: "Zilla Slab",
        fonte: Font {
            weight: font::Weight::Bold,
            ..f("Zilla Slab")
        },
        escala: 1.0,
    },
    Fonte {
        id: "pacifico",
        rotulo: "Pacifico",
        fonte: f("Pacifico"),
        escala: 0.9,
    },
    Fonte {
        id: "creepster",
        rotulo: "Creepster",
        fonte: f("Creepster"),
        escala: 1.05,
    },
    Fonte {
        id: "orbitron",
        rotulo: "Orbitron",
        fonte: f("Orbitron"),
        escala: 0.85,
    },
    Fonte {
        id: "pressstart",
        rotulo: "Press Start",
        fonte: f("Press Start 2P"),
        escala: 0.62,
    },
    Fonte {
        id: "righteous",
        rotulo: "Righteous",
        fonte: f("Righteous"),
        escala: 1.0,
    },
    Fonte {
        id: "monoton",
        rotulo: "Monoton",
        fonte: f("Monoton"),
        escala: 0.8,
    },
    Fonte {
        id: "silkscreen",
        rotulo: "Silkscreen",
        fonte: f("Silkscreen"),
        escala: 0.85,
    },
];

pub const EFEITOS: [(&str, &str); 7] = [
    ("solido", "Sólido"),
    ("gradiente", "Gradiente"),
    ("neon", "Neon"),
    ("desenho", "Desenho"),
    ("pop", "Pop"),
    ("gummy", "Gummy"),
    ("prisma", "Prisma"),
];

/// Cores prontas da paleta (a primeira posição da linha é o "personalizar").
pub const CORES: [&str; 12] = [
    "#7c3aed", "#ec4899", "#22c55e", "#0ea5e9", "#c026d3", "#e11d48", "#f97316", "#eab308",
    "#14b8a6", "#f8fafc", "#a3e635", "#60a5fa",
];

/// Os arquivos das fontes, para registrar na abertura.
pub const ARQUIVOS: [&[u8]; 12] = [
    include_bytes!("../../assets/fonts/nome/Bangers-Regular.ttf"),
    include_bytes!("../../assets/fonts/nome/CherryBombOne-Regular.ttf"),
    include_bytes!("../../assets/fonts/nome/Chicle-Regular.ttf"),
    include_bytes!("../../assets/fonts/nome/PixelifySans-Var.ttf"),
    include_bytes!("../../assets/fonts/nome/ZillaSlab-Bold.ttf"),
    include_bytes!("../../assets/fonts/nome/Pacifico-Regular.ttf"),
    include_bytes!("../../assets/fonts/nome/Creepster-Regular.ttf"),
    include_bytes!("../../assets/fonts/nome/Orbitron-Var.ttf"),
    include_bytes!("../../assets/fonts/nome/PressStart2P-Regular.ttf"),
    include_bytes!("../../assets/fonts/nome/Righteous-Regular.ttf"),
    include_bytes!("../../assets/fonts/nome/Monoton-Regular.ttf"),
    include_bytes!("../../assets/fonts/nome/Silkscreen-Regular.ttf"),
];

pub fn fonte(id: &str) -> Option<&'static Fonte> {
    FONTES.iter().find(|f| f.id == id)
}

pub fn cor_hex(hex: &str) -> Option<Color> {
    crate::ui::perfil::parse_hex_color(hex)
}

pub fn hex(c: Color) -> String {
    let [r, g, b, _] = c.into_rgba8();
    format!("#{r:02x}{g:02x}{b:02x}")
}

fn mistura(a: Color, b: Color, t: f32) -> Color {
    Color::from_rgb(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
    )
}

fn escurece(c: Color, k: f32) -> Color {
    Color::from_rgb(c.r * k, c.g * k, c.b * k)
}

fn com_alfa(c: Color, a: f32) -> Color {
    Color { a, ..c }
}

fn hsv(h: f32, s: f32, v: f32) -> Color {
    let h = h.rem_euclid(360.0) / 60.0;
    let i = h.floor();
    let f = h - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    let (r, g, b) = match i as i32 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    Color::from_rgb(r, g, b)
}

/// A segunda cor quando a pessoa não escolheu: um tom ao lado da primeira.
fn segunda(c: Color) -> Color {
    mistura(c, Color::WHITE, 0.55)
}

/// Uma cor por letra.
fn spans_por_letra<'a>(t: &str, fonte: Font, cor: impl Fn(f32) -> Color) -> Vec<Span<'a, ()>> {
    let letras: Vec<char> = t.chars().collect();
    let n = letras.len().max(2) as f32 - 1.0;
    letras
        .into_iter()
        .enumerate()
        .map(|(i, ch)| span(ch.to_string()).color(cor(i as f32 / n)).font(fonte))
        .collect()
}

fn camada<'a>(e: Element<'a, Msg>, dx: f32, dy: f32) -> Element<'a, Msg> {
    const P: f32 = 3.0;
    container(e)
        .padding(Padding {
            top: P + dy,
            bottom: P - dy,
            left: P + dx,
            right: P - dx,
        })
        .into()
}

/// Nome com o estilo dado. Sem estilo (ou estilo vazio): texto normal em
/// `cor_padrao` e na fonte `fonte_padrao`.
pub fn nome<'a>(
    t: &str,
    estilo: Option<&EstiloNome>,
    tamanho: f32,
    cor_padrao: Color,
    fonte_padrao: Font,
) -> Element<'a, Msg> {
    let Some(e) = estilo.filter(|e| !e.vazio()) else {
        return text(t.to_string())
            .size(tamanho)
            .color(cor_padrao)
            .font(fonte_padrao)
            .into();
    };
    let (fonte, tam) = match e.fonte.as_deref().and_then(fonte) {
        Some(f) => (f.fonte, tamanho * f.escala),
        None => (fonte_padrao, tamanho),
    };
    let c1 = e.cor.as_deref().and_then(cor_hex).unwrap_or(cor_padrao);
    let c2 = e
        .cor2
        .as_deref()
        .and_then(cor_hex)
        .unwrap_or_else(|| segunda(c1));
    let simples = |cor: Color| -> Element<'a, Msg> {
        text(t.to_string()).size(tam).color(cor).font(fonte).into()
    };
    match e.efeito.as_deref().unwrap_or("solido") {
        "gradiente" => iced::widget::rich_text(spans_por_letra(t, fonte, |k| mistura(c1, c2, k)))
            .size(tam)
            .into(),
        "prisma" => {
            let base = {
                let [r, g, b, _] = c1.into_rgba8();
                let (r, g, b) = (f32::from(r), f32::from(g), f32::from(b));
                let max = r.max(g).max(b);
                let min = r.min(g).min(b);
                let d = (max - min).max(1.0);
                (if max == r {
                    ((g - b) / d).rem_euclid(6.0)
                } else if max == g {
                    (b - r) / d + 2.0
                } else {
                    (r - g) / d + 4.0
                }) * 60.0
            };
            iced::widget::rich_text(spans_por_letra(t, fonte, |k| {
                hsv(base + k * 300.0, 0.65, 1.0)
            }))
            .size(tam)
            .into()
        }
        "neon" => {
            let mut camadas: Vec<Element<'a, Msg>> = Vec::new();
            for (dx, dy) in [
                (-2.0, 0.0),
                (2.0, 0.0),
                (0.0, -2.0),
                (0.0, 2.0),
                (-1.5, -1.5),
                (1.5, 1.5),
                (-1.5, 1.5),
                (1.5, -1.5),
            ] {
                camadas.push(camada(simples(com_alfa(c1, 0.28)), dx, dy));
            }
            camadas.push(camada(simples(mistura(c1, Color::WHITE, 0.7)), 0.0, 0.0));
            stack(camadas).into()
        }
        "desenho" => {
            let borda = escurece(c1, 0.22);
            let mut camadas: Vec<Element<'a, Msg>> = Vec::new();
            for (dx, dy) in [
                (-1.5, 0.0),
                (1.5, 0.0),
                (0.0, -1.5),
                (0.0, 1.5),
                (-1.0, -1.0),
                (1.0, 1.0),
                (-1.0, 1.0),
                (1.0, -1.0),
            ] {
                camadas.push(camada(simples(borda), dx, dy));
            }
            camadas.push(camada(simples(c1), 0.0, 0.0));
            stack(camadas).into()
        }
        "pop" => stack![camada(simples(c2), 2.0, 2.0), camada(simples(c1), 0.0, 0.0)].into(),
        "gummy" => stack![
            camada(simples(escurece(c1, 0.55)), 0.0, 2.0),
            camada(simples(com_alfa(Color::WHITE, 0.55)), 0.0, -1.0),
            camada(simples(c1), 0.0, 0.0)
        ]
        .into(),
        _ => simples(c1),
    }
}

/// Nome de uma pessoa (uid) com o estilo que ela escolheu.
pub fn nome_de<'a>(
    uid: &str,
    t: &str,
    tamanho: f32,
    cor_padrao: Color,
    fonte_padrao: Font,
) -> Element<'a, Msg> {
    let e = crate::cosmeticos::de(uid);
    nome(t, e.nome.as_ref(), tamanho, cor_padrao, fonte_padrao)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn fontes_tem_arquivo_e_id_unico() {
        assert_eq!(FONTES.len(), ARQUIVOS.len());
        let mut ids: Vec<_> = FONTES.iter().map(|f| f.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), FONTES.len());
    }

    #[test]
    fn hex_ida_e_volta() {
        assert_eq!(hex(cor_hex("#7c3aed").unwrap()), "#7c3aed");
    }
}
