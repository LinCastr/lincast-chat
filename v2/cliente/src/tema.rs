//! Cores e estilos. Nenhuma cor solta fora daqui: toda tela pega de `Cores`,
//! que é o que o wallust vai preencher depois (um arquivo gerado por template
//! → `Cores`). Mesma regra da v1, que aprendeu do jeito difícil que hex solto
//! numa regra faz aquele pedaço parar de trocar de tema em silêncio.
//!
//! Direção visual (AGENTS.md, seção 9): sóbrio, denso, hierarquia pelo texto.
//! Um destaque por tela, só no que está ligado ou é a ação principal. Nada de
//! gradiente, brilho ou sombra colorida. Borda fina e neutra, raio pequeno.

use std::path::PathBuf;
use std::sync::RwLock;

use iced::border::Radius;
use iced::widget::{button, container, scrollable, slider, svg, text_input};
use iced::{Background, Border, Color, Shadow, Theme};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cores {
    /// Fundo da área principal (chat, conteúdo).
    pub fundo: Color,
    /// Barras laterais e cabeçalho: um degrau abaixo do fundo.
    pub lateral: Color,
    /// Campo de texto, item em foco, hover.
    pub elevado: Color,
    /// Linhas divisórias e contorno de campo.
    pub borda: Color,
    pub texto: Color,
    /// Rótulos, horários, informação de apoio.
    pub texto_fraco: Color,
    pub destaque: Color,
    /// Texto em cima do destaque (escolhido por contraste, não por gosto).
    pub sobre_destaque: Color,
    pub perigo: Color,
    pub sucesso: Color,
    /// Palco da chamada e fundo da janela: o degrau mais fundo (`--bg-0`).
    pub palco: Color,
    /// Hover forte e botões neutros: um degrau acima do elevado (`--bg-4`).
    pub forte: Color,
    /// Texto secundário legível (`--fg-2`); `texto_fraco` é o terciário (`--fg-3`).
    pub texto_medio: Color,
    /// Aviso e ausente (`--amber`).
    pub alerta: Color,
    /// Verde de ação (entrar na chamada, atender) (`--green-hi`).
    pub sucesso_forte: Color,
}

pub const fn rgb(hex: u32) -> Color {
    Color {
        r: ((hex >> 16) & 0xff) as f32 / 255.0,
        g: ((hex >> 8) & 0xff) as f32 / 255.0,
        b: (hex & 0xff) as f32 / 255.0,
        a: 1.0,
    }
}

/// Converte string hexadecimal (#RRGGBB ou RRGGBB) para Color do Iced.
pub fn parse_hex(hex: &str) -> Option<Color> {
    let limpo = hex.trim().trim_start_matches('#');
    if limpo.len() != 6 {
        return None;
    }
    let val = u32::from_str_radix(limpo, 16).ok()?;
    Some(rgb(val))
}

/// Luminância relativa perceptual aproximada (padrão WCAG).
pub fn luminancia(c: Color) -> f32 {
    0.299 * c.r + 0.587 * c.g + 0.114 * c.b
}

/// Retorna cor de texto de alto contraste (preto ou quase branco) para sobrepor uma cor de fundo.
pub fn sobre_contraste(c: Color) -> Color {
    if luminancia(c) > 0.55 {
        rgb(0x0f1115)
    } else {
        rgb(0xf8f8f2)
    }
}

// --- Paletas da v1 e predefinições ---

/// Tema padrão: grafite neutro com azul contido de destaque.
pub const GRAFITE: Cores = Cores {
    fundo: rgb(0x232429),
    lateral: rgb(0x1c1d22),
    elevado: rgb(0x2c2e35),
    borda: rgb(0x30323a),
    texto: rgb(0xe6e7ec),
    texto_fraco: rgb(0x9ca0b2),
    destaque: rgb(0x6f86f0),
    sobre_destaque: rgb(0xffffff),
    perigo: rgb(0xe0434b),
    sucesso: rgb(0x2fa863),
    palco: rgb(0x15161a),
    forte: rgb(0x373a43),
    texto_medio: rgb(0xa9acb8),
    alerta: rgb(0xe2a336),
    sucesso_forte: rgb(0x3cc476),
};

/// Paleta Nord clássica (tons árticos sóbrios).
pub const NORD: Cores = Cores {
    fundo: rgb(0x2e3440),
    lateral: rgb(0x242933),
    elevado: rgb(0x3b4252),
    borda: rgb(0x434c5e),
    texto: rgb(0xeceff4),
    texto_fraco: rgb(0xd8dee9),
    destaque: rgb(0x88c0d0),
    sobre_destaque: rgb(0x242933),
    perigo: rgb(0xbf616a),
    sucesso: rgb(0xa3be8c),
    palco: rgb(0x191d24),
    forte: rgb(0x494f5e),
    texto_medio: rgb(0xe2e6ee),
    alerta: rgb(0xe2a336),
    sucesso_forte: rgb(0xb1c89d),
};

/// Paleta Drácula (tons roxos e escuros com destaques vibrantes).
pub const DRACULA: Cores = Cores {
    fundo: rgb(0x282a36),
    lateral: rgb(0x21222c),
    elevado: rgb(0x343746),
    borda: rgb(0x44475a),
    texto: rgb(0xf8f8f2),
    texto_fraco: rgb(0x929dc2),
    destaque: rgb(0xbd93f9),
    sobre_destaque: rgb(0x21222c),
    perigo: rgb(0xff5555),
    sucesso: rgb(0x50fa7b),
    palco: rgb(0x17181f),
    forte: rgb(0x424553),
    texto_medio: rgb(0xadb5cb),
    alerta: rgb(0xe2a336),
    sucesso_forte: rgb(0x6afb8f),
};

/// Paleta Tokyo Night.
pub const TOKYO_NIGHT: Cores = Cores {
    fundo: rgb(0x1a1b26),
    lateral: rgb(0x16161e),
    elevado: rgb(0x24283b),
    borda: rgb(0x414868),
    texto: rgb(0xc0caf5),
    texto_fraco: rgb(0x8690b6),
    destaque: rgb(0x7aa2f7),
    sobre_destaque: rgb(0x15161e),
    perigo: rgb(0xf7768e),
    sucesso: rgb(0x9ece6a),
    palco: rgb(0x0f0f15),
    forte: rgb(0x333749),
    texto_medio: rgb(0x8b94bf),
    alerta: rgb(0xe2a336),
    sucesso_forte: rgb(0xadd580),
};

/// Paleta Catppuccin Mocha.
pub const CATPPUCCIN: Cores = Cores {
    fundo: rgb(0x1e1e2e),
    lateral: rgb(0x181825),
    elevado: rgb(0x313244),
    borda: rgb(0x45475a),
    texto: rgb(0xcdd6f4),
    texto_fraco: rgb(0xa6adc8),
    destaque: rgb(0xcba6f7),
    sobre_destaque: rgb(0x11111b),
    perigo: rgb(0xf38ba8),
    sucesso: rgb(0xa6e3a1),
    palco: rgb(0x11111a),
    forte: rgb(0x3f4051),
    texto_medio: rgb(0xbac2de),
    alerta: rgb(0xe2a336),
    sucesso_forte: rgb(0xb3e7af),
};

/// Paleta Gruvbox Dark.
pub const GRUVBOX: Cores = Cores {
    fundo: rgb(0x282828),
    lateral: rgb(0x1d2021),
    elevado: rgb(0x3c3836),
    borda: rgb(0x504945),
    texto: rgb(0xebdbb2),
    texto_fraco: rgb(0xa89984),
    destaque: rgb(0xfabd2f),
    sobre_destaque: rgb(0x282828),
    perigo: rgb(0xfb4934),
    sucesso: rgb(0xb8bb26),
    palco: rgb(0x141617),
    forte: rgb(0x4a4644),
    texto_medio: rgb(0xcaba9b),
    alerta: rgb(0xe2a336),
    sucesso_forte: rgb(0xc3c547),
};

/// Paleta One Dark.
pub const ONE_DARK: Cores = Cores {
    fundo: rgb(0x282c34),
    lateral: rgb(0x21252b),
    elevado: rgb(0x353b45),
    borda: rgb(0x3e4451),
    texto: rgb(0xabb2bf),
    texto_fraco: rgb(0x8890a2),
    destaque: rgb(0x61afef),
    sobre_destaque: rgb(0x21252b),
    perigo: rgb(0xe06c75),
    sucesso: rgb(0x98c379),
    palco: rgb(0x171a1e),
    forte: rgb(0x434952),
    texto_medio: rgb(0x848a98),
    alerta: rgb(0xe2a336),
    sucesso_forte: rgb(0xa7cc8d),
};

/// Paleta Meia-noite (escuro profundo).
pub const MEIA_NOITE: Cores = Cores {
    fundo: rgb(0x0d0f12),
    lateral: rgb(0x08090b),
    elevado: rgb(0x16191f),
    borda: rgb(0x232730),
    texto: rgb(0xdce1eb),
    texto_fraco: rgb(0x9aa2b4),
    destaque: rgb(0x6a86b8),
    sobre_destaque: rgb(0x08090b),
    perigo: rgb(0xe05561),
    sucesso: rgb(0x4ec990),
    palco: rgb(0x060608),
    forte: rgb(0x26292f),
    texto_medio: rgb(0xa9b0be),
    alerta: rgb(0xe2a336),
    sucesso_forte: rgb(0x69d1a1),
};

/// Paleta Café (tons quentes terrosos).
pub const CAFE: Cores = Cores {
    fundo: rgb(0x241c15),
    lateral: rgb(0x1a140f),
    elevado: rgb(0x33271e),
    borda: rgb(0x4a3a2d),
    texto: rgb(0xede0d4),
    texto_fraco: rgb(0xb5a392),
    destaque: rgb(0xd9a05b),
    sobre_destaque: rgb(0x1a140f),
    perigo: rgb(0xe05a47),
    sucesso: rgb(0x8cb369),
    palco: rgb(0x120e0a),
    forte: rgb(0x41362e),
    texto_medio: rgb(0xc4b5a6),
    alerta: rgb(0xe2a336),
    sucesso_forte: rgb(0x9dbe80),
};

/// Paleta Clara (fundo claro sóbrio e legível).
pub const CLARO: Cores = Cores {
    fundo: rgb(0xf2f3f7),
    lateral: rgb(0xe5e7eb),
    elevado: rgb(0xffffff),
    borda: rgb(0xd1d5db),
    texto: rgb(0x1f2937),
    texto_fraco: rgb(0x6b7280),
    destaque: rgb(0x3d5afe),
    sobre_destaque: rgb(0xffffff),
    perigo: rgb(0xef4444),
    sucesso: rgb(0x10b981),
    palco: rgb(0xd7d9dd),
    forte: rgb(0xf0f0f0),
    texto_medio: rgb(0x454e5c),
    alerta: rgb(0xe2a336),
    sucesso_forte: rgb(0x34c494),
};

// --- Wallust Parser e integração dinâmica ---

/// Tenta carregar e montar as cores a partir dos arquivos de cache e configuração do Wallust.
pub fn extrai_cores_wallust() -> Option<Cores> {
    let home = std::env::var("HOME").ok().map(PathBuf::from)?;

    // 1. Tentar ler JSON gerado pelo template (~/.cache/wallust/colors-quickshell.json ou colors.json)
    let json_caminhos = [
        home.join(".cache/wallust/colors-quickshell.json"),
        home.join(".cache/wallust/colors.json"),
        home.join(".cache/wallust/nui.json"),
    ];

    for caminho in json_caminhos {
        if let Ok(conteudo) = std::fs::read_to_string(&caminho)
            && let Ok(v) = serde_json::from_str::<serde_json::Value>(&conteudo)
        {
            let bg = v
                .get("background")
                .and_then(|s| s.as_str())
                .and_then(parse_hex);
            let fg = v
                .get("foreground")
                .and_then(|s| s.as_str())
                .and_then(parse_hex);
            let cor4 = v.get("color4").and_then(|s| s.as_str()).and_then(parse_hex);
            let cor1 = v.get("color1").and_then(|s| s.as_str()).and_then(parse_hex);
            let cor2 = v.get("color2").and_then(|s| s.as_str()).and_then(parse_hex);

            if let (Some(bg), Some(fg)) = (bg, fg) {
                let destaque = cor4.unwrap_or(rgb(0x7f9cf5));
                return Some(constroi_paleta_base(bg, fg, destaque, cor1, cor2));
            }
        }
    }

    // 2. Tentar ler de ~/.config/hypr/colors.conf (variáveis do Wallust)
    let hypr_colors = home.join(".config/hypr/colors.conf");
    if let Ok(conteudo) = std::fs::read_to_string(&hypr_colors) {
        let mut bg = None;
        let mut fg = None;
        let mut accent = None;

        for linha in conteudo.lines() {
            let l = linha.trim();
            if let Some(resto) = l.strip_prefix("$wallust_background = ") {
                bg = parse_hex(resto);
            } else if let Some(resto) = l.strip_prefix("$wallust_foreground = ") {
                fg = parse_hex(resto);
            } else if let Some(resto) = l.strip_prefix("$wallust_accent1 = ") {
                accent = parse_hex(resto);
            }
        }

        if let (Some(bg), Some(fg)) = (bg, fg) {
            let destaque = accent.unwrap_or(rgb(0x7f9cf5));
            return Some(constroi_paleta_base(bg, fg, destaque, None, None));
        }
    }

    // 3. Tentar ler de ~/.config/kitty/colors-wallust.conf
    let kitty_colors = home.join(".config/kitty/colors-wallust.conf");
    if let Ok(conteudo) = std::fs::read_to_string(&kitty_colors) {
        let mut bg = None;
        let mut fg = None;
        let mut c4 = None;
        let mut c1 = None;
        let mut c2 = None;

        for linha in conteudo.lines() {
            let l = linha.trim();
            if let Some(resto) = l.strip_prefix("background ") {
                bg = parse_hex(resto);
            } else if let Some(resto) = l.strip_prefix("foreground ") {
                fg = parse_hex(resto);
            } else if let Some(resto) = l.strip_prefix("color4 ") {
                c4 = parse_hex(resto);
            } else if let Some(resto) = l.strip_prefix("color1 ") {
                c1 = parse_hex(resto);
            } else if let Some(resto) = l.strip_prefix("color2 ") {
                c2 = parse_hex(resto);
            }
        }

        if let (Some(bg), Some(fg)) = (bg, fg) {
            let destaque = c4.unwrap_or(rgb(0x7f9cf5));
            return Some(constroi_paleta_base(bg, fg, destaque, c1, c2));
        }
    }

    None
}

/// Constrói um conjunto coeso de cores a partir de um fundo, texto e destaque básicos.
pub fn constroi_paleta_base(
    bg: Color,
    fg: Color,
    destaque: Color,
    perigo_opt: Option<Color>,
    sucesso_opt: Option<Color>,
) -> Cores {
    let escuro = luminancia(bg) < 0.5;
    let lateral = if escuro {
        mistura(bg, Color::BLACK, 0.25)
    } else {
        mistura(bg, Color::BLACK, 0.06)
    };
    let elevado = if escuro {
        mistura(bg, Color::WHITE, 0.08)
    } else {
        mistura(bg, Color::WHITE, 0.25)
    };
    let borda = if escuro {
        mistura(bg, Color::WHITE, 0.16)
    } else {
        mistura(bg, Color::BLACK, 0.15)
    };
    let texto_fraco = mistura(fg, bg, 0.45);
    let sobre_destaque = sobre_contraste(destaque);
    let perigo = perigo_opt.unwrap_or(rgb(0xe5686b));
    let sucesso = sucesso_opt.unwrap_or(rgb(0x5fbf8f));

    Cores {
        fundo: bg,
        lateral,
        elevado,
        borda,
        texto: fg,
        texto_fraco,
        destaque,
        sobre_destaque,
        perigo,
        sucesso,
        palco: if escuro {
            mistura(bg, Color::BLACK, 0.40)
        } else {
            mistura(bg, Color::BLACK, 0.10)
        },
        forte: if escuro {
            mistura(bg, Color::WHITE, 0.14)
        } else {
            mistura(bg, Color::BLACK, 0.20)
        },
        texto_medio: mistura(fg, bg, 0.25),
        alerta: rgb(0xe2a336),
        sucesso_forte: mistura(sucesso, Color::WHITE, 0.15),
    }
}

/// Verifica se há arquivos de cores do Wallust disponíveis para leitura.
pub fn wallust_disponivel() -> bool {
    extrai_cores_wallust().is_some()
}

// --- Estado Global e Controle de Tema ---

static TEMA_ATUAL: RwLock<Cores> = RwLock::new(GRAFITE);
static ID_TEMA_ATUAL: RwLock<String> = RwLock::new(String::new());

pub fn cores() -> Cores {
    *TEMA_ATUAL.read().unwrap_or_else(|p| p.into_inner())
}

pub fn tema_atual_id() -> String {
    let id = ID_TEMA_ATUAL.read().unwrap_or_else(|p| p.into_inner());
    if id.is_empty() {
        "padrao".to_string()
    } else {
        id.clone()
    }
}

pub fn define_tema(id: &str) -> bool {
    let novas_cores = match id {
        "wallust" => extrai_cores_wallust().unwrap_or(GRAFITE),
        "padrao" | "grafite" => GRAFITE,
        "nord" => NORD,
        "dracula" => DRACULA,
        "tokyo-night" => TOKYO_NIGHT,
        "catppuccin" => CATPPUCCIN,
        "gruvbox" => GRUVBOX,
        "one-dark" => ONE_DARK,
        "meia-noite" => MEIA_NOITE,
        "cafe" => CAFE,
        "claro" => CLARO,
        _ => return false,
    };

    if let Ok(mut t) = TEMA_ATUAL.write() {
        *t = novas_cores;
    }
    if let Ok(mut i) = ID_TEMA_ATUAL.write() {
        *i = id.to_string();
    }
    true
}

/// Recarrega as cores do Wallust se o tema ativo for Wallust.
pub fn recarrega_wallust() -> bool {
    if tema_atual_id() == "wallust"
        && let Some(cores) = extrai_cores_wallust()
    {
        if let Ok(mut t) = TEMA_ATUAL.write() {
            *t = cores;
        }
        return true;
    }
    false
}

// --- Informações de Temas para UI ---

#[derive(Debug, Clone, PartialEq)]
pub struct InfoTema {
    pub id: String,
    pub nome: String,
    pub chips: [Color; 3], // fundo, lateral/elevado, destaque
    pub eh_wallust: bool,
}

pub fn lista_temas() -> Vec<InfoTema> {
    let mut lista = Vec::new();

    if let Some(w) = extrai_cores_wallust() {
        lista.push(InfoTema {
            id: "wallust".to_string(),
            nome: "Wallust (Sistema)".to_string(),
            chips: [w.fundo, w.elevado, w.destaque],
            eh_wallust: true,
        });
    }

    let fixos = [
        ("padrao", "Grafite neutro", GRAFITE),
        ("nord", "Nord", NORD),
        ("dracula", "Drácula", DRACULA),
        ("tokyo-night", "Tokyo Night", TOKYO_NIGHT),
        ("catppuccin", "Catppuccin Mocha", CATPPUCCIN),
        ("gruvbox", "Gruvbox Dark", GRUVBOX),
        ("one-dark", "One Dark", ONE_DARK),
        ("meia-noite", "Meia-noite", MEIA_NOITE),
        ("cafe", "Café", CAFE),
        ("claro", "Claro", CLARO),
    ];

    for (id, nome, c) in fixos {
        lista.push(InfoTema {
            id: id.to_string(),
            nome: nome.to_string(),
            chips: [c.fundo, c.elevado, c.destaque],
            eh_wallust: false,
        });
    }

    lista
}

// --- Persistência em disco (~/.config/lincast-chat/tema.json) ---

use crate::ui::layout::Densidade;

fn padrao_tema() -> String {
    "padrao".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigAparencia {
    #[serde(default = "padrao_tema")]
    pub tema: String,
    #[serde(default)]
    pub densidade: Densidade,
}

impl Default for ConfigAparencia {
    fn default() -> Self {
        Self {
            tema: "padrao".to_string(),
            densidade: Densidade::Confortavel,
        }
    }
}

fn arquivo_tema() -> Option<PathBuf> {
    let dir = directories::ProjectDirs::from("", "", "lincast-chat")?;
    let arq = dir.config_dir().join("tema.json");
    if let Some(parent) = arq.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    Some(arq)
}

pub fn salva_aparencia(tema_id: &str, densidade: Densidade) {
    let Some(caminho) = arquivo_tema() else {
        return;
    };
    let cfg = ConfigAparencia {
        tema: tema_id.to_string(),
        densidade,
    };
    if let Ok(json) = serde_json::to_string_pretty(&cfg) {
        let _ = std::fs::write(caminho, json);
    }
}

pub fn salva_tema(id: &str) {
    let mut cfg = carrega_aparencia_salva();
    cfg.tema = id.to_string();
    if let Some(caminho) = arquivo_tema()
        && let Ok(json) = serde_json::to_string_pretty(&cfg)
    {
        let _ = std::fs::write(caminho, json);
    }
}

pub fn carrega_aparencia_salva() -> ConfigAparencia {
    if let Some(caminho) = arquivo_tema()
        && let Ok(conteudo) = std::fs::read_to_string(caminho)
        && let Ok(cfg) = serde_json::from_str::<ConfigAparencia>(&conteudo)
    {
        cfg
    } else {
        ConfigAparencia::default()
    }
}

pub fn carrega_tema_salvo() {
    let cfg = carrega_aparencia_salva();
    if !define_tema(&cfg.tema) {
        define_tema("padrao");
    }
}

pub fn tema_iced() -> Theme {
    let c = cores();
    Theme::custom(
        "Lincast Chat",
        iced::theme::Palette {
            background: c.fundo,
            text: c.texto,
            primary: c.destaque,
            success: c.sucesso,
            warning: c.destaque,
            danger: c.perigo,
        },
    )
}

/// Fonte do app: Figtree, embutida (licença OFL em `assets/fonts`).
pub const FONTE: iced::Font = iced::Font::with_name("Figtree");
/// Emoji sempre com a Noto Color Emoji: no fallback automático a DejaVu Sans
/// e a Font Awesome vêm antes e desenham boa parte dos emojis só em contorno.
pub const FONTE_EMOJI: iced::Font = iced::Font::with_name("Noto Color Emoji");

/// Caractere que pertence a um emoji (inclui junção, seletor de variação,
/// tom de pele e keycap, para não quebrar a sequência no meio).
pub fn eh_emoji(c: char) -> bool {
    matches!(c as u32,
        0x1F000..=0x1FAFF | 0x2600..=0x27BF | 0x2300..=0x23FF | 0x2B00..=0x2BFF
        | 0x200D | 0xFE0F | 0x20E3 | 0xE0020..=0xE007F | 0x3030 | 0x303D | 0x3297 | 0x3299
        | 0x00A9 | 0x00AE | 0x203C | 0x2049 | 0x2122 | 0x2139 | 0x2194..=0x21AA)
}

/// Divide o texto em trechos `(é_emoji, trecho)`, na ordem.
pub fn trechos_de_emoji(s: &str) -> Vec<(bool, &str)> {
    let mut out: Vec<(bool, &str)> = Vec::new();
    let mut ini = 0;
    let mut atual: Option<bool> = None;
    for (i, c) in s.char_indices() {
        let e = eh_emoji(c);
        if atual.is_some_and(|a| a != e) {
            out.push((atual.unwrap_or(false), &s[ini..i]));
            ini = i;
        }
        atual = Some(e);
    }
    if ini < s.len() {
        out.push((atual.unwrap_or(false), &s[ini..]));
    }
    out
}

/// Raios do protótipo: 6px em botões e campos, 8px em cartões e janelas.
pub const RAIO: f32 = 8.0;
pub const RAIO_GRANDE: f32 = 12.0;

/// Tamanhos de texto do protótipo: 15px no corpo do chat, 12px nos rótulos
/// em maiúsculas.
pub const TEXTO_CHAT: f32 = 15.0;
pub const TEXTO_ROTULO: f32 = 12.0;

fn borda(cor: Color, raio: f32) -> Border {
    Border {
        color: cor,
        width: 1.0,
        radius: Radius::from(raio),
    }
}

fn sem_borda(raio: f32) -> Border {
    Border {
        color: Color::TRANSPARENT,
        width: 0.0,
        radius: Radius::from(raio),
    }
}

pub fn mistura(a: Color, b: Color, t: f32) -> Color {
    Color {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: 1.0,
    }
}

/// Cores de fundo dos avatares sem foto. Fixas e iguais em qualquer tema: a
/// mesma pessoa tem a mesma cor para todo mundo, e o branco por cima lê bem em
/// todas.
const CORES_DE_AVATAR: [Color; 8] = [
    rgb(0x5865c4),
    rgb(0x2f8f83),
    rgb(0xb5566b),
    rgb(0xa87a2a),
    rgb(0x7a5cc0),
    rgb(0x3b86b8),
    rgb(0x4f9a4f),
    rgb(0xb8643a),
];

/// Cor fixa do avatar para esta identidade (uid, ou nome quando não há uid).
pub fn cor_de_avatar(chave: &str) -> Color {
    let i = (crate::avatar::hash_texto(chave) % CORES_DE_AVATAR.len() as u64) as usize;
    CORES_DE_AVATAR[i]
}

// --- contêineres ------------------------------------------------------------

/// Cartão flutuante (popup, menu, barra): fundo lateral, borda fina e canto
/// arredondado. Para colunas de borda a borda use `painel_lateral_reto`.
pub fn painel_lateral(_: &Theme) -> container::Style {
    let c = cores();
    container::Style {
        background: Some(Background::Color(c.lateral)),
        text_color: Some(c.texto),
        border: borda(c.borda, RAIO_GRANDE),
        ..Default::default()
    }
}

/// Coluna encostada nas bordas da janela (lista de conversas, membros).
pub fn painel_lateral_reto(_: &Theme) -> container::Style {
    let c = cores();
    container::Style {
        background: Some(Background::Color(c.lateral)),
        text_color: Some(c.texto),
        ..Default::default()
    }
}

pub fn painel_principal(_: &Theme) -> container::Style {
    let c = cores();
    container::Style {
        background: Some(Background::Color(c.fundo)),
        text_color: Some(c.texto),
        ..Default::default()
    }
}

/// Balão de dica (tooltip): escuro, borda fina e canto arredondado.
pub fn dica(_: &Theme) -> container::Style {
    let c = cores();
    container::Style {
        background: Some(Background::Color(mistura(c.fundo, Color::BLACK, 0.35))),
        text_color: Some(c.texto),
        border: borda(c.borda, RAIO),
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.4),
            offset: iced::Vector::new(0.0, 2.0),
            blur_radius: 8.0,
        },
        ..Default::default()
    }
}

/// Linha fina de 1px usada como divisória.
pub fn divisoria(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(cores().borda)),
        ..Default::default()
    }
}

/// Caixa de aviso de erro: texto na cor de perigo, sem fundo berrante.
pub fn aviso_erro(_: &Theme) -> container::Style {
    let c = cores();
    container::Style {
        background: Some(Background::Color(mistura(c.fundo, c.perigo, 0.10))),
        text_color: Some(c.perigo),
        border: borda(mistura(c.fundo, c.perigo, 0.35), RAIO_GRANDE),
        ..Default::default()
    }
}

/// Caixa de aviso / alerta: fundo sutil e borda em tom âmbar/aviso.
pub fn aviso_alerta(_: &Theme) -> container::Style {
    let c = cores();
    let cor_alerta = Color::from_rgb8(0xd9, 0x9b, 0x26);
    container::Style {
        background: Some(Background::Color(mistura(c.fundo, cor_alerta, 0.12))),
        text_color: Some(c.texto),
        border: borda(mistura(c.fundo, cor_alerta, 0.40), RAIO_GRANDE),
        ..Default::default()
    }
}

/// Fundo e borda de mensagem que cita você (estilo Discord: âmbar/dourado sutil).
pub fn destaque_mencao(_: &Theme) -> container::Style {
    let c = cores();
    let cor_mencao = Color::from_rgb8(0xd9, 0x9b, 0x26);
    container::Style {
        background: Some(Background::Color(mistura(c.fundo, cor_mencao, 0.12))),
        border: Border {
            color: cor_mencao,
            width: 1.5,
            radius: Radius {
                top_left: RAIO,
                top_right: 0.0,
                bottom_right: 0.0,
                bottom_left: RAIO,
            },
        },
        ..Default::default()
    }
}

/// Destaque de mensagem de sussurro: fundo suave e borda lateral no tom de destaque.
pub fn destaque_sussurro(_: &Theme) -> container::Style {
    let c = cores();
    let cor_borda = mistura(c.destaque, Color::WHITE, 0.2);
    container::Style {
        background: Some(Background::Color(mistura(c.fundo, c.destaque, 0.08))),
        border: Border {
            color: cor_borda,
            width: 1.5,
            radius: Radius {
                top_left: RAIO,
                top_right: 0.0,
                bottom_right: 0.0,
                bottom_left: RAIO,
            },
        },
        ..Default::default()
    }
}

/// Moldura do vídeo de quem está apresentando.
pub fn moldura_video(_: &Theme) -> container::Style {
    let c = cores();
    container::Style {
        background: Some(Background::Color(Color::BLACK)),
        border: borda(c.borda, RAIO_GRANDE),
        ..Default::default()
    }
}

/// Linha divisória de mensagens não lidas (estilo danger do Discord/v1).
pub fn linha_divisor_nao_lidas(_: &Theme) -> container::Style {
    let c = cores();
    container::Style {
        background: Some(Background::Color(c.perigo)),
        ..Default::default()
    }
}

// --- botões -----------------------------------------------------------------

/// A ação principal da tela. É o único lugar que pinta com o destaque.
pub fn botao_primario(_: &Theme, status: button::Status) -> button::Style {
    let c = cores();
    let fundo = match status {
        button::Status::Hovered => mistura(c.destaque, Color::WHITE, 0.08),
        button::Status::Pressed => mistura(c.destaque, Color::BLACK, 0.10),
        button::Status::Disabled => mistura(c.elevado, c.destaque, 0.25),
        button::Status::Active => c.destaque,
    };
    button::Style {
        background: Some(Background::Color(fundo)),
        text_color: if status == button::Status::Disabled {
            c.texto_fraco
        } else {
            c.sobre_destaque
        },
        border: sem_borda(RAIO),
        shadow: Shadow::default(),
        snap: true,
    }
}

/// Ação secundária: contorno neutro, sem cor.
pub fn botao_secundario(_: &Theme, status: button::Status) -> button::Style {
    let c = cores();
    let fundo = match status {
        button::Status::Hovered => Some(Background::Color(mistura(c.elevado, c.texto, 0.08))),
        button::Status::Pressed => Some(Background::Color(mistura(c.elevado, Color::BLACK, 0.15))),
        _ => None,
    };
    button::Style {
        background: fundo,
        text_color: if status == button::Status::Disabled {
            c.texto_fraco
        } else {
            c.texto
        },
        border: borda(c.borda, RAIO),
        shadow: Shadow::default(),
        snap: true,
    }
}

/// Item de lista (sala, pessoa): sem fundo até passar o mouse.
pub fn item_de_lista(_: &Theme, status: button::Status) -> button::Style {
    let c = cores();
    let fundo = match status {
        button::Status::Hovered => Some(Background::Color(mistura(c.elevado, c.texto, 0.08))),
        button::Status::Pressed => Some(Background::Color(mistura(c.elevado, c.borda, 0.5))),
        _ => None,
    };
    button::Style {
        background: fundo,
        text_color: c.texto,
        border: sem_borda(RAIO),
        shadow: Shadow::default(),
        snap: true,
    }
}

/// Botão de ícone da barra de controles. `ligado` = estado que merece atenção
/// (mudo, ensurdecido, apresentando): é aí que a cor aparece, e só aí.
pub fn botao_icone(
    ligado: bool,
    perigoso: bool,
) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let c = cores();
        let base = if perigoso {
            Some(mistura(c.lateral, c.perigo, 0.18))
        } else if ligado {
            Some(mistura(c.lateral, c.destaque, 0.18))
        } else {
            None
        };
        let fundo = match status {
            button::Status::Hovered => Some(
                base.map(|b| mistura(b, Color::WHITE, 0.10))
                    .unwrap_or_else(|| mistura(c.elevado, c.texto, 0.10)),
            ),
            // Afunda ao clicar: mais escuro que o repouso.
            button::Status::Pressed => Some(mistura(base.unwrap_or(c.elevado), Color::BLACK, 0.18)),
            _ => base,
        };
        button::Style {
            background: fundo.map(Background::Color),
            text_color: if perigoso {
                c.perigo
            } else if ligado {
                c.destaque
            } else {
                c.texto_fraco
            },
            border: sem_borda(RAIO),
            shadow: Shadow::default(),
            snap: true,
        }
    }
}

// --- campos -----------------------------------------------------------------

pub fn campo(_: &Theme, status: text_input::Status) -> text_input::Style {
    let c = cores();
    let contorno = match status {
        text_input::Status::Focused { .. } => mistura(c.borda, c.destaque, 0.7),
        text_input::Status::Hovered => mistura(c.borda, c.texto_fraco, 0.3),
        _ => c.borda,
    };
    text_input::Style {
        background: Background::Color(c.elevado),
        border: borda(contorno, RAIO),
        icon: c.texto_fraco,
        placeholder: c.texto_fraco,
        value: c.texto,
        selection: mistura(c.elevado, c.destaque, 0.4),
    }
}

/// Caixa de seleção (pick_list): fechada parece um campo; aberta, o menu
/// desce com cantos arredondados e a opção marcada em destaque.
pub fn lista_suspensa(
    _: &Theme,
    status: iced::widget::pick_list::Status,
) -> iced::widget::pick_list::Style {
    use iced::widget::pick_list::Status;
    let c = cores();
    let contorno = match status {
        Status::Opened { .. } => c.destaque,
        Status::Hovered => mistura(c.borda, c.texto_fraco, 0.4),
        Status::Active => c.borda,
    };
    iced::widget::pick_list::Style {
        text_color: c.texto,
        placeholder_color: c.texto_fraco,
        handle_color: c.texto_fraco,
        background: Background::Color(c.elevado),
        border: borda(contorno, RAIO),
    }
}

pub fn menu_suspenso(_: &Theme) -> iced::overlay::menu::Style {
    let c = cores();
    iced::overlay::menu::Style {
        background: Background::Color(c.lateral),
        border: Border {
            color: c.borda,
            width: 1.0,
            radius: RAIO.into(),
        },
        text_color: c.texto,
        selected_text_color: c.sobre_destaque,
        selected_background: Background::Color(c.destaque),
        shadow: iced::Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.4),
            offset: iced::Vector::new(0.0, 4.0),
            blur_radius: 14.0,
        },
    }
}

/// Campo de busca da coluna da esquerda: fundo no degrau mais fundo, sem
/// contorno até ganhar foco (`.find` do protótipo).
pub fn campo_busca(_: &Theme, status: text_input::Status) -> text_input::Style {
    let c = cores();
    let contorno = match status {
        text_input::Status::Focused { .. } => c.destaque,
        _ => Color::TRANSPARENT,
    };
    text_input::Style {
        background: Background::Color(c.palco),
        border: borda(contorno, RAIO),
        icon: c.texto_fraco,
        placeholder: c.texto_fraco,
        value: c.texto,
        selection: mistura(c.palco, c.destaque, 0.4),
    }
}

// --- rolagem ----------------------------------------------------------------

/// Barra de rolagem discreta: só o polegar, fino, sem trilho visível.
pub fn rolagem(theme: &Theme, status: scrollable::Status) -> scrollable::Style {
    let c = cores();
    let mut s = scrollable::default(theme, status);
    for trilho in [&mut s.vertical_rail, &mut s.horizontal_rail] {
        trilho.background = None;
        trilho.border = sem_borda(3.0);
        trilho.scroller.border = sem_borda(3.0);
        trilho.scroller.background = Background::Color(c.borda);
    }
    s
}

// --- ícones -----------------------------------------------------------------

pub fn icone_cor(cor: Color) -> impl Fn(&Theme, svg::Status) -> svg::Style {
    move |_, _| svg::Style { color: Some(cor) }
}

// --- sliders ----------------------------------------------------------------

pub fn estilo_slider(_theme: &Theme, status: slider::Status) -> slider::Style {
    let c = cores();
    let destaque = match status {
        slider::Status::Active => c.destaque,
        slider::Status::Hovered | slider::Status::Dragged => {
            mistura(c.destaque, Color::WHITE, 0.15)
        }
    };
    slider::Style {
        rail: slider::Rail {
            backgrounds: (Background::Color(destaque), Background::Color(c.elevado)),
            width: 4.0,
            border: sem_borda(2.0),
        },
        handle: slider::Handle {
            shape: slider::HandleShape::Circle { radius: 6.0 },
            background: Background::Color(destaque),
            border_width: 1.0,
            border_color: c.borda,
        },
    }
}

/// Ação de perigo/interrupção (ex: parar teste de microfone, sair de chamada).
pub fn botao_perigo(_: &Theme, status: button::Status) -> button::Style {
    let c = cores();
    let fundo = match status {
        button::Status::Hovered => mistura(c.perigo, Color::WHITE, 0.08),
        button::Status::Pressed => mistura(c.perigo, Color::BLACK, 0.10),
        button::Status::Disabled => mistura(c.elevado, c.perigo, 0.25),
        button::Status::Active => c.perigo,
    };
    button::Style {
        background: Some(Background::Color(fundo)),
        text_color: if status == button::Status::Disabled {
            c.texto_fraco
        } else {
            Color::WHITE
        },
        border: sem_borda(RAIO),
        shadow: Shadow::default(),
        snap: true,
    }
}

/// Botão flutuante ou barra de aviso de mensagens não lidas.
pub fn botao_nao_lidas(_: &Theme, status: button::Status) -> button::Style {
    let c = cores();
    let fundo = match status {
        button::Status::Hovered => mistura(c.perigo, Color::WHITE, 0.10),
        button::Status::Pressed => mistura(c.perigo, Color::BLACK, 0.10),
        button::Status::Disabled => mistura(c.fundo, c.perigo, 0.3),
        button::Status::Active => c.perigo,
    };
    button::Style {
        background: Some(Background::Color(fundo)),
        text_color: Color::WHITE,
        border: borda(Color::TRANSPARENT, 12.0),
        shadow: Shadow::default(),
        snap: true,
    }
}

/// Botão de opção de enquete com destaque de borda e fundo quando já votada.
pub fn opcao_enquete(votada: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let c = cores();
        let base = if votada {
            mistura(c.destaque, c.lateral, 0.25)
        } else {
            c.elevado
        };
        let fundo = match status {
            button::Status::Hovered => mistura(base, Color::WHITE, 0.08),
            button::Status::Pressed => mistura(base, Color::BLACK, 0.10),
            _ => base,
        };
        let borda_cor = if votada { c.destaque } else { c.borda };
        button::Style {
            background: Some(Background::Color(fundo)),
            text_color: c.texto,
            border: borda(borda_cor, RAIO),
            shadow: Shadow::default(),
            snap: true,
        }
    }
}

pub fn medidor_fundo(_: &Theme) -> container::Style {
    let c = cores();
    container::Style {
        background: Some(Background::Color(c.fundo)),
        border: borda(c.borda, 4.0),
        ..Default::default()
    }
}

pub fn medidor_preenchimento(_: &Theme) -> container::Style {
    let c = cores();
    container::Style {
        background: Some(Background::Color(c.destaque)),
        border: sem_borda(3.0),
        ..Default::default()
    }
}

/// Anel indicador de quem está falando para avatares e molduras de vídeo.
pub fn anel_fala(falando: bool) -> impl Fn(&Theme) -> container::Style {
    move |_| {
        let c = cores();
        if falando {
            container::Style {
                border: borda(c.sucesso, 2.0),
                ..Default::default()
            }
        } else {
            container::Style {
                border: sem_borda(2.0),
                ..Default::default()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn testa_parse_hex_cores() {
        assert_eq!(parse_hex("#ffffff"), Some(rgb(0xffffff)));
        assert_eq!(parse_hex("000000"), Some(rgb(0x000000)));
        assert_eq!(parse_hex("#7f9cf5"), Some(rgb(0x7f9cf5)));
        assert_eq!(parse_hex("invalido"), None);
        assert_eq!(parse_hex("#123"), None);
    }

    #[test]
    fn testa_luminancia_e_contraste() {
        let preto = rgb(0x000000);
        let branco = rgb(0xffffff);
        let azul = rgb(0x0000ff);
        let amarelo = rgb(0xffff00);

        assert!(luminancia(preto) < 0.1);
        assert!(luminancia(branco) > 0.9);

        // Cor escura deve pedir texto claro
        assert_eq!(sobre_contraste(preto), rgb(0xf8f8f2));
        assert_eq!(sobre_contraste(azul), rgb(0xf8f8f2));

        // Cor clara deve pedir texto escuro
        assert_eq!(sobre_contraste(branco), rgb(0x0f1115));
        assert_eq!(sobre_contraste(amarelo), rgb(0x0f1115));
    }

    #[test]
    fn testa_selecao_e_troca_de_temas() {
        assert!(define_tema("nord"));
        assert_eq!(tema_atual_id(), "nord");
        assert_eq!(cores().fundo, NORD.fundo);

        assert!(define_tema("dracula"));
        assert_eq!(tema_atual_id(), "dracula");
        assert_eq!(cores().fundo, DRACULA.fundo);

        assert!(define_tema("padrao"));
        assert_eq!(tema_atual_id(), "padrao");
        assert_eq!(cores().fundo, GRAFITE.fundo);

        // Tema inexistente deve falhar
        assert!(!define_tema("tema_inexistente_xyz"));
    }

    #[test]
    fn testa_lista_temas_contem_paletas_principais() {
        let lista = lista_temas();
        assert!(!lista.is_empty());
        let ids: Vec<&str> = lista.iter().map(|t| t.id.as_str()).collect();
        assert!(ids.contains(&"padrao"));
        assert!(ids.contains(&"nord"));
        assert!(ids.contains(&"dracula"));
        assert!(ids.contains(&"catppuccin"));
        assert!(ids.contains(&"tokyo-night"));
    }

    #[test]
    fn testa_constroi_paleta_base() {
        let bg = rgb(0x101010);
        let fg = rgb(0xeeeeee);
        let destaque = rgb(0x4a6cf7);
        let paleta = constroi_paleta_base(bg, fg, destaque, None, None);

        assert_eq!(paleta.fundo, bg);
        assert_eq!(paleta.texto, fg);
        assert_eq!(paleta.destaque, destaque);
        assert_eq!(paleta.sobre_destaque, rgb(0xf8f8f2));
    }
}
