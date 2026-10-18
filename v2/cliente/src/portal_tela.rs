//! Sessão própria do portal ScreenCast (Wayland).
//!
//! Em vez de deixar o libwebrtc abrir o seletor do sistema a cada transmissão,
//! o app abre a sessão do portal, pede que a escolha seja lembrada
//! (`PersistMode::ExplicitlyRevoked` + `restore_token`) e entrega o PipeWire
//! do portal ao `concord-audio video`, que devolve os quadros em BGRA. Na
//! primeira vez o seletor do sistema aparece; nas seguintes, a escolha vem
//! pronta. Qualquer falha aqui faz o chamador voltar ao capturador do libwebrtc.

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};

use anyhow::{Context, Result, bail};
use ashpd::desktop::PersistMode;
use ashpd::desktop::screencast::{
    CursorMode, Screencast, SelectSourcesOptions, SourceType, StartCastOptions,
};
use ashpd::enumflags2::BitFlags;

/// A pessoa cancelou o seletor do sistema: não adianta tentar outro caminho.
#[derive(Debug)]
pub struct Cancelado;

impl std::fmt::Display for Cancelado {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("seleção cancelada")
    }
}

impl std::error::Error for Cancelado {}

fn erro_da_resposta(e: ashpd::Error, contexto: &'static str) -> anyhow::Error {
    if matches!(
        e,
        ashpd::Error::Response(ashpd::desktop::ResponseError::Cancelled)
    ) {
        anyhow::Error::new(Cancelado)
    } else {
        anyhow::Error::new(e).context(contexto)
    }
}

/// Um quadro BGRA compacto (sem sobra de linha).
pub struct Quadro {
    pub largura: u32,
    pub altura: u32,
    pub bgra: Vec<u8>,
}

pub struct Portal {
    filho: Child,
    rt: tokio::runtime::Runtime,
    // `Option` para poder soltar a sessão dentro do runtime.
    sessao: Option<ashpd::desktop::Session<Screencast>>,
    pub quadros: Receiver<Quadro>,
}

fn arquivo_do_token() -> Option<std::path::PathBuf> {
    let d = directories::ProjectDirs::from("", "", "lincast-chat")?;
    Some(d.config_dir().join("tela-token"))
}

fn le_token() -> Option<String> {
    let t = std::fs::read_to_string(arquivo_do_token()?).ok()?;
    let t = t.trim().to_string();
    (!t.is_empty()).then_some(t)
}

fn guarda_token(t: &str) {
    if let Some(p) = arquivo_do_token() {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(p, t);
    }
}

/// Há uma tela/janela lembrada de uma transmissão anterior?
pub fn tem_escolha_lembrada() -> bool {
    le_token().is_some()
}

/// Esquece a escolha lembrada (a próxima transmissão mostra o seletor de novo).
pub fn esquece_escolha() {
    if let Some(p) = arquivo_do_token() {
        let _ = std::fs::remove_file(p);
    }
}

/// Acha o `concord-audio`: ao lado do executável e, se não estiver lá (app
/// aberto por script/wrapper que o move de lugar), nas pastas do `PATH`.
pub fn acha_concord_audio() -> Option<std::path::PathBuf> {
    let ao_lado = std::env::current_exe()
        .ok()?
        .with_file_name("concord-audio");
    if ao_lado.is_file() {
        return Some(ao_lado);
    }
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|p| p.join("concord-audio"))
        .find(|p| p.is_file())
}

/// Abre a sessão (mostra o seletor se não houver escolha lembrada), liga o
/// processo auxiliar e devolve o canal de quadros. Bloqueia até o portal
/// responder: chame de uma thread própria.
pub fn abre(cursor: bool, fps: u32) -> Result<Portal> {
    let Some(bin) = acha_concord_audio() else {
        bail!("não achei o concord-audio ao lado do app");
    };
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .context("runtime do portal")?;

    let (sessao, no, fd) = rt.block_on(async {
        let proxy = Screencast::new().await.context("portal ScreenCast")?;
        let sessao = proxy
            .create_session(Default::default())
            .await
            .context("criar sessão")?;
        let token = le_token();
        let tipos: BitFlags<SourceType> = SourceType::Monitor | SourceType::Window;
        proxy
            .select_sources(
                &sessao,
                SelectSourcesOptions::default()
                    .set_sources(tipos)
                    .set_multiple(false)
                    .set_cursor_mode(if cursor {
                        CursorMode::Embedded
                    } else {
                        CursorMode::Hidden
                    })
                    .set_persist_mode(PersistMode::ExplicitlyRevoked)
                    .set_restore_token(token.as_deref()),
            )
            .await
            .context("escolher fontes")?
            .response()
            .map_err(|e| erro_da_resposta(e, "seleção recusada"))?;
        let resposta = proxy
            .start(&sessao, None, StartCastOptions::default())
            .await
            .context("iniciar")?
            .response()
            .map_err(|e| erro_da_resposta(e, "o portal recusou"))?;
        if let Some(t) = resposta.restore_token() {
            guarda_token(t);
        }
        let no = resposta
            .streams()
            .first()
            .map(|s| s.pipe_wire_node_id())
            .context("o portal não devolveu nenhuma tela")?;
        let fd = proxy
            .open_pipe_wire_remote(&sessao, Default::default())
            .await
            .context("abrir o PipeWire do portal")?;
        Ok::<_, anyhow::Error>((sessao, no, fd))
    })?;
    log::info!("[portal] nó PipeWire {no}");

    let mut filho = Command::new(bin)
        .args(["video", "--no", &no.to_string(), "--fps", &fps.to_string()])
        .stdin(Stdio::from(fd))
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .context("rodar o concord-audio video")?;
    let mut saida = filho.stdout.take().context("saída do concord-audio")?;

    // Leitor: quadros inteiros; se o consumo atrasar, descarta (não acumula).
    let (tx, rx) = mpsc::sync_channel::<Quadro>(2);
    std::thread::Builder::new()
        .name("portal-leitor".into())
        .spawn(move || {
            loop {
                let mut cab = [0u8; 12];
                if saida.read_exact(&mut cab).is_err() {
                    break;
                }
                let n = |i: usize| u32::from_le_bytes([cab[i], cab[i + 1], cab[i + 2], cab[i + 3]]);
                let (largura, altura, tam) = (n(0), n(4), n(8) as usize);
                if tam == 0 || tam > 8192 * 8192 * 4 {
                    break;
                }
                let mut bgra = vec![0u8; tam];
                if saida.read_exact(&mut bgra).is_err() {
                    break;
                }
                let _ = tx.try_send(Quadro {
                    largura,
                    altura,
                    bgra,
                });
            }
        })
        .context("thread leitora")?;

    Ok(Portal {
        filho,
        rt,
        sessao: Some(sessao),
        quadros: rx,
    })
}

impl Portal {
    /// O processo auxiliar já terminou?
    pub fn morreu(&mut self) -> bool {
        matches!(self.filho.try_wait(), Ok(Some(_)))
    }

    /// Encerra o auxiliar e fecha a sessão do portal.
    pub fn fecha(mut self) {
        let _ = self.filho.kill();
        let _ = self.filho.wait();
        {
            let _g = self.rt.enter();
            drop(self.sessao.take());
        }
        self.rt.shutdown_background();
    }
}
