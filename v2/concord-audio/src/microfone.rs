//! Microfone com supressão de ruído (RNNoise, pelo `nnnoiseless`).
//!
//! O microfone normal do app vem do módulo de áudio do libwebrtc, onde não dá
//! para pôr um filtro no meio. Com a supressão ligada, o app usa este modo:
//! a captura vem direto do PipeWire, passa pelo RNNoise em quadros de 10 ms e
//! sai no stdout como PCM 16 bits, 48 kHz, mono, little-endian.

use std::io::Write;
use std::sync::mpsc;

use anyhow::Result;
use nnnoiseless::DenoiseState;
use pipewire as pw;
use pw::properties::properties;
use pw::spa;
use pw::stream::StreamRc;

/// Amostras de 10 ms a 48 kHz (o quadro do RNNoise).
const QUADRO: usize = DenoiseState::FRAME_SIZE;

/// Roda até o stdout fechar. `alvo`: `node.name` ou serial do microfone
/// (vazio = o padrão do sistema).
pub fn roda(alvo: Option<String>) -> Result<()> {
    let (tx, rx) = mpsc::channel::<Vec<f32>>();
    std::thread::Builder::new()
        .name("pw-microfone".into())
        .spawn(move || {
            if let Err(e) = captura(alvo, tx) {
                log::error!("[mic] PipeWire: {e}");
            }
        })?;

    let mut ruido = DenoiseState::new();
    let mut ganho = GanhoAutomatico::default();
    let mut entrada: Vec<f32> = Vec::with_capacity(QUADRO * 4);
    let mut limpo = [0f32; QUADRO];
    let mut bytes = [0u8; QUADRO * 2];
    let mut saida = std::io::stdout().lock();
    while let Ok(bloco) = rx.recv() {
        entrada.extend(bloco);
        while entrada.len() >= QUADRO {
            // O RNNoise trabalha na escala do i16.
            let quadro: Vec<f32> = entrada.drain(..QUADRO).map(|s| s * 32767.0).collect();
            let voz = ruido.process_frame(&mut limpo, &quadro);
            ganho.processa(&mut limpo, voz);
            for (b, s) in bytes.as_chunks_mut::<2>().0.iter_mut().zip(limpo) {
                *b = (s.clamp(-32768.0, 32767.0) as i16).to_le_bytes();
            }
            if saida.write_all(&bytes).and_then(|_| saida.flush()).is_err() {
                return Ok(()); // O app principal fechou.
            }
        }
    }
    Ok(())
}

/// Automatic gain after RNNoise. Frames sent through a `NativeAudioSource` skip
/// libwebrtc's audio processing, so without this the voice went out with no
/// AGC at all and sounded much quieter than the normal microphone.
/// The gain only moves on frames RNNoise flags as voice; a soft limiter keeps
/// loud peaks from clipping.
pub(crate) struct GanhoAutomatico {
    ganho: f32,
}

impl Default for GanhoAutomatico {
    fn default() -> Self {
        Self { ganho: 2.0 }
    }
}

impl GanhoAutomatico {
    /// Speech RMS target, on the i16 scale (about -18 dBFS).
    const ALVO: f32 = 4000.0;
    const MAXIMO: f32 = 10.0;

    pub(crate) fn processa(&mut self, quadro: &mut [f32], voz: f32) {
        let rms = (quadro.iter().map(|s| s * s).sum::<f32>() / quadro.len() as f32).sqrt();
        if voz > 0.6 && rms > 50.0 {
            let desejado = (Self::ALVO / rms).clamp(1.0, Self::MAXIMO);
            // Fast to go down (a shout), slow to go up (about 1 s).
            let passo = if desejado < self.ganho { 0.3 } else { 0.01 };
            self.ganho += (desejado - self.ganho) * passo;
        }
        for s in quadro.iter_mut() {
            *s = limita(*s * self.ganho);
        }
    }
}

/// Linear up to 24000, then bends smoothly towards 32767 without clipping.
fn limita(x: f32) -> f32 {
    const JOELHO: f32 = 24000.0;
    const FOLGA: f32 = 32767.0 - JOELHO;
    let a = x.abs();
    if a <= JOELHO {
        return x;
    }
    let y = JOELHO + FOLGA * ((a - JOELHO) / FOLGA).tanh();
    y.copysign(x)
}

fn captura(alvo: Option<String>, tx: mpsc::Sender<Vec<f32>>) -> Result<(), pw::Error> {
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;
    let mut props = properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_ROLE => "Communication",
        *pw::keys::NODE_NAME => "lincast-microfone",
        *pw::keys::APP_NAME => "Lincast Chat",
    };
    if let Some(a) = alvo.filter(|a| !a.is_empty()) {
        props.insert(*pw::keys::TARGET_OBJECT, a);
    }
    let stream = StreamRc::new(core.clone(), "lincast-microfone", props)?;
    let _ouvinte = stream
        .add_local_listener_with_user_data(())
        .process(move |stream, _| {
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };
            let Some(data) = buffer.datas_mut().first_mut() else {
                return;
            };
            let offset = data.chunk().offset() as usize;
            let size = data.chunk().size() as usize;
            let Some(bytes) = data.data().and_then(|b| b.get(offset..offset + size)) else {
                return;
            };
            let amostras = bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|b| f32::from_le_bytes(*b))
                .collect();
            let _ = tx.send(amostras);
        })
        .register()?;

    let mut info = spa::param::audio::AudioInfoRaw::new();
    info.set_format(spa::param::audio::AudioFormat::F32LE);
    info.set_rate(48_000);
    info.set_channels(1);
    let mut posicao = [0u32; spa::sys::SPA_AUDIO_MAX_CHANNELS as usize];
    posicao[0] = spa::sys::SPA_AUDIO_CHANNEL_MONO;
    info.set_position(posicao);
    let objeto = spa::pod::Object {
        type_: spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
        id: spa::param::ParamType::EnumFormat.as_raw(),
        properties: info.into(),
    };
    let bytes: Vec<u8> = spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &spa::pod::Value::Object(objeto),
    )
    .expect("formato de áudio inválido")
    .0
    .into_inner();
    let mut params = [spa::pod::Pod::from_bytes(&bytes).unwrap()];
    stream.connect(
        spa::utils::Direction::Input,
        None,
        pw::stream::StreamFlags::AUTOCONNECT
            | pw::stream::StreamFlags::MAP_BUFFERS
            | pw::stream::StreamFlags::RT_PROCESS,
        &mut params,
    )?;
    mainloop.run();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seno(amplitude: f32) -> Vec<f32> {
        (0..QUADRO)
            .map(|i| amplitude * (i as f32 * 0.1).sin())
            .collect()
    }

    fn rms(q: &[f32]) -> f32 {
        (q.iter().map(|s| s * s).sum::<f32>() / q.len() as f32).sqrt()
    }

    #[test]
    fn voz_baixa_sobe_para_o_alvo() {
        let mut g = GanhoAutomatico::default();
        let mut q = seno(700.0);
        for _ in 0..500 {
            q = seno(700.0);
            g.processa(&mut q, 0.9);
        }
        let r = rms(&q);
        assert!((3500.0..4500.0).contains(&r), "rms {r}");
    }

    #[test]
    fn ruido_sem_voz_nao_mexe_no_ganho() {
        let mut g = GanhoAutomatico::default();
        for _ in 0..500 {
            g.processa(&mut seno(300.0), 0.1);
        }
        assert_eq!(g.ganho, 2.0);
    }

    #[test]
    fn grito_nao_estoura() {
        let mut g = GanhoAutomatico { ganho: 8.0 };
        let mut q = seno(20000.0);
        g.processa(&mut q, 0.9);
        assert!(q.iter().all(|s| s.abs() <= 32767.0));
        for _ in 0..20 {
            q = seno(20000.0);
            g.processa(&mut q, 0.9);
        }
        assert!(g.ganho < 1.5, "ganho {}", g.ganho);
    }
}
