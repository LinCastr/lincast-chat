//! Módulo de detecção nativa de atividade no Linux (S9).
//!
//! Detecta jogos (Steam AppId, atalhos não-Steam em shortcuts.vdf, jogos nativos
//! via cgroup systemd e .desktop com Categories=Game, executáveis Wine/Proton) e
//! músicas em reprodução via MPRIS no D-Bus com proteção contra falsos positivos
//! de vídeos do YouTube.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::api::{Atividade, JogoAtividade, MusicaAtividade};

/// Executáveis genéricos que não identificam um jogo (infraestrutura do Wine/Proton e runtimes).
pub const EXES_QUE_NAO_SAO_JOGO: &[&str] = &[
    "start.exe",
    "explorer.exe",
    "services.exe",
    "winedevice.exe",
    "plugplay.exe",
    "rpcss.exe",
    "svchost.exe",
    "tabtip.exe",
    "wineboot.exe",
    "winemenubuilder.exe",
    "conhost.exe",
    "cmd.exe",
    "rundll32.exe",
    "winecfg.exe",
    "regedit.exe",
    "control.exe",
    "taskmgr.exe",
    "notepad.exe",
    "iexplore.exe",
    "msiexec.exe",
    "steam.exe",
    "steamwebhelper.exe",
    "gameoverlayui.exe",
    "steamerrorreporter.exe",
    "steamservice.exe",
    "origin.exe",
    "eadesktop.exe",
    "epicgameslauncher.exe",
    "java.exe",
    "javaw.exe",
    "python.exe",
    "pythonw.exe",
    "mono.exe",
    "dotnet.exe",
    "launcher.exe",
    "game.exe",
    "main.exe",
    "client.exe",
    "run.exe",
    "play.exe",
    "setup.exe",
    "engine.exe",
    "update.exe",
    "updater.exe",
    "default.exe",
    "server.exe",
    "loader.exe",
    "unityplayer.exe",
    "electron.exe",
    "chrome.exe",
    "unins000.exe",
    "crashhandler.exe",
    "crashpad_handler.exe",
    "dxsetup.exe",
    "vcredist.exe",
];

/// Lançadores conhecidos que não devem ser anunciados como jogo.
pub const LANCADORES: &[&str] = &[
    "steam",
    "lutris",
    "heroic",
    "hgl",
    "bottles",
    "prismlauncher",
    "freesmlauncher",
    "modrinth-app",
    "r2modman",
    "protonplus",
    "protonup-qt",
    "pupgui2",
    "nexusmods-app",
    "app",
    "goverlay",
    "dlss-updater",
    "vinegar",
    "gamehub",
    "itch",
    "minigalaxy",
    "playonlinux",
    "winetricks",
    "gamemode",
    "mangohud",
    "antimicrox",
    "piper",
    "boilr",
    "steamtinkerlaunch",
    "cartridges",
    "gamescope",
    "olympus",
    "cloudredirect",
];

/// Mapeamento de identificadores de .desktop cujo nome oficial engana.
pub const NOMES_PROPRIOS: &[(&str, &str)] = &[("org.vinegarhq.Sober", "Roblox")];

/// Extrai o ID do escopo de aplicativo a partir do conteúdo de `/proc/<pid>/cgroup`.
///
/// Importante: `/proc/<pid>/cgroup` termina com quebra de linha `\n`. É necessário
/// fazer `.trim()` antes de quebrar em segmentos para que o regex/ancoragem case
/// corretamente e não deixe a camada de jogos nativos inativa.
pub fn id_do_escopo_de_app(cgroup: &str) -> Option<String> {
    for segmento in cgroup.trim().split('/') {
        let seg = segmento.trim();
        let sem_prefixo = if let Some(resto) = seg.strip_prefix("app-flatpak-") {
            resto
        } else if let Some(resto) = seg.strip_prefix("app-") {
            resto
        } else {
            continue;
        };

        let sufixo_valido = sem_prefixo.ends_with(".scope") || sem_prefixo.ends_with(".service");
        if !sufixo_valido {
            continue;
        }

        let base = if let Some(s) = sem_prefixo.strip_suffix(".scope") {
            s
        } else if let Some(s) = sem_prefixo.strip_suffix(".service") {
            s
        } else {
            continue;
        };

        // Remove sufixos como @instancia e -<pid>
        let sem_instancia = base.split('@').next().unwrap_or(base);
        let sem_pid = if let Some((corpo, suf)) = sem_instancia.rsplit_once('-') {
            if suf.chars().all(|c| c.is_ascii_digit()) {
                corpo
            } else {
                sem_instancia
            }
        } else {
            sem_instancia
        };

        // Desescapa sequências \x2d do systemd
        return Some(desescapa_hex(sem_pid));
    }
    None
}

/// Converte sequências como `\x2d` para o caractere correspondente (`-`).
pub fn desescapa_hex(s: &str) -> String {
    let mut resultado = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek() == Some(&'x') {
            chars.next(); // consome 'x'
            let h1 = chars.next();
            let h2 = chars.next();
            if let (Some(c1), Some(c2)) = (h1, h2) {
                let hex_str = format!("{c1}{c2}");
                if let Ok(b) = u8::from_str_radix(&hex_str, 16) {
                    resultado.push(b as char);
                    continue;
                }
                resultado.push('\\');
                resultado.push('x');
                resultado.push(c1);
                resultado.push(c2);
            } else {
                resultado.push('\\');
                resultado.push('x');
                if let Some(c1) = h1 {
                    resultado.push(c1);
                }
            }
        } else {
            resultado.push(c);
        }
    }
    resultado
}

/// Verifica se o id ou nome pertence a um lançador.
pub fn eh_lancador(id: &str, nome: Option<&str>) -> bool {
    let id_min = id.to_lowercase();
    let ultimo = id_min.split('.').next_back().unwrap_or(&id_min);
    for lanc in LANCADORES {
        if id_min == *lanc || ultimo == *lanc {
            return true;
        }
    }
    if id_min.contains("launcher") {
        return true;
    }
    if let Some(n) = nome
        && n.to_lowercase().contains("launcher")
    {
        return true;
    }
    false
}

/// Verifica se uma faixa MPRIS é um vídeo comum do YouTube e não música real.
pub fn eh_youtube_que_nao_e_musica(url: &str, arte: &str) -> bool {
    if !url.is_empty()
        && let Some(host) = url.split("://").nth(1).and_then(|s| s.split('/').next())
    {
        let host_min = host.to_lowercase();
        if host_min == "music.youtube.com" {
            return false;
        }
        if host_min.ends_with("youtube.com")
            || host_min.ends_with("youtube-nocookie.com")
            || host_min == "youtu.be"
        {
            return true;
        }
    }
    if arte.starts_with("http://i.ytimg.com/") || arte.starts_with("https://i.ytimg.com/") {
        return true;
    }
    false
}

/// Pastas padrão de instalação da Steam no Linux.
fn raizes_steam() -> Vec<PathBuf> {
    let mut raizes = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        let p_home = Path::new(&home);
        let candidatos = [
            p_home.join(".steam/steam"),
            p_home.join(".local/share/Steam"),
            p_home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
        ];
        for c in candidatos {
            if c.exists() {
                raizes.push(c);
            }
        }
    }
    raizes
}

/// Descobre todas as pastas de biblioteca da Steam (lendo libraryfolders.vdf).
fn bibliotecas_steam() -> Vec<PathBuf> {
    let mut bibliotecas = HashSet::new();
    for raiz in raizes_steam() {
        let steamapps = raiz.join("steamapps");
        if steamapps.exists() {
            bibliotecas.insert(raiz.clone());
        }
        let vdf = raiz.join("steamapps/libraryfolders.vdf");
        if let Ok(conteudo) = fs::read_to_string(&vdf) {
            for linha in conteudo.lines() {
                if linha.contains("\"path\"")
                    && let Some(caminho) = linha.split('"').nth(3)
                {
                    let p = PathBuf::from(caminho);
                    if p.exists() {
                        bibliotecas.insert(p);
                    }
                }
            }
        }
    }
    bibliotecas.into_iter().collect()
}

/// Busca o nome do jogo a partir do appmanifest_<id>.acf.
fn nome_do_app_steam(appid: &str) -> Option<String> {
    for lib in bibliotecas_steam() {
        let acf = lib.join(format!("steamapps/appmanifest_{appid}.acf"));
        if let Ok(conteudo) = fs::read_to_string(&acf) {
            for linha in conteudo.lines() {
                if linha.contains("\"name\"")
                    && let Some(nome) = linha.split('"').nth(3)
                {
                    return Some(nome.trim().to_string());
                }
            }
        }
    }
    None
}

/// Lê atalhos não-Steam do arquivo binário shortcuts.vdf da Steam.
fn atalhos_nao_steam() -> HashMap<u32, String> {
    let mut mapa = HashMap::new();
    for raiz in raizes_steam() {
        let userdata = raiz.join("userdata");
        let Ok(entradas) = fs::read_dir(userdata) else {
            continue;
        };
        for entry in entradas.flatten() {
            let config_vdf = entry.path().join("config/shortcuts.vdf");
            let Ok(bytes) = fs::read(&config_vdf) else {
                continue;
            };

            let mut i = 0;
            while i + 4 < bytes.len() {
                if bytes[i] == 0x02 {
                    let chave = b"appid\0";
                    if bytes[i + 1..].starts_with(chave) {
                        let offset = i + 1 + chave.len();
                        if offset + 4 <= bytes.len() {
                            let appid = u32::from_le_bytes(
                                bytes[offset..offset + 4].try_into().unwrap_or_default(),
                            );

                            let mut j = offset + 4;
                            while j + 8 < bytes.len() && j < offset + 500 {
                                if bytes[j] == 0x01 {
                                    let nome_chave = b"appname\0";
                                    let nome_chave_alt = b"AppName\0";
                                    if bytes[j + 1..].starts_with(nome_chave)
                                        || bytes[j + 1..].starts_with(nome_chave_alt)
                                    {
                                        let v_offset = j + 1 + nome_chave.len();
                                        if let Some(fim) =
                                            bytes[v_offset..].iter().position(|&b| b == 0)
                                            && let Ok(nome) = std::str::from_utf8(
                                                &bytes[v_offset..v_offset + fim],
                                            )
                                        {
                                            mapa.insert(appid, nome.trim().to_string());
                                            break;
                                        }
                                    }
                                }
                                j += 1;
                            }
                        }
                    }
                }
                i += 1;
            }
        }
    }
    mapa
}

/// Extrai o appid de atalho do valor de SteamGameId (bits altos).
pub fn appid_do_atalho(environ: &[u8]) -> Option<u32> {
    let env_str = std::str::from_utf8(environ).ok()?;
    for linha in env_str.split('\0') {
        if let Some(resto) = linha.strip_prefix("SteamGameId=")
            && let Ok(val) = resto.trim().parse::<u64>()
        {
            let alto = (val >> 32) as u32;
            if alto > 0 {
                return Some(alto);
            }
        }
    }
    None
}

/// Lê uma variável do ambiente NUL-delimited de `/proc/<pid>/environ`.
fn variavel_do_ambiente(environ: &[u8], nome: &str) -> Option<String> {
    let env_str = std::str::from_utf8(environ).ok()?;
    let prefixo = format!("{nome}=");
    for parte in env_str.split('\0') {
        if let Some(val) = parte.strip_prefix(&prefixo) {
            return Some(val.to_string());
        }
    }
    None
}

/// Monta o índice de jogos nativos a partir dos arquivos `.desktop` instalados.
pub fn indice_de_jogos_nativos() -> (HashMap<String, String>, HashMap<String, String>) {
    let mut por_id = HashMap::new();
    let mut por_exec = HashMap::new();

    let mut pastas = vec![
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/usr/local/share/applications"),
        PathBuf::from("/var/lib/flatpak/exports/share/applications"),
    ];
    if let Ok(home) = std::env::var("HOME") {
        let p_home = Path::new(&home);
        pastas.push(p_home.join(".local/share/applications"));
        pastas.push(p_home.join(".local/share/flatpak/exports/share/applications"));
    }

    for pasta in pastas {
        let Ok(entradas) = fs::read_dir(pasta) else {
            continue;
        };
        for entry in entradas.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("desktop") {
                continue;
            }
            let Ok(conteudo) = fs::read_to_string(&path) else {
                continue;
            };
            let Some(secao) = conteudo
                .split("[Desktop Entry]")
                .nth(1)
                .and_then(|s| s.split("\n[").next())
            else {
                continue;
            };

            let mut eh_jogo = false;
            let mut no_display = false;
            let mut nome: Option<String> = None;
            let mut execs = Vec::new();

            for linha in secao.lines() {
                let l = linha.trim();
                if let Some(cat) = l.strip_prefix("Categories=") {
                    let cats: Vec<&str> = cat.split(';').map(str::trim).collect();
                    // `Game` junto de categorias de ferramenta é rótulo errado (VPN, rede...).
                    let ferramenta = cats.iter().any(|c| {
                        [
                            "Network",
                            "Utility",
                            "System",
                            "Development",
                            "Office",
                            "Settings",
                        ]
                        .contains(c)
                    });
                    if cats.contains(&"Game") && !ferramenta {
                        eh_jogo = true;
                    }
                } else if l.eq_ignore_ascii_case("NoDisplay=true") {
                    no_display = true;
                } else if let Some(n) = l.strip_prefix("Name=") {
                    if nome.is_none() {
                        nome = Some(n.trim().to_string());
                    }
                } else if let Some(ex) = l
                    .strip_prefix("Exec=")
                    .or_else(|| l.strip_prefix("TryExec="))
                {
                    // Flatpak é identificado por FLATPAK_ID/cgroup, não pelos argumentos.
                    let eh_flatpak = ex
                        .split_whitespace()
                        .next()
                        .is_some_and(|t| t.ends_with("flatpak"));
                    if eh_flatpak {
                        continue;
                    }
                    for termo in ex.split_whitespace() {
                        if termo.starts_with('-')
                            || termo.contains('=')
                            || termo.contains('%')
                            || termo.contains('@')
                        {
                            continue;
                        }
                        if termo.starts_with('/') {
                            execs.extend(executaveis_do_script(termo));
                        }
                        if let Some(base) = termo.split('/').next_back() {
                            let base_min = base.to_lowercase();
                            if !base_min.is_empty()
                                && !base_min.chars().all(|c| c.is_ascii_digit())
                                && !EXECUTORES_GENERICOS.contains(&base_min.as_str())
                            {
                                execs.push(base_min);
                            }
                        }
                    }
                }
            }

            if !eh_jogo || no_display {
                continue;
            }
            let Some(nome_jogo) = nome else { continue };
            let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };

            if eh_lancador(id, Some(&nome_jogo)) {
                continue;
            }

            let nome_final = NOMES_PROPRIOS
                .iter()
                .find(|(proprio, _)| *proprio == id)
                .map(|(_, substituto)| substituto.to_string())
                .unwrap_or(nome_jogo);

            por_id.insert(id.to_string(), nome_final.clone());
            for ex in execs {
                por_exec.entry(ex).or_insert_with(|| nome_final.clone());
            }
        }
    }

    (por_id, por_exec)
}

/// Se `caminho` é um script curto (`#!`), devolve os executáveis absolutos que ele chama
/// (ex.: `/usr/bin/osu-lazer` chama `/opt/osu-lazer/osu.AppImage`), em minúsculas.
fn executaveis_do_script(caminho: &str) -> Vec<String> {
    let Ok(meta) = fs::metadata(caminho) else {
        return Vec::new();
    };
    if !meta.is_file() || meta.len() > 4096 {
        return Vec::new();
    }
    let Ok(texto) = fs::read_to_string(caminho) else {
        return Vec::new();
    };
    if !texto.starts_with("#!") {
        return Vec::new();
    }
    texto
        .split_whitespace()
        .filter(|t| t.starts_with('/') && !t.starts_with("/usr/bin/env"))
        .filter_map(|t| t.rsplit('/').next())
        .map(|b| b.trim_matches(['"', '\'']).to_lowercase())
        .filter(|b| !b.is_empty() && !EXECUTORES_GENERICOS.contains(&b.as_str()))
        .collect()
}

/// Interpretadores e envoltórios: nunca identificam um jogo sozinhos.
const EXECUTORES_GENERICOS: &[&str] = &[
    "sh",
    "bash",
    "env",
    "python",
    "python3",
    "game-run",
    "gamemoderun",
    "prime-run",
    "flatpak",
    "mangohud",
    "gamescope",
    "steam",
    "wine",
    "wine64",
    "proton",
    "dash",
    "zsh",
    "fish",
    "xdg-open",
];

/// O executável (nome do arquivo, minúsculo) é infraestrutura, não jogo?
pub fn exe_nao_e_jogo(exe: &str) -> bool {
    EXES_QUE_NAO_SAO_JOGO.contains(&exe) || EXECUTORES_GENERICOS.contains(&exe)
}

/// Detecta se há algum jogo em execução no Linux.
pub fn detecta_jogo_linux() -> Option<JogoAtividade> {
    let Ok(entradas_proc) = fs::read_dir("/proc") else {
        return None;
    };

    let mut candidato_atalho: Option<u32> = None;
    let mut processos_wine = Vec::new();
    let mut pids_cgroups = Vec::new();
    let detectaveis = crate::jogos_detectaveis::indice();
    let (jogos_por_id, jogos_por_exec) = indice_de_jogos_nativos();
    let mut nativo_por_exec: Option<String> = None;
    let mut nativo_flatpak: Option<String> = None;
    let mut detectado_discord: Option<String> = None;
    let mut exes_abertos: std::collections::HashSet<String> = std::collections::HashSet::new();

    for entry in entradas_proc.flatten() {
        let nome_pasta = entry.file_name();
        let Some(nome_str) = nome_pasta.to_str() else {
            continue;
        };
        if !nome_str.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }

        let pid_path = entry.path();
        if let Ok(environ) = fs::read(pid_path.join("environ")) {
            // 1. Steam AppId
            if let Some(appid) = variavel_do_ambiente(&environ, "SteamAppId")
                && appid != "0"
                && appid != "7"
                && let Some(nome) = nome_do_app_steam(&appid)
            {
                return Some(JogoAtividade {
                    nome,
                    origem: Some("steam".to_string()),
                    desde: None,
                    capa: None,
                });
            }

            // Flatpak: o app sabe o próprio id mesmo fora de um escopo systemd próprio.
            if nativo_flatpak.is_none()
                && let Some(fid) = variavel_do_ambiente(&environ, "FLATPAK_ID")
                && let Some(nome) = jogos_por_id.get(&fid)
            {
                nativo_flatpak = Some(nome.clone());
            }

            if candidato_atalho.is_none()
                && let Some(atalho_id) = appid_do_atalho(&environ)
            {
                candidato_atalho = Some(atalho_id);
            }
        }

        if let Ok(cgroup) = fs::read_to_string(pid_path.join("cgroup"))
            && let Some(id) = id_do_escopo_de_app(&cgroup)
        {
            pids_cgroups.push(id);
        }

        if let Ok(cmdline) = fs::read(pid_path.join("cmdline"))
            && let Some(primeiro) = cmdline.split(|&b| b == 0).next()
            && let Ok(cmd_str) = std::str::from_utf8(primeiro)
        {
            // Wine mostra caminho do Windows (`C:\windows\system32\services.exe`).
            let normalizado = crate::jogos_detectaveis::normaliza(cmd_str);
            let exe = normalizado.rsplit('/').next().unwrap_or("").to_string();
            exes_abertos.insert(exe.clone());
            let infra = normalizado.contains("/windows/system32/")
                || normalizado.contains("/windows/syswow64/")
                || EXES_QUE_NAO_SAO_JOGO.contains(&exe.as_str());
            if !infra {
                if nativo_por_exec.is_none() {
                    // argv[0] e, se for interpretador (`sh script`), o primeiro argumento.
                    let mut partes = cmdline.split(|&b| b == 0).filter(|p| !p.is_empty());
                    let argv0 = partes.next();
                    let eh_interpretador = argv0
                        .and_then(|a| std::str::from_utf8(a).ok())
                        .and_then(|a| a.rsplit('/').next())
                        .is_some_and(|b| EXECUTORES_GENERICOS.contains(&b));
                    let candidatos = [argv0, partes.next().filter(|_| eh_interpretador)];
                    for cand in candidatos.into_iter().flatten() {
                        let Ok(c) = std::str::from_utf8(cand) else {
                            continue;
                        };
                        let base = c.rsplit(['/', '\\']).next().unwrap_or("").to_lowercase();
                        if !EXECUTORES_GENERICOS.contains(&base.as_str())
                            && let Some(nome) = jogos_por_exec.get(&base)
                        {
                            nativo_por_exec = Some(nome.clone());
                            break;
                        }
                    }
                }
                if let Some(indice) = &detectaveis
                    && let Some(nome) = indice.acha(&normalizado)
                {
                    detectado_discord.get_or_insert_with(|| nome.to_string());
                }
                let utilitario = [
                    "launcher",
                    "vpn",
                    "update",
                    "helper",
                    "crash",
                    "setup",
                    "install",
                    "unins",
                    "service",
                    "tray",
                    "agent",
                    "bootstrap",
                ]
                .iter()
                .any(|t| exe.contains(t));
                let em_pasta_de_jogo = [
                    "/games/",
                    "steamapps",
                    "lutris",
                    "heroic",
                    "/epic games/",
                    "/gog",
                ]
                .iter()
                .any(|t| normalizado.to_lowercase().contains(t));
                if exe.ends_with(".exe") && !utilitario && em_pasta_de_jogo {
                    processos_wine.push(exe);
                }
            }
        }
    }

    // Programas que a pessoa registrou à mão em Configurações → Jogos registrados.
    if let Some(r) = crate::jogos_registrados::carrega()
        .into_iter()
        .find(|r| r.manual && r.exec.as_deref().is_some_and(|e| exes_abertos.contains(e)))
    {
        return Some(JogoAtividade {
            nome: r.nome,
            origem: Some("manual".to_string()),
            desde: None,
            capa: r.capa,
        });
    }

    // Atalho da Steam
    if let Some(appid) = candidato_atalho {
        let atalhos = atalhos_nao_steam();
        if let Some(nome) = atalhos.get(&appid) {
            return Some(JogoAtividade {
                nome: nome.clone(),
                origem: Some("atalho".to_string()),
                desde: None,
                capa: None,
            });
        }
    }

    // Jogos nativos pelo cgroup
    for id in pids_cgroups {
        // Lançadores (uwsm etc.) prefixam o id: `Hyprland-osu-lazer`.
        let achado = jogos_por_id.get(&id).or_else(|| {
            jogos_por_id
                .iter()
                .find(|(chave, _)| id.ends_with(&format!("-{chave}")))
                .map(|(_, v)| v)
        });
        if let Some(nome) = achado {
            return Some(JogoAtividade {
                nome: nome.clone(),
                origem: Some("nativo".to_string()),
                desde: None,
                capa: None,
            });
        }
    }
    if let Some(nome) = nativo_flatpak.or(nativo_por_exec) {
        return Some(JogoAtividade {
            nome,
            origem: Some("nativo".to_string()),
            desde: None,
            capa: None,
        });
    }

    // Lista de jogos detectáveis do Discord (nome oficial do jogo)
    if let Some(nome) = detectado_discord {
        return Some(JogoAtividade {
            nome,
            origem: Some("discord".to_string()),
            desde: None,
            capa: None,
        });
    }

    // Wine / Proton
    if let Some(exe_jogo) = processos_wine.into_iter().next() {
        let limpo = exe_jogo.strip_suffix(".exe").unwrap_or(&exe_jogo);
        let nome_formatado = limpo
            .split(['_', '-'])
            .map(|palavra| {
                let mut c = palavra.chars();
                match c.next() {
                    None => String::new(),
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ");

        return Some(JogoAtividade {
            nome: nome_formatado,
            origem: Some("wine".to_string()),
            desde: None,
            capa: None,
        });
    }

    None
}

/// Posição atual da faixa (ms) de um player MPRIS, se ele informar.
fn posicao_mpris(servico: &str) -> Option<u64> {
    let saida = Command::new("busctl")
        .args([
            "--user",
            "--json=short",
            "call",
            servico,
            "/org/mpris/MediaPlayer2",
            "org.freedesktop.DBus.Properties",
            "Get",
            "ss",
            "org.mpris.MediaPlayer2.Player",
            "Position",
        ])
        .output()
        .ok()?;
    if !saida.status.success() {
        return None;
    }
    let v: serde_json::Value = serde_json::from_slice(&saida.stdout).ok()?;
    let us = v.get("data")?.get(0)?.get("data")?.as_i64()?;
    (us >= 0).then_some((us / 1000) as u64)
}

/// Detecta se há música tocando via MPRIS no Linux usando `busctl`.
pub fn detecta_musica_linux() -> Option<MusicaAtividade> {
    let saida_list = Command::new("busctl")
        .args([
            "--user",
            "--json=short",
            "call",
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "ListNames",
        ])
        .output()
        .ok()?;

    if !saida_list.status.success() {
        return None;
    }

    let json_val: serde_json::Value = serde_json::from_slice(&saida_list.stdout).ok()?;
    let nomes = json_val.get("data")?.get(0)?.as_array()?;

    for nome_val in nomes {
        let Some(servico) = nome_val.as_str() else {
            continue;
        };
        if !servico.starts_with("org.mpris.MediaPlayer2.") {
            continue;
        }

        // PlaybackStatus
        let saida_status = Command::new("busctl")
            .args([
                "--user",
                "--json=short",
                "call",
                servico,
                "/org/mpris/MediaPlayer2",
                "org.freedesktop.DBus.Properties",
                "Get",
                "ss",
                "org.mpris.MediaPlayer2.Player",
                "PlaybackStatus",
            ])
            .output()
            .ok()?;

        if !saida_status.status.success() {
            continue;
        }

        let status_json: serde_json::Value = serde_json::from_slice(&saida_status.stdout).ok()?;
        let status_str = status_json.get("data")?.get(0)?.get("data")?.as_str()?;
        if status_str != "Playing" {
            continue;
        }

        // Metadata
        let saida_meta = Command::new("busctl")
            .args([
                "--user",
                "--json=short",
                "call",
                servico,
                "/org/mpris/MediaPlayer2",
                "org.freedesktop.DBus.Properties",
                "Get",
                "ss",
                "org.mpris.MediaPlayer2.Player",
                "Metadata",
            ])
            .output()
            .ok()?;

        if !saida_meta.status.success() {
            continue;
        }

        let meta_json: serde_json::Value = serde_json::from_slice(&saida_meta.stdout).ok()?;
        let meta_data = meta_json.get("data")?.get(0)?.get("data")?;

        let url = meta_data
            .get("xesam:url")
            .and_then(|v| v.get("data"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let arte = meta_data
            .get("mpris:artUrl")
            .and_then(|v| v.get("data"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if eh_youtube_que_nao_e_musica(url, arte) {
            continue;
        }

        let titulo = meta_data
            .get("xesam:title")
            .and_then(|v| v.get("data"))
            .and_then(|v| v.as_str())?;
        if titulo.trim().is_empty() {
            continue;
        }

        let artista = if let Some(artistas_arr) = meta_data
            .get("xesam:artist")
            .and_then(|v| v.get("data"))
            .and_then(|v| v.as_array())
        {
            artistas_arr
                .iter()
                .filter_map(|a| a.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        } else if let Some(art) = meta_data
            .get("xesam:artist")
            .and_then(|v| v.get("data"))
            .and_then(|v| v.as_str())
        {
            art.to_string()
        } else {
            String::new()
        };

        let album = meta_data
            .get("xesam:album")
            .and_then(|v| v.get("data"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let arte_valida = if arte.starts_with("https://") {
            Some(arte.to_string())
        } else {
            None
        };

        // Duração (mpris:length, em µs) e início presumido (agora − posição),
        // para o cartão mostrar a barra de progresso como o Discord.
        let duracao = meta_data
            .get("mpris:length")
            .and_then(|v| v.get("data"))
            .and_then(serde_json::Value::as_i64)
            .filter(|&us| us > 0)
            .map(|us| (us / 1000) as u64);
        let inicio = posicao_mpris(servico).map(|pos_ms| {
            let agora = chrono::Utc::now().timestamp_millis().max(0) as u64;
            // Arredonda a 10 s: o relógio da varredura oscila e, sem isso, a
            // atividade pareceria mudar a cada ciclo.
            (agora.saturating_sub(pos_ms) / 10_000) * 10_000
        });

        return Some(MusicaAtividade {
            inicio,
            duracao,
            titulo: titulo.trim().to_string(),
            artista: if artista.is_empty() {
                None
            } else {
                Some(artista)
            },
            album,
            arte: arte_valida,
        });
    }

    None
}

/// Detecta a atividade completa (jogo e música) no Linux.
pub fn detecta_atividade() -> Option<Atividade> {
    let mut jogo = detecta_jogo_linux();
    if let Some(j) = &mut jogo {
        // Nome certo e capa, quando o jogo já está registrado; e anota que foi visto.
        crate::jogos_registrados::enriquece(j);
        crate::jogos_registrados::registra_visto(j);
    }
    let musica = detecta_musica_linux();

    if jogo.is_none() && musica.is_none() {
        None
    } else {
        Some(Atividade { jogo, musica })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn testa_id_do_escopo_de_app_com_quebra_de_linha() {
        let cgroup = "0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-flatpak-org.vinegarhq.Sober-12345.scope\n";
        assert_eq!(
            id_do_escopo_de_app(cgroup).as_deref(),
            Some("org.vinegarhq.Sober")
        );
    }

    #[test]
    fn testa_desescapa_hex_systemd() {
        assert_eq!(desescapa_hex("osu\\x2dlazer"), "osu-lazer");
        assert_eq!(desescapa_hex("normal"), "normal");
    }

    #[test]
    fn caminho_wine_com_barra_invertida_e_infra() {
        let n = crate::jogos_detectaveis::normaliza(r"C:\windows\system32\services.exe");
        assert!(n.contains("/windows/system32/"));
        assert_eq!(n.rsplit('/').next(), Some("services.exe"));
    }

    #[test]
    fn testa_eh_lancador() {
        assert!(eh_lancador("lutris", None));
        assert!(eh_lancador("net.lutris.Lutris", None));
        assert!(eh_lancador("sklauncher", None));
        assert!(eh_lancador("custom", Some("SKLauncher")));
        assert!(!eh_lancador("org.vinegarhq.Sober", Some("Roblox Player")));
        assert!(!eh_lancador("hl2", Some("Half-Life 2")));
    }

    #[test]
    fn testa_eh_youtube_que_nao_e_musica() {
        // YouTube comum (vídeo) -> true (ignorado)
        assert!(eh_youtube_que_nao_e_musica(
            "https://www.youtube.com/watch?v=123",
            ""
        ));
        assert!(eh_youtube_que_nao_e_musica("https://youtu.be/123", ""));
        assert!(eh_youtube_que_nao_e_musica(
            "",
            "https://i.ytimg.com/vi/123/hqdefault.jpg"
        ));

        // YouTube Music -> false (aceito como música!)
        assert!(!eh_youtube_que_nao_e_musica(
            "https://music.youtube.com/watch?v=abc",
            ""
        ));

        // Spotify / Bandcamp / local -> false (aceito)
        assert!(!eh_youtube_que_nao_e_musica(
            "https://open.spotify.com/track/123",
            ""
        ));
    }

    #[test]
    #[ignore = "usa os processos reais da máquina"]
    fn detecta_real() {
        println!("{:?}", detecta_jogo_linux());
    }
}
