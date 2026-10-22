//! Protótipo da v2: tela + áudio de aplicativo no Wayland, mandados por WebRTC
//! pra uma sala do servidor atual.
//!
//! Não é o app. Existe pra responder uma pergunta só antes de a reescrita
//! começar: as peças em Rust (portal do Wayland pelo libwebrtc, áudio por
//! aplicativo pelo PipeWire, WebRTC nativo) funcionam juntas de verdade?
//! Quem assiste usa o app atual, no navegador, na mesma sala.

mod chamada;
mod tela;

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use libwebrtc::audio_frame::AudioFrame;
use libwebrtc::audio_source::AudioSourceOptions;
use libwebrtc::audio_source::native::NativeAudioSource;
use tokio::io::AsyncReadExt;

// Formato do que o `concord-audio` escreve no stdout.
const TAXA: u32 = 48_000;
const CANAIS: u32 = 2;
const AMOSTRAS_10MS: usize = (TAXA / 100 * CANAIS) as usize;

#[derive(Parser)]
#[command(
    name = "concord-proto",
    about = "Protótipo de transmissão de tela com áudio (v2)"
)]
struct Cli {
    #[command(subcommand)]
    comando: Comando,
}

#[derive(Subcommand)]
enum Comando {
    /// Mostra os aplicativos que estão tocando som agora.
    Listar,
    /// Entra numa sala e transmite tela + áudio dos apps escolhidos.
    Transmitir {
        /// Endereço do servidor (o mesmo da página).
        #[arg(long, default_value = "https://meuteamspeak6.duckdns.org:8443")]
        servidor: String,
        /// Nome da sala.
        #[arg(long)]
        sala: String,
        /// Senha da sala, se ela tiver.
        #[arg(long, default_value = "")]
        senha: String,
        /// Aplicativo cujo som vai junto (parte do nome ou do binário, ver
        /// `listar`). Pode repetir.
        #[arg(long = "app")]
        apps: Vec<String>,
        /// Manda o som de TODOS os aplicativos, menos este programa.
        #[arg(long, conflicts_with = "apps")]
        todos: bool,
        /// Só áudio, sem tela (pra testar o PipeWire isolado).
        #[arg(long)]
        sem_tela: bool,
        /// Quadros por segundo da tela.
        #[arg(long, default_value_t = 30)]
        fps: u32,
        /// Inclui o cursor na imagem.
        #[arg(long)]
        cursor: bool,
        /// Sessão X11: captura a tela inteira direto, sem o portal do Wayland.
        #[arg(long)]
        x11: bool,
    },
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();
    // O tokio-tungstenite usa rustls, que exige escolher o provedor de
    // criptografia uma vez no processo.
    let _ = rustls::crypto::ring::default_provider().install_default();

    match Cli::parse().comando {
        Comando::Listar => {
            let status = std::process::Command::new(binario_de_audio()?)
                .arg("listar")
                .status()?;
            if !status.success() {
                bail!("o concord-audio falhou ({status})");
            }
            Ok(())
        }
        Comando::Transmitir {
            servidor,
            sala,
            senha,
            apps,
            todos,
            sem_tela,
            fps,
            cursor,
            x11,
        } => {
            // Argumentos repassados ao `concord-audio`; `None` = sem áudio.
            let filtro: Option<Vec<String>> = if todos {
                Some(vec!["--todos".into()])
            } else if apps.is_empty() {
                None
            } else {
                Some(
                    apps.iter()
                        .flat_map(|a| ["--app".to_string(), a.clone()])
                        .collect(),
                )
            };
            if sem_tela && filtro.is_none() {
                bail!("sem tela e sem áudio não sobra nada pra transmitir: use --app ou --todos");
            }
            transmite(
                chamada::Config {
                    servidor,
                    sala,
                    senha,
                },
                filtro,
                sem_tela,
                fps,
                cursor,
                x11,
            )
        }
    }
}

fn transmite(
    cfg: chamada::Config,
    filtro: Option<Vec<String>>,
    sem_tela: bool,
    fps: u32,
    cursor: bool,
    x11: bool,
) -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    // O `NativeVideoSource` sobe uma tarefa interna no tokio ao nascer, então
    // a captura tem que começar já dentro do runtime — fora dele é panic.
    let _dentro = rt.enter();
    let captura = if sem_tela {
        None
    } else {
        if !x11 {
            println!("Escolha a tela ou janela no diálogo do sistema...");
        }
        match tela::inicia(fps, cursor, x11) {
            Some(c) => {
                println!("Capturando {}x{}.", c.resolucao.width, c.resolucao.height);
                Some(c)
            }
            None => bail!("a captura de tela não começou (cancelado, ou sem xdg-desktop-portal)"),
        }
    };

    rt.block_on(async move {
        let audio = match filtro {
            None => None,
            Some(args) => {
                // Áudio de tela: os três processamentos de voz DESLIGADOS.
                // Eles existem pra microfone e destroem jogo/música (o
                // supressor de ruído trata som contínuo como ruído). Foi o
                // bug 4 de 2026-08-30.
                let fonte = NativeAudioSource::new(
                    AudioSourceOptions {
                        echo_cancellation: false,
                        noise_suppression: false,
                        auto_gain_control: false,
                    },
                    TAXA,
                    CANAIS,
                    0,
                );
                let mut filho = tokio::process::Command::new(binario_de_audio()?)
                    .arg("capturar")
                    .args(&args)
                    .args(["--excluir-pid", &std::process::id().to_string()])
                    .stdout(Stdio::piped())
                    .kill_on_drop(true)
                    .spawn()?;
                let stdout = filho.stdout.take().expect("stdout do concord-audio");
                tokio::spawn(bombeia_audio(filho, stdout, fonte.clone()));
                Some(fonte)
            }
        };
        if let Some(c) = &captura {
            let quadros = c.quadros.clone();
            tokio::spawn(async move {
                let mut antes = 0;
                let mut t = tokio::time::interval(Duration::from_secs(5));
                t.tick().await;
                loop {
                    t.tick().await;
                    let agora = *quadros.lock().unwrap();
                    log::info!("[tela] {:.1} quadros/s", (agora - antes) as f64 / 5.0);
                    antes = agora;
                }
            });
        }
        let midia = chamada::Midia {
            video: captura.as_ref().map(|c| c.fonte.clone()),
            audio,
        };
        let r = chamada::roda(cfg, midia).await;
        if let Some(c) = &captura {
            c.para();
        }
        r
    })
}

/// O `concord-audio` mora ao lado deste executável.
fn binario_de_audio() -> Result<PathBuf> {
    let eu = std::env::current_exe()?;
    let ao_lado = eu.with_file_name("concord-audio");
    if !ao_lado.exists() {
        bail!(
            "não achei {} (ele é compilado junto: cargo build)",
            ao_lado.display()
        );
    }
    Ok(ao_lado)
}

/// Lê blocos de 10ms do `concord-audio` e entrega ao libwebrtc. O ritmo é
/// dado pelo outro lado, que escreve 100 blocos por segundo.
async fn bombeia_audio(
    _filho: tokio::process::Child,
    mut stdout: tokio::process::ChildStdout,
    fonte: NativeAudioSource,
) {
    let mut bytes = vec![0u8; AMOSTRAS_10MS * 2];
    let mut quadro = AudioFrame::new(TAXA, CANAIS, TAXA / 100);
    loop {
        if stdout.read_exact(&mut bytes).await.is_err() {
            log::error!("[audio] o concord-audio parou; a transmissão segue sem som");
            return;
        }
        for (s, b) in quadro
            .data
            .to_mut()
            .iter_mut()
            .zip(bytes.as_chunks::<2>().0)
        {
            *s = i16::from_le_bytes(*b);
        }
        if let Err(e) = fonte.capture_frame(&quadro).await {
            log::warn!("[audio] o libwebrtc recusou o bloco: {e:?}");
        }
    }
}
