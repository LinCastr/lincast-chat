//! Manual e guia de mecânicas do Lincast Chat.
//!
//! Documenta atalhos, comandos de chat, recursos de chamada, atividades e
//! gerenciamento de sala, com suporte a busca em tempo real.

use iced::widget::{Column, button, column, container, row, scrollable, space, text, text_input};
use iced::{Alignment, Border, Color, Element, Length, Shadow, Vector};

use crate::tema::Cores;

/// Item individual de documentação no manual.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemManual {
    pub titulo: &'static str,
    pub descricao: &'static str,
}

/// Seção temática contendo itens relacionados.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecaoManual {
    pub nome: &'static str,
    pub itens: &'static [ItemManual],
}

pub const SECOES_MANUAL: &[SecaoManual] = &[
    SecaoManual {
        nome: "Chat",
        itens: &[
            ItemManual {
                titulo: "Enter / Shift+Enter",
                descricao: "Enter envia a mensagem; Shift+Enter quebra a linha no rascunho sem enviar.",
            },
            ItemManual {
                titulo: "@nome (Menção)",
                descricao: "Menciona um participante da sala — a mensagem ganha destaque visual e notificação.",
            },
            ItemManual {
                titulo: "``` Código",
                descricao: "Três crases criam bloco de código com formatação monoespaçada; crase simples destaca trechos no texto.",
            },
            ItemManual {
                titulo: "/rolar NdM+B",
                descricao: "Rola dados no chat (ex: /rolar 2d20+5, /rolar 1d6) e publica o resultado para a sala.",
            },
            ItemManual {
                titulo: "/sortear a, b, c",
                descricao: "Sorteia aleatoriamente uma das opções separadas por vírgula ou barra vertical.",
            },
            ItemManual {
                titulo: "Ctrl+V / Arrastar Imagem",
                descricao: "Cole imagens da área de transferência ou arraste arquivos para o chat (PNG, JPEG, GIF, WEBP até 5MB).",
            },
            ItemManual {
                titulo: "Ações da Mensagem",
                descricao: "Passe o cursor sobre uma mensagem para responder com citação, copiar o texto, fixar no topo, editar ou apagar.",
            },
            ItemManual {
                titulo: "Ctrl+Shift+F (Busca no Chat)",
                descricao: "Abre a barra de pesquisa rápida no histórico de mensagens carregado na sessão.",
            },
            ItemManual {
                titulo: "Sussurrar (Mensagem Privada)",
                descricao: "No menu de ações ou botão '+', envie mensagem efêmera recebida exclusivamente pelas pessoas selecionadas.",
            },
            ItemManual {
                titulo: "GIFs Animados (Giphy)",
                descricao: "Pesquise e envie GIFs animados diretamente no chat através do botão de GIF.",
            },
            ItemManual {
                titulo: "Enquetes Interativas",
                descricao: "Crie enquetes com até 6 opções pelo comando /enquete ou botão de gráfico, com contagem e alternância de voto ao vivo.",
            },
        ],
    },
    SecaoManual {
        nome: "Chamada e WebRTC",
        itens: &[
            ItemManual {
                titulo: "Entrar na Chamada",
                descricao: "Ao entrar numa sala você está só no chat: para conectar voz e vídeo, clique no botão 'Entrar na chamada'.",
            },
            ItemManual {
                titulo: "Aviso de Fala no Mudo",
                descricao: "Se você falar com o microfone mutado por mais de um segundo, o app emite um aviso visual discreto.",
            },
            ItemManual {
                titulo: "Push-to-Talk (PTT)",
                descricao: "Nas configurações de Atalhos, configure sua tecla favorita para falar apenas enquanto mantiver pressionada.",
            },
            ItemManual {
                titulo: "Bolinha de Conexão",
                descricao: "Indica a qualidade da rede com cada participante: verde (boa), âmbar (instável) e vermelha (ruim).",
            },
            ItemManual {
                titulo: "Ctrl+Shift+D (Diagnóstico)",
                descricao: "Abre o painel técnico com telemetria WebRTC por peer (rotas ICE, RTT, perda, jitter, bitrate) e métricas do servidor.",
            },
            ItemManual {
                titulo: "Foco e Grade de Vídeo",
                descricao: "Alterne entre modo grade e destaque automático do falante, ou clique em um vídeo para fixá-lo.",
            },
            ItemManual {
                titulo: "Qualidade de Tela",
                descricao: "Configure perfis (Leitura 1080p@15fps, Equilibrado 1080p@30fps, Fluido 720p@60fps) com auto-bitrate adaptativo.",
            },
        ],
    },
    SecaoManual {
        nome: "Atividades e Diversão",
        itens: &[
            ItemManual {
                titulo: "Quadro Branco Colaborativo",
                descricao: "Desenho compartilhado em tempo real na chamada com paleta de cores e espessura ajustável.",
            },
            ItemManual {
                titulo: "Roletas da Sorte",
                descricao: "Crie roletas com opções personalizadas e giro sincronizado pelo servidor sem risco de trapaça.",
            },
            ItemManual {
                titulo: "Soundboard",
                descricao: "Dispare efeitos sonoros sintetizados para a sala toda, com controle de silenciamento individual por participante.",
            },
            ItemManual {
                titulo: "Reações Flutuantes",
                descricao: "Selecione emojis para subir animações visuais flutuantes na tela de todos os membros da sala.",
            },
            ItemManual {
                titulo: "Cutucar Participante",
                descricao: "Chame a atenção de alguém na lista de pessoas com um aviso visual destacado e sonoro.",
            },
        ],
    },
    SecaoManual {
        nome: "Sala e Moderação",
        itens: &[
            ItemManual {
                titulo: "Dono da Sala",
                descricao: "Quem cria a sala pode expulsar membros (com ban temporário), silenciar forçadamente e redefinir senha.",
            },
            ItemManual {
                titulo: "Senha da Sala",
                descricao: "Proteja sua sala com senha exclusiva; salas trancadas mantêm histórico isolado e reservado.",
            },
            ItemManual {
                titulo: "Ícone da Sala",
                descricao: "Defina um emoji ou ícone visual para identificar sua sala no cabeçalho e na lista de salas ativas.",
            },
            ItemManual {
                titulo: "Link de Convite",
                descricao: "Compartilhe links no protocolo concord:// ou web; senhas são anexadas no fragmento sem vazamento na rede.",
            },
        ],
    },
    SecaoManual {
        nome: "Aparência e Layouts",
        itens: &[
            ItemManual {
                titulo: "10 Temas e Wallust",
                descricao: "Escolha entre paletas clássicas (Grafite, Nord, Drácula, Catppuccin...) ou sincronize cores dinâmicas com o Wallust do seu rice.",
            },
            ItemManual {
                titulo: "5 Layouts de Chamada (Botão ⊞)",
                descricao: "Clássico, Chat Lateral, Vídeo Cheio, Sidebar à Direita ou Só Chat: alterne instantaneamente no topo do chat.",
            },
            ItemManual {
                titulo: "Densidade Compacta",
                descricao: "Modo compacto otimizado para telas menores e monitores ultrawide, com avatares reduzidos e espaçamento denso.",
            },
            ItemManual {
                titulo: "Perfil e Identidade",
                descricao: "Personalize foto de avatar, imagem de banner, biografia, cor de exibição e efeitos cosméticos para seu nome.",
            },
        ],
    },
    SecaoManual {
        nome: "Áudio e Sistema",
        itens: &[
            ItemManual {
                titulo: "Dispositivos de Entrada e Saída",
                descricao: "Selecione seu microfone e fone de ouvido favoritos, com teste de captação ao vivo nas configurações de áudio.",
            },
            ItemManual {
                titulo: "Supressão de Eco",
                descricao: "Algoritmo de correlação cruzada detecta retorno acústico entre fones e microfones e notifica o usuário.",
            },
            ItemManual {
                titulo: "Controle de Volume Individual",
                descricao: "Ajuste o volume de cada pessoa na sala de 0% a 400% com persistência por conta de usuário.",
            },
            ItemManual {
                titulo: "Atividade (Jogando / Ouvindo)",
                descricao: "Detecção automática de jogos Steam/Linux e músicas ativas via MPRIS D-Bus embaixo do seu nome.",
            },
        ],
    },
];

/// Filtra seções e itens que contenham o termo de busca.
pub fn filtra_manual(filtro: &str) -> Vec<(&'static str, Vec<&'static ItemManual>)> {
    let termo = filtro.trim().to_lowercase();
    let mut resultado = Vec::new();

    for secao in SECOES_MANUAL {
        let itens_filtrados: Vec<&'static ItemManual> = if termo.is_empty() {
            secao.itens.iter().collect()
        } else {
            secao
                .itens
                .iter()
                .filter(|item| {
                    item.titulo.to_lowercase().contains(&termo)
                        || item.descricao.to_lowercase().contains(&termo)
                        || secao.nome.to_lowercase().contains(&termo)
                })
                .collect()
        };

        if !itens_filtrados.is_empty() {
            resultado.push((secao.nome, itens_filtrados));
        }
    }

    resultado
}

/// Renderiza o modal interativo do manual do usuário.
pub fn modal_manual<'a, M: 'a + Clone>(
    filtro: &str,
    p: Cores,
    ao_mudar_filtro: impl Fn(String) -> M + 'static,
    ao_fechar: M,
) -> Element<'a, M> {
    let titulo = text("Manual do Usuário e Guia de Recursos")
        .size(16)
        .color(p.texto);

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

    let cabecalho =
        row![titulo, space().width(Length::Fill), btn_fechar,].align_y(Alignment::Center);

    let campo_busca = text_input(
        "Filtrar por palavra-chave (ex: som, tela, atalho, roleta)...",
        filtro,
    )
    .on_input(ao_mudar_filtro)
    .padding([7, 12])
    .size(13);

    let secoes = filtra_manual(filtro);

    let corpo_conteudo: Element<'a, M> = if secoes.is_empty() {
        container(
            text("Nenhum recurso encontrado com esse termo.")
                .size(13)
                .color(p.texto_fraco),
        )
        .padding(20)
        .center_x(Length::Fill)
        .into()
    } else {
        let mut col = Column::new().spacing(14);
        for (nome_secao, itens) in secoes {
            let titulo_secao = text(nome_secao).size(14).color(p.destaque);

            let mut col_itens = Column::new().spacing(6);
            for item in itens {
                let linha = row![
                    text(item.titulo)
                        .size(13)
                        .color(p.texto)
                        .width(Length::Fixed(190.0)),
                    text(item.descricao)
                        .size(12)
                        .color(p.texto_fraco)
                        .width(Length::Fill),
                ]
                .spacing(10)
                .align_y(Alignment::Start);

                col_itens = col_itens.push(linha);
            }

            let bloco_secao = container(column![titulo_secao, col_itens].spacing(8))
                .padding(10)
                .width(Length::Fill)
                .style(move |_| container::Style {
                    background: Some(p.lateral.into()),
                    border: Border {
                        color: p.borda,
                        width: 1.0,
                        radius: 6.0.into(),
                    },
                    ..Default::default()
                });

            col = col.push(bloco_secao);
        }
        col.into()
    };

    let lista = scrollable(corpo_conteudo)
        .height(Length::FillPortion(1))
        .width(Length::Fill);

    let rodape = row![
        text("Dica: pressione Esc a qualquer momento para fechar")
            .size(11)
            .color(p.texto_fraco),
        space().width(Length::Fill),
        button(text("Entendi").size(12).color(p.texto))
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

    let painel = container(
        column![cabecalho, campo_busca, lista, rodape]
            .spacing(12)
            .width(Length::Fixed(660.0))
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
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.6),
            offset: Vector::new(0.0, 6.0),
            blur_radius: 18.0,
        },
        ..Default::default()
    });

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn testa_secoes_manual_nao_vazias() {
        assert!(SECOES_MANUAL.len() >= 5);
        for secao in SECOES_MANUAL {
            assert!(!secao.itens.is_empty());
            for item in secao.itens {
                assert!(!item.titulo.is_empty());
                assert!(!item.descricao.is_empty());
            }
        }
    }

    #[test]
    fn testa_filtro_manual_encontra_itens() {
        let todas = filtra_manual("");
        assert_eq!(todas.len(), SECOES_MANUAL.len());

        let roleta = filtra_manual("roleta");
        assert!(!roleta.is_empty());
        assert!(
            roleta
                .iter()
                .any(|(_, itens)| itens.iter().any(|i| i.titulo.contains("Roleta")))
        );

        let inexistente = filtra_manual("termo_absolutamente_inexistente_xyz");
        assert!(inexistente.is_empty());
    }

    #[test]
    fn testa_instanciacao_modal_manual_sem_panico() {
        let cores = crate::tema::GRAFITE;
        let _elem = modal_manual::<()>("", cores, |_| (), ());
    }
}
