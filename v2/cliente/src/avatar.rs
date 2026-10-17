//! Cache de imagens de perfil e de grupo (avatar, ícone).
//!
//! Duas camadas: memória (`Handle` pronto para o iced) e disco
//! (`<cache>/lincast-chat/avatares/<hash>.bin`). A chave é a URL inteira, que
//! já traz o `?v=` do servidor: foto nova é URL nova, então nada precisa ser
//! invalidado à mão. Acesso global, igual ao tema, para o desenho (que só tem
//! `&self`) poder consultar sem receber o cache por parâmetro.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{LazyLock, RwLock};
use std::time::{Duration, SystemTime};

use iced::widget::image::Handle;

/// Arquivo de cache sem uso por mais que isso é apagado na abertura.
const VALIDADE_NO_DISCO: Duration = Duration::from_secs(30 * 24 * 3600);

#[derive(Default)]
pub struct CacheDeImagens {
    memoria: HashMap<String, Handle>,
    pendentes: HashSet<String>,
    falhas: HashSet<String>,
}

static CACHE: LazyLock<RwLock<CacheDeImagens>> =
    LazyLock::new(|| RwLock::new(CacheDeImagens::default()));

/// A imagem pronta para desenhar, se já está na memória.
pub fn handle(url: &str) -> Option<Handle> {
    CACHE.read().ok()?.memoria.get(url).cloned()
}

/// O que desenhar agora: o quadro atual se for GIF animado (e puder andar),
/// senão a imagem parada. Use no lugar de `handle` ao desenhar.
pub fn handle_exibicao(url: &str) -> Option<Handle> {
    crate::gifanim::quadro_atual(url).or_else(|| handle(url))
}

/// `true` quando esta URL ainda não foi pedida e deve ser buscada agora.
/// Marca como pendente: duas chamadas seguidas não pedem duas vezes.
pub fn precisa_baixar(url: &str) -> bool {
    if url.is_empty() {
        return false;
    }
    let Ok(mut c) = CACHE.write() else {
        return false;
    };
    if c.memoria.contains_key(url) || c.pendentes.contains(url) || c.falhas.contains(url) {
        return false;
    }
    c.pendentes.insert(url.to_string());
    true
}

/// Guarda o resultado de um download. Bytes que não decodificam como imagem
/// contam como falha (e não são gravados no disco).
pub fn guarda(url: &str, bytes: Result<Vec<u8>, String>) {
    let Ok(mut c) = CACHE.write() else {
        return;
    };
    c.pendentes.remove(url);
    match bytes {
        Ok(b) if image::load_from_memory(&b).is_ok() => {
            grava_no_disco(url, &b);
            crate::gifanim::prepara(url, b.clone());
            c.memoria.insert(url.to_string(), Handle::from_bytes(b));
        }
        Ok(_) => {
            log::warn!("[avatar] {url}: resposta não é uma imagem");
            c.falhas.insert(url.to_string());
        }
        Err(e) => {
            log::warn!("[avatar] {url}: {e}");
            c.falhas.insert(url.to_string());
        }
    }
}

/// Lê do disco, se houver. Usado antes da rede, na tarefa de fundo.
pub fn le_do_disco(url: &str) -> Option<Vec<u8>> {
    std::fs::read(caminho_no_disco(url)?).ok()
}

fn grava_no_disco(url: &str, bytes: &[u8]) {
    if let Some(p) = caminho_no_disco(url) {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(p, bytes);
    }
}

fn dir_do_cache() -> Option<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "lincast-chat")?;
    Some(dirs.cache_dir().join("avatares"))
}

/// FNV-1a de 64 bits. Estável entre execuções (o `DefaultHasher` não é), e
/// basta: é só um nome de arquivo.
pub fn hash_texto(url: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in url.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn caminho_no_disco(url: &str) -> Option<PathBuf> {
    Some(dir_do_cache()?.join(format!("{:016x}.bin", hash_texto(url))))
}

/// Remove do disco o que não é usado há muito tempo. Barato; roda uma vez.
pub fn limpa_cache_antigo() {
    let Some(dir) = dir_do_cache() else {
        return;
    };
    let Ok(lista) = std::fs::read_dir(dir) else {
        return;
    };
    let agora = SystemTime::now();
    for e in lista.flatten() {
        let velho = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| agora.duration_since(t).ok())
            .is_some_and(|d| d > VALIDADE_NO_DISCO);
        if velho {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn hash_e_estavel_e_distingue_versoes() {
        let a = "/api/conta/avatar/ana?v=aaaa1111";
        assert_eq!(hash_texto(a), hash_texto(a));
        assert_ne!(
            hash_texto(a),
            hash_texto("/api/conta/avatar/ana?v=bbbb2222")
        );
    }

    #[test]
    fn pede_uma_vez_so_e_lembra_da_falha() {
        let u = "/teste/avatar-unico?v=1";
        assert!(precisa_baixar(u));
        assert!(!precisa_baixar(u), "já está pendente");
        guarda(u, Err("sem rede".into()));
        assert!(!precisa_baixar(u), "falha não é repetida em laço");
        assert!(handle(u).is_none());
        assert!(!precisa_baixar(""));
    }

    #[test]
    fn bytes_que_nao_sao_imagem_viram_falha() {
        let u = "/teste/lixo?v=1";
        assert!(precisa_baixar(u));
        guarda(u, Ok(b"isto nao e uma imagem".to_vec()));
        assert!(handle(u).is_none());
        assert!(!precisa_baixar(u));
    }
}
