//! Relógio de animação. O iced não anima hover, então guardamos aqui, por id,
//! quando o mouse entrou/saiu, e as telas leem o progresso ao desenhar.
//! O `tick` (16 ms) só roda enquanto `ativo()` — com tudo parado a interface
//! não redesenha sozinha (bateria, ver Z1 no RESUMO).

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

const FADE: Duration = Duration::from_millis(140);
const BALANCO: Duration = Duration::from_millis(560);
const GIRO: Duration = Duration::from_millis(700);
const ABERTURA: Duration = Duration::from_millis(180);

/// Emojis sorteados no botão de emoji a cada hover.
pub const EMOJIS_HOVER: &[&str] = &[
    "😀", "😂", "😎", "🥳", "😍", "🤩", "😜", "🤔", "😴", "🤠", "👻", "🔥", "✨", "🎉", "🍀", "🌈",
];

#[derive(Clone)]
struct Item {
    dentro: bool,
    desde: Instant,
    de: f32,
    emoji: usize,
}

#[derive(Default)]
struct Estado {
    itens: HashMap<String, Item>,
    /// Janelas: quando foram vistas pela primeira vez (e em que quadro por último).
    janelas: HashMap<String, (Instant, u64)>,
    quadro: u64,
    /// Animações de duração fixa disparadas por um acontecimento (`dispara`).
    eventos: HashMap<String, (Instant, Duration)>,
}

static ESTADO: Mutex<Option<Estado>> = Mutex::new(None);

fn com<R>(f: impl FnOnce(&mut Estado) -> R) -> R {
    let mut g = ESTADO.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(Estado::default))
}

fn suave(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

fn valor(i: &Item, agora: Instant) -> f32 {
    let alvo = if i.dentro { 1.0 } else { 0.0 };
    if crate::desempenho::reduz_animacoes() {
        return alvo;
    }
    let p = agora.saturating_duration_since(i.desde).as_secs_f32() / FADE.as_secs_f32();
    i.de + (alvo - i.de) * suave(p)
}

/// O mouse entrou (`dentro`) ou saiu do elemento `id`.
pub fn hover(id: &str, dentro: bool) {
    let agora = Instant::now();
    com(|e| {
        let atual = e.itens.get(id).map(|i| valor(i, agora)).unwrap_or(0.0);
        let emoji = if dentro {
            fastrand::usize(..EMOJIS_HOVER.len())
        } else {
            e.itens.get(id).map(|i| i.emoji).unwrap_or(0)
        };
        e.itens.insert(
            id.to_string(),
            Item {
                dentro,
                desde: agora,
                de: atual,
                emoji,
            },
        );
    });
}

/// Desde quando o mouse está em cima (`None` fora).
pub fn dentro_desde(id: &str) -> Option<Instant> {
    com(|e| e.itens.get(id).filter(|i| i.dentro).map(|i| i.desde))
}

/// 0.0 (fora) … 1.0 (mouse em cima), com transição suave.
pub fn t(id: &str) -> f32 {
    let agora = Instant::now();
    com(|e| e.itens.get(id).map(|i| valor(i, agora)).unwrap_or(0.0))
}

/// Mouse em cima agora (sem transição).
pub fn dentro(id: &str) -> bool {
    com(|e| e.itens.get(id).is_some_and(|i| i.dentro))
}

/// Ângulo (rad) de um balanço amortecido iniciado no último hover.
pub fn balanco(id: &str) -> f32 {
    if crate::desempenho::reduz_animacoes() {
        return 0.0;
    }
    let agora = Instant::now();
    com(|e| match e.itens.get(id) {
        Some(i) if i.dentro => {
            let s = agora.saturating_duration_since(i.desde);
            if s >= BALANCO {
                0.0
            } else {
                let x = s.as_secs_f32();
                0.38 * (x * std::f32::consts::TAU * 3.5).sin() * (-x * 5.0).exp()
            }
        }
        _ => 0.0,
    })
}

/// Ângulo (rad) de uma volta completa iniciada no último hover.
pub fn giro(id: &str) -> f32 {
    if crate::desempenho::reduz_animacoes() {
        return 0.0;
    }
    let agora = Instant::now();
    com(|e| match e.itens.get(id) {
        Some(i) if i.dentro => {
            let p = agora.saturating_duration_since(i.desde).as_secs_f32() / GIRO.as_secs_f32();
            if p >= 1.0 {
                0.0
            } else {
                let inv = 1.0 - p;
                std::f32::consts::TAU * (1.0 - inv * inv * inv)
            }
        }
        _ => 0.0,
    })
}

/// O emoji sorteado no último hover de `id`.
pub fn emoji(id: &str) -> &'static str {
    com(|e| EMOJIS_HOVER[e.itens.get(id).map(|i| i.emoji).unwrap_or(0) % EMOJIS_HOVER.len()])
}

/// Progresso (0..1) da entrada de uma janela. A primeira chamada marca o
/// início; a janela some do mapa quando deixa de ser pedida.
pub fn abertura(id: &str) -> f32 {
    if crate::desempenho::reduz_animacoes() {
        return 1.0;
    }
    let agora = Instant::now();
    com(|e| {
        let q = e.quadro;
        let (inicio, visto) = e.janelas.entry(id.to_string()).or_insert((agora, q));
        *visto = q;
        suave(agora.saturating_duration_since(*inicio).as_secs_f32() / ABERTURA.as_secs_f32())
    })
}

/// Começa (ou recomeça) a animação `id`, que dura `duracao`: peça do Wordle
/// virando, linha tremendo. Ler com `progresso`.
pub fn dispara(id: &str, duracao: Duration) {
    if crate::desempenho::reduz_animacoes() {
        return;
    }
    com(|e| {
        e.eventos.insert(id.to_string(), (Instant::now(), duracao));
    });
}

/// 0.0 … 1.0 enquanto a animação `id` roda; `None` antes e depois.
pub fn progresso(id: &str) -> Option<f32> {
    let agora = Instant::now();
    com(|e| {
        let (ini, dur) = e.eventos.get(id)?;
        let p = agora.saturating_duration_since(*ini).as_secs_f32() / dur.as_secs_f32();
        (p < 1.0).then_some(p)
    })
}

/// Chamado a cada `view`: esquece as janelas que não foram desenhadas no
/// quadro anterior (fecharam), para a próxima abertura animar de novo.
pub fn inicio_quadro() {
    com(|e| {
        e.quadro += 1;
        let q = e.quadro;
        e.janelas.retain(|_, (_, visto)| *visto + 1 >= q);
    });
}

/// Há algo animando? (decide se o relógio de 16 ms fica ligado)
pub fn ativo() -> bool {
    if crate::desempenho::reduz_animacoes() {
        return false;
    }
    let agora = Instant::now();
    com(|e| {
        let limite = BALANCO.max(GIRO);
        e.itens
            .values()
            .any(|i| agora.saturating_duration_since(i.desde) < limite)
            || e.janelas
                .values()
                .any(|(ini, _)| agora.saturating_duration_since(*ini) < ABERTURA)
            || e.eventos
                .values()
                .any(|(ini, dur)| agora.saturating_duration_since(*ini) < *dur)
    })
}

/// Joga fora o que já terminou e não tem o mouse em cima.
pub fn limpa() {
    let agora = Instant::now();
    com(|e| {
        e.itens.retain(|_, i| {
            i.dentro || agora.saturating_duration_since(i.desde) < Duration::from_secs(2)
        });
        e.eventos
            .retain(|_, (ini, dur)| agora.saturating_duration_since(*ini) < *dur);
    });
}

/// Com `LINCAST_MEDE_VIEW=1`, registra a cada 5 s quantas vezes o `view` rodou
/// e quanto tempo levou em média (para achar travadas de animação).
pub struct MedeView(Option<Instant>);

static MEDE: LazyLock<bool> = LazyLock::new(|| std::env::var_os("LINCAST_MEDE_VIEW").is_some());
static MEDIDAS: Mutex<(u32, Duration, Option<Instant>)> = Mutex::new((0, Duration::ZERO, None));

impl MedeView {
    pub fn agora() -> Self {
        Self(MEDE.then(Instant::now))
    }
}

impl Drop for MedeView {
    fn drop(&mut self) {
        let Some(t0) = self.0 else { return };
        let Ok(mut m) = MEDIDAS.lock() else { return };
        m.0 += 1;
        m.1 += t0.elapsed();
        let inicio = *m.2.get_or_insert(t0);
        if inicio.elapsed() >= Duration::from_secs(5) {
            log::info!(
                "[desempenho] view: {} vezes em 5 s, média {:.2} ms",
                m.0,
                m.1.as_secs_f64() * 1000.0 / f64::from(m.0.max(1))
            );
            *m = (0, Duration::ZERO, Some(Instant::now()));
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn hover_sobe_e_desce() {
        hover("teste-a", true);
        assert!(dentro("teste-a"));
        assert!(t("teste-a") < 0.5);
        std::thread::sleep(Duration::from_millis(200));
        assert!((t("teste-a") - 1.0).abs() < 1e-3);
        hover("teste-a", false);
        std::thread::sleep(Duration::from_millis(200));
        assert!(t("teste-a") < 1e-3);
    }

    #[test]
    fn balanco_amortece_e_acaba() {
        hover("teste-b", true);
        std::thread::sleep(Duration::from_millis(750));
        assert_eq!(balanco("teste-b"), 0.0);
        assert_eq!(giro("teste-b"), 0.0);
    }

    #[test]
    fn janela_reinicia_depois_de_fechar() {
        inicio_quadro();
        let _ = abertura("teste-j");
        std::thread::sleep(Duration::from_millis(220));
        assert!((abertura("teste-j") - 1.0).abs() < 1e-3);
        // Dois quadros sem pedir: a janela fechou.
        inicio_quadro();
        inicio_quadro();
        inicio_quadro();
        assert!(abertura("teste-j") < 0.2);
    }

    #[test]
    fn emoji_sempre_da_lista() {
        hover("teste-e", true);
        assert!(EMOJIS_HOVER.contains(&emoji("teste-e")));
    }
}
