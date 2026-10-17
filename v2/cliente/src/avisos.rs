//! Canal de avisos (v3): uma conexão WebSocket por app, aberta logo após o
//! login e mantida sempre, fora de qualquer sala. É por aqui que o servidor
//! conta de conversa nova, mensagem nova, chamada, toque e status dos amigos.
//!
//! O ator (`roda`) só fala com a rede e entrega `Evento`s; quem interpreta é
//! `EstadoAvisos::aplica`, puro e testável. Reconecta sozinho com o mesmo
//! backoff do M0 (500ms dobrando até 5s) e, a cada conexão, busca a lista de
//! conversas por HTTP: o que se perdeu enquanto estava fora se recupera ali.

// O estado e os comandos ganham consumidores nas telas da Fase C (lista de
// conversas, palco da chamada, toque recebido); até lá, só o ator e a lista
// de conversas são lidos.
#![allow(dead_code)]

use std::collections::HashMap;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use futures_util::{SinkExt as _, StreamExt as _};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

use crate::api::Servidor;

/// O que o ator precisa para conectar. `Hash` porque identifica a
/// `Subscription`: trocar de token reinicia o canal.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Entrada {
    pub servidor: Servidor,
    pub token: String,
}

/// Comandos da interface para o canal.
#[derive(Debug, Clone)]
pub enum Comando {
    Ligar(String),
    Recusar(String),
    TocarDeNovo { conversa: String, uid: String },
}

impl Comando {
    fn json(&self) -> Value {
        match self {
            Comando::Ligar(c) => json!({"type": "ligar", "conversa": c}),
            Comando::Recusar(c) => json!({"type": "recusar", "conversa": c}),
            Comando::TocarDeNovo { conversa, uid } => {
                json!({"type": "tocar-de-novo", "conversa": conversa, "uid": uid})
            }
        }
    }
}

/// Um membro como o servidor o descreve (cartão público).
#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
pub struct Membro {
    pub uid: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub usuario: Option<String>,
    #[serde(default)]
    pub avatar: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub removida: bool,
    #[serde(default)]
    pub bio: Option<String>,
    #[serde(default, rename = "bannerUrl")]
    pub banner_url: Option<String>,
    #[serde(default)]
    pub cor: Option<String>,
    #[serde(default)]
    pub atividade: Option<Value>,
    #[serde(default, rename = "statusCustom")]
    pub status_custom: Option<crate::api::StatusCustom>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
pub struct UltimaMensagem {
    #[serde(default)]
    pub autor: Option<String>,
    #[serde(default)]
    pub previa: Option<String>,
    #[serde(default)]
    pub sys: Option<String>,
    #[serde(default)]
    pub texto: Option<String>,
    #[serde(default)]
    pub ts: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
pub struct ChamadaDaConversa {
    #[serde(default)]
    pub ativa: bool,
    #[serde(default)]
    pub dentro: Vec<String>,
    #[serde(default)]
    pub chamando: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
pub struct Conversa {
    pub id: String,
    /// `"dm"` ou `"grupo"`.
    pub tipo: String,
    #[serde(default)]
    pub nome: String,
    #[serde(default)]
    pub icone: Option<String>,
    #[serde(default)]
    pub dono: Option<String>,
    #[serde(default, rename = "criadaEm")]
    pub criada_em: i64,
    #[serde(default)]
    pub membros: Vec<Membro>,
    #[serde(default, rename = "ultimaMensagem")]
    pub ultima_mensagem: Option<UltimaMensagem>,
    #[serde(default)]
    pub chamada: ChamadaDaConversa,
}

#[derive(Debug, Clone)]
pub enum Evento {
    /// O ator está de pé; guarde o canal para mandar `Comando`s.
    Pronto(mpsc::UnboundedSender<Comando>),
    Conectado,
    Desconectado(String),
    /// Retrato completo, vindo de `GET /api/conversas` a cada conexão.
    Lista(Vec<Conversa>),
    ConversaNova(Conversa),
    ConversaMudou(Conversa),
    ConversaSaiu(String),
    Mensagem {
        conversa: String,
        autor: Option<String>,
        uid: Option<String>,
        previa: String,
        sys: Option<String>,
        ts: i64,
    },
    Chamada {
        conversa: String,
        dentro: Vec<String>,
        /// Quem está tocando agora (ainda não atendeu).
        chamando: Vec<String>,
        ativa: bool,
    },
    Toque {
        conversa: String,
        quem_uid: String,
        quem_nome: String,
        restante_ms: u64,
        duracao_ms: u64,
    },
    ToqueFim {
        conversa: String,
        motivo: String,
    },
    ToqueErro {
        conversa: String,
        motivo: String,
    },
    StatusAmigo {
        uid: String,
        status: String,
    },
    AtividadeAmigo {
        uid: String,
        atividade: Option<Value>,
    },
    StatusCustomAmigo {
        uid: String,
        status_custom: Option<crate::api::StatusCustom>,
    },
}

/// Traduz uma mensagem do servidor. `None` para o que não é aviso (ou não
/// conhecemos): ignorar é o certo, o servidor pode ganhar tipos novos.
pub fn parse_evento(v: &Value) -> Option<Evento> {
    let texto = |k: &str| v[k].as_str().map(String::from);
    match v["type"].as_str()? {
        "avisos-ok" => Some(Evento::Conectado),
        "conversa-nova" => serde_json::from_value(v["conversa"].clone())
            .ok()
            .map(Evento::ConversaNova),
        "conversa-mudou" => serde_json::from_value(v["conversa"].clone())
            .ok()
            .map(Evento::ConversaMudou),
        "conversa-saiu" => Some(Evento::ConversaSaiu(texto("conversa")?)),
        "mensagem" => Some(Evento::Mensagem {
            conversa: texto("conversa")?,
            autor: texto("autor"),
            uid: texto("uid"),
            previa: texto("previa").unwrap_or_default(),
            sys: texto("sys"),
            ts: v["ts"].as_i64().unwrap_or(0),
        }),
        "chamada" => Some(Evento::Chamada {
            conversa: texto("conversa")?,
            dentro: v["dentro"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            chamando: v["chamando"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            ativa: v["ativa"].as_bool().unwrap_or(false),
        }),
        "toque" => Some(Evento::Toque {
            conversa: texto("conversa")?,
            quem_uid: v["quem"]["uid"].as_str()?.to_string(),
            quem_nome: v["quem"]["name"].as_str().unwrap_or("Alguém").to_string(),
            restante_ms: v["restanteMs"].as_u64().unwrap_or(30_000),
            duracao_ms: v["duracaoMs"].as_u64().unwrap_or(30_000),
        }),
        "toque-fim" => Some(Evento::ToqueFim {
            conversa: texto("conversa")?,
            motivo: texto("motivo").unwrap_or_default(),
        }),
        "toque-erro" => Some(Evento::ToqueErro {
            conversa: texto("conversa").unwrap_or_default(),
            motivo: texto("motivo").unwrap_or_default(),
        }),
        "status-amigo" => Some(Evento::StatusAmigo {
            uid: texto("uid")?,
            status: texto("status")?,
        }),
        "status-custom-amigo" => Some(Evento::StatusCustomAmigo {
            uid: texto("uid")?,
            status_custom: serde_json::from_value(v["statusCustom"].clone()).ok(),
        }),
        "atividade-amigo" => Some(Evento::AtividadeAmigo {
            uid: texto("uid")?,
            atividade: match &v["atividade"] {
                Value::Null => None,
                a => Some(a.clone()),
            },
        }),
        _ => None,
    }
}

// --- estado -----------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ToqueRecebido {
    pub conversa: String,
    pub quem_uid: String,
    pub quem_nome: String,
    pub termina_em: Instant,
    pub duracao_ms: u64,
}

/// O que a interface sabe pelo canal de avisos.
#[derive(Debug, Default)]
pub struct EstadoAvisos {
    pub conectado: bool,
    pub conversas: Vec<Conversa>,
    /// Mensagens não lidas por conversa (sem contar as suas nem as de sistema).
    pub nao_lidas: HashMap<String, u32>,
    /// Quem está dentro da chamada de cada conversa (só as ativas).
    pub chamadas: HashMap<String, Vec<String>>,
    /// Quem está tocando em cada conversa.
    pub chamando: HashMap<String, Vec<String>>,
    pub toques: Vec<ToqueRecebido>,
    pub status_amigos: HashMap<String, String>,
    pub atividade_amigos: HashMap<String, Value>,
    /// Quem sou eu, para não contar a minha própria mensagem como não lida.
    pub meu_usuario: Option<String>,
    /// A conversa que a pessoa está olhando agora, com a janela em foco.
    /// Mensagem dela chega já lida.
    pub conversa_ativa: Option<String>,
    pub ultimo_erro_toque: Option<String>,
}

impl EstadoAvisos {
    pub fn meu_uid(&self) -> Option<&str> {
        let eu = self.meu_usuario.as_deref()?;
        self.conversas
            .iter()
            .flat_map(|c| c.membros.iter())
            .find(|m| m.usuario.as_deref() == Some(eu))
            .map(|m| m.uid.as_str())
    }

    pub fn marca_lida(&mut self, conversa: &str) {
        self.nao_lidas.remove(conversa);
    }

    pub fn total_nao_lidas(&self) -> u32 {
        self.nao_lidas.values().sum()
    }

    fn poe_conversa(&mut self, c: Conversa) {
        if c.chamada.chamando.is_empty() {
            self.chamando.remove(&c.id);
        } else {
            self.chamando
                .insert(c.id.clone(), c.chamada.chamando.clone());
        }
        if c.chamada.ativa {
            self.chamadas.insert(c.id.clone(), c.chamada.dentro.clone());
        } else {
            self.chamadas.remove(&c.id);
        }
        match self.conversas.iter_mut().find(|x| x.id == c.id) {
            Some(x) => *x = c,
            None => self.conversas.push(c),
        }
    }

    pub fn aplica(&mut self, ev: &Evento) {
        match ev {
            Evento::Pronto(_) => {}
            Evento::Conectado => self.conectado = true,
            Evento::Desconectado(_) => {
                self.conectado = false;
                // Toque que estava na tela não vale mais: não sabemos se
                // acabou. A conexão nova traz o retrato do que está tocando.
                self.toques.clear();
            }
            Evento::Lista(lista) => {
                self.chamadas.clear();
                self.chamando.clear();
                self.conversas.clear();
                for c in lista {
                    self.poe_conversa(c.clone());
                }
                let ids: Vec<&String> = self.conversas.iter().map(|c| &c.id).collect();
                self.nao_lidas.retain(|k, _| ids.contains(&k));
            }
            Evento::ConversaNova(c) | Evento::ConversaMudou(c) => self.poe_conversa(c.clone()),
            Evento::ConversaSaiu(id) => {
                self.conversas.retain(|c| &c.id != id);
                self.nao_lidas.remove(id);
                self.chamadas.remove(id);
                self.chamando.remove(id);
                self.toques.retain(|t| &t.conversa != id);
            }
            Evento::Mensagem {
                conversa,
                autor,
                uid,
                previa,
                sys,
                ts,
            } => {
                if let Some(c) = self.conversas.iter_mut().find(|c| &c.id == conversa) {
                    c.ultima_mensagem = Some(UltimaMensagem {
                        autor: autor.clone(),
                        previa: Some(previa.clone()),
                        sys: sys.clone(),
                        texto: None,
                        ts: Some(*ts),
                    });
                }
                let minha = uid.as_deref().is_some_and(|u| Some(u) == self.meu_uid());
                let olhando = self.conversa_ativa.as_deref() == Some(conversa.as_str());
                if sys.is_none() && !minha && !olhando {
                    *self.nao_lidas.entry(conversa.clone()).or_insert(0) += 1;
                }
            }
            Evento::Chamada {
                conversa,
                dentro,
                chamando,
                ativa,
            } => {
                if chamando.is_empty() {
                    self.chamando.remove(conversa);
                } else {
                    self.chamando.insert(conversa.clone(), chamando.clone());
                }
                if *ativa && !dentro.is_empty() {
                    self.chamadas.insert(conversa.clone(), dentro.clone());
                } else {
                    self.chamadas.remove(conversa);
                }
                if let Some(c) = self.conversas.iter_mut().find(|c| &c.id == conversa) {
                    c.chamada = ChamadaDaConversa {
                        ativa: *ativa,
                        dentro: dentro.clone(),
                        chamando: chamando.clone(),
                    };
                }
            }
            Evento::Toque {
                conversa,
                quem_uid,
                quem_nome,
                restante_ms,
                duracao_ms,
            } => {
                self.toques.retain(|t| &t.conversa != conversa);
                self.toques.push(ToqueRecebido {
                    conversa: conversa.clone(),
                    quem_uid: quem_uid.clone(),
                    quem_nome: quem_nome.clone(),
                    termina_em: Instant::now() + Duration::from_millis(*restante_ms),
                    duracao_ms: *duracao_ms,
                });
            }
            Evento::ToqueFim { conversa, .. } => {
                self.toques.retain(|t| &t.conversa != conversa);
            }
            Evento::ToqueErro { motivo, .. } => self.ultimo_erro_toque = Some(motivo.clone()),
            Evento::StatusAmigo { uid, status } => {
                self.status_amigos.insert(uid.clone(), status.clone());
                for m in self
                    .conversas
                    .iter_mut()
                    .flat_map(|c| c.membros.iter_mut())
                    .filter(|m| &m.uid == uid)
                {
                    m.status = Some(status.clone());
                }
            }
            Evento::StatusCustomAmigo { uid, status_custom } => {
                for m in self
                    .conversas
                    .iter_mut()
                    .flat_map(|c| c.membros.iter_mut())
                    .filter(|m| &m.uid == uid)
                {
                    m.status_custom = status_custom.clone();
                }
            }
            Evento::AtividadeAmigo { uid, atividade } => match atividade {
                Some(a) => {
                    self.atividade_amigos.insert(uid.clone(), a.clone());
                }
                // Guarda o "nada" (Null) em vez de apagar a chave: quem consulta usa
                // `.or(retrato da conversa)`, e sem a marca o retrato velho voltava
                // (a música que já tinha parado continuava na lista).
                None => {
                    self.atividade_amigos.insert(uid.clone(), Value::Null);
                }
            },
        }
    }
}

// --- ator -------------------------------------------------------------------

const ESPERA_INICIAL: Duration = Duration::from_millis(500);
const ESPERA_MAXIMA: Duration = Duration::from_secs(5);

enum Fim {
    InterfaceFechou,
    /// O servidor recusou o canal (conta sem direito a avisos): não adianta
    /// tentar de novo.
    Negado,
    Caiu(String),
}

/// Corpo da `Subscription`: roda até a interface largar o canal.
pub async fn roda(entrada: Entrada, mut saida: iced::futures::channel::mpsc::Sender<Evento>) {
    let (tx_cmd, mut rx_cmd) = mpsc::unbounded_channel::<Comando>();
    if saida.send(Evento::Pronto(tx_cmd)).await.is_err() {
        return;
    }
    let mut espera = ESPERA_INICIAL;
    loop {
        let fim = match sessao(&entrada, &mut rx_cmd, &mut saida, &mut espera).await {
            Ok(f) => f,
            Err(e) => Fim::Caiu(format!("{e:#}")),
        };
        match fim {
            Fim::InterfaceFechou => return,
            Fim::Negado => {
                let _ = saida
                    .send(Evento::Desconectado("canal de avisos negado".into()))
                    .await;
                return;
            }
            Fim::Caiu(motivo) => {
                log::warn!("[avisos] {motivo}; tentando de novo em {espera:?}");
                if saida.send(Evento::Desconectado(motivo)).await.is_err() {
                    return;
                }
            }
        }
        tokio::time::sleep(espera).await;
        espera = (espera * 2).min(ESPERA_MAXIMA);
    }
}

async fn sessao(
    entrada: &Entrada,
    rx_cmd: &mut mpsc::UnboundedReceiver<Comando>,
    saida: &mut iced::futures::channel::mpsc::Sender<Evento>,
    espera: &mut Duration,
) -> Result<Fim> {
    let mut pedido = entrada.servidor.url_ws().into_client_request()?;
    pedido.headers_mut().insert(
        "Cookie",
        format!("session={}", entrada.token)
            .parse()
            .context("token inválido")?,
    );
    let (ws, _) = tokio_tungstenite::connect_async(pedido)
        .await
        .context("não consegui abrir o canal de avisos")?;
    let (mut ws_tx, mut ws_rx) = ws.split();
    *espera = ESPERA_INICIAL;
    ws_tx
        .send(Message::Text(json!({"type": "avisos"}).to_string().into()))
        .await?;

    // Retrato inicial por HTTP: cobre o que passou enquanto estávamos fora.
    let (srv, tok) = (entrada.servidor.clone(), entrada.token.clone());
    let lista = tokio::task::spawn_blocking(move || crate::api::conversas(&srv, &tok)).await?;
    match lista {
        Ok(v) => {
            let conversas: Vec<Conversa> = v
                .into_iter()
                .filter_map(|c| serde_json::from_value(c).ok())
                .collect();
            if saida.send(Evento::Lista(conversas)).await.is_err() {
                return Ok(Fim::InterfaceFechou);
            }
        }
        Err(e) => log::warn!("[avisos] não consegui a lista de conversas: {e:#}"),
    }

    loop {
        tokio::select! {
            cmd = rx_cmd.recv() => {
                let Some(cmd) = cmd else { return Ok(Fim::InterfaceFechou) };
                ws_tx.send(Message::Text(cmd.json().to_string().into())).await?;
            }
            msg = ws_rx.next() => match msg {
                Some(Ok(Message::Text(t))) => {
                    let Ok(v) = serde_json::from_str::<Value>(&t) else { continue };
                    if v["type"] == "avisos-negado" {
                        return Ok(Fim::Negado);
                    }
                    if let Some(ev) = parse_evento(&v)
                        && saida.send(ev).await.is_err()
                    {
                        return Ok(Fim::InterfaceFechou);
                    }
                }
                Some(Ok(Message::Close(_))) | None => {
                    return Ok(Fim::Caiu("o servidor fechou o canal".into()));
                }
                Some(Err(e)) => return Ok(Fim::Caiu(format!("canal perdido: {e}"))),
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn conversa(id: &str, usuarios: &[(&str, &str)]) -> Conversa {
        Conversa {
            id: id.into(),
            tipo: "grupo".into(),
            membros: usuarios
                .iter()
                .map(|(u, uid)| Membro {
                    uid: (*uid).into(),
                    usuario: Some((*u).into()),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn le_o_formato_do_servidor() {
        let v = json!({
            "type": "conversa-nova",
            "conversa": {
                "id": "grp_ab", "tipo": "grupo", "nome": "Os Cria", "icone": null,
                "dono": "local:1", "criadaEm": 5,
                "membros": [
                    {"uid": "local:1", "usuario": "ana", "name": "Ana", "avatar": "/api/x", "status": "online"},
                    {"uid": "local:9", "name": "Conta removida", "removida": true}
                ],
                "ultimaMensagem": {"autor": "Ana", "previa": "oi", "ts": 7},
                "chamada": {"ativa": true, "dentro": ["local:1"]}
            }
        });
        let Some(Evento::ConversaNova(c)) = parse_evento(&v) else {
            panic!("não leu a conversa");
        };
        assert_eq!(c.nome, "Os Cria");
        assert_eq!(c.membros.len(), 2);
        assert!(c.membros[1].removida);
        assert!(c.chamada.ativa);
        assert_eq!(c.ultima_mensagem.unwrap().previa.as_deref(), Some("oi"));
    }

    #[test]
    fn le_toque_e_ignora_tipo_desconhecido() {
        let v = json!({"type": "toque", "conversa": "c1",
            "quem": {"uid": "local:1", "name": "Ana"}, "restanteMs": 1500, "duracaoMs": 30000});
        let Some(Evento::Toque {
            restante_ms,
            quem_nome,
            ..
        }) = parse_evento(&v)
        else {
            panic!("não leu o toque");
        };
        assert_eq!((restante_ms, quem_nome.as_str()), (1500, "Ana"));
        assert!(parse_evento(&json!({"type": "coisa-nova"})).is_none());
        assert!(parse_evento(&json!({"type": "toque", "conversa": "c1"})).is_none());
    }

    #[test]
    fn nao_lidas_ignoram_minha_mensagem_sistema_e_conversa_aberta() {
        let mut e = EstadoAvisos {
            meu_usuario: Some("ana".into()),
            ..Default::default()
        };
        e.aplica(&Evento::Lista(vec![
            conversa("c1", &[("ana", "local:1"), ("bob", "local:2")]),
            conversa("c2", &[("ana", "local:1"), ("cai", "local:3")]),
        ]));
        let msg = |conv: &str, uid: &str, sys: Option<&str>| Evento::Mensagem {
            conversa: conv.into(),
            autor: Some("x".into()),
            uid: Some(uid.into()),
            previa: "oi".into(),
            sys: sys.map(String::from),
            ts: 1,
        };
        e.aplica(&msg("c1", "local:2", None));
        e.aplica(&msg("c1", "local:2", None));
        e.aplica(&msg("c1", "local:1", None)); // minha
        e.aplica(&msg("c1", "local:2", Some("chamada"))); // sistema
        assert_eq!(e.nao_lidas.get("c1"), Some(&2));
        e.conversa_ativa = Some("c2".into());
        e.aplica(&msg("c2", "local:3", None));
        assert_eq!(e.nao_lidas.get("c2"), None);
        assert_eq!(e.total_nao_lidas(), 2);
        e.marca_lida("c1");
        assert_eq!(e.total_nao_lidas(), 0);
    }

    #[test]
    fn chamada_toque_e_saida_de_conversa() {
        let mut e = EstadoAvisos::default();
        e.aplica(&Evento::Lista(vec![conversa("c1", &[("ana", "local:1")])]));
        e.aplica(&Evento::Chamada {
            conversa: "c1".into(),
            dentro: vec!["local:1".into()],
            chamando: vec!["local:2".into()],
            ativa: true,
        });
        assert_eq!(e.chamando.get("c1").map(Vec::len), Some(1));
        assert_eq!(e.chamadas.get("c1").map(Vec::len), Some(1));
        assert!(e.conversas[0].chamada.ativa);
        e.aplica(&Evento::Toque {
            conversa: "c1".into(),
            quem_uid: "local:1".into(),
            quem_nome: "Ana".into(),
            restante_ms: 30_000,
            duracao_ms: 30_000,
        });
        assert_eq!(e.toques.len(), 1);
        e.aplica(&Evento::ToqueFim {
            conversa: "c1".into(),
            motivo: "atendeu".into(),
        });
        assert!(e.toques.is_empty());
        e.aplica(&Evento::Chamada {
            conversa: "c1".into(),
            dentro: vec![],
            chamando: vec![],
            ativa: false,
        });
        assert!(e.chamando.is_empty());
        assert!(e.chamadas.is_empty());
        e.aplica(&Evento::ConversaSaiu("c1".into()));
        assert!(e.conversas.is_empty());
    }

    #[test]
    fn queda_limpa_toques_e_lista_nova_substitui() {
        let mut e = EstadoAvisos::default();
        e.aplica(&Evento::Conectado);
        e.aplica(&Evento::Toque {
            conversa: "c1".into(),
            quem_uid: "u".into(),
            quem_nome: "U".into(),
            restante_ms: 1000,
            duracao_ms: 30_000,
        });
        e.aplica(&Evento::Desconectado("caiu".into()));
        assert!(!e.conectado && e.toques.is_empty());
        e.aplica(&Evento::Lista(vec![conversa("a", &[]), conversa("b", &[])]));
        e.aplica(&Evento::Lista(vec![conversa("b", &[])]));
        assert_eq!(e.conversas.len(), 1);
    }

    #[test]
    fn status_do_amigo_chega_aos_membros() {
        let mut e = EstadoAvisos::default();
        e.aplica(&Evento::Lista(vec![conversa("c1", &[("bob", "local:2")])]));
        e.aplica(&Evento::StatusAmigo {
            uid: "local:2".into(),
            status: "ocupado".into(),
        });
        assert_eq!(e.conversas[0].membros[0].status.as_deref(), Some("ocupado"));
    }

    /// Fala com um servidor de verdade. Roda só com `--ignored` e com
    /// `LINCAST_TESTE_URL` (ex.: `http://localhost:8099`) e
    /// `LINCAST_TESTE_TOKEN` (cookie `session` de uma conta local).
    #[tokio::test]
    #[ignore]
    async fn contra_servidor_local() {
        let (Ok(url), Ok(token)) = (
            std::env::var("LINCAST_TESTE_URL"),
            std::env::var("LINCAST_TESTE_TOKEN"),
        ) else {
            panic!("defina LINCAST_TESTE_URL e LINCAST_TESTE_TOKEN");
        };
        let entrada = Entrada {
            servidor: Servidor { base: url },
            token,
        };
        let (tx, mut rx) = iced::futures::channel::mpsc::channel::<Evento>(64);
        let tarefa = tokio::spawn(roda(entrada, tx));
        let espera = Duration::from_secs(5);

        let mut cmd = None;
        let (mut conectou, mut lista) = (false, false);
        while !(conectou && lista && cmd.is_some()) {
            match tokio::time::timeout(espera, rx.next()).await {
                Ok(Some(Evento::Pronto(c))) => cmd = Some(c),
                Ok(Some(Evento::Conectado)) => conectou = true,
                Ok(Some(Evento::Lista(_))) => lista = true,
                Ok(Some(outro)) => panic!("evento inesperado: {outro:?}"),
                _ => panic!("sem resposta do servidor"),
            }
        }
        // Comando de ida e volta: conversa inexistente volta como erro.
        // O canal precisa continuar vivo até a resposta: sem remetente, o ator
        // entende que a interface fechou.
        let cmd = cmd.unwrap();
        cmd.send(Comando::Ligar("grp_que_nao_existe".into()))
            .unwrap();
        loop {
            match tokio::time::timeout(espera, rx.next()).await {
                Ok(Some(Evento::ToqueErro { motivo, .. })) => {
                    assert_eq!(motivo, "nao-membro");
                    break;
                }
                Ok(Some(_)) => {}
                outro => panic!("o servidor não respondeu ao comando: {outro:?}"),
            }
        }
        drop(cmd);
        tarefa.abort();
    }
}
