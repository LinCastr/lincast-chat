//! Lista pública de jogos detectáveis do Discord (`/api/v9/applications/detectable`).
//!
//! Baixada em segundo plano e reduzida a um índice executável → nome do jogo,
//! guardado em `~/.config/lincast-chat/jogos-detectaveis.json` e renovado a cada 7 dias.
//! Sem a lista (offline, primeira execução) a detecção segue só com o que já existia.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

const URL: &str = "https://discord.com/api/v9/applications/detectable";
const VALIDADE_S: u64 = 7 * 24 * 3600;

/// nome do arquivo (minúsculo) → [(sufixo de caminho exigido, jogo)].
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Indice {
    pub baixado_em: u64,
    pub jogos: HashMap<String, Vec<(String, String)>>,
}

/// Normaliza caminho de executável: minúsculo, `\` vira `/`, sem `>` inicial.
pub fn normaliza(caminho: &str) -> String {
    caminho
        .trim()
        .trim_start_matches('>')
        .replace('\\', "/")
        .to_lowercase()
}

fn nome_do_arquivo(caminho_normalizado: &str) -> &str {
    caminho_normalizado.rsplit('/').next().unwrap_or("")
}

/// Reduz o JSON do Discord (uns 13 MB) ao índice. Ignora launchers e macOS.
pub fn compacta(lista: &Value, agora: u64) -> Indice {
    let mut jogos: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for app in lista.as_array().into_iter().flatten() {
        let Some(nome) = app["name"].as_str() else {
            continue;
        };
        for exe in app["executables"].as_array().into_iter().flatten() {
            if exe["is_launcher"].as_bool().unwrap_or(false) {
                continue;
            }
            if !matches!(exe["os"].as_str(), Some("win32" | "linux")) {
                continue;
            }
            let Some(caminho) = exe["name"].as_str() else {
                continue;
            };
            let caminho = normaliza(caminho);
            let arquivo = nome_do_arquivo(&caminho).to_string();
            if arquivo.is_empty() {
                continue;
            }
            let item = (caminho, nome.to_string());
            let v = jogos.entry(arquivo).or_default();
            if !v.contains(&item) {
                v.push(item);
            }
        }
    }
    Indice {
        baixado_em: agora,
        jogos,
    }
}

impl Indice {
    /// Jogo cujo executável termina o caminho do processo (em fronteira de pasta).
    pub fn acha(&self, caminho_do_processo: &str) -> Option<&str> {
        let caminho = normaliza(caminho_do_processo);
        let candidatos = self.jogos.get(nome_do_arquivo(&caminho))?;
        // Entrada com pasta ("star conflict/game.exe") tem prioridade sobre a só com nome.
        let com_pasta = candidatos.iter().find(|(suf, _)| {
            suf.contains('/') && (caminho == *suf || caminho.ends_with(&format!("/{suf}")))
        });
        let escolhido = com_pasta.or_else(|| candidatos.iter().find(|(s, _)| !s.contains('/')))?;
        Some(escolhido.1.as_str())
    }
}

fn arquivo() -> Option<PathBuf> {
    let d = directories::ProjectDirs::from("", "", "lincast-chat")?;
    Some(d.config_dir().join("jogos-detectaveis.json"))
}

fn agora() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn le_do_disco() -> Option<Indice> {
    let bytes = std::fs::read(arquivo()?).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn salva_no_disco(i: &Indice) {
    let Some(p) = arquivo() else { return };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // Escrita atômica: temporário + rename.
    let tmp = p.with_extension("json.tmp");
    if let Ok(bytes) = serde_json::to_vec(i)
        && std::fs::write(&tmp, bytes).is_ok()
    {
        let _ = std::fs::rename(&tmp, &p);
    }
}

fn baixa() -> Option<Indice> {
    let agente: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(90)))
        .build()
        .into();
    let mut resp = agente.get(URL).call().ok()?;
    let lista: Value = resp
        .body_mut()
        .with_config()
        .limit(64 * 1024 * 1024)
        .read_json()
        .ok()?;
    Some(compacta(&lista, agora()))
}

static ATUAL: LazyLock<RwLock<Option<Arc<Indice>>>> = LazyLock::new(|| RwLock::new(None));
static CARREGOU: AtomicBool = AtomicBool::new(false);
static BAIXANDO: AtomicBool = AtomicBool::new(false);

/// O índice atual (do disco na primeira chamada). Dispara a renovação em segundo
/// plano quando falta ou venceu; nunca bloqueia a varredura.
pub fn indice() -> Option<Arc<Indice>> {
    if !CARREGOU.swap(true, Ordering::SeqCst)
        && let Some(i) = le_do_disco()
        && let Ok(mut g) = ATUAL.write()
    {
        *g = Some(Arc::new(i));
    }
    let atual = ATUAL.read().ok()?.clone();
    let vencida = atual
        .as_ref()
        .is_none_or(|i| agora().saturating_sub(i.baixado_em) > VALIDADE_S);
    if vencida && !BAIXANDO.swap(true, Ordering::SeqCst) {
        std::thread::spawn(|| {
            // Falha (sem rede) deixa BAIXANDO ligado: uma tentativa por execução.
            if let Some(novo) = baixa() {
                salva_no_disco(&novo);
                if let Ok(mut g) = ATUAL.write() {
                    *g = Some(Arc::new(novo));
                }
                BAIXANDO.store(false, Ordering::SeqCst);
            }
        });
    }
    atual
}

#[cfg(test)]
mod testes {
    use super::*;

    fn amostra() -> Indice {
        let lista = serde_json::json!([
            {"name": "Overwatch", "executables": [{"name": "Overwatch.exe", "os": "win32", "is_launcher": false}]},
            {"name": "Star Conflict", "executables": [{"name": "star conflict/game.exe", "os": "win32", "is_launcher": false}]},
            {"name": "Outro", "executables": [{"name": "game.exe", "os": "win32", "is_launcher": false}]},
            {"name": "Launcher X", "executables": [{"name": "lx.exe", "os": "win32", "is_launcher": true}]},
            {"name": "Mac", "executables": [{"name": "foo_osx", "os": "darwin", "is_launcher": false}]},
            {"name": "HL2", "executables": [{"name": ">hl2.exe", "os": "win32", "is_launcher": false}]}
        ]);
        compacta(&lista, 1)
    }

    #[test]
    fn acha_por_nome_e_caminho_wine() {
        let i = amostra();
        assert_eq!(
            i.acha(r"C:\Games\Overwatch\Overwatch.exe"),
            Some("Overwatch")
        );
        assert_eq!(i.acha("/home/x/hl2.exe"), Some("HL2"));
        assert_eq!(i.acha("lx.exe"), None);
        assert_eq!(i.acha("foo_osx"), None);
    }

    #[test]
    fn entrada_com_pasta_tem_prioridade() {
        let i = amostra();
        assert_eq!(
            i.acha(r"Z:\games\Star Conflict\game.exe"),
            Some("Star Conflict")
        );
        assert_eq!(i.acha(r"Z:\games\Qualquer\game.exe"), Some("Outro"));
    }

    /// Manual: `LISTA_DISCORD=<arquivo> cargo test -- --ignored lista_real`.
    #[test]
    #[ignore]
    fn lista_real() {
        let caminho = std::env::var("LISTA_DISCORD").unwrap();
        let v: Value = serde_json::from_slice(&std::fs::read(caminho).unwrap()).unwrap();
        let i = compacta(&v, 1);
        assert!(i.acha(r"C:\x\Overwatch.exe").is_some());
        println!("arquivos: {}", i.jogos.len());
    }
}
