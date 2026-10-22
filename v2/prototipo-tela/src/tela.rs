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
pub fn inicia(fps: u32, cursor: bool, x11: bool) -> Option<Captura> {
    let (tx_res, rx_res) = mpsc::channel::<Option<VideoResolution>>();
    let (tx_parar, rx_parar) = mpsc::channel::<()>();
    let fonte: Arc<Mutex<Option<NativeVideoSource>>> = Arc::default();
    let quadros: Arc<Mutex<u64>> = Arc::default();

    {
        let fonte = fonte.clone();
        let quadros = quadros.clone();
        std::thread::Builder::new()
            .name("captura-tela".into())
            .spawn(move || laco(fps, cursor, x11, tx_res, rx_parar, fonte, quadros))
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

fn laco(
    fps: u32,
    cursor: bool,
    x11: bool,
    tx_res: mpsc::Sender<Option<VideoResolution>>,
    rx_parar: mpsc::Receiver<()>,
    fonte: Arc<Mutex<Option<NativeVideoSource>>>,
    quadros: Arc<Mutex<u64>>,
) {
    let mut avisou = false;
    let tx_res_cb = tx_res.clone();
    let mut quadro = VideoFrame {
        rotation: VideoRotation::VideoRotation0,
        timestamp_us: 0,
        frame_metadata: None,
        buffer: I420Buffer::new(2, 2),
    };
    let callback = move |resultado: Result<DesktopFrame, CaptureError>| {
        let f = match resultado {
            Ok(f) => f,
            Err(CaptureError::Temporary) => return,
            Err(CaptureError::Permanent) => {
                log::error!("[tela] a captura falhou de vez (seletor cancelado ou portal ausente)");
                if !avisou {
                    let _ = tx_res_cb.send(None);
                    avisou = true;
                }
                return;
            }
        };
        let (w, h) = (f.width(), f.height());
        if w <= 0 || h <= 0 {
            return;
        }
        if !avisou {
            log::info!("[tela] primeira imagem: {w}x{h}");
            let _ = tx_res_cb.send(Some(VideoResolution {
                width: w as u32,
                height: h as u32,
            }));
            avisou = true;
        }
        if quadro.buffer.width() as i32 != w || quadro.buffer.height() as i32 != h {
            log::info!("[tela] resolução mudou para {w}x{h}");
            quadro.buffer = I420Buffer::new(w as u32, h as u32);
        }
        let (sy, su, sv) = quadro.buffer.strides();
        let (y, u, v) = quadro.buffer.data_mut();
        yuv_helper::argb_to_i420(f.data(), f.stride(), y, sy, u, su, v, sv, w, h);
        if let Some(fonte) = fonte.lock().unwrap().as_ref() {
            fonte.capture_frame(&quadro);
            *quadros.lock().unwrap() += 1;
        }
    };

    // Wayland: `Generic` = diálogo do portal. X11 não tem portal obrigatório:
    // o capturador de tela do libwebrtc lê o servidor X direto.
    let tipo = if x11 {
        DesktopCaptureSourceType::Screen
    } else {
        DesktopCaptureSourceType::Generic
    };
    let mut opcoes = DesktopCapturerOptions::new(tipo);
    opcoes.set_include_cursor(cursor);
    let Some(mut capturador) = DesktopCapturer::new(opcoes) else {
        log::error!("[tela] o libwebrtc não conseguiu criar o capturador (sem portal/PipeWire?)");
        let _ = tx_res.send(None);
        return;
    };
    // No modo Generic a fonte é escolhida no diálogo do portal, não aqui; no
    // X11 vai a primeira tela da lista.
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

    let intervalo = Duration::from_millis(1000 / fps.clamp(1, 60) as u64);
    loop {
        match rx_parar.recv_timeout(intervalo) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => capturador.capture_frame(),
        }
    }
    log::info!("[tela] captura encerrada");
}
