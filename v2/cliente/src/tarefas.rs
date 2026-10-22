//! Tarefas assíncronas auxiliares (rede em segundo plano, GIFs).

use super::*;

pub(crate) fn em_segundo_plano<T: Send + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
    msg: impl Fn(T) -> Msg + Send + 'static,
) -> Task<Msg> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(f)
                .await
                .expect("tarefa de fundo morreu")
        },
        msg,
    )
}

/// Busca uma foto de perfil/ícone: primeiro no disco, depois na rede.
pub(crate) fn baixa_imagem_de_perfil(
    srv: Servidor,
    token: Option<String>,
    url: String,
) -> Task<Msg> {
    em_segundo_plano(
        move || {
            let bytes = match avatar::le_do_disco(&url) {
                Some(b) => Ok(b),
                None => {
                    api::baixa_imagem(&srv, token.as_deref(), &url).map_err(|e| format!("{e:#}"))
                }
            };
            (url, bytes)
        },
        |(url, bytes)| Msg::ImagemBaixada { url, bytes },
    )
}

pub(crate) fn baixa_gif(msg_id: String, url: String) -> Task<Msg> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let ag: ureq::Agent = ureq::Agent::config_builder()
                    .timeout_global(Some(std::time::Duration::from_secs(10)))
                    .build()
                    .into();
                let r = ag.get(&url).call();
                match r {
                    Ok(mut resp) => {
                        let mut bytes = Vec::new();
                        use std::io::Read;
                        let _ = resp
                            .body_mut()
                            .as_reader()
                            .take(10 * 1024 * 1024)
                            .read_to_end(&mut bytes);
                        Some((msg_id, bytes))
                    }
                    Err(e) => {
                        log::warn!("[gif] falha ao baixar {url}: {e}");
                        None
                    }
                }
            })
            .await
            .ok()
            .flatten()
        },
        |res| match res {
            Some((id, bytes)) => Msg::GifBaixado { id, bytes },
            None => Msg::Nenhum,
        },
    )
}

pub(crate) fn baixa_previa_gif(url: String, previa_url: String) -> Task<Msg> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let ag: ureq::Agent = ureq::Agent::config_builder()
                    .timeout_global(Some(std::time::Duration::from_secs(8)))
                    .build()
                    .into();
                let r = ag.get(&previa_url).call();
                match r {
                    Ok(mut resp) => {
                        let mut bytes = Vec::new();
                        use std::io::Read;
                        let _ = resp
                            .body_mut()
                            .as_reader()
                            .take(2 * 1024 * 1024)
                            .read_to_end(&mut bytes);
                        Some((url, bytes))
                    }
                    Err(_) => None,
                }
            })
            .await
            .ok()
            .flatten()
        },
        |res| match res {
            Some((url, bytes)) => Msg::PreviaGifBaixada { url, bytes },
            None => Msg::Nenhum,
        },
    )
}

/// Busca (disco, senão rede) e decodifica um cosmético; guarda no cache
/// global e avisa para redesenhar.
pub(crate) fn baixa_cosmetico(srv: Servidor, url: String, animar: bool) -> Task<Msg> {
    em_segundo_plano(
        move || {
            let bytes = match crate::cosmeticos::le_do_disco(&url) {
                Some(b) => Ok(b),
                None => api::baixa_imagem(&srv, None, &url)
                    .inspect(|b| crate::cosmeticos::grava_no_disco(&url, b))
                    .map_err(|e| format!("{e:#}")),
            };
            let r = bytes.and_then(|b| {
                if url.contains("/mini/") {
                    crate::cosmeticos::calcula_cor(&url, &b);
                }
                crate::cosmeticos::decodifica(&url, &b, animar)
            });
            crate::cosmeticos::guarda(&url, r);
        },
        |()| Msg::CosmeticoPronto,
    )
}

pub(crate) fn carrega_cosmeticos(srv: Servidor, token: Option<String>) -> Task<Msg> {
    let s2 = srv.clone();
    let cat = em_segundo_plano(
        move || api::catalogo_cosmeticos(&s2).map_err(|e| format!("{e:#}")),
        Msg::CatalogoCosmeticos,
    );
    let Some(t) = token else { return cat };
    let pessoas = em_segundo_plano(
        move || api::cosmeticos_pessoas(&srv, &t).map_err(|e| format!("{e:#}")),
        Msg::PessoasCosmeticos,
    );
    Task::batch([cat, pessoas])
}
