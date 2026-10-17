//! Painel de diagnóstico do Lincast Chat (Ctrl+Shift+D).
//!
//! Exibe métricas de rede WebRTC por par (candidatos ICE, tipo de rota, RTT,
//! perda de pacotes, jitter, bitrate), estado dos dispositivos de mídia locais,
//! servidores ICE configurados e telemetria operacional do servidor (/api/metrics).

use iced::widget::{
    Column, Container, Row, button, column, container, row, scrollable, space, text,
};
use iced::{Alignment, Border, Color, Element, Length, Shadow, Vector};

use crate::api::MetricasServidor;
use crate::sala::QualidadeConexao;
use crate::tema::Cores;

/// Dados locais do cliente para exibição no diagnóstico.
#[derive(Debug, Clone, Default)]
pub struct InfoDiagnosticoLocal {
    pub versao_app: String,
    pub sala: Option<String>,
    pub usuario: String,
    pub espectador: bool,
    pub sou_dono: bool,
    pub tema_nome: String,
    pub densidade_nome: String,
    pub mic_nome: String,
    pub saida_nome: String,
    pub camera_nome: String,
    pub ice_servers_qtd: usize,
    pub suporte_turn: bool,
}

/// Dados de cada par WebRTC conectado na chamada.
#[derive(Debug, Clone, Default)]
pub struct InfoDiagnosticoPeer {
    pub id: String,
    pub nome: String,
    pub uid: Option<String>,
    pub qualidade: Option<QualidadeConexao>,
    pub falando: bool,
    pub mutado: bool,
    pub ensurdecido: bool,
    pub transmitindo_tela: bool,
    pub transmitindo_camera: bool,
}

/// Formata segundos em texto amigável: dias, horas e minutos.
pub fn formata_uptime(segundos: u64) -> String {
    if segundos == 0 {
        return "0s".into();
    }
    let dias = segundos / 86400;
    let horas = (segundos % 86400) / 3600;
    let minutos = (segundos % 3600) / 60;
    let seg = segundos % 60;

    if dias > 0 {
        format!("{dias}d {horas}h {minutos}m")
    } else if horas > 0 {
        format!("{horas}h {minutos}m")
    } else if minutos > 0 {
        format!("{minutos}m {seg}s")
    } else {
        format!("{seg}s")
    }
}

/// Formata uso de memória em MB e porcentagem.
pub fn formata_memoria(usada_mb: u64, total_mb: u64) -> String {
    if total_mb == 0 {
        return format!("{usada_mb} MB");
    }
    let pct = (usada_mb as f64 / total_mb as f64) * 100.0;
    format!("{usada_mb} MB / {total_mb} MB ({pct:.1}%)")
}

/// Formata taxa de transmissão em kbps ou Mbps.
pub fn formata_bitrate(kbps: Option<u64>) -> String {
    match kbps {
        Some(k) if k >= 1000 => format!("{:.2} Mbps", k as f64 / 1000.0),
        Some(k) => format!("{k} kbps"),
        None => "--".into(),
    }
}

/// Formata perda de pacotes em porcentagem.
pub fn formata_perda(perda: Option<f64>) -> String {
    match perda {
        Some(p) => format!("{:.1}%", (p * 100.0).clamp(0.0, 100.0)),
        None => "0.0%".into(),
    }
}

/// Formata tempo de ida e volta (RTT) em milissegundos.
pub fn formata_rtt(rtt_ms: Option<u64>) -> String {
    match rtt_ms {
        Some(r) => format!("{r} ms"),
        None => "--".into(),
    }
}

/// Formata variação estatística de chegada de pacotes (jitter).
pub fn formata_jitter(jitter_ms: Option<u64>) -> String {
    match jitter_ms {
        Some(j) => format!("{j} ms"),
        None => "--".into(),
    }
}

/// Formata vetor de carga média (1m, 5m, 15m).
pub fn formata_carga(carga: &[f64]) -> String {
    if carga.is_empty() {
        return "--".into();
    }
    let partes: Vec<String> = carga.iter().map(|c| format!("{c:.2}")).collect();
    partes.join(", ")
}

/// Monta o modal do painel de diagnóstico.
pub fn modal_diagnostico<'a, M: 'a + Clone>(
    local: &InfoDiagnosticoLocal,
    peers: &[InfoDiagnosticoPeer],
    metricas_servidor: Option<&MetricasServidor>,
    carregando_metricas: bool,
    p: Cores,
    ao_fechar: M,
    ao_atualizar: M,
) -> Element<'a, M> {
    // --- Cabeçalho do modal ---
    let titulo = text("Painel de Diagnóstico").size(17).color(p.texto);

    let badge_atalho = container(text("Ctrl+Shift+D ou Esc").size(11).color(p.texto_fraco))
        .padding([3, 8])
        .style(move |_| container::Style {
            background: Some(p.lateral.into()),
            border: Border {
                color: p.borda,
                width: 1.0,
                radius: 4.0.into(),
            },
            ..Default::default()
        });

    let btn_atualizar = button(
        row![
            crate::ui::efeitos::ico(crate::icones::atualizar(), 12.0, p.texto),
            text(if carregando_metricas {
                "Atualizando..."
            } else {
                "Atualizar"
            })
            .size(12)
            .color(p.texto),
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center),
    )
    .padding([5, 10])
    .style(move |_, status| {
        let mut base = p.lateral;
        if status == button::Status::Hovered {
            base = p.borda;
        }
        button::Style {
            background: Some(base.into()),
            text_color: p.texto,
            border: Border {
                color: p.borda,
                width: 1.0,
                radius: 5.0.into(),
            },
            ..Default::default()
        }
    })
    .on_press(ao_atualizar);

    let btn_fechar = button(crate::ui::efeitos::ico(
        crate::icones::fechar(),
        14.0,
        p.texto_fraco,
    ))
    .padding([4, 8])
    .style(move |_, status| {
        let bg = if status == button::Status::Hovered {
            Some(p.lateral.into())
        } else {
            None
        };
        button::Style {
            background: bg,
            text_color: p.texto,
            border: Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    })
    .on_press(ao_fechar.clone());

    let cabecalho = row![
        titulo,
        badge_atalho,
        space().width(Length::Fill),
        btn_atualizar,
        btn_fechar,
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    // --- Seção 1: Cliente e Mídia Local ---
    let cartao_local = cria_cartao(
        "Cliente e Dispositivos Locais",
        column![
            item_diagnostico("Versão do App", &local.versao_app, p.texto, p.texto_fraco),
            item_diagnostico("Usuário Conectado", &local.usuario, p.texto, p.texto_fraco),
            item_diagnostico(
                "Sala / Modo",
                &match &local.sala {
                    Some(s) => {
                        let modo = if local.espectador {
                            "espectador"
                        } else {
                            "na chamada"
                        };
                        let dono = if local.sou_dono { " · dono" } else { "" };
                        format!("#{s} ({modo}{dono})")
                    }
                    None => "Fora de sala".into(),
                },
                p.texto,
                p.texto_fraco,
            ),
            item_diagnostico(
                "Aparência e Geometria",
                &format!("{} · {}", local.tema_nome, local.densidade_nome),
                p.texto,
                p.texto_fraco,
            ),
            item_diagnostico(
                "Microfone de Entrada",
                &local.mic_nome,
                p.texto,
                p.texto_fraco
            ),
            item_diagnostico("Saída de Áudio", &local.saida_nome, p.texto, p.texto_fraco),
            item_diagnostico("Câmera / Vídeo", &local.camera_nome, p.texto, p.texto_fraco),
            item_diagnostico(
                "Infraestrutura ICE / TURN",
                &format!(
                    "{} servidores configurados (STUN/TURN: {})",
                    local.ice_servers_qtd,
                    if local.suporte_turn {
                        "ativo / porta 5349 suportada"
                    } else {
                        "apenas STUN público"
                    }
                ),
                p.texto,
                p.texto_fraco,
            ),
        ]
        .spacing(6),
        p.lateral,
        p.borda,
        p.texto,
    );

    // --- Seção 2: Conexões WebRTC (Peers) ---
    let conteudo_peers: Element<'a, M> = if peers.is_empty() {
        text("Nenhum par WebRTC conectado na chamada no momento.")
            .size(13)
            .color(p.texto_fraco)
            .into()
    } else {
        let mut col_pares = Column::new().spacing(8);
        for peer in peers {
            let mut itens = Column::new().spacing(5);

            let rota_str = peer
                .qualidade
                .as_ref()
                .and_then(|q| q.rota.as_deref())
                .unwrap_or("medindo / descobrindo rota...");

            let cor_rota = if rota_str.contains("TURN") {
                Color::from_rgb8(0xf0, 0xb2, 0x32) // âmbar para relay
            } else if rota_str.contains("host") || rota_str.contains("P2P") {
                Color::from_rgb8(0x23, 0xa5, 0x5a) // verde para direta
            } else {
                p.texto_fraco
            };

            let perda_str = formata_perda(peer.qualidade.as_ref().and_then(|q| q.perda));
            let rtt_str = formata_rtt(peer.qualidade.as_ref().and_then(|q| q.rtt_ms));
            let jitter_str = formata_jitter(peer.qualidade.as_ref().and_then(|q| q.jitter_ms));
            let bitrate_str = formata_bitrate(peer.qualidade.as_ref().and_then(|q| q.bitrate_kbps));

            let status_midia = {
                let mut partes = Vec::new();
                if peer.falando {
                    partes.push("falando");
                }
                if peer.mutado {
                    partes.push("mutado");
                }
                if peer.ensurdecido {
                    partes.push("ensurdecido");
                }
                if peer.transmitindo_tela {
                    partes.push("tela");
                }
                if peer.transmitindo_camera {
                    partes.push("câmera");
                }
                if partes.is_empty() {
                    "ouvindo".into()
                } else {
                    partes.join(" · ")
                }
            };

            itens = itens.push(item_diagnostico(
                "Status de Mídia",
                &status_midia,
                p.texto,
                p.texto_fraco,
            ));
            itens = itens.push(item_diagnostico(
                "Rota WebRTC",
                rota_str,
                cor_rota,
                p.texto_fraco,
            ));
            itens = itens.push(
                row![
                    item_diagnostico_metade("Latência (RTT)", &rtt_str, p.texto, p.texto_fraco),
                    item_diagnostico_metade("Perda de Pacotes", &perda_str, p.texto, p.texto_fraco),
                ]
                .spacing(10),
            );
            itens = itens.push(
                row![
                    item_diagnostico_metade("Jitter", &jitter_str, p.texto, p.texto_fraco),
                    item_diagnostico_metade(
                        "Bitrate Estimado",
                        &bitrate_str,
                        p.texto,
                        p.texto_fraco
                    ),
                ]
                .spacing(10),
            );

            let id_info = match &peer.uid {
                Some(u) => format!("{} (UID: {})", peer.nome, u),
                None => format!("{} (ID: {})", peer.nome, peer.id),
            };

            let cartao_peer =
                container(column![text(id_info).size(13).color(p.destaque), itens,].spacing(6))
                    .padding(10)
                    .width(Length::Fill)
                    .style(move |_| container::Style {
                        background: Some(p.fundo.into()),
                        border: Border {
                            color: p.borda,
                            width: 1.0,
                            radius: 6.0.into(),
                        },
                        ..Default::default()
                    });

            col_pares = col_pares.push(cartao_peer);
        }
        col_pares.into()
    };

    let cartao_peers = cria_cartao(
        &format!("Pares WebRTC em Chamada ({})", peers.len()),
        conteudo_peers,
        p.lateral,
        p.borda,
        p.texto,
    );

    // --- Seção 3: Telemetria do Servidor (/api/metrics) ---
    let conteudo_servidor: Element<'a, M> = match metricas_servidor {
        Some(m) => column![
            item_diagnostico("Versão do Servidor", &m.versao, p.texto, p.texto_fraco),
            item_diagnostico(
                "Tempo de Atividade (Uptime)",
                &formata_uptime(m.uptime_segundos),
                p.texto,
                p.texto_fraco
            ),
            row![
                item_diagnostico_metade(
                    "Memória do Processo",
                    &format!("{} MB (heap {} MB)", m.memoria_processo_mb, m.heap_usado_mb),
                    p.texto,
                    p.texto_fraco
                ),
                item_diagnostico_metade(
                    "Memória da Máquina",
                    &formata_memoria(
                        m.memoria_total_mb.saturating_sub(m.memoria_livre_mb),
                        m.memoria_total_mb
                    ),
                    p.texto,
                    p.texto_fraco
                ),
            ]
            .spacing(10),
            row![
                item_diagnostico_metade(
                    "Carga Média (1m, 5m, 15m)",
                    &formata_carga(&m.carga_media),
                    p.texto,
                    p.texto_fraco
                ),
                item_diagnostico_metade(
                    "Conexões WebSocket",
                    &m.conexoes.to_string(),
                    p.texto,
                    p.texto_fraco
                ),
            ]
            .spacing(10),
            row![
                item_diagnostico_metade(
                    "Salas Ativas",
                    &m.salas_ativas.to_string(),
                    p.texto,
                    p.texto_fraco
                ),
                item_diagnostico_metade(
                    "Pessoas em Chamada",
                    &format!("{} (pico: {})", m.pessoas_em_chamada, m.pico_de_pessoas),
                    p.texto,
                    p.texto_fraco
                ),
            ]
            .spacing(10),
            row![
                item_diagnostico_metade(
                    "Sessões Abertas",
                    &m.sessoes_abertas.to_string(),
                    p.texto,
                    p.texto_fraco
                ),
                item_diagnostico_metade(
                    "Dados de Chat",
                    &format!("{} arquivos ({:.2} MB)", m.arquivos_de_chat, m.chat_mb),
                    p.texto,
                    p.texto_fraco
                ),
            ]
            .spacing(10),
        ]
        .spacing(6)
        .into(),
        None if carregando_metricas => text("Consultando /api/metrics no servidor...")
            .size(13)
            .color(p.texto_fraco)
            .into(),
        None => text("Métricas do servidor indisponíveis (requer login ativo).")
            .size(13)
            .color(p.texto_fraco)
            .into(),
    };

    let cartao_servidor = cria_cartao(
        "Métricas Operacionais do Servidor (/api/metrics)",
        conteudo_servidor,
        p.lateral,
        p.borda,
        p.texto,
    );

    // --- Rodapé ---
    let rodape = row![
        text("LinCast Chat • WebRTC Diagnóstico em Tempo Real")
            .size(11)
            .color(p.texto_fraco),
        space().width(Length::Fill),
        button(text("Fechar").size(12).color(p.texto))
            .padding([6, 16])
            .style(move |_, status| {
                let bg = if status == button::Status::Hovered {
                    p.borda
                } else {
                    p.lateral
                };
                button::Style {
                    background: Some(bg.into()),
                    text_color: p.texto,
                    border: Border {
                        color: p.borda,
                        width: 1.0,
                        radius: 6.0.into(),
                    },
                    ..Default::default()
                }
            })
            .on_press(ao_fechar),
    ]
    .align_y(Alignment::Center);

    let corpo = scrollable(
        column![cartao_local, cartao_peers, cartao_servidor]
            .spacing(14)
            .width(Length::Fill),
    )
    .height(Length::FillPortion(1));

    let painel = container(
        column![cabecalho, corpo, rodape]
            .spacing(12)
            .width(Length::Fixed(640.0))
            .height(Length::Fixed(560.0)),
    )
    .padding(18)
    .style(move |_| container::Style {
        background: Some(p.elevado.into()),
        border: Border {
            color: p.destaque,
            width: 1.5,
            radius: 10.0.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.5),
            offset: Vector::new(0.0, 6.0),
            blur_radius: 16.0,
        },
        ..Default::default()
    });

    // Fundo semitransparente escuro
    container(painel)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(|_| container::Style {
            background: Some(Color::from_rgba(0.0, 0.0, 0.0, 0.65).into()),
            ..Default::default()
        })
        .into()
}

fn cria_cartao<'a, M: 'a>(
    titulo: &str,
    conteudo: impl Into<Element<'a, M>>,
    fundo: Color,
    borda: Color,
    cor_titulo: Color,
) -> Container<'a, M> {
    container(
        column![
            text(titulo.to_string()).size(13).color(cor_titulo),
            conteudo.into(),
        ]
        .spacing(8)
        .width(Length::Fill),
    )
    .padding(12)
    .width(Length::Fill)
    .style(move |_| container::Style {
        background: Some(fundo.into()),
        border: Border {
            color: borda,
            width: 1.0,
            radius: 8.0.into(),
        },
        ..Default::default()
    })
}

fn item_diagnostico<'a, M: 'a>(
    rotulo: &str,
    valor: &str,
    cor_valor: Color,
    cor_rotulo: Color,
) -> Row<'a, M> {
    row![
        text(rotulo.to_string())
            .size(12)
            .color(cor_rotulo)
            .width(Length::Fixed(190.0)),
        text(valor.to_string())
            .size(12)
            .color(cor_valor)
            .width(Length::Fill),
    ]
    .align_y(Alignment::Center)
    .spacing(8)
}

fn item_diagnostico_metade<'a, M: 'a>(
    rotulo: &str,
    valor: &str,
    cor_valor: Color,
    cor_rotulo: Color,
) -> Row<'a, M> {
    row![
        text(rotulo.to_string())
            .size(12)
            .color(cor_rotulo)
            .width(Length::Fixed(120.0)),
        text(valor.to_string())
            .size(12)
            .color(cor_valor)
            .width(Length::Fill),
    ]
    .align_y(Alignment::Center)
    .spacing(6)
    .width(Length::FillPortion(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn testa_formata_uptime() {
        assert_eq!(formata_uptime(0), "0s");
        assert_eq!(formata_uptime(45), "45s");
        assert_eq!(formata_uptime(125), "2m 5s");
        assert_eq!(formata_uptime(3660), "1h 1m");
        assert_eq!(formata_uptime(90060), "1d 1h 1m");
    }

    #[test]
    fn testa_formata_memoria() {
        assert_eq!(formata_memoria(50, 0), "50 MB");
        assert_eq!(formata_memoria(4000, 16000), "4000 MB / 16000 MB (25.0%)");
    }

    #[test]
    fn testa_formata_bitrate() {
        assert_eq!(formata_bitrate(None), "--");
        assert_eq!(formata_bitrate(Some(450)), "450 kbps");
        assert_eq!(formata_bitrate(Some(1250)), "1.25 Mbps");
        assert_eq!(formata_bitrate(Some(2500)), "2.50 Mbps");
    }

    #[test]
    fn testa_formata_perda_e_rtt_e_jitter() {
        assert_eq!(formata_perda(None), "0.0%");
        assert_eq!(formata_perda(Some(0.025)), "2.5%");
        assert_eq!(formata_rtt(None), "--");
        assert_eq!(formata_rtt(Some(35)), "35 ms");
        assert_eq!(formata_jitter(None), "--");
        assert_eq!(formata_jitter(Some(4)), "4 ms");
    }

    #[test]
    fn testa_formata_carga() {
        assert_eq!(formata_carga(&[]), "--");
        assert_eq!(formata_carga(&[0.15, 0.22, 0.18]), "0.15, 0.22, 0.18");
    }

    #[test]
    fn testa_modal_diagnostico_instancia_sem_panico() {
        let local = InfoDiagnosticoLocal {
            versao_app: "0.1.0".into(),
            sala: Some("test-room".into()),
            usuario: "Val".into(),
            espectador: false,
            sou_dono: true,
            tema_nome: "Nord".into(),
            densidade_nome: "Confortável".into(),
            mic_nome: "Microfone USB".into(),
            saida_nome: "Fone P2".into(),
            camera_nome: "Webcam HD".into(),
            ice_servers_qtd: 2,
            suporte_turn: true,
        };

        let peers = vec![InfoDiagnosticoPeer {
            id: "peer-1".into(),
            nome: "Alice".into(),
            uid: Some("u123".into()),
            qualidade: Some(QualidadeConexao {
                nivel: crate::sala::NivelConexao::Boa,
                perda: Some(0.01),
                rtt_ms: Some(25),
                jitter_ms: Some(2),
                bitrate_kbps: Some(1500),
                rota: Some("direta, mesma rede (host)".into()),
            }),
            falando: true,
            mutado: false,
            ensurdecido: false,
            transmitindo_tela: false,
            transmitindo_camera: false,
        }];

        let metricas = MetricasServidor {
            uptime_segundos: 7200,
            memoria_processo_mb: 65,
            heap_usado_mb: 35,
            memoria_livre_mb: 4000,
            memoria_total_mb: 16000,
            carga_media: vec![0.12, 0.15, 0.18],
            conexoes: 4,
            salas_ativas: 1,
            pessoas_em_chamada: 2,
            pico_de_pessoas: 5,
            salas_com_senha_salva: 1,
            sessoes_abertas: 3,
            arquivos_de_chat: 10,
            chat_mb: 0.8,
            versao: "2.1.0".into(),
        };

        let cores = crate::tema::GRAFITE;

        // Deve instanciar elemento de UI válido sem disparar panic
        let _elem = modal_diagnostico::<()>(&local, &peers, Some(&metricas), false, cores, (), ());
    }
}
