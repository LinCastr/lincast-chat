//! Volume do microfone e da saída do SISTEMA (PipeWire, via `wpctl`).
//!
//! O libwebrtc não tem ganho próprio de entrada/saída, então os controles das
//! configurações mexem no volume do dispositivo padrão do sistema — o mesmo
//! que o mixer do desktop mostra.

use std::process::Command;

const SAIDA: &str = "@DEFAULT_AUDIO_SINK@";
const ENTRADA: &str = "@DEFAULT_AUDIO_SOURCE@";

fn alvo(mic: bool) -> &'static str {
    if mic { ENTRADA } else { SAIDA }
}

/// "Volume: 0.45" ou "Volume: 0.45 [MUTED]" → 0.45.
pub fn parse_volume(saida: &str) -> Option<f32> {
    saida
        .split_whitespace()
        .nth(1)
        .and_then(|v| v.parse::<f32>().ok())
}

/// Volume atual (0.0–1.0+) ou `None` se não há `wpctl`/dispositivo.
pub fn le_volume(mic: bool) -> Option<f32> {
    let o = Command::new("wpctl")
        .args(["get-volume", alvo(mic)])
        .output()
        .ok()?;
    if !o.status.success() {
        return None;
    }
    parse_volume(&String::from_utf8_lossy(&o.stdout))
}

/// Define o volume (0.0–1.0); o sistema pode passar disso, o app não.
pub fn define_volume(mic: bool, valor: f32) {
    let v = format!("{:.2}", valor.clamp(0.0, 1.0));
    let _ = Command::new("wpctl")
        .args(["set-volume", alvo(mic), &v])
        .output();
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn le_a_saida_do_wpctl() {
        assert_eq!(parse_volume("Volume: 0.45\n"), Some(0.45));
        assert_eq!(parse_volume("Volume: 1.00 [MUTED]\n"), Some(1.0));
        assert_eq!(parse_volume("lixo"), None);
    }
}
