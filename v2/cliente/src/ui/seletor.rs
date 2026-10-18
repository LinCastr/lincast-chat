//! Seletor de GIF e emoji (um só popup com duas abas), ancorado no campo de
//! mensagem: busca, categorias e grade com cantos arredondados.

use std::collections::HashMap;

use iced::widget::{
    Column, Row, button, column, container, image, row, scrollable, space, text, text_input,
};
use iced::{
    Alignment, Background, Border, ContentFit, Element, Length, Shadow, Vector,
    widget::image as img,
};

use crate::api::ItemGif;
use crate::{Msg, fraco, negrito, tema, texto_perigo};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbaSeletor {
    Gif,
    Emoji,
}

pub const LARGURA: f32 = 420.0;
const ALTURA_LISTA: f32 = 330.0;

/// (categoria, [(emoji, palavras de busca)]).
const CATEGORIAS: &[(&str, &[(&str, &str)])] = &[
    (
        "Rostos",
        &[
            ("😀", "feliz sorriso alegre"),
            ("😃", "feliz sorriso"),
            ("😄", "rindo feliz"),
            ("😁", "sorrindo dentes"),
            ("😆", "rindo muito"),
            ("😅", "suor alivio"),
            ("😂", "chorando de rir kkk"),
            ("🤣", "rolando rindo kkk"),
            ("🙂", "sorriso leve"),
            ("🙃", "invertido ironia"),
            ("😉", "piscando"),
            ("😊", "corado feliz"),
            ("😇", "anjo santo"),
            ("🥰", "apaixonado amor"),
            ("😍", "olhos de coracao amei"),
            ("🤩", "deslumbrado estrela"),
            ("😘", "beijo"),
            ("😋", "gostoso delicia"),
            ("😛", "lingua"),
            ("😜", "lingua piscando louco"),
            ("🤪", "doido maluco"),
            ("🤗", "abraco"),
            ("🤔", "pensando duvida hm"),
            ("🤨", "desconfiado sobrancelha"),
            ("😐", "neutro sem graca"),
            ("😑", "sem expressao"),
            ("😶", "mudo sem boca"),
            ("🙄", "revirando olhos"),
            ("😏", "malandro sorrisinho"),
            ("😒", "desdem"),
            ("😬", "tenso constrangido"),
            ("😮‍💨", "suspiro alivio"),
            ("😴", "dormindo sono"),
            ("😪", "sono"),
            ("🤤", "babando"),
            ("😷", "mascara doente"),
            ("🤒", "febre doente"),
            ("🤢", "enjoado"),
            ("🤮", "vomito"),
            ("🥵", "calor"),
            ("🥶", "frio"),
            ("😵", "tonto"),
            ("🤯", "mente explodindo chocado"),
            ("🥳", "festa comemorando"),
            ("😎", "oculos legal"),
            ("🤓", "nerd"),
            ("😕", "confuso"),
            ("😟", "preocupado"),
            ("🙁", "triste"),
            ("😮", "surpreso"),
            ("😲", "chocado"),
            ("😳", "envergonhado vermelho"),
            ("🥺", "implorando fofo"),
            ("😢", "chorando triste"),
            ("😭", "chorando muito"),
            ("😱", "gritando medo"),
            ("😖", "frustrado"),
            ("😞", "decepcionado"),
            ("😤", "bravo bufando"),
            ("😡", "raiva furioso"),
            ("🤬", "xingando"),
            ("😈", "diabinho"),
            ("💀", "caveira morri kkk"),
            ("👻", "fantasma"),
        ],
    ),
    (
        "Gestos",
        &[
            ("👍", "joinha curti ok"),
            ("👎", "nao gostei"),
            ("👌", "perfeito ok"),
            ("✌️", "paz"),
            ("🤞", "dedos cruzados sorte"),
            ("🤟", "te amo"),
            ("🤘", "rock"),
            ("🤙", "liga"),
            ("👈", "esquerda"),
            ("👉", "direita"),
            ("👆", "cima"),
            ("👇", "baixo"),
            ("☝️", "um dedo"),
            ("✋", "mao parar"),
            ("🖐️", "mao aberta"),
            ("👋", "oi tchau acenando"),
            ("👏", "palmas aplauso"),
            ("🙌", "maos pra cima"),
            ("🙏", "obrigado por favor rezar"),
            ("🤝", "aperto de mao acordo"),
            ("💪", "forca musculo"),
            ("🫶", "coracao maos"),
            ("🫡", "continencia"),
            ("🤷", "sei la"),
            ("🤦", "facepalm"),
            ("🙋", "levantando a mao"),
            ("🙈", "macaco nao vejo"),
            ("🙉", "macaco nao ouco"),
        ],
    ),
    (
        "Corações",
        &[
            ("❤️", "coracao amor vermelho"),
            ("🧡", "coracao laranja"),
            ("💛", "coracao amarelo"),
            ("💚", "coracao verde"),
            ("💙", "coracao azul"),
            ("💜", "coracao roxo"),
            ("🖤", "coracao preto"),
            ("🤍", "coracao branco"),
            ("💔", "coracao partido"),
            ("❣️", "exclamacao coracao"),
            ("💕", "dois coracoes"),
            ("💞", "coracoes girando"),
            ("💓", "coracao batendo"),
            ("💗", "coracao crescendo"),
            ("💖", "coracao brilhando"),
            ("💘", "coracao flecha"),
            ("✨", "brilho"),
            ("⭐", "estrela"),
            ("🌟", "estrela brilhante"),
            ("💫", "tontura estrela"),
        ],
    ),
    (
        "Animais",
        &[
            ("🐶", "cachorro cao"),
            ("🐱", "gato"),
            ("🐭", "rato"),
            ("🐹", "hamster"),
            ("🐰", "coelho"),
            ("🦊", "raposa"),
            ("🐻", "urso"),
            ("🐼", "panda"),
            ("🐨", "coala"),
            ("🐯", "tigre"),
            ("🦁", "leao"),
            ("🐮", "vaca"),
            ("🐷", "porco"),
            ("🐸", "sapo"),
            ("🐵", "macaco"),
            ("🐔", "galinha"),
            ("🐧", "pinguim"),
            ("🐦", "passaro"),
            ("🦆", "pato"),
            ("🦉", "coruja"),
            ("🐴", "cavalo"),
            ("🦄", "unicornio"),
            ("🐝", "abelha"),
            ("🦋", "borboleta"),
            ("🐢", "tartaruga"),
            ("🐍", "cobra"),
            ("🐙", "polvo"),
            ("🐬", "golfinho"),
        ],
    ),
    (
        "Comida",
        &[
            ("🍎", "maca"),
            ("🍌", "banana"),
            ("🍇", "uva"),
            ("🍓", "morango"),
            ("🍉", "melancia"),
            ("🍕", "pizza"),
            ("🍔", "hamburguer"),
            ("🍟", "batata frita"),
            ("🌭", "cachorro quente"),
            ("🌮", "taco"),
            ("🍣", "sushi"),
            ("🍜", "lamen macarrao"),
            ("🍩", "rosquinha donut"),
            ("🍪", "biscoito"),
            ("🍫", "chocolate"),
            ("🍿", "pipoca"),
            ("☕", "cafe"),
            ("🍺", "cerveja"),
            ("🍻", "brindar cerveja"),
            ("🥤", "refrigerante copo"),
        ],
    ),
    (
        "Objetos",
        &[
            ("🎮", "controle jogo game"),
            ("🕹️", "joystick"),
            ("🎧", "fone musica"),
            ("🎵", "musica nota"),
            ("🎤", "microfone"),
            ("🎸", "guitarra"),
            ("📱", "celular"),
            ("💻", "notebook computador"),
            ("🖥️", "monitor pc"),
            ("⌨️", "teclado"),
            ("🖱️", "mouse"),
            ("📷", "camera foto"),
            ("🔥", "fogo quente"),
            ("💧", "gota agua"),
            ("⚡", "raio energia"),
            ("🌈", "arco iris"),
            ("🎉", "festa parabens"),
            ("🎂", "bolo aniversario"),
            ("🎁", "presente"),
            ("🏆", "trofeu"),
            ("💯", "cem perfeito"),
            ("🚀", "foguete"),
            ("💰", "dinheiro"),
            ("🧠", "cerebro"),
            ("👀", "olhos olhando"),
            ("🫠", "derretendo"),
            ("🗿", "moai"),
            ("💩", "coco"),
        ],
    ),
    (
        "Símbolos",
        &[
            ("✅", "certo ok verificado"),
            ("❌", "errado nao x"),
            ("❓", "pergunta duvida"),
            ("❗", "exclamacao atencao"),
            ("⚠️", "aviso atencao"),
            ("🚫", "proibido"),
            ("➕", "mais"),
            ("➖", "menos"),
            ("⬆️", "cima"),
            ("⬇️", "baixo"),
            ("⬅️", "esquerda"),
            ("➡️", "direita"),
            ("🔔", "sino notificacao"),
            ("🔕", "sem notificacao"),
            ("🔒", "cadeado trancado"),
            ("🔓", "destrancado"),
        ],
    ),
];

fn aba_botao<'a>(rotulo: &'static str, ativa: bool, aba: AbaSeletor) -> Element<'a, Msg> {
    let c = tema::cores();
    button(text(rotulo).size(13).font(negrito()))
        .padding([6, 14])
        .style(move |_, st| button::Style {
            background: if ativa {
                Some(Background::Color(c.forte))
            } else if matches!(st, button::Status::Hovered) {
                Some(Background::Color(c.elevado))
            } else {
                None
            },
            text_color: if ativa || matches!(st, button::Status::Hovered) {
                c.texto
            } else {
                c.texto_medio
            },
            border: Border {
                radius: 16.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .on_press(Msg::TrocaAbaSeletor(aba))
        .into()
}

fn campo_busca<'a>(
    dica: &'static str,
    valor: &'a str,
    ao_digitar: fn(String) -> Msg,
    enviar: Option<Msg>,
) -> Element<'a, Msg> {
    let mut campo = text_input(dica, valor)
        .on_input(ao_digitar)
        .padding([8, 12])
        .size(13)
        .style(tema::campo_busca);
    if let Some(m) = enviar {
        campo = campo.on_submit(m);
    }
    campo.into()
}

/// Quadro do GIF (prévia arredondada ou, enquanto carrega, um bloco neutro).
fn quadro_gif<'a>(
    item: &'a ItemGif,
    previas: &'a HashMap<String, img::Handle>,
) -> Element<'a, Msg> {
    let c = tema::cores();
    let miolo: Element<'a, Msg> = match previas.get(&item.url) {
        Some(h) => image(h.clone())
            .width(Length::Fill)
            .height(112)
            .content_fit(ContentFit::Cover)
            .border_radius(10.0)
            .into(),
        None => container(
            text(if item.descricao.is_empty() {
                "GIF"
            } else {
                &item.descricao
            })
            .size(11)
            .style(fraco),
        )
        .width(Length::Fill)
        .height(112)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_| container::Style {
            background: Some(Background::Color(c.elevado)),
            border: Border {
                radius: 10.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into(),
    };
    button(miolo)
        .padding(0)
        .width(Length::FillPortion(1))
        .style(move |_, st| button::Style {
            background: None,
            border: Border {
                color: if matches!(st, button::Status::Hovered | button::Status::Pressed) {
                    c.destaque
                } else {
                    iced::Color::TRANSPARENT
                },
                width: 2.0,
                radius: 10.0.into(),
            },
            ..Default::default()
        })
        .on_press(Msg::EnviaGif(item.url.clone()))
        .into()
}

fn lista_gif<'a>(
    buscando: bool,
    erro: Option<&'a str>,
    resultados: &'a [ItemGif],
    previas: &'a HashMap<String, img::Handle>,
) -> Element<'a, Msg> {
    if buscando {
        return aviso("Buscando GIFs…");
    }
    if let Some(e) = erro {
        return container(text(e).size(12).style(texto_perigo))
            .padding(12)
            .height(ALTURA_LISTA)
            .into();
    }
    if resultados.is_empty() {
        return aviso("Nenhum GIF. Digite algo e aperte Enter.");
    }
    let mut col = Column::new().spacing(6);
    for par in resultados.chunks(2) {
        let mut linha = Row::new().spacing(6);
        for item in par {
            linha = linha.push(quadro_gif(item, previas));
        }
        if par.len() == 1 {
            linha = linha.push(space().width(Length::FillPortion(1)));
        }
        col = col.push(linha);
    }
    scrollable(container(col).padding([0, 8]))
        .height(ALTURA_LISTA)
        .style(tema::rolagem)
        .into()
}

fn aviso<'a>(t: &'static str) -> Element<'a, Msg> {
    container(text(t).size(13).style(fraco))
        .width(Length::Fill)
        .height(ALTURA_LISTA)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
}

fn celula_emoji<'a>(e: &'static str) -> Element<'a, Msg> {
    let c = tema::cores();
    button(text(e).size(22).center().font(crate::tema::FONTE_EMOJI))
        .width(40)
        .height(40)
        .padding(0)
        .style(move |_, st| button::Style {
            background: matches!(st, button::Status::Hovered | button::Status::Pressed)
                .then_some(Background::Color(c.forte)),
            border: Border {
                radius: 8.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .on_press(Msg::InsereEmoji(e.to_string()))
        .into()
}

fn lista_emoji<'a>(busca: &str) -> Element<'a, Msg> {
    let termo = busca.trim().to_lowercase();
    let mut col = Column::new().spacing(10);
    let mut achou = false;
    for (cat, itens) in CATEGORIAS {
        let filtrados: Vec<&(&str, &str)> = itens
            .iter()
            .filter(|(e, palavras)| {
                termo.is_empty()
                    || palavras.contains(termo.as_str())
                    || cat.to_lowercase().contains(termo.as_str())
                    || e.contains(termo.as_str())
            })
            .collect();
        if filtrados.is_empty() {
            continue;
        }
        achou = true;
        col = col.push(text(*cat).size(12).font(negrito()).style(fraco));
        for linha in filtrados.chunks(9) {
            let mut r = Row::new().spacing(2);
            for (e, _) in linha.iter().copied() {
                r = r.push(celula_emoji(e));
            }
            col = col.push(r);
        }
    }
    if !achou {
        return aviso("Nenhum emoji com esse nome.");
    }
    scrollable(container(col).padding([0, 8]))
        .height(ALTURA_LISTA)
        .style(tema::rolagem)
        .into()
}

/// O popup completo.
pub fn seletor<'a>(
    aba: AbaSeletor,
    busca_gif: &'a str,
    buscando_gif: bool,
    erro_gif: Option<&'a str>,
    resultados_gif: &'a [ItemGif],
    previas_gif: &'a HashMap<String, img::Handle>,
    busca_emoji: &'a str,
) -> Element<'a, Msg> {
    let c = tema::cores();
    let abas = row![
        aba_botao("GIFs", aba == AbaSeletor::Gif, AbaSeletor::Gif),
        aba_botao("Emoji", aba == AbaSeletor::Emoji, AbaSeletor::Emoji),
    ]
    .spacing(4);
    let (busca, lista): (Element<'a, Msg>, Element<'a, Msg>) = match aba {
        AbaSeletor::Gif => (
            campo_busca(
                "Buscar GIFs no Giphy",
                busca_gif,
                Msg::BuscaGifInput,
                Some(Msg::SubmeteBuscaGif),
            ),
            lista_gif(buscando_gif, erro_gif, resultados_gif, previas_gif),
        ),
        AbaSeletor::Emoji => (
            campo_busca("Buscar emoji", busca_emoji, Msg::BuscaEmojiInput, None),
            lista_emoji(busca_emoji),
        ),
    };
    container(column![abas, busca, lista].spacing(10))
        .padding(12)
        .width(LARGURA)
        .style(move |_| container::Style {
            background: Some(Background::Color(c.lateral)),
            text_color: Some(c.texto),
            border: Border {
                color: c.borda,
                width: 1.0,
                radius: tema::RAIO_GRANDE.into(),
            },
            shadow: Shadow {
                color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.45),
                offset: Vector::new(0.0, 6.0),
                blur_radius: 18.0,
            },
            ..Default::default()
        })
        .into()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn categorias_nao_tem_emoji_repetido() {
        let mut vistos = std::collections::HashSet::new();
        for (_, itens) in CATEGORIAS {
            for (e, _) in *itens {
                assert!(vistos.insert(*e), "emoji repetido: {e}");
            }
        }
    }
}
