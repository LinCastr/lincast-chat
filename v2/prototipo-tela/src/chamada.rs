//! Conversa com o servidor da v1 e mantém uma conexão WebRTC com cada pessoa
//! que está na chamada.
//!
//! O protocolo é exatamente o do `public/index.html` (ver `handleSignal` lá),
//! de propósito: assim o protótipo entra numa sala normal e quem assiste usa o
//! app de sempre, no navegador. As regras que importam, copiadas de lá:
//!
//! - Só existe conexão entre duas pessoas quando as DUAS estão na chamada
//!   (espectador não conta).
//! - Quem chega oferece (`room-info`); quem já estava espera (`peer-joined`);
//!   quando alguém sai do modo espectador (`mode-state`), quem oferece é
//!   decidido comparando ids — a mesma conta dos dois lados.
//! - Negociação perfeita: o lado "educado" desfaz a própria oferta numa
//!   colisão; o outro ignora a oferta que chegou.
//! - Candidato ICE que chega antes da descrição remota vai pra fila.
//! - A tela é anunciada (`screen-state`) ANTES de qualquer SDP sair, senão o
//!   outro lado classifica a stream como voz.
//!
//! Tudo roda numa tarefa só (um "ator"): cada mensagem do servidor e cada
//! evento das conexões entra numa fila e é tratado até o fim antes do próximo.
//! Isso dá de graça a serialização por conexão que a v1 teve que montar na
//! mão (`queueSignaling`) depois de apanhar de `InvalidStateError`.

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use futures_util::{SinkExt, StreamExt};
use libwebrtc::audio_source::native::NativeAudioSource;
use libwebrtc::ice_candidate::IceCandidate;
use libwebrtc::media_stream_track::MediaStreamTrack;
use libwebrtc::peer_connection::{OfferOptions, PeerConnection, PeerConnectionState};
use libwebrtc::peer_connection_factory::native::PeerConnectionFactoryExt;
use libwebrtc::peer_connection_factory::{IceServer, PeerConnectionFactory, RtcConfiguration};
use libwebrtc::prelude::{AnswerOptions, SdpType, SessionDescription, SignalingState};
use libwebrtc::video_source::native::NativeVideoSource;
use libwebrtc::{audio_track::RtcAudioTrack, video_track::RtcVideoTrack};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

pub struct Config {
    pub servidor: String,
    pub sala: String,
    pub senha: String,
}

enum Evento {
    Servidor(Value),
    ServidorCaiu,
    Negociar(String),
    Candidato(String, IceCandidate),
    Estado(String, PeerConnectionState),
    Encerrar,
}

struct Par {
    pc: PeerConnection,
    educado: bool,
    pendentes: Vec<IceCandidate>,
}

pub struct Midia {
    pub video: Option<NativeVideoSource>,
    pub audio: Option<NativeAudioSource>,
}

struct Ator {
    fabrica: PeerConnectionFactory,
    rtc: RtcConfiguration,
    trilha_video: Option<RtcVideoTrack>,
    trilha_audio: Option<RtcAudioTrack>,
    id_stream: String,
    meu_id: String,
    espectadores: HashMap<String, bool>,
    pares: HashMap<String, Par>,
    tx_ws: mpsc::UnboundedSender<Value>,
    tx_ev: mpsc::UnboundedSender<Evento>,
}

/// Converte `https://host:porta` em `wss://host:porta` (e http → ws).
fn url_do_ws(servidor: &str) -> String {
    let s = servidor.trim_end_matches('/');
    if let Some(r) = s.strip_prefix("https://") {
        format!("wss://{r}")
    } else if let Some(r) = s.strip_prefix("http://") {
        format!("ws://{r}")
    } else {
        s.to_string()
    }
}

/// Os mesmos servidores STUN/TURN que a página usa, vindos de
/// `/api/ice-config` (que já resolve o hostname do coturn pra IP).
fn busca_ice(servidor: &str) -> Vec<IceServer> {
    let url = format!("{}/api/ice-config", servidor.trim_end_matches('/'));
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
        Ok(l) if !l.is_empty() => {
            let todas: Vec<_> = l.iter().flat_map(|s| s.urls.clone()).collect();
            log::info!("[ice] do servidor: {}", todas.join(", "));
            l
        }
        Ok(_) | Err(_) => {
            log::warn!("[ice] não consegui ler {url}; usando só STUN do Google");
            vec![IceServer {
                urls: vec!["stun:stun.l.google.com:19302".into()],
                username: String::new(),
                password: String::new(),
            }]
        }
    }
}

pub async fn roda(cfg: Config, midia: Midia) -> Result<()> {
    let servidor = cfg.servidor.clone();
    let ice = tokio::task::spawn_blocking(move || busca_ice(&servidor)).await?;
    let mut rtc = RtcConfiguration::default();
    rtc.ice_servers = ice;

    let fabrica = PeerConnectionFactory::default();
    let trilha_video = midia
        .video
        .map(|v| fabrica.create_video_track("concord-tela-video", v));
    let trilha_audio = midia
        .audio
        .map(|a| fabrica.create_audio_track("concord-tela-audio", a));

    let url = url_do_ws(&cfg.servidor);
    log::info!("[ws] conectando em {url}");
    let (ws, _) = tokio_tungstenite::connect_async(&url)
        .await
        .with_context(|| format!("não consegui abrir o WebSocket em {url}"))?;
    let (mut ws_tx, mut ws_rx) = ws.split();

    let (tx_ws, mut rx_ws) = mpsc::unbounded_channel::<Value>();
    let (tx_ev, mut rx_ev) = mpsc::unbounded_channel::<Evento>();

    tokio::spawn(async move {
        while let Some(v) = rx_ws.recv().await {
            if ws_tx
                .send(Message::Text(v.to_string().into()))
                .await
                .is_err()
            {
                break;
            }
        }
    });
    {
        let tx_ev = tx_ev.clone();
        tokio::spawn(async move {
            while let Some(m) = ws_rx.next().await {
                match m {
                    Ok(Message::Text(t)) => {
                        if let Ok(v) = serde_json::from_str::<Value>(&t) {
                            let _ = tx_ev.send(Evento::Servidor(v));
                        }
                    }
                    Ok(Message::Close(_)) | Err(_) => break,
                    _ => {}
                }
            }
            let _ = tx_ev.send(Evento::ServidorCaiu);
        });
    }
    {
        let tx_ev = tx_ev.clone();
        tokio::spawn(async move {
            let _ = tokio::signal::ctrl_c().await;
            let _ = tx_ev.send(Evento::Encerrar);
        });
    }

    // `espectador: false` porque o protótipo entra direto na chamada.
    tx_ws.send(json!({
        "type": "join",
        "room": cfg.sala,
        "password": cfg.senha,
        "espectador": false,
    }))?;

    let mut ator = Ator {
        fabrica,
        rtc,
        trilha_video,
        trilha_audio,
        id_stream: format!("concord-tela-{:08x}", rand_u32()),
        meu_id: String::new(),
        espectadores: HashMap::new(),
        pares: HashMap::new(),
        tx_ws,
        tx_ev,
    };

    let mut relatorio = tokio::time::interval(Duration::from_secs(5));
    relatorio.tick().await;
    loop {
        tokio::select! {
            ev = rx_ev.recv() => {
                let Some(ev) = ev else { break };
                match ev {
                    Evento::Encerrar => {
                        log::info!("encerrando: avisando a sala que a tela parou");
                        ator.anuncia_tela(false);
                        tokio::time::sleep(Duration::from_millis(300)).await;
                        for (_, p) in ator.pares.drain() { p.pc.close(); }
                        return Ok(());
                    }
                    Evento::ServidorCaiu => bail!("o servidor fechou a conexão"),
                    outro => {
                        if let Err(e) = ator.trata(outro).await {
                            log::warn!("{e:#}");
                        }
                    }
                }
            }
            _ = relatorio.tick() => ator.relata().await,
        }
    }
    Ok(())
}

fn rand_u32() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    n ^ std::process::id().rotate_left(16)
}

impl Ator {
    fn envia(&self, v: Value) {
        let _ = self.tx_ws.send(v);
    }

    fn anuncia_tela(&self, ativa: bool) {
        self.envia(json!({ "type": "screen-state", "streamId": self.id_stream, "active": ativa }));
    }

    async fn trata(&mut self, ev: Evento) -> Result<()> {
        match ev {
            Evento::Servidor(msg) => self.mensagem(msg).await,
            Evento::Negociar(id) => self.oferece(&id).await,
            Evento::Candidato(id, c) => {
                self.envia(json!({
                    "type": "ice-candidate",
                    "to": id,
                    "candidate": {
                        "candidate": c.candidate(),
                        "sdpMid": c.sdp_mid(),
                        "sdpMLineIndex": c.sdp_mline_index(),
                    }
                }));
                Ok(())
            }
            Evento::Estado(id, estado) => {
                log::info!("[{id}] conexão: {estado:?}");
                if estado == PeerConnectionState::Failed
                    && let Some(p) = self.pares.get(&id)
                {
                    log::warn!("[{id}] falhou; reiniciando ICE");
                    p.pc.restart_ice();
                }
                Ok(())
            }
            Evento::ServidorCaiu | Evento::Encerrar => Ok(()),
        }
    }

    async fn mensagem(&mut self, msg: Value) -> Result<()> {
        let tipo = msg["type"].as_str().unwrap_or("");
        let de = msg["from"].as_str().unwrap_or("").to_string();
        match tipo {
            "join-denied" => bail!("o servidor recusou a entrada: {}", msg["reason"]),
            "room-info" => {
                self.meu_id = msg["id"].as_str().unwrap_or("").to_string();
                log::info!("entrei na sala como {} (id {})", "Anônimo", self.meu_id);
                // Anuncia a tela antes de qualquer oferta sair.
                self.anuncia_tela(true);
                let mut n = 0;
                for p in msg["peers"].as_array().cloned().unwrap_or_default() {
                    let id = p["id"].as_str().unwrap_or("").to_string();
                    let esp = p["espectador"].as_bool().unwrap_or(false);
                    self.espectadores.insert(id.clone(), esp);
                    if esp {
                        continue;
                    }
                    self.cria_par(&id, true)?;
                    n += 1;
                }
                log::info!(
                    "{} na sala, {n} na chamada. Quem estiver só no chat precisa clicar em \"Entrar na chamada\" pra ver a tela.",
                    self.espectadores.len()
                );
            }
            "peer-joined" => {
                let id = msg["id"].as_str().unwrap_or("").to_string();
                let esp = msg["espectador"].as_bool().unwrap_or(false);
                self.espectadores.insert(id.clone(), esp);
                log::info!(
                    "[{id}] entrou na sala{}",
                    if esp { " (só no chat)" } else { "" }
                );
                self.anuncia_tela(true);
                if !esp {
                    self.cria_par(&id, false)?;
                }
            }
            "mode-state" => {
                let esp = msg["espectador"].as_bool().unwrap_or(false);
                self.espectadores.insert(de.clone(), esp);
                if esp {
                    log::info!("[{de}] saiu da chamada (continua no chat)");
                    if let Some(p) = self.pares.remove(&de) {
                        p.pc.close();
                    }
                } else if !self.pares.contains_key(&de) {
                    log::info!("[{de}] entrou na chamada");
                    self.anuncia_tela(true);
                    let eu_ofereco = self.meu_id.as_str() > de.as_str();
                    self.cria_par(&de, eu_ofereco)?;
                }
            }
            "peer-left" => {
                let id = msg["id"].as_str().unwrap_or("").to_string();
                self.espectadores.remove(&id);
                if let Some(p) = self.pares.remove(&id) {
                    p.pc.close();
                }
                log::info!("[{id}] saiu da sala");
            }
            "offer" | "answer" => self.sdp(&de, tipo, &msg["sdp"]).await?,
            "ice-candidate" => self.candidato_remoto(&de, &msg["candidate"]).await?,
            _ => {}
        }
        Ok(())
    }

    fn cria_par(&mut self, id: &str, iniciador: bool) -> Result<()> {
        if self.pares.contains_key(id) {
            return Ok(());
        }
        let pc = self
            .fabrica
            .create_peer_connection(self.rtc.clone())
            .map_err(|e| anyhow::anyhow!("create_peer_connection: {e:?}"))?;

        let ids = [self.id_stream.as_str()];
        if let Some(v) = &self.trilha_video {
            pc.add_track(MediaStreamTrack::Video(v.clone()), &ids)
                .map_err(|e| anyhow::anyhow!("add_track vídeo: {e:?}"))?;
        }
        if let Some(a) = &self.trilha_audio {
            pc.add_track(MediaStreamTrack::Audio(a.clone()), &ids)
                .map_err(|e| anyhow::anyhow!("add_track áudio: {e:?}"))?;
        }

        let alvo = id.to_string();
        let tx = self.tx_ev.clone();
        pc.on_negotiation_needed(Some(Box::new(move |_| {
            let _ = tx.send(Evento::Negociar(alvo.clone()));
        })));
        let alvo = id.to_string();
        let tx = self.tx_ev.clone();
        pc.on_ice_candidate(Some(Box::new(move |c| {
            let _ = tx.send(Evento::Candidato(alvo.clone(), c));
        })));
        let alvo = id.to_string();
        let tx = self.tx_ev.clone();
        pc.on_connection_state_change(Some(Box::new(move |s| {
            let _ = tx.send(Evento::Estado(alvo.clone(), s));
        })));

        log::info!(
            "[{id}] conexão criada ({})",
            if iniciador {
                "eu ofereço"
            } else {
                "espero a oferta"
            }
        );
        self.pares.insert(
            id.to_string(),
            Par {
                pc,
                educado: !iniciador,
                pendentes: Vec::new(),
            },
        );
        // O `negotiationneeded` já foi disparado pelos `add_track` acima e está
        // na fila de eventos; quem não é iniciador ignora (ver `oferece`) e
        // responde à oferta do outro lado, que é o que a página faz.
        Ok(())
    }

    async fn oferece(&mut self, id: &str) -> Result<()> {
        let Some(par) = self.pares.get(id) else {
            return Ok(());
        };
        // Quem espera não oferece na primeira rodada: a página do outro lado,
        // quando é iniciadora, é "mal-educada" e ignoraria nossa oferta numa
        // colisão. Depois da primeira resposta, qualquer renegociação vale.
        if par.educado && par.pc.current_remote_description().is_none() {
            return Ok(());
        }
        if par.pc.signaling_state() != SignalingState::Stable {
            return Ok(());
        }
        let oferta = par
            .pc
            .create_offer(OfferOptions::default())
            .await
            .map_err(|e| anyhow::anyhow!("[{id}] create_offer: {e:?}"))?;
        par.pc
            .set_local_description(oferta.clone())
            .await
            .map_err(|e| anyhow::anyhow!("[{id}] set_local(offer): {e:?}"))?;
        self.envia(json!({ "type": "offer", "to": id, "sdp": { "type": "offer", "sdp": oferta.to_string() } }));
        log::info!("[{id}] oferta enviada");
        Ok(())
    }

    async fn sdp(&mut self, id: &str, tipo: &str, sdp: &Value) -> Result<()> {
        if !self.pares.contains_key(id) {
            // Oferta de quem a gente ainda não conhecia como "na chamada" (o
            // mode-state pode chegar depois). A página faz o mesmo: cria na hora.
            self.cria_par(id, false)?;
        }
        let texto = sdp["sdp"].as_str().context("sdp sem texto")?;
        let (pc, educado) = {
            let par = &self.pares[id];
            (par.pc.clone(), par.educado)
        };
        let estavel = pc.signaling_state() == SignalingState::Stable;
        let colisao = tipo == "offer" && !estavel;
        if colisao && !educado {
            log::warn!("[{id}] colisão de ofertas — ignorando a do outro lado");
            return Ok(());
        }
        if colisao {
            log::warn!("[{id}] colisão de ofertas — cedendo a vez (rollback)");
            let rb = SessionDescription::parse("", SdpType::Rollback)
                .map_err(|e| anyhow::anyhow!("rollback: {e:?}"))?;
            pc.set_local_description(rb)
                .await
                .map_err(|e| anyhow::anyhow!("[{id}] rollback: {e:?}"))?;
        }
        let tipo_sdp = if tipo == "offer" {
            SdpType::Offer
        } else {
            SdpType::Answer
        };
        let desc = SessionDescription::parse(texto, tipo_sdp)
            .map_err(|e| anyhow::anyhow!("[{id}] SDP inválido: {e:?}"))?;
        pc.set_remote_description(desc)
            .await
            .map_err(|e| anyhow::anyhow!("[{id}] set_remote({tipo}): {e:?}"))?;
        let pendentes = self
            .pares
            .get_mut(id)
            .map(|p| std::mem::take(&mut p.pendentes))
            .unwrap_or_default();
        for c in pendentes {
            if let Err(e) = pc.add_ice_candidate(c).await {
                log::warn!("[{id}] candidato atrasado recusado: {e:?}");
            }
        }
        if tipo == "offer" {
            let resposta = pc
                .create_answer(AnswerOptions::default())
                .await
                .map_err(|e| anyhow::anyhow!("[{id}] create_answer: {e:?}"))?;
            pc.set_local_description(resposta.clone())
                .await
                .map_err(|e| anyhow::anyhow!("[{id}] set_local(answer): {e:?}"))?;
            self.envia(json!({ "type": "answer", "to": id, "sdp": { "type": "answer", "sdp": resposta.to_string() } }));
            log::info!("[{id}] resposta enviada");
            // Se a oferta do outro lado não tinha lugar pro nosso vídeo, uma
            // renegociação nossa resolve; agora que existe descrição remota,
            // ela passa pela regra do `oferece`.
            let _ = self.tx_ev.send(Evento::Negociar(id.to_string()));
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
        .map_err(|e| anyhow::anyhow!("[{id}] candidato inválido: {e:?}"))?;
        if par.pc.current_remote_description().is_none() {
            par.pendentes.push(candidato);
            return Ok(());
        }
        if let Err(e) = par.pc.add_ice_candidate(candidato).await {
            log::warn!("[{id}] candidato recusado: {e:?}");
        }
        Ok(())
    }

    async fn relata(&self) {
        for (id, par) in &self.pares {
            let Ok(stats) = par.pc.get_stats().await else {
                continue;
            };
            let mut linhas = Vec::new();
            for s in stats {
                if let libwebrtc::stats::RtcStats::OutboundRtp(o) = s {
                    linhas.push(format!(
                        "{} {} kB enviados",
                        o.stream.kind,
                        o.sent.bytes_sent / 1024
                    ));
                }
            }
            log::info!(
                "[{id}] {:?} — {}",
                par.pc.connection_state(),
                linhas.join(", ")
            );
        }
    }
}
