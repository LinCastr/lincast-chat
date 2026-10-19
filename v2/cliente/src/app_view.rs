//! `App`: assinaturas e desenho das telas.

use super::*;

fn sub_sala(entrada: sala::Entrada) -> Subscription<Msg> {
    let id = entrada.sala.clone();
    Subscription::run_with(entrada, |e| {
        let e = e.clone();
        iced::stream::channel(256, async move |saida| sala::roda(e, saida).await)
    })
    .with(id)
    .map(|(id, ev)| Msg::SalaDe(id, ev))
}

impl App {
    pub(crate) fn subscription(&self) -> Subscription<Msg> {
        let sub_atividade =
            iced::time::every(std::time::Duration::from_secs(10)).map(|_| Msg::VarreAtividade);

        let sub_tela = match &self.tela {
            Tela::Salas(s) => {
                // A lista de conversas chega pelo canal de avisos; só os
                // amigos ainda são buscados de tempos em tempos.
                if s.aba == AbaSalas::Amigos {
                    iced::time::every(std::time::Duration::from_secs(6)).map(|_| Msg::CarregaAmigos)
                } else {
                    Subscription::none()
                }
            }
            Tela::Sala(s) => {
                let sub_sala = sub_sala(s.entrada.clone());

                let sub_tick = iced::time::every(std::time::Duration::from_millis(500))
                    .map(|_| Msg::TickDigitando);

                let mut subs = vec![sub_sala, sub_tick];
                if s.roletas.animacao.is_some() {
                    subs.push(
                        iced::time::every(std::time::Duration::from_millis(16))
                            .map(|_| Msg::Roleta(ui::roleta::RoletaMsg::TickAnimacao)),
                    );
                }
                Subscription::batch(subs)
            }
            _ => Subscription::none(),
        };

        let sub_anim = if anim::ativo() {
            iced::time::every(std::time::Duration::from_millis(16)).map(|_| Msg::AnimTick)
        } else {
            Subscription::none()
        };

        let sub_presenca = if self.presenca_fecha_em.is_some() {
            iced::time::every(std::time::Duration::from_millis(80)).map(|_| Msg::PresencaTick)
        } else {
            Subscription::none()
        };

        let sub_gif = if crate::gifanim::animando() {
            iced::time::every(std::time::Duration::from_millis(50)).map(|_| Msg::TickGif)
        } else {
            Subscription::none()
        };

        let sub_toast = if self.toast.is_some() {
            iced::time::every(std::time::Duration::from_millis(400)).map(|_| Msg::ToastTick)
        } else {
            Subscription::none()
        };

        let sub_bandeja = Subscription::run_with((), |_| {
            iced::stream::channel(
                16,
                |mut output: iced::futures::channel::mpsc::Sender<Msg>| async move {
                    use futures_util::sink::SinkExt;
                    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
                    let cfg = ui::bandeja::carrega_config();
                    let bandeja = ui::bandeja::BandejaApp::novo(tx, cfg.minimizar_para_bandeja);
                    match ksni::TrayMethods::spawn(bandeja).await {
                        Ok(handle) => {
                            let _ = output
                                .send(Msg::BandejaIniciada(ui::bandeja::HandleBandeja(handle)))
                                .await;
                            while let Some(acao) = rx.recv().await {
                                let _ = output.send(Msg::AcaoBandeja(acao)).await;
                            }
                        }
                        Err(e) => {
                            log::warn!("[bandeja] StatusNotifierItem não disponível: {e}");
                        }
                    }
                },
            )
        });

        let sub_atualizacao = iced::time::every(std::time::Duration::from_secs(
            ui::atualizacao::INTERVALO_RECHECAGEM_SEG,
        ))
        .map(|_| Msg::ChecaAtualizacao);

        let mut subs = vec![
            sub_atualizacao,
            sub_tela,
            sub_atividade,
            iced::event::listen().map(Msg::EventoIced),
            sub_bandeja,
            sub_anim,
            sub_gif,
            sub_presenca,
            sub_toast,
        ];
        if let Some(c) = &self.chamada_ativa {
            subs.push(sub_sala(c.entrada.clone()));
        }
        // Toque tocando: a barra de 30s e o pulso precisam de redesenho.
        if !self.avisos.toques.is_empty() {
            subs.push(
                iced::time::every(std::time::Duration::from_millis(250)).map(|_| Msg::TickChamada),
            );
        }
        // Em tela cheia, redesenha para os controles sumirem depois de 2s parado.
        if matches!(&self.tela, Tela::Sala(s) if s.video_em_tela_cheia.is_some()) {
            subs.push(
                iced::time::every(std::time::Duration::from_millis(500)).map(|_| Msg::TickChamada),
            );
        }
        // Atalhos globais (portal GlobalShortcuts): de pé com a sessão, em
        // qualquer tela — "atender" precisa funcionar fora de uma sala.
        if self.eu.is_some() {
            let gatilhos: Vec<(String, String, Option<String>)> = atalhos::ACOES
                .iter()
                .map(|(id, rotulo, _)| {
                    (
                        (*id).to_string(),
                        format!("{rotulo} — Lincast Chat"),
                        atalhos::gatilho_para_portal(&self.atalhos.combo(id)),
                    )
                })
                .collect();
            subs.push(Subscription::run_with(gatilhos, |gatilhos| {
                let gatilhos = gatilhos.clone();
                iced::stream::channel(
                    32,
                    async move |mut output: iced::futures::channel::mpsc::Sender<Msg>| {
                        use futures_util::sink::SinkExt;
                        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
                        match ptt::inicia_portal_global(tx, gatilhos).await {
                            Ok(()) => {
                                let _ = output.send(Msg::PortalGlobalResultado(Ok(()))).await;
                                while let Some((acao, pressionado)) = rx.recv().await {
                                    let _ =
                                        output.send(Msg::PortalAtalho { acao, pressionado }).await;
                                }
                            }
                            Err(e) => {
                                let _ = output.send(Msg::PortalGlobalResultado(Err(e))).await;
                            }
                        }
                    },
                )
            }));
        }
        // Cronômetro da coluna: redesenha uma vez por segundo durante a chamada.
        if self.em_chamada() {
            subs.push(
                iced::time::every(std::time::Duration::from_secs(1)).map(|_| Msg::TickChamada),
            );
        }
        // Canal de avisos: de pé enquanto houver sessão, em qualquer tela.
        if let (Some(_), Some(token)) = (&self.eu, &self.token) {
            let entrada = avisos::Entrada {
                servidor: self.srv.clone(),
                token: token.clone(),
            };
            subs.push(
                Subscription::run_with(entrada, |e| {
                    let e = e.clone();
                    iced::stream::channel(256, async move |saida| avisos::roda(e, saida).await)
                })
                .map(Msg::Avisos),
            );
        }
        if self.painel_diag_aberto {
            subs.push(
                iced::time::every(std::time::Duration::from_secs(2))
                    .map(|_| Msg::AtualizaMetricasServidor),
            );
        }
        Subscription::batch(subs)
    }

    /// Popup da seta do microfone: acima do botão clicado (como os outros da
    /// barra da chamada), com a lista de dispositivos ao lado quando aberta.
    fn com_popup_voz<'a>(&'a self, base: Element<'a, Msg>) -> Element<'a, Msg> {
        let (Some(p), Tela::Sala(s)) = (&self.popup_voz, &self.tela) else {
            return base;
        };
        let pa = &self.painel_audio;
        let dados = ui::popup_voz::DadosPopupVoz {
            entradas: &pa.entradas,
            saidas: &pa.saidas,
            entrada_sel: pa.entrada_selecionada.as_deref(),
            saida_sel: pa.saida_selecionada.as_deref(),
            volume_mic: self.volume_mic,
            volume_saida: self.volume_saida,
            nivel: pa.nivel_mic,
            surdo: s.surdo,
            sub: p.sub,
        };
        let mut conteudo = row![ui::popup_voz::principal(&dados)].spacing(6);
        if let Some(sub) = p.sub {
            conteudo = conteudo.push(iced::widget::column![
                space().height(ui::popup_voz::deslocamento_da_lista(sub)),
                ui::popup_voz::lista(&dados, sub),
            ]);
        }
        let largura = if p.sub.is_some() {
            2.0 * ui::popup_voz::LARGURA + 6.0
        } else {
            ui::popup_voz::LARGURA
        };
        let janela = self.janela;
        let esq = (p.x - ui::popup_voz::LARGURA / 2.0)
            .min(janela.width - largura - 8.0)
            .max(8.0);
        // Acima do botão quando cabe; a barra fica no topo da conversa, então
        // quase sempre abre embaixo dele (como no Discord).
        let cabe_acima = p.y - 14.0 >= ui::popup_voz::ALTURA + 8.0;
        let padding = if cabe_acima {
            iced::Padding {
                left: esq,
                bottom: (janela.height - p.y + 14.0).max(8.0),
                ..Default::default()
            }
        } else {
            iced::Padding {
                left: esq,
                top: (p.y + 24.0)
                    .min(janela.height - ui::popup_voz::ALTURA - 8.0)
                    .max(8.0),
                ..Default::default()
            }
        };
        let fundo =
            iced::widget::mouse_area(container(space()).width(Length::Fill).height(Length::Fill))
                .on_press(Msg::FechaPopupVoz)
                .on_right_press(Msg::FechaPopupVoz);
        let posicionado = container(iced::widget::opaque(ui::efeitos::entrada(
            "popup-voz",
            conteudo,
        )))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(if cabe_acima {
            Alignment::End
        } else {
            Alignment::Start
        })
        .padding(padding);
        iced::widget::stack([base, fundo.into(), posicionado.into()]).into()
    }

    /// Sobrepõe o menu de contexto aberto (se houver) à tela.
    fn com_menu_de_contexto<'a>(&'a self, base: Element<'a, Msg>) -> Element<'a, Msg> {
        use ui::menu::{coluna, item, posiciona, separador, sobre, titulo};
        let Some(m) = &self.menu_contexto else {
            return base;
        };
        let meu_usuario = self.eu.as_ref().and_then(|e| e.usuario.as_deref());
        match &m.alvo {
            AlvoMenu::Conversa(id) => {
                let Some(c) = self.avisos.conversas.iter().find(|c| &c.id == id) else {
                    return base;
                };
                let silenciada = self.prefs_conversas.silenciadas.contains(id);
                let mut v: Vec<Element<'a, Msg>> =
                    vec![titulo(&nome_da_conversa(c, meu_usuario)), separador()];
                if c.tipo == "grupo" {
                    v.push(item(
                        icones::lapis(),
                        "Editar nome e ícone",
                        Msg::AbreEditarGrupo(id.clone()),
                        false,
                    ));
                }
                v.push(item(
                    if silenciada {
                        icones::volume()
                    } else {
                        icones::volume_mudo()
                    },
                    if silenciada {
                        "Reativar conversa"
                    } else {
                        "Silenciar conversa"
                    },
                    Msg::AlternaSilenciarConversa(id.clone()),
                    false,
                ));
                if c.tipo == "grupo" && c.dono.as_deref() == self.avisos.meu_uid() {
                    v.push(item(
                        icones::lixeira(),
                        "Apagar grupo",
                        Msg::PedeApagarGrupo(id.clone()),
                        true,
                    ));
                }
                v.push(item(
                    icones::sair(),
                    if c.tipo == "grupo" {
                        "Sair do grupo"
                    } else {
                        "Fechar conversa"
                    },
                    if c.tipo == "grupo" {
                        Msg::SaiDoGrupo(id.clone())
                    } else {
                        Msg::FechaConversa(id.clone())
                    },
                    true,
                ));
                let pad = posiciona(m.x, m.y, self.janela, 40.0 + v.len() as f32 * 36.0);
                sobre(base, coluna(v), pad)
            }
            AlvoMenu::Mensagem(id) => {
                let Tela::Sala(s) = &self.tela else {
                    return base;
                };
                let Some(msg) = s.mensagens.iter().find(|x| &x.id == id) else {
                    return base;
                };
                let minha = ui::chat::eh_minha_mensagem(msg, &s.entrada.nome, s.meu_uid.as_deref());
                let fixada = s.mensagem_fixada.as_ref().is_some_and(|f| &f.id == id);
                let faz = |m: Msg| Msg::MenuFaz(Box::new(m));
                // Só o que a lista pede: copiar (texto, link, imagem), salvar
                // imagem, fixar, responder, encaminhar, favoritar. Editar e
                // apagar aparecem só nas mensagens da própria pessoa.
                let mut v: Vec<Element<'a, Msg>> = Vec::new();
                if !msg.texto.is_empty() {
                    v.push(item(
                        icones::copiar(),
                        "Copiar texto",
                        faz(Msg::CopiaTexto(msg.texto.clone())),
                        false,
                    ));
                }
                let link = ui::chat::separa_links(&msg.texto)
                    .into_iter()
                    .find_map(|seg| match seg {
                        ui::chat::SegmentoTexto::Link(l) => Some(l.to_string()),
                        _ => None,
                    })
                    .or_else(|| msg.gif.clone());
                if let Some(l) = link {
                    v.push(item(
                        icones::link(),
                        "Copiar link",
                        faz(Msg::CopiaTexto(l)),
                        false,
                    ));
                }
                if msg.imagem.is_some() {
                    v.push(item(
                        icones::imagem(),
                        "Copiar imagem",
                        faz(Msg::CopiaImagem(id.clone())),
                        false,
                    ));
                    v.push(item(
                        icones::baixar(),
                        "Salvar imagem",
                        faz(Msg::SalvaImagem(id.clone())),
                        false,
                    ));
                }
                v.push(item(
                    icones::pin(),
                    if fixada {
                        "Desafixar"
                    } else {
                        "Fixar mensagem"
                    },
                    faz(if fixada {
                        Msg::DesafixaChat
                    } else {
                        Msg::FixaChat(id.clone())
                    }),
                    false,
                ));
                v.push(item(
                    icones::responder(),
                    "Responder",
                    faz(Msg::IniciaRespostaChat {
                        id: id.clone(),
                        autor: msg.autor.clone(),
                        texto: ui::chat::trunca_trecho(&msg.texto, 120),
                    }),
                    false,
                ));
                v.push(item(
                    icones::encaminhar(),
                    "Encaminhar",
                    faz(Msg::AbreEncaminhar(id.clone())),
                    false,
                ));
                v.push(item(
                    icones::favorito(),
                    "Favoritar mensagem",
                    faz(Msg::Favorita(id.clone())),
                    false,
                ));
                if minha {
                    v.push(separador());
                    if !msg.texto.is_empty() {
                        v.push(item(
                            icones::editar(),
                            "Editar",
                            faz(Msg::IniciaEdicaoChat(id.clone())),
                            false,
                        ));
                    }
                    v.push(item(
                        icones::lixeira(),
                        "Apagar",
                        faz(Msg::ApagaChat(id.clone())),
                        true,
                    ));
                }
                let pad = posiciona(m.x, m.y, self.janela, 12.0 + v.len() as f32 * 36.0);
                sobre(base, coluna(v), pad)
            }
            AlvoMenu::Midia(avatar) => {
                let faz = |m: Msg| Msg::MenuFaz(Box::new(m));
                let tem = if *avatar {
                    self.eu.as_ref().is_some_and(|e| e.avatar.is_some())
                } else {
                    self.eu.as_ref().is_some_and(|e| e.banner_url.is_some())
                };
                // Imagem abre o explorador de arquivos; GIF, o buscador do app.
                let mut v: Vec<Element<'a, Msg>> = vec![
                    item(
                        icones::imagem_sobe(),
                        "Enviar imagem",
                        faz(if *avatar {
                            Msg::EscolheArquivoAvatar
                        } else {
                            Msg::EscolheArquivoBanner
                        }),
                        false,
                    ),
                    item(
                        icones::imagem(),
                        "Escolher GIF",
                        faz(Msg::AbreGifPerfil(*avatar)),
                        false,
                    ),
                ];
                if tem {
                    v.push(item(
                        icones::lixeira(),
                        if *avatar {
                            "Remover avatar"
                        } else {
                            "Remover banner"
                        },
                        faz(if *avatar {
                            Msg::RemoveAvatar
                        } else {
                            Msg::RemoveBanner
                        }),
                        true,
                    ));
                }
                let pad = posiciona(m.x, m.y, self.janela, 12.0 + v.len() as f32 * 36.0);
                sobre(base, coluna(v), pad)
            }
            AlvoMenu::Imagem(id) => {
                let faz = |m: Msg| Msg::MenuFaz(Box::new(m));
                let v: Vec<Element<'a, Msg>> = vec![
                    item(
                        icones::imagem(),
                        "Copiar imagem",
                        faz(Msg::CopiaImagem(id.clone())),
                        false,
                    ),
                    item(
                        icones::baixar(),
                        "Salvar imagem",
                        faz(Msg::SalvaImagem(id.clone())),
                        false,
                    ),
                ];
                let pad = posiciona(m.x, m.y, self.janela, 12.0 + v.len() as f32 * 36.0);
                sobre(base, coluna(v), pad)
            }
            AlvoMenu::Transmissao(dono, de_tela) => {
                let Tela::Sala(s) = &self.tela else {
                    return base;
                };
                let minha = &s.meu_id == dono;
                let quem = s
                    .pessoas
                    .iter()
                    .find(|p| &p.id == dono)
                    .map(|p| p.nome.clone())
                    .unwrap_or_else(|| "Transmissão".into());
                let mut v: Vec<Element<'a, Msg>> = vec![titulo(&quem), separador()];
                if !minha && let Some(p) = s.pessoas.iter().find(|p| &p.id == dono) {
                    let (vol, mudo) = if *de_tela {
                        (p.volume_tela, p.volume_tela <= 0.0)
                    } else {
                        (p.volume, p.volume <= 0.0)
                    };
                    v.push(
                        container(ui::volume::controle_volume_video(
                            dono.clone(),
                            *de_tela,
                            vol,
                            mudo,
                        ))
                        .padding([6, 10])
                        .into(),
                    );
                    v.push(separador());
                }
                if !(minha && *de_tela) {
                    v.push(item(
                        icones::maximizar(),
                        "Tela cheia",
                        Msg::AlternaTelaCheiaVideo(Some((dono.clone(), *de_tela))),
                        false,
                    ));
                }
                if minha && *de_tela {
                    let oculta = matches!(&self.tela, Tela::Sala(s) if s.previa_propria_oculta);
                    v.push(item(
                        icones::olho(),
                        if oculta {
                            "Ver minha tela"
                        } else {
                            "Parar de ver minha tela"
                        },
                        Msg::AlternaPreviaPropria,
                        false,
                    ));
                }
                if minha {
                    v.push(item(
                        icones::tela_off(),
                        "Parar de transmitir",
                        Msg::ParaTela,
                        true,
                    ));
                } else {
                    v.push(item(
                        icones::fechar(),
                        "Parar de assistir",
                        Msg::PausaTransmissao(dono.clone(), *de_tela),
                        false,
                    ));
                }
                let pad = posiciona(m.x, m.y, self.janela, 40.0 + v.len() as f32 * 36.0);
                sobre(base, coluna(v), pad)
            }
            AlvoMenu::WordleJogador { dia, uid } => {
                let jogador = match &self.tela {
                    Tela::Sala(s) => s.wordle_grupo.get(dia).and_then(|g| {
                        g.hoje
                            .iter()
                            .chain(&g.ontem)
                            .find(|j| j.uid == *uid)
                            .map(|j| (j, g.numero))
                    }),
                    _ => None,
                };
                let Some((j, numero)) = jogador else {
                    return base;
                };
                ui::menu::sobre_cartao(
                    base,
                    ui::wordle::cartao_tentativas(j, numero),
                    m.x,
                    m.y,
                    self.janela,
                    ui::wordle::LARGURA_TENTATIVAS,
                    ui::wordle::ALTURA_TENTATIVAS,
                )
            }
            AlvoMenu::Previa { uid, nome, app } => {
                if *app {
                    return ui::menu::sobre_cartao(
                        base,
                        ui::previa::previa_app(nome),
                        m.x,
                        m.y,
                        self.janela,
                        ui::previa::LARGURA,
                        ui::previa::ALTURA,
                    );
                }
                let conv = match &self.tela {
                    Tela::Sala(s) => self
                        .avisos
                        .conversas
                        .iter()
                        .find(|c| c.id == s.entrada.sala),
                    _ => None,
                };
                let membro = conv.and_then(|c| {
                    c.membros
                        .iter()
                        .find(|x| uid.as_deref().is_some_and(|u| x.uid == u) || x.name == *nome)
                });
                let uid_final = membro
                    .map(|x| x.uid.as_str())
                    .or(uid.as_deref())
                    .unwrap_or(nome);
                let usuario = membro.and_then(|x| x.usuario.as_deref());
                let sou_eu = usuario.is_some() && usuario == meu_usuario;
                let dados = ui::previa::DadosPrevia {
                    uid: uid_final,
                    nome: membro.map(|x| x.name.as_str()).unwrap_or(nome),
                    usuario,
                    avatar: membro.and_then(|x| x.avatar.as_deref()),
                    banner: membro.and_then(|x| x.banner_url.as_deref()),
                    cor: membro
                        .and_then(|x| x.cor.as_deref())
                        .and_then(ui::perfil::parse_hex_color),
                    status: membro.and_then(|x| x.status.as_deref()),
                    bio: membro.and_then(|x| x.bio.as_deref()),
                    sou_eu,
                };
                ui::menu::sobre_cartao(
                    base,
                    ui::previa::previa(dados),
                    m.x,
                    m.y,
                    self.janela,
                    ui::previa::LARGURA,
                    ui::previa::ALTURA,
                )
            }
            AlvoMenu::Pessoa { uid, usuario, nome } => {
                let sala = match &self.tela {
                    Tela::Sala(s) => Some(s),
                    _ => None,
                };
                let conv = sala.and_then(|s| {
                    self.avisos
                        .conversas
                        .iter()
                        .find(|c| c.id == s.entrada.sala)
                });
                let sou_eu = conv
                    .and_then(|c| c.membros.iter().find(|x| &x.uid == uid))
                    .is_some_and(|x| x.usuario.as_deref() == meu_usuario && meu_usuario.is_some());
                let alvo_perfil = usuario.clone().unwrap_or_else(|| nome.clone());
                let mut v: Vec<Element<'a, Msg>> = vec![titulo(nome), separador()];
                v.push(item(
                    icones::amigos(),
                    "Perfil",
                    Msg::AbrePerfilUsuario(alvo_perfil.clone()),
                    false,
                ));
                if sala.is_some() {
                    v.push(item(
                        icones::mensagem(),
                        "Mencionar",
                        Msg::MencionaPessoa(alvo_perfil),
                        false,
                    ));
                }
                if sala.is_some() && !sou_eu {
                    v.push(item(
                        icones::fone(),
                        "Sussurrar",
                        Msg::MenuFaz(Box::new(Msg::IniciaSussurro {
                            uid: Some(uid.clone()),
                            nome: nome.clone(),
                        })),
                        false,
                    ));
                }
                if !sou_eu {
                    v.push(item(
                        icones::mensagem(),
                        "Mensagem",
                        Msg::AbreDm {
                            uid: uid.clone(),
                            nome: nome.clone(),
                            ligar: false,
                        },
                        false,
                    ));
                }
                let dentro = conv
                    .and_then(|c| self.avisos.chamadas.get(&c.id))
                    .is_some_and(|d| d.contains(uid));
                let eu_na_chamada = sala.is_some_and(|s| s.na_chamada);
                if let (Some(c), true, false, false) = (conv, eu_na_chamada, dentro, sou_eu) {
                    v.push(item(
                        icones::chamada(),
                        "Toque para chamar",
                        Msg::TocaPara {
                            conversa: c.id.clone(),
                            uid: uid.clone(),
                        },
                        false,
                    ));
                }
                if let Some(p) = sala
                    .filter(|_| dentro && !sou_eu)
                    .and_then(|s| s.pessoas.iter().find(|p| p.uid.as_deref() == Some(uid)))
                {
                    v.push(separador());
                    v.push(
                        container(ui::volume::controle_volume_participante(p))
                            .padding([6, 10])
                            .into(),
                    );
                }
                if let Some(c) = conv
                    && c.tipo == "grupo"
                    && !sou_eu
                    && meu_uid_do(c, meu_usuario).as_deref() == c.dono.as_deref()
                    && c.dono.is_some()
                {
                    v.push(separador());
                    v.push(item(
                        icones::expulsar(),
                        "Remover do grupo",
                        Msg::RemoveDoGrupo {
                            conversa: c.id.clone(),
                            uid: uid.clone(),
                        },
                        true,
                    ));
                }
                v.push(separador());
                v.push(item(
                    icones::copiar(),
                    "Copiar ID",
                    Msg::CopiaTexto(uid.clone()),
                    false,
                ));
                let pad = posiciona(m.x, m.y, self.janela, 40.0 + v.len() as f32 * 36.0);
                sobre(base, coluna(v), pad)
            }
        }
    }

    /// A coluna da esquerda, igual dentro e fora de uma conversa.
    fn coluna(&self) -> Element<'_, Msg> {
        use ui::coluna::{ChamadaLocal, ContextoColuna};
        let status = self
            .eu
            .as_ref()
            .and_then(|e| e.perfil.as_ref())
            .and_then(|p| p.status.as_deref())
            .unwrap_or("online");
        let sala_chamada: Option<&SalaAberta> = match &self.tela {
            Tela::Sala(s) if s.na_chamada => Some(s.as_ref()),
            _ => self.chamada_ativa.as_deref(),
        };
        let chamada = sala_chamada.map(|s| ChamadaLocal {
            titulo: &s.titulo,
            desde: s.chamada_desde,
            com_camera: s.com_camera,
            apresentando: s.apresentando,
            som_aberto: s.painel == Some(PainelFlutuante::Sons),
            falando: s.pessoas.iter().any(|p| p.id == s.meu_id && p.falando),
        });
        let mudo = sala_chamada.map(|s| s.mudo);
        let surdo = sala_chamada.map(|s| s.surdo);
        let (ativa, amigos_aberto) = match &self.tela {
            Tela::Sala(s) => (Some(s.entrada.sala.as_str()), false),
            Tela::Salas(s) => (None, s.aba == AbaSalas::Amigos),
            _ => (None, false),
        };
        ui::coluna::coluna(&ContextoColuna {
            eu: self.eu.as_ref(),
            avisos: &self.avisos,
            filtro: &self.filtro_conversas,
            ativa,
            amigos_aberto,
            pedidos_pendentes: self.pedidos_pendentes,
            status,
            chamada,
            wordle: match &self.tela {
                Tela::Sala(s) => s.wordle.as_ref().map(|w| (s.titulo.as_str(), w.modo)),
                _ => None,
            },
            mudo,
            surdo,
            prefs: &self.prefs_conversas,
        })
    }

    pub(crate) fn view(&self) -> Element<'_, Msg> {
        anim::inicio_quadro();
        let base: Element<'_, Msg> = match &self.tela {
            Tela::Carregando => center(text("Abrindo…").style(fraco))
                .style(tema::painel_principal)
                .into(),
            Tela::Login(l) => tela_login(l, &self.srv),
            Tela::Salas(s) => tela_salas(s, self.coluna()),
            Tela::Sala(s) => {
                let meu_usuario = self.eu.as_ref().and_then(|e| e.usuario.as_deref());
                let info = self
                    .avisos
                    .conversas
                    .iter()
                    .find(|c| c.id == s.entrada.sala)
                    .map(|conversa| ui::conversa::InfoConversa {
                        conversa,
                        meu_usuario,
                    });
                s.view(self.coluna(), info.as_ref(), &self.avisos)
            }
        };
        // Popup do avatar: abre sobre a coluna, acima do rodapé do usuário.
        let base: Element<'_, Msg> = if self.menu_status_aberto && self.eu.is_some() {
            let eu = self.eu.as_ref();
            let atual = eu
                .and_then(|e| e.perfil.as_ref())
                .and_then(|p| p.status.as_deref())
                .unwrap_or("online");
            let uid = self.avisos.meu_uid().unwrap_or("");
            let atividade = self
                .avisos
                .atividade_amigos
                .get(uid)
                .and_then(|v| serde_json::from_value::<api::Atividade>(v.clone()).ok());
            let popup = ui::popup_perfil::popup(ui::popup_perfil::DadosPopup {
                nome: eu.map(|e| e.name.as_str()).unwrap_or(""),
                usuario: eu.and_then(|e| e.usuario.as_deref()),
                uid,
                avatar: eu.and_then(|e| e.avatar.as_deref()),
                banner: eu.and_then(|e| e.banner_url.as_deref()),
                status: atual,
                custom: eu
                    .and_then(|e| e.perfil.as_ref())
                    .and_then(|p| p.status_custom.as_ref()),
                atividade,
                presenca_aberta: self.presenca_aberta,
            });
            let fora = iced::widget::mouse_area(
                container(iced::widget::space())
                    .width(Length::Fill)
                    .height(Length::Fill),
            )
            .on_press(Msg::AlternaMenuStatus);
            let menu = container(iced::widget::opaque(ui::efeitos::entrada(
                "popup-avatar",
                popup,
            )))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_y(Alignment::End)
            .padding(iced::Padding {
                top: 0.0,
                right: 0.0,
                bottom: 62.0,
                left: 16.0,
            });
            let mut camadas = vec![base, fora.into(), menu.into()];
            if self.presenca_aberta {
                camadas.push(
                    container(iced::widget::opaque(ui::popup_perfil::submenu_presenca(
                        atual,
                    )))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_y(Alignment::End)
                    .padding(iced::Padding {
                        top: 0.0,
                        right: 0.0,
                        // Alinhado à linha da presença dentro do popup.
                        bottom: 118.0,
                        // Encosta na borda direita do popup (16 + 320) para o
                        // mouse passar da linha ao submenu sem sair de nenhum.
                        left: 334.0,
                    })
                    .into(),
                );
            }
            iced::widget::stack(camadas).into()
        } else {
            base
        };

        let mut tela_final: Element<'_, Msg> = if let Some(aba_ativa) = self.modal_config {
            let soundboard_mudo = if let Tela::Sala(s) = &self.tela {
                s.soundboard.mudo_global
            } else {
                false
            };
            let ctx = ui::configuracoes::ContextoConfiguracoes {
                so_perfil: self.editar_perfil_janela,
                aba_ativa,
                prefs: &self.prefs_conversas,
                atalhos: &self.atalhos,
                capturando_atalho: self.capturando_atalho.as_deref(),
                aviso_atalho: self.aviso_atalho.as_deref(),
                painel_audio: &self.painel_audio,
                soundboard_mudo,
                cameras: &self.cameras,
                camera_selecionada: self.camera_selecionada,
                espelhar_camera: self.espelhar_camera,
                perfil_tela: self.perfil_tela,
                ptt: &self.ptt,
                capturando_ptt: self.capturando_ptt,
                portal_global_ativo: self.portal_global_ativo,
                minimizar_para_bandeja: self.minimizar_para_bandeja,
                tema_atual: tema::tema_atual_id(),
                temas: tema::lista_temas(),
                densidade: self.densidade,
                edicao_perfil: self.modal_meu_perfil.as_ref(),
                usuario: self
                    .eu
                    .as_ref()
                    .and_then(|e| e.usuario.as_deref())
                    .unwrap_or(""),
                avatar_url: self.eu.as_ref().and_then(|e| e.avatar.as_deref()),
                banner_url: self.eu.as_ref().and_then(|e| e.banner_url.as_deref()),
                uid: self.avisos.meu_uid().unwrap_or(""),
                nome_atual: self.eu.as_ref().map(|e| e.name.as_str()).unwrap_or(""),
                janela: self.janela,
                jogos_reg: &self.jogos_reg,
                jogos_reg_filtro: &self.jogos_reg_filtro,
                processos_abertos: &self.processos_abertos,
                jogos_reg_renomeando: self
                    .jogos_reg_renomeando
                    .as_ref()
                    .map(|(e, n)| (e.as_str(), n.as_str())),
                jogos_reg_seletor_aberto: self.jogos_reg_seletor_aberto,
                atualizacao_status: self.atualizacao_status.as_deref(),
                atualizando: self.atualizando,
                volume_mic: self.volume_mic,
                volume_saida: self.volume_saida,
                status_custom: self
                    .eu
                    .as_ref()
                    .and_then(|e| e.perfil.as_ref())
                    .and_then(|p| p.status_custom.as_ref()),
                // A atividade detectada aqui vale mais que o eco do servidor.
                atividade: self.atividade_atual.clone().or_else(|| {
                    self.avisos
                        .atividade_amigos
                        .get(self.avisos.meu_uid().unwrap_or(""))
                        .and_then(|v| serde_json::from_value::<api::Atividade>(v.clone()).ok())
                }),
            };
            let modal_content = ui::configuracoes::modal_configuracoes(ctx);
            let backdrop = ui::efeitos::janela_fechavel(
                "config",
                modal_content,
                0.65,
                Msg::FechaConfiguracoes,
            );
            iced::widget::stack![base, backdrop].into()
        } else if let Some(cartao) = &self.modal_outro_perfil {
            let relacao = if Some(cartao.usuario.as_str())
                == self.eu.as_ref().and_then(|e| e.usuario.as_deref())
            {
                ui::perfil::Relacao::Eu
            } else if let Some(a) = &self.amigos_cache {
                let tem = |l: &[api::CartaoPerfil]| l.iter().any(|c| c.uid == cartao.uid);
                if tem(&a.amigos) {
                    ui::perfil::Relacao::Amigo
                } else if tem(&a.enviados) {
                    ui::perfil::Relacao::PedidoEnviado
                } else if tem(&a.recebidos) {
                    ui::perfil::Relacao::PedidoRecebido
                } else {
                    ui::perfil::Relacao::Nenhuma
                }
            } else {
                ui::perfil::Relacao::Nenhuma
            };
            let modal_content = ui::perfil::modal_perfil_usuario(
                cartao,
                relacao,
                &self.ver_perfil,
                self.avisos.meu_uid().unwrap_or(""),
            );
            let backdrop =
                ui::efeitos::janela_fechavel("perfil", modal_content, 0.6, Msg::FechaModalPerfil);
            iced::widget::stack![base, backdrop].into()
        } else if self.painel_tema_aberto {
            let modal_content =
                ui::tema::modal_seletor_temas(&tema::tema_atual_id()).map(Msg::Tema);
            let backdrop = ui::efeitos::janela("tema", modal_content, 0.6);
            iced::widget::stack![base, backdrop].into()
        } else if self.painel_diag_aberto {
            let mic = self
                .painel_audio
                .entrada_selecionada
                .as_deref()
                .and_then(|guid| {
                    self.painel_audio
                        .entradas
                        .iter()
                        .find(|d| d.guid == guid)
                        .map(|d| d.nome.clone())
                })
                .unwrap_or_else(|| "Padrão do sistema".into());
            let saida = self
                .painel_audio
                .saida_selecionada
                .as_deref()
                .and_then(|guid| {
                    self.painel_audio
                        .saidas
                        .iter()
                        .find(|d| d.guid == guid)
                        .map(|d| d.nome.clone())
                })
                .unwrap_or_else(|| "Padrão do sistema".into());
            let camera = self
                .cameras
                .get(self.camera_selecionada.unwrap_or(0))
                .map(|c| c.nome.clone())
                .unwrap_or_else(|| "Nenhuma".into());

            let local = {
                let (sala_nome, espectador, sou_dono, ice_qtd, turn_suporte) = match &self.tela {
                    Tela::Sala(s) => (
                        Some(s.entrada.sala.clone()),
                        !s.na_chamada,
                        s.sou_dono,
                        3,
                        true,
                    ),
                    _ => (None, false, false, 0, false),
                };

                ui::diagnostico::InfoDiagnosticoLocal {
                    versao_app: env!("CARGO_PKG_VERSION").into(),
                    sala: sala_nome,
                    usuario: self
                        .eu
                        .as_ref()
                        .map(|e| e.name.clone())
                        .unwrap_or_else(|| "Anônimo".into()),
                    espectador,
                    sou_dono,
                    tema_nome: tema::tema_atual_id(),
                    densidade_nome: self.densidade.rotulo().into(),
                    mic_nome: mic,
                    saida_nome: saida,
                    camera_nome: camera,
                    ice_servers_qtd: ice_qtd,
                    suporte_turn: turn_suporte,
                }
            };

            let peers: Vec<ui::diagnostico::InfoDiagnosticoPeer> = match &self.tela {
                Tela::Sala(s) => s
                    .pessoas
                    .iter()
                    .map(|p| ui::diagnostico::InfoDiagnosticoPeer {
                        id: p.id.clone(),
                        nome: p.nome.clone(),
                        uid: p.uid.clone(),
                        qualidade: p.qualidade.clone(),
                        falando: p.falando,
                        mutado: p.mudo,
                        ensurdecido: p.surdo,
                        transmitindo_tela: p.apresentando,
                        transmitindo_camera: p.com_camera,
                    })
                    .collect(),
                _ => Vec::new(),
            };

            let modal_content = ui::diagnostico::modal_diagnostico(
                &local,
                &peers,
                self.metricas_servidor.as_ref(),
                self.carregando_metricas,
                tema::cores(),
                Msg::FechaPainelDiagnostico,
                Msg::AtualizaMetricasServidor,
            );
            iced::widget::stack![base, modal_content].into()
        } else {
            base
        };

        if self.modal_manual_aberto {
            let modal_content = ui::manual::modal_manual(
                &self.filtro_manual,
                tema::cores(),
                Msg::MudaFiltroManual,
                Msg::FechaManual,
            );
            tela_final = iced::widget::stack![tela_final, modal_content].into();
        }

        if let Some(e) = &self.erro_global {
            let c = tema::cores();
            let faixa = container(
                row![
                    text(e.as_str()).size(13).color(c.texto),
                    iced::widget::space().width(Length::Fill),
                    button(text("Fechar").size(12))
                        .padding([4, 10])
                        .style(tema::botao_secundario)
                        .on_press(Msg::FechaErroGlobal),
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            )
            .padding([8, 14])
            .width(Length::Fill)
            .style(move |_| container::Style {
                background: Some(iced::Background::Color(c.perigo)),
                ..Default::default()
            });
            tela_final = iced::widget::stack![
                tela_final,
                container(faixa)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_y(Alignment::End)
            ]
            .into();
        }
        if let Some(info) = &self.atualizacao {
            tela_final = iced::widget::stack![tela_final, ui::atualizacao::banner(info)].into();
        }

        if let Some(m) = &self.modal_editar_grupo {
            let conv = self.avisos.conversas.iter().find(|c| c.id == m.conversa);
            let meu_usuario = self.eu.as_ref().and_then(|e| e.usuario.as_deref());
            let gif_ativo = self.eu.as_ref().map(|e| e.gif_ativo).unwrap_or(false);
            tela_final = iced::widget::stack![
                tela_final,
                ui::editar_grupo::modal(m, conv, meu_usuario, gif_ativo)
            ]
            .into();
        }

        if let Some(m) = &self.modal_grupo {
            let amigos = self
                .amigos_cache
                .as_ref()
                .map(|a| a.amigos.as_slice())
                .unwrap_or(&[]);
            let ja: Vec<String> = match &m.modo {
                ui::grupo::ModoGrupo::Adicionar(id) => self
                    .avisos
                    .conversas
                    .iter()
                    .find(|c| &c.id == id)
                    .map(|c| c.membros.iter().map(|x| x.uid.clone()).collect())
                    .unwrap_or_default(),
                ui::grupo::ModoGrupo::Criar => Vec::new(),
            };
            let cands = ui::grupo::candidatos(amigos, &m.busca, &ja);
            let modal = ui::grupo::modal(m, &cands, self.amigos_cache.is_none());
            tela_final = iced::widget::stack![tela_final, modal].into();
        }

        if let Some(tipo) = &self.modal_confirmacao {
            let modal_content = ui::confirmacao::modal_confirmacao(
                tipo,
                tema::cores(),
                Msg::ConfirmaAcaoCritica,
                Msg::CancelaAcaoCritica,
            );
            tela_final = iced::widget::stack![tela_final, modal_content].into();
        }

        if let Some(m) = &self.modal_status_custom {
            tela_final = iced::widget::stack![
                tela_final,
                ui::efeitos::janela_fechavel(
                    "status-custom",
                    ui::status_custom::modal(m),
                    0.65,
                    Msg::FechaStatusCustom,
                )
            ]
            .into();
        }

        if let Some(m) = &self.modal_encaminhar {
            let meu = self.eu.as_ref().and_then(|e| e.usuario.as_deref());
            tela_final = iced::widget::stack![
                tela_final,
                ui::efeitos::janela_fechavel(
                    "encaminhar",
                    ui::encaminhar::modal(m, &self.avisos.conversas, meu),
                    0.65,
                    Msg::FechaEncaminhar,
                )
            ]
            .into();
        }

        if let Some(m) = &self.modal_favoritos {
            let meu = self.eu.as_ref().and_then(|e| e.usuario.as_deref());
            tela_final = iced::widget::stack![
                tela_final,
                ui::efeitos::janela_fechavel(
                    "favoritos",
                    ui::favoritos::modal(m, &self.avisos.conversas, meu),
                    0.65,
                    Msg::FechaFavoritos,
                )
            ]
            .into();
        }

        tela_final = self.com_popup_voz(tela_final);

        // Menu de contexto (botão direito / foto e banner): por cima das janelas
        // abertas, senão ele abre escondido atrás da janela de editar perfil.
        tela_final = self.com_menu_de_contexto(tela_final);

        if let Some((t, _)) = &self.toast {
            tela_final = iced::widget::stack![tela_final, ui::efeitos::toast(t)].into();
        }

        // O toque vai por cima de tudo, inclusive das janelas abertas.
        if let Some(t) = self.avisos.toques.first() {
            let meu = self.eu.as_ref().and_then(|e| e.usuario.as_deref());
            let conv = self.avisos.conversas.iter().find(|c| c.id == t.conversa);
            let destino = conv
                .map(|c| nome_da_conversa(c, meu))
                .unwrap_or_else(|| t.quem_nome.clone());
            let foto = conv
                .and_then(|c| c.membros.iter().find(|m| m.uid == t.quem_uid))
                .and_then(|m| m.avatar.as_deref());
            let pulso = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| (d.as_millis() / 500) % 2 == 0)
                .unwrap_or(false);
            tela_final =
                iced::widget::stack![tela_final, ui::toque::modal(t, destino, foto, pulso)].into();
        }

        if let Some(j) = &self.modal_visao_jogo {
            tela_final =
                iced::widget::stack![tela_final, ui::configuracoes::modal_visao_jogo(j)].into();
        }

        if let Some(g) = &self.gif_perfil {
            tela_final = iced::widget::stack![tela_final, ui::perfil::modal_gif_perfil(g)].into();
        }

        if let Some(f) = &self.modal_foto {
            // Imagem do chat: o quadro atual do GIF ou a própria imagem.
            let da_mensagem = match (&f.mensagem, &self.tela) {
                (Some(id), Tela::Sala(s)) => {
                    let chave = crate::gifanim::chave_mensagem(id);
                    crate::gifanim::quadro_atual(&chave)
                        .or_else(|| crate::gifanim::quadro_parado(&chave))
                        .or_else(|| s.imagens.get(id).cloned())
                        .map(|h| (h, chave))
                }
                _ => None,
            };
            tela_final = iced::widget::stack![
                tela_final,
                ui::perfil::modal_visualizador_foto(f, da_mensagem)
            ]
            .into();
        }

        tela_final
    }
}

/// O uid de quem sou eu dentro desta conversa (casa pelo nome de usuário).
fn meu_uid_do(c: &avisos::Conversa, meu_usuario: Option<&str>) -> Option<String> {
    let eu = meu_usuario?;
    c.membros
        .iter()
        .find(|m| m.usuario.as_deref() == Some(eu))
        .map(|m| m.uid.clone())
}
