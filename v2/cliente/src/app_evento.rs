//! `App::evento_da_sala`: reage aos eventos do ator de rede.

use super::*;

impl App {
    pub(crate) fn evento_da_sala(&mut self, ev: Evento) -> Task<Msg> {
        let Tela::Sala(s) = &mut self.tela else {
            return Task::none();
        };
        match ev {
            Evento::Pronta(tx) => {
                let ptt_ativo = s.ptt.ativo;
                s.comandos = Some(tx);
                s.manda(Comando::PttConfigurado { ativo: ptt_ativo });
                if let Some(atv) = &self.atividade_atual {
                    s.manda(Comando::TrocarAtividade(Some(atv.clone())));
                }
            }
            Evento::PttTransmitindo(t) => s.ptt_transmitindo = t,
            Evento::Entrou { meu_id } => {
                s.meu_id = meu_id;
                if self.ligar_ao_abrir.as_deref() == Some(s.entrada.sala.as_str()) {
                    let id = s.entrada.sala.clone();
                    self.ligar_ao_abrir = None;
                    if let Some(tx) = &self.avisos_cmd {
                        let _ = tx.send(avisos::Comando::Ligar(id));
                    }
                    s.manda(sala::Comando::EntrarNaChamada);
                }
            }
            Evento::Pessoas(p) => {
                if let Some(eu) = p.iter().find(|x| x.eu)
                    && eu.uid.is_some()
                {
                    s.meu_uid = eu.uid.clone();
                }
                let ids: std::collections::HashSet<&str> =
                    p.iter().map(|x| x.id.as_str()).collect();
                s.digitando_remotos
                    .retain(|id, _| ids.contains(id.as_str()));
                s.pessoas = p;
            }
            Evento::Historico(h) => {
                s.mensagens = h;
                // Divisor "novas" no ponto onde a pessoa parou.
                let n = std::mem::take(&mut s.novas_ao_abrir);
                if n > 0 && n <= s.mensagens.len() {
                    s.primeira_nao_lida_id = Some(s.mensagens[s.mensagens.len() - n].id.clone());
                }
                // Um resumo por rodada que aparece no histórico.
                let mut dias: Vec<String> = s
                    .mensagens
                    .iter()
                    .filter(|m| m.extra.app.as_deref() == Some("wordle"))
                    .filter_map(|m| m.extra.wordle_dia.clone())
                    .collect();
                dias.sort();
                dias.dedup();
                let imagens = s.indexa_imagens();
                return Task::batch(std::iter::once(imagens).chain(dias.into_iter().map(|d| {
                    Task::done(Msg::Wordle(ui::wordle::WordleMsg::CarregaGrupo(Some(d))))
                })));
            }
            Evento::Chat(m) => {
                let eh_minha =
                    ui::chat::eh_minha_mensagem(&m, &s.entrada.nome, s.meu_uid.as_deref());

                if !eh_minha {
                    if !s.janela_focada {
                        s.nao_lidas += 1;
                        if s.primeira_nao_lida_id.is_none() {
                            s.primeira_nao_lida_id = Some(m.id.clone());
                        }
                    }
                    // Menção e mensagem comum notificam pelo canal de avisos
                    // (respeita silêncio e preferências); sussurro não passa por lá.
                    if m.sussurro {
                        let (titulo, corpo) = ui::chat::prepara_notificacao_chat(
                            ui::chat::TipoNotificacaoChat::Sussurro,
                            &s.entrada.sala,
                            &m.autor,
                            &m.texto,
                            m.imagem.is_some(),
                            m.gif.is_some(),
                            m.enquete.is_some(),
                        );
                        ui::chat::dispara_notificacao_desktop(&titulo, &corpo, "normal");
                    }
                }
                let dia_wordle = (m.extra.app.as_deref() == Some("wordle"))
                    .then(|| m.extra.wordle_dia.clone())
                    .flatten();
                s.mensagens.push(*m);
                // A v1 guarda as últimas 300; acima disso a tela só pesa.
                if s.mensagens.len() > 500 {
                    s.mensagens.drain(..100);
                }
                let imagens = s.indexa_imagens();
                return match dia_wordle {
                    Some(d) => Task::batch([
                        imagens,
                        Task::done(Msg::Wordle(ui::wordle::WordleMsg::CarregaGrupo(Some(d)))),
                    ]),
                    None => imagens,
                };
            }
            Evento::MensagemEditada {
                id,
                texto,
                editada_em,
                mencoes,
                mencoes_uids,
            } => {
                if let Some(m) = s.mensagens.iter_mut().find(|m| m.id == id) {
                    m.texto = texto;
                    m.editada_em = Some(editada_em);
                    m.mencoes = mencoes;
                    m.mencoes_uids = mencoes_uids;
                }
            }
            Evento::MensagemApagada(id) => {
                s.mensagens.retain(|m| m.id != id);
                s.imagens.remove(&id);
                if s.editando.as_ref().map(|e| &e.id) == Some(&id) {
                    s.editando = None;
                    s.rascunho.clear();
                }
                if s.mensagem_fixada.as_ref().map(|f| &f.id) == Some(&id) {
                    s.mensagem_fixada = None;
                }
            }
            Evento::MensagemFixada(fixada) => {
                s.mensagem_fixada = fixada;
            }
            Evento::Digitando {
                de,
                nome,
                digitando,
            } => {
                if digitando {
                    s.digitando_remotos.insert(
                        de,
                        (
                            nome,
                            std::time::Instant::now() + std::time::Duration::from_secs(6),
                        ),
                    );
                } else {
                    s.digitando_remotos.remove(&de);
                }
            }
            Evento::ReacoesAtualizadas { id, reacoes } => {
                if let Some(m) = s.mensagens.iter_mut().find(|m| m.id == id) {
                    m.reacoes = reacoes;
                }
            }
            Evento::Voto { id, votos } => {
                if let Some(m) = s.mensagens.iter_mut().find(|m| m.id == id) {
                    m.votos = votos;
                }
            }
            Evento::Quadro(q) => {
                let imagem = image::Handle::from_rgba(q.largura, q.altura, q.rgba);
                match s
                    .videos
                    .iter_mut()
                    .find(|v| v.dono == q.dono && v.de_tela == q.de_tela)
                {
                    Some(v) => {
                        v.imagem = imagem;
                        v.largura = q.largura;
                        v.altura = q.altura;
                    }
                    None => s.videos.push(Video {
                        dono: q.dono,
                        de_tela: q.de_tela,
                        largura: q.largura,
                        altura: q.altura,
                        imagem,
                    }),
                }
            }
            Evento::VideoFechou { dono, de_tela } => {
                s.videos
                    .retain(|v| !(v.dono == dono && v.de_tela == de_tela));
                if s.video_fixado.as_ref() == Some(&(dono.clone(), de_tela)) {
                    s.video_fixado = None;
                }
                if s.video_em_tela_cheia.as_ref() == Some(&(dono, de_tela)) {
                    s.video_em_tela_cheia = None;
                }
            }
            Evento::QuadroPrevia(q) => {
                if let Some(prev) = &mut s.previa_camera {
                    prev.imagem = Some(image::Handle::from_rgba(q.largura, q.altura, q.rgba));
                }
            }
            Evento::MeuEstado {
                na_chamada,
                mudo,
                surdo,
                apresentando,
                com_camera,
                mudo_forcado,
            } => {
                s.na_chamada = na_chamada;
                if na_chamada && s.chamada_desde.is_none() {
                    s.chamada_desde = Some(std::time::Instant::now());
                } else if !na_chamada {
                    s.chamada_desde = None;
                }
                s.mudo = mudo;
                s.surdo = surdo;
                s.apresentando = apresentando;
                s.com_camera = com_camera;
                s.mudo_forcado = mudo_forcado;
                self.atualiza_bandeja();
            }
            Evento::DispositivosAudio { entradas, saidas } => {
                let cfg = dispositivos::carrega_config();
                let entrada_sel = cfg
                    .entrada_guid
                    .or_else(|| entradas.first().map(|d| d.guid.clone()));
                let saida_sel = cfg
                    .saida_guid
                    .or_else(|| saidas.first().map(|d| d.guid.clone()));
                if let Some(pa) = &mut s.painel_audio {
                    pa.entradas = entradas.clone();
                    pa.saidas = saidas.clone();
                    if pa.entrada_selecionada.is_none() {
                        pa.entrada_selecionada = entrada_sel.clone();
                    }
                    if pa.saida_selecionada.is_none() {
                        pa.saida_selecionada = saida_sel.clone();
                    }
                }
                self.painel_audio.entradas = entradas;
                self.painel_audio.saidas = saidas;
                if self.painel_audio.entrada_selecionada.is_none() {
                    self.painel_audio.entrada_selecionada = entrada_sel;
                }
                if self.painel_audio.saida_selecionada.is_none() {
                    self.painel_audio.saida_selecionada = saida_sel;
                }
            }
            Evento::NivelMic(nivel) => {
                s.nivel_mic_atual = nivel;
                if let Some(pa) = &mut s.painel_audio {
                    pa.nivel_mic = nivel;
                }
                self.painel_audio.nivel_mic = nivel;
            }
            Evento::Falando { id, falando } => {
                if falando {
                    s.ultimo_falante = Some(id.clone());
                }
                if let Some(p) = s.pessoas.iter_mut().find(|p| p.id == id) {
                    p.falando = falando;
                }
            }
            Evento::FalandoNoMudo(ativo) => {
                s.falando_no_mudo = ativo;
            }
            Evento::AvisoDeEco => {
                s.aviso_eco = true;
            }
            Evento::QualidadeConexao { id, qualidade } => {
                if let Some(p) = s.pessoas.iter_mut().find(|p| p.id == id) {
                    p.qualidade = Some(qualidade);
                }
            }
            Evento::Aviso(a) => s.aviso = Some(a),
            Evento::Recusada(motivo) => {
                let tarefa = self.vai_pras_salas();
                if let Tela::Salas(salas) = &mut self.tela {
                    salas.recusa = Some(format!("Não deu pra entrar na conversa: {motivo}."));
                }
                return tarefa;
            }
            Evento::Caiu(motivo) => {
                s.aviso = Some(format!("Conexão com a sala perdida ({motivo})."));
                s.comandos = None;
                s.na_chamada = false;
                if s.painel == Some(PainelFlutuante::Sons) {
                    s.painel = None;
                }
                s.videos.clear();
            }
            Evento::InfoSala { sou_dono } => {
                s.sou_dono = sou_dono;
            }
            Evento::ReacaoFlutuante { de, emoji } => {
                s.id_reacao_seq = s.id_reacao_seq.wrapping_add(1);
                let x_frac = 0.20 + ((s.id_reacao_seq * 17) % 60) as f32 / 100.0;
                s.reacoes_flutuantes.push(ui::reacao::ReacaoAtiva {
                    id: s.id_reacao_seq,
                    emoji,
                    quem: de,
                    x_frac,
                    tempo_restante_ms: 2600,
                });
            }
            Evento::WordleGrupoMudou(dia) => {
                // Só se a rodada aparece nesta conversa.
                let tem = s.mensagens.iter().any(|m| {
                    m.extra.app.as_deref() == Some("wordle")
                        && m.extra.wordle_dia.as_deref() == Some(dia.as_str())
                });
                if tem {
                    return Task::done(Msg::Wordle(ui::wordle::WordleMsg::CarregaGrupo(Some(dia))));
                }
            }
            Evento::CutucadaRecebida { de_nome } => {
                s.aviso_cutucada = Some(ui::reacao::AvisoCutucada {
                    texto: format!("{de_nome} te cutucou!"),
                    tempo_restante_ms: 2500,
                });
            }
            Evento::CutucadaEnviada { para_nome } => {
                s.aviso_cutucada = Some(ui::reacao::AvisoCutucada {
                    texto: format!("Você cutucou {para_nome}."),
                    tempo_restante_ms: 2000,
                });
            }
            Evento::SomRecebido {
                de_id,
                de_nome,
                id,
                vol,
            } => {
                if !s.soundboard.esta_silenciado(&de_nome) && !s.soundboard.esta_silenciado(&de_id)
                {
                    som::toca_som(&id, vol);
                }
            }
            Evento::QuadroEstado(tracos) => {
                s.quadro.carregar_estado(tracos);
            }
            Evento::QuadroTraco(traco) => {
                s.quadro.adicionar_traco(traco);
            }
            Evento::QuadroLimpo { quem } => {
                s.quadro.limpar();
                s.mensagens.push(sala::Mensagem {
                    id: format!("quadro-limpo-{}", fastrand::u64(..)),
                    autor: String::new(),
                    uid: None,
                    texto: format!("{quem} limpou o quadro branco."),
                    ts: chrono::Utc::now().timestamp_millis(),
                    editada_em: None,
                    imagem: None,
                    gif: None,
                    resposta_a: None,
                    reacoes: std::collections::HashMap::new(),
                    mencoes: Vec::new(),
                    mencoes_uids: Vec::new(),
                    sussurro: false,
                    sussurro_para: Vec::new(),
                    enquete: None,
                    votos: std::collections::HashMap::new(),
                    sistema: Some("grupo".to_string()),
                    extra: Default::default(),
                });
            }
            Evento::Roletas(lista) => {
                s.roletas.atualizar_roletas(lista);
            }
            Evento::RoletaResultado { id, indice, por } => {
                s.roletas.iniciar_giro(&id, indice, por);
            }
        }
        Task::none()
    }

    pub(crate) fn evento_da_chamada_ativa(&mut self, ev: Evento) -> Task<Msg> {
        let Some(c) = &mut self.chamada_ativa else {
            return Task::none();
        };
        match ev {
            Evento::Pronta(tx) => {
                let ptt_ativo = c.ptt.ativo;
                c.comandos = Some(tx);
                c.manda(Comando::PttConfigurado { ativo: ptt_ativo });
                if let Some(atv) = &self.atividade_atual {
                    c.manda(Comando::TrocarAtividade(Some(atv.clone())));
                }
            }
            Evento::PttTransmitindo(t) => c.ptt_transmitindo = t,
            Evento::Entrou { meu_id } => {
                c.meu_id = meu_id;
            }
            Evento::Pessoas(p) => {
                if let Some(eu) = p.iter().find(|x| x.eu)
                    && eu.uid.is_some()
                {
                    c.meu_uid = eu.uid.clone();
                }
                c.pessoas = p;
            }
            Evento::MeuEstado {
                na_chamada,
                mudo,
                surdo,
                apresentando,
                com_camera,
                mudo_forcado,
            } => {
                c.na_chamada = na_chamada;
                if na_chamada && c.chamada_desde.is_none() {
                    c.chamada_desde = Some(std::time::Instant::now());
                } else if !na_chamada {
                    c.chamada_desde = None;
                }
                c.mudo = mudo;
                c.surdo = surdo;
                c.apresentando = apresentando;
                c.com_camera = com_camera;
                c.mudo_forcado = mudo_forcado;
                self.atualiza_bandeja();
            }
            Evento::Falando { id, falando } => {
                if falando {
                    c.ultimo_falante = Some(id.clone());
                }
                if let Some(p) = c.pessoas.iter_mut().find(|p| p.id == id) {
                    p.falando = falando;
                }
            }
            Evento::DispositivosAudio { entradas, saidas } => {
                let cfg = dispositivos::carrega_config();
                let entrada_sel = cfg
                    .entrada_guid
                    .or_else(|| entradas.first().map(|d| d.guid.clone()));
                let saida_sel = cfg
                    .saida_guid
                    .or_else(|| saidas.first().map(|d| d.guid.clone()));
                if let Some(pa) = &mut c.painel_audio {
                    pa.entradas = entradas.clone();
                    pa.saidas = saidas.clone();
                    if pa.entrada_selecionada.is_none() {
                        pa.entrada_selecionada = entrada_sel.clone();
                    }
                    if pa.saida_selecionada.is_none() {
                        pa.saida_selecionada = saida_sel.clone();
                    }
                }
                self.painel_audio.entradas = entradas;
                self.painel_audio.saidas = saidas;
                if self.painel_audio.entrada_selecionada.is_none() {
                    self.painel_audio.entrada_selecionada = entrada_sel;
                }
                if self.painel_audio.saida_selecionada.is_none() {
                    self.painel_audio.saida_selecionada = saida_sel;
                }
            }
            Evento::NivelMic(nivel) => {
                c.nivel_mic_atual = nivel;
                if let Some(pa) = &mut c.painel_audio {
                    pa.nivel_mic = nivel;
                }
                self.painel_audio.nivel_mic = nivel;
            }
            Evento::Quadro(q) => {
                let imagem = image::Handle::from_rgba(q.largura, q.altura, q.rgba);
                match c
                    .videos
                    .iter_mut()
                    .find(|v| v.dono == q.dono && v.de_tela == q.de_tela)
                {
                    Some(v) => {
                        v.imagem = imagem;
                        v.largura = q.largura;
                        v.altura = q.altura;
                    }
                    None => c.videos.push(Video {
                        dono: q.dono,
                        de_tela: q.de_tela,
                        largura: q.largura,
                        altura: q.altura,
                        imagem,
                    }),
                }
            }
            Evento::VideoFechou { dono, de_tela } => {
                c.videos
                    .retain(|v| !(v.dono == dono && v.de_tela == de_tela));
            }
            Evento::QualidadeConexao { id, qualidade } => {
                if let Some(p) = c.pessoas.iter_mut().find(|p| p.id == id) {
                    p.qualidade = Some(qualidade);
                }
            }
            Evento::Caiu(_) | Evento::Recusada(_) => {
                self.chamada_ativa = None;
                self.atualiza_bandeja();
            }
            Evento::Chat(m) => {
                let eh_minha =
                    ui::chat::eh_minha_mensagem(&m, &c.entrada.nome, c.meu_uid.as_deref());
                if !eh_minha && m.sussurro {
                    let (titulo, corpo) = ui::chat::prepara_notificacao_chat(
                        ui::chat::TipoNotificacaoChat::Sussurro,
                        &c.entrada.sala,
                        &m.autor,
                        &m.texto,
                        m.imagem.is_some(),
                        m.gif.is_some(),
                        m.enquete.is_some(),
                    );
                    ui::chat::dispara_notificacao_desktop(&titulo, &corpo, "normal");
                }
                c.mensagens.push(*m);
                if c.mensagens.len() > 500 {
                    c.mensagens.drain(..100);
                }
            }
            Evento::SomRecebido {
                de_id,
                de_nome,
                id,
                vol,
            } if !c.soundboard.esta_silenciado(&de_nome)
                && !c.soundboard.esta_silenciado(&de_id) =>
            {
                som::toca_som(&id, vol);
            }
            _ => {}
        }
        if let Some(c) = &self.chamada_ativa
            && !c.na_chamada
        {
            self.chamada_ativa = None;
            self.atualiza_bandeja();
        }
        Task::none()
    }

    pub(crate) fn evento_da_sala_id(&mut self, id: &str, ev: Evento) -> Task<Msg> {
        if let Tela::Sala(s) = &self.tela
            && s.entrada.sala == id
        {
            return self.evento_da_sala(ev);
        }
        if self
            .chamada_ativa
            .as_ref()
            .is_some_and(|c| c.entrada.sala == id)
        {
            return self.evento_da_chamada_ativa(ev);
        }
        Task::none()
    }
}
