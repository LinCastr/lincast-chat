//! GIF animado. O iced só mostra o primeiro quadro de um GIF, então os quadros
//! são decodificados uma vez (em segundo plano, reduzidos) e o quadro certo é
//! escolhido a cada desenho pelo relógio. Respeita as opções de desempenho:
//! com "só com o mouse em cima" ou "reduzir animações", só anda o GIF sob o
//! cursor; os demais ficam no primeiro quadro.

use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, RwLock};
use std::time::Instant;

use iced::widget::image::Handle;
use image::AnimationDecoder;

/// Lado máximo de cada quadro guardado.
const LADO_MAXIMO: u32 = 360;
/// Quadros no máximo por GIF (os que passarem são pulados, com o tempo somado).
const QUADROS_MAXIMO: usize = 150;
/// Orçamento de pixels por GIF (RGBA = 4 bytes cada): ~48 MB.
const PIXELS_MAXIMO: u64 = 12_000_000;

pub struct Animacao {
    quadros: Vec<Handle>,
    /// Fim de cada quadro, em ms desde o início do ciclo.
    fins: Vec<u64>,
}

impl Animacao {
    fn total(&self) -> u64 {
        self.fins.last().copied().unwrap_or(1).max(1)
    }

    fn quadro_em(&self, ms: u64) -> &Handle {
        let t = ms % self.total();
        let i = self.fins.partition_point(|&f| f <= t);
        &self.quadros[i.min(self.quadros.len() - 1)]
    }
}

static ANIMS: LazyLock<RwLock<HashMap<String, Arc<Animacao>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));
static EM_CURSO: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(|| Mutex::new(HashSet::new()));
static HOVER: Mutex<Option<String>> = Mutex::new(None);
static INICIO: LazyLock<Instant> = LazyLock::new(Instant::now);
/// Último instante (ms desde `INICIO`) em que algum GIF em movimento foi desenhado.
static ULTIMO_USO: AtomicU64 = AtomicU64::new(0);

fn agora_ms() -> u64 {
    INICIO.elapsed().as_millis() as u64
}

pub fn eh_gif(bytes: &[u8]) -> bool {
    bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")
}

/// Decodifica os quadros num thread à parte. Sem efeito para GIF de um quadro só.
pub fn prepara(url: &str, bytes: Vec<u8>) {
    if !eh_gif(&bytes) {
        return;
    }
    {
        let Ok(mut c) = EM_CURSO.lock() else { return };
        if !c.insert(url.to_string()) {
            return;
        }
    }
    let url = url.to_string();
    std::thread::spawn(move || {
        if let Some(a) = decodifica(&bytes)
            && let Ok(mut m) = ANIMS.write()
        {
            m.insert(url.clone(), Arc::new(a));
        }
        if let Ok(mut c) = EM_CURSO.lock() {
            c.remove(&url);
        }
    });
}

fn decodifica(bytes: &[u8]) -> Option<Animacao> {
    let dec = image::codecs::gif::GifDecoder::new(Cursor::new(bytes)).ok()?;
    let mut quadros = Vec::new();
    let mut fins = Vec::new();
    let mut acum: u64 = 0;
    let mut pixels: u64 = 0;
    let mut pendente_ms: u64 = 0;
    // Pulo: se o GIF tem mais quadros que o limite, guarda 1 a cada N.
    let passo = {
        let n = image::codecs::gif::GifDecoder::new(Cursor::new(bytes))
            .ok()?
            .into_frames()
            .count();
        n.div_ceil(QUADROS_MAXIMO).max(1)
    };
    for (indice, f) in dec.into_frames().enumerate() {
        let f = f.ok()?;
        let (num, den) = f.delay().numer_denom_ms();
        let mut ms = num.checked_div(den).map_or(100, u64::from);
        if ms < 20 {
            ms = 100; // convenção dos navegadores para atraso 0/1
        }
        pendente_ms += ms;
        let guarda = indice.is_multiple_of(passo);
        if !guarda {
            continue;
        }
        let img = f.into_buffer();
        let (w, h) = img.dimensions();
        let maior = w.max(h);
        let img = if maior > LADO_MAXIMO {
            let k = f64::from(LADO_MAXIMO) / f64::from(maior);
            image::imageops::resize(
                &img,
                ((f64::from(w) * k) as u32).max(1),
                ((f64::from(h) * k) as u32).max(1),
                image::imageops::FilterType::Triangle,
            )
        } else {
            img
        };
        let (w, h) = img.dimensions();
        pixels += u64::from(w) * u64::from(h);
        if pixels > PIXELS_MAXIMO && !quadros.is_empty() {
            break;
        }
        acum += pendente_ms;
        pendente_ms = 0;
        quadros.push(Handle::from_rgba(w, h, img.into_raw()));
        fins.push(acum);
    }
    (quadros.len() > 1).then_some(Animacao { quadros, fins })
}

/// A URL sob o cursor agora (para o modo "só com o mouse em cima").
pub fn define_hover(url: Option<String>) {
    if let Ok(mut h) = HOVER.lock() {
        *h = url;
    }
}

/// Esta URL é um GIF já decodificado?
pub fn animada(url: &str) -> bool {
    ANIMS.read().is_ok_and(|m| m.contains_key(url))
}

/// O quadro a desenhar agora para `url`, se for GIF animado e puder andar.
pub fn quadro_atual(url: &str) -> Option<Handle> {
    let a = ANIMS.read().ok()?.get(url).cloned()?;
    let restrito = crate::desempenho::gif_so_hover() || crate::desempenho::reduz_animacoes();
    if restrito {
        let sob_mouse = HOVER.lock().ok().is_some_and(|h| h.as_deref() == Some(url));
        if !sob_mouse {
            return None; // quem chama usa o quadro parado (o GIF original)
        }
    }
    ULTIMO_USO.store(agora_ms(), Ordering::Relaxed);
    Some(a.quadro_em(agora_ms()).clone())
}

/// Algum GIF foi desenhado em movimento há pouco? (liga o relógio de redesenho)
pub fn animando() -> bool {
    agora_ms().saturating_sub(ULTIMO_USO.load(Ordering::Relaxed)) < 400
        && ULTIMO_USO.load(Ordering::Relaxed) != 0
}

#[cfg(test)]
mod testes {
    use super::*;

    fn gif_de_dois_quadros() -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut enc = image::codecs::gif::GifEncoder::new(&mut out);
            for cor in [[255u8, 0, 0, 255], [0, 0, 255, 255]] {
                let img = image::RgbaImage::from_pixel(4, 4, image::Rgba(cor));
                let f =
                    image::Frame::from_parts(img, 0, 0, image::Delay::from_numer_denom_ms(100, 1));
                enc.encode_frame(f).unwrap();
            }
        }
        out
    }

    #[test]
    fn decodifica_e_escolhe_quadro_pelo_tempo() {
        let b = gif_de_dois_quadros();
        assert!(eh_gif(&b));
        let a = decodifica(&b).expect("dois quadros");
        assert_eq!(a.quadros.len(), 2);
        assert_eq!(a.total(), 200);
        assert!(std::ptr::eq(a.quadro_em(10), &a.quadros[0]));
        assert!(std::ptr::eq(a.quadro_em(150), &a.quadros[1]));
        assert!(std::ptr::eq(a.quadro_em(210), &a.quadros[0]));
    }

    #[test]
    fn gif_de_um_quadro_nao_vira_animacao() {
        let mut out = Vec::new();
        {
            let mut enc = image::codecs::gif::GifEncoder::new(&mut out);
            let img = image::RgbaImage::from_pixel(2, 2, image::Rgba([1, 2, 3, 255]));
            enc.encode_frame(image::Frame::new(img)).unwrap();
        }
        assert!(decodifica(&out).is_none());
    }
}
