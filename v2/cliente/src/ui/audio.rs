use crate::{Msg, PainelAudio, destaque, fraco, icones, negrito, rotulo, tema};
use iced::widget::{Column, button, container, row, space, svg, text};
use iced::{Alignment, Element, Length};

pub fn painel_config_audio<'a>(pa: &'a PainelAudio) -> Element<'a, Msg> {
    let c = tema::cores();
    let mut corpo = Column::new().spacing(12);

    let cabecalho = row![
        svg(icones::configuracoes())
            .width(18)
            .height(18)
            .style(tema::icone_cor(c.destaque)),
        text("Dispositivos de áudio e teste")
            .size(15)
            .font(negrito()),
        space().width(Length::Fill),
        button(svg(icones::fechar()).width(14).height(14))
            .padding([4, 6])
            .style(tema::botao_secundario)
            .on_press(Msg::FechaConfigAudio),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    corpo = corpo.push(cabecalho);

    // Seção Microfone (Entrada)
    let mut col_mic = Column::new().spacing(6);
    col_mic = col_mic.push(rotulo("MICROFONE (DISPOSITIVO DE ENTRADA)"));
    if pa.entradas.is_empty() {
        col_mic = col_mic.push(
            text("Nenhum microfone detectado pelo sistema.")
                .size(12)
                .style(fraco),
        );
    } else {
        let mut fila_mic = Column::new().spacing(4);
        for d in &pa.entradas {
            let ativo = pa.entrada_selecionada.as_ref() == Some(&d.guid);
            fila_mic = fila_mic.push(
                button(
                    row![
                        svg(if ativo {
                            icones::mic()
                        } else {
                            icones::mic_off()
                        })
                        .width(14)
                        .height(14)
                        .style(tema::icone_cor(if ativo {
                            c.destaque
                        } else {
                            c.texto_fraco
                        })),
                        text(&d.nome).size(12)
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .width(Length::Fill)
                .padding([6, 10])
                .style(if ativo {
                    tema::botao_primario
                } else {
                    tema::botao_secundario
                })
                .on_press(Msg::SelecionaEntradaAudio(d.guid.clone())),
            );
        }
        col_mic = col_mic.push(fila_mic);
    }
    corpo = corpo.push(col_mic);

    // Seção Saída (Fones ou Caixas)
    let mut col_saida = Column::new().spacing(6);
    col_saida = col_saida.push(rotulo("SAÍDA DE ÁUDIO (FONES OU ALTO-FALANTES)"));
    if pa.saidas.is_empty() {
        col_saida = col_saida.push(
            text("Nenhuma saída de áudio detectada pelo sistema.")
                .size(12)
                .style(fraco),
        );
    } else {
        let mut fila_saida = Column::new().spacing(4);
        for d in &pa.saidas {
            let ativo = pa.saida_selecionada.as_ref() == Some(&d.guid);
            fila_saida = fila_saida.push(
                button(
                    row![
                        svg(if ativo {
                            icones::fone()
                        } else {
                            icones::fone_off()
                        })
                        .width(14)
                        .height(14)
                        .style(tema::icone_cor(if ativo {
                            c.destaque
                        } else {
                            c.texto_fraco
                        })),
                        text(&d.nome).size(12)
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .width(Length::Fill)
                .padding([6, 10])
                .style(if ativo {
                    tema::botao_primario
                } else {
                    tema::botao_secundario
                })
                .on_press(Msg::SelecionaSaidaAudio(d.guid.clone())),
            );
        }
        col_saida = col_saida.push(fila_saida);
    }
    corpo = corpo.push(col_saida);

    // Seção Teste de Microfone
    let medidor = container(
        container(space())
            .width(Length::Fixed(
                120.0 * (pa.nivel_mic / 100.0).clamp(0.0, 1.0),
            ))
            .height(8)
            .style(tema::medidor_preenchimento),
    )
    .width(120)
    .height(8)
    .style(tema::medidor_fundo);

    let linha_teste = row![
        text("Testar microfone:").size(13).font(negrito()),
        medidor,
        button(
            text(if pa.testando_mic {
                "Parar teste"
            } else {
                "Testar"
            })
            .size(12)
        )
        .padding([4, 10])
        .style(if pa.testando_mic {
            tema::botao_perigo
        } else {
            tema::botao_primario
        })
        .on_press(Msg::AlternaTesteMic),
        if pa.testando_mic {
            text(format!("{:.0}%", pa.nivel_mic))
                .size(11)
                .style(destaque)
        } else {
            text("Fale no microfone para testar").size(11).style(fraco)
        }
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    corpo = corpo.push(container(linha_teste).padding([6, 0]));

    container(corpo)
        .padding(14)
        .style(tema::painel_lateral)
        .into()
}
