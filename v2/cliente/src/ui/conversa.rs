//! Peças da tela de conversa (C3): cabeçalho, separador de dia, mensagem de
//! sistema e o texto de data das mensagens.

use chrono::{Datelike, Local, NaiveDate, TimeZone};
use iced::widget::{button, container, row, space, svg, text};
use iced::{Alignment, Background, Border, Element, Length};

use crate::avisos::{Conversa, Membro};
use crate::{Msg, botao_icone, fraco, icones, negrito, tema, ui};

const MESES: [&str; 12] = [
    "janeiro",
    "fevereiro",
    "março",
    "abril",
    "maio",
    "junho",
    "julho",
    "agosto",
    "setembro",
    "outubro",
    "novembro",
    "dezembro",
];

/// "13 de outubro de 2026".
pub fn data_por_extenso(d: NaiveDate) -> String {
    format!(
        "{} de {} de {}",
        d.day(),
        MESES[d.month0() as usize],
        d.year()
    )
}

fn dia_local(ts_ms: i64) -> Option<NaiveDate> {
    Local
        .timestamp_millis_opt(ts_ms)
        .single()
        .map(|d| d.date_naive())
}

/// Os dois instantes caem no mesmo dia (no fuso do usuário)?
pub fn mesmo_dia(a_ms: i64, b_ms: i64) -> bool {
    dia_local(a_ms) == dia_local(b_ms)
}

/// "Hoje às 15:40", "Ontem às 22:03" ou "13/10/2026 15:40".
pub fn quando(ts_ms: i64) -> String {
    let Some(d) = Local.timestamp_millis_opt(ts_ms).single() else {
        return String::new();
    };
    let hoje = Local::now().date_naive();
    let hora = d.format("%H:%M");
    match (hoje - d.date_naive()).num_days() {
        0 => format!("Hoje às {hora}"),
        1 => format!("Ontem às {hora}"),
        _ => format!("{} {hora}", d.format("%d/%m/%Y")),
    }
}

/// Linha fina com a data no meio, entre dias diferentes.
pub fn separador_de_dia<'a>(ts_ms: i64) -> Element<'a, Msg> {
    let c = tema::cores();
    let rotulo = dia_local(ts_ms).map(data_por_extenso).unwrap_or_default();
    let linha = || {
        container(space().height(1))
            .width(Length::Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(c.borda)),
                ..Default::default()
            })
    };
    container(
        row![linha(), text(rotulo).size(12).style(fraco), linha()]
            .spacing(12)
            .align_y(Alignment::Center),
    )
    .padding([12, 4])
    .into()
}

/// O texto da linha de sistema com o nome de `quem` clicável (abre a prévia do
/// perfil). Sem `quem`, ou se o nome não aparece no texto, vai como texto puro.
fn texto_com_nome<'a>(conteudo: &str, quem: Option<&str>, cor: iced::Color) -> Element<'a, Msg> {
    let Some((nome, pos)) = quem
        .filter(|q| !q.is_empty())
        .and_then(|q| conteudo.find(q).map(|p| (q, p)))
    else {
        return text(conteudo.to_string()).size(14).color(cor).into();
    };
    let (antes, depois) = (&conteudo[..pos], &conteudo[pos + nome.len()..]);
    let id = format!("sis-nome-{nome}");
    let t = crate::anim::t(&id);
    let c = tema::cores();
    let cor_nome = tema::mistura(cor, c.destaque, t);
    row![
        text(antes.to_string()).size(14).color(cor),
        crate::ui::efeitos::com_hover(
            &id,
            iced::widget::mouse_area(
                text(nome.to_string())
                    .size(14)
                    .font(negrito())
                    .color(cor_nome)
            )
            .interaction(iced::mouse::Interaction::Pointer)
            .on_press(Msg::AbrePrevia {
                uid: None,
                nome: nome.to_string(),
                app: false,
            }),
        ),
        text(depois.to_string()).size(14).color(cor),
    ]
    .into()
}

/// Mensagem de sistema. Chamada em verde, perdida em vermelho; entrada no
/// grupo seta verde, saída seta vermelha; o resto neutro.
pub fn linha_de_sistema<'a>(
    tipo: &str,
    texto: &'a str,
    ts_ms: i64,
    extra: &'a crate::sala::MensagemExtra,
    meu_uid: Option<&str>,
) -> Element<'a, Msg> {
    let c = tema::cores();
    let perdi = tipo == "chamada"
        && meu_uid.is_some_and(|u| extra.perdeu.iter().any(|p| p == u))
        && extra.quem.is_some()
        && extra.duracao.is_some();
    let (icone, cor_icone, cor_texto, conteudo): (_, _, _, String) = if perdi {
        (
            icones::chamada_perdida(),
            c.perigo,
            c.texto_medio,
            format!(
                "Você perdeu uma chamada de {} que durou {}.",
                extra.quem.as_deref().unwrap_or(""),
                extra.duracao.as_deref().unwrap_or("")
            ),
        )
    } else {
        match tipo {
            "chamada" => (
                icones::chamada(),
                c.sucesso_forte,
                c.sucesso_forte,
                texto.to_string(),
            ),
            "perdida" => (
                icones::chamada_perdida(),
                c.perigo,
                c.perigo,
                texto.to_string(),
            ),
            _ if texto.contains(" adicionou ") || texto.contains(" entrou ") => (
                icones::seta_direita(),
                c.sucesso_forte,
                c.texto_medio,
                texto.to_string(),
            ),
            _ if texto.contains(" removeu ") || texto.contains(" saiu ") => (
                icones::esquerda(),
                c.perigo,
                c.texto_medio,
                texto.to_string(),
            ),
            _ => (
                icones::mensagem(),
                c.texto_fraco,
                c.texto_fraco,
                texto.to_string(),
            ),
        }
    };
    container(
        row![
            svg(icone)
                .width(16)
                .height(16)
                .style(tema::icone_cor(cor_icone)),
            texto_com_nome(&conteudo, extra.quem.as_deref(), cor_texto),
            text(quando(ts_ms)).size(12).style(fraco),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .padding([6, 8])
    .into()
}

/// O que o cabeçalho sabe da conversa (vem do canal de avisos).
pub struct InfoConversa<'a> {
    pub conversa: &'a Conversa,
    pub meu_usuario: Option<&'a str>,
}

impl InfoConversa<'_> {
    fn outros(&self) -> Vec<&Membro> {
        self.conversa
            .membros
            .iter()
            .filter(|m| m.usuario.as_deref() != self.meu_usuario)
            .collect()
    }
}

/// O ícone da conversa em `tamanho` px (foto da pessoa, ou ícone do grupo).
pub fn icone_da_conversa<'a>(
    info: Option<&InfoConversa<'a>>,
    nome: &str,
    tamanho: f32,
    fundo: iced::Color,
) -> Element<'a, Msg> {
    let Some(info) = info else {
        return ui::avatar::avatar(None, nome, nome, tamanho, None, fundo);
    };
    let outros = info.outros();
    if info.conversa.tipo == "grupo" {
        let membros: Vec<ui::avatar::MembroDoIcone<'_>> = outros
            .iter()
            .map(|m| ui::avatar::MembroDoIcone {
                chave: &m.uid,
                nome: &m.name,
                url: m.avatar.as_deref(),
            })
            .collect();
        ui::avatar::icone_grupo(info.conversa.icone.as_deref(), &membros, tamanho, fundo)
    } else {
        let m = outros.first();
        ui::avatar::avatar(
            m.and_then(|m| m.avatar.as_deref()),
            m.map_or(nome, |m| m.uid.as_str()),
            nome,
            tamanho,
            m.and_then(|m| m.status.as_deref()),
            fundo,
        )
    }
}

/// O menu do botão "+" do campo de texto.
pub fn menu_mais<'a>(pode_anexar: bool, em_chamada: bool) -> Element<'a, Msg> {
    let c = tema::cores();
    let item = |icone: iced::widget::svg::Handle, rotulo: &'static str, msg: Option<Msg>| {
        let ativo = msg.is_some();
        button(
            row![
                svg(icone)
                    .width(18)
                    .height(18)
                    .style(tema::icone_cor(c.texto_medio)),
                text(rotulo).size(14),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding([8, 12])
        .style(move |_, st| button::Style {
            background: matches!(st, button::Status::Hovered).then_some(Background::Color(c.forte)),
            text_color: if ativo { c.texto } else { c.texto_fraco },
            border: Border {
                radius: tema::RAIO.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .on_press_maybe(msg.clone())
    };
    let itens = iced::widget::column![
        item(
            icones::anexo(),
            "Enviar arquivo",
            pode_anexar.then_some(Msg::EscolheArquivoImagem)
        ),
        item(
            icones::grafico(),
            "Criar enquete",
            Some(Msg::AlternarCriadorEnquete)
        ),
        item(icones::roda(), "Roleta", Some(Msg::AlternarRoletas)),
        item(icones::lapis(), "Quadro branco", Some(Msg::AlternarQuadro)),
        item(
            icones::grade_3x3(),
            "Wordle",
            Some(Msg::Wordle(crate::ui::wordle::WordleMsg::Inicia)),
        ),
        item(icones::fone(), "Sussurrar", Some(Msg::AlternaMenuSussurro)),
        item(
            icones::smile(),
            "Reações na tela",
            Some(Msg::AlternaMenuReacoesFlutuantes)
        ),
    ]
    .spacing(2);
    // Os sons só existem dentro de uma chamada.
    let itens = if em_chamada {
        itens.push(item(
            icones::musica(),
            "Sons",
            Some(Msg::AlternaPainelSoundboard),
        ))
    } else {
        itens
    };
    container(itens)
        .padding(6)
        .width(240)
        .style(move |_| container::Style {
            background: Some(Background::Color(c.elevado)),
            border: Border {
                color: c.borda,
                width: 1.0,
                radius: tema::RAIO_GRANDE.into(),
            },
            ..Default::default()
        })
        .into()
}

/// Grade de emojis que entram no texto.
pub struct EstadoCabecalho<'a> {
    pub painel_aberto: bool,
    /// Texto do campo de busca do cabeçalho.
    pub busca: &'a str,
    pub fixadas_ativas: bool,
    pub tem_fixada: bool,
    pub na_chamada: bool,
}

pub fn cabecalho<'a>(
    titulo: &'a str,
    info: Option<&InfoConversa<'a>>,
    est: &EstadoCabecalho<'a>,
) -> Element<'a, Msg> {
    let c = tema::cores();
    let eh_grupo = info.is_some_and(|i| i.conversa.tipo == "grupo");
    let icone = icone_da_conversa(info, titulo, 28.0, c.fundo);
    let identidade = row![
        icone,
        text(titulo)
            .size(16)
            .font(negrito())
            .wrapping(iced::widget::text::Wrapping::None)
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    let esquerda: Element<'a, Msg> = if eh_grupo {
        // No grupo, nome e ícone abrem "Editar grupo".
        button(identidade)
            .padding([4, 6])
            .style(move |_, st| button::Style {
                background: matches!(st, button::Status::Hovered)
                    .then_some(Background::Color(c.elevado)),
                text_color: c.texto,
                border: Border {
                    radius: tema::RAIO.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .on_press(Msg::AbreEditarGrupo(
                info.map(|i| i.conversa.id.clone()).unwrap_or_default(),
            ))
            .into()
    } else {
        container(identidade).padding([4, 6]).into()
    };
    // O nome cede lugar aos botões quando a janela é estreita.
    let esquerda: Element<'a, Msg> = container(esquerda)
        .width(Length::Shrink)
        .max_width(10_000)
        .clip(true)
        .into();
    let tooltip_pessoas = if eh_grupo {
        Msg::AbreAdicionarPessoas(info.map(|i| i.conversa.id.clone()).unwrap_or_default())
    } else {
        // Da conversa direta, cria um grupo já com esta pessoa marcada.
        let uid = info.and_then(|i| i.outros().first().map(|m| m.uid.clone()));
        Msg::AbreCriarGrupo(uid)
    };
    row![
        container(esquerda).width(Length::Fill).clip(true),
        crate::ui::efeitos::com_dica(
            botao_icone(
                icones::chamada(),
                est.na_chamada,
                false,
                (!est.na_chamada).then_some(Msg::LigarConversa),
            ),
            "Iniciar chamada de voz",
            iced::widget::tooltip::Position::Bottom
        ),
        crate::ui::efeitos::com_dica(
            botao_icone(
                icones::pin(),
                est.fixadas_ativas,
                false,
                est.tem_fixada.then_some(Msg::AlternaFixadas),
            ),
            "Mensagens fixadas",
            iced::widget::tooltip::Position::Bottom
        ),
        crate::ui::efeitos::com_dica(
            botao_icone(icones::usuario_mais(), false, false, Some(tooltip_pessoas)),
            "Adicionar pessoas ao grupo",
            iced::widget::tooltip::Position::Bottom
        ),
        crate::ui::efeitos::com_dica(
            botao_icone(
                icones::amigos(),
                est.painel_aberto,
                false,
                Some(Msg::AlternaPainelDireito),
            ),
            "Mostrar lista de membros",
            iced::widget::tooltip::Position::Bottom
        ),
        container(
            row![
                iced::widget::text_input(&format!("Buscar {titulo}"), est.busca)
                    .id("busca-chat")
                    .on_input(Msg::BuscaChatInput)
                    .padding([5, 8])
                    .size(13)
                    .style(tema::campo),
                container(ui::efeitos::ico(icones::busca(), 14.0, c.texto_fraco)).padding(
                    iced::Padding {
                        left: 6.0,
                        ..Default::default()
                    }
                ),
            ]
            .align_y(Alignment::Center)
        )
        .width(220),
    ]
    .spacing(4)
    .align_y(Alignment::Center)
    .into()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn data_em_portugues() {
        let d = NaiveDate::from_ymd_opt(2026, 10, 13).unwrap();
        assert_eq!(data_por_extenso(d), "13 de outubro de 2026");
        let d = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();
        assert_eq!(data_por_extenso(d), "1 de março de 2026");
    }

    #[test]
    fn mesmo_dia_no_fuso_local() {
        let meio = Local
            .with_ymd_and_hms(2026, 10, 13, 12, 0, 0)
            .single()
            .unwrap()
            .timestamp_millis();
        assert!(mesmo_dia(meio, meio + 60_000));
        assert!(!mesmo_dia(meio, meio + 24 * 3600 * 1000));
    }

    #[test]
    fn quando_hoje_e_ontem() {
        let agora = Local::now().timestamp_millis();
        assert!(quando(agora).starts_with("Hoje às "));
        assert!(quando(agora - 24 * 3600 * 1000).starts_with("Ontem às "));
        assert!(quando(agora - 5 * 24 * 3600 * 1000).contains('/'));
    }
}
