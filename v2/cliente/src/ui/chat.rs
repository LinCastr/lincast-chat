//! Interface de chat e anexo de imagens (C1).

use ::image::ImageEncoder as _;
use base64::Engine as _;
use iced::widget::{Column, button, column, container, image, row, space, svg, text, text_input};
use iced::{Alignment, Background, Border, Element, Length};
use std::collections::HashMap;
use std::path::Path;

use crate::{Msg, destaque, fraco, icones, negrito, tema, texto_perigo};

/// Primeiro caminho livre em `pasta` para `nome` ("a.zip", "a (1).zip", …).
pub fn caminho_livre(pasta: &std::path::Path, nome: &str) -> std::path::PathBuf {
    let base: String = nome
        .chars()
        .map(|c| if matches!(c, '/' | '\\') { '_' } else { c })
        .collect();
    let p = pasta.join(&base);
    if !p.exists() {
        return p;
    }
    let (stem, ext) = match base.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
        _ => (base.clone(), String::new()),
    };
    (1..1000)
        .map(|i| pasta.join(format!("{stem} ({i}){ext}")))
        .find(|p| !p.exists())
        .unwrap_or_else(|| pasta.join(format!("{stem}-{}{ext}", std::process::id())))
}

/// Tamanho legível: "812 KB", "6,3 MB".
pub fn tamanho_legivel(b: u64) -> String {
    const KB: f64 = 1024.0;
    let x = b as f64;
    if x < KB {
        format!("{b} B")
    } else if x < KB * KB {
        format!("{:.0} KB", x / KB)
    } else if x < KB * KB * KB {
        format!("{:.1} MB", x / (KB * KB)).replace('.', ",")
    } else {
        format!("{:.2} GB", x / (KB * KB * KB)).replace('.', ",")
    }
}

pub const MAX_CHAT_IMAGE_BYTES: usize = 5 * 1024 * 1024; // 5MB

#[derive(Debug, Clone)]
pub struct ImagemPendente {
    pub nome: String,
    pub mime: String,
    pub bytes: Vec<u8>,
    pub base64: String,
    pub handle: image::Handle,
}

/// Detecta o tipo MIME da imagem por magic bytes ou extensão de arquivo.
pub fn detecta_mime(bytes: &[u8], nome: &str) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some("image/png");
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    if bytes.starts_with(b"RIFF") && bytes.len() >= 12 && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }

    let ext = Path::new(nome)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());

    match ext.as_deref() {
        Some("png") => Some("image/png"),
        Some("jpg") | Some("jpeg") => Some("image/jpeg"),
        Some("gif") => Some("image/gif"),
        Some("webp") => Some("image/webp"),
        _ => None,
    }
}

/// Valida tamanho (5MB) e formato (png, jpeg, gif, webp) da imagem.
pub fn valida_e_prepara_imagem(nome: String, bytes: Vec<u8>) -> Result<ImagemPendente, String> {
    if bytes.len() > MAX_CHAT_IMAGE_BYTES {
        return Err("Imagem maior que 5MB, escolha uma menor.".to_string());
    }

    let Some(mime) = detecta_mime(&bytes, &nome) else {
        return Err("Dá pra mandar PNG, JPEG, GIF ou WEBP.".to_string());
    };

    let base64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let handle = image::Handle::from_bytes(bytes.clone());

    Ok(ImagemPendente {
        nome,
        mime: mime.to_string(),
        bytes,
        base64,
        handle,
    })
}

/// Tenta capturar uma imagem da área de transferência (arboard com fallback para wl-paste).
pub fn tenta_imagem_clipboard() -> Option<Result<ImagemPendente, String>> {
    // 1. Tentar arboard
    if let Ok(mut cb) = arboard::Clipboard::new()
        && let Ok(img) = cb.get_image()
    {
        let mut png_bytes = Vec::new();
        let encoder = ::image::codecs::png::PngEncoder::new(&mut png_bytes);
        if encoder
            .write_image(
                &img.bytes,
                img.width as u32,
                img.height as u32,
                ::image::ExtendedColorType::Rgba8,
            )
            .is_ok()
        {
            return Some(valida_e_prepara_imagem("clipboard.png".into(), png_bytes));
        }
    }

    // 2. Fallback Wayland (wl-paste)
    for mime in ["image/png", "image/jpeg", "image/webp"] {
        if let Ok(saida) = std::process::Command::new("wl-paste")
            .args(["-t", mime])
            .output()
            && saida.status.success()
            && !saida.stdout.is_empty()
        {
            let ext = match mime {
                "image/png" => "png",
                "image/jpeg" => "jpg",
                _ => "webp",
            };
            return Some(valida_e_prepara_imagem(
                format!("clipboard.{ext}"),
                saida.stdout,
            ));
        }
    }

    None
}

/// Barra visual de prévia da imagem anexada exibida logo acima do campo de envio.
pub fn barra_previa_anexo<'a>(anexo: &'a ImagemPendente) -> Element<'a, Msg> {
    let thumbnail = image(anexo.handle.clone())
        .width(42)
        .height(42)
        .content_fit(iced::ContentFit::Cover);

    let moldura_thumb = container(thumbnail)
        .width(42)
        .height(42)
        .style(tema::moldura_video);

    let tamanho_kb = (anexo.bytes.len() as f64 / 1024.0).round() as u64;
    let info = column![
        text(&anexo.nome).size(13).font(negrito()),
        text(format!("{} KB · {}", tamanho_kb, anexo.mime))
            .size(11)
            .style(fraco),
    ]
    .spacing(2);

    let botao_fechar = button(svg(icones::fechar()).width(14).height(14))
        .padding([4, 6])
        .style(tema::botao_secundario)
        .on_press(Msg::CancelaImagemPendente);

    let barra = row![
        moldura_thumb,
        info,
        space().width(Length::Fill),
        botao_fechar,
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    container(barra)
        .padding([6, 12])
        .width(Length::Fill)
        .style(tema::painel_lateral)
        .into()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MensagemEmEdicao {
    pub id: String,
    pub texto_original: String,
}

/// Identifica se a mensagem pertence ao usuário local (por uid ou nome).
pub fn eh_minha_mensagem(m: &crate::sala::Mensagem, meu_nome: &str, meu_uid: Option<&str>) -> bool {
    if let (Some(u), Some(meu_u)) = (m.uid.as_deref(), meu_uid) {
        u == meu_u
    } else {
        m.autor == meu_nome
    }
}

/// Barra visual de indicação de edição ativa sobre o campo de envio.
pub fn barra_edicao<'a>() -> Element<'a, Msg> {
    let aviso = row![
        svg(icones::editar()).width(14).height(14),
        text("Editando mensagem — Esc cancela")
            .size(12)
            .style(fraco),
        space().width(Length::Fill),
        button(svg(icones::fechar()).width(14).height(14))
            .padding([2, 6])
            .style(tema::botao_secundario)
            .on_press(Msg::CancelaEdicaoChat),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    container(aviso)
        .padding([4, 12])
        .width(Length::Fill)
        .style(tema::painel_lateral)
        .into()
}

/// Trunca texto longo para cabeçalhos e citações curtas.
pub fn trunca_trecho(texto: &str, max_chars: usize) -> String {
    let limpo = texto.replace('\n', " ").trim().to_string();
    if limpo.chars().count() > max_chars {
        let mut s: String = limpo.chars().take(max_chars).collect();
        s.push_str("...");
        s
    } else {
        limpo
    }
}

/// Barra visual de resposta ativa exibida sobre o campo de envio.
pub fn barra_resposta<'a>(resposta: &'a crate::sala::RespostaA) -> Element<'a, Msg> {
    let trecho = trunca_trecho(&resposta.texto, 60);
    let rotulo = if trecho.is_empty() {
        format!("Respondendo a {}", resposta.autor)
    } else {
        format!("Respondendo a {}: \"{}\"", resposta.autor, trecho)
    };

    let aviso = row![
        svg(icones::responder()).width(14).height(14),
        text(rotulo).size(12).style(fraco),
        space().width(Length::Fill),
        button(svg(icones::fechar()).width(14).height(14))
            .padding([2, 6])
            .style(tema::botao_secundario)
            .on_press(Msg::CancelaRespostaChat),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    container(aviso)
        .padding([4, 12])
        .width(Length::Fill)
        .style(tema::painel_lateral)
        .into()
}

/// Renderização visual da citação de resposta acima do corpo da mensagem.
pub fn citacao_resposta<'a>(resposta: &'a crate::sala::RespostaA) -> Element<'a, Msg> {
    let trecho = trunca_trecho(&resposta.texto, 80);
    let citacao = row![
        text("↳").size(12).style(fraco),
        text(&resposta.autor).size(12).font(negrito()),
        text(format!(": {trecho}")).size(12).style(fraco),
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    container(citacao).into()
}

pub const EMOJIS_PERMITIDOS: &[&str] = &[
    "👍", "👎", "😂", "😮", "😢", "🔥", "❤️", "🎉", "👀", "🤔", "💀", "🙏", "✅", "❌", "🚀", "🧠",
];

/// Lista de reações já presentes em uma mensagem.
pub fn barra_reacoes_mensagem<'a>(
    id: &'a str,
    reacoes: &'a std::collections::HashMap<String, Vec<String>>,
    meu_nome: &str,
) -> Option<Element<'a, Msg>> {
    let mut itens: Vec<(&String, &Vec<String>)> =
        reacoes.iter().filter(|(_, q)| !q.is_empty()).collect();
    if itens.is_empty() {
        return None;
    }
    itens.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(b.0)));

    let mut linha = row![].spacing(4).align_y(Alignment::Center);
    for (emoji, quem) in itens {
        let eh_meu = quem.iter().any(|n| n == meu_nome);
        let mut btn = button(
            row![
                text(emoji.as_str()).size(15).font(tema::FONTE_EMOJI),
                text(quem.len()).size(12),
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        )
        .padding([2, 6])
        .on_press(Msg::ReagirChat {
            id: id.to_string(),
            emoji: emoji.to_string(),
        });

        if eh_meu {
            btn = btn.style(tema::botao_primario);
        } else {
            btn = btn.style(tema::botao_secundario);
        }
        linha = linha.push(btn);
    }

    linha = linha.push(
        button(
            svg(icones::smile())
                .width(14)
                .height(14)
                .style(tema::icone_cor(tema::cores().texto_medio)),
        )
        .padding([3, 5])
        .style(tema::botao_secundario)
        .on_press(Msg::AlternaSeletorReacao(id.to_string())),
    );

    Some(linha.into())
}

/// Seletor rápido de emojis para reagir a uma mensagem específica.
pub fn seletor_rapido_emoji<'a>(id: &'a str) -> Element<'a, Msg> {
    let mut grade = row![].spacing(3).align_y(Alignment::Center);
    for &emoji in EMOJIS_PERMITIDOS.iter().take(8) {
        grade = grade.push(
            button(text(emoji).size(15).font(crate::tema::FONTE_EMOJI))
                .padding([2, 5])
                .style(tema::botao_secundario)
                .on_press(Msg::ReagirChat {
                    id: id.to_string(),
                    emoji: emoji.to_string(),
                }),
        );
    }

    let caixa = container(grade).padding([4, 6]).style(tema::painel_lateral);

    caixa.into()
}

/// Barra de mensagem fixada exibida no topo do histórico de chat.
pub fn barra_fixada<'a>(fixada: &'a crate::sala::MensagemFixada) -> Element<'a, Msg> {
    let trecho = trunca_trecho(&fixada.texto, 80);
    let rotulo = if trecho.is_empty() {
        format!("{}: [mídia]", fixada.autor)
    } else {
        format!("{}: \"{}\"", fixada.autor, trecho)
    };

    let conteudo = row![
        svg(icones::pin()).width(14).height(14),
        column![
            text(format!("Fixado por {}", fixada.fixado_por))
                .size(11)
                .style(fraco),
            text(rotulo).size(13).font(negrito()),
        ]
        .spacing(1),
        space().width(Length::Fill),
        button(svg(icones::fechar()).width(14).height(14))
            .padding([4, 6])
            .style(tema::botao_secundario)
            .on_press(Msg::DesafixaChat),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    container(conteudo)
        .padding([6, 12])
        .width(Length::Fill)
        .style(tema::painel_lateral)
        .into()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SegmentoTexto<'a> {
    Texto(&'a str),
    Link(&'a str),
}

/// Identifica URLs (http://, https://, www.) em um texto plano.
pub fn separa_links<'a>(texto: &'a str) -> Vec<SegmentoTexto<'a>> {
    let mut segmentos = Vec::new();
    let mut restante = texto;

    while let Some(idx) = restante
        .find("http://")
        .or_else(|| restante.find("https://"))
        .or_else(|| restante.find("www."))
    {
        if idx > 0 {
            segmentos.push(SegmentoTexto::Texto(&restante[..idx]));
        }
        let depois = &restante[idx..];
        let fim = depois
            .find(|c: char| c.is_whitespace() || c == '<' || c == '>' || c == '"' || c == '\'')
            .unwrap_or(depois.len());
        let mut url_bruta = &depois[..fim];
        while url_bruta.ends_with('.')
            || url_bruta.ends_with(',')
            || url_bruta.ends_with(';')
            || url_bruta.ends_with(':')
            || url_bruta.ends_with('!')
            || url_bruta.ends_with('?')
            || url_bruta.ends_with(')')
        {
            url_bruta = &url_bruta[..url_bruta.len() - 1];
        }
        if !url_bruta.is_empty() {
            segmentos.push(SegmentoTexto::Link(url_bruta));
            restante = &depois[url_bruta.len()..];
        } else {
            segmentos.push(SegmentoTexto::Texto(&depois[..fim]));
            restante = &depois[fim..];
        }
    }

    if !restante.is_empty() {
        segmentos.push(SegmentoTexto::Texto(restante));
    }

    if segmentos.is_empty() {
        segmentos.push(SegmentoTexto::Texto(""));
    }

    segmentos
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrechoMensagem<'a> {
    Texto(&'a str),
    BlocoCodigo { linguagem: &'a str, codigo: &'a str },
}

/// Separa texto livre e blocos de código delimitados por ``` (cercas).
pub fn separa_blocos_codigo<'a>(texto: &'a str) -> Vec<TrechoMensagem<'a>> {
    let mut trechos = Vec::new();
    let mut restante = texto;

    while let Some(inicio) = restante.find("```") {
        if inicio > 0 {
            trechos.push(TrechoMensagem::Texto(&restante[..inicio]));
        }
        let depois_cerca = &restante[inicio + 3..];
        if let Some(fim) = depois_cerca.find("```") {
            let conteudo_bloco = &depois_cerca[..fim];
            let (linguagem, codigo) = if let Some(quebra) = conteudo_bloco.find('\n') {
                let lang = conteudo_bloco[..quebra].trim();
                let code = conteudo_bloco[quebra + 1..].trim_end_matches('\r');
                (lang, code)
            } else {
                ("", conteudo_bloco)
            };
            trechos.push(TrechoMensagem::BlocoCodigo {
                linguagem: if linguagem.is_empty() {
                    "código"
                } else {
                    linguagem
                },
                codigo: codigo.trim_matches('\n'),
            });
            restante = &depois_cerca[fim + 3..];
        } else {
            trechos.push(TrechoMensagem::Texto(restante));
            restante = "";
            break;
        }
    }

    if !restante.is_empty() {
        trechos.push(TrechoMensagem::Texto(restante));
    }

    if trechos.is_empty() {
        trechos.push(TrechoMensagem::Texto(""));
    }

    trechos
}

/// Renderiza um bloco de código monoespaçado com cabeçalho e botão de cópia.
pub fn renderiza_bloco_codigo<'a>(linguagem: &'a str, codigo: &'a str) -> Element<'a, Msg> {
    let topo = row![
        text(linguagem).size(11).style(fraco),
        space().width(Length::Fill),
        button(
            row![
                svg(icones::copiar()).width(12).height(12),
                text("Copiar").size(11),
            ]
            .spacing(4)
            .align_y(Alignment::Center)
        )
        .padding([2, 6])
        .style(tema::botao_secundario)
        .on_press(Msg::CopiaTexto(codigo.to_string())),
    ]
    .align_y(Alignment::Center);

    let corpo_codigo = text(codigo).font(iced::Font::MONOSPACE).size(13);

    let caixa = column![
        topo,
        container(corpo_codigo)
            .padding([8, 10])
            .width(Length::Fill)
            .style(tema::painel_lateral)
    ]
    .spacing(4);

    container(caixa)
        .padding([6, 8])
        .width(Length::Fill)
        .style(tema::painel_lateral)
        .into()
}

/// Texto da mensagem: links clicáveis no meio do texto (quebrando a linha
/// junto, como no Discord), emoji colorido e o "(editada)" no fim. Mensagem só
/// de emoji (até 27) sai grande.
/// Mensagem só de emoji (até 27): aparece grande, como no Discord.
pub fn so_emoji(texto: &str) -> bool {
    !texto.trim().is_empty()
        && texto
            .chars()
            .all(|ch| ch.is_whitespace() || tema::eh_emoji(ch))
        && texto.chars().filter(|ch| !ch.is_whitespace()).count() <= 27
}

pub fn renderiza_texto_com_links<'a>(texto: &'a str, editada: bool) -> Element<'a, Msg> {
    use iced::widget::span;
    let c = tema::cores();
    let so_emoji = so_emoji(texto);
    let mut spans: Vec<iced::widget::text::Span<'a, String>> = Vec::new();
    for s in separa_links(texto) {
        match s {
            SegmentoTexto::Texto(t) => {
                for (e, pedaco) in tema::trechos_de_emoji(t) {
                    let sp = span(pedaco);
                    spans.push(if e { sp.font(tema::FONTE_EMOJI) } else { sp });
                }
            }
            SegmentoTexto::Link(url) => {
                spans.push(span(url).color(c.destaque).link(url.to_string()));
            }
        }
    }
    if editada {
        spans.push(span(" (editada)").size(11).color(c.texto_medio));
    }
    iced::widget::rich_text(spans)
        .size(if so_emoji { 44.0 } else { tema::TEXTO_CHAT })
        .on_link_click(Msg::AbreLink)
        .into()
}

/// Tag visual `(editada)` exibida ao lado do texto da mensagem.
pub fn tag_editada<'a>() -> Element<'a, Msg> {
    text(" (editada)").size(11).style(fraco).into()
}

/// Formata o texto descritivo de quem está digitando na sala.
pub fn texto_digitando(nomes: &[&str]) -> Option<String> {
    match nomes.len() {
        0 => None,
        1 => Some(format!("{} está digitando...", nomes[0])),
        2 => Some(format!("{} e {} estão digitando...", nomes[0], nomes[1])),
        _ => Some("Várias pessoas estão digitando...".to_string()),
    }
}

/// Barra visual informativa exibida quando alguém está digitando.
pub fn barra_digitando<'a>(texto: impl iced::widget::text::IntoFragment<'a>) -> Element<'a, Msg> {
    text(texto).size(12).style(fraco).into()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidatoMencao {
    pub nome: String,
    pub arroba: Option<String>,
}

/// Extrai o prefixo de menção ativa no final do texto do rascunho.
/// Só detecta quando o '@' estiver no início do texto ou precedido de espaço.
pub fn detecta_mencao_rascunho(rascunho: &str) -> Option<&str> {
    let idx = rascunho.rfind('@')?;
    if idx > 0 {
        let antes = rascunho[..idx].chars().last()?;
        if !antes.is_whitespace() {
            return None;
        }
    }
    let trecho = &rascunho[idx + 1..];
    if trecho.contains(|c: char| c.is_whitespace()) {
        return None;
    }
    Some(trecho)
}

/// Filtra e ordena candidatos para autocompletar menção com base no prefixo.
pub fn filtra_candidatos_mencao(
    prefixo: &str,
    pessoas: &[crate::sala::Pessoa],
) -> Vec<CandidatoMencao> {
    let p_lower = prefixo.to_lowercase();
    let mut candidatos: Vec<CandidatoMencao> = pessoas
        .iter()
        .filter(|p| !p.eu)
        .filter(|p| {
            p.nome.to_lowercase().contains(&p_lower)
                || p.uid
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&p_lower)
        })
        .map(|p| CandidatoMencao {
            nome: p.nome.clone(),
            arroba: p.uid.clone(),
        })
        .collect();

    candidatos.sort_by(|a, b| {
        let a_starts = a.nome.to_lowercase().starts_with(&p_lower);
        let b_starts = b.nome.to_lowercase().starts_with(&p_lower);
        b_starts.cmp(&a_starts).then_with(|| a.nome.cmp(&b.nome))
    });

    candidatos.truncate(6);
    candidatos
}

/// Renderiza o menu de sugestões de autocompletar menção acima do campo de envio.
pub fn menu_autocompletar_mencao<'a>(candidatos: &[CandidatoMencao]) -> Element<'a, Msg> {
    let mut itens = row![].spacing(6).align_y(Alignment::Center);
    itens = itens.push(text("@ Mencionar:").size(12).style(fraco));

    for c in candidatos {
        let rotulo = if let Some(arr) = &c.arroba {
            format!("@{} ({})", c.nome, arr)
        } else {
            format!("@{}", c.nome)
        };
        let btn = button(text(rotulo).size(12))
            .padding([3, 8])
            .style(tema::botao_secundario)
            .on_press(Msg::CompletaMencao(c.nome.clone()));
        itens = itens.push(btn);
    }

    container(itens)
        .padding([4, 8])
        .style(tema::painel_lateral)
        .into()
}

/// Verifica se o usuário local foi mencionado na mensagem.
pub fn citou_voce(m: &crate::sala::Mensagem, meu_nome: &str, meu_uid: Option<&str>) -> bool {
    if eh_minha_mensagem(m, meu_nome, meu_uid) {
        return false;
    }
    if let Some(uid) = meu_uid
        && !m.mencoes_uids.is_empty()
        && m.mencoes_uids.iter().any(|u| u == uid)
    {
        return true;
    }
    if !m.mencoes.is_empty() && m.mencoes.iter().any(|n| n.eq_ignore_ascii_case(meu_nome)) {
        return true;
    }
    let alvo = m.texto.to_lowercase();
    let arroba_nome = format!("@{}", meu_nome.to_lowercase());
    alvo.contains(&arroba_nome)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoNotificacaoChat {
    Mencao,
    Sussurro,
    Mensagem,
}

/// Prepara título e corpo para notificação do sistema com base no tipo de evento e conteúdo.
pub fn prepara_notificacao_chat(
    tipo: TipoNotificacaoChat,
    sala: &str,
    autor: &str,
    texto: &str,
    tem_imagem: bool,
    tem_gif: bool,
    tem_enquete: bool,
) -> (String, String) {
    let titulo = match tipo {
        TipoNotificacaoChat::Mencao => format!("{autor} mencionou você"),
        TipoNotificacaoChat::Sussurro => format!("{autor} sussurrou para você"),
        TipoNotificacaoChat::Mensagem => format!("{autor} em #{sala}"),
    };
    let trecho = trunca_trecho(texto, 100);
    let corpo = if !trecho.is_empty() {
        trecho
    } else if tem_enquete {
        "Enquete".to_string()
    } else if tem_gif {
        "GIF".to_string()
    } else if tem_imagem {
        "Imagem".to_string()
    } else {
        "Nova mensagem".to_string()
    };
    (titulo, corpo)
}

/// Dispara notificação desktop no sistema operacional via notify-send em thread separada.
pub fn dispara_notificacao_desktop(titulo: &str, corpo: &str, urgencia: &str) {
    let titulo = titulo.to_string();
    let corpo = corpo.to_string();
    let urgencia = urgencia.to_string();
    std::thread::spawn(move || {
        let _ = std::process::Command::new("notify-send")
            .args(["-a", "Lincast Chat", "-u", &urgencia, &titulo, &corpo])
            .output();
    });
}

/// Compatibilidade com chamadas diretas de menção.
#[allow(dead_code)]
pub fn notifica_mencao(autor: &str, texto: &str) {
    let (titulo, corpo) = prepara_notificacao_chat(
        TipoNotificacaoChat::Mencao,
        "",
        autor,
        texto,
        false,
        false,
        false,
    );
    dispara_notificacao_desktop(&titulo, &corpo, "normal");
}

/// Valida se a URL é HTTPS e aponta para o domínio do Giphy (mesma regra do servidor e v1).
pub fn eh_url_de_gif(url: &str) -> bool {
    let limpo = url.trim();
    if limpo.len() > 400 {
        return false;
    }
    let rest = match limpo.strip_prefix("https://") {
        Some(r) => r,
        None => return false,
    };
    let host = match rest.split('/').next() {
        Some(h) => h.split(':').next().unwrap_or("").to_lowercase(),
        None => return false,
    };
    host == "giphy.com" || host.ends_with(".giphy.com")
}

/// Indicador visual enquanto um GIF da mensagem está sendo baixado.
pub fn gif_carregando<'a>(url: &'a str) -> Element<'a, Msg> {
    container(
        row![
            text("Carregando GIF...").size(12).style(fraco),
            space().width(Length::Fixed(8.0)),
            button(text("Abrir").size(11))
                .style(tema::botao_secundario)
                .padding([2, 6])
                .on_press(Msg::AbreLink(url.to_string())),
        ]
        .align_y(Alignment::Center),
    )
    .padding(6)
    .style(tema::painel_lateral)
    .into()
}

/// Painel de busca e seleção de GIFs.
/// Alvo de mensagem privada (sussurro).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlvoSussurro {
    pub uid: Option<String>,
    pub nome: String,
}

impl AlvoSussurro {
    pub fn mesma_pessoa(&self, p: &crate::sala::Pessoa) -> bool {
        if let (Some(u1), Some(u2)) = (&self.uid, &p.uid) {
            return u1 == u2;
        }
        !self.nome.is_empty() && self.nome == p.nome
    }
}

/// Barra visual de sussurro pendente exibida sobre o campo de envio.
pub fn barra_sussurro<'a>(
    alvos: &'a [AlvoSussurro],
    presentes: usize,
    erro: Option<&'a str>,
) -> Element<'a, Msg> {
    let nomes = alvos
        .iter()
        .map(|a| a.nome.as_str())
        .collect::<Vec<_>>()
        .join(", ");

    let texto_aviso = if let Some(e) = erro {
        e.to_string()
    } else if presentes > 0 {
        format!("Sussurrando para {nomes} — só texto, e quem não está aqui agora não recebe.")
    } else {
        format!("{nomes} não está na sala agora — o sussurro só sai quando voltar.")
    };

    let linha = row![
        crate::ui::efeitos::ico(crate::icones::fone(), 14.0, tema::cores().destaque),
        text(texto_aviso).size(12).style(if erro.is_some() {
            texto_perigo
        } else {
            destaque
        }),
        space().width(Length::Fill),
        button(svg(icones::fechar()).width(14).height(14))
            .style(tema::botao_secundario)
            .padding([2, 6])
            .on_press(Msg::LimpaSussurro),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    container(linha)
        .padding([4, 12])
        .width(Length::Fill)
        .style(tema::painel_lateral)
        .into()
}

/// Menu para marcar participantes da sala como alvos do sussurro.
pub fn menu_sussurro<'a>(
    pessoas: &'a [crate::sala::Pessoa],
    alvos: &'a [AlvoSussurro],
) -> Element<'a, Msg> {
    let outros: Vec<&crate::sala::Pessoa> = pessoas.iter().filter(|p| !p.eu).collect();
    let mut conteudo = column![].spacing(6);

    let topo = row![
        text(if outros.is_empty() {
            "Ninguém mais na sala para sussurrar"
        } else {
            "Sussurrar para (marque quem vai ler):"
        })
        .size(12)
        .style(fraco),
        space().width(Length::Fill),
        button(svg(icones::fechar()).width(14).height(14))
            .style(tema::botao_secundario)
            .padding([2, 6])
            .on_press(Msg::AlternaMenuSussurro),
    ]
    .align_y(Alignment::Center);

    conteudo = conteudo.push(topo);

    if !outros.is_empty() {
        let mut lista_pessoas = row![].spacing(6).align_y(Alignment::Center);
        for p in outros {
            let marcado = alvos.iter().any(|a| a.mesma_pessoa(p));
            let rotulo = if marcado {
                format!("✓ {}", p.nome)
            } else {
                p.nome.clone()
            };
            let btn = button(text(rotulo).size(12))
                .style(if marcado {
                    tema::botao_primario
                } else {
                    tema::botao_secundario
                })
                .padding([4, 8])
                .on_press(Msg::AlternaAlvoSussurro {
                    uid: p.uid.clone(),
                    nome: p.nome.clone(),
                });
            lista_pessoas = lista_pessoas.push(btn);
        }
        conteudo = conteudo.push(lista_pessoas);
    }

    if !alvos.is_empty() {
        let limpar = button(text("Voltar a falar com a sala toda").size(12))
            .style(tema::botao_secundario)
            .padding([3, 8])
            .on_press(Msg::LimpaSussurro);
        conteudo = conteudo.push(limpar);
    }

    container(conteudo)
        .padding([6, 12])
        .width(Length::Fill)
        .style(tema::painel_lateral)
        .into()
}

/// Marca visual do sussurro exibida antes do nome na mensagem.
pub fn marca_sussurro<'a>(para: &'a [String], eh_minha: bool) -> Element<'a, Msg> {
    let rotulo = if eh_minha && !para.is_empty() {
        format!("só para {}", para.join(", "))
    } else {
        "sussurro".to_string()
    };
    text(rotulo).size(12).style(destaque).into()
}

/// Barra de busca no chat (`chatSearchBar`).
pub fn barra_busca_chat<'a>(termo: &'a str, encontradas: usize, total: usize) -> Element<'a, Msg> {
    let texto_contador = if termo.trim().is_empty() {
        format!("{total} carregadas")
    } else if encontradas > 0 {
        format!("{encontradas} de {total}")
    } else {
        "nada encontrado".to_string()
    };

    let campo = text_input("Buscar nas mensagens carregadas...", termo)
        .on_input(Msg::BuscaChatInput)
        .padding([6, 10])
        .size(13)
        .style(tema::campo);

    let linha = row![
        crate::ui::efeitos::ico(crate::icones::busca(), 14.0, tema::cores().texto_fraco),
        campo,
        text(texto_contador).size(11).style(fraco),
        button(svg(icones::fechar()).width(14).height(14))
            .style(tema::botao_secundario)
            .padding([3, 6])
            .on_press(Msg::FechaBuscaChat),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    container(linha)
        .padding([6, 12])
        .width(Length::Fill)
        .style(tema::painel_lateral)
        .into()
}

/// Divisor visual de novas mensagens não lidas inserido entre mensagens (`unread-divider`).
pub fn divisor_nao_lidas<'a>() -> Element<'a, Msg> {
    let linha_esq = container(space().height(1))
        .width(Length::Fill)
        .style(tema::linha_divisor_nao_lidas);
    let linha_dir = container(space().height(1))
        .width(Length::Fill)
        .style(tema::linha_divisor_nao_lidas);
    let etiqueta = text("NOVAS MENSAGENS")
        .size(10)
        .font(negrito())
        .style(texto_perigo);

    row![linha_esq, etiqueta, linha_dir]
        .spacing(8)
        .align_y(Alignment::Center)
        .padding([6, 0])
        .into()
}

/// Pílula de mensagens não lidas exibida acima do envio (`chatUnreadPill`).
pub fn pilula_nao_lidas<'a>(quantidade: usize) -> Element<'a, Msg> {
    let rotulo = if quantidade == 1 {
        "1 mensagem nova ↓".to_string()
    } else {
        format!("{quantidade} mensagens novas ↓")
    };

    let btn = button(text(rotulo).size(12).font(negrito()))
        .style(tema::botao_nao_lidas)
        .padding([4, 12])
        .on_press(Msg::LimpaNaoLidas);

    container(btn)
        .align_x(Alignment::Center)
        .width(Length::Fill)
        .padding([2, 0])
        .into()
}

/// Filtra mensagens que casam com o termo de busca (no texto ou autor).
pub fn filtra_mensagens<'a>(
    mensagens: &'a [crate::sala::Mensagem],
    termo: &str,
) -> Vec<&'a crate::sala::Mensagem> {
    let limpo = termo.trim().to_lowercase();
    if limpo.is_empty() {
        return mensagens.iter().collect();
    }
    mensagens
        .iter()
        .filter(|m| {
            m.texto.to_lowercase().contains(&limpo) || m.autor.to_lowercase().contains(&limpo)
        })
        .collect()
}

/// Chave de identidade usada para registro de voto em enquete (UID se existir, nome caso contrário).
pub fn minha_chave_voto(meu_nome: &str, meu_uid: Option<&str>) -> String {
    meu_uid
        .map(String::from)
        .unwrap_or_else(|| format!("nome:{meu_nome}"))
}

/// Renderiza o quadro interativo de enquete (`.enquete`) com opções de votação.
pub fn renderiza_enquete<'a>(
    id: &str,
    enquete: &'a crate::sala::Enquete,
    votos: &HashMap<String, usize>,
    minha_chave: &str,
) -> Element<'a, Msg> {
    let mut opcoes_col = column![].spacing(6);

    let total_votos = votos.len();
    let meu_voto = votos.get(minha_chave).copied();

    for (i, texto_opcao) in enquete.opcoes.iter().enumerate() {
        let contagem = votos.values().filter(|&&v| v == i).count();
        let porcentagem = (contagem * 100).checked_div(total_votos).unwrap_or(0);
        let votada = meu_voto == Some(i);

        let texto_rotulo = text(texto_opcao).size(13);
        let texto_stats = if total_votos > 0 {
            format!("{contagem} · {porcentagem}%")
        } else {
            String::new()
        };

        let linha_opcao = row![
            texto_rotulo,
            space().width(Length::Fill),
            text(texto_stats).size(12).style(fraco),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let botao = button(linha_opcao)
            .style(tema::opcao_enquete(votada))
            .padding([8, 12])
            .width(Length::Fill)
            .on_press(Msg::VotaEnquete {
                id: id.to_string(),
                opcao: i,
            });

        opcoes_col = opcoes_col.push(botao);
    }

    let texto_rodape = if total_votos == 0 {
        "Ninguém votou ainda".to_string()
    } else if total_votos == 1 {
        "1 voto — clique de novo para retirar o seu".to_string()
    } else {
        format!("{total_votos} votos — clique de novo para retirar o seu")
    };

    let caixa = column![
        text(enquete.pergunta.clone()).size(14).font(negrito()),
        opcoes_col,
        text(texto_rodape).size(11).style(fraco),
    ]
    .spacing(8);

    container(caixa)
        .padding(10)
        .width(Length::Fill)
        .style(tema::painel_lateral)
        .into()
}

/// Resultado da interpretação de comandos do chat (/rolar, /sortear, /enquete, /wordle).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComandoChatResultado {
    MensagemTexto(String),
    CriarEnquete(crate::sala::Enquete),
    /// `/wordle`: abre o jogo do dia (e garante a mensagem na conversa).
    AbreWordle,
    /// [DEV] Sorteia uma nova palavra para o Wordle de hoje para todos os jogadores.
    WordleTrocaPalavra,
    Erro(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SugestaoComando {
    pub nome: &'static str,
    pub sintaxe: &'static str,
    pub descricao: &'static str,
    pub preenche: &'static str,
}

/// Detecta se o rascunho atual começou com `/` para autocompletar comando.
pub fn detecta_comando_rascunho(rascunho: &str) -> Option<&str> {
    let limpo = rascunho.trim_start();
    if limpo.starts_with('/') && !limpo.contains(' ') {
        Some(&limpo[1..])
    } else {
        None
    }
}

/// Filtra os comandos disponíveis para sugestão no chat.
pub fn filtra_comandos_chat(filtro: &str, eh_dev: bool, eh_grupo: bool) -> Vec<SugestaoComando> {
    let f = filtro.to_lowercase();
    let mut todos = Vec::new();

    if eh_grupo {
        todos.push(SugestaoComando {
            nome: "/wordle",
            sintaxe: "/wordle",
            descricao: "Abre o jogo Wordle do dia",
            preenche: "/wordle",
        });
    }

    todos.extend([
        SugestaoComando {
            nome: "/rolar",
            sintaxe: "/rolar 2d20",
            descricao: "Rola dados de RPG (ex: 2d6, 1d20)",
            preenche: "/rolar ",
        },
        SugestaoComando {
            nome: "/sortear",
            sintaxe: "/sortear pizza | sushi",
            descricao: "Sorteia entre opções separadas por |",
            preenche: "/sortear ",
        },
        SugestaoComando {
            nome: "/enquete",
            sintaxe: "/enquete Pergunta? | Opção 1 | Opção 2",
            descricao: "Cria uma enquete de votação interativa",
            preenche: "/enquete ",
        },
    ]);

    if eh_dev && eh_grupo {
        todos.push(SugestaoComando {
            nome: "/wordle-trocar",
            sintaxe: "/wordle-trocar",
            descricao: "[DEV] Sorteia uma nova palavra do Wordle hoje para todos",
            preenche: "/wordle-trocar",
        });
    }

    todos
        .into_iter()
        .filter(|c| c.nome[1..].starts_with(&f))
        .collect()
}

/// Menu flutuante que exibe os comandos ao digitar `/` no chat.
pub fn menu_comandos_chat<'a>(comandos: &[SugestaoComando]) -> Element<'a, Msg> {
    let c = tema::cores();
    let mut lista = Column::new().spacing(3);

    for cmd in comandos {
        let icone = match cmd.nome {
            "/wordle" => icones::controle(),
            "/rolar" => icones::dados(),
            "/sortear" => icones::roda(),
            "/enquete" => icones::grafico(),
            _ => icones::lapis(),
        };

        let btn = button(
            row![
                svg(icone)
                    .width(16)
                    .height(16)
                    .style(tema::icone_cor(c.destaque)),
                column![
                    row![
                        text(cmd.nome).size(13).font(negrito()),
                        space().width(6),
                        text(cmd.sintaxe).size(12).style(fraco),
                    ]
                    .align_y(Alignment::Center),
                    text(cmd.descricao).size(11).style(fraco),
                ]
                .spacing(2)
                .width(Length::Fill),
                text("Comando").size(11).style(fraco),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding([6, 10])
        .style(tema::item_de_lista)
        .on_press(Msg::AplicaComandoChat(cmd.preenche.to_string()));

        lista = lista.push(btn);
    }

    container(
        column![
            row![
                svg(icones::ajustes())
                    .width(13)
                    .height(13)
                    .style(tema::icone_cor(c.texto_fraco)),
                text("COMANDOS DISPONÍVEIS")
                    .size(10)
                    .font(negrito())
                    .style(fraco),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
            lista,
        ]
        .spacing(6)
        .padding(8),
    )
    .width(Length::Fill)
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

/// Analisa e processa comandos com prefixo `/` digitados no chat.
pub fn processa_comando_chat(entrada: &str) -> Option<ComandoChatResultado> {
    let limpo = entrada.trim();
    if !limpo.starts_with('/') {
        return None;
    }
    let (cmd, resto) = match limpo.split_once(char::is_whitespace) {
        Some((c, r)) => (c.to_lowercase(), r.trim()),
        None => (limpo.to_lowercase(), ""),
    };
    match cmd.as_str() {
        "/wordle" => Some(ComandoChatResultado::AbreWordle),
        "/wordle-trocar" | "/wordle-novo" | "/wordle-reset" => {
            Some(ComandoChatResultado::WordleTrocaPalavra)
        }
        "/rolar" | "/dados" => {
            if resto.is_empty() {
                return Some(ComandoChatResultado::Erro("Uso: /rolar 2d6".into()));
            }
            Some(executa_comando_rolar(resto))
        }
        "/sortear" | "/escolher" => {
            if resto.is_empty() {
                return Some(ComandoChatResultado::Erro(
                    "Uso: /sortear pizza | hambúrguer | japonês".into(),
                ));
            }
            Some(executa_comando_sortear(resto))
        }
        "/enquete" => {
            if resto.is_empty() {
                return Some(ComandoChatResultado::Erro(
                    "Uso: /enquete Pergunta? | Opção 1 | Opção 2".into(),
                ));
            }
            Some(executa_comando_enquete(resto))
        }
        _ => None,
    }
}

pub fn executa_comando_rolar(resto: &str) -> ComandoChatResultado {
    let lower = resto.to_lowercase();
    let Some((quantos_str, lados_str)) = lower.split_once('d') else {
        return ComandoChatResultado::Erro("Uso: /rolar 2d6".into());
    };
    let quantos: u32 = if quantos_str.trim().is_empty() {
        1
    } else {
        match quantos_str.trim().parse() {
            Ok(n) => n,
            Err(_) => return ComandoChatResultado::Erro("Uso: /rolar 2d6".into()),
        }
    };
    let lados: u32 = match lados_str.trim().parse() {
        Ok(n) => n,
        Err(_) => return ComandoChatResultado::Erro("Uso: /rolar 2d6".into()),
    };
    let quantos = quantos.clamp(1, 50);
    let lados = lados.clamp(2, 1000);

    let mut valores = Vec::with_capacity(quantos as usize);
    let mut soma: u32 = 0;
    for _ in 0..quantos {
        let v = fastrand::u32(1..=lados);
        soma += v;
        valores.push(v);
    }
    let resultado = if quantos > 1 {
        let lista = valores
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        format!("🎲 {quantos}d{lados} = {soma} [{lista}]")
    } else {
        format!("🎲 {quantos}d{lados} = {soma}")
    };
    ComandoChatResultado::MensagemTexto(resultado)
}

pub fn executa_comando_sortear(resto: &str) -> ComandoChatResultado {
    let opcoes: Vec<&str> = resto
        .split('|')
        .map(|o| o.trim())
        .filter(|o| !o.is_empty())
        .collect();
    if opcoes.len() < 2 {
        return ComandoChatResultado::Erro("Uso: /sortear pizza | hambúrguer | japonês".into());
    }
    let idx = fastrand::usize(0..opcoes.len());
    let escolhida = opcoes[idx];
    ComandoChatResultado::MensagemTexto(format!("🎯 Sorteei entre {}: {escolhida}", opcoes.len()))
}

pub fn executa_comando_enquete(resto: &str) -> ComandoChatResultado {
    let partes: Vec<&str> = resto
        .split('|')
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect();
    if partes.len() < 3 {
        return ComandoChatResultado::Erro("Uso: /enquete Pergunta? | Opção 1 | Opção 2".into());
    }
    let mut pergunta: String = partes[0].chars().take(140).collect();
    pergunta = pergunta.replace(['\r', '\n', '\t'], " ").trim().to_string();
    let opcoes: Vec<String> = partes[1..]
        .iter()
        .take(6)
        .map(|o| {
            let s: String = o.chars().take(60).collect();
            s.replace(['\r', '\n', '\t'], " ").trim().to_string()
        })
        .filter(|o| !o.is_empty())
        .collect();
    if pergunta.is_empty() || opcoes.len() < 2 {
        return ComandoChatResultado::Erro(
            "Precisa de uma pergunta e pelo menos duas opções.".into(),
        );
    }
    ComandoChatResultado::CriarEnquete(crate::sala::Enquete { pergunta, opcoes })
}

#[cfg(test)]
mod tests {
    #[test]
    fn tamanho_legivel_usa_a_unidade_certa() {
        assert_eq!(tamanho_legivel(512), "512 B");
        assert_eq!(tamanho_legivel(2048), "2 KB");
        assert_eq!(tamanho_legivel(6 * 1024 * 1024 + 300 * 1024), "6,3 MB");
        assert_eq!(tamanho_legivel(3 * 1024 * 1024 * 1024), "3,00 GB");
    }

    #[test]
    fn caminho_livre_nao_sobrescreve() {
        let dir = std::env::temp_dir().join(format!("lincast-teste-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p1 = caminho_livre(&dir, "relatorio.zip");
        assert_eq!(p1, dir.join("relatorio.zip"));
        std::fs::write(&p1, b"x").unwrap();
        let p2 = caminho_livre(&dir, "relatorio.zip");
        assert_eq!(p2, dir.join("relatorio (1).zip"));
        // Barras no nome não saem da pasta.
        assert_eq!(caminho_livre(&dir, "../fuga"), dir.join(".._fuga"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    use super::*;

    #[test]
    fn validacao_de_mimes_e_tamanho() {
        // PNG válido por magic bytes
        let png_bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00];
        let res = valida_e_prepara_imagem("foto.png".into(), png_bytes);
        assert!(res.is_ok());
        let anexo = res.unwrap();
        assert_eq!(anexo.mime, "image/png");

        // JPEG válido por magic bytes
        let jpeg_bytes = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00];
        let res = valida_e_prepara_imagem("foto.jpg".into(), jpeg_bytes);
        assert!(res.is_ok());
        assert_eq!(res.unwrap().mime, "image/jpeg");

        // Arquivo maior que 5MB deve ser recusado
        let mut grande = vec![0u8; MAX_CHAT_IMAGE_BYTES + 10];
        grande[0..8].copy_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        let res = valida_e_prepara_imagem("grande.png".into(), grande);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err(), "Imagem maior que 5MB, escolha uma menor.");

        // Formato não permitido (ex: arquivo de texto)
        let txt_bytes = b"Hello world".to_vec();
        let res = valida_e_prepara_imagem("texto.txt".into(), txt_bytes);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err(), "Dá pra mandar PNG, JPEG, GIF ou WEBP.");
    }

    #[test]
    fn autoria_de_mensagem() {
        let m = crate::sala::Mensagem {
            id: "1".into(),
            autor: "Val".into(),
            uid: Some("u123".into()),
            texto: "teste".into(),
            ts: 1000,
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
            sistema: None,
            extra: Default::default(),
        };
        assert!(eh_minha_mensagem(&m, "Val", Some("u123")));
        assert!(!eh_minha_mensagem(&m, "Val", Some("u999")));
        assert!(eh_minha_mensagem(&m, "Val", None));
        assert!(!eh_minha_mensagem(&m, "Outro", None));
    }

    #[test]
    fn truncamento_de_trecho() {
        let curto = "olá mundo";
        assert_eq!(trunca_trecho(curto, 10), "olá mundo");

        let longo = "este é um texto razoavelmente longo para teste de truncamento de mensagem";
        let truncado = trunca_trecho(longo, 10);
        assert_eq!(truncado, "este é um ...");

        let multilinha = "linha 1\nlinha 2\nlinha 3";
        assert_eq!(trunca_trecho(multilinha, 30), "linha 1 linha 2 linha 3");
    }

    #[test]
    fn reacoes_permitidas() {
        assert!(EMOJIS_PERMITIDOS.contains(&"👍"));
        assert!(EMOJIS_PERMITIDOS.contains(&"❤️"));
        assert!(EMOJIS_PERMITIDOS.contains(&"🔥"));
        assert!(!EMOJIS_PERMITIDOS.contains(&"xyz"));
    }

    #[test]
    fn mensagem_fixada_trecho() {
        let fixada = crate::sala::MensagemFixada {
            id: "msg-1".into(),
            autor: "Val".into(),
            texto: "Mensagem importante do canal".into(),
            fixado_por: "Moderador".into(),
            ts: 12345678,
        };
        assert_eq!(fixada.autor, "Val");
        assert_eq!(fixada.fixado_por, "Moderador");
        assert_eq!(trunca_trecho(&fixada.texto, 10), "Mensagem i...");
    }

    #[test]
    fn formatacao_texto_digitando() {
        assert_eq!(texto_digitando(&[]), None);
        assert_eq!(
            texto_digitando(&["Alice"]),
            Some("Alice está digitando...".to_string())
        );
        assert_eq!(
            texto_digitando(&["Alice", "Bob"]),
            Some("Alice e Bob estão digitando...".to_string())
        );
        assert_eq!(
            texto_digitando(&["Alice", "Bob", "Carlos"]),
            Some("Várias pessoas estão digitando...".to_string())
        );
    }

    #[test]
    fn deteccao_de_mencao_no_rascunho() {
        assert_eq!(detecta_mencao_rascunho("olá @"), Some(""));
        assert_eq!(detecta_mencao_rascunho("@"), Some(""));
        assert_eq!(detecta_mencao_rascunho("@val"), Some("val"));
        assert_eq!(detecta_mencao_rascunho("olá @val"), Some("val"));
        assert_eq!(detecta_mencao_rascunho("email@dominio.com"), None);
        assert_eq!(detecta_mencao_rascunho("olá @val tudo bem"), None);
    }

    #[test]
    fn verificacao_citou_voce() {
        let m = crate::sala::Mensagem {
            id: "1".into(),
            autor: "Outro".into(),
            uid: Some("u2".into()),
            texto: "olá @Val tudo bem?".into(),
            ts: 1000,
            editada_em: None,
            imagem: None,
            gif: None,
            resposta_a: None,
            reacoes: std::collections::HashMap::new(),
            mencoes: vec!["Val".into()],
            mencoes_uids: vec!["u1".into()],
            sussurro: false,
            sussurro_para: Vec::new(),
            enquete: None,
            votos: std::collections::HashMap::new(),
            sistema: None,
            extra: Default::default(),
        };

        // Outro mencionou Val
        assert!(citou_voce(&m, "Val", Some("u1")));
        // Não mencionou Bob
        assert!(!citou_voce(&m, "Bob", Some("u99")));

        // Própria mensagem não cita a si mesmo para alerta
        let m_propria = crate::sala::Mensagem {
            id: "2".into(),
            autor: "Val".into(),
            uid: Some("u1".into()),
            texto: "@Val teste".into(),
            ts: 1000,
            editada_em: None,
            imagem: None,
            gif: None,
            resposta_a: None,
            reacoes: std::collections::HashMap::new(),
            mencoes: vec!["Val".into()],
            mencoes_uids: vec!["u1".into()],
            sussurro: false,
            sussurro_para: Vec::new(),
            enquete: None,
            votos: std::collections::HashMap::new(),
            sistema: None,
            extra: Default::default(),
        };
        assert!(!citou_voce(&m_propria, "Val", Some("u1")));
    }

    #[test]
    fn alvo_sussurro_comparacao() {
        let alvo = AlvoSussurro {
            uid: Some("u10".into()),
            nome: "Alice".into(),
        };
        let make_pessoa = |id: &str, nome: &str, uid: Option<&str>| crate::sala::Pessoa {
            id: id.into(),
            nome: nome.into(),
            uid: uid.map(String::from),
            eu: false,
            espectador: false,
            mudo: false,
            surdo: false,
            apresentando: false,
            com_camera: false,
            conexao: None,
            volume: 1.0,
            volume_tela: 1.0,
            volume_anterior: None,
            volume_tela_anterior: None,
            falando: false,
            mudo_forcado: false,
            dono: false,
            qualidade: None,
            avatar: None,
            cor: None,
            cor_nome: None,
            efeito_nome: None,
            bio: None,
            status: None,
            arroba: None,
            atividade: None,
        };
        let p_igual_uid = make_pessoa("conn-1", "Alice Mudou Nome", Some("u10"));
        assert!(alvo.mesma_pessoa(&p_igual_uid));

        let p_sem_uid_mesmo_nome = make_pessoa("conn-2", "Alice", None);
        assert!(alvo.mesma_pessoa(&p_sem_uid_mesmo_nome));

        let p_diferente = make_pessoa("conn-3", "Bob", Some("u20"));
        assert!(!alvo.mesma_pessoa(&p_diferente));
    }

    #[test]
    fn separacao_de_links() {
        let texto = "acesse https://github.com/LinCastr ou www.google.com para ver.";
        let segmentos = separa_links(texto);
        assert_eq!(
            segmentos,
            vec![
                SegmentoTexto::Texto("acesse "),
                SegmentoTexto::Link("https://github.com/LinCastr"),
                SegmentoTexto::Texto(" ou "),
                SegmentoTexto::Link("www.google.com"),
                SegmentoTexto::Texto(" para ver."),
            ]
        );
    }

    #[test]
    fn separacao_de_blocos_de_codigo() {
        let texto =
            "olha esse código:\n```rust\nfn main() {\n    println!(\"oi\");\n}\n```\nlegal né?";
        let trechos = separa_blocos_codigo(texto);
        assert_eq!(trechos.len(), 3);
        assert_eq!(trechos[0], TrechoMensagem::Texto("olha esse código:\n"));
        assert_eq!(
            trechos[1],
            TrechoMensagem::BlocoCodigo {
                linguagem: "rust",
                codigo: "fn main() {\n    println!(\"oi\");\n}",
            }
        );
        assert_eq!(trechos[2], TrechoMensagem::Texto("\nlegal né?"));
    }

    #[test]
    fn validacao_url_de_gif() {
        assert!(eh_url_de_gif(
            "https://media.giphy.com/media/test/giphy.gif"
        ));
        assert!(eh_url_de_gif("https://giphy.com/gifs/funny-cat"));
        assert!(eh_url_de_gif("https://i.giphy.com/test.gif"));
        assert!(!eh_url_de_gif(
            "http://media.giphy.com/media/test/giphy.gif"
        ));
        assert!(!eh_url_de_gif("https://malicious.com/giphy.com"));
        assert!(!eh_url_de_gif("https://notgiphy.com"));
        assert!(!eh_url_de_gif("ftp://giphy.com/test.gif"));
        assert!(!eh_url_de_gif(""));
    }

    #[test]
    fn filtragem_busca_chat() {
        let msg1 = crate::sala::Mensagem {
            id: "m1".into(),
            autor: "Alice".into(),
            uid: None,
            texto: "Olá mundo do Rust".into(),
            ts: 1000,
            editada_em: None,
            imagem: None,
            gif: None,
            resposta_a: None,
            reacoes: HashMap::new(),
            mencoes: Vec::new(),
            mencoes_uids: Vec::new(),
            sussurro: false,
            sussurro_para: Vec::new(),
            enquete: None,
            votos: HashMap::new(),
            sistema: None,
            extra: Default::default(),
        };
        let msg2 = crate::sala::Mensagem {
            id: "m2".into(),
            autor: "Bob".into(),
            uid: None,
            texto: "Bom dia pessoal".into(),
            ts: 2000,
            editada_em: None,
            imagem: None,
            gif: None,
            resposta_a: None,
            reacoes: HashMap::new(),
            mencoes: Vec::new(),
            mencoes_uids: Vec::new(),
            sussurro: false,
            sussurro_para: Vec::new(),
            enquete: None,
            votos: HashMap::new(),
            sistema: None,
            extra: Default::default(),
        };
        let lista = vec![msg1, msg2];

        // Busca vazia retorna tudo
        assert_eq!(filtra_mensagens(&lista, "").len(), 2);
        assert_eq!(filtra_mensagens(&lista, "   ").len(), 2);

        // Busca por texto (case-insensitive)
        let res_rust = filtra_mensagens(&lista, "rust");
        assert_eq!(res_rust.len(), 1);
        assert_eq!(res_rust[0].id, "m1");

        // Busca por autor
        let res_bob = filtra_mensagens(&lista, "bob");
        assert_eq!(res_bob.len(), 1);
        assert_eq!(res_bob[0].id, "m2");

        // Sem resultado
        assert_eq!(filtra_mensagens(&lista, "inexistente").len(), 0);
    }

    #[test]
    fn comandos_rolar_e_sortear_e_enquete() {
        // /rolar
        let r1 = processa_comando_chat("/rolar 2d6");
        assert!(
            matches!(r1, Some(ComandoChatResultado::MensagemTexto(txt)) if txt.starts_with("🎲 2d6 = "))
        );
        let r_invalido = processa_comando_chat("/rolar abc");
        assert!(matches!(r_invalido, Some(ComandoChatResultado::Erro(_))));

        // /sortear
        let s1 = processa_comando_chat("/sortear pizza | sushi | burger");
        assert!(
            matches!(s1, Some(ComandoChatResultado::MensagemTexto(txt)) if txt.starts_with("🎯 Sorteei entre 3: "))
        );
        let s_pouco = processa_comando_chat("/sortear apenas_um");
        assert!(matches!(s_pouco, Some(ComandoChatResultado::Erro(_))));

        // /enquete
        let e1 =
            processa_comando_chat("/enquete Qual o jogo hoje? | Valheim | Minecraft | Terraria");
        assert!(
            matches!(e1, Some(ComandoChatResultado::CriarEnquete(enq)) if enq.pergunta == "Qual o jogo hoje?" && enq.opcoes.len() == 3)
        );
        let e_invalida = processa_comando_chat("/enquete So a pergunta sem opcoes");
        assert!(matches!(e_invalida, Some(ComandoChatResultado::Erro(_))));

        // Não é comando
        assert_eq!(processa_comando_chat("olá mundo"), None);
    }

    #[test]
    fn notificacoes_chat_formatacao() {
        // Menção com texto longo
        let (t1, c1) = prepara_notificacao_chat(
            TipoNotificacaoChat::Mencao,
            "geral",
            "Alice",
            "Olá @Bob, dá uma olhada nisto aqui por favor!",
            false,
            false,
            false,
        );
        assert_eq!(t1, "Alice mencionou você");
        assert_eq!(c1, "Olá @Bob, dá uma olhada nisto aqui por favor!");

        // Sussurro com anexo de imagem
        let (t2, c2) = prepara_notificacao_chat(
            TipoNotificacaoChat::Sussurro,
            "geral",
            "Charlie",
            "",
            true,
            false,
            false,
        );
        assert_eq!(t2, "Charlie sussurrou para você");
        assert_eq!(c2, "Imagem");

        // Mensagem de sala com GIF
        let (t3, c3) = prepara_notificacao_chat(
            TipoNotificacaoChat::Mensagem,
            "devs",
            "Dave",
            "",
            false,
            true,
            false,
        );
        assert_eq!(t3, "Dave em #devs");
        assert_eq!(c3, "GIF");

        // Mensagem de sala com Enquete
        let (t4, c4) = prepara_notificacao_chat(
            TipoNotificacaoChat::Mensagem,
            "jogos",
            "Eve",
            "",
            false,
            false,
            true,
        );
        assert_eq!(t4, "Eve em #jogos");
        assert_eq!(c4, "Enquete");
    }
}
