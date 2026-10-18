//! Wordle (em inglês, como o do Discord): a palavra do dia é do servidor, que
//! valida cada tentativa e devolve só as cores. Aqui ficam o jogo, a barra de
//! controles, a mensagem de "app" no chat e o estado do que está na tela.

use iced::widget::{Column, button, column, container, row, space, text};
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
    pub rodada_dia: Option<String>,
}

impl EstadoUiWordle {
    pub fn novo() -> Self {
        Self {
            estado: None,
            atual: String::new(),
            erro: None,
            modo: ModoWordle::Normal,
            enviando: false,
            rodada_dia: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum WordleMsg {
    /// "Play now!" na mensagem: abre o jogo de hoje ou da rodada específica.
    Abre(Option<String>),
    /// /wordle ou botão em Atividades: garante a mensagem do dia e abre o jogo.
    Inicia,
    Iniciado(Result<(), String>),
    Carregado(Result<EstadoWordle, String>),
    Tecla(char),
    Apaga,
    Envia,
    Resposta(Result<EstadoWordle, (String, String)>),
    Modo(ModoWordle),
    Fecha,
    CarregaGrupo(Option<String>),
    Grupo(Result<GrupoWordle, String>),
}

/// Só letras a–z, em minúsculas, no máximo cinco.
#[allow(dead_code)]
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
                if !conversa.starts_with("grp_") {
                    self.aviso = Some("O Wordle só está disponível em grupos.".to_string());
                    return Task::none();
                }
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
                    Task::done(Msg::Wordle(WordleMsg::Abre(None))),
                    Task::done(Msg::Wordle(WordleMsg::CarregaGrupo(None))),
                ]);
            }
            WordleMsg::Abre(dia_opt) => {
                if self.wordle.is_none() {
                    self.wordle = Some(EstadoUiWordle::novo());
                }
                if let Some(w) = &mut self.wordle {
                    w.modo = ModoWordle::Normal;
                    if dia_opt.is_some() {
                        w.rodada_dia = dia_opt.clone();
                    }
                }
                let dia_para_busca =
                    dia_opt.or_else(|| self.wordle.as_ref().and_then(|w| w.rodada_dia.clone()));
                return em_segundo_plano(
                    move || {
                        api::wordle_estado(&srv, &token, dia_para_busca.as_deref())
                            .map_err(|e| format!("{e:#}"))
                    },
                    |r| Msg::Wordle(WordleMsg::Carregado(r)),
                );
            }
            WordleMsg::Carregado(r) => {
                if let Some(w) = &mut self.wordle {
                    match r {
                        Ok(e) => w.estado = Some(e),
                        Err(e) => w.erro = Some(e),
                    }
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
                let dia_envio = w.rodada_dia.clone();
                return em_segundo_plano(
                    move || api::wordle_tenta(&srv, &token, &palavra, dia_envio.as_deref()),
                    |r| Msg::Wordle(WordleMsg::Resposta(r)),
                );
            }
            WordleMsg::Resposta(r) => {
                let mut acabou = false;
                let dia_opt = self.wordle.as_ref().and_then(|w| w.rodada_dia.clone());
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
                return Task::done(Msg::Wordle(WordleMsg::CarregaGrupo(dia_opt)));
            }
            WordleMsg::Modo(modo) => {
                if let Some(w) = &mut self.wordle {
                    w.modo = modo;
                }
            }
            WordleMsg::Fecha => self.wordle = None,
            WordleMsg::CarregaGrupo(dia_opt) => {
                let conversa = self.entrada.sala.clone();
                let dia_busca =
                    dia_opt.or_else(|| self.wordle.as_ref().and_then(|w| w.rodada_dia.clone()));
                return em_segundo_plano(
                    move || {
                        api::wordle_grupo(&srv, &token, &conversa, dia_busca.as_deref())
                            .map_err(|e| format!("{e:#}"))
                    },
                    |r| Msg::Wordle(WordleMsg::Grupo(r)),
                );
            }
            WordleMsg::Grupo(r) => match r {
                Ok(g) => {
                    self.wordle_grupo.insert(g.dia.clone(), g);
                }
                Err(e) => log::warn!("[wordle] resumo do grupo: {e}"),
            },
        }
        Task::none()
    }
}

fn cor_da_letra(c: char) -> Color {
    match c {
        'g' => VERDE,
        'y' => AMARELO,
        // No tema claro o cinza do Wordle é mais leve (#787c7e), como no original.
        _ if tema::luminancia(tema::cores().fundo) > 0.5 => tema::rgb(0x787c7e),
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

fn tecla<'a>(
    rotulo: &str,
    msg: Msg,
    cor: Option<char>,
    largura: f32,
    altura: f32,
) -> Element<'a, Msg> {
    let tam_txt = (altura * 0.32).clamp(11.0, 14.0);
    let fundo = cor
        .map(cor_da_letra)
        .unwrap_or(Color::from_rgb8(0x81, 0x83, 0x84));
    button(
        text(rotulo.to_string())
            .size(tam_txt)
            .font(negrito())
            .color(Color::WHITE)
            .width(Length::Fill)
            .align_x(Alignment::Center),
    )
    .width(largura)
    .height(altura)
    .padding([((altura - tam_txt * 1.3) / 2.0).max(0.0), 0.0])
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
        .padding([7, 11])
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
        .spacing(6),
    )
    .into()
}

/// O jogo (grade, mensagem, campo, teclado e controles). Se `lateral` não for
/// vazio, as pessoas da chamada ficam numa faixa ao lado (modo apresentação).
pub fn tela_jogo<'a>(
    w: &'a EstadoUiWordle,
    pessoas_laterais: Vec<(&'a str, &'a str, Option<&'a str>)>,
) -> Element<'a, Msg> {
    let c = tema::cores();
    // O tamanho das peças acompanha o espaço: com altura fixa de 48 px o
    // teclado ficava cortado no modo normal (3/5 da coluna).
    let miolo = iced::widget::responsive(move |tam| jogo_no_tamanho(w, tam))
        .width(Length::Fill)
        .height(Length::Fill);
    let miolo = container(miolo)
        .width(Length::Fill)
        .height(Length::Fill)
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

/// Lado da peça para caber em `tam`: grade de 6 linhas + 3 fileiras de teclas
/// (cada uma ~0,85 da peça) + título, linha de estado e espaços (~150 px).
pub fn lado_da_peca(tam: iced::Size, com_teclado: bool) -> f32 {
    let (por_peca, fixo) = if com_teclado {
        (8.55, 150.0)
    } else {
        (6.0, 110.0)
    };
    let pela_altura = (tam.height - fixo) / por_peca;
    // Largura: 10 teclas de 0,75 da peça + espaços.
    let pela_largura = (tam.width - 70.0) / 7.5;
    pela_altura.min(pela_largura).clamp(22.0, 52.0)
}

fn jogo_no_tamanho<'a>(w: &'a EstadoUiWordle, tam: iced::Size) -> Element<'a, Msg> {
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
    let lado = lado_da_peca(tam, fim.is_none());
    let alt_tecla = (lado * 0.85).round();
    let larg_tecla = (lado * 0.75).round();
    let mut grade = Column::new().spacing(5).align_x(Alignment::Center);
    for i in 0..max {
        let mut linha = row![].spacing(5);
        if let Some(t) = tentativas.get(i) {
            for (l, cor) in t.palavra.chars().zip(t.cores.chars()) {
                linha = linha.push(tile(Some(l), Some(cor), lado));
            }
        } else if i == tentativas.len() && fim.is_none() {
            let mut cs = w.atual.chars();
            for _ in 0..5 {
                linha = linha.push(tile(cs.next(), None, lado));
            }
        } else {
            for _ in 0..5 {
                linha = linha.push(tile(None, None, lado));
            }
        }
        grade = grade.push(linha);
    }

    let teclas = w.estado.as_ref().map(cores_do_teclado).unwrap_or_default();
    let mut teclado = Column::new().spacing(6).align_x(Alignment::Center);
    for (i, linha) in ["qwertyuiop", "asdfghjkl", "zxcvbnm"].iter().enumerate() {
        let mut r = row![].spacing(5);
        if i == 2 {
            r = r.push(tecla(
                "ENTER",
                Msg::Wordle(WordleMsg::Envia),
                None,
                larg_tecla * 1.7,
                alt_tecla,
            ));
        }
        for ch in linha.chars() {
            r = r.push(tecla(
                &ch.to_ascii_uppercase().to_string(),
                Msg::Wordle(WordleMsg::Tecla(ch)),
                teclas.get(&ch).copied(),
                larg_tecla,
                alt_tecla,
            ));
        }
        if i == 2 {
            r = r.push(tecla(
                "⌫",
                Msg::Wordle(WordleMsg::Apaga),
                None,
                larg_tecla * 1.7,
                alt_tecla,
            ));
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
    // Controles na linha do título (economiza uma linha inteira de altura).
    let topo = row![
        space().width(Length::Fill),
        titulo,
        container(barra_de_controles(w.modo))
            .width(Length::Fill)
            .align_x(Alignment::End),
    ]
    .align_y(Alignment::Center);

    let mut jogo = column![topo, grade, linha_estado]
        .spacing(10)
        .align_x(Alignment::Center);
    if fim.is_none() {
        jogo = jogo.push(teclado);
    }

    container(jogo)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .padding(12)
        .into()
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
                    // Quadrado vazio: só a borda, como no Discord (e legível no tema claro).
                    r = r.push(container(space()).width(16).height(16).style(|_| {
                        container::Style {
                            background: None,
                            border: Border {
                                color: tema::cores().borda,
                                width: 1.0,
                                radius: 2.0.into(),
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

/// "Ana was playing", "Ana and Bia were playing", "Ana, Bia and Caio were playing".
pub fn frase_jogando(nomes: &[&str]) -> String {
    match nomes {
        [] => String::new(),
        [um] => format!("{um} was playing"),
        [resto @ .., ultimo] => format!("{} and {ultimo} were playing", resto.join(", ")),
    }
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
            corpo = corpo.push(text(frase_jogando(&nomes)).size(14));
            // Como no Discord: uma pessoa, avatar grande ao lado da grade; mais
            // de uma, um cartão em pé por pessoa (avatar em cima).
            let sozinho = g.hoje.len() == 1;
            let cartao = |j: &'a JogadorWordle| -> Element<'a, Msg> {
                let av = crate::ui::avatar::avatar(
                    j.avatar.as_deref(),
                    &j.uid,
                    &j.name,
                    if sozinho { 96.0 } else { 64.0 },
                    None,
                    c.palco,
                );
                let dentro: Element<'a, Msg> = if sozinho {
                    row![av, mini_grade(j)]
                        .spacing(28)
                        .align_y(Alignment::Center)
                        .into()
                } else {
                    column![av, mini_grade(j)]
                        .spacing(10)
                        .align_x(Alignment::Center)
                        .into()
                };
                container(dentro)
                    .padding(if sozinho { [16, 28] } else { [12, 14] })
                    .style(move |_| container::Style {
                        background: None,
                        border: Border {
                            color: c.borda,
                            width: 1.0,
                            radius: tema::RAIO_GRANDE.into(),
                        },
                        ..Default::default()
                    })
                    .into()
            };
            let mut cards = row![].spacing(12);
            for j in &g.hoje {
                cards = cards.push(cartao(j));
            }
            let cards = cards.wrap().vertical_spacing(12);
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
            .on_press(Msg::Wordle(WordleMsg::Abre(m.extra.wordle_dia.clone()))),
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
    fn pecas_cabem_no_espaco() {
        // Janela de 720 px no modo normal: ~370 px para o jogo.
        let l = lado_da_peca(iced::Size::new(760.0, 370.0), true);
        assert!(8.55 * l + 150.0 <= 370.0 + 0.5 || l == 22.0);
        // Com espaço sobrando, para em 52.
        assert_eq!(lado_da_peca(iced::Size::new(2000.0, 2000.0), true), 52.0);
    }

    #[test]
    fn frase_de_quem_jogou() {
        assert_eq!(frase_jogando(&["Ana"]), "Ana was playing");
        assert_eq!(frase_jogando(&["Ana", "Bia"]), "Ana and Bia were playing");
        assert_eq!(
            frase_jogando(&["Ana", "Bia", "Caio"]),
            "Ana, Bia and Caio were playing"
        );
    }

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
