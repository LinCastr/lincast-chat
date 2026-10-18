//! O núcleo de uma sala: WebSocket com o servidor, quem está lá, chat, e uma
//! conexão WebRTC com cada pessoa que está na chamada.
//!
//! Fala exatamente o protocolo da v1 (`public/index.html`, `handleSignal`),
//! então o app novo e o site convivem na mesma sala. As regras que importam
//! vieram de lá e foram validadas no protótipo (`v2/prototipo-tela`):
//!
//! - Entra-se na sala como espectador; a chamada é outro passo.
//! - Só existe conexão entre duas pessoas quando as DUAS estão na chamada.
//! - Quem chega oferece (`room-info`); quem já estava espera (`peer-joined`);
//!   no `mode-state`, quem oferece sai da comparação de ids.
//! - Negociação perfeita com rollback; candidato ICE adiantado vai pra fila.
//! - A tela é anunciada (`screen-state`) antes de qualquer SDP.
//! - `removeTrack` do outro lado não dispara `ended`: caixa de vídeo fecha
//!   pelo aviso (`screen-state`/`camera-state`/`mode-state`/`peer-left`).
//!
//! Tudo roda num ator só: cada mensagem e cada evento das conexões é tratado
//! até o fim antes do próximo, o que serializa a negociação de graça.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine;
use futures_util::StreamExt;
use libwebrtc::audio_frame::AudioFrame;
use libwebrtc::audio_source::AudioSourceOptions;
use libwebrtc::audio_source::native::NativeAudioSource;
use libwebrtc::ice_candidate::IceCandidate;
use libwebrtc::media_stream_track::MediaStreamTrack;
use libwebrtc::peer_connection::{OfferOptions, PeerConnection, PeerConnectionState, TrackEvent};
use libwebrtc::peer_connection_factory::native::PeerConnectionFactoryExt;
use libwebrtc::peer_connection_factory::{IceServer, PeerConnectionFactory, RtcConfiguration};
use libwebrtc::prelude::{
    AnswerOptions, RtpSender, SdpType, SessionDescription, SignalingState, VideoFormatType,
};
use libwebrtc::video_stream::native::NativeVideoStream;
use libwebrtc::{audio_track::RtcAudioTrack, video_track::RtcVideoTrack};
use serde_json::{Value, json};
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

use crate::api::Servidor;
use crate::captura_camera;
use crate::captura_tela;
use crate::volumes;

// --- o que a interface vê --------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NivelConexao {
    #[default]
    Indefinido,
    Boa,
    Media,
    Ruim,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct QualidadeConexao {
    pub nivel: NivelConexao,
    pub perda: Option<f64>,
    pub rtt_ms: Option<u64>,
    pub jitter_ms: Option<u64>,
    pub bitrate_kbps: Option<u64>,
    pub rota: Option<String>,
}

impl QualidadeConexao {
    #[allow(dead_code)]
    pub fn texto(&self) -> String {
        if self.nivel == NivelConexao::Indefinido {
            return "Medindo conexão...".into();
        }
        let mut partes = Vec::new();
        if let Some(p) = self.perda {
            partes.push(format!("perda {:.1}%", p * 100.0));
        }
        if let Some(r) = self.rtt_ms {
            partes.push(format!("ida e volta {}ms", r));
        }
        let rotulo = match self.nivel {
            NivelConexao::Boa => "Conexão boa",
            NivelConexao::Media => "Conexão instável",
            NivelConexao::Ruim => "Conexão ruim",
            NivelConexao::Indefinido => "Medindo conexão...",
        };
        if partes.is_empty() {
            rotulo.into()
        } else {
            format!("{rotulo} ({})", partes.join(", "))
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Pessoa {
    pub id: String,
    pub uid: Option<String>,
    pub nome: String,
    pub eu: bool,
    pub espectador: bool,
    pub mudo: bool,
    pub surdo: bool,
    pub apresentando: bool,
    pub com_camera: bool,
    pub conexao: Option<PeerConnectionState>,
    pub volume: f32,
    pub volume_tela: f32,
    pub volume_anterior: Option<f32>,
    pub volume_tela_anterior: Option<f32>,
    pub falando: bool,
    pub mudo_forcado: bool,
    pub dono: bool,
    pub qualidade: Option<QualidadeConexao>,
    pub avatar: Option<String>,
    pub cor: Option<String>,
    pub cor_nome: Option<String>,
    pub efeito_nome: Option<String>,
    pub bio: Option<String>,
    pub status: Option<String>,
    pub arroba: Option<String>,
    pub atividade: Option<crate::api::Atividade>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RespostaA {
    pub id: String,
    pub autor: String,
    pub texto: String,
}

#[derive(Debug, Clone)]
pub struct Mensagem {
    pub id: String,
    pub autor: String,
    pub uid: Option<String>,
    pub texto: String,
    pub ts: i64,
    pub editada_em: Option<i64>,
    /// Bytes da imagem (PNG/JPEG/GIF/WebP), já decodificados do base64.
    pub imagem: Option<Vec<u8>>,
    pub gif: Option<String>,
    pub resposta_a: Option<RespostaA>,
    pub reacoes: std::collections::HashMap<String, Vec<String>>,
    pub mencoes: Vec<String>,
    pub mencoes_uids: Vec<String>,
    pub sussurro: bool,
    pub sussurro_para: Vec<String>,
    pub enquete: Option<Enquete>,
    pub votos: std::collections::HashMap<String, usize>,
    /// Mensagem de sistema (`chamada`, `perdida`, `grupo`): sem autor, sem
    /// ações. O texto vem pronto do servidor.
    pub sistema: Option<String>,
    /// Dados extras: de quem foi encaminhada, ou detalhes da chamada.
    pub extra: MensagemExtra,
}

/// Campos opcionais de uma mensagem que só alguns tipos têm.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MensagemExtra {
    /// Mensagem encaminhada: nome de quem escreveu o original.
    pub encaminhada_de: Option<String>,
    /// Mensagem de sistema `chamada`: quem ligou, quanto durou, quem perdeu.
    pub quem: Option<String>,
    pub duracao: Option<String>,
    pub perdeu: Vec<String>,
    /// Mensagem de "app" (hoje só `wordle`) e o número do jogo do dia.
    pub app: Option<String>,
    pub wordle_numero: Option<i64>,
    pub wordle_dia: Option<String>,
    /// Arquivo anexado (enviado por `/api/arquivo`, baixado por demanda).
    pub arquivo: Option<ArquivoMsg>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArquivoMsg {
    pub id: String,
    pub nome: String,
    pub tam: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct Enquete {
    pub pergunta: String,
    pub opcoes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MensagemFixada {
    pub id: String,
    pub autor: String,
    pub texto: String,
    pub fixado_por: String,
    pub ts: i64,
}

#[derive(Debug, Clone)]
pub struct Quadro {
    pub dono: String,
    pub de_tela: bool,
    pub largura: u32,
    pub altura: u32,
    /// RGBA, 4 bytes por pixel.
    pub rgba: Vec<u8>,
}

#[derive(Debug, Clone)]
pub enum Evento {
    Pronta(mpsc::UnboundedSender<Comando>),
    Entrou {
        meu_id: String,
    },
    Pessoas(Vec<Pessoa>),
    Historico(Vec<Mensagem>),
    Chat(Box<Mensagem>),
    MensagemEditada {
        id: String,
        texto: String,
        editada_em: i64,
        mencoes: Vec<String>,
        mencoes_uids: Vec<String>,
    },
    MensagemApagada(String),
    MensagemFixada(Option<MensagemFixada>),
    ReacoesAtualizadas {
        id: String,
        reacoes: std::collections::HashMap<String, Vec<String>>,
    },
    Quadro(Quadro),
    VideoFechou {
        dono: String,
        de_tela: bool,
    },
    QuadroPrevia(Quadro),
    MeuEstado {
        na_chamada: bool,
        mudo: bool,
        surdo: bool,
        apresentando: bool,
        com_camera: bool,
        mudo_forcado: bool,
    },
    Aviso(String),
    Recusada(String),
    Caiu(String),
    PttTransmitindo(bool),
    DispositivosAudio {
        entradas: Vec<crate::dispositivos::DispositivoAudio>,
        saidas: Vec<crate::dispositivos::DispositivoAudio>,
    },
    NivelMic(f32),
    Falando {
        id: String,
        falando: bool,
    },
    FalandoNoMudo(bool),
    AvisoDeEco,
    QualidadeConexao {
        id: String,
        qualidade: QualidadeConexao,
    },
    Digitando {
        de: String,
        nome: String,
        digitando: bool,
    },
    Voto {
        id: String,
        votos: std::collections::HashMap<String, usize>,
    },
    InfoSala {
        sou_dono: bool,
    },
    ReacaoFlutuante {
        de: String,
        emoji: String,
    },
    /// Alguém do grupo jogou o Wordle dessa rodada: recarregar a grade.
    WordleGrupoMudou(String),
    CutucadaRecebida {
        de_nome: String,
    },
    CutucadaEnviada {
        para_nome: String,
    },
    SomRecebido {
        de_id: String,
        de_nome: String,
        id: String,
        /// Volume (0–100) escolhido por quem soltou o som.
        vol: u8,
    },
    QuadroEstado(Vec<crate::ui::quadro::Traco>),
    QuadroTraco(crate::ui::quadro::Traco),
    QuadroLimpo {
        quem: String,
    },
    Roletas(Vec<crate::ui::roleta::Roleta>),
    RoletaResultado {
        id: String,
        indice: usize,
        por: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct ImagemAnexo {
    pub mime: String,
    pub data: String,
}

#[derive(Debug, Clone)]
pub enum Comando {
    Chat {
        texto: String,
        imagem: Option<ImagemAnexo>,
        gif: Option<String>,
        resposta_a: Option<RespostaA>,
        para: Option<Vec<String>>,
        enquete: Option<Enquete>,
    },
    /// Mensagem com um arquivo já enviado por `/api/arquivo`.
    ChatArquivo {
        texto: String,
        arquivo_id: String,
    },
    VotarEnquete {
        id: String,
        opcao: usize,
    },
    EditaChat {
        id: String,
        texto: String,
    },
    ApagaChat {
        id: String,
    },
    ReagirChat {
        id: String,
        emoji: String,
    },
    FixarChat(Option<String>),
    Digitando(bool),
    EntrarNaChamada,
    SairDaChamada,
    Mudo(bool),
    Surdo(bool),
    CompartilharTela {
        apps: Vec<String>,
        perfil: captura_tela::PerfilTela,
        auto_bitrate: bool,
    },
    PararTela,
    PreviaCamera(Option<usize>),
    CancelaPreviaCamera,
    IniciarCamera {
        indice: usize,
    },
    PararCamera,
    AjustaVolume {
        id: String,
        volume: f32,
        de_tela: bool,
    },
    AlternaMute {
        id: String,
        de_tela: bool,
    },
    PttConfigurado {
        ativo: bool,
    },
    PttTransmitindo(bool),
    AtualizaDispositivosAudio,
    SelecionaDispositivoGravacao(String),
    SelecionaDispositivoReproducao(String),
    IniciaTesteMic,
    ParaTesteMic,
    AtualizaPerfilSala {
        cor: Option<String>,
        cor_nome: Option<String>,
        efeito_nome: Option<String>,
    },
    TrocarStatus(String),
    TrocarAtividade(Option<crate::api::Atividade>),
    EnviarReacao(String),
    Cutucar(String),
    /// Id do som e volume (0–100) do emissor.
    EnviarSom(String, u8),
    EnviarTraco(crate::ui::quadro::Traco),
    LimparQuadro,
    CriarRoleta {
        nome: String,
        opcoes: Vec<String>,
    },
    EditarRoleta {
        id: String,
        nome: String,
        opcoes: Vec<String>,
    },
    ApagarRoleta(String),
    GirarRoleta(String),
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Entrada {
    pub servidor: Servidor,
    pub token: String,
    pub sala: String,
    /// Nome de exibição da conta (o servidor não o repete no `room-info`).
    pub nome: String,
}

// --- escada de bitrate de tela ---------------------------------------------

pub const DEGRAUS_DE_BITRATE: [f64; 4] = [1.0, 0.5, 0.25, 0.12];
pub const BITRATE_MINIMO: u64 = 200_000;
pub const AMOSTRAS_BOAS_PRA_SUBIR: u32 = 4;

#[derive(Debug, Clone, Copy, Default)]
pub struct AmostraEnvio {
    pub enviados: u64,
    pub perdidos: u64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AmostraInbound {
    pub perdidos: u64,
    pub recebidos: u64,
}

#[derive(Debug, Clone, Default)]
pub struct EscadaBitrate {
    pub degrau: usize,
    pub amostras_boas: u32,
}

impl EscadaBitrate {
    pub fn reinicia(&mut self) {
        self.degrau = 0;
        self.amostras_boas = 0;
    }

    pub fn bitrate_atual(&self, teto: u64) -> u64 {
        let frac = DEGRAUS_DE_BITRATE.get(self.degrau).copied().unwrap_or(0.12);
        ((teto as f64 * frac).round() as u64).max(BITRATE_MINIMO)
    }

    pub fn atualiza(&mut self, nivel: &str) -> bool {
        let antes = self.degrau;
        match nivel {
            "ruim" => {
                self.degrau = (self.degrau + 1).min(DEGRAUS_DE_BITRATE.len() - 1);
                self.amostras_boas = 0;
            }
            "boa" => {
                self.amostras_boas += 1;
                if self.amostras_boas >= AMOSTRAS_BOAS_PRA_SUBIR && self.degrau > 0 {
                    self.degrau -= 1;
                    self.amostras_boas = 0;
                }
            }
            _ => {
                self.amostras_boas = 0;
            }
        }
        self.degrau != antes
    }
}

pub fn classifica_conexao(perda: Option<f64>, rtt_ms: Option<u64>) -> &'static str {
    if perda.is_none() && rtt_ms.is_none() {
        return "indefinido";
    }
    let perda_ruim = perda.map(|p| p > 0.08).unwrap_or(false);
    let perda_media = perda.map(|p| p > 0.02).unwrap_or(false);
    let rtt_ruim = rtt_ms.map(|r| r > 350).unwrap_or(false);
    let rtt_medio = rtt_ms.map(|r| r > 150).unwrap_or(false);

    if perda_ruim || rtt_ruim {
        "ruim"
    } else if perda_media || rtt_medio {
        "media"
    } else {
        "boa"
    }
}

pub fn configura_remetente_tela(
    sender: &RtpSender,
    perfil: captura_tela::PerfilTela,
    degrau: usize,
) {
    let mut params = sender.parameters();
    params.set_degradation_preference(libwebrtc::rtp_parameters::DegradationPreference::Balanced);
    let bitrate = EscadaBitrate {
        degrau,
        amostras_boas: 0,
    }
    .bitrate_atual(perfil.teto_bitrate());
    let fps = perfil.fps() as f64;
    if params.encodings.is_empty() {
        params
            .encodings
            .push(libwebrtc::rtp_parameters::RtpEncodingParameters::default());
    }
    let reducao = perfil.fator_de_reducao();
    for enc in &mut params.encodings {
        enc.max_bitrate = Some(bitrate);
        enc.max_framerate = Some(fps);
        enc.scale_resolution_down_by = (reducao > 1.0).then_some(reducao);
    }
    if let Err(e) = sender.set_parameters(params) {
        log::warn!("[tela] erro ao aplicar parâmetros no RtpSender: {e:?}");
    }
}

// --- ator ------------------------------------------------------------------

enum Interno {
    Servidor(Value),
    Caiu(String),
    Ui(Comando),
    Negociar(String),
    Candidato(String, IceCandidate),
    Estado(String, PeerConnectionState),
    Trilha(String, TrackEvent),
    NivelMic(f32),
    Falando(String, bool),
    NivelMicLocal(f32),
    NivelRemoto(String, f32),
    CicloEco,
    CicloQualidade,
}

struct Par {
    pc: PeerConnection,
    educado: bool,
    renegociar: bool,
    pendentes: Vec<IceCandidate>,
    remetentes_tela: Vec<RtpSender>,
    remetentes_camera: Vec<RtpSender>,
    trilhas_voz: Vec<RtcAudioTrack>,
    trilhas_tela: Vec<RtcAudioTrack>,
    monitores_audio: Vec<tokio::task::JoinHandle<()>>,
    escada_bitrate: EscadaBitrate,
    ultimo_envio: Option<AmostraEnvio>,
    ultimo_inbound: Option<AmostraInbound>,
}

impl Drop for Par {
    fn drop(&mut self) {
        for h in &self.monitores_audio {
            h.abort();
        }
    }
}

struct Tela {
    captura: captura_tela::Captura,
    video: RtcVideoTrack,
    audio: Option<RtcAudioTrack>,
    perfil: captura_tela::PerfilTela,
    auto_bitrate: bool,
    _filho: Option<tokio::process::Child>,
}

struct Camera {
    captura: captura_camera::CapturaCamera,
    video: RtcVideoTrack,
}

struct Previa {
    parar: std::sync::mpsc::Sender<()>,
}

struct Ator {
    saida: iced::futures::channel::mpsc::Sender<Evento>,
    ws: mpsc::UnboundedSender<Value>,
    tx: mpsc::UnboundedSender<Interno>,
    fabrica: PeerConnectionFactory,
    rtc: RtcConfiguration,
    meu_id: String,
    meu_uid: Option<String>,
    meu_nome: String,
    pessoas: BTreeMap<String, Pessoa>,
    pares: HashMap<String, Par>,
    telas_anunciadas: HashSet<String>,
    na_chamada: bool,
    mudo: bool,
    surdo: bool,
    mic: Option<RtcAudioTrack>,
    id_voz: String,
    id_tela: String,
    id_camera: String,
    tela: Option<Tela>,
    camera: Option<Camera>,
    previa: Option<Previa>,
    ptt_ativo: bool,
    ptt_segurando: bool,
    teste_mic: Option<(tokio::task::JoinHandle<()>, Option<RtcAudioTrack>)>,
    monitor_mic_local: Option<tokio::task::JoinHandle<()>>,
    meu_falando: bool,
    falando_no_mudo: bool,
    inicio_fala_mudo: Option<tokio::time::Instant>,
    ultimo_som_mudo: Option<tokio::time::Instant>,
    detector_eco: crate::eco::DetectorEco,
    mudo_forcado: bool,
    sou_dono: bool,
}

fn aleatorio() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{:x}", (n as u64) ^ ((std::process::id() as u64) << 32))
}

/// Roda a sala até o servidor cair ou a interface largar o canal. É o corpo
/// da `Subscription` da tela de sala.
pub async fn roda(entrada: Entrada, mut saida: iced::futures::channel::mpsc::Sender<Evento>) {
    use iced::futures::SinkExt as _;
    if let Err(e) = roda_interno(entrada, saida.clone()).await {
        let _ = saida.send(Evento::Caiu(format!("{e:#}"))).await;
    }
}

async fn conecta_ws(
    entrada: &Entrada,
    tx_interno: mpsc::UnboundedSender<Interno>,
) -> Result<(mpsc::UnboundedSender<Value>, Tarefas)> {
    use iced::futures::SinkExt as _;

    let mut pedido = entrada.servidor.url_ws().into_client_request()?;
    pedido.headers_mut().insert(
        "Cookie",
        format!("session={}", entrada.token)
            .parse()
            .context("token inválido")?,
    );
    let (ws, _) = tokio_tungstenite::connect_async(pedido)
        .await
        .context("não consegui conectar na sala")?;
    let (mut ws_tx, mut ws_rx) = ws.split();

    let (tx_ws, mut rx_ws) = mpsc::unbounded_channel::<Value>();
    let mut tarefas = Tarefas(Vec::new());

    tarefas.0.push(tokio::spawn(async move {
        while let Some(v) = rx_ws.recv().await {
            if ws_tx
                .send(Message::Text(v.to_string().into()))
                .await
                .is_err()
            {
                break;
            }
        }
        let _ = ws_tx.close().await;
    }));

    {
        let tx = tx_interno.clone();
        tarefas.0.push(tokio::spawn(async move {
            let motivo = loop {
                match ws_rx.next().await {
                    Some(Ok(Message::Text(t))) => {
                        if let Ok(v) = serde_json::from_str::<Value>(&t) {
                            let _ = tx.send(Interno::Servidor(v));
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        break "o servidor fechou a conexão".to_string();
                    }
                    Some(Err(e)) => break format!("conexão perdida: {e}"),
                    _ => {}
                }
            };
            let _ = tx.send(Interno::Caiu(motivo));
        }));
    }

    Ok((tx_ws, tarefas))
}

async fn roda_interno(
    entrada: Entrada,
    mut saida: iced::futures::channel::mpsc::Sender<Evento>,
) -> Result<()> {
    use iced::futures::SinkExt as _;

    let srv = entrada.servidor.clone();
    let ice = tokio::task::spawn_blocking(move || busca_ice(&srv)).await?;
    let mut rtc = RtcConfiguration::default();
    rtc.ice_servers = ice;

    let (tx, mut rx) = mpsc::unbounded_channel::<Interno>();
    let (tx_ui, mut rx_ui) = mpsc::unbounded_channel::<Comando>();
    saida.send(Evento::Pronta(tx_ui)).await?;

    let mut _tarefas_ui = Tarefas(Vec::new());
    {
        let tx = tx.clone();
        _tarefas_ui.0.push(tokio::spawn(async move {
            while let Some(c) = rx_ui.recv().await {
                if tx.send(Interno::Ui(c)).is_err() {
                    break;
                }
            }
        }));
    }
    {
        let tx = tx.clone();
        _tarefas_ui.0.push(tokio::spawn(async move {
            let mut intervalo = tokio::time::interval(std::time::Duration::from_millis(100));
            loop {
                intervalo.tick().await;
                if tx.send(Interno::CicloEco).is_err() {
                    break;
                }
            }
        }));
    }
    {
        let tx = tx.clone();
        _tarefas_ui.0.push(tokio::spawn(async move {
            let mut intervalo = tokio::time::interval(std::time::Duration::from_millis(3000));
            loop {
                intervalo.tick().await;
                if tx.send(Interno::CicloQualidade).is_err() {
                    break;
                }
            }
        }));
    }

    let (tx_ws, mut tarefas_ws) = conecta_ws(&entrada, tx.clone()).await?;

    tx_ws.send(json!({
        "type": "join",
        "conversa": entrada.sala,
        "espectador": true,
    }))?;

    let mut ator = Ator {
        saida,
        ws: tx_ws,
        tx: tx.clone(),
        fabrica: PeerConnectionFactory::default(),
        rtc,
        meu_id: String::new(),
        meu_uid: None,
        meu_nome: entrada.nome.clone(),
        pessoas: BTreeMap::new(),
        pares: HashMap::new(),
        telas_anunciadas: HashSet::new(),
        na_chamada: false,
        mudo: false,
        surdo: false,
        mic: None,
        id_voz: format!("lincast-voz-{}", aleatorio()),
        id_tela: format!("lincast-tela-{}", aleatorio()),
        id_camera: format!("lincast-cam-{}", aleatorio()),
        tela: None,
        camera: None,
        previa: None,
        ptt_ativo: false,
        ptt_segurando: false,
        teste_mic: None,
        monitor_mic_local: None,
        meu_falando: false,
        falando_no_mudo: false,
        inicio_fala_mudo: None,
        ultimo_som_mudo: None,
        detector_eco: crate::eco::DetectorEco::new(),
        mudo_forcado: false,
        sou_dono: false,
    };

    let _ = ator.fabrica.acquire_platform_adm();
    let cfg_audio = crate::dispositivos::carrega_config();
    if let Some(guid) = &cfg_audio.entrada_guid {
        crate::dispositivos::aplica_dispositivo_gravacao(&ator.fabrica, guid);
    }
    if let Some(guid) = &cfg_audio.saida_guid {
        crate::dispositivos::aplica_dispositivo_reproducao(&ator.fabrica, guid);
    }

    while let Some(ev) = rx.recv().await {
        if let Interno::Caiu(motivo) = ev {
            ator.limpa_pares();
            ator.emite(Evento::Aviso(format!(
                "Conexão perdida ({motivo}). Reconectando…"
            )))
            .await;
            tarefas_ws.aborta_todas();

            let mut backoff = std::time::Duration::from_millis(500);
            loop {
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(std::time::Duration::from_secs(5));

                match conecta_ws(&entrada, tx.clone()).await {
                    Ok((novo_tx_ws, novas_tarefas_ws)) => {
                        ator.ws = novo_tx_ws;
                        tarefas_ws.substitui(novas_tarefas_ws);
                        let _ = ator.ws.send(json!({
                            "type": "join",
                            "conversa": entrada.sala,
                            "espectador": !ator.na_chamada,
                        }));
                        ator.emite(Evento::Aviso("Reconectado ao servidor.".into()))
                            .await;
                        break;
                    }
                    Err(e) => {
                        log::warn!("[reconexao] tentativa falhou ({e:#}); tentando novamente");
                    }
                }
            }
            continue;
        }

        if let Err(e) = ator.trata(ev).await {
            let texto = format!("{e:#}");
            if let Some(m) = texto.strip_prefix("RECUSADA:") {
                ator.encerra_midia();
                ator.emite(Evento::Recusada(m.to_string())).await;
                return Ok(());
            }
            log::warn!("{texto}");
        }
    }
    ator.fabrica.release_platform_adm();
    Ok(())
}

struct Tarefas(Vec<tokio::task::JoinHandle<()>>);

impl Drop for Tarefas {
    fn drop(&mut self) {
        for t in &self.0 {
            t.abort();
        }
    }
}

impl Tarefas {
    fn aborta_todas(&mut self) {
        for t in self.0.drain(..) {
            t.abort();
        }
    }

    fn substitui(&mut self, mut novas: Tarefas) {
        self.aborta_todas();
        self.0.append(&mut novas.0);
    }
}

fn busca_ice(srv: &Servidor) -> Vec<IceServer> {
    let url = srv.url("/api/ice-config");
    let lido = (|| -> Result<Vec<IceServer>> {
        let v: Value = ureq::get(&url).call()?.body_mut().read_json()?;
        let mut lista = Vec::new();
        for s in v["iceServers"].as_array().context("sem iceServers")? {
            let urls = match &s["urls"] {
                Value::String(u) => vec![u.clone()],
                Value::Array(a) => a
                    .iter()
                    .filter_map(|u| u.as_str().map(String::from))
                    .collect(),
                _ => continue,
            };
            lista.push(IceServer {
                urls,
                username: s["username"].as_str().unwrap_or("").into(),
                password: s["credential"].as_str().unwrap_or("").into(),
            });
        }
        Ok(lista)
    })();
    match lido {
        Ok(l) if !l.is_empty() => l,
        _ => {
            log::warn!("[ice] não consegui ler {url}; usando só STUN público");
            vec![IceServer {
                urls: vec!["stun:stun.l.google.com:19302".into()],
                username: String::new(),
                password: String::new(),
            }]
        }
    }
}

/// Resumo de uma SDP pro log: `mid:direção` de cada linha de mídia.
fn resumo_sdp(sdp: &str) -> String {
    let mut out = Vec::new();
    let mut mid = String::from("?");
    for l in sdp.lines() {
        if let Some(m) = l.strip_prefix("a=mid:") {
            mid = m.trim().to_string();
        }
        for d in ["sendrecv", "sendonly", "recvonly", "inactive"] {
            if l.trim() == format!("a={d}") {
                out.push(format!("{mid}:{d}"));
            }
        }
    }
    out.join(" ")
}

fn mensagem_de(v: &Value) -> Option<Mensagem> {
    if let Some(tipo) = v["sys"].as_str() {
        let texto = v["texto"].as_str().unwrap_or("").to_string();
        if texto.is_empty() {
            return None;
        }
        return Some(Mensagem {
            id: v["id"].as_str().unwrap_or("").to_string(),
            autor: String::new(),
            uid: None,
            texto,
            ts: v["ts"].as_i64().or_else(|| v["hora"].as_i64()).unwrap_or(0),
            editada_em: None,
            imagem: None,
            gif: None,
            resposta_a: None,
            reacoes: std::collections::HashMap::new(),
            mencoes: Vec::new(),
            mencoes_uids: Vec::new(),
            sussurro: false,
            sussurro_para: Vec::new(),
            enquete: None,
            votos: std::collections::HashMap::new(),
            sistema: Some(tipo.to_string()),
            extra: MensagemExtra {
                encaminhada_de: None,
                app: None,
                wordle_numero: None,
                wordle_dia: None,
                arquivo: None,
                quem: v["quem"].as_str().map(String::from),
                duracao: v["duracao"].as_str().map(String::from),
                perdeu: v["perdeu"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default(),
            },
        });
    }
    let texto = v["text"].as_str().unwrap_or("").to_string();
    let imagem = v["image"]["data"]
        .as_str()
        .and_then(|d| base64::engine::general_purpose::STANDARD.decode(d).ok());
    let gif = v["gif"].as_str().map(String::from);
    let enquete = v["enquete"].as_object().and_then(|obj| {
        let pergunta = obj.get("pergunta")?.as_str()?.to_string();
        let opcoes = obj
            .get("opcoes")?
            .as_array()?
            .iter()
            .filter_map(|x| x.as_str().map(String::from))
            .collect::<Vec<_>>();
        if pergunta.is_empty() || opcoes.len() < 2 {
            None
        } else {
            Some(Enquete { pergunta, opcoes })
        }
    });
    let app = v["app"].as_str().map(String::from);
    if texto.is_empty() && imagem.is_none() && gif.is_none() && enquete.is_none() && app.is_none() {
        return None;
    }
    Some(Mensagem {
        id: v["id"].as_str().unwrap_or("").to_string(),
        autor: v["name"].as_str().unwrap_or("?").to_string(),
        uid: v["uid"].as_str().map(String::from),
        texto,
        ts: v["ts"].as_i64().unwrap_or(0),
        editada_em: v["editedAt"].as_i64(),
        imagem,
        gif,
        resposta_a: v["replyTo"]["id"]
            .as_str()
            .map(|id| RespostaA {
                id: id.to_string(),
                autor: v["replyTo"]["name"].as_str().unwrap_or("?").to_string(),
                texto: v["replyTo"]["text"].as_str().unwrap_or("").to_string(),
            })
            .or_else(|| {
                v["replyTo"]["name"].as_str().map(|n| RespostaA {
                    id: String::new(),
                    autor: n.to_string(),
                    texto: v["replyTo"]["text"].as_str().unwrap_or("").to_string(),
                })
            }),
        reacoes: v["reactions"]
            .as_object()
            .map(|obj| {
                obj.iter()
                    .map(|(k, v)| {
                        let quem = v
                            .as_array()
                            .map(|a| {
                                a.iter()
                                    .filter_map(|x| x.as_str().map(String::from))
                                    .collect()
                            })
                            .unwrap_or_default();
                        (k.clone(), quem)
                    })
                    .collect()
            })
            .unwrap_or_default(),
        mencoes: v["mentions"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        mencoes_uids: v["mentionUids"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        sussurro: v["sussurro"].as_bool().unwrap_or(false),
        sussurro_para: v["sussurroPara"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        enquete,
        votos: v["votos"]
            .as_object()
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| v.as_u64().map(|idx| (k.clone(), idx as usize)))
                    .collect()
            })
            .unwrap_or_default(),
        sistema: None,
        extra: MensagemExtra {
            encaminhada_de: v["fwd"]["nome"].as_str().map(String::from),
            app,
            wordle_numero: v["wordle"]["numero"].as_i64(),
            wordle_dia: v["wordle"]["dia"]
                .as_str()
                .map(String::from)
                .or_else(|| v["wordle"]["dia"].as_i64().map(|n| n.to_string())),
            arquivo: v["arquivo"]["id"].as_str().map(|id| ArquivoMsg {
                id: id.to_string(),
                nome: v["arquivo"]["nome"]
                    .as_str()
                    .unwrap_or("arquivo")
                    .to_string(),
                tam: v["arquivo"]["tam"].as_u64().unwrap_or(0),
            }),
            ..Default::default()
        },
    })
}

impl Drop for Ator {
    // Mesmo motivo das `Tarefas`: sair da sala pela interface não passa por
    // nenhum comando, o ator simplesmente deixa de existir. Conexões, captura
    // de tela e o microfone têm que ser soltos aqui.
    fn drop(&mut self) {
        self.encerra_midia();
    }
}

impl Ator {
    async fn emite(&mut self, ev: Evento) {
        use iced::futures::SinkExt as _;
        let _ = self.saida.send(ev).await;
    }

    fn envia(&self, v: Value) {
        let _ = self.ws.send(v);
    }

    async fn publica_pessoas(&mut self) {
        let lista: Vec<Pessoa> = self.pessoas.values().cloned().collect();
        self.emite(Evento::Pessoas(lista)).await;
    }

    async fn publica_meu_estado(&mut self) {
        let ev = Evento::MeuEstado {
            na_chamada: self.na_chamada,
            mudo: self.mudo,
            surdo: self.surdo,
            apresentando: self.tela.is_some(),
            com_camera: self.camera.is_some(),
            mudo_forcado: self.mudo_forcado,
        };
        if let Some(eu) = self.pessoas.get_mut(&self.meu_id) {
            eu.espectador = !self.na_chamada;
            eu.mudo = self.mudo || self.surdo;
            eu.surdo = self.surdo;
            eu.apresentando = self.tela.is_some();
            eu.com_camera = self.camera.is_some();
            eu.mudo_forcado = self.mudo_forcado;
            eu.dono = self.sou_dono;
        }
        self.emite(ev).await;
        self.publica_pessoas().await;
    }

    fn limpa_pares(&mut self) {
        self.detector_eco.limpar();
        for (_, p) in self.pares.drain() {
            p.pc.close();
        }
        for pessoa in self.pessoas.values_mut() {
            pessoa.conexao = None;
        }
    }

    fn encerra_midia(&mut self) {
        self.cancela_previa_camera();
        for (_, p) in self.pares.drain() {
            p.pc.close();
        }
        if let Some(c) = self.camera.take() {
            c.captura.para();
        }
        if let Some(t) = self.tela.take() {
            t.captura.para();
        }
        if self.mic.take().is_some() {
            self.fabrica.release_platform_adm();
        }
        if let Some((handle, temp_track)) = self.teste_mic.take() {
            handle.abort();
            drop(temp_track);
        }
        if let Some(h) = self.monitor_mic_local.take() {
            h.abort();
        }
    }

    async fn trata(&mut self, ev: Interno) -> Result<()> {
        match ev {
            Interno::Servidor(msg) => self.mensagem(msg).await,
            Interno::Ui(c) => self.comando(c).await,
            Interno::Negociar(id) => self.oferece(&id).await,
            Interno::Candidato(id, c) => {
                self.envia(json!({
                    "type": "ice-candidate", "to": id,
                    "candidate": { "candidate": c.candidate(), "sdpMid": c.sdp_mid(), "sdpMLineIndex": c.sdp_mline_index() }
                }));
                Ok(())
            }
            Interno::Estado(id, estado) => {
                log::info!("[{id}] conexão: {estado:?}");
                if estado == PeerConnectionState::Failed
                    && let Some(p) = self.pares.get(&id)
                {
                    p.pc.restart_ice();
                }
                if let Some(p) = self.pessoas.get_mut(&id) {
                    p.conexao = Some(estado);
                }
                self.publica_pessoas().await;
                Ok(())
            }
            Interno::Trilha(id, t) => {
                self.trilha_remota(&id, t);
                Ok(())
            }
            Interno::NivelMic(nivel) => {
                self.emite(Evento::NivelMic(nivel)).await;
                Ok(())
            }
            Interno::Falando(id, falando) => {
                if let Some(p) = self.pessoas.get_mut(&id)
                    && p.falando != falando
                {
                    p.falando = falando;
                    self.emite(Evento::Falando { id, falando }).await;
                }
                Ok(())
            }
            Interno::NivelMicLocal(nivel) => {
                let mut mic_ativo = !(self.mudo || self.surdo || self.mudo_forcado);
                if self.ptt_ativo && mic_ativo {
                    mic_ativo = self.ptt_segurando;
                }
                if mic_ativo {
                    if self.falando_no_mudo {
                        self.falando_no_mudo = false;
                        self.inicio_fala_mudo = None;
                        self.ultimo_som_mudo = None;
                        self.emite(Evento::FalandoNoMudo(false)).await;
                    }
                    let falando = nivel > 12.0;
                    if falando != self.meu_falando {
                        self.meu_falando = falando;
                        if let Some(eu) = self.pessoas.get_mut(&self.meu_id) {
                            eu.falando = falando;
                        }
                        self.emite(Evento::Falando {
                            id: self.meu_id.clone(),
                            falando,
                        })
                        .await;
                    }
                } else {
                    if self.meu_falando {
                        self.meu_falando = false;
                        if let Some(eu) = self.pessoas.get_mut(&self.meu_id) {
                            eu.falando = false;
                        }
                        self.emite(Evento::Falando {
                            id: self.meu_id.clone(),
                            falando: false,
                        })
                        .await;
                    }
                    if nivel > 12.0 {
                        if self.inicio_fala_mudo.is_none() {
                            self.inicio_fala_mudo = Some(tokio::time::Instant::now());
                        }
                        self.ultimo_som_mudo = Some(tokio::time::Instant::now());
                        if let Some(inicio) = self.inicio_fala_mudo
                            && inicio.elapsed() >= std::time::Duration::from_millis(1200)
                            && !self.falando_no_mudo
                        {
                            self.falando_no_mudo = true;
                            self.emite(Evento::FalandoNoMudo(true)).await;
                        }
                    } else if let Some(ult) = self.ultimo_som_mudo
                        && ult.elapsed() >= std::time::Duration::from_millis(600)
                    {
                        self.inicio_fala_mudo = None;
                        self.ultimo_som_mudo = None;
                        if self.falando_no_mudo {
                            self.falando_no_mudo = false;
                            self.emite(Evento::FalandoNoMudo(false)).await;
                        }
                    }
                }
                if mic_ativo {
                    self.detector_eco.registra_meu_nivel(nivel);
                } else {
                    self.detector_eco.registra_meu_nivel(0.0);
                }
                Ok(())
            }
            Interno::NivelRemoto(id, nivel) => {
                self.detector_eco.registra_nivel_peer(&id, nivel);
                Ok(())
            }
            Interno::CicloEco => {
                let avisos = self.detector_eco.verificar_eco();
                for id in avisos {
                    log::info!("[eco] detectado eco de {id}; enviando echo-warning");
                    self.envia(json!({ "type": "echo-warning", "to": id }));
                }
                Ok(())
            }
            Interno::CicloQualidade => {
                self.processa_qualidade_e_bitrate().await;
                Ok(())
            }
            Interno::Caiu(_) => Ok(()),
        }
    }

    // --- comandos da interface ---------------------------------------------

    async fn comando(&mut self, c: Comando) -> Result<()> {
        match c {
            Comando::Chat {
                texto,
                imagem,
                gif,
                resposta_a,
                para,
                enquete,
            } => {
                let texto = texto.trim().to_string();
                if !texto.is_empty() || imagem.is_some() || gif.is_some() || enquete.is_some() {
                    let mut payload = json!({ "type": "chat", "text": texto });
                    if let Some(img) = imagem {
                        payload["image"] = json!({
                            "mime": img.mime,
                            "data": img.data,
                        });
                    }
                    if let Some(g) = gif {
                        payload["gif"] = json!(g);
                    }
                    if let Some(r) = resposta_a {
                        payload["replyTo"] = json!({
                            "id": r.id,
                            "name": r.autor,
                            "text": r.texto,
                        });
                    }
                    if let Some(p) = para {
                        payload["para"] = json!(p);
                    }
                    if let Some(enq) = enquete {
                        payload["enquete"] = json!({
                            "pergunta": enq.pergunta,
                            "opcoes": enq.opcoes,
                        });
                    }
                    self.envia(payload);
                }
            }
            Comando::ChatArquivo { texto, arquivo_id } => {
                self.envia(json!({
                    "type": "chat",
                    "text": texto.trim(),
                    "arquivoId": arquivo_id,
                }));
            }
            Comando::VotarEnquete { id, opcao } => {
                let id = id.trim().to_string();
                if !id.is_empty() {
                    self.envia(json!({
                        "type": "voto",
                        "id": id,
                        "opcao": opcao,
                    }));
                }
            }
            Comando::EditaChat { id, texto } => {
                let texto = texto.trim().to_string();
                let id = id.trim().to_string();
                if !id.is_empty() && !texto.is_empty() {
                    self.envia(json!({
                        "type": "chat-edit",
                        "id": id,
                        "text": texto,
                    }));
                }
            }
            Comando::ApagaChat { id } => {
                let id = id.trim().to_string();
                if !id.is_empty() {
                    self.envia(json!({
                        "type": "chat-delete",
                        "id": id,
                    }));
                }
            }
            Comando::ReagirChat { id, emoji } => {
                let id = id.trim().to_string();
                let emoji = emoji.trim().to_string();
                if !id.is_empty() && !emoji.is_empty() {
                    self.envia(json!({
                        "type": "chat-react",
                        "id": id,
                        "emoji": emoji,
                    }));
                }
            }
            Comando::FixarChat(id) => {
                let id_val = id.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
                self.envia(json!({
                    "type": "chat-pin",
                    "id": id_val,
                }));
            }
            Comando::Digitando(d) => {
                self.envia(json!({
                    "type": "typing",
                    "typing": d,
                }));
            }
            Comando::AtualizaPerfilSala {
                cor,
                cor_nome,
                efeito_nome,
            } => {
                self.envia(json!({
                    "type": "profile",
                    "color": cor,
                    "nameColor": cor_nome,
                    "nameEffect": efeito_nome,
                }));
                if let Some(me) = self.pessoas.get_mut(&self.meu_id) {
                    me.cor = cor;
                    me.cor_nome = cor_nome;
                    me.efeito_nome = efeito_nome;
                }
                self.publica_pessoas().await;
            }
            Comando::TrocarStatus(status) => {
                self.envia(json!({
                    "type": "status-state",
                    "status": status,
                }));
                if let Some(me) = self.pessoas.get_mut(&self.meu_id) {
                    me.status = Some(status);
                }
                self.publica_pessoas().await;
            }
            Comando::TrocarAtividade(atv) => {
                self.envia(json!({
                    "type": "activity-state",
                    "atividade": atv,
                }));
                if let Some(me) = self.pessoas.get_mut(&self.meu_id) {
                    me.atividade = atv;
                }
                self.publica_pessoas().await;
            }
            Comando::EnviarReacao(emoji) => {
                self.envia(json!({
                    "type": "reacao",
                    "emoji": emoji,
                }));
            }
            Comando::Cutucar(id) => {
                self.envia(json!({
                    "type": "cutucada",
                    "to": id,
                }));
            }
            Comando::EnviarSom(id, vol) => {
                self.envia(json!({
                    "type": "som",
                    "id": id,
                    "vol": vol,
                }));
            }
            Comando::EnviarTraco(traco) => {
                self.envia(json!({
                    "type": "quadro",
                    "traco": traco,
                }));
            }
            Comando::LimparQuadro => {
                self.envia(json!({
                    "type": "quadro-limpar",
                }));
            }
            Comando::CriarRoleta { nome, opcoes } => {
                self.envia(json!({
                    "type": "roleta-criar",
                    "roleta": { "nome": nome, "opcoes": opcoes },
                }));
            }
            Comando::EditarRoleta { id, nome, opcoes } => {
                self.envia(json!({
                    "type": "roleta-editar",
                    "id": id,
                    "roleta": { "nome": nome, "opcoes": opcoes },
                }));
            }
            Comando::ApagarRoleta(id) => {
                self.envia(json!({
                    "type": "roleta-apagar",
                    "id": id,
                }));
            }
            Comando::GirarRoleta(id) => {
                self.envia(json!({
                    "type": "roleta-girar",
                    "id": id,
                }));
            }
            Comando::EntrarNaChamada => self.entra_na_chamada().await?,
            Comando::SairDaChamada => self.sai_da_chamada().await,
            Comando::Mudo(m) => {
                if self.mudo_forcado && !m {
                    log::warn!("Tentativa de desmutar bloqueada por force-mute do dono");
                    return Ok(());
                }
                self.mudo = m;
                self.aplica_mic();
                if !self.surdo {
                    self.envia(json!({ "type": "mic-state", "muted": m }));
                }
                self.publica_meu_estado().await;
            }
            Comando::Surdo(s) => {
                self.surdo = s;
                // Ensurdecer cala o que chega (playout do ADM) e, como no
                // Discord e na v1, o microfone junto.
                self.fabrica.set_adm_playout_enabled(!s);
                self.aplica_mic();
                self.envia(json!({ "type": "deafen-state", "deafened": s }));
                self.envia(
                    json!({ "type": "mic-state", "muted": s || self.mudo, "viaDeafen": true }),
                );
                self.publica_meu_estado().await;
            }
            Comando::CompartilharTela {
                apps,
                perfil,
                auto_bitrate,
            } => self.compartilha_tela(apps, perfil, auto_bitrate).await?,
            Comando::PararTela => self.para_tela().await,
            Comando::PreviaCamera(opt_idx) => self.inicia_previa_camera(opt_idx),
            Comando::CancelaPreviaCamera => self.cancela_previa_camera(),
            Comando::IniciarCamera { indice } => self.inicia_camera(indice).await?,
            Comando::PararCamera => self.para_camera().await,
            Comando::AjustaVolume {
                id,
                volume,
                de_tela,
            } => {
                let volume = volume.clamp(0.0, 2.0);
                if let Some(p) = self.pessoas.get_mut(&id) {
                    if de_tela {
                        p.volume_tela = volume;
                        volumes::salvar(p.uid.as_deref(), &p.nome, true, volume);
                        if let Some(par) = self.pares.get(&id) {
                            for trilha in &par.trilhas_tela {
                                trilha.set_enabled(volume > 0.0);
                            }
                        }
                    } else {
                        p.volume = volume;
                        volumes::salvar(p.uid.as_deref(), &p.nome, false, volume);
                        if let Some(par) = self.pares.get(&id) {
                            for trilha in &par.trilhas_voz {
                                trilha.set_enabled(volume > 0.0);
                            }
                        }
                    }
                }
                self.publica_pessoas().await;
            }
            Comando::AlternaMute { id, de_tela } => {
                if let Some(p) = self.pessoas.get_mut(&id) {
                    if de_tela {
                        if p.volume_tela > 0.0 {
                            p.volume_tela_anterior = Some(p.volume_tela);
                            p.volume_tela = 0.0;
                        } else {
                            p.volume_tela = p.volume_tela_anterior.unwrap_or(1.0).max(0.05);
                        }
                        let vol = p.volume_tela;
                        let uid = p.uid.clone();
                        let nome = p.nome.clone();
                        volumes::salvar(uid.as_deref(), &nome, true, vol);
                        if let Some(par) = self.pares.get(&id) {
                            for trilha in &par.trilhas_tela {
                                trilha.set_enabled(vol > 0.0);
                            }
                        }
                    } else {
                        if p.volume > 0.0 {
                            p.volume_anterior = Some(p.volume);
                            p.volume = 0.0;
                        } else {
                            p.volume = p.volume_anterior.unwrap_or(1.0).max(0.05);
                        }
                        let vol = p.volume;
                        let uid = p.uid.clone();
                        let nome = p.nome.clone();
                        volumes::salvar(uid.as_deref(), &nome, false, vol);
                        if let Some(par) = self.pares.get(&id) {
                            for trilha in &par.trilhas_voz {
                                trilha.set_enabled(vol > 0.0);
                            }
                        }
                    }
                }
                self.publica_pessoas().await;
            }
            Comando::PttConfigurado { ativo } => {
                self.ptt_ativo = ativo;
                self.aplica_mic();
                self.publica_meu_estado().await;
            }
            Comando::PttTransmitindo(segurando) => {
                if self.ptt_segurando != segurando {
                    self.ptt_segurando = segurando;
                    self.aplica_mic();
                    self.emite(Evento::PttTransmitindo(segurando)).await;
                }
            }
            Comando::AtualizaDispositivosAudio => {
                let (entradas, saidas) = crate::dispositivos::lista_dispositivos(&self.fabrica);
                self.emite(Evento::DispositivosAudio { entradas, saidas })
                    .await;
            }
            Comando::SelecionaDispositivoGravacao(guid) => {
                let _ = crate::dispositivos::aplica_dispositivo_gravacao(&self.fabrica, &guid);
                let mut cfg = crate::dispositivos::carrega_config();
                cfg.entrada_guid = Some(guid.clone());
                let (entradas, _) = crate::dispositivos::lista_dispositivos(&self.fabrica);
                if let Some(d) = entradas.iter().find(|d| d.guid == guid) {
                    cfg.entrada_nome = Some(d.nome.clone());
                }
                crate::dispositivos::salva_config(&cfg);
                if self.na_chamada && self.mic.is_some() {
                    let novo_mic = self.fabrica.create_device_audio_track("lincast-mic");
                    let mut ligado = !(self.mudo || self.surdo);
                    if self.ptt_ativo && ligado {
                        ligado = self.ptt_segurando;
                    }
                    novo_mic.set_enabled(ligado);
                    self.mic = Some(novo_mic.clone());
                    for p in self.pares.values() {
                        for sender in p.pc.senders() {
                            if let Some(libwebrtc::media_stream_track::MediaStreamTrack::Audio(_)) =
                                sender.track()
                            {
                                let _ = sender.set_track(Some(
                                    libwebrtc::media_stream_track::MediaStreamTrack::Audio(
                                        novo_mic.clone(),
                                    ),
                                ));
                            }
                        }
                    }
                    self.inicia_monitor_mic_local();
                }
            }
            Comando::SelecionaDispositivoReproducao(guid) => {
                let _ = crate::dispositivos::aplica_dispositivo_reproducao(&self.fabrica, &guid);
                let mut cfg = crate::dispositivos::carrega_config();
                cfg.saida_guid = Some(guid.clone());
                let (_, saidas) = crate::dispositivos::lista_dispositivos(&self.fabrica);
                if let Some(d) = saidas.iter().find(|d| d.guid == guid) {
                    cfg.saida_nome = Some(d.nome.clone());
                }
                crate::dispositivos::salva_config(&cfg);
            }
            Comando::IniciaTesteMic => {
                if self.teste_mic.is_none() {
                    let (track, temp) = if let Some(m) = &self.mic {
                        (m.clone(), false)
                    } else {
                        (
                            self.fabrica.create_device_audio_track("lincast-mic-teste"),
                            true,
                        )
                    };

                    let stream = libwebrtc::audio_stream::native::NativeAudioStream::new(
                        track.clone(),
                        48000,
                        1,
                    );
                    let tx = self.tx.clone();
                    let handle = tokio::spawn(async move {
                        use futures_util::StreamExt;
                        let mut stream = stream;
                        let mut ultimo = tokio::time::Instant::now();
                        let mut acumulado: f32 = 0.0;
                        while let Some(frame) = stream.next().await {
                            let mut max_abs: f32 = 0.0;
                            for &s in frame.data.iter() {
                                let v = (s as f32).abs() / 32768.0;
                                if v > max_abs {
                                    max_abs = v;
                                }
                            }
                            if max_abs > acumulado {
                                acumulado = max_abs;
                            }
                            if ultimo.elapsed() >= std::time::Duration::from_millis(50) {
                                let nivel = (acumulado.sqrt() * 100.0).clamp(0.0, 100.0);
                                let _ = tx.send(Interno::NivelMic(nivel));
                                ultimo = tokio::time::Instant::now();
                                acumulado = 0.0;
                            }
                        }
                        let _ = tx.send(Interno::NivelMic(0.0));
                    });

                    self.teste_mic = Some((handle, if temp { Some(track) } else { None }));
                }
            }
            Comando::ParaTesteMic => {
                if let Some((handle, temp_track)) = self.teste_mic.take() {
                    handle.abort();
                    drop(temp_track);
                    self.emite(Evento::NivelMic(0.0)).await;
                }
            }
        }
        Ok(())
    }

    fn aplica_mic(&self) {
        if let Some(m) = &self.mic {
            let mut ligado = !(self.mudo || self.surdo || self.mudo_forcado);
            if self.ptt_ativo && ligado {
                ligado = self.ptt_segurando;
            }
            m.set_enabled(ligado);
        }
    }

    async fn entra_na_chamada(&mut self) -> Result<()> {
        if self.na_chamada || self.meu_id.is_empty() {
            return Ok(());
        }
        // Microfone e alto-falante pelo módulo de áudio da plataforma
        // (PulseAudio/PipeWire no Linux), com o cancelamento de eco do próprio
        // WebRTC — que só funciona porque o som recebido também toca por esse
        // módulo. Sem `acquire` o libwebrtc fica no modo "sintético": nada é
        // gravado e nada toca (foi o primeiro teste: conexão de pé, 0 bytes).
        if !self.fabrica.acquire_platform_adm() {
            self.emite(Evento::Aviso(
                "Não consegui abrir o áudio do sistema (microfone/alto-falante).".into(),
            ))
            .await;
            return Ok(());
        }
        self.fabrica.set_adm_recording_enabled(true);
        self.fabrica.set_adm_playout_enabled(!self.surdo);
        let mic = self.fabrica.create_device_audio_track("concord-mic");
        self.mic = Some(mic);
        self.aplica_mic();
        self.na_chamada = true;
        self.envia(json!({ "type": "mode-state", "espectador": false }));
        let alvos: Vec<String> = self
            .pessoas
            .values()
            .filter(|p| !p.eu && !p.espectador)
            .map(|p| p.id.clone())
            .collect();
        for id in alvos {
            let ofereco = self.meu_id.as_str() > id.as_str();
            self.cria_par(&id, ofereco)?;
        }
        // Reanuncia o estado: quem já estava precisa saber se entrei mudo.
        if self.mudo || self.surdo {
            self.envia(json!({ "type": "mic-state", "muted": true, "resync": true }));
        }
        self.inicia_monitor_mic_local();
        self.publica_meu_estado().await;
        Ok(())
    }

    fn inicia_monitor_mic_local(&mut self) {
        if let Some(h) = self.monitor_mic_local.take() {
            h.abort();
        }
        let track = self
            .fabrica
            .create_device_audio_track("lincast-monitor-mic");
        let stream = libwebrtc::audio_stream::native::NativeAudioStream::new(track, 48000, 1);
        let tx = self.tx.clone();
        let handle = tokio::spawn(async move {
            use futures_util::StreamExt;
            let mut stream = stream;
            let mut ultimo = tokio::time::Instant::now();
            let mut acumulado: f32 = 0.0;
            while let Some(frame) = stream.next().await {
                let mut max_abs: f32 = 0.0;
                for &s in frame.data.iter() {
                    let v = (s as f32).abs() / 32768.0;
                    if v > max_abs {
                        max_abs = v;
                    }
                }
                if max_abs > acumulado {
                    acumulado = max_abs;
                }
                if ultimo.elapsed() >= std::time::Duration::from_millis(100) {
                    let nivel = (acumulado.sqrt() * 100.0).clamp(0.0, 100.0);
                    acumulado = 0.0;
                    ultimo = tokio::time::Instant::now();
                    let _ = tx.send(Interno::NivelMicLocal(nivel));
                }
            }
        });
        self.monitor_mic_local = Some(handle);
    }

    async fn sai_da_chamada(&mut self) {
        if !self.na_chamada {
            return;
        }
        if let Some(h) = self.monitor_mic_local.take() {
            h.abort();
        }
        self.falando_no_mudo = false;
        self.meu_falando = false;
        self.inicio_fala_mudo = None;
        self.ultimo_som_mudo = None;
        self.emite(Evento::FalandoNoMudo(false)).await;
        if let Some(eu) = self.pessoas.get_mut(&self.meu_id) {
            eu.falando = false;
        }
        self.emite(Evento::Falando {
            id: self.meu_id.clone(),
            falando: false,
        })
        .await;

        if self.camera.is_some() {
            self.para_camera().await;
        }
        if self.tela.is_some() {
            self.para_tela().await;
        }
        for (id, p) in self.pares.drain() {
            p.pc.close();
            if let Some(pessoa) = self.pessoas.get_mut(&id) {
                pessoa.conexao = None;
            }
        }
        self.mic = None;
        self.fabrica.release_platform_adm();
        self.na_chamada = false;
        self.envia(json!({ "type": "mode-state", "espectador": true }));
        let donos: Vec<String> = self.pessoas.keys().cloned().collect();
        for d in donos {
            self.emite(Evento::VideoFechou {
                dono: d.clone(),
                de_tela: true,
            })
            .await;
            self.emite(Evento::VideoFechou {
                dono: d,
                de_tela: false,
            })
            .await;
        }
        self.publica_meu_estado().await;
    }

    fn inicia_previa_camera(&mut self, indice: Option<usize>) {
        self.cancela_previa_camera();
        let indice = indice.unwrap_or(0);
        let (tx_parar, rx_parar) = std::sync::mpsc::channel();
        let saida = self.saida.clone();

        std::thread::Builder::new()
            .name("previa-cam".into())
            .spawn(move || {
                use v4l::video::Capture;
                let dev = match v4l::Device::new(indice) {
                    Ok(d) => d,
                    Err(e) => {
                        log::warn!("[previa] erro abrindo câmera {indice}: {e}");
                        return;
                    }
                };
                let mut fmt = match dev.format() {
                    Ok(f) => f,
                    Err(_) => return,
                };
                fmt.width = 640;
                fmt.height = 480;
                fmt.fourcc = v4l::FourCC::new(b"YUYV");
                let fmt = dev
                    .set_format(&fmt)
                    .unwrap_or_else(|_| dev.format().unwrap_or(fmt));
                let (w, h) = (fmt.width, fmt.height);
                let fourcc = fmt.fourcc;
                let mut stream = match v4l::io::mmap::Stream::with_buffers(
                    &dev,
                    v4l::buffer::Type::VideoCapture,
                    2,
                ) {
                    Ok(s) => s,
                    Err(e) => {
                        log::warn!("[previa] erro stream mmap: {e}");
                        return;
                    }
                };
                let mut rgba = vec![0u8; (w * h * 4) as usize];
                let mut ultimo = std::time::Instant::now() - Duration::from_secs(1);
                use v4l::io::traits::CaptureStream;
                while rx_parar.try_recv().is_err() {
                    let (buf, _) = match stream.next() {
                        Ok(b) => b,
                        Err(_) => {
                            std::thread::sleep(Duration::from_millis(20));
                            continue;
                        }
                    };
                    if ultimo.elapsed() < Duration::from_millis(40) {
                        continue;
                    }
                    ultimo = std::time::Instant::now();
                    let mut ok = false;
                    if fourcc == v4l::FourCC::new(b"YUYV") && buf.len() >= (w * h * 2) as usize {
                        captura_camera::converte_yuyv_para_rgba(
                            buf, w as usize, h as usize, &mut rgba,
                        );
                        ok = true;
                    } else if fourcc == v4l::FourCC::new(b"MJPG")
                        && let Ok(img) =
                            image::load_from_memory_with_format(buf, image::ImageFormat::Jpeg)
                    {
                        let img_rgba = img.to_rgba8();
                        rgba.copy_from_slice(img_rgba.as_raw());
                        ok = true;
                    }
                    if ok {
                        let q = Quadro {
                            dono: "previa".into(),
                            de_tela: false,
                            largura: w,
                            altura: h,
                            rgba: rgba.clone(),
                        };
                        let mut s = saida.clone();
                        let _ = s.try_send(Evento::QuadroPrevia(q));
                    }
                }
            })
            .expect("thread previa");

        self.previa = Some(Previa { parar: tx_parar });
    }

    fn cancela_previa_camera(&mut self) {
        if let Some(prev) = self.previa.take() {
            let _ = prev.parar.send(());
        }
    }

    async fn inicia_camera(&mut self, indice: usize) -> Result<()> {
        self.cancela_previa_camera();
        if self.camera.is_some() || !self.na_chamada {
            return Ok(());
        }
        let (tx_quadro, mut rx_quadro) = tokio::sync::mpsc::unbounded_channel();
        let captura = tokio::task::spawn_blocking(move || {
            captura_camera::inicia(indice, 640, 480, Some(tx_quadro))
        })
        .await?
        .map_err(|e| anyhow!("não consegui iniciar a câmera: {e:#}"))?;

        let video = self
            .fabrica
            .create_video_track("concord-cam-video", captura.fonte.clone());
        self.envia(json!({ "type": "camera-state", "active": true }));

        let ids = [self.id_camera.clone()];
        for par in self.pares.values_mut() {
            if let Ok(s) = par
                .pc
                .add_track(MediaStreamTrack::Video(video.clone()), &ids)
            {
                par.remetentes_camera.push(s);
            }
        }

        let dono = self.meu_id.clone();
        let mut saida = self.saida.clone();
        tokio::spawn(async move {
            use iced::futures::SinkExt as _;
            while let Some((w, h, rgba)) = rx_quadro.recv().await {
                let quadro = Quadro {
                    dono: dono.clone(),
                    de_tela: false,
                    largura: w,
                    altura: h,
                    rgba,
                };
                if saida.send(Evento::Quadro(quadro)).await.is_err() {
                    break;
                }
            }
        });

        self.camera = Some(Camera { captura, video });
        self.publica_meu_estado().await;
        Ok(())
    }

    async fn para_camera(&mut self) {
        let Some(c) = self.camera.take() else { return };
        self.envia(json!({ "type": "camera-state", "active": false }));
        for par in self.pares.values_mut() {
            for s in par.remetentes_camera.drain(..) {
                let _ = par.pc.remove_track(s);
            }
        }
        c.captura.para();
        self.emite(Evento::VideoFechou {
            dono: self.meu_id.clone(),
            de_tela: false,
        })
        .await;
        self.publica_meu_estado().await;
    }

    async fn compartilha_tela(
        &mut self,
        apps: Vec<String>,
        perfil: captura_tela::PerfilTela,
        auto_bitrate: bool,
    ) -> Result<()> {
        if self.tela.is_some() || !self.na_chamada {
            return Ok(());
        }
        // O diálogo do portal bloqueia até a pessoa escolher: fora do ator.
        // Sessão Wayland: portal (o diálogo do sistema). Sessão X11 pura: o
        // capturador lê o servidor X direto, sem diálogo.
        let fps = perfil.fps();
        let mut perfil = perfil;
        let x11 =
            std::env::var_os("WAYLAND_DISPLAY").is_none() && std::env::var_os("DISPLAY").is_some();
        let (tx_previa, mut rx_previa) = tokio::sync::mpsc::unbounded_channel();
        let captura = tokio::task::spawn_blocking(move || {
            captura_tela::inicia(fps, true, x11, Some(tx_previa))
        })
        .await?
        .ok_or_else(|| anyhow!("a captura de tela não começou (cancelada?)"))?;
        // Agora se sabe a altura do que está sendo capturado: o teto de
        // resolução e o de bitrate dependem dela.
        perfil.fonte_altura = captura.resolucao.height;
        let video = self
            .fabrica
            .create_video_track("concord-tela-video", captura.fonte.clone());

        let (audio, filho) = if apps.is_empty() {
            (None, None)
        } else {
            match inicia_audio_de_apps(&apps) {
                Ok((fonte, filho)) => (
                    Some(self.fabrica.create_audio_track("concord-tela-audio", fonte)),
                    Some(filho),
                ),
                Err(e) => {
                    self.emite(Evento::Aviso(format!("Transmitindo sem som: {e:#}")))
                        .await;
                    (None, None)
                }
            }
        };

        // Anuncia ANTES das trilhas entrarem: senão o outro lado classifica a
        // stream como voz.
        self.envia(json!({ "type": "screen-state", "streamId": self.id_tela, "active": true }));
        let ids = [self.id_tela.clone()];
        for par in self.pares.values_mut() {
            par.escada_bitrate.reinicia();
            par.ultimo_envio = None;
            if let Ok(s) = par
                .pc
                .add_track(MediaStreamTrack::Video(video.clone()), &ids)
            {
                configura_remetente_tela(&s, perfil, 0);
                par.remetentes_tela.push(s);
            }
            if let Some(a) = &audio
                && let Ok(s) = par.pc.add_track(MediaStreamTrack::Audio(a.clone()), &ids)
            {
                par.remetentes_tela.push(s);
            }
        }
        // A própria transmissão também aparece para quem transmite.
        let dono = self.meu_id.clone();
        let mut saida = self.saida.clone();
        tokio::spawn(async move {
            use iced::futures::SinkExt as _;
            while let Some((w, h, rgba)) = rx_previa.recv().await {
                let quadro = Quadro {
                    dono: dono.clone(),
                    de_tela: true,
                    largura: w,
                    altura: h,
                    rgba,
                };
                if saida.send(Evento::Quadro(quadro)).await.is_err() {
                    break;
                }
            }
        });
        self.tela = Some(Tela {
            captura,
            video,
            audio,
            perfil,
            auto_bitrate,
            _filho: filho,
        });
        self.publica_meu_estado().await;
        Ok(())
    }

    async fn para_tela(&mut self) {
        let Some(t) = self.tela.take() else { return };
        // Aviso primeiro: é ele que fecha a caixa do outro lado.
        self.envia(json!({ "type": "screen-state", "streamId": self.id_tela, "active": false }));
        for par in self.pares.values_mut() {
            par.escada_bitrate.reinicia();
            par.ultimo_envio = None;
            for s in par.remetentes_tela.drain(..) {
                let _ = par.pc.remove_track(s);
            }
        }
        t.captura.para();
        self.emite(Evento::VideoFechou {
            dono: self.meu_id.clone(),
            de_tela: true,
        })
        .await;
        self.publica_meu_estado().await;
    }

    async fn processa_qualidade_e_bitrate(&mut self) {
        let (perfil, auto_bitrate) = match &self.tela {
            Some(t) => (Some(t.perfil), t.auto_bitrate),
            None => (None, false),
        };

        let mut eventos_qualidade = Vec::new();

        for (id, par) in &mut self.pares {
            let estado_pc = par.pc.connection_state();
            if estado_pc == PeerConnectionState::Failed
                || estado_pc == PeerConnectionState::Disconnected
            {
                eventos_qualidade.push((
                    id.clone(),
                    QualidadeConexao {
                        nivel: NivelConexao::Ruim,
                        perda: None,
                        rtt_ms: None,
                        jitter_ms: None,
                        bitrate_kbps: None,
                        rota: None,
                    },
                ));
                continue;
            }

            let Ok(stats) = par.pc.get_stats().await else {
                continue;
            };

            let mut perdidos_inbound = 0u64;
            let mut recebidos_inbound = 0u64;
            let mut rtt_ms: Option<u64> = None;
            let mut jitter_ms: Option<u64> = None;
            let mut bitrate_kbps: Option<u64> = None;
            let mut id_candidato_remoto: Option<String> = None;
            let mut tipos_candidatos: std::collections::HashMap<
                String,
                libwebrtc::stats::IceCandidateType,
            > = std::collections::HashMap::new();
            let mut enviados = 0u64;
            let mut perdidos_no_envio = 0u64;
            let mut rtt_envio_ms: Option<u64> = None;

            for s in &stats {
                match s {
                    libwebrtc::stats::RtcStats::InboundRtp(inbound) => {
                        perdidos_inbound = perdidos_inbound
                            .saturating_add(inbound.received.packets_lost.max(0) as u64);
                        recebidos_inbound =
                            recebidos_inbound.saturating_add(inbound.received.packets_received);
                        if inbound.received.jitter > 0.0 && jitter_ms.is_none() {
                            jitter_ms = Some((inbound.received.jitter * 1000.0).round() as u64);
                        }
                    }
                    libwebrtc::stats::RtcStats::OutboundRtp(out) => {
                        enviados = enviados.saturating_add(out.sent.packets_sent);
                    }
                    libwebrtc::stats::RtcStats::RemoteInboundRtp(rem) => {
                        perdidos_no_envio = perdidos_no_envio
                            .saturating_add(rem.received.packets_lost.max(0) as u64);
                        if rem.remote_inbound.round_trip_time > 0.0 {
                            rtt_envio_ms =
                                Some((rem.remote_inbound.round_trip_time * 1000.0).round() as u64);
                        }
                    }
                    libwebrtc::stats::RtcStats::CandidatePair(cp)
                        if cp.candidate_pair.nominated
                            || cp.candidate_pair.current_round_trip_time > 0.0 =>
                    {
                        if rtt_ms.is_none() && cp.candidate_pair.current_round_trip_time > 0.0 {
                            rtt_ms =
                                Some((cp.candidate_pair.current_round_trip_time * 1000.0).round()
                                    as u64);
                        }
                        if cp.candidate_pair.available_outgoing_bitrate > 0.0
                            && bitrate_kbps.is_none()
                        {
                            bitrate_kbps = Some(
                                (cp.candidate_pair.available_outgoing_bitrate / 1000.0).round()
                                    as u64,
                            );
                        }
                        if id_candidato_remoto.is_none()
                            && !cp.candidate_pair.remote_candidate_id.is_empty()
                        {
                            id_candidato_remoto =
                                Some(cp.candidate_pair.remote_candidate_id.clone());
                        }
                    }
                    libwebrtc::stats::RtcStats::RemoteCandidate(rc) => {
                        if let Some(t) = rc.remote_candidate.candidate_type {
                            tipos_candidatos.insert(rc.rtc.id.clone(), t);
                        }
                    }
                    _ => {}
                }
            }

            let rota = id_candidato_remoto
                .and_then(|id_remota| tipos_candidatos.get(&id_remota).copied())
                .or_else(|| tipos_candidatos.values().next().copied())
                .map(|tipo| match tipo {
                    libwebrtc::stats::IceCandidateType::Relay => "via TURN (relay)".to_string(),
                    libwebrtc::stats::IceCandidateType::Host => {
                        "direta, mesma rede (host)".to_string()
                    }
                    libwebrtc::stats::IceCandidateType::Srflx
                    | libwebrtc::stats::IceCandidateType::Prflx => "direta (P2P)".to_string(),
                });

            let mut perda_inbound: Option<f64> = None;
            if let Some(ant) = par.ultimo_inbound {
                let d_perdidos = perdidos_inbound.saturating_sub(ant.perdidos);
                let d_recebidos = recebidos_inbound.saturating_sub(ant.recebidos);
                let total = d_perdidos + d_recebidos;
                if total > 0 {
                    perda_inbound = Some((d_perdidos as f64 / total as f64).clamp(0.0, 1.0));
                }
            }
            par.ultimo_inbound = Some(AmostraInbound {
                perdidos: perdidos_inbound,
                recebidos: recebidos_inbound,
            });

            let nivel_str = classifica_conexao(perda_inbound, rtt_ms);
            let nivel = match nivel_str {
                "boa" => NivelConexao::Boa,
                "media" => NivelConexao::Media,
                "ruim" => NivelConexao::Ruim,
                _ => NivelConexao::Indefinido,
            };

            eventos_qualidade.push((
                id.clone(),
                QualidadeConexao {
                    nivel,
                    perda: perda_inbound,
                    rtt_ms,
                    jitter_ms,
                    bitrate_kbps,
                    rota,
                },
            ));

            if auto_bitrate
                && let Some(perfil) = perfil
                && !par.remetentes_tela.is_empty()
            {
                let mut perda_envio: Option<f64> = None;
                if let Some(ant) = par.ultimo_envio {
                    let d_perdidos = perdidos_no_envio.saturating_sub(ant.perdidos);
                    let d_enviados = enviados.saturating_sub(ant.enviados);
                    if d_enviados > 0 {
                        perda_envio = Some((d_perdidos as f64 / d_enviados as f64).clamp(0.0, 1.0));
                    }
                }
                par.ultimo_envio = Some(AmostraEnvio {
                    enviados,
                    perdidos: perdidos_no_envio,
                });

                let nivel_envio = classifica_conexao(perda_envio, rtt_envio_ms.or(rtt_ms));
                if nivel_envio != "indefinido" && par.escada_bitrate.atualiza(nivel_envio) {
                    let novo_degrau = par.escada_bitrate.degrau;
                    log::info!(
                        "[{id}] escada de bitrate de tela ajustada para degrau {novo_degrau} ({nivel_envio}, perda={perda_envio:?}, rtt={rtt_ms:?})"
                    );
                    for sender in &par.remetentes_tela {
                        configura_remetente_tela(sender, perfil, novo_degrau);
                    }
                }
            }
        }

        for (id, qual) in eventos_qualidade {
            if let Some(p) = self.pessoas.get_mut(&id) {
                p.qualidade = Some(qual.clone());
            }
            self.emite(Evento::QualidadeConexao {
                id,
                qualidade: qual,
            })
            .await;
        }
    }

    // --- mensagens do servidor ---------------------------------------------

    async fn mensagem(&mut self, msg: Value) -> Result<()> {
        let tipo = msg["type"].as_str().unwrap_or("");
        let de = msg["from"].as_str().unwrap_or("").to_string();
        match tipo {
            "join-denied" => {
                let motivo = match msg["reason"].as_str() {
                    Some("nao-membro") => "você não faz parte desta conversa",
                    Some("sem-conversa") => "o app pediu uma sala em vez de uma conversa",
                    _ => "o servidor recusou a entrada",
                };
                bail!("RECUSADA:{motivo}");
            }
            "room-info" => {
                self.meu_id = msg["id"].as_str().unwrap_or("").to_string();
                self.meu_uid = msg["uid"].as_str().map(|s| s.to_string());
                self.sou_dono = msg["youAreOwner"].as_bool().unwrap_or(false);
                self.mudo_forcado = msg["forcedMute"].as_bool().unwrap_or(false);
                if self.mudo_forcado {
                    self.mudo = true;
                    self.aplica_mic();
                }
                self.pessoas.clear();
                for p in msg["peers"].as_array().cloned().unwrap_or_default() {
                    let id = p["id"].as_str().unwrap_or("").to_string();
                    let uid = p["uid"].as_str().map(|s| s.to_string());
                    let nome = p["name"].as_str().unwrap_or("?").to_string();
                    let vol_voz = volumes::obter(uid.as_deref(), &nome, false);
                    let vol_tela = volumes::obter(uid.as_deref(), &nome, true);
                    let p_mudo_forcado = p["forcedMute"].as_bool().unwrap_or(false);
                    self.pessoas.insert(
                        id.clone(),
                        Pessoa {
                            id,
                            uid,
                            nome,
                            eu: false,
                            espectador: p["espectador"].as_bool().unwrap_or(false),
                            mudo: p["muted"].as_bool().unwrap_or(false) || p_mudo_forcado,
                            surdo: p["deafened"].as_bool().unwrap_or(false),
                            apresentando: false,
                            com_camera: false,
                            conexao: None,
                            volume: vol_voz,
                            volume_tela: vol_tela,
                            volume_anterior: None,
                            volume_tela_anterior: None,
                            falando: false,
                            mudo_forcado: p_mudo_forcado,
                            dono: p["owner"].as_bool().unwrap_or(false),
                            qualidade: None,
                            avatar: p["avatar"].as_str().map(String::from),
                            cor: p["color"].as_str().map(String::from),
                            cor_nome: p["nameColor"].as_str().map(String::from),
                            efeito_nome: p["nameEffect"].as_str().map(String::from),
                            bio: p["bio"].as_str().map(String::from),
                            status: p["status"].as_str().map(String::from),
                            arroba: p["arroba"].as_str().map(String::from),
                            atividade: serde_json::from_value(p["atividade"].clone()).ok(),
                        },
                    );
                }
                self.emite(Evento::Entrou {
                    meu_id: self.meu_id.clone(),
                })
                .await;
                self.emite(Evento::InfoSala {
                    sou_dono: self.sou_dono,
                })
                .await;
                // Meu nome não vem no room-info; vem no eco do primeiro chat
                // ou do /api/me. Até lá, "você".
                self.pessoas.insert(
                    self.meu_id.clone(),
                    Pessoa {
                        id: self.meu_id.clone(),
                        uid: self.meu_uid.clone(),
                        nome: if self.meu_nome.is_empty() {
                            "você".into()
                        } else {
                            self.meu_nome.clone()
                        },
                        eu: true,
                        espectador: !self.na_chamada,
                        mudo: self.mudo || self.surdo || self.mudo_forcado,
                        surdo: self.surdo,
                        apresentando: self.tela.is_some(),
                        com_camera: self.camera.is_some(),
                        conexao: None,
                        volume: 1.0,
                        volume_tela: 1.0,
                        volume_anterior: None,
                        volume_tela_anterior: None,
                        falando: self.meu_falando,
                        mudo_forcado: self.mudo_forcado,
                        dono: self.sou_dono,
                        qualidade: None,
                        avatar: None,
                        cor: None,
                        cor_nome: None,
                        efeito_nome: None,
                        bio: None,
                        status: None,
                        arroba: msg["arroba"].as_str().map(String::from),
                        atividade: None,
                    },
                );
                self.publica_meu_estado().await;

                if self.na_chamada {
                    self.envia(json!({ "type": "mode-state", "espectador": false }));
                    let alvos: Vec<String> = self
                        .pessoas
                        .values()
                        .filter(|p| !p.eu && !p.espectador)
                        .map(|p| p.id.clone())
                        .collect();
                    for id in alvos {
                        let ofereco = self.meu_id.as_str() > id.as_str();
                        let _ = self.cria_par(&id, ofereco);
                    }
                }
                if self.mudo || self.surdo {
                    self.envia(json!({
                        "type": "mic-state",
                        "muted": self.mudo,
                        "viaDeafen": self.surdo,
                        "resync": true
                    }));
                }
                if self.surdo {
                    self.envia(json!({
                        "type": "deafen-state",
                        "deafened": true,
                        "resync": true
                    }));
                }
                if self.tela.is_some() {
                    self.envia(json!({
                        "type": "screen-state",
                        "streamId": self.id_tela,
                        "active": true
                    }));
                }
                if self.camera.is_some() {
                    self.envia(json!({
                        "type": "camera-state",
                        "active": true
                    }));
                }

                self.publica_meu_estado().await;
            }
            "peer-joined" => {
                let id = msg["id"].as_str().unwrap_or("").to_string();
                let uid = msg["uid"].as_str().map(|s| s.to_string());
                let nome = msg["name"].as_str().unwrap_or("?").to_string();
                let vol_voz = volumes::obter(uid.as_deref(), &nome, false);
                let vol_tela = volumes::obter(uid.as_deref(), &nome, true);
                let esp = msg["espectador"].as_bool().unwrap_or(false);
                self.pessoas.insert(
                    id.clone(),
                    Pessoa {
                        id: id.clone(),
                        uid,
                        nome,
                        eu: false,
                        espectador: esp,
                        mudo: msg["forcedMute"].as_bool().unwrap_or(false),
                        surdo: false,
                        apresentando: false,
                        com_camera: false,
                        conexao: None,
                        volume: vol_voz,
                        volume_tela: vol_tela,
                        volume_anterior: None,
                        volume_tela_anterior: None,
                        falando: false,
                        mudo_forcado: msg["forcedMute"].as_bool().unwrap_or(false),
                        dono: msg["owner"].as_bool().unwrap_or(false),
                        qualidade: None,
                        avatar: msg["avatar"].as_str().map(String::from),
                        cor: msg["color"].as_str().map(String::from),
                        cor_nome: msg["nameColor"].as_str().map(String::from),
                        efeito_nome: msg["nameEffect"].as_str().map(String::from),
                        bio: msg["bio"].as_str().map(String::from),
                        status: msg["status"].as_str().map(String::from),
                        arroba: msg["arroba"].as_str().map(String::from),
                        atividade: serde_json::from_value(msg["atividade"].clone()).ok(),
                    },
                );
                if self.tela.is_some() {
                    self.envia(
                        json!({ "type": "screen-state", "streamId": self.id_tela, "active": true }),
                    );
                }
                if self.camera.is_some() {
                    self.envia(json!({ "type": "camera-state", "active": true }));
                }
                if self.na_chamada && !esp {
                    self.cria_par(&id, false)?;
                }
                self.publica_pessoas().await;
            }
            "mode-state" => {
                let esp = msg["espectador"].as_bool().unwrap_or(false);
                if let Some(p) = self.pessoas.get_mut(&de) {
                    p.espectador = esp;
                    if esp {
                        p.apresentando = false;
                        p.conexao = None;
                    }
                }
                if esp {
                    if let Some(p) = self.pares.remove(&de) {
                        p.pc.close();
                    }
                    self.emite(Evento::VideoFechou {
                        dono: de.clone(),
                        de_tela: true,
                    })
                    .await;
                    self.emite(Evento::VideoFechou {
                        dono: de.clone(),
                        de_tela: false,
                    })
                    .await;
                } else if self.na_chamada && !self.pares.contains_key(&de) {
                    if self.tela.is_some() {
                        self.envia(json!({ "type": "screen-state", "streamId": self.id_tela, "active": true }));
                    }
                    let ofereco = self.meu_id.as_str() > de.as_str();
                    self.cria_par(&de, ofereco)?;
                }
                self.publica_pessoas().await;
            }
            "peer-left" => {
                let id = msg["id"].as_str().unwrap_or("").to_string();
                self.detector_eco.esquece_peer(&id);
                self.pessoas.remove(&id);
                if let Some(p) = self.pares.remove(&id) {
                    p.pc.close();
                }
                self.emite(Evento::VideoFechou {
                    dono: id.clone(),
                    de_tela: true,
                })
                .await;
                self.emite(Evento::VideoFechou {
                    dono: id,
                    de_tela: false,
                })
                .await;
                self.publica_pessoas().await;
            }
            "mic-state" => {
                if let Some(p) = self.pessoas.get_mut(&de) {
                    p.mudo = msg["muted"].as_bool().unwrap_or(false);
                }
                self.publica_pessoas().await;
            }
            "deafen-state" => {
                if let Some(p) = self.pessoas.get_mut(&de) {
                    p.surdo = msg["deafened"].as_bool().unwrap_or(false);
                }
                self.publica_pessoas().await;
            }
            "screen-state" => {
                let sid = msg["streamId"].as_str().unwrap_or("").to_string();
                let ativa = msg["active"].as_bool().unwrap_or(false);
                log::info!("[{de}] screen-state active={ativa} stream={sid}");
                if ativa {
                    self.telas_anunciadas.insert(sid);
                } else {
                    self.telas_anunciadas.remove(&sid);
                    self.emite(Evento::VideoFechou {
                        dono: de.clone(),
                        de_tela: true,
                    })
                    .await;
                }
                if let Some(p) = self.pessoas.get_mut(&de) {
                    p.apresentando = ativa;
                }
                self.publica_pessoas().await;
            }
            "camera-state" => {
                let ativa = msg["active"].as_bool().unwrap_or(false);
                if let Some(p) = self.pessoas.get_mut(&de) {
                    p.com_camera = ativa;
                }
                if !ativa {
                    self.emite(Evento::VideoFechou {
                        dono: de,
                        de_tela: false,
                    })
                    .await;
                }
                self.publica_pessoas().await;
            }
            "sistema" => {
                if let Some(m) = mensagem_de(&msg) {
                    self.emite(Evento::Chat(Box::new(m))).await;
                }
            }
            "chat" => {
                if let Some(m) = mensagem_de(&msg) {
                    if de == self.meu_id && self.meu_nome != m.autor {
                        self.meu_nome = m.autor.clone();
                        if let Some(eu) = self.pessoas.get_mut(&self.meu_id) {
                            eu.nome = m.autor.clone();
                        }
                        self.publica_pessoas().await;
                    }
                    self.emite(Evento::Chat(Box::new(m))).await;
                }
            }
            "chat-history" => {
                let lista = msg["messages"]
                    .as_array()
                    .map(|a| a.iter().filter_map(mensagem_de).collect())
                    .unwrap_or_default();
                self.emite(Evento::Historico(lista)).await;
            }
            "chat-edit" => {
                if let Some(id) = msg["id"].as_str() {
                    let texto = msg["text"].as_str().unwrap_or("").to_string();
                    let editada_em = msg["editedAt"]
                        .as_i64()
                        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
                    let mencoes = msg["mentions"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|x| x.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();
                    let mencoes_uids = msg["mentionUids"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|x| x.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();
                    self.emite(Evento::MensagemEditada {
                        id: id.to_string(),
                        texto,
                        editada_em,
                        mencoes,
                        mencoes_uids,
                    })
                    .await;
                }
            }
            "chat-delete" => {
                if let Some(id) = msg["id"].as_str() {
                    self.emite(Evento::MensagemApagada(id.to_string())).await;
                }
            }
            "chat-react" => {
                if let Some(id) = msg["id"].as_str() {
                    let reacoes: std::collections::HashMap<String, Vec<String>> = msg["reactions"]
                        .as_object()
                        .map(|obj| {
                            obj.iter()
                                .map(|(k, v)| {
                                    let quem = v
                                        .as_array()
                                        .map(|a| {
                                            a.iter()
                                                .filter_map(|x| x.as_str().map(String::from))
                                                .collect()
                                        })
                                        .unwrap_or_default();
                                    (k.clone(), quem)
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    self.emite(Evento::ReacoesAtualizadas {
                        id: id.to_string(),
                        reacoes,
                    })
                    .await;
                }
            }
            "chat-pin" => {
                let fixada = if msg["pinned"].is_object() {
                    let p = &msg["pinned"];
                    let id = p["id"].as_str().unwrap_or("").to_string();
                    let autor = p["name"].as_str().unwrap_or("").to_string();
                    let texto = p["text"].as_str().unwrap_or("").to_string();
                    let fixado_por = p["by"].as_str().unwrap_or("").to_string();
                    let ts = p["ts"].as_i64().unwrap_or(0);
                    if !id.is_empty() {
                        Some(MensagemFixada {
                            id,
                            autor,
                            texto,
                            fixado_por,
                            ts,
                        })
                    } else {
                        None
                    }
                } else {
                    None
                };
                self.emite(Evento::MensagemFixada(fixada)).await;
            }
            "voto" => {
                if let Some(id) = msg["id"].as_str() {
                    let votos: std::collections::HashMap<String, usize> = msg["votos"]
                        .as_object()
                        .map(|obj| {
                            obj.iter()
                                .filter_map(|(k, v)| {
                                    v.as_u64().map(|idx| (k.clone(), idx as usize))
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    self.emite(Evento::Voto {
                        id: id.to_string(),
                        votos,
                    })
                    .await;
                }
            }
            "typing" => {
                let de = msg["from"].as_str().unwrap_or("").to_string();
                let nome = msg["name"].as_str().unwrap_or("Alguém").to_string();
                let digitando = msg["typing"].as_bool().unwrap_or(false);
                if !de.is_empty() && de != self.meu_id {
                    self.emite(Evento::Digitando {
                        de,
                        nome,
                        digitando,
                    })
                    .await;
                }
            }
            "chat-recusado" => {
                let m = if msg["motivo"] == "imagens" {
                    "Muitas imagens seguidas — espere um minuto antes de mandar outra."
                } else {
                    "Devagar: muita mensagem em pouco tempo. Tente de novo em alguns segundos."
                };
                self.emite(Evento::Aviso(m.into())).await;
            }
            "offer" | "answer" => self.sdp(&de, tipo, &msg["sdp"]).await?,
            "ice-candidate" => self.candidato_remoto(&de, &msg["candidate"]).await?,
            "echo-warning" => {
                log::info!("[eco] aviso de eco recebido do servidor (emitido por {de})");
                self.emite(Evento::AvisoDeEco).await;
            }
            "force-mute" => {
                let id = msg["id"].as_str().unwrap_or("");
                let silenciar = msg["muted"].as_bool().unwrap_or(false);
                if id == self.meu_id {
                    self.mudo_forcado = silenciar;
                    if silenciar {
                        self.mudo = true;
                        self.aplica_mic();
                    }
                    self.publica_meu_estado().await;
                } else if let Some(p) = self.pessoas.get_mut(id) {
                    p.mudo_forcado = silenciar;
                    if silenciar {
                        p.mudo = true;
                    }
                    self.publica_pessoas().await;
                }
            }
            "profile" => {
                let id = msg["from"].as_str().unwrap_or(&de);
                if let Some(p) = self.pessoas.get_mut(id) {
                    p.cor = msg["color"].as_str().map(String::from);
                    p.cor_nome = msg["nameColor"].as_str().map(String::from);
                    p.efeito_nome = msg["nameEffect"].as_str().map(String::from);
                }
                self.publica_pessoas().await;
            }
            "status-state" => {
                let id = msg["from"].as_str().unwrap_or(&de);
                let status = msg["status"].as_str().map(String::from);
                if let Some(p) = self.pessoas.get_mut(id) {
                    p.status = status;
                }
                self.publica_pessoas().await;
            }
            "activity-state" => {
                let id = msg["from"].as_str().unwrap_or(&de);
                let atv: Option<crate::api::Atividade> =
                    serde_json::from_value(msg["atividade"].clone()).ok();
                if let Some(p) = self.pessoas.get_mut(id) {
                    p.atividade = atv;
                }
                self.publica_pessoas().await;
            }
            "kicked" => {
                let motivo = if msg["motivo"].as_str() == Some("removido") {
                    "você foi removido deste grupo".to_string()
                } else {
                    "você foi desconectado desta conversa".to_string()
                };
                bail!("RECUSADA:{motivo}");
            }
            "reacao" => {
                let de_nome = msg["name"].as_str().unwrap_or("Alguém").to_string();
                let emoji = msg["emoji"].as_str().unwrap_or("").to_string();
                if !emoji.is_empty() {
                    self.emite(Evento::ReacaoFlutuante { de: de_nome, emoji })
                        .await;
                }
            }
            "wordle-grupo" => {
                if let Some(dia) = msg["dia"].as_str() {
                    self.emite(Evento::WordleGrupoMudou(dia.to_string())).await;
                }
            }
            "cutucada" => {
                let de_nome = msg["name"].as_str().unwrap_or("Alguém").to_string();
                self.emite(Evento::CutucadaRecebida { de_nome }).await;
            }
            "cutucada-enviada" => {
                let para_nome = msg["name"].as_str().unwrap_or("Alguém").to_string();
                self.emite(Evento::CutucadaEnviada { para_nome }).await;
            }
            "som" => {
                let id = msg["id"].as_str().unwrap_or("").to_string();
                let de_nome = msg["name"].as_str().unwrap_or("Alguém").to_string();
                let de_id = msg["from"].as_str().unwrap_or(&de).to_string();
                let vol = msg["vol"].as_u64().unwrap_or(40).min(100) as u8;
                if !id.is_empty() {
                    self.emite(Evento::SomRecebido {
                        de_id,
                        de_nome,
                        id,
                        vol,
                    })
                    .await;
                }
            }
            "quadro-estado" => {
                let tracos: Vec<crate::ui::quadro::Traco> =
                    serde_json::from_value(msg["tracos"].clone()).unwrap_or_default();
                self.emite(Evento::QuadroEstado(tracos)).await;
            }
            "quadro" => {
                if let Ok(traco) =
                    serde_json::from_value::<crate::ui::quadro::Traco>(msg["traco"].clone())
                {
                    self.emite(Evento::QuadroTraco(traco)).await;
                }
            }
            "quadro-limpar" => {
                let quem = msg["name"].as_str().unwrap_or("Alguém").to_string();
                self.emite(Evento::QuadroLimpo { quem }).await;
            }
            "roletas" => {
                let roletas: Vec<crate::ui::roleta::Roleta> =
                    serde_json::from_value(msg["roletas"].clone()).unwrap_or_default();
                self.emite(Evento::Roletas(roletas)).await;
            }
            "roleta-resultado" => {
                let id = msg["id"].as_str().unwrap_or("").to_string();
                let indice = msg["indice"].as_u64().unwrap_or(0) as usize;
                let por = msg["por"].as_str().map(String::from);
                if !id.is_empty() {
                    self.emite(Evento::RoletaResultado { id, indice, por })
                        .await;
                }
            }
            _ => {}
        }
        Ok(())
    }

    // --- WebRTC --------------------------------------------------------------

    fn cria_par(&mut self, id: &str, iniciador: bool) -> Result<()> {
        if self.pares.contains_key(id) {
            return Ok(());
        }
        log::info!("[{id}] criando conexão (iniciador={iniciador})");
        let pc = self
            .fabrica
            .create_peer_connection(self.rtc.clone())
            .map_err(|e| anyhow!("create_peer_connection: {e:?}"))?;
        let (alvo, tx) = (id.to_string(), self.tx.clone());
        pc.on_ice_candidate(Some(Box::new(move |c| {
            let _ = tx.send(Interno::Candidato(alvo.clone(), c));
        })));
        let (alvo, tx) = (id.to_string(), self.tx.clone());
        pc.on_connection_state_change(Some(Box::new(move |s| {
            let _ = tx.send(Interno::Estado(alvo.clone(), s));
        })));
        let (alvo, tx) = (id.to_string(), self.tx.clone());
        pc.on_track(Some(Box::new(move |t| {
            let _ = tx.send(Interno::Trilha(alvo.clone(), t));
        })));
        let mut remetentes_tela = Vec::new();
        let mut remetentes_camera = Vec::new();
        if let Some(m) = &self.mic {
            pc.add_track(MediaStreamTrack::Audio(m.clone()), &[self.id_voz.as_str()])
                .map_err(|e| anyhow!("add_track mic: {e:?}"))?;
        }
        if let Some(c) = &self.camera {
            let ids = [self.id_camera.as_str()];
            if let Ok(s) = pc.add_track(MediaStreamTrack::Video(c.video.clone()), &ids) {
                remetentes_camera.push(s);
            }
        }
        if let Some(t) = &self.tela {
            let ids = [self.id_tela.as_str()];
            if let Ok(s) = pc.add_track(MediaStreamTrack::Video(t.video.clone()), &ids) {
                configura_remetente_tela(&s, t.perfil, 0);
                remetentes_tela.push(s);
            }
            if let Some(a) = &t.audio
                && let Ok(s) = pc.add_track(MediaStreamTrack::Audio(a.clone()), &ids)
            {
                remetentes_tela.push(s);
            }
        }
        let (alvo, tx) = (id.to_string(), self.tx.clone());
        pc.on_negotiation_needed(Some(Box::new(move |_| {
            let _ = tx.send(Interno::Negociar(alvo.clone()));
        })));
        let precisa_renegociar = !iniciador && (self.camera.is_some() || self.tela.is_some());
        self.pares.insert(
            id.to_string(),
            Par {
                pc,
                educado: !iniciador,
                renegociar: precisa_renegociar,
                pendentes: Vec::new(),
                remetentes_tela,
                remetentes_camera,
                trilhas_voz: Vec::new(),
                trilhas_tela: Vec::new(),
                monitores_audio: Vec::new(),
                escada_bitrate: EscadaBitrate::default(),
                ultimo_envio: None,
                ultimo_inbound: None,
            },
        );
        if iniciador {
            let _ = self.tx.send(Interno::Negociar(id.to_string()));
        }
        Ok(())
    }

    fn trilha_remota(&mut self, id: &str, t: TrackEvent) {
        let de_tela = t.streams.iter().any(|s| {
            let sid = s.id();
            sid.starts_with("lincast-tela-") || self.telas_anunciadas.contains(&sid)
        });
        log::info!(
            "[{id}] trilha recebida: {} (de_tela={de_tela})",
            match &t.track {
                MediaStreamTrack::Video(_) => "vídeo",
                MediaStreamTrack::Audio(_) => "áudio",
            }
        );
        match t.track {
            MediaStreamTrack::Video(video) => {
                let dono = id.to_string();
                let mut saida = self.saida.clone();
                tokio::spawn(async move {
                    use iced::futures::SinkExt as _;
                    let mut fluxo = NativeVideoStream::new(video);
                    // A interface não precisa de mais que ~30 quadros/s, e cada quadro
                    // em RGBA é grande: converter só o que vai ser mostrado.
                    let mut ultimo = std::time::Instant::now() - Duration::from_secs(1);
                    let mut primeiro_quadro = false;
                    while let Some(q) = fluxo.next().await {
                        if ultimo.elapsed() < Duration::from_millis(33) {
                            continue;
                        }
                        ultimo = std::time::Instant::now();
                        let (w, h) = (q.buffer.width(), q.buffer.height());
                        if w == 0 || h == 0 {
                            continue;
                        }
                        if !primeiro_quadro {
                            primeiro_quadro = true;
                            log::info!(
                                "[{dono}] primeiro quadro de vídeo: {w}x{h} (de_tela={de_tela})"
                            );
                        }
                        let mut rgba = vec![0u8; (w * h * 4) as usize];
                        q.buffer.to_argb(
                            VideoFormatType::ABGR,
                            &mut rgba,
                            w * 4,
                            w as i32,
                            h as i32,
                        );
                        let quadro = Quadro {
                            dono: dono.clone(),
                            de_tela,
                            largura: w,
                            altura: h,
                            rgba,
                        };
                        if saida.send(Evento::Quadro(quadro)).await.is_err() {
                            break;
                        }
                    }
                });
            }
            MediaStreamTrack::Audio(audio) => {
                if let Some(par) = self.pares.get_mut(id) {
                    if let Some(p) = self.pessoas.get(id) {
                        let ativo = if de_tela {
                            p.volume_tela > 0.0
                        } else {
                            p.volume > 0.0
                        };
                        audio.set_enabled(ativo);
                    }
                    if de_tela {
                        par.trilhas_tela.push(audio);
                    } else {
                        par.trilhas_voz.push(audio.clone());
                        let stream = libwebrtc::audio_stream::native::NativeAudioStream::new(
                            audio, 48000, 1,
                        );
                        let tx = self.tx.clone();
                        let alvo = id.to_string();
                        let monitor = tokio::spawn(async move {
                            use futures_util::StreamExt;
                            let mut stream = stream;
                            let mut ultimo = tokio::time::Instant::now();
                            let mut acumulado: f32 = 0.0;
                            let mut estado_anterior = false;
                            let mut ultimo_som = tokio::time::Instant::now();
                            while let Some(frame) = stream.next().await {
                                let mut max_abs: f32 = 0.0;
                                for &s in frame.data.iter() {
                                    let v = (s as f32).abs() / 32768.0;
                                    if v > max_abs {
                                        max_abs = v;
                                    }
                                }
                                if max_abs > acumulado {
                                    acumulado = max_abs;
                                }
                                if ultimo.elapsed() >= std::time::Duration::from_millis(100) {
                                    let nivel = (acumulado.sqrt() * 100.0).clamp(0.0, 100.0);
                                    acumulado = 0.0;
                                    ultimo = tokio::time::Instant::now();
                                    if nivel > 12.0 {
                                        ultimo_som = tokio::time::Instant::now();
                                        if !estado_anterior {
                                            estado_anterior = true;
                                            let _ = tx.send(Interno::Falando(alvo.clone(), true));
                                        }
                                    } else if estado_anterior
                                        && ultimo_som.elapsed()
                                            >= std::time::Duration::from_millis(250)
                                    {
                                        estado_anterior = false;
                                        let _ = tx.send(Interno::Falando(alvo.clone(), false));
                                    }
                                    let _ = tx.send(Interno::NivelRemoto(alvo.clone(), nivel));
                                }
                            }
                            let _ = tx.send(Interno::Falando(alvo, false));
                        });
                        par.monitores_audio.push(monitor);
                    }
                }
            }
        }
    }

    async fn oferece(&mut self, id: &str) -> Result<()> {
        let Some(par) = self.pares.get_mut(id) else {
            return Ok(());
        };
        if par.educado && par.pc.current_remote_description().is_none() {
            par.renegociar = true;
            return Ok(());
        }
        if par.pc.signaling_state() != SignalingState::Stable {
            par.renegociar = true;
            return Ok(());
        }
        par.renegociar = false;
        let pc = par.pc.clone();
        let oferta = pc
            // `OfferOptions::default()` manda offerToReceive* = false, e no
            // WebRTC isso ainda tem o efeito legado de rebaixar as linhas
            // existentes pra "só envia": o microfone saía e nada chegava.
            .create_offer(OfferOptions {
                offer_to_receive_audio: true,
                offer_to_receive_video: true,
                ..Default::default()
            })
            .await
            .map_err(|e| anyhow!("[{id}] create_offer: {e:?}"))?;
        pc.set_local_description(oferta.clone())
            .await
            .map_err(|e| anyhow!("[{id}] set_local(offer): {e:?}"))?;
        log::info!("[{id}] oferta enviada: {}", resumo_sdp(&oferta.to_string()));
        self.envia(json!({ "type": "offer", "to": id, "sdp": { "type": "offer", "sdp": oferta.to_string() } }));
        Ok(())
    }

    async fn sdp(&mut self, id: &str, tipo: &str, sdp: &Value) -> Result<()> {
        if !self.na_chamada {
            return Ok(());
        }
        if !self.pares.contains_key(id) {
            self.cria_par(id, false)?;
        }
        let texto = sdp["sdp"].as_str().context("sdp sem texto")?;
        log::info!("[{id}] {tipo} recebida: {}", resumo_sdp(texto));
        let (pc, educado) = {
            let par = &self.pares[id];
            (par.pc.clone(), par.educado)
        };
        let colisao = tipo == "offer" && pc.signaling_state() != SignalingState::Stable;
        if colisao && !educado {
            return Ok(());
        }
        if colisao {
            let rb = SessionDescription::parse("", SdpType::Rollback)
                .map_err(|e| anyhow!("rollback: {e:?}"))?;
            pc.set_local_description(rb)
                .await
                .map_err(|e| anyhow!("[{id}] rollback: {e:?}"))?;
        }
        let tipo_sdp = if tipo == "offer" {
            SdpType::Offer
        } else {
            SdpType::Answer
        };
        let desc = SessionDescription::parse(texto, tipo_sdp)
            .map_err(|e| anyhow!("[{id}] SDP inválido: {e:?}"))?;
        pc.set_remote_description(desc)
            .await
            .map_err(|e| anyhow!("[{id}] set_remote({tipo}): {e:?}"))?;
        let pendentes = self
            .pares
            .get_mut(id)
            .map(|p| std::mem::take(&mut p.pendentes))
            .unwrap_or_default();
        for c in pendentes {
            let _ = pc.add_ice_candidate(c).await;
        }
        if tipo == "offer" {
            let resposta = pc
                .create_answer(AnswerOptions::default())
                .await
                .map_err(|e| anyhow!("[{id}] create_answer: {e:?}"))?;
            pc.set_local_description(resposta.clone())
                .await
                .map_err(|e| anyhow!("[{id}] set_local(answer): {e:?}"))?;
            log::info!(
                "[{id}] resposta enviada: {}",
                resumo_sdp(&resposta.to_string())
            );
            self.envia(json!({ "type": "answer", "to": id, "sdp": { "type": "answer", "sdp": resposta.to_string() } }));
        }
        if self.pares.get(id).is_some_and(|p| p.renegociar) {
            let _ = self.tx.send(Interno::Negociar(id.to_string()));
        }
        Ok(())
    }

    async fn candidato_remoto(&mut self, id: &str, c: &Value) -> Result<()> {
        let Some(par) = self.pares.get_mut(id) else {
            return Ok(());
        };
        let cand = c["candidate"].as_str().unwrap_or("");
        if cand.is_empty() {
            return Ok(());
        }
        let candidato = IceCandidate::parse(
            c["sdpMid"].as_str().unwrap_or("0"),
            c["sdpMLineIndex"].as_i64().unwrap_or(0) as i32,
            cand,
        )
        .map_err(|e| anyhow!("[{id}] candidato inválido: {e:?}"))?;
        if par.pc.current_remote_description().is_none() {
            par.pendentes.push(candidato);
            return Ok(());
        }
        let pc = par.pc.clone();
        let _ = pc.add_ice_candidate(candidato).await;
        Ok(())
    }
}

// --- áudio dos apps (processo concord-audio) --------------------------------

const TAXA: u32 = 48_000;
const CANAIS: u32 = 2;

fn inicia_audio_de_apps(apps: &[String]) -> Result<(NativeAudioSource, tokio::process::Child)> {
    let Some(bin) = crate::portal_tela::acha_concord_audio() else {
        bail!("o som da transmissão precisa do concord-audio, que não foi achado: reinstale o app");
    };
    let mut args = vec!["capturar".to_string()];
    if apps.iter().any(|a| a == "*") {
        args.push("--todos".into());
    } else {
        for a in apps {
            args.push("--app".into());
            args.push(a.clone());
        }
    }
    args.push("--excluir-pid".into());
    args.push(std::process::id().to_string());
    let mut filho = tokio::process::Command::new(bin)
        .args(&args)
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let mut stdout = filho.stdout.take().context("stdout do concord-audio")?;
    // Áudio de tela com os processamentos de voz DESLIGADOS (destroem música).
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
    let destino = fonte.clone();
    tokio::spawn(async move {
        let mut bytes = vec![0u8; (TAXA / 100 * CANAIS * 2) as usize];
        let mut quadro = AudioFrame::new(TAXA, CANAIS, TAXA / 100);
        while stdout.read_exact(&mut bytes).await.is_ok() {
            for (s, b) in quadro
                .data
                .to_mut()
                .iter_mut()
                .zip(bytes.as_chunks::<2>().0)
            {
                *s = i16::from_le_bytes(*b);
            }
            let _ = destino.capture_frame(&quadro).await;
        }
        log::warn!("[audio] o concord-audio parou");
    });
    Ok((fonte, filho))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escada_bitrate_fracoes_e_piso() {
        let escada = EscadaBitrate::default();
        // Degrau 0: 100%
        assert_eq!(escada.bitrate_atual(2_500_000), 2_500_000);
        assert_eq!(escada.bitrate_atual(1_200_000), 1_200_000);
        assert_eq!(escada.bitrate_atual(4_000_000), 4_000_000);

        // Degrau 1: 50%
        let escada = EscadaBitrate {
            degrau: 1,
            amostras_boas: 0,
        };
        assert_eq!(escada.bitrate_atual(2_500_000), 1_250_000);

        // Degrau 2: 25%
        let escada = EscadaBitrate {
            degrau: 2,
            amostras_boas: 0,
        };
        assert_eq!(escada.bitrate_atual(2_500_000), 625_000);

        // Degrau 3: 12% (ou piso mínimo de 200 kbps)
        let escada = EscadaBitrate {
            degrau: 3,
            amostras_boas: 0,
        };
        assert_eq!(escada.bitrate_atual(2_500_000), 300_000);
        // Teto de 1.2M com 12% daria 144_000, mas o piso é 200_000
        assert_eq!(escada.bitrate_atual(1_200_000), BITRATE_MINIMO);
    }

    #[test]
    fn escada_bitrate_maquina_de_estados() {
        let mut escada = EscadaBitrate::default();
        assert_eq!(escada.degrau, 0);

        // Conexão ruim desce imediatamente
        assert!(escada.atualiza("ruim"));
        assert_eq!(escada.degrau, 1);
        assert_eq!(escada.amostras_boas, 0);

        // Outra amostra ruim desce mais um
        assert!(escada.atualiza("ruim"));
        assert_eq!(escada.degrau, 2);

        // Amostra média segura onde está
        assert!(!escada.atualiza("media"));
        assert_eq!(escada.degrau, 2);
        assert_eq!(escada.amostras_boas, 0);

        // Amostras boas: só sobe no 4º ciclo consecutivo
        assert!(!escada.atualiza("boa"));
        assert_eq!(escada.degrau, 2);
        assert_eq!(escada.amostras_boas, 1);

        assert!(!escada.atualiza("boa"));
        assert_eq!(escada.amostras_boas, 2);

        assert!(!escada.atualiza("boa"));
        assert_eq!(escada.amostras_boas, 3);

        // 4ª amostra boa: sobe um degrau
        assert!(escada.atualiza("boa"));
        assert_eq!(escada.degrau, 1);
        assert_eq!(escada.amostras_boas, 0);

        // Ruim desce logo de volta
        assert!(escada.atualiza("ruim"));
        assert_eq!(escada.degrau, 2);

        // Desce até o limite 3 e não passa dele
        assert!(escada.atualiza("ruim"));
        assert_eq!(escada.degrau, 3);
        assert!(!escada.atualiza("ruim"));
        assert_eq!(escada.degrau, 3);

        // Reiniciar zera tudo
        escada.reinicia();
        assert_eq!(escada.degrau, 0);
        assert_eq!(escada.amostras_boas, 0);
    }

    #[test]
    fn classificacao_de_conexao() {
        assert_eq!(classifica_conexao(None, None), "indefinido");
        assert_eq!(classifica_conexao(Some(0.005), Some(50)), "boa");
        assert_eq!(classifica_conexao(Some(0.03), Some(60)), "media");
        assert_eq!(classifica_conexao(Some(0.01), Some(200)), "media");
        assert_eq!(classifica_conexao(Some(0.09), Some(50)), "ruim");
        assert_eq!(classifica_conexao(Some(0.01), Some(400)), "ruim");
    }
}
