//! Processo auxiliar que captura o áudio dos aplicativos pelo PipeWire e
//! escreve no stdout blocos de 10ms (PCM 16 bits, 48kHz, estéreo,
//! intercalado, little-endian).
//!
//! Por que um processo à parte, e não uma thread: o libwebrtc pré-compilado
//! embute stubs com os MESMOS nomes das funções do PipeWire (`pw_init`,
//! `pw_stream_new`...), que ele só preenche via `dlopen` quando a captura de
//! tela começa. No link estático, o `pipewire-rs` acaba chamando esses stubs
//! vazios — segfault em `pw_init` (conferido com gdb em 2026-10-12). Este
//! binário não linka o libwebrtc, então aqui `pw_*` é a biblioteca de verdade.
//! Bônus: se a captura de áudio cair, a chamada continua de pé.

mod audio_apps;
mod video_tela;

use std::io::Write;
use std::os::fd::FromRawFd;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "concord-audio",
    about = "Captura de áudio por aplicativo (PipeWire)"
)]
struct Cli {
    #[command(subcommand)]
    comando: Comando,
}

#[derive(Subcommand)]
enum Comando {
    /// Mostra os aplicativos que estão tocando som agora.
    Listar {
        /// Uma linha JSON por app (é o que a interface do cliente lê).
        #[arg(long)]
        json: bool,
    },
    /// Captura e escreve PCM no stdout até o stdout fechar.
    Capturar {
        #[arg(long = "app")]
        apps: Vec<String>,
        #[arg(long, conflicts_with = "apps")]
        todos: bool,
        /// Pid a nunca capturar, além deste processo (o do app principal).
        #[arg(long)]
        excluir_pid: Option<u32>,
    },
    /// Vídeo da tela: lê o PipeWire do portal (descritor na entrada padrão) e
    /// escreve quadros BGRA na saída padrão (ver `video_tela`).
    Video {
        /// Id do nó PipeWire devolvido pelo portal.
        #[arg(long)]
        no: u32,
        #[arg(long, default_value_t = 30)]
        fps: u32,
    },
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .target(env_logger::Target::Stderr)
        .init();
    match Cli::parse().comando {
        Comando::Listar { json } => {
            let apps = audio_apps::lista()?;
            if json {
                for a in apps {
                    println!("{}", a.json());
                }
                return Ok(());
            }
            if apps.is_empty() {
                println!(
                    "Nenhum aplicativo tocando som agora. Dê play em alguma coisa e rode de novo."
                );
            }
            for a in apps {
                println!("- {a}");
            }
            Ok(())
        }
        Comando::Video { no, fps } => {
            // O descritor do portal chega como a entrada padrão (fd 0).
            // SAFETY: o app principal garante que o fd 0 é o socket do PipeWire
            // e este processo é o único dono dele.
            let fd = unsafe { std::os::fd::OwnedFd::from_raw_fd(0) };
            video_tela::roda(fd, no, fps)
        }
        Comando::Capturar {
            apps,
            todos,
            excluir_pid,
        } => {
            let filtro = if todos {
                audio_apps::Filtro::TodosMenosEu
            } else {
                audio_apps::Filtro::Apps(apps)
            };
            captura(filtro, excluir_pid)
        }
    }
}

fn captura(filtro: audio_apps::Filtro, excluir_pid: Option<u32>) -> Result<()> {
    let mixer = Arc::new(audio_apps::Mixer::default());
    audio_apps::inicia(filtro, excluir_pid, mixer.clone());

    let mut bloco = [0i16; audio_apps::AMOSTRAS_10MS];
    let mut bytes = [0u8; audio_apps::AMOSTRAS_10MS * 2];
    let mut saida = std::io::stdout().lock();
    let inicio = Instant::now();
    let mut n: u64 = 0;
    let mut pico_max = 0f32;
    loop {
        // Ritmo pelo relógio absoluto, não por "dorme 10ms": somar sleeps
        // acumula atraso, e o outro lado contaria com exatamente 100 blocos/s.
        n += 1;
        let prazo = inicio + Duration::from_millis(10 * n);
        if let Some(espera) = prazo.checked_duration_since(Instant::now()) {
            std::thread::sleep(espera);
        }
        pico_max = pico_max.max(mixer.puxa_10ms(&mut bloco));
        for (b, s) in bytes.as_chunks_mut::<2>().0.iter_mut().zip(bloco) {
            *b = s.to_le_bytes();
        }
        if saida.write_all(&bytes).and_then(|_| saida.flush()).is_err() {
            // O app principal fechou: acabou.
            return Ok(());
        }
        if n.is_multiple_of(500) {
            log::info!("[audio] pico nos últimos 5s: {:.0}%", pico_max * 100.0);
            pico_max = 0.0;
        }
    }
}
