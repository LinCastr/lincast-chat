//! Painel de vídeos: grade, foco em quem fala e tela cheia (M9).

use std::collections::HashSet;

use iced::widget::{
    Column, button, column, container, image, mouse_area, row, scrollable, space, stack, svg, text,
};
use iced::{Alignment, Element, Length};

use crate::{Msg, Pessoa, Video, fraco, icones, negrito, tema, ui};

/// O quadro atual sobre o anterior. O iced sobe imagem grande para a GPU em
/// segundo plano e, no primeiro desenho dela, não mostra nada: o anterior (já na
/// GPU) cobre esse buraco, senão o vídeo pisca a cada quadro novo.
fn quadro_de_video<'a>(v: &Video, largura: Length, altura: Length) -> Element<'a, Msg> {
    let camada = |h: &image::Handle| {
        image(h.clone())
            .width(largura)
            .height(altura)
            .content_fit(iced::ContentFit::Contain)
    };
    match &v.anterior {
        Some(anterior) => stack![camada(anterior), camada(&v.imagem)].into(),
        None => camada(&v.imagem).into(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModoVideo {
    #[default]
    Grade,
    Foco,
}

pub fn escolhe_destaque<'a>(
    videos: &'a [Video],
    fixado: Option<&(String, bool)>,
    ultimo_falante: Option<&str>,
) -> &'a Video {
    if let Some((dono_fix, tela_fix)) = fixado
        && let Some(v) = videos
            .iter()
            .find(|v| v.dono == *dono_fix && v.de_tela == *tela_fix)
    {
        return v;
    }
    if let Some(falante) = ultimo_falante
        && let Some(v) = videos.iter().find(|v| v.dono == falante)
    {
        return v;
    }
    if let Some(v) = videos.iter().find(|v| v.de_tela) {
        return v;
    }
    &videos[0]
}

/// Quais transmissões a pessoa já pediu para assistir e se os controles da
/// tela cheia estão à mostra (somem depois de uns 2s sem mexer o mouse).
pub struct EstadoTransmissao<'a> {
    pub assistindo: &'a HashSet<(String, bool)>,
    pub controles_visiveis: bool,
    /// A transmissão que ocupa a janela inteira, se houver.
    pub tela_cheia: Option<&'a (String, bool)>,
    /// A pessoa escondeu a prévia da própria tela.
    pub ocultar_propria: bool,
    /// Quantas pessoas assistem a minha transmissão.
    pub espectadores: usize,
}

/// A transmissão de tela dos outros só toca depois do clique; a sua própria
/// aparece sempre.
fn esta_assistindo(v: &Video, meu_id: &str, est: &EstadoTransmissao<'_>) -> bool {
    if v.de_tela && v.dono == meu_id {
        return !est.ocultar_propria;
    }
    !v.de_tela || est.assistindo.contains(&(v.dono.clone(), v.de_tela))
}

pub fn renderiza_videos<'a>(
    videos: &'a [Video],
    pessoas: &'a [Pessoa],
    meu_id: &'a str,
    modo: ModoVideo,
    fixado: Option<&(String, bool)>,
    ultimo_falante: Option<&str>,
    est: &EstadoTransmissao<'a>,
) -> Element<'a, Msg> {
    if videos.is_empty() {
        return container(space().width(0).height(0)).into();
    }

    if let Some((dono_tc, tela_tc)) = est.tela_cheia
        && let Some(v) = videos
            .iter()
            .find(|v| v.dono == *dono_tc && v.de_tela == *tela_tc)
    {
        return renderiza_tela_cheia(v, pessoas, meu_id, est.controles_visiveis);
    }

    if modo == ModoVideo::Foco && videos.len() > 1 {
        let destaque_vid = escolhe_destaque(videos, fixado, ultimo_falante);
        let mut corpo = Column::new().spacing(8);

        corpo = corpo.push(renderiza_caixa_video(destaque_vid, pessoas, meu_id, est));

        let mut fila_miniaturas = row![].spacing(8);
        for v in videos {
            if v.dono == destaque_vid.dono && v.de_tela == destaque_vid.de_tela {
                continue;
            }
            fila_miniaturas = fila_miniaturas.push(renderiza_miniatura(v, pessoas, meu_id, est));
        }
        corpo = corpo.push(scrollable(container(fila_miniaturas).padding([4, 0])));

        return container(corpo)
            .padding([8, 16])
            .height(Length::FillPortion(4))
            .into();
    }

    let mut fila = row![].spacing(10);
    for v in videos {
        fila = fila.push(renderiza_caixa_video(v, pessoas, meu_id, est));
    }
    container(fila)
        .padding([12, 16])
        .height(Length::FillPortion(3))
        .into()
}

pub(crate) fn renderiza_caixa_video<'a>(
    v: &'a Video,
    pessoas: &'a [Pessoa],
    meu_id: &'a str,
    est: &EstadoTransmissao<'a>,
) -> Element<'a, Msg> {
    let quem = pessoas
        .iter()
        .find(|p| p.id == v.dono)
        .map(|p| p.nome.as_str())
        .unwrap_or("Amigo");
    let rotulo_txt = if v.dono == meu_id {
        if v.de_tela {
            match est.espectadores {
                0 => "Sua tela".into(),
                n => format!("Sua tela · {n} assistindo"),
            }
        } else {
            "Sua câmera".into()
        }
    } else if v.de_tela {
        format!("Tela de {quem}")
    } else {
        format!("Câmera de {quem}")
    };

    let falando = pessoas
        .iter()
        .find(|p| p.id == v.dono)
        .map(|p| p.falando)
        .unwrap_or(false);

    // Volume fica no clique direito da transmissão (por pessoa); aqui só o rótulo.
    let info_col = Column::new().spacing(4).push(
        text(format!("{rotulo_txt} · {}×{}", v.largura, v.altura))
            .size(12)
            .style(fraco),
    );

    let assistindo = esta_assistindo(v, meu_id, est);
    let conteudo: Element<'a, Msg> = if assistindo {
        let img = mouse_area(quadro_de_video(v, Length::Fill, Length::Fill))
            .on_press(Msg::AlternaFixacaoVideo(v.dono.clone(), v.de_tela));
        let pode_tela_cheia = !(v.de_tela && v.dono == meu_id);
        let botao_tela_cheia = crate::ui::efeitos::com_dica(
            crate::botao_icone(
                icones::maximizar(),
                false,
                false,
                Some(Msg::AlternaTelaCheiaVideo(Some((
                    v.dono.clone(),
                    v.de_tela,
                )))),
            ),
            "Tela cheia",
            iced::widget::tooltip::Position::Top,
        );
        let botao_exclusiva = crate::ui::efeitos::com_dica(
            crate::botao_icone(
                icones::expandir(),
                false,
                false,
                Some(Msg::TelaCheiaExclusiva(v.dono.clone(), v.de_tela)),
            ),
            "Tela cheia exclusiva",
            iced::widget::tooltip::Position::Top,
        );
        let tela_cheia = container(row![botao_exclusiva, botao_tela_cheia].spacing(4))
            .width(Length::Fill)
            .align_x(Alignment::End)
            .padding(8);
        if pode_tela_cheia {
            stack![img, tela_cheia].into()
        } else {
            img.into()
        }
    } else {
        // Retângulo cinza: o vídeo só aparece (e só gasta rede na tela) depois
        // do clique.
        let c = tema::cores();
        container(
            button(text("Assistir transmissão").size(14).font(negrito()))
                .padding([10, 18])
                .style(tema::botao_primario)
                .on_press(Msg::AssisteTransmissao(v.dono.clone(), v.de_tela)),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(c.forte)),
            ..Default::default()
        })
        .into()
    };
    let moldura = container(
        container(conteudo)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(tema::moldura_video),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(tema::anel_fala(falando));

    mouse_area(column![moldura, info_col].spacing(6).width(Length::Fill))
        .on_right_press(Msg::AbreMenuTransmissao(v.dono.clone(), v.de_tela))
        .into()
}

pub(crate) fn renderiza_miniatura<'a>(
    v: &'a Video,
    pessoas: &'a [Pessoa],
    meu_id: &'a str,
    est: &EstadoTransmissao<'a>,
) -> Element<'a, Msg> {
    let quem = pessoas
        .iter()
        .find(|p| p.id == v.dono)
        .map(|p| p.nome.as_str())
        .unwrap_or("Amigo");
    let rotulo_txt = if v.dono == meu_id {
        if v.de_tela { "Sua tela" } else { "Sua câmera" }
    } else {
        quem
    };

    let falando = pessoas
        .iter()
        .find(|p| p.id == v.dono)
        .map(|p| p.falando)
        .unwrap_or(false);

    let miolo: Element<'a, Msg> = if esta_assistindo(v, meu_id, est) {
        quadro_de_video(v, Length::Fixed(160.0), Length::Fixed(90.0))
    } else {
        container(text("Transmissão").size(12).style(fraco))
            .width(160)
            .height(90)
            .center_x(160)
            .center_y(90)
            .into()
    };
    let img = container(miolo).style(tema::moldura_video);

    let botao = button(
        column![
            container(img).style(tema::anel_fala(falando)),
            text(rotulo_txt).size(11).style(fraco),
        ]
        .spacing(3)
        .align_x(Alignment::Center),
    )
    .padding(2)
    .style(tema::botao_secundario)
    .on_press(Msg::AlternaFixacaoVideo(v.dono.clone(), v.de_tela));
    mouse_area(botao)
        .on_right_press(Msg::AbreMenuTransmissao(v.dono.clone(), v.de_tela))
        .into()
}

fn renderiza_tela_cheia<'a>(
    v: &'a Video,
    pessoas: &'a [Pessoa],
    meu_id: &'a str,
    controles_visiveis: bool,
) -> Element<'a, Msg> {
    let quem = pessoas
        .iter()
        .find(|p| p.id == v.dono)
        .map(|p| p.nome.as_str())
        .unwrap_or("Amigo");
    let rotulo_txt = if v.dono == meu_id {
        if v.de_tela {
            "Sua tela".into()
        } else {
            "Sua câmera".into()
        }
    } else if v.de_tela {
        format!("Tela de {quem}")
    } else {
        format!("Câmera de {quem}")
    };

    let falando = pessoas
        .iter()
        .find(|p| p.id == v.dono)
        .map(|p| p.falando)
        .unwrap_or(false);

    let barra_topo = row![
        text(format!("{rotulo_txt} · {}×{}", v.largura, v.altura))
            .size(14)
            .font(negrito()),
        text(" (Tela cheia — Esc para sair)").size(12).style(fraco),
        space().width(Length::Fill),
        button(
            row![
                svg(icones::minimizar()).width(14).height(14),
                text("Sair da tela cheia").size(12),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .padding([6, 12])
        .style(tema::botao_primario)
        .on_press(Msg::AlternaTelaCheiaVideo(None)),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let img_grande = container(quadro_de_video(v, Length::Fill, Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(tema::moldura_video);

    let quadro = container(img_grande)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(tema::anel_fala(falando));
    let mut corpo = Column::new().spacing(8);
    if controles_visiveis {
        corpo = corpo.push(barra_topo);
    }
    corpo = corpo.push(quadro);

    if controles_visiveis
        && v.dono != meu_id
        && let Some(p) = pessoas.iter().find(|p| p.id == v.dono)
    {
        let (vol, mudo) = if v.de_tela {
            (p.volume_tela, p.volume_tela <= 0.0)
        } else {
            (p.volume, p.volume <= 0.0)
        };
        corpo = corpo.push(
            container(ui::volume::controle_volume_video(
                v.dono.clone(),
                v.de_tela,
                vol,
                mudo,
            ))
            .padding([4, 8])
            .style(tema::painel_lateral_reto),
        );
    }

    container(corpo)
        .padding(12)
        .width(Length::Fill)
        .height(Length::FillPortion(4))
        .style(tema::painel_principal)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_video(dono: &str, de_tela: bool) -> Video {
        Video {
            dono: dono.to_string(),
            de_tela,
            largura: 1280,
            altura: 720,
            imagem: image::Handle::from_rgba(1, 1, vec![0, 0, 0, 255]),
            anterior: None,
        }
    }

    #[test]
    fn escolhe_destaque_ordem_de_prioridade() {
        let v1 = mock_video("p1", false);
        let v2 = mock_video("p2", true);
        let v3 = mock_video("p3", false);
        let lista = [v1, v2, v3];

        // 1. Manual fixado tem prioridade máxima
        let fixado = Some(("p3".to_string(), false));
        let dest = escolhe_destaque(&lista, fixado.as_ref(), Some("p1"));
        assert_eq!(dest.dono, "p3");
        assert!(!dest.de_tela);

        // 2. Último falante tem prioridade sobre tela e fallback
        let dest = escolhe_destaque(&lista, None, Some("p1"));
        assert_eq!(dest.dono, "p1");

        // 3. Tela tem prioridade sobre fallback quando não há falante recente
        let dest = escolhe_destaque(&lista, None, None);
        assert_eq!(dest.dono, "p2");
        assert!(dest.de_tela);

        // 4. Sem tela e sem falante: primeiro vídeo
        let v_cam1 = mock_video("c1", false);
        let v_cam2 = mock_video("c2", false);
        let cams = [v_cam1, v_cam2];
        let dest = escolhe_destaque(&cams, None, None);
        assert_eq!(dest.dono, "c1");
    }
}
