//! `SalaAberta`: comandos e atualização de estado da sala.

use super::*;

impl SalaAberta {
    /// Abre o painel pedido (fechando qualquer outro) ou fecha se já estava aberto.
    pub(crate) fn alterna_painel(&mut self, p: PainelFlutuante) {
        // Os popups dos botões da chamada abrem no lugar do clique.
        self.painel_ancora = self.cursor_pos;
        self.painel = if self.painel == Some(p) {
            None
        } else {
            Some(p)
        };
    }

    /// Abre a aba de GIFs: busca os em alta na primeira vez.
    fn carrega_gifs_se_preciso(&mut self) -> Task<Msg> {
        if !self.resultados_gif.is_empty() || self.buscando_gif {
            return Task::none();
        }
        self.buscando_gif = true;
        self.erro_gif = None;
        let (srv, token, busca) = (
            self.entrada.servidor.clone(),
            self.entrada.token.clone(),
            self.busca_gif.clone(),
        );
        em_segundo_plano(
            move || api::busca_gifs(&srv, &token, &busca).map_err(|e| format!("{e:#}")),
            Msg::GifsCarregados,
        )
    }

    pub(crate) fn manda(&self, c: Comando) {
        if let Some(tx) = &self.comandos {
            let _ = tx.send(c);
        }
    }

    pub(crate) fn indexa_imagens(&mut self) -> Task<Msg> {
        let mut tarefas = Vec::new();
        for m in &self.mensagens {
            if let Some(b) = &m.imagem
                && !self.imagens.contains_key(&m.id)
            {
                self.imagens
                    .insert(m.id.clone(), image::Handle::from_bytes(b.clone()));
            } else if let Some(url) = &m.gif
                && !self.imagens.contains_key(&m.id)
                && ui::chat::eh_url_de_gif(url)
            {
                tarefas.push(baixa_gif(m.id.clone(), url.clone()));
            }
        }
        Task::batch(tarefas)
    }

    pub(crate) fn update(&mut self, msg: Msg) -> Task<Msg> {
        // Escolher um item do menu "+" fecha o menu.
        if matches!(
            msg,
            Msg::EscolheArquivoImagem
                | Msg::AlternarCriadorEnquete
                | Msg::AlternarRoletas
                | Msg::AlternarQuadro
                | Msg::Wordle(ui::wordle::WordleMsg::Inicia)
        ) {
            self.painel = None;
        }
        match msg {
            Msg::Rascunho(v) => {
                if !self.capturando_ptt {
                    self.rascunho = v;
                    let agora = std::time::Instant::now();
                    self.ultimo_input_chat = Some(agora);
                    if !self.rascunho.trim().is_empty() {
                        if !self.digitando_anunciado {
                            self.digitando_anunciado = true;
                            self.manda(Comando::Digitando(true));
                        }
                    } else if self.digitando_anunciado {
                        self.digitando_anunciado = false;
                        self.manda(Comando::Digitando(false));
                    }
                }
            }
            Msg::EnviaChat => {
                self.nao_lidas = 0;
                self.primeira_nao_lida_id = None;
                if self.digitando_anunciado {
                    self.digitando_anunciado = false;
                    self.manda(Comando::Digitando(false));
                }
                if !self.sussurro_alvos.is_empty() {
                    let destinos: Vec<String> = self
                        .pessoas
                        .iter()
                        .filter(|p| !p.eu && self.sussurro_alvos.iter().any(|a| a.mesma_pessoa(p)))
                        .map(|p| p.id.clone())
                        .collect();
                    if destinos.is_empty() {
                        self.erro_sussurro = Some(
                            "Ninguém do sussurro está na sala agora — nada foi enviado."
                                .to_string(),
                        );
                        return Task::none();
                    }
                    let t = std::mem::take(&mut self.rascunho);
                    let texto = t.trim().to_string();
                    if !texto.is_empty() {
                        self.manda(Comando::Chat {
                            texto,
                            imagem: None,
                            gif: None,
                            resposta_a: None,
                            para: Some(destinos),
                            enquete: None,
                        });
                    }
                    self.erro_sussurro = None;
                    return Task::none();
                }
                let mut t = std::mem::take(&mut self.rascunho);
                if let Some(ed) = self.editando.take() {
                    let texto = t.trim().to_string();
                    if !texto.is_empty() {
                        self.manda(Comando::EditaChat { id: ed.id, texto });
                    }
                } else {
                    let anexo = self.imagem_pendente.take();
                    let resposta_a = self.resposta_pendente.take();
                    let mut enquete = None;
                    if anexo.is_none() && t.trim().starts_with('/') {
                        match ui::chat::processa_comando_chat(&t) {
                            Some(ui::chat::ComandoChatResultado::Erro(err)) => {
                                self.aviso = Some(err);
                                return Task::none();
                            }
                            Some(ui::chat::ComandoChatResultado::AbreWordle) => {
                                return Task::done(Msg::Wordle(ui::wordle::WordleMsg::Inicia));
                            }
                            Some(ui::chat::ComandoChatResultado::CriarEnquete(enq)) => {
                                enquete = Some(enq);
                                t.clear();
                            }
                            Some(ui::chat::ComandoChatResultado::MensagemTexto(txt)) => {
                                t = txt;
                            }
                            None => {}
                        }
                    }
                    if !t.trim().is_empty() || anexo.is_some() || enquete.is_some() {
                        let imagem = anexo.map(|img| sala::ImagemAnexo {
                            mime: img.mime,
                            data: img.base64,
                        });
                        self.manda(Comando::Chat {
                            texto: t,
                            imagem,
                            gif: None,
                            resposta_a,
                            para: None,
                            enquete,
                        });
                    }
                }
            }
            Msg::IniciaEdicaoChat(id) => {
                if let Some(m) = self.mensagens.iter().find(|m| m.id == id) {
                    self.resposta_pendente = None;
                    self.editando = Some(ui::chat::MensagemEmEdicao {
                        id: id.clone(),
                        texto_original: m.texto.clone(),
                    });
                    self.rascunho = m.texto.clone();
                }
            }
            Msg::CancelaEdicaoChat => {
                self.editando = None;
                self.rascunho.clear();
            }
            Msg::IniciaRespostaChat { id, autor, texto } => {
                self.editando = None;
                self.resposta_pendente = Some(sala::RespostaA { id, autor, texto });
            }
            Msg::CancelaRespostaChat => {
                self.resposta_pendente = None;
            }
            Msg::AlternaSeletorReacao(id) => {
                if self.seletor_reacao.as_deref() == Some(&id) {
                    self.seletor_reacao = None;
                } else {
                    self.seletor_reacao = Some(id);
                }
            }
            Msg::ReagirChat { id, emoji } => {
                self.seletor_reacao = None;
                self.manda(Comando::ReagirChat { id, emoji });
            }
            Msg::FixaChat(id) => {
                self.manda(Comando::FixarChat(Some(id)));
            }
            Msg::DesafixaChat => {
                self.manda(Comando::FixarChat(None));
            }
            Msg::TickDigitando => {
                let agora = std::time::Instant::now();
                self.digitando_remotos
                    .retain(|_, (_, expira)| *expira > agora);
                if self.digitando_anunciado
                    && let Some(ult) = self.ultimo_input_chat
                    && agora.duration_since(ult) >= std::time::Duration::from_secs(3)
                {
                    self.digitando_anunciado = false;
                    self.manda(Comando::Digitando(false));
                }
                ui::reacao::atualiza_reacoes(&mut self.reacoes_flutuantes, 500);
                ui::reacao::atualiza_cutucada(&mut self.aviso_cutucada, 500);
            }
            Msg::AlternaMenuReacoesFlutuantes => self.alterna_painel(PainelFlutuante::ReacoesTela),
            Msg::FechaPainel => self.painel = None,
            Msg::EnviarReacao(emoji) => {
                self.painel = None;
                self.manda(Comando::EnviarReacao(emoji));
            }
            Msg::Cutucar(id) => {
                self.manda(Comando::Cutucar(id));
            }
            Msg::AlternaPainelSoundboard => {
                // Os sons só existem dentro de uma chamada.
                if self.na_chamada {
                    self.alterna_painel(PainelFlutuante::Sons);
                } else if self.painel == Some(PainelFlutuante::Sons) {
                    self.painel = None;
                }
            }
            Msg::VolumeSoundboard(v) => {
                self.soundboard.volume = v.min(100);
            }
            Msg::SalvaVolumeSoundboard => {
                ui::soundboard::salva_volume(self.soundboard.volume);
            }
            Msg::AlternaMuteSoundboardGlobal => {
                self.soundboard.mudo_global = !self.soundboard.mudo_global;
            }
            Msg::AlternaMuteSoundboardPessoa(nome) => {
                self.soundboard.alterna_silenciado(&nome);
            }
            Msg::TocarSom(id) => {
                let agora = std::time::Instant::now();
                if let Some(ult) = self.ultimo_som_tocado
                    && agora.duration_since(ult) < std::time::Duration::from_millis(2000)
                {
                    return Task::none();
                }
                self.ultimo_som_tocado = Some(agora);
                let vol = self.soundboard.volume;
                self.manda(Comando::EnviarSom(id.clone(), vol));
                som::toca_som(&id, vol);
            }
            Msg::CompletaMencao(nome) => {
                if let Some(idx) = self.rascunho.rfind('@') {
                    self.rascunho.truncate(idx);
                    self.rascunho.push('@');
                    self.rascunho.push_str(&nome);
                    self.rascunho.push(' ');
                }
            }
            Msg::AbreLink(url) => {
                let alvo = if url.starts_with("http://") || url.starts_with("https://") {
                    url
                } else {
                    format!("https://{url}")
                };
                std::thread::spawn(move || {
                    let _ = std::process::Command::new("xdg-open").arg(&alvo).output();
                });
            }
            Msg::ApagaChat(id) => {
                if self.editando.as_ref().map(|e| &e.id) == Some(&id) {
                    self.editando = None;
                    self.rascunho.clear();
                }
                if self.resposta_pendente.as_ref().map(|r| &r.id) == Some(&id) {
                    self.resposta_pendente = None;
                }
                if self.seletor_reacao.as_ref() == Some(&id) {
                    self.seletor_reacao = None;
                }
                self.manda(Comando::ApagaChat { id });
            }
            Msg::EscolheArquivoImagem => {
                return Task::perform(
                    async {
                        let handle = rfd::AsyncFileDialog::new().pick_file().await?;
                        Some((handle.file_name(), handle.path().to_path_buf()))
                    },
                    Msg::ArquivoEscolhido,
                );
            }
            Msg::ArquivoEscolhido(Some((nome, caminho))) => {
                let tam = std::fs::metadata(&caminho).map(|m| m.len()).unwrap_or(0);
                let ext = caminho
                    .extension()
                    .map(|e| e.to_string_lossy().to_ascii_lowercase())
                    .unwrap_or_default();
                let eh_imagem = matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp");
                if tam == 0 {
                    self.aviso = Some("Não consegui ler esse arquivo.".into());
                } else if eh_imagem && tam <= ui::chat::MAX_CHAT_IMAGE_BYTES as u64 {
                    // Imagem pequena aparece inline no chat, como sempre.
                    return Task::perform(
                        async move {
                            let bytes = tokio::fs::read(&caminho).await.ok()?;
                            Some((nome, bytes))
                        },
                        Msg::ArquivoImagemEscolhido,
                    );
                } else if tam > 200 * 1024 * 1024 {
                    self.aviso = Some("O arquivo passa de 200 MB.".into());
                } else {
                    self.aviso = Some(format!("Enviando {nome}…"));
                    let (srv, token, conversa) = (
                        self.entrada.servidor.clone(),
                        self.entrada.token.clone(),
                        self.entrada.sala.clone(),
                    );
                    return em_segundo_plano(
                        move || {
                            api::envia_arquivo(&srv, &token, &conversa, &caminho)
                                .map_err(|e| format!("{e:#}"))
                        },
                        Msg::ArquivoEnviado,
                    );
                }
            }
            Msg::ArquivoEscolhido(None) => {}
            Msg::ArquivoEnviado(Ok(a)) => {
                self.aviso = None;
                let texto = std::mem::take(&mut self.rascunho);
                self.manda(Comando::ChatArquivo {
                    texto,
                    arquivo_id: a.id,
                });
            }
            Msg::ArquivoEnviado(Err(e)) => self.aviso = Some(format!("Falha no envio: {e}")),
            Msg::BaixaArquivo { id, nome } => {
                let pasta = directories::UserDirs::new()
                    .and_then(|d| d.download_dir().map(|p| p.to_path_buf()))
                    .unwrap_or_else(std::env::temp_dir);
                let destino = ui::chat::caminho_livre(&pasta, &nome);
                self.aviso = Some(format!("Baixando {nome}…"));
                let (srv, token, conversa) = (
                    self.entrada.servidor.clone(),
                    self.entrada.token.clone(),
                    self.entrada.sala.clone(),
                );
                return em_segundo_plano(
                    move || {
                        api::baixa_arquivo(&srv, &token, &conversa, &id, &destino)
                            .map(|_| destino)
                            .map_err(|e| format!("{e:#}"))
                    },
                    Msg::ArquivoBaixado,
                );
            }
            Msg::ArquivoBaixado(Ok(p)) => {
                self.aviso = Some(format!("Salvo em {}", p.display()));
            }
            Msg::ArquivoBaixado(Err(e)) => self.aviso = Some(format!("Falha no download: {e}")),
            Msg::ArquivoImagemEscolhido(Some((nome, bytes))) => {
                match ui::chat::valida_e_prepara_imagem(nome, bytes) {
                    Ok(pendente) => self.imagem_pendente = Some(pendente),
                    Err(e) => self.aviso = Some(e),
                }
            }
            Msg::ArquivoImagemEscolhido(None) => {}
            Msg::CancelaImagemPendente => {
                self.imagem_pendente = None;
            }
            Msg::AlternaPainelDireito => self.painel_direito_aberto = !self.painel_direito_aberto,
            Msg::AlternaFixadas => self.fixadas_ativas = !self.fixadas_ativas,
            Msg::AlternaMenuAtividades => {
                self.painel_ancora = self.cursor_pos;
                self.alterna_painel(PainelFlutuante::Atividades)
            }
            Msg::AlternaMenuMaisPalco => {
                self.painel_ancora = self.cursor_pos;
                self.alterna_painel(PainelFlutuante::MaisPalco)
            }
            Msg::AlternaMenuMais => self.alterna_painel(PainelFlutuante::Mais),
            Msg::AlternaMenuEmoji => self.alterna_painel(PainelFlutuante::Emoji),
            Msg::InsereEmoji(e) => {
                self.rascunho.push_str(&e);
                self.painel = None;
            }
            Msg::SaiDaChamada => self.manda(Comando::SairDaChamada),
            Msg::AlternaMudo => {
                if !self.mudo_forcado {
                    self.manda(Comando::Mudo(!self.mudo));
                }
            }
            Msg::AlternaSurdo => self.manda(Comando::Surdo(!self.surdo)),
            Msg::PedeTela => {
                self.config_tela = Some(ui::tela::ConfigCompartilharTela::guardada());
                return em_segundo_plano(lista_apps_tocando, Msg::AppsTocando);
            }
            Msg::AppsTocando(Ok(apps)) => {
                if let Some(cfg) = &mut self.config_tela {
                    cfg.apps = apps.into_iter().map(|a| (a, false)).collect();
                    cfg.remarca_apps();
                }
            }
            Msg::AppsTocando(Err(e)) => {
                self.aviso = Some(format!(
                    "Não deu pra ver quais aplicativos tocam som ({e}). A tela pode ir sem som."
                ));
            }
            Msg::MudaPerfilTela(perfil) => {
                if let Some(cfg) = &mut self.config_tela {
                    cfg.perfil = perfil;
                    cfg.salva();
                }
            }
            Msg::AlternaAutoBitrate(v) => {
                if let Some(cfg) = &mut self.config_tela {
                    cfg.auto_bitrate = v;
                    cfg.salva();
                }
            }
            Msg::MarcaApp(i, v) => {
                if let Some(cfg) = &mut self.config_tela
                    && let Some(item) = cfg.apps.get_mut(i)
                {
                    item.1 = v;
                }
            }
            Msg::ConfirmaTela => {
                if let Some(cfg) = self.config_tela.take() {
                    cfg.salva();
                    let apps = if cfg.todo_pc {
                        vec!["*".to_string()]
                    } else {
                        cfg.apps
                            .into_iter()
                            .filter(|(_, marcado)| *marcado)
                            .map(|(a, _)| {
                                if a.binario.is_empty() {
                                    a.nome
                                } else {
                                    a.binario
                                }
                            })
                            .collect()
                    };
                    self.manda(Comando::CompartilharTela {
                        apps,
                        perfil: cfg.perfil,
                        auto_bitrate: cfg.auto_bitrate,
                    });
                }
            }
            Msg::EsqueceTelaLembrada => crate::portal_tela::esquece_escolha(),
            Msg::AlternaSomTela => {
                if let Some(cfg) = &mut self.config_tela {
                    cfg.som_aberto = !cfg.som_aberto;
                }
            }
            Msg::AlternaTodoPc => {
                if let Some(cfg) = &mut self.config_tela {
                    cfg.todo_pc = !cfg.todo_pc;
                    cfg.salva();
                }
            }
            Msg::CancelaTela => self.config_tela = None,
            Msg::ParaTela => self.manda(Comando::PararTela),
            Msg::AlternaPreviaPropria => {
                self.previa_propria_oculta = !self.previa_propria_oculta;
                if self.previa_propria_oculta
                    && self
                        .video_fixado
                        .as_ref()
                        .is_some_and(|(d, t)| *t && *d == self.meu_id)
                {
                    self.video_fixado = None;
                }
            }
            Msg::AlternaFixacaoVideo(dono, de_tela) => {
                if de_tela && dono == self.meu_id {
                    self.previa_propria_oculta = false;
                }
                if self.video_fixado.as_ref() == Some(&(dono.clone(), de_tela)) {
                    self.video_fixado = None;
                } else {
                    // Focar uma transmissão também começa a assisti-la.
                    self.assistindo.insert((dono.clone(), de_tela));
                    self.video_fixado = Some((dono, de_tela));
                }
            }
            Msg::AssisteTransmissao(dono, de_tela) => {
                self.assistindo.insert((dono, de_tela));
            }
            Msg::PausaTransmissao(dono, de_tela) => {
                self.assistindo.remove(&(dono.clone(), de_tela));
                if self.video_em_tela_cheia.as_ref() == Some(&(dono, de_tela)) {
                    self.video_em_tela_cheia = None;
                }
            }
            Msg::AlternaTelaCheiaVideo(tc) => {
                self.ultimo_movimento = std::time::Instant::now();
                // Quem apresenta não põe a própria tela em tela cheia.
                self.video_em_tela_cheia = tc.filter(|(d, t)| !(*t && *d == self.meu_id));
            }
            Msg::PedeCamera => {
                let cameras = captura_camera::lista_cameras();
                if cameras.is_empty() {
                    self.aviso = Some("Nenhuma câmera encontrada no sistema.".into());
                } else {
                    let indice = cameras[0].indice;
                    self.manda(Comando::PreviaCamera(Some(indice)));
                    self.previa_camera = Some(PreviaCamera {
                        cameras,
                        indice_selecionado: indice,
                        imagem: None,
                    });
                }
            }
            Msg::TrocaCameraPrevia(indice) => {
                if let Some(prev) = &mut self.previa_camera {
                    prev.indice_selecionado = indice;
                    prev.imagem = None;
                    self.manda(Comando::PreviaCamera(Some(indice)));
                }
            }
            Msg::ConfirmaCamera => {
                if let Some(prev) = self.previa_camera.take() {
                    self.manda(Comando::IniciarCamera {
                        indice: prev.indice_selecionado,
                    });
                }
            }
            Msg::CancelaPreviaCamera => {
                self.previa_camera = None;
                self.manda(Comando::CancelaPreviaCamera);
            }
            Msg::ParaCamera => {
                self.manda(Comando::PararCamera);
            }
            Msg::AjustaVolume(id, vol) => {
                self.manda(Comando::AjustaVolume {
                    id,
                    volume: vol,
                    de_tela: false,
                });
            }
            Msg::AlternaMute(id) => {
                self.manda(Comando::AlternaMute { id, de_tela: false });
            }
            Msg::AjustaVolumeTela(id, vol) => {
                self.manda(Comando::AjustaVolume {
                    id,
                    volume: vol,
                    de_tela: true,
                });
            }
            Msg::AlternaMuteTela(id) => {
                self.manda(Comando::AlternaMute { id, de_tela: true });
            }
            Msg::IniciaCapturaPtt => {
                self.capturando_ptt = !self.capturando_ptt;
            }
            Msg::CancelaCapturaPtt => {
                self.capturando_ptt = false;
            }
            Msg::PttTransmitindo(transmitindo) => {
                if self.ptt.ativo && self.na_chamada && self.ptt_transmitindo != transmitindo {
                    self.ptt_transmitindo = transmitindo;
                    self.manda(Comando::PttTransmitindo(transmitindo));
                }
            }
            Msg::PortalGlobalResultado(res) => match res {
                Ok(()) => self.portal_global_ativo = true,
                Err(_) => self.portal_global_ativo = false,
            },
            Msg::EventoIced(ev) => match ev {
                iced::Event::Window(iced::window::Event::FileDropped(path)) => {
                    return Task::perform(
                        async move {
                            let nome = path.file_name()?.to_string_lossy().to_string();
                            Some((nome, path))
                        },
                        Msg::ArquivoEscolhido,
                    );
                }
                iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
                    self.cursor_pos = position;
                    self.ultimo_movimento = std::time::Instant::now();
                }
                iced::Event::Window(iced::window::Event::Focused) => {
                    self.janela_focada = true;
                    self.nao_lidas = 0;
                    self.primeira_nao_lida_id = None;
                }
                iced::Event::Window(iced::window::Event::Unfocused) => {
                    self.janela_focada = false;
                    if self.capturando_ptt {
                        self.capturando_ptt = false;
                    }
                    if self.ptt_transmitindo {
                        self.ptt_transmitindo = false;
                        self.manda(Comando::PttTransmitindo(false));
                    }
                }
                iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key,
                    modifiers,
                    repeat,
                    ..
                }) => {
                    if (modifiers.command() || modifiers.control())
                        && modifiers.shift()
                        && matches!(&key, iced::keyboard::Key::Character(c) if c.eq_ignore_ascii_case("d"))
                    {
                        return Task::done(Msg::TogglePainelDiagnostico);
                    } else if (modifiers.command() || modifiers.control())
                        && modifiers.shift()
                        && matches!(&key, iced::keyboard::Key::Character(c) if c.eq_ignore_ascii_case("f"))
                    {
                        // Foca o campo de busca do cabeçalho.
                        return iced::widget::operation::focus("busca-chat");
                    } else if modifiers.shift()
                        && matches!(
                            key,
                            iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter)
                        )
                    {
                        self.rascunho.push('\n');
                        self.manda(Comando::Digitando(true));
                    } else if modifiers.command()
                        && matches!(&key, iced::keyboard::Key::Character(c) if c.eq_ignore_ascii_case("v"))
                    {
                        if let Some(res) = ui::chat::tenta_imagem_clipboard() {
                            match res {
                                Ok(pendente) => self.imagem_pendente = Some(pendente),
                                Err(e) => self.aviso = Some(e),
                            }
                        }
                    } else if self.capturando_ptt {
                        if matches!(
                            key,
                            iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape)
                        ) {
                            self.capturando_ptt = false;
                        } else if let Some((id, rotulo)) = ptt::descreve_tecla(&key) {
                            self.ptt.tecla = id;
                            self.ptt.rotulo = rotulo;
                            ptt::salva_config(&self.ptt);
                            self.capturando_ptt = false;
                        }
                    } else if matches!(
                        key,
                        iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape)
                    ) {
                        if self.busca_chat_aberta {
                            self.busca_chat_aberta = false;
                            self.busca_chat.clear();
                        } else if self.painel.is_some() {
                            self.painel = None;
                        } else if !self.sussurro_alvos.is_empty() {
                            self.sussurro_alvos.clear();
                            self.erro_sussurro = None;
                        } else if self.seletor_reacao.is_some() {
                            self.seletor_reacao = None;
                        } else if self.quadro.aberto {
                            self.quadro.aberto = false;
                        } else if self.roletas.aberto {
                            self.roletas.aberto = false;
                        } else if self.criador_enquete.aberto {
                            self.criador_enquete.fechar();
                        } else if self.editando.is_some() {
                            self.editando = None;
                            self.rascunho.clear();
                        } else if self.resposta_pendente.is_some() {
                            self.resposta_pendente = None;
                        } else if self.video_em_tela_cheia.is_some() {
                            self.video_em_tela_cheia = None;
                        }
                    } else if self.ptt.ativo
                        && self.na_chamada
                        && !repeat
                        && ptt::corresponde(&self.ptt.tecla, &key)
                        && !self.ptt_transmitindo
                    {
                        self.ptt_transmitindo = true;
                        self.manda(Comando::PttTransmitindo(true));
                    }
                }
                iced::Event::Keyboard(iced::keyboard::Event::KeyReleased { key, .. })
                    if self.ptt.ativo
                        && self.ptt_transmitindo
                        && ptt::corresponde(&self.ptt.tecla, &key) =>
                {
                    self.ptt_transmitindo = false;
                    self.manda(Comando::PttTransmitindo(false));
                }
                _ => {}
            },
            Msg::AbreConfigAudio => {
                let cfg = dispositivos::carrega_config();
                self.painel_audio = Some(PainelAudio {
                    entradas: Vec::new(),
                    saidas: Vec::new(),
                    entrada_selecionada: cfg.entrada_guid,
                    saida_selecionada: cfg.saida_guid,
                    testando_mic: false,
                    nivel_mic: 0.0,
                });
                self.manda(Comando::AtualizaDispositivosAudio);
            }
            Msg::FechaConfigAudio => {
                if let Some(pa) = &self.painel_audio
                    && pa.testando_mic
                {
                    self.manda(Comando::ParaTesteMic);
                }
                self.painel_audio = None;
            }
            Msg::SelecionaEntradaAudio(guid) => {
                if let Some(pa) = &mut self.painel_audio {
                    pa.entrada_selecionada = Some(guid.clone());
                }
                self.manda(Comando::SelecionaDispositivoGravacao(guid));
            }
            Msg::SelecionaSaidaAudio(guid) => {
                if let Some(pa) = &mut self.painel_audio {
                    pa.saida_selecionada = Some(guid.clone());
                }
                self.manda(Comando::SelecionaDispositivoReproducao(guid));
            }
            Msg::AlternaTesteMic => {
                if let Some(pa) = &mut self.painel_audio {
                    pa.testando_mic = !pa.testando_mic;
                    if pa.testando_mic {
                        self.manda(Comando::IniciaTesteMic);
                    } else {
                        pa.nivel_mic = 0.0;
                        self.manda(Comando::ParaTesteMic);
                    }
                }
            }
            Msg::FechaAviso => self.aviso = None,
            Msg::DispensaAvisoEco => self.aviso_eco = false,
            Msg::AlternaPainelGif => {
                self.alterna_painel(PainelFlutuante::Gif);
                if self.painel == Some(PainelFlutuante::Gif) {
                    return self.carrega_gifs_se_preciso();
                }
            }
            Msg::TrocaAbaSeletor(aba) => match aba {
                ui::seletor::AbaSeletor::Gif => {
                    self.painel = Some(PainelFlutuante::Gif);
                    return self.carrega_gifs_se_preciso();
                }
                ui::seletor::AbaSeletor::Emoji => self.painel = Some(PainelFlutuante::Emoji),
            },
            Msg::BuscaEmojiInput(t) => self.busca_emoji = t,
            Msg::BuscaGifInput(v) => {
                self.busca_gif = v;
            }
            Msg::SubmeteBuscaGif => {
                self.buscando_gif = true;
                self.erro_gif = None;
                let (srv, token, busca) = (
                    self.entrada.servidor.clone(),
                    self.entrada.token.clone(),
                    self.busca_gif.clone(),
                );
                return em_segundo_plano(
                    move || api::busca_gifs(&srv, &token, &busca).map_err(|e| format!("{e:#}")),
                    Msg::GifsCarregados,
                );
            }
            Msg::GifsCarregados(res) => {
                self.buscando_gif = false;
                match res {
                    Ok(lista) => {
                        let mut tarefas = Vec::new();
                        for item in lista.iter().take(16) {
                            if !self.previas_gif.contains_key(&item.url) {
                                tarefas
                                    .push(baixa_previa_gif(item.url.clone(), item.previa.clone()));
                            }
                        }
                        self.resultados_gif = lista;
                        return Task::batch(tarefas);
                    }
                    Err(e) => {
                        self.erro_gif = Some(e);
                    }
                }
            }
            Msg::PreviaGifBaixada { url, bytes } => {
                self.previas_gif
                    .insert(url, image::Handle::from_bytes(bytes));
            }
            Msg::GifBaixado { id, bytes } => {
                self.imagens.insert(id, image::Handle::from_bytes(bytes));
            }
            Msg::EnviaGif(url) => {
                let resposta_a = self.resposta_pendente.take();
                self.manda(Comando::Chat {
                    texto: String::new(),
                    imagem: None,
                    gif: Some(url),
                    resposta_a,
                    para: None,
                    enquete: None,
                });
                if self.painel == Some(PainelFlutuante::Gif) {
                    self.painel = None;
                }
            }
            Msg::VotaEnquete { id, opcao } => {
                self.manda(Comando::VotarEnquete { id, opcao });
            }
            Msg::AlternaMenuSussurro => self.alterna_painel(PainelFlutuante::Sussurro),
            Msg::AlternaAlvoSussurro { uid, nome } => {
                let alvo = ui::chat::AlvoSussurro { uid, nome };
                if let Some(pos) = self
                    .sussurro_alvos
                    .iter()
                    .position(|a| a.uid == alvo.uid && a.nome == alvo.nome)
                {
                    self.sussurro_alvos.remove(pos);
                } else {
                    self.sussurro_alvos.push(alvo);
                }
                self.erro_sussurro = None;
            }
            Msg::IniciaSussurro { uid, nome } => {
                self.sussurro_alvos = vec![ui::chat::AlvoSussurro { uid, nome }];
                self.painel = None;
                self.erro_sussurro = None;
            }
            Msg::LimpaSussurro => {
                self.sussurro_alvos.clear();
                self.erro_sussurro = None;
            }
            Msg::BuscaChatInput(v) => {
                // O campo do cabeçalho abre a barra de resultados ao digitar.
                self.busca_chat_aberta = !v.is_empty();
                self.busca_chat = v;
            }
            Msg::FechaBuscaChat => {
                self.busca_chat_aberta = false;
                self.busca_chat.clear();
            }
            Msg::LimpaNaoLidas => {
                self.nao_lidas = 0;
                self.primeira_nao_lida_id = None;
            }
            Msg::AlternarQuadro => {
                self.quadro.aberto = !self.quadro.aberto;
            }
            Msg::Wordle(m) => return self.wordle_update(m),
            Msg::Quadro(qmsg) => match qmsg {
                ui::quadro::QuadroMsg::NovoPonto(x, y) => {
                    if let Some(pontos) = &mut self.quadro.traco_em_andamento {
                        pontos.push(x);
                        pontos.push(y);
                    } else {
                        self.quadro.traco_em_andamento = Some(vec![x, y]);
                    }
                }
                ui::quadro::QuadroMsg::FinalizarTraco => {
                    if let Some(pontos) = self.quadro.traco_em_andamento.take()
                        && pontos.len() >= 2
                    {
                        let traco = ui::quadro::Traco {
                            c: self.quadro.cor_atual.clone(),
                            l: self.quadro.espessura_atual,
                            p: pontos,
                            b: if self.quadro.borracha { 1 } else { 0 },
                        };
                        self.quadro.adicionar_traco(traco.clone());
                        self.manda(Comando::EnviarTraco(traco));
                    }
                }
                ui::quadro::QuadroMsg::MudarCor(cor) => {
                    self.quadro.cor_atual = cor;
                    self.quadro.borracha = false;
                }
                ui::quadro::QuadroMsg::MudarEspessura(esp) => {
                    self.quadro.espessura_atual = esp;
                }
                ui::quadro::QuadroMsg::AlternarBorracha => {
                    self.quadro.borracha = !self.quadro.borracha;
                }
                ui::quadro::QuadroMsg::LimparQuadro => {
                    self.quadro.limpar();
                    self.manda(Comando::LimparQuadro);
                }
                ui::quadro::QuadroMsg::Fechar => {
                    self.quadro.aberto = false;
                }
            },
            Msg::AlternarRoletas => {
                self.roletas.aberto = !self.roletas.aberto;
            }
            Msg::Roleta(rmsg) => match rmsg {
                ui::roleta::RoletaMsg::Selecionar(id) => {
                    self.roletas.selecionar(id);
                }
                ui::roleta::RoletaMsg::NovaRoleta => {
                    self.roletas.nova_roleta();
                }
                ui::roleta::RoletaMsg::RascunhoNome(n) => {
                    self.roletas.rascunho_nome = n;
                }
                ui::roleta::RoletaMsg::RascunhoOpcoes(o) => {
                    self.roletas.rascunho_opcoes = o;
                }
                ui::roleta::RoletaMsg::Salvar => {
                    let nome = self.roletas.rascunho_nome.trim().to_string();
                    let opcoes: Vec<String> = self
                        .roletas
                        .rascunho_opcoes
                        .split(['\n', ','])
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                    if opcoes.len() < 2 {
                        self.roletas.erro =
                            Some("A roleta precisa de pelo menos duas opções.".to_string());
                    } else {
                        self.roletas.erro = None;
                        let nome_final = if nome.is_empty() {
                            "Roleta".to_string()
                        } else {
                            nome
                        };
                        if let Some(r) = self.roletas.roleta_atual()
                            && self.roletas.roleta_selecionada_id.is_some()
                        {
                            self.manda(Comando::EditarRoleta {
                                id: r.id.clone(),
                                nome: nome_final,
                                opcoes,
                            });
                        } else {
                            self.manda(Comando::CriarRoleta {
                                nome: nome_final,
                                opcoes,
                            });
                        }
                    }
                }
                ui::roleta::RoletaMsg::Apagar(id) => {
                    self.manda(Comando::ApagarRoleta(id));
                    self.roletas.nova_roleta();
                }
                ui::roleta::RoletaMsg::Girar(id) => {
                    self.manda(Comando::GirarRoleta(id));
                }
                ui::roleta::RoletaMsg::TickAnimacao => {
                    self.roletas.atualizar_animacao();
                }
                ui::roleta::RoletaMsg::Fechar => {
                    self.roletas.aberto = false;
                }
            },
            Msg::AlternarCriadorEnquete => {
                if self.criador_enquete.aberto {
                    self.criador_enquete.fechar();
                } else {
                    self.criador_enquete.abrir();
                }
            }
            Msg::Enquete(emsg) => match emsg {
                ui::enquete::EnqueteMsg::Pergunta(p) => {
                    self.criador_enquete.pergunta = p;
                }
                ui::enquete::EnqueteMsg::Opcao(i, val) => {
                    if let Some(op) = self.criador_enquete.opcoes.get_mut(i) {
                        *op = val;
                    }
                }
                ui::enquete::EnqueteMsg::AdicionarOpcao => {
                    self.criador_enquete.adicionar_opcao();
                }
                ui::enquete::EnqueteMsg::RemoverOpcao(i) => {
                    self.criador_enquete.remover_opcao(i);
                }
                ui::enquete::EnqueteMsg::Criar => match self.criador_enquete.validar() {
                    Ok((pergunta, opcoes)) => {
                        self.manda(Comando::Chat {
                            texto: format!("/enquete {pergunta}"),
                            imagem: None,
                            gif: None,
                            resposta_a: None,
                            para: None,
                            enquete: Some(sala::Enquete { pergunta, opcoes }),
                        });
                        self.criador_enquete.fechar();
                    }
                    Err(err) => {
                        self.criador_enquete.erro = Some(err);
                    }
                },
                ui::enquete::EnqueteMsg::Fechar => {
                    self.criador_enquete.fechar();
                }
            },
            _ => {}
        }
        Task::none()
    }
}
