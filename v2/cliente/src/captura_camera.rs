//! Captura de câmera de vídeo via V4L2 (Video4Linux).
//!
//! Permite listar as câmeras disponíveis no sistema, exibir prévia local
//! e alimentar trilhas de vídeo WebRTC usando `NativeVideoSource`.

use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Result, anyhow};
use libwebrtc::native::yuv_helper;
use libwebrtc::prelude::{I420Buffer, VideoFrame, VideoResolution, VideoRotation};
use libwebrtc::video_source::native::NativeVideoSource;
use v4l::buffer::Type;
use v4l::io::mmap::Stream;
use v4l::io::traits::CaptureStream;
use v4l::video::Capture;
use v4l::{FourCC, context};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispositivoCamera {
    pub indice: usize,
    pub nome: String,
    pub caminho: String,
}

/// Lista todas as câmeras de captura de vídeo presentes no sistema.
pub fn lista_cameras() -> Vec<DispositivoCamera> {
    let mut lista = Vec::new();
    for node in context::enum_devices() {
        if let Ok(dev) = v4l::Device::new(node.index())
            && let Ok(caps) = dev.query_caps()
            && caps
                .capabilities
                .contains(v4l::capability::Flags::VIDEO_CAPTURE)
        {
            let nome = node
                .name()
                .unwrap_or_else(|| format!("Câmera {}", node.index()));
            let caminho = node.path().to_string_lossy().to_string();
            lista.push(DispositivoCamera {
                indice: node.index(),
                nome,
                caminho,
            });
        }
    }
    lista
}

#[allow(dead_code)]
pub struct CapturaCamera {
    pub fonte: NativeVideoSource,
    pub resolucao: VideoResolution,
    parar: mpsc::Sender<()>,
    pub quadros: Arc<Mutex<u64>>,
}

impl CapturaCamera {
    pub fn para(&self) {
        let _ = self.parar.send(());
    }
}

/// Inicia a captura da câmera indicada por `indice` e alimenta `NativeVideoSource`.
/// Se fornecido `tx_quadros_local`, envia cópias dos quadros convertidos para RGBA
/// para exibição da própria câmera na interface.
pub fn inicia(
    indice: usize,
    largura_alvo: u32,
    altura_alvo: u32,
    tx_quadros_local: Option<tokio::sync::mpsc::UnboundedSender<(u32, u32, Vec<u8>)>>,
) -> Result<CapturaCamera> {
    let (tx_res, rx_res) = mpsc::channel::<Option<VideoResolution>>();
    let (tx_parar, rx_parar) = mpsc::channel::<()>();
    let fonte: Arc<Mutex<Option<NativeVideoSource>>> = Arc::default();
    let quadros: Arc<Mutex<u64>> = Arc::default();

    {
        let fonte = fonte.clone();
        let quadros = quadros.clone();
        std::thread::Builder::new()
            .name("captura-camera".into())
            .spawn(move || {
                laco_captura(
                    indice,
                    largura_alvo,
                    altura_alvo,
                    tx_res,
                    rx_parar,
                    fonte,
                    quadros,
                    tx_quadros_local,
                )
            })
            .map_err(|e| anyhow!("não consegui criar thread de captura: {e}"))?;
    }

    let resolucao = rx_res
        .recv_timeout(Duration::from_secs(5))
        .map_err(|_| anyhow!("tempo esgotado esperando primeiro quadro da câmera"))?
        .ok_or_else(|| anyhow!("falha ao inicializar dispositivo de câmera {indice}"))?;

    let nova = NativeVideoSource::new(resolucao.clone(), true);
    *fonte.lock().unwrap() = Some(nova.clone());

    Ok(CapturaCamera {
        fonte: nova,
        resolucao,
        parar: tx_parar,
        quadros,
    })
}

/// Loop de captura executado em thread dedicada.
#[allow(clippy::too_many_arguments)]
fn laco_captura(
    indice: usize,
    largura_alvo: u32,
    altura_alvo: u32,
    tx_res: mpsc::Sender<Option<VideoResolution>>,
    rx_parar: mpsc::Receiver<()>,
    fonte: Arc<Mutex<Option<NativeVideoSource>>>,
    quadros: Arc<Mutex<u64>>,
    tx_local: Option<tokio::sync::mpsc::UnboundedSender<(u32, u32, Vec<u8>)>>,
) {
    let dev = match v4l::Device::new(indice) {
        Ok(d) => d,
        Err(e) => {
            log::error!("[camera] não conseguiu abrir dispositivo {indice}: {e}");
            let _ = tx_res.send(None);
            return;
        }
    };

    // Tenta configurar preferencialmente YUYV ou MJPG
    let mut fmt = match dev.format() {
        Ok(f) => f,
        Err(e) => {
            log::error!("[camera] falha ao consultar formato de vídeo: {e}");
            let _ = tx_res.send(None);
            return;
        }
    };

    fmt.width = largura_alvo;
    fmt.height = altura_alvo;
    // Tenta definir formato YUYV por padrão; se falhar, tenta manter o padrão do dispositivo
    fmt.fourcc = FourCC::new(b"YUYV");
    let fmt = dev
        .set_format(&fmt)
        .unwrap_or_else(|_| dev.format().unwrap_or(fmt));

    let fourcc = fmt.fourcc;
    let (w, h) = (fmt.width, fmt.height);
    log::info!("[camera] dispositivo {indice} aberto: {w}x{h} ({fourcc})");

    let mut stream = match Stream::with_buffers(&dev, Type::VideoCapture, 4) {
        Ok(s) => s,
        Err(e) => {
            log::error!("[camera] falha ao iniciar mmap stream: {e}");
            let _ = tx_res.send(None);
            return;
        }
    };

    let mut avisou = false;
    let mut quadro_rtc = VideoFrame {
        rotation: VideoRotation::VideoRotation0,
        timestamp_us: 0,
        frame_metadata: None,
        buffer: I420Buffer::new(w, h),
    };

    let mut rgba_buf = vec![0u8; (w * h * 4) as usize];
    let mut ultimo_envio_local = std::time::Instant::now() - Duration::from_secs(1);

    loop {
        if rx_parar.try_recv().is_ok() {
            break;
        }

        let (buf, _meta) = match stream.next() {
            Ok(par) => par,
            Err(e) => {
                log::warn!("[camera] erro ao ler quadro: {e}");
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
        };

        let mut converteu = false;
        if fourcc == FourCC::new(b"YUYV") && buf.len() >= (w * h * 2) as usize {
            converte_yuyv_para_i420(buf, w as usize, h as usize, &mut quadro_rtc.buffer);
            if tx_local.is_some() && ultimo_envio_local.elapsed() >= Duration::from_millis(33) {
                converte_yuyv_para_rgba(buf, w as usize, h as usize, &mut rgba_buf);
                converteu = true;
            }
        } else if fourcc == FourCC::new(b"MJPG")
            && let Ok(img) = image::load_from_memory_with_format(buf, image::ImageFormat::Jpeg)
        {
            let img_rgba = img.to_rgba8();
            let raw_rgba = img_rgba.as_raw();
            let (sy, su, sv) = quadro_rtc.buffer.strides();
            let (y, u, v) = quadro_rtc.buffer.data_mut();
            yuv_helper::abgr_to_i420(raw_rgba, w * 4, y, sy, u, su, v, sv, w as i32, h as i32);
            if tx_local.is_some() && ultimo_envio_local.elapsed() >= Duration::from_millis(33) {
                rgba_buf.copy_from_slice(raw_rgba);
                converteu = true;
            }
        }

        if !avisou {
            let _ = tx_res.send(Some(VideoResolution {
                width: w,
                height: h,
            }));
            avisou = true;
        }

        if let Some(f) = fonte.lock().unwrap().as_ref() {
            f.capture_frame(&quadro_rtc);
            *quadros.lock().unwrap() += 1;
        }

        if converteu && let Some(tx) = &tx_local {
            ultimo_envio_local = std::time::Instant::now();
            let _ = tx.send((w, h, rgba_buf.clone()));
        }
    }

    log::info!("[camera] captura finalizada.");
}

/// Converte buffer YUYV (4:2:2 empacotado) para I420 (plano YUV 4:2:0).
pub fn converte_yuyv_para_i420(yuyv: &[u8], w: usize, h: usize, buffer: &mut I420Buffer) {
    let (sy, su, sv) = buffer.strides();
    let (y_plane, u_plane, v_plane) = buffer.data_mut();
    let sy = sy as usize;
    let su = su as usize;
    let sv = sv as usize;

    for row in 0..h {
        let src_row = &yuyv[row * w * 2..(row + 1) * w * 2];
        let dst_y = &mut y_plane[row * sy..row * sy + w];
        for col in 0..(w / 2) {
            dst_y[col * 2] = src_row[col * 4];
            dst_y[col * 2 + 1] = src_row[col * 4 + 2];
        }
        if row % 2 == 0 {
            let uv_row = row / 2;
            let dst_u = &mut u_plane[uv_row * su..uv_row * su + (w / 2)];
            let dst_v = &mut v_plane[uv_row * sv..uv_row * sv + (w / 2)];
            for col in 0..(w / 2) {
                dst_u[col] = src_row[col * 4 + 1];
                dst_v[col] = src_row[col * 4 + 3];
            }
        }
    }
}

/// Converte buffer YUYV (4:2:2) para RGBA (8-bit por canal).
pub fn converte_yuyv_para_rgba(yuyv: &[u8], w: usize, h: usize, rgba: &mut [u8]) {
    let total_pixels = w * h;
    for i in 0..(total_pixels / 2) {
        let y0 = yuyv[i * 4] as i32;
        let u = yuyv[i * 4 + 1] as i32;
        let y1 = yuyv[i * 4 + 2] as i32;
        let v = yuyv[i * 4 + 3] as i32;

        let c0 = y0 - 16;
        let c1 = y1 - 16;
        let d = u - 128;
        let e = v - 128;

        let r0 = ((298 * c0 + 409 * e + 128) >> 8).clamp(0, 255) as u8;
        let g0 = ((298 * c0 - 100 * d - 208 * e + 128) >> 8).clamp(0, 255) as u8;
        let b0 = ((298 * c0 + 516 * d + 128) >> 8).clamp(0, 255) as u8;

        let r1 = ((298 * c1 + 409 * e + 128) >> 8).clamp(0, 255) as u8;
        let g1 = ((298 * c1 - 100 * d - 208 * e + 128) >> 8).clamp(0, 255) as u8;
        let b1 = ((298 * c1 + 516 * d + 128) >> 8).clamp(0, 255) as u8;

        let base = i * 8;
        if base + 8 <= rgba.len() {
            rgba[base..base + 4].copy_from_slice(&[r0, g0, b0, 255]);
            rgba[base + 4..base + 8].copy_from_slice(&[r1, g1, b1, 255]);
        }
    }
}
