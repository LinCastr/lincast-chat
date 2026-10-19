//! Popup da seta do microfone na barra da chamada (como o do Discord,
//! `refs/discord/02-popup-seta-microfone.png`): dispositivos de entrada e
//! saída (lista ao lado), volumes do sistema, nível do microfone ao vivo,
//! "Desativar áudio" e o atalho para as configurações de voz.

use iced::widget::{Column, button, column, container, row, space, svg, text};
use iced::{Alignment, Background, Border, Color, Element, Length};

use crate::dispositivos::DispositivoAudio;
use crate::ui::configuracoes::{interruptor, nome_de_dispositivo, slider_volume};
use crate::{Msg, fraco, icones, tema, ui};

/// Largura de cada cartão (o principal e a lista de dispositivos ao lado).
pub const LARGURA: f32 = 280.0;
/// Altura aproximada do cartão principal, para posicioná-lo acima do botão.
pub const ALTURA: f32 = 390.0;

/// Qual lista de dispositivos está aberta ao lado do popup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubVoz {
    Entrada,
    Saida,
}

pub struct DadosPopupVoz<'a> {
    pub entradas: &'a [DispositivoAudio],
    pub saidas: &'a [DispositivoAudio],
    pub entrada_sel: Option<&'a str>,
    pub saida_sel: Option<&'a str>,
    pub volume_mic: Option<f32>,
    pub volume_saida: Option<f32>,
    /// Nível do microfone, 0–100 (mesma escala do teste de microfone).
    pub nivel: f32,
    pub surdo: bool,
    pub sub: Option<SubVoz>,
}

/// Nome do dispositivo escolhido (ou o primeiro da lista, que é o que o app usa).
fn nome_escolhido(lista: &[DispositivoAudio], sel: Option<&str>) -> String {
    sel.and_then(|g| lista.iter().find(|d| d.guid == g))
        .or_else(|| lista.first())
        .map(|d| nome_de_dispositivo(&d.nome))
        .unwrap_or_else(|| "Nenhum detectado".into())
}

fn cartao<'a>(corpo: Column<'a, Msg>) -> Element<'a, Msg> {
    let c = tema::cores();
    container(corpo)
        .padding(6)
        .width(LARGURA)
        .style(move |_| container::Style {
            background: Some(Background::Color(c.lateral)),
            border: Border {
                color: c.borda,
                width: 1.0,
                radius: tema::RAIO_GRANDE.into(),
            },
            shadow: iced::Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.4),
                offset: iced::Vector::new(0.0, 4.0),
                blur_radius: 14.0,
            },
            ..Default::default()
        })
        .into()
}

fn estilo_linha(aberta: bool) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_, st| {
        let c = tema::cores();
        button::Style {
            background: (aberta || matches!(st, button::Status::Hovered))
                .then_some(Background::Color(c.elevado)),
            text_color: c.texto,
            border: Border {
                radius: tema::RAIO.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

/// "Dispositivo de entrada / USB PnP ... >": abre a lista ao lado.
fn linha_de_dispositivo<'a>(
    titulo: &'a str,
    escolhido: String,
    aberta: bool,
    sub: SubVoz,
) -> Element<'a, Msg> {
    let c = tema::cores();
    button(
        row![
            column![text(titulo).size(14), text(escolhido).size(12).style(fraco),]
                .spacing(2)
                .width(Length::Fill),
            svg(icones::seta_direita())
                .width(14)
                .height(14)
                .style(tema::icone_cor(if aberta {
                    c.texto
                } else {
                    c.texto_fraco
                })),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding([8, 10])
    .width(Length::Fill)
    .style(estilo_linha(aberta))
    .on_press(Msg::AlternaSubVoz(sub))
    .into()
}

/// Medidor em segmentos, aceso até o nível atual.
fn medidor<'a>(nivel: f32) -> Element<'a, Msg> {
    const SEGMENTOS: usize = 28;
    let c = tema::cores();
    let acesos = ((nivel / 100.0).clamp(0.0, 1.0) * SEGMENTOS as f32).round() as usize;
    row((0..SEGMENTOS).map(|i| -> Element<'a, Msg> {
        let ligado = i < acesos;
        container(space().width(4).height(16))
            .style(move |_| container::Style {
                background: Some(Background::Color(if ligado {
                    c.sucesso_forte
                } else {
                    c.forte
                })),
                border: Border {
                    radius: 2.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
    }))
    .spacing(4)
    .into()
}

fn bloco<'a>(rotulo: &'a str, corpo: Element<'a, Msg>) -> Element<'a, Msg> {
    container(column![text(rotulo).size(13).style(fraco), corpo].spacing(6))
        .padding([6, 10])
        .into()
}

/// O cartão principal.
pub fn principal<'a>(d: &DadosPopupVoz<'a>) -> Element<'a, Msg> {
    let col = column![
        linha_de_dispositivo(
            "Dispositivo de entrada",
            nome_escolhido(d.entradas, d.entrada_sel),
            d.sub == Some(SubVoz::Entrada),
            SubVoz::Entrada,
        ),
        linha_de_dispositivo(
            "Dispositivo de saída",
            nome_escolhido(d.saidas, d.saida_sel),
            d.sub == Some(SubVoz::Saida),
            SubVoz::Saida,
        ),
        container(ui::menu::separador()).padding([4, 6]),
        bloco("Volume de entrada", slider_volume(true, d.volume_mic)),
        bloco("Nível de entrada", medidor(d.nivel)),
        bloco("Volume de saída", slider_volume(false, d.volume_saida)),
        container(ui::menu::separador()).padding([4, 6]),
        container(
            row![
                text("Desativar áudio").size(14).width(Length::Fill),
                interruptor(d.surdo, Msg::AlternaSurdo),
            ]
            .align_y(Alignment::Center),
        )
        .padding([6, 10]),
        ui::menu::item(
            icones::configuracoes(),
            "Configurações de voz",
            Msg::AbreConfiguracoes(Some(ui::configuracoes::AbaConfig::VozVideo)),
            false,
        ),
    ]
    .spacing(2);
    cartao(col)
}

/// A lista de dispositivos aberta ao lado (escolha única, com ✓ no atual).
pub fn lista<'a>(d: &DadosPopupVoz<'a>, sub: SubVoz) -> Element<'a, Msg> {
    let c = tema::cores();
    let (todos, sel) = match sub {
        SubVoz::Entrada => (d.entradas, d.entrada_sel),
        SubVoz::Saida => (d.saidas, d.saida_sel),
    };
    let atual = sel
        .filter(|g| todos.iter().any(|d| d.guid == *g))
        .or_else(|| todos.first().map(|d| d.guid.as_str()));
    let mut col = Column::new().spacing(2);
    if todos.is_empty() {
        col = col.push(
            container(text("Nenhum dispositivo detectado.").size(13).style(fraco)).padding([8, 10]),
        );
    }
    for disp in todos {
        let marcado = Some(disp.guid.as_str()) == atual;
        let msg = match sub {
            SubVoz::Entrada => Msg::SelecionaEntradaAudio(disp.guid.clone()),
            SubVoz::Saida => Msg::SelecionaSaidaAudio(disp.guid.clone()),
        };
        let marca: Element<'a, Msg> = if marcado {
            svg(icones::check())
                .width(16)
                .height(16)
                .style(tema::icone_cor(c.destaque))
                .into()
        } else {
            space().width(16).into()
        };
        col = col.push(
            button(
                row![
                    text(nome_de_dispositivo(&disp.nome))
                        .size(14)
                        .width(Length::Fill),
                    marca,
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding([8, 10])
            .width(Length::Fill)
            .style(estilo_linha(false))
            .on_press(msg),
        );
    }
    cartao(col)
}

/// Distância do topo do cartão principal até a linha da lista aberta: a
/// lista ao lado começa na altura da linha que a abriu.
pub fn deslocamento_da_lista(sub: SubVoz) -> f32 {
    match sub {
        SubVoz::Entrada => 0.0,
        SubVoz::Saida => 56.0,
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn d(guid: &str, nome: &str) -> DispositivoAudio {
        DispositivoAudio {
            indice: 0,
            nome: nome.into(),
            guid: guid.into(),
        }
    }

    #[test]
    fn nome_escolhido_cai_no_primeiro_e_tira_o_default() {
        let l = vec![d("nome:default", "default: Fone USB"), d("nome:b", "Mic B")];
        assert_eq!(nome_escolhido(&l, Some("nome:b")), "Mic B");
        assert_eq!(nome_escolhido(&l, Some("sumiu")), "Fone USB (padrão)");
        assert_eq!(nome_escolhido(&l, None), "Fone USB (padrão)");
        assert_eq!(nome_escolhido(&[], None), "Nenhum detectado");
    }
}
