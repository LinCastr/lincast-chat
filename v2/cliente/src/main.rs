//! Lincast Chat v2 — cliente desktop nativo.
//!
//! Primeira versão com janela: entrar na conta, ver e entrar em salas, chat,
//! voz e compartilhar tela com o som dos apps escolhidos. Conversa com o
//! servidor ATUAL (v1), então convive na mesma sala com quem usa o site.

mod anim;
mod api;
mod app_evento;
mod app_geral;
mod app_update;
mod app_view;
mod atalhos;
mod atividade;
mod avatar;
mod avisos;
mod captura_camera;
mod captura_tela;
mod cosmeticos;
mod desempenho;
mod dispositivos;
mod eco;
mod estado;
mod gifanim;
mod icones;
mod jogos_detectaveis;
mod jogos_registrados;
mod portal_tela;
mod prefs_conversas;
mod ptt;
mod relatorio;
mod sala;
mod sala_update;
mod sala_view;
mod sistema_audio;
mod som;
mod tarefas;
mod telas;
mod tema;
mod ui;
mod volumes;

use std::collections::HashMap;

use iced::widget::{
    Column, button, center, container, image, row, scrollable, space, svg, text, text_input,
};
use iced::{Alignment, Element, Font, Length, Subscription, Task, font};
use tokio::sync::mpsc;

use api::{Eu, Servidor};
use ptt::PttConfig;
use sala::{Comando, Evento, Mensagem, Pessoa};

pub(crate) use estado::*;
pub(crate) use tarefas::*;
pub(crate) use telas::*;

fn main() -> iced::Result {
    // O iced pede a GPU de alto desempenho quando nada é dito, e num notebook
    // híbrido isso acorda a dedicada só pra desenhar a janela. A integrada dá
    // conta e gasta menos bateria. Quem quiser a dedicada define a variável.
    if std::env::var_os("WGPU_POWER_PREF").is_none() {
        // SAFETY: primeira linha do main, antes de existir qualquer outra thread.
        unsafe { std::env::set_var("WGPU_POWER_PREF", "low") };
    }
    desempenho::carrega();
    relatorio::carrega();
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(
        "info,iced_winit=warn,winit=warn,wgpu=warn,naga=warn,zbus=warn,tracing=warn,fontdb=error",
    ))
    .format_timestamp_millis()
    .target(relatorio::alvo_do_log())
    .init();
    std::panic::set_hook(Box::new(|info| {
        let bt = std::backtrace::Backtrace::capture();
        log::error!("PANIC DETECTADO: {info}\n{bt:?}");
        eprintln!("PANIC DETECTADO: {info}\n{bt:?}");
    }));
    let _ = rustls::crypto::ring::default_provider().install_default();
    tema::carrega_tema_salvo();

    let mut app = iced::application(App::novo, App::update, App::view)
        .title(App::titulo)
        .font(include_bytes!("../assets/fonts/Figtree.ttf").as_slice());
    // Fontes do "Estilo do nome exibido".
    for f in ui::nome::ARQUIVOS {
        app = app.font(f);
    }
    app.default_font(tema::FONTE)
        .subscription(App::subscription)
        .theme(|_: &App| tema::tema_iced())
        .window(janela())
        .run()
}

fn janela() -> iced::window::Settings {
    #[allow(unused_mut)] // só é mexido no Linux
    let mut w = iced::window::Settings {
        size: iced::Size::new(1100.0, 720.0),
        min_size: Some(iced::Size::new(760.0, 480.0)),
        icon: iced::window::icon::from_file_data(include_bytes!("../assets/icone-256.png"), None)
            .ok(),
        exit_on_close_request: false,
        ..Default::default()
    };
    // No Wayland o ícone da barra de tarefas não vem da janela: vem do
    // arquivo `.desktop` cujo nome bate com este id (lincast-chat.desktop).
    #[cfg(target_os = "linux")]
    {
        w.platform_specific.application_id = "lincast-chat".into();
    }
    w
}
