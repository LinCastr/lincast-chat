//! `App`: construção, título e bandeja.

use super::*;

impl App {
    /// Porta de entrada do `update`: trata a mensagem e, em seguida, pede as
    /// fotos que ficaram faltando (a tela só desenha, nunca busca).
    pub(crate) fn update(&mut self, msg: Msg) -> Task<Msg> {
        let tarefa = self.atualiza(msg);
        let pedidos = self.pede_imagens();
        if pedidos.is_empty() {
            tarefa
        } else {
            Task::batch([tarefa, Task::batch(pedidos)])
        }
    }

    /// Endereços de foto que as telas abertas vão querer desenhar.
    fn urls_de_imagens(&self) -> Vec<String> {
        let mut v: Vec<String> = Vec::new();
        v.extend(self.eu.as_ref().and_then(|e| e.avatar.clone()));
        v.extend(
            self.modal_outro_perfil
                .as_ref()
                .and_then(|p| p.avatar.clone()),
        );
        // Capas dos jogos das listas (minhas e do perfil aberto).
        let capas = |l: &api::ListasJogos| -> Vec<String> {
            l.favorito
                .iter()
                .chain(&l.gosto)
                .chain(&l.quero)
                .map(|j| api::capa_url(j.id))
                .collect()
        };
        if let Some(ed) = &self.modal_meu_perfil {
            v.extend(capas(&ed.jogos.listas));
        }
        if let Some(p) = &self.modal_outro_perfil {
            v.extend(capas(&p.listas));
        }
        // Capas de música das atividades à vista (precisam ser https).
        let artes = |a: &api::Atividade| a.musica.as_ref().and_then(|m| m.arte.clone());
        for val in self.avisos.atividade_amigos.values() {
            if let Ok(a) = serde_json::from_value::<api::Atividade>(val.clone()) {
                v.extend(artes(&a));
            }
        }
        if let Some(a) = &self.atividade_atual {
            v.extend(artes(a));
        }
        // Capas dos jogos: da atividade (minha e dos amigos) e da lista registrada.
        let capa_do_jogo =
            |a: &api::Atividade| a.jogo.as_ref().and_then(|j| j.capa).map(api::capa_url);
        for val in self.avisos.atividade_amigos.values() {
            if let Ok(a) = serde_json::from_value::<api::Atividade>(val.clone()) {
                v.extend(capa_do_jogo(&a));
            }
        }
        if let Some(a) = &self.atividade_atual {
            v.extend(capa_do_jogo(a));
        }
        if self.modal_config == Some(ui::configuracoes::AbaConfig::JogosRegistrados) {
            v.extend(
                self.jogos_reg
                    .iter()
                    .filter_map(|j| j.capa)
                    .map(api::capa_url),
            );
        }
        for c in &self.avisos.conversas {
            v.extend(c.icone.clone().filter(|u| u.starts_with('/')));
            for m in &c.membros {
                v.extend(m.avatar.clone());
                v.extend(m.banner_url.clone());
            }
        }
        // Prévias de GIF e GIF escolhido na janela de editar grupo.
        if let Some(m) = &self.modal_editar_grupo {
            v.extend(m.resultados_gif.iter().map(|g| g.previa.clone()));
            if let Some(ui::editar_grupo::IconeNovo::Gif(u)) = &m.novo_icone {
                v.push(u.clone());
            }
        }
        match &self.tela {
            Tela::Sala(s) => v.extend(s.pessoas.iter().filter_map(|p| p.avatar.clone())),
            Tela::Salas(s) => {
                if let Some(a) = &s.amigos {
                    for c in a.amigos.iter().chain(&a.recebidos).chain(&a.enviados) {
                        v.extend(c.avatar.clone());
                    }
                }
            }
            _ => {}
        }
        v
    }

    /// Pede as capas dos jogos das listas abertas (minhas e do perfil que estou vendo).
    /// O toque deve soar? Para quem está sendo chamado, enquanto houver toque.
    /// Para quem chamou: enquanto ninguém além de mim estiver na chamada e
    /// ainda houver gente sendo chamada (alguém entrou ou parou de chamar: cala).
    pub(crate) fn deve_tocar(&self) -> bool {
        if !self.avisos.toques.is_empty() {
            return true;
        }
        let Some(meu) = self.avisos.meu_uid() else {
            return false;
        };
        self.avisos.chamando.iter().any(|(conv, chamando)| {
            !chamando.is_empty()
                && self.avisos.chamadas.get(conv).is_some_and(|dentro| {
                    dentro.iter().any(|u| u == meu) && dentro.iter().all(|u| u == meu)
                })
        })
    }

    pub(crate) fn pede_capas(&self) -> Task<Msg> {
        Task::batch(self.pede_imagens())
    }

    /// Grava as minhas listas de jogos no servidor.
    pub(crate) fn salva_listas(&self) -> Task<Msg> {
        let (Some(token), Some(ed)) = (self.token.clone(), &self.modal_meu_perfil) else {
            return Task::none();
        };
        let (srv, listas) = (self.srv.clone(), ed.jogos.listas.clone());
        em_segundo_plano(
            move || api::salva_listas(&srv, &token, &listas).map_err(|e| format!("{e:#}")),
            Msg::JogosSalvos,
        )
    }

    fn pede_imagens(&self) -> Vec<Task<Msg>> {
        self.urls_de_imagens()
            .into_iter()
            .filter(|u| avatar::precisa_baixar(u))
            .map(|u| baixa_imagem_de_perfil(self.srv.clone(), self.token.clone(), u))
            .collect()
    }

    pub(crate) fn novo() -> (Self, Task<Msg>) {
        let srv = Servidor::padrao();
        std::thread::spawn(avatar::limpa_cache_antigo);
        let token = api::sessao_salva();
        let tarefa = match &token {
            Some(t) => {
                let (s, t) = (srv.clone(), t.clone());
                em_segundo_plano(
                    move || api::eu(&s, &t).map_err(|e| format!("{e:#}")),
                    Msg::Sessao,
                )
            }
            None => Task::done(Msg::Sessao(Ok(None))),
        };
        let ptt = ptt::carrega_config();
        let cfg_audio = dispositivos::carrega_config();
        let cameras = captura_camera::lista_cameras();
        let camera_selecionada = cameras.first().map(|c| c.indice);
        let painel_audio = PainelAudio {
            entradas: Vec::new(),
            saidas: Vec::new(),
            entrada_selecionada: cfg_audio.entrada_guid,
            saida_selecionada: cfg_audio.saida_guid,
            testando_mic: false,
            nivel_mic: 0.0,
        };
        let cfg_aparencia = tema::carrega_aparencia_salva();
        let cfg_bandeja = ui::bandeja::carrega_config();
        (
            App {
                srv,
                token,
                eu: None,
                tela: Tela::Carregando,
                modal_meu_perfil: None,
                modal_outro_perfil: None,
                ver_perfil: Default::default(),
                atividade_atual: None,
                atividade_enviada_em: None,
                jogos_reg: Vec::new(),
                jogos_reg_filtro: String::new(),
                processos_abertos: Vec::new(),
                painel_tema_aberto: false,
                modal_config: None,
                editar_perfil_janela: false,
                painel_audio,
                cameras,
                camera_selecionada,
                espelhar_camera: false,
                perfil_tela: captura_tela::PerfilTela::PADRAO,
                ptt,
                capturando_ptt: false,
                portal_global_ativo: false,
                densidade: cfg_aparencia.densidade,
                painel_diag_aberto: false,
                metricas_servidor: None,
                carregando_metricas: false,
                modal_confirmacao: None,
                modal_manual_aberto: false,
                filtro_manual: String::new(),
                minimizar_para_bandeja: cfg_bandeja.minimizar_para_bandeja,
                handle_bandeja: None,
                atualizacao: None,
                atualizacao_status: None,
                atualizando: false,
                tentou_auto: false,
                avisos: avisos::EstadoAvisos::default(),
                filtro_conversas: String::new(),
                menu_status_aberto: false,
                pedidos_pendentes: 0,
                titulos_extra: HashMap::new(),
                menu_contexto: None,
                atalhos: atalhos::Atalhos::carrega(),
                capturando_atalho: None,
                aviso_atalho: None,
                toque_sonoro: None,
                janela_focada: true,
                modal_grupo: None,
                modal_status_custom: None,
                presenca_aberta: false,
                presenca_fecha_em: None,
                volume_mic: None,
                volume_saida: None,
                modal_encaminhar: None,
                modal_favoritos: None,
                toast: None,
                modal_editar_grupo: None,
                amigos_cache: None,
                erro_global: None,
                cursor: iced::Point::ORIGIN,
                janela: iced::Size::new(1100.0, 720.0),
                prefs_conversas: prefs_conversas::PrefsConversas::carrega(),
                ligar_ao_abrir: None,
                abrir_apos_sair: None,
                avisos_cmd: None,
            },
            Task::batch([
                tarefa,
                Task::done(Msg::VarreAtividade),
                Task::done(Msg::ChecaAtualizacao),
            ]),
        )
    }

    pub(crate) fn titulo(&self) -> String {
        match &self.tela {
            Tela::Sala(s) => format!("{} — Lincast Chat", s.entrada.sala),
            _ => "Lincast Chat".into(),
        }
    }

    /// A conversa "à vista" é a aberta com a janela em foco; só ela não conta
    /// não lidas. Ao ganhar foco, o que chegou nela deixa de ser pendente.
    pub(crate) fn sincroniza_conversa_ativa(&mut self) {
        self.avisos.conversa_ativa = match &self.tela {
            Tela::Sala(s) if self.janela_focada => Some(s.entrada.sala.clone()),
            _ => None,
        };
        if let Some(id) = self.avisos.conversa_ativa.clone() {
            self.avisos.marca_lida(&id);
        }
    }

    /// Notificação do sistema para mensagem de conversa fora de vista,
    /// respeitando "Silenciar conversa" e as preferências de notificação.
    pub(crate) fn notifica_mensagem(&self, ev: &avisos::Evento) {
        let avisos::Evento::Mensagem {
            conversa,
            autor,
            uid,
            previa,
            sys,
            ..
        } = ev
        else {
            return;
        };
        let minha = uid
            .as_deref()
            .is_some_and(|u| Some(u) == self.avisos.meu_uid());
        let a_vista = self.avisos.conversa_ativa.as_deref() == Some(conversa.as_str());
        if sys.is_some() || minha || a_vista {
            return;
        }
        let meu = self.eu.as_ref().and_then(|e| e.usuario.as_deref());
        let mencao = meu.is_some_and(|u| {
            previa
                .to_lowercase()
                .contains(&format!("@{}", u.to_lowercase()))
        });
        let silenciada = self.prefs_conversas.silenciadas.contains(conversa);
        let permite = if mencao {
            self.prefs_conversas.notificar_mencoes
        } else {
            !silenciada && self.prefs_conversas.notificar_mensagens
        };
        if !permite {
            return;
        }
        let nome_conversa = self
            .avisos
            .conversas
            .iter()
            .find(|c| &c.id == conversa)
            .map(|c| nome_da_conversa(c, meu))
            .unwrap_or_else(|| "Conversa".into());
        let autor = autor.as_deref().unwrap_or("Alguém");
        let (tipo, urgencia) = if mencao {
            (ui::chat::TipoNotificacaoChat::Mencao, "normal")
        } else {
            (ui::chat::TipoNotificacaoChat::Mensagem, "low")
        };
        let (titulo, corpo) = ui::chat::prepara_notificacao_chat(
            tipo,
            &nome_conversa,
            autor,
            previa,
            false,
            false,
            false,
        );
        ui::chat::dispara_notificacao_desktop(&titulo, &corpo, urgencia);
    }

    pub(crate) fn vai_pras_salas(&mut self) -> Task<Msg> {
        self.tela = Tela::Salas(Salas::default());
        self.atualiza_bandeja();
        Task::done(Msg::CarregaAmigos)
    }

    pub(crate) fn atualiza_bandeja(&self) {
        if let Some(handle) = &self.handle_bandeja {
            let (mutado, em_chamada, sala) = match &self.tela {
                Tela::Sala(s) => (s.mudo, s.na_chamada, Some(s.entrada.sala.clone())),
                _ => (false, false, None),
            };
            let minimizar = self.minimizar_para_bandeja;
            let handle = handle.clone();
            tokio::spawn(async move {
                let _ = handle
                    .update(move |tray| {
                        tray.mutado = mutado;
                        tray.em_chamada = em_chamada;
                        tray.sala = sala;
                        tray.minimizar_para_bandeja = minimizar;
                    })
                    .await;
            });
        }
    }
}
