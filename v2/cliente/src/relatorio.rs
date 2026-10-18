//! Relatório de problemas: o log da sessão fica em arquivo e é enviado ao
//! servidor (se a opção estiver ligada) ao fechar o app e na abertura
//! seguinte. Antes de sair do computador, tokens, cookies e senhas de convite
//! são trocados por `[oculto]`.

use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

/// Teto do arquivo de log de uma sessão (o que passar disso é descartado).
const TETO_BYTES: u64 = 2 * 1024 * 1024;

static ENVIAR: AtomicBool = AtomicBool::new(true);
static JA_ENVIOU_NA_ABERTURA: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct Config {
    #[serde(default = "verdadeiro")]
    enviar: bool,
}

fn verdadeiro() -> bool {
    true
}

fn pasta() -> Option<PathBuf> {
    let dir = directories::ProjectDirs::from("", "", "lincast-chat")?;
    Some(dir.config_dir().to_path_buf())
}

fn arquivo_config() -> Option<PathBuf> {
    Some(pasta()?.join("relatorio.json"))
}

fn log_atual() -> Option<PathBuf> {
    Some(pasta()?.join("logs").join("ultimo.log"))
}

fn log_anterior() -> Option<PathBuf> {
    Some(pasta()?.join("logs").join("anterior.log"))
}

pub fn enviar_ligado() -> bool {
    ENVIAR.load(Ordering::Relaxed)
}

/// Lê a opção do disco (ligada por padrão). Chamado uma vez na abertura.
pub fn carrega() {
    let ligado = arquivo_config()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str::<Config>(&t).ok())
        .is_none_or(|c| c.enviar);
    ENVIAR.store(ligado, Ordering::Relaxed);
}

pub fn define_enviar(ligado: bool) {
    ENVIAR.store(ligado, Ordering::Relaxed);
    let Some(p) = arquivo_config() else { return };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(j) = serde_json::to_string(&Config { enviar: ligado }) {
        let _ = std::fs::write(p, j);
    }
}

/// Escreve cada linha do log no terminal e no arquivo da sessão.
struct Duplo {
    arquivo: Option<std::fs::File>,
    escrito: u64,
}

impl Write for Duplo {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let _ = std::io::stderr().write_all(buf);
        if let Some(f) = &mut self.arquivo
            && self.escrito < TETO_BYTES
        {
            if f.write_all(buf).is_ok() {
                self.escrito += buf.len() as u64;
            } else {
                self.arquivo = None;
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if let Some(f) = &mut self.arquivo {
            let _ = f.flush();
        }
        Ok(())
    }
}

/// Prepara o arquivo da sessão (o da sessão anterior, se não foi enviado,
/// vira `anterior.log`) e devolve o destino para o `env_logger`.
pub fn alvo_do_log() -> env_logger::Target {
    let mut arquivo = None;
    if let (Some(atual), Some(ant)) = (log_atual(), log_anterior()) {
        if let Some(dir) = atual.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if atual.exists() {
            let _ = std::fs::rename(&atual, &ant);
        }
        arquivo = std::fs::File::create(&atual).ok();
    }
    env_logger::Target::Pipe(Box::new(Duplo {
        arquivo,
        escrito: 0,
    }))
}

/// Troca por `[oculto]` o que não pode sair do computador.
pub fn limpa(texto: &str) -> String {
    let mut saida = String::with_capacity(texto.len());
    for linha in texto.lines() {
        let mut l = linha.to_string();
        for marca in [
            "session=",
            "#k=",
            "token=",
            "senha=",
            "password=",
            "Cookie: ",
        ] {
            while let Some(i) = l.find(marca) {
                let ini = i + marca.len();
                let fim = l[ini..]
                    .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '&' | ';' | ','))
                    .map_or(l.len(), |n| ini + n);
                if l[ini..fim] == *"[oculto]" {
                    break;
                }
                l.replace_range(ini..fim, "[oculto]");
            }
        }
        saida.push_str(&l);
        saida.push('\n');
    }
    saida
}

fn manda(srv: &crate::api::Servidor, token: &str, caminho: &PathBuf) -> bool {
    let Ok(bruto) = std::fs::read(caminho) else {
        return false;
    };
    let texto = String::from_utf8_lossy(&bruto);
    if texto.trim().is_empty() {
        let _ = std::fs::remove_file(caminho);
        return true;
    }
    match crate::api::envia_relatorio(srv, token, env!("CARGO_PKG_VERSION"), &limpa(&texto)) {
        Ok(()) => {
            let _ = std::fs::remove_file(caminho);
            true
        }
        Err(e) => {
            log::warn!("[relatorio] não consegui enviar: {e:#}");
            false
        }
    }
}

/// Abertura do app com sessão: manda o log da sessão anterior (uma vez).
pub fn envia_anterior(srv: &crate::api::Servidor, token: &str) {
    if !enviar_ligado() || JA_ENVIOU_NA_ABERTURA.swap(true, Ordering::Relaxed) {
        return;
    }
    let (srv, token) = (srv.clone(), token.to_string());
    let _ = std::thread::Builder::new()
        .name("relatorio-anterior".into())
        .spawn(move || {
            if let Some(p) = log_anterior()
                && p.exists()
            {
                manda(&srv, &token, &p);
            }
        });
}

/// Ao fechar: manda o log desta sessão e espera (o servidor responde rápido;
/// o `agente()` já tem teto de 15 s). Se falhar, o arquivo fica e sai na
/// próxima abertura como `anterior.log`.
pub fn envia_ao_fechar(srv: &crate::api::Servidor, token: Option<&str>) {
    if !enviar_ligado() {
        return;
    }
    let (Some(token), Some(p)) = (token, log_atual()) else {
        return;
    };
    log::logger().flush();
    manda(srv, token, &p);
}

#[cfg(test)]
mod testes {
    use super::limpa;

    #[test]
    fn oculta_token_cookie_e_senha_de_convite() {
        let t = limpa("GET x Cookie: session=abc123 depois\nconcord://sala/x#k=ZZZ fim\n");
        assert!(!t.contains("abc123") && !t.contains("ZZZ"));
        assert!(t.contains("depois") && t.contains("fim"));
    }

    #[test]
    fn linha_sem_segredo_passa_igual() {
        assert_eq!(limpa("[ice] ok 1920x1080"), "[ice] ok 1920x1080\n");
    }
}
