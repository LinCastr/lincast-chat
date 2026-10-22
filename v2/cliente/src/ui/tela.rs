//! Compartilhar tela em duas etapas (C10): o que transmitir, e depois som e
//! qualidade. É uma janela por cima da conversa.

use iced::widget::{Column, button, checkbox, column, container, row, space, svg, text};
use iced::{Alignment, Background, Border, Element, Length};

use crate::captura_tela::{FPS_DA_TELA, PerfilTela, ResolucaoTela};
use crate::{AppTocando, Msg, fraco, icones, negrito, rotulo, tema};

/// O que fica guardado entre uma transmissão e outra
/// (`~/.config/lincast-chat/transmissao.json`).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct Guardado {
    resolucao: Option<String>,
    fps: Option<u32>,
    auto_bitrate: Option<bool>,
    #[serde(default)]
    todo_pc: bool,
    /// Executáveis marcados na última vez.
    #[serde(default)]
    apps: Vec<String>,
}

fn caminho_guardado() -> Option<std::path::PathBuf> {
    let d = directories::ProjectDirs::from("", "", "lincast-chat")?;
    Some(d.config_dir().join("transmissao.json"))
}

fn le_guardado() -> Guardado {
    caminho_guardado()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn chave_do_app(a: &AppTocando) -> String {
    if a.binario.is_empty() {
        a.nome.clone()
    } else {
        a.binario.clone()
    }
}

#[derive(Debug, Clone)]
pub struct ConfigCompartilharTela {
    pub perfil: PerfilTela,
    pub auto_bitrate: bool,
    pub apps: Vec<(AppTocando, bool)>,
    /// "Todo o PC": todo o som, menos o do próprio app (e da chamada).
    pub todo_pc: bool,
    /// A lista de sons está aberta (popup abaixo do botão).
    pub som_aberto: bool,
    /// Changing a stream that is already going (button "Aplicar").
    pub ajustando: bool,
}

impl Default for ConfigCompartilharTela {
    fn default() -> Self {
        Self {
            perfil: PerfilTela::PADRAO,
            auto_bitrate: true,
            apps: Vec::new(),
            todo_pc: false,
            som_aberto: false,
            ajustando: false,
        }
    }
}

impl ConfigCompartilharTela {
    /// Começa com o que a pessoa escolheu da última vez.
    pub fn guardada() -> Self {
        let g = le_guardado();
        let mut c = Self::default();
        if let Some(r) = g
            .resolucao
            .and_then(|n| ResolucaoTela::TODAS.into_iter().find(|r| r.rotulo() == n))
        {
            c.perfil.resolucao = r;
        }
        if let Some(f) = g.fps.filter(|f| FPS_DA_TELA.contains(f)) {
            c.perfil.fps = f;
        }
        if let Some(a) = g.auto_bitrate {
            c.auto_bitrate = a;
        }
        c.todo_pc = g.todo_pc;
        c
    }

    /// Marca de novo os aplicativos escolhidos da última vez (os que ainda tocam).
    pub fn remarca_apps(&mut self) {
        let g = le_guardado();
        for (a, marcado) in &mut self.apps {
            *marcado = g.apps.contains(&chave_do_app(a));
        }
    }

    pub fn salva(&self) {
        let Some(p) = caminho_guardado() else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        // Só troca a lista de apps quando ela foi carregada (senão apagaria a antiga).
        let apps = if self.apps.is_empty() {
            le_guardado().apps
        } else {
            self.apps
                .iter()
                .filter(|(_, m)| *m)
                .map(|(a, _)| chave_do_app(a))
                .collect()
        };
        let g = Guardado {
            resolucao: Some(self.perfil.resolucao.rotulo().to_string()),
            fps: Some(self.perfil.fps),
            auto_bitrate: Some(self.auto_bitrate),
            todo_pc: self.todo_pc,
            apps,
        };
        if let Ok(j) = serde_json::to_string_pretty(&g) {
            let _ = std::fs::write(p, j);
        }
    }
}

struct Opcao<T> {
    valor: T,
    rotulo: String,
}

impl<T> Clone for Opcao<T>
where
    T: Copy,
{
    fn clone(&self) -> Self {
        Self {
            valor: self.valor,
            rotulo: self.rotulo.clone(),
        }
    }
}

impl<T> PartialEq for Opcao<T>
where
    T: PartialEq,
{
    fn eq(&self, o: &Self) -> bool {
        self.valor == o.valor
    }
}

impl<T> std::fmt::Display for Opcao<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.rotulo)
    }
}

/// Escolha única entre várias opções: uma caixa que abre o resto para baixo.
pub fn segmentos<'a, T: Copy + PartialEq + 'a>(
    opcoes: &[(T, String)],
    atual: T,
    ao_escolher: impl Fn(T) -> Msg + 'a,
) -> Element<'a, Msg> {
    let lista: Vec<Opcao<T>> = opcoes
        .iter()
        .map(|(v, r)| Opcao {
            valor: *v,
            rotulo: r.clone(),
        })
        .collect();
    let sel = lista.iter().find(|o| o.valor == atual).cloned();
    iced::widget::pick_list(lista, sel, move |o| ao_escolher(o.valor))
        .width(Length::Fill)
        .padding([8, 12])
        .text_size(13)
        .style(tema::lista_suspensa)
        .menu_style(tema::menu_suspenso)
        .into()
}

fn secao_qualidade<'a>(perfil: PerfilTela) -> Element<'a, Msg> {
    let resolucoes: Vec<(ResolucaoTela, String)> = ResolucaoTela::TODAS
        .iter()
        .map(|r| (*r, r.rotulo().to_string()))
        .collect();
    let fps: Vec<(u32, String)> = FPS_DA_TELA.iter().map(|f| (*f, f.to_string())).collect();
    column![
        column![
            text("Resolução").size(12).style(fraco),
            segmentos(&resolucoes, perfil.resolucao, move |r| {
                Msg::MudaPerfilTela(PerfilTela {
                    resolucao: r,
                    ..perfil
                })
            }),
        ]
        .spacing(4),
        column![
            text("Quadros por segundo").size(12).style(fraco),
            segmentos(&fps, perfil.fps, move |f| {
                Msg::MudaPerfilTela(PerfilTela { fps: f, ..perfil })
            }),
        ]
        .spacing(4),
    ]
    .spacing(10)
    .into()
}

/// A mesma escolha de resolução e fps, para as Configurações.
pub fn seletor_de_qualidade<'a>(
    perfil: PerfilTela,
    ao_mudar: fn(PerfilTela) -> Msg,
) -> Element<'a, Msg> {
    let resolucoes: Vec<(ResolucaoTela, String)> = ResolucaoTela::TODAS
        .iter()
        .map(|r| (*r, r.rotulo().to_string()))
        .collect();
    let fps: Vec<(u32, String)> = FPS_DA_TELA.iter().map(|f| (*f, f.to_string())).collect();
    column![
        segmentos(&resolucoes, perfil.resolucao, move |r| ao_mudar(
            PerfilTela {
                resolucao: r,
                ..perfil
            }
        )),
        segmentos(&fps, perfil.fps, move |f| ao_mudar(PerfilTela {
            fps: f,
            ..perfil
        })),
    ]
    .spacing(6)
    .into()
}

/// Janela única "Compartilhar tela": som, qualidade e Transmitir. No Wayland o
/// seletor de tela/janela é do sistema e abre logo depois do clique.
pub fn modal_compartilhar_tela<'a>(
    cfg: &'a ConfigCompartilharTela,
    destino: &'a str,
    wayland: bool,
) -> Element<'a, Msg> {
    let c = tema::cores();
    let fechar = crate::ui::efeitos::com_dica(
        crate::botao_icone(icones::fechar(), false, false, Some(Msg::CancelaTela)),
        "Fechar",
        iced::widget::tooltip::Position::Bottom,
    );
    // Botão com o resumo; a lista abre num popup flutuante (ver `popup_sons`).
    let marcados = cfg.apps.iter().filter(|(_, m)| *m).count();
    let resumo = if cfg.todo_pc {
        "Todo o PC".to_string()
    } else if marcados == 0 {
        "Nenhum som".to_string()
    } else if marcados == 1 {
        "1 aplicativo".to_string()
    } else {
        format!("{marcados} aplicativos")
    };
    let botao_sons = button(
        row![
            text(resumo).size(13).width(Length::Fill),
            crate::ui::efeitos::ico(
                if cfg.som_aberto {
                    icones::seta_cima()
                } else {
                    icones::seta_baixo()
                },
                14.0,
                c.texto_fraco,
            ),
        ]
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding([8, 12])
    .style(move |_, st| button::Style {
        background: Some(Background::Color(c.elevado)),
        text_color: c.texto,
        border: Border {
            color: if cfg.som_aberto || matches!(st, button::Status::Hovered) {
                c.destaque
            } else {
                c.borda
            },
            width: 1.0,
            radius: tema::RAIO.into(),
        },
        ..Default::default()
    })
    .on_press(Msg::AlternaSomTela);
    let apps = botao_sons;
    let lembrada = wayland && crate::portal_tela::tem_escolha_lembrada();
    let aviso_fonte = if wayland {
        if lembrada {
            "Vai a mesma tela ou janela da última vez, sem perguntar."
        } else {
            "Ao clicar em Transmitir, o sistema pergunta qual tela ou janela. Depois ele lembra a escolha."
        }
    } else {
        "Ao clicar em Transmitir, a tela principal vai para a chamada."
    };
    let corpo = column![
        row![
            svg(icones::tela())
                .width(22)
                .height(22)
                .style(tema::icone_cor(c.destaque)),
            column![
                text(if cfg.ajustando {
                    "Configurações da transmissão"
                } else {
                    "Compartilhar tela"
                })
                .size(20)
                .font(negrito()),
                text(format!("Para {destino}")).size(13).style(fraco),
            ]
            .spacing(2)
            .width(Length::Fill),
            fechar,
        ]
        .spacing(12)
        .align_y(Alignment::Center),
        rotulo("SOM QUE VAI JUNTO"),
        apps,
        text("O som da chamada nunca vai junto, então ninguém escuta eco.")
            .size(12)
            .style(fraco),
        rotulo("QUALIDADE"),
        secao_qualidade(cfg.perfil),
        checkbox(cfg.auto_bitrate)
            .label("Ajustar a qualidade sozinho quando a rede piorar")
            .on_toggle(Msg::AlternaAutoBitrate)
            .size(16)
            .text_size(13),
        if cfg.ajustando {
            Element::from(space().height(0))
        } else {
            text(aviso_fonte).size(12).style(fraco).into()
        },
        if lembrada && !cfg.ajustando {
            button(text("Escolher outra tela ou janela").size(12))
                .padding([6, 12])
                .style(tema::botao_secundario)
                .on_press(Msg::EsqueceTelaLembrada)
                .into()
        } else {
            Element::from(space().height(0))
        },
        row![
            space().width(Length::Fill),
            button(text("Cancelar").size(14))
                .padding([8, 16])
                .style(tema::botao_secundario)
                .on_press(Msg::CancelaTela),
            button(
                text(if cfg.ajustando {
                    "Aplicar"
                } else {
                    "Transmitir"
                })
                .size(14)
            )
            .padding([8, 16])
            .style(tema::botao_primario)
            .on_press(Msg::ConfirmaTela),
        ]
        .spacing(8),
    ]
    .spacing(12);
    let janela = container(corpo)
        .padding(20)
        .width(460)
        .style(move |_| container::Style {
            background: Some(Background::Color(c.lateral)),
            border: Border {
                color: c.borda,
                width: 1.0,
                radius: tema::RAIO_GRANDE.into(),
            },
            ..Default::default()
        });
    if !cfg.som_aberto {
        return janela.into();
    }
    // Popup flutuante logo abaixo do botão de som: nada empurra o resto da janela.
    let fora =
        iced::widget::mouse_area(container(space()).width(Length::Fill).height(Length::Fill))
            .on_press(Msg::AlternaSomTela);
    let popup = container(iced::widget::opaque(popup_sons(cfg)))
        .width(Length::Fill)
        .padding(iced::Padding {
            top: 150.0,
            left: 20.0,
            right: 20.0,
            bottom: 0.0,
        });
    iced::widget::stack![janela, fora, popup].into()
}

fn popup_sons<'a>(cfg: &'a ConfigCompartilharTela) -> Element<'a, Msg> {
    let c = tema::cores();
    let mut lista = Column::new().spacing(6);
    lista = lista.push(
        checkbox(cfg.todo_pc)
            .label("Todo o PC (menos este app e a chamada)")
            .on_toggle(|_| Msg::AlternaTodoPc)
            .size(16)
            .text_size(14),
    );
    if cfg.apps.is_empty() {
        lista = lista.push(
            text("Nenhum aplicativo tocando som agora.")
                .size(13)
                .style(fraco),
        );
    }
    for (i, (app, marcado)) in cfg.apps.iter().enumerate() {
        let rot = if app.midia.is_empty() {
            app.nome.clone()
        } else {
            format!("{} — {}", app.nome, app.midia)
        };
        let mut cb = checkbox(*marcado && !cfg.todo_pc)
            .label(rot)
            .size(16)
            .text_size(14);
        if !cfg.todo_pc {
            cb = cb.on_toggle(move |v| Msg::MarcaApp(i, v));
        }
        lista = lista.push(cb);
    }
    container(iced::widget::scrollable(lista).height(Length::Shrink))
        .padding(12)
        .max_height(240)
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(c.elevado)),
            border: Border {
                color: c.borda,
                width: 1.0,
                radius: tema::RAIO.into(),
            },
            shadow: iced::Shadow {
                color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.4),
                offset: iced::Vector::new(0.0, 4.0),
                blur_radius: 14.0,
            },
            ..Default::default()
        })
        .into()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn comeca_com_o_perfil_padrao() {
        let c = ConfigCompartilharTela::default();
        assert_eq!(c.perfil, PerfilTela::PADRAO);
        assert!(c.auto_bitrate && c.apps.is_empty());
    }
}
