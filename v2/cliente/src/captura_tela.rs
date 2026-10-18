//! Vídeo da tela pelo portal do Wayland.
//!
//! Quem faz o trabalho pesado é o capturador de tela do próprio libwebrtc (o
//! mesmo código que o Chromium usa): no Linux, com o tipo `Generic`, ele abre
//! o diálogo do portal (`xdg-desktop-portal`), a pessoa escolhe monitor ou
//! janela, e os quadros chegam pelo PipeWire. O portal precisa de um loop do
//! glib rodando — é o que a feature `glib-main-loop` do crate liga.

use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use libwebrtc::desktop_capturer::{
    CaptureError, DesktopCaptureSourceType, DesktopCapturer, DesktopCapturerOptions, DesktopFrame,
};
use libwebrtc::native::yuv_helper;
use libwebrtc::prelude::{I420Buffer, VideoBuffer, VideoFrame, VideoResolution, VideoRotation};
use libwebrtc::video_source::native::NativeVideoSource;

#[allow(dead_code)] // `resolucao`/`quadros` são lidos pelo protótipo, não pelo cliente.
pub struct Captura {
    pub fonte: NativeVideoSource,
    pub resolucao: VideoResolution,
    parar: mpsc::Sender<()>,
    pub quadros: Arc<Mutex<u64>>,
}

impl Captura {
    pub fn para(&self) {
        let _ = self.parar.send(());
    }
}

/// Abre o seletor do sistema e espera a primeira imagem. Devolve `None` se a
/// pessoa cancelar ou se o portal não existir.
/// Quadro de prévia local: largura, altura e RGBA.
pub type QuadroPrevia = (u32, u32, Vec<u8>);

pub fn inicia(
    fps: u32,
    cursor: bool,
    x11: bool,
    previa: Option<tokio::sync::mpsc::UnboundedSender<QuadroPrevia>>,
) -> Option<Captura> {
    let (tx_res, rx_res) = mpsc::channel::<Option<VideoResolution>>();
    let (tx_parar, rx_parar) = mpsc::channel::<()>();
    let fonte: Arc<Mutex<Option<NativeVideoSource>>> = Arc::default();
    let quadros: Arc<Mutex<u64>> = Arc::default();

    {
        let fonte = fonte.clone();
        let quadros = quadros.clone();
        std::thread::Builder::new()
            .name("captura-tela".into())
            .spawn(move || laco(fps, cursor, x11, tx_res, rx_parar, fonte, quadros, previa))
            .expect("não consegui criar a thread de captura");
    }

    // O diálogo do portal fica esperando a pessoa; não há prazo aqui de
    // propósito — quem escolhe a janela está olhando pra tela.
    let resolucao = rx_res.recv().ok().flatten()?;
    let nova = NativeVideoSource::new(resolucao.clone(), true);
    *fonte.lock().unwrap() = Some(nova.clone());
    Some(Captura {
        fonte: nova,
        resolucao,
        parar: tx_parar,
        quadros,
    })
}

/// Teto de resolução da transmissão.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolucaoTela {
    P720,
    P1080,
    P1440,
    /// Sem teto: a do que está sendo capturado.
    Original,
}

impl ResolucaoTela {
    pub const TODAS: [ResolucaoTela; 4] = [
        ResolucaoTela::P720,
        ResolucaoTela::P1080,
        ResolucaoTela::P1440,
        ResolucaoTela::Original,
    ];

    pub fn rotulo(&self) -> &'static str {
        match self {
            ResolucaoTela::P720 => "720p",
            ResolucaoTela::P1080 => "1080p",
            ResolucaoTela::P1440 => "1440p",
            ResolucaoTela::Original => "Original",
        }
    }

    /// Altura máxima em pixels, se houver teto.
    pub fn altura_maxima(&self) -> Option<u32> {
        match self {
            ResolucaoTela::P720 => Some(720),
            ResolucaoTela::P1080 => Some(1080),
            ResolucaoTela::P1440 => Some(1440),
            ResolucaoTela::Original => None,
        }
    }
}

pub const FPS_DA_TELA: [u32; 3] = [15, 30, 60];

/// Resolução e quadros por segundo da transmissão de tela.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PerfilTela {
    pub resolucao: ResolucaoTela,
    pub fps: u32,
    /// Altura do que está sendo capturado (0 = ainda não se sabe). Entra no
    /// cálculo do teto de bitrate e do fator de redução.
    pub fonte_altura: u32,
}

impl Default for PerfilTela {
    fn default() -> Self {
        Self::PADRAO
    }
}

impl PerfilTela {
    /// 1080p a 30 quadros: o equilíbrio.
    pub const PADRAO: PerfilTela = PerfilTela {
        resolucao: ResolucaoTela::P1080,
        fps: 30,
        fonte_altura: 0,
    };

    pub fn rotulo(&self) -> String {
        format!("{} · {} fps", self.resolucao.rotulo(), self.fps)
    }

    pub fn fps(&self) -> u32 {
        self.fps.clamp(1, 60)
    }

    /// Altura que vai de fato no ar: o menor entre o teto e a fonte.
    fn altura_efetiva(&self) -> u32 {
        match (self.resolucao.altura_maxima(), self.fonte_altura) {
            (Some(teto), 0) => teto,
            (Some(teto), fonte) => teto.min(fonte),
            (None, 0) => 1080,
            (None, fonte) => fonte,
        }
    }

    /// Quanto reduzir a imagem para respeitar o teto (1.0 = não reduz).
    pub fn fator_de_reducao(&self) -> f64 {
        match (self.resolucao.altura_maxima(), self.fonte_altura) {
            (Some(teto), fonte) if fonte > teto => f64::from(fonte) / f64::from(teto),
            _ => 1.0,
        }
    }

    /// Teto de bitrate: ~0,04 bit por pixel por quadro (que é o que os perfis
    /// antigos davam: 1080p30 → 2,5 Mbps), entre 0,6 e 8 Mbps.
    pub fn teto_bitrate(&self) -> u64 {
        let alt = u64::from(self.altura_efetiva());
        let larg = alt * 16 / 9;
        let bits = larg * alt * u64::from(self.fps()) * 4 / 100;
        bits.clamp(600_000, 8_000_000)
    }
}

/// BGRA (o que o capturador entrega) → RGBA reduzido a no máximo ~640 de largura.
fn reduz_para_previa(dados: &[u8], stride: u32, w: u32, h: u32) -> Option<QuadroPrevia> {
    let passo = (w / 640).max(1);
    let (nw, nh) = (w / passo, h / passo);
    if nw == 0 || nh == 0 || stride == 0 {
        return None;
    }
    let stride = stride as usize;
    let mut rgba = Vec::with_capacity((nw * nh * 4) as usize);
    for y in 0..nh {
        let linha = (y * passo) as usize * stride;
        for x in 0..nw {
            let i = linha + (x * passo) as usize * 4;
            let px = dados.get(i..i + 4)?;
            rgba.extend_from_slice(&[px[2], px[1], px[0], 255]);
        }
    }
    Some((nw, nh, rgba))
}

/// Recebe quadros BGRA de qualquer origem (libwebrtc ou portal próprio):
/// converte para I420, entrega à fonte de vídeo e manda a prévia local.
struct Entrega {
    avisou: bool,
    ultima_previa: std::time::Instant,
    tx_res: mpsc::Sender<Option<VideoResolution>>,
    quadro: VideoFrame<I420Buffer>,
    fonte: Arc<Mutex<Option<NativeVideoSource>>>,
    quadros: Arc<Mutex<u64>>,
    previa: Option<tokio::sync::mpsc::UnboundedSender<QuadroPrevia>>,
}

impl Entrega {
    fn nova(
        tx_res: mpsc::Sender<Option<VideoResolution>>,
        fonte: Arc<Mutex<Option<NativeVideoSource>>>,
        quadros: Arc<Mutex<u64>>,
        previa: Option<tokio::sync::mpsc::UnboundedSender<QuadroPrevia>>,
    ) -> Self {
        Self {
            avisou: false,
            ultima_previa: std::time::Instant::now() - std::time::Duration::from_secs(1),
            tx_res,
            quadro: VideoFrame {
                rotation: VideoRotation::VideoRotation0,
                timestamp_us: 0,
                frame_metadata: None,
                buffer: I420Buffer::new(2, 2),
            },
            fonte,
            quadros,
            previa,
        }
    }

    /// A captura falhou de vez (avisa uma vez só quem está esperando).
    fn falhou(&mut self) {
        if !self.avisou {
            let _ = self.tx_res.send(None);
            self.avisou = true;
        }
    }

    fn recebe(&mut self, dados: &[u8], stride: u32, w: i32, h: i32) {
        if w <= 0 || h <= 0 {
            return;
        }
        if !self.avisou {
            log::info!("[tela] primeira imagem: {w}x{h}");
            let _ = self.tx_res.send(Some(VideoResolution {
                width: w as u32,
                height: h as u32,
            }));
            self.avisou = true;
            for _ in 0..50 {
                if self.fonte.lock().unwrap().is_some() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        if self.quadro.buffer.width() as i32 != w || self.quadro.buffer.height() as i32 != h {
            log::info!("[tela] resolução mudou para {w}x{h}");
            self.quadro.buffer = I420Buffer::new(w as u32, h as u32);
        }
        let (sy, su, sv) = self.quadro.buffer.strides();
        let (y, u, v) = self.quadro.buffer.data_mut();
        yuv_helper::argb_to_i420(dados, stride, y, sy, u, su, v, sv, w, h);
        self.quadro.timestamp_us = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as i64;
        if let Some(fonte) = self.fonte.lock().unwrap().as_ref() {
            fonte.capture_frame(&self.quadro);
            *self.quadros.lock().unwrap() += 1;
        }
        // Prévia para quem está transmitindo: ~30 quadros por segundo, reduzida.
        if let Some(tx) = &self.previa
            && self.ultima_previa.elapsed() >= std::time::Duration::from_millis(33)
        {
            self.ultima_previa = std::time::Instant::now();
            if let Some(q) = reduz_para_previa(dados, stride, w as u32, h as u32) {
                let _ = tx.send(q);
            }
        }
    }

    fn reenvia_ultimo(&mut self) {
        if !self.avisou {
            return;
        }
        self.quadro.timestamp_us = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as i64;
        if let Some(fonte) = self.fonte.lock().unwrap().as_ref() {
            fonte.capture_frame(&self.quadro);
            *self.quadros.lock().unwrap() += 1;
        }
    }
}

/// Wayland: tenta a sessão própria do portal (escolha lembrada). Devolve
/// `true` se a transmissão rodou e terminou por aqui; `false` se o portal
/// próprio não funcionou e o chamador deve usar o capturador do libwebrtc.
fn laco_do_portal(
    fps: u32,
    cursor: bool,
    rx_parar: &mpsc::Receiver<()>,
    ent: &mut Entrega,
) -> bool {
    let mut portal = match crate::portal_tela::abre(cursor, fps) {
        Ok(p) => p,
        Err(e) if e.downcast_ref::<crate::portal_tela::Cancelado>().is_some() => {
            log::info!("[tela] a pessoa cancelou o seletor do sistema");
            ent.falhou();
            return true;
        }
        Err(e) => {
            log::warn!("[tela] portal próprio indisponível ({e:#}); usando o seletor do libwebrtc");
            return false;
        }
    };
    let mut recebeu = false;
    let inicio = std::time::Instant::now();
    loop {
        if !matches!(rx_parar.try_recv(), Err(mpsc::TryRecvError::Empty)) {
            break;
        }
        match portal.quadros.recv_timeout(Duration::from_millis(100)) {
            Ok(q) => {
                recebeu = true;
                ent.recebe(&q.bgra, q.largura * 4, q.largura as i32, q.altura as i32);
            }
            Err(RecvTimeoutError::Timeout) => {
                if portal.morreu() || (!recebeu && inicio.elapsed() > Duration::from_secs(8)) {
                    break;
                }
                if recebeu {
                    ent.reenvia_ultimo();
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    portal.fecha();
    if recebeu {
        log::info!("[tela] captura pelo portal encerrada");
        return true;
    }
    log::warn!("[tela] o portal próprio não entregou imagem; usando o seletor do libwebrtc");
    false
}

#[allow(clippy::too_many_arguments)]
/// Roda o `DesktopCapturer` do libwebrtc (`Generic` no Wayland, `Screen` no X11).
/// Devolve `true` se entregou ao menos um quadro ou foi parado por `rx_parar`.
fn laco_do_capturador(
    fps: u32,
    cursor: bool,
    x11: bool,
    rx_parar: &mpsc::Receiver<()>,
    ent: Arc<Mutex<Entrega>>,
) -> bool {
    let falhou_permanente = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let recebeu = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let falhou_cb = falhou_permanente.clone();
    let recebeu_cb = recebeu.clone();
    let ent_cb = ent.clone();
    let callback = move |resultado: Result<DesktopFrame, CaptureError>| {
        let f = match resultado {
            Ok(f) => f,
            Err(CaptureError::Temporary) => return,
            Err(CaptureError::Permanent) => {
                log::error!("[tela] a captura falhou de vez (seletor cancelado ou portal ausente)");
                falhou_cb.store(true, std::sync::atomic::Ordering::Relaxed);
                return;
            }
        };
        recebeu_cb.store(true, std::sync::atomic::Ordering::Relaxed);
        ent_cb
            .lock()
            .unwrap()
            .recebe(f.data(), f.stride(), f.width() as i32, f.height() as i32);
    };

    let tipo = if x11 {
        DesktopCaptureSourceType::Screen
    } else {
        DesktopCaptureSourceType::Generic
    };
    let mut opcoes = DesktopCapturerOptions::new(tipo);
    opcoes.set_include_cursor(cursor);
    let Some(mut capturador) = DesktopCapturer::new(opcoes) else {
        log::error!("[tela] o libwebrtc não conseguiu criar o capturador (sem portal/PipeWire?)");
        return false;
    };
    let fonte_x11 = if x11 {
        capturador.get_source_list().into_iter().next()
    } else {
        None
    };
    if x11 {
        match &fonte_x11 {
            Some(f) => log::info!("[tela] X11: capturando {f}"),
            None => log::warn!("[tela] X11: nenhuma tela listada"),
        }
    }
    capturador.start_capture(fonte_x11, callback);
    capturador.capture_frame();
    if falhou_permanente.load(std::sync::atomic::Ordering::Relaxed) {
        return false;
    }

    let intervalo = Duration::from_millis(1000 / fps.clamp(1, 60) as u64);
    loop {
        if falhou_permanente.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        match rx_parar.recv_timeout(intervalo) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => {
                log::info!("[tela] captura encerrada");
                return true;
            }
            Err(RecvTimeoutError::Timeout) => capturador.capture_frame(),
        }
    }
    recebeu.load(std::sync::atomic::Ordering::Relaxed)
}

#[allow(clippy::too_many_arguments)]
fn laco(
    fps: u32,
    cursor: bool,
    x11: bool,
    tx_res: mpsc::Sender<Option<VideoResolution>>,
    rx_parar: mpsc::Receiver<()>,
    fonte: Arc<Mutex<Option<NativeVideoSource>>>,
    quadros: Arc<Mutex<u64>>,
    previa: Option<tokio::sync::mpsc::UnboundedSender<QuadroPrevia>>,
) {
    if !x11 {
        let mut ent = Entrega::nova(
            tx_res.clone(),
            fonte.clone(),
            quadros.clone(),
            previa.clone(),
        );
        if laco_do_portal(fps, cursor, &rx_parar, &mut ent) {
            return;
        }
    }
    let ent = Arc::new(Mutex::new(Entrega::nova(tx_res, fonte, quadros, previa)));
    if laco_do_capturador(fps, cursor, x11, &rx_parar, ent.clone()) {
        return;
    }
    if !x11 && std::env::var_os("DISPLAY").is_some() {
        log::info!("[tela] portal indisponível; tentando captura X11 via DISPLAY");
        if laco_do_capturador(fps, cursor, true, &rx_parar, ent.clone()) {
            return;
        }
    }
    ent.lock().unwrap().falhou();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perfis_de_tela_parametros() {
        let p = PerfilTela::PADRAO;
        assert_eq!(p.fps(), 30);
        // 1080p30 mantém o teto dos perfis antigos (~2,5 Mbps).
        assert!((2_300_000..=2_700_000).contains(&p.teto_bitrate()));
        let leve = PerfilTela {
            resolucao: ResolucaoTela::P720,
            fps: 15,
            fonte_altura: 0,
        };
        assert!(leve.teto_bitrate() < p.teto_bitrate());
        let jogo = PerfilTela {
            resolucao: ResolucaoTela::P1080,
            fps: 60,
            fonte_altura: 0,
        };
        assert!(jogo.teto_bitrate() > p.teto_bitrate());
    }

    #[test]
    fn fator_de_reducao_respeita_o_teto() {
        let mut p = PerfilTela {
            resolucao: ResolucaoTela::P720,
            fps: 30,
            fonte_altura: 1440,
        };
        assert!((p.fator_de_reducao() - 2.0).abs() < 1e-9);
        p.fonte_altura = 600;
        assert_eq!(p.fator_de_reducao(), 1.0, "não amplia");
        p.resolucao = ResolucaoTela::Original;
        p.fonte_altura = 2160;
        assert_eq!(p.fator_de_reducao(), 1.0);
        // O teto de bitrate acompanha a altura que vai no ar.
        let mut q = PerfilTela {
            resolucao: ResolucaoTela::P1440,
            fps: 30,
            fonte_altura: 1080,
        };
        let a = q.teto_bitrate();
        q.fonte_altura = 1440;
        assert!(q.teto_bitrate() > a);
    }

    #[test]
    fn rotulo_do_perfil() {
        assert_eq!(PerfilTela::PADRAO.rotulo(), "1080p · 30 fps");
    }
}

#[cfg(test)]
mod testes_previa {
    use super::*;

    #[test]
    fn previa_converte_bgra_para_rgba() {
        // 2x2 BGRA (sem redução: largura < 640).
        let dados = [
            255, 0, 0, 255, 0, 255, 0, 255, // azul, verde
            0, 0, 255, 255, 10, 20, 30, 255, // vermelho, outro
        ];
        let (w, h, rgba) = reduz_para_previa(&dados, 8, 2, 2).unwrap();
        assert_eq!((w, h), (2, 2));
        assert_eq!(&rgba[0..4], &[0, 0, 255, 255]); // azul vira RGBA(0,0,255)
        assert_eq!(&rgba[8..12], &[255, 0, 0, 255]); // vermelho
    }

    #[test]
    fn previa_recusa_dados_curtos() {
        assert!(reduz_para_previa(&[0; 4], 8, 2, 2).is_none());
    }
}
