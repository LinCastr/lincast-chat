//! Janela "Alterar estilo do nome exibido" (fonte, efeito, cores) e o tema do
//! perfil (degradê do cartão), com a prévia ao vivo à direita: cartão de
//! perfil, mensagem no chat e linha da lista de membros.

use iced::widget::{button, column, container, row, space, text, text_input};
use iced::{Alignment, Background, Border, Color, Element, Length};

use crate::cosmeticos::{Escolha, EstiloNome, TemaPerfil};
use crate::ui::efeitos::ico;
use crate::ui::loja::Eu;
use crate::ui::nome::{self, CORES, EFEITOS, FONTES};
use crate::{Msg, fraco, icones, negrito, tema};

#[derive(Debug, Clone, Default)]
pub struct EdicaoEstilo {
    pub estilo: EstiloNome,
    pub tema: Option<TemaPerfil>,
    pub hex1: String,
    pub hex2: String,
}

impl EdicaoEstilo {
    pub fn de(e: &Escolha) -> Self {
        let estilo = e.nome.clone().unwrap_or_default();
        Self {
            hex1: estilo.cor.clone().unwrap_or_default(),
            hex2: estilo.cor2.clone().unwrap_or_default(),
            estilo,
            tema: e.tema.clone(),
        }
    }

    /// A escolha completa com este estilo aplicado.
    pub fn aplicado(&self, base: &Escolha) -> Escolha {
        let mut e = base.clone();
        e.nome = (!self.estilo.vazio()).then(|| self.estilo.clone());
        e.tema = self.tema.clone();
        e
    }
}

#[derive(Debug, Clone)]
pub enum EstiloMsg {
    Fonte(Option<&'static str>),
    Efeito(&'static str),
    Cor(String),
    Cor2(String),
    Hex1(String),
    Hex2(String),
    Tema(Option<(&'static str, &'static str)>),
    Surpreenda,
    Restaura,
}

/// Degradês prontos para o tema do perfil.
pub const TEMAS: [(&str, &str); 10] = [
    ("#e255c4", "#5b2bd6"),
    ("#1e3a8a", "#0ea5e9"),
    ("#14532d", "#84cc16"),
    ("#7c2d12", "#f97316"),
    ("#4c0519", "#e11d48"),
    ("#0f172a", "#6366f1"),
    ("#164e63", "#2dd4bf"),
    ("#3b0764", "#a855f7"),
    ("#422006", "#eab308"),
    ("#18181b", "#52525b"),
];

fn valido(h: &str) -> Option<String> {
    let h = h.trim();
    let h = if h.starts_with('#') {
        h.to_string()
    } else {
        format!("#{h}")
    };
    (h.len() == 7 && h[1..].chars().all(|c| c.is_ascii_hexdigit())).then(|| h.to_lowercase())
}

pub fn aplica(ed: &mut EdicaoEstilo, m: EstiloMsg) {
    let e = &mut ed.estilo;
    match m {
        EstiloMsg::Fonte(f) => e.fonte = f.map(String::from),
        EstiloMsg::Efeito(x) => e.efeito = Some(x.to_string()),
        EstiloMsg::Cor(c) => {
            ed.hex1 = c.clone();
            e.cor = Some(c);
        }
        EstiloMsg::Cor2(c) => {
            ed.hex2 = c.clone();
            e.cor2 = Some(c);
        }
        EstiloMsg::Hex1(h) => {
            if let Some(v) = valido(&h) {
                e.cor = Some(v);
            }
            ed.hex1 = h;
        }
        EstiloMsg::Hex2(h) => {
            if let Some(v) = valido(&h) {
                e.cor2 = Some(v);
            }
            ed.hex2 = h;
        }
        EstiloMsg::Tema(t) => {
            ed.tema = t.map(|(a, b)| TemaPerfil {
                cor1: a.into(),
                cor2: b.into(),
            });
        }
        EstiloMsg::Surpreenda => {
            let n = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos() as usize ^ d.as_secs() as usize)
                .unwrap_or(7);
            e.fonte = Some(FONTES[n % FONTES.len()].id.to_string());
            e.efeito = Some(EFEITOS[(n / 7) % EFEITOS.len()].0.to_string());
            let c1 = CORES[(n / 13) % CORES.len()];
            let c2 = CORES[(n / 29 + 3) % CORES.len()];
            e.cor = Some(c1.into());
            e.cor2 = Some(c2.into());
            ed.hex1 = c1.into();
            ed.hex2 = c2.into();
        }
        EstiloMsg::Restaura => *ed = EdicaoEstilo::default(),
    }
}

fn titulo<'a>(t: &'a str) -> Element<'a, Msg> {
    text(t).size(15).font(negrito()).into()
}

fn ladrilho<'a>(
    conteudo: Element<'a, Msg>,
    ativo: bool,
    msg: Msg,
    largura: f32,
    altura: f32,
) -> Element<'a, Msg> {
    let c = tema::cores();
    button(container(conteudo).center(Length::Fill))
        .width(largura)
        .height(altura)
        .padding(0)
        .on_press(msg)
        .style(move |_, st| {
            let sobre = matches!(st, button::Status::Hovered);
            button::Style {
                background: Some(Background::Color(if sobre { c.elevado } else { c.lateral })),
                text_color: c.texto,
                border: Border {
                    color: if ativo {
                        c.destaque
                    } else if sobre {
                        tema::mistura(c.borda, c.texto, 0.25)
                    } else {
                        c.borda
                    },
                    width: if ativo { 2.0 } else { 1.0 },
                    radius: tema::RAIO.into(),
                },
                ..Default::default()
            }
        })
        .into()
}

fn bolinhas<'a>(atual: Option<&str>, msg: fn(String) -> EstiloMsg) -> Element<'a, Msg> {
    let c = tema::cores();
    let mut r = row![].spacing(8);
    for hex in CORES {
        let cor = nome::cor_hex(hex).unwrap_or(Color::WHITE);
        let ativo = atual == Some(hex);
        r = r.push(
            button(if ativo {
                container(ico(icones::check(), 16.0, tema::sobre_contraste(cor)))
                    .center(Length::Fill)
            } else {
                container(space()).center(Length::Fill)
            })
            .width(32)
            .height(32)
            .padding(0)
            .on_press(Msg::EstiloNome(msg(hex.to_string())))
            .style(move |_, st| button::Style {
                background: Some(Background::Color(cor)),
                border: Border {
                    color: if ativo || matches!(st, button::Status::Hovered) {
                        c.texto
                    } else {
                        Color::TRANSPARENT
                    },
                    width: 2.0,
                    radius: tema::RAIO.into(),
                },
                ..Default::default()
            }),
        );
    }
    r.wrap().vertical_spacing(8).into()
}

pub fn linha_de_mensagem<'a>(eu: &Eu<'_>, escolha: &Escolha) -> Element<'a, Msg> {
    let c = tema::cores();
    let deco = escolha.decoracao.as_ref().map(|d| d.arquivo.clone());
    container(
        row![
            crate::ui::avatar::avatar_com(
                eu.avatar,
                eu.uid,
                eu.nome,
                40.0,
                None,
                c.fundo,
                deco.as_deref(),
                false
            ),
            column![
                row![
                    nome::nome(
                        eu.nome,
                        escolha.nome.as_ref(),
                        15.0,
                        c.texto,
                        crate::negrito()
                    ),
                    text("Hoje às 09:58").size(11).color(c.texto_fraco),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                text("será que alguém lê isso?").size(14),
            ]
            .spacing(2),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .padding([10, 14])
    .width(Length::Fill)
    .style(move |_| container::Style {
        background: Some(Background::Color(c.fundo)),
        border: Border {
            color: c.borda,
            width: 1.0,
            radius: tema::RAIO.into(),
        },
        ..Default::default()
    })
    .into()
}

pub fn modal<'a>(ed: &EdicaoEstilo, eu: &Eu<'_>) -> Element<'a, Msg> {
    let c = tema::cores();
    let e = &ed.estilo;
    let previa_txt = |f: iced::Font, escala: f32| text("Gg").size(26.0 * escala).font(f);

    let mut fontes = column![].spacing(8);
    let mut linha = row![ladrilho(
        previa_txt(crate::negrito(), 1.0).into(),
        e.fonte.is_none(),
        Msg::EstiloNome(EstiloMsg::Fonte(None)),
        84.0,
        60.0,
    )]
    .spacing(8);
    for (i, f) in FONTES.iter().enumerate() {
        linha = linha.push(ladrilho(
            column![
                previa_txt(f.fonte, f.escala),
                text(f.rotulo).size(10).style(fraco)
            ]
            .align_x(Alignment::Center)
            .into(),
            e.fonte.as_deref() == Some(f.id),
            Msg::EstiloNome(EstiloMsg::Fonte(Some(f.id))),
            84.0,
            60.0,
        ));
        if (i + 2) % 5 == 0 {
            fontes = fontes.push(linha);
            linha = row![].spacing(8);
        }
    }
    fontes = fontes.push(linha);

    let atual_ef = e.efeito.as_deref().unwrap_or("solido");
    let cor_ex = e.cor.as_deref().and_then(nome::cor_hex).unwrap_or(c.texto);
    let mut efeitos = row![].spacing(8);
    for (id, rotulo) in EFEITOS {
        let ex = EstiloNome {
            fonte: e.fonte.clone(),
            efeito: Some(id.into()),
            cor: Some(nome::hex(cor_ex)),
            cor2: e.cor2.clone(),
        };
        efeitos = efeitos.push(ladrilho(
            nome::nome(rotulo, Some(&ex), 14.0, c.texto, crate::negrito()),
            atual_ef == id,
            Msg::EstiloNome(EstiloMsg::Efeito(id)),
            100.0,
            56.0,
        ));
    }
    let campo_hex = |valor: &str, msg: fn(String) -> EstiloMsg| {
        container(
            text_input("#rrggbb", valor)
                .on_input(move |s| Msg::EstiloNome(msg(s)))
                .padding([6, 10])
                .size(13)
                .style(tema::campo),
        )
        .width(110)
    };
    let mut temas = row![ladrilho(
        ico(icones::proibido(), 16.0, c.texto_fraco),
        ed.tema.is_none(),
        Msg::EstiloNome(EstiloMsg::Tema(None)),
        44.0,
        44.0,
    )]
    .spacing(8);
    for (a, b) in TEMAS {
        let (ca, cb) = (
            nome::cor_hex(a).unwrap_or(c.fundo),
            nome::cor_hex(b).unwrap_or(c.fundo),
        );
        let ativo = ed.tema.as_ref().is_some_and(|t| t.cor1 == a && t.cor2 == b);
        temas = temas.push(
            button(space())
                .width(44)
                .height(44)
                .on_press(Msg::EstiloNome(EstiloMsg::Tema(Some((a, b)))))
                .style(move |_, st| button::Style {
                    background: Some(Background::Gradient(iced::Gradient::Linear(
                        iced::gradient::Linear::new(std::f32::consts::PI)
                            .add_stop(0.0, ca)
                            .add_stop(1.0, cb),
                    ))),
                    border: Border {
                        color: if ativo || matches!(st, button::Status::Hovered) {
                            c.texto
                        } else {
                            Color::TRANSPARENT
                        },
                        width: 2.0,
                        radius: tema::RAIO.into(),
                    },
                    ..Default::default()
                }),
        );
    }

    let esquerda = iced::widget::scrollable(
        column![
            text("Alterar estilo do nome exibido")
                .size(20)
                .font(negrito()),
            titulo("Escolha a fonte"),
            fontes,
            titulo("Escolha o efeito"),
            efeitos.wrap().vertical_spacing(8),
            titulo("Escolha a cor"),
            row![
                campo_hex(&ed.hex1, EstiloMsg::Hex1),
                bolinhas(e.cor.as_deref(), EstiloMsg::Cor)
            ]
            .spacing(10)
            .align_y(Alignment::Start),
            text("Segunda cor (degradê, pop)").size(13).style(fraco),
            row![
                campo_hex(&ed.hex2, EstiloMsg::Hex2),
                bolinhas(e.cor2.as_deref(), EstiloMsg::Cor2)
            ]
            .spacing(10)
            .align_y(Alignment::Start),
            titulo("Tema do perfil"),
            temas.wrap().vertical_spacing(8),
        ]
        .spacing(14)
        .padding(24)
        .width(500),
    )
    .style(tema::rolagem);

    let mut escolha = ed.aplicado(eu.escolha);
    // A prévia é do nome: o efeito tocando por cima não deixaria ver.
    escolha.efeito = None;
    let placa = escolha
        .placa
        .as_ref()
        .and_then(|p| crate::cosmeticos::quadro(&p.arquivo, true))
        .map(|q| crate::ui::loja::img_q(q, Length::Fill, Length::Fill, iced::ContentFit::Cover));
    let direita = container(
        column![
            crate::ui::loja::mini_cartao(eu, &escolha, 300.0, 300.0, true),
            linha_de_mensagem(eu, &escolha),
            crate::ui::loja::linha_com_placa(eu, &escolha, placa, Length::Fixed(300.0)),
        ]
        .spacing(14)
        .align_x(Alignment::Center),
    )
    .padding(24)
    .width(400)
    .height(Length::Fill)
    .style(move |_| container::Style {
        background: Some(Background::Color(c.lateral)),
        ..Default::default()
    });

    let rodape = row![
        button(text("Restaurar padrão").size(14))
            .padding([10, 14])
            .style(tema::botao_transparente)
            .on_press(Msg::EstiloNome(EstiloMsg::Restaura)),
        space().width(Length::Fill),
        button(
            row![
                ico(icones::dados(), 16.0, c.texto),
                text("Surpreenda-me").size(14)
            ]
            .spacing(8)
            .align_y(Alignment::Center)
        )
        .padding([10, 14])
        .style(tema::botao_secundario)
        .on_press(Msg::EstiloNome(EstiloMsg::Surpreenda)),
        button(text("Aplicar").size(14))
            .padding([10, 22])
            .style(tema::botao_primario)
            .on_press(Msg::AplicaEstiloNome),
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .padding([14, 20]);

    let fechar = button(ico(icones::fechar(), 18.0, c.texto_fraco))
        .padding(6)
        .style(tema::botao_transparente)
        .on_press(Msg::FechaEstiloNome);
    let corpo = column![
        iced::widget::stack![
            row![esquerda, direita].height(560),
            container(fechar).align_right(Length::Fill).padding(10),
        ],
        container(space().height(1))
            .width(Length::Fill)
            .style(tema::divisoria),
        rodape,
    ];
    let janela = container(corpo)
        .width(900)
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
    crate::ui::efeitos::janela_fechavel("estilo-nome", janela.into(), 0.7, Msg::FechaEstiloNome)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn hex_digitado_so_vale_quando_completo() {
        let mut ed = EdicaoEstilo::default();
        aplica(&mut ed, EstiloMsg::Hex1("#12".into()));
        assert!(ed.estilo.cor.is_none());
        aplica(&mut ed, EstiloMsg::Hex1("AABBCC".into()));
        assert_eq!(ed.estilo.cor.as_deref(), Some("#aabbcc"));
    }

    #[test]
    fn restaurar_limpa_tudo() {
        let mut ed = EdicaoEstilo::default();
        aplica(&mut ed, EstiloMsg::Surpreenda);
        assert!(!ed.estilo.vazio());
        aplica(&mut ed, EstiloMsg::Restaura);
        assert!(ed.aplicado(&Escolha::default()).nome.is_none());
    }
}
