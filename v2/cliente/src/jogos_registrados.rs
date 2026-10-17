//! Jogos registrados: os que o app já viu a pessoa jogando e os que ela
//! adicionou à mão (um programa aberto que deve aparecer como "jogando").
//! Ficam em `~/.config/lincast-chat/jogos-registrados.json`. Quando o nome é
//! igual ou bem parecido com o de um jogo do SteamGridDB, guardamos o nome
//! certo e o id da capa.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::api::{JogoAtividade, JogoLista};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct JogoRegistrado {
    /// Nome exibido (o oficial, quando houve correspondência).
    pub nome: String,
    /// Nome do executável a procurar nos processos (só para os manuais).
    #[serde(default)]
    pub exec: Option<String>,
    #[serde(default)]
    pub manual: bool,
    /// Última vez que foi visto jogando (segundos Unix); 0 = nunca.
    #[serde(default)]
    pub ultimo: u64,
    /// Id da capa no SteamGridDB.
    #[serde(default)]
    pub capa: Option<u64>,
    /// Já tentamos achar o nome oficial e a capa.
    #[serde(default)]
    pub procurado: bool,
}

fn caminho() -> Option<PathBuf> {
    let d = directories::ProjectDirs::from("", "", "lincast-chat")?;
    Some(d.config_dir().join("jogos-registrados.json"))
}

fn agora() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn carrega() -> Vec<JogoRegistrado> {
    caminho()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn salva(lista: &[JogoRegistrado]) {
    let Some(p) = caminho() else { return };
    if let Some(dir) = p.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string_pretty(lista) {
        // Escrita atômica: arquivo temporário + rename.
        let tmp = p.with_extension("json.tmp");
        if fs::write(&tmp, json).is_ok() {
            let _ = fs::rename(&tmp, &p);
        }
    }
}

/// Minúsculas, só letras e números (e espaço simples).
fn chave(nome: &str) -> String {
    nome.chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_lowercase().next().unwrap_or(c)
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// 0.0–1.0: 1.0 quando os nomes são iguais (ignorando caixa e pontuação);
/// senão, a fração de palavras em comum (Jaccard), com bônus se um contém o outro.
pub fn similaridade(a: &str, b: &str) -> f32 {
    let (ka, kb) = (chave(a), chave(b));
    if ka.is_empty() || kb.is_empty() {
        return 0.0;
    }
    if ka == kb {
        return 1.0;
    }
    let pa: HashSet<&str> = ka.split(' ').collect();
    let pb: HashSet<&str> = kb.split(' ').collect();
    let comum = pa.intersection(&pb).count() as f32;
    let total = pa.union(&pb).count() as f32;
    let mut s = comum / total;
    if ka.contains(&kb) || kb.contains(&ka) {
        s = s.max(0.75);
    }
    s
}

/// O resultado da busca mais parecido com `nome`, se for bem parecido.
pub fn melhor_parecido<'a>(nome: &str, candidatos: &'a [JogoLista]) -> Option<&'a JogoLista> {
    candidatos
        .iter()
        .map(|j| (similaridade(nome, &j.nome), j))
        .filter(|(s, _)| *s >= 0.7)
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, j)| j)
}

/// "my_game-x64.exe" → "My Game X64".
pub fn nome_sugerido(exec: &str) -> String {
    let limpo = exec.strip_suffix(".exe").unwrap_or(exec);
    limpo
        .split(['_', '-', '.'])
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut c = p.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Executáveis dos processos do usuário (nome do arquivo, minúsculo, sem
/// repetir), sem o que é claramente infraestrutura.
pub fn processos_abertos() -> Vec<String> {
    let Ok(dir) = fs::read_dir("/proc") else {
        return Vec::new();
    };
    // Só os processos do próprio usuário (o dono do /proc/<pid> é o dono do processo).
    let meu_uid = fs::metadata("/proc/self")
        .map(|m| std::os::unix::fs::MetadataExt::uid(&m))
        .ok();
    let mut vistos = HashSet::new();
    for e in dir.flatten() {
        let nome = e.file_name();
        let Some(pid) = nome
            .to_str()
            .filter(|s| s.bytes().all(|b| b.is_ascii_digit()))
        else {
            continue;
        };
        let p = format!("/proc/{pid}");
        if let (Some(uid), Ok(m)) = (meu_uid, fs::metadata(&p))
            && std::os::unix::fs::MetadataExt::uid(&m) != uid
        {
            continue;
        }
        let Ok(cmd) = fs::read(format!("{p}/cmdline")) else {
            continue;
        };
        let Some(arg0) = cmd.split(|&b| b == 0).next() else {
            continue;
        };
        let Ok(arg0) = std::str::from_utf8(arg0) else {
            continue;
        };
        // Alguns programas reescrevem o cmdline inteiro (Electron): fica só o 1º termo.
        let arg0 = arg0.split_whitespace().next().unwrap_or("");
        let base = arg0.rsplit(['/', '\\']).next().unwrap_or("").to_lowercase();
        let nome_valido = base
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+'));
        if base.is_empty()
            || base.len() > 40
            || !nome_valido
            || !base.starts_with(|c: char| c.is_ascii_alphanumeric())
            || crate::atividade::exe_nao_e_jogo(&base)
            || arg0.contains("/windows/system32/")
            || arg0.contains("/windows/syswow64/")
        {
            continue;
        }
        vistos.insert(base);
    }
    let mut v: Vec<String> = vistos.into_iter().collect();
    v.sort();
    v
}

/// Troca nome e capa pelos do jogo registrado que for igual ou bem parecido.
pub fn enriquece(jogo: &mut JogoAtividade) {
    let lista = carrega();
    if let Some(r) = lista
        .iter()
        .filter(|r| r.capa.is_some() || r.manual)
        .map(|r| (similaridade(&jogo.nome, &r.nome), r))
        .filter(|(s, _)| *s >= 0.85)
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, r)| r)
    {
        jogo.nome = r.nome.clone();
        jogo.capa = r.capa;
    }
}

/// Anota que o jogo foi visto agora (cria a entrada se for novo).
pub fn registra_visto(jogo: &JogoAtividade) {
    let mut lista = carrega();
    let agora = agora();
    let k = chave(&jogo.nome);
    if let Some(r) = lista.iter_mut().find(|r| chave(&r.nome) == k) {
        if agora.saturating_sub(r.ultimo) >= 60 {
            r.ultimo = agora;
            salva(&lista);
        }
        return;
    }
    lista.push(JogoRegistrado {
        nome: jogo.nome.clone(),
        exec: None,
        manual: false,
        ultimo: agora,
        capa: jogo.capa,
        procurado: jogo.capa.is_some(),
    });
    salva(&lista);
}

/// O jogo existe na lista e ainda não teve nome oficial/capa procurados?
pub fn precisa_procurar(nome: &str) -> bool {
    let k = chave(nome);
    carrega()
        .iter()
        .any(|r| chave(&r.nome) == k && r.capa.is_none() && !r.procurado)
}

/// Guarda o resultado da busca de capa (ou que não houve) para o jogo.
pub fn define_capa(nome: &str, oficial: Option<&JogoLista>) {
    let mut lista = carrega();
    let k = chave(nome);
    if let Some(r) = lista.iter_mut().find(|r| chave(&r.nome) == k) {
        r.procurado = true;
        if let Some(o) = oficial {
            r.nome = o.nome.clone();
            r.capa = Some(o.id);
        }
        salva(&lista);
    }
}

/// Adiciona (ou atualiza) um jogo manual.
pub fn adiciona_manual(exec: &str, nome: String, capa: Option<u64>) {
    let mut lista = carrega();
    lista.retain(|r| r.exec.as_deref() != Some(exec));
    lista.push(JogoRegistrado {
        nome,
        exec: Some(exec.to_string()),
        manual: true,
        ultimo: 0,
        capa,
        procurado: true,
    });
    salva(&lista);
}

pub fn remove(nome: &str) {
    let mut lista = carrega();
    let k = chave(nome);
    lista.retain(|r| chave(&r.nome) != k);
    salva(&lista);
}

/// Texto "há 3 horas" para a última vez jogado.
pub fn ha_quanto_tempo(ultimo: u64) -> String {
    if ultimo == 0 {
        return "Ainda não jogado".into();
    }
    let s = agora().saturating_sub(ultimo);
    let (n, un) = match s {
        0..=59 => return "Jogado agora há pouco".into(),
        60..=3599 => (s / 60, if s / 60 == 1 { "minuto" } else { "minutos" }),
        3600..=86399 => (s / 3600, if s / 3600 == 1 { "hora" } else { "horas" }),
        _ => (s / 86400, if s / 86400 == 1 { "dia" } else { "dias" }),
    };
    format!("Jogado pela última vez há {n} {un}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nomes_iguais_ou_parecidos() {
        assert_eq!(
            similaridade("God of War III: Remastered", "god of war iii remastered"),
            1.0
        );
        assert!(similaridade("Minecraft", "Minecraft - FreesmLauncher") >= 0.75);
        assert!(similaridade("Celeste", "Hollow Knight") < 0.3);
    }

    #[test]
    fn melhor_parecido_ignora_o_que_nao_se_parece() {
        let c = vec![
            JogoLista {
                id: 1,
                nome: "Hollow Knight".into(),
            },
            JogoLista {
                id: 2,
                nome: "Celeste".into(),
            },
        ];
        assert_eq!(melhor_parecido("celeste", &c).map(|j| j.id), Some(2));
        assert!(melhor_parecido("Outra Coisa", &c).is_none());
    }

    #[test]
    fn sugere_nome_do_executavel() {
        assert_eq!(nome_sugerido("hollow_knight.exe"), "Hollow Knight");
        assert_eq!(nome_sugerido("osu-lazer"), "Osu Lazer");
    }

    #[test]
    fn tempo_legivel() {
        assert_eq!(ha_quanto_tempo(0), "Ainda não jogado");
    }
}
