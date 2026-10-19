//! `SalaAberta`: desenho da sala (lateral, chat, mensagens).

use super::*;
use iced::widget::column;

/// Coluna de conversas (252) + divisória (1): onde começa a área do chat.
const LARGURA_COLUNA: f32 = 253.0;

impl SalaAberta {
    /// O conteúdo de um painel flutuante (e se ele fica no topo, legado).
    fn conteudo_do_painel<'a>(&'a self, p: PainelFlutuante) -> (Element<'a, Msg>, bool) {
        match p {
            PainelFlutuante::Mais => (
                ui::conversa::menu_mais(
                    self.editando.is_none() && self.sussurro_alvos.is_empty(),
                    self.na_chamada,
                    self.entrada.sala.starts_with("grp_"),
                ),
                false,
            ),
            PainelFlutuante::Emoji => (
                ui::seletor::seletor(
                    ui::seletor::AbaSeletor::Emoji,
                    &self.busca_gif,
                    self.buscando_gif,
                    self.erro_gif.as_deref(),
                    &self.resultados_gif,
                    &self.previas_gif,
                    &self.busca_emoji,
                ),
                false,
            ),
            PainelFlutuante::Sons => (ui::soundboard::painel_soundboard(&self.soundboard), false),
            PainelFlutuante::ReacoesTela => (ui::reacao::seletor_reacoes(), false),
            PainelFlutuante::Sussurro => (
                ui::chat::menu_sussurro(&self.pessoas, &self.sussurro_alvos),
                false,
            ),
            PainelFlutuante::Gif => (
                ui::seletor::seletor(
                    ui::seletor::AbaSeletor::Gif,
                    &self.busca_gif,
                    self.buscando_gif,
                    self.erro_gif.as_deref(),
                    &self.resultados_gif,
                    &self.previas_gif,
                    &self.busca_emoji,
                ),
                false,
            ),
            PainelFlutuante::Atividades => (
                ui::palco::menu_atividades(self.entrada.sala.starts_with("grp_")),
                true,
            ),
            PainelFlutuante::MaisPalco => (ui::palco::menu_mais_palco(), true),
        }
    }

    /// Painel flutuante: desenhado por cima de tudo (nunca empurra o chat) com
    /// uma camada transparente atrás que fecha ao clicar fora.
    fn camada_painel<'a>(&'a self, p: PainelFlutuante, largura_coluna: f32) -> Element<'a, Msg> {
        let fora =
            iced::widget::mouse_area(container(space()).width(Length::Fill).height(Length::Fill))
                .on_press(Msg::FechaPainel);
        let (conteudo, no_topo) = self.conteudo_do_painel(p);
        let conteudo = iced::widget::opaque(ui::efeitos::entrada("seletor", conteudo));
        let ancorado = matches!(
            p,
            PainelFlutuante::Atividades
                | PainelFlutuante::MaisPalco
                | PainelFlutuante::Sons
                | PainelFlutuante::ReacoesTela
                | PainelFlutuante::Sussurro
        );
        let _ = no_topo;
        let (ax, ay) = (self.painel_ancora.x, self.painel_ancora.y);
        let posicionado: Element<'a, Msg> = if ancorado {
            // Popup acima do botão clicado (como no Discord), sem sair da janela.
            iced::widget::responsive(move |tam| {
                container(iced::widget::opaque(ui::efeitos::entrada(
                    "seletor",
                    self.conteudo_do_painel(p).0,
                )))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Start)
                .align_y(Alignment::End)
                .padding(iced::Padding {
                    bottom: (tam.height - ay + 14.0).clamp(8.0, tam.height),
                    left: (ax - 190.0).clamp(8.0, (tam.width - 400.0).max(8.0)),
                    ..Default::default()
                })
                .into()
            })
            .into()
        } else if matches!(p, PainelFlutuante::Emoji | PainelFlutuante::Gif) {
            // Perto dos botões de GIF/emoji, no canto direito do campo de mensagem
            // (fora da coluna de membros, quando ela está aberta).
            container(conteudo)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::End)
                .align_y(Alignment::End)
                .padding(iced::Padding {
                    bottom: 68.0,
                    right: if self.painel_direito_aberto {
                        248.0 + 17.0
                    } else {
                        16.0
                    },
                    ..Default::default()
                })
                .into()
        } else {
            container(conteudo)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Start)
                .align_y(Alignment::End)
                .padding(iced::Padding {
                    bottom: 68.0,
                    left: largura_coluna + 16.0,
                    ..Default::default()
                })
                .into()
        };
        iced::widget::stack![fora, posicionado].into()
    }

    pub(crate) fn view<'a>(
        &'a self,
        coluna: Element<'a, Msg>,
        info: Option<&ui::conversa::InfoConversa<'a>>,
        avisos: &'a avisos::EstadoAvisos,
    ) -> Element<'a, Msg> {
        let principal = container(self.coluna_principal(info, avisos))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(tema::painel_principal);
        let corpo: Element<'_, Msg> = match (self.painel_direito_aberto, info) {
            (true, Some(i)) => row![
                principal,
                linha_vertical(),
                ui::painel::painel_direito(i, avisos)
            ]
            .into(),
            _ => principal.into(),
        };
        let direita = column![self.barra_superior(info), linha_horizontal(), corpo];
        let base: Element<'_, Msg> = row![coluna, linha_vertical(), direita].into();

        let largura_coluna = LARGURA_COLUNA;
        let mut camadas = vec![base];

        if !self.reacoes_flutuantes.is_empty() {
            camadas.push(ui::reacao::camada_reacoes(&self.reacoes_flutuantes));
        }

        if let Some(aviso) = &self.aviso_cutucada {
            camadas.push(ui::reacao::banner_cutucada(aviso));
        }

        if let Some(cfg) = &self.config_tela {
            camadas.push(ui::efeitos::janela(
                "tela",
                ui::tela::modal_compartilhar_tela(
                    cfg,
                    &self.titulo,
                    std::env::var_os("WAYLAND_DISPLAY").is_some(),
                ),
                0.65,
            ));
        }

        if let Some(p) = self.painel {
            camadas.push(self.camada_painel(p, largura_coluna));
        }

        if self.quadro.aberto {
            let modal_content = ui::quadro::modal_quadro(&self.quadro).map(Msg::Quadro);
            let backdrop = ui::efeitos::janela("quadro", modal_content, 0.65);
            camadas.push(backdrop);
        }

        if self.roletas.aberto {
            let modal_content = ui::roleta::modal_roletas(&self.roletas).map(Msg::Roleta);
            let backdrop = ui::efeitos::janela("roleta", modal_content, 0.65);
            camadas.push(backdrop);
        }

        if self.criador_enquete.aberto {
            let modal_content =
                ui::enquete::modal_criador_enquete(&self.criador_enquete).map(Msg::Enquete);
            let backdrop = ui::efeitos::janela("enquete", modal_content, 0.65);
            camadas.push(backdrop);
        }

        iced::widget::stack(camadas).into()
    }

    /// A área do jogo: três quintos da altura (ou tudo, se expandido).
    fn area_do_wordle<'a>(
        &'a self,
        w: &'a ui::wordle::EstadoUiWordle,
        laterais: Vec<(&'a str, &'a str, Option<&'a str>)>,
    ) -> Element<'a, Msg> {
        let altura = if w.modo == ui::wordle::ModoWordle::Expandido {
            Length::Fill
        } else {
            Length::FillPortion(3)
        };
        container(ui::wordle::tela_jogo(w, laterais))
            .width(Length::Fill)
            .height(altura)
            .into()
    }

    /// O palco da chamada, quando há (ou vai haver) gente nela.
    fn palco<'a>(
        &'a self,
        info: Option<&ui::conversa::InfoConversa<'a>>,
        avisos: &'a avisos::EstadoAvisos,
    ) -> Option<Element<'a, Msg>> {
        let i = info?;
        let conv = i.conversa;
        // Wordle aberto (e não minimizado): ocupa o lugar do palco.
        if let Some(w) = &self.wordle
            && w.modo != ui::wordle::ModoWordle::Minimizado
            && !self.na_chamada
        {
            return Some(self.area_do_wordle(w, Vec::new()));
        }
        let dentro = avisos.chamadas.get(&conv.id).cloned().unwrap_or_default();
        // Quem está fora vê só quem está dentro; os que estão tocando só
        // aparecem (apagados) para quem já está na chamada.
        let chamando: Vec<String> = if self.na_chamada {
            avisos
                .chamando
                .get(&conv.id)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|u| !dentro.contains(u))
                .collect()
        } else {
            Vec::new()
        };
        if dentro.is_empty() && !self.na_chamada {
            return None;
        }
        let mut pessoas: Vec<ui::palco::PessoaNoPalco<'a>> = Vec::new();
        for (uid, chamando) in dentro
            .iter()
            .map(|u| (u, false))
            .chain(chamando.iter().map(|u| (u, true)))
        {
            let Some(m) = conv.membros.iter().find(|m| &m.uid == uid) else {
                continue;
            };
            let sou_eu = m.usuario.is_some() && m.usuario.as_deref() == i.meu_usuario;
            let conexao = self
                .pessoas
                .iter()
                .find(|p| !p.espectador && p.uid.as_deref() == Some(m.uid.as_str()));
            pessoas.push(ui::palco::PessoaNoPalco {
                uid: &m.uid,
                nome: &m.name,
                usuario: m.usuario.as_deref(),
                avatar: m.avatar.as_deref(),
                falando: !chamando && conexao.is_some_and(|p| p.falando),
                mudo: if sou_eu {
                    self.mudo
                } else {
                    conexao.is_some_and(|p| p.mudo || p.surdo)
                },
                chamando,
            });
        }
        // Wordle durante a chamada: modo apresentação, com as pessoas ao lado.
        if let Some(w) = &self.wordle
            && w.modo != ui::wordle::ModoWordle::Minimizado
            && self.na_chamada
        {
            let laterais = pessoas.iter().map(|p| (p.uid, p.nome, p.avatar)).collect();
            return Some(self.area_do_wordle(w, laterais));
        }
        let est = ui::video::EstadoTransmissao {
            tela_cheia: None,
            assistindo: &self.assistindo,
            ocultar_propria: self.previa_propria_oculta,
            espectadores: self.espectadores_da_tela.len(),
            controles_visiveis: true,
        };
        let em_foco = self
            .video_fixado
            .as_ref()
            .and_then(|(d, t)| self.videos.iter().find(|v| v.dono == *d && v.de_tela == *t));
        let foco =
            em_foco.map(|v| ui::video::renderiza_caixa_video(v, &self.pessoas, &self.meu_id, &est));
        let transmissoes: Vec<Element<'a, Msg>> = self
            .videos
            .iter()
            .map(|v| ui::video::renderiza_miniatura(v, &self.pessoas, &self.meu_id, &est))
            .collect();
        let pulso = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() % 2 == 0)
            .unwrap_or(false);
        Some(ui::palco::palco(
            &pessoas,
            self.na_chamada,
            &ui::palco::BarraDoPalco {
                mudo: self.mudo,
                mudo_forcado: self.mudo_forcado,
                camera: self.com_camera,
                apresentando: self.apresentando,
                menu_dispositivos_aberto: self.popup_voz_aberto,
                menu_atividades_aberto: self.painel == Some(PainelFlutuante::Atividades),
                menu_mais_aberto: self.painel == Some(PainelFlutuante::MaisPalco),
                sons_abertos: self.painel == Some(PainelFlutuante::Sons),
            },
            pulso,
            transmissoes,
            foco,
        ))
    }

    /// Barra de cima: nome da conversa, ligar, fixadas, adicionar pessoas,
    /// membros e busca. Fica acima de chat e membros (como no Discord).
    fn barra_superior<'a>(
        &'a self,
        info: Option<&ui::conversa::InfoConversa<'a>>,
    ) -> Element<'a, Msg> {
        let topo = ui::conversa::cabecalho(
            &self.titulo,
            info,
            &ui::conversa::EstadoCabecalho {
                painel_aberto: self.painel_direito_aberto,
                busca: &self.busca_chat,
                fixadas_ativas: self.fixadas_ativas,
                tem_fixada: self.mensagem_fixada.is_some(),
                na_chamada: self.na_chamada,
            },
        );
        container(topo)
            .padding(if self.densidade == ui::layout::Densidade::Compacta {
                [4, 10]
            } else {
                [8, 16]
            })
            .width(Length::Fill)
            .into()
    }

    pub(crate) fn coluna_principal<'a>(
        &'a self,
        info: Option<&ui::conversa::InfoConversa<'a>>,
        avisos: &'a avisos::EstadoAvisos,
    ) -> Element<'a, Msg> {
        if let Some((dono_tc, tela_tc)) = &self.video_em_tela_cheia
            && self
                .videos
                .iter()
                .any(|v| v.dono == *dono_tc && v.de_tela == *tela_tc)
        {
            return ui::video::renderiza_videos(
                &self.videos,
                &self.pessoas,
                &self.meu_id,
                self.modo_video,
                self.video_fixado.as_ref(),
                self.ultimo_falante.as_deref(),
                &ui::video::EstadoTransmissao {
                    tela_cheia: self.video_em_tela_cheia.as_ref(),
                    assistindo: &self.assistindo,
                    ocultar_propria: self.previa_propria_oculta,
                    espectadores: self.espectadores_da_tela.len(),
                    controles_visiveis: self.ultimo_movimento.elapsed()
                        < std::time::Duration::from_secs(2),
                },
            );
        }

        let foco_ativo = self
            .video_fixado
            .as_ref()
            .is_some_and(|(d, t)| self.videos.iter().any(|v| v.dono == *d && v.de_tela == *t));
        let mut col_midia = Column::new();
        if foco_ativo {
            // Transmissão em foco: a área de mídia ganha ~60% da altura.
            col_midia = col_midia.height(Length::FillPortion(3));
        }
        let mut tem_midia = false;

        let tem_palco = if let Some(p) = self.palco(info, avisos) {
            col_midia = col_midia.push(p);
            tem_midia = true;
            true
        } else {
            false
        };

        if let Some(a) = &self.aviso {
            col_midia = col_midia
                .push(container(aviso(a.clone(), Some(Msg::FechaAviso))).padding([10, 16]));
            tem_midia = true;
        }
        if self.apresentando {
            // Sem isto, a única pista de que a tela está saindo era o ícone
            // aceso no canto — fácil de esquecer apresentando.
            let faixa = row![
                svg(icones::tela())
                    .width(16)
                    .height(16)
                    .style(tema::icone_cor(tema::cores().destaque)),
                text("Você está apresentando a tela.")
                    .size(13)
                    .width(Length::Fill),
                button(text("Parar").size(13))
                    .padding([5, 12])
                    .style(tema::botao_secundario)
                    .on_press(Msg::ParaTela),
            ]
            .spacing(8)
            .align_y(Alignment::Center);
            col_midia = col_midia.push(
                container(faixa)
                    .padding([8, 16])
                    .width(Length::Fill)
                    .style(tema::painel_lateral),
            );
            col_midia = col_midia.push(linha_horizontal());
            tem_midia = true;
        }
        if let Some(prev) = &self.previa_camera {
            col_midia =
                col_midia.push(container(ui::camera::painel_previa_camera(prev)).padding([10, 16]));
            col_midia = col_midia.push(linha_horizontal());
            tem_midia = true;
        }
        if let Some(pa) = &self.painel_audio {
            col_midia =
                col_midia.push(container(ui::audio::painel_config_audio(pa)).padding([10, 16]));
            col_midia = col_midia.push(linha_horizontal());
            tem_midia = true;
        }
        if self.falando_no_mudo {
            col_midia = col_midia.push(
                container(ui::voz::aviso_falando_no_mudo(self.mudo_forcado)).padding([6, 16]),
            );
            col_midia = col_midia.push(linha_horizontal());
            tem_midia = true;
        }
        if self.aviso_eco {
            col_midia = col_midia.push(container(ui::voz::aviso_eco()).padding([6, 16]));
            col_midia = col_midia.push(linha_horizontal());
            tem_midia = true;
        }
        if !self.videos.is_empty() && !tem_palco {
            col_midia = col_midia.push(ui::video::renderiza_videos(
                &self.videos,
                &self.pessoas,
                &self.meu_id,
                self.modo_video,
                self.video_fixado.as_ref(),
                self.ultimo_falante.as_deref(),
                &ui::video::EstadoTransmissao {
                    tela_cheia: None,
                    assistindo: &self.assistindo,
                    ocultar_propria: self.previa_propria_oculta,
                    espectadores: self.espectadores_da_tela.len(),
                    controles_visiveis: true,
                },
            ));
            col_midia = col_midia.push(linha_horizontal());
            tem_midia = true;
        }

        let mut col_chat = Column::new().height(if foco_ativo {
            Length::FillPortion(2)
        } else {
            Length::Fill
        });

        if self.fixadas_ativas
            && let Some(fixada) = &self.mensagem_fixada
        {
            col_chat = col_chat.push(ui::chat::barra_fixada(fixada));
        }

        let mensagens_visiveis = ui::chat::filtra_mensagens(&self.mensagens, &self.busca_chat);
        if self.busca_chat_aberta {
            col_chat = col_chat.push(ui::chat::barra_busca_chat(
                &self.busca_chat,
                mensagens_visiveis.len(),
                self.mensagens.len(),
            ));
        }

        let mut mensagens = Column::new()
            .spacing(self.densidade.espacamento_mensagens())
            .padding(if self.densidade == ui::layout::Densidade::Compacta {
                [4, 8]
            } else {
                [12, 16]
            });
        if mensagens_visiveis.is_empty() {
            let aviso_vazio = if !self.busca_chat.trim().is_empty() {
                "Nenhuma mensagem encontrada para essa busca."
            } else {
                "Nenhuma mensagem ainda. O que for escrito aqui fica salvo na sala."
            };
            mensagens = mensagens.push(text(aviso_vazio).size(13).style(fraco));
        }
        let mut anterior: Option<(&str, i64)> = None;
        for m in &mensagens_visiveis {
            if anterior.is_none_or(|(_, ts)| !ui::conversa::mesmo_dia(ts, m.ts)) {
                mensagens = mensagens.push(ui::conversa::separador_de_dia(m.ts));
                anterior = None;
            }
            if Some(&m.id) == self.primeira_nao_lida_id.as_ref() {
                mensagens = mensagens.push(ui::chat::divisor_nao_lidas());
            }
            if let Some(tipo) = &m.sistema {
                // "X não atendeu" não aparece mais (só o "Você perdeu…" de quem perdeu).
                if tipo == "perdida" {
                    continue;
                }
                mensagens = mensagens.push(ui::efeitos::linha_com_hover(
                    &format!("msg-{}", m.id),
                    ui::conversa::linha_de_sistema(
                        tipo,
                        &m.texto,
                        m.ts,
                        &m.extra,
                        self.meu_uid.as_deref(),
                    ),
                ));
                // Mensagem de sistema quebra a sequência de uma mesma pessoa.
                anterior = Some(("", m.ts));
                continue;
            }
            // Mensagens seguidas da mesma pessoa em até 5 minutos se juntam
            // sob um cabeçalho só, como no Discord.
            let junta = anterior.is_some_and(|(a, ts)| a == m.autor && m.ts - ts < 5 * 60 * 1000);
            let foto = m
                .uid
                .as_deref()
                .and_then(|u| {
                    info.and_then(|i| i.conversa.membros.iter().find(|x| x.uid == u))
                        .and_then(|x| x.avatar.as_deref())
                })
                .or_else(|| {
                    self.pessoas
                        .iter()
                        .find(|p| p.uid.is_some() && p.uid == m.uid)
                        .and_then(|p| p.avatar.as_deref())
                });
            mensagens = mensagens.push(self.linha_de_mensagem(m, !junta, foto));
            anterior = Some((&m.autor, m.ts));
        }

        let mut area_envio = Column::new().spacing(4);
        if self.nao_lidas > 0 {
            area_envio = area_envio.push(ui::chat::pilula_nao_lidas(self.nao_lidas));
        }
        if !self.sussurro_alvos.is_empty() {
            let presentes = self
                .pessoas
                .iter()
                .filter(|p| !p.eu && self.sussurro_alvos.iter().any(|a| a.mesma_pessoa(p)))
                .count();
            area_envio = area_envio.push(ui::chat::barra_sussurro(
                &self.sussurro_alvos,
                presentes,
                self.erro_sussurro.as_deref(),
            ));
        }
        if self.editando.is_some() {
            area_envio = area_envio.push(ui::chat::barra_edicao());
        }
        if let Some(resp) = &self.resposta_pendente {
            area_envio = area_envio.push(ui::chat::barra_resposta(resp));
        }
        if let Some(anexo) = &self.imagem_pendente {
            area_envio = area_envio.push(ui::chat::barra_previa_anexo(anexo));
        }
        let mut nomes_digitando: Vec<&str> = self
            .digitando_remotos
            .values()
            .map(|(n, _)| n.as_str())
            .collect();
        nomes_digitando.sort_unstable();
        nomes_digitando.dedup();

        if let Some(txt) = ui::chat::texto_digitando(&nomes_digitando) {
            area_envio =
                area_envio.push(container(ui::chat::barra_digitando(txt)).padding([0, 16]));
        }

        if let Some(filtro) = ui::chat::detecta_comando_rascunho(&self.rascunho) {
            // Só sugere: quem decide é o servidor (login "val47").
            // `entrada.nome` é o nome de exibição ("Val47"); o login está na conversa.
            let eh_dev = info.and_then(|i| i.meu_usuario) == Some("val47");
            let eh_grupo = self.entrada.sala.starts_with("grp_");
            let sugestoes = ui::chat::filtra_comandos_chat(filtro, eh_dev, eh_grupo);
            if !sugestoes.is_empty() {
                area_envio = area_envio.push(ui::chat::menu_comandos_chat(&sugestoes));
            }
        }

        if let Some(prefixo) = ui::chat::detecta_mencao_rascunho(&self.rascunho) {
            let candidatos = info.map_or_else(Vec::new, |i| {
                ui::chat::filtra_candidatos_mencao(prefixo, &i.conversa.membros, i.meu_usuario)
            });
            if !candidatos.is_empty() {
                area_envio = area_envio.push(ui::chat::menu_autocompletar_mencao(&candidatos));
            }
        }

        let wordle_ativo = self.wordle.as_ref().is_some_and(|w| {
            w.modo != ui::wordle::ModoWordle::Minimizado
                && w.estado.as_ref().and_then(|e| e.fim.as_ref()).is_none()
        });

        let placeholder = if wordle_ativo {
            "Wordle em andamento (digite seu palpite diretamente no teclado)...".to_string()
        } else if self.editando.is_some() {
            "Editar mensagem...".to_string()
        } else if let Some(r) = &self.resposta_pendente {
            format!("Respondendo a {}...", r.autor)
        } else if !self.sussurro_alvos.is_empty() {
            format!(
                "Sussurrando para {}...",
                self.sussurro_alvos
                    .iter()
                    .map(|a| a.nome.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        } else {
            format!("Conversar em {}", self.titulo)
        };

        let mut campo = text_input(&placeholder, &self.rascunho)
            .id("chat-rascunho-input")
            .padding([10, 12])
            .size(14)
            .style(tema::campo);

        if !wordle_ativo {
            campo = campo.on_input(Msg::Rascunho).on_submit(Msg::EnviaChat);
        }

        let dx = anim::balanco("botao-gif") * 16.0;
        let botao_gif = button(text("GIF").size(12).font(negrito()))
            .style(if self.painel == Some(PainelFlutuante::Gif) {
                tema::botao_primario
            } else {
                tema::botao_secundario
            })
            .padding(iced::Padding {
                top: 6.0,
                bottom: 6.0,
                left: (8.0 + dx).max(0.0),
                right: (8.0 - dx).max(0.0),
            })
            .on_press(Msg::AlternaPainelGif);
        let botao_gif = ui::efeitos::com_hover("botao-gif", botao_gif);

        // Com o popup aberto, a dica sairia em cima dele (como na barra da chamada).
        let mais_aberto = self.painel == Some(PainelFlutuante::Mais);
        let botao_mais = botao_icone(
            icones::mais(),
            mais_aberto,
            false,
            Some(Msg::AlternaMenuMais),
        );
        let botao_mais = if mais_aberto {
            botao_mais
        } else {
            crate::ui::efeitos::com_dica(
                botao_mais,
                "Anexar e mais",
                iced::widget::tooltip::Position::Top,
            )
        };
        let emoji_aberto = self.painel == Some(PainelFlutuante::Emoji);
        let botao_emoji = ui::efeitos::botao_icone_anim(
            "botao-emoji",
            icones::smile(),
            emoji_aberto,
            false,
            Some(Msg::AlternaMenuEmoji),
            ui::efeitos::Efeito::EmojiAleatorio,
        );
        let botao_emoji = if emoji_aberto {
            botao_emoji
        } else {
            crate::ui::efeitos::com_dica(botao_emoji, "Emoji", iced::widget::tooltip::Position::Top)
        };
        let mut envio = row![botao_mais, campo]
            .spacing(6)
            .align_y(Alignment::Center);
        if self.gif_ativo && self.sussurro_alvos.is_empty() {
            envio = envio.push(botao_gif);
        }
        let envio = envio
            .push(botao_emoji)
            .push(crate::ui::efeitos::com_dica(
                botao_icone(
                    icones::enviar(),
                    false,
                    false,
                    (!self.rascunho.trim().is_empty() || self.imagem_pendente.is_some())
                        .then_some(Msg::EnviaChat),
                ),
                "Enviar mensagem",
                iced::widget::tooltip::Position::Top,
            ))
            .padding([10, 16]);

        area_envio = area_envio.push(envio);

        col_chat = col_chat
            .push(
                scrollable(mensagens)
                    .anchor_bottom()
                    .height(Length::Fill)
                    .style(tema::rolagem),
            )
            .push(area_envio);

        if self
            .wordle
            .as_ref()
            .is_some_and(|w| w.modo == ui::wordle::ModoWordle::Expandido)
        {
            return col_midia.into();
        }
        if tem_midia {
            column![col_midia, col_chat].into()
        } else {
            col_chat.into()
        }
    }

    pub(crate) fn linha_de_mensagem<'a>(
        &'a self,
        m: &'a Mensagem,
        cabecalho: bool,
        foto: Option<&'a str>,
    ) -> Element<'a, Msg> {
        if m.extra.app.as_deref() == Some("wordle") {
            return ui::efeitos::linha_com_hover(
                &format!("msg-{}", m.id),
                container(ui::wordle::mensagem_app(
                    m,
                    m.extra
                        .wordle_dia
                        .as_deref()
                        .and_then(|d| self.wordle_grupo.get(d)),
                ))
                .padding([0, 4])
                .width(Length::Fill)
                .into(),
            );
        }
        let eh_minha = ui::chat::eh_minha_mensagem(m, &self.entrada.nome, self.meu_uid.as_deref());
        let mut corpo = Column::new().spacing(4);
        if cabecalho {
            let mut linha_topo = row![];
            if m.sussurro {
                linha_topo = linha_topo.push(ui::chat::marca_sussurro(&m.sussurro_para, eh_minha));
            }
            linha_topo = linha_topo
                .push({
                    let id_nome = format!("nome-{}", m.id);
                    let t = anim::t(&id_nome);
                    let cores = tema::cores();
                    let cor = tema::mistura(cores.texto, cores.destaque, t);
                    ui::efeitos::com_hover(
                        &id_nome,
                        iced::widget::mouse_area(
                            text(&m.autor).size(14).font(negrito()).color(cor),
                        )
                        .interaction(iced::mouse::Interaction::Pointer)
                        .on_press(Msg::AbrePrevia {
                            uid: m.uid.clone(),
                            nome: m.autor.clone(),
                            app: false,
                        }),
                    )
                })
                .push(text(ui::conversa::quando(m.ts)).size(12).style(fraco))
                .spacing(8)
                .align_y(Alignment::Center);

            corpo = corpo.push(linha_topo);
        }
        if m.extra.encaminhada_de.is_some() {
            let c = tema::cores();
            corpo = corpo.push(
                row![
                    ui::efeitos::ico(icones::encaminhar(), 13.0, c.texto_fraco),
                    text("Encaminhada").size(12).style(fraco),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            );
        }
        if let Some(resp) = &m.resposta_a {
            corpo = corpo.push(ui::chat::citacao_resposta(resp));
        }
        if !m.texto.is_empty() {
            let trechos = ui::chat::separa_blocos_codigo(&m.texto);
            for (idx, trecho) in trechos.into_iter().enumerate() {
                match trecho {
                    ui::chat::TrechoMensagem::BlocoCodigo { linguagem, codigo } => {
                        corpo = corpo.push(ui::chat::renderiza_bloco_codigo(linguagem, codigo));
                    }
                    ui::chat::TrechoMensagem::Texto(t) => {
                        let chave = format!("{}#{idx}", m.id);
                        if t.is_empty() {
                        } else if let Some(sel) = self.selecionaveis.get(&chave) {
                            let tam = if ui::chat::so_emoji(t) {
                                44.0
                            } else {
                                tema::TEXTO_CHAT
                            };
                            let texto = ui::selecionavel::vista(&chave, sel, tam);
                            corpo = corpo.push(if m.editada_em.is_some() && idx == 0 {
                                row![texto, ui::chat::tag_editada()]
                                    .align_y(Alignment::End)
                                    .into()
                            } else {
                                texto
                            });
                        } else {
                            corpo = corpo.push(ui::chat::renderiza_texto_com_links(
                                t,
                                m.editada_em.is_some() && idx == 0,
                            ));
                        }
                    }
                }
            }
        } else if m.editada_em.is_some() {
            corpo = corpo.push(ui::chat::tag_editada());
        }
        if let Some(a) = &m.extra.arquivo {
            corpo = corpo.push(card_de_arquivo(a));
        }
        if let Some(h) = self.imagens.get(&m.id) {
            // GIF: o quadro do momento (ou o primeiro, parado, no modo "só com
            // o mouse em cima"); imagem comum: ela mesma.
            let chave_gif = crate::gifanim::chave_mensagem(&m.id);
            let quadro = crate::gifanim::quadro_atual(&chave_gif)
                .or_else(|| crate::gifanim::quadro_parado(&chave_gif))
                .unwrap_or_else(|| h.clone());
            let img_elem = image(quadro)
                .width(Length::Shrink)
                .height(Length::Shrink)
                .content_fit(iced::ContentFit::ScaleDown);
            let img_elem = iced::widget::mouse_area(img_elem)
                .on_enter(Msg::HoverGif(Some(chave_gif)))
                .on_exit(Msg::HoverGif(None));
            // Clique abre a prévia grande no próprio app (GIF e imagem).
            let titulo = if m.gif.is_some() { "GIF" } else { "Imagem" };
            corpo = corpo.push(
                img_elem
                    .interaction(iced::mouse::Interaction::Pointer)
                    .on_press(Msg::AbreImagemMensagem {
                        id: m.id.clone(),
                        titulo: format!("{titulo} de {}", m.autor),
                    })
                    .on_right_press(Msg::AbreMenuImagem(m.id.clone())),
            );
        } else if let Some(g) = &m.gif {
            corpo = corpo.push(ui::chat::gif_carregando(g));
        }
        if let Some(enq) = &m.enquete {
            let minha_chave =
                ui::chat::minha_chave_voto(&self.entrada.nome, self.meu_uid.as_deref());
            corpo = corpo.push(ui::chat::renderiza_enquete(
                &m.id,
                enq,
                &m.votos,
                &minha_chave,
            ));
        }
        if let Some(reacoes) =
            ui::chat::barra_reacoes_mensagem(&m.id, &m.reacoes, &self.entrada.nome)
        {
            corpo = corpo.push(reacoes);
        }
        if self.seletor_reacao.as_deref() == Some(&m.id) {
            corpo = corpo.push(ui::chat::seletor_rapido_emoji(&m.id));
        }
        let me_mencionou = ui::chat::citou_voce(
            m,
            &self.entrada.nome,
            self.entrada.usuario.as_deref(),
            self.meu_uid.as_deref(),
        );
        let mut cont = container(corpo)
            .padding(iced::Padding {
                top: if cabecalho { 10.0 } else { 0.0 },
                bottom: if me_mencionou || m.sussurro { 4.0 } else { 0.0 },
                left: if me_mencionou || m.sussurro { 8.0 } else { 0.0 },
                right: if me_mencionou || m.sussurro { 8.0 } else { 0.0 },
            })
            .width(Length::Fill);

        if m.sussurro {
            cont = cont.style(tema::destaque_sussurro);
        } else if me_mencionou {
            cont = cont.style(tema::destaque_mencao);
        }
        // Coluna do avatar (40px): a foto no primeiro da sequência, vazio nos
        // seguintes, para o texto alinhar.
        let lateral: Element<'a, Msg> = if cabecalho {
            container(
                iced::widget::mouse_area(ui::avatar::avatar(
                    foto,
                    m.uid.as_deref().unwrap_or(&m.autor),
                    &m.autor,
                    40.0,
                    None,
                    tema::cores().fundo,
                ))
                .interaction(iced::mouse::Interaction::Pointer)
                .on_press(Msg::AbrePrevia {
                    uid: m.uid.clone(),
                    nome: m.autor.clone(),
                    app: false,
                }),
            )
            .padding(iced::Padding {
                top: 10.0,
                ..Default::default()
            })
            .into()
        } else {
            space().width(40).into()
        };
        let id_hover = format!("msg-{}", m.id);
        let c = tema::cores();
        let t = anim::t(&id_hover);
        let linha =
            container(row![lateral, cont, space().height(28)].spacing(12)).style(move |_| {
                container::Style {
                    background: (t > 0.01).then_some(iced::Background::Color(iced::Color {
                        a: 0.05 * t,
                        ..c.texto
                    })),
                    ..Default::default()
                }
            });
        let conteudo: Element<'a, Msg> = if anim::dentro(&id_hover) {
            iced::widget::stack![
                linha,
                container(ui::mensagem::barra_hover(&m.id, &m.autor, &m.texto))
                    .width(Length::Fill)
                    .align_x(Alignment::End)
                    .padding(iced::Padding {
                        right: 8.0,
                        ..Default::default()
                    })
            ]
            .into()
        } else {
            linha.into()
        };
        iced::widget::mouse_area(conteudo)
            .on_enter(Msg::Hover(id_hover.clone(), true))
            .on_exit(Msg::Hover(id_hover, false))
            .on_right_press(Msg::AbreMenuMensagem(m.id.clone()))
            .into()
    }
}

/// Cartão de um arquivo anexado: ícone, nome, tamanho e botão de baixar.
fn card_de_arquivo<'a>(a: &'a sala::ArquivoMsg) -> Element<'a, Msg> {
    let c = tema::cores();
    let baixar = button(ui::efeitos::ico(icones::baixar(), 18.0, c.texto_medio))
        .padding(8)
        .style(tema::botao_icone(false, false))
        .on_press(Msg::BaixaArquivo {
            id: a.id.clone(),
            nome: a.nome.clone(),
        });
    container(
        row![
            ui::efeitos::ico(icones::arquivo(), 30.0, c.destaque),
            column![
                text(&a.nome).size(14).font(negrito()),
                text(ui::chat::tamanho_legivel(a.tam)).size(12).style(fraco),
            ]
            .spacing(2)
            .width(Length::Fill),
            ui::efeitos::com_dica(baixar, "Baixar", iced::widget::tooltip::Position::Top),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .padding(12)
    .max_width(420)
    .style(move |_| container::Style {
        background: Some(iced::Background::Color(c.lateral)),
        border: iced::Border {
            color: c.borda,
            width: 1.0,
            radius: tema::RAIO_GRANDE.into(),
        },
        ..Default::default()
    })
    .into()
}
