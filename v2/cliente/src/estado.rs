//! Estado da aplicação: tipos de `App`, das telas e das mensagens (`Msg`).

use super::*;

pub(crate) struct App {
    pub(crate) srv: Servidor,
    pub(crate) token: Option<String>,
    pub(crate) eu: Option<Eu>,
    pub(crate) tela: Tela,
    /// Sala cuja chamada de voz continua ativa enquanto o usuário navega em Amigos ou em outra conversa.
    pub(crate) chamada_ativa: Option<Box<SalaAberta>>,
    pub(crate) modal_meu_perfil: Option<ui::perfil::EdicaoPerfil>,
    pub(crate) modal_outro_perfil: Option<api::PerfilCompleto>,
    pub(crate) ver_perfil: ui::perfil::EstadoVerPerfil,
    pub(crate) atividade_atual: Option<api::Atividade>,
    /// Quando a atividade foi enviada ao servidor pela última vez (batimento: ele a expira em 90 s).
    pub(crate) atividade_enviada_em: Option<std::time::Instant>,
    /// Configurações → Jogos registrados: lista, filtro e programas abertos.
    pub(crate) jogos_reg: Vec<crate::jogos_registrados::JogoRegistrado>,
    pub(crate) jogos_reg_filtro: String,
    pub(crate) processos_abertos: Vec<String>,
    pub(crate) painel_tema_aberto: bool,
    pub(crate) modal_config: Option<ui::configuracoes::AbaConfig>,
    /// A janela própria "Editar perfil" (usa o mesmo estado de edição das configurações).
    pub(crate) editar_perfil_janela: bool,
    pub(crate) painel_audio: PainelAudio,
    pub(crate) cameras: Vec<captura_camera::DispositivoCamera>,
    pub(crate) camera_selecionada: Option<usize>,
    pub(crate) espelhar_camera: bool,
    pub(crate) perfil_tela: captura_tela::PerfilTela,
    pub(crate) ptt: PttConfig,
    pub(crate) capturando_ptt: bool,
    pub(crate) portal_global_ativo: bool,
    pub(crate) densidade: ui::layout::Densidade,
    pub(crate) painel_diag_aberto: bool,
    pub(crate) metricas_servidor: Option<api::MetricasServidor>,
    pub(crate) carregando_metricas: bool,
    pub(crate) modal_confirmacao: Option<ui::confirmacao::TipoConfirmacao>,
    pub(crate) modal_manual_aberto: bool,
    pub(crate) filtro_manual: String,
    pub(crate) minimizar_para_bandeja: bool,
    pub(crate) handle_bandeja: Option<ui::bandeja::HandleBandeja>,
    pub(crate) atualizacao: Option<ui::atualizacao::InfoAtualizacao>,
    /// Texto de andamento do botão "Verificar atualização" (Configurações → Sistema).
    pub(crate) atualizacao_status: Option<String>,
    pub(crate) atualizando: bool,
    /// A atualização automática da abertura só roda uma vez por execução.
    pub(crate) tentou_auto: bool,
    /// O que o canal de avisos sabe: conversas, não lidas, chamadas, toque.
    pub(crate) avisos: avisos::EstadoAvisos,
    /// Texto da busca da coluna da esquerda.
    pub(crate) filtro_conversas: String,
    pub(crate) menu_status_aberto: bool,
    pub(crate) pedidos_pendentes: usize,
    /// Nome de conversas recém-abertas que o canal de avisos ainda não listou.
    pub(crate) titulos_extra: HashMap<String, String>,
    pub(crate) menu_contexto: Option<MenuContexto>,
    pub(crate) atalhos: atalhos::Atalhos,
    /// Ação cujo atalho está esperando a próxima tecla.
    pub(crate) capturando_atalho: Option<String>,
    pub(crate) aviso_atalho: Option<String>,
    /// Toque de chamada tocando agora (para quando largar, o som para).
    pub(crate) toque_sonoro: Option<som::ToqueSonoro>,
    pub(crate) janela_focada: bool,
    pub(crate) modal_grupo: Option<ui::grupo::EstadoModalGrupo>,
    pub(crate) modal_status_custom: Option<ui::status_custom::EstadoStatusCustom>,
    pub(crate) presenca_aberta: bool,
    /// Quando fechar o submenu de presença (tolerância para o mouse atravessar).
    pub(crate) presenca_fecha_em: Option<std::time::Instant>,
    /// Volume do mic e da saída do sistema (0..1), lido ao abrir as configurações.
    pub(crate) volume_mic: Option<f32>,
    pub(crate) volume_saida: Option<f32>,
    pub(crate) modal_encaminhar: Option<ui::encaminhar::EstadoEncaminhar>,
    pub(crate) modal_favoritos: Option<ui::favoritos::EstadoFavoritos>,
    /// Aviso curto no topo ("Mensagem encaminhada!"), some sozinho.
    pub(crate) toast: Option<(String, std::time::Instant)>,
    pub(crate) modal_editar_grupo: Option<ui::editar_grupo::EstadoEditarGrupo>,
    /// Última lista de amigos recebida (alimenta a janela de escolher amigos).
    pub(crate) amigos_cache: Option<api::RespostaAmigos>,
    /// Erro de ação (abrir conversa etc.) mostrado em faixa sobre qualquer tela.
    pub(crate) erro_global: Option<String>,
    /// Última posição do mouse e tamanho da janela (para abrir menus no lugar).
    pub(crate) cursor: iced::Point,
    pub(crate) janela: iced::Size,
    pub(crate) prefs_conversas: prefs_conversas::PrefsConversas,
    /// Conversa a ligar assim que a sala abrir (botão Ligar dos amigos).
    pub(crate) ligar_ao_abrir: Option<String>,
    /// Para onde ir depois que a pessoa confirmar sair da chamada.
    pub(crate) abrir_apos_sair: Option<Destino>,
    pub(crate) avisos_cmd: Option<tokio::sync::mpsc::UnboundedSender<avisos::Comando>>,
    pub(crate) modal_foto: Option<ModalFoto>,
    pub(crate) modal_visao_jogo: Option<ModalVisaoJogo>,
    pub(crate) jogos_reg_renomeando: Option<(String, String)>,
    pub(crate) jogos_reg_seletor_aberto: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct ModalFoto {
    pub url: String,
    pub titulo: String,
    pub zoom: f32,
}

#[derive(Debug, Clone)]
pub(crate) struct ModalVisaoJogo {
    pub nome: String,
    pub exec: Option<String>,
    pub capa: Option<u64>,
    pub manual: bool,
    pub ultimo: u64,
}

/// Destino pedido enquanto havia uma chamada em andamento.
#[allow(dead_code)]
pub(crate) enum Destino {
    Conversa(String),
    Amigos,
}

/// Para quem o menu de contexto foi aberto.
pub(crate) enum AlvoMenu {
    /// Uma transmissão (dono = id da conexão, de_tela).
    Transmissao(String, bool),
    /// Uma mensagem do chat (id da mensagem).
    Mensagem(String),
    /// Clique direito na imagem de uma mensagem (id da mensagem).
    Imagem(String),
    /// Foto (`true`) ou banner (`false`) do próprio perfil.
    Midia(bool),
    Pessoa {
        uid: String,
        usuario: Option<String>,
        nome: String,
    },
    Conversa(String),
    /// Prévia de perfil ao clicar na foto ou no nome do autor de uma mensagem.
    Previa {
        uid: Option<String>,
        nome: String,
        app: bool,
    },
}

pub(crate) struct MenuContexto {
    pub(crate) alvo: AlvoMenu,
    pub(crate) x: f32,
    pub(crate) y: f32,
}

pub(crate) enum Tela {
    Carregando,
    Login(Login),
    Salas(Salas),
    Sala(Box<SalaAberta>),
}

#[derive(Default)]
pub(crate) struct Login {
    pub(crate) usuario: String,
    pub(crate) senha: String,
    pub(crate) criando: bool,
    pub(crate) enviando: bool,
    pub(crate) erro: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AbaSalas {
    #[default]
    Salas,
    Amigos,
}

#[derive(Default)]
pub(crate) struct Salas {
    /// Por que a última tentativa de entrar não deu certo. Fica até a pessoa
    /// tentar de novo — a atualização da lista a cada 4s não pode apagar isso.
    pub(crate) recusa: Option<String>,
    pub(crate) aba: AbaSalas,
    pub(crate) amigos: Option<api::RespostaAmigos>,
    pub(crate) carregando_amigos: bool,
    pub(crate) estado_amigos: ui::amigos::EstadoAmigos,
}

/// Painéis que abrem por cima do chat. Só um por vez; Esc e clique fora fecham.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PainelFlutuante {
    Mais,
    Emoji,
    Sons,
    ReacoesTela,
    Sussurro,
    Gif,
    Atividades,
    MaisPalco,
}

pub(crate) struct SalaAberta {
    pub(crate) entrada: sala::Entrada,
    /// O que o cabeçalho mostra: nome do grupo, ou da outra pessoa.
    pub(crate) titulo: String,
    pub(crate) painel_direito_aberto: bool,
    pub(crate) fixadas_ativas: bool,
    /// O único painel flutuante aberto (sobre o conteúdo, nunca empurrando o chat).
    pub(crate) painel: Option<PainelFlutuante>,
    /// Jogo de Wordle aberto nesta conversa e o resumo do grupo (quem jogou, cores).
    pub(crate) wordle: Option<ui::wordle::EstadoUiWordle>,
    /// Resumo do grupo por rodada (`dia`): cada mensagem do Wordle mostra a sua.
    pub(crate) wordle_grupo: std::collections::HashMap<String, api::GrupoWordle>,
    /// Quando a pessoa entrou na chamada (cronômetro da coluna).
    pub(crate) chamada_desde: Option<std::time::Instant>,
    pub(crate) comandos: Option<mpsc::UnboundedSender<Comando>>,
    pub(crate) meu_id: String,
    pub(crate) pessoas: Vec<Pessoa>,
    pub(crate) mensagens: Vec<Mensagem>,
    pub(crate) imagens: HashMap<String, image::Handle>,
    pub(crate) rascunho: String,
    pub(crate) na_chamada: bool,
    pub(crate) mudo: bool,
    pub(crate) surdo: bool,
    pub(crate) apresentando: bool,
    pub(crate) com_camera: bool,
    pub(crate) videos: Vec<Video>,
    pub(crate) config_tela: Option<ui::tela::ConfigCompartilharTela>,
    pub(crate) previa_camera: Option<PreviaCamera>,
    pub(crate) aviso: Option<String>,
    pub(crate) ptt: PttConfig,
    pub(crate) capturando_ptt: bool,
    pub(crate) ptt_transmitindo: bool,
    pub(crate) portal_global_ativo: bool,
    pub(crate) painel_audio: Option<PainelAudio>,
    pub(crate) nivel_mic_atual: f32,
    pub(crate) falando_no_mudo: bool,
    pub(crate) aviso_eco: bool,
    pub(crate) mudo_forcado: bool,
    pub(crate) modo_video: ui::video::ModoVideo,
    pub(crate) video_fixado: Option<(String, bool)>,
    /// Esconde a prévia da minha própria transmissão (clique direito).
    pub(crate) previa_propria_oculta: bool,
    /// Última posição do mouse e onde ele estava ao abrir um popup de botão.
    pub(crate) cursor_pos: iced::Point,
    pub(crate) painel_ancora: iced::Point,
    pub(crate) video_em_tela_cheia: Option<(String, bool)>,
    /// Transmissões que a pessoa clicou para assistir.
    pub(crate) assistindo: std::collections::HashSet<(String, bool)>,
    /// Último movimento do mouse (esconde os controles da tela cheia).
    pub(crate) ultimo_movimento: std::time::Instant,
    pub(crate) ultimo_falante: Option<String>,
    pub(crate) imagem_pendente: Option<ui::chat::ImagemPendente>,
    pub(crate) meu_uid: Option<String>,
    pub(crate) editando: Option<ui::chat::MensagemEmEdicao>,
    pub(crate) resposta_pendente: Option<sala::RespostaA>,
    pub(crate) seletor_reacao: Option<String>,
    pub(crate) mensagem_fixada: Option<sala::MensagemFixada>,
    pub(crate) digitando_remotos: HashMap<String, (String, std::time::Instant)>,
    pub(crate) digitando_anunciado: bool,
    pub(crate) ultimo_input_chat: Option<std::time::Instant>,
    pub(crate) gif_ativo: bool,
    pub(crate) busca_gif: String,
    /// Texto do campo de busca da aba Emoji do seletor.
    pub(crate) busca_emoji: String,
    pub(crate) buscando_gif: bool,
    pub(crate) erro_gif: Option<String>,
    pub(crate) resultados_gif: Vec<crate::api::ItemGif>,
    pub(crate) previas_gif: HashMap<String, image::Handle>,
    pub(crate) sussurro_alvos: Vec<ui::chat::AlvoSussurro>,
    pub(crate) erro_sussurro: Option<String>,
    pub(crate) busca_chat_aberta: bool,
    pub(crate) busca_chat: String,
    pub(crate) janela_focada: bool,
    pub(crate) nao_lidas: usize,
    /// Não lidas que a conversa tinha ao ser aberta (vira o divisor "novas").
    pub(crate) novas_ao_abrir: usize,
    pub(crate) primeira_nao_lida_id: Option<String>,
    pub(crate) sou_dono: bool,
    pub(crate) reacoes_flutuantes: Vec<ui::reacao::ReacaoAtiva>,
    pub(crate) aviso_cutucada: Option<ui::reacao::AvisoCutucada>,
    pub(crate) id_reacao_seq: u64,
    pub(crate) soundboard: ui::soundboard::EstadoSoundboard,
    pub(crate) ultimo_som_tocado: Option<std::time::Instant>,
    pub(crate) quadro: ui::quadro::EstadoQuadro,
    pub(crate) roletas: ui::roleta::EstadoRoletas,
    pub(crate) criador_enquete: ui::enquete::EstadoCriadorEnquete,
    pub(crate) densidade: ui::layout::Densidade,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PainelAudio {
    pub(crate) entradas: Vec<dispositivos::DispositivoAudio>,
    pub(crate) saidas: Vec<dispositivos::DispositivoAudio>,
    pub(crate) entrada_selecionada: Option<String>,
    pub(crate) saida_selecionada: Option<String>,
    pub(crate) testando_mic: bool,
    pub(crate) nivel_mic: f32,
}

#[derive(Debug, Clone)]
pub(crate) struct AppTocando {
    pub(crate) nome: String,
    pub(crate) binario: String,
    pub(crate) midia: String,
}

#[derive(Debug, Clone)]
pub(crate) struct PreviaCamera {
    pub(crate) cameras: Vec<captura_camera::DispositivoCamera>,
    pub(crate) indice_selecionado: usize,
    pub(crate) imagem: Option<image::Handle>,
}

pub(crate) struct Video {
    pub(crate) dono: String,
    pub(crate) de_tela: bool,
    pub(crate) largura: u32,
    pub(crate) altura: u32,
    pub(crate) imagem: image::Handle,
}

#[derive(Debug, Clone)]
pub(crate) enum Msg {
    Sessao(Result<Option<Eu>, String>),
    // login
    Usuario(String),
    Senha(String),
    AlternaCriar,
    Entrar,
    Entrou(Result<String, String>),
    // salas
    /// Abre a conversa (o id é o nome da sala interna no servidor).
    AbreSala(String),
    Deslogar,
    // sala
    SalaDe(String, Evento),
    Rascunho(String),
    EnviaChat,
    IniciaEdicaoChat(String),
    CancelaEdicaoChat,
    ApagaChat(String),
    IniciaRespostaChat {
        id: String,
        autor: String,
        texto: String,
    },
    CancelaRespostaChat,
    AlternaSeletorReacao(String),
    ReagirChat {
        id: String,
        emoji: String,
    },
    FixaChat(String),
    DesafixaChat,
    TickDigitando,
    CompletaMencao(String),
    AplicaComandoChat(String),
    CopiaTexto(String),
    AbreLink(String),
    EscolheArquivoImagem,
    ArquivoImagemEscolhido(Option<(String, Vec<u8>)>),
    /// Qualquer arquivo escolhido para anexar (nome de exibição e caminho).
    ArquivoEscolhido(Option<(String, std::path::PathBuf)>),
    ArquivoEnviado(Result<api::ArquivoEnviado, String>),
    BaixaArquivo {
        id: String,
        nome: String,
    },
    ArquivoBaixado(Result<std::path::PathBuf, String>),
    CancelaImagemPendente,
    SaiDaChamada,
    AlternaMudo,
    AlternaSurdo,
    IniciaCapturaPtt,
    CancelaCapturaPtt,
    PttTransmitindo(bool),
    PortalGlobalResultado(Result<(), String>),
    /// Um atalho global foi apertado (`pressionado`) ou solto.
    PortalAtalho {
        acao: String,
        pressionado: bool,
    },
    /// Começa a capturar a tecla nova de uma ação (Esc cancela).
    CapturaAtalho(String),
    RestauraAtalho(String),
    EventoIced(iced::Event),
    #[allow(dead_code)]
    AbreConfigAudio,
    #[allow(dead_code)]
    FechaConfigAudio,
    SelecionaEntradaAudio(String),
    SelecionaSaidaAudio(String),
    AlternaTesteMic,
    DispositivosAvulsosLidos {
        entradas: Vec<dispositivos::DispositivoAudio>,
        saidas: Vec<dispositivos::DispositivoAudio>,
    },
    PedeCamera,
    TrocaCameraPrevia(usize),
    ConfirmaCamera,
    CancelaPreviaCamera,
    ParaCamera,
    AjustaVolume(String, f32),
    AlternaMute(String),
    AjustaVolumeTela(String, f32),
    AlternaMuteTela(String),
    PedeTela,
    AppsTocando(Result<Vec<AppTocando>, String>),
    MudaPerfilTela(captura_tela::PerfilTela),
    AlternaAutoBitrate(bool),
    MarcaApp(usize, bool),
    AlternaSomTela,
    EsqueceTelaLembrada,
    AlternaTodoPc,
    ConfirmaTela,
    CancelaTela,
    ParaTela,
    AlternaFixacaoVideo(String, bool),
    AlternaPreviaPropria,
    /// Começa a assistir a transmissão de tela de alguém.
    AssisteTransmissao(String, bool),
    /// Para de assistir (volta ao retângulo cinza).
    PausaTransmissao(String, bool),
    AbreMenuTransmissao(String, bool),
    AlternaTelaCheiaVideo(Option<(String, bool)>),
    FechaAviso,
    DispensaAvisoEco,
    AlternaPainelGif,
    BuscaGifInput(String),
    SubmeteBuscaGif,
    GifsCarregados(Result<Vec<crate::api::ItemGif>, String>),
    PreviaGifBaixada {
        url: String,
        bytes: Vec<u8>,
    },
    GifBaixado {
        id: String,
        bytes: Vec<u8>,
    },
    EnviaGif(String),
    /// O mouse entrou/saiu de um elemento animado (ver `anim`).
    Hover(String, bool),
    AnimTick,
    FechaPainel,
    AlternaMenuSussurro,
    AlternaAlvoSussurro {
        uid: Option<String>,
        nome: String,
    },
    IniciaSussurro {
        uid: Option<String>,
        nome: String,
    },
    LimpaSussurro,
    BuscaChatInput(String),
    FechaBuscaChat,
    LimpaNaoLidas,
    VotaEnquete {
        id: String,
        opcao: usize,
    },
    // perfil
    FechaModalPerfil,
    AbrePerfilUsuario(String),
    PerfilCarregado(Result<Box<api::PerfilCompleto>, String>),
    MuralCarregado(Result<Vec<api::Recado>, String>),
    RascunhoRecado(String),
    EnviaRecado,
    ApagaRecado(String),
    Avisos(avisos::Evento),
    /// Foto de perfil ou ícone de grupo baixado (ou que falhou).
    ImagemBaixada {
        url: String,
        bytes: Result<Vec<u8>, String>,
    },
    RascunhoNomePerfil(String),
    RascunhoBioPerfil(String),
    EscolheCorPerfil(String),
    SalvaMeuPerfil,
    PerfilSalvo(Result<(), String>),
    EscolheArquivoAvatar,
    UploadAvatar(Vec<u8>),
    AvatarAtualizado(Result<String, String>),
    RemoveAvatar,
    EscolheArquivoBanner,
    UploadBanner(Vec<u8>),
    BannerAtualizado(Result<String, String>),
    RemoveBanner,
    TrocaAbaPerfil(ui::perfil::AbaPerfil),
    RascunhoSenhaAtual(String),
    RascunhoSenhaNova(String),
    RascunhoSenhaNovaConfirma(String),
    SubmeteTrocaSenha,
    SenhaTrocada(Result<(), String>),
    IniciaConfirmacaoApagarConta,
    CancelaConfirmacaoApagarConta,
    RascunhoSenhaApagarConta(String),
    SubmeteApagarConta,
    ContaApagada(Result<(), String>),
    DefineStatus(String),
    FiltraConversas(String),
    AlternaNotificaMensagens,
    AlternaNotificaMencoes,
    AlternaSomDoToque,
    AlternaAutostart,
    AlternaEnviarRelatorio,
    JogosRegFiltro(String),
    JogosRegAtualiza,
    /// Executável escolhido na lista de programas abertos.
    JogosRegAdiciona(String),
    JogosRegAdicionado,
    JogosRegRemove(String),
    JogosRegIniciaRenomear {
        exec: String,
        nome_atual: String,
    },
    JogosRegRascunhoNome(String),
    JogosRegSalvaNome,
    JogosRegCancelaRenomear,
    JogosRegAlternaAtivo(String),
    JogosRegAlternaSeletor,
    AbreVisaoJogo(ModalVisaoJogo),
    FechaVisaoJogo,
    AbreFoto {
        url: String,
        titulo: String,
    },
    FechaFoto,
    ZoomFoto(f32),
    ResetaZoomFoto,
    AtendeToque(String),
    RecusaToque(String),
    AbreMenuPessoa {
        uid: String,
        usuario: Option<String>,
        nome: String,
    },
    AbreMenuConversa(String),
    /// O mouse entrou/saiu de um GIF animado (para animar só o que está sob o cursor).
    HoverGif(Option<String>),
    JogosBuscaInput(String),
    JogosBusca,
    JogosResultado(Result<Vec<api::JogoLista>, String>),
    JogoAdiciona(api::JogoLista, ui::jogos_perfil::ListaJogo),
    JogoRemove(ui::jogos_perfil::ListaJogo, u64),
    JogosSalvos(Result<(), String>),
    MinhasListas(Result<Box<api::PerfilCompleto>, String>),
    PedeApagarGrupo(String),
    AbreEditarPerfil,
    TickGif,
    AlternaReduzAnimacoes,
    AlternaGifSoHover,
    AbrePrevia {
        uid: Option<String>,
        nome: String,
        app: bool,
    },
    AbreMenuMensagem(String),
    /// Fecha o menu de contexto e então executa a ação escolhida nele.
    MenuFaz(Box<Msg>),
    FechaMenuContexto,
    MencionaPessoa(String),
    /// Toca de novo para alguém que está fora da chamada.
    TocaPara {
        conversa: String,
        uid: String,
    },
    RemoveDoGrupo {
        conversa: String,
        uid: String,
    },
    SaiDoGrupo(String),
    GrupoAlterado(Result<(), String>),
    AlternaSilenciarConversa(String),
    FechaConversa(String),
    /// Abre (ou cria) a conversa direta com este amigo; `ligar` toca em seguida.
    AbreDm {
        uid: String,
        nome: String,
        ligar: bool,
    },
    DmPronta {
        id: String,
        nome: String,
        ligar: bool,
        erro: Option<String>,
    },
    AbreAmigos,
    AlternaMenuStatus,
    /// Abre a janela de criar grupo (com uma pessoa já marcada, opcionalmente).
    AbreCriarGrupo(Option<String>),
    /// Abre a janela de adicionar pessoas ao grupo (id da conversa).
    AbreAdicionarPessoas(String),
    GrupoBusca(String),
    GrupoAlterna(String),
    GrupoConfirma,
    GrupoCancela,
    AlternaPresenca,
    PresencaHover(bool),
    PresencaTick,
    /// Slider de volume do sistema: `true` = microfone.
    VolumeSistema(bool, f32),
    AplicaVolumeSistema(bool),
    VolumesSistemaLidos(Option<f32>, Option<f32>),
    AbreMenuMidia(bool),
    TrocaPreviaPerfil(bool),
    AbreStatusCustom,
    FechaStatusCustom,
    StatusCustomTexto(String),
    StatusCustomEmoji(String),
    StatusCustomAlternaEmojis,
    StatusCustomAlternaPrazos,
    StatusCustomPrazo(Option<u32>),
    SalvaStatusCustom,
    LimpaStatusCustom,
    StatusCustomSalvo(Result<Option<api::StatusCustom>, String>),
    AbreEncaminhar(String),
    EncaminhaBusca(String),
    EncaminhaAlterna(String),
    EncaminhaExtra(String),
    EncaminhaEnvia,
    EncaminhaFim(Result<usize, String>),
    FechaEncaminhar,
    Favorita(String),
    Desfavorita {
        conversa: String,
        id: String,
    },
    FavoritoFeito(Result<String, String>),
    AbreFavoritos,
    FechaFavoritos,
    FavoritosCarregados(Result<Vec<api::Favorito>, String>),
    AbreMenuImagem(String),
    CopiaImagem(String),
    SalvaImagem(String),
    ImagemSalva(Result<bool, String>),
    Toast(String),
    ToastTick,
    /// Resultado de criar/adicionar: `Some((id, nome))` quando criou um grupo.
    GrupoResultado(Result<Option<(String, String)>, String>),
    /// Liga para a conversa aberta (toca nos outros e entra na chamada).
    LigarConversa,
    /// Abre "Editar grupo" da conversa pedida.
    AbreEditarGrupo(String),
    EditaGrupoNome(String),
    EditaGrupoPasso(ui::editar_grupo::Passo),
    EditaGrupoEscolheImagem,
    EditaGrupoImagemEscolhida(Option<Vec<u8>>),
    EditaGrupoBuscaInput(String),
    EditaGrupoBusca,
    EditaGrupoGifs(Result<Vec<api::ItemGif>, String>),
    EditaGrupoEscolheGif(String),
    EditaGrupoRemoveIcone,
    EditaGrupoSalva,
    EditaGrupoSalvo(Result<(), String>),
    EditaGrupoCancela,
    AlternaPainelDireito,
    AlternaFixadas,
    AlternaMenuMais,
    AlternaMenuAtividades,
    AlternaMenuMaisPalco,
    AlternaMenuEmoji,
    TrocaAbaSeletor(ui::seletor::AbaSeletor),
    BuscaEmojiInput(String),
    InsereEmoji(String),
    /// Só para redesenhar o cronômetro da chamada.
    TickChamada,
    StatusDefinido(Result<(), String>),
    // amigos
    TrocaSubAbaAmigos(ui::amigos::SubAbaAmigos),
    CarregaAmigos,
    AmigosCarregados(Result<api::RespostaAmigos, String>),
    InputUsuarioAmigo(String),
    EnviaPedidoAmigo,
    EnviaPedidoAmigoPorNome(String),
    FechaErroGlobal,
    PedidoAmigoEnviado(Result<bool, String>),
    RespondePedidoAmigo {
        uid: String,
        aceita: bool,
    },
    PedidoAmigoRespondido(Result<(), String>),
    PedeConfirmacaoRemocaoAmigo(String),
    CancelaRemocaoAmigo,
    ConfirmaRemocaoAmigo(String),
    RemoveAmigo(String),
    AmigoRemovido(Result<(), String>),
    VarreAtividade,
    AtividadeDetectada(Option<api::Atividade>),
    AlternaMenuReacoesFlutuantes,
    EnviarReacao(String),
    /// Cutucar alguém. Sem botão até o menu de contexto da C5.
    #[allow(dead_code)]
    Cutucar(String),
    AlternaPainelSoundboard,
    AlternaMuteSoundboardGlobal,
    /// Silenciar os sons de uma pessoa. Sem botão até o menu de contexto (C5).
    #[allow(dead_code)]
    AlternaMuteSoundboardPessoa(String),
    TocarSom(String),
    /// Slider do volume dos sons que EU solto (0–100).
    VolumeSoundboard(u8),
    SalvaVolumeSoundboard,
    AlternarQuadro,
    Quadro(ui::quadro::QuadroMsg),
    Wordle(ui::wordle::WordleMsg),
    AlternarRoletas,
    Roleta(ui::roleta::RoletaMsg),
    AlternarCriadorEnquete,
    Enquete(ui::enquete::EnqueteMsg),
    Tema(ui::tema::TemaMsg),
    AbreConfiguracoes(Option<ui::configuracoes::AbaConfig>),
    FechaConfiguracoes,
    TrocaAbaConfig(ui::configuracoes::AbaConfig),
    SelecionaCameraConfig(usize),
    AlternaEspelharCamera,
    SelecionaPerfilTelaConfig(captura_tela::PerfilTela),
    AlternaPttAtivo,
    SelecionaDensidade(ui::layout::Densidade),
    TogglePainelDiagnostico,
    FechaPainelDiagnostico,
    AtualizaMetricasServidor,
    MetricasServidorRecebidas(Result<api::MetricasServidor, String>),
    ConfirmaAcaoCritica,
    CancelaAcaoCritica,
    AbreManual,
    FechaManual,
    MudaFiltroManual(String),
    BandejaIniciada(ui::bandeja::HandleBandeja),
    AcaoBandeja(ui::bandeja::AcaoBandeja),
    AlternaMinimizarParaBandeja,
    ChecaAtualizacao,
    AtualizacaoVerificada(Option<ui::atualizacao::InfoAtualizacao>),
    AbreUrlAtualizacao(String),
    DispensaAtualizacao,
    VerificaAtualizacaoManual,
    AtualizacaoManual(Result<Option<ui::atualizacao::InfoAtualizacao>, String>),
    InstalaAtualizacao,
    AtualizacaoInstalada(Result<String, String>),
    WordleDevTrocou(Result<(), String>),
    DefineVolumeToque(u8),
    DefineVolumeEfeitos(u8),
    TestaToque,
    Nenhum,
}

impl App {
    /// Indica se o usuário está em chamada ativa, na tela visível ou em segundo plano.
    pub(crate) fn em_chamada(&self) -> bool {
        match &self.tela {
            Tela::Sala(s) if s.na_chamada => true,
            _ => self.chamada_ativa.as_ref().is_some_and(|c| c.na_chamada),
        }
    }
}
