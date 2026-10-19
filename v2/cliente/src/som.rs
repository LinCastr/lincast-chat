//! Módulo de síntese de áudio e reprodução para o Soundboard (A2).
//!
//! Os sons são sintetizados diretamente em memória em formato WAV PCM (16-bit, 44.1kHz)
//! sem depender de arquivos externos pesados ou de bibliotecas C conflitantes com o libwebrtc.

use std::f32::consts::PI;
use std::io::Write;
use std::process::{Command, Stdio};

/// Lista dos sons disponíveis no soundboard da sala.
pub const SONS: &[(&str, &str)] = &[
    ("buzina", "Buzina"),
    ("tambor", "Rufar"),
    ("rimshot", "Ba dum tss"),
    ("moeda", "Moeda"),
    ("erro", "Errou"),
    ("fanfarra", "Fanfarra"),
    ("laser", "Laser"),
    ("pop", "Pop"),
    ("apito", "Apito"),
    ("batida", "Batida"),
];

/// Volume padrão de quem solta o som (0–100).
pub const VOLUME_PADRAO: u8 = 40;
/// Ganho máximo (volume 100): os sons sintetizados chegavam a ~0,6 de pico,
/// altos demais perto de uma voz. Com isto o teto fica bem abaixo disso.
const GANHO_MAXIMO: f32 = 0.5;

/// Escala as amostras (i16 little-endian, depois do cabeçalho de 44 bytes) pelo
/// ganho que corresponde ao volume (0–100).
pub fn aplica_volume(wav: &mut [u8], volume: u8) {
    let ganho = f32::from(volume.min(100)) / 100.0 * GANHO_MAXIMO;
    let (pares, _) = wav.get_mut(44..).unwrap_or(&mut []).as_chunks_mut::<2>();
    for par in pares {
        let v = i16::from_le_bytes(*par);
        let novo = (f32::from(v) * ganho).round() as i16;
        *par = novo.to_le_bytes();
    }
}

const TAXA_AMOSTRAGEM: u32 = 44100;

/// Gera um container WAV mono PCM 16-bit a partir de amostras f32 em [-1.0, 1.0].
pub fn gera_wav(amostras: &[f32], taxa: u32) -> Vec<u8> {
    let num_amostras = amostras.len() as u32;
    let tamanho_dados = num_amostras * 2;
    let tamanho_total = 36 + tamanho_dados;

    let mut wav = Vec::with_capacity((44 + tamanho_dados) as usize);

    // RIFF chunk descriptor
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&tamanho_total.to_le_bytes());
    wav.extend_from_slice(b"WAVE");

    // fmt sub-chunk
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // tamanho do sub-chunk
    wav.extend_from_slice(&1u16.to_le_bytes()); // áudio PCM = 1
    wav.extend_from_slice(&1u16.to_le_bytes()); // canais = 1 (mono)
    wav.extend_from_slice(&taxa.to_le_bytes()); // taxa de amostragem
    wav.extend_from_slice(&(taxa * 2).to_le_bytes()); // byte rate (taxa * canais * 2)
    wav.extend_from_slice(&2u16.to_le_bytes()); // block align
    wav.extend_from_slice(&16u16.to_le_bytes()); // bits por amostra

    // data sub-chunk
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&tamanho_dados.to_le_bytes());

    for &s in amostras {
        let limitado = s.clamp(-1.0, 1.0);
        let valor_i16 = (limitado * 32767.0) as i16;
        wav.extend_from_slice(&valor_i16.to_le_bytes());
    }

    wav
}

/// Sintetiza um som específico pelo identificador, retornando os bytes WAV completos.
pub fn sintetiza_som(id: &str) -> Option<Vec<u8>> {
    let taxa = TAXA_AMOSTRAGEM as f32;

    match id {
        "buzina" => {
            // Dente de serra duplo 233Hz e 349.5Hz com decaimento suave (0.7s)
            let dur = 0.7;
            let total = (dur * taxa) as usize;
            let mut amostras = Vec::with_capacity(total);
            for i in 0..total {
                let t = i as f32 / taxa;
                let env = (1.0 - t / dur).max(0.0).powf(0.8);
                let d1 = 2.0 * ((t * 233.0).fract()) - 1.0;
                let d2 = 2.0 * ((t * 349.5).fract()) - 1.0;
                amostras.push((d1 * 0.5 + d2 * 0.35) * env * 0.6);
            }
            Some(gera_wav(&amostras, TAXA_AMOSTRAGEM))
        }
        "tambor" => {
            // 14 batidas curtas de ruído filtrado seguidas de boom grave (90Hz) aos 0.8s
            let dur = 1.15;
            let total = (dur * taxa) as usize;
            let mut amostras = vec![0.0f32; total];
            let mut seed = 12345u32;
            for batida in 0..14 {
                let inicio = (batida as f32 * 0.055 * taxa) as usize;
                let tam_batida = (0.045 * taxa) as usize;
                for i in 0..tam_batida {
                    if inicio + i < total {
                        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                        let ruido = (seed as f32 / u32::MAX as f32) * 2.0 - 1.0;
                        let env = 1.0 - (i as f32 / tam_batida as f32);
                        amostras[inicio + i] += ruido * env * 0.45;
                    }
                }
            }
            // boom final
            let inicio_boom = (0.8 * taxa) as usize;
            let tam_boom = (0.35 * taxa) as usize;
            for i in 0..tam_boom {
                if inicio_boom + i < total {
                    let t = i as f32 / taxa;
                    let env = (1.0 - t / 0.35).max(0.0).powf(1.5);
                    let tom = (2.0 * PI * 90.0 * t).sin();
                    amostras[inicio_boom + i] += tom * env * 0.7;
                }
            }
            Some(gera_wav(&amostras, TAXA_AMOSTRAGEM))
        }
        "rimshot" => {
            // Toque duplo e prato: 180Hz triângulo (0.12s), 140Hz triângulo (0.14s) aos 0.16s, prato aos 0.34s
            let dur = 0.75;
            let total = (dur * taxa) as usize;
            let mut amostras = vec![0.0f32; total];

            // toque 1
            let limite1 = (0.12 * taxa) as usize;
            for (i, val) in amostras.iter_mut().enumerate().take(limite1) {
                let t = i as f32 / taxa;
                let env = 1.0 - t / 0.12;
                let tri = 2.0 * (2.0 * (t * 180.0).fract() - 1.0).abs() - 1.0;
                *val += tri * env * 0.5;
            }
            // toque 2
            let offset2 = (0.16 * taxa) as usize;
            for i in 0..((0.14 * taxa) as usize) {
                if offset2 + i < total {
                    let t = i as f32 / taxa;
                    let env = 1.0 - t / 0.14;
                    let tri = 2.0 * (2.0 * (t * 140.0).fract() - 1.0).abs() - 1.0;
                    amostras[offset2 + i] += tri * env * 0.5;
                }
            }
            // prato de ruído
            let offset3 = (0.34 * taxa) as usize;
            let mut seed = 54321u32;
            for i in 0..((0.38 * taxa) as usize) {
                if offset3 + i < total {
                    let t = i as f32 / taxa;
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    let ruido = (seed as f32 / u32::MAX as f32) * 2.0 - 1.0;
                    let env = (1.0 - t / 0.38).max(0.0).powf(2.0);
                    amostras[offset3 + i] += ruido * env * 0.45;
                }
            }
            Some(gera_wav(&amostras, TAXA_AMOSTRAGEM))
        }
        "moeda" => {
            // 988Hz quadrada por 0.08s, depois 1319Hz quadrada por 0.35s
            let dur = 0.45;
            let total = (dur * taxa) as usize;
            let mut amostras = Vec::with_capacity(total);
            for i in 0..total {
                let t = i as f32 / taxa;
                if t < 0.08 {
                    let sq = if (t * 988.0).fract() < 0.5 { 0.5 } else { -0.5 };
                    amostras.push(sq * 0.5);
                } else {
                    let t2 = t - 0.08;
                    let env = (1.0 - t2 / 0.37).max(0.0).powf(1.8);
                    let sq = if (t * 1319.0).fract() < 0.5 {
                        0.5
                    } else {
                        -0.5
                    };
                    amostras.push(sq * env * 0.5);
                }
            }
            Some(gera_wav(&amostras, TAXA_AMOSTRAGEM))
        }
        "erro" => {
            // Duas notas graves quadradas: 160Hz por 0.22s, pausa curta, 120Hz por 0.3s
            let dur = 0.56;
            let total = (dur * taxa) as usize;
            let mut amostras = Vec::with_capacity(total);
            for i in 0..total {
                let t = i as f32 / taxa;
                if t < 0.22 {
                    let env = (1.0 - t / 0.22).max(0.0);
                    let sq = if (t * 160.0).fract() < 0.5 { 0.6 } else { -0.6 };
                    amostras.push(sq * env * 0.6);
                } else if (0.24..0.54).contains(&t) {
                    let t2 = t - 0.24;
                    let env = (1.0 - t2 / 0.30).max(0.0);
                    let sq = if (t * 120.0).fract() < 0.5 { 0.6 } else { -0.6 };
                    amostras.push(sq * env * 0.6);
                } else {
                    amostras.push(0.0);
                }
            }
            Some(gera_wav(&amostras, TAXA_AMOSTRAGEM))
        }
        "fanfarra" => {
            // Arpeggio brilhante: 523Hz, 659Hz, 784Hz, 1047Hz em triângulos espaçados por 0.11s
            let dur = 0.75;
            let total = (dur * taxa) as usize;
            let mut amostras = vec![0.0f32; total];
            let notas = [523.0f32, 659.0, 784.0, 1047.0];
            for (idx, &freq) in notas.iter().enumerate() {
                let offset = (idx as f32 * 0.11 * taxa) as usize;
                let tam_nota = (0.35 * taxa) as usize;
                for i in 0..tam_nota {
                    if offset + i < total {
                        let t = i as f32 / taxa;
                        let env = (1.0 - t / 0.35).max(0.0).powf(1.2);
                        let tri = 2.0 * (2.0 * (t * freq).fract() - 1.0).abs() - 1.0;
                        amostras[offset + i] += tri * env * 0.35;
                    }
                }
            }
            Some(gera_wav(&amostras, TAXA_AMOSTRAGEM))
        }
        "laser" => {
            // Queda exponencial rápida de 1800Hz para 120Hz em dente-de-serra (0.34s)
            let dur = 0.34;
            let total = (dur * taxa) as usize;
            let mut amostras = Vec::with_capacity(total);
            let mut fase = 0.0f32;
            for i in 0..total {
                let t = i as f32 / taxa;
                let progresso = t / dur;
                let freq = 1800.0 * (120.0f32 / 1800.0).powf(progresso);
                fase += freq / taxa;
                let env = (1.0 - progresso).max(0.0);
                let saw = 2.0 * fase.fract() - 1.0;
                amostras.push(saw * env * 0.5);
            }
            Some(gera_wav(&amostras, TAXA_AMOSTRAGEM))
        }
        "pop" => {
            // Senoide subindo rápido de 420Hz para 900Hz em 0.1s
            let dur = 0.12;
            let total = (dur * taxa) as usize;
            let mut amostras = Vec::with_capacity(total);
            let mut fase = 0.0f32;
            for i in 0..total {
                let t = i as f32 / taxa;
                let prog = (t / dur).min(1.0);
                let freq = 420.0 + (900.0 - 420.0) * prog;
                fase += freq / taxa;
                let env = (1.0 - prog).max(0.0).powf(2.0);
                let s = (2.0 * PI * fase).sin();
                amostras.push(s * env * 0.6);
            }
            Some(gera_wav(&amostras, TAXA_AMOSTRAGEM))
        }
        "apito" => {
            // Apito agudo com leve trinado (1900Hz seguido de 2300Hz)
            let dur = 0.42;
            let total = (dur * taxa) as usize;
            let mut amostras = Vec::with_capacity(total);
            for i in 0..total {
                let t = i as f32 / taxa;
                let freq = if t < 0.18 { 1900.0 } else { 2300.0 };
                let env = (1.0 - t / dur).max(0.0).powf(0.8);
                let s = (2.0 * PI * freq * t).sin();
                amostras.push(s * env * 0.45);
            }
            Some(gera_wav(&amostras, TAXA_AMOSTRAGEM))
        }
        "batida" => {
            // Batimento cardíaco: duas batidas graves de 60Hz e 55Hz
            let dur = 0.52;
            let total = (dur * taxa) as usize;
            let mut amostras = vec![0.0f32; total];
            // batida 1
            let limite1 = (0.18 * taxa) as usize;
            for (i, val) in amostras.iter_mut().enumerate().take(limite1) {
                let t = i as f32 / taxa;
                let env = (1.0 - t / 0.18).max(0.0).powf(1.5);
                let s = (2.0 * PI * 60.0 * t).sin();
                *val += s * env * 0.7;
            }
            // batida 2
            let offset2 = (0.28 * taxa) as usize;
            for i in 0..((0.22 * taxa) as usize) {
                if offset2 + i < total {
                    let t = i as f32 / taxa;
                    let env = (1.0 - t / 0.22).max(0.0).powf(1.5);
                    let s = (2.0 * PI * 55.0 * t).sin();
                    amostras[offset2 + i] += s * env * 0.7;
                }
            }
            Some(gera_wav(&amostras, TAXA_AMOSTRAGEM))
        }
        _ => None,
    }
}

/// Envia o buffer WAV para o tocador de áudio do sistema (`pw-play`, `paplay` ou `aplay`).
pub fn reproduz_wav(wav: &[u8]) {
    let players = [
        ("pw-play", &["-"][..]),
        ("paplay", &[][..]),
        ("aplay", &["-q", "-"][..]),
    ];

    for (cmd, args) in players {
        if let Ok(mut child) = Command::new(cmd)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(wav);
            }
            let _ = child.wait();
            return;
        }
    }
}

/// O toque de chamada: arpejo curto repetido, como no protótipo (triângulo
/// com decaimento), seguido de um respiro. Sem arquivo de terceiros.
pub fn sintetiza_toque() -> Vec<u8> {
    let taxa = TAXA_AMOSTRAGEM as f32;
    let total = (2.2 * taxa) as usize;
    let mut amostras = vec![0.0_f32; total];
    // (frequência, início em segundos)
    let notas = [
        (659.25_f32, 0.0_f32),
        (783.99, 0.16),
        (987.77, 0.32),
        (783.99, 0.62),
        (987.77, 0.78),
    ];
    for (freq, inicio) in notas {
        let dur = 0.35_f32;
        let i0 = (inicio * taxa) as usize;
        let n = (dur * taxa) as usize;
        for k in 0..n {
            let t = k as f32 / taxa;
            let fase = (t * freq).fract();
            // Onda triangular.
            let tri = 4.0 * (fase - 0.5).abs() - 1.0;
            let ataque = (t / 0.01).min(1.0);
            let queda = (-t * 9.0).exp();
            if let Some(a) = amostras.get_mut(i0 + k) {
                *a += tri * 0.18 * ataque * queda;
            }
        }
    }
    gera_wav(&amostras, TAXA_AMOSTRAGEM)
}

/// Toca o toque em laço até ser largado (o `Drop` para).
pub struct ToqueSonoro {
    parar: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl ToqueSonoro {
    pub fn inicia(volume: u8) -> Self {
        use std::sync::atomic::{AtomicBool, Ordering};
        let parar = std::sync::Arc::new(AtomicBool::new(false));
        let flag = parar.clone();
        std::thread::spawn(move || {
            let mut wav = sintetiza_toque();
            aplica_volume(&mut wav, volume);
            while !flag.load(Ordering::Relaxed) {
                reproduz_wav(&wav);
            }
        });
        Self { parar }
    }
}

impl Drop for ToqueSonoro {
    fn drop(&mut self) {
        self.parar.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Sons de retorno da interface: só quem fez a ação (ou recebeu o aviso)
/// ouve, nunca vão para a chamada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retorno {
    MicMudo,
    MicAberto,
    FoneDesligado,
    FoneLigado,
    EntrouNaChamada,
    SaiuDaChamada,
    TelaIniciada,
    TelaParada,
    OutroTransmite,
    OutroFechouTransmissao,
    AlguemAssiste,
    AlguemParouDeAssistir,
    OutroEntrou,
    OutroSaiu,
}

/// Notas de cada retorno: (frequência em Hz, início em s, duração em s, onda).
/// Onda: 0 seno, 1 triângulo, 2 quadrada suave.
fn notas_do_retorno(r: Retorno) -> &'static [(f32, f32, f32, u8)] {
    match r {
        // Mic: dois tons curtos, descendo ao mutar e subindo ao abrir.
        Retorno::MicMudo => &[(520.0, 0.0, 0.07, 0), (330.0, 0.07, 0.10, 0)],
        Retorno::MicAberto => &[(330.0, 0.0, 0.07, 0), (520.0, 0.07, 0.10, 0)],
        // Fone: três tons mais graves, para não confundir com o mic.
        Retorno::FoneDesligado => &[
            (440.0, 0.0, 0.06, 1),
            (330.0, 0.06, 0.06, 1),
            (220.0, 0.12, 0.12, 1),
        ],
        Retorno::FoneLigado => &[
            (220.0, 0.0, 0.06, 1),
            (330.0, 0.06, 0.06, 1),
            (440.0, 0.12, 0.12, 1),
        ],
        Retorno::EntrouNaChamada => &[
            (523.25, 0.0, 0.09, 0),
            (659.25, 0.09, 0.09, 0),
            (783.99, 0.18, 0.16, 0),
        ],
        Retorno::SaiuDaChamada => &[
            (783.99, 0.0, 0.09, 0),
            (659.25, 0.09, 0.09, 0),
            (523.25, 0.18, 0.16, 0),
        ],
        // Tela: varredura brilhante (quadrada suave) para começar e parar.
        Retorno::TelaIniciada => &[
            (392.0, 0.0, 0.08, 2),
            (587.33, 0.08, 0.08, 2),
            (880.0, 0.16, 0.14, 2),
        ],
        Retorno::TelaParada => &[
            (880.0, 0.0, 0.08, 2),
            (587.33, 0.08, 0.08, 2),
            (392.0, 0.16, 0.14, 2),
        ],
        // Outros: sinos (seno agudo), bem diferentes dos de cima.
        Retorno::OutroTransmite => &[(1046.5, 0.0, 0.12, 0), (1318.5, 0.12, 0.20, 0)],
        Retorno::OutroFechouTransmissao => &[(1318.5, 0.0, 0.12, 0), (1046.5, 0.12, 0.20, 0)],
        Retorno::AlguemAssiste => &[(1174.7, 0.0, 0.07, 0), (1568.0, 0.07, 0.07, 0)],
        Retorno::AlguemParouDeAssistir => &[(1568.0, 0.0, 0.07, 0), (1174.7, 0.07, 0.07, 0)],
        // Chegada e saída de outra pessoa da chamada: pulso grave único.
        Retorno::OutroEntrou => &[(587.33, 0.0, 0.08, 1), (880.0, 0.08, 0.12, 1)],
        Retorno::OutroSaiu => &[(880.0, 0.0, 0.08, 1), (587.33, 0.08, 0.12, 1)],
    }
}

pub fn sintetiza_retorno(r: Retorno) -> Vec<u8> {
    let taxa = TAXA_AMOSTRAGEM as f32;
    let notas = notas_do_retorno(r);
    let fim = notas.iter().map(|n| n.1 + n.2).fold(0.0_f32, f32::max) + 0.05;
    let mut amostras = vec![0.0_f32; (fim * taxa) as usize];
    for &(freq, inicio, dur, onda) in notas {
        let i0 = (inicio * taxa) as usize;
        let n = (dur * taxa) as usize;
        for k in 0..n {
            let t = k as f32 / taxa;
            let fase = (t * freq).fract();
            let v = match onda {
                0 => (2.0 * PI * fase).sin(),
                1 => 4.0 * (fase - 0.5).abs() - 1.0,
                _ => (2.0 * PI * fase).sin().signum() * 0.5 + (2.0 * PI * fase).sin() * 0.5,
            };
            let ataque = (t / 0.005).min(1.0);
            let queda = (1.0 - t / dur).clamp(0.0, 1.0);
            if let Some(a) = amostras.get_mut(i0 + k) {
                *a += v * 0.5 * ataque * queda;
            }
        }
    }
    gera_wav(&amostras, TAXA_AMOSTRAGEM)
}

/// Estado da minha chamada, para decidir qual retorno tocar numa mudança.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EstadoPropio {
    pub na_chamada: bool,
    pub mudo: bool,
    pub surdo: bool,
    pub apresentando: bool,
}

/// Retornos devidos pela mudança `antes` → `depois`. Surdo também muta o
/// microfone, então a mudança de fone vale pelas duas.
pub fn retornos_da_mudanca(antes: EstadoPropio, depois: EstadoPropio) -> Vec<Retorno> {
    let mut v = Vec::new();
    if antes.na_chamada != depois.na_chamada {
        v.push(if depois.na_chamada {
            Retorno::EntrouNaChamada
        } else {
            Retorno::SaiuDaChamada
        });
        return v;
    }
    if antes.surdo != depois.surdo {
        v.push(if depois.surdo {
            Retorno::FoneDesligado
        } else {
            Retorno::FoneLigado
        });
    } else if antes.mudo != depois.mudo {
        v.push(if depois.mudo {
            Retorno::MicMudo
        } else {
            Retorno::MicAberto
        });
    }
    if antes.apresentando != depois.apresentando {
        v.push(if depois.apresentando {
            Retorno::TelaIniciada
        } else {
            Retorno::TelaParada
        });
    }
    v
}

/// Retorno devido à mudança de quem transmite a tela (fora eu) entre duas
/// listas de pessoas: começou, ou parou sem sair da chamada.
pub fn retorno_de_transmissao_alheia(
    antes: &[crate::sala::Pessoa],
    depois: &[crate::sala::Pessoa],
) -> Option<Retorno> {
    let mudou = |quer: bool| {
        depois.iter().any(|n| {
            !n.eu
                && n.apresentando == quer
                && antes.iter().any(|a| a.id == n.id && a.apresentando != quer)
        })
    };
    if mudou(true) {
        Some(Retorno::OutroTransmite)
    } else if mudou(false) {
        Some(Retorno::OutroFechouTransmissao)
    } else {
        None
    }
}

/// Alguém (fora eu) entrou ou saiu da chamada entre duas listas de pessoas.
/// Só conta quem está na chamada, não quem só olha a conversa (espectador).
pub fn retorno_de_chegada(
    antes: &[crate::sala::Pessoa],
    depois: &[crate::sala::Pessoa],
) -> Option<Retorno> {
    let na_chamada =
        |l: &[crate::sala::Pessoa], id: &str| l.iter().any(|p| p.id == id && !p.espectador);
    if depois
        .iter()
        .any(|p| !p.eu && !p.espectador && !na_chamada(antes, &p.id))
    {
        Some(Retorno::OutroEntrou)
    } else if antes
        .iter()
        .any(|p| !p.eu && !p.espectador && !na_chamada(depois, &p.id))
    {
        Some(Retorno::OutroSaiu)
    } else {
        None
    }
}

/// Toca um som de retorno em segundo plano, no volume dos efeitos.
pub fn toca_retorno(r: Retorno, volume: u8) {
    std::thread::spawn(move || {
        let mut wav = sintetiza_retorno(r);
        aplica_volume(&mut wav, volume);
        reproduz_wav(&wav);
    });
}

/// Reproduz um som do soundboard em segundo plano.
pub fn toca_som(id: &str, volume: u8) {
    let id_str = id.to_string();
    std::thread::spawn(move || {
        if let Some(mut wav) = sintetiza_som(&id_str) {
            aplica_volume(&mut wav, volume);
            reproduz_wav(&wav);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toque_sintetizado_e_um_wav_valido_de_2s() {
        let w = sintetiza_toque();
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(&w[8..12], b"WAVE");
        // 44 bytes de cabeçalho + 2,2s * 44100 * 2 bytes
        assert_eq!(w.len(), 44 + (2.2_f32 * 44100.0) as usize * 2);
        // Não é silêncio.
        assert!(w[44..].iter().any(|&b| b != 0));
    }

    #[test]
    fn testa_sintese_de_todos_os_sons_gera_wav_valido() {
        for &(id, _) in SONS {
            let wav_opt = sintetiza_som(id);
            assert!(wav_opt.is_some(), "Som {id} deve sintetizar com sucesso");
            let wav = wav_opt.unwrap();
            assert!(wav.len() > 44, "WAV deve ter mais que o cabeçalho");
            assert_eq!(&wav[0..4], b"RIFF");
            assert_eq!(&wav[8..12], b"WAVE");
            assert_eq!(&wav[12..16], b"fmt ");
        }
    }

    #[test]
    fn volume_escala_as_amostras_e_zero_silencia() {
        let mut w = gera_wav(&[0.5, -0.5, 1.0], 44100);
        let original = w.clone();
        aplica_volume(&mut w, 0);
        assert!(w[44..].iter().all(|&b| b == 0));
        let mut w2 = original.clone();
        aplica_volume(&mut w2, 100);
        let amostra = |b: &[u8], i: usize| i16::from_le_bytes([b[44 + 2 * i], b[45 + 2 * i]]);
        // 100% = GANHO_MAXIMO (0,5) do original.
        let esperado = (f32::from(amostra(&original, 2)) * GANHO_MAXIMO).round() as i16;
        assert_eq!(amostra(&w2, 2), esperado);
        // 50% é metade de 100%.
        let mut w3 = original;
        aplica_volume(&mut w3, 50);
        assert!((i32::from(amostra(&w3, 2)) * 2 - i32::from(esperado)).abs() <= 1);
    }

    #[test]
    fn retornos_sao_wav_validos_e_distintos() {
        use Retorno::*;
        let todos = [
            MicMudo,
            MicAberto,
            FoneDesligado,
            FoneLigado,
            EntrouNaChamada,
            SaiuDaChamada,
            TelaIniciada,
            TelaParada,
            OutroTransmite,
            OutroFechouTransmissao,
            AlguemAssiste,
            AlguemParouDeAssistir,
            OutroEntrou,
            OutroSaiu,
        ];
        let wavs: Vec<_> = todos.iter().map(|r| sintetiza_retorno(*r)).collect();
        for (i, w) in wavs.iter().enumerate() {
            assert_eq!(&w[0..4], b"RIFF");
            assert!(w[44..].iter().any(|&b| b != 0));
            for outro in &wavs[i + 1..] {
                assert_ne!(w, outro);
            }
        }
    }

    #[test]
    fn retornos_da_mudanca_escolhe_o_som_certo() {
        let base = EstadoPropio {
            na_chamada: true,
            mudo: false,
            surdo: false,
            apresentando: false,
        };
        let surdo = EstadoPropio {
            surdo: true,
            mudo: true,
            ..base
        };
        assert_eq!(
            retornos_da_mudanca(base, surdo),
            vec![Retorno::FoneDesligado]
        );
        assert_eq!(retornos_da_mudanca(surdo, base), vec![Retorno::FoneLigado]);
        let mudo = EstadoPropio { mudo: true, ..base };
        assert_eq!(retornos_da_mudanca(base, mudo), vec![Retorno::MicMudo]);
        let tela = EstadoPropio {
            apresentando: true,
            ..base
        };
        assert_eq!(retornos_da_mudanca(base, tela), vec![Retorno::TelaIniciada]);
        let fora = EstadoPropio {
            na_chamada: false,
            ..base
        };
        assert_eq!(
            retornos_da_mudanca(base, fora),
            vec![Retorno::SaiuDaChamada]
        );
        assert!(retornos_da_mudanca(base, base).is_empty());
    }

    fn pessoa(id: &str, eu: bool, espectador: bool, apresentando: bool) -> crate::sala::Pessoa {
        crate::sala::Pessoa {
            id: id.into(),
            uid: None,
            nome: id.into(),
            eu,
            espectador,
            mudo: false,
            surdo: false,
            apresentando,
            com_camera: false,
            conexao: None,
            volume: 1.0,
            volume_tela: 1.0,
            volume_anterior: None,
            volume_tela_anterior: None,
            falando: false,
            mudo_forcado: false,
            dono: false,
            qualidade: None,
            avatar: None,
            cor: None,
            cor_nome: None,
            efeito_nome: None,
            bio: None,
            status: None,
            arroba: None,
            atividade: None,
        }
    }

    #[test]
    fn retornos_de_pessoas_alheias() {
        let eu = pessoa("eu", true, false, false);
        let so_eu = vec![eu.clone()];
        let com_a = vec![eu.clone(), pessoa("a", false, false, false)];
        assert_eq!(
            retorno_de_chegada(&so_eu, &com_a),
            Some(Retorno::OutroEntrou)
        );
        assert_eq!(retorno_de_chegada(&com_a, &so_eu), Some(Retorno::OutroSaiu));
        // Espectador (só olhando a conversa) não conta como chegada.
        let espec = vec![eu.clone(), pessoa("a", false, true, false)];
        assert_eq!(retorno_de_chegada(&so_eu, &espec), None);
        // Passar de espectador para a chamada conta.
        assert_eq!(
            retorno_de_chegada(&espec, &com_a),
            Some(Retorno::OutroEntrou)
        );
        let transmite = vec![eu.clone(), pessoa("a", false, false, true)];
        assert_eq!(
            retorno_de_transmissao_alheia(&com_a, &transmite),
            Some(Retorno::OutroTransmite)
        );
        assert_eq!(
            retorno_de_transmissao_alheia(&transmite, &com_a),
            Some(Retorno::OutroFechouTransmissao)
        );
        // Minha própria transmissão não toca o som de "outro".
        let eu_tela = vec![pessoa("eu", true, false, true)];
        assert_eq!(retorno_de_transmissao_alheia(&so_eu, &eu_tela), None);
    }

    #[test]
    fn testa_som_desconhecido_retorna_none() {
        assert!(sintetiza_som("som_que_nao_existe").is_none());
    }
}
