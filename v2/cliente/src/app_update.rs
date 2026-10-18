//! `App::update`: reage às mensagens da interface.

use super::*;

impl App {
    pub(crate) fn atualiza(&mut self, msg: Msg) -> Task<Msg> {
        // Posição do mouse e tamanho da janela: os menus abrem no lugar certo.
        if let Msg::EventoIced(ev) = &msg {
            match ev {
                iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
                    self.cursor = *position;
                }
                iced::Event::Window(iced::window::Event::Resized(tam)) => self.janela = *tam,
                iced::Event::Window(iced::window::Event::Focused) => self.janela_focada = true,
                iced::Event::Window(iced::window::Event::Unfocused) => self.janela_focada = false,
                _ => {}
            }
            self.sincroniza_conversa_ativa();
        }
        match msg {
            Msg::ImagemBaixada { url, bytes } => avatar::guarda(&url, bytes),
            Msg::Avisos(ev) => {
                if let avisos::Evento::Pronto(tx) = &ev {
                    self.avisos_cmd = Some(tx.clone());
                }
                self.avisos.meu_usuario = self.eu.as_ref().and_then(|e| e.usuario.clone());
                self.sincroniza_conversa_ativa();
                // Mensagem nova numa conversa fechada traz ela de volta à lista.
                if let avisos::Evento::Mensagem { conversa, sys, .. } = &ev
                    && sys.is_none()
                    && self.prefs_conversas.reabre(conversa)
                {
                    self.prefs_conversas.salva();
                }
                let havia_toque = !self.avisos.toques.is_empty();
                self.avisos.aplica(&ev);
                self.notifica_mensagem(&ev);
                // Toque: som em laço enquanto houver, e um aviso do sistema
                // quando a janela não está à vista.
                if !self.deve_tocar() {
                    self.toque_sonoro = None;
                } else if self.toque_sonoro.is_none() && self.prefs_conversas.som_do_toque {
                    self.toque_sonoro =
                        Some(som::ToqueSonoro::inicia(self.prefs_conversas.volume_toque));
                }
                if !havia_toque
                    && !self.janela_focada
                    && let Some(t) = self.avisos.toques.first()
                {
                    let destino = self
                        .avisos
                        .conversas
                        .iter()
                        .find(|c| c.id == t.conversa)
                        .map(|c| {
                            nome_da_conversa(c, self.eu.as_ref().and_then(|e| e.usuario.as_deref()))
                        })
                        .unwrap_or_else(|| "Conversa".into());
                    ui::chat::dispara_notificacao_desktop(
                        &format!("{} está chamando", t.quem_nome),
                        &destino,
                        "critical",
                    );
                }
            }
            Msg::Sessao(Ok(Some(eu))) => {
                self.eu = Some(eu);
                if let Some(t) = &self.token {
                    crate::relatorio::envia_anterior(&self.srv, t);
                }
                return self.vai_pras_salas();
            }
            Msg::Sessao(Ok(None)) => {
                self.token = None;
                api::salva_sessao(None);
                self.tela = Tela::Login(Login::default());
            }
            Msg::Sessao(Err(e)) => {
                self.tela = Tela::Login(Login {
                    erro: Some(format!("Não consegui falar com o servidor: {e}")),
                    ..Default::default()
                });
            }

            Msg::Usuario(v) => {
                if let Tela::Login(l) = &mut self.tela {
                    l.usuario = v;
                }
            }
            Msg::Senha(v) => {
                if let Tela::Login(l) = &mut self.tela {
                    l.senha = v;
                }
            }
            Msg::AlternaCriar => {
                if let Tela::Login(l) = &mut self.tela {
                    l.criando = !l.criando;
                    l.erro = None;
                }
            }
            Msg::Entrar => {
                if let Tela::Login(l) = &mut self.tela {
                    if l.enviando || l.usuario.trim().is_empty() || l.senha.is_empty() {
                        return Task::none();
                    }
                    l.enviando = true;
                    l.erro = None;
                    let (srv, u, s, c) = (
                        self.srv.clone(),
                        l.usuario.trim().to_string(),
                        l.senha.clone(),
                        l.criando,
                    );
                    return em_segundo_plano(
                        move || api::entra(&srv, &u, &s, c).map_err(|e| format!("{e:#}")),
                        Msg::Entrou,
                    );
                }
            }
            Msg::Entrou(Ok(token)) => {
                api::salva_sessao(Some(&token));
                self.token = Some(token.clone());
                let srv = self.srv.clone();
                return em_segundo_plano(
                    move || api::eu(&srv, &token).map_err(|e| format!("{e:#}")),
                    Msg::Sessao,
                );
            }
            Msg::Entrou(Err(e)) => {
                if let Tela::Login(l) = &mut self.tela {
                    l.enviando = false;
                    l.erro = Some(e);
                }
            }

            Msg::AbreSala(id) => {
                let id = id.trim().to_string();
                if id.is_empty() {
                    return Task::none();
                }
                self.menu_status_aberto = false;
                if self
                    .chamada_ativa
                    .as_ref()
                    .is_some_and(|c| c.entrada.sala == id)
                {
                    self.avisos.marca_lida(&id);
                    self.tela = Tela::Sala(self.chamada_ativa.take().unwrap());
                    self.atualiza_bandeja();
                    return Task::none();
                }
                if let Tela::Sala(s) = &mut self.tela {
                    if s.entrada.sala == id {
                        return Task::none();
                    }
                    if s.na_chamada {
                        let s = match std::mem::replace(
                            &mut self.tela,
                            Tela::Salas(Salas::default()),
                        ) {
                            Tela::Sala(s) => s,
                            _ => unreachable!(),
                        };
                        self.chamada_ativa = Some(s);
                    }
                }
                let Some(token) = self.token.clone() else {
                    return Task::none();
                };
                let ptt_cfg = ptt::carrega_config();
                let meu_usuario = self.eu.as_ref().and_then(|e| e.usuario.clone());
                let titulo = self
                    .avisos
                    .conversas
                    .iter()
                    .find(|c| c.id == id)
                    .map(|c| nome_da_conversa(c, meu_usuario.as_deref()))
                    .or_else(|| self.titulos_extra.get(&id).cloned())
                    .unwrap_or_else(|| "Conversa".to_string());
                let novas = self.avisos.nao_lidas.get(&id).map_or(0, |n| *n as usize);
                self.avisos.marca_lida(&id);
                self.tela = Tela::Sala(Box::new(SalaAberta {
                    titulo,
                    painel_direito_aberto: true,
                    fixadas_ativas: true,
                    painel: None,
                    wordle: None,
                    wordle_grupo: Default::default(),
                    chamada_desde: None,
                    entrada: sala::Entrada {
                        servidor: self.srv.clone(),
                        token,
                        sala: id,
                        nome: self.eu.as_ref().map(|e| e.name.clone()).unwrap_or_default(),
                    },
                    comandos: None,
                    meu_id: String::new(),
                    pessoas: Vec::new(),
                    mensagens: Vec::new(),
                    imagens: HashMap::new(),
                    rascunho: String::new(),
                    na_chamada: false,
                    mudo: false,
                    surdo: false,
                    apresentando: false,
                    com_camera: false,
                    videos: Vec::new(),
                    config_tela: None,
                    previa_camera: None,
                    aviso: None,
                    ptt: ptt_cfg,
                    capturando_ptt: false,
                    ptt_transmitindo: false,
                    portal_global_ativo: false,
                    painel_audio: None,
                    nivel_mic_atual: 0.0,
                    falando_no_mudo: false,
                    aviso_eco: false,
                    mudo_forcado: false,
                    modo_video: ui::video::ModoVideo::Grade,
                    video_fixado: None,
                    previa_propria_oculta: false,
                    cursor_pos: iced::Point::ORIGIN,
                    painel_ancora: iced::Point::ORIGIN,
                    video_em_tela_cheia: None,
                    assistindo: std::collections::HashSet::new(),
                    ultimo_movimento: std::time::Instant::now(),
                    ultimo_falante: None,
                    imagem_pendente: None,
                    meu_uid: None,
                    editando: None,
                    resposta_pendente: None,
                    seletor_reacao: None,
                    mensagem_fixada: None,
                    digitando_remotos: HashMap::new(),
                    digitando_anunciado: false,
                    ultimo_input_chat: None,
                    gif_ativo: self.eu.as_ref().map(|e| e.gif_ativo).unwrap_or(true),
                    busca_gif: String::new(),
                    busca_emoji: String::new(),
                    buscando_gif: false,
                    erro_gif: None,
                    resultados_gif: Vec::new(),
                    previas_gif: HashMap::new(),
                    sussurro_alvos: Vec::new(),
                    erro_sussurro: None,
                    busca_chat_aberta: false,
                    busca_chat: String::new(),
                    janela_focada: true,
                    nao_lidas: 0,
                    novas_ao_abrir: novas,
                    primeira_nao_lida_id: None,
                    sou_dono: false,
                    reacoes_flutuantes: Vec::new(),
                    aviso_cutucada: None,
                    id_reacao_seq: 0,
                    soundboard: ui::soundboard::EstadoSoundboard::default(),
                    ultimo_som_tocado: None,
                    quadro: ui::quadro::EstadoQuadro::default(),
                    roletas: ui::roleta::EstadoRoletas::default(),
                    criador_enquete: ui::enquete::EstadoCriadorEnquete::default(),
                    densidade: self.densidade,
                }));
            }
            Msg::Deslogar => {
                self.modal_config = None;
                self.editar_perfil_janela = false;
                if let Some(ca) = self.chamada_ativa.take() {
                    ca.manda(sala::Comando::SairDaChamada);
                }
                if let Some(t) = self.token.take() {
                    let srv = self.srv.clone();
                    std::thread::spawn(move || api::sai(&srv, &t));
                }
                api::salva_sessao(None);
                self.eu = None;
                self.tela = Tela::Login(Login::default());
            }

            Msg::Hover(id, dentro) => anim::hover(&id, dentro),
            Msg::AnimTick => anim::limpa(),
            Msg::SalaDe(id, ev) => return self.evento_da_sala_id(&id, ev),
            Msg::ConfirmaAcaoCritica => {
                let acao = self.modal_confirmacao.take();
                match acao {
                    Some(ui::confirmacao::TipoConfirmacao::SairChamada { .. }) => {
                        let ir_para = self.abrir_apos_sair.take();
                        let base = self.vai_pras_salas();
                        return match ir_para {
                            Some(Destino::Conversa(id)) => Task::done(Msg::AbreSala(id)),
                            Some(Destino::Amigos) => Task::done(Msg::AbreAmigos),
                            None => base,
                        };
                    }
                    Some(ui::confirmacao::TipoConfirmacao::FecharAplicativo { .. }) => {
                        crate::relatorio::envia_ao_fechar(&self.srv, self.token.as_deref());
                        return iced::exit();
                    }
                    Some(ui::confirmacao::TipoConfirmacao::ApagarGrupo { id, .. }) => {
                        if matches!(&self.tela, Tela::Sala(s) if s.entrada.sala == id) {
                            let _ = self.vai_pras_salas();
                        }
                        let Some(token) = self.token.clone() else {
                            return Task::none();
                        };
                        let srv = self.srv.clone();
                        let caminho = format!("/api/grupos/{}", api::codifica_segmento(&id));
                        return em_segundo_plano(
                            move || {
                                api::acao_de_grupo(&srv, &token, "DELETE", &caminho)
                                    .map_err(|e| format!("{e:#}"))
                            },
                            Msg::GrupoAlterado,
                        );
                    }
                    None => {}
                }
            }
            Msg::CancelaAcaoCritica => {
                self.modal_confirmacao = None;
                self.abrir_apos_sair = None;
            }
            Msg::AbreManual => {
                self.modal_manual_aberto = true;
            }
            Msg::FechaManual => {
                self.modal_manual_aberto = false;
            }
            Msg::MudaFiltroManual(f) => {
                self.filtro_manual = f;
            }
            Msg::BandejaIniciada(handle) => {
                self.handle_bandeja = Some(handle);
                self.atualiza_bandeja();
            }
            Msg::ChecaAtualizacao => {
                return em_segundo_plano(ui::atualizacao::consulta, Msg::AtualizacaoVerificada);
            }
            Msg::AtualizacaoVerificada(info) => {
                // Só substitui quando acha algo: falha de rede não apaga o aviso.
                if info.is_some() {
                    self.atualizacao = info;
                    // Ao abrir o app: atualiza sozinho (uma vez por execução; o
                    // processo reaberto depois da troca não tenta de novo).
                    if !self.tentou_auto
                        && !self.atualizando
                        && std::env::var_os("LINCAST_ATUALIZADO").is_none()
                    {
                        self.tentou_auto = true;
                        return Task::done(Msg::InstalaAtualizacao);
                    }
                }
                self.tentou_auto = true;
            }
            Msg::VerificaAtualizacaoManual => {
                if self.atualizando {
                    return Task::none();
                }
                self.atualizacao_status = Some("Verificando…".into());
                return em_segundo_plano(
                    ui::atualizacao::consulta_detalhada,
                    Msg::AtualizacaoManual,
                );
            }
            Msg::AtualizacaoManual(res) => match res {
                Ok(Some(info)) => {
                    self.atualizacao = Some(info);
                    return Task::done(Msg::InstalaAtualizacao);
                }
                Ok(None) => {
                    self.atualizacao_status = Some(format!(
                        "Você já está na versão mais recente (v{}).",
                        ui::atualizacao::versao_atual()
                    ));
                }
                Err(e) => self.atualizacao_status = Some(format!("Não consegui verificar: {e}.")),
            },
            Msg::InstalaAtualizacao => {
                let Some(info) = self.atualizacao.clone() else {
                    return Task::none();
                };
                if self.atualizando {
                    return Task::none();
                }
                self.atualizando = true;
                self.atualizacao_status = Some(format!("Baixando e instalando {}…", info.versao));
                return em_segundo_plano(
                    move || ui::atualizacao::instala(&info),
                    Msg::AtualizacaoInstalada,
                );
            }
            Msg::AtualizacaoInstalada(res) => {
                self.atualizando = false;
                match res {
                    Ok(versao) => {
                        self.atualizacao = None;
                        let em_chamada = matches!(&self.tela, Tela::Sala(s) if s.na_chamada);
                        if em_chamada {
                            self.atualizacao_status = Some(format!(
                                "Atualizado para a v{versao}. Feche e abra o app quando terminar a chamada."
                            ));
                        } else {
                            self.atualizacao_status =
                                Some(format!("Atualizado para a v{versao}. Reiniciando…"));
                            ui::atualizacao::reabre();
                            return iced::exit();
                        }
                    }
                    Err(e) => {
                        self.atualizacao_status = Some(format!("Não consegui atualizar: {e}."));
                    }
                }
            }
            Msg::AbreUrlAtualizacao(url) => {
                // O link vem da rede: só abre se for da página de releases.
                if ui::atualizacao::url_permitida(&url) {
                    std::thread::spawn(move || {
                        let _ = std::process::Command::new("xdg-open").arg(&url).output();
                    });
                }
            }
            Msg::DispensaAtualizacao => {
                self.atualizacao = None;
            }
            Msg::AlternaMinimizarParaBandeja => {
                self.minimizar_para_bandeja = !self.minimizar_para_bandeja;
                ui::bandeja::salva_config(&ui::bandeja::ConfigBandeja {
                    minimizar_para_bandeja: self.minimizar_para_bandeja,
                });
                self.atualiza_bandeja();
            }
            Msg::AcaoBandeja(acao) => match acao {
                ui::bandeja::AcaoBandeja::RestaurarJanela => {
                    return iced::window::latest().and_then(|id| {
                        Task::batch([
                            iced::window::minimize(id, false),
                            iced::window::gain_focus(id),
                        ])
                    });
                }
                ui::bandeja::AcaoBandeja::AlternarMudo => {
                    if let Tela::Sala(_) = &self.tela {
                        return Task::done(Msg::AlternaMudo);
                    }
                }
                ui::bandeja::AcaoBandeja::AlternarMinimizarParaBandeja => {
                    return Task::done(Msg::AlternaMinimizarParaBandeja);
                }
                ui::bandeja::AcaoBandeja::Sair => {
                    let em_chamada = if let Tela::Sala(s) = &self.tela {
                        if s.na_chamada {
                            Some(s.titulo.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    if let Some(sala) = em_chamada {
                        self.modal_confirmacao =
                            Some(ui::confirmacao::TipoConfirmacao::FecharAplicativo {
                                sala: Some(sala),
                            });
                        return iced::window::latest().and_then(|id| {
                            Task::batch([
                                iced::window::minimize(id, false),
                                iced::window::gain_focus(id),
                            ])
                        });
                    } else {
                        crate::relatorio::envia_ao_fechar(&self.srv, self.token.as_deref());
                        return iced::exit();
                    }
                }
            },
            Msg::FechaModalPerfil => {
                self.modal_meu_perfil = None;
                self.modal_outro_perfil = None;
            }
            Msg::AbrePerfilUsuario(usuario) => {
                if let Some(token) = &self.token {
                    let srv = self.srv.clone();
                    let tok = token.clone();
                    return em_segundo_plano(
                        move || {
                            api::busca_perfil(&srv, &tok, &usuario)
                                .map(Box::new)
                                .map_err(|e| format!("{e:#}"))
                        },
                        Msg::PerfilCarregado,
                    );
                }
            }
            Msg::PerfilCarregado(Ok(cartao)) => {
                self.ver_perfil = Default::default();
                let amigo = cartao.amigo_de_mim;
                let usuario = cartao.usuario.clone();
                self.modal_outro_perfil = Some(*cartao);
                // O mural vem junto com o perfil (tudo numa tela só).
                if amigo && let Some(token) = &self.token {
                    self.ver_perfil.carregando = true;
                    let (srv, tok) = (self.srv.clone(), token.clone());
                    let mural = em_segundo_plano(
                        move || {
                            api::mural_lista(&srv, &tok, &usuario).map_err(|e| format!("{e:#}"))
                        },
                        Msg::MuralCarregado,
                    );
                    return Task::batch([mural, self.pede_capas()]);
                }
                return self.pede_capas();
            }
            Msg::MuralCarregado(r) => {
                self.ver_perfil.carregando = false;
                match r {
                    Ok(recados) => {
                        self.ver_perfil.recados = recados;
                        self.ver_perfil.erro = None;
                    }
                    Err(e) => self.ver_perfil.erro = Some(e),
                }
            }
            Msg::RascunhoRecado(v) => {
                self.ver_perfil.rascunho = v.chars().take(200).collect();
            }
            Msg::EnviaRecado => {
                let texto = self.ver_perfil.rascunho.trim().to_string();
                if texto.is_empty() {
                    return Task::none();
                }
                if let (Some(token), Some(p)) = (&self.token, &self.modal_outro_perfil) {
                    self.ver_perfil.rascunho.clear();
                    let (srv, tok, usuario) = (self.srv.clone(), token.clone(), p.usuario.clone());
                    return em_segundo_plano(
                        move || {
                            api::mural_envia(&srv, &tok, &usuario, &texto)
                                .map_err(|e| format!("{e:#}"))
                        },
                        Msg::MuralCarregado,
                    );
                }
            }
            Msg::ApagaRecado(id) => {
                if let (Some(token), Some(p)) = (&self.token, &self.modal_outro_perfil) {
                    let (srv, tok, usuario) = (self.srv.clone(), token.clone(), p.usuario.clone());
                    return em_segundo_plano(
                        move || {
                            api::mural_apaga(&srv, &tok, &usuario, &id)
                                .map_err(|e| format!("{e:#}"))
                        },
                        Msg::MuralCarregado,
                    );
                }
            }
            Msg::PerfilCarregado(Err(e)) => {
                log::warn!("Falha ao carregar perfil: {e}");
                self.erro_global = Some(format!("Não consegui abrir o perfil: {e}"));
            }
            Msg::RascunhoNomePerfil(v) => {
                if let Some(ed) = &mut self.modal_meu_perfil {
                    ed.nome = v;
                }
            }
            Msg::RascunhoBioPerfil(v) => {
                if let Some(ed) = &mut self.modal_meu_perfil {
                    ed.bio = v;
                }
            }
            Msg::EscolheCorPerfil(cor) => {
                if let Some(ed) = &mut self.modal_meu_perfil {
                    ed.cor = if cor.is_empty() { None } else { Some(cor) };
                }
            }
            Msg::SalvaMeuPerfil => {
                if let Some(ed) = &mut self.modal_meu_perfil {
                    ed.salvando = true;
                    ed.erro = None;
                    let srv = self.srv.clone();
                    let token = self.token.clone().unwrap_or_default();
                    let nome = ed.nome.clone();
                    let bio = ed.bio.clone();
                    let cor = ed.cor.clone();
                    return em_segundo_plano(
                        move || {
                            api::atualiza_perfil(
                                &srv,
                                &token,
                                Some(&nome),
                                Some(&bio),
                                cor.as_deref(),
                                None,
                            )
                            .map_err(|e| format!("{e:#}"))
                        },
                        Msg::PerfilSalvo,
                    );
                }
            }
            Msg::PerfilSalvo(Ok(())) => {
                if let Some(ed) = &mut self.modal_meu_perfil {
                    ed.salvando = false;
                    let novo_nome = ed.nome.clone();
                    let nova_cor = ed.cor.clone();
                    let nova_bio = ed.bio.clone();
                    if let Some(eu) = &mut self.eu {
                        eu.name = novo_nome;
                        let p = eu.perfil.get_or_insert_with(Default::default);
                        p.bio = Some(nova_bio);
                        p.cor = nova_cor.clone();
                    }
                    if let Tela::Sala(s) = &self.tela {
                        s.manda(sala::Comando::AtualizaPerfilSala {
                            cor: nova_cor,
                            cor_nome: None,
                            efeito_nome: None,
                        });
                    }
                }
                self.modal_meu_perfil = None;
            }
            Msg::PerfilSalvo(Err(e)) => {
                if let Some(ed) = &mut self.modal_meu_perfil {
                    ed.salvando = false;
                    ed.erro = Some(e);
                }
            }
            Msg::EscolheArquivoAvatar => {
                return Task::perform(
                    async {
                        let handle = rfd::AsyncFileDialog::new()
                            .add_filter("Imagens", &["png", "jpg", "jpeg", "webp", "gif"])
                            .pick_file()
                            .await?;
                        let bytes = handle.read().await;
                        Some(bytes)
                    },
                    |res| match res {
                        Some(bytes) => Msg::UploadAvatar(bytes),
                        None => Msg::Nenhum,
                    },
                );
            }
            Msg::UploadAvatar(bytes) => {
                if let Some(token) = &self.token {
                    let srv = self.srv.clone();
                    let tok = token.clone();
                    return em_segundo_plano(
                        move || api::envia_avatar(&srv, &tok, &bytes).map_err(|e| format!("{e:#}")),
                        Msg::AvatarAtualizado,
                    );
                }
            }
            Msg::AvatarAtualizado(Ok(url)) => {
                if let Some(eu) = &mut self.eu {
                    eu.avatar = if url.is_empty() { None } else { Some(url) };
                }
            }
            Msg::AvatarAtualizado(Err(e)) => {
                if let Some(ed) = &mut self.modal_meu_perfil {
                    ed.erro = Some(format!("Falha no avatar: {e}"));
                }
            }
            Msg::RemoveAvatar => {
                if let Some(token) = &self.token {
                    let srv = self.srv.clone();
                    let tok = token.clone();
                    return em_segundo_plano(
                        move || api::remove_avatar(&srv, &tok).map_err(|e| format!("{e:#}")),
                        |res| match res {
                            Ok(()) => Msg::AvatarAtualizado(Ok(String::new())),
                            Err(e) => Msg::AvatarAtualizado(Err(e)),
                        },
                    );
                }
            }
            Msg::EscolheArquivoBanner => {
                return Task::perform(
                    async {
                        let handle = rfd::AsyncFileDialog::new()
                            .add_filter("Imagens e GIFs", &["png", "jpg", "jpeg", "gif", "webp"])
                            .pick_file()
                            .await?;
                        let bytes = handle.read().await;
                        Some(bytes)
                    },
                    |res| match res {
                        Some(bytes) => Msg::UploadBanner(bytes),
                        None => Msg::Nenhum,
                    },
                );
            }
            Msg::UploadBanner(bytes) => {
                if let Some(token) = &self.token {
                    let srv = self.srv.clone();
                    let tok = token.clone();
                    return em_segundo_plano(
                        move || api::envia_banner(&srv, &tok, &bytes).map_err(|e| format!("{e:#}")),
                        Msg::BannerAtualizado,
                    );
                }
            }
            Msg::BannerAtualizado(Ok(url)) => {
                if let Some(eu) = &mut self.eu {
                    eu.banner_url = if url.is_empty() { None } else { Some(url) };
                }
            }
            Msg::BannerAtualizado(Err(e)) => {
                if let Some(ed) = &mut self.modal_meu_perfil {
                    ed.erro = Some(format!("Falha no banner: {e}"));
                }
            }
            Msg::RemoveBanner => {
                if let Some(token) = &self.token {
                    let srv = self.srv.clone();
                    let tok = token.clone();
                    return em_segundo_plano(
                        move || api::remove_banner(&srv, &tok).map_err(|e| format!("{e:#}")),
                        |res| match res {
                            Ok(()) => Msg::BannerAtualizado(Ok(String::new())),
                            Err(e) => Msg::BannerAtualizado(Err(e)),
                        },
                    );
                }
            }
            Msg::TrocaAbaPerfil(aba) => {
                if let Some(ed) = &mut self.modal_meu_perfil {
                    ed.aba = aba;
                    ed.erro = None;
                    ed.sucesso = None;
                }
            }
            Msg::RascunhoSenhaAtual(v) => {
                if let Some(ed) = &mut self.modal_meu_perfil {
                    ed.senha_atual = v;
                }
            }
            Msg::RascunhoSenhaNova(v) => {
                if let Some(ed) = &mut self.modal_meu_perfil {
                    ed.senha_nova = v;
                }
            }
            Msg::RascunhoSenhaNovaConfirma(v) => {
                if let Some(ed) = &mut self.modal_meu_perfil {
                    ed.senha_nova_confirma = v;
                }
            }
            Msg::SubmeteTrocaSenha => {
                if let Some(ed) = &mut self.modal_meu_perfil {
                    if ed.senha_nova.len() < 8 {
                        ed.erro = Some("A nova senha precisa ter pelo menos 8 caracteres.".into());
                        return Task::none();
                    }
                    if ed.senha_nova != ed.senha_nova_confirma {
                        ed.erro = Some("A confirmação da nova senha não confere.".into());
                        return Task::none();
                    }
                    ed.trocando_senha = true;
                    ed.erro = None;
                    ed.sucesso = None;
                    let srv = self.srv.clone();
                    let token = self.token.clone().unwrap_or_default();
                    let atual = ed.senha_atual.clone();
                    let nova = ed.senha_nova.clone();
                    return em_segundo_plano(
                        move || {
                            api::troca_senha(&srv, &token, &atual, &nova)
                                .map_err(|e| format!("{e:#}"))
                        },
                        Msg::SenhaTrocada,
                    );
                }
            }
            Msg::SenhaTrocada(Ok(())) => {
                if let Some(ed) = &mut self.modal_meu_perfil {
                    ed.trocando_senha = false;
                    ed.senha_atual.clear();
                    ed.senha_nova.clear();
                    ed.senha_nova_confirma.clear();
                    ed.sucesso = Some(
                        "Senha alterada com sucesso! As outras sessões foram desconectadas.".into(),
                    );
                    ed.erro = None;
                }
            }
            Msg::SenhaTrocada(Err(e)) => {
                if let Some(ed) = &mut self.modal_meu_perfil {
                    ed.trocando_senha = false;
                    ed.erro = Some(e);
                    ed.sucesso = None;
                }
            }
            Msg::IniciaConfirmacaoApagarConta => {
                if let Some(ed) = &mut self.modal_meu_perfil {
                    ed.confirma_apagar = true;
                    ed.senha_apagar.clear();
                    ed.erro = None;
                }
            }
            Msg::CancelaConfirmacaoApagarConta => {
                if let Some(ed) = &mut self.modal_meu_perfil {
                    ed.confirma_apagar = false;
                    ed.senha_apagar.clear();
                }
            }
            Msg::RascunhoSenhaApagarConta(v) => {
                if let Some(ed) = &mut self.modal_meu_perfil {
                    ed.senha_apagar = v;
                }
            }
            Msg::SubmeteApagarConta => {
                if let Some(ed) = &mut self.modal_meu_perfil {
                    ed.apagando = true;
                    ed.erro = None;
                    let srv = self.srv.clone();
                    let token = self.token.clone().unwrap_or_default();
                    let senha = ed.senha_apagar.clone();
                    return em_segundo_plano(
                        move || {
                            api::apaga_conta(&srv, &token, &senha).map_err(|e| format!("{e:#}"))
                        },
                        Msg::ContaApagada,
                    );
                }
            }
            Msg::ContaApagada(Ok(())) => {
                self.modal_meu_perfil = None;
                return Task::done(Msg::Deslogar);
            }
            Msg::ContaApagada(Err(e)) => {
                if let Some(ed) = &mut self.modal_meu_perfil {
                    ed.apagando = false;
                    ed.erro = Some(e);
                }
            }
            Msg::DefineStatus(st) => {
                self.menu_status_aberto = false;
                let st_valida = match st.as_str() {
                    "online" | "ausente" | "ocupado" | "invisivel" => st,
                    _ => "online".to_string(),
                };
                if let Some(eu) = &mut self.eu {
                    let p = eu.perfil.get_or_insert_with(Default::default);
                    p.status = Some(st_valida.clone());
                }
                if let Some(ed) = &mut self.modal_meu_perfil {
                    ed.status = st_valida.clone();
                }
                if let Tela::Sala(s) = &mut self.tela {
                    s.manda(sala::Comando::TrocarStatus(st_valida.clone()));
                }
                if let Some(token) = self.token.clone() {
                    let srv = self.srv.clone();
                    let st_env = st_valida;
                    return em_segundo_plano(
                        move || {
                            api::define_status(&srv, &token, &st_env).map_err(|e| format!("{e:#}"))
                        },
                        Msg::StatusDefinido,
                    );
                }
            }
            Msg::StatusDefinido(Err(e)) => {
                eprintln!("[status] erro ao salvar status no servidor: {e}");
            }
            Msg::StatusDefinido(Ok(())) => {}
            Msg::FiltraConversas(v) => self.filtro_conversas = v,
            Msg::PortalGlobalResultado(res) => {
                self.portal_global_ativo = res.is_ok();
                if let Tela::Sala(s) = &mut self.tela {
                    return s.update(Msg::PortalGlobalResultado(res));
                }
            }
            Msg::PortalAtalho { acao, pressionado } => match (acao.as_str(), pressionado) {
                ("ptt" | "push_to_talk", p) => return Task::done(Msg::PttTransmitindo(p)),
                ("mute", true)
                    if (matches!(&self.tela, Tela::Sala(s) if s.na_chamada)
                        || self.chamada_ativa.is_some()) =>
                {
                    return Task::done(Msg::AlternaMudo);
                }
                ("deaf", true)
                    if (matches!(&self.tela, Tela::Sala(s) if s.na_chamada)
                        || self.chamada_ativa.is_some()) =>
                {
                    return Task::done(Msg::AlternaSurdo);
                }
                ("answer", true) => {
                    if let Some(t) = self.avisos.toques.first() {
                        return Task::done(Msg::AtendeToque(t.conversa.clone()));
                    }
                }
                ("hangup", true)
                    if (matches!(&self.tela, Tela::Sala(s) if s.na_chamada)
                        || self.chamada_ativa.is_some()) =>
                {
                    return Task::done(Msg::SaiDaChamada);
                }
                _ => {}
            },
            Msg::CapturaAtalho(acao) => {
                self.aviso_atalho = None;
                self.capturando_atalho = if self.capturando_atalho.as_deref() == Some(&acao) {
                    None
                } else {
                    Some(acao)
                };
            }
            Msg::RestauraAtalho(acao) => {
                self.atalhos.restaura(&acao);
                self.atalhos.salva();
                self.aviso_atalho = None;
            }
            Msg::AlternaNotificaMensagens => {
                self.prefs_conversas.notificar_mensagens =
                    !self.prefs_conversas.notificar_mensagens;
                self.prefs_conversas.salva();
            }
            Msg::AlternaNotificaMencoes => {
                self.prefs_conversas.notificar_mencoes = !self.prefs_conversas.notificar_mencoes;
                self.prefs_conversas.salva();
            }
            Msg::AlternaEnviarRelatorio => {
                crate::relatorio::define_enviar(!crate::relatorio::enviar_ligado());
            }
            Msg::AlternaAutostart => {
                crate::ui::configuracoes::alterna_autostart();
            }
            Msg::JogosRegFiltro(t) => self.jogos_reg_filtro = t,
            Msg::JogosRegAtualiza => {
                self.jogos_reg = jogos_registrados::carrega();
                self.processos_abertos = jogos_registrados::processos_abertos();
                return self.pede_capas();
            }
            Msg::JogosRegAdiciona(exec) => {
                // Procura o nome oficial e a capa; sem token, fica o nome do programa.
                let sugerido = jogos_registrados::nome_sugerido(&exec);
                let (srv, token) = (self.srv.clone(), self.token.clone());
                return em_segundo_plano(
                    move || {
                        let achado = token.and_then(|t| api::busca_jogos(&srv, &t, &sugerido).ok());
                        let melhor = achado
                            .as_deref()
                            .and_then(|c| jogos_registrados::melhor_parecido(&sugerido, c));
                        match melhor {
                            Some(j) => jogos_registrados::adiciona_manual(
                                &exec,
                                j.nome.clone(),
                                Some(j.id),
                            ),
                            None => jogos_registrados::adiciona_manual(&exec, sugerido, None),
                        }
                    },
                    |_| Msg::JogosRegAdicionado,
                );
            }
            Msg::JogosRegAdicionado => return Task::done(Msg::JogosRegAtualiza),
            Msg::JogosRegRemove(nome) => {
                jogos_registrados::remove(&nome);
                return Task::done(Msg::JogosRegAtualiza);
            }
            Msg::JogosRegIniciaRenomear { exec, nome_atual } => {
                self.jogos_reg_renomeando = Some((exec, nome_atual));
            }
            Msg::JogosRegRascunhoNome(n) => {
                if let Some((_, r)) = &mut self.jogos_reg_renomeando {
                    *r = n;
                }
            }
            Msg::JogosRegCancelaRenomear => {
                self.jogos_reg_renomeando = None;
            }
            Msg::JogosRegSalvaNome => {
                if let Some((exec, novo_nome)) = self.jogos_reg_renomeando.take() {
                    let novo_nome = novo_nome.trim().to_string();
                    if !novo_nome.is_empty() {
                        let (srv, token) = (self.srv.clone(), self.token.clone());
                        let nome_busca = novo_nome.clone();
                        let exec_c = exec.clone();
                        return em_segundo_plano(
                            move || {
                                let achado = token
                                    .as_ref()
                                    .and_then(|t| api::busca_jogos(&srv, t, &nome_busca).ok());
                                let melhor = achado.as_deref().and_then(|c| {
                                    jogos_registrados::melhor_parecido(&nome_busca, c)
                                });
                                match melhor {
                                    Some(j) => jogos_registrados::renomeia(
                                        &exec_c,
                                        j.nome.clone(),
                                        Some(j.id),
                                    ),
                                    None => jogos_registrados::renomeia(&exec_c, nome_busca, None),
                                }
                            },
                            |_| Msg::JogosRegAdicionado,
                        );
                    }
                }
            }
            Msg::JogosRegAlternaAtivo(exec_ou_nome) => {
                jogos_registrados::alterna_ativo(&exec_ou_nome);
                self.jogos_reg = jogos_registrados::carrega();
                return Task::done(Msg::VarreAtividade);
            }
            Msg::JogosRegAlternaSeletor => {
                self.jogos_reg_seletor_aberto = !self.jogos_reg_seletor_aberto;
                if self.jogos_reg_seletor_aberto && self.processos_abertos.is_empty() {
                    self.processos_abertos = jogos_registrados::processos_abertos();
                }
            }
            Msg::AbreVisaoJogo(j) => {
                self.modal_visao_jogo = Some(j);
            }
            Msg::FechaVisaoJogo => {
                self.modal_visao_jogo = None;
            }
            Msg::AbreFoto { url, titulo } => {
                let cmd = if avatar::precisa_baixar(&url) {
                    tarefas::baixa_imagem_de_perfil(
                        self.srv.clone(),
                        self.token.clone(),
                        url.clone(),
                    )
                } else {
                    Task::none()
                };
                self.modal_foto = Some(ModalFoto {
                    url,
                    titulo,
                    zoom: 1.0,
                });
                return cmd;
            }
            Msg::FechaFoto => {
                self.modal_foto = None;
            }
            Msg::ZoomFoto(delta) => {
                if let Some(f) = &mut self.modal_foto {
                    f.zoom = (f.zoom + delta).clamp(0.5, 4.0);
                }
            }
            Msg::ResetaZoomFoto => {
                if let Some(f) = &mut self.modal_foto {
                    f.zoom = 1.0;
                }
            }
            Msg::AlternaSomDoToque => {
                self.prefs_conversas.som_do_toque = !self.prefs_conversas.som_do_toque;
                self.prefs_conversas.salva();
                if !self.prefs_conversas.som_do_toque {
                    self.toque_sonoro = None;
                }
            }
            Msg::DefineVolumeToque(v) => {
                self.prefs_conversas.volume_toque = v.min(100);
                self.prefs_conversas.salva();
            }
            Msg::DefineVolumeEfeitos(v) => {
                let v = v.min(100);
                self.prefs_conversas.volume_efeitos = v;
                self.prefs_conversas.salva();
                if let Tela::Sala(s) = &mut self.tela {
                    s.soundboard.volume = v;
                }
            }
            Msg::TestaToque => {
                let vol = self.prefs_conversas.volume_toque;
                std::thread::spawn(move || {
                    let mut wav = som::sintetiza_toque();
                    som::aplica_volume(&mut wav, vol);
                    som::reproduz_wav(&wav);
                });
            }
            Msg::RecusaToque(id) => {
                if let Some(tx) = &self.avisos_cmd {
                    let _ = tx.send(avisos::Comando::Recusar(id.clone()));
                }
                self.avisos.toques.retain(|t| t.conversa != id);
                if self.avisos.toques.is_empty() {
                    self.toque_sonoro = None;
                }
            }
            Msg::AtendeToque(id) => {
                self.avisos.toques.retain(|t| t.conversa != id);
                if self.avisos.toques.is_empty() {
                    self.toque_sonoro = None;
                }
                // Já na sala da conversa: só entra na chamada.
                if matches!(&self.tela, Tela::Sala(s) if s.entrada.sala == id) {
                    return Task::done(Msg::LigarConversa);
                }
                self.ligar_ao_abrir = Some(id.clone());
                return Task::done(Msg::AbreSala(id));
            }
            Msg::AbreDm { uid, nome, ligar } => {
                let Some(token) = self.token.clone() else {
                    return Task::none();
                };
                let srv = self.srv.clone();
                return em_segundo_plano(
                    move || api::abre_dm(&srv, &token, &uid).map_err(|e| format!("{e:#}")),
                    move |r| match r {
                        Ok(id) => Msg::DmPronta {
                            id,
                            nome: nome.clone(),
                            ligar,
                            erro: None,
                        },
                        Err(e) => Msg::DmPronta {
                            id: String::new(),
                            nome: nome.clone(),
                            ligar,
                            erro: Some(e),
                        },
                    },
                );
            }
            Msg::DmPronta {
                id,
                nome,
                ligar,
                erro,
            } => {
                if let Some(e) = erro {
                    self.erro_global = Some(format!("Não deu pra abrir a conversa: {e}"));
                    return Task::none();
                }
                self.modal_outro_perfil = None;
                self.titulos_extra.insert(id.clone(), nome);
                if ligar {
                    self.ligar_ao_abrir = Some(id.clone());
                }
                return Task::done(Msg::AbreSala(id));
            }
            Msg::FechaErroGlobal => self.erro_global = None,
            Msg::AlternaMenuStatus => self.menu_status_aberto = !self.menu_status_aberto,
            Msg::TickChamada => {
                self.menu_contexto = None;
            }
            Msg::AbreEditarGrupo(id) => {
                self.menu_contexto = None;
                if let Some(c) = self
                    .avisos
                    .conversas
                    .iter()
                    .find(|c| c.id == id && c.tipo == "grupo")
                {
                    self.modal_editar_grupo = Some(ui::editar_grupo::EstadoEditarGrupo::novo(c));
                }
            }
            Msg::EditaGrupoNome(v) => {
                if let Some(m) = &mut self.modal_editar_grupo {
                    m.nome = v.chars().take(60).collect();
                }
            }
            Msg::EditaGrupoPasso(p) => {
                if let Some(m) = &mut self.modal_editar_grupo {
                    m.passo = p;
                    m.erro = None;
                }
            }
            Msg::EditaGrupoEscolheImagem => {
                return Task::perform(
                    async {
                        let h = rfd::AsyncFileDialog::new()
                            .add_filter("Imagens", &["png", "jpg", "jpeg", "gif", "webp"])
                            .pick_file()
                            .await?;
                        Some(h.read().await)
                    },
                    Msg::EditaGrupoImagemEscolhida,
                );
            }
            Msg::EditaGrupoImagemEscolhida(bytes) => {
                if let (Some(m), Some(b)) = (&mut self.modal_editar_grupo, bytes) {
                    if b.len() > 5 * 1024 * 1024 {
                        m.erro = Some("A imagem passa de 5 MB.".into());
                    } else {
                        m.previa_imagem = Some(iced::widget::image::Handle::from_bytes(b.clone()));
                        m.novo_icone = Some(ui::editar_grupo::IconeNovo::Imagem(b));
                        m.passo = ui::editar_grupo::Passo::Principal;
                        m.erro = None;
                    }
                }
            }
            Msg::EditaGrupoBuscaInput(v) => {
                if let Some(m) = &mut self.modal_editar_grupo {
                    m.busca_gif = v;
                }
            }
            Msg::EditaGrupoBusca => {
                let (Some(m), Some(token)) = (&mut self.modal_editar_grupo, self.token.clone())
                else {
                    return Task::none();
                };
                m.buscando_gif = true;
                m.erro = None;
                let (srv, termo) = (self.srv.clone(), m.busca_gif.clone());
                return em_segundo_plano(
                    move || api::busca_gifs(&srv, &token, &termo).map_err(|e| format!("{e:#}")),
                    Msg::EditaGrupoGifs,
                );
            }
            Msg::EditaGrupoGifs(r) => {
                if let Some(m) = &mut self.modal_editar_grupo {
                    m.buscando_gif = false;
                    match r {
                        Ok(l) => m.resultados_gif = l,
                        Err(e) => m.erro = Some(e),
                    }
                }
            }
            Msg::EditaGrupoEscolheGif(url) => {
                if let Some(m) = &mut self.modal_editar_grupo {
                    m.novo_icone = Some(ui::editar_grupo::IconeNovo::Gif(url));
                    m.passo = ui::editar_grupo::Passo::Principal;
                }
            }
            Msg::EditaGrupoRemoveIcone => {
                if let Some(m) = &mut self.modal_editar_grupo {
                    m.novo_icone = Some(ui::editar_grupo::IconeNovo::Remover);
                    m.previa_imagem = None;
                }
            }
            Msg::EditaGrupoCancela => self.modal_editar_grupo = None,
            Msg::EditaGrupoSalva => {
                let (Some(m), Some(token)) = (&mut self.modal_editar_grupo, self.token.clone())
                else {
                    return Task::none();
                };
                if !m.mudou() || m.enviando {
                    return Task::none();
                }
                m.enviando = true;
                m.erro = None;
                let srv = self.srv.clone();
                let conversa = m.conversa.clone();
                let nome = m.nome.trim().to_string();
                let nome = (!nome.is_empty() && nome != m.nome_original).then_some(nome);
                let icone = m.novo_icone.clone();
                return em_segundo_plano(
                    move || {
                        api::salva_grupo(&srv, &token, &conversa, nome.as_deref(), icone.as_ref())
                            .map_err(|e| format!("{e:#}"))
                    },
                    Msg::EditaGrupoSalvo,
                );
            }
            Msg::EditaGrupoSalvo(r) => match r {
                Ok(()) => self.modal_editar_grupo = None,
                Err(e) => {
                    if let Some(m) = &mut self.modal_editar_grupo {
                        m.enviando = false;
                        m.erro = Some(e);
                    }
                }
            },
            Msg::AbreCriarGrupo(marcado) => {
                self.menu_contexto = None;
                self.modal_grupo = Some(ui::grupo::EstadoModalGrupo::novo(
                    ui::grupo::ModoGrupo::Criar,
                    marcado,
                ));
                return Task::done(Msg::CarregaAmigos);
            }
            Msg::AbreAdicionarPessoas(id) => {
                self.menu_contexto = None;
                self.modal_grupo = Some(ui::grupo::EstadoModalGrupo::novo(
                    ui::grupo::ModoGrupo::Adicionar(id),
                    None,
                ));
                return Task::done(Msg::CarregaAmigos);
            }
            Msg::GrupoBusca(v) => {
                if let Some(m) = &mut self.modal_grupo {
                    m.busca = v;
                }
            }
            Msg::GrupoAlterna(uid) => {
                if let Some(m) = &mut self.modal_grupo {
                    m.alterna(&uid);
                }
            }
            Msg::GrupoCancela => self.modal_grupo = None,
            Msg::GrupoConfirma => {
                let (Some(m), Some(token)) = (&mut self.modal_grupo, self.token.clone()) else {
                    return Task::none();
                };
                if m.marcados.is_empty() || m.enviando {
                    return Task::none();
                }
                m.enviando = true;
                m.erro = None;
                let srv = self.srv.clone();
                let uids = m.marcados.clone();
                return match m.modo.clone() {
                    ui::grupo::ModoGrupo::Criar => em_segundo_plano(
                        move || {
                            api::cria_grupo(&srv, &token, &uids)
                                .map(Some)
                                .map_err(|e| format!("{e:#}"))
                        },
                        Msg::GrupoResultado,
                    ),
                    ui::grupo::ModoGrupo::Adicionar(conv) => em_segundo_plano(
                        move || {
                            api::adiciona_membros(&srv, &token, &conv, &uids)
                                .map(|_| None)
                                .map_err(|e| format!("{e:#}"))
                        },
                        Msg::GrupoResultado,
                    ),
                };
            }
            Msg::GrupoResultado(r) => match r {
                Ok(criado) => {
                    self.modal_grupo = None;
                    if let Some((id, nome)) = criado {
                        self.titulos_extra.insert(id.clone(), nome);
                        return Task::done(Msg::AbreSala(id));
                    }
                }
                Err(e) => {
                    if let Some(m) = &mut self.modal_grupo {
                        m.enviando = false;
                        m.erro = Some(e);
                    }
                }
            },
            Msg::AbreMenuPessoa { uid, usuario, nome } => {
                self.menu_contexto = Some(MenuContexto {
                    alvo: AlvoMenu::Pessoa { uid, usuario, nome },
                    x: self.cursor.x,
                    y: self.cursor.y,
                });
            }
            Msg::AbreMenuTransmissao(dono, de_tela) => {
                self.menu_contexto = Some(MenuContexto {
                    alvo: AlvoMenu::Transmissao(dono, de_tela),
                    x: self.cursor.x,
                    y: self.cursor.y,
                });
            }
            Msg::HoverGif(url) => crate::gifanim::define_hover(url),
            Msg::MinhasListas(Ok(p)) => {
                if let Some(ed) = &mut self.modal_meu_perfil {
                    ed.jogos.listas = p.listas.clone();
                }
                return self.pede_capas();
            }
            Msg::MinhasListas(Err(_)) => {}
            Msg::JogosBuscaInput(t) => {
                if let Some(ed) = &mut self.modal_meu_perfil {
                    ed.jogos.busca = t;
                }
            }
            Msg::JogosBusca => {
                let (Some(ed), Some(token)) = (&mut self.modal_meu_perfil, self.token.clone())
                else {
                    return Task::none();
                };
                let termo = ed.jogos.busca.trim().to_string();
                if termo.len() < 2 {
                    return Task::none();
                }
                ed.jogos.buscando = true;
                ed.jogos.erro = None;
                let srv = self.srv.clone();
                return em_segundo_plano(
                    move || api::busca_jogos(&srv, &token, &termo).map_err(|e| format!("{e:#}")),
                    Msg::JogosResultado,
                );
            }
            Msg::JogosResultado(r) => {
                if let Some(ed) = &mut self.modal_meu_perfil {
                    ed.jogos.buscando = false;
                    match r {
                        Ok(l) => {
                            if l.is_empty() {
                                ed.jogos.erro = Some("Nenhum jogo com esse nome.".into());
                            }
                            ed.jogos.resultados = l;
                        }
                        Err(e) => ed.jogos.erro = Some(e),
                    }
                }
            }
            Msg::JogoAdiciona(jogo, lista) => {
                let Some(ed) = &mut self.modal_meu_perfil else {
                    return Task::none();
                };
                if let Err(e) = ed.jogos.adiciona(jogo, lista) {
                    ed.jogos.erro = Some(e.into());
                    return Task::none();
                }
                ed.jogos.erro = None;
                return Task::batch([self.salva_listas(), self.pede_capas()]);
            }
            Msg::JogoRemove(lista, id) => {
                if let Some(ed) = &mut self.modal_meu_perfil {
                    ed.jogos.remove(lista, id);
                }
                return self.salva_listas();
            }
            Msg::JogosSalvos(r) => {
                if let (Err(e), Some(ed)) = (r, &mut self.modal_meu_perfil) {
                    ed.jogos.erro = Some(format!("Não consegui salvar: {e}"));
                }
            }
            Msg::AbreEditarPerfil => {
                self.modal_outro_perfil = None;
                self.menu_status_aberto = false;
                self.menu_contexto = None;
                self.editar_perfil_janela = true;
                return Task::done(Msg::AbreConfiguracoes(Some(
                    ui::configuracoes::AbaConfig::Perfil,
                )));
            }
            Msg::TickGif => {}
            Msg::AlternaReduzAnimacoes => {
                let mut c = crate::desempenho::atual();
                c.reduz_animacoes = !c.reduz_animacoes;
                crate::desempenho::salva(c);
            }
            Msg::AlternaGifSoHover => {
                let mut c = crate::desempenho::atual();
                c.gif_so_hover = !c.gif_so_hover;
                crate::desempenho::salva(c);
            }
            Msg::AbrePrevia { uid, nome, app } => {
                self.menu_contexto = Some(MenuContexto {
                    alvo: AlvoMenu::Previa { uid, nome, app },
                    x: self.cursor.x,
                    y: self.cursor.y,
                });
            }
            Msg::AbreMenuConversa(id) => {
                self.menu_contexto = Some(MenuContexto {
                    alvo: AlvoMenu::Conversa(id),
                    x: self.cursor.x,
                    y: self.cursor.y,
                });
            }
            Msg::AbreMenuMensagem(id) => {
                self.menu_contexto = Some(MenuContexto {
                    alvo: AlvoMenu::Mensagem(id),
                    x: self.cursor.x,
                    y: self.cursor.y,
                });
            }
            Msg::AbreMenuMidia(avatar) => {
                self.menu_contexto = Some(MenuContexto {
                    alvo: AlvoMenu::Midia(avatar),
                    x: self.cursor.x,
                    y: self.cursor.y,
                });
            }
            Msg::TrocaPreviaPerfil(atividade) => {
                if let Some(m) = &mut self.modal_meu_perfil {
                    m.previa_atividade = atividade;
                }
            }
            Msg::VolumeSistema(mic, v) => {
                if mic {
                    self.volume_mic = Some(v);
                } else {
                    self.volume_saida = Some(v);
                }
            }
            Msg::AplicaVolumeSistema(mic) => {
                let v = if mic {
                    self.volume_mic
                } else {
                    self.volume_saida
                };
                if let Some(v) = v {
                    std::thread::spawn(move || sistema_audio::define_volume(mic, v));
                }
            }
            Msg::VolumesSistemaLidos(mic, saida) => {
                self.volume_mic = mic;
                self.volume_saida = saida;
            }
            Msg::AlternaPresenca => self.presenca_aberta = !self.presenca_aberta,
            Msg::PresencaHover(true) => {
                self.presenca_aberta = true;
                self.presenca_fecha_em = None;
            }
            Msg::PresencaHover(false) => {
                self.presenca_fecha_em =
                    Some(std::time::Instant::now() + std::time::Duration::from_millis(250));
            }
            Msg::PresencaTick => {
                if self
                    .presenca_fecha_em
                    .is_some_and(|t| std::time::Instant::now() >= t)
                {
                    self.presenca_aberta = false;
                    self.presenca_fecha_em = None;
                }
            }
            Msg::AbreStatusCustom => {
                self.menu_status_aberto = false;
                let atual = self
                    .eu
                    .as_ref()
                    .and_then(|e| e.perfil.as_ref())
                    .and_then(|p| p.status_custom.as_ref())
                    .filter(|s| !s.vencido());
                self.modal_status_custom = Some(ui::status_custom::EstadoStatusCustom::novo(atual));
            }
            Msg::FechaStatusCustom => self.modal_status_custom = None,
            Msg::StatusCustomTexto(t) => {
                if let Some(m) = &mut self.modal_status_custom {
                    m.texto = t.chars().take(60).collect();
                }
            }
            Msg::StatusCustomEmoji(e) => {
                if let Some(m) = &mut self.modal_status_custom {
                    m.emoji = e;
                    m.emojis_abertos = false;
                }
            }
            Msg::StatusCustomAlternaEmojis => {
                if let Some(m) = &mut self.modal_status_custom {
                    m.emojis_abertos = !m.emojis_abertos;
                    m.prazos_abertos = false;
                }
            }
            Msg::StatusCustomAlternaPrazos => {
                if let Some(m) = &mut self.modal_status_custom {
                    m.prazos_abertos = !m.prazos_abertos;
                    m.emojis_abertos = false;
                }
            }
            Msg::StatusCustomPrazo(p) => {
                if let Some(m) = &mut self.modal_status_custom {
                    m.minutos = p;
                    m.prazos_abertos = false;
                }
            }
            Msg::LimpaStatusCustom => {
                if let Some(m) = &mut self.modal_status_custom {
                    m.texto.clear();
                    m.emoji.clear();
                }
                return Task::done(Msg::SalvaStatusCustom);
            }
            Msg::SalvaStatusCustom => {
                let (Some(m), Some(token)) = (&mut self.modal_status_custom, self.token.clone())
                else {
                    return Task::none();
                };
                if m.salvando {
                    return Task::none();
                }
                m.salvando = true;
                m.erro = None;
                let srv = self.srv.clone();
                let (texto, emoji, minutos) = (m.texto.clone(), m.emoji.clone(), m.minutos);
                return em_segundo_plano(
                    move || {
                        api::define_status_custom(&srv, &token, &texto, &emoji, minutos)
                            .map_err(|e| format!("{e:#}"))
                    },
                    Msg::StatusCustomSalvo,
                );
            }
            Msg::StatusCustomSalvo(r) => match r {
                Ok(novo) => {
                    self.modal_status_custom = None;
                    if let Some(p) = self.eu.as_mut().and_then(|e| e.perfil.as_mut()) {
                        p.status_custom = novo;
                    }
                }
                Err(e) => {
                    if let Some(m) = &mut self.modal_status_custom {
                        m.salvando = false;
                        m.erro = Some(e);
                    }
                }
            },
            Msg::AbreEncaminhar(id) => {
                self.menu_contexto = None;
                if let Tela::Sala(s) = &self.tela
                    && let Some(m) = s.mensagens.iter().find(|m| m.id == id)
                {
                    {
                        let resumo = if !m.texto.is_empty() {
                            ui::chat::trunca_trecho(&m.texto, 80)
                        } else if m.imagem.is_some() {
                            "1 imagem".to_string()
                        } else if m.gif.is_some() {
                            "1 GIF".to_string()
                        } else {
                            "Mensagem".to_string()
                        };
                        self.modal_encaminhar = Some(ui::encaminhar::EstadoEncaminhar::novo(
                            s.entrada.sala.clone(),
                            id,
                            resumo,
                        ));
                    }
                }
            }
            Msg::EncaminhaBusca(b) => {
                if let Some(m) = &mut self.modal_encaminhar {
                    m.busca = b;
                }
            }
            Msg::EncaminhaAlterna(id) => {
                if let Some(m) = &mut self.modal_encaminhar {
                    m.alterna(&id);
                }
            }
            Msg::EncaminhaExtra(t) => {
                if let Some(m) = &mut self.modal_encaminhar {
                    m.extra = t;
                }
            }
            Msg::FechaEncaminhar => self.modal_encaminhar = None,
            Msg::EncaminhaEnvia => {
                let (Some(m), Some(token)) = (&mut self.modal_encaminhar, self.token.clone())
                else {
                    return Task::none();
                };
                if m.marcadas.is_empty() || m.enviando {
                    return Task::none();
                }
                m.enviando = true;
                m.erro = None;
                let srv = self.srv.clone();
                let (origem, mensagem, extra) =
                    (m.origem.clone(), m.mensagem.clone(), m.extra.clone());
                let destinos = m.marcadas.clone();
                return em_segundo_plano(
                    move || {
                        let mut feitos = 0;
                        for d in &destinos {
                            api::encaminha_mensagem(&srv, &token, d, &origem, &mensagem, &extra)
                                .map_err(|e| format!("{e:#}"))?;
                            feitos += 1;
                        }
                        Ok(feitos)
                    },
                    Msg::EncaminhaFim,
                );
            }
            Msg::EncaminhaFim(r) => match r {
                Ok(_) => {
                    self.modal_encaminhar = None;
                    return Task::done(Msg::Toast("Mensagem encaminhada!".into()));
                }
                Err(e) => {
                    if let Some(m) = &mut self.modal_encaminhar {
                        m.enviando = false;
                        m.erro = Some(e);
                    }
                }
            },
            Msg::Toast(t) => self.toast = Some((t, std::time::Instant::now())),
            Msg::ToastTick => {
                if self
                    .toast
                    .as_ref()
                    .is_some_and(|(_, ini)| ini.elapsed() > std::time::Duration::from_millis(2600))
                {
                    self.toast = None;
                }
            }
            Msg::Favorita(id) => {
                self.menu_contexto = None;
                let (Tela::Sala(s), Some(token)) = (&self.tela, self.token.clone()) else {
                    return Task::none();
                };
                let (srv, conv) = (self.srv.clone(), s.entrada.sala.clone());
                return em_segundo_plano(
                    move || {
                        api::favorita(&srv, &token, &conv, &id)
                            .map(|_| "Mensagem favoritada!".to_string())
                            .map_err(|e| format!("{e:#}"))
                    },
                    Msg::FavoritoFeito,
                );
            }
            Msg::FavoritoFeito(r) => {
                return Task::done(Msg::Toast(match r {
                    Ok(t) => t,
                    Err(e) => format!("Não consegui favoritar: {e}"),
                }));
            }
            Msg::Desfavorita { conversa, id } => {
                let Some(token) = self.token.clone() else {
                    return Task::none();
                };
                if let Some(Some(v)) = self.modal_favoritos.as_mut().map(|m| m.itens.as_mut()) {
                    v.retain(|f| !(f.conversa == conversa && f.id == id));
                }
                let srv = self.srv.clone();
                return em_segundo_plano(
                    move || {
                        api::desfavorita(&srv, &token, &conversa, &id)
                            .map(|_| "Removida dos favoritos.".to_string())
                            .map_err(|e| format!("{e:#}"))
                    },
                    Msg::FavoritoFeito,
                );
            }
            Msg::AbreFavoritos => {
                self.menu_status_aberto = false;
                self.modal_favoritos = Some(ui::favoritos::EstadoFavoritos::default());
                let Some(token) = self.token.clone() else {
                    return Task::none();
                };
                let srv = self.srv.clone();
                return em_segundo_plano(
                    move || api::lista_favoritos(&srv, &token).map_err(|e| format!("{e:#}")),
                    Msg::FavoritosCarregados,
                );
            }
            Msg::FechaFavoritos => self.modal_favoritos = None,
            Msg::FavoritosCarregados(r) => {
                if let Some(m) = &mut self.modal_favoritos {
                    match r {
                        Ok(v) => m.itens = Some(v),
                        Err(e) => {
                            m.itens = Some(Vec::new());
                            m.erro = Some(e);
                        }
                    }
                }
            }
            Msg::AbreMenuImagem(id) => {
                self.menu_contexto = Some(MenuContexto {
                    alvo: AlvoMenu::Imagem(id),
                    x: self.cursor.x,
                    y: self.cursor.y,
                });
            }
            Msg::CopiaImagem(id) => {
                self.menu_contexto = None;
                if let Some(bytes) = self.bytes_da_imagem(&id) {
                    std::thread::spawn(move || copia_imagem_para_area(&bytes));
                    return Task::done(Msg::Toast("Imagem copiada.".into()));
                }
            }
            Msg::SalvaImagem(id) => {
                self.menu_contexto = None;
                if let Some(bytes) = self.bytes_da_imagem(&id) {
                    let nome = format!("imagem.{}", extensao_da_imagem(&bytes));
                    return Task::perform(
                        async move {
                            let Some(h) = rfd::AsyncFileDialog::new()
                                .set_file_name(nome)
                                .save_file()
                                .await
                            else {
                                return Ok(false);
                            };
                            h.write(&bytes)
                                .await
                                .map(|_| true)
                                .map_err(|e| e.to_string())
                        },
                        Msg::ImagemSalva,
                    );
                }
            }
            Msg::ImagemSalva(r) => match r {
                Ok(true) => return Task::done(Msg::Toast("Imagem salva.".into())),
                Ok(false) => {}
                Err(e) => return Task::done(Msg::Toast(format!("Não consegui salvar: {e}"))),
            },
            Msg::MenuFaz(inner) => {
                self.menu_contexto = None;
                self.menu_status_aberto = false;
                return Task::done(*inner);
            }
            Msg::FechaMenuContexto => self.menu_contexto = None,
            Msg::MencionaPessoa(nome) => {
                self.menu_contexto = None;
                if let Tela::Sala(s) = &mut self.tela {
                    if !s.rascunho.is_empty() && !s.rascunho.ends_with(' ') {
                        s.rascunho.push(' ');
                    }
                    s.rascunho.push_str(&format!("@{nome} "));
                }
            }
            Msg::CopiaTexto(t) => {
                self.menu_contexto = None;
                std::thread::spawn(move || {
                    if let Ok(mut cb) = arboard::Clipboard::new() {
                        let _ = cb.set_text(t.clone());
                    }
                    let _ = std::process::Command::new("wl-copy").arg(&t).output();
                });
            }
            Msg::TocaPara { conversa, uid } => {
                self.menu_contexto = None;
                if let Some(tx) = &self.avisos_cmd {
                    let _ = tx.send(avisos::Comando::TocarDeNovo { conversa, uid });
                }
            }
            Msg::RemoveDoGrupo { conversa, uid } => {
                self.menu_contexto = None;
                let Some(token) = self.token.clone() else {
                    return Task::none();
                };
                let srv = self.srv.clone();
                let caminho = format!(
                    "/api/grupos/{}/membros/{}",
                    api::codifica_segmento(&conversa),
                    api::codifica_segmento(&uid)
                );
                return em_segundo_plano(
                    move || {
                        api::acao_de_grupo(&srv, &token, "DELETE", &caminho)
                            .map_err(|e| format!("{e:#}"))
                    },
                    Msg::GrupoAlterado,
                );
            }
            Msg::PedeApagarGrupo(id) => {
                self.menu_contexto = None;
                let nome = self
                    .avisos
                    .conversas
                    .iter()
                    .find(|c| c.id == id)
                    .map(|c| c.nome.clone())
                    .unwrap_or_default();
                self.modal_confirmacao =
                    Some(ui::confirmacao::TipoConfirmacao::ApagarGrupo { id, nome });
            }
            Msg::SaiDoGrupo(id) => {
                self.menu_contexto = None;
                // Se o grupo está aberto, sai da tela antes.
                if matches!(&self.tela, Tela::Sala(s) if s.entrada.sala == id) {
                    let _ = self.vai_pras_salas();
                }
                let Some(token) = self.token.clone() else {
                    return Task::none();
                };
                let srv = self.srv.clone();
                let caminho = format!("/api/grupos/{}/sair", api::codifica_segmento(&id));
                return em_segundo_plano(
                    move || {
                        api::acao_de_grupo(&srv, &token, "POST", &caminho)
                            .map_err(|e| format!("{e:#}"))
                    },
                    Msg::GrupoAlterado,
                );
            }
            Msg::GrupoAlterado(r) => {
                if let Err(e) = r
                    && let Tela::Salas(s) = &mut self.tela
                {
                    s.recusa = Some(e);
                }
            }
            Msg::AlternaSilenciarConversa(id) => {
                self.menu_contexto = None;
                self.prefs_conversas.alterna_silencio(&id);
                self.prefs_conversas.salva();
            }
            Msg::FechaConversa(id) => {
                self.menu_contexto = None;
                if matches!(&self.tela, Tela::Sala(s) if s.entrada.sala == id) {
                    let _ = self.vai_pras_salas();
                }
                self.prefs_conversas.fecha(&id);
                self.prefs_conversas.salva();
            }
            Msg::LigarConversa => {
                if let Tela::Sala(s) = &mut self.tela {
                    if let Some(ca) = self.chamada_ativa.take() {
                        ca.manda(sala::Comando::SairDaChamada);
                    }
                    let id = s.entrada.sala.clone();
                    if let Some(tx) = &self.avisos_cmd {
                        let _ = tx.send(avisos::Comando::Ligar(id));
                    }
                    s.manda(sala::Comando::EntrarNaChamada);
                }
            }
            Msg::AbreAmigos => {
                self.menu_status_aberto = false;
                if let Tela::Sala(s) = &mut self.tela {
                    if s.na_chamada {
                        let s = match std::mem::replace(
                            &mut self.tela,
                            Tela::Salas(Salas::default()),
                        ) {
                            Tela::Sala(s) => s,
                            _ => unreachable!(),
                        };
                        self.chamada_ativa = Some(s);
                        self.atualiza_bandeja();
                    } else {
                        let _ = self.vai_pras_salas();
                    }
                }
                if let Tela::Salas(s) = &mut self.tela {
                    s.aba = AbaSalas::Amigos;
                }
                return Task::done(Msg::CarregaAmigos);
            }
            Msg::TrocaSubAbaAmigos(sub) => {
                if let Tela::Salas(s) = &mut self.tela {
                    s.estado_amigos.sub_aba = sub;
                }
            }
            Msg::CarregaAmigos => {
                if let Tela::Salas(s) = &mut self.tela {
                    s.carregando_amigos = true;
                }
                if let Some(token) = self.token.clone() {
                    let srv = self.srv.clone();
                    return em_segundo_plano(
                        move || api::lista_amigos(&srv, &token).map_err(|e| format!("{e:#}")),
                        Msg::AmigosCarregados,
                    );
                }
            }
            Msg::AmigosCarregados(res) => {
                if let Ok(dados) = &res {
                    self.pedidos_pendentes = dados.recebidos.len();
                    self.amigos_cache = Some(dados.clone());
                }
                if let Tela::Salas(s) = &mut self.tela {
                    s.carregando_amigos = false;
                    match res {
                        Ok(dados) => {
                            s.amigos = Some(dados);
                            s.estado_amigos.erro = None;
                        }
                        Err(e) => {
                            s.estado_amigos.erro = Some(e);
                        }
                    }
                }
            }
            Msg::InputUsuarioAmigo(txt) => {
                if let Tela::Salas(s) = &mut self.tela {
                    s.estado_amigos.input_usuario = txt;
                    s.estado_amigos.erro = None;
                    s.estado_amigos.sucesso = None;
                }
            }
            Msg::EnviaPedidoAmigo => {
                let user = if let Tela::Salas(s) = &mut self.tela {
                    let u = s.estado_amigos.input_usuario.trim().to_string();
                    if u.is_empty() {
                        return Task::none();
                    }
                    s.estado_amigos.enviando_pedido = true;
                    s.estado_amigos.erro = None;
                    s.estado_amigos.sucesso = None;
                    u
                } else {
                    return Task::none();
                };
                if let Some(token) = self.token.clone() {
                    let srv = self.srv.clone();
                    return em_segundo_plano(
                        move || api::pede_amigo(&srv, &token, &user).map_err(|e| format!("{e:#}")),
                        Msg::PedidoAmigoEnviado,
                    );
                }
            }
            Msg::EnviaPedidoAmigoPorNome(user) => {
                if let Some(token) = self.token.clone() {
                    let srv = self.srv.clone();
                    return em_segundo_plano(
                        move || api::pede_amigo(&srv, &token, &user).map_err(|e| format!("{e:#}")),
                        Msg::PedidoAmigoEnviado,
                    );
                }
            }
            Msg::PedidoAmigoEnviado(res) => {
                if let Tela::Salas(s) = &mut self.tela {
                    s.estado_amigos.enviando_pedido = false;
                    match res {
                        Ok(viraram) => {
                            s.estado_amigos.input_usuario.clear();
                            s.estado_amigos.sucesso = Some(if viraram {
                                "Vocês já tinham pedidos mútuos e agora são amigos!".to_string()
                            } else {
                                "Pedido de amizade enviado com sucesso!".to_string()
                            });
                            return Task::done(Msg::CarregaAmigos);
                        }
                        Err(e) => {
                            s.estado_amigos.erro = Some(e);
                        }
                    }
                }
            }
            Msg::RespondePedidoAmigo { uid, aceita } => {
                if let Some(token) = self.token.clone() {
                    let srv = self.srv.clone();
                    return em_segundo_plano(
                        move || {
                            api::responde_amigo(&srv, &token, &uid, aceita)
                                .map_err(|e| format!("{e:#}"))
                        },
                        Msg::PedidoAmigoRespondido,
                    );
                }
            }
            Msg::PedidoAmigoRespondido(res) => match res {
                Ok(()) => return Task::done(Msg::CarregaAmigos),
                Err(e) => {
                    if let Tela::Salas(s) = &mut self.tela {
                        s.estado_amigos.erro = Some(e);
                    }
                }
            },
            Msg::PedeConfirmacaoRemocaoAmigo(uid) => {
                if let Tela::Salas(s) = &mut self.tela {
                    s.estado_amigos.amigo_para_remover = Some(uid);
                }
            }
            Msg::CancelaRemocaoAmigo => {
                if let Tela::Salas(s) = &mut self.tela {
                    s.estado_amigos.amigo_para_remover = None;
                }
            }
            Msg::ConfirmaRemocaoAmigo(uid) => {
                if let Tela::Salas(s) = &mut self.tela {
                    s.estado_amigos.amigo_para_remover = None;
                }
                return Task::done(Msg::RemoveAmigo(uid));
            }
            Msg::RemoveAmigo(uid) => {
                if let Some(token) = self.token.clone() {
                    let srv = self.srv.clone();
                    return em_segundo_plano(
                        move || api::remove_amigo(&srv, &token, &uid).map_err(|e| format!("{e:#}")),
                        Msg::AmigoRemovido,
                    );
                }
            }
            Msg::AmigoRemovido(res) => match res {
                Ok(()) => return Task::done(Msg::CarregaAmigos),
                Err(e) => {
                    if let Tela::Salas(s) = &mut self.tela {
                        s.estado_amigos.erro = Some(e);
                    }
                }
            },
            Msg::VarreAtividade => {
                return em_segundo_plano(atividade::detecta_atividade, Msg::AtividadeDetectada);
            }
            Msg::AtividadeDetectada(mut nova) => {
                if self.modal_config == Some(ui::configuracoes::AbaConfig::JogosRegistrados) {
                    self.jogos_reg = jogos_registrados::carrega();
                }
                // Preserva o início do jogo (desde) para não reiniciar a contagem de tempo
                if let Some(jogo_novo) = nova.as_mut().and_then(|a| a.jogo.as_mut()) {
                    if let Some(jogo_antigo) =
                        self.atividade_atual.as_ref().and_then(|a| a.jogo.as_ref())
                        && jogo_antigo.nome == jogo_novo.nome
                        && jogo_antigo.desde.is_some()
                    {
                        jogo_novo.desde = jogo_antigo.desde;
                    } else {
                        let agora_ms = chrono::Utc::now().timestamp_millis().max(0) as u64;
                        jogo_novo.desde = Some(agora_ms);
                    }
                }
                let mudou = nova != self.atividade_atual;
                // O servidor expira a atividade em 90 s sem notícia: reenvia a
                // mesma a cada 45 s (batimento) para ela não sumir com o app aberto.
                let batimento = nova.is_some()
                    && self
                        .atividade_enviada_em
                        .is_none_or(|t| t.elapsed() > std::time::Duration::from_secs(45));
                if mudou
                    && let Some(j) = nova.as_ref().and_then(|a| a.jogo.clone())
                    && j.capa.is_none()
                    && jogos_registrados::precisa_procurar(&j.nome)
                    && let Some(token) = self.token.clone()
                {
                    // Jogo novo sem capa: procura o nome oficial e a capa (uma vez só).
                    let srv = self.srv.clone();
                    std::thread::spawn(move || {
                        let achados = api::busca_jogos(&srv, &token, &j.nome).unwrap_or_default();
                        let melhor = jogos_registrados::melhor_parecido(&j.nome, &achados);
                        jogos_registrados::define_capa(&j.nome, melhor);
                    });
                }
                if mudou || batimento {
                    self.atividade_atual = nova.clone();
                    self.atividade_enviada_em = Some(std::time::Instant::now());
                    if let Some(token) = self.token.clone() {
                        let srv = self.srv.clone();
                        let atv_clone = nova.clone();
                        std::thread::spawn(move || {
                            let _ = api::envia_atividade(&srv, &token, atv_clone.as_ref());
                        });
                    }
                    if mudou && let Tela::Sala(s) = &mut self.tela {
                        s.manda(sala::Comando::TrocarAtividade(nova));
                    }
                }
            }
            Msg::Tema(tema_msg) => match tema_msg {
                ui::tema::TemaMsg::Selecionar(id) => {
                    if tema::define_tema(&id) {
                        tema::salva_tema(&id);
                    }
                }
                ui::tema::TemaMsg::Abrir => {
                    return Task::done(Msg::AbreConfiguracoes(Some(
                        ui::configuracoes::AbaConfig::Aparencia,
                    )));
                }
                ui::tema::TemaMsg::Fechar => {
                    self.painel_tema_aberto = false;
                    self.modal_config = None;
                    self.editar_perfil_janela = false;
                }
                ui::tema::TemaMsg::RecarregarWallust => {
                    let _ = tema::recarrega_wallust();
                }
            },
            Msg::AbreConfiguracoes(aba_opt) => {
                let aba = aba_opt.unwrap_or_default();
                self.modal_config = Some(aba);
                self.modal_outro_perfil = None;
                self.menu_status_aberto = false;
                self.menu_contexto = None;

                if aba == ui::configuracoes::AbaConfig::VozVideo {
                    self.cameras = captura_camera::lista_cameras();
                    if self.camera_selecionada.is_none() {
                        self.camera_selecionada = self.cameras.first().map(|c| c.indice);
                    }
                }
                if self.modal_meu_perfil.is_none()
                    && let Some(eu) = &self.eu
                {
                    self.modal_meu_perfil = Some(ui::perfil::EdicaoPerfil {
                        aba: ui::perfil::AbaPerfil::Perfil,
                        previa_atividade: false,
                        nome: eu.name.clone(),
                        bio: eu
                            .perfil
                            .as_ref()
                            .and_then(|p| p.bio.clone())
                            .unwrap_or_default(),
                        cor: eu.perfil.as_ref().and_then(|p| p.cor.clone()),
                        status: eu
                            .perfil
                            .as_ref()
                            .and_then(|p| p.status.clone())
                            .unwrap_or_else(|| "online".into()),
                        salvando: false,
                        erro: None,
                        sucesso: None,
                        senha_atual: String::new(),
                        senha_nova: String::new(),
                        senha_nova_confirma: String::new(),
                        trocando_senha: false,
                        confirma_apagar: false,
                        senha_apagar: String::new(),
                        apagando: false,
                        jogos: Default::default(),
                    });
                }
                let tarefa_audio = if aba == ui::configuracoes::AbaConfig::VozVideo {
                    if let Tela::Sala(s) = &mut self.tela {
                        s.manda(sala::Comando::AtualizaDispositivosAudio);
                        Task::none()
                    } else if let Some(ca) = &mut self.chamada_ativa {
                        ca.manda(sala::Comando::AtualizaDispositivosAudio);
                        Task::none()
                    } else {
                        em_segundo_plano(
                            dispositivos::lista_dispositivos_avulso,
                            |(entradas, saidas)| Msg::DispositivosAvulsosLidos { entradas, saidas },
                        )
                    }
                } else {
                    Task::none()
                };
                // Minhas listas de jogos (o editor mostra e altera).
                let listas = match (
                    &self.token,
                    self.eu.as_ref().and_then(|e| e.usuario.clone()),
                ) {
                    (Some(token), Some(usuario)) => {
                        let (srv, tok) = (self.srv.clone(), token.clone());
                        em_segundo_plano(
                            move || {
                                api::busca_perfil(&srv, &tok, &usuario)
                                    .map(Box::new)
                                    .map_err(|e| format!("{e:#}"))
                            },
                            Msg::MinhasListas,
                        )
                    }
                    _ => Task::none(),
                };
                let tarefa_vol = if aba == ui::configuracoes::AbaConfig::VozVideo {
                    em_segundo_plano(
                        || {
                            (
                                sistema_audio::le_volume(true),
                                sistema_audio::le_volume(false),
                            )
                        },
                        |(mic, saida)| Msg::VolumesSistemaLidos(mic, saida),
                    )
                } else {
                    Task::none()
                };
                return Task::batch([tarefa_vol, listas, tarefa_audio]);
            }
            Msg::FechaConfiguracoes => {
                if self.painel_audio.testando_mic {
                    self.painel_audio.testando_mic = false;
                    self.painel_audio.nivel_mic = 0.0;
                    if let Tela::Sala(s) = &mut self.tela {
                        s.manda(sala::Comando::ParaTesteMic);
                    } else if let Some(ca) = &mut self.chamada_ativa {
                        ca.manda(sala::Comando::ParaTesteMic);
                    }
                }
                self.modal_config = None;
                self.editar_perfil_janela = false;
            }
            Msg::TrocaAbaConfig(aba) => {
                self.modal_config = Some(aba);
                // Cada aba começa do topo (a rolagem era a mesma para todas).
                let topo = iced::widget::operation::snap_to(
                    ui::configuracoes::ID_ROLAGEM,
                    iced::widget::scrollable::RelativeOffset::START,
                );
                if aba == ui::configuracoes::AbaConfig::JogosRegistrados {
                    return Task::batch([topo, Task::done(Msg::JogosRegAtualiza)]);
                }
                if aba == ui::configuracoes::AbaConfig::VozVideo {
                    self.cameras = captura_camera::lista_cameras();
                    if self.camera_selecionada.is_none() {
                        self.camera_selecionada = self.cameras.first().map(|c| c.indice);
                    }
                    // Os volumes do sistema também (antes só eram lidos ao abrir
                    // as configurações já nesta aba).
                    let volumes = em_segundo_plano(
                        || {
                            (
                                sistema_audio::le_volume(true),
                                sistema_audio::le_volume(false),
                            )
                        },
                        |(mic, saida)| Msg::VolumesSistemaLidos(mic, saida),
                    );
                    let tarefa_audio = if let Tela::Sala(s) = &mut self.tela {
                        s.manda(sala::Comando::AtualizaDispositivosAudio);
                        Task::none()
                    } else if let Some(ca) = &mut self.chamada_ativa {
                        ca.manda(sala::Comando::AtualizaDispositivosAudio);
                        Task::none()
                    } else {
                        em_segundo_plano(
                            dispositivos::lista_dispositivos_avulso,
                            |(entradas, saidas)| Msg::DispositivosAvulsosLidos { entradas, saidas },
                        )
                    };
                    return Task::batch([topo, volumes, tarefa_audio]);
                }
                return topo;
            }
            Msg::DispositivosAvulsosLidos { entradas, saidas } => {
                let cfg = dispositivos::carrega_config();
                let entrada_sel = cfg
                    .entrada_guid
                    .or_else(|| entradas.first().map(|d| d.guid.clone()));
                let saida_sel = cfg
                    .saida_guid
                    .or_else(|| saidas.first().map(|d| d.guid.clone()));
                self.painel_audio.entradas = entradas;
                self.painel_audio.saidas = saidas;
                if self.painel_audio.entrada_selecionada.is_none() {
                    self.painel_audio.entrada_selecionada = entrada_sel;
                }
                if self.painel_audio.saida_selecionada.is_none() {
                    self.painel_audio.saida_selecionada = saida_sel;
                }
            }
            Msg::SelecionaEntradaAudio(guid) => {
                self.painel_audio.entrada_selecionada = Some(guid.clone());
                dispositivos::salva_dispositivo_gravacao(&guid);
                if let Tela::Sala(s) = &mut self.tela {
                    if let Some(pa) = &mut s.painel_audio {
                        pa.entrada_selecionada = Some(guid.clone());
                    }
                    s.manda(sala::Comando::SelecionaDispositivoGravacao(guid));
                } else if let Some(ca) = &mut self.chamada_ativa {
                    if let Some(pa) = &mut ca.painel_audio {
                        pa.entrada_selecionada = Some(guid.clone());
                    }
                    ca.manda(sala::Comando::SelecionaDispositivoGravacao(guid));
                }
            }
            Msg::SelecionaSaidaAudio(guid) => {
                self.painel_audio.saida_selecionada = Some(guid.clone());
                dispositivos::salva_dispositivo_reproducao(&guid);
                if let Tela::Sala(s) = &mut self.tela {
                    if let Some(pa) = &mut s.painel_audio {
                        pa.saida_selecionada = Some(guid.clone());
                    }
                    s.manda(sala::Comando::SelecionaDispositivoReproducao(guid));
                } else if let Some(ca) = &mut self.chamada_ativa {
                    if let Some(pa) = &mut ca.painel_audio {
                        pa.saida_selecionada = Some(guid.clone());
                    }
                    ca.manda(sala::Comando::SelecionaDispositivoReproducao(guid));
                }
            }
            Msg::AlternaTesteMic => {
                self.painel_audio.testando_mic = !self.painel_audio.testando_mic;
                let testando = self.painel_audio.testando_mic;
                if let Tela::Sala(s) = &mut self.tela {
                    if let Some(pa) = &mut s.painel_audio {
                        pa.testando_mic = testando;
                    }
                    if testando {
                        s.manda(sala::Comando::IniciaTesteMic);
                    } else {
                        self.painel_audio.nivel_mic = 0.0;
                        s.manda(sala::Comando::ParaTesteMic);
                    }
                } else if let Some(ca) = &mut self.chamada_ativa {
                    if let Some(pa) = &mut ca.painel_audio {
                        pa.testando_mic = testando;
                    }
                    if testando {
                        ca.manda(sala::Comando::IniciaTesteMic);
                    } else {
                        self.painel_audio.nivel_mic = 0.0;
                        ca.manda(sala::Comando::ParaTesteMic);
                    }
                }
            }
            Msg::SelecionaCameraConfig(idx) => {
                self.camera_selecionada = Some(idx);
                if let Tela::Sala(s) = &mut self.tela
                    && s.com_camera
                {
                    s.manda(sala::Comando::IniciarCamera { indice: idx });
                }
            }
            Msg::AlternaEspelharCamera => {
                self.espelhar_camera = !self.espelhar_camera;
            }
            Msg::SelecionaPerfilTelaConfig(perfil) => {
                self.perfil_tela = perfil;
                if let Tela::Sala(s) = &mut self.tela
                    && let Some(cfg) = &mut s.config_tela
                {
                    cfg.perfil = perfil;
                }
            }
            Msg::AlternaPttAtivo => {
                self.ptt.ativo = !self.ptt.ativo;
                ptt::salva_config(&self.ptt);
                if let Tela::Sala(s) = &mut self.tela {
                    s.ptt.ativo = self.ptt.ativo;
                    s.manda(sala::Comando::PttConfigurado {
                        ativo: self.ptt.ativo,
                    });
                }
            }
            Msg::SelecionaDensidade(d) => {
                self.densidade = d;
                tema::salva_aparencia(&tema::tema_atual_id(), self.densidade);
                if let Tela::Sala(s) = &mut self.tela {
                    s.densidade = d;
                }
            }
            Msg::TogglePainelDiagnostico => {
                self.painel_diag_aberto = !self.painel_diag_aberto;
                if self.painel_diag_aberto {
                    return Task::done(Msg::AtualizaMetricasServidor);
                }
            }
            Msg::FechaPainelDiagnostico => {
                self.painel_diag_aberto = false;
            }
            Msg::AtualizaMetricasServidor => {
                self.carregando_metricas = true;
                let srv = self.srv.clone();
                let tok = self.token.clone().unwrap_or_default();
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || api::busca_metricas(&srv, &tok))
                            .await
                            .map_err(|e| e.to_string())
                            .and_then(|r| r.map_err(|e| e.to_string()))
                    },
                    Msg::MetricasServidorRecebidas,
                );
            }
            Msg::MetricasServidorRecebidas(res) => {
                self.carregando_metricas = false;
                match res {
                    Ok(m) => self.metricas_servidor = Some(m),
                    Err(e) => log::warn!("[diagnostico] erro ao buscar /api/metrics: {e}"),
                }
            }
            Msg::IniciaCapturaPtt => {
                self.capturando_ptt = true;
                if let Tela::Sala(s) = &mut self.tela {
                    s.capturando_ptt = true;
                }
            }
            Msg::CancelaCapturaPtt => {
                self.capturando_ptt = false;
                if let Tela::Sala(s) = &mut self.tela {
                    s.capturando_ptt = false;
                }
            }
            Msg::AbreConfigAudio => {
                return Task::done(Msg::AbreConfiguracoes(Some(
                    ui::configuracoes::AbaConfig::VozVideo,
                )));
            }
            Msg::FechaConfigAudio => {
                return Task::done(Msg::FechaConfiguracoes);
            }
            Msg::EventoIced(iced::Event::Window(iced::window::Event::CloseRequested)) => {
                if self.minimizar_para_bandeja {
                    return iced::window::latest().and_then(|id| iced::window::minimize(id, true));
                }
                let em_chamada = if let Tela::Sala(s) = &self.tela {
                    if s.na_chamada {
                        Some(s.titulo.clone())
                    } else {
                        self.chamada_ativa.as_ref().map(|c| c.titulo.clone())
                    }
                } else {
                    self.chamada_ativa.as_ref().map(|c| c.titulo.clone())
                };
                if let Some(sala) = em_chamada {
                    self.modal_confirmacao =
                        Some(ui::confirmacao::TipoConfirmacao::FecharAplicativo {
                            sala: Some(sala),
                        });
                    return Task::none();
                } else {
                    crate::relatorio::envia_ao_fechar(&self.srv, self.token.as_deref());
                    return iced::exit();
                }
            }
            Msg::EventoIced(iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key,
                modifiers,
                physical_key,
                ..
            })) if self.capturando_atalho.is_some() => {
                let acao = self.capturando_atalho.clone().unwrap_or_default();
                if matches!(
                    key,
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape)
                ) {
                    self.capturando_atalho = None;
                } else if let Some(combo) =
                    atalhos::combo_de_evento_completo(&key, Some(&physical_key), modifiers)
                {
                    if let Some(outra) = self.atalhos.acao_de(&combo, &acao) {
                        self.aviso_atalho = Some(format!(
                            "{combo} já está em uso por \"{}\".",
                            atalhos::ACOES
                                .iter()
                                .find(|(id, _, _)| *id == outra)
                                .map(|(_, r, _)| *r)
                                .unwrap_or(outra)
                        ));
                    } else {
                        self.atalhos.define(&acao, combo);
                        self.atalhos.salva();
                        self.aviso_atalho = None;
                    }
                    self.capturando_atalho = None;
                }
                return Task::none();
            }
            Msg::EventoIced(iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Named(iced::keyboard::key::Named::F1),
                ..
            })) => {
                self.modal_manual_aberto = !self.modal_manual_aberto;
                return Task::none();
            }
            Msg::EventoIced(iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key,
                modifiers,
                ..
            })) if (modifiers.command() || modifiers.control())
                && modifiers.shift()
                && matches!(&key, iced::keyboard::Key::Character(c) if c.eq_ignore_ascii_case("d")) =>
            {
                return Task::done(Msg::TogglePainelDiagnostico);
            }
            Msg::EventoIced(iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                ..
            })) if self.modal_foto.is_some() => {
                self.modal_foto = None;
            }
            Msg::EventoIced(iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                ..
            })) if self.modal_visao_jogo.is_some() => {
                self.modal_visao_jogo = None;
            }
            Msg::EventoIced(iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                ..
            })) if self.jogos_reg_renomeando.is_some() => {
                self.jogos_reg_renomeando = None;
            }
            Msg::EventoIced(iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                ..
            })) if self.modal_config.is_some()
                || self.modal_editar_grupo.is_some()
                || self.modal_grupo.is_some()
                || self.menu_contexto.is_some()
                || self.menu_status_aberto
                || self.painel_tema_aberto
                || self.painel_diag_aberto
                || self.modal_meu_perfil.is_some()
                || self.modal_outro_perfil.is_some()
                || self.modal_confirmacao.is_some()
                || self.modal_manual_aberto =>
            {
                if self.painel_audio.testando_mic {
                    self.painel_audio.testando_mic = false;
                    self.painel_audio.nivel_mic = 0.0;
                    if let Tela::Sala(s) = &mut self.tela {
                        s.manda(sala::Comando::ParaTesteMic);
                    }
                }
                self.modal_config = None;
                self.editar_perfil_janela = false;
                self.modal_editar_grupo = None;
                self.modal_grupo = None;
                self.menu_contexto = None;
                self.menu_status_aberto = false;
                self.painel_tema_aberto = false;
                self.painel_diag_aberto = false;
                self.modal_meu_perfil = None;
                self.modal_outro_perfil = None;
                self.modal_confirmacao = None;
                self.modal_manual_aberto = false;
                self.modal_foto = None;
                self.modal_visao_jogo = None;
                self.jogos_reg_renomeando = None;
                self.jogos_reg_seletor_aberto = false;
            }
            Msg::EventoIced(iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key,
                physical_key,
                ..
            })) if self.capturando_ptt => {
                if matches!(
                    key,
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape)
                ) {
                    self.capturando_ptt = false;
                    if let Tela::Sala(s) = &mut self.tela {
                        s.capturando_ptt = false;
                    }
                } else if let Some((id, rotulo)) =
                    ptt::descreve_tecla_completa(&key, Some(&physical_key))
                {
                    self.ptt.tecla = id;
                    self.ptt.rotulo = rotulo;
                    ptt::salva_config(&self.ptt);
                    self.capturando_ptt = false;
                    if let Tela::Sala(s) = &mut self.tela {
                        s.ptt = self.ptt.clone();
                        s.capturando_ptt = false;
                    }
                }
            }
            Msg::EventoIced(iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key,
                modifiers,
                physical_key,
                ..
            })) if self.modal_config.is_none()
                && self.modal_meu_perfil.is_none()
                && self.modal_outro_perfil.is_none()
                && self.modal_editar_grupo.is_none()
                && self.modal_grupo.is_none()
                && self.modal_confirmacao.is_none()
                && self.capturando_atalho.is_none()
                && !self.capturando_ptt
                && atalhos::combo_de_evento_completo(&key, Some(&physical_key), modifiers)
                    .as_deref()
                    .and_then(|c| self.atalhos.acao_na_janela(c))
                    .is_some() =>
            {
                let combo = atalhos::combo_de_evento_completo(&key, Some(&physical_key), modifiers)
                    .unwrap();
                let acao = self.atalhos.acao_na_janela(&combo).unwrap();
                return Task::done(Msg::PortalAtalho {
                    acao: acao.to_string(),
                    pressionado: true,
                });
            }
            // Chamada em segundo plano: a sala aberta não é a da chamada, então
            // apertar e soltar a tecla do PTT são tratados aqui (os dois com a
            // mesma tecla; antes apertar usava o atalho e soltar o `ptt.json`).
            Msg::EventoIced(iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key,
                physical_key,
                repeat: false,
                ..
            })) if self.ptt.ativo
                && self.chamada_ativa.is_some()
                && self.capturando_atalho.is_none()
                && !self.capturando_ptt
                && ptt::corresponde_completo(&self.ptt.tecla, &key, Some(&physical_key)) =>
            {
                return Task::done(Msg::PttTransmitindo(true));
            }
            Msg::EventoIced(iced::Event::Keyboard(iced::keyboard::Event::KeyReleased {
                key,
                physical_key,
                ..
            })) if self.ptt.ativo
                && self.chamada_ativa.is_some()
                && ptt::corresponde_completo(&self.ptt.tecla, &key, Some(&physical_key)) =>
            {
                return Task::done(Msg::PttTransmitindo(false));
            }
            Msg::AlternaMudo => {
                if let Tela::Sala(s) = &mut self.tela
                    && s.na_chamada
                {
                    let res = s.update(Msg::AlternaMudo);
                    self.atualiza_bandeja();
                    return res;
                }
                if let Some(ca) = &mut self.chamada_ativa {
                    ca.mudo = !ca.mudo;
                    ca.manda(sala::Comando::Mudo(ca.mudo));
                    self.atualiza_bandeja();
                }
            }
            Msg::AlternaSurdo => {
                if let Tela::Sala(s) = &mut self.tela
                    && s.na_chamada
                {
                    let res = s.update(Msg::AlternaSurdo);
                    self.atualiza_bandeja();
                    return res;
                }
                if let Some(ca) = &mut self.chamada_ativa {
                    ca.surdo = !ca.surdo;
                    ca.manda(sala::Comando::Surdo(ca.surdo));
                    self.atualiza_bandeja();
                }
            }
            Msg::SaiDaChamada => {
                if let Tela::Sala(s) = &mut self.tela
                    && s.na_chamada
                {
                    let res = s.update(Msg::SaiDaChamada);
                    self.atualiza_bandeja();
                    return res;
                }
                if let Some(ca) = self.chamada_ativa.take() {
                    ca.manda(sala::Comando::SairDaChamada);
                    self.atualiza_bandeja();
                }
            }
            Msg::PttTransmitindo(p) => {
                if let Tela::Sala(s) = &mut self.tela
                    && s.na_chamada
                {
                    return s.update(Msg::PttTransmitindo(p));
                }
                if let Some(ca) = &mut self.chamada_ativa {
                    ca.ptt_transmitindo = p;
                    ca.manda(sala::Comando::PttTransmitindo(p));
                }
            }
            Msg::ParaCamera => {
                if let Tela::Sala(s) = &mut self.tela
                    && s.na_chamada
                {
                    return s.update(Msg::ParaCamera);
                }
                if let Some(ca) = &mut self.chamada_ativa {
                    ca.manda(sala::Comando::PararCamera);
                    ca.com_camera = false;
                }
            }
            Msg::PedeCamera => {
                if let Tela::Sala(s) = &mut self.tela
                    && s.na_chamada
                {
                    return s.update(Msg::PedeCamera);
                }
                if let Some(ca) = &mut self.chamada_ativa {
                    let indice = self.camera_selecionada.unwrap_or(0);
                    ca.manda(sala::Comando::IniciarCamera { indice });
                    ca.com_camera = true;
                }
            }
            Msg::ParaTela => {
                if let Tela::Sala(s) = &mut self.tela
                    && s.na_chamada
                {
                    return s.update(Msg::ParaTela);
                }
                if let Some(ca) = &mut self.chamada_ativa {
                    ca.manda(sala::Comando::PararTela);
                    ca.apresentando = false;
                }
            }
            Msg::PedeTela => {
                if let Tela::Sala(s) = &mut self.tela
                    && s.na_chamada
                {
                    return s.update(Msg::PedeTela);
                }
                if let Some(ca) = self.chamada_ativa.take() {
                    let id = ca.entrada.sala.clone();
                    self.avisos.marca_lida(&id);
                    self.tela = Tela::Sala(ca);
                    self.atualiza_bandeja();
                    return Task::done(Msg::PedeTela);
                }
            }
            other => {
                if let Tela::Sala(s) = &mut self.tela {
                    let atualiza = matches!(other, Msg::AlternaMudo | Msg::SaiDaChamada);
                    let res = s.update(other);
                    if atualiza {
                        self.atualiza_bandeja();
                    }
                    return res;
                }
            }
        }
        Task::none()
    }
}

impl App {
    /// Bytes da imagem de uma mensagem da sala aberta.
    fn bytes_da_imagem(&self, id: &str) -> Option<Vec<u8>> {
        let Tela::Sala(s) = &self.tela else {
            return None;
        };
        s.mensagens
            .iter()
            .find(|m| m.id == id)
            .and_then(|m| m.imagem.clone())
    }
}

/// Extensão (e tipo) da imagem pelos primeiros bytes.
pub(crate) fn tipo_da_imagem(b: &[u8]) -> (&'static str, &'static str) {
    if b.starts_with(&[0x89, b'P', b'N', b'G']) {
        ("png", "image/png")
    } else if b.starts_with(&[0xFF, 0xD8]) {
        ("jpg", "image/jpeg")
    } else if b.starts_with(b"GIF8") {
        ("gif", "image/gif")
    } else if b.len() > 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        ("webp", "image/webp")
    } else {
        ("png", "image/png")
    }
}

fn extensao_da_imagem(b: &[u8]) -> &'static str {
    tipo_da_imagem(b).0
}

/// Põe a imagem na área de transferência: `wl-copy` com o tipo certo (serve
/// para qualquer formato); se não houver, `arboard` (PNG/JPEG decodificados).
fn copia_imagem_para_area(bytes: &[u8]) {
    use std::io::Write;
    let (_, mime) = tipo_da_imagem(bytes);
    if let Ok(mut filho) = std::process::Command::new("wl-copy")
        .args(["-t", mime])
        .stdin(std::process::Stdio::piped())
        .spawn()
    {
        if let Some(mut si) = filho.stdin.take() {
            let _ = si.write_all(bytes);
        }
        if filho.wait().map(|s| s.success()).unwrap_or(false) {
            return;
        }
    }
    if let Ok(img) = ::image::load_from_memory(bytes) {
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        if let Ok(mut cb) = arboard::Clipboard::new() {
            let _ = cb.set_image(arboard::ImageData {
                width: w as usize,
                height: h as usize,
                bytes: std::borrow::Cow::Owned(rgba.into_raw()),
            });
        }
    }
}
