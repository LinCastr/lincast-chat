//! Módulo de interface para o Soundboard da sala (A2).
//!
//! Exibe o painel de efeitos sonoros rápidos da sala com suporte a silenciamento
//! global e silenciamento individual por participante.

use std::collections::HashSet;

use iced::widget::{button, column, container, row, slider, svg, text};
use iced::{Alignment, Element, Length};

use crate::som::{SONS, VOLUME_PADRAO};
use crate::{Msg, destaque, fraco, icones, negrito, tema};

/// Estado do soundboard da sala no cliente.
#[derive(Debug, Clone)]
pub struct EstadoSoundboard {
    /// Se o usuário silenciou o soundboard inteiro da sala.
    pub mudo_global: bool,
    /// Nomes ou UIDs de participantes cujos sons foram silenciados individualmente.
    pub silenciados: HashSet<String>,
    /// Volume (0–100) dos sons que EU solto: vale igual para quem ouve.
    pub volume: u8,
}

impl Default for EstadoSoundboard {
    fn default() -> Self {
        Self {
            mudo_global: false,
            silenciados: HashSet::new(),
            volume: carrega_volume(),
        }
    }
}

fn arquivo_volume() -> Option<std::path::PathBuf> {
    Some(
        directories::ProjectDirs::from("", "", "lincast-chat")?
            .config_dir()
            .join("soundboard.json"),
    )
}

/// Volume guardado em disco (ou o padrão).
pub fn carrega_volume() -> u8 {
    arquivo_volume()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v["volume"].as_u64())
        .map(|v| v.min(100) as u8)
        .unwrap_or(VOLUME_PADRAO)
}

pub fn salva_volume(volume: u8) {
    if let Some(p) = arquivo_volume() {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(p, format!("{{\"volume\":{volume}}}"));
    }
}

impl EstadoSoundboard {
    /// Alterna o silenciamento individual de um participante.
    pub fn alterna_silenciado(&mut self, id_ou_nome: &str) {
        if self.silenciados.contains(id_ou_nome) {
            self.silenciados.remove(id_ou_nome);
        } else {
            self.silenciados.insert(id_ou_nome.to_string());
        }
    }

    /// Verifica se um determinado participante está com sons silenciados.
    pub fn esta_silenciado(&self, id_ou_nome: &str) -> bool {
        self.mudo_global || self.silenciados.contains(id_ou_nome)
    }
}

fn icone_do_som(id: &str) -> svg::Handle {
    match id {
        "buzina" => icones::megafone(),
        "tambor" => icones::tambor(),
        "rimshot" => icones::smile(),
        "moeda" => icones::moedas(),
        "erro" => icones::erro_circulo(),
        "fanfarra" => icones::musica(),
        "laser" => icones::raio(),
        "pop" => icones::brilho(),
        "apito" => icones::sino_toque(),
        _ => icones::batimento(),
    }
}

/// Renderiza o painel completo do soundboard da sala.
pub fn painel_soundboard<'a>(estado: &'a EstadoSoundboard) -> Element<'a, Msg> {
    let c = tema::cores();
    let titulo = text("Soundboard da Sala")
        .size(13)
        .font(negrito())
        .style(destaque);

    let botao_mudo = button(
        row![
            crate::ui::efeitos::ico(
                if estado.mudo_global {
                    icones::volume_mudo()
                } else {
                    icones::volume()
                },
                13.0,
                c.texto
            ),
            text(if estado.mudo_global {
                "Reativar sons da sala"
            } else {
                "Silenciar sala"
            })
            .size(11)
            .font(negrito()),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .padding([3, 8])
    .style(if estado.mudo_global {
        tema::botao_perigo
    } else {
        tema::botao_secundario
    })
    .on_press(Msg::AlternaMuteSoundboardGlobal);

    let topo = row![titulo, botao_mudo]
        .spacing(10)
        .align_y(Alignment::Center);

    // Volume de quem solta: vale para todos que ouvem.
    let volume = row![
        crate::ui::efeitos::ico(icones::volume_baixo(), 14.0, c.texto_medio),
        slider(0..=100u8, estado.volume, Msg::VolumeSoundboard)
            .on_release(Msg::SalvaVolumeSoundboard)
            .style(tema::estilo_slider)
            .width(Length::Fixed(170.0)),
        text(format!("{}%", estado.volume))
            .size(11)
            .width(34)
            .style(fraco),
        text("Volume dos sons que você solta (todos ouvem igual)")
            .size(11)
            .style(fraco),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    // Grade dos 10 sons organizados em 2 linhas de 5 botões
    let mut linha1 = row![].spacing(6).align_y(Alignment::Center);
    let mut linha2 = row![].spacing(6).align_y(Alignment::Center);

    for (i, &(id, rotulo)) in SONS.iter().enumerate() {
        let b = button(
            row![
                crate::ui::efeitos::ico(icone_do_som(id), 14.0, c.texto_medio),
                text(rotulo).size(12).font(negrito()),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .padding([6, 9])
        .style(tema::botao_secundario)
        .on_press(Msg::TocarSom(id.to_string()));

        if i < 5 {
            linha1 = linha1.push(b);
        } else {
            linha2 = linha2.push(b);
        }
    }

    let dica = text("Um som a cada 2s. Botão de som nos participantes silencia individualmente.")
        .size(11)
        .style(fraco);

    let painel = column![topo, volume, linha1, linha2, dica].spacing(8);

    container(painel)
        .padding([8, 12])
        .style(tema::painel_lateral)
        .into()
}

/// Botão compacto para silenciar os sons de um participante específico na linha de pessoa.
// Sem consumidor até o menu de contexto (C5) e o palco da chamada (C8).
#[allow(dead_code)]
pub fn botao_silenciar_participante<'a>(nome: &'a str, silenciado: bool) -> Element<'a, Msg> {
    let c = tema::cores();
    button(crate::ui::efeitos::ico(
        if silenciado {
            icones::volume_mudo()
        } else {
            icones::volume()
        },
        12.0,
        c.texto,
    ))
    .padding([2, 5])
    .style(if silenciado {
        tema::botao_perigo
    } else {
        tema::botao_secundario
    })
    .on_press(Msg::AlternaMuteSoundboardPessoa(nome.to_string()))
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn testa_estado_soundboard_silenciamento() {
        let mut st = EstadoSoundboard::default();
        assert!(!st.esta_silenciado("Alice"));

        st.alterna_silenciado("Alice");
        assert!(st.esta_silenciado("Alice"));
        assert!(!st.esta_silenciado("Bob"));

        st.alterna_silenciado("Alice");
        assert!(!st.esta_silenciado("Alice"));

        st.mudo_global = true;
        assert!(st.esta_silenciado("Alice"));
        assert!(st.esta_silenciado("Bob"));
    }
}
