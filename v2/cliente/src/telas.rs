//! Telas de fora da sala (login, salas) e pequenos widgets compartilhados.

use super::*;
use iced::widget::column;

/// `Err` com o motivo quando não deu pra perguntar ao PipeWire — que é
/// diferente de "ninguém tocando", e dizer a segunda coisa no lugar da
/// primeira esconde o defeito (aconteceu com um concord-audio desatualizado).
pub(crate) fn lista_apps_tocando() -> Result<Vec<AppTocando>, String> {
    let eu = std::env::current_exe().map_err(|e| e.to_string())?;
    let bin = eu.with_file_name("concord-audio");
    let saida = std::process::Command::new(&bin)
        .args(["listar", "--json"])
        .output()
        .map_err(|e| format!("não consegui rodar {}: {e}", bin.display()))?;
    if !saida.status.success() {
        let erro = String::from_utf8_lossy(&saida.stderr);
        return Err(format!(
            "o concord-audio falhou: {}",
            erro.lines()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("sem detalhe")
        ));
    }
    Ok(String::from_utf8_lossy(&saida.stdout)
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .map(|v| AppTocando {
            nome: v["nome"].as_str().unwrap_or("").into(),
            binario: v["binario"].as_str().unwrap_or("").into(),
            midia: v["midia"].as_str().unwrap_or("").into(),
        })
        // O próprio app nunca aparece: escolher ele é justamente o erro
        // que faz a voz da sala voltar na transmissão.
        .filter(|a| !a.binario.starts_with("concord") && !a.binario.starts_with("lincast"))
        // Um app com dois fluxos de som (abas do navegador, por exemplo)
        // aparece uma vez só: a captura é por nome e pega todos eles.
        .fold(Vec::<AppTocando>::new(), |mut lista, a| {
            if !lista
                .iter()
                .any(|x| x.nome == a.nome && x.binario == a.binario)
            {
                lista.push(a);
            }
            lista
        }))
}

// --- telas de fora da sala --------------------------------------------------

pub(crate) fn tela_login<'a>(l: &'a Login, srv: &'a Servidor) -> Element<'a, Msg> {
    let c = tema::cores();
    let mut col = column![
        image(image::Handle::from_bytes(
            include_bytes!("../assets/logo-96.png").as_slice()
        ))
        .width(56)
        .height(56),
        space().height(4),
        text("Lincast Chat").size(24).font(negrito()),
    ]
    .spacing(6)
    .width(320);

    if l.aguardando_discord {
        col = col.push(space().height(6));
        col = col.push(
            row![
                svg(icones::discord())
                    .width(24)
                    .height(24)
                    .style(tema::icone_cor(c.destaque)),
                text("Autorização com Discord").size(15).font(negrito()),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );
        col = col.push(
            text("Uma aba foi aberta no seu navegador padrão para autorizar.\nAssim que você confirmar lá, o app entrará sozinho.")
                .size(13)
                .style(fraco),
        );
        col = col.push(space().height(8));
        col = col.push(
            button(center(text("Cancelar").size(14)).height(20))
                .width(Length::Fill)
                .padding(9)
                .style(tema::botao_secundario)
                .on_press(Msg::CancelaDiscordLogin),
        );
    } else {
        let titulo = if l.criando { "Criar conta" } else { "Entrar" };
        let pode = !l.enviando
            && !l.usuario.trim().is_empty()
            && l.senha.len() >= if l.criando { 8 } else { 1 };

        col = col.push(text(titulo).size(15).style(fraco));
        col = col.push(space().height(8));
        col = col.push(rotulo("Nome de usuário"));
        col = col.push(
            text_input("", &l.usuario)
                .on_input(Msg::Usuario)
                .on_submit(Msg::Entrar)
                .padding(10)
                .size(14)
                .style(tema::campo),
        );
        col = col.push(rotulo(if l.criando {
            "Senha (8 caracteres ou mais)"
        } else {
            "Senha"
        }));
        col = col.push(
            text_input("", &l.senha)
                .secure(true)
                .on_input(Msg::Senha)
                .on_submit(Msg::Entrar)
                .padding(10)
                .size(14)
                .style(tema::campo),
        );
        col = col.push(space().height(4));
        col = col.push(
            button(center(text(if l.enviando { "Aguarde…" } else { titulo }).size(14)).height(20))
                .width(Length::Fill)
                .padding(9)
                .style(tema::botao_primario)
                .on_press_maybe(pode.then_some(Msg::Entrar)),
        );
        col = col.push(
            button(
                text(if l.criando {
                    "Já tenho conta"
                } else {
                    "Criar uma conta"
                })
                .size(13),
            )
            .padding([4, 0])
            .style(|t, s| {
                let mut e = tema::item_de_lista(t, s);
                e.background = None;
                e.text_color = tema::cores().texto_fraco;
                e
            })
            .on_press(Msg::AlternaCriar),
        );
        col = col.push(linha_horizontal());
        col = col.push(
            button(
                row![
                    svg(icones::discord()).width(18).height(18),
                    text("Entrar com Discord").size(14).font(negrito()),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .padding([9, 14])
            .style(tema::botao_secundario)
            .on_press(Msg::IniciaDiscordLogin),
        );
    }
    if let Some(e) = &l.erro {
        col = col.push(aviso(e.clone(), None));
    }
    let host = srv
        .base
        .split("://")
        .nth(1)
        .unwrap_or(&srv.base)
        .to_string();
    let rodape = text(format!("Servidor: {host}")).size(12).style(fraco);
    container(column![center(col), container(rodape).padding(14)])
        .width(Length::Fill)
        .height(Length::Fill)
        .style(tema::painel_principal)
        .into()
}

/// Nome que a conversa mostra: o do grupo, ou o da outra pessoa na direta.
pub(crate) fn nome_da_conversa(c: &avisos::Conversa, meu_usuario: Option<&str>) -> String {
    if c.tipo == "grupo" {
        return if c.nome.is_empty() {
            "Grupo".into()
        } else {
            c.nome.clone()
        };
    }
    c.membros
        .iter()
        .find(|m| m.usuario.as_deref() != meu_usuario)
        .map(|m| m.name.clone())
        .unwrap_or_else(|| "Conversa".into())
}

/// Tela de fora de qualquer conversa: a coluna da esquerda e, ao lado, os
/// amigos ou um convite para escolher uma conversa.
pub(crate) fn tela_salas<'a>(s: &'a Salas, coluna: Element<'a, Msg>) -> Element<'a, Msg> {
    let central: Element<'a, Msg> = match s.aba {
        AbaSalas::Amigos => {
            ui::amigos::painel_amigos(s.amigos.as_ref(), &s.estado_amigos, s.carregando_amigos)
        }
        AbaSalas::Salas => {
            let mut c =
                column![
                text("Escolha uma conversa").size(18).font(negrito()),
                text("Abra uma conversa na coluna da esquerda, ou comece uma nova pela aba Amigos.")
                    .size(13)
                    .style(fraco),
            ]
                .spacing(8)
                .align_x(Alignment::Center);
            if let Some(r) = &s.recusa {
                c = c.push(aviso(r.clone(), None));
            }
            container(center(c))
                .width(Length::Fill)
                .height(Length::Fill)
                .style(tema::painel_principal)
                .into()
        }
    };
    row![coluna, linha_vertical(), central].into()
}

// --- peças pequenas ---------------------------------------------------------

pub(crate) fn negrito() -> Font {
    Font {
        weight: font::Weight::Semibold,
        ..crate::tema::FONTE
    }
}

pub(crate) fn fraco(_: &iced::Theme) -> text::Style {
    text::Style {
        color: Some(tema::cores().texto_fraco),
    }
}

pub(crate) fn destaque(_: &iced::Theme) -> text::Style {
    text::Style {
        color: Some(tema::cores().destaque),
    }
}

pub(crate) fn texto_perigo(_: &iced::Theme) -> text::Style {
    text::Style {
        color: Some(tema::cores().perigo),
    }
}

pub(crate) fn rotulo(t: &str) -> Element<'_, Msg> {
    text(t).size(12).style(fraco).into()
}

pub(crate) fn linha_horizontal<'a>() -> Element<'a, Msg> {
    container(space())
        .width(Length::Fill)
        .height(1)
        .style(tema::divisoria)
        .into()
}

pub(crate) fn linha_vertical<'a>() -> Element<'a, Msg> {
    container(space())
        .width(1)
        .height(Length::Fill)
        .style(tema::divisoria)
        .into()
}

pub(crate) fn botao_icone<'a>(
    icone: svg::Handle,
    ligado: bool,
    perigoso: bool,
    msg: Option<Msg>,
) -> Element<'a, Msg> {
    let c = tema::cores();
    let cor = if perigoso {
        c.perigo
    } else if ligado {
        c.destaque
    } else {
        c.texto_fraco
    };
    button(svg(icone).width(18).height(18).style(tema::icone_cor(cor)))
        .padding(7)
        .style(tema::botao_icone(ligado, perigoso))
        .on_press_maybe(msg)
        .into()
}

pub(crate) fn aviso<'a>(t: String, fechar: Option<Msg>) -> Element<'a, Msg> {
    let c = tema::cores();
    let mut r = row![
        svg(icones::alerta())
            .width(16)
            .height(16)
            .style(tema::icone_cor(c.perigo)),
        text(t).size(13).width(Length::Fill),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    if let Some(m) = fechar {
        r = r.push(crate::ui::efeitos::com_dica(
            botao_icone(icones::fechar(), false, false, Some(m)),
            "Fechar",
            iced::widget::tooltip::Position::Top,
        ));
    }
    container(r)
        .padding([8, 10])
        .width(Length::Fill)
        .style(tema::aviso_erro)
        .into()
}
