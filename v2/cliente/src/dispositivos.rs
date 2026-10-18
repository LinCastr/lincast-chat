//! Módulo de dispositivos de áudio (microfone e reprodução).
//!
//! Permite listar entradas e saídas de áudio disponíveis via WebRTC ADM,
//! selecionar dispositivo ativo por índice ou GUID estável e persistir as
//! escolhas do usuário em `~/.config/lincast-chat/dispositivos.json`.

use libwebrtc::peer_connection_factory::PeerConnectionFactory;
use libwebrtc::peer_connection_factory::native::PeerConnectionFactoryExt;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DispositivoAudio {
    pub indice: u16,
    pub nome: String,
    pub guid: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConfigDispositivos {
    pub entrada_guid: Option<String>,
    pub entrada_nome: Option<String>,
    pub saida_guid: Option<String>,
    pub saida_nome: Option<String>,
}

fn arquivo_config() -> Option<PathBuf> {
    let dir = directories::ProjectDirs::from("", "", "lincast-chat")?;
    Some(dir.config_dir().join("dispositivos.json"))
}

pub fn carrega_config() -> ConfigDispositivos {
    let Some(arq) = arquivo_config() else {
        return ConfigDispositivos::default();
    };
    let Ok(conteudo) = std::fs::read_to_string(arq) else {
        return ConfigDispositivos::default();
    };
    serde_json::from_str(&conteudo).unwrap_or_default()
}

pub fn salva_config(config: &ConfigDispositivos) {
    let Some(arq) = arquivo_config() else {
        return;
    };
    if let Some(parent) = arq.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(config) {
        let _ = std::fs::write(arq, json);
    }
}

/// Lista todas as entradas (microfones) e saídas (fones/alto-falantes) disponíveis.
pub fn lista_dispositivos(
    fabrica: &PeerConnectionFactory,
) -> (Vec<DispositivoAudio>, Vec<DispositivoAudio>) {
    let mut entradas = Vec::new();
    let num_rec = fabrica.recording_devices();
    for i in 0..num_rec.max(0) as u16 {
        let nome = fabrica.recording_device_name(i);
        let guid = fabrica.recording_device_guid(i);
        entradas.push(DispositivoAudio {
            indice: i,
            nome,
            guid,
        });
    }

    let mut saidas = Vec::new();
    let num_play = fabrica.playout_devices();
    for i in 0..num_play.max(0) as u16 {
        let nome = fabrica.playout_device_name(i);
        let guid = fabrica.playout_device_guid(i);
        saidas.push(DispositivoAudio {
            indice: i,
            nome,
            guid,
        });
    }

    (entradas, saidas)
}

/// Lista dispositivos fora de uma chamada ativa (adquire e libera o ADM da fábrica temporária).
pub fn lista_dispositivos_avulso() -> (Vec<DispositivoAudio>, Vec<DispositivoAudio>) {
    let fabrica = PeerConnectionFactory::default();
    let _ = fabrica.acquire_platform_adm();
    let res = lista_dispositivos(&fabrica);
    fabrica.release_platform_adm();
    res
}

/// Salva dispositivo de gravação e atualiza o nome correspondente.
pub fn salva_dispositivo_gravacao(guid: &str) {
    let mut cfg = carrega_config();
    cfg.entrada_guid = Some(guid.to_string());
    let (entradas, _) = lista_dispositivos_avulso();
    if let Some(d) = entradas.iter().find(|d| d.guid == guid) {
        cfg.entrada_nome = Some(d.nome.clone());
    }
    salva_config(&cfg);
}

/// Salva dispositivo de reprodução e atualiza o nome correspondente.
pub fn salva_dispositivo_reproducao(guid: &str) {
    let mut cfg = carrega_config();
    cfg.saida_guid = Some(guid.to_string());
    let (_, saidas) = lista_dispositivos_avulso();
    if let Some(d) = saidas.iter().find(|d| d.guid == guid) {
        cfg.saida_nome = Some(d.nome.clone());
    }
    salva_config(&cfg);
}

/// Aplica o dispositivo de gravação (microfone).
pub fn aplica_dispositivo_gravacao(fabrica: &PeerConnectionFactory, guid: &str) -> bool {
    fabrica.set_recording_device_by_guid(guid)
}

/// Aplica o dispositivo de reprodução (saída).
pub fn aplica_dispositivo_reproducao(fabrica: &PeerConnectionFactory, guid: &str) -> bool {
    fabrica.set_playout_device_by_guid(guid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn testa_serializacao_config_dispositivos() {
        let cfg = ConfigDispositivos {
            entrada_guid: Some("mic-guid-1".into()),
            entrada_nome: Some("Microfone USB".into()),
            saida_guid: Some("fone-guid-2".into()),
            saida_nome: Some("Fone de Ouvido".into()),
        };

        let json = serde_json::to_string(&cfg).expect("serializa");
        let recuperado: ConfigDispositivos = serde_json::from_str(&json).expect("deserializa");
        assert_eq!(recuperado.entrada_guid.as_deref(), Some("mic-guid-1"));
        assert_eq!(recuperado.entrada_nome.as_deref(), Some("Microfone USB"));
        assert_eq!(recuperado.saida_guid.as_deref(), Some("fone-guid-2"));
        assert_eq!(recuperado.saida_nome.as_deref(), Some("Fone de Ouvido"));
    }
}
