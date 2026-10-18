//! Áudio de aplicativos escolhidos, direto do PipeWire.
//!
//! É a peça que a v1 não tem como ter: o portal do Wayland entrega só o vídeo
//! da tela, e o Chromium não captura áudio do sistema no Linux. Aqui cada
//! aplicativo que toca som aparece como um nó `Stream/Output/Audio`, e a gente
//! liga uma captura direto nele (`target.object` = serial do nó). Nada de sink
//! virtual, nada de trocar a saída padrão do sistema — e o som do próprio app
//! nunca entra, porque ele simplesmente não é escolhido.
//!
//! Cada app escolhido ganha uma captura própria, e um mixer soma tudo em
//! blocos de 10ms, que é o tamanho que o libwebrtc quer receber.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pipewire as pw;
use pw::node::{Node, NodeListener};
use pw::properties::properties;
use pw::spa;
use pw::spa::utils::dict::DictRef;
use pw::stream::{StreamListener, StreamRc};
use pw::types::ObjectType;

pub const TAXA: u32 = 48_000;
pub const CANAIS: usize = 2;
/// Amostras (já intercaladas) que cabem em 10ms.
pub const AMOSTRAS_10MS: usize = TAXA as usize / 100 * CANAIS;

/// Quanto cada fila acumula antes de começar a ser tocada. O PipeWire entrega
/// em blocos do tamanho do quantum (tipicamente ~21ms), e o mixer puxa a cada
/// 10ms: sem essa folga a fila esvazia entre uma entrega e outra e o som sai
/// picotado.
const PRE_BUFFER: usize = AMOSTRAS_10MS * 4;
/// Teto de atraso. Os relógios do PipeWire e do mixer nunca batem exatamente;
/// sem teto, a diferença vira atraso crescente.
const MAX_FILA: usize = AMOSTRAS_10MS * 20;

#[derive(Clone, Debug)]
pub struct AppDeAudio {
    pub id: u32,
    pub serial: String,
    pub nome: String,
    pub binario: String,
    pub pid: Option<u32>,
    pub midia: String,
}

impl AppDeAudio {
    /// `{"nome":..,"binario":..,"midia":..}` sem depender de serde: são três
    /// textos, e o escape abaixo cobre o que um nome de app pode trazer.
    pub fn json(&self) -> String {
        fn esc(s: &str) -> String {
            let mut o = String::with_capacity(s.len() + 2);
            for c in s.chars() {
                match c {
                    '"' => o.push_str("\\\""),
                    '\\' => o.push_str("\\\\"),
                    c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
                    c => o.push(c),
                }
            }
            o
        }
        format!(
            "{{\"nome\":\"{}\",\"binario\":\"{}\",\"midia\":\"{}\"}}",
            esc(&self.nome),
            esc(&self.binario),
            esc(&self.midia)
        )
    }
}

impl std::fmt::Display for AppDeAudio {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (binário: {}, pid: ", self.nome, self.binario)?;
        match self.pid {
            Some(p) => write!(f, "{p}")?,
            None => write!(f, "?")?,
        }
        write!(f, ", nó {}, serial {})", self.id, self.serial)?;
        if !self.midia.is_empty() {
            write!(f, " — \"{}\"", self.midia)?;
        }
        Ok(())
    }
}

fn le_app(id: u32, props: &DictRef) -> Option<AppDeAudio> {
    if props.get(*pw::keys::MEDIA_CLASS) != Some("Stream/Output/Audio") {
        return None;
    }
    let texto = |k: &str| props.get(k).unwrap_or("").to_string();
    let nome = [*pw::keys::APP_NAME, *pw::keys::NODE_NAME]
        .iter()
        .map(|k| texto(k))
        .find(|v| !v.is_empty())
        .unwrap_or_else(|| "sem nome".into());
    Some(AppDeAudio {
        id,
        serial: texto(*pw::keys::OBJECT_SERIAL),
        nome,
        binario: texto(*pw::keys::APP_PROCESS_BINARY),
        pid: props
            .get(*pw::keys::APP_PROCESS_ID)
            .and_then(|p| p.parse().ok()),
        midia: texto("media.name"),
    })
}

/// Quais aplicativos entram na transmissão. O próprio processo e o do app
/// principal (`excluir_pid`) nunca entram — é isso que impede a voz da sala de
/// voltar na transmissão, o defeito que a v1 só contornava com sink virtual.
#[derive(Clone, Debug)]
pub enum Filtro {
    /// Só os que casarem (sem diferenciar maiúscula) com o nome ou o binário.
    Apps(Vec<String>),
    /// Todos que tocam som, menos este processo. Cuidado: se a chamada estiver
    /// tocando em outro programa na mesma máquina (um navegador, por exemplo),
    /// ela entra junto.
    TodosMenosEu,
}

impl Filtro {
    fn aceita(&self, app: &AppDeAudio, excluir_pid: Option<u32>) -> bool {
        if app.pid == Some(std::process::id()) || (app.pid.is_some() && app.pid == excluir_pid) {
            return false;
        }
        match self {
            Filtro::TodosMenosEu => true,
            Filtro::Apps(nomes) => {
                let nome = app.nome.to_lowercase();
                let bin = app.binario.to_lowercase();
                nomes.iter().any(|n| {
                    let n = n.to_lowercase();
                    nome.contains(&n) || bin.contains(&n)
                })
            }
        }
    }
}

#[derive(Default)]
struct Fila {
    amostras: VecDeque<f32>,
    tocando: bool,
}

/// Soma as capturas de todos os apps escolhidos.
#[derive(Default)]
pub struct Mixer {
    filas: Mutex<HashMap<u32, Fila>>,
}

impl Mixer {
    fn empurra(&self, id: u32, amostras: impl Iterator<Item = f32>) {
        let mut filas = self.filas.lock().unwrap();
        let fila = filas.entry(id).or_default();
        fila.amostras.extend(amostras);
        let sobra = fila.amostras.len().saturating_sub(MAX_FILA);
        fila.amostras.drain(..sobra);
    }

    fn esquece(&self, id: u32) {
        self.filas.lock().unwrap().remove(&id);
    }

    /// Preenche `saida` (10ms intercalados) e devolve o pico, de 0 a 1.
    pub fn puxa_10ms(&self, saida: &mut [i16]) -> f32 {
        let mut soma = [0f32; AMOSTRAS_10MS];
        let mut filas = self.filas.lock().unwrap();
        for fila in filas.values_mut() {
            if !fila.tocando && fila.amostras.len() >= PRE_BUFFER {
                fila.tocando = true;
            }
            if !fila.tocando {
                continue;
            }
            if fila.amostras.len() < AMOSTRAS_10MS {
                // Esvaziou: espera encher de novo em vez de tocar pedaço.
                fila.tocando = false;
                continue;
            }
            for (s, a) in soma.iter_mut().zip(fila.amostras.drain(..AMOSTRAS_10MS)) {
                *s += a;
            }
        }
        drop(filas);
        let mut pico = 0f32;
        for (o, s) in saida.iter_mut().zip(soma) {
            let s = s.clamp(-1.0, 1.0);
            pico = pico.max(s.abs());
            *o = (s * i16::MAX as f32) as i16;
        }
        pico
    }
}

/// Lista o que está tocando som agora (uma olhada de ~1s no registro).
pub fn lista() -> Result<Vec<AppDeAudio>, pw::Error> {
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;
    let registry = core.get_registry_rc()?;

    let achados: Rc<RefCell<Vec<AppDeAudio>>> = Rc::default();
    let nos: Rc<RefCell<Vec<(Node, NodeListener)>>> = Rc::default();
    let registry_fraco = registry.downgrade();
    let _ouvinte = registry
        .add_listener_local()
        .global({
            let achados = achados.clone();
            let nos = nos.clone();
            move |obj| {
                if obj.type_ != ObjectType::Node {
                    return;
                }
                let Some(registry) = registry_fraco.upgrade() else {
                    return;
                };
                let Ok(no) = registry.bind::<Node, _>(obj) else {
                    return;
                };
                let id = obj.id;
                let achados = achados.clone();
                let ouvinte = no
                    .add_listener_local()
                    .info(move |info| {
                        if let Some(app) = info.props().and_then(|p| le_app(id, p)) {
                            let mut a = achados.borrow_mut();
                            if !a.iter().any(|x| x.id == id) {
                                a.push(app);
                            }
                        }
                    })
                    .register();
                nos.borrow_mut().push((no, ouvinte));
            }
        })
        .register();

    let ml = mainloop.clone();
    let _timer = mainloop.loop_().add_timer(move |_| ml.quit());
    _timer
        .update_timer(Some(Duration::from_millis(1200)), None)
        .into_result()?;
    mainloop.run();

    let resultado = achados.borrow().clone();
    Ok(resultado)
}

/// Sobe a thread do PipeWire, que fica capturando enquanto o programa vive.
/// Apps que começarem a tocar depois também entram, se passarem no filtro.
pub fn inicia(
    filtro: Filtro,
    excluir_pid: Option<u32>,
    mixer: Arc<Mixer>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("pipewire-audio".into())
        .spawn(move || {
            if let Err(e) = roda(filtro, excluir_pid, mixer) {
                log::error!("[audio] PipeWire parou: {e}");
            }
        })
        .expect("não consegui criar a thread do PipeWire")
}

type Captura = (StreamRc, StreamListener<()>);

fn roda(filtro: Filtro, excluir_pid: Option<u32>, mixer: Arc<Mixer>) -> Result<(), pw::Error> {
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;
    let registry = core.get_registry_rc()?;

    // Tudo que precisa continuar vivo: o proxy de cada nó (pra receber o
    // `info` com as propriedades completas) e cada captura ativa.
    let nos: Rc<RefCell<HashMap<u32, (Node, NodeListener)>>> = Rc::default();
    let capturas: Rc<RefCell<HashMap<u32, Captura>>> = Rc::default();

    let registry_fraco = registry.downgrade();
    let _ouvinte = registry
        .add_listener_local()
        .global({
            let nos = nos.clone();
            let capturas = capturas.clone();
            let core = core.clone();
            let mixer = mixer.clone();
            move |obj| {
                if obj.type_ != ObjectType::Node {
                    return;
                }
                let Some(registry) = registry_fraco.upgrade() else {
                    return;
                };
                let Ok(no) = registry.bind::<Node, _>(obj) else {
                    return;
                };
                let id = obj.id;
                let capturas = capturas.clone();
                let core = core.clone();
                let mixer = mixer.clone();
                let filtro = filtro.clone();
                // As propriedades do anúncio global são um resumo; as que
                // dizem QUAL programa é (nome, binário, pid) vêm no `info`.
                let ouvinte = no
                    .add_listener_local()
                    .info(move |info| {
                        if capturas.borrow().contains_key(&id) {
                            return;
                        }
                        let Some(app) = info.props().and_then(|p| le_app(id, p)) else {
                            return;
                        };
                        if !filtro.aceita(&app, excluir_pid) {
                            log::info!("[audio] ignorando: {app}");
                            return;
                        }
                        match cria_captura(&core, &app, mixer.clone()) {
                            Ok(c) => {
                                log::info!("[audio] capturando: {app}");
                                capturas.borrow_mut().insert(id, c);
                            }
                            Err(e) => log::warn!("[audio] não consegui capturar {app}: {e}"),
                        }
                    })
                    .register();
                nos.borrow_mut().insert(id, (no, ouvinte));
            }
        })
        .global_remove({
            let nos = nos.clone();
            let capturas = capturas.clone();
            move |id| {
                nos.borrow_mut().remove(&id);
                if capturas.borrow_mut().remove(&id).is_some() {
                    mixer.esquece(id);
                    log::info!("[audio] app parou de tocar (nó {id})");
                }
            }
        })
        .register();

    mainloop.run();
    Ok(())
}

fn cria_captura(
    core: &pw::core::CoreRc,
    app: &AppDeAudio,
    mixer: Arc<Mixer>,
) -> Result<Captura, pw::Error> {
    let props = properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_ROLE => "Screen",
        *pw::keys::NODE_NAME => "concord-tela-audio",
        *pw::keys::APP_NAME => "Concord",
        *pw::keys::TARGET_OBJECT => app.serial.clone(),
        // Sem isto, quando o app fecha o PipeWire religa a captura no padrão
        // do sistema — ou seja, no MICROFONE, e a voz vazaria na transmissão.
        *pw::keys::NODE_DONT_RECONNECT => "true",
    };
    let stream = StreamRc::new(core.clone(), "concord-tela-audio", props)?;
    let id = app.id;

    let ouvinte = stream
        .add_local_listener_with_user_data(())
        .process(move |stream, _| {
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };
            let datas = buffer.datas_mut();
            let Some(data) = datas.first_mut() else {
                return;
            };
            let offset = data.chunk().offset() as usize;
            let size = data.chunk().size() as usize;
            let Some(bytes) = data.data() else { return };
            let Some(bytes) = bytes.get(offset..offset + size) else {
                return;
            };
            mixer.empurra(
                id,
                bytes
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|b| f32::from_le_bytes(*b)),
            );
        })
        .register()?;

    // Pede o formato exato que o mixer usa; o PipeWire converte o que for
    // preciso (taxa, número de canais) antes de entregar.
    let mut info = spa::param::audio::AudioInfoRaw::new();
    info.set_format(spa::param::audio::AudioFormat::F32LE);
    info.set_rate(TAXA);
    info.set_channels(CANAIS as u32);
    let mut posicao = [0u32; spa::sys::SPA_AUDIO_MAX_CHANNELS as usize];
    posicao[0] = spa::sys::SPA_AUDIO_CHANNEL_FL;
    posicao[1] = spa::sys::SPA_AUDIO_CHANNEL_FR;
    info.set_position(posicao);
    let objeto = spa::pod::Object {
        type_: spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
        id: spa::param::ParamType::EnumFormat.as_raw(),
        properties: info.into(),
    };
    let bytes: Vec<u8> = spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &spa::pod::Value::Object(objeto),
    )
    .expect("formato de áudio inválido")
    .0
    .into_inner();
    let mut params = [spa::pod::Pod::from_bytes(&bytes).unwrap()];

    stream.connect(
        spa::utils::Direction::Input,
        None,
        pw::stream::StreamFlags::AUTOCONNECT
            | pw::stream::StreamFlags::MAP_BUFFERS
            | pw::stream::StreamFlags::RT_PROCESS,
        &mut params,
    )?;
    Ok((stream, ouvinte))
}

#[cfg(test)]
mod testes {
    use super::*;

    fn app(nome: &str, binario: &str, pid: Option<u32>) -> AppDeAudio {
        AppDeAudio {
            id: 1,
            serial: "1".into(),
            nome: nome.into(),
            binario: binario.into(),
            pid,
            midia: String::new(),
        }
    }

    #[test]
    fn filtro_casa_nome_ou_binario_sem_diferenciar_maiuscula() {
        let f = Filtro::Apps(vec!["spotify".into()]);
        assert!(f.aceita(&app("Spotify", "", Some(10)), None));
        assert!(f.aceita(&app("sem nome", "SPOTIFY-launcher", Some(10)), None));
        assert!(!f.aceita(&app("Firefox", "firefox", Some(10)), None));
    }

    #[test]
    fn nunca_captura_a_si_mesmo_nem_o_app_principal() {
        let todos = Filtro::TodosMenosEu;
        assert!(!todos.aceita(&app("x", "x", Some(std::process::id())), None));
        assert!(!todos.aceita(&app("Concord", "concord-proto", Some(4242)), Some(4242)));
        assert!(todos.aceita(&app("Jogo", "jogo", Some(7)), Some(4242)));
        // Sem pid conhecido, o excluir_pid não pode casar por acidente.
        assert!(todos.aceita(&app("Jogo", "jogo", None), None));
    }

    #[test]
    fn mixer_espera_o_pre_buffer_antes_de_tocar() {
        let m = Mixer::default();
        let mut saida = [0i16; AMOSTRAS_10MS];
        m.empurra(1, std::iter::repeat_n(0.5, PRE_BUFFER - 1));
        assert_eq!(m.puxa_10ms(&mut saida), 0.0, "tocou antes de encher");
        m.empurra(1, std::iter::once(0.5));
        assert!(m.puxa_10ms(&mut saida) > 0.49);
        assert!(saida.iter().all(|&s| s > 16000));
    }

    #[test]
    fn mixer_soma_os_apps_e_satura_sem_estourar() {
        let m = Mixer::default();
        let mut saida = [0i16; AMOSTRAS_10MS];
        m.empurra(1, std::iter::repeat_n(0.8, PRE_BUFFER));
        m.empurra(2, std::iter::repeat_n(0.8, PRE_BUFFER));
        assert_eq!(m.puxa_10ms(&mut saida), 1.0);
        assert!(
            saida.iter().all(|&s| s == i16::MAX),
            "a soma 1,6 tem que saturar em 1,0"
        );
    }

    #[test]
    fn mixer_corta_o_atraso_acumulado() {
        let m = Mixer::default();
        m.empurra(1, std::iter::repeat_n(0.1, MAX_FILA * 3));
        assert_eq!(m.filas.lock().unwrap()[&1].amostras.len(), MAX_FILA);
    }
}
