//! Wordle (em inglês, como o do Discord): a palavra do dia é do servidor, que
//! valida cada tentativa e devolve só as cores. Aqui ficam o jogo, a barra de
//! controles, a mensagem de "app" no chat e o estado do que está na tela.

use iced::widget::{Column, button, column, container, row, space, text, text_input};
use iced::{Alignment, Background, Border, Color, Element, Length, Task};

use crate::api::{EstadoWordle, GrupoWordle, JogadorWordle};
use crate::sala::Mensagem;
use crate::tarefas::em_segundo_plano;
use crate::ui::efeitos::ico;
use crate::{Msg, SalaAberta, api, fraco, icones, negrito, tema};

const VERDE: Color = tema::rgb(0x538d4e);
const AMARELO: Color = tema::rgb(0xb59f3b);
const CINZA: Color = tema::rgb(0x3a3a3c);
const VAZIO: Color = tema::rgb(0x565758);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModoWordle {
    /// Ocupa o lugar do palco, com o chat embaixo.
    Normal,
    /// Só o cartão na coluna da esquerda; o jogo continua guardado.
    Minimizado,
    /// Ocupa a área toda (sem chat).
    Expandido,
}

#[derive(Debug, Clone)]
pub struct EstadoUiWordle {
    pub estado: Option<EstadoWordle>,
    pub atual: String,
    pub erro: Option<String>,
    pub modo: ModoWordle,
    pub enviando: bool,
}

impl EstadoUiWordle {
    pub fn novo() -> Self {
        Self {
            estado: None,
            atual: String::new(),
            erro: None,
            modo: ModoWordle::Normal,
            enviando: false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum WordleMsg {
    /// "Play now!" na mensagem: abre o jogo de hoje.
    Abre,
    /// /wordle ou botão em Atividades: garante a mensagem do dia e abre o jogo.
    Inicia,
    Iniciado(Result<(), String>),
    Carregado(Result<EstadoWordle, String>),
    Input(String),
    Tecla(char),
    Apaga,
    Envia,
    Resposta(Result<EstadoWordle, (String, String)>),
    Modo(ModoWordle),
    Fecha,
    CarregaGrupo,
    Grupo(Result<GrupoWordle, String>),
}

/// Só letras a–z, em minúsculas, no máximo cinco.
pub fn filtra_entrada(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_lowercase())
        .take(5)
        .collect()
}

/// Melhor cor já vista de cada letra (verde > amarelo > cinza).
pub fn cores_do_teclado(est: &EstadoWordle) -> std::collections::HashMap<char, char> {
    let valor = |c: char| match c {
        'g' => 3,
        'y' => 2,
        _ => 1,
    };
    let mut m: std::collections::HashMap<char, char> = std::collections::HashMap::new();
    for t in &est.tentativas {
        for (l, c) in t.palavra.chars().zip(t.cores.chars()) {
            let e = m.entry(l).or_insert(c);
            if valor(c) > valor(*e) {
                *e = c;
            }
        }
    }
    m
}

fn mensagem_do_codigo(codigo: &str, texto: &str) -> String {
    match codigo {
        "curta" => "Not enough letters".into(),
        "invalida" => "Not in word list".into(),
        "fim" => "You already finished today's puzzle".into(),
        _ => texto.to_string(),
    }
}

impl SalaAberta {
    pub(crate) fn wordle_update(&mut self, m: WordleMsg) -> Task<Msg> {
        let (srv, token) = (self.entrada.servidor.clone(), self.entrada.token.clone());
        match m {
            WordleMsg::Inicia => {
                let conversa = self.entrada.sala.clone();
                self.wordle = Some(EstadoUiWordle::novo());
                return em_segundo_plano(
                    move || {
                        api::wordle_inicia(&srv, &token, &conversa).map_err(|e| format!("{e:#}"))
                    },
                    |r| Msg::Wordle(WordleMsg::Iniciado(r)),
                );
            }
            WordleMsg::Iniciado(r) => {
                if let Err(e) = r
                    && let Some(w) = &mut self.wordle
                {
                    w.erro = Some(e);
                }
                return Task::batch([
                    Task::done(Msg::Wordle(WordleMsg::Abre)),
                    Task::done(Msg::Wordle(WordleMsg::CarregaGrupo)),
                ]);
            }
            WordleMsg::Abre => {
                if self.wordle.is_none() {
                    self.wordle = Some(EstadoUiWordle::novo());
                }
                if let Some(w) = &mut self.wordle {
                    w.modo = ModoWordle::Normal;
                }
                return Task::batch([
                    em_segundo_plano(
                        move || api::wordle_estado(&srv, &token).map_err(|e| format!("{e:#}")),
                        |r| Msg::Wordle(WordleMsg::Carregado(r)),
                    ),
                    iced::widget::operation::focus("wordle-input"),
                ]);
            }
            WordleMsg::Carregado(r) => {
                if let Some(w) = &mut self.wordle {
                    match r {
                        Ok(e) => w.estado = Some(e),
                        Err(e) => w.erro = Some(e),
                    }
                }
            }
            WordleMsg::Input(s) => {
                if let Some(w) = &mut self.wordle {
                    w.atual = filtra_entrada(&s);
                    w.erro = None;
                }
            }
            WordleMsg::Tecla(c) => {
                if let Some(w) = &mut self.wordle
                    && w.atual.len() < 5
                {
                    w.atual.push(c.to_ascii_lowercase());
                    w.erro = None;
                }
            }
            WordleMsg::Apaga => {
                if let Some(w) = &mut self.wordle {
                    w.atual.pop();
                    w.erro = None;
                }
            }
            WordleMsg::Envia => {
                let Some(w) = &mut self.wordle else {
                    return Task::none();
                };
                if w.enviando || w.estado.as_ref().is_some_and(|e| e.fim.is_some()) {
                    return Task::none();
                }
                if w.atual.len() < 5 {
                    w.erro = Some("Not enough letters".into());
                    return Task::none();
                }
                w.enviando = true;
                let palavra = w.atual.clone();
                return em_segundo_plano(
                    move || api::wordle_tenta(&srv, &token, &palavra),
                    |r| Msg::Wordle(WordleMsg::Resposta(r)),
                );
            }
            WordleMsg::Resposta(r) => {
                let mut acabou = false;
                if let Some(w) = &mut self.wordle {
                    w.enviando = false;
                    match r {
                        Ok(e) => {
                            acabou = e.fim.is_some();
                            w.estado = Some(e);
                            w.atual.clear();
                            w.erro = None;
                        }
                        Err((codigo, texto)) => w.erro = Some(mensagem_do_codigo(&codigo, &texto)),
                    }
                }
                // O resumo do grupo (cores de quem jogou) muda a cada tentativa.
                let _ = acabou;
                return Task::done(Msg::Wordle(WordleMsg::CarregaGrupo));
            }
            WordleMsg::Modo(modo) => {
                if let Some(w) = &mut self.wordle {
                    w.modo = modo;
                }
            }
            WordleMsg::Fecha => self.wordle = None,
            WordleMsg::CarregaGrupo => {
                let conversa = self.entrada.sala.clone();
                return em_segundo_plano(
                    move || {
                        api::wordle_grupo(&srv, &token, &conversa).map_err(|e| format!("{e:#}"))
                    },
                    |r| Msg::Wordle(WordleMsg::Grupo(r)),
                );
            }
            WordleMsg::Grupo(r) => {
                if let Ok(g) = r {
                    self.wordle_grupo = Some(g);
                }
            }
        }
        Task::none()
    }
}

fn cor_da_letra(c: char) -> Color {
    match c {
        'g' => VERDE,
        'y' => AMARELO,
        _ => CINZA,
    }
}

fn tile<'a>(letra: Option<char>, cor: Option<char>, lado: f32) -> Element<'a, Msg> {
    let fundo = cor.map(cor_da_letra);
    container(
        text(
            letra
                .map(|l| l.to_ascii_uppercase().to_string())
                .unwrap_or_default(),
        )
        .size(lado * 0.5)
        .font(negrito())
        .color(Color::WHITE),
    )
    .width(lado)
    .height(lado)
    .center_x(lado)
    .center_y(lado)
    .style(move |_| container::Style {
        background: fundo.map(Background::Color),
        border: Border {
            color: if fundo.is_some() {
                Color::TRANSPARENT
            } else if letra.is_some() {
                Color::from_rgb8(0x86, 0x88, 0x8a)
            } else {
                VAZIO
            },
            width: 2.0,
            radius: 2.0.into(),
        },
        ..Default::default()
    })
    .into()
}

fn tecla<'a>(rotulo: &str, msg: Msg, cor: Option<char>, largura: f32) -> Element<'a, Msg> {
    let fundo = cor
        .map(cor_da_letra)
        .unwrap_or(Color::from_rgb8(0x81, 0x83, 0x84));
    button(
        text(rotulo.to_string())
            .size(14)
            .font(negrito())
            .color(Color::WHITE)
            .width(Length::Fill)
            .align_x(Alignment::Center),
    )
    .width(largura)
    .padding([12, 0])
    .style(move |_, st| button::Style {
        background: Some(Background::Color(
            if matches!(st, button::Status::Hovered) {
                tema::mistura(fundo, Color::WHITE, 0.12)
            } else {
                fundo
            },
        )),
        text_color: Color::WHITE,
        border: Border {
            radius: 4.0.into(),
            ..Default::default()
        },
        ..Default::default()
    })
    .on_press(msg)
    .into()
}

/// A barra de baixo: minimizar, expandir/recolher e sair.
fn barra_de_controles<'a>(modo: ModoWordle) -> Element<'a, Msg> {
    let c = tema::cores();
    let b = |icone: iced::widget::svg::Handle, msg: WordleMsg, perigo: bool| -> Element<'a, Msg> {
        button(ico(
            icone,
            18.0,
            if perigo { Color::WHITE } else { c.texto },
        ))
        .padding([10, 14])
        .style(move |_, st| button::Style {
            background: Some(Background::Color(if perigo {
                c.perigo
            } else if matches!(st, button::Status::Hovered) {
                c.forte
            } else {
                c.elevado
            })),
            border: Border {
                radius: tema::RAIO.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .on_press(Msg::Wordle(msg))
        .into()
    };
    container(
        row![
            b(
                icones::seta_baixo(),
                WordleMsg::Modo(ModoWordle::Minimizado),
                false
            ),
            b(
                if modo == ModoWordle::Expandido {
                    icones::minimizar()
                } else {
                    icones::maximizar()
                },
                WordleMsg::Modo(if modo == ModoWordle::Expandido {
                    ModoWordle::Normal
                } else {
                    ModoWordle::Expandido
                }),
                false
            ),
            b(icones::sair(), WordleMsg::Fecha, true),
        ]
        .spacing(8),
    )
    .center_x(Length::Fill)
    .padding([8, 0])
    .into()
}

/// O jogo (grade, mensagem, campo, teclado e controles). Se `lateral` não for
/// vazio, as pessoas da chamada ficam numa faixa ao lado (modo apresentação).
pub fn tela_jogo<'a>(
    w: &'a EstadoUiWordle,
    pessoas_laterais: Vec<(&'a str, &'a str, Option<&'a str>)>,
) -> Element<'a, Msg> {
    let c = tema::cores();
    let max = w
        .estado
        .as_ref()
        .map(|e| e.max)
        .filter(|m| *m > 0)
        .unwrap_or(6) as usize;
    let tentativas = w
        .estado
        .as_ref()
        .map(|e| e.tentativas.as_slice())
        .unwrap_or(&[]);
    let fim = w.estado.as_ref().and_then(|e| e.fim.clone());
    let mut grade = Column::new().spacing(5).align_x(Alignment::Center);
    for i in 0..max {
        let mut linha = row![].spacing(5);
        if let Some(t) = tentativas.get(i) {
            for (l, cor) in t.palavra.chars().zip(t.cores.chars()) {
                linha = linha.push(tile(Some(l), Some(cor), 48.0));
            }
        } else if i == tentativas.len() && fim.is_none() {
            let mut cs = w.atual.chars();
            for _ in 0..5 {
                linha = linha.push(tile(cs.next(), None, 48.0));
            }
        } else {
            for _ in 0..5 {
                linha = linha.push(tile(None, None, 48.0));
            }
        }
        grade = grade.push(linha);
    }

    let teclas = w.estado.as_ref().map(cores_do_teclado).unwrap_or_default();
    let mut teclado = Column::new().spacing(6).align_x(Alignment::Center);
    for (i, linha) in ["qwertyuiop", "asdfghjkl", "zxcvbnm"].iter().enumerate() {
        let mut r = row![].spacing(5);
        if i == 2 {
            r = r.push(tecla("ENTER", Msg::Wordle(WordleMsg::Envia), None, 62.0));
        }
        for ch in linha.chars() {
            r = r.push(tecla(
                &ch.to_ascii_uppercase().to_string(),
                Msg::Wordle(WordleMsg::Tecla(ch)),
                teclas.get(&ch).copied(),
                36.0,
            ));
        }
        if i == 2 {
            r = r.push(tecla("⌫", Msg::Wordle(WordleMsg::Apaga), None, 62.0));
        }
        teclado = teclado.push(r);
    }

    let linha_estado: Element<'a, Msg> = match (&fim, &w.erro) {
        (Some(f), _) if f == "ganhou" => text(format!(
            "Nicely done! Solved in {}/{max}.",
            tentativas.len()
        ))
        .size(15)
        .color(VERDE)
        .into(),
        (Some(_), _) => text(format!(
            "The word was {}.",
            w.estado
                .as_ref()
                .and_then(|e| e.resposta.clone())
                .unwrap_or_default()
                .to_uppercase()
        ))
        .size(15)
        .color(c.perigo)
        .into(),
        (None, Some(e)) => text(e.clone()).size(14).color(c.perigo).into(),
        (None, None) => text(" ").size(14).into(),
    };

    let titulo = row![
        text("Wordle").size(26).font(negrito()),
        text(
            w.estado
                .as_ref()
                .map(|e| format!("No. {}", e.numero))
                .unwrap_or_default()
        )
        .size(13)
        .style(fraco),
    ]
    .spacing(10)
    .align_y(Alignment::End);

    let mut jogo = column![titulo, grade, linha_estado]
        .spacing(10)
        .align_x(Alignment::Center);
    if fim.is_none() {
        jogo = jogo.push(
            text_input("Type your guess…", &w.atual)
                .id("wordle-input")
                .on_input(|s| Msg::Wordle(WordleMsg::Input(s)))
                .on_submit(Msg::Wordle(WordleMsg::Envia))
                .padding([8, 10])
                .size(14)
                .width(220)
                .style(tema::campo),
        );
        jogo = jogo.push(teclado);
    }
    jogo = jogo.push(barra_de_controles(w.modo));

    let miolo = container(jogo)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .padding(12)
        .style(move |_| container::Style {
            background: Some(Background::Color(c.palco)),
            ..Default::default()
        });
    if pessoas_laterais.is_empty() {
        return miolo.into();
    }
    // Modo apresentação: pessoas da chamada em faixa pequena ao lado.
    let mut faixa = Column::new().spacing(10).padding(10).width(110);
    for (uid, nome, foto) in pessoas_laterais {
        faixa = faixa.push(
            column![
                crate::ui::avatar::avatar(foto, uid, nome, 48.0, None, c.palco),
                text(nome.to_string())
                    .size(11)
                    .style(fraco)
                    .wrapping(iced::widget::text::Wrapping::None),
            ]
            .spacing(4)
            .align_x(Alignment::Center),
        );
    }
    row![miolo, container(faixa).style(tema::painel_lateral_reto)].into()
}

/// O cartão "Wordle" da coluna da esquerda (jogo aberto ou minimizado).
pub fn cartao_da_coluna<'a>(titulo_da_conversa: &'a str, modo: ModoWordle) -> Element<'a, Msg> {
    let c = tema::cores();
    container(
        row![
            button(
                row![
                    ico(icones::grade(), 18.0, c.texto),
                    column![
                        text("Wordle").size(14).font(negrito()),
                        text(titulo_da_conversa).size(12).style(fraco),
                    ]
                    .spacing(1),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .padding([6, 8])
            .style(tema::item_de_lista)
            .on_press(Msg::Wordle(WordleMsg::Modo(
                if modo == ModoWordle::Minimizado {
                    ModoWordle::Normal
                } else {
                    ModoWordle::Minimizado
                }
            ))),
            button(ico(icones::sair(), 16.0, c.perigo))
                .padding(8)
                .style(tema::botao_icone(false, true))
                .on_press(Msg::Wordle(WordleMsg::Fecha)),
        ]
        .align_y(Alignment::Center),
    )
    .padding(4)
    .style(move |_| container::Style {
        background: Some(Background::Color(c.palco)),
        border: Border {
            color: c.borda,
            width: 1.0,
            radius: tema::RAIO.into(),
        },
        ..Default::default()
    })
    .into()
}

fn mini_grade<'a>(j: &'a JogadorWordle) -> Element<'a, Msg> {
    let mut col = Column::new().spacing(3);
    for i in 0..6 {
        let mut r = row![].spacing(3);
        match j.cores.get(i) {
            Some(cores) => {
                for ch in cores.chars() {
                    r = r.push(tile(None, Some(ch), 16.0));
                }
            }
            None => {
                for _ in 0..5 {
                    r = r.push(container(space()).width(16).height(16).style(|_| {
                        container::Style {
                            background: Some(Background::Color(Color::from_rgb8(0x18, 0x18, 0x1a))),
                            border: Border {
                                radius: 2.0.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        }
                    }));
                }
            }
        }
        col = col.push(r);
    }
    col.into()
}

/// Quem ganhou primeiro (menos tentativas); quem perdeu por último.
pub fn ordena_resultados(v: &[JogadorWordle]) -> Vec<&JogadorWordle> {
    let mut o: Vec<&JogadorWordle> = v.iter().collect();
    o.sort_by_key(|j| (j.fim.as_deref() != Some("ganhou"), j.n));
    o
}

/// "Wordle APP" no chat (prints 16 e 17).
fn abre_previa_app() -> Msg {
    Msg::AbrePrevia {
        uid: None,
        nome: "Wordle".into(),
        app: true,
    }
}

pub fn mensagem_app<'a>(m: &'a Mensagem, grupo: Option<&'a GrupoWordle>) -> Element<'a, Msg> {
    let c = tema::cores();
    let logo = container(
        column![
            row![
                tile(None, Some('g'), 12.0),
                tile(None, Some('y'), 12.0),
                tile(None, Some('x'), 12.0)
            ]
            .spacing(2),
            row![
                tile(None, Some('y'), 12.0),
                tile(None, Some('g'), 12.0),
                tile(None, Some('g'), 12.0)
            ]
            .spacing(2),
        ]
        .spacing(2),
    )
    .padding(6)
    .style(move |_| container::Style {
        background: Some(Background::Color(c.palco)),
        border: Border {
            radius: tema::RAIO.into(),
            ..Default::default()
        },
        ..Default::default()
    });
    let mut corpo = Column::new().spacing(6);
    corpo = corpo.push(
        row![
            iced::widget::mouse_area(text("Wordle").size(14).font(negrito()))
                .interaction(iced::mouse::Interaction::Pointer)
                .on_press(abre_previa_app()),
            container(text("APP").size(10).font(negrito()).color(Color::WHITE))
                .padding([1, 5])
                .style(move |_| container::Style {
                    background: Some(Background::Color(c.destaque)),
                    border: Border {
                        radius: 3.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            text(crate::ui::conversa::quando(m.ts))
                .size(12)
                .style(fraco),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    );
    if let Some(g) = grupo {
        let ontem = ordena_resultados(&g.ontem);
        if g.streak > 0 && !ontem.is_empty() {
            corpo =
                corpo.push(text(format!("Your group is on a {} day streak!", g.streak)).size(14));
            corpo = corpo.push(text("Here are yesterday's results:").size(14));
            for (i, j) in ontem.iter().enumerate() {
                let placar = if j.fim.as_deref() == Some("ganhou") {
                    format!("{}/6", j.n)
                } else {
                    "X/6".to_string()
                };
                corpo = corpo.push(
                    row![
                        if i == 0 && j.fim.as_deref() == Some("ganhou") {
                            ico(icones::coroa(), 14.0, c.alerta)
                        } else {
                            space().width(14).into()
                        },
                        text(format!("{placar}: @{}", j.name)).size(14),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                );
            }
        }
        if !g.hoje.is_empty() {
            let nomes: Vec<&str> = g.hoje.iter().map(|j| j.name.as_str()).collect();
            corpo = corpo.push(text(format!("{} was playing", nomes.join(", "))).size(14));
            let mut cards = row![].spacing(12);
            for j in &g.hoje {
                cards = cards.push(
                    container(
                        row![
                            crate::ui::avatar::avatar(
                                j.avatar.as_deref(),
                                &j.uid,
                                &j.name,
                                44.0,
                                None,
                                c.palco
                            ),
                            mini_grade(j),
                        ]
                        .spacing(12)
                        .align_y(Alignment::Center),
                    )
                    .padding(12)
                    .style(move |_| container::Style {
                        background: None,
                        border: Border {
                            color: c.borda,
                            width: 1.0,
                            radius: tema::RAIO_GRANDE.into(),
                        },
                        ..Default::default()
                    }),
                );
            }
            corpo = corpo.push(
                container(
                    column![text(format!("Wordle No. {}", g.numero)).size(13), cards,]
                        .spacing(10)
                        .align_x(Alignment::Center),
                )
                .padding(14)
                .style(move |_| container::Style {
                    background: Some(Background::Color(c.palco)),
                    border: Border {
                        radius: tema::RAIO_GRANDE.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            );
        }
    }
    corpo = corpo.push(
        button(text("Play now!").size(13).font(negrito()))
            .padding([6, 12])
            .style(tema::botao_primario)
            .on_press(Msg::Wordle(WordleMsg::Abre)),
    );
    row![
        iced::widget::mouse_area(logo)
            .interaction(iced::mouse::Interaction::Pointer)
            .on_press(abre_previa_app()),
        corpo
    ]
    .spacing(12)
    .padding([10, 0])
    .into()
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::api::TentativaWordle;

    #[test]
    fn entrada_so_aceita_letras_ate_cinco() {
        assert_eq!(filtra_entrada("Cr4ne!x"), "crnex");
        assert_eq!(filtra_entrada("abcdefgh"), "abcde");
    }

    #[test]
    fn teclado_guarda_a_melhor_cor() {
        let e = EstadoWordle {
            tentativas: vec![
                TentativaWordle {
                    palavra: "crate".into(),
                    cores: "gggxg".into(),
                },
                TentativaWordle {
                    palavra: "eager".into(),
                    cores: "yyxxy".into(),
                },
            ],
            ..Default::default()
        };
        let m = cores_do_teclado(&e);
        assert_eq!(m[&'e'], 'g'); // verde na 1ª, amarelo na 2ª: fica verde
        assert_eq!(m[&'t'], 'x');
    }

    #[test]
    fn quem_ganhou_primeiro_aparece_antes() {
        let j = |n: u32, fim: &str| JogadorWordle {
            uid: n.to_string(),
            name: n.to_string(),
            avatar: None,
            n,
            fim: Some(fim.into()),
            cores: vec![],
        };
        let v = vec![j(6, "perdeu"), j(5, "ganhou"), j(3, "ganhou")];
        let o: Vec<u32> = ordena_resultados(&v).iter().map(|x| x.n).collect();
        assert_eq!(o, vec![3, 5, 6]);
    }
}
