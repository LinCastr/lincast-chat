//! Catálogo de cosméticos, no formato da Loja do Discord (sem preço: tudo é
//! de graça). Grade no meio, filtros à direita (tipo, cor, temas, coleção),
//! busca e ordenação no topo. Cada cartão mostra o item já aplicado no SEU
//! perfil; com o mouse em cima, animado. Clicar abre o detalhe com "Usar".

use std::collections::HashSet;

use iced::widget::{
    Space, button, checkbox, column, container, image, mouse_area, pick_list, row, scrollable,
    space, stack, svg, text, text_input,
};
use iced::{Alignment, Background, Border, Color, ContentFit, Element, Length, Padding, Size};

use crate::cosmeticos::{self, Escolha, Item, Tipo};
use crate::ui::efeitos::ico;
use crate::{Msg, fraco, icones, negrito, tema};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Ordem {
    #[default]
    Recentes,
    Nome,
    Colecao,
}

impl Ordem {
    const TODAS: [Ordem; 3] = [Ordem::Recentes, Ordem::Nome, Ordem::Colecao];
}

impl std::fmt::Display for Ordem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Ordem::Recentes => "Mais novos",
            Ordem::Nome => "Nome (A–Z)",
            Ordem::Colecao => "Coleção",
        })
    }
}

#[derive(Debug, Default)]
pub struct EstadoLoja {
    pub busca: String,
    /// "Exibir apenas": vazio mostra todos os tipos.
    pub tipos: HashSet<Tipo>,
    pub grupos: HashSet<String>,
    pub colecao: Option<String>,
    pub cor: Option<usize>,
    pub ordem: Ordem,
    pub sem_filtros: bool,
    pub hover: Option<String>,
    pub detalhe: Option<String>,
}

impl EstadoLoja {
    pub fn de_tipo(t: Option<Tipo>) -> Self {
        Self {
            tipos: t.into_iter().collect(),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone)]
pub enum LojaMsg {
    Busca(String),
    AlternaTipo(Tipo),
    AlternaGrupo(String),
    Colecao(Option<String>),
    Cor(Option<usize>),
    Ordem(Ordem),
    AlternaFiltros,
    Limpa,
    Hover(Option<String>),
    Detalhe(Option<String>),
    /// Equipa (ou tira, com `None`) o item do tipo.
    Usa(Tipo, Option<String>),
}

/// Quem sou eu, para as prévias.
pub struct Eu<'a> {
    pub uid: &'a str,
    pub nome: &'a str,
    pub usuario: Option<&'a str>,
    pub avatar: Option<&'a str>,
    pub banner: Option<&'a str>,
    pub cor: Color,
    pub escolha: &'a Escolha,
}

pub fn aplica(e: &mut EstadoLoja, m: LojaMsg) {
    match m {
        LojaMsg::Busca(b) => e.busca = b,
        LojaMsg::AlternaTipo(t) => {
            if !e.tipos.remove(&t) {
                e.tipos.insert(t);
            }
        }
        LojaMsg::AlternaGrupo(g) => {
            if !e.grupos.remove(&g) {
                e.grupos.insert(g);
            }
        }
        LojaMsg::Colecao(c) => e.colecao = c,
        LojaMsg::Cor(c) => e.cor = if e.cor == c { None } else { c },
        LojaMsg::Ordem(o) => e.ordem = o,
        LojaMsg::AlternaFiltros => e.sem_filtros = !e.sem_filtros,
        LojaMsg::Limpa => {
            let ordem = e.ordem;
            *e = EstadoLoja {
                ordem,
                ..Default::default()
            };
        }
        LojaMsg::Hover(h) => e.hover = h,
        LojaMsg::Detalhe(d) => e.detalhe = d,
        LojaMsg::Usa(..) => {}
    }
}

fn normaliza(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' => 'a',
            'é' | 'ê' => 'e',
            'í' => 'i',
            'ó' | 'ô' | 'õ' => 'o',
            'ú' => 'u',
            'ç' => 'c',
            c => c,
        })
        .collect()
}

/// Itens que passam pelos filtros, na ordem escolhida.
pub fn filtra<'a>(e: &EstadoLoja, itens: &'a [Item]) -> Vec<&'a Item> {
    let busca = normaliza(e.busca.trim());
    let mut v: Vec<(usize, &Item)> = itens
        .iter()
        .enumerate()
        .filter(|(_, i)| e.tipos.is_empty() || e.tipos.contains(&i.tipo))
        .filter(|(_, i)| e.grupos.is_empty() || e.grupos.contains(&i.grupo))
        .filter(|(_, i)| e.colecao.as_ref().is_none_or(|c| &i.categoria == c))
        .filter(|(_, i)| {
            e.cor.is_none_or(|c| {
                i.previa
                    .as_deref()
                    .and_then(cosmeticos::cor_de)
                    .is_some_and(|x| x == c)
            })
        })
        .filter(|(_, i)| {
            busca.is_empty()
                || normaliza(&i.nome).contains(&busca)
                || normaliza(&i.categoria).contains(&busca)
                || normaliza(&i.descricao).contains(&busca)
        })
        .collect();
    match e.ordem {
        Ordem::Recentes => {}
        Ordem::Nome => v.sort_by_key(|(_, i)| normaliza(&i.nome)),
        Ordem::Colecao => v.sort_by(|a, b| a.1.categoria.cmp(&b.1.categoria).then(a.0.cmp(&b.0))),
    }
    v.into_iter().map(|(_, i)| i).collect()
}

/// Nome curto para o cartão: os nomes do catálogo às vezes são a descrição
/// inteira ("Um anel de lâminas irregulares, semelhantes a garras.").
pub fn nome_curto(n: &str) -> String {
    let n = n.trim().trim_end_matches('.');
    let corte = n
        .find([',', '.', ';'])
        .filter(|&i| i >= 12)
        .unwrap_or(n.len());
    let n = &n[..corte];
    if n.chars().count() > 46 {
        let mut s: String = n.chars().take(44).collect();
        s.push('…');
        s
    } else {
        n.to_string()
    }
}

/// A escolha de agora com `item` no lugar do que havia do mesmo tipo.
pub fn com_item(base: &Escolha, item: &Item) -> Escolha {
    let mut e = base.clone();
    *e.slot_mut(item.tipo) = Some(cosmeticos::Ref {
        id: item.id.clone(),
        arquivo: item.arquivo.clone(),
    });
    e
}

/// Para a prévia de um item: a escolha de agora com ele, sem o que cobriria
/// o que se quer ver (o efeito ativo tapa a moldura; a moldura tapa o efeito).
fn so_o_item(base: &Escolha, item: &Item) -> Escolha {
    let mut e = com_item(base, item);
    match item.tipo {
        Tipo::Efeito => e.moldura = None,
        // O efeito ativo tocando por cima tapa o que se quer ver.
        _ => e.efeito = None,
    }
    e
}

// --- Prévias ------------------------------------------------------------------

/// Imagem de um cosmético: animada quando `anda`, senão a miniatura (ou o
/// primeiro quadro enquanto a miniatura não chega).
fn img_item<'a>(item: &Item, anda: bool, fit: ContentFit) -> Option<Element<'a, Msg>> {
    let q = if anda {
        cosmeticos::quadro(&item.arquivo, true).or_else(|| {
            item.previa
                .as_deref()
                .and_then(cosmeticos::miniatura)
                .map(cosmeticos::Quadro::from)
        })
    } else {
        item.previa
            .as_deref()
            .and_then(cosmeticos::miniatura)
            .map(cosmeticos::Quadro::from)
            .or_else(|| cosmeticos::quadro(&item.arquivo, false))
    }?;
    Some(img_q(q, Length::Fill, Length::Fill, fit))
}

/// Um quadro de animação; na transição, o próximo entra por cima aos poucos
/// (os arquivos são de 12 fps: assim a animação não parece travar).
pub fn img_q<'a>(q: cosmeticos::Quadro, w: Length, h: Length, fit: ContentFit) -> Element<'a, Msg> {
    img_q_opaca(q, w, h, fit, 1.0)
}

/// [`img_q`] com a opacidade de tudo multiplicada por `op`.
fn img_q_opaca<'a>(
    q: cosmeticos::Quadro,
    w: Length,
    h: Length,
    fit: ContentFit,
    op: f32,
) -> Element<'a, Msg> {
    match q.proximo {
        None => image(q.atual)
            .width(w)
            .height(h)
            .content_fit(fit)
            .opacity(op)
            .into(),
        Some((prox, f)) => stack![
            image(q.atual)
                .width(w)
                .height(h)
                .content_fit(fit)
                .opacity(op * (1.0 - f)),
            image(prox)
                .width(w)
                .height(h)
                .content_fit(fit)
                .opacity(op * f),
        ]
        .into(),
    }
}

/// Linha de membro com a placa ao fundo (como na lista de membros).
pub fn linha_com_placa<'a>(
    eu: &Eu<'_>,
    escolha: &Escolha,
    placa: Option<Element<'a, Msg>>,
    largura: Length,
) -> Element<'a, Msg> {
    let c = tema::cores();
    let deco = escolha.decoracao.as_ref().map(|d| d.arquivo.clone());
    let conteudo = row![
        crate::ui::avatar::avatar_com(
            eu.avatar,
            eu.uid,
            eu.nome,
            32.0,
            Some("online"),
            c.lateral,
            deco.as_deref(),
            false
        ),
        crate::ui::nome::nome(
            eu.nome,
            escolha.nome.as_ref(),
            15.0,
            c.texto,
            crate::negrito()
        ),
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    let fundo: Element<'a, Msg> = match placa {
        Some(p) => container(p)
            .width(Length::Fill)
            .height(Length::Fill)
            .clip(true)
            .into(),
        None => space().into(),
    };
    // A linha é a base do `stack` (ela dá o tamanho); a placa vai por baixo.
    container(
        stack![
            container(conteudo)
                .padding([0, 10])
                .width(Length::Fill)
                .height(Length::Fill)
                .align_y(Alignment::Center)
        ]
        .push_under(fundo),
    )
    .width(largura)
    .height(46)
    .clip(true)
    .style(move |_| container::Style {
        background: Some(Background::Color(c.lateral)),
        border: Border {
            radius: tema::RAIO.into(),
            ..Default::default()
        },
        ..Default::default()
    })
    .into()
}

/// A moldura por cima de um cartão: faixa no alto, na largura do cartão
/// ("topo"), ou esticada em volta dele inteiro ("inteira").
pub fn camada_moldura<'a>(m: &cosmeticos::Ref) -> Option<Element<'a, Msg>> {
    let h = cosmeticos::quadro(&m.arquivo, false)?.atual;
    let (w, alt) = cosmeticos::tamanho(&m.arquivo)?;
    let inteira = cosmeticos::item(&m.id).and_then(|i| i.ancora).as_deref() == Some("inteira");
    Some(crate::ui::transborda::moldura_sobre(
        image(h)
            .width(Length::Fill)
            .height(Length::Fill)
            .content_fit(ContentFit::Fill),
        inteira,
        alt as f32 / w.max(1) as f32,
    ))
}

/// O cartão com a moldura por cima, presa na borda de cima e passando um
/// pouco para fora (por isso fora do recorte do cartão). Sem moldura, o
/// cartão como veio.
pub fn com_moldura<'a>(cartao: Element<'a, Msg>, escolha: &Escolha) -> Element<'a, Msg> {
    match escolha.moldura.as_ref().and_then(camada_moldura) {
        Some(m) => stack![cartao, m].into(),
        None => cartao,
    }
}

/// Fundo do cartão de perfil: o tema (degradê) se houver, senão a cor lateral.
pub fn fundo_do_cartao(escolha: &Escolha) -> Background {
    let c = tema::cores();
    match escolha.tema.as_ref().and_then(|t| {
        Some((
            crate::ui::nome::cor_hex(&t.cor1)?,
            crate::ui::nome::cor_hex(&t.cor2)?,
        ))
    }) {
        Some((a, b)) => Background::Gradient(iced::Gradient::Linear(
            iced::gradient::Linear::new(std::f32::consts::PI)
                .add_stop(0.0, a)
                .add_stop(1.0, b),
        )),
        None => Background::Color(c.lateral),
    }
}

/// Mini cartão de perfil (efeito e moldura): faixa, foto, nome. `anda` toca o
/// efeito em laço (prévia); a moldura é parada.
pub fn mini_cartao<'a>(
    eu: &Eu<'_>,
    escolha: &Escolha,
    largura: f32,
    altura: f32,
    anda: bool,
) -> Element<'a, Msg> {
    let c = tema::cores();
    let faixa_alt = altura * 0.28;
    let foto = (largura * 0.3).min(96.0);
    let banner: Element<'a, Msg> = match eu.banner.and_then(crate::avatar::handle_exibicao) {
        Some(h) => image(h)
            .width(Length::Fill)
            .height(faixa_alt)
            .content_fit(ContentFit::Cover)
            .into(),
        None => {
            let cor = eu.cor;
            container(space())
                .width(Length::Fill)
                .height(faixa_alt)
                .style(move |_| container::Style {
                    background: Some(Background::Color(cor)),
                    ..Default::default()
                })
                .into()
        }
    };
    let deco = escolha.decoracao.as_ref().map(|d| d.arquivo.clone());
    let corpo = column![
        stack![
            column![banner, space().height(foto * 0.55)],
            container(crate::ui::avatar::avatar_com(
                eu.avatar,
                eu.uid,
                eu.nome,
                foto,
                Some("online"),
                c.lateral,
                deco.as_deref(),
                anda
            ))
            .padding(Padding {
                top: faixa_alt - foto * 0.45,
                left: 12.0,
                ..Default::default()
            }),
        ],
        container(crate::ui::nome::nome(
            eu.nome,
            escolha.nome.as_ref(),
            16.0,
            c.texto,
            crate::negrito()
        ))
        .padding([4, 12]),
        container(
            text(eu.usuario.unwrap_or("").to_string())
                .size(12)
                .style(fraco)
        )
        .padding([0, 12]),
    ];
    let mut camadas: Vec<Element<'a, Msg>> = vec![corpo.into()];
    if let Some(ef) = escolha.efeito.as_ref() {
        let q = if anda {
            cosmeticos::quadro(&ef.arquivo, true)
        } else {
            cosmeticos::item(&ef.id)
                .and_then(|i| i.previa)
                .and_then(|p| cosmeticos::miniatura(&p))
                .map(cosmeticos::Quadro::from)
                .or_else(|| cosmeticos::quadro(&ef.arquivo, false))
        };
        if let Some(q) = q {
            camadas.push(img_q(q, Length::Fill, Length::Fill, ContentFit::Cover));
        }
    }
    let fundo = fundo_do_cartao(escolha);
    let cartao = container(stack(camadas))
        .width(largura)
        .height(altura)
        .clip(true)
        .style(move |_| container::Style {
            background: Some(fundo),
            border: Border {
                color: c.borda,
                width: 1.0,
                radius: tema::RAIO_GRANDE.into(),
            },
            ..Default::default()
        })
        .into();
    com_moldura(cartao, escolha)
}

/// O item aplicado no meu perfil, do jeito que ele aparece de verdade.
fn previa_do_item<'a>(eu: &Eu<'_>, item: &Item, anda: bool, lado: f32) -> Element<'a, Msg> {
    let c = tema::cores();
    let escolha = so_o_item(eu.escolha, item);
    match item.tipo {
        Tipo::Decoracao => {
            let foto = lado * 0.52;
            let deco: Element<'a, Msg> = match img_item(item, anda, ContentFit::Contain) {
                Some(i) => crate::ui::transborda::transborda(
                    i,
                    Size::new(foto, foto),
                    crate::ui::avatar::ESCALA_DECORACAO,
                ),
                None => space().into(),
            };
            container(
                stack![
                    crate::ui::avatar::avatar_com(
                        eu.avatar, eu.uid, eu.nome, foto, None, c.lateral, None, false
                    ),
                    deco
                ]
                .width(foto)
                .height(foto),
            )
            .center(Length::Fill)
            .into()
        }
        Tipo::Placa => container(linha_com_placa(
            eu,
            &escolha,
            img_item(item, anda, ContentFit::Cover),
            Length::Fill,
        ))
        .padding([0, 10])
        .center_y(Length::Fill)
        .into(),
        // A moldura fica acima do cartão: cartão menor e mais baixo, com
        // espaço em cima para ela.
        Tipo::Moldura => container(
            column![
                space().height(lado * 0.12),
                mini_cartao(eu, &escolha, lado * 0.52, lado * 0.62, anda)
            ]
            .align_x(Alignment::Center),
        )
        .center_x(Length::Fill)
        .into(),
        Tipo::Efeito => container(mini_cartao(eu, &escolha, lado * 0.62, lado * 0.92, anda))
            .center(Length::Fill)
            .into(),
    }
}

// --- Tela ---------------------------------------------------------------------

fn cartao<'a>(
    eu: &Eu<'_>,
    item: &Item,
    largura: f32,
    hover: bool,
    em_uso: bool,
) -> Element<'a, Msg> {
    let c = tema::cores();
    let lado = largura - 24.0;
    let selo: Element<'a, Msg> = if em_uso {
        container(
            text("EM USO")
                .size(10)
                .font(negrito())
                .color(c.sobre_destaque),
        )
        .padding([2, 6])
        .style(move |_| container::Style {
            background: Some(Background::Color(c.destaque)),
            border: Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
    } else {
        space().into()
    };
    let topo = container(stack![
        previa_do_item(eu, item, hover, lado),
        container(selo).padding(8),
    ])
    .width(Length::Fill)
    .height(lado * 0.82)
    .clip(true);
    let corpo = column![
        topo,
        column![
            text(nome_curto(&item.nome)).size(14).color(c.texto),
            text(item.categoria.clone()).size(12).style(fraco),
        ]
        .spacing(2)
        .padding(Padding {
            top: 4.0,
            left: 14.0,
            right: 14.0,
            bottom: 14.0,
        }),
    ];
    let id = item.id.clone();
    let b = button(corpo)
        .padding(0)
        .width(largura)
        .on_press(Msg::Loja(LojaMsg::Detalhe(Some(id.clone()))))
        .style(move |_, st| {
            let sobre = matches!(st, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: Some(Background::Color(if sobre { c.elevado } else { c.lateral })),
                text_color: c.texto,
                border: Border {
                    color: if em_uso {
                        c.destaque
                    } else if sobre {
                        tema::mistura(c.borda, c.texto, 0.25)
                    } else {
                        c.borda
                    },
                    width: 1.0,
                    radius: tema::RAIO_GRANDE.into(),
                },
                ..Default::default()
            }
        });
    mouse_area(b)
        .on_enter(Msg::Loja(LojaMsg::Hover(Some(id))))
        .on_exit(Msg::Loja(LojaMsg::Hover(None)))
        .into()
}

fn rotulo_secao<'a>(t: &'a str) -> Element<'a, Msg> {
    text(t).size(14).font(negrito()).into()
}

fn chip<'a>(rotulo: String, ativo: bool, msg: Msg) -> Element<'a, Msg> {
    let c = tema::cores();
    button(text(rotulo).size(13))
        .padding([6, 10])
        .on_press(msg)
        .style(move |_, st| {
            let sobre = matches!(st, button::Status::Hovered);
            button::Style {
                background: Some(Background::Color(if ativo {
                    tema::mistura(c.elevado, c.destaque, 0.35)
                } else if sobre {
                    c.forte
                } else {
                    c.elevado
                })),
                text_color: c.texto,
                border: Border {
                    color: if ativo {
                        c.destaque
                    } else {
                        Color::TRANSPARENT
                    },
                    width: 1.0,
                    radius: tema::RAIO.into(),
                },
                ..Default::default()
            }
        })
        .into()
}

fn filtros<'a>(e: &EstadoLoja, itens: &[Item]) -> Element<'a, Msg> {
    let c = tema::cores();
    let mut tipos = column![rotulo_secao("Exibir apenas")].spacing(10);
    for t in Tipo::TODOS {
        tipos = tipos.push(
            checkbox(e.tipos.contains(&t))
                .label(t.rotulo())
                .size(18)
                .text_size(14)
                .on_toggle(move |_| Msg::Loja(LojaMsg::AlternaTipo(t))),
        );
    }
    let mut cores = row![].spacing(8);
    let mut cores2 = row![].spacing(8);
    for (i, (r, g, b)) in cosmeticos::CORES_FILTRO.iter().enumerate() {
        let cor = Color::from_rgb8(*r, *g, *b);
        let ativo = e.cor == Some(i);
        let bola = button(space().width(30).height(30))
            .padding(0)
            .on_press(Msg::Loja(LojaMsg::Cor(Some(i))))
            .style(move |_, st| button::Style {
                background: Some(Background::Color(cor)),
                border: Border {
                    color: if ativo {
                        c.texto
                    } else if matches!(st, button::Status::Hovered) {
                        c.texto_fraco
                    } else {
                        Color::TRANSPARENT
                    },
                    width: 2.0,
                    radius: 15.0.into(),
                },
                ..Default::default()
            });
        if i < 5 {
            cores = cores.push(bola);
        } else {
            cores2 = cores2.push(bola);
        }
    }
    let mut temas = iced::widget::Row::new().spacing(6);
    for (id, rotulo) in cosmeticos::GRUPOS {
        temas = temas.push(chip(
            rotulo.to_string(),
            e.grupos.contains(id),
            Msg::Loja(LojaMsg::AlternaGrupo(id.to_string())),
        ));
    }
    // Coleções que existem no que os outros filtros deixam passar.
    let mut colecoes: Vec<&str> = itens
        .iter()
        .filter(|i| e.tipos.is_empty() || e.tipos.contains(&i.tipo))
        .filter(|i| e.grupos.is_empty() || e.grupos.contains(&i.grupo))
        .map(|i| i.categoria.as_str())
        .collect();
    colecoes.sort_unstable();
    colecoes.dedup();
    let mut lista_col = iced::widget::Row::new().spacing(6);
    for col in colecoes {
        let ativo = e.colecao.as_deref() == Some(col);
        lista_col = lista_col.push(chip(
            col.to_string(),
            ativo,
            Msg::Loja(LojaMsg::Colecao(if ativo {
                None
            } else {
                Some(col.to_string())
            })),
        ));
    }
    let limpar = button(text("Limpar todos os filtros").size(14).center())
        .width(Length::Fill)
        .padding([10, 12])
        .style(tema::botao_secundario)
        .on_press(Msg::Loja(LojaMsg::Limpa));
    container(
        scrollable(
            column![
                tipos,
                column![rotulo_secao("Cor"), cores, cores2].spacing(10),
                column![rotulo_secao("Temas"), temas.wrap().vertical_spacing(6)].spacing(10),
                column![
                    rotulo_secao("Coleções"),
                    lista_col.wrap().vertical_spacing(6)
                ]
                .spacing(10),
                limpar,
            ]
            .spacing(22)
            .padding(Padding {
                top: 18.0,
                left: 16.0,
                right: 16.0,
                bottom: 18.0,
            }),
        )
        .style(tema::rolagem),
    )
    .width(270)
    .height(Length::Fill)
    .style(move |_| container::Style {
        background: Some(Background::Color(c.lateral)),
        border: Border {
            color: c.borda,
            width: 1.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    })
    .into()
}

fn detalhe<'a>(eu: &Eu<'_>, item: &Item) -> Element<'a, Msg> {
    let c = tema::cores();
    let escolha = so_o_item(eu.escolha, item);
    let em_uso = eu.escolha.slot(item.tipo).is_some_and(|r| r.id == item.id);
    let placa = escolha
        .placa
        .as_ref()
        .and_then(|p| cosmeticos::quadro(&p.arquivo, true))
        .map(|q| img_q(q, Length::Fill, Length::Fill, ContentFit::Cover));
    let grande: Element<'a, Msg> = column![
        mini_cartao(eu, &escolha, 340.0, 300.0, true),
        crate::ui::estilo_nome::linha_de_mensagem(eu, &escolha),
        linha_com_placa(eu, &escolha, placa, Length::Fill),
    ]
    .spacing(12)
    .width(340)
    .into();
    let tipo = item.tipo;
    let acao: Element<'a, Msg> = if em_uso {
        button(text("Remover do perfil").size(14).center())
            .width(Length::Fill)
            .padding([10, 14])
            .style(tema::botao_secundario)
            .on_press(Msg::Loja(LojaMsg::Usa(tipo, None)))
            .into()
    } else {
        button(text("Usar no perfil").size(14).center())
            .width(Length::Fill)
            .padding([10, 14])
            .style(tema::botao_primario)
            .on_press(Msg::Loja(LojaMsg::Usa(tipo, Some(item.id.clone()))))
            .into()
    };
    let lado = column![
        text(tipo.singular().to_uppercase())
            .size(11)
            .font(negrito())
            .style(fraco),
        text(nome_curto(&item.nome)).size(22).font(negrito()),
        text(item.categoria.clone()).size(13).color(c.destaque),
        text(if item.descricao.is_empty() {
            item.nome.clone()
        } else {
            item.descricao.clone()
        })
        .size(13)
        .style(fraco),
        space().height(Length::Fill),
        text("Grátis para todo mundo no Lincast.")
            .size(12)
            .style(fraco),
        acao,
        button(text("Fechar").size(14).center())
            .width(Length::Fill)
            .padding([10, 14])
            .style(tema::botao_transparente)
            .on_press(Msg::Loja(LojaMsg::Detalhe(None))),
    ]
    .spacing(8)
    .width(300)
    .height(Length::Fixed(440.0));
    let cartao = container(row![grande, lado].spacing(28).align_y(Alignment::Center))
        .padding(26)
        .style(move |_| container::Style {
            background: Some(Background::Color(c.fundo)),
            text_color: Some(c.texto),
            border: Border {
                color: c.borda,
                width: 1.0,
                radius: (tema::RAIO_GRANDE + 4.0).into(),
            },
            shadow: iced::Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.5),
                offset: iced::Vector::new(0.0, 8.0),
                blur_radius: 30.0,
            },
            ..Default::default()
        });
    let fundo = mouse_area(
        container(Space::new())
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.55))),
                ..Default::default()
            }),
    )
    .on_press(Msg::Loja(LojaMsg::Detalhe(None)));
    stack![
        fundo,
        container(mouse_area(cartao).on_press(Msg::Nenhum)).center(Length::Fill)
    ]
    .into()
}

pub fn tela<'a>(e: &EstadoLoja, eu: &Eu<'_>, janela: Size) -> Element<'a, Msg> {
    let c = tema::cores();
    let todos = cosmeticos::catalogo();
    let lista = filtra(e, &todos);
    let largura = (janela.width - 48.0).clamp(640.0, 1500.0);
    let altura = (janela.height - 48.0).clamp(420.0, 1000.0);
    let largura_grade = largura - if e.sem_filtros { 0.0 } else { 270.0 } - 48.0;
    let colunas = ((largura_grade + 18.0) / 230.0).floor().max(1.0) as usize;
    let largura_cartao = ((largura_grade - 18.0 * (colunas as f32 - 1.0)) / colunas as f32).floor();

    let topo = row![
        ico(icones::brilho(), 20.0, c.destaque),
        text("Cosméticos").size(18).font(negrito()),
        text("Tudo grátis").size(12).color(c.sucesso),
        space().width(Length::Fill),
        container(
            text_input("Buscar no catálogo", &e.busca)
                .on_input(|s| Msg::Loja(LojaMsg::Busca(s)))
                .padding([8, 12])
                .size(14)
                .style(tema::campo_busca)
        )
        .width(280),
        button(
            svg(icones::fechar())
                .width(18)
                .height(18)
                .style(tema::icone_cor(c.texto_fraco))
        )
        .padding(6)
        .style(tema::botao_transparente)
        .on_press(Msg::FechaLoja),
    ]
    .spacing(12)
    .align_y(Alignment::Center)
    .padding([12, 20]);

    let titulo_tipo = match e.tipos.iter().collect::<Vec<_>>().as_slice() {
        [t] => t.rotulo().to_string(),
        _ => "Todos os itens".to_string(),
    };
    let cabecalho = row![
        text(format!("{titulo_tipo} ({})", lista.len()))
            .size(20)
            .font(negrito()),
        space().width(Length::Fill),
        text("Ordenar por").size(14).font(negrito()),
        pick_list(&Ordem::TODAS[..], Some(e.ordem), |o| Msg::Loja(
            LojaMsg::Ordem(o)
        ))
        .padding([8, 12])
        .text_size(14)
        .style(tema::lista_suspensa)
        .menu_style(tema::menu_suspenso),
        button(
            row![
                text(if e.sem_filtros {
                    "Mostrar filtros"
                } else {
                    "Ocultar filtros"
                })
                .size(14),
                ico(icones::ajustes(), 16.0, c.texto),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
        )
        .padding([8, 12])
        .style(tema::botao_secundario)
        .on_press(Msg::Loja(LojaMsg::AlternaFiltros)),
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    let mut grade = column![].spacing(18);
    if todos.is_empty() {
        grade = grade.push(
            container(text("Carregando o catálogo…").size(14).style(fraco))
                .padding(40)
                .center_x(Length::Fill),
        );
    } else if lista.is_empty() {
        grade = grade.push(
            container(text("Nada com esses filtros.").size(14).style(fraco))
                .padding(40)
                .center_x(Length::Fill),
        );
    }
    for linha in lista.chunks(colunas) {
        let mut r = row![].spacing(18);
        for item in linha {
            let hover = e.hover.as_deref() == Some(item.id.as_str());
            let em_uso = eu.escolha.slot(item.tipo).is_some_and(|x| x.id == item.id);
            r = r.push(cartao(eu, item, largura_cartao, hover, em_uso));
        }
        grade = grade.push(r);
    }
    let centro = container(
        scrollable(column![cabecalho, grade].spacing(18).padding(24))
            .height(Length::Fill)
            .style(tema::rolagem),
    )
    .width(Length::Fill)
    .height(Length::Fill);
    let mut meio = row![centro];
    if !e.sem_filtros {
        meio = meio.push(filtros(e, &todos));
    }
    let separador = container(space().height(1))
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(c.destaque)),
            ..Default::default()
        });
    let janela_el = container(column![topo, separador, meio.height(Length::Fill)])
        .width(largura)
        .height(altura)
        .clip(true)
        .style(move |_| container::Style {
            background: Some(Background::Color(c.fundo)),
            text_color: Some(c.texto),
            border: Border {
                color: c.borda,
                width: 1.0,
                radius: (tema::RAIO_GRANDE + 4.0).into(),
            },
            ..Default::default()
        });
    let base = crate::ui::efeitos::janela_fechavel("loja", janela_el.into(), 0.7, Msg::FechaLoja);
    match e
        .detalhe
        .as_deref()
        .and_then(|id| todos.iter().find(|i| i.id == id))
    {
        Some(item) => stack![base, detalhe(eu, item)].into(),
        None => base,
    }
}

/// Seção "Personalização" do Editar perfil: um ladrilho por tipo (abre o
/// catálogo já filtrado), o estilo do nome e o tema do perfil.
pub fn secao_editor<'a>(uid: &str, nome: &str) -> Element<'a, Msg> {
    let c = tema::cores();
    let minha = cosmeticos::de(uid);
    let ladrilho = |rotulo: &'static str, miolo: Element<'a, Msg>, msg: Msg| -> Element<'a, Msg> {
        button(
            column![
                container(miolo).center(Length::Fill).height(84),
                text(rotulo)
                    .size(12)
                    .style(fraco)
                    .center()
                    .width(Length::Fill),
            ]
            .spacing(6),
        )
        .width(128)
        .padding(8)
        .on_press(msg)
        .style(move |_, st| {
            let sobre = matches!(st, button::Status::Hovered);
            button::Style {
                background: Some(Background::Color(if sobre { c.elevado } else { c.lateral })),
                text_color: c.texto,
                border: Border {
                    color: if sobre { c.destaque } else { c.borda },
                    width: 1.0,
                    radius: tema::RAIO_GRANDE.into(),
                },
                ..Default::default()
            }
        })
        .into()
    };
    let miolo_de = |t: Tipo| -> Element<'a, Msg> {
        match minha
            .slot(t)
            .and_then(|r| cosmeticos::item(&r.id))
            .and_then(|i| i.previa.as_deref().and_then(cosmeticos::miniatura))
        {
            Some(h) => image(h)
                .height(Length::Fill)
                .content_fit(ContentFit::Contain)
                .into(),
            None => ico(icones::mais_circulo(), 28.0, c.texto_fraco),
        }
    };
    let mut linha = row![].spacing(10);
    for t in Tipo::TODOS {
        let rotulo = match t {
            Tipo::Decoracao => "Decoração",
            Tipo::Placa => "Placa",
            Tipo::Efeito => "Efeito de perfil",
            Tipo::Moldura => "Moldura",
        };
        linha = linha.push(ladrilho(rotulo, miolo_de(t), Msg::AbreLoja(Some(t))));
    }
    linha = linha.push(ladrilho(
        "Estilo do nome",
        crate::ui::nome::nome(nome, minha.nome.as_ref(), 18.0, c.texto, crate::negrito()),
        Msg::AbreEstiloNome,
    ));
    column![
        row![
            text("Personalização").size(16).font(negrito()),
            text("tudo grátis").size(12).color(c.sucesso),
            space().width(Length::Fill),
            button(
                row![
                    ico(icones::brilho(), 14.0, c.texto),
                    text("Ver catálogo").size(13)
                ]
                .spacing(6)
                .align_y(Alignment::Center)
            )
            .padding([6, 12])
            .style(tema::botao_secundario)
            .on_press(Msg::AbreLoja(None)),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        linha.wrap().vertical_spacing(10),
    ]
    .spacing(12)
    .into()
}

/// Põe o efeito de perfil (toca uma vez ao abrir; de novo com o mouse em
/// cima) e a moldura por cima do conteúdo de um cartão de perfil.
pub fn com_efeito<'a>(conteudo: Element<'a, Msg>, escolha: &Escolha) -> Element<'a, Msg> {
    let mut camadas: Vec<Element<'a, Msg>> = vec![conteudo];
    if let Some(ef) = escolha.efeito.as_ref()
        && let Some(q) = cosmeticos::quadro_uma_vez(&ef.arquivo)
    {
        camadas.push(img_q(q, Length::Fill, Length::Fill, ContentFit::Cover));
    }
    if camadas.len() == 1 {
        return camadas.pop().expect("um");
    }
    stack(camadas).into()
}

/// Linha (de membro, de conversa) com a placa de identificação da pessoa ao
/// fundo, como no Discord. A placa anda com o mouse em cima (`anda`). Sem
/// placa, devolve a linha como veio.
/// Placa ao fundo de uma linha. `hover`: desde quando o mouse está em cima
/// (a animação começa do primeiro quadro). `offline`: placa apagada, como no
/// Discord, até passar o mouse.
pub fn com_placa<'a>(
    linha: Element<'a, Msg>,
    uid: &str,
    hover: Option<std::time::Instant>,
    offline: bool,
    fundo: Color,
) -> Element<'a, Msg> {
    let Some(p) = cosmeticos::de(uid).placa.clone() else {
        return linha;
    };
    let Some(q) = cosmeticos::quadro_desde(&p.arquivo, hover) else {
        return linha;
    };
    let op = if offline && hover.is_none() { 0.3 } else { 1.0 };
    // Véu da cor do painel à esquerda: o nome continua legível.
    let veu = container(space())
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| container::Style {
            background: Some(Background::Gradient(iced::Gradient::Linear(
                iced::gradient::Linear::new(std::f32::consts::FRAC_PI_2)
                    .add_stop(0.0, Color { a: 0.75, ..fundo })
                    .add_stop(0.55, Color { a: 0.25, ..fundo })
                    .add_stop(1.0, Color { a: 0.0, ..fundo }),
            ))),
            ..Default::default()
        });
    // A linha é a base do `stack` (ela dá o tamanho); placa e véu por baixo.
    container(stack![linha].push_under(veu).push_under(img_q_opaca(
        q,
        Length::Fill,
        Length::Fill,
        ContentFit::Cover,
        op,
    )))
    .clip(true)
    .style(|_| container::Style {
        border: Border {
            radius: tema::RAIO.into(),
            ..Default::default()
        },
        ..Default::default()
    })
    .into()
}

#[cfg(test)]
mod testes {
    use super::*;

    fn item(id: &str, tipo: Tipo, nome: &str, cat: &str, grupo: &str) -> Item {
        Item {
            id: id.into(),
            tipo,
            nome: nome.into(),
            categoria: cat.into(),
            grupo: grupo.into(),
            arquivo: format!("/cosmeticos/{id}.webp"),
            previa: None,
            ancora: None,
            descricao: String::new(),
        }
    }

    #[test]
    fn filtra_por_tipo_grupo_busca_e_ordena() {
        let itens = vec![
            item("a", Tipo::Decoracao, "Zebra", "Jujutsu Kaisen", "anime"),
            item("b", Tipo::Placa, "Árvore", "Arcane", "filmes"),
            item("c", Tipo::Decoracao, "Abóbora", "Zodíaco", "originais"),
        ];
        let mut e = EstadoLoja::de_tipo(Some(Tipo::Decoracao));
        assert_eq!(filtra(&e, &itens).len(), 2);
        e.ordem = Ordem::Nome;
        assert_eq!(filtra(&e, &itens)[0].id, "c");
        e.grupos.insert("anime".into());
        assert_eq!(filtra(&e, &itens)[0].id, "a");
        let mut e = EstadoLoja {
            busca: "arvore".into(),
            ..Default::default()
        };
        assert_eq!(filtra(&e, &itens)[0].id, "b");
        e.busca = "kaisen".into();
        assert_eq!(filtra(&e, &itens)[0].id, "a");
    }

    #[test]
    fn nome_curto_corta_descricao() {
        assert_eq!(
            nome_curto("Um anel de lâminas irregulares, semelhantes a garras."),
            "Um anel de lâminas irregulares"
        );
        assert_eq!(nome_curto("Pumpkin King"), "Pumpkin King");
    }
}
