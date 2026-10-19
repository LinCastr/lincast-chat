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
    /// Microfone pelo RNNoise (`concord-audio microfone`) em vez do libwebrtc.
    #[serde(default)]
    pub supressao_ruido: bool,
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

/// Prefixo da chave de um dispositivo que o libwebrtc devolveu sem GUID.
const CHAVE_POR_NOME: &str = "nome:";

/// Chave estável do dispositivo. No Linux (PulseAudio/PipeWire) o libwebrtc
/// devolve o GUID **vazio** para todos: com ele como chave, todos ficavam
/// iguais, a escolha não era lembrada e o aplicado era sempre o primeiro. Sem
/// GUID, vale o nome; o "default: <nome do padrão atual>" vira só "default",
/// porque o nome dele muda quando o padrão do sistema muda.
pub fn chave_do_dispositivo(nome: &str, guid: &str) -> String {
    if !guid.is_empty() {
        guid.to_string()
    } else if nome == "default" || nome.starts_with("default:") {
        format!("{CHAVE_POR_NOME}default")
    } else {
        format!("{CHAVE_POR_NOME}{nome}")
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
        let guid = chave_do_dispositivo(&nome, &fabrica.recording_device_guid(i));
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
        let guid = chave_do_dispositivo(&nome, &fabrica.playout_device_guid(i));
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

/// Salva o dispositivo de gravação escolhido. O nome vem da lista que a
/// tela já tem: enumerar de novo aqui abria um segundo ADM da plataforma.
pub fn salva_dispositivo_gravacao(guid: &str, nome: Option<&str>) {
    let mut cfg = carrega_config();
    cfg.entrada_guid = Some(guid.to_string());
    if let Some(n) = nome {
        cfg.entrada_nome = Some(n.to_string());
    }
    salva_config(&cfg);
}

/// Salva o dispositivo de reprodução escolhido (ver o de gravação).
pub fn salva_dispositivo_reproducao(guid: &str, nome: Option<&str>) {
    let mut cfg = carrega_config();
    cfg.saida_guid = Some(guid.to_string());
    if let Some(n) = nome {
        cfg.saida_nome = Some(n.to_string());
    }
    salva_config(&cfg);
}

/// Índice atual do dispositivo com esta chave (a lista muda quando se pluga algo).
fn indice_da_chave(lista: &[DispositivoAudio], chave: &str) -> Option<u16> {
    lista.iter().find(|d| d.guid == chave).map(|d| d.indice)
}

/// Aplica o dispositivo de gravação (microfone). Chave vazia (config antiga,
/// de quando o GUID vinha vazio) não aplica nada: fica o padrão do sistema.
pub fn aplica_dispositivo_gravacao(fabrica: &PeerConnectionFactory, guid: &str) -> bool {
    if !guid.starts_with(CHAVE_POR_NOME) {
        return !guid.is_empty() && fabrica.set_recording_device_by_guid(guid);
    }
    let (entradas, _) = lista_dispositivos(fabrica);
    indice_da_chave(&entradas, guid).is_some_and(|i| troca_gravacao(fabrica, i))
}

/// O ADM do PulseAudio recusa trocar de microfone com a gravação já iniciada
/// (no meio da chamada): para, troca e volta a gravar.
fn troca_gravacao(fabrica: &PeerConnectionFactory, indice: u16) -> bool {
    if fabrica.set_recording_device(indice) {
        return true;
    }
    if !fabrica.recording_is_initialized() {
        return false;
    }
    fabrica.stop_recording();
    let ok = fabrica.set_recording_device(indice);
    fabrica.init_recording();
    fabrica.start_recording();
    ok
}

/// Mesmo caso da saída: com a reprodução iniciada, para, troca e volta.
fn troca_reproducao(fabrica: &PeerConnectionFactory, indice: u16) -> bool {
    if fabrica.set_playout_device(indice) {
        return true;
    }
    if !fabrica.playout_is_initialized() {
        return false;
    }
    fabrica.stop_playout();
    let ok = fabrica.set_playout_device(indice);
    fabrica.init_playout();
    fabrica.start_playout();
    ok
}

/// Aplica o dispositivo de reprodução (saída).
pub fn aplica_dispositivo_reproducao(fabrica: &PeerConnectionFactory, guid: &str) -> bool {
    if !guid.starts_with(CHAVE_POR_NOME) {
        return !guid.is_empty() && fabrica.set_playout_device_by_guid(guid);
    }
    let (_, saidas) = lista_dispositivos(fabrica);
    indice_da_chave(&saidas, guid).is_some_and(|i| troca_reproducao(fabrica, i))
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
            supressao_ruido: true,
        };

        let json = serde_json::to_string(&cfg).expect("serializa");
        let recuperado: ConfigDispositivos = serde_json::from_str(&json).expect("deserializa");
        assert_eq!(recuperado.entrada_guid.as_deref(), Some("mic-guid-1"));
        assert_eq!(recuperado.entrada_nome.as_deref(), Some("Microfone USB"));
        assert_eq!(recuperado.saida_guid.as_deref(), Some("fone-guid-2"));
        assert_eq!(recuperado.saida_nome.as_deref(), Some("Fone de Ouvido"));
    }

    #[test]
    fn chave_sem_guid_usa_o_nome() {
        // Linux: o libwebrtc devolve GUID vazio para todos os dispositivos.
        assert_eq!(
            chave_do_dispositivo("MicFix-Headset", ""),
            "nome:MicFix-Headset"
        );
        assert_ne!(
            chave_do_dispositivo("MicFix-Headset", ""),
            chave_do_dispositivo("Áudio interno Estéreo analógico", "")
        );
        // O "default" carrega o nome do padrão atual, que muda.
        assert_eq!(
            chave_do_dispositivo("default: Áudio interno Estéreo analógico", ""),
            "nome:default"
        );
        // Com GUID de verdade (Windows/macOS), ele é a chave.
        assert_eq!(chave_do_dispositivo("Mic", "{abc}"), "{abc}");
        let lista = vec![
            DispositivoAudio {
                indice: 0,
                nome: "default: X".into(),
                guid: chave_do_dispositivo("default: X", ""),
            },
            DispositivoAudio {
                indice: 1,
                nome: "MicFix-Headset".into(),
                guid: chave_do_dispositivo("MicFix-Headset", ""),
            },
        ];
        assert_eq!(indice_da_chave(&lista, "nome:MicFix-Headset"), Some(1));
        assert_eq!(indice_da_chave(&lista, "nome:sumiu"), None);
    }

    /// Diagnóstico com o áudio de verdade da máquina (não roda no CI):
    /// `cargo test -p lincast-chat enumera_real -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn enumera_real() {
        let mostra = |rotulo: &str, (e, s): &(Vec<DispositivoAudio>, Vec<DispositivoAudio>)| {
            println!("{rotulo}: {} entradas, {} saídas", e.len(), s.len());
            for d in e.iter().chain(s) {
                println!("   [{}] {:?} guid={:?}", d.indice, d.nome, d.guid);
            }
        };
        for i in 0..3 {
            mostra(&format!("avulso #{i}"), &lista_dispositivos_avulso());
        }
        // Como na chamada: uma fábrica com o ADM aberto enquanto outra enumera.
        let chamada = PeerConnectionFactory::default();
        println!("ADM da chamada: {}", chamada.acquire_platform_adm());
        mostra("na chamada", &lista_dispositivos(&chamada));
        for i in 0..3 {
            mostra(
                &format!("avulso com chamada #{i}"),
                &lista_dispositivos_avulso(),
            );
        }
        mostra("na chamada, depois", &lista_dispositivos(&chamada));
        chamada.release_platform_adm();
    }

    /// Troca de microfone com a gravação rodando, como no meio da chamada:
    /// `cargo test -p lincast-chat troca_mic_real -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn troca_mic_real() {
        let f = PeerConnectionFactory::default();
        assert!(f.acquire_platform_adm());
        let (entradas, _) = lista_dispositivos(&f);
        println!("init={} start={}", f.init_recording(), f.start_recording());
        println!("gravando: {}", f.recording_is_initialized());
        for d in &entradas {
            println!(
                "set_recording_device({}) direto: {}",
                d.indice,
                f.set_recording_device(d.indice)
            );
            println!(
                "aplica {:?}: {} (gravando depois: {})",
                d.guid,
                aplica_dispositivo_gravacao(&f, &d.guid),
                f.recording_is_initialized()
            );
        }
        f.stop_recording();
        f.release_platform_adm();
    }
}
