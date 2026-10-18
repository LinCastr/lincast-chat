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
    let mut entrada: Vec<f32> = Vec::with_capacity(QUADRO * 4);
    let mut limpo = [0f32; QUADRO];
    let mut bytes = [0u8; QUADRO * 2];
    let mut saida = std::io::stdout().lock();
    while let Ok(bloco) = rx.recv() {
        entrada.extend(bloco);
        while entrada.len() >= QUADRO {
            // O RNNoise trabalha na escala do i16.
            let quadro: Vec<f32> = entrada.drain(..QUADRO).map(|s| s * 32767.0).collect();
            ruido.process_frame(&mut limpo, &quadro);
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
