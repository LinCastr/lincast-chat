//! Aviso de atualização (D2).
//!
//! Plano B da v1 (`fetchLatestRelease` em `electron/main.js`): o app consulta a
//! última release pública no GitHub e, se houver versão mais nova, mostra um
//! banner com link para a página da release. A troca do binário continua
//! manual quando o app foi instalado por AUR/AppImage/`make install` num lugar
//! sem permissão de escrita. Instalado em `~/.local/bin` (pacote da release), o
//! app se atualiza: baixa o `.tar.gz`, confere o SHA-256 da release e troca os
//! executáveis (`instala`), depois reabre.

use iced::widget::{button, container, row, text};
use iced::{Alignment, Border, Element, Length};
use serde::Deserialize;

use crate::{Msg, negrito, tema};

/// Repositório público de binários (a antiga `Valcones47/concord-premium-releases`
/// foi transferida para cá). Ele guarda as releases da v1 (`v1.7.x`, assets
/// `concord-premium-*`); o v2 só enxerga as que trazem asset `lincast-chat*`,
/// senão `v1.7.4` pareceria "mais nova" que `0.1.0` e levaria o instalador do
/// Electron a quem usa o v2.
pub const REPO_RELEASES: &str = "LinCastr/lincast-chat-releases";
/// De quanto em quanto tempo rechecar, para quem deixa o app aberto por dias.
pub const INTERVALO_RECHECAGEM_SEG: u64 = 6 * 60 * 60;

pub fn versao_atual() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfoAtualizacao {
    pub versao: String,
    pub url: String,
    /// Pacote `.tar.gz` e o `.sha256` dele (só links da própria página de releases).
    pub pacote: Option<String>,
    pub sha256: Option<String>,
}

#[derive(Deserialize)]
struct AssetGithub {
    name: String,
    #[serde(default)]
    browser_download_url: Option<String>,
}

#[derive(Deserialize)]
struct ReleaseGithub {
    tag_name: Option<String>,
    html_url: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<AssetGithub>,
}

impl ReleaseGithub {
    /// Release feita para o app v2 (e não para a v1/Electron).
    fn eh_do_v2(&self) -> bool {
        self.assets
            .iter()
            .any(|a| a.name.to_ascii_lowercase().starts_with("lincast-chat"))
    }
}

/// Extrai (maior, menor, remendo) ignorando prefixo `v` e sufixos (`-rc1`, `+build`).
fn parse_versao(v: &str) -> [u64; 3] {
    let limpa = v.trim().trim_start_matches(['v', 'V']);
    let nucleo = limpa.split(['-', '+']).next().unwrap_or("");
    let mut out = [0u64; 3];
    for (i, parte) in nucleo.split('.').take(3).enumerate() {
        out[i] = parte.parse().unwrap_or(0);
    }
    out
}

/// `true` se `remota` for estritamente mais nova que `atual`.
pub fn eh_mais_nova(remota: &str, atual: &str) -> bool {
    parse_versao(remota) > parse_versao(atual)
}

/// Só abre links da própria página de releases: o JSON vem da rede.
pub fn url_permitida(url: &str) -> bool {
    url.starts_with(&format!("https://github.com/{REPO_RELEASES}/"))
}

/// Decide, a partir do JSON da lista de releases, se há atualização para o v2.
/// Separado da rede para teste.
pub fn interpreta_releases(json: &str, atual: &str) -> Option<InfoAtualizacao> {
    let lista: Vec<ReleaseGithub> = serde_json::from_str(json).ok()?;
    let melhor = lista
        .into_iter()
        .filter(|r| !r.draft && !r.prerelease && r.eh_do_v2())
        .filter_map(|r| r.tag_name.clone().map(|t| (t, r)))
        .filter(|(t, _)| eh_mais_nova(t, atual))
        .max_by_key(|(t, _)| parse_versao(t))?;
    let (versao, r) = melhor;
    let url = r
        .html_url
        .filter(|u| url_permitida(u))
        .unwrap_or_else(|| format!("https://github.com/{REPO_RELEASES}/releases/latest"));
    let achado = |fim: &str| {
        r.assets
            .iter()
            .find(|a| {
                let n = a.name.to_ascii_lowercase();
                n.starts_with("lincast-chat") && n.ends_with(fim) && n.contains("linux-x86_64")
            })
            .and_then(|a| a.browser_download_url.clone())
            .filter(|u| url_permitida(u))
    };
    Some(InfoAtualizacao {
        versao,
        url,
        pacote: achado(".tar.gz"),
        sha256: achado(".tar.gz.sha256"),
    })
}

/// Bloqueante: chamar sempre em `spawn_blocking`. Falha de rede = `None`
/// (checar atualização nunca pode incomodar quem está sem internet).
pub fn consulta() -> Option<InfoAtualizacao> {
    consulta_detalhada().ok().flatten()
}

/// Como `consulta`, mas diz o motivo quando a rede falha (botão das configurações).
pub fn consulta_detalhada() -> Result<Option<InfoAtualizacao>, String> {
    let ag: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(5)))
        .build()
        .into();
    let mut resp = ag
        .get(&format!(
            "https://api.github.com/repos/{REPO_RELEASES}/releases?per_page=20"
        ))
        .header("User-Agent", "lincast-chat")
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("sem conexão com o GitHub ({e})"))?;
    let corpo = resp
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("resposta ilegível ({e})"))?;
    Ok(interpreta_releases(&corpo, versao_atual()))
}

/// Primeiro campo do arquivo `.sha256` (`<hash>  <nome>`), em minúsculas.
fn hash_do_arquivo(texto: &str) -> Option<String> {
    let h = texto.split_whitespace().next()?.to_ascii_lowercase();
    (h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit())).then_some(h)
}

fn baixa(url: &str, destino: &std::path::Path, limite: u64) -> Result<(), String> {
    if !url_permitida(url) {
        return Err("endereço de download fora da página de releases".into());
    }
    let ag: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(900)))
        .build()
        .into();
    let mut resp = ag
        .get(url)
        .header("User-Agent", "lincast-chat")
        .call()
        .map_err(|e| format!("falha no download ({e})"))?;
    let mut leitor = resp.body_mut().with_config().limit(limite).reader();
    let mut arq = std::fs::File::create(destino).map_err(|e| e.to_string())?;
    std::io::copy(&mut leitor, &mut arq).map_err(|e| format!("download interrompido ({e})"))?;
    Ok(())
}

fn sha256_de(caminho: &std::path::Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let mut f = std::fs::File::open(caminho).map_err(|e| e.to_string())?;
    let mut h = Sha256::new();
    std::io::copy(&mut f, &mut h).map_err(|e| e.to_string())?;
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Troca `destino` por `origem` de forma atômica (copia ao lado e renomeia;
/// o Linux deixa substituir um executável em uso).
fn troca_arquivo(origem: &std::path::Path, destino: &std::path::Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let novo = destino.with_extension("novo");
    std::fs::copy(origem, &novo)
        .map_err(|e| format!("sem permissão em {} ({e})", destino.display()))?;
    std::fs::set_permissions(&novo, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())?;
    std::fs::rename(&novo, destino).map_err(|e| e.to_string())
}

/// Bloqueante. Baixa o pacote da release, confere o SHA-256 e troca os dois
/// executáveis (`lincast-chat` e `concord-audio`) ao lado do que está rodando.
/// Devolve a versão instalada. Qualquer falha deixa os executáveis como estavam.
pub fn instala(info: &InfoAtualizacao) -> Result<String, String> {
    let (Some(pacote), Some(sha)) = (&info.pacote, &info.sha256) else {
        return Err("a release não traz o pacote para Linux x86_64".into());
    };
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    // Aberto por wrapper (`ld.so app`), o executável em uso é o carregador do
    // sistema, não o app: trocá-lo quebraria a instalação. Só se atualiza
    // sozinho o que está instalado com o nome do próprio app.
    if !exe
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n == "lincast-chat")
    {
        return Err(
            "este app foi aberto por um script; atualize rodando o instalador de novo".into(),
        );
    }
    let dir = exe
        .parent()
        .ok_or("não achei a pasta do app")?
        .to_path_buf();
    let tmp = std::env::temp_dir().join(format!("lincast-update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let r = (|| {
        let tgz = tmp.join("pacote.tar.gz");
        let shaf = tmp.join("pacote.sha256");
        baixa(sha, &shaf, 4096)?;
        baixa(pacote, &tgz, 300 * 1024 * 1024)?;
        let esperado = hash_do_arquivo(&std::fs::read_to_string(&shaf).map_err(|e| e.to_string())?)
            .ok_or("arquivo .sha256 inválido")?;
        if sha256_de(&tgz)? != esperado {
            return Err("o pacote baixado não confere com o SHA-256 da release".to_string());
        }
        let saida = std::process::Command::new("tar")
            .arg("xzf")
            .arg(&tgz)
            .arg("-C")
            .arg(&tmp)
            .output()
            .map_err(|e| format!("não consegui rodar o tar ({e})"))?;
        if !saida.status.success() {
            return Err("não consegui abrir o pacote".into());
        }
        let pasta = std::fs::read_dir(&tmp)
            .map_err(|e| e.to_string())?
            .flatten()
            .map(|e| e.path())
            .find(|p| {
                p.is_dir()
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("lincast-chat-"))
            })
            .ok_or("pacote sem a pasta esperada")?;
        // Primeiro o auxiliar (o app principal é o último a mudar).
        for nome in ["concord-audio", "lincast-chat"] {
            let origem = pasta.join(nome);
            if !origem.is_file() {
                return Err(format!("pacote sem o arquivo {nome}"));
            }
        }
        troca_arquivo(&pasta.join("concord-audio"), &dir.join("concord-audio"))?;
        troca_arquivo(&pasta.join("lincast-chat"), &exe)?;
        Ok(info.versao.trim_start_matches(['v', 'V']).to_string())
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    r
}

/// Abre o app de novo (já atualizado). `LINCAST_ATUALIZADO` evita que o novo
/// processo tente atualizar outra vez de cara.
pub fn reabre() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe)
            .env("LINCAST_ATUALIZADO", "1")
            .spawn();
    }
}

/// Faixa discreta no topo, dispensável.
pub fn banner<'a>(info: &'a InfoAtualizacao) -> Element<'a, Msg> {
    let c = tema::cores();
    let conteudo = row![
        text(format!(
            "Nova versão {} disponível (você está na v{})",
            info.versao,
            versao_atual()
        ))
        .size(13)
        .font(negrito())
        .style(move |_| text::Style {
            color: Some(c.sobre_destaque),
        }),
        button(text("Atualizar agora").size(12))
            .padding([3, 10])
            .style(tema::botao_secundario)
            .on_press(Msg::InstalaAtualizacao),
        button(text("Ver notas").size(12))
            .padding([3, 10])
            .style(tema::botao_secundario)
            .on_press(Msg::AbreUrlAtualizacao(info.url.clone())),
        button(text("Dispensar").size(12))
            .padding([3, 10])
            .style(tema::botao_secundario)
            .on_press(Msg::DispensaAtualizacao),
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    let faixa = container(conteudo)
        .padding([6, 14])
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(c.destaque)),
            border: Border {
                radius: 20.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

    container(faixa)
        .width(Length::Fill)
        .align_x(Alignment::Center)
        .padding([10, 0])
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_do_arquivo_sha256() {
        let h = "a".repeat(64);
        assert_eq!(hash_do_arquivo(&format!("{h}  x.tar.gz\n")), Some(h));
        assert!(hash_do_arquivo("curto  x").is_none());
        assert!(hash_do_arquivo(&"g".repeat(64)).is_none());
    }

    #[test]
    fn release_traz_pacote_e_hash_so_do_repo_de_releases() {
        let json = r#"[{"tag_name":"v9.0.0","html_url":"https://github.com/LinCastr/lincast-chat-releases/releases/tag/v9.0.0","assets":[
          {"name":"lincast-chat-9.0.0-linux-x86_64.tar.gz","browser_download_url":"https://github.com/LinCastr/lincast-chat-releases/releases/download/v9.0.0/lincast-chat-9.0.0-linux-x86_64.tar.gz"},
          {"name":"lincast-chat-9.0.0-linux-x86_64.tar.gz.sha256","browser_download_url":"https://evil.example/x.sha256"}]}]"#;
        let i = interpreta_releases(json, "0.1.0").unwrap();
        assert!(i.pacote.is_some());
        assert!(i.sha256.is_none(), "link fora do repositório é descartado");
    }

    #[test]
    fn testa_comparacao_de_versoes() {
        assert!(eh_mais_nova("v0.2.0", "0.1.0"));
        assert!(eh_mais_nova("0.1.1", "0.1.0"));
        assert!(eh_mais_nova("1.0.0", "0.9.9"));
        assert!(eh_mais_nova("0.10.0", "0.9.0"));
        assert!(!eh_mais_nova("0.1.0", "0.1.0"));
        assert!(!eh_mais_nova("v0.1.0", "0.1.0"));
        assert!(!eh_mais_nova("0.0.9", "0.1.0"));
        assert!(!eh_mais_nova("lixo", "0.1.0"));
    }

    const REPO: &str = "https://github.com/LinCastr/lincast-chat-releases/releases/tag";

    fn rel(tag: &str, asset: &str, extra: &str) -> String {
        format!(
            r#"{{"tag_name":"{tag}","html_url":"{REPO}/{tag}","assets":[{{"name":"{asset}"}}]{extra}}}"#
        )
    }

    #[test]
    fn testa_interpreta_release_nova_do_v2() {
        let j = format!("[{}]", rel("v0.2.0", "Lincast-Chat-x86_64.AppImage", ""));
        let i = interpreta_releases(&j, "0.1.0").expect("há atualização");
        assert_eq!(i.versao, "v0.2.0");
        assert!(i.url.ends_with("v0.2.0"));
    }

    #[test]
    fn testa_nunca_oferece_release_da_v1() {
        // v1.7.4 > 0.1.0 numericamente, mas é o Electron: não pode aparecer.
        let j = format!("[{}]", rel("v1.7.4", "concord-premium-1.7.4.AppImage", ""));
        assert!(interpreta_releases(&j, "0.1.0").is_none());
    }

    #[test]
    fn testa_escolhe_a_mais_nova_entre_as_do_v2() {
        let j = format!(
            "[{},{},{}]",
            rel("v1.7.4", "concord-premium-1.7.4.exe", ""),
            rel("v0.2.0", "lincast-chat-0.2.0.tar.gz", ""),
            rel("v0.3.0", "Lincast-Chat-x86_64.AppImage", "")
        );
        assert_eq!(interpreta_releases(&j, "0.1.0").unwrap().versao, "v0.3.0");
    }

    #[test]
    fn testa_ignora_rascunho_prerelease_e_versao_igual() {
        let rasc = format!(
            "[{}]",
            rel("v9.0.0", "lincast-chat.AppImage", r#","draft":true"#)
        );
        let pre = format!(
            "[{}]",
            rel("v9.0.0", "lincast-chat.AppImage", r#","prerelease":true"#)
        );
        let igual = format!("[{}]", rel("v0.1.0", "lincast-chat.AppImage", ""));
        assert!(interpreta_releases(&rasc, "0.1.0").is_none());
        assert!(interpreta_releases(&pre, "0.1.0").is_none());
        assert!(interpreta_releases(&igual, "0.1.0").is_none());
        assert!(interpreta_releases("não é json", "0.1.0").is_none());
        assert!(interpreta_releases("[]", "0.1.0").is_none());
    }

    #[test]
    fn testa_url_fora_do_repo_vira_pagina_de_releases() {
        let j = r#"[{"tag_name":"v0.2.0","html_url":"https://evil.example/x","assets":[{"name":"lincast-chat.AppImage"}]}]"#;
        let i = interpreta_releases(j, "0.1.0").unwrap();
        assert!(url_permitida(&i.url));
        assert!(!url_permitida("https://evil.example/x"));
        assert!(!url_permitida("file:///etc/passwd"));
    }

    #[test]
    fn testa_banner_instancia_sem_panico() {
        let info = InfoAtualizacao {
            versao: "v0.2.0".into(),
            url: format!("https://github.com/{REPO_RELEASES}/releases/latest"),
            pacote: None,
            sha256: None,
        };
        let _ = banner(&info);
    }
}
