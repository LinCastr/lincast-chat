//! Conversa HTTP com o servidor: entrar na conta, quem sou eu, salas ativas.
//!
//! A sessão é o mesmo cookie `session` que o navegador recebe; aqui ele é
//! guardado num arquivo da pasta de configuração (modo 600) pra ninguém ter
//! que digitar a senha a cada abertura. Ele dura 30 dias no servidor.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Servidor {
    pub base: String,
}

impl Servidor {
    pub fn padrao() -> Self {
        let base = if let Ok(val) = std::env::var("LINCAST_SERVIDOR") {
            val
        } else if let Ok(val) = std::env::var("CONCORD_SERVIDOR") {
            log::warn!("[config] CONCORD_SERVIDOR é obsoleta; use LINCAST_SERVIDOR");
            val
        } else {
            "https://meuteamspeak6.duckdns.org:8443".into()
        };
        Self {
            base: base.trim_end_matches('/').to_string(),
        }
    }

    pub fn url(&self, caminho: &str) -> String {
        format!("{}{caminho}", self.base)
    }

    pub fn url_ws(&self) -> String {
        if let Some(r) = self.base.strip_prefix("https://") {
            format!("wss://{r}")
        } else if let Some(r) = self.base.strip_prefix("http://") {
            format!("ws://{r}")
        } else {
            self.base.clone()
        }
    }
}

/// "Passatempo recente?": texto curto + emoji, com prazo opcional (ms).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct StatusCustom {
    #[serde(default)]
    pub texto: String,
    #[serde(default)]
    pub emoji: String,
    #[serde(default)]
    pub ate: Option<i64>,
}

impl StatusCustom {
    /// Vencido? (o servidor também apaga, mas o aviso pode demorar até 30s)
    pub fn vencido(&self) -> bool {
        self.ate
            .is_some_and(|t| t <= chrono::Utc::now().timestamp_millis())
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[allow(dead_code)]
pub struct PerfilConta {
    #[serde(default, rename = "statusCustom")]
    pub status_custom: Option<StatusCustom>,
    #[serde(default)]
    pub bio: Option<String>,
    #[serde(default)]
    pub cor: Option<String>,
    #[serde(default)]
    pub banner: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Eu {
    pub name: String,
    #[serde(default)]
    pub usuario: Option<String>,
    #[serde(default, rename = "gifAtivo")]
    pub gif_ativo: bool,
    #[serde(default)]
    pub avatar: Option<String>,
    #[serde(default, rename = "bannerUrl")]
    pub banner_url: Option<String>,
    #[serde(default)]
    pub perfil: Option<PerfilConta>,
    #[serde(default)]
    pub tipo: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct JogoAtividade {
    pub nome: String,
    #[serde(default)]
    pub origem: Option<String>,
    #[serde(default)]
    pub desde: Option<u64>,
    /// Id da capa no SteamGridDB, quando o jogo está registrado.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capa: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct MusicaAtividade {
    pub titulo: String,
    #[serde(default)]
    pub artista: Option<String>,
    #[serde(default)]
    pub album: Option<String>,
    #[serde(default)]
    pub arte: Option<String>,
    /// Quando a faixa "começou" (ms desde 1970, já descontada a posição): dá a barra de progresso.
    #[serde(default)]
    pub inicio: Option<u64>,
    /// Duração da faixa em ms.
    #[serde(default)]
    pub duracao: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct Atividade {
    #[serde(default)]
    pub jogo: Option<JogoAtividade>,
    #[serde(default)]
    pub musica: Option<MusicaAtividade>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct CartaoPerfil {
    pub uid: String,
    pub usuario: String,
    pub name: String,
    #[serde(default)]
    pub avatar: Option<String>,
    #[serde(default)]
    pub bio: Option<String>,
    #[serde(default)]
    pub cor: Option<String>,
    #[serde(default)]
    pub banner: Option<String>,
    #[serde(default, rename = "bannerUrl")]
    pub banner_url: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default, rename = "emChamada")]
    pub em_chamada: bool,
    #[serde(default)]
    pub atividade: Option<Atividade>,
    #[serde(default, rename = "statusCustom")]
    pub status_custom: Option<StatusCustom>,
}

/// Insígnia calculada pelo servidor (nada é guardado: some quando a condição some).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Insignia {
    pub id: String,
    pub nome: String,
    #[serde(default)]
    pub detalhe: String,
}

/// Um jogo da coleção da pessoa (detectado pelo app dela).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct JogoVisto {
    pub nome: String,
    #[serde(default)]
    pub primeira: i64,
    #[serde(default)]
    pub ultima: i64,
    #[serde(default)]
    pub dias: u32,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct AmigoEmComum {
    pub uid: String,
    pub usuario: String,
    pub name: String,
    #[serde(default)]
    pub avatar: Option<String>,
}

/// Recado do mural de um perfil.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Recado {
    pub id: String,
    pub de: String,
    #[serde(default, rename = "deNome")]
    pub de_nome: String,
    pub texto: String,
    #[serde(default)]
    pub ts: i64,
}

/// Resposta de `/api/perfil/<usuario>`: o cartão mais o que só amigos veem.
#[derive(Debug, Clone, Deserialize)]
pub struct PerfilCompleto {
    #[serde(flatten)]
    pub cartao: CartaoPerfil,
    #[serde(default)]
    pub insignias: Vec<Insignia>,
    #[serde(default, rename = "criadaEm")]
    pub criada_em: Option<i64>,
    /// Sou eu ou sou amigo: libera jogos, amigos em comum e mural.
    #[serde(default, rename = "amigoDeMim")]
    pub amigo_de_mim: bool,
    #[serde(default)]
    pub jogos: Vec<JogoVisto>,
    #[serde(default)]
    pub mutuos: Vec<AmigoEmComum>,
    #[serde(default)]
    pub listas: ListasJogos,
}

/// Um jogo escolhido à mão numa lista do perfil (id do SteamGridDB).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JogoLista {
    pub id: u64,
    pub nome: String,
}

/// As três listas do perfil: favorito (1), "jogos que eu gosto" e "quero jogar" (até 20).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListasJogos {
    #[serde(default)]
    pub favorito: Option<JogoLista>,
    #[serde(default)]
    pub gosto: Vec<JogoLista>,
    #[serde(default)]
    pub quero: Vec<JogoLista>,
}

pub const MAX_JOGOS_POR_LISTA: usize = 20;

/// Endereço (no servidor) da capa de um jogo.
pub fn capa_url(id: u64) -> String {
    format!("/api/jogos/capa/{id}")
}

/// Procura jogos pelo nome (o servidor consulta o SteamGridDB).
pub fn busca_jogos(srv: &Servidor, token: &str, termo: &str) -> Result<Vec<JogoLista>> {
    let url = srv.url(&format!("/api/jogos/busca?q={}", codifica_url(termo)));
    let mut r = com_sessao(agente().get(&url), token)
        .call()
        .context("não consegui buscar jogos")?;
    let status = r.status().as_u16();
    let v: Value = r.body_mut().read_json().unwrap_or(Value::Null);
    if status != 200 {
        bail!("{}", erro_do_corpo(&v, status));
    }
    Ok(v["resultados"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|j| {
                    Some(JogoLista {
                        id: j["id"].as_u64()?,
                        nome: j["nome"].as_str()?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default())
}

/// Grava as três listas do próprio perfil.
pub fn salva_listas(srv: &Servidor, token: &str, listas: &ListasJogos) -> Result<()> {
    let mut r = com_sessao(agente().put(&srv.url("/api/conta/jogos")), token)
        .send_json(listas)
        .context("não consegui salvar os jogos")?;
    let status = r.status().as_u16();
    if status == 200 {
        return Ok(());
    }
    let v: Value = r.body_mut().read_json().unwrap_or(Value::Null);
    bail!("{}", erro_do_corpo(&v, status));
}

impl std::ops::Deref for PerfilCompleto {
    type Target = CartaoPerfil;
    fn deref(&self) -> &CartaoPerfil {
        &self.cartao
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RespostaAmigos {
    #[serde(default)]
    pub amigos: Vec<CartaoPerfil>,
    #[serde(default)]
    pub recebidos: Vec<CartaoPerfil>,
    #[serde(default)]
    pub enviados: Vec<CartaoPerfil>,
}

fn agente() -> ureq::Agent {
    ureq::Agent::config_builder()
        // 401/409 são respostas normais aqui ("senha errada", "já existe"):
        // a gente quer ler o corpo delas, não receber um erro genérico.
        .http_status_as_error(false)
        .timeout_global(Some(std::time::Duration::from_secs(15)))
        .build()
        .into()
}

/// Para envio e download de arquivos grandes: o relógio de 15 s do `agente()`
/// cortaria um upload de centenas de MB no meio.
fn agente_longo() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(std::time::Duration::from_secs(6 * 3600)))
        .build()
        .into()
}

/// Codifica um trecho para ir numa URL (tudo que não é seguro vira `%XX`).
pub fn codifica_url(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ArquivoEnviado {
    pub id: String,
    pub nome: String,
    pub tam: u64,
}

/// Envia um arquivo (até 200 MB) em streaming: o corpo sai direto do disco.
pub fn envia_arquivo(
    srv: &Servidor,
    token: &str,
    conversa: &str,
    caminho: &std::path::Path,
) -> Result<ArquivoEnviado> {
    let nome = caminho
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "arquivo".into());
    let arq = std::fs::File::open(caminho).context("não consegui abrir o arquivo")?;
    let url = srv.url(&format!(
        "/api/arquivo?conversa={}&nome={}",
        codifica_url(conversa),
        codifica_url(&nome)
    ));
    let mut r = com_sessao(agente_longo().post(&url), token)
        .header("Content-Type", "application/octet-stream")
        .send(arq)
        .context("não consegui enviar o arquivo")?;
    let status = r.status().as_u16();
    let v: Value = r.body_mut().read_json().unwrap_or(Value::Null);
    if status != 200 {
        bail!("{}", erro_do_corpo(&v, status));
    }
    Ok(ArquivoEnviado {
        id: v["arquivoId"].as_str().unwrap_or("").to_string(),
        nome: v["nome"].as_str().unwrap_or(&nome).to_string(),
        tam: v["tam"].as_u64().unwrap_or(0),
    })
}

/// Baixa um arquivo do chat para `destino`, em streaming.
pub fn baixa_arquivo(
    srv: &Servidor,
    token: &str,
    conversa: &str,
    id: &str,
    destino: &std::path::Path,
) -> Result<()> {
    let url = srv.url(&format!("/api/arquivo/{}/{id}", codifica_url(conversa)));
    let mut r = com_sessao(agente_longo().get(&url), token)
        .call()
        .context("não consegui baixar o arquivo")?;
    if r.status().as_u16() != 200 {
        bail!("servidor respondeu {}", r.status().as_u16());
    }
    let parcial = destino.with_extension("parte");
    let mut f = std::fs::File::create(&parcial).context("não consegui criar o arquivo")?;
    std::io::copy(&mut r.body_mut().as_reader(), &mut f).context("download interrompido")?;
    std::fs::rename(&parcial, destino).context("não consegui salvar o arquivo")?;
    Ok(())
}

fn erro_do_corpo(v: &Value, status: u16) -> String {
    v["erro"]
        .as_str()
        .map(String::from)
        .unwrap_or_else(|| format!("o servidor respondeu {status}"))
}

/// Entra (ou cria a conta) e devolve o token de sessão.
pub fn entra(srv: &Servidor, usuario: &str, senha: &str, criar: bool) -> Result<String> {
    let rota = if criar {
        "/api/conta/criar"
    } else {
        "/api/conta/entrar"
    };
    let mut r = agente()
        .post(&srv.url(rota))
        .send_json(json!({ "usuario": usuario, "senha": senha }))
        .context("não consegui falar com o servidor")?;
    let status = r.status().as_u16();
    let token = r
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find_map(|c| c.strip_prefix("session="))
        .map(|c| c.split(';').next().unwrap_or("").to_string());
    let corpo: Value = r.body_mut().read_json().unwrap_or(Value::Null);
    if status != 200 {
        bail!(erro_do_corpo(&corpo, status));
    }
    token
        .filter(|t| !t.is_empty())
        .context("o servidor não mandou a sessão")
}

fn com_sessao<B>(req: ureq::RequestBuilder<B>, token: &str) -> ureq::RequestBuilder<B> {
    req.header("Cookie", format!("session={token}"))
}

/// Baixa uma imagem (avatar, ícone de grupo). `url` pode ser caminho do
/// servidor (`/api/...`, com a sessão) ou `https://` de fora. Teto de 21MB (avatar e banner aceitam até 20MB).
pub fn baixa_imagem(srv: &Servidor, token: Option<&str>, url: &str) -> Result<Vec<u8>> {
    use std::io::Read;
    let completa = if url.starts_with('/') {
        srv.url(url)
    } else if url.starts_with("https://") {
        url.to_string()
    } else {
        anyhow::bail!("endereço de imagem não aceito");
    };
    let mut req = agente_longo().get(&completa);
    if let (Some(t), true) = (token, url.starts_with('/')) {
        req = com_sessao(req, t);
    }
    let mut r = req.call()?;
    if r.status().as_u16() != 200 {
        anyhow::bail!("servidor respondeu {}", r.status().as_u16());
    }
    let mut bytes = Vec::new();
    r.body_mut()
        .as_reader()
        .take(21 * 1024 * 1024)
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Abre (ou recupera) a conversa direta com um amigo e devolve o id dela.
pub fn abre_dm(srv: &Servidor, token: &str, uid: &str) -> Result<String> {
    let mut r = com_sessao(agente().post(&srv.url("/api/conversas/dm")), token)
        .send_json(json!({ "uid": uid }))?;
    let status = r.status().as_u16();
    let v: Value = r.body_mut().read_json()?;
    if status != 200 {
        anyhow::bail!("{}", erro_do_corpo(&v, status));
    }
    v["conversa"]["id"]
        .as_str()
        .map(String::from)
        .context("resposta sem id de conversa")
}

/// Codifica um pedaço de caminho de URL (ids contêm `:`).
pub fn codifica_segmento(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            o.push(b as char);
        } else {
            o.push_str(&format!("%{b:02X}"));
        }
    }
    o
}

/// Cria um grupo com estes amigos (além de quem cria). Devolve `(id, nome)`.
pub fn cria_grupo(srv: &Servidor, token: &str, uids: &[String]) -> Result<(String, String)> {
    let mut r = com_sessao(agente().post(&srv.url("/api/grupos")), token)
        .send_json(json!({ "membros": uids }))?;
    let status = r.status().as_u16();
    let v: Value = r.body_mut().read_json()?;
    if status != 200 {
        anyhow::bail!("{}", erro_do_corpo(&v, status));
    }
    let id = v["conversa"]["id"]
        .as_str()
        .context("resposta sem id de grupo")?
        .to_string();
    let nome = v["conversa"]["nome"]
        .as_str()
        .unwrap_or("Grupo")
        .to_string();
    Ok((id, nome))
}

/// Adiciona amigos a um grupo existente.
pub fn adiciona_membros(
    srv: &Servidor,
    token: &str,
    conversa: &str,
    uids: &[String],
) -> Result<()> {
    let caminho = format!("/api/grupos/{}/membros", codifica_segmento(conversa));
    let mut r =
        com_sessao(agente().post(&srv.url(&caminho)), token).send_json(json!({ "uids": uids }))?;
    let status = r.status().as_u16();
    if status != 200 {
        let v: Value = r.body_mut().read_json().unwrap_or(Value::Null);
        anyhow::bail!("{}", erro_do_corpo(&v, status));
    }
    Ok(())
}

/// Salva nome e/ou ícone do grupo (`icone`: imagem em bytes, GIF ou remover).
pub fn salva_grupo(
    srv: &Servidor,
    token: &str,
    conversa: &str,
    nome: Option<&str>,
    icone: Option<&crate::ui::editar_grupo::IconeNovo>,
) -> Result<()> {
    use crate::ui::editar_grupo::IconeNovo;
    let base = format!("/api/grupos/{}", codifica_segmento(conversa));
    let confere = |mut r: ureq::http::Response<ureq::Body>| -> Result<()> {
        let status = r.status().as_u16();
        if status != 200 {
            let v: Value = r.body_mut().read_json().unwrap_or(Value::Null);
            anyhow::bail!("{}", erro_do_corpo(&v, status));
        }
        Ok(())
    };
    if let Some(n) = nome {
        let r =
            com_sessao(agente().post(&srv.url(&base)), token).send_json(json!({ "nome": n }))?;
        confere(r)?;
    }
    match icone {
        Some(IconeNovo::Imagem(bytes)) => {
            let dados = base64::engine::general_purpose::STANDARD.encode(bytes);
            let r = com_sessao(agente().post(&srv.url(&format!("{base}/icone"))), token)
                .send_json(json!({ "data": dados }))?;
            confere(r)?;
        }
        Some(IconeNovo::Gif(url)) => {
            let r = com_sessao(agente().post(&srv.url(&format!("{base}/icone"))), token)
                .send_json(json!({ "gif": url }))?;
            confere(r)?;
        }
        Some(IconeNovo::Remover) => {
            let r =
                com_sessao(agente().delete(&srv.url(&format!("{base}/icone"))), token).call()?;
            confere(r)?;
        }
        None => {}
    }
    Ok(())
}

/// Ações de grupo sem corpo: sair (`POST .../sair`) e remover alguém
/// (`DELETE .../membros/<uid>`).
pub fn acao_de_grupo(srv: &Servidor, token: &str, metodo: &str, caminho: &str) -> Result<()> {
    let url = srv.url(caminho);
    let mut r = match metodo {
        "DELETE" => com_sessao(agente().delete(&url), token).call()?,
        _ => com_sessao(agente().post(&url), token).send_empty()?,
    };
    let status = r.status().as_u16();
    if status != 200 {
        let v: Value = r.body_mut().read_json().unwrap_or(Value::Null);
        anyhow::bail!("{}", erro_do_corpo(&v, status));
    }
    Ok(())
}

/// Minhas conversas (`GET /api/conversas`), cada uma como JSON bruto: quem
/// lê é o canal de avisos, que ignora a que não entender.
pub fn conversas(srv: &Servidor, token: &str) -> Result<Vec<Value>> {
    let mut r = com_sessao(agente().get(&srv.url("/api/conversas")), token).call()?;
    if r.status().as_u16() != 200 {
        anyhow::bail!("servidor respondeu {}", r.status().as_u16());
    }
    let v: Value = r.body_mut().read_json()?;
    Ok(v["conversas"].as_array().cloned().unwrap_or_default())
}

/// `None` quando a sessão não vale mais (expirou, ou o servidor esqueceu).
pub fn eu(srv: &Servidor, token: &str) -> Result<Option<Eu>> {
    let mut r = com_sessao(agente().get(&srv.url("/api/me")), token).call()?;
    let v: Value = r.body_mut().read_json()?;
    if v["loggedIn"].as_bool() != Some(true) {
        return Ok(None);
    }
    Ok(Some(serde_json::from_value(v)?))
}

pub fn sai(srv: &Servidor, token: &str) {
    let _ = com_sessao(agente().get(&srv.url("/auth/logout")), token).call();
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ItemGif {
    pub url: String,
    pub previa: String,
    #[serde(default)]
    pub descricao: String,
}

#[derive(Debug, Clone, Deserialize)]
struct RespostaGifs {
    #[serde(default)]
    resultados: Vec<ItemGif>,
}

/// Busca GIFs pelo servidor ou lista os em alta quando o termo estiver em branco.
pub fn busca_gifs(srv: &Servidor, token: &str, termo: &str) -> Result<Vec<ItemGif>> {
    let url = if termo.trim().is_empty() {
        srv.url("/api/gif")
    } else {
        let q: String = termo
            .trim()
            .chars()
            .map(|c| match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
                ' ' => "+".to_string(),
                _ => {
                    let mut b = [0; 4];
                    let s = c.encode_utf8(&mut b);
                    s.bytes().map(|byte| format!("%{:02X}", byte)).collect()
                }
            })
            .collect();
        format!("{}/api/gif?q={}", srv.base, q)
    };
    let mut r = com_sessao(agente().get(&url), token).call()?;
    let status = r.status().as_u16();
    let corpo: Value = r.body_mut().read_json().unwrap_or(Value::Null);
    if status != 200 {
        bail!(erro_do_corpo(&corpo, status));
    }
    let resp: RespostaGifs = serde_json::from_value(corpo)?;
    Ok(resp.resultados)
}

// --- sessão guardada em disco ---------------------------------------------

fn arquivo_da_sessao() -> Option<PathBuf> {
    let novo_dir = directories::ProjectDirs::from("", "", "lincast-chat")?;
    let novo_arq = novo_dir.config_dir().join("sessao");

    if !novo_arq.exists()
        && let Some(velho_dir) = directories::ProjectDirs::from("", "", "concord")
    {
        let velho_arq = velho_dir.config_dir().join("sessao");
        if velho_arq.exists() {
            if let Some(parent) = novo_arq.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(conteudo) = std::fs::read_to_string(&velho_arq)
                && std::fs::write(&novo_arq, conteudo).is_ok()
            {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ =
                        std::fs::set_permissions(&novo_arq, std::fs::Permissions::from_mode(0o600));
                }
                log::info!(
                    "[sessao] sessão migrada com sucesso de ~/.config/concord para ~/.config/lincast-chat"
                );
            }
        }
    }

    Some(novo_arq)
}

pub fn sessao_salva() -> Option<String> {
    let t = std::fs::read_to_string(arquivo_da_sessao()?).ok()?;
    let t = t.trim().to_string();
    (!t.is_empty()).then_some(t)
}

pub fn salva_sessao(token: Option<&str>) {
    let Some(arq) = arquivo_da_sessao() else {
        return;
    };
    match token {
        None => {
            let _ = std::fs::remove_file(arq);
        }
        Some(t) => {
            if let Some(dir) = arq.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if std::fs::write(&arq, t).is_ok() {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(&arq, std::fs::Permissions::from_mode(0o600));
                }
            }
        }
    }
}

/// Consulta o cartão de perfil público de um usuário.
pub fn busca_perfil(srv: &Servidor, token: &str, usuario: &str) -> Result<PerfilCompleto> {
    let url = format!("{}/api/perfil/{}", srv.base, usuario);
    let mut r = com_sessao(agente().get(&url), token)
        .call()
        .context("não consegui buscar o perfil")?;
    let status = r.status().as_u16();
    if status == 200 {
        let cartao: PerfilCompleto = r
            .body_mut()
            .read_json()
            .context("resposta de perfil inválida")?;
        Ok(cartao)
    } else {
        let v: Value = r.body_mut().read_json().unwrap_or(Value::Null);
        bail!("{}", erro_do_corpo(&v, status));
    }
}

/// Atualiza as informações do próprio perfil (nome de exibição, bio, cor, banner).
pub fn atualiza_perfil(
    srv: &Servidor,
    token: &str,
    nome: Option<&str>,
    bio: Option<&str>,
    cor: Option<&str>,
    banner: Option<&str>,
) -> Result<()> {
    let mut corpo = json!({});
    if let Some(n) = nome {
        corpo["nome"] = json!(n);
    }
    if let Some(b) = bio {
        corpo["bio"] = json!(b);
    }
    if let Some(c) = cor {
        corpo["cor"] = json!(c);
    }
    if let Some(bn) = banner {
        corpo["banner"] = json!(bn);
    }
    let mut r = com_sessao(agente().post(&srv.url("/api/conta/perfil")), token)
        .send_json(corpo)
        .context("não consegui atualizar o perfil")?;
    let status = r.status().as_u16();
    if status == 200 {
        Ok(())
    } else {
        let v: Value = r.body_mut().read_json().unwrap_or(Value::Null);
        bail!("{}", erro_do_corpo(&v, status));
    }
}

/// Envia uma imagem de avatar codificada em base64 e retorna a URL pública gerada.
pub fn envia_avatar(srv: &Servidor, token: &str, bytes: &[u8]) -> Result<String> {
    use base64::engine::general_purpose::STANDARD;
    let b64 = STANDARD.encode(bytes);
    let mut r = com_sessao(agente_longo().post(&srv.url("/api/conta/avatar")), token)
        .send_json(json!({ "data": b64 }))
        .context("não consegui enviar o avatar")?;
    let status = r.status().as_u16();
    let v: Value = r.body_mut().read_json().unwrap_or(Value::Null);
    if status == 200 {
        let avatar_url = v["avatar"].as_str().unwrap_or("").to_string();
        Ok(avatar_url)
    } else {
        bail!("{}", erro_do_corpo(&v, status));
    }
}

/// Remove o avatar da conta no servidor.
pub fn remove_avatar(srv: &Servidor, token: &str) -> Result<()> {
    let mut r = com_sessao(agente().delete(&srv.url("/api/conta/avatar")), token)
        .call()
        .context("não consegui remover o avatar")?;
    let status = r.status().as_u16();
    if status == 200 {
        Ok(())
    } else {
        let v: Value = r.body_mut().read_json().unwrap_or(Value::Null);
        bail!("{}", erro_do_corpo(&v, status));
    }
}

/// Envia uma imagem/GIF de banner codificada em base64 e retorna a URL gerada.
pub fn envia_banner(srv: &Servidor, token: &str, bytes: &[u8]) -> Result<String> {
    use base64::engine::general_purpose::STANDARD;
    let b64 = STANDARD.encode(bytes);
    let mut r = com_sessao(agente_longo().post(&srv.url("/api/conta/banner")), token)
        .send_json(json!({ "data": b64 }))
        .context("não consegui enviar o banner")?;
    let status = r.status().as_u16();
    let v: Value = r.body_mut().read_json().unwrap_or(Value::Null);
    if status == 200 {
        let banner_url = v["bannerUrl"].as_str().unwrap_or("").to_string();
        Ok(banner_url)
    } else {
        bail!("{}", erro_do_corpo(&v, status));
    }
}

/// Remove o banner da conta no servidor.
pub fn remove_banner(srv: &Servidor, token: &str) -> Result<()> {
    let mut r = com_sessao(agente().delete(&srv.url("/api/conta/banner")), token)
        .call()
        .context("não consegui remover o banner")?;
    let status = r.status().as_u16();
    if status == 200 {
        Ok(())
    } else {
        let v: Value = r.body_mut().read_json().unwrap_or(Value::Null);
        bail!("{}", erro_do_corpo(&v, status));
    }
}

/// Troca a senha da conta local (POST /api/conta/senha).
pub fn troca_senha(srv: &Servidor, token: &str, senha_atual: &str, senha_nova: &str) -> Result<()> {
    if senha_nova.len() < 8 {
        bail!("A senha nova precisa ter pelo menos 8 caracteres.");
    }
    let mut r = com_sessao(agente().post(&srv.url("/api/conta/senha")), token)
        .send_json(json!({
            "senhaAtual": senha_atual,
            "senhaNova": senha_nova,
        }))
        .context("não consegui falar com o servidor")?;
    let status = r.status().as_u16();
    if status == 200 {
        Ok(())
    } else {
        let v: Value = r.body_mut().read_json().unwrap_or(Value::Null);
        bail!("{}", erro_do_corpo(&v, status));
    }
}

/// Apaga a própria conta permanentemente (POST /api/conta/apagar).
pub fn apaga_conta(srv: &Servidor, token: &str, senha: &str) -> Result<()> {
    let mut r = com_sessao(agente().post(&srv.url("/api/conta/apagar")), token)
        .send_json(json!({ "senha": senha }))
        .context("não consegui falar com o servidor")?;
    let status = r.status().as_u16();
    if status == 200 {
        Ok(())
    } else {
        let v: Value = r.body_mut().read_json().unwrap_or(Value::Null);
        bail!("{}", erro_do_corpo(&v, status));
    }
}

/// Manda o log da sessão ao servidor (POST /api/relatorio, texto puro).
pub fn envia_relatorio(srv: &Servidor, token: &str, versao: &str, texto: &str) -> Result<()> {
    let mut r = com_sessao(agente().post(&srv.url("/api/relatorio")), token)
        .header("Content-Type", "text/plain; charset=utf-8")
        .header("X-Lincast-Versao", versao)
        .send(texto.as_bytes())
        .context("não consegui falar com o servidor")?;
    let status = r.status().as_u16();
    if status == 200 {
        Ok(())
    } else {
        let v: Value = r.body_mut().read_json().unwrap_or(Value::Null);
        bail!("{}", erro_do_corpo(&v, status));
    }
}

/// Atualiza o status de presença (POST /api/status).
/// Valores aceitos: "online", "ausente", "ocupado".
pub fn define_status(srv: &Servidor, token: &str, status_str: &str) -> Result<()> {
    let mut r = com_sessao(agente().post(&srv.url("/api/status")), token)
        .send_json(json!({ "status": status_str }))
        .context("não consegui atualizar o status")?;
    let status = r.status().as_u16();
    if status == 200 {
        Ok(())
    } else {
        let v: Value = r.body_mut().read_json().unwrap_or(Value::Null);
        bail!("{}", erro_do_corpo(&v, status));
    }
}

/// Consulta a lista de amigos, pedidos recebidos e enviados (GET /api/amigos).
pub fn lista_amigos(srv: &Servidor, token: &str) -> Result<RespostaAmigos> {
    let mut r = com_sessao(agente().get(&srv.url("/api/amigos")), token)
        .call()
        .context("não consegui consultar lista de amigos")?;
    let status = r.status().as_u16();
    if status == 200 {
        r.body_mut().read_json().context("json de amigos inválido")
    } else {
        let v: Value = r.body_mut().read_json().unwrap_or(Value::Null);
        bail!("{}", erro_do_corpo(&v, status));
    }
}

/// Envia um pedido de amizade pelo nome de usuário (POST /api/amigos/pedir).
pub fn pede_amigo(srv: &Servidor, token: &str, usuario: &str) -> Result<bool> {
    let mut r = com_sessao(agente().post(&srv.url("/api/amigos/pedir")), token)
        .send_json(json!({ "usuario": usuario.trim() }))
        .context("não consegui enviar pedido de amizade")?;
    let status = r.status().as_u16();
    if status == 200 {
        let v: Value = r.body_mut().read_json().unwrap_or(Value::Null);
        let viraram_amigos = v["viraramAmigos"].as_bool().unwrap_or(false);
        Ok(viraram_amigos)
    } else {
        let v: Value = r.body_mut().read_json().unwrap_or(Value::Null);
        bail!("{}", erro_do_corpo(&v, status));
    }
}

/// Aceita ou recusa um pedido de amizade recebido (POST /api/amigos/responder).
pub fn responde_amigo(srv: &Servidor, token: &str, uid: &str, aceita: bool) -> Result<()> {
    let mut r = com_sessao(agente().post(&srv.url("/api/amigos/responder")), token)
        .send_json(json!({ "uid": uid, "aceita": aceita }))
        .context("não consegui responder ao pedido de amizade")?;
    let status = r.status().as_u16();
    if status == 200 {
        Ok(())
    } else {
        let v: Value = r.body_mut().read_json().unwrap_or(Value::Null);
        bail!("{}", erro_do_corpo(&v, status));
    }
}

/// Remove um amigo ou cancela um pedido de amizade (POST /api/amigos/remover).
pub fn remove_amigo(srv: &Servidor, token: &str, uid: &str) -> Result<()> {
    let mut r = com_sessao(agente().post(&srv.url("/api/amigos/remover")), token)
        .send_json(json!({ "uid": uid }))
        .context("não consegui remover amizade")?;
    let status = r.status().as_u16();
    if status == 200 {
        Ok(())
    } else {
        let v: Value = r.body_mut().read_json().unwrap_or(Value::Null);
        bail!("{}", erro_do_corpo(&v, status));
    }
}

/// Envia a atividade atual do usuário (POST /api/atividade).
pub fn envia_atividade(srv: &Servidor, token: &str, atv: Option<&Atividade>) -> Result<()> {
    let mut r = com_sessao(agente().post(&srv.url("/api/atividade")), token)
        .send_json(json!({ "atividade": atv }))
        .context("não consegui atualizar atividade")?;
    let status = r.status().as_u16();
    if status == 200 {
        Ok(())
    } else {
        let v: Value = r.body_mut().read_json().unwrap_or(Value::Null);
        bail!("{}", erro_do_corpo(&v, status));
    }
}

/// Métricas do servidor retornadas por /api/metrics.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MetricasServidor {
    #[serde(default)]
    pub uptime_segundos: u64,
    #[serde(default, rename = "memoriaProcessoMB")]
    pub memoria_processo_mb: u64,
    #[serde(default, rename = "heapUsadoMB")]
    pub heap_usado_mb: u64,
    #[serde(default, rename = "memoriaLivreMB")]
    pub memoria_livre_mb: u64,
    #[serde(default, rename = "memoriaTotalMB")]
    pub memoria_total_mb: u64,
    #[serde(default)]
    pub carga_media: Vec<f64>,
    #[serde(default)]
    pub conexoes: u64,
    #[serde(default)]
    pub salas_ativas: u64,
    #[serde(default)]
    pub pessoas_em_chamada: u64,
    #[serde(default)]
    pub pico_de_pessoas: u64,
    #[serde(default)]
    pub salas_com_senha_salva: u64,
    #[serde(default)]
    pub sessoes_abertas: u64,
    #[serde(default)]
    pub arquivos_de_chat: u64,
    #[serde(default, rename = "chatMB")]
    pub chat_mb: f64,
    #[serde(default)]
    pub versao: String,
}

/// Consulta as métricas operacionais do servidor (/api/metrics).
pub fn busca_metricas(srv: &Servidor, token: &str) -> Result<MetricasServidor> {
    let url = srv.url("/api/metrics");
    let mut req = agente().get(&url);
    if !token.is_empty() {
        req = req.header("Cookie", &format!("session={token}"));
    }
    let mut r = req.call().context("falha ao consultar /api/metrics")?;
    let status = r.status().as_u16();
    if status == 200 {
        let m: MetricasServidor = r
            .body_mut()
            .read_json()
            .context("JSON inválido de /api/metrics")?;
        Ok(m)
    } else {
        let v: Value = r.body_mut().read_json().unwrap_or(Value::Null);
        bail!("{}", erro_do_corpo(&v, status));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grupo_do_wordle_aceita_dia_como_texto_ou_numero() {
        // Resposta real do servidor depois das rodadas: `dia` vem como texto.
        let v = json!({"dia":"20743_r2","numero":290,"streak":0,
            "hoje":[{"uid":"local:1","name":"val47","avatar":null,"n":1,"fim":null,"cores":["yyxxx"]}],
            "ontem":[]});
        let g: GrupoWordle = serde_json::from_value(v).unwrap();
        assert_eq!(g.dia, "20743_r2");
        assert_eq!(g.hoje.len(), 1);
        let g: GrupoWordle = serde_json::from_value(json!({"dia":20743})).unwrap();
        assert_eq!(g.dia, "20743");
    }

    #[test]
    fn codifica_url_escapa_o_que_nao_e_seguro() {
        assert_eq!(codifica_url("dm_local:abc"), "dm_local%3Aabc");
        assert_eq!(
            codifica_url("relatório final.bin"),
            "relat%C3%B3rio%20final.bin"
        );
        assert_eq!(codifica_url("a-b_c.d~e"), "a-b_c.d~e");
    }

    #[test]
    fn testa_troca_senha_valida_tamanho_minimo() {
        let srv = Servidor {
            base: "http://127.0.0.1:9999".into(),
        };
        let res = troca_senha(&srv, "tok", "senha-antiga", "curta");
        assert!(res.is_err());
        assert!(
            res.unwrap_err()
                .to_string()
                .contains("pelo menos 8 caracteres")
        );
    }

    #[test]
    fn testa_desserializacao_metricas_servidor() {
        let json_raw = r#"{
            "uptimeSegundos": 14200,
            "memoriaProcessoMB": 82,
            "heapUsadoMB": 45,
            "memoriaLivreMB": 3410,
            "memoriaTotalMB": 16000,
            "cargaMedia": [0.45, 0.52, 0.60],
            "conexoes": 8,
            "salasAtivas": 3,
            "pessoasEmChamada": 5,
            "picoDePessoas": 12,
            "salasComSenhaSalva": 2,
            "sessoesAbertas": 7,
            "arquivosDeChat": 19,
            "chatMB": 1.45,
            "versao": "2.1.0"
        }"#;

        let m: MetricasServidor = serde_json::from_str(json_raw).expect("deve desserializar");
        assert_eq!(m.uptime_segundos, 14200);
        assert_eq!(m.memoria_processo_mb, 82);
        assert_eq!(m.heap_usado_mb, 45);
        assert_eq!(m.memoria_livre_mb, 3410);
        assert_eq!(m.memoria_total_mb, 16000);
        assert_eq!(m.carga_media, vec![0.45, 0.52, 0.60]);
        assert_eq!(m.conexoes, 8);
        assert_eq!(m.salas_ativas, 3);
        assert_eq!(m.pessoas_em_chamada, 5);
        assert_eq!(m.pico_de_pessoas, 12);
        assert_eq!(m.versao, "2.1.0");
    }
}

#[cfg(test)]
mod testes_url {
    use super::codifica_segmento;

    #[test]
    fn codifica_dois_pontos_e_barra() {
        assert_eq!(codifica_segmento("local:ab12"), "local%3Aab12");
        assert_eq!(codifica_segmento("a/b c"), "a%2Fb%20c");
        assert_eq!(codifica_segmento("grp_9f-1.x~"), "grp_9f-1.x~");
    }
}

/// Copia uma mensagem de `origem` para a conversa `destino` (o servidor lê o
/// original do histórico: o cliente só diz qual). `extra` vira uma mensagem
/// comum logo depois.
pub fn encaminha_mensagem(
    srv: &Servidor,
    token: &str,
    destino: &str,
    origem: &str,
    mensagem: &str,
    extra: &str,
) -> Result<()> {
    let url = srv.url(&format!(
        "/api/conversas/{}/encaminhar",
        codifica_segmento(destino)
    ));
    let mut r = com_sessao(agente().post(&url), token)
        .send_json(json!({ "origem": origem, "mensagem": mensagem, "extra": extra }))?;
    let status = r.status().as_u16();
    let v: Value = r.body_mut().read_json()?;
    if status != 200 {
        anyhow::bail!("{}", erro_do_corpo(&v, status));
    }
    Ok(())
}

/// Retrato de uma mensagem favoritada (vem do servidor).
#[derive(Debug, Clone, serde::Deserialize, PartialEq, Eq)]
pub struct Favorito {
    pub conversa: String,
    pub id: String,
    #[serde(default)]
    pub autor: String,
    #[serde(default)]
    pub texto: String,
    #[serde(default)]
    pub tipo: String,
    #[serde(default)]
    pub ts: i64,
}

pub fn lista_favoritos(srv: &Servidor, token: &str) -> Result<Vec<Favorito>> {
    let mut r = com_sessao(agente().get(&srv.url("/api/favoritos")), token).call()?;
    let status = r.status().as_u16();
    let v: Value = r.body_mut().read_json()?;
    if status != 200 {
        anyhow::bail!("{}", erro_do_corpo(&v, status));
    }
    Ok(serde_json::from_value(v["favoritos"].clone()).unwrap_or_default())
}

pub fn favorita(srv: &Servidor, token: &str, conversa: &str, mensagem: &str) -> Result<()> {
    let mut r = com_sessao(agente().post(&srv.url("/api/favoritos")), token)
        .send_json(json!({ "conversa": conversa, "mensagem": mensagem }))?;
    let status = r.status().as_u16();
    let v: Value = r.body_mut().read_json()?;
    if status != 200 {
        anyhow::bail!("{}", erro_do_corpo(&v, status));
    }
    Ok(())
}

pub fn desfavorita(srv: &Servidor, token: &str, conversa: &str, mensagem: &str) -> Result<()> {
    let url = srv.url(&format!(
        "/api/favoritos/{}/{}",
        codifica_segmento(conversa),
        codifica_segmento(mensagem)
    ));
    let mut r = com_sessao(agente().delete(&url), token).call()?;
    let status = r.status().as_u16();
    let v: Value = r.body_mut().read_json()?;
    if status != 200 {
        anyhow::bail!("{}", erro_do_corpo(&v, status));
    }
    Ok(())
}

/// Define (ou limpa, se texto e emoji vierem vazios) o status personalizado.
/// `minutos`: 30, 60, 240, 1440 ou `None` para não limpar sozinho.
pub fn define_status_custom(
    srv: &Servidor,
    token: &str,
    texto: &str,
    emoji: &str,
    minutos: Option<u32>,
) -> Result<Option<StatusCustom>> {
    let mut r = com_sessao(agente().post(&srv.url("/api/status-custom")), token)
        .send_json(json!({ "texto": texto, "emoji": emoji, "minutos": minutos }))?;
    let status = r.status().as_u16();
    let v: Value = r.body_mut().read_json()?;
    if status != 200 {
        anyhow::bail!("{}", erro_do_corpo(&v, status));
    }
    Ok(serde_json::from_value(v["statusCustom"].clone()).ok())
}

// --- Mural do perfil -----------------------------------------------------------

fn recados_da_resposta(mut r: ureq::http::Response<ureq::Body>) -> Result<Vec<Recado>> {
    let status = r.status().as_u16();
    let v: Value = r.body_mut().read_json().unwrap_or(Value::Null);
    if status != 200 {
        anyhow::bail!("{}", erro_do_corpo(&v, status));
    }
    Ok(serde_json::from_value(v["recados"].clone()).unwrap_or_default())
}

pub fn mural_lista(srv: &Servidor, token: &str, usuario: &str) -> Result<Vec<Recado>> {
    let url = srv.url(&format!("/api/mural/{usuario}"));
    recados_da_resposta(com_sessao(agente().get(&url), token).call()?)
}

pub fn mural_envia(srv: &Servidor, token: &str, usuario: &str, texto: &str) -> Result<Vec<Recado>> {
    let url = srv.url(&format!("/api/mural/{usuario}"));
    recados_da_resposta(
        com_sessao(agente().post(&url), token).send_json(json!({ "texto": texto }))?,
    )
}

pub fn mural_apaga(srv: &Servidor, token: &str, usuario: &str, id: &str) -> Result<Vec<Recado>> {
    let url = srv.url(&format!("/api/mural/{usuario}/{id}"));
    recados_da_resposta(com_sessao(agente().delete(&url), token).call()?)
}

// --- Wordle ------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct TentativaWordle {
    pub palavra: String,
    /// Cinco letras: `g` certa, `y` em outro lugar, `x` não tem.
    pub cores: String,
}

/// O jogo de hoje de quem pergunta. A resposta só vem quando acaba.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default)]
pub struct EstadoWordle {
    #[serde(default)]
    pub numero: i64,
    #[serde(default)]
    pub max: u32,
    #[serde(default)]
    pub fim: Option<String>,
    #[serde(default)]
    pub tentativas: Vec<TentativaWordle>,
    #[serde(default)]
    pub resposta: Option<String>,
}

/// Quem do grupo jogou e as cores (nunca as letras).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct JogadorWordle {
    pub uid: String,
    pub name: String,
    #[serde(default)]
    pub avatar: Option<String>,
    #[serde(default)]
    pub n: u32,
    #[serde(default)]
    pub fim: Option<String>,
    #[serde(default)]
    pub cores: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default)]
pub struct GrupoWordle {
    /// Chave da rodada: `"20743"` ou `"20743_r2"`. O servidor manda texto
    /// (antes das rodadas mandava número), por isso aceita os dois.
    #[serde(default, deserialize_with = "texto_ou_numero")]
    pub dia: String,
    #[serde(default)]
    pub numero: i64,
    #[serde(default)]
    pub streak: u32,
    #[serde(default)]
    pub hoje: Vec<JogadorWordle>,
    #[serde(default)]
    pub ontem: Vec<JogadorWordle>,
}

fn texto_ou_numero<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<String, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::String(s) => s,
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    })
}

fn le_json_ou_erro(r: &mut ureq::http::Response<ureq::Body>) -> Result<(u16, Value)> {
    let status = r.status().as_u16();
    let v: Value = r.body_mut().read_json().unwrap_or(Value::Null);
    Ok((status, v))
}

pub fn wordle_estado(srv: &Servidor, token: &str, dia: Option<&str>) -> Result<EstadoWordle> {
    let mut url = srv.url("/api/wordle");
    if let Some(d) = dia {
        url.push_str(&format!("?dia={}", codifica_segmento(d)));
    }
    let mut r = com_sessao(agente().get(&url), token).call()?;
    let (status, v) = le_json_ou_erro(&mut r)?;
    if status != 200 {
        anyhow::bail!("{}", erro_do_corpo(&v, status));
    }
    Ok(serde_json::from_value(v)?)
}

/// Manda uma tentativa. Erro de jogo (curta, fora da lista, já terminou) vem
/// como `Err((codigo, texto))`.
pub fn wordle_tenta(
    srv: &Servidor,
    token: &str,
    palavra: &str,
    dia: Option<&str>,
) -> std::result::Result<EstadoWordle, (String, String)> {
    let mut corpo = json!({ "palavra": palavra });
    if let Some(d) = dia {
        corpo["dia"] = json!(d);
    }
    let mut r = com_sessao(agente().post(&srv.url("/api/wordle/tentar")), token)
        .send_json(corpo)
        .map_err(|e| ("rede".to_string(), e.to_string()))?;
    let (status, v) = le_json_ou_erro(&mut r).map_err(|e| ("rede".to_string(), e.to_string()))?;
    if status == 200 {
        return serde_json::from_value(v).map_err(|e| ("rede".to_string(), e.to_string()));
    }
    Err((
        v["codigo"].as_str().unwrap_or("erro").to_string(),
        erro_do_corpo(&v, status),
    ))
}

pub fn wordle_grupo(
    srv: &Servidor,
    token: &str,
    conversa: &str,
    dia: Option<&str>,
) -> Result<GrupoWordle> {
    let mut url = srv.url(&format!(
        "/api/conversas/{}/wordle",
        codifica_segmento(conversa)
    ));
    if let Some(d) = dia {
        url.push_str(&format!("?dia={}", codifica_segmento(d)));
    }
    let mut r = com_sessao(agente().get(&url), token).call()?;
    let (status, v) = le_json_ou_erro(&mut r)?;
    if status != 200 {
        anyhow::bail!("{}", erro_do_corpo(&v, status));
    }
    Ok(serde_json::from_value(v)?)
}

/// Garante a mensagem do Wordle de hoje na conversa (cria se não houver).
pub fn wordle_inicia(srv: &Servidor, token: &str, conversa: &str) -> Result<()> {
    let url = srv.url(&format!(
        "/api/conversas/{}/wordle",
        codifica_segmento(conversa)
    ));
    let mut r = com_sessao(agente().post(&url), token).send_json(json!({}))?;
    let (status, v) = le_json_ou_erro(&mut r)?;
    if status != 200 {
        anyhow::bail!("{}", erro_do_corpo(&v, status));
    }
    Ok(())
}

/// [DEV] Sorteia uma nova palavra do Wordle para hoje e inicia nova rodada para o grupo.
pub fn wordle_dev_trocar(srv: &Servidor, token: &str, conversa: Option<&str>) -> Result<()> {
    let mut corpo = json!({});
    if let Some(c) = conversa {
        corpo["conversa"] = json!(c);
    }
    let mut r =
        com_sessao(agente().post(&srv.url("/api/wordle/dev/trocar")), token).send_json(corpo)?;
    let (status, v) = le_json_ou_erro(&mut r)?;
    if status != 200 {
        anyhow::bail!("{}", erro_do_corpo(&v, status));
    }
    Ok(())
}

#[cfg(test)]
mod testes_perfil_completo {
    use super::*;

    #[test]
    fn perfil_completo_le_cartao_e_extras() {
        let v = json!({
            "uid": "local:1", "usuario": "ana", "name": "Ana", "status": "online",
            "insignias": [{ "id": "veterano", "nome": "Veterano", "detalhe": "30 dias" }],
            "criadaEm": 1_700_000_000_000_i64, "amigoDeMim": true,
            "jogos": [{ "nome": "Celeste", "primeira": 1, "ultima": 2, "dias": 3 }],
            "mutuos": [{ "uid": "local:2", "usuario": "bob", "name": "Bob" }]
        });
        let p: PerfilCompleto = serde_json::from_value(v).unwrap();
        assert_eq!(p.usuario, "ana"); // Deref para o cartão
        assert!(p.amigo_de_mim);
        assert_eq!(p.jogos[0].dias, 3);
        assert_eq!(p.mutuos[0].usuario, "bob");
        assert_eq!(p.insignias[0].nome, "Veterano");
    }

    #[test]
    fn perfil_de_servidor_antigo_nao_quebra() {
        let v = json!({ "uid": "local:1", "usuario": "ana", "name": "Ana" });
        let p: PerfilCompleto = serde_json::from_value(v).unwrap();
        assert!(!p.amigo_de_mim && p.jogos.is_empty() && p.mutuos.is_empty());
    }

    #[test]
    fn recado_aceita_campos_do_servidor() {
        let v = json!({ "id": "ab", "de": "local:2", "deNome": "Bob", "texto": "oi", "ts": 5 });
        let r: Recado = serde_json::from_value(v).unwrap();
        assert_eq!((r.de_nome.as_str(), r.texto.as_str()), ("Bob", "oi"));
    }
}

// --- Cosméticos de perfil ------------------------------------------------------

pub fn catalogo_cosmeticos(srv: &Servidor) -> Result<Vec<crate::cosmeticos::Item>> {
    let mut r = agente().get(&srv.url("/api/cosmeticos")).call()?;
    if r.status().as_u16() != 200 {
        anyhow::bail!("servidor respondeu {}", r.status().as_u16());
    }
    let v: Value = r.body_mut().read_json()?;
    Ok(v["itens"]
        .as_array()
        .map(|l| {
            l.iter()
                .filter_map(|i| serde_json::from_value(i.clone()).ok())
                .collect()
        })
        .unwrap_or_default())
}

pub fn cosmeticos_pessoas(
    srv: &Servidor,
    token: &str,
) -> Result<std::collections::HashMap<String, crate::cosmeticos::Escolha>> {
    let mut r = com_sessao(agente().get(&srv.url("/api/cosmeticos/pessoas")), token).call()?;
    if r.status().as_u16() != 200 {
        anyhow::bail!("servidor respondeu {}", r.status().as_u16());
    }
    let v: Value = r.body_mut().read_json()?;
    Ok(serde_json::from_value(v["pessoas"].clone()).unwrap_or_default())
}

pub fn salva_cosmeticos(
    srv: &Servidor,
    token: &str,
    e: &crate::cosmeticos::Escolha,
) -> Result<Option<crate::cosmeticos::Escolha>> {
    let mut r = com_sessao(agente().post(&srv.url("/api/conta/cosmeticos")), token)
        .send_json(e.para_envio())?;
    let status = r.status().as_u16();
    let v: Value = r.body_mut().read_json()?;
    if status != 200 {
        anyhow::bail!("{}", erro_do_corpo(&v, status));
    }
    Ok(serde_json::from_value(v["cosmeticos"].clone()).ok())
}
