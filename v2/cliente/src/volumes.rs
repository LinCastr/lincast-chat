//! Persistência de volumes individuais por participante.
//!
//! Guarda em `~/.config/lincast-chat/volumes.json` um mapa de chave -> volume (0.0 a 2.0).
//! A chave é o `uid` da conta quando disponível, ou o `nome` como fallback (mesma regra da v1).
//! Áudio de transmissão de tela usa o sufixo `::tela`.

use std::collections::HashMap;
use std::path::PathBuf;

const PADRAO: f32 = 1.0;

fn arquivo_volumes() -> Option<PathBuf> {
    let novo_dir = directories::ProjectDirs::from("", "", "lincast-chat")?;
    let novo_arq = novo_dir.config_dir().join("volumes.json");

    if !novo_arq.exists()
        && let Some(velho_dir) = directories::ProjectDirs::from("", "", "concord")
    {
        let velho_arq = velho_dir.config_dir().join("volumes.json");
        if velho_arq.exists() {
            if let Some(parent) = novo_arq.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::copy(&velho_arq, &novo_arq);
        }
    }

    Some(novo_arq)
}

pub fn chave_para(uid: Option<&str>, nome: &str, de_tela: bool) -> Option<String> {
    let base = uid.filter(|s| !s.is_empty()).unwrap_or(nome);
    if base.is_empty() {
        return None;
    }
    if de_tela {
        Some(format!("{base}::tela"))
    } else {
        Some(base.to_string())
    }
}

pub fn carrega_todos() -> HashMap<String, f32> {
    let Some(caminho) = arquivo_volumes() else {
        return HashMap::new();
    };
    let Ok(conteudo) = std::fs::read_to_string(caminho) else {
        return HashMap::new();
    };
    serde_json::from_str(&conteudo).unwrap_or_default()
}

pub fn obter(uid: Option<&str>, nome: &str, de_tela: bool) -> f32 {
    let todos = carrega_todos();
    if let Some(chave) = chave_para(uid, nome, de_tela)
        && let Some(&vol) = todos.get(&chave)
    {
        return vol.clamp(0.0, 2.0);
    }
    // Fallback: se temos uid mas não achou nele, tenta buscar pela chave antiga (só o nome)
    if uid.is_some()
        && let Some(chave_antiga) = chave_para(None, nome, de_tela)
        && let Some(&vol) = todos.get(&chave_antiga)
    {
        return vol.clamp(0.0, 2.0);
    }
    PADRAO
}

pub fn salvar(uid: Option<&str>, nome: &str, de_tela: bool, volume: f32) {
    let Some(caminho) = arquivo_volumes() else {
        return;
    };
    let Some(chave) = chave_para(uid, nome, de_tela) else {
        return;
    };

    let mut todos = carrega_todos();
    let volume = volume.clamp(0.0, 2.0);

    // Se salvamos com uid, limpa chave legada por nome para evitar conflito com homônimos
    if uid.is_some()
        && let Some(chave_antiga) = chave_para(None, nome, de_tela)
        && chave_antiga != chave
    {
        todos.remove(&chave_antiga);
    }

    // 1.0 é o padrão; podemos guardar mesmo assim para ser explícito,
    // ou manter mapeado. Guardamos para persistir a intenção do usuário.
    todos.insert(chave, volume);

    if let Some(parent) = caminho.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    if let Ok(json) = serde_json::to_string_pretty(&todos) {
        let _ = std::fs::write(caminho, json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn testa_chave_para_uid_e_nome() {
        assert_eq!(
            chave_para(Some("usr_123"), "Gabriel", false),
            Some("usr_123".to_string())
        );
        assert_eq!(
            chave_para(Some("usr_123"), "Gabriel", true),
            Some("usr_123::tela".to_string())
        );
        assert_eq!(
            chave_para(None, "Gabriel", false),
            Some("Gabriel".to_string())
        );
        assert_eq!(
            chave_para(None, "Gabriel", true),
            Some("Gabriel::tela".to_string())
        );
        assert_eq!(
            chave_para(Some(""), "Gabriel", false),
            Some("Gabriel".to_string())
        );
        assert_eq!(chave_para(None, "", false), None);
    }
}
