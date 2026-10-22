//! Cosméticos de perfil: decoração de avatar, placa de identificação, efeito
//! de perfil, moldura, estilo do nome e tema do perfil. Tudo de graça.
//!
//! Três partes, todas globais (como o cache de avatares) para o desenho, que
//! só tem `&self`, poder consultar sem receber nada por parâmetro:
//! - o catálogo (`GET /api/cosmeticos`), carregado ao conectar;
//! - a escolha de cada pessoa por uid (`GET /api/cosmeticos/pessoas` e o aviso
//!   `cosmeticos`);
//! - as imagens: o arquivo é um WebP animado; a miniatura parada fica sempre
//!   na memória, os quadros da animação só enquanto alguém está olhando
//!   (orçamento de memória, sai o que foi visto há mais tempo).
//!
//! Pedido de imagem é registrado durante o desenho (`quadro`/`miniatura`) e
//! buscado pelo app no próximo `update` (`pedidos`).

use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, RwLock};
use std::time::Instant;

use iced::widget::image::Handle;
use image::AnimationDecoder;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tipo {
    Decoracao,
    Placa,
    Efeito,
    Moldura,
}

impl Tipo {
    pub const TODOS: [Tipo; 4] = [Tipo::Decoracao, Tipo::Placa, Tipo::Efeito, Tipo::Moldura];

    pub fn rotulo(self) -> &'static str {
        match self {
            Tipo::Decoracao => "Decorações de avatar",
            Tipo::Placa => "Placas de identificação",
            Tipo::Efeito => "Efeitos de perfil",
            Tipo::Moldura => "Molduras de perfil",
        }
    }

    pub fn singular(self) -> &'static str {
        match self {
            Tipo::Decoracao => "Decoração de avatar",
            Tipo::Placa => "Placa de identificação",
            Tipo::Efeito => "Efeito de perfil",
            Tipo::Moldura => "Moldura de perfil",
        }
    }

    /// Lado máximo dos quadros guardados. O efeito é o maior (450×880 no
    /// original) e é o que mais pesa: 300 de largura já cobre o cartão.
    fn lado_maximo(self) -> u32 {
        match self {
            Tipo::Decoracao => 200,
            Tipo::Placa => 448,
            Tipo::Efeito => 640,
            Tipo::Moldura => 640,
        }
    }
}

/// Grupo do filtro "Temas" (o servidor decide pela coleção).
pub const GRUPOS: [(&str, &str); 4] = [
    ("anime", "Anime"),
    ("jogos", "Jogos"),
    ("filmes", "Filmes e TV"),
    ("originais", "Originais"),
];

#[derive(Debug, Clone, Deserialize)]
pub struct Item {
    pub id: String,
    pub tipo: Tipo,
    pub nome: String,
    pub categoria: String,
    pub grupo: String,
    /// WebP (animado na maioria).
    pub arquivo: String,
    /// Miniatura parada.
    #[serde(default)]
    pub previa: Option<String>,
    /// Moldura: "topo" (faixa no alto do cartão) ou "inteira" (em volta).
    #[serde(default)]
    pub ancora: Option<String>,
    /// Descrição longa (o texto original do item), mostrada no detalhe.
    #[serde(default)]
    pub descricao: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ref {
    pub id: String,
    pub arquivo: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EstiloNome {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fonte: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub efeito: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cor2: Option<String>,
}

impl EstiloNome {
    pub fn vazio(&self) -> bool {
        self.fonte.is_none() && self.efeito.is_none() && self.cor.is_none() && self.cor2.is_none()
    }
}

/// Tema do perfil: degradê de duas cores no fundo do cartão.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemaPerfil {
    pub cor1: String,
    pub cor2: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Escolha {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decoracao: Option<Ref>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placa: Option<Ref>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub efeito: Option<Ref>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moldura: Option<Ref>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nome: Option<EstiloNome>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tema: Option<TemaPerfil>,
}

impl Escolha {
    pub fn slot(&self, t: Tipo) -> Option<&Ref> {
        match t {
            Tipo::Decoracao => self.decoracao.as_ref(),
            Tipo::Placa => self.placa.as_ref(),
            Tipo::Efeito => self.efeito.as_ref(),
            Tipo::Moldura => self.moldura.as_ref(),
        }
    }

    pub fn slot_mut(&mut self, t: Tipo) -> &mut Option<Ref> {
        match t {
            Tipo::Decoracao => &mut self.decoracao,
            Tipo::Placa => &mut self.placa,
            Tipo::Efeito => &mut self.efeito,
            Tipo::Moldura => &mut self.moldura,
        }
    }

    /// O que vai no `POST /api/conta/cosmeticos` (ids, não arquivos).
    pub fn para_envio(&self) -> serde_json::Value {
        let id = |r: &Option<Ref>| r.as_ref().map(|r| r.id.clone());
        serde_json::json!({
            "decoracao": id(&self.decoracao),
            "placa": id(&self.placa),
            "efeito": id(&self.efeito),
            "moldura": id(&self.moldura),
            "nome": self.nome,
            "tema": self.tema,
        })
    }
}

// --- Catálogo e escolhas --------------------------------------------------

static CATALOGO: LazyLock<RwLock<Arc<Vec<Item>>>> =
    LazyLock::new(|| RwLock::new(Arc::new(Vec::new())));
static PESSOAS: LazyLock<RwLock<HashMap<String, Arc<Escolha>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

pub fn catalogo() -> Arc<Vec<Item>> {
    CATALOGO.read().map(|c| c.clone()).unwrap_or_default()
}

pub fn define_catalogo(itens: Vec<Item>) {
    if let Ok(mut c) = CATALOGO.write() {
        *c = Arc::new(itens);
    }
}

pub fn item(id: &str) -> Option<Item> {
    catalogo().iter().find(|i| i.id == id).cloned()
}

/// A escolha de uma pessoa (vazia se ela não escolheu nada).
pub fn de(uid: &str) -> Arc<Escolha> {
    PESSOAS
        .read()
        .ok()
        .and_then(|p| p.get(uid).cloned())
        .unwrap_or_default()
}

pub fn define_pessoa(uid: &str, escolha: Option<Escolha>) {
    if let Ok(mut p) = PESSOAS.write() {
        match escolha {
            Some(e) if e != Escolha::default() => {
                p.insert(uid.to_string(), Arc::new(e));
            }
            _ => {
                p.remove(uid);
            }
        }
    }
}

pub fn define_todas(mapa: HashMap<String, Escolha>) {
    if let Ok(mut p) = PESSOAS.write() {
        *p = mapa.into_iter().map(|(k, v)| (k, Arc::new(v))).collect();
    }
}

// --- Imagens ----------------------------------------------------------------

struct Animacao {
    quadros: Vec<Handle>,
    /// Fim de cada quadro, em ms desde o início do ciclo.
    fins: Vec<u64>,
    bytes: usize,
}

impl Animacao {
    fn total(&self) -> u64 {
        self.fins.last().copied().unwrap_or(1).max(1)
    }

    fn indice_em(&self, ms: u64) -> usize {
        self.fins
            .partition_point(|&f| f <= ms)
            .min(self.quadros.len() - 1)
    }

    /// O quadro do instante `ms` e, na metade final de um quadro longo
    /// (>= 60 ms: as decorações e efeitos são de 12 fps), o próximo entrando
    /// aos poucos por cima. `em_laco` falso: o último quadro não emenda no primeiro.
    fn quadro_em(&self, ms: u64, em_laco: bool) -> Quadro {
        let i = self.indice_em(ms);
        let inicio = if i == 0 { 0 } else { self.fins[i - 1] };
        let dur = self.fins[i].saturating_sub(inicio).max(1);
        let pos = ms.saturating_sub(inicio) as f32 / dur as f32;
        let ultimo = i + 1 >= self.quadros.len();
        let proximo = (dur >= 60 && pos > 0.5 && (em_laco || !ultimo)).then(|| {
            let j = (i + 1) % self.quadros.len();
            (self.quadros[j].clone(), ((pos - 0.5) / 0.5).clamp(0.0, 1.0))
        });
        Quadro {
            atual: self.quadros[i].clone(),
            proximo,
        }
    }
}

/// O que desenhar de uma animação: o quadro atual e, na transição, o próximo
/// com a opacidade dele (0..1).
#[derive(Clone)]
pub struct Quadro {
    pub atual: Handle,
    pub proximo: Option<(Handle, f32)>,
}

impl From<Handle> for Quadro {
    fn from(atual: Handle) -> Self {
        Self {
            atual,
            proximo: None,
        }
    }
}

/// Resultado de uma decodificação, entregue ao app pela `Msg`.
pub struct Decodificado {
    tamanho: (u32, u32),
    parado: Handle,
    animacao: Option<Animacao>,
}

impl std::fmt::Debug for Decodificado {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Decodificado")
    }
}

#[derive(Default)]
struct Imagens {
    parados: HashMap<String, Handle>,
    animadas: HashMap<String, (Arc<Animacao>, u64)>,
    /// Pedido de imagem parada / animação ainda não buscado.
    quer_parado: HashSet<String>,
    quer_animado: HashSet<String>,
    /// Já pedido (buscando) ou que falhou: não pede de novo.
    em_curso: HashSet<String>,
    falhas: HashSet<String>,
    tamanhos: HashMap<String, (u32, u32)>,
    /// url -> (início em ms, número do último desenho) do efeito que toca uma vez.
    uma_vez: HashMap<String, (u64, u64)>,
}

/// Memória para quadros de animação (RGBA). Passou, sai a mais antiga.
const ORCAMENTO: usize = 220 * 1024 * 1024;

static IMGS: LazyLock<Mutex<Imagens>> = LazyLock::new(|| Mutex::new(Imagens::default()));
static INICIO: LazyLock<Instant> = LazyLock::new(Instant::now);
static ULTIMO_USO: AtomicU64 = AtomicU64::new(0);

/// Número do desenho atual (`App::view`). O efeito de perfil volta ao início
/// só quando não foi desenhado no desenho anterior (o cartão tinha sumido),
/// não por tempo: parado, a tela não redesenha e o tempo enganava.
static DESENHO: AtomicU64 = AtomicU64::new(1);

pub fn novo_desenho() {
    DESENHO.fetch_add(1, Ordering::Relaxed);
}

fn agora_ms() -> u64 {
    INICIO.elapsed().as_millis() as u64 + 1
}

/// Liga o relógio de redesenho por um instante. O iced decide as assinaturas
/// antes de desenhar: sem isso, uma animação que acabou de aparecer (perfil
/// aberto, mouse em cima) ficaria parada no primeiro quadro até o próximo
/// evento. Se o desenho seguinte usar alguma animação, ele mantém ligado.
pub fn cutuca() {
    ULTIMO_USO.store(agora_ms(), Ordering::Relaxed);
}

/// Alguma animação foi desenhada há pouco? Liga o relógio de redesenho.
pub fn animando() -> bool {
    let u = ULTIMO_USO.load(Ordering::Relaxed);
    u != 0 && agora_ms().saturating_sub(u) < 400
}

/// Imagem parada (miniatura ou primeiro quadro). Pede se ainda não tem.
pub fn miniatura(url: &str) -> Option<Handle> {
    let mut m = IMGS.lock().ok()?;
    if let Some(h) = m.parados.get(url) {
        return Some(h.clone());
    }
    if !m.em_curso.contains(url) && !m.falhas.contains(url) {
        m.quer_parado.insert(url.to_string());
    }
    None
}

/// Largura e altura da imagem parada (já reduzida), se carregada.
pub fn tamanho(url: &str) -> Option<(u32, u32)> {
    let m = IMGS.lock().ok()?;
    m.tamanhos.get(url).copied()
}

/// Quadro de um cosmético. `animar` falso devolve o primeiro quadro (e não
/// gasta memória com a animação). Sem a animação ainda, também o parado.
pub fn quadro(url: &str, animar: bool) -> Option<Quadro> {
    quadro_com_inicio(url, animar, 0)
}

/// Como [`quadro`], mas a animação começa do primeiro quadro em `desde`
/// (o instante em que o mouse entrou); `None` devolve o quadro parado.
pub fn quadro_desde(url: &str, desde: Option<std::time::Instant>) -> Option<Quadro> {
    let inicio = desde.map(|d| agora_ms().saturating_sub(d.elapsed().as_millis() as u64));
    quadro_com_inicio(url, inicio.is_some(), inicio.unwrap_or(0))
}

fn quadro_com_inicio(url: &str, animar: bool, inicio: u64) -> Option<Quadro> {
    let animar = animar && !crate::desempenho::reduz_animacoes();
    let mut m = IMGS.lock().ok()?;
    if animar {
        let agora = agora_ms();
        if let Some((a, uso)) = m.animadas.get_mut(url) {
            *uso = agora;
            ULTIMO_USO.store(agora, Ordering::Relaxed);
            return Some(a.quadro_em(agora.saturating_sub(inicio) % a.total(), true));
        }
        if !m.em_curso.contains(url) && !m.falhas.contains(url) {
            m.quer_animado.insert(url.to_string());
        }
    } else if !m.parados.contains_key(url) && !m.em_curso.contains(url) && !m.falhas.contains(url) {
        m.quer_parado.insert(url.to_string());
    }
    m.parados.get(url).cloned().map(Quadro::from)
}

/// Efeito de perfil: toca uma vez a partir da primeira vez que é desenhado e
/// some no fim. Toca de novo só quando o cartão volta a aparecer (não foi
/// desenhado no desenho anterior).
pub fn quadro_uma_vez(url: &str) -> Option<Quadro> {
    if crate::desempenho::reduz_animacoes() {
        return quadro(url, false);
    }
    let mut m = IMGS.lock().ok()?;
    let agora = agora_ms();
    let Some((a, uso)) = m.animadas.get_mut(url) else {
        if !m.em_curso.contains(url) && !m.falhas.contains(url) {
            m.quer_animado.insert(url.to_string());
        }
        return None;
    };
    *uso = agora;
    let a = a.clone();
    let (inicio, ultimo) = m
        .uma_vez
        .entry(url.to_string())
        .or_insert((agora, DESENHO.load(Ordering::Relaxed)));
    let desenho = DESENHO.load(Ordering::Relaxed);
    if desenho.saturating_sub(*ultimo) > 1 {
        *inicio = agora;
    }
    *ultimo = desenho;
    let t = agora - *inicio;
    // Terminou: some (o último quadro dos efeitos é vazio ou quase).
    if t >= a.total() {
        return None;
    }
    ULTIMO_USO.store(agora, Ordering::Relaxed);
    Some(a.quadro_em(t, false))
}

/// O que o desenho pediu e ainda não foi buscado: (url, animar).
pub fn pedidos() -> Vec<(String, bool)> {
    let Ok(mut m) = IMGS.lock() else {
        return Vec::new();
    };
    let animados: Vec<String> = m.quer_animado.drain().collect();
    let parados: Vec<String> = m
        .quer_parado
        .drain()
        .filter(|u| !animados.contains(u))
        .collect();
    let mut saida = Vec::new();
    for u in animados {
        m.em_curso.insert(u.clone());
        saida.push((u, true));
    }
    for u in parados {
        m.em_curso.insert(u.clone());
        saida.push((u, false));
    }
    saida
}

pub fn tem_pedidos() -> bool {
    IMGS.lock()
        .map(|m| !m.quer_animado.is_empty() || !m.quer_parado.is_empty())
        .unwrap_or(false)
}

/// Guarda o resultado de uma busca+decodificação.
pub fn guarda(url: &str, r: Result<Decodificado, String>) {
    let Ok(mut m) = IMGS.lock() else { return };
    m.em_curso.remove(url);
    match r {
        Ok(d) => {
            m.tamanhos.insert(url.to_string(), d.tamanho);
            m.parados.insert(url.to_string(), d.parado);
            if let Some(a) = d.animacao {
                cutuca();
                m.animadas
                    .insert(url.to_string(), (Arc::new(a), agora_ms()));
                // Orçamento: tira a animação vista há mais tempo.
                loop {
                    let total: usize = m.animadas.values().map(|(a, _)| a.bytes).sum();
                    if total <= ORCAMENTO || m.animadas.len() <= 1 {
                        break;
                    }
                    let Some(velha) = m
                        .animadas
                        .iter()
                        .min_by_key(|(_, (_, uso))| *uso)
                        .map(|(k, _)| k.clone())
                    else {
                        break;
                    };
                    m.animadas.remove(&velha);
                }
            }
        }
        Err(e) => {
            log::warn!("[cosmeticos] {url}: {e}");
            m.falhas.insert(url.to_string());
        }
    }
}

/// Tipo pelo caminho do arquivo (a pasta diz o que é).
fn tipo_pelo_caminho(url: &str) -> Tipo {
    if url.contains("/placas-") {
        Tipo::Placa
    } else if url.contains("/efeitos-") {
        Tipo::Efeito
    } else if url.contains("/molduras-") {
        Tipo::Moldura
    } else {
        Tipo::Decoracao
    }
}

/// Lê os bytes e decodifica (em segundo plano). `animar` decodifica todos os
/// quadros; senão só o primeiro.
pub fn decodifica(url: &str, bytes: &[u8], animar: bool) -> Result<Decodificado, String> {
    let lado = if url.contains("/mini/") {
        640
    } else {
        tipo_pelo_caminho(url).lado_maximo()
    };
    let reduz = |img: image::RgbaImage| -> image::RgbaImage {
        let (w, h) = img.dimensions();
        let maior = w.max(h);
        if maior <= lado {
            return img;
        }
        let k = f64::from(lado) / f64::from(maior);
        image::imageops::resize(
            &img,
            ((f64::from(w) * k) as u32).max(1),
            ((f64::from(h) * k) as u32).max(1),
            image::imageops::FilterType::Triangle,
        )
    };
    let para_handle = |img: image::RgbaImage| {
        let (w, h) = img.dimensions();
        Handle::from_rgba(w, h, img.into_raw())
    };
    let eh_webp = bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP";
    if eh_webp {
        let dec =
            image::codecs::webp::WebPDecoder::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
        if dec.has_animation() {
            let mut quadros = Vec::new();
            let mut fins = Vec::new();
            let mut acum = 0u64;
            let mut total_bytes = 0usize;
            let mut parado = None;
            let mut tamanho = (1, 1);
            for f in dec.into_frames() {
                let f = f.map_err(|e| e.to_string())?;
                let (num, den) = f.delay().numer_denom_ms();
                let mut ms = num.checked_div(den).map_or(50, u64::from);
                if ms < 10 {
                    ms = 50;
                }
                let img = reduz(f.into_buffer());
                tamanho = img.dimensions();
                total_bytes += img.as_raw().len();
                let h = para_handle(img);
                if parado.is_none() {
                    parado = Some(h.clone());
                    if !animar {
                        break;
                    }
                }
                acum += ms;
                quadros.push(h);
                fins.push(acum);
            }
            let parado = parado.ok_or("sem quadros")?;
            let animacao = (animar && quadros.len() > 1).then_some(Animacao {
                quadros,
                fins,
                bytes: total_bytes,
            });
            return Ok(Decodificado {
                tamanho,
                parado,
                animacao,
            });
        }
    }
    let img = reduz(
        image::load_from_memory(bytes)
            .map_err(|e| e.to_string())?
            .to_rgba8(),
    );
    Ok(Decodificado {
        tamanho: img.dimensions(),
        parado: para_handle(img),
        animacao: None,
    })
}

// --- Disco ------------------------------------------------------------------

fn dir_do_cache() -> Option<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "lincast-chat")?;
    Some(dirs.cache_dir().join("cosmeticos"))
}

fn caminho_no_disco(url: &str) -> Option<PathBuf> {
    Some(dir_do_cache()?.join(format!("{:016x}.bin", crate::avatar::hash_texto(url))))
}

/// O arquivo nunca muda de conteúdo (nome novo = item novo): o disco vale
/// para sempre.
pub fn le_do_disco(url: &str) -> Option<Vec<u8>> {
    std::fs::read(caminho_no_disco(url)?).ok()
}

pub fn grava_no_disco(url: &str, bytes: &[u8]) {
    if let Some(p) = caminho_no_disco(url) {
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let tmp = p.with_extension("tmp");
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(tmp, p);
        }
    }
}

// --- Cor dominante (filtro "Cor") ------------------------------------------

/// Cores do filtro, como no Discord: roxo, azul, verde, marrom, amarelo,
/// laranja, vermelho, rosa, branco, preto.
pub const CORES_FILTRO: [(u8, u8, u8); 10] = [
    (0x9b, 0x59, 0xb6),
    (0x34, 0x98, 0xdb),
    (0x2e, 0xcc, 0x71),
    (0xa0, 0x52, 0x2d),
    (0xf1, 0xc4, 0x0f),
    (0xe6, 0x7e, 0x22),
    (0xe7, 0x4c, 0x3c),
    (0xe8, 0x43, 0x93),
    (0xf0, 0xf0, 0xf0),
    (0x2c, 0x2c, 0x30),
];

static COR_DE: LazyLock<RwLock<HashMap<String, usize>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Índice em `CORES_FILTRO` da cor que domina a miniatura, se já calculada.
pub fn cor_de(url: &str) -> Option<usize> {
    COR_DE.read().ok()?.get(url).copied()
}

/// Calcula pela miniatura (pixels visíveis, por matiz).
pub fn calcula_cor(url: &str, bytes: &[u8]) {
    let Ok(img) = image::load_from_memory(bytes) else {
        return;
    };
    let img = img.thumbnail(48, 48).to_rgba8();
    let mut votos = [0f32; 10];
    for p in img.pixels() {
        let [r, g, b, a] = p.0;
        if a < 128 {
            continue;
        }
        let (r, g, b) = (
            f32::from(r) / 255.0,
            f32::from(g) / 255.0,
            f32::from(b) / 255.0,
        );
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let sat = if max > 0.0 { (max - min) / max } else { 0.0 };
        let idx = if sat < 0.18 {
            if max > 0.75 {
                8
            } else if max < 0.3 {
                9
            } else {
                continue;
            }
        } else {
            let d = max - min;
            let mut h = if max == r {
                ((g - b) / d).rem_euclid(6.0)
            } else if max == g {
                (b - r) / d + 2.0
            } else {
                (r - g) / d + 4.0
            } * 60.0;
            if h < 0.0 {
                h += 360.0;
            }
            match h as u32 {
                0..=14 | 345..=360 => 6,
                15..=40 if max < 0.6 => 3,
                15..=40 => 5,
                41..=70 => 4,
                71..=170 => 2,
                171..=250 => 1,
                251..=300 => 0,
                _ => 7,
            }
        };
        votos[idx] += sat.max(0.3);
    }
    let melhor = votos
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .filter(|(_, v)| **v > 0.0)
        .map(|(i, _)| i);
    if let (Some(i), Ok(mut c)) = (melhor, COR_DE.write()) {
        c.insert(url.to_string(), i);
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn escolha_vazia_some_do_mapa() {
        define_pessoa("local:x", Some(Escolha::default()));
        assert_eq!(*de("local:x"), Escolha::default());
        let e = Escolha {
            decoracao: Some(Ref {
                id: "a".into(),
                arquivo: "/cosmeticos/a.webp".into(),
            }),
            ..Default::default()
        };
        define_pessoa("local:x", Some(e.clone()));
        assert_eq!(*de("local:x"), e);
        define_pessoa("local:x", None);
        assert!(de("local:x").decoracao.is_none());
    }

    #[test]
    fn envio_leva_ids() {
        let e = Escolha {
            placa: Some(Ref {
                id: "p1".into(),
                arquivo: "/x.webp".into(),
            }),
            ..Default::default()
        };
        let v = e.para_envio();
        assert_eq!(v["placa"], "p1");
        assert!(v["decoracao"].is_null());
    }

    #[test]
    fn decodifica_webp_animado_e_parado() {
        // WebP de dois quadros feito à mão não é trivial; usa PNG parado.
        let mut png = Vec::new();
        image::RgbaImage::from_pixel(400, 400, image::Rgba([255, 0, 0, 255]))
            .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let d = decodifica("/cosmeticos/decoracoes-avatar/webp/x.webp", &png, true).unwrap();
        assert!(d.animacao.is_none());
    }

    #[test]
    fn cor_dominante_vermelha() {
        let mut png = Vec::new();
        image::RgbaImage::from_pixel(10, 10, image::Rgba([230, 30, 30, 255]))
            .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        calcula_cor("/t/vermelho", &png);
        assert_eq!(cor_de("/t/vermelho"), Some(6));
    }
}
