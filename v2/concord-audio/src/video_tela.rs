//! Vídeo da tela por uma sessão do portal ScreenCast.
//!
//! O app principal abre a sessão do portal (escolha da tela/janela, lembrada
//! entre usos) e entrega a este processo o descritor do PipeWire pela entrada
//! padrão. Aqui só se lê o vídeo e se escreve na saída padrão, quadro a quadro:
//!
//! ```text
//! [largura u32 LE][altura u32 LE][tamanho u32 LE][pixels BGRA, sem sobra de linha]
//! ```
//!
//! É processo à parte pelo mesmo motivo do áudio: o libwebrtc embute stubs do
//! PipeWire que quebram o `pipewire-rs` se estiverem no mesmo binário.

use std::io::Write;
use std::os::fd::OwnedFd;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use pipewire as pw;
use pw::spa;
use spa::param::video::VideoFormat;

struct Dados {
    formato: spa::param::video::VideoInfoRaw,
    ultimo: Instant,
    intervalo: Duration,
    saida: Vec<u8>,
    chamadas: u32,
}

/// Troca R e B de cada pixel (RGBx/RGBA → BGRA).
fn troca_r_b(px: &mut [u8]) {
    for p in px.chunks_exact_mut(4) {
        p.swap(0, 2);
    }
}

/// Copia as linhas sem a sobra do stride para `saida` (BGRA compacto).
fn compacta(
    origem: &[u8],
    stride: usize,
    w: usize,
    h: usize,
    rgb_ordem: bool,
    saida: &mut Vec<u8>,
) {
    saida.clear();
    for y in 0..h {
        let ini = y * stride;
        let Some(linha) = origem.get(ini..ini + w * 4) else {
            break;
        };
        saida.extend_from_slice(linha);
    }
    if rgb_ordem {
        troca_r_b(saida);
    }
}

pub fn roda(fd: OwnedFd, no: u32, fps: u32) -> Result<()> {
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None).context("main loop")?;
    let context = pw::context::ContextRc::new(&mainloop, None).context("contexto")?;
    let core = context
        .connect_fd_rc(fd, None)
        .context("conectar no PipeWire do portal")?;

    let stream = pw::stream::StreamBox::new(
        &core,
        "lincast-tela",
        pw::properties::properties! {
            *pw::keys::MEDIA_TYPE => "Video",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Screen",
        },
    )
    .context("criar o stream")?;

    let dados = Dados {
        formato: Default::default(),
        ultimo: Instant::now() - Duration::from_secs(1),
        // 80% do período: com folga para a variação de ritmo da fonte (senão alterna quadros).
        intervalo: Duration::from_micros(800_000 / u64::from(fps.clamp(1, 120))),
        saida: Vec::new(),
        chamadas: 0,
    };
    let ml_fim = mainloop.clone();
    let _ouvinte = stream
        .add_local_listener_with_user_data(dados)
        .state_changed(|_, _, antigo, novo| {
            log::info!("[video] estado {antigo:?} -> {novo:?}");
        })
        .param_changed(|_, d, id, param| {
            let Some(param) = param else { return };
            if id != spa::param::ParamType::Format.as_raw() {
                return;
            }
            let Ok((tipo, sub)) = spa::param::format_utils::parse_format(param) else {
                return;
            };
            if tipo != spa::param::format::MediaType::Video
                || sub != spa::param::format::MediaSubtype::Raw
            {
                return;
            }
            if d.formato.parse(param).is_ok() {
                let t = d.formato.size();
                log::info!(
                    "[video] formato {:?} {}x{}",
                    d.formato.format(),
                    t.width,
                    t.height
                );
            }
        })
        .process(move |stream, d| {
            d.chamadas += 1;
            let Some(mut buffer) = stream.dequeue_buffer() else {
                if d.chamadas <= 3 {
                    log::info!("[video] process sem buffer");
                }
                return;
            };
            if d.ultimo.elapsed() < d.intervalo {
                return;
            }
            let datas = buffer.datas_mut();
            let Some(data) = datas.first_mut() else {
                return;
            };
            let stride = data.chunk().stride().max(0) as usize;
            let tam = d.formato.size();
            let (w, h) = (tam.width as usize, tam.height as usize);
            if w == 0 || h == 0 {
                return;
            }
            if d.chamadas <= 3 {
                log::info!(
                    "[video] buffer: stride {stride}, chunk {} bytes, tipo {:?}, {w}x{h}",
                    data.chunk().size(),
                    data.type_()
                );
            }
            let Some(bytes) = data.data() else {
                if d.chamadas <= 3 {
                    log::info!("[video] buffer sem mapa (DMA-BUF?)");
                }
                return;
            };
            let f = d.formato.format();
            let rgb = f == VideoFormat::RGBx || f == VideoFormat::RGBA;
            let stride = if stride == 0 { w * 4 } else { stride };
            compacta(bytes, stride, w, h, rgb, &mut d.saida);
            if d.saida.len() != w * h * 4 {
                return;
            }
            d.ultimo = Instant::now();
            let mut saida = std::io::stdout().lock();
            let cab = [w as u32, h as u32, d.saida.len() as u32];
            let ok = cab
                .iter()
                .try_for_each(|n| saida.write_all(&n.to_le_bytes()))
                .and_then(|_| saida.write_all(&d.saida))
                .and_then(|_| saida.flush());
            if ok.is_err() {
                // O app principal foi embora: encerra.
                ml_fim.quit();
            }
        })
        .register()
        .context("registrar o ouvinte")?;

    let obj = spa::pod::object!(
        spa::utils::SpaTypes::ObjectParamFormat,
        spa::param::ParamType::EnumFormat,
        spa::pod::property!(
            spa::param::format::FormatProperties::MediaType,
            Id,
            spa::param::format::MediaType::Video
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::MediaSubtype,
            Id,
            spa::param::format::MediaSubtype::Raw
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoFormat,
            Choice,
            Enum,
            Id,
            VideoFormat::BGRx,
            VideoFormat::BGRx,
            VideoFormat::BGRA,
            VideoFormat::RGBx,
            VideoFormat::RGBA,
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoSize,
            Choice,
            Range,
            Rectangle,
            spa::utils::Rectangle {
                width: 1920,
                height: 1080
            },
            spa::utils::Rectangle {
                width: 1,
                height: 1
            },
            spa::utils::Rectangle {
                width: 8192,
                height: 8192
            }
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoFramerate,
            Choice,
            Range,
            Fraction,
            spa::utils::Fraction { num: fps, denom: 1 },
            spa::utils::Fraction { num: 0, denom: 1 },
            spa::utils::Fraction { num: 240, denom: 1 }
        ),
    );
    let bytes: Vec<u8> = spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &spa::pod::Value::Object(obj),
    )
    .map_err(|e| anyhow::anyhow!("formato inválido: {e:?}"))?
    .0
    .into_inner();
    let mut params = [spa::pod::Pod::from_bytes(&bytes).context("pod")?];

    stream
        .connect(
            spa::utils::Direction::Input,
            Some(no),
            pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
            &mut params,
        )
        .context("conectar o stream de vídeo")?;

    mainloop.run();
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn compacta_remove_a_sobra_da_linha() {
        // 2x2, stride 12 (4 bytes de sobra por linha).
        let mut o = vec![];
        for y in 0..2u8 {
            for x in 0..2u8 {
                o.extend_from_slice(&[y * 10 + x, 1, 2, 255]);
            }
            o.extend_from_slice(&[9, 9, 9, 9]);
        }
        let mut s = vec![];
        compacta(&o, 12, 2, 2, false, &mut s);
        assert_eq!(s.len(), 16);
        assert_eq!(&s[8..12], &[10, 1, 2, 255]);
    }

    #[test]
    fn troca_r_e_b() {
        let mut p = [1, 2, 3, 4];
        troca_r_b(&mut p);
        assert_eq!(p, [3, 2, 1, 4]);
    }
}
