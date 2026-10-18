// Servidor de sinalização para chamadas WebRTC entre amigos.
// Não processa áudio/vídeo — só ajuda os navegadores a se encontrarem.
const http = require('http');
const https = require('https');   // só pra busca de GIF (Giphy)
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const dns = require('dns').promises;
const net = require('net');
const os = require('os');
const WebSocket = require('ws');

// Só pra /api/metrics dizer o que está rodando. Falhar aqui não pode derrubar
// o boot — o package.json pode não ter sido copiado no deploy.
let APP_VERSION = 'desconhecida';
try { APP_VERSION = require('./package.json').version || APP_VERSION; } catch { /* tanto faz */ }

// Maior número de pessoas em chamada ao mesmo tempo desde que o processo subiu.
// Zera no restart de propósito: é medida desta execução, e o pm2 já conta os
// restarts.
let picoDePessoas = 0;

const PORT = process.env.PORT || 8080;

// --- Login com Discord (OAuth2) ---
// Credenciais ficam num arquivo fora do controle de versão, nunca no código
// (mesmo esquema de segredo local que a .chatkey já usa).
const DISCORD_CONFIG_PATH = path.join(__dirname, '.discord-oauth.json');
let DISCORD_CLIENT_ID = null, DISCORD_CLIENT_SECRET = null, DISCORD_REDIRECT_URI = null;
if (fs.existsSync(DISCORD_CONFIG_PATH)) {
  const cfg = JSON.parse(fs.readFileSync(DISCORD_CONFIG_PATH, 'utf8'));
  DISCORD_CLIENT_ID = cfg.clientId;
  DISCORD_CLIENT_SECRET = cfg.clientSecret;
  DISCORD_REDIRECT_URI = cfg.redirectUri;
} else {
  console.warn('.discord-oauth.json ausente — login com Discord desativado.');
}

// --- Bot Discord (atividade de amigos: quem tá online e jogando o quê) ---
// Roda no MESMO processo do server.js de propósito — essa VPS tem menos de
// 1GB de RAM, então um segundo processo Node só pra isso desperdiçaria
// memória com um V8 heap e módulos carregados em duplicidade.
const DISCORD_BOT_CONFIG_PATH = path.join(__dirname, '.discord-bot.json');
const friendsActivity = new Map(); // userId -> { id, name, avatar, status, activity }

if (fs.existsSync(DISCORD_BOT_CONFIG_PATH)) {
  const { Client, GatewayIntentBits } = require('discord.js');
  const botCfg = JSON.parse(fs.readFileSync(DISCORD_BOT_CONFIG_PATH, 'utf8'));
  const FRIENDS_GUILD_ID = String(botCfg.guildId);

  const bot = new Client({
    intents: [GatewayIntentBits.Guilds, GatewayIntentBits.GuildPresences, GatewayIntentBits.GuildMembers]
  });

  function extractPresence(member) {
    const presence = member.presence;
    // type 4 é "status personalizado" (emoji + texto livre) — não é bem um
    // jogo, então ignora e pega a primeira atividade de verdade se tiver.
    const activity = presence?.activities.find(a => a.type !== 4);
    return {
      id: member.id,
      name: member.displayName,
      avatar: member.displayAvatarURL({ size: 64 }),
      status: presence?.status || 'offline',
      activity: activity ? activity.name : null
    };
  }

  bot.once('clientReady', async () => {
    console.log('Bot Discord conectado:', bot.user.tag);
    const guild = await bot.guilds.fetch(FRIENDS_GUILD_ID).catch(() => null);
    if (!guild) {
      console.warn('Bot não está no servidor configurado em .discord-bot.json (guildId).');
      return;
    }
    const members = await guild.members.fetch().catch(() => null);
    if (members) members.forEach(m => friendsActivity.set(m.id, extractPresence(m)));
  });

  bot.on('presenceUpdate', (_old, presence) => {
    const member = presence?.member;
    if (!member || member.guild.id !== FRIENDS_GUILD_ID) return;
    friendsActivity.set(member.id, extractPresence(member));
  });

  bot.on('guildMemberRemove', member => {
    if (member.guild.id === FRIENDS_GUILD_ID) friendsActivity.delete(member.id);
  });

  bot.on('error', err => console.error('Erro no bot Discord:', err.message));

  // O login falhava com ECONNRESET/EPIPE contra gateway.discord.gg de vez em
  // quando (visto no log de produção), e o `.catch` simples que existia aqui
  // deixava a lista de amigos morta até alguém reiniciar o servidor. Agora
  // tenta de novo com espera crescente. Só afeta essa funcionalidade — nada
  // disso toca chamada nem chat.
  let botLoginAttempts = 0;
  const BOT_LOGIN_MAX_DELAY_MS = 5 * 60 * 1000;

  function loginBot() {
    bot.login(botCfg.token).catch(err => {
      botLoginAttempts++;
      const delay = Math.min(15000 * 2 ** (botLoginAttempts - 1), BOT_LOGIN_MAX_DELAY_MS);
      console.error(`Falha ao logar o bot Discord (tentativa ${botLoginAttempts}):`, err.message,
        `— nova tentativa em ${Math.round(delay / 1000)}s`);
      setTimeout(loginBot, delay).unref();
    });
  }

  bot.once('clientReady', () => { botLoginAttempts = 0; });
  loginBot();
} else {
  console.warn('.discord-bot.json ausente — atividade de amigos desativada.');
}

// Sessão vive em memória E em disco. Só em memória, como era antes, todo
// `pm2 restart` — ou seja, todo deploy, que aqui é manual e frequente —
// deslogava a sala inteira: o cookie continuava no navegador, mas o servidor
// não conhecia mais o token, então `/api/me` respondia deslogado e o WebSocket
// passava a tratar todo mundo como "Anônimo" (perdendo nome, avatar e a
// autoria das próprias mensagens no chat). Ninguém tinha o que fazer além de
// logar de novo, sem entender por quê.
//
// E as entradas nunca saíam: o cookie vale 30 dias, o Map valia pra sempre.
// Numa VPS de 954MB que o OOM-killer já visitou, "pra sempre" não é uma
// política de retenção.
const sessions = new Map(); // token -> { id, name, avatar, criadaEm }
const SESSION_TTL_MS = 30 * 24 * 60 * 60 * 1000;   // igual ao Max-Age do cookie

// Login "por fora" (usado pelo app Electron): a janela do app abre o login
// no navegador padrão do sistema em vez de fazer OAuth dentro de si mesma —
// mais familiar (aproveita sessão do Discord já salva no navegador) e evita
// o app pedir pra confiar numa janela sem cara de navegador de verdade. O
// app gera um código aleatório, manda pro navegador via ?loginId=, e fica
// perguntando aqui se aquele código já foi confirmado.
const pendingLogins = new Map(); // loginId -> { sessionToken, createdAt }
const PENDING_LOGIN_TTL_MS = 10 * 60 * 1000;
setInterval(() => {
  const cutoff = Date.now() - PENDING_LOGIN_TTL_MS;
  for (const [id, entry] of pendingLogins) {
    if (entry.createdAt < cutoff) pendingLogins.delete(id);
  }
}, 60 * 1000);

function parseCookies(req) {
  const out = {};
  const header = req.headers.cookie;
  if (!header) return out;
  header.split(';').forEach(pair => {
    const idx = pair.indexOf('=');
    if (idx === -1) return;
    out[pair.slice(0, idx).trim()] = decodeURIComponent(pair.slice(idx + 1).trim());
  });
  return out;
}

// --- Identidade: id, não nome (fatia 1 do item 39) ---------------------------
// Toda autorização daqui (apagar/editar mensagem, dono da sala, silenciar,
// expulsar) era comparação de NOME. Funciona enquanto o nome vem do Discord e
// não colide na prática; deixa de funcionar no minuto em que existir conta
// própria, porque aí "Gabriel" pode ser duas pessoas — e uma apagaria a
// mensagem da outra.
//
// O `uid` sai da sessão, nunca do cliente (é o mesmo cuidado do nome: ver a
// seção de identidade no CLAUDE.md, onde isso já teve furo uma vez). Quem
// conecta sem sessão fica com `uid` nulo e cai na comparação por nome, igual a
// hoje — é o que mantém as duas suítes rodando contra um servidor de verdade.
//
// O prefixo não é enfeite: quando existir conta local o uid dela vai ser
// `local:<id>`, e sem prefixo um id de conta poderia coincidir com um id do
// Discord — duas pessoas viram uma.
function uidDaSessao(session) {
  if (!session || !session.id) return null;
  // Sessão salva antes de existir conta local não tem `tipo` — era tudo
  // Discord. Assumir 'discord' aqui é o que mantém o uid dessas sessões igual
  // ao que ele já era, em vez de trocar a identidade de quem estava logado.
  return (session.tipo || 'discord') + ':' + session.id;
}

// Chave usada em tudo que precisa dizer "esta pessoa": uid quando existe,
// senão o nome. Ter uma função só é o que impede metade do código migrar pra
// id e a outra metade continuar no nome.
function chaveDeIdentidade(uid, nome) {
  return uid || ('nome:' + (nome || ''));
}

// Chave de MODERAÇÃO — expulsão e silenciamento. Precisa ser diferente da de
// identidade por um motivo concreto: sem conta, TODO MUNDO se chama "Anônimo",
// então a chave `nome:Anônimo` é a mesma pessoa pro servidor. Expulsar um
// anônimo expulsava, na prática, todos os anônimos da sala por dez minutos — e
// silenciar um silenciava todos.
//
// Com o IP junto, o castigo continua sobrevivendo a um F5 (que é o ponto de
// não usar o id de conexão, sorteado de novo a cada vez) sem alcançar quem
// está noutra máquina. Duas pessoas atrás do mesmo NAT ainda colidem; é bem
// mais estreito que "todos os anônimos" e não custa estado nenhum.
function chaveDeModeracao(uid, nome, ip) {
  return uid || ('anon:' + (ip || '?') + '|' + (nome || ''));
}

// Autorização: se os dois lados têm uid, vale o uid. Se algum não tem (entrada
// antiga no disco, ou gente sem sessão), cai no nome — assim mensagem velha
// continua sendo de quem a escreveu, sem histórico órfão.
function mesmaPessoa(uidA, nomeA, uidB, nomeB) {
  if (uidA && uidB) return uidA === uidB;
  return !!nomeA && nomeA === nomeB;
}

function discordAvatarUrl(user) {
  if (user.avatar) return `https://cdn.discordapp.com/avatars/${user.id}/${user.avatar}.png?size=64`;
  const fallbackIndex = Number((BigInt(user.id) >> 22n) % 6n);
  return `https://cdn.discordapp.com/embed/avatars/${fallbackIndex}.png`;
}

// --- Chat persistente, criptografado em disco ---
// Sem banco de dados (essa VPS não aguenta compilar módulo nativo — já
// travou a máquina uma vez tentando). Cada sala vira um arquivo texto com
// uma linha criptografada por mensagem; imagens ficam em arquivos à parte,
// também criptografados. A chave mora só no servidor, fora da pasta public/,
// gerada sozinha na primeira vez que o servidor sobe.
const CHAT_DIR = path.join(__dirname, 'chatdata');
const IMAGES_DIR = path.join(CHAT_DIR, 'images');
const ARQUIVOS_DIR = path.join(CHAT_DIR, 'arquivos');
fs.mkdirSync(IMAGES_DIR, { recursive: true });
fs.mkdirSync(ARQUIVOS_DIR, { recursive: true });

const KEY_PATH = path.join(__dirname, '.chatkey');
let CHAT_KEY;
if (fs.existsSync(KEY_PATH)) {
  CHAT_KEY = Buffer.from(fs.readFileSync(KEY_PATH, 'utf8').trim(), 'hex');
} else {
  CHAT_KEY = crypto.randomBytes(32);
  fs.writeFileSync(KEY_PATH, CHAT_KEY.toString('hex'), { mode: 0o600 });
}

function encryptBuf(buf) {
  const iv = crypto.randomBytes(12);
  const cipher = crypto.createCipheriv('aes-256-gcm', CHAT_KEY, iv);
  const enc = Buffer.concat([cipher.update(buf), cipher.final()]);
  return Buffer.concat([iv, cipher.getAuthTag(), enc]);
}

function decryptBuf(blob) {
  const iv = blob.subarray(0, 12);
  const tag = blob.subarray(12, 28);
  const enc = blob.subarray(28);
  const decipher = crypto.createDecipheriv('aes-256-gcm', CHAT_KEY, iv);
  decipher.setAuthTag(tag);
  return Buffer.concat([decipher.update(enc), decipher.final()]);
}

// --- Sessões em disco --------------------------------------------------------
// Mesmo cofre do chat e das senhas de sala: um arquivo só, criptografado com a
// `.chatkey`. É token de sessão, então não pode ficar em texto puro — e é
// exatamente o tipo de coisa que um backup descuidado carrega junto.
const SESSIONS_FILE = path.join(CHAT_DIR, '.sessoes');

function limpaSessoesVencidas() {
  const corte = Date.now() - SESSION_TTL_MS;
  let removidas = 0;
  for (const [token, s] of sessions) {
    if (!s.criadaEm || s.criadaEm < corte) { sessions.delete(token); removidas++; }
  }
  return removidas;
}

// Escrita atômica: grava num arquivo temporário ao lado e RENOMEIA por cima.
//
// Não é preciosismo. Cada um destes arquivos é a única cópia de uma coisa que
// não dá pra recuperar (contas, sessões, senhas de sala, amizades), e esta
// máquina já teve o processo morto pelo OOM-killer no meio do trabalho. Um
// `writeFileSync` interrompido deixa o arquivo pela metade — e aí, no boot
// seguinte, o JSON não abre e TODAS as contas somem. O `rename` no mesmo
// sistema de arquivos é atômico: ou o arquivo antigo continua inteiro, ou o
// novo está inteiro; nunca metade de um.
function escreveArquivoAtomico(caminho, buf, opcoes) {
  const temporario = caminho + '.tmp-' + process.pid;
  try {
    fs.writeFileSync(temporario, buf, opcoes);
    fs.renameSync(temporario, caminho);
  } catch (e) {
    try { fs.unlinkSync(temporario); } catch { /* não existia */ }
    throw e;
  }
}

// Arquivo que existe mas não abre é o pior caso: se a gente seguir com o Map
// vazio, o próximo save escreve `{}` por cima e o estado antigo morre de vez.
// Guardar de lado custa nada e é a diferença entre "deu pra recuperar" e "foi".
function guardaArquivoIlegivel(caminho, motivo) {
  const destino = caminho + '.corrompido-' + new Date().toISOString().replace(/[:.]/g, '-');
  try {
    fs.renameSync(caminho, destino);
    console.error(`ATENÇÃO: ${caminho} não pôde ser lido (${motivo}). Guardei em ${destino} — o app segue com esse estado VAZIO.`);
  } catch (e) {
    console.error(`ATENÇÃO: ${caminho} não pôde ser lido (${motivo}) e nem ser movido (${e.message}).`);
  }
}

function saveSessions() {
  try {
    if (!sessions.size) { try { fs.unlinkSync(SESSIONS_FILE); } catch {} return; }
    const dados = Object.fromEntries(sessions);
    escreveArquivoAtomico(SESSIONS_FILE, encryptBuf(Buffer.from(JSON.stringify(dados), 'utf8')), { mode: 0o600 });
  } catch (e) {
    console.error('Não consegui salvar as sessões:', e.message);
  }
}

function loadSessions() {
  if (!fs.existsSync(SESSIONS_FILE)) return;
  try {
    const dados = JSON.parse(decryptBuf(fs.readFileSync(SESSIONS_FILE)).toString('utf8'));
    for (const [token, s] of Object.entries(dados)) {
      if (s && typeof s.name === 'string') {
        // `usuario` precisa sobreviver ao restart: é ele que autoriza apagar a
        // conta e é ele que vira a arroba da citação. Sem isto, reiniciar o
        // servidor deixava quem já estava logado numa conta local sem os dois.
        sessions.set(token, { tipo: s.tipo || 'discord', id: s.id, usuario: s.usuario || null,
          name: s.name, avatar: s.avatar || null, criadaEm: s.criadaEm || 0 });
      }
    }
    // Sessão sem `criadaEm` é de antes deste campo existir: `criadaEm: 0` a
    // deixa vencida na hora, o que é o certo — não dá pra saber a idade dela,
    // e mandar logar de novo é bem melhor que estender pra sempre um token de
    // idade desconhecida.
    const foram = limpaSessoesVencidas();
    if (foram) saveSessions();
  } catch (e) {
    guardaArquivoIlegivel(SESSIONS_FILE, e.message);
  }
}

loadSessions();
// Varre de hora em hora: o custo é percorrer um Map pequeno, e sem isso uma
// sessão só sai do ar quando alguém acerta o token dela de novo.
setInterval(() => { if (limpaSessoesVencidas()) saveSessions(); }, 60 * 60 * 1000).unref();

// --- Contas locais (fatia 2 do item 39) --------------------------------------
// Conta no app, com nome de usuário e senha, sem depender do Discord. Guardada
// no mesmo cofre do resto (`.chatkey`), num arquivo só.
//
// A senha usa o `scrypt` do `crypto` embutido, e isso não é preferência: as
// escolhas óbvias (`bcrypt`, `argon2`) são módulos nativos, e instalar módulo
// nativo nesta VPS já travou a máquina uma vez. `scrypt` vem no Node, é
// deliberadamente caro em memória e serve pra isso.
//
// O custo é escolhido pra caber numa máquina de 954MB que o OOM-killer já
// visitou: N=16384 gasta ~16MB por hash. Subir pra N=65536 seriam 64MB por
// login simultâneo — bom pra segurança, péssimo pra esta máquina continuar de
// pé. O `maxmem` vai explícito porque o padrão do Node (32MB) fica perto
// demais do que N=16384 pede, e um dia alguém sobe o N sem lembrar disso.
const CONTAS_FILE = path.join(CHAT_DIR, '.contas');
const SCRYPT = { N: 16384, r: 8, p: 1, maxmem: 64 * 1024 * 1024 };
const TAMANHO_DO_HASH = 64;
const contas = new Map();   // usuário normalizado -> conta

function normalizaUsuario(v) {
  return typeof v === 'string' ? v.trim().toLowerCase() : '';
}

// Nome de usuário é identificador, não texto livre: ele aparece em URL, em
// log e no lugar do nome quando a pessoa ainda não escolheu um. Deixar espaço
// e maiúscula passar é o caminho curto pra duas contas que parecem a mesma.
function usuarioValido(v) {
  return /^[a-z0-9][a-z0-9._-]{2,23}$/.test(v);
}

// Nome de exibição: o que aparece na sala e no chat. Ao contrário do nome de
// usuário, aceita espaço e acento — só não pode ser vazio nem carregar quebra
// de linha (que estragaria a linha da sidebar e do chat).
function limpaNomeDeExibicao(v) {
  if (typeof v !== 'string') return null;
  const limpo = v.replace(/[\r\n\t]/g, ' ').trim().slice(0, 32);
  return limpo || null;
}

function hashDeSenha(senha, salt) {
  return new Promise((resolve, reject) => {
    crypto.scrypt(senha, salt, TAMANHO_DO_HASH, SCRYPT, (err, chave) => {
      if (err) reject(err); else resolve(chave);
    });
  });
}

function loadContas() {
  if (!fs.existsSync(CONTAS_FILE)) return;
  try {
    const dados = JSON.parse(decryptBuf(fs.readFileSync(CONTAS_FILE)).toString('utf8'));
    for (const [chave, c] of Object.entries(dados)) {
      if (c && typeof c.usuario === 'string' && typeof c.hash === 'string') contas.set(chave, c);
    }
  } catch (e) {
    guardaArquivoIlegivel(CONTAS_FILE, e.message);
  }
}

function saveContas() {
  try {
    if (!contas.size) { try { fs.unlinkSync(CONTAS_FILE); } catch {} return; }
    escreveArquivoAtomico(CONTAS_FILE, encryptBuf(Buffer.from(JSON.stringify(Object.fromEntries(contas)), 'utf8')), { mode: 0o600 });
  } catch (e) {
    console.error('Não consegui salvar as contas:', e.message);
  }
}

loadContas();

// Freio de força bruta: ATRASO que cresce, não travamento.
//
// Travar a conta depois de N erros é o reflexo, e é uma armadilha aqui:
// qualquer um que saiba o nome de usuário do amigo tranca ele pra fora
// errando a senha de propósito. Numa casa de cinco amigos isso não é ameaça
// teórica, é brincadeira de mau gosto — e o dono da conta não teria o que
// fazer além de esperar.
//
// Atraso resolve os dois lados: a senha certa continua entrando (só um pouco
// mais devagar), e quem tenta adivinhar sai de ~28 tentativas por segundo pra
// uma a cada cinco segundos. O `scrypt` já custa ~35ms sozinho; o atraso é o
// que transforma isso em inviável.
//
// É por conta e não por IP de propósito: trocar de IP é fácil, e travar por IP
// ainda pegaria junto todo mundo atrás da mesma saída NAT. O teto de conexões
// por IP que já existe pro WebSocket cuida do outro lado.
const ERROS_ANTES_DO_ATRASO = 3;
const ATRASO_MAXIMO_MS = 5000;
const MEMORIA_DE_ERROS_MS = 15 * 60 * 1000;
const tentativasDeLogin = new Map();   // usuário -> { erros, ultimoErro }

const dorme = (ms) => new Promise(r => setTimeout(r, ms));

function atrasoDeLogin(usuario) {
  const t = tentativasDeLogin.get(usuario);
  if (!t) return 0;
  // Quem errou e sumiu por quinze minutos volta do zero: senão um engano de
  // hoje de manhã ainda cobra pedágio à noite.
  if (Date.now() - t.ultimoErro > MEMORIA_DE_ERROS_MS) { tentativasDeLogin.delete(usuario); return 0; }
  if (t.erros <= ERROS_ANTES_DO_ATRASO) return 0;
  return Math.min(500 * 2 ** (t.erros - ERROS_ANTES_DO_ATRASO - 1), ATRASO_MAXIMO_MS);
}

function contaErro(usuario) {
  const t = tentativasDeLogin.get(usuario) || { erros: 0, ultimoErro: 0 };
  t.erros++;
  t.ultimoErro = Date.now();
  tentativasDeLogin.set(usuario, t);
}

// Sem isto o Map guarda um registro por nome de usuário que alguém já tentou —
// e nome de usuário tentado é escolha de quem ataca, não das contas que
// existem. Ou seja: memória que cresce com o ataque.
setInterval(() => {
  const corte = Date.now() - MEMORIA_DE_ERROS_MS;
  for (const [u, t] of tentativasDeLogin) if (t.ultimoErro < corte) tentativasDeLogin.delete(u);
}, 5 * 60 * 1000).unref();

function abreSessao(res, sessao) {
  const token = crypto.randomBytes(24).toString('hex');
  sessions.set(token, { ...sessao, criadaEm: Date.now() });
  saveSessions();
  res.setHeader('Set-Cookie', `session=${token}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=2592000`);
  return token;
}

// Nome de sala vem do usuário — em vez de usar direto como nome de arquivo
// (risco de path traversal), sempre passa por hash.
function roomFile(room) {
  const hash = crypto.createHash('sha256').update(room).digest('hex');
  return path.join(CHAT_DIR, hash + '.ndjson');
}

const MAX_MESSAGES_PER_ROOM = 300;
const HISTORY_LIMIT = 50;
const MAX_IMAGE_BYTES = 5 * 1024 * 1024;
const ALLOWED_IMAGE_MIME = new Set(['image/png', 'image/jpeg', 'image/gif', 'image/webp']);

// O tipo sai dos BYTES, não do que o cliente afirmou. Não é questão de XSS —
// um <img> nunca executa o conteúdo, e SVG está fora da lista de propósito —
// é que um upload corrompido ou disfarçado ficava guardado pra sempre e
// aparecia quebrado pra sala inteira, sem ninguém entender por quê. Melhor
// recusar na porta.
function sniffImageMime(buf) {
  if (buf.length < 12) return null;
  if (buf[0] === 0x89 && buf[1] === 0x50 && buf[2] === 0x4e && buf[3] === 0x47) return 'image/png';
  if (buf[0] === 0xff && buf[1] === 0xd8 && buf[2] === 0xff) return 'image/jpeg';
  if (buf.toString('ascii', 0, 6) === 'GIF87a' || buf.toString('ascii', 0, 6) === 'GIF89a') return 'image/gif';
  if (buf.toString('ascii', 0, 4) === 'RIFF' && buf.toString('ascii', 8, 12) === 'WEBP') return 'image/webp';
  return null;
}

// --- Arquivos anexados ao chat (até 200MB) ------------------------------
// Diferente da imagem, o arquivo NUNCA passa pela RAM nem pelo WebSocket: o
// corpo cru do POST vai direto pro disco (stream), e a mensagem só guarda o
// id, o nome e o tamanho. Sem criptografia em disco (decisão do dono: o chat
// de texto continua cifrado, o arquivo não). Apagar o grupo apaga a pasta.
const MAX_ARQUIVO_BYTES = 200 * 1024 * 1024;
const RESERVA_DE_DISCO_BYTES = 2 * 1024 * 1024 * 1024;   // nunca encher a VPS
const arquivosPendentes = new Map();   // id -> { uid, conversa, nome, tam, em }

function dirDeArquivos(conversaId) {
  const h = crypto.createHash('sha256').update(String(conversaId)).digest('hex').slice(0, 24);
  return path.join(ARQUIVOS_DIR, h);
}

function idDeArquivoValido(id) {
  return typeof id === 'string' && /^[0-9a-f]{32}$/.test(id);
}

function nomeDeArquivoLimpo(nome) {
  const limpo = String(nome || '').replace(/[\x00-\x1f\x7f/\\]/g, '_').replace(/^\.+/, '').trim().slice(0, 120);
  return limpo || 'arquivo';
}

function apagaArquivo(conversaId, id) {
  if (!idDeArquivoValido(id)) return;
  try { fs.unlinkSync(path.join(dirDeArquivos(conversaId), id)); } catch { /* já não existia */ }
}

// Arquivo enviado e nunca anexado a mensagem (a pessoa desistiu) não fica no
// disco pra sempre.
setInterval(() => {
  const corte = Date.now() - 60 * 60 * 1000;
  for (const [id, p] of arquivosPendentes) {
    if (p.em < corte) { arquivosPendentes.delete(id); apagaArquivo(p.conversa, id); }
  }
}, 10 * 60 * 1000).unref();

function saveImage(buf) {
  const id = crypto.randomBytes(16).toString('hex');
  fs.writeFileSync(path.join(IMAGES_DIR, id + '.bin'), encryptBuf(buf));
  return id;
}

function loadImage(id) {
  if (!idDeImagemValido(id)) return null;   // mesmo motivo do deleteImage
  const filePath = path.join(IMAGES_DIR, id + '.bin');
  if (!fs.existsSync(filePath)) return null;
  try { return decryptBuf(fs.readFileSync(filePath)); } catch { return null; }
}

// O id é sempre gerado aqui (`saveImage` faz `crypto.randomBytes`), mas quem
// PASSA o id na hora de apagar é o arquivo de chat lido do disco. Se aquele
// arquivo um dia tiver lixo — bug futuro, edição na mão, restauração de backup
// pela metade — um id com `../` faria o unlink sair da pasta de imagens.
// Conferir o formato custa uma linha e fecha isso pra sempre.
function idDeImagemValido(id) {
  return typeof id === 'string' && /^[0-9a-f]{16,64}$/.test(id);
}

function deleteImage(id) {
  if (!idDeImagemValido(id)) return;
  try { fs.unlinkSync(path.join(IMAGES_DIR, id + '.bin')); } catch {}
}

// Avatar próprio (fatia 3b do item 39). Menor que o teto do chat de propósito
// — é um ícone, não uma foto — e reaproveita o mesmo armazenamento
// (`saveImage`/`loadImage`, criptografado, sem acúmulo). Não há
// redimensionamento aqui: biblioteca de imagem é módulo nativo, e nativo
// nesta VPS já travou a máquina uma vez. Quem corta pro quadrado é o
// <canvas> no cliente, antes de mandar — o servidor só limita o tamanho final.
const MAX_AVATAR_BYTES = 20 * 1024 * 1024;
// O banner é maior na tela que o avatar, mas o teto é o mesmo: ele vai
// inteiro pro cliente a cada cartão aberto, e o avatar ainda passa por um
// canvas de 256px antes de subir enquanto o banner sobe como veio (senão o
// GIF pararia de andar).
const MAX_BANNER_BYTES = 20 * 1024 * 1024;

// URL pública do avatar de uma conta local. Precisa ser pública (sem sessão)
// porque QUALQUER pessoa numa sala com você busca essa URL num <img src>,
// igual já acontece com a URL do CDN do Discord — quem serve de volta é
// `GET /api/conta/avatar/:usuario`. O `?v=` é cache-busting: sem ele, trocar
// o avatar não apareceria pra quem já tinha a URL antiga em cache.
function avatarUrlDaConta(conta) {
  if (!conta?.perfil?.avatarId) return null;
  return `/api/conta/avatar/${encodeURIComponent(conta.usuario)}?v=${conta.perfil.avatarId.slice(0, 8)}`;
}

// O banner é a faixa do topo do cartão de perfil. Mora no mesmo lugar do
// avatar e pelo mesmo motivo (`saveImage`, criptografado, sem redimensionar no
// servidor). O `?v=` é o que permite `immutable` no cache: muda quando a
// imagem muda.
function bannerUrlDaConta(conta) {
  if (!conta?.perfil?.bannerId) return null;
  return `/api/conta/banner/${encodeURIComponent(conta.usuario)}?v=${conta.perfil.bannerId.slice(0, 8)}`;
}

// Mensagens salvas antes desse campo existir não têm `id` — deriva um id
// estável a partir do conteúdo pra elas continuarem endereçáveis.
function entryId(e) {
  return e.id || crypto.createHash('sha1').update(`${e.from}:${e.ts}`).digest('hex').slice(0, 12);
}

function readRoomEntries(room) {
  const file = roomFile(room);
  if (!fs.existsSync(file)) return [];
  const lines = fs.readFileSync(file, 'utf8').split('\n').filter(Boolean);
  return lines.map(line => {
    try { return JSON.parse(decryptBuf(Buffer.from(line, 'base64')).toString('utf8')); }
    catch { return null; }
  }).filter(Boolean);
}

// Mantém no máximo MAX_MESSAGES_PER_ROOM por sala — evita crescimento sem
// fim, e apaga as imagens órfãs junto com as mensagens antigas removidas.
// Sala que ficou sem nenhuma mensagem não precisa deixar arquivo vazio pra
// trás — sem isso cada sala descartável (as dos testes, por exemplo) deixa um
// resíduo permanente no disco.
function writeRoomEntries(room, entries) {
  const caminho = roomFile(room);
  if (!entries.length) {
    try { fs.unlinkSync(caminho); } catch { /* já não existia */ }
    return;
  }
  const lines = entries.map(e => encryptBuf(Buffer.from(JSON.stringify(e), 'utf8')).toString('base64'));
  fs.writeFileSync(caminho, lines.join('\n') + '\n');
}

function pruneRoom(room, entries) {
  if (entries.length <= MAX_MESSAGES_PER_ROOM) return entries;
  const removed = entries.slice(0, entries.length - MAX_MESSAGES_PER_ROOM);
  const kept = entries.slice(entries.length - MAX_MESSAGES_PER_ROOM);
  removed.forEach(e => { if (e.imageId) deleteImage(e.imageId); if (e.arquivo) apagaArquivo(room, e.arquivo.id); });
  writeRoomEntries(room, kept);
  return kept;
}

function appendMessage(room, entry) {
  const line = encryptBuf(Buffer.from(JSON.stringify(entry), 'utf8')).toString('base64');
  fs.appendFileSync(roomFile(room), line + '\n');
  pruneRoom(room, readRoomEntries(room));
}

// Apaga uma mensagem por id, só se `requesterName` bater com o autor
// salvo. Usa o nome (não o id de conexão, que é sorteado de novo a cada
// reconexão) pra quem recarregou a página continuar podendo apagar
// mensagens antigas suas.
function deleteMessageEntry(room, id, requesterName, requesterUid) {
  const entries = readRoomEntries(room);
  const idx = entries.findIndex(e => entryId(e) === id);
  if (idx === -1) return false;
  const entry = entries[idx];
  if (entry.sys) return false;   // mensagem de sistema: nem apagar
  if (!mesmaPessoa(entry.uid, entry.name, requesterUid, requesterName)) return false;
  if (entry.imageId) deleteImage(entry.imageId);
  if (entry.arquivo) apagaArquivo(room, entry.arquivo.id);
  entries.splice(idx, 1);
  writeRoomEntries(room, entries);
  return true;
}

// Editar: mesma autorização de apagar (por nome, não pelo id da conexão, que
// é sorteado de novo a cada reconexão). Devolve a entrada atualizada pro
// handler poder recalcular menção e repassar pra sala.
function editMessageEntry(room, id, requesterName, requesterUid, novoTexto) {
  const entries = readRoomEntries(room);
  const idx = entries.findIndex(e => entryId(e) === id);
  if (idx === -1) return null;
  const entry = entries[idx];
  if (entry.sys) return null;   // mensagem de sistema: nem editar
  if (!mesmaPessoa(entry.uid, entry.name, requesterUid, requesterName)) return null;
  // Mensagem que só tinha imagem não vira mensagem de texto por edição — e
  // esvaziar o texto de uma mensagem sem imagem seria um apagar disfarçado,
  // sem passar pelo caminho que limpa a imagem do disco.
  if (!novoTexto && !entry.imageId && !entry.arquivo) return null;
  entry.text = novoTexto || undefined;
  entry.editedAt = Date.now();
  writeRoomEntries(room, entries);
  return entry;
}

// Reagir é alternar: quem já tinha reagido com aquele emoji sai da lista.
// Guardado por NOME pelo mesmo motivo de sempre, e a lista de emojis é fechada
// aqui pra ninguém enfiar texto arbitrário num campo que fica salvo em disco.
const EMOJIS_PERMITIDOS = new Set([
  '👍', '👎', '😂', '😮', '😢', '🔥', '❤️', '🎉',
  '👀', '🤔', '💀', '🙏', '✅', '❌', '🚀', '🧠'
]);
const MAX_REACOES_POR_MENSAGEM = 12;

// --- Enquete -------------------------------------------------------------
// Uma enquete é uma MENSAGEM com um campo a mais, não uma estrutura nova: ela
// nasce, é editada, apagada e some junto com o resto do chat, sem caminho de
// limpeza próprio pra alguém esquecer.
const MAX_OPCOES_DE_ENQUETE = 6;

function limpaEnquete(bruto) {
  if (!bruto || typeof bruto !== 'object') return null;
  const pergunta = typeof bruto.pergunta === 'string'
    ? bruto.pergunta.replace(/[\r\n\t]/g, ' ').trim().slice(0, 140) : '';
  const opcoes = Array.isArray(bruto.opcoes)
    ? bruto.opcoes.map(o => typeof o === 'string' ? o.replace(/[\r\n\t]/g, ' ').trim().slice(0, 60) : '')
        .filter(Boolean).slice(0, MAX_OPCOES_DE_ENQUETE)
    : [];
  // Menos de duas opções não é enquete, é anúncio — e aí o chat já serve.
  if (!pergunta || opcoes.length < 2) return null;
  return { pergunta, opcoes };
}

// O voto é guardado por IDENTIDADE (uid quando existe, nome quando não), a
// mesma chave da moderação e do status: por id de conexão, um F5 daria direito
// a votar de novo.
function registraVoto(room, id, chaveDeQuemVota, opcao) {
  const entries = readRoomEntries(room);
  const idx = entries.findIndex(e => entryId(e) === id);
  if (idx === -1) return null;
  const entry = entries[idx];
  if (entry.sys || !entry.enquete || !Array.isArray(entry.enquete.opcoes)) return null;
  if (opcao < 0 || opcao >= entry.enquete.opcoes.length) return null;
  entry.votos = entry.votos || {};
  // Votar de novo na MESMA opção tira o voto; noutra, troca. É o que a pessoa
  // espera de um botão que fica marcado.
  if (entry.votos[chaveDeQuemVota] === opcao) delete entry.votos[chaveDeQuemVota];
  else entry.votos[chaveDeQuemVota] = opcao;
  writeRoomEntries(room, entries);
  return entry.votos;
}

function toggleReaction(room, id, requesterName, emoji) {
  if (!EMOJIS_PERMITIDOS.has(emoji)) return null;
  const entries = readRoomEntries(room);
  const idx = entries.findIndex(e => entryId(e) === id);
  if (idx === -1) return null;
  const entry = entries[idx];
  if (entry.sys) return null;
  const reacoes = entry.reactions || {};
  const quem = reacoes[emoji] || [];
  const posicao = quem.indexOf(requesterName);
  if (posicao === -1) {
    // O teto é de emojis DIFERENTES por mensagem: sem ele, uma mensagem vira
    // um saco de reações que cresce sem limite dentro do arquivo da sala.
    if (!reacoes[emoji] && Object.keys(reacoes).length >= MAX_REACOES_POR_MENSAGEM) return null;
    quem.push(requesterName);
  } else {
    quem.splice(posicao, 1);
  }
  if (quem.length) reacoes[emoji] = quem;
  else delete reacoes[emoji];
  entry.reactions = Object.keys(reacoes).length ? reacoes : undefined;
  writeRoomEntries(room, entries);
  return entry.reactions || {};
}

function loadHistoryPayload(room) {
  const entries = readRoomEntries(room).slice(-HISTORY_LIMIT);
  return entries.map(e => {
    if (e.sys) return { type: 'sistema', id: entryId(e), sys: e.sys, texto: e.texto, hora: e.hora || e.ts, ts: e.ts,
      quem: e.quem, duracao: e.duracao, perdeu: e.perdeu };
    const out = {
      type: 'chat', id: entryId(e), from: e.from, uid: e.uid || undefined, name: e.name, text: e.text,
      replyTo: e.replyTo || undefined, ts: e.ts,
      mentions: e.mentions && e.mentions.length ? e.mentions : undefined,
      mentionUids: e.mentionUids && e.mentionUids.length ? e.mentionUids : undefined,
      editedAt: e.editedAt || undefined,
      gif: e.gif || undefined,
      arquivo: e.arquivo || undefined,
      fwd: e.fwd || undefined,
      app: e.app || undefined,
      wordle: e.wordle || undefined,
      enquete: e.enquete || undefined,
      votos: e.votos && Object.keys(e.votos).length ? e.votos : undefined,
      reactions: e.reactions && Object.keys(e.reactions).length ? e.reactions : undefined
    };
    if (e.imageId) {
      const buf = loadImage(e.imageId);
      if (buf) out.image = { mime: e.imageMime, data: buf.toString('base64') };
    }
    return out;
  });
}

// --- GIF (pedido em 2026-09-02) -----------------------------------------
//
// **Não é o Tenor.** A primeira versão disto era, e durou algumas horas: o
// Google parou de aceitar cliente novo da API do Tenor em 13/01/2026 e
// desligou a API inteira em 30/06/2026 — nem chave antiga funciona mais. Não
// adianta procurar caminho pra criar chave lá.
//
// O provedor é o **Giphy**, cuja chave beta sai na hora do painel de
// desenvolvedor. Ela tem limite de 100 buscas por hora, e é por isso que o
// cache de cinco minutos aqui embaixo não é enfeite: sem ele, cinco pessoas
// procurando "gato" ao mesmo tempo gastam cinco consultas por letra digitada.
//
// A busca acontece AQUI e não no cliente: chave de API dentro da página é
// chave pública, e a cota é da conta de quem hospeda. O arquivo fica ao lado
// dos outros segredos, modo 600, e sem ele a busca simplesmente não existe (o
// botão some) em vez de o app quebrar.
//
// O GIF em si NÃO passa por aqui e nem pelo `chatdata/`: o que viaja no chat é
// a URL, e quem baixa é o <img> de cada pessoa. Isso mantém o teto de 5MB ×
// 300 por sala longe do assunto — mas quer dizer que o Giphy vê o IP de quem
// está na sala, como qualquer imagem de fora veria.
const GIF_FILE = path.join(__dirname, '.giphy.json');
let chaveDeGif = null;
try {
  chaveDeGif = (JSON.parse(fs.readFileSync(GIF_FILE, 'utf8')).key || '').trim() || null;
} catch { chaveDeGif = null; }
if (!chaveDeGif) console.log('.giphy.json ausente — busca de GIF desativada.');

// Só host do provedor entra no chat como imagem. Sem esta trava, "mandar GIF
// por URL" viraria "mandar qualquer URL", que é rastreador de IP de graça pra
// quem manda o link — e a mesma checagem existe no cliente, na hora de
// desenhar.
function limpaUrlDeGif(v) {
  if (typeof v !== 'string' || v.length > 400) return null;
  let u; try { u = new URL(v); } catch { return null; }
  if (u.protocol !== 'https:') return null;
  const host = u.hostname.toLowerCase();
  if (host !== 'giphy.com' && !host.endsWith('.giphy.com')) return null;
  return u.href;
}

// --- Capas de jogo (SteamGridDB) -----------------------------------------
// A chave fica só aqui (`.steamgriddb.json`, modo 600, fora do git). O cliente
// nunca vê a chave nem fala com o SteamGridDB: pede a busca e a capa ao
// servidor, que guarda a capa em disco uma vez só (`chatdata/capas/`).
const SGDB_FILE = path.join(__dirname, '.steamgriddb.json');
let chaveSgdb = null;
try { chaveSgdb = (JSON.parse(fs.readFileSync(SGDB_FILE, 'utf8')).key || '').trim() || null; } catch { chaveSgdb = null; }
if (!chaveSgdb) console.log('.steamgriddb.json ausente — busca de capas de jogo desativada.');
const CAPAS_DIR = path.join(CHAT_DIR, 'capas');
fs.mkdirSync(CAPAS_DIR, { recursive: true });

function sgdbGet(caminho, comoBuffer) {
  return new Promise((resolve) => {
    const alvo = caminho.startsWith('https://') ? caminho : `https://www.steamgriddb.com/api/v2${caminho}`;
    const host = new URL(alvo).hostname;
    if (host !== 'steamgriddb.com' && !host.endsWith('.steamgriddb.com')) { resolve(null); return; }
    const opcoes = { timeout: 8000, headers: caminho.startsWith('https://') ? {} : { Authorization: `Bearer ${chaveSgdb}` } };
    const req = https.get(alvo, opcoes, (resp) => {
      if (resp.statusCode !== 200) { resp.resume(); resolve(null); return; }
      const partes = [];
      let total = 0;
      const teto = comoBuffer ? 4 * 1024 * 1024 : 512 * 1024;
      resp.on('data', (p) => { total += p.length; if (total > teto) { req.destroy(); resolve(null); return; } partes.push(p); });
      resp.on('end', () => {
        const buf = Buffer.concat(partes);
        if (comoBuffer) { resolve(buf); return; }
        try { resolve(JSON.parse(buf.toString('utf8'))); } catch { resolve(null); }
      });
    });
    req.on('timeout', () => { req.destroy(); resolve(null); });
    req.on('error', () => resolve(null));
  });
}

const buscasDeJogo = new Map();   // termo -> { em, resultados }
async function buscaJogos(termo) {
  const c = buscasDeJogo.get(termo);
  if (c && Date.now() - c.em < 10 * 60 * 1000) return c.resultados;
  const r = await sgdbGet(`/search/autocomplete/${encodeURIComponent(termo)}`);
  const resultados = r && Array.isArray(r.data)
    ? r.data.slice(0, 8).filter(j => Number.isInteger(j.id) && typeof j.name === 'string').map(j => ({ id: j.id, nome: j.name.slice(0, 80) }))
    : null;
  if (resultados) {
    buscasDeJogo.set(termo, { em: Date.now(), resultados });
    if (buscasDeJogo.size > 200) buscasDeJogo.delete(buscasDeJogo.keys().next().value);
  }
  return resultados;
}

async function capaDeJogo(id) {
  const arq = path.join(CAPAS_DIR, String(id));
  try { return fs.readFileSync(arq); } catch { /* ainda não baixada */ }
  const r = await sgdbGet(`/grids/game/${id}?dimensions=342x482&types=static&limit=1`);
  const alvo = r && Array.isArray(r.data) && r.data[0] && r.data[0].url;
  if (typeof alvo !== 'string') return null;
  const buf = await sgdbGet(alvo, true);
  if (!buf || !sniffImageMime(buf)) return null;
  try { fs.writeFileSync(arq, buf); } catch { /* sem disco: serve sem guardar */ }
  return buf;
}

// Cache curto da busca: a mesma palavra é buscada várias vezes seguidas
// enquanto se escolhe, e a cota da chave beta é de 100 por hora.
const buscasDeGif = new Map();   // termo -> { em, resultados }
const VALIDADE_DA_BUSCA_MS = 5 * 60 * 1000;

function buscaGif(termo) {
  return new Promise((resolve) => {
    const cacheado = buscasDeGif.get(termo);
    if (cacheado && Date.now() - cacheado.em < VALIDADE_DA_BUSCA_MS) { resolve(cacheado.resultados); return; }
    const alvo = termo
      ? `https://api.giphy.com/v1/gifs/search?q=${encodeURIComponent(termo)}&api_key=${encodeURIComponent(chaveDeGif)}&limit=48&rating=pg-13&lang=pt`
      : `https://api.giphy.com/v1/gifs/trending?api_key=${encodeURIComponent(chaveDeGif)}&limit=48&rating=pg-13`;
    const req = https.get(alvo, { timeout: 6000 }, (resp) => {
      let corpo = '';
      resp.setEncoding('utf8');
      // Teto no que se lê de fora: resposta enorme não pode virar RAM nesta
      // máquina de 954MB.
      resp.on('data', (p) => { if (corpo.length < 512 * 1024) corpo += p; });
      resp.on('end', () => {
        let dados; try { dados = JSON.parse(corpo); } catch { resolve(null); return; }
        const lista = Array.isArray(dados.data) ? dados.data : [];
        const resultados = lista.map(r => {
          const im = r.images || {};
          // `downsized` em vez de `original`: o original do Giphy passa de
          // 5MB com facilidade, e quem paga o download é cada pessoa da sala.
          const url = limpaUrlDeGif((im.downsized && im.downsized.url) || (im.original && im.original.url));
          const previa = limpaUrlDeGif((im.fixed_height_small && im.fixed_height_small.url)
            || (im.fixed_height && im.fixed_height.url) || url);
          if (!url || !previa) return null;
          return { url, previa, descricao: String(r.title || 'GIF').slice(0, 80) };
        }).filter(Boolean);
        buscasDeGif.set(termo, { em: Date.now(), resultados });
        // O cache é conveniência, não banco: não pode crescer sem fim.
        if (buscasDeGif.size > 60) buscasDeGif.delete(buscasDeGif.keys().next().value);
        resolve(resultados);
      });
    });
    req.on('timeout', () => { req.destroy(); resolve(null); });
    req.on('error', () => resolve(null));
  });
}

// --- ICE (STUN/TURN) ---
// O coturn roda nesta mesma VPS e responde tanto binding STUN quanto Allocate
// TURN na 3478. O IP é resolvido aqui (e recacheado de tempos em tempos,
// porque o domínio é DuckDNS e já mudou uma vez) pro cliente receber URLs com
// IP literal, sem depender do DNS da máquina dele.
//
// Isso existe porque dentro do app Electron o resolvedor do Chromium falhava
// em TODOS os hostnames de STUN/TURN ("host lookup received error", código
// 701) por causa do split-DNS do systemd-resolved com o NetBird — enquanto o
// mesmo servidor respondia normalmente quando testado fora do Chromium. Com o
// IP literal, a negociação ICE deixa de depender de DNS.
const TURN_HOST = process.env.TURN_HOST || 'meuteamspeak6.duckdns.org';
const TURN_USER = process.env.TURN_USER || 'ts6user';
const TURN_PASS = process.env.TURN_PASS || 'MinhaSenhaForte123!@';
const ICE_CACHE_TTL_MS = 10 * 60 * 1000;
let iceCache = { at: 0, servers: null };

// TURN sobre TLS (porta 5349) atravessa firewall chato — rede de empresa,
// hotel, wifi público — que costuma bloquear UDP e portas soltas, porque na
// aparência é só uma conexão HTTPS. O coturn já está configurado com o cert
// Let's Encrypt, mas a porta ainda precisa estar liberada na Security List
// da Oracle (console, sem CLI na máquina), que é uma camada separada do
// iptables.
//
// Em vez de anunciar um servidor que talvez não responda (o openrelay já
// ensinou que candidato que só dá erro atrasa o gathering e não ajuda
// ninguém), o servidor testa a porta pelo próprio IP público — o NAT da
// Oracle faz hairpin, então isso reflete de verdade o que um cliente de fora
// enxerga. No dia em que a regra for criada no console, o TURNS passa a ser
// oferecido sozinho, sem mexer em código.
function probeTcp(host, port, timeoutMs = 1500) {
  return new Promise(resolve => {
    const sock = new net.Socket();
    let terminado = false;
    const encerra = (alcancavel) => {
      if (terminado) return;
      terminado = true;
      sock.destroy();
      resolve(alcancavel);
    };
    sock.setTimeout(timeoutMs);
    sock.once('connect', () => encerra(true));
    sock.once('timeout', () => encerra(false));
    sock.once('error', () => encerra(false));
    sock.connect(port, host);
  });
}

async function getIceServers() {
  if (iceCache.servers && Date.now() - iceCache.at < ICE_CACHE_TTL_MS) return iceCache.servers;

  let ip = null;
  try {
    const addrs = await dns.resolve4(TURN_HOST);
    ip = addrs && addrs[0] ? addrs[0] : null;
  } catch { /* sem DNS aqui: cai pro hostname mesmo, abaixo */ }

  const cred = { username: TURN_USER, credential: TURN_PASS };
  const servers = [];

  if (ip) {
    // IP literal primeiro: é o caminho que funciona mesmo com o DNS do
    // cliente quebrado (o caso do Electron com NetBird).
    servers.push({ urls: [`stun:${ip}:3478`] });
    servers.push({ urls: [`turn:${ip}:3478?transport=udp`, `turn:${ip}:3478?transport=tcp`], ...cred });
  }

  // Hostname como reserva, pra quem tem DNS normal (navegador comum).
  servers.push({ urls: [`stun:${TURN_HOST}:3478`] });
  servers.push({ urls: [`turn:${TURN_HOST}:3478?transport=udp`, `turn:${TURN_HOST}:3478?transport=tcp`], ...cred });

  // TURNS obrigatoriamente pelo hostname, não por IP: o cliente valida o
  // certificado TLS contra o nome. Por isso ele entra depois das entradas
  // com IP literal, que são as que funcionam mesmo com DNS quebrado.
  if (ip && await probeTcp(ip, 5349)) {
    servers.push({ urls: [`turns:${TURN_HOST}:5349?transport=tcp`], ...cred });
  } else {
    console.warn('TURNS (5349) inalcançável de fora — falta liberar a porta na Security List da Oracle. ' +
      'Seguindo só com TURN na 3478.');
  }
  servers.push({ urls: ['stun:stun.l.google.com:19302', 'stun:stun1.l.google.com:19302'] });

  iceCache = { at: Date.now(), servers };
  return servers;
}

// Corpo JSON de um POST, com teto. Sem teto, um cliente qualquer segura
// memória do processo mandando corpo infinito — numa máquina de 954MB isso é
// o bastante pra derrubar a chamada de todo mundo.
const MAX_CORPO_BYTES = 64 * 1024;

function leJsonDoCorpo(req, limiteBytes) {
  const limite = limiteBytes || MAX_CORPO_BYTES;
  return new Promise(resolve => {
    let total = 0;
    const partes = [];
    req.on('data', (pedaco) => {
      total += pedaco.length;
      if (total > limite) { resolve(null); req.destroy(); return; }
      partes.push(pedaco);
    });
    req.on('end', () => {
      try {
        const obj = JSON.parse(Buffer.concat(partes).toString('utf8'));
        resolve(obj && typeof obj === 'object' ? obj : null);
      } catch { resolve(null); }
    });
    req.on('error', () => resolve(null));
  });
}

function responde(res, status, obj) {
  res.writeHead(status, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store' });
  res.end(JSON.stringify(obj));
}

// --- Servidor HTTP simples, serve a pasta public/ + rotas de login ---
// Cabeçalhos que valem pra TODA resposta. Postos aqui, antes do roteamento,
// porque `setHeader` convive com o `writeHead` de cada rota (o writeHead ganha
// no que repetir) — pendurar em cada resposta seria esquecer numa.
//
// A CSP é frouxa em dois pontos DE PROPÓSITO e não adianta apertar sem mexer
// no resto: a página inteira é `<style>`/`<script>` embutidos (daí
// 'unsafe-inline'), e `img-src https:` existe porque o app mostra capa de
// álbum e avatar de fora, de domínio que ninguém controla. O que ela fecha é o
// que importa: script de outro domínio, iframe, `<base>` e ser embutido em
// página alheia.
function poeCabecalhosDeSeguranca(res) {
  res.setHeader('X-Content-Type-Options', 'nosniff');
  res.setHeader('Referrer-Policy', 'no-referrer');
  // Ninguém além desta página precisa de câmera/mic/tela — e é ela que pede.
  res.setHeader('Permissions-Policy', 'geolocation=(), payment=(), usb=(), interest-cohort=()');
  res.setHeader('Content-Security-Policy', [
    "default-src 'self'",
    // `blob:` aqui não é sobra: o módulo do AudioWorklet da supressão de ruído
    // é carregado de um Blob (o server.js não tem rota pra servir arquivo
    // extra), e worklet é regido por `script-src`, não por `worker-src`. Sem
    // isto a cadeia de supressão simplesmente não monta — o teste pegou.
    "script-src 'self' 'unsafe-inline' blob:",
    "style-src 'self' 'unsafe-inline'",
    "img-src 'self' data: blob: https:",
    "media-src 'self' blob: data: mediastream:",
    "font-src 'self' data:",
    "connect-src 'self' ws: wss:",
    "worker-src 'self' blob:",     // a porta de ruído é um worklet carregado de Blob
    "frame-ancestors 'none'",
    "base-uri 'none'",
    "form-action 'self'"
  ].join('; '));
}

const server = http.createServer(async (req, res) => {
  poeCabecalhosDeSeguranca(res);
  const url = new URL(req.url, `http://${req.headers.host}`);

  if (url.pathname === '/auth/discord' || url.pathname === '/auth/discord/callback') {
    res.writeHead(410, { 'Content-Type': 'text/plain; charset=utf-8' });
    res.end('Login com Discord foi desativado.');
    return;
  }

  // O app Electron pergunta aqui de tempos em tempos se o login feito no
  // navegador do sistema já foi confirmado. Uso único: some da lista assim
  // que respondido, pra ninguém mais reaproveitar o mesmo código.
  if (url.pathname === '/api/login-poll') {
    const loginId = url.searchParams.get('loginId');
    const entry = loginId && pendingLogins.get(loginId);
    if (!entry) {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ ready: false }));
      return;
    }
    pendingLogins.delete(loginId);
    res.writeHead(200, {
      'Content-Type': 'application/json',
      'Set-Cookie': `session=${entry.sessionToken}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=2592000`
    });
    res.end(JSON.stringify({ ready: true }));
    return;
  }

  // --- editar o próprio perfil (fatia 4) ---
  // Nome de exibição e descrição. O nome de USUÁRIO não muda aqui de
  // propósito: ele é a arroba com que as pessoas te citam e o identificador
  // que aparece em mensagem já enviada — trocar quebraria citação antiga e
  // abriria a porta pra alguém liberar o próprio nome e outra pessoa pegar.
  if (url.pathname === '/api/conta/perfil') {
    if (req.method !== 'POST') { responde(res, 405, { erro: 'método' }); return; }
    const cookies = parseCookies(req);
    const token = cookies.session;
    const session = token && sessions.get(token);
    if (!session || session.tipo !== 'local') { responde(res, 401, { erro: 'Entre na sua conta primeiro.' }); return; }
    const conta = contas.get(session.usuario);
    if (!conta) { responde(res, 404, { erro: 'Conta não encontrada.' }); return; }
    const corpo = await leJsonDoCorpo(req);
    if (!corpo) { responde(res, 400, { erro: 'corpo inválido' }); return; }

    if (corpo.nome !== undefined) {
      const nome = limpaNomeDeExibicao(corpo.nome);
      if (!nome) { responde(res, 400, { erro: 'O nome não pode ficar vazio.' }); return; }
      conta.nome = nome;
    }
    if (corpo.bio !== undefined) {
      // Uma linha só, e curta: isso aparece no cartão do perfil, não é post.
      const bio = typeof corpo.bio === 'string' ? corpo.bio.replace(/[\r\n\t]/g, ' ').trim().slice(0, 140) : '';
      conta.perfil = conta.perfil || {};
      if (bio) conta.perfil.bio = bio; else delete conta.perfil.bio;
    }
    if (corpo.cor !== undefined) {
      const cor = limpaCor(corpo.cor);
      conta.perfil = conta.perfil || {};
      if (cor) conta.perfil.cor = cor; else delete conta.perfil.cor;
    }
    if (corpo.banner !== undefined) {
      const banner = limpaBanner(corpo.banner);
      conta.perfil = conta.perfil || {};
      if (banner) conta.perfil.banner = banner; else delete conta.perfil.banner;
    }
    saveContas();
    // Toda sessão da conta passa a ver o nome novo, não só a que pediu — quem
    // está logado em duas máquinas não pode ficar com dois nomes.
    for (const [, s] of sessions) if (s.tipo === 'local' && s.id === conta.id) s.name = conta.nome;
    saveSessions();
    responde(res, 200, { ok: true, nome: conta.nome, perfil: conta.perfil || {}, bannerUrl: bannerUrlDaConta(conta) });
    return;
  }

  // --- Avatar da conta local (fatia 3b) ---------------------------------
  // Upload (bytes escolhidos pela pessoa) ou geração por IA — os dois caem
  // no mesmo lugar: mesma validação de bytes do chat (`sniffImageMime`
  // sobre os BYTES, nunca o mime que o cliente afirmou), e o mesmo
  // armazenamento (`saveImage`/`loadImage`, criptografado).
  if (url.pathname === '/api/conta/avatar' && (req.method === 'POST' || req.method === 'DELETE')) {
    const cookies = parseCookies(req);
    const session = cookies.session && sessions.get(cookies.session);
    if (!session || session.tipo !== 'local') { responde(res, 401, { erro: 'Entre na sua conta primeiro.' }); return; }
    const conta = contas.get(session.usuario);
    if (!conta) { responde(res, 404, { erro: 'Conta não encontrada.' }); return; }

    if (req.method === 'DELETE') {
      if (conta.perfil?.avatarId) {
        deleteImage(conta.perfil.avatarId);
        delete conta.perfil.avatarId;
        delete conta.perfil.avatarMime;
        saveContas();
      }
      responde(res, 200, { ok: true });
      return;
    }

    // O corpo é o mesmo JSON-com-base64 que o chat já usa pra imagem; o
    // limite aqui é maior que o `MAX_CORPO_BYTES` padrão (64KB) porque
    // base64 de uma imagem pequena já passa disso.
    const corpo = await leJsonDoCorpo(req, MAX_AVATAR_BYTES * 2);
    const dados = typeof corpo?.data === 'string' ? corpo.data : '';
    let buf; try { buf = Buffer.from(dados, 'base64'); } catch { buf = null; }
    if (!buf || !buf.length || buf.length > MAX_AVATAR_BYTES) {
      responde(res, 400, { erro: 'Imagem inválida ou grande demais.' });
      return;
    }
    const mime = sniffImageMime(buf);
    if (!mime || !ALLOWED_IMAGE_MIME.has(mime)) {
      responde(res, 400, { erro: 'Formato de imagem não reconhecido.' });
      return;
    }
    const idAntigo = conta.perfil?.avatarId;
    conta.perfil = conta.perfil || {};
    conta.perfil.avatarId = saveImage(buf);
    conta.perfil.avatarMime = mime;
    saveContas();
    if (idAntigo) deleteImage(idAntigo); // depois de salvar o novo: se algo acima falhasse, a conta não fica sem avatar nenhum
    responde(res, 200, { ok: true, avatar: avatarUrlDaConta(conta) });
    return;
  }

  // --- Banner da conta ---------------------------------------------------
  // Mesmo caminho do avatar, com duas diferenças de propósito: **não passa por
  // canvas no cliente** (recortar mataria o GIF animado, que é metade do
  // pedido) e por isso o teto de bytes é o que segura o tamanho.
  //
  // GIF entra: `ALLOWED_IMAGE_MIME` já aceita, e `sniffImageMime` decide pelos
  // BYTES, não pelo mime que o cliente afirmou.
  if (url.pathname === '/api/conta/banner' && (req.method === 'POST' || req.method === 'DELETE')) {
    const cookies = parseCookies(req);
    const session = cookies.session && sessions.get(cookies.session);
    if (!session || session.tipo !== 'local') { responde(res, 401, { erro: 'Entre na sua conta primeiro.' }); return; }
    const conta = contas.get(session.usuario);
    if (!conta) { responde(res, 404, { erro: 'Conta não encontrada.' }); return; }

    if (req.method === 'DELETE') {
      if (conta.perfil?.bannerId) {
        deleteImage(conta.perfil.bannerId);
        delete conta.perfil.bannerId;
        delete conta.perfil.bannerMime;
        saveContas();
      }
      responde(res, 200, { ok: true });
      return;
    }

    const corpo = await leJsonDoCorpo(req, MAX_BANNER_BYTES * 2);
    const dados = typeof corpo?.data === 'string' ? corpo.data : '';
    let buf; try { buf = Buffer.from(dados, 'base64'); } catch { buf = null; }
    if (!buf || !buf.length || buf.length > MAX_BANNER_BYTES) {
      responde(res, 400, { erro: 'Imagem inválida ou grande demais (máx. 20MB).' });
      return;
    }
    const mime = sniffImageMime(buf);
    if (!mime || !ALLOWED_IMAGE_MIME.has(mime)) {
      responde(res, 400, { erro: 'Formato de imagem não reconhecido.' });
      return;
    }
    const idAntigo = conta.perfil?.bannerId;
    conta.perfil = conta.perfil || {};
    conta.perfil.bannerId = saveImage(buf);
    conta.perfil.bannerMime = mime;
    saveContas();
    if (idAntigo) deleteImage(idAntigo);
    responde(res, 200, { ok: true, bannerUrl: bannerUrlDaConta(conta) });
    return;
  }

  if (url.pathname.startsWith('/api/conta/banner/') && req.method === 'GET') {
    const usuario = normalizaUsuario(decodeURIComponent(url.pathname.slice('/api/conta/banner/'.length)));
    const conta = usuario && contas.get(usuario);
    const bannerId = conta?.perfil?.bannerId;
    const buf = bannerId ? loadImage(bannerId) : null;
    if (!buf) { responde(res, 404, { erro: 'sem banner' }); return; }
    res.writeHead(200, {
      'Content-Type': conta.perfil.bannerMime || 'image/png',
      'Cache-Control': 'public, max-age=604800, immutable'
    });
    res.end(buf);
    return;
  }

  // Serve o avatar de volta. PÚBLICA de propósito (sem checar sessão): quem
  // busca aqui é o <img src> de qualquer pessoa na mesma sala, do mesmo jeito
  // que já busca a URL do CDN do Discord pra quem entrou por lá.
  if (url.pathname.startsWith('/api/conta/avatar/') && req.method === 'GET') {
    const usuario = normalizaUsuario(decodeURIComponent(url.pathname.slice('/api/conta/avatar/'.length)));
    const conta = usuario && contas.get(usuario);
    const avatarId = conta?.perfil?.avatarId;
    const buf = avatarId ? loadImage(avatarId) : null;
    if (!buf) { responde(res, 404, { erro: 'sem avatar' }); return; }
    res.writeHead(200, {
      'Content-Type': conta.perfil.avatarMime || 'image/png',
      // immutable vale porque o `?v=` muda sempre que o avatar muda — ver
      // `avatarUrlDaConta`.
      'Cache-Control': 'public, max-age=604800, immutable'
    });
    res.end(buf);
    return;
  }

  // Trocar a senha. Pede a atual mesmo já tendo sessão, pelo mesmo motivo do
  // apagar: sessão roubada não pode virar posse da conta.
  if (url.pathname === '/api/conta/senha') {
    if (req.method !== 'POST') { responde(res, 405, { erro: 'método' }); return; }
    const cookies = parseCookies(req);
    const token = cookies.session;
    const session = token && sessions.get(token);
    if (!session || session.tipo !== 'local') { responde(res, 401, { erro: 'Entre na sua conta primeiro.' }); return; }
    const conta = contas.get(session.usuario);
    if (!conta) { responde(res, 404, { erro: 'Conta não encontrada.' }); return; }
    const corpo = await leJsonDoCorpo(req);
    const atual = typeof corpo?.senhaAtual === 'string' ? corpo.senhaAtual : '';
    const nova = typeof corpo?.senhaNova === 'string' ? corpo.senhaNova : '';
    if (nova.length < 8 || nova.length > 200) {
      responde(res, 400, { erro: 'A senha nova precisa ter pelo menos 8 caracteres.' });
      return;
    }
    const espera = atrasoDeLogin(session.usuario);
    if (espera) await dorme(espera);
    const calculado = await hashDeSenha(atual, conta.salt);
    const guardado = Buffer.from(conta.hash, 'base64');
    if (guardado.length !== calculado.length || !crypto.timingSafeEqual(calculado, guardado)) {
      contaErro(session.usuario);
      responde(res, 401, { erro: 'A senha atual não confere.' });
      return;
    }
    tentativasDeLogin.delete(session.usuario);
    conta.salt = crypto.randomBytes(16).toString('hex');
    conta.hash = (await hashDeSenha(nova, conta.salt)).toString('base64');
    saveContas();
    // As OUTRAS sessões caem. Trocar senha é o que se faz quando se desconfia
    // de que alguém entrou; deixar as sessões dele de pé anularia o gesto.
    for (const [t, s] of sessions) {
      if (t !== token && s.tipo === 'local' && s.id === conta.id) sessions.delete(t);
    }
    saveSessions();
    responde(res, 200, { ok: true });
    return;
  }

  // Apagar a própria conta. Pede a senha de novo mesmo já tendo sessão: é a
  // única operação daqui que não dá pra desfazer, e sessão roubada não pode
  // ser suficiente pra destruir a conta de alguém.
  //
  // O que NÃO é apagado: as mensagens que a pessoa escreveu. Elas são da
  // conversa, e sumir com metade de um diálogo estraga o histórico de todo
  // mundo — apagar mensagem continua sendo uma de cada vez, por quem escreveu.
  if (url.pathname === '/api/conta/apagar') {
    if (req.method !== 'POST') { responde(res, 405, { erro: 'método' }); return; }
    const cookies = parseCookies(req);
    const session = cookies.session && sessions.get(cookies.session);
    if (!session || session.tipo !== 'local') { responde(res, 401, { erro: 'Entre na sua conta primeiro.' }); return; }
    const corpo = await leJsonDoCorpo(req);
    const conta = contas.get(session.usuario);
    if (!conta) { responde(res, 404, { erro: 'Conta não encontrada.' }); return; }
    const calculado = await hashDeSenha(typeof corpo?.senha === 'string' ? corpo.senha : '', conta.salt);
    const guardado = Buffer.from(conta.hash, 'base64');
    if (guardado.length !== calculado.length || !crypto.timingSafeEqual(calculado, guardado)) {
      responde(res, 401, { erro: 'Senha não confere.' });
      return;
    }
    if (conta.perfil?.avatarId) deleteImage(conta.perfil.avatarId); // senão sobra um .bin órfão pra sempre
    if (conta.perfil?.bannerId) deleteImage(conta.perfil.bannerId);
    // A amizade morre junto com a conta. Sem isso o uid apagado fica na lista
    // de todo mundo pra sempre, como um cartão sem nome que ninguém consegue
    // remover — a mesma invariante do avatar órfão, um degrau acima.
    esqueceUid('local:' + conta.id);
    contas.delete(session.usuario);
    saveContas();
    // Toda sessão daquela conta cai junto, não só a que pediu: deixar as
    // outras de pé manteria a pessoa logada numa conta que não existe mais.
    for (const [token, s] of sessions) if (s.tipo === 'local' && s.id === conta.id) sessions.delete(token);
    saveSessions();
    res.setHeader('Set-Cookie', 'session=; HttpOnly; Path=/; Max-Age=0');
    responde(res, 200, { ok: true });
    return;
  }

  if (url.pathname === '/auth/logout') {
    const cookies = parseCookies(req);
    if (cookies.session && sessions.delete(cookies.session)) saveSessions();
    res.writeHead(302, { Location: '/', 'Set-Cookie': 'session=; HttpOnly; Path=/; Max-Age=0' });
    res.end();
    return;
  }

  if (url.pathname === '/api/me') {
    const cookies = parseCookies(req);
    const session = cookies.session && sessions.get(cookies.session);
    // Avatar de conta local é resolvido AGORA, não lido do que ficou salvo na
    // sessão no login — senão trocar o avatar só apareceria pra quem
    // deslogasse e entrasse de novo.
    const conta = session?.usuario ? contas.get(session.usuario) : null;
    const avatar = session ? (session.tipo === 'local' ? avatarUrlDaConta(conta) : session.avatar) : null;
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify(session
      ? { loggedIn: true, gifAtivo: !!chaveDeGif, name: session.name, avatar, tipo: session.tipo || 'discord',
          usuario: session.usuario || null,
          // Mesma razão do avatar: a URL do banner é resolvida agora, senão
          // trocar a imagem só apareceria depois de deslogar e entrar de novo.
          bannerUrl: bannerUrlDaConta(conta),
          perfil: conta?.perfil || {} }
      : { loggedIn: false }));
    return;
  }

  // --- Conta própria (fatia 2 do item 39) ---
  // Criar conta e entrar. As duas respondem a mesma coisa em caso de sucesso
  // (um cookie de sessão), então o resto do app não precisa saber de onde a
  // identidade veio — é o que `uidDaSessao` já resolve.
  if (url.pathname === '/api/conta/criar' || url.pathname === '/api/conta/entrar') {
    if (req.method !== 'POST') { responde(res, 405, { erro: 'método' }); return; }
    const corpo = await leJsonDoCorpo(req);
    if (!corpo) { responde(res, 400, { erro: 'corpo inválido' }); return; }

    const usuario = normalizaUsuario(corpo.usuario);
    const senha = typeof corpo.senha === 'string' ? corpo.senha : '';

    if (!usuarioValido(usuario)) {
      responde(res, 400, { erro: 'Nome de usuário: de 3 a 24 letras, números, ponto, hífen ou _, começando por letra ou número.' });
      return;
    }
    // Oito é o piso, não a recomendação. Sem contar caractere especial nem
    // maiúscula: regra de composição empurra a pessoa pra "Senha1!" e não pra
    // uma senha melhor.
    if (senha.length < 8 || senha.length > 200) {
      responde(res, 400, { erro: 'A senha precisa ter pelo menos 8 caracteres.' });
      return;
    }

    // O atraso vale pras duas rotas: sem isso, criar conta vira um oráculo de
    // "esse nome existe?" livre de pedágio.
    const espera = atrasoDeLogin(usuario);
    if (espera) await dorme(espera);

    if (url.pathname === '/api/conta/criar') {
      if (contas.has(usuario)) { responde(res, 409, { erro: 'Esse nome de usuário já existe.' }); return; }
      const salt = crypto.randomBytes(16).toString('hex');
      const hash = (await hashDeSenha(senha, salt)).toString('base64');
      // Nome de exibição é separado do nome de usuário e NÃO é único — duas
      // pessoas podem se chamar "Gabriel", que é exatamente o caso que a
      // identidade por uid (fatia 1) existe pra aguentar. O nome de usuário
      // continua sendo o único.
      const nome = limpaNomeDeExibicao(corpo.nome) || corpo.usuario.trim().slice(0, 32) || usuario;
      const conta = {
        id: crypto.randomBytes(8).toString('hex'),
        usuario, nome,
        salt, hash, criadaEm: Date.now(),
        // O perfil nasce vazio de propósito: a fatia 3 é que preenche isso, e
        // um campo que ninguém escreve ainda não precisa de formato agora.
        perfil: {}
      };
      contas.set(usuario, conta);
      saveContas();
      abreSessao(res, { tipo: 'local', id: conta.id, usuario, name: conta.nome, avatar: null });
      responde(res, 200, { ok: true, nome: conta.nome, usuario });
      return;
    }

    const conta = contas.get(usuario);
    // Hash mesmo sem conta, e comparação em tempo constante: sem isso o tempo
    // de resposta diz quais nomes de usuário existem.
    const salt = conta ? conta.salt : 'sal-de-mentira-pra-gastar-o-mesmo-tempo';
    const calculado = await hashDeSenha(senha, salt);
    const guardado = conta ? Buffer.from(conta.hash, 'base64') : Buffer.alloc(TAMANHO_DO_HASH);
    const bate = conta
      && guardado.length === calculado.length
      && crypto.timingSafeEqual(calculado, guardado);
    if (!bate) {
      contaErro(usuario);
      responde(res, 401, { erro: 'Usuário ou senha não confere.' });
      return;
    }
    tentativasDeLogin.delete(usuario);
    abreSessao(res, { tipo: 'local', id: conta.id, usuario, name: conta.nome, avatar: avatarUrlDaConta(conta) });
    responde(res, 200, { ok: true, nome: conta.nome, usuario });
    return;
  }

  // Lista de salas com gente dentro agora, pra tela de entrada — nome da
  // sala já É a chave do Map, então nunca existe uma sala "duplicada" com
  // esse nome: entrar com um nome já ativo sempre cai na mesma sala.
  // Atividade dos amigos (bot Discord) — quem tá online e jogando o quê.
  // Onde o app conta o que está fazendo FORA da chamada. Dentro dela quem
  // carrega isso é o `activity-state` do WebSocket; aqui é o caminho da tela
  // inicial, onde nem existe WebSocket aberto.
  if (url.pathname === '/api/atividade' && req.method === 'POST') {
    const cookies = parseCookies(req);
    const session = cookies.session && sessions.get(cookies.session);
    if (!session) { responde(res, 401, { erro: 'Entre na sua conta primeiro.' }); return; }
    marcaVisto(session);
    const corpo = await leJsonDoCorpo(req);
    guardaAtividade(chaveDeIdentidade(uidDaSessao(session), session.name), limpaAtividade(corpo && corpo.atividade));
    responde(res, 200, { ok: true });
    return;
  }

  if (url.pathname === '/api/friends-activity') {
    const cookies = parseCookies(req);
    const session = cookies.session && sessions.get(cookies.session);
    if (!session) { res.writeHead(401, { 'Content-Type': 'application/json' }); res.end('{"error":"not logged in"}'); return; }
    marcaVisto(session);
    // A atividade detectada pelo próprio app ganha da que o bot do Discord
    // reporta: ela é mais fresca, sabe de música (o bot só vê jogo) e existe
    // mesmo pra quem não deixou o Discord aberto.
    const lista = [...friendsActivity.values()].map(amigo => {
      const nativa = atividadeDe('discord:' + amigo.id);
      return nativa ? { ...amigo, atividade: nativa } : amigo;
    });
    // Conta local não está em guild nenhuma do Discord, então não vem do bot —
    // entra aqui quem estiver com alguma atividade no ar agora. Sem isso a
    // tela inicial não mostraria nada de quem entrou sem Discord.
    const jaListados = new Set([...friendsActivity.keys()].map(id => 'discord:' + id));
    for (const [chave, reg] of atividadePorChave) {
      if (jaListados.has(chave) || !chave.startsWith('local:')) continue;
      if (Date.now() - reg.ts > VALIDADE_DA_ATIVIDADE_MS) continue;
      const conta = [...contas.values()].find(c => 'local:' + c.id === chave);
      if (!conta) continue;
      lista.push({
        id: chave, name: conta.nome, avatar: avatarUrlDaConta(conta),
        // O status é o que a pessoa escolheu, não um "online" fixo: quem pôs
        // "não perturbe" apareceria disponível aqui enquanto o ícone dela
        // dentro da chamada diria o contrário.
        status: statusParaOutros(chave), activity: null, atividade: statusDe(chave) === 'invisivel' ? null : reg.atividade
      });
    }
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify(lista));
    return;
  }

  // --- Status escolhido à mão -------------------------------------------
  // Fora da chamada é por aqui; dentro dela o mesmo estado viaja pelo
  // WebSocket (`status-state`), como mic e surdez. Os dois escrevem no mesmo
  // lugar de propósito — ver `guardaStatus`.
  if (url.pathname === '/api/status' && req.method === 'POST') {
    const cookies = parseCookies(req);
    const session = cookies.session && sessions.get(cookies.session);
    if (!session) { responde(res, 401, { erro: 'Entre na sua conta primeiro.' }); return; }
    const corpo = await leJsonDoCorpo(req);
    const status = limpaStatus(corpo && corpo.status);
    if (!status) { responde(res, 400, { erro: 'Status inválido.' }); return; }
    marcaVisto(session);
    const chave = chaveDeIdentidade(uidDaSessao(session), session.name);
    guardaStatus(session, chave, status);
    responde(res, 200, { ok: true, status });
    return;
  }

  if (url.pathname === '/api/status-custom' && req.method === 'POST') {
    const cookies = parseCookies(req);
    const session = cookies.session && sessions.get(cookies.session);
    if (!session || session.tipo !== 'local') { responde(res, 401, { erro: 'Entre na sua conta primeiro.' }); return; }
    const conta = contas.get(session.usuario);
    if (!conta) { responde(res, 404, { erro: 'Conta não encontrada.' }); return; }
    marcaVisto(session);
    const corpo = await leJsonDoCorpo(req);
    const novo = limpaStatusCustom(corpo && corpo.texto, corpo && corpo.emoji, corpo && corpo.minutos);
    conta.perfil = conta.perfil || {};
    if (novo) conta.perfil.statusCustom = novo; else delete conta.perfil.statusCustom;
    saveContas();
    avisaStatusCustom(conta);
    responde(res, 200, { ok: true, statusCustom: novo });
    return;
  }

  // --- Conversas e grupos (v3) ------------------------------------------
  // Conversa direta (só entre amigos) e grupo (sem limite de pessoas). Toda
  // rota exige sessão de conta local — é ela que dá o uid que decide quem é
  // membro. Quem manda o uid dos outros é o cliente, mas só passam amigos de
  // quem pede: o servidor confere em `ehAmigo`, nunca confia no texto.
  if (url.pathname === '/api/conversas' || url.pathname.startsWith('/api/conversas/')
      || url.pathname === '/api/grupos' || url.pathname.startsWith('/api/grupos/')
      || url.pathname.startsWith('/api/convites/')
      || url.pathname === '/api/favoritos' || url.pathname.startsWith('/api/favoritos/')
      || url.pathname === '/api/arquivo' || url.pathname.startsWith('/api/arquivo/')
      || url.pathname === '/api/wordle' || url.pathname.startsWith('/api/wordle/')) {
    const cookies = parseCookies(req);
    const session = cookies.session && sessions.get(cookies.session);
    if (!session) { responde(res, 401, { erro: 'Entre na sua conta primeiro.' }); return; }
    marcaVisto(session);
    const meuUid = uidDaSessao(session);
    if (session.tipo !== 'local' || !meuUid) {
      responde(res, 409, { erro: 'Conversas são para quem tem conta no app.' });
      return;
    }
    const uidsValidos = lista => (Array.isArray(lista) ? lista : [])
      .filter((u, i, a) => typeof u === 'string' && a.indexOf(u) === i && u !== meuUid);

    if (url.pathname === '/api/conversas' && req.method === 'GET') {
      const lista = conversasDe(meuUid).map(c => conversaParaCliente(c, meuUid));
      // Mais recente primeiro: última mensagem, ou a criação se ainda calada.
      const quando = c => (c.ultimaMensagem && c.ultimaMensagem.ts) || c.criadaEm;
      lista.sort((a, b) => quando(b) - quando(a));
      responde(res, 200, { conversas: lista });
      return;
    }

    if (url.pathname === '/api/conversas/dm' && req.method === 'POST') {
      const corpo = await leJsonDoCorpo(req);
      const alvo = typeof corpo?.uid === 'string' ? corpo.uid : '';
      if (!alvo || alvo === meuUid || !contaPorUid(alvo)) { responde(res, 404, { erro: 'Pessoa não encontrada.' }); return; }
      if (!ehAmigo(meuUid, alvo)) { responde(res, 403, { erro: 'Só dá para conversar com amigos.' }); return; }
      const existia = conversas.has(idDeDm(meuUid, alvo));
      const c = abreConversaDireta(meuUid, alvo);
      if (!existia) avisaMembros(c, 'conversa-nova');
      responde(res, 200, { conversa: conversaParaCliente(c, meuUid) });
      return;
    }

    // --- Wordle ---------------------------------------------------------------
    if (url.pathname === '/api/wordle' && req.method === 'GET') {
      if (!wordleRespostas.length) { responde(res, 503, { erro: 'Wordle indisponível neste servidor.' }); return; }
      const dia = url.searchParams.get('dia') || wordleChaveAtual();
      if (!wordleChaveValida(dia)) { responde(res, 400, { erro: 'Rodada do Wordle inválida.' }); return; }
      responde(res, 200, wordleEstadoDe(meuUid, dia));
      return;
    }
    if (url.pathname === '/api/wordle/dev/trocar' && req.method === 'POST') {
      const conta = contas.get(session.usuario) || contaPorUid(meuUid);
      // Só o login (único); o nome de exibição qualquer um pode trocar para "val47".
      const usuario = (session.usuario || conta?.usuario || '').toLowerCase();
      if (usuario !== 'val47') {
        responde(res, 403, { erro: 'Apenas o desenvolvedor pode alterar a palavra do Wordle.' });
        return;
      }
      if (!wordleRespostas.length) {
        responde(res, 503, { erro: 'Wordle indisponível neste servidor.' });
        return;
      }
      const corpo = await leJsonDoCorpo(req);
      const diaBase = wordleDia();
      const chaveAtual = wordleChaveAtual();
      const respostaAtual = wordlePalavraDoDia(chaveAtual);
      let nova = respostaAtual;
      if (wordleRespostas.length > 1) {
        for (let i = 0; i < 50; i++) {
          const idx = Math.floor(Math.random() * wordleRespostas.length);
          if (wordleRespostas[idx] !== respostaAtual) {
            nova = wordleRespostas[idx];
            break;
          }
        }
      }
      wordleDados.rodadas = wordleDados.rodadas || {};
      const novaRodada = ((wordleDados.rodadas[diaBase] || 1) + 1);
      wordleDados.rodadas[diaBase] = novaRodada;
      const novaChave = `${diaBase}_r${novaRodada}`;
      (wordleDados.overrides = wordleDados.overrides || {})[novaChave] = nova;
      wordleDados.dias[novaChave] = {};
      saveWordle();

      // Envia uma nova mensagem do Wordle no chat do grupo
      if (corpo && typeof corpo.conversa === 'string') {
        const c = conversaPorId(corpo.conversa);
        if (c && c.tipo === 'grupo') {
          const ts = Date.now();
          const entrada = {
            id: crypto.randomBytes(6).toString('hex'),
            uid: 'app:wordle',
            name: 'Wordle',
            app: 'wordle',
            wordle: { dia: novaChave, numero: wordleNumero(diaBase), rodada: novaRodada },
            iniciadoPor: session.name,
            ts
          };
          appendMessage(c.id, entrada);
          const pub = JSON.stringify({ type: 'chat', id: entrada.id, uid: entrada.uid, name: 'Wordle', app: 'wordle', wordle: entrada.wordle, ts });
          const sala = rooms.get(c.id);
          if (sala) for (const [, peer] of sala) safeSend(peer.ws, pub);
          avisaUids(c.membros, { type: 'mensagem', conversa: c.id, autor: 'Wordle', previa: `Nova palavra do Wordle disponível!`, ts });
        }
      }
      // Não devolve a palavra para não revelar o segredo
      responde(res, 200, { ok: true, dia: novaChave, rodada: novaRodada });
      return;
    }
    if (url.pathname === '/api/wordle/tentar' && req.method === 'POST') {
      if (!wordleRespostas.length) { responde(res, 503, { erro: 'Wordle indisponível neste servidor.' }); return; }
      const corpo = await leJsonDoCorpo(req);
      const palavra = typeof corpo?.palavra === 'string' ? corpo.palavra.trim().toLowerCase() : '';
      const dia = (corpo && typeof corpo.dia === 'string' && corpo.dia) || wordleChaveAtual();
      if (!wordleChaveValida(dia)) { responde(res, 400, { codigo: 'rodada', erro: 'Rodada do Wordle inválida.' }); return; }
      const dias = wordleDados.dias;
      const reg = ((dias[dia] = dias[dia] || {})[meuUid] = dias[dia][meuUid] || { tentativas: [], fim: null });
      if (reg.fim) { responde(res, 409, { codigo: 'fim', erro: 'Você já terminou o de hoje.', ...wordleEstadoDe(meuUid, dia) }); return; }
      if (!/^[a-z]{5}$/.test(palavra)) { responde(res, 400, { codigo: 'curta', erro: 'Not enough letters' }); return; }
      if (!wordleTodas.includes(palavra)) { responde(res, 400, { codigo: 'invalida', erro: 'Not in word list' }); return; }
      reg.tentativas.push(palavra);
      const resposta = wordlePalavraDoDia(dia);
      if (palavra === resposta) reg.fim = 'ganhou';
      else if (reg.tentativas.length >= WORDLE_TENTATIVAS) reg.fim = 'perdeu';
      saveWordle();
      responde(res, 200, wordleEstadoDe(meuUid, dia));
      // Quem está com um grupo da pessoa aberto recarrega a grade da mensagem
      // (só o dia vai junto; as cores vêm pelo GET, que confere o membro).
      const avisoGrade = JSON.stringify({ type: 'wordle-grupo', dia });
      for (const c of conversasDe(meuUid)) {
        if (c.tipo !== 'grupo') continue;
        const sala = rooms.get(c.id);
        if (sala) for (const [, peer] of sala) safeSend(peer.ws, avisoGrade);
      }
      return;
    }
    const mw = url.pathname.match(/^\/api\/conversas\/([^/]+)\/wordle$/);
    if (mw) {
      const c = conversaPorId(decodeURIComponent(mw[1]));
      if (!c || !ehMembro(c, meuUid)) { responde(res, 404, { erro: 'Conversa não encontrada.' }); return; }
      if (c.tipo !== 'grupo') { responde(res, 400, { erro: 'O Wordle só está disponível em grupos.' }); return; }
      if (!wordleRespostas.length) { responde(res, 503, { erro: 'Wordle indisponível neste servidor.' }); return; }
      const dia = url.searchParams.get('dia') || wordleChaveAtual();
      if (!wordleChaveValida(dia)) { responde(res, 400, { erro: 'Rodada do Wordle inválida.' }); return; }
      if (req.method === 'GET') { responde(res, 200, wordleDoGrupo(c, dia)); return; }
      if (req.method === 'POST') {
        // Uma mensagem de "app" por conversa e por rodada/dia.
        const jaTem = readRoomEntries(c.id).slice(-200).find(e => e.app === 'wordle' && e.wordle && String(e.wordle.dia) === String(dia));
        if (jaTem) { responde(res, 200, { ok: true, id: entryId(jaTem), existia: true }); return; }
        const ts = Date.now();
        const entrada = { id: crypto.randomBytes(6).toString('hex'), uid: 'app:wordle', name: 'Wordle', app: 'wordle',
          wordle: { dia, numero: wordleNumero(dia) }, iniciadoPor: session.name, ts };
        appendMessage(c.id, entrada);
        const pub = JSON.stringify({ type: 'chat', id: entrada.id, uid: entrada.uid, name: 'Wordle', app: 'wordle', wordle: entrada.wordle, ts });
        const sala = rooms.get(c.id);
        if (sala) for (const [, peer] of sala) safeSend(peer.ws, pub);
        avisaUids(c.membros, { type: 'mensagem', conversa: c.id, autor: 'Wordle', previa: `${session.name} está jogando Wordle`, ts });
        responde(res, 200, { ok: true, id: entrada.id, existia: false });
        return;
      }
    }

    // --- Favoritos (por pessoa) ---------------------------------------------
    if (url.pathname === '/api/favoritos' && req.method === 'GET') {
      const lista = (favoritos.get(meuUid) || []).filter(f => {
        const c = conversaPorId(f.conversa);
        return c && ehMembro(c, meuUid);   // saiu do grupo: some da lista (volta se entrar de novo)
      });
      responde(res, 200, { favoritos: lista.slice().sort((a, b) => b.em - a.em) });
      return;
    }
    if (url.pathname === '/api/favoritos' && req.method === 'POST') {
      const corpo = await leJsonDoCorpo(req);
      const c = conversaPorId(typeof corpo?.conversa === 'string' ? corpo.conversa : '');
      if (!c || !ehMembro(c, meuUid)) { responde(res, 404, { erro: 'Conversa não encontrada.' }); return; }
      const alvo = typeof corpo?.mensagem === 'string' ? corpo.mensagem : '';
      const e = readRoomEntries(c.id).find(x => !x.sys && entryId(x) === alvo);
      if (!e) { responde(res, 404, { erro: 'Mensagem não encontrada.' }); return; }
      const lista = favoritos.get(meuUid) || [];
      if (!lista.some(f => f.conversa === c.id && f.id === alvo)) {
        lista.push(retratoDeFavorito(c.id, e));
        favoritos.set(meuUid, lista.slice(-LIMITE_FAVORITOS));
        saveFavoritos();
      }
      responde(res, 200, { ok: true });
      return;
    }
    const mf = url.pathname.match(/^\/api\/favoritos\/([^/]+)\/([^/]+)$/);
    if (mf && req.method === 'DELETE') {
      const conv = decodeURIComponent(mf[1]), idm = decodeURIComponent(mf[2]);
      const lista = favoritos.get(meuUid) || [];
      favoritos.set(meuUid, lista.filter(f => !(f.conversa === conv && f.id === idm)));
      saveFavoritos();
      responde(res, 200, { ok: true });
      return;
    }

    // --- Encaminhar mensagem ---------------------------------------------
    // O servidor copia a mensagem do histórico de origem (o cliente só diz
    // qual): ninguém forja "encaminhada" de outra pessoa. Texto, imagem ou GIF;
    // enquete e mensagem de sistema não se encaminham.
    const me = url.pathname.match(/^\/api\/conversas\/([^/]+)\/encaminhar$/);
    if (me && req.method === 'POST') {
      const destino = conversaPorId(decodeURIComponent(me[1]));
      const corpo = await leJsonDoCorpo(req);
      const origem = conversaPorId(typeof corpo?.origem === 'string' ? corpo.origem : '');
      if (!destino || !ehMembro(destino, meuUid)) { responde(res, 404, { erro: 'Conversa não encontrada.' }); return; }
      if (!origem || !ehMembro(origem, meuUid)) { responde(res, 404, { erro: 'Mensagem não encontrada.' }); return; }
      const alvoId = typeof corpo?.mensagem === 'string' ? corpo.mensagem : '';
      const orig = readRoomEntries(origem.id).find(e => !e.sys && entryId(e) === alvoId);
      if (!orig) { responde(res, 404, { erro: 'Mensagem não encontrada.' }); return; }
      if (orig.enquete) { responde(res, 400, { erro: 'Enquetes não podem ser encaminhadas.' }); return; }
      if (!orig.text && !orig.imageId && !orig.gif) { responde(res, 400, { erro: 'Nada para encaminhar.' }); return; }
      let imageId = null;
      if (orig.imageId) {
        const buf = loadImage(orig.imageId);
        if (buf) imageId = saveImage(buf);   // cópia: apagar a original não some com esta
      }
      const ts = Date.now();
      const id = crypto.randomBytes(6).toString('hex');
      // Quem foi o autor original: o nome fica gravado (a conta pode mudar de nome).
      const fwd = { nome: String(orig.name || '').slice(0, 64), uid: orig.uid || undefined, ts: orig.ts || undefined };
      const entrada = { id, from: undefined, uid: meuUid, name: session.name, text: orig.text || undefined,
        imageId: imageId || undefined, imageMime: imageId ? orig.imageMime : undefined, gif: orig.gif || undefined, fwd, ts };
      appendMessage(destino.id, entrada);
      const publica = e => {
        const out = { type: 'chat', id: e.id, uid: e.uid, name: e.name, text: e.text, gif: e.gif, fwd: e.fwd, ts: e.ts };
        if (e.imageId) { const b = loadImage(e.imageId); if (b) out.image = { mime: e.imageMime, data: b.toString('base64') }; }
        return JSON.stringify(out);
      };
      const sala = rooms.get(destino.id);
      const pub = publica(entrada);
      if (sala) for (const [, peer] of sala) safeSend(peer.ws, pub);
      const previaFwd = entrada.text ? entrada.text.replace(/\s+/g, ' ').slice(0, 80) : (imageId ? '[imagem]' : '[GIF]');
      avisaUids(destino.membros, { type: 'mensagem', conversa: destino.id, autor: session.name, uid: meuUid, previa: previaFwd, ts });
      // Mensagem opcional, depois, como texto comum.
      const extra = typeof corpo?.extra === 'string' ? corpo.extra.slice(0, 2000).replace(/\n{3,}/g, '\n\n').trim() : '';
      if (extra) {
        const e2 = { id: crypto.randomBytes(6).toString('hex'), uid: meuUid, name: session.name, text: extra, ts: ts + 1 };
        appendMessage(destino.id, e2);
        const p2 = JSON.stringify({ type: 'chat', id: e2.id, uid: e2.uid, name: e2.name, text: e2.text, ts: e2.ts });
        if (sala) for (const [, peer] of sala) safeSend(peer.ws, p2);
        avisaUids(destino.membros, { type: 'mensagem', conversa: destino.id, autor: session.name, uid: meuUid, previa: extra.replace(/\s+/g, ' ').slice(0, 80), ts: ts + 1 });
      }
      responde(res, 200, { ok: true });
      return;
    }

    if (url.pathname === '/api/grupos' && req.method === 'POST') {
      const corpo = await leJsonDoCorpo(req);
      const outros = uidsValidos(corpo?.membros);
      if (!outros.length) { responde(res, 400, { erro: 'Escolha pelo menos uma pessoa.' }); return; }
      if (outros.some(u => !contaPorUid(u))) { responde(res, 404, { erro: 'Alguém da lista não existe.' }); return; }
      if (outros.some(u => !ehAmigo(meuUid, u))) { responde(res, 403, { erro: 'Só dá para criar grupo com amigos.' }); return; }
      const c = criaGrupo(meuUid, outros, nomePadraoDeGrupo([meuUid, ...outros]));
      registraSistema(c, 'grupo', `${session.name} criou o grupo.`);
      avisaMembros(c, 'conversa-nova');
      responde(res, 200, { conversa: conversaParaCliente(c, meuUid) });
      return;
    }

    // --- Convite por link ---------------------------------------------
    // Quem abre o link logado entra no grupo; amizade não é exigida (o link
    // É o convite). O token tem 192 bits, é por grupo, e o dono troca (o
    // antigo morre) ou revoga.
    const mc = url.pathname.match(/^\/api\/convites\/([0-9a-f]{48})$/);
    if (mc) {
      const alvo = [...conversas.values()].find(c => c.tipo === 'grupo' && c.convite === mc[1]);
      if (!alvo) { responde(res, 404, { erro: 'Convite inválido ou expirado.' }); return; }
      if (req.method === 'GET') {
        responde(res, 200, { id: alvo.id, nome: alvo.nome, icone: alvo.icone && alvo.icone.gif ? alvo.icone.gif : null,
          membros: alvo.membros.length, jaMembro: ehMembro(alvo, meuUid) });
        return;
      }
      if (req.method === 'POST') {
        if (!ehMembro(alvo, meuUid)) {
          alvo.membros.push(meuUid);
          saveConversas();
          registraSistema(alvo, 'grupo', `${session.name} entrou pelo link de convite.`);
          // Quem entrou recebe `conversa-nova`; os de antes, `conversa-mudou`.
          avisaUids([meuUid], { type: 'conversa-nova', conversa: conversaParaCliente(alvo, meuUid) });
          avisaUids(alvo.membros.filter(u => u !== meuUid),
            uid => ({ type: 'conversa-mudou', conversa: conversaParaCliente(alvo, uid) }));
        }
        responde(res, 200, { conversa: conversaParaCliente(alvo, meuUid) });
        return;
      }
    }

    // --- Arquivos do chat ---------------------------------------------------
    if (url.pathname === '/api/arquivo' && req.method === 'POST') {
      const conv = conversaPorId(url.searchParams.get('conversa'));
      if (!conv || !ehMembro(conv, meuUid)) { responde(res, 404, { erro: 'Conversa não encontrada.' }); return; }
      const tam = Number(req.headers['content-length']);
      if (!Number.isFinite(tam) || tam <= 0) { responde(res, 411, { erro: 'Tamanho do arquivo não informado.' }); return; }
      if (tam > MAX_ARQUIVO_BYTES) { responde(res, 413, { erro: 'Arquivo grande demais (máx. 200MB).' }); return; }
      const meus = [...arquivosPendentes.values()].filter(p => p.uid === meuUid).length;
      if (meus >= 5) { responde(res, 429, { erro: 'Muitos envios pendentes. Tente de novo em instantes.' }); return; }
      try {
        const st = fs.statfsSync(CHAT_DIR);
        if (st.bavail * st.bsize < tam + RESERVA_DE_DISCO_BYTES) { responde(res, 507, { erro: 'Sem espaço no servidor.' }); return; }
      } catch { /* sem statfs: segue */ }
      const nome = nomeDeArquivoLimpo(url.searchParams.get('nome'));
      const id = crypto.randomBytes(16).toString('hex');
      const dir = dirDeArquivos(conv.id);
      fs.mkdirSync(dir, { recursive: true });
      const destino = path.join(dir, id);
      const parcial = destino + '.part';
      const saida = fs.createWriteStream(parcial);
      let recebido = 0;
      let abortado = false;
      const aborta = (status, erro) => {
        if (abortado) return;
        abortado = true;
        saida.destroy();
        try { fs.unlinkSync(parcial); } catch { /* ok */ }
        if (!res.headersSent) responde(res, status, { erro });
        req.destroy();
      };
      req.on('data', pedaco => {
        recebido += pedaco.length;
        if (recebido > MAX_ARQUIVO_BYTES || recebido > tam) aborta(413, 'Arquivo grande demais (máx. 200MB).');
      });
      req.on('error', () => aborta(400, 'Envio interrompido.'));
      req.on('aborted', () => aborta(400, 'Envio interrompido.'));
      saida.on('error', () => aborta(500, 'Não consegui gravar o arquivo.'));
      saida.on('finish', () => {
        if (abortado) return;
        try { fs.renameSync(parcial, destino); } catch { aborta(500, 'Não consegui gravar o arquivo.'); return; }
        arquivosPendentes.set(id, { uid: meuUid, conversa: conv.id, nome, tam: recebido, em: Date.now() });
        responde(res, 200, { arquivoId: id, nome, tam: recebido });
      });
      req.pipe(saida);
      return;
    }
    const ma = url.pathname.match(/^\/api\/arquivo\/([^/]+)\/([0-9a-f]{32})$/);
    if (ma && req.method === 'GET') {
      const conv = conversaPorId(decodeURIComponent(ma[1]));
      if (!conv || !ehMembro(conv, meuUid)) { responde(res, 404, { erro: 'Arquivo não encontrado.' }); return; }
      const entrada = readRoomEntries(conv.id).find(e => e.arquivo && e.arquivo.id === ma[2]);
      if (!entrada) { responde(res, 404, { erro: 'Arquivo não encontrado.' }); return; }
      const caminho = path.join(dirDeArquivos(conv.id), ma[2]);
      let st; try { st = fs.statSync(caminho); } catch { responde(res, 404, { erro: 'Arquivo não encontrado.' }); return; }
      res.writeHead(200, {
        'Content-Type': 'application/octet-stream',
        'Content-Length': st.size,
        'Content-Disposition': `attachment; filename*=UTF-8''${encodeURIComponent(entrada.arquivo.nome)}`,
        'Cache-Control': 'private, no-store',
        'X-Content-Type-Options': 'nosniff'
      });
      fs.createReadStream(caminho).pipe(res);
      return;
    }

    const mg = url.pathname.match(/^\/api\/grupos\/([^/]+)(?:\/(icone|membros|sair|convite)(?:\/([^/]+))?)?$/);
    if (mg) {
      const c = conversaPorId(decodeURIComponent(mg[1]));
      // 404 igual pra "não existe" e "não é seu": não confirma id alheio.
      if (!c || c.tipo !== 'grupo' || !ehMembro(c, meuUid)) { responde(res, 404, { erro: 'Grupo não encontrado.' }); return; }
      const parte = mg[2] || '';

      // Apagar o grupo: só o dono. Some o chat inteiro, as imagens e os
      // arquivos — pra sempre. Quem estiver conectado é desconectado.
      if (!parte && req.method === 'DELETE') {
        if (c.dono !== meuUid) { responde(res, 403, { erro: 'Só o dono do grupo pode apagá-lo.' }); return; }
        const membros = [...c.membros];
        for (const e of readRoomEntries(c.id)) { if (e.imageId) deleteImage(e.imageId); }
        for (const [id, p] of arquivosPendentes) if (p.conversa === c.id) arquivosPendentes.delete(id);
        try { fs.rmSync(dirDeArquivos(c.id), { recursive: true, force: true }); } catch { /* sem pasta */ }
        writeRoomEntries(c.id, []);
        for (const u of membros) expulsaDaConversa(c.id, u);
        conversas.delete(c.id);
        saveConversas();
        avisaUids(membros, { type: 'conversa-saiu', conversa: c.id });
        responde(res, 200, { ok: true });
        return;
      }

      if (!parte && req.method === 'POST') {
        const corpo = await leJsonDoCorpo(req);
        const nome = typeof corpo?.nome === 'string' ? corpo.nome.replace(/[\r\n\t]/g, ' ').trim() : '';
        if (!nome || nome.length > MAX_NOME_DE_GRUPO) {
          responde(res, 400, { erro: `O nome precisa ter de 1 a ${MAX_NOME_DE_GRUPO} caracteres.` });
          return;
        }
        c.nome = nome;
        saveConversas();
        registraSistema(c, 'grupo', `${session.name} mudou o nome do grupo para "${nome}".`);
        avisaMembros(c, 'conversa-mudou');
        responde(res, 200, { conversa: conversaParaCliente(c, meuUid) });
        return;
      }

      if (parte === 'icone' && req.method === 'GET') {
        const buf = c.icone && c.icone.imagemId ? loadImage(c.icone.imagemId) : null;
        if (!buf) { responde(res, 404, { erro: 'Sem ícone.' }); return; }
        res.writeHead(200, { 'Content-Type': c.icone.mime || sniffImageMime(buf) || 'application/octet-stream', 'Cache-Control': 'private, max-age=86400' });
        res.end(buf);
        return;
      }

      if (parte === 'icone' && req.method === 'POST') {
        const corpo = await leJsonDoCorpo(req, MAX_IMAGE_BYTES * 2);
        let novo = null;
        if (typeof corpo?.gif === 'string') {
          const gif = limpaUrlDeGif(corpo.gif);
          if (!gif) { responde(res, 400, { erro: 'GIF não aceito.' }); return; }
          novo = { gif };
        } else {
          let buf; try { buf = Buffer.from(typeof corpo?.data === 'string' ? corpo.data : '', 'base64'); } catch { buf = null; }
          if (!buf || !buf.length || buf.length > MAX_IMAGE_BYTES) { responde(res, 400, { erro: 'Imagem inválida ou grande demais.' }); return; }
          const mime = sniffImageMime(buf);
          if (!mime || !ALLOWED_IMAGE_MIME.has(mime)) { responde(res, 400, { erro: 'Formato de imagem não reconhecido.' }); return; }
          novo = { imagemId: saveImage(buf), mime };
        }
        const antigo = c.icone && c.icone.imagemId;
        c.icone = novo;
        saveConversas();
        if (antigo) deleteImage(antigo);   // depois de salvar o novo, como no avatar
        registraSistema(c, 'grupo', `${session.name} mudou o ícone do grupo.`);
        avisaMembros(c, 'conversa-mudou');
        responde(res, 200, { conversa: conversaParaCliente(c, meuUid) });
        return;
      }

      if (parte === 'icone' && req.method === 'DELETE') {
        const antigo = c.icone && c.icone.imagemId;
        c.icone = null;
        saveConversas();
        if (antigo) deleteImage(antigo);
        avisaMembros(c, 'conversa-mudou');
        responde(res, 200, { conversa: conversaParaCliente(c, meuUid) });
        return;
      }

      if (parte === 'membros' && !mg[3] && req.method === 'POST') {
        const corpo = await leJsonDoCorpo(req);
        const novos = uidsValidos(corpo?.uids).filter(u => !ehMembro(c, u));
        if (!novos.length) { responde(res, 400, { erro: 'Escolha alguém que ainda não está no grupo.' }); return; }
        if (novos.some(u => !contaPorUid(u))) { responde(res, 404, { erro: 'Alguém da lista não existe.' }); return; }
        if (novos.some(u => !ehAmigo(meuUid, u))) { responde(res, 403, { erro: 'Só dá para adicionar seus amigos.' }); return; }
        c.membros.push(...novos);
        saveConversas();
        const nomes = novos.map(u => contaPorUid(u)?.nome).filter(Boolean).join(', ');
        registraSistema(c, 'grupo', `${session.name} adicionou ${nomes}.`);
        avisaMembros(c, 'conversa-mudou', { novos });
        responde(res, 200, { conversa: conversaParaCliente(c, meuUid) });
        return;
      }

      if (parte === 'membros' && mg[3] && req.method === 'DELETE') {
        const alvo = decodeURIComponent(mg[3]);
        if (c.dono !== meuUid) { responde(res, 403, { erro: 'Só o dono do grupo remove pessoas.' }); return; }
        if (alvo === meuUid) { responde(res, 400, { erro: 'Para você sair, use "Sair do grupo".' }); return; }
        if (!ehMembro(c, alvo)) { responde(res, 404, { erro: 'Essa pessoa não está no grupo.' }); return; }
        c.membros = c.membros.filter(u => u !== alvo);
        saveConversas();
        expulsaDaConversa(c.id, alvo);
        registraSistema(c, 'grupo', `${session.name} removeu ${contaPorUid(alvo)?.nome || 'alguém'} do grupo.`);
        avisaMembros(c, 'conversa-mudou', { removidos: [alvo] });
        responde(res, 200, { conversa: conversaParaCliente(c, meuUid) });
        return;
      }

      if (parte === 'convite' && (req.method === 'POST' || req.method === 'DELETE')) {
        if (c.dono !== meuUid) { responde(res, 403, { erro: 'Só o dono do grupo gerencia o link de convite.' }); return; }
        c.convite = req.method === 'POST' ? crypto.randomBytes(24).toString('hex') : null;
        saveConversas();
        avisaMembros(c, 'conversa-mudou');
        responde(res, 200, { convite: c.convite });
        return;
      }

      if (parte === 'sair' && req.method === 'POST') {
        c.membros = c.membros.filter(u => u !== meuUid);
        // Posse passa ao membro mais antigo (a lista é por ordem de entrada).
        // Grupo que ficou vazio continua guardado, sem dono.
        if (c.dono === meuUid) c.dono = c.membros[0] || null;
        saveConversas();
        expulsaDaConversa(c.id, meuUid);
        registraSistema(c, 'grupo', `${session.name} saiu do grupo.`);
        avisaMembros(c, 'conversa-mudou', { removidos: [meuUid] });
        responde(res, 200, { ok: true });
        return;
      }
    }

    responde(res, 404, { erro: 'rota' });
    return;
  }

  // --- Relatório de problemas (log do app, enviado ao fechar) -----------------
  // Texto puro, até 1MB, só com sessão. Fica em ./diagnosticos/, um arquivo por
  // envio, fora de chatdata/. Guarda os 20 mais novos de cada pessoa.
  if (url.pathname === '/api/relatorio' && req.method === 'POST') {
    const cookies = parseCookies(req);
    const session = cookies.session && sessions.get(cookies.session);
    if (!session) { responde(res, 401, { erro: 'Entre na sua conta primeiro.' }); return; }
    const uid = uidDaSessao(session) || 'anon';
    const partes = []; let total = 0; let estourou = false;
    await new Promise(resolve => {
      req.on('data', p => {
        total += p.length;
        if (total > 1024 * 1024) { estourou = true; req.destroy(); resolve(); return; }
        partes.push(p);
      });
      req.on('end', resolve);
      req.on('error', resolve);
    });
    if (estourou) { responde(res, 413, { erro: 'Relatório grande demais.' }); return; }
    try {
      const dir = path.join(__dirname, 'diagnosticos');
      fs.mkdirSync(dir, { recursive: true, mode: 0o700 });
      const quem = uid.replace(/[^a-zA-Z0-9_-]/g, '_').slice(0, 60);
      const versao = String(req.headers['x-lincast-versao'] || '?').replace(/[^0-9a-zA-Z._-]/g, '').slice(0, 20);
      const nome = `${quem}-${new Date().toISOString().replace(/[:.]/g, '-')}-v${versao}.log`;
      fs.writeFileSync(path.join(dir, nome), Buffer.concat(partes), { mode: 0o600 });
      const meus = fs.readdirSync(dir).filter(f => f.startsWith(quem + '-')).sort();
      for (const velho of meus.slice(0, Math.max(0, meus.length - 20))) {
        try { fs.unlinkSync(path.join(dir, velho)); } catch {}
      }
      responde(res, 200, { ok: true });
    } catch (e) {
      console.error('Relatório não salvo:', e.message);
      responde(res, 500, { erro: 'Não consegui guardar o relatório.' });
    }
    return;
  }

  // --- Amigos do app -----------------------------------------------------
  // Pedido, aceite e remoção. Tudo entre uids; a busca é pelo nome de usuário,
  // que é a única coisa única e digitável que existe.
  if (url.pathname.startsWith('/api/amigos')) {
    const cookies = parseCookies(req);
    const session = cookies.session && sessions.get(cookies.session);
    if (!session) { responde(res, 401, { erro: 'Entre na sua conta primeiro.' }); return; }
    marcaVisto(session);
    const meuUid = uidDaSessao(session);
    // Sem conta própria não há arroba: nem pra te acharem, nem pra você achar
    // alguém. Dizer isso é melhor do que devolver uma lista vazia que parece
    // "você não tem amigos".
    if (session.tipo !== 'local' || !meuUid) {
      responde(res, 409, { erro: 'A lista de amigos do app é pra quem tem conta aqui. Quem entra pelo Discord continua vendo a lista do servidor do Discord.' });
      return;
    }

    if (url.pathname === '/api/amigos' && req.method === 'GET') {
      const cartao = uid => { const c = contaPorUid(uid); return c ? cartaoDaConta(c, meuUid) : null; };
      const lista = [...(amizades.get(meuUid) || [])].map(cartao).filter(Boolean);
      // Online primeiro, depois quem está fazendo alguma coisa, depois por
      // nome: é a ordem que responde "com quem dá pra jogar agora".
      const peso = a => (a.status !== 'offline' ? 2 : 0) + (a.atividade ? 1 : 0);
      lista.sort((a, b) => peso(b) - peso(a) || a.name.localeCompare(b.name));
      responde(res, 200, {
        amigos: lista,
        recebidos: pedidos.filter(p => p.para === meuUid).map(p => cartao(p.de)).filter(Boolean),
        enviados: pedidos.filter(p => p.de === meuUid).map(p => cartao(p.para)).filter(Boolean)
      });
      return;
    }

    if (url.pathname === '/api/amigos/pedir' && req.method === 'POST') {
      const corpo = await leJsonDoCorpo(req);
      const alvo = contas.get(normalizaUsuario(corpo && corpo.usuario));
      // Resposta igual pra "não existe" e pra qualquer outro engano de nome:
      // sem isso a caixa de busca vira uma lista de quem tem conta aqui.
      if (!alvo) { responde(res, 404, { erro: 'Não achei ninguém com esse nome de usuário.' }); return; }
      const uidAlvo = 'local:' + alvo.id;
      if (uidAlvo === meuUid) { responde(res, 400, { erro: 'Esse é você.' }); return; }
      if (ehAmigo(meuUid, uidAlvo)) { responde(res, 409, { erro: 'Vocês já são amigos.' }); return; }
      // Pedido cruzado vira amizade na hora: os dois já disseram que sim, e
      // fazer o segundo esperar aceitação seria pedir a mesma resposta duas
      // vezes.
      if (achaPedido(uidAlvo, meuUid) >= 0) {
        removePedido(uidAlvo, meuUid);
        criaAmizade(meuUid, uidAlvo);
        saveAmizades();
        responde(res, 200, { ok: true, viraramAmigos: true });
        return;
      }
      if (achaPedido(meuUid, uidAlvo) >= 0) { responde(res, 409, { erro: 'Você já mandou um pedido pra essa pessoa.' }); return; }
      // Teto por pessoa: sem ele, uma conta consegue encher o disco de pedido
      // e a tela de todo mundo de notificação. Vinte é folgado pra uma casa de
      // cinco amigos e baixo o bastante pra não virar arma.
      if (pedidos.filter(p => p.de === meuUid).length >= 20) {
        responde(res, 429, { erro: 'Você tem pedidos demais em aberto. Espere alguém responder.' });
        return;
      }
      pedidos.push({ de: meuUid, para: uidAlvo, ts: Date.now() });
      saveAmizades();
      responde(res, 200, { ok: true });
      return;
    }

    if (url.pathname === '/api/amigos/responder' && req.method === 'POST') {
      const corpo = await leJsonDoCorpo(req);
      const deUid = typeof corpo?.uid === 'string' ? corpo.uid : '';
      if (achaPedido(deUid, meuUid) < 0) { responde(res, 404, { erro: 'Esse pedido não existe mais.' }); return; }
      removePedido(deUid, meuUid);
      if (corpo.aceita) criaAmizade(meuUid, deUid);
      saveAmizades();
      responde(res, 200, { ok: true });
      return;
    }

    if (url.pathname === '/api/amigos/remover' && req.method === 'POST') {
      const corpo = await leJsonDoCorpo(req);
      const outro = typeof corpo?.uid === 'string' ? corpo.uid : '';
      desfazAmizade(meuUid, outro);
      // Some com o pedido nos dois sentidos junto: remover alguém e continuar
      // com um pedido pendente pra mesma pessoa é estado sem sentido.
      removePedido(meuUid, outro);
      removePedido(outro, meuUid);
      saveAmizades();
      responde(res, 200, { ok: true });
      return;
    }

    responde(res, 404, { erro: 'rota' });
    return;
  }

  // --- Jogos do perfil ------------------------------------------------------
  if (url.pathname === '/api/jogos/busca' && req.method === 'GET') {
    const cookies = parseCookies(req);
    const session = cookies.session && sessions.get(cookies.session);
    if (!session || session.tipo !== 'local') { responde(res, 401, { erro: 'Entre na sua conta primeiro.' }); return; }
    if (!chaveSgdb) { responde(res, 503, { erro: 'Busca de jogos desativada neste servidor.' }); return; }
    const termo = (url.searchParams.get('q') || '').trim().slice(0, 60);
    if (termo.length < 2) { responde(res, 200, { resultados: [] }); return; }
    const resultados = await buscaJogos(termo);
    if (!resultados) { responde(res, 502, { erro: 'Não consegui buscar agora.' }); return; }
    responde(res, 200, { resultados });
    return;
  }
  const mCapa = url.pathname.match(/^\/api\/jogos\/capa\/(\d{1,9})$/);
  if (mCapa && req.method === 'GET') {
    const cookies = parseCookies(req);
    const session = cookies.session && sessions.get(cookies.session);
    if (!session || session.tipo !== 'local') { responde(res, 401, { erro: 'Entre na sua conta primeiro.' }); return; }
    if (!chaveSgdb) { responde(res, 503, { erro: 'Capas desativadas neste servidor.' }); return; }
    const buf = await capaDeJogo(Number(mCapa[1]));
    if (!buf) { responde(res, 404, { erro: 'Sem capa.' }); return; }
    res.writeHead(200, { 'Content-Type': sniffImageMime(buf) || 'application/octet-stream', 'Cache-Control': 'private, max-age=604800' });
    res.end(buf);
    return;
  }
  if (url.pathname === '/api/conta/jogos' && req.method === 'PUT') {
    const cookies = parseCookies(req);
    const session = cookies.session && sessions.get(cookies.session);
    if (!session || session.tipo !== 'local') { responde(res, 401, { erro: 'Entre na sua conta primeiro.' }); return; }
    const uid = uidDaSessao(session);
    const corpo = await leJsonDoCorpo(req, 32 * 1024);
    if (!uid || !corpo) { responde(res, 400, { erro: 'Corpo inválido.' }); return; }
    perfilExtra.listas[uid] = {
      favorito: limpaJogoDeLista(corpo.favorito),
      gosto: limpaListaDeJogos(corpo.gosto),
      quero: limpaListaDeJogos(corpo.quero)
    };
    perfilExtraSujo = true;
    savePerfilExtra();
    responde(res, 200, { ok: true, listas: listasDe(uid) });
    return;
  }

  // Perfil público de uma conta, pelo nome de usuário. É o que o cartão de
  // perfil abre pra quem NÃO está na sala com você (a lista de amigos), já que
  // ali não existe `peer` nenhum de onde tirar bio e cor.
  if (url.pathname.startsWith('/api/perfil/') && req.method === 'GET') {
    const cookies = parseCookies(req);
    const session = cookies.session && sessions.get(cookies.session);
    if (!session) { responde(res, 401, { erro: 'Entre na sua conta primeiro.' }); return; }
    const usuario = normalizaUsuario(decodeURIComponent(url.pathname.slice('/api/perfil/'.length)));
    const conta = usuario && contas.get(usuario);
    if (!conta) { responde(res, 404, { erro: 'Perfil não encontrado.' }); return; }
    const meuUid = uidDaSessao(session);
    const uid = 'local:' + conta.id;
    const cartao = cartaoDaConta(conta, meuUid);
    // Jogos e amigos em comum são só para amigos (e para a própria pessoa): a
    // coleção diz o que alguém joga, e a lista de amigos em comum, com quem anda.
    const intimo = !!meuUid && (meuUid === uid || ehAmigo(meuUid, uid));
    cartao.insignias = insigniasDe(conta);
    cartao.criadaEm = conta.criadaEm || null;
    cartao.amigoDeMim = intimo;
    cartao.jogos = intimo ? jogosDe(uid) : [];
    cartao.listas = intimo ? listasDe(uid) : { favorito: null, gosto: [], quero: [] };
    cartao.mutuos = intimo && meuUid !== uid
      ? amigosEmComum(meuUid, uid).map(u => contaPorUid(u)).filter(Boolean)
          .map(c => ({ uid: 'local:' + c.id, usuario: c.usuario, name: c.nome, avatar: avatarUrlDaConta(c) }))
      : [];
    responde(res, 200, cartao);
    return;
  }

  // Mural do perfil: recados curtos de amigos. Ler e escrever é só para amigos
  // da pessoa (e ela mesma); apagar é de quem escreveu ou do dono do mural.
  const mMural = url.pathname.match(/^\/api\/mural\/([^/]+)(?:\/([^/]+))?$/);
  if (mMural) {
    const cookies = parseCookies(req);
    const session = cookies.session && sessions.get(cookies.session);
    if (!session || session.tipo !== 'local') { responde(res, 401, { erro: 'Entre na sua conta primeiro.' }); return; }
    marcaVisto(session);
    const meuUid = uidDaSessao(session);
    const conta = contas.get(normalizaUsuario(decodeURIComponent(mMural[1])));
    if (!conta) { responde(res, 404, { erro: 'Perfil não encontrado.' }); return; }
    const dono = 'local:' + conta.id;
    if (meuUid !== dono && !ehAmigo(meuUid, dono)) { responde(res, 403, { erro: 'O mural é só para amigos.' }); return; }
    const mural = perfilExtra.mural[dono] || (perfilExtra.mural[dono] = []);
    if (req.method === 'GET' && !mMural[2]) {
      responde(res, 200, { recados: mural.slice().reverse() });
      return;
    }
    if (req.method === 'POST' && !mMural[2]) {
      const corpo = await leJsonDoCorpo(req);
      const texto = String((corpo && corpo.texto) || '').replace(/\s+/g, ' ').trim().slice(0, MAX_TEXTO_MURAL);
      if (!texto) { responde(res, 400, { erro: 'Escreva alguma coisa.' }); return; }
      const meuNome = (contaPorUid(meuUid) || {}).nome || session.name || 'Alguém';
      // Um recado por pessoa a cada 20s: o mural é de cinco amigos, não de uma fila.
      const ultimo = mural.slice().reverse().find(r => r.de === meuUid);
      if (ultimo && Date.now() - ultimo.ts < 20000) { responde(res, 429, { erro: 'Calma, espere um pouco para recadar de novo.' }); return; }
      mural.push({ id: crypto.randomBytes(6).toString('hex'), de: meuUid, deNome: meuNome, texto, ts: Date.now() });
      while (mural.length > LIMITE_MURAL) mural.shift();
      savePerfilExtra();
      responde(res, 200, { ok: true, recados: mural.slice().reverse() });
      return;
    }
    if (req.method === 'DELETE' && mMural[2]) {
      const i = mural.findIndex(r => r.id === mMural[2]);
      if (i < 0) { responde(res, 404, { erro: 'Recado não encontrado.' }); return; }
      if (mural[i].de !== meuUid && meuUid !== dono) { responde(res, 403, { erro: 'Só quem escreveu ou o dono do mural apaga.' }); return; }
      mural.splice(i, 1);
      savePerfilExtra();
      responde(res, 200, { ok: true, recados: mural.slice().reverse() });
      return;
    }
    responde(res, 405, { erro: 'método' });
    return;
  }

  // Busca de GIF. Atrás de sessão: a cota é da conta de quem hospeda, e não há
  // motivo pra um estranho gastar ela.
  if (url.pathname === '/api/gif' && req.method === 'GET') {
    const cookies = parseCookies(req);
    if (!cookies.session || !sessions.get(cookies.session)) { responde(res, 401, { erro: 'Entre primeiro.' }); return; }
    if (!chaveDeGif) { responde(res, 503, { erro: 'Busca de GIF não configurada no servidor.' }); return; }
    const termo = (url.searchParams.get('q') || '').trim().slice(0, 80);
    const resultados = await buscaGif(termo);
    if (!resultados) { responde(res, 502, { erro: 'O provedor de GIF não respondeu. Tente de novo.' }); return; }
    responde(res, 200, { resultados });
    return;
  }

  // Sem checagem de sessão de propósito: o cliente precisa disso antes mesmo
  // de entrar numa sala, e as credenciais do TURN já estavam públicas dentro
  // do index.html até agora — não é um segredo novo sendo exposto aqui.
  if (url.pathname === '/api/ice-config') {
    res.writeHead(200, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store' });
    res.end(JSON.stringify({ iceServers: await getIceServers() }));
    return;
  }

  // Métricas (item 34). Numa máquina que já sofreu OOM, saber quanto de RAM o
  // processo está usando e quanta gente está conectada é diagnóstico barato —
  // e evita ter que entrar por SSH pra descobrir. Fica atrás de sessão, igual
  // a /api/rooms: não é segredo de estado, mas também não é pra ficar aberto.
  if (url.pathname === '/api/metrics') {
    const cookies = parseCookies(req);
    const session = cookies.session && sessions.get(cookies.session);
    if (!session) { res.writeHead(401, { 'Content-Type': 'application/json' }); res.end('{"error":"not logged in"}'); return; }

    const mem = process.memoryUsage();
    let arquivosDeChat = 0, bytesDeChat = 0;
    try {
      for (const dir of [CHAT_DIR, IMAGES_DIR]) {
        for (const nome of fs.readdirSync(dir)) {
          const caminho = path.join(dir, nome);
          // Os arquivos de estado do servidor (`.salas`, `.sessoes`) moram no
          // mesmo diretório e não são chat: contá-los faz a métrica dizer que
          // existe conversa guardada onde não existe nenhuma.
          if (nome.startsWith('.')) continue;
          const st = fs.statSync(caminho);
          if (st.isFile()) { arquivosDeChat++; bytesDeChat += st.size; }
        }
      }
    } catch { /* diretório sumiu: as outras métricas continuam valendo */ }

    res.writeHead(200, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store' });
    res.end(JSON.stringify({
      uptimeSegundos: Math.round(process.uptime()),
      memoriaProcessoMB: Math.round(mem.rss / 1048576),
      heapUsadoMB: Math.round(mem.heapUsed / 1048576),
      memoriaLivreMB: Math.round(os.freemem() / 1048576),
      memoriaTotalMB: Math.round(os.totalmem() / 1048576),
      cargaMedia: os.loadavg().map(n => Number(n.toFixed(2))),
      conexoes: wss.clients.size,
      salasAtivas: [...rooms.values()].filter(r => r.size > 0).length,
      pessoasEmChamada: [...rooms.values()].reduce((total, r) => total + r.size, 0),
      picoDePessoas,
      salasComSenhaSalva: roomSecrets.size,
      sessoesAbertas: sessions.size,
      arquivosDeChat, chatMB: Number((bytesDeChat / 1048576).toFixed(2)),
      versao: APP_VERSION
    }));
    return;
  }

  // Salas por nome acabaram na v3: o que existe são conversas (`/api/conversas`).
  // Os dados delas (`.salas`, `.salas-conf`, histórico) seguem em disco, só
  // deixam de ser servidos.
  if (url.pathname === '/api/rooms') {
    responde(res, 410, { erro: 'Salas por nome acabaram. Use as conversas.' });
    return;
  }

  const publicRoot = path.join(__dirname, 'public');
  const filePath = path.join(publicRoot, url.pathname === '/' ? '/index.html' : url.pathname);

  // Travessia de caminho hoje não passa, mas por acidente feliz: quem
  // normaliza os ".." (inclusive os percent-encoded, tipo %2e%2e) é o parser
  // do `new URL()` lá em cima, não este join. Trocar por `req.url` cru num
  // refactor reabriria o buraco em silêncio — e logo acima do `public/` moram
  // `.chatkey`, `.discord-oauth.json` e `chatdata/`. Esta checagem deixa a
  // garantia explícita em vez de dependente de um detalhe distante.
  //
  // A checagem é sobre o CAMINHO, não sobre o destino final: `public/setup-audio/`
  // são links simbólicos de propósito pros scripts de áudio que moram em
  // `scripts/` (fonte única — os amigos não conseguem clonar o repo, que é
  // privado, então baixam de lá por HTTP). Ou seja, link simbólico dentro do
  // public/ continua sendo servido; só não deixe entrar link apontando pra
  // coisa que não deveria ser pública.
  if (path.relative(publicRoot, filePath).split(path.sep)[0] === '..') {
    res.writeHead(404);
    res.end('Não encontrado');
    return;
  }

  const ext = path.extname(filePath);
  const types = {
    '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css',
    '.jpg': 'image/jpeg', '.jpeg': 'image/jpeg', '.png': 'image/png'
  };

  fs.readFile(filePath, (err, content) => {
    if (err) {
      res.writeHead(404);
      res.end('Não encontrado');
      return;
    }
    res.writeHead(200, { 'Content-Type': types[ext] || 'text/plain' });
    res.end(content);
  });
});

// --- Servidor WebSocket, cuida das "salas" ---
// maxPayload maior que o padrão pra caber imagem de chat em base64 (até 5MB
// crus viram uns 6.7MB em base64 + margem do envelope JSON).
const wss = new WebSocket.Server({ server, maxPayload: 8 * 1024 * 1024 });

// Senha de sala. Vive **só enquanto a sala existe**, ou seja, enquanto tem
// pelo menos uma pessoa dentro: quem cria a sala define a senha, e quando o
// último sai a sala e a senha somem juntas. Quem criar uma sala com o mesmo
// nome depois define uma senha nova (ou nenhuma).
//
// Consequência que vale saber: a senha **não protege o histórico de chat**,
// que fica em disco por nome de sala. Sala vazia pode ser recriada por
// qualquer um, e aí o histórico antigo aparece. Se um dia isso incomodar, é
// aqui que a senha precisaria virar persistente.
//
// Guardado como hash com salt em vez do texto puro — não porque isso aqui
// seja um cofre (é RAM de um processo só), mas porque um dump/log acidental
// não deve entregar senha que a pessoa provavelmente reusa em outro lugar.
const MAX_ROOM_PASSWORD = 128;

// A senha da sala agora SOBREVIVE à sala esvaziar, e é guardada em disco junto
// do dono. Antes ela morria com o último a sair — e como o histórico do chat é
// persistido por NOME de sala, qualquer um podia recriar o nome e cair no
// histórico antigo. Era o furo descrito no item 24 do ROADMAP.
//
// Só sala COM senha é gravada. Sala aberta não deixa nada pra trás: senão cada
// nome digitado errado viraria um registro permanente no disco.
const ROOMS_FILE = path.join(CHAT_DIR, '.salas');
const roomSecrets = new Map(); // nome_da_sala -> { salt, hash, owner }

// Dono da sala VIVA. Sala com senha carrega o dono no registro persistido
// (acima); sala aberta só tem dono enquanto tem gente, o que é coerente: o que
// é efêmero morre junto.
const roomOwners = new Map();  // nome_da_sala -> { uid, nome } do dono

// Mensagem fixada da sala. Vive só enquanto a sala existe — é atalho pra "o
// link do servidor de hoje", não arquivo.
const roomPins = new Map();    // nome_da_sala -> { id, name, text, by, ts }

// Expulsão vale por um tempo, senão a pessoa volta no segundo seguinte e o
// botão não serve pra nada. Como o resto do estado de moderação, morre com a
// sala — não existe banimento permanente aqui.
const BAN_MS = 10 * 60 * 1000;
const roomBans = new Map();        // nome_da_sala -> Map(chaveDeIdentidade -> expiraEm)
// Silenciado pelo dono. Guardado por NOME e não por id de conexão: o id é
// sorteado de novo a cada reconexão, então por id bastava dar F5 pra falar de
// novo.
const roomForcedMutes = new Map(); // nome_da_sala -> Set de chaveDeIdentidade

function hashRoomPassword(salt, senha) {
  return crypto.createHash('sha256').update(salt + ':' + senha).digest();
}

// Registro antigo em disco guardava o dono como uma STRING (o nome). Ler os
// dois formatos é o que faz o arquivo que já está lá continuar valendo — sem
// isso, toda sala com senha criada antes desta mudança perderia o dono.
function leDonoSalvo(valor) {
  if (!valor) return null;
  // Formato antigo: só o nome. Vira um dono SEM uid — e é justamente por isso
  // que a comparação de dono usa `mesmaPessoa` e não a chave crua: sem uid dos
  // dois lados ela cai no nome, então quem trancou a sala antes desta mudança
  // continua sendo dono dela depois.
  if (typeof valor === 'string') return { uid: null, nome: valor };
  if (typeof valor.nome === 'string') return { uid: valor.uid || null, nome: valor.nome };
  return null;
}

function loadRoomSecrets() {
  if (!fs.existsSync(ROOMS_FILE)) return;
  try {
    const dados = JSON.parse(decryptBuf(fs.readFileSync(ROOMS_FILE)).toString('utf8'));
    for (const [nome, reg] of Object.entries(dados)) {
      if (reg && typeof reg.salt === 'string' && typeof reg.hash === 'string') {
        roomSecrets.set(nome, { salt: reg.salt, hash: Buffer.from(reg.hash, 'base64'), owner: leDonoSalvo(reg.owner) });
      }
    }
  } catch (e) {
    guardaArquivoIlegivel(ROOMS_FILE, e.message);
  }
}

function saveRoomSecrets() {
  try {
    if (!roomSecrets.size) { try { fs.unlinkSync(ROOMS_FILE); } catch {} return; }
    const dados = {};
    for (const [nome, reg] of roomSecrets) {
      dados[nome] = { salt: reg.salt, hash: reg.hash.toString('base64'), owner: reg.owner || null };
    }
    escreveArquivoAtomico(ROOMS_FILE, encryptBuf(Buffer.from(JSON.stringify(dados), 'utf8')), { mode: 0o600 });
  } catch (e) {
    console.error('Não consegui salvar as senhas de sala:', e.message);
  }
}

function setRoomPassword(nome, senha, dono) {
  if (!senha) {
    // Tirar a senha apaga o registro inteiro — inclusive o dono persistido.
    // É o preço de voltar a ser sala aberta, e é o único jeito de limpar um
    // nome de sala do disco.
    roomSecrets.delete(nome);
  } else {
    const salt = crypto.randomBytes(16).toString('hex');
    const anterior = roomSecrets.get(nome);
    roomSecrets.set(nome, {
      salt, hash: hashRoomPassword(salt, senha),
      owner: dono !== undefined ? dono : (anterior ? anterior.owner : null)
    });
  }
  saveRoomSecrets();
}

// Sala sem senha só aceita campo vazio — mandar senha numa sala que não tem
// senha é "senha inválida", que foi pedido de propósito: assim quem errou o
// nome da sala percebe que caiu em outra, em vez de entrar achando que está
// na sala certa.
function roomPasswordOk(nome, senha) {
  const segredo = roomSecrets.get(nome);
  if (!segredo) return senha === '';
  return crypto.timingSafeEqual(hashRoomPassword(segredo.salt, senha), segredo.hash);
}

// O dono sai do registro persistido quando existe; senão, de quem criou a sala
// que está de pé agora.
// Devolve `{ chave, nome }` — a chave é quem decide autorização, o nome é o que
// vai pra tela. Antes isto devolvia só o nome, e o nome fazia as duas coisas.
function donoDaSala(nome) {
  const segredo = roomSecrets.get(nome);
  if (segredo && segredo.owner) return segredo.owner;
  return roomOwners.get(nome) || null;
}

function nomeDoDono(sala) {
  const dono = donoDaSala(sala);
  return dono ? dono.nome : null;
}

function ehODono(sala, uid, nome) {
  const dono = donoDaSala(sala);
  return !!dono && mesmaPessoa(dono.uid, dono.nome, uid, nome);
}

function estaBanido(nome, chave) {
  const banidos = roomBans.get(nome);
  if (!banidos) return false;
  const ate = banidos.get(chave);
  if (!ate) return false;
  if (ate <= Date.now()) { banidos.delete(chave); return false; }
  return true;
}

loadRoomSecrets();

// Limite de ritmo do chat, por conexão. Os números são folgados de propósito:
// conversa animada de cinco amigos não pode esbarrar neles, e quem esbarra
// está fazendo outra coisa. Imagem tem limite próprio e bem mais apertado
// porque é ela que ocupa disco.
const LIMITE_DE_TEXTO = 15;            // por janela curta
const JANELA_DE_TEXTO_MS = 10 * 1000;
const LIMITE_DE_IMAGEM = 8;
const JANELA_DE_IMAGEM_MS = 60 * 1000;

// --- Configuração da sala que SOBREVIVE à sala esvaziar ------------------
//
// Ícone e roletas. É o segundo arquivo desse tipo (o primeiro é o `.salas`, das
// senhas) e existe pela mesma razão: o pedido foi explicitamente "fica salvo na
// sala mesmo depois de sair".
//
// Regra herdada do `.salas`, e ela é importante: **só sala que TEM alguma
// coisa é gravada**. Sem isso, cada nome de sala digitado errado viraria um
// registro permanente no disco.
const CONF_FILE = path.join(CHAT_DIR, '.salas-conf');
const confDeSalas = new Map();   // sala -> { icone, roletas: [] }

function loadConfDeSalas() {
  if (!fs.existsSync(CONF_FILE)) return;
  try {
    const dados = JSON.parse(decryptBuf(fs.readFileSync(CONF_FILE)).toString('utf8'));
    for (const [sala, conf] of Object.entries(dados)) {
      if (conf && typeof conf === 'object') {
        confDeSalas.set(sala, { icone: conf.icone || null, roletas: Array.isArray(conf.roletas) ? conf.roletas : [] });
      }
    }
  } catch (e) {
    guardaArquivoIlegivel(CONF_FILE, e.message);
  }
}

function saveConfDeSalas() {
  try {
    // Sala que ficou sem ícone e sem roleta sai do arquivo: é o que impede o
    // disco de acumular nome de sala pra sempre.
    for (const [sala, conf] of [...confDeSalas]) {
      if (!conf.icone && (!conf.roletas || !conf.roletas.length)) confDeSalas.delete(sala);
    }
    if (!confDeSalas.size) { try { fs.unlinkSync(CONF_FILE); } catch {} return; }
    escreveArquivoAtomico(CONF_FILE, encryptBuf(Buffer.from(JSON.stringify(Object.fromEntries(confDeSalas)), 'utf8')), { mode: 0o600 });
  } catch (e) {
    console.error('Não consegui salvar a configuração das salas:', e.message);
  }
}

function confDaSala(sala) {
  if (!confDeSalas.has(sala)) confDeSalas.set(sala, { icone: null, roletas: [] });
  return confDeSalas.get(sala);
}

// Carrega AQUI e não junto do `loadRoomSecrets()` lá em cima: `CONF_FILE` é um
// `const` declarado neste bloco, e chamar antes esbarra na zona morta temporal
// (o servidor nem sobe).
loadConfDeSalas();

// O ícone é um emoji OU uma URL de GIF do provedor. Emoji entra como texto
// curto (o cliente desenha com textContent), e URL passa pela mesma peneira de
// host do GIF do chat — senão "ícone da sala" viraria um jeito de fazer todo
// mundo baixar de um domínio qualquer.
function limpaIconeDeSala(v) {
  if (typeof v !== 'string') return null;
  const t = v.trim();
  if (!t) return null;
  if (/^https?:/i.test(t)) return limpaUrlDeGif(t);
  // Sem quebra de linha e curto: é um ícone, não um nome.
  const limpo = t.replace(/[\r\n\t]/g, '').slice(0, 8);
  return limpo || null;
}

const MAX_ROLETAS_POR_SALA = 12;
const MAX_OPCOES_DA_ROLETA = 24;

function limpaRoleta(bruto) {
  if (!bruto || typeof bruto !== 'object') return null;
  const nome = typeof bruto.nome === 'string'
    ? bruto.nome.replace(/[\r\n\t]/g, ' ').trim().slice(0, 40) : '';
  const opcoes = Array.isArray(bruto.opcoes)
    ? bruto.opcoes.map(o => typeof o === 'string' ? o.replace(/[\r\n\t]/g, ' ').trim().slice(0, 40) : '')
        .filter(Boolean).slice(0, MAX_OPCOES_DA_ROLETA)
    : [];
  if (opcoes.length < 2) return null;
  return { nome: nome || 'Roleta', opcoes };
}

// --- Atividades da sala (quadro branco) ---------------------------------
//
// O quadro vive em RAM e morre com a sala, como o fixado. Nada disso vai pro
// disco de propósito: traço é coisa de momento, e persistir daria trabalho de
// limpeza (e espaço) pra uma coisa que ninguém volta pra ler amanhã.
//
// O teto é em PONTOS, não em traços: um rabisco longo tem centenas de pontos e
// um toque tem dois, então contar traços não diria nada sobre a memória usada.
// As coordenadas chegam como inteiros de 0 a 10000 (proporção da tela), o que
// mantém o desenho igual em telas de tamanhos diferentes e evita guardar
// float.
const roomBoards = new Map();          // sala -> { tracos: [], pontos: 0 }
const MAX_PONTOS_DO_QUADRO = 12000;

function quadroDaSala(sala) {
  if (!roomBoards.has(sala)) roomBoards.set(sala, { tracos: [], pontos: 0 });
  return roomBoards.get(sala);
}

function limpaTracosAntigos(quadro) {
  while (quadro.pontos > MAX_PONTOS_DO_QUADRO && quadro.tracos.length) {
    const fora = quadro.tracos.shift();
    quadro.pontos -= (fora.p.length / 2) | 0;
  }
}

// Um traço que chegou do cliente, peneirado. Tudo aqui é desenhado na tela dos
// outros, então nada passa sem forma conferida.
function limpaTraco(bruto) {
  if (!bruto || typeof bruto !== 'object') return null;
  const p = Array.isArray(bruto.p) ? bruto.p : null;
  if (!p || p.length < 2 || p.length > 2000 || p.length % 2) return null;
  const pontos = [];
  for (const n of p) {
    if (!Number.isFinite(n)) return null;
    pontos.push(Math.max(0, Math.min(10000, Math.round(n))));
  }
  const cor = limpaCor(bruto.c);
  const largura = Number.isFinite(bruto.l) ? Math.max(1, Math.min(64, Math.round(bruto.l))) : 3;
  return { c: cor || '#ffffff', l: largura, p: pontos, b: bruto.b ? 1 : 0 };
}

// rooms: nome_da_sala -> Set de conexões
const rooms = new Map();

// O WS fica ocioso a chamada inteira (áudio/vídeo vai direto P2P via WebRTC,
// só o handshake inicial passa por aqui) — sem tráfego, o nginx na frente
// derruba a conexão pelo timeout de leitura dele, e a pessoa "some" da sala
// sem ter saído de fato. Ping periódico gera tráfego pra evitar isso, e de
// quebra derruba de vez quem já morreu sem completar o fechamento do TCP.
function heartbeat() { this.isAlive = true; }

// Enviar num socket que já está fechando joga exceção. Como quase todo envio
// aqui acontece dentro de um `for` sobre a sala, uma única conexão morrendo
// no momento errado abortava o laço no meio — os peers seguintes não
// recebiam a mensagem — e a exceção ainda subia pro handler de 'message'.
// Um cliente que sai no instante em que outro entra é situação normal, não
// motivo pra derrubar o resto da sala.
function safeSend(peerWs, payload) {
  if (!peerWs || peerWs.readyState !== WebSocket.OPEN) return false;
  try {
    peerWs.send(payload);
    return true;
  } catch (err) {
    console.warn('Falha ao enviar pra um peer (ignorado):', err.message);
    return false;
  }
}

// Num servidor de sinalização, morrer é MUITO pior que seguir capenga: se o
// processo cai, todo mundo perde a chamada ao mesmo tempo e as conexões
// WebRTC têm que ser refeitas do zero. Uma falha isolada (um socket que
// morreu na hora errada, um hiccup de rede do bot do Discord) não justifica
// isso. Então erros soltos ficam registrados bem visíveis e o processo
// continua de pé — o pm2 ainda reinicia se o estado ficar realmente ruim.
process.on('uncaughtException', (err) => {
  console.error('[uncaughtException] servidor seguindo de pé:', err && err.stack || err);
});
process.on('unhandledRejection', (reason) => {
  console.error('[unhandledRejection] servidor seguindo de pé:', reason && reason.stack || reason);
});

const MAX_ROOM_NAME = 64;

// A aparência do nome (cor/gradiente e efeito) é DECORAÇÃO, nunca identidade:
// o `name` continua saindo da sessão do Discord e é ele que autoriza apagar
// mensagem. O servidor não conhece a lista de estilos — guarda um identificador
// curto e o cliente é quem sabe pintar. Identificador desconhecido simplesmente
// não pinta nada, então nada aqui precisa mudar quando um estilo novo nasce no
// CSS.
function limpaIdentificador(v) {
  return typeof v === 'string' && /^[a-z][a-z0-9-]{0,23}$/.test(v) ? v : null;
}

function limpaCor(v) {
  return typeof v === 'string' && /^#[0-9a-fA-F]{6}$/.test(v) ? v.toLowerCase() : null;
}

// O banner sem imagem é uma cor (`#rrggbb`) ou o id de um degradê. Como no
// estilo de nome, **o servidor não conhece a lista de degradês** — ele só
// exige forma de identificador, e quem sabe pintar é o CSS. É isso que faz um
// degradê novo nascer só no cliente; id desconhecido não pinta nada em vez de
// pintar errado.
function limpaBanner(v) {
  if (typeof v !== 'string') return null;
  const t = v.trim();
  if (!t) return null;
  return limpaCor(t) || limpaIdentificador(t);
}

// --- Atividade (jogo/música) -------------------------------------------
// O nome do jogo e da faixa vêm do cliente — não há de onde mais tirar, a
// detecção acontece no processo principal do Electron. Por isso passam pela
// mesma régua da decoração: string, sem quebra de linha (que estragaria a
// linha da sidebar) e curta. Nada disso é usado pra decidir permissão de
// coisa nenhuma.
const MAX_ATIVIDADE = 120;

function limpaTextoDeAtividade(v) {
  if (typeof v !== 'string') return null;
  const limpo = v.replace(/[\r\n\t]/g, ' ').trim().slice(0, MAX_ATIVIDADE);
  return limpo || null;
}

// De onde a detecção tirou o nome. Serve só pra escrever "pela Steam" no
// cartão de perfil; não decide nada. Lista fechada porque é o cliente que
// manda — valor de fora vira `null` em vez de aparecer como texto na tela.
const ORIGENS_DE_JOGO = new Set(['steam', 'atalho', 'wine', 'nativo', 'lista', 'discord', 'manual']);

// "Desde quando": vem do cliente e por isso não dá pra confiar. O que importa
// é não deixar entrar valor que estrague a conta do outro lado — data no
// futuro (que viraria tempo negativo) ou coisa que não é número.
function limpaInstante(v) {
  if (typeof v !== 'number' || !Number.isFinite(v)) return null;
  const agora = Date.now();
  // Nada antes de 2020 nem no futuro. O relógio de quem manda pode estar
  // alguns segundos adiantado, daí a folga de um minuto.
  if (v < 1577836800000 || v > agora + 60000) return null;
  return Math.floor(v);
}

function limpaAtividade(v) {
  if (!v || typeof v !== 'object') return null;
  // O jogo virou objeto (nome + origem + desde) quando o cartão de perfil
  // passou a mostrar mais que uma linha. String continua aceita: é o que um
  // app de versão anterior manda, e recusar deixaria ele sem atividade nenhuma.
  const jogoBruto = typeof v.jogo === 'string' ? { nome: v.jogo } : (v.jogo || {});
  const nomeDoJogo = limpaTextoDeAtividade(jogoBruto.nome);
  const jogo = nomeDoJogo ? {
    nome: nomeDoJogo,
    origem: ORIGENS_DE_JOGO.has(jogoBruto.origem) ? jogoBruto.origem : null,
    desde: limpaInstante(jogoBruto.desde),
    // Id da capa (SteamGridDB) de um jogo registrado; a imagem vem de /api/jogos/capa/<id>.
    capa: Number.isInteger(jogoBruto.capa) && jogoBruto.capa > 0 && jogoBruto.capa <= 999999999 ? jogoBruto.capa : null
  } : null;

  const titulo = limpaTextoDeAtividade(v.musica && v.musica.titulo);
  const artista = limpaTextoDeAtividade(v.musica && v.musica.artista);
  const album = limpaTextoDeAtividade(v.musica && v.musica.album);
  // A capa vira `<img src>` na tela de todo mundo da sala. Só `https://`
  // passa: `file://` seria caminho da máquina de quem ouve (que do outro lado
  // não é imagem nenhuma e ainda conta a estrutura de pastas dela), `data:`
  // deixaria enfiar imagem de qualquer tamanho por aqui, e `http://` viraria
  // conteúdo misto numa página que é servida por HTTPS.
  const arteBruta = v.musica && v.musica.arte;
  const arte = typeof arteBruta === 'string' && /^https:\/\/[^\s"'<>]{1,300}$/.test(arteBruta) ? arteBruta : '';
  const inicio = limpaInstante(v.musica && v.musica.inicio);
  const duracaoBruta = Number(v.musica && v.musica.duracao);
  const duracao = Number.isFinite(duracaoBruta) && duracaoBruta > 0 && duracaoBruta <= 24 * 3600 * 1000 ? Math.round(duracaoBruta) : null;
  const musica = titulo ? { titulo, artista: artista || '', album: album || '', arte, inicio, duracao } : null;

  return (jogo || musica) ? { jogo, musica } : null;
}

// Presença fora da chamada, pra lista de amigos da tela inicial. Chaveada
// pela mesma identidade que autoriza o resto (`chaveDeIdentidade`).
//
// Tem PRAZO de propósito: quem fecha o app não manda "parei", então sem isso
// a pessoa ficaria jogando o mesmo jogo pra sempre. O prazo é bem maior que o
// intervalo de detecção do cliente (10s) pra não piscar quando uma varredura
// demora.
const VALIDADE_DA_ATIVIDADE_MS = 90 * 1000;
const atividadePorChave = new Map();   // chave de identidade -> { atividade, ts }

function guardaAtividade(chave, atividade) {
  if (!atividade) atividadePorChave.delete(chave);
  else { atividadePorChave.set(chave, { atividade, ts: Date.now() }); registraJogoVisto(chave, atividade); }
  avisaAmigos(chave, { type: 'atividade-amigo', uid: chave, atividade: statusDe(chave) === 'invisivel' ? null : (atividade || null) });
}

function atividadeDe(chave) {
  const reg = atividadePorChave.get(chave);
  if (!reg) return null;
  if (Date.now() - reg.ts > VALIDADE_DA_ATIVIDADE_MS) { atividadePorChave.delete(chave); return null; }
  return reg.atividade;
}

// Sem isto o Map guarda uma entrada por pessoa que já passou por aqui, pra
// sempre — numa VPS onde o OOM-killer já derrubou o app, memória que só
// cresce não é detalhe.
setInterval(() => {
  const corte = Date.now() - VALIDADE_DA_ATIVIDADE_MS;
  for (const [chave, reg] of [...atividadePorChave]) {
    if (reg.ts >= corte) continue;
    // Expirou sem ninguém dizer "parei": some do mapa e AVISA, senão a lista de
    // quem já recebeu a atividade fica com ela pra sempre.
    atividadePorChave.delete(chave);
    avisaAmigos(chave, { type: 'atividade-amigo', uid: chave, atividade: null });
  }
}, 5 * 60 * 1000).unref();

// --- Status escolhido à mão (online / ausente / ocupado) ---------------
//
// "Offline" NÃO está na lista de propósito: offline é o que se conclui de
// não haver ninguém conectado, não uma escolha. Deixar escolher "offline"
// seria o modo invisível, que é outra conversa (a pessoa apareceria fora do
// ar continuando a ouvir a sala).
//
// O status vale pras duas telas: dentro da chamada ele viaja pelo WebSocket
// (como mic/surdez) e fora dela pela rota HTTP (como a atividade). Os dois
// caminhos escrevem no MESMO lugar — se fossem dois estados, entrar numa
// chamada esqueceria o que a pessoa escolheu na tela inicial.
const STATUS_VALIDOS = new Set(['online', 'ausente', 'ocupado', 'invisivel']);
const STATUS_PADRAO = 'online';
const statusPorChave = new Map();   // chave de identidade -> status

function limpaStatus(v) {
  return STATUS_VALIDOS.has(v) ? v : null;
}

function statusDe(chave) {
  return statusPorChave.get(chave) || STATUS_PADRAO;
}

// O que os OUTROS veem: invisível aparece como offline. Só a própria pessoa
// (e o app dela) vê "invisivel".
function statusParaOutros(chave) {
  const s = statusDe(chave);
  return s === 'invisivel' ? 'offline' : s;
}

// --- "Está por aí?" -----------------------------------------------------
//
// Separado da ATIVIDADE de propósito. A primeira versão concluía isso de ter
// jogo ou música no ar, e errava justamente no caso mais comum: quem escolheu
// "não perturbe", não está jogando nada e não está numa chamada aparecia como
// **offline** — ou seja, o status escolhido à mão nunca chegava a ser visto
// por ninguém.
//
// O sinal certo é o próprio app falando com o servidor: a tela inicial já
// consulta a lista de amigos de 15 em 15 segundos, e o app manda um batimento
// de 60 em 60. O prazo aqui é folgado em cima dos dois.
const VALIDADE_DO_VISTO_MS = 90 * 1000;
const vistoPorChave = new Map();   // chave de identidade -> ts

function marcaVisto(session) {
  if (!session) return;
  vistoPorChave.set(chaveDeIdentidade(uidDaSessao(session), session.name), Date.now());
}

function foiVistoAgora(chave) {
  // O canal de avisos aberto É o sinal de vida: o app está ali.
  if (avisosPorUid.get(chave)?.size) return true;
  const ts = vistoPorChave.get(chave);
  return !!ts && Date.now() - ts <= VALIDADE_DO_VISTO_MS;
}

// Mesmo cuidado do Map de atividade: sem limpeza ele guardaria uma entrada por
// pessoa que já passou por aqui, pra sempre.
setInterval(() => {
  const corte = Date.now() - VALIDADE_DO_VISTO_MS;
  for (const [chave, ts] of vistoPorChave) if (ts < corte) vistoPorChave.delete(chave);
}, 5 * 60 * 1000).unref();

// Guarda o status. Conta local carrega ele NO DISCO junto do resto do perfil:
// escolher "não perturbe" e ver o app voltar pra "online" no próximo restart
// (que aqui é todo deploy) faria a escolha não valer nada. Quem entrou pelo
// Discord fica só na memória — não há conta pra gravar.
function guardaStatus(session, chave, status) {
  statusPorChave.set(chave, status);
  avisaAmigos(chave, { type: 'status-amigo', uid: chave, status: statusVisivelDe(chave) });
  avisaAmigos(chave, { type: 'atividade-amigo', uid: chave, atividade: status === 'invisivel' ? null : (atividadeDe(chave) || null) });
  if (session && session.tipo === 'local') {
    const conta = contas.get(session.usuario);
    if (conta) {
      conta.perfil = conta.perfil || {};
      conta.perfil.status = status;
      saveContas();
    }
  }
}

// --- Status personalizado ("Passatempo recente?") ------------------------------
// Texto curto + um emoji + prazo opcional, guardado em `conta.perfil.statusCustom`
// (no disco, junto do resto do perfil). Aparece no lugar da atividade. Quando o
// prazo passa o servidor apaga e avisa; a leitura também confere o prazo, para
// nunca mostrar um status vencido entre uma varredura e outra.
const PRAZOS_STATUS_CUSTOM_MIN = new Set([30, 60, 240, 1440]);

function limpaStatusCustom(texto, emoji, minutos) {
  const t = typeof texto === 'string' ? texto.replace(/[\r\n\t]/g, ' ').trim().slice(0, 60) : '';
  // Emoji: no máximo 4 símbolos e nada de letra/dígito (senão vira texto escondido).
  let e = typeof emoji === 'string' ? emoji.trim() : '';
  if ([...e].length > 4 || /[\p{L}\p{N}]/u.test(e)) e = '';
  if (!t && !e) return null;
  const ate = PRAZOS_STATUS_CUSTOM_MIN.has(Number(minutos)) ? Date.now() + Number(minutos) * 60000 : null;
  return { texto: t, emoji: e, ate };
}

function statusCustomDe(conta) {
  const sc = conta && conta.perfil && conta.perfil.statusCustom;
  if (!sc) return null;
  if (sc.ate && sc.ate <= Date.now()) return null;
  return { texto: sc.texto || '', emoji: sc.emoji || '', ate: sc.ate || null };
}

function avisaStatusCustom(conta) {
  const uid = 'local:' + conta.id;
  const invisivel = statusDe(uid) === 'invisivel';
  avisaUids([uid, ...(amizades.get(uid) || [])], {
    type: 'status-custom-amigo', uid, statusCustom: invisivel ? null : statusCustomDe(conta)
  });
}

setInterval(() => {
  let mudou = false;
  for (const conta of contas.values()) {
    const sc = conta.perfil && conta.perfil.statusCustom;
    if (sc && sc.ate && sc.ate <= Date.now()) {
      delete conta.perfil.statusCustom;
      mudou = true;
      avisaStatusCustom(conta);
    }
  }
  if (mudou) saveContas();
}, 30 * 1000).unref();

// Semeia o Map com o que está no disco. Sem isto o status salvo só voltaria a
// valer depois que a pessoa mexesse nele de novo — ou seja, nunca.
for (const conta of contas.values()) {
  const salvo = limpaStatus(conta.perfil && conta.perfil.status);
  if (salvo) statusPorChave.set('local:' + conta.id, salvo);
}

// --- Amizades ----------------------------------------------------------
//
// Até aqui "amigos" era a lista de membros de um servidor do Discord, lida
// por um bot: nada disso é do app, e quem entra por conta própria não aparece
// em lista nenhuma. Agora existe amizade de verdade — pedido, aceite e
// remoção — guardada no mesmo cofre criptografado do chat e das senhas.
//
// A amizade é entre UIDS, não entre nomes: nome de exibição não é único (dois
// "Gabriel"), e é exatamente por isso que a fatia 1 existe. Quem se procura
// por nome é o nome de USUÁRIO, que é único.
//
// Consequência aceita: só quem tem conta no app entra nisso. Quem entrou pelo
// Discord não tem arroba, então não há como escrever o nome dele numa caixa de
// busca — e a lista do bot continua sendo o que aquele mundo tem.
const AMIZADES_FILE = path.join(CHAT_DIR, '.amizades');
const amizades = new Map();   // uid -> Set de uids
const pedidos = [];           // { de, para, ts }

function loadAmizades() {
  if (!fs.existsSync(AMIZADES_FILE)) return;
  try {
    const dados = JSON.parse(decryptBuf(fs.readFileSync(AMIZADES_FILE)).toString('utf8'));
    for (const [uid, lista] of Object.entries(dados.amizades || {})) {
      if (Array.isArray(lista)) amizades.set(uid, new Set(lista.filter(x => typeof x === 'string')));
    }
    for (const p of (dados.pedidos || [])) {
      if (p && typeof p.de === 'string' && typeof p.para === 'string') {
        pedidos.push({ de: p.de, para: p.para, ts: Number(p.ts) || Date.now() });
      }
    }
  } catch (e) {
    guardaArquivoIlegivel(AMIZADES_FILE, e.message);
  }
}

function saveAmizades() {
  try {
    if (!amizades.size && !pedidos.length) { try { fs.unlinkSync(AMIZADES_FILE); } catch {} return; }
    const dados = { amizades: {}, pedidos };
    for (const [uid, set] of amizades) if (set.size) dados.amizades[uid] = [...set];
    escreveArquivoAtomico(AMIZADES_FILE, encryptBuf(Buffer.from(JSON.stringify(dados), 'utf8')), { mode: 0o600 });
  } catch (e) {
    console.error('Não consegui salvar as amizades:', e.message);
  }
}

loadAmizades();

// A amizade é gravada NOS DOIS SENTIDOS. Guardar só num lado obrigaria toda
// leitura a varrer o mapa inteiro procurando quem cita você — e numa
// remoção pela metade a pessoa sumiria da sua lista e você continuaria na dela.
function ehAmigo(a, b) {
  return !!(amizades.get(a) && amizades.get(a).has(b));
}

function criaAmizade(a, b) {
  if (!amizades.has(a)) amizades.set(a, new Set());
  if (!amizades.has(b)) amizades.set(b, new Set());
  amizades.get(a).add(b);
  amizades.get(b).add(a);
}

function desfazAmizade(a, b) {
  amizades.get(a)?.delete(b);
  amizades.get(b)?.delete(a);
  if (!amizades.get(a)?.size) amizades.delete(a);
  if (!amizades.get(b)?.size) amizades.delete(b);
}

function achaPedido(de, para) {
  return pedidos.findIndex(p => p.de === de && p.para === para);
}

function removePedido(de, para) {
  const i = achaPedido(de, para);
  if (i >= 0) pedidos.splice(i, 1);
}

// Índice `uid -> conta`. Montado na hora porque são cinco contas: manter um
// Map paralelo seria mais uma coisa pra esquecer de atualizar ao apagar conta.
function contaPorUid(uid) {
  if (typeof uid !== 'string' || !uid.startsWith('local:')) return null;
  const id = uid.slice('local:'.length);
  for (const conta of contas.values()) if (conta.id === id) return conta;
  return null;
}

// O cartão que a lista de amigos e o perfil mostram. Só sai daqui o que é
// público de propósito: nada de hash de senha, sal, ou a lista de amigos da
// pessoa. `status` só é dito de quem está mesmo por aqui — ver abaixo.
// `quemPergunta` é o uid de quem vai VER o cartão. Ele decide se a atividade
// entra: o que a pessoa está jogando é pra amigo, não pra qualquer um que tenha
// conta aqui e saiba (ou adivinhe) um nome de usuário. Sem esse parâmetro,
// `/api/perfil/:usuario` viraria um rastreador de quem está jogando o quê.
//
// O resto (nome, avatar, bio, cor) é público de propósito: é o que aparece na
// sala pra qualquer um que entre nela.
function cartaoDaConta(conta, quemPergunta) {
  const uid = 'local:' + conta.id;
  const chave = uid;
  const podeVerAtividade = !quemPergunta || quemPergunta === uid || ehAmigo(quemPergunta, uid);
  const invisivelParaQuem = statusDe(chave) === 'invisivel' && quemPergunta !== uid;
  const atividade = podeVerAtividade && !invisivelParaQuem ? atividadeDe(chave) : null;
  // "Online" só quando existe sinal de vida: o app falando com o servidor
  // agora, ou uma chamada em andamento. Sem isso, quem escolheu "não perturbe"
  // há três semanas apareceria como se estivesse na frente do computador.
  //
  // O sinal NÃO é a atividade, e essa foi a primeira versão: quem não está
  // jogando nada continua estando ali, e concluir "offline" disso apagava o
  // status escolhido à mão — justamente o de quem põe "não perturbe" pra
  // trabalhar em paz. Ver `foiVistoAgora`.
  const emChamada = [...rooms.values()].some(sala =>
    [...sala.values()].some(peer => peer.uid === uid));
  const vivo = !invisivelParaQuem && (emChamada || foiVistoAgora(chave));
  return {
    uid,
    usuario: conta.usuario,
    name: conta.nome,
    avatar: avatarUrlDaConta(conta),
    bio: (conta.perfil && conta.perfil.bio) || '',
    cor: (conta.perfil && conta.perfil.cor) || null,
    banner: (conta.perfil && conta.perfil.banner) || null,
    bannerUrl: bannerUrlDaConta(conta),
    status: vivo ? statusDe(chave) : 'offline',
    emChamada,
    atividade,
    statusCustom: invisivelParaQuem ? null : statusCustomDe(conta)
  };
}

// Apagar a conta tem que apagar a amizade junto: sem isso o uid morto fica na
// lista de todo mundo pra sempre, aparecendo como um cartão vazio que ninguém
// consegue remover.
function esqueceUid(uid) {
  for (const outro of [...(amizades.get(uid) || [])]) desfazAmizade(uid, outro);
  amizades.delete(uid);
  for (let i = pedidos.length - 1; i >= 0; i--) {
    if (pedidos[i].de === uid || pedidos[i].para === uid) pedidos.splice(i, 1);
  }
  saveAmizades();
  statusPorChave.delete(uid);
  atividadePorChave.delete(uid);
  delete perfilExtra.jogos[uid];
  delete perfilExtra.mural[uid];
  delete perfilExtra.listas[uid];
  for (const lista of Object.values(perfilExtra.mural)) {
    for (let i = lista.length - 1; i >= 0; i--) if (lista[i].de === uid) lista.splice(i, 1);
  }
  savePerfilExtra();
}

// --- Conversas (v3) ------------------------------------------------------
// Conversa = sala interna: o id dela é o nome da sala que a sinalização e o
// chat já usam, então `rooms`, `roomFile` e o histórico servem sem mudança.
// O que a conversa acrescenta é QUEM pode entrar (`membros`).
//
//   { id, tipo: 'dm'|'grupo', membros: [uid], dono, nome, icone, criadaEm }
//
// `membros` fica na ORDEM DE ENTRADA: é ela que decide a posse quando o dono
// sai (o primeiro da lista é o mais antigo). `icone` é `{ imagemId }`,
// `{ gif }` ou null (aí o cliente mostra as fotos de dois membros).
// Conversa nunca é apagada do disco, nem a de grupo sem membros: o histórico
// dela mora em `chatdata/` e é a única cópia.
const CONVERSAS_FILE = path.join(CHAT_DIR, '.conversas');
const conversas = new Map();   // id -> conversa
const MAX_NOME_DE_GRUPO = 60;

function loadConversas() {
  if (!fs.existsSync(CONVERSAS_FILE)) return;
  try {
    const dados = JSON.parse(decryptBuf(fs.readFileSync(CONVERSAS_FILE)).toString('utf8'));
    for (const c of (Array.isArray(dados.conversas) ? dados.conversas : [])) {
      if (!c || typeof c.id !== 'string' || (c.tipo !== 'dm' && c.tipo !== 'grupo')) continue;
      conversas.set(c.id, {
        id: c.id,
        tipo: c.tipo,
        membros: (Array.isArray(c.membros) ? c.membros : []).filter(x => typeof x === 'string'),
        dono: typeof c.dono === 'string' ? c.dono : null,
        nome: typeof c.nome === 'string' ? c.nome : '',
        icone: limpaIconeDeConversa(c.icone),
        convite: typeof c.convite === 'string' && /^[0-9a-f]{48}$/.test(c.convite) ? c.convite : null,
        criadaEm: Number(c.criadaEm) || Date.now()
      });
    }
  } catch (e) {
    guardaArquivoIlegivel(CONVERSAS_FILE, e.message);
  }
}

function saveConversas() {
  try {
    const dados = { conversas: [...conversas.values()] };
    escreveArquivoAtomico(CONVERSAS_FILE, encryptBuf(Buffer.from(JSON.stringify(dados), 'utf8')), { mode: 0o600 });
  } catch (e) {
    console.error('Não consegui salvar as conversas:', e.message);
  }
}

// O ícone vem de gente: ou é uma imagem que JÁ foi guardada pelo servidor
// (`imagemId`, mesmo formato validado do avatar), ou uma URL que passa no filtro
// do Giphy. Qualquer outra coisa vira "sem ícone".
function limpaIconeDeConversa(v) {
  if (!v || typeof v !== 'object') return null;
  if (idDeImagemValido(v.imagemId)) return { imagemId: v.imagemId, mime: typeof v.mime === 'string' ? v.mime : undefined };
  const gif = limpaUrlDeGif(v.gif);
  return gif ? { gif } : null;
}

// Id da conversa direta: determinístico e igual nos dois lados, o que a faz
// única por par. Ordem por comparação de string, nunca pela ordem de quem pediu.
function idDeDm(uidA, uidB) {
  const [menor, maior] = uidA < uidB ? [uidA, uidB] : [uidB, uidA];
  return `dm_${menor}_${maior}`;
}

function conversaPorId(id) {
  return typeof id === 'string' ? (conversas.get(id) || null) : null;
}

function ehMembro(conversa, uid) {
  return !!(conversa && uid && conversa.membros.includes(uid));
}

function conversasDe(uid) {
  if (!uid) return [];
  return [...conversas.values()].filter(c => c.membros.includes(uid));
}

// Abre a conversa direta com `outro` ou devolve a que já existe. Quem chama
// confere a amizade antes: aqui só se garante a unicidade.
function abreConversaDireta(meuUid, outro) {
  const id = idDeDm(meuUid, outro);
  let c = conversas.get(id);
  if (!c) {
    c = { id, tipo: 'dm', membros: [meuUid, outro], dono: null, nome: '', icone: null, criadaEm: Date.now() };
    conversas.set(id, c);
    saveConversas();
  }
  return c;
}

function criaGrupo(donoUid, outros, nome) {
  const membros = [donoUid, ...outros.filter((u, i) => u !== donoUid && outros.indexOf(u) === i)];
  const c = {
    id: 'grp_' + crypto.randomBytes(8).toString('hex'),
    tipo: 'grupo',
    membros,
    dono: donoUid,
    nome: (typeof nome === 'string' ? nome.trim() : '').slice(0, MAX_NOME_DE_GRUPO),
    icone: null,
    convite: null,
    criadaEm: Date.now()
  };
  conversas.set(c.id, c);
  saveConversas();
  return c;
}

// Quem está DENTRO da chamada da conversa: na sala e fora do modo espectador.
// A sala interna tem o mesmo id da conversa, então `rooms` responde direto.
function quemEstaNaChamada(conversaId) {
  const sala = rooms.get(conversaId);
  if (!sala) return [];
  const uids = [];
  for (const [, peer] of sala) {
    if (!peer.espectador && peer.uid && !uids.includes(peer.uid)) uids.push(peer.uid);
  }
  return uids;
}

// Fecha a participação de `uid` na chamada/sala da conversa (removido ou saiu
// do grupo). Fechar o socket é o que o handler de `close` usa pra avisar
// `peer-left` e limpar o estado dele.
function expulsaDaConversa(conversaId, uid) {
  fimDoToque(conversaId, uid, 'cancelado');
  const sala = rooms.get(conversaId);
  if (!sala) return;
  for (const [, peer] of [...sala]) {
    if (peer.uid !== uid) continue;
    safeSend(peer.ws, JSON.stringify({ type: 'kicked', motivo: 'removido', minutos: 0 }));
    try { peer.ws.close(4003, 'removido'); } catch { /* já caiu */ }
  }
}

// Pontos de extensão: o canal de avisos (A4) e as mensagens de sistema no
// histórico (A6) entram aqui. As rotas de A2 já chamam os dois no lugar certo.

// --- Canal de avisos (v3) ------------------------------------------------
// Uma conexão WebSocket por app, aberta logo após o login e mantida sempre.
// Primeira mensagem `{type:'avisos'}`; daí em diante só o servidor fala. É por
// aqui que o app fica sabendo de conversa nova, mensagem nova, chamada e
// toque SEM estar dentro de nenhuma sala. Guardado por uid (nunca por id de
// conexão) e com várias conexões por pessoa: dois computadores, duas abas.
const avisosPorUid = new Map();   // uid -> Set de ws
const AVISOS_GRACA_MS = Number(process.env.AVISOS_GRACA_MS) || 5000;

// `evento` é um objeto, ou uma função uid -> objeto (para quando o conteúdo
// depende de quem recebe, como o cartão de membro que respeita amizade).
function avisaUids(uids, evento) {
  for (const uid of new Set(uids)) {
    const conexoes = avisosPorUid.get(uid);
    if (!conexoes || !conexoes.size) continue;
    const dados = typeof evento === 'function' ? evento(uid) : evento;
    if (!dados) continue;
    const payload = JSON.stringify(dados);
    for (const w of conexoes) safeSend(w, payload);
  }
}

// Avisa `conversa-nova`, `conversa-mudou` ou `conversa-saiu`. `extra.removidos`
// são quem acabou de sair da lista de membros: eles não recebem a mudança,
// recebem `conversa-saiu`.
function avisaMembros(conversa, evento, extra) {
  const removidos = (extra && extra.removidos) || [];
  if (evento === 'conversa-saiu') {
    avisaUids([...conversa.membros, ...removidos], { type: 'conversa-saiu', conversa: conversa.id });
    return;
  }
  avisaUids(conversa.membros, uid => ({ type: evento, conversa: conversaParaCliente(conversa, uid) }));
  if (removidos.length) avisaUids(removidos, { type: 'conversa-saiu', conversa: conversa.id });
}

// Última lista de quem estava dentro, por conversa: só vai `chamada` quando
// MUDOU. Entrar como espectador, por exemplo, não muda a chamada.
const ultimaChamadaAvisada = new Map();   // conversaId -> 'uid,uid'

function avisaChamada(conversaId, forca) {
  const c = conversaPorId(conversaId);
  if (!c) return;
  sincronizaToques(c, quemEstaNaChamada(conversaId));
  emiteChamada(conversaId, forca);
}

// Só emite (sem sincronizar toques): usado quando é o próprio toque que mudou.
function emiteChamada(conversaId, forca) {
  const c = conversaPorId(conversaId);
  if (!c) return;
  const dentro = quemEstaNaChamada(conversaId);
  // `chamando`: quem está tocando agora. O palco da chamada mostra essas
  // pessoas apagadas, com o anel pulsando, para todo mundo que está dentro.
  const chamando = [...(toques.get(conversaId)?.keys() || [])];
  const chave = dentro.join(',') + '|' + chamando.join(',');
  if (!forca && (ultimaChamadaAvisada.get(conversaId) || '|') === chave) return;
  if (chave !== '|') ultimaChamadaAvisada.set(conversaId, chave); else ultimaChamadaAvisada.delete(conversaId);
  avisaUids(c.membros, { type: 'chamada', conversa: conversaId, dentro, chamando, ativa: dentro.length > 0 });
}

// Status e atividade dos AMIGOS também passam por aqui — é o que mantém a
// coluna "Ativo agora" e a bolinha de status sem ficar fazendo polling.
function avisaAmigos(uid, evento) {
  if (typeof uid !== 'string' || !uid.startsWith('local:')) return;
  avisaUids([...(amizades.get(uid) || [])], evento);
}

// --- Chamada e toque (v3) ------------------------------------------------
// `ligar` toca para todos os membros que não estão na chamada. O toque dura
// TOQUE_MS por pessoa e acaba por um destes motivos, avisado em `toque-fim`:
//   atendeu   a pessoa entrou na chamada
//   recusou   a pessoa recusou (só vale para o próprio toque)
//   tempo     ninguém atendeu; vira "X não atendeu" no chat
//   cancelado a chamada ficou sem ninguém dentro, ou a pessoa saiu do grupo
// Estado só em memória: toque não sobrevive a restart, de propósito.
const TOQUE_MS = Number(process.env.TOQUE_MS) || 30000;
const TOQUE_REPIQUE_MS = 10 * 1000;               // `tocar-de-novo`, por alvo
const TOQUE_ESPERA_DE_ENTRADA_MS = 15 * 1000;     // quem liga tem esse tempo pra entrar
const toques = new Map();            // conversaId -> Map(uid -> { timer, ate, quem })
const ultimoRepique = new Map();     // `${conversaId}|${uid}` -> instante
const chamadaIniciada = new Map();   // conversaId -> { ts, quem }
const perderamChamada = new Map();   // conversaId -> Set(uid) que não atendeu/recusou nesta chamada

function nomeDeUid(uid) {
  const conta = contaPorUid(uid);
  return conta ? conta.nome : 'Alguém';
}

function iniciaToque(c, alvo, quemUid, avisar = true) {
  let mapa = toques.get(c.id);
  if (!mapa) { mapa = new Map(); toques.set(c.id, mapa); }
  const anterior = mapa.get(alvo);
  if (anterior) clearTimeout(anterior.timer);
  const ate = Date.now() + TOQUE_MS;
  const timer = setTimeout(() => fimDoToque(c.id, alvo, 'tempo'), TOQUE_MS);
  timer.unref();
  mapa.set(alvo, { timer, ate, quem: quemUid });
  avisaUids([alvo], eventoDeToque(c.id, quemUid, ate));
  if (avisar) emiteChamada(c.id);
}

function eventoDeToque(conversaId, quemUid, ate) {
  return {
    type: 'toque', conversa: conversaId,
    quem: { uid: quemUid, name: nomeDeUid(quemUid) },
    ate, restanteMs: Math.max(0, ate - Date.now()), duracaoMs: TOQUE_MS
  };
}

function fimDoToque(conversaId, uid, motivo, avisar = true) {
  const mapa = toques.get(conversaId);
  const t = mapa && mapa.get(uid);
  if (!t) return;
  clearTimeout(t.timer);
  mapa.delete(uid);
  if (!mapa.size) toques.delete(conversaId);
  avisaUids([uid], { type: 'toque-fim', conversa: conversaId, motivo });
  if (motivo === 'atendeu') perderamChamada.get(conversaId)?.delete(uid);
  if (motivo === 'tempo' || motivo === 'recusou') {
    if (!perderamChamada.has(conversaId)) perderamChamada.set(conversaId, new Set());
    perderamChamada.get(conversaId).add(uid);
  }
  const c = conversaPorId(conversaId);
  if (avisar) emiteChamada(conversaId);
}

function toqueAtivoDe(conversaId, uid) {
  const mapa = toques.get(conversaId);
  return mapa ? mapa.get(uid) || null : null;
}

// Chamada às mudanças de quem está dentro (vem de `avisaChamada`): quem entrou
// atendeu; chamada que esvaziou cancela o resto e grava a duração.
function sincronizaToques(c, dentro) {
  for (const uid of dentro) fimDoToque(c.id, uid, 'atendeu', false);
  if (dentro.length && !chamadaIniciada.has(c.id)) {
    chamadaIniciada.set(c.id, { ts: Date.now(), quem: dentro[0] });
  }
  if (!dentro.length) {
    for (const uid of [...(toques.get(c.id)?.keys() || [])]) fimDoToque(c.id, uid, 'cancelado', false);
    const ini = chamadaIniciada.get(c.id);
    if (ini) {
      chamadaIniciada.delete(c.id);
      const minutos = Math.round((Date.now() - ini.ts) / 60000);
      const horas = Math.round(minutos / 60);
      const duracao = minutos < 1 ? 'menos de 1 minuto'
        : minutos === 1 ? '1 minuto'
        : minutos < 60 ? `${minutos} minutos`
        : (horas === 1 ? '1 hora' : `${horas} horas`);
      const perdeu = [...(perderamChamada.get(c.id) || [])].filter(u => !dentro.includes(u));
      perderamChamada.delete(c.id);
      registraSistema(c, 'chamada', `${nomeDeUid(ini.quem)} começou uma chamada que durou ${duracao}.`,
        { quem: nomeDeUid(ini.quem), duracao, perdeu });
    }
  }
}

// `ligar`: começa a chamada (quem liga entra em seguida, com `join`) e toca
// para os membros que ainda não estão nela.
function ligaParaConversa(c, quemUid) {
  const dentro = quemEstaNaChamada(c.id);
  if (dentro.length) return { jaAtiva: true };
  for (const alvo of c.membros) {
    if (alvo !== quemUid) iniciaToque(c, alvo, quemUid, false);
  }
  emiteChamada(c.id);
  // Quem liga e nunca entra não pode deixar a casa toda tocando: sem ninguém
  // dentro depois da espera, os toques são cancelados (sem mensagem).
  const espera = setTimeout(() => {
    if (quemEstaNaChamada(c.id).length) return;
    for (const uid of [...(toques.get(c.id)?.keys() || [])]) fimDoToque(c.id, uid, 'cancelado');
  }, TOQUE_ESPERA_DE_ENTRADA_MS);
  espera.unref();
  return { jaAtiva: false };
}

function statusVisivelDe(uid) {
  const emChamada = [...rooms.values()].some(sala => [...sala.values()].some(peer => peer.uid === uid));
  return emChamada || foiVistoAgora(uid) ? statusParaOutros(uid) : 'offline';
}

// Mensagem de sistema no histórico: salva e criptografada como as outras (vai
// por `appendMessage`), mas com `sys` no lugar de autor — e por isso as
// rotinas de editar, apagar, reagir, votar e fixar a recusam. Tipos fechados:
// `chamada`, `perdida` e `grupo`.
const TIPOS_DE_SISTEMA = new Set(['chamada', 'perdida', 'grupo']);

function registraSistema(conversa, tipo, texto, extra) {
  if (!conversa || !TIPOS_DE_SISTEMA.has(tipo) || typeof texto !== 'string' || !texto) return;
  const ts = Date.now();
  const entrada = { id: crypto.randomBytes(6).toString('hex'), sys: tipo, texto: texto.slice(0, 200), hora: ts, ts };
  // `extra` (só `chamada`): quem ligou, duração e quem perdeu a ligação — o
  // cliente monta "Você perdeu uma chamada de X que durou N" para os que
  // perderam e o texto comum para os outros.
  if (extra && typeof extra === 'object') {
    if (typeof extra.quem === 'string') entrada.quem = extra.quem.slice(0, 64);
    if (typeof extra.duracao === 'string') entrada.duracao = extra.duracao.slice(0, 40);
    if (Array.isArray(extra.perdeu)) entrada.perdeu = extra.perdeu.filter(u => typeof u === 'string').slice(0, 200);
  }
  try {
    appendMessage(conversa.id, entrada);
  } catch (e) {
    console.error('Não consegui gravar a mensagem de sistema:', e.message);
    return;
  }
  // Quem está na sala vê na hora; quem está fora recebe o aviso `mensagem`
  // (com `sys`, pra o cliente atualizar a prévia sem notificar).
  const payload = JSON.stringify({ type: 'sistema', id: entrada.id, sys: tipo, texto: entrada.texto, hora: ts, ts,
    quem: entrada.quem, duracao: entrada.duracao, perdeu: entrada.perdeu });
  const sala = rooms.get(conversa.id);
  if (sala) for (const [, peer] of sala) safeSend(peer.ws, payload);
  avisaUids(conversa.membros, { type: 'mensagem', conversa: conversa.id, sys: tipo, previa: entrada.texto, ts });
}

function urlDoIconeDeGrupo(c) {
  if (!c.icone) return null;
  if (c.icone.gif) return c.icone.gif;
  return `/api/grupos/${encodeURIComponent(c.id)}/icone?v=${c.icone.imagemId.slice(0, 8)}`;
}

function resumoDaUltimaMensagem(conversaId) {
  const entradas = readRoomEntries(conversaId);
  for (let i = entradas.length - 1; i >= 0; i--) {
    const e = entradas[i];
    if (e.sys) return { sys: e.sys, texto: String(e.texto || '').slice(0, 80), ts: e.ts || e.hora || null };
    const previa = e.text ? String(e.text).replace(/\s+/g, ' ').slice(0, 80)
      : (e.imageId ? '[imagem]' : (e.arquivo ? '[arquivo]' : (e.gif ? '[GIF]' : (e.enquete ? '[enquete]' : (e.app === 'wordle' ? 'Wordle' : '')))));
    return { autor: e.name || null, uid: e.uid || null, previa, ts: e.ts || null };
  }
  return null;
}

// A visão de UMA conversa pra quem pergunta: membros já como cartão (nome,
// avatar, status, atividade — com a regra de privacidade do `cartaoDaConta`).
function conversaParaCliente(c, quemPergunta) {
  const dentro = quemEstaNaChamada(c.id);
  return {
    id: c.id,
    tipo: c.tipo,
    nome: c.nome,
    icone: urlDoIconeDeGrupo(c),
    dono: c.dono,
    criadaEm: c.criadaEm,
    // O token do link só sai pro dono, que é quem o gera e revoga.
    convite: c.tipo === 'grupo' && c.dono === quemPergunta ? c.convite : undefined,
    temConvite: !!c.convite,
    membros: c.membros.map(uid => {
      const conta = contaPorUid(uid);
      return conta ? cartaoDaConta(conta, quemPergunta) : { uid, name: 'Conta removida', removida: true };
    }),
    ultimaMensagem: resumoDaUltimaMensagem(c.id),
    chamada: { ativa: dentro.length > 0, dentro, chamando: [...(toques.get(c.id)?.keys() || [])] }
  };
}

function nomePadraoDeGrupo(uids) {
  const nomes = uids.map(u => contaPorUid(u)?.nome).filter(Boolean);
  return nomes.join(', ').slice(0, MAX_NOME_DE_GRUPO) || 'Grupo';
}

loadConversas();

// --- Mensagens favoritas (por pessoa) -----------------------------------------
// `chatdata/.favoritos`: { [uid]: [{ conversa, id, autor, texto, tipo, ts, em }] }.
// Guarda um retrato da mensagem (não o original): o favorito sobrevive a ela ser
// apagada e a pessoa sai do grupo sem a lista apontar para o vazio. É estado da
// PESSOA, não da conexão (igual a volume e mute forçado).
const FAVORITOS_FILE = path.join(CHAT_DIR, '.favoritos');
const LIMITE_FAVORITOS = 500;
const favoritos = new Map();   // uid -> Array

function loadFavoritos() {
  if (!fs.existsSync(FAVORITOS_FILE)) return;
  try {
    const dados = JSON.parse(decryptBuf(fs.readFileSync(FAVORITOS_FILE)).toString('utf8'));
    for (const [uid, lista] of Object.entries(dados || {})) {
      if (!Array.isArray(lista)) continue;
      favoritos.set(uid, lista.filter(f => f && typeof f.conversa === 'string' && typeof f.id === 'string').slice(0, LIMITE_FAVORITOS));
    }
  } catch (e) {
    // Ilegível: põe de lado, nunca sobrescreve com vazio.
    try { fs.renameSync(FAVORITOS_FILE, FAVORITOS_FILE + '.ilegivel-' + Date.now()); } catch { /* sem o que fazer */ }
    console.error('Favoritos ilegíveis, postos de lado:', e.message);
  }
}

function saveFavoritos() {
  try {
    const dados = Object.fromEntries(favoritos);
    escreveArquivoAtomico(FAVORITOS_FILE, encryptBuf(Buffer.from(JSON.stringify(dados), 'utf8')), { mode: 0o600 });
  } catch (e) {
    console.error('Não consegui salvar os favoritos:', e.message);
  }
}
loadFavoritos();

// --- Perfil: coleção de jogos e mural -------------------------------------------
// `chatdata/.perfil-extra`: { jogos: { [uid]: { [nome]: { primeira, ultima, vistas } } },
//                             mural: { [uid]: [{ id, de, deNome, texto, ts }] } }.
// Estado da PESSOA (igual aos favoritos). Jogos só somam quando o app avisa a
// atividade; `vistas` conta dias distintos, não avisos, para não inflar a cada 10s.
const PERFIL_EXTRA_FILE = path.join(CHAT_DIR, '.perfil-extra');
const LIMITE_JOGOS = 200;
const LIMITE_MURAL = 50;
const MAX_TEXTO_MURAL = 200;
let perfilExtra = { jogos: {}, mural: {}, listas: {} };

function loadPerfilExtra() {
  if (!fs.existsSync(PERFIL_EXTRA_FILE)) return;
  try {
    const d = JSON.parse(decryptBuf(fs.readFileSync(PERFIL_EXTRA_FILE)).toString('utf8'));
    if (d && typeof d === 'object') perfilExtra = { jogos: d.jogos || {}, mural: d.mural || {}, listas: d.listas || {} };
  } catch (e) {
    try { fs.renameSync(PERFIL_EXTRA_FILE, PERFIL_EXTRA_FILE + '.ilegivel-' + Date.now()); } catch { /* sem o que fazer */ }
    console.error('Estado extra do perfil ilegível, posto de lado:', e.message);
  }
}
let perfilExtraSujo = false;
function savePerfilExtra() {
  perfilExtraSujo = false;
  try {
    escreveArquivoAtomico(PERFIL_EXTRA_FILE, encryptBuf(Buffer.from(JSON.stringify(perfilExtra), 'utf8')), { mode: 0o600 });
  } catch (e) {
    console.error('Não consegui salvar o estado extra do perfil:', e.message);
  }
}
loadPerfilExtra();
// Jogo novo na coleção grava na hora; o resto (última vez vista) só marca como sujo.
setInterval(() => { if (perfilExtraSujo) savePerfilExtra(); }, 60 * 1000).unref();

function registraJogoVisto(chave, atividade) {
  const nome = atividade && atividade.jogo && atividade.jogo.nome;
  if (!nome || !chave || !chave.startsWith('local:')) return;
  const dia = Math.floor((Date.now() - 3 * 3600 * 1000) / 86400000);
  const meus = perfilExtra.jogos[chave] || (perfilExtra.jogos[chave] = {});
  let reg = meus[nome];
  if (!reg) {
    if (Object.keys(meus).length >= LIMITE_JOGOS) return;
    reg = meus[nome] = { primeira: Date.now(), ultima: Date.now(), dias: 0, ultimoDia: -1 };
    if (reg.ultimoDia !== dia) { reg.ultimoDia = dia; reg.dias = 1; }
    savePerfilExtra();
    return;
  }
  reg.ultima = Date.now();
  if (reg.ultimoDia !== dia) { reg.ultimoDia = dia; reg.dias = (reg.dias || 0) + 1; }
  perfilExtraSujo = true;
}

// Insígnias são calculadas na hora a partir do que o servidor já sabe: nada é
// guardado, então não há como "ganhar" uma por requisição forjada.
function insigniasDe(conta) {
  const uid = 'local:' + conta.id;
  const lista = [];
  const dias = (Date.now() - (conta.criadaEm || Date.now())) / 86400000;
  if (dias >= 30) lista.push({ id: 'veterano', nome: 'Veterano', detalhe: 'Conta com mais de 30 dias' });
  if ([...conversas.values()].some(c => c.tipo === 'grupo' && c.dono === uid)) {
    lista.push({ id: 'dono-de-grupo', nome: 'Dono de grupo', detalhe: 'Criou um grupo' });
  }
  if (Object.keys(perfilExtra.jogos[uid] || {}).length >= 5) {
    lista.push({ id: 'colecionador', nome: 'Colecionador', detalhe: '5 jogos ou mais na coleção' });
  }
  let vitorias = 0;
  for (const dia of Object.values(wordleDados.dias || {})) {
    const r = dia && dia[uid];
    if (r && r.fim === 'ganhou') vitorias++;
  }
  if (vitorias >= 3) lista.push({ id: 'wordle', nome: 'Mestre do Wordle', detalhe: vitorias + ' vitórias no Wordle' });
  if ((amizades.get(uid) || new Set()).size >= 3) lista.push({ id: 'popular', nome: 'Popular', detalhe: '3 amigos ou mais' });
  return lista;
}

// Listas de jogos escolhidas à mão no perfil (favorito, gosto, quero jogar).
// Só guarda { id (do SteamGridDB), nome }; a capa vem de /api/jogos/capa/<id>.
const MAX_JOGOS_POR_LISTA = 20;
function listasDe(uid) {
  const l = perfilExtra.listas[uid] || {};
  return { favorito: l.favorito || null, gosto: l.gosto || [], quero: l.quero || [] };
}

function limpaJogoDeLista(j) {
  if (!j || typeof j !== 'object') return null;
  const id = Number(j.id);
  const nome = typeof j.nome === 'string' ? j.nome.replace(/[\r\n\t]/g, ' ').trim().slice(0, 80) : '';
  if (!Number.isInteger(id) || id <= 0 || id > 999999999 || !nome) return null;
  return { id, nome };
}

function limpaListaDeJogos(lista) {
  const vistos = new Set();
  const out = [];
  for (const j of Array.isArray(lista) ? lista : []) {
    const l = limpaJogoDeLista(j);
    if (l && !vistos.has(l.id)) { vistos.add(l.id); out.push(l); }
    if (out.length >= MAX_JOGOS_POR_LISTA) break;
  }
  return out;
}

function jogosDe(uid) {
  return Object.entries(perfilExtra.jogos[uid] || {})
    .map(([nome, r]) => ({ nome, primeira: r.primeira, ultima: r.ultima, dias: r.dias || 1 }))
    .sort((a, b) => b.ultima - a.ultima);
}

function amigosEmComum(a, b) {
  const sa = amizades.get(a), sb = amizades.get(b);
  if (!sa || !sb) return [];
  return [...sa].filter(u => sb.has(u) && u !== a && u !== b);
}

// --- Wordle (palavra do dia, igual para todos) --------------------------------
// O servidor guarda a palavra e valida cada tentativa; o cliente só recebe as
// cores. A resposta só sai quando a pessoa termina (ganhou ou perdeu).
// Lista de palavras: `wordle-words.txt` (5790 palavras de cinco letras em ordem de
// frequência, base: Stanford GraphBase, D. Knuth). As ~2400 mais comuns são as
// candidatas a "palavra do dia"; todas valem como tentativa. Sem o arquivo o
// Wordle fica desligado (o resto do servidor não depende dele).
const WORDLE_FILE = path.join(CHAT_DIR, '.wordle');
const WORDLE_TENTATIVAS = 6;
let wordleTodas = [];
let wordleRespostas = [];
(function carregaPalavrasDoWordle() {
  try {
    const bruto = fs.readFileSync(path.join(__dirname, 'wordle-words.txt'), 'utf8');
    wordleTodas = [...new Set(bruto.split(/\s+/).map(w => w.toLowerCase()).filter(w => /^[a-z]{5}$/.test(w)))];
    // Embaralha as mais comuns com uma semente fixa: a ordem é a mesma em todo
    // restart, mas dois dias seguidos não ficam parecidos (nem alfabéticos).
    const candidatas = wordleTodas.slice(0, 2400);
    let x = 0x9e3779b9;
    const prng = () => { x ^= x << 13; x >>>= 0; x ^= x >>> 17; x ^= x << 5; x >>>= 0; return x / 4294967296; };
    for (let i = candidatas.length - 1; i > 0; i--) {
      const j = Math.floor(prng() * (i + 1));
      [candidatas[i], candidatas[j]] = [candidatas[j], candidatas[i]];
    }
    wordleRespostas = candidatas;
  } catch (e) {
    console.error('Wordle desligado (sem wordle-words.txt):', e.message);
  }
})();

// Dia = dias desde 1970 no fuso de Brasília (UTC-3, sem horário de verão).
function wordleDia() {
  if (process.env.WORDLE_DIA) return Number(process.env.WORDLE_DIA);
  return Math.floor((Date.now() - 3 * 3600 * 1000) / 86400000);
}
const WORDLE_DIA_ZERO = Math.floor(Date.UTC(2026, 0, 1) / 86400000);   // Wordle No. 1 = 2026-01-01
const wordleNumero = dia => (Number(String(dia).split('_')[0]) || wordleDia()) - WORDLE_DIA_ZERO + 1;
function wordleChaveAtual() {
  const base = wordleDia();
  const rodada = (wordleDados.rodadas && wordleDados.rodadas[base]) || 1;
  return rodada > 1 ? `${base}_r${rodada}` : String(base);
}
// Número do dia de uma chave de rodada ("20743" ou "20743_r2").
const wordleBase = dia => Number(String(dia).split('_')[0]);
// Só vale dia que já chegou e rodada que existe: com chave inventada dava para
// jogar um dia futuro e ver a resposta dele ao perder.
function wordleChaveValida(dia) {
  const m = /^(\d+)(?:_r(\d+))?$/.exec(String(dia || ''));
  if (!m || Number(m[1]) > wordleDia()) return false;
  if (!m[2]) return true;
  const n = Number(m[2]);
  return n >= 2 && n <= ((wordleDados.rodadas && wordleDados.rodadas[Number(m[1])]) || 1);
}
// Chaves de todas as rodadas de um dia.
function wordleChavesDoDia(base) {
  const n = (wordleDados.rodadas && wordleDados.rodadas[base]) || 1;
  const k = [String(base)];
  for (let r = 2; r <= n; r++) k.push(`${base}_r${r}`);
  return k;
}
function wordlePalavraDoDia(dia) {
  // A palavra sorteada de cada rodada fica gravada no `.wordle`: só em memória,
  // um restart trocava a resposta de quem já estava jogando.
  if (wordleDados.overrides && wordleDados.overrides[dia]) return wordleDados.overrides[dia];
  const num = Number(String(dia).split('_')[0]) || wordleDia();
  if (process.env.WORDLE_PALAVRA && num === wordleDia()) return process.env.WORDLE_PALAVRA;   // só teste
  if (!wordleRespostas.length) return null;
  return wordleRespostas[((num % wordleRespostas.length) + wordleRespostas.length) % wordleRespostas.length];
}

// 'g' certa, 'y' existe em outro lugar, 'x' não tem. Letra repetida na
// tentativa só ganha amarelo enquanto sobrar daquela letra na resposta.
function wordleCores(tentativa, resposta) {
  const cores = Array(5).fill('x');
  const sobra = {};
  for (let i = 0; i < 5; i++) {
    if (tentativa[i] === resposta[i]) cores[i] = 'g';
    else sobra[resposta[i]] = (sobra[resposta[i]] || 0) + 1;
  }
  for (let i = 0; i < 5; i++) {
    if (cores[i] !== 'x') continue;
    if (sobra[tentativa[i]] > 0) { cores[i] = 'y'; sobra[tentativa[i]]--; }
  }
  return cores.join('');
}

let wordleDados = { dias: {} };   // dias[dia][uid] = { tentativas: [palavra], fim: 'ganhou'|'perdeu'|null }
function loadWordle() {
  if (!fs.existsSync(WORDLE_FILE)) return;
  try {
    const d = JSON.parse(decryptBuf(fs.readFileSync(WORDLE_FILE)).toString('utf8'));
    if (d && typeof d.dias === 'object') wordleDados = d;
  } catch (e) {
    try { fs.renameSync(WORDLE_FILE, WORDLE_FILE + '.ilegivel-' + Date.now()); } catch { /* sem o que fazer */ }
    console.error('Wordle ilegível, posto de lado:', e.message);
  }
}
function saveWordle() {
  try {
    // Guarda ~400 dias: o suficiente para a sequência ("streak") do grupo.
    const corte = wordleDia() - 400;
    for (const d of Object.keys(wordleDados.dias)) if (wordleBase(d) < corte) delete wordleDados.dias[d];
    for (const d of Object.keys(wordleDados.overrides || {})) if (wordleBase(d) < corte) delete wordleDados.overrides[d];
    for (const d of Object.keys(wordleDados.rodadas || {})) if (Number(d) < corte) delete wordleDados.rodadas[d];
    escreveArquivoAtomico(WORDLE_FILE, encryptBuf(Buffer.from(JSON.stringify(wordleDados), 'utf8')), { mode: 0o600 });
  } catch (e) {
    console.error('Não consegui salvar o Wordle:', e.message);
  }
}
loadWordle();

function wordleEstadoDe(uid, dia) {
  const reg = (wordleDados.dias[dia] || {})[uid] || { tentativas: [], fim: null };
  const resposta = wordlePalavraDoDia(dia);
  return {
    dia, numero: wordleNumero(dia), max: WORDLE_TENTATIVAS, fim: reg.fim,
    tentativas: reg.tentativas.map(p => ({ palavra: p, cores: wordleCores(p, resposta) })),
    resposta: reg.fim ? resposta : undefined
  };
}

// Resumo do grupo no dia: quem jogou e só as CORES (nunca as letras), mais a
// sequência de dias seguidos (até ontem) em que alguém do grupo acertou.
function wordleDoGrupo(conversa, dia) {
  const resposta = wordlePalavraDoDia(dia);
  const jogadores = [];
  for (const uid of conversa.membros) {
    const reg = (wordleDados.dias[dia] || {})[uid];
    if (!reg || !reg.tentativas.length) continue;
    const conta = contaPorUid(uid);
    jogadores.push({
      uid, name: conta ? conta.nome : 'Alguém', avatar: conta ? avatarUrlDaConta(conta) : null,
      n: reg.tentativas.length, fim: reg.fim,
      cores: reg.tentativas.map(p => wordleCores(p, resposta))
    });
  }
  // A sequência conta dias (qualquer rodada do dia vale), não rodadas.
  const base = wordleBase(dia);
  const acertouNoDia = d => wordleChavesDoDia(d).some(k =>
    conversa.membros.some(u => ((wordleDados.dias[k] || {})[u] || {}).fim === 'ganhou'));
  let streak = 0;
  for (let d = base - 1; streak < 400 && acertouNoDia(d); d--) streak++;
  if (acertouNoDia(base)) streak++;
  // Resultados de ontem (para a mensagem "Your group is on a N day streak").
  const ontemResp = wordlePalavraDoDia(String(base - 1));
  const ontem = [];
  for (const uid of conversa.membros) {
    const reg = (wordleDados.dias[base - 1] || {})[uid];
    if (!reg || !reg.fim) continue;
    const conta = contaPorUid(uid);
    ontem.push({ uid, name: conta ? conta.nome : 'Alguém', avatar: conta ? avatarUrlDaConta(conta) : null,
      n: reg.tentativas.length, fim: reg.fim, cores: reg.tentativas.map(p => wordleCores(p, ontemResp)) });
  }
  return { dia, numero: wordleNumero(dia), streak, hoje: jogadores, ontem };
}

// Retrato de uma entrada do histórico para o favorito.
function retratoDeFavorito(conversaId, e) {
  return {
    conversa: conversaId, id: entryId(e), autor: e.name || '', uid: e.uid || undefined,
    texto: e.text ? String(e.text).slice(0, 300) : '',
    tipo: e.imageId ? 'imagem' : (e.gif ? 'gif' : (e.enquete ? 'enquete' : 'texto')),
    ts: e.ts || 0, em: Date.now()
  };
}

// Quem foi citado com @ na mensagem. A lista sai de quem está na sala AGORA, e
// a menção é guardada pelo nome do Discord — que é a única identidade que
// existe aqui.
// O que se digita depois do @ pra citar alguém. Quem tem conta é citado pelo
// nome de USUÁRIO, que é único; quem entrou pelo Discord (ou sem conta) é
// citado pelo nome mesmo, como sempre foi.
//
// Isso existe porque nome de exibição deixou de ser único: com dois "Gabriel"
// na sala, `@Gabriel` avisava os DOIS, e nenhum dos dois tinha como citar só o
// outro.
function arrobaDe(peer) {
  return peer.arroba || peer.name || null;
}

// Devolve os nomes (pra mostrar e pro histórico antigo continuar valendo) e os
// uids (que é quem decide de verdade se você foi citado).
function achaMencoes(texto, room) {
  if (!texto || !texto.includes('@')) return { nomes: [], uids: [] };
  const alvo = texto.toLowerCase();
  const nomes = new Set(), uids = new Set();
  for (const [, peer] of room) {
    const arroba = arrobaDe(peer);
    if (!arroba || !alvo.includes('@' + arroba.toLowerCase())) continue;
    if (peer.name) nomes.add(peer.name);
    if (peer.uid) uids.add(peer.uid);
  }
  return { nomes: [...nomes], uids: [...uids] };
}

// --- Limite de conexões por IP (item 35) -------------------------------------
// Não existia nenhum: uma pessoa mal-intencionada abria milhares de conexões e
// derrubava uma VPS de 954MB que o OOM-killer já visitou.
//
// O detalhe que faz ou quebra isso: em produção o app fica atrás de um nginx,
// então `remoteAddress` é SEMPRE 127.0.0.1 e o limite contaria o mundo inteiro
// como um IP só — na prática, derrubaria todo mundo depois do 12º. Por isso o
// X-Forwarded-For só é levado em conta quando quem conectou é a própria
// máquina; vindo de fora, esse cabeçalho é escrito pelo cliente e não vale nada.
const MAX_WS_POR_IP = 12;
const conexoesPorIp = new Map();   // ip -> quantidade

function ehLocal(ip) {
  return ip === '127.0.0.1' || ip === '::1' || ip === '::ffff:127.0.0.1';
}

function ipDoPedido(req) {
  const direto = req.socket.remoteAddress || 'desconhecido';
  if (!ehLocal(direto)) return direto;
  const encaminhado = req.headers['x-forwarded-for'];
  if (!encaminhado) return direto;
  return String(encaminhado).split(',')[0].trim() || direto;
}

wss.on('connection', (ws, req) => {
  const ip = ipDoPedido(req);
  const abertas = conexoesPorIp.get(ip) || 0;
  if (abertas >= MAX_WS_POR_IP) {
    // 1013 é "tente de novo mais tarde" — o cliente já reconecta com espera
    // crescente, então isso vira uma desistência educada e não um laço apertado.
    try { ws.close(1013, 'muitas conexões'); } catch { /* já caiu */ }
    return;
  }
  conexoesPorIp.set(ip, abertas + 1);
  ws.on('close', () => {
    const agora = (conexoesPorIp.get(ip) || 1) - 1;
    if (agora <= 0) conexoesPorIp.delete(ip);
    else conexoesPorIp.set(ip, agora);
  });

  let currentRoom = null;
  let myId = Math.random().toString(36).slice(2, 9);
  let avisosUid = null;   // uid quando esta conexão é o canal de avisos

  // A identidade sai da sessão do Discord, resolvida aqui no handshake — NÃO
  // do payload de `join`. Antes o servidor aceitava qualquer `name` que o
  // cliente mandasse, e como a checagem de dono pra apagar mensagem do chat é
  // por nome (`deleteMessageEntry`), bastava conectar dizendo se chamar como
  // outra pessoa pra apagar as mensagens dela — e pra falar por ela na sala.
  // O login do Discord era exigido só nas rotas HTTP e na UI; o WebSocket não
  // olhava sessão nenhuma. Funciona porque o WS é mesma origem
  // (`wss://${location.host}`) e o cookie é `SameSite=Lax`, então ele chega
  // no handshake como em qualquer requisição.
  //
  // Quem conecta sem sessão continua entrando (é o que mantém os smoke tests
  // rodando contra um servidor de verdade), mas não consegue mais se
  // apresentar com o nome de ninguém.
  const cookies = parseCookies(req);
  const session = (cookies.session && sessions.get(cookies.session)) || null;
  let myName = session ? session.name : 'Anônimo';
  // Conta local: resolve o avatar AGORA a partir da conta, não do que ficou
  // salvo na sessão no login — senão trocar o avatar só valeria depois de
  // deslogar e entrar de novo. Discord continua vindo da sessão, como sempre.
  let myAvatar = session ? (session.tipo === 'local' ? avatarUrlDaConta(contas.get(session.usuario)) : session.avatar) : null;
  // Identidade de verdade — ver `uidDaSessao`. Sem sessão fica nulo e tudo
  // continua caindo na comparação por nome, como antes.
  const myUid = uidDaSessao(session);
  // Conta local tem nome de usuário; Discord não tem nada equivalente aqui, e
  // aí a citação continua sendo pelo nome de exibição.
  const myArroba = (session && session.usuario) || null;
  // A bio viaja junto com o resto da identidade pra que o cartão de perfil de
  // quem está na sala não precise de uma volta ao servidor pra ser aberto.
  // Vem da conta AGORA, não da sessão, pelo mesmo motivo do avatar: editar a
  // bio tem que valer sem deslogar.
  const minhaBio = (session && session.tipo === 'local'
    && contas.get(session.usuario)?.perfil?.bio) || '';
  // Ritmo de envio desta conexão (ver o limite mais abaixo, no handler de chat).
  let historicoDeEnvio = [];

  // Orçamento das ações de atividade (quadro, som, reação, cutucada). Uma
  // única função porque o remédio é o mesmo pra todas e o que muda é só o
  // número — e porque cada uma com o seu controle acabaria com uma esquecida.
  const ultimasAcoes = {};
  function podeAgir(nome, limite, janelaMs) {
    const agora = Date.now();
    const lista = (ultimasAcoes[nome] = (ultimasAcoes[nome] || []).filter(t => agora - t < janelaMs));
    if (lista.length >= limite) return false;
    lista.push(agora);
    return true;
  }

  const minhaChave = chaveDeIdentidade(myUid, myName);
  const minhaChaveDeModeracao = chaveDeModeracao(myUid, myName, ip);
  // Decoração escolhida pelo próprio usuário — ao contrário do nome, essas
  // duas VÊM do cliente de propósito (não há de onde mais tirar). Nenhuma
  // decisão de autorização olha pra elas.
  let myColor = null;
  let myNameColor = null;
  let myNameEffect = null;

  ws.isAlive = true;
  ws.on('pong', heartbeat);

  ws.on('message', (raw) => {
    let msg;
    try { msg = JSON.parse(raw); } catch { return; }

    if (msg.type === 'avisos') {
      // Identidade da SESSÃO, como em tudo: sem conta local não há quem avisar.
      if (!myUid || !session || session.tipo !== 'local') {
        safeSend(ws, JSON.stringify({ type: 'avisos-negado' }));
        return;
      }
      if (avisosUid) return;   // já aberto nesta conexão
      avisosUid = myUid;
      if (!avisosPorUid.has(myUid)) avisosPorUid.set(myUid, new Set());
      avisosPorUid.get(myUid).add(ws);
      marcaVisto(session);
      safeSend(ws, JSON.stringify({ type: 'avisos-ok' }));
      // Retrato do que já está acontecendo: quem entra no canal no meio de uma
      // chamada precisa saber, senão só descobre na próxima mudança.
      for (const c of conversasDe(myUid)) {
        const dentro = quemEstaNaChamada(c.id);
        if (dentro.length) safeSend(ws, JSON.stringify({ type: 'chamada', conversa: c.id, dentro, chamando: [...(toques.get(c.id)?.keys() || [])], ativa: true }));
      }
      for (const c of conversasDe(myUid)) {
        const t = toqueAtivoDe(c.id, myUid);
        if (t) safeSend(ws, JSON.stringify(eventoDeToque(c.id, t.quem, t.ate)));
      }
      avisaAmigos(myUid, { type: 'status-amigo', uid: myUid, status: statusVisivelDe(myUid) });
      return;
    }

    // --- Toque ---------------------------------------------------------
    // Valem em qualquer conexão com sessão (canal de avisos ou sala): quem
    // decide é o uid da SESSÃO e a lista de membros, nunca texto do cliente.
    if (msg.type === 'ligar' || msg.type === 'recusar' || msg.type === 'tocar-de-novo') {
      const c = conversaPorId(msg.conversa);
      if (!myUid || !c || !ehMembro(c, myUid)) {
        safeSend(ws, JSON.stringify({ type: 'toque-erro', conversa: msg.conversa, motivo: 'nao-membro' }));
        return;
      }
      if (msg.type === 'ligar') {
        if (!podeAgir('ligar', 5, 30000)) {
          safeSend(ws, JSON.stringify({ type: 'toque-erro', conversa: c.id, motivo: 'limite' }));
          return;
        }
        const r = ligaParaConversa(c, myUid);
        safeSend(ws, JSON.stringify({ type: 'ligar-ok', conversa: c.id, jaAtiva: r.jaAtiva }));
        return;
      }
      if (msg.type === 'recusar') {
        // Só o PRÓPRIO toque: recusar não derruba a chamada pra ninguém.
        fimDoToque(c.id, myUid, 'recusou');
        return;
      }
      // tocar-de-novo: quem está dentro, para um membro que está fora.
      const alvo = typeof msg.uid === 'string' ? msg.uid : '';
      const dentro = quemEstaNaChamada(c.id);
      let erro = null;
      if (!dentro.includes(myUid)) erro = 'nao-esta-na-chamada';
      else if (!ehMembro(c, alvo) || alvo === myUid) erro = 'nao-membro';
      else if (dentro.includes(alvo)) erro = 'alvo-na-chamada';
      else if (Date.now() - (ultimoRepique.get(`${c.id}|${alvo}`) || 0) < TOQUE_REPIQUE_MS) erro = 'limite';
      if (erro) {
        safeSend(ws, JSON.stringify({ type: 'toque-erro', conversa: c.id, motivo: erro }));
        return;
      }
      ultimoRepique.set(`${c.id}|${alvo}`, Date.now());
      iniciaToque(c, alvo, myUid);
      return;
    }

    if (msg.type === 'join') {
      // `msg.name`/`msg.avatar` são ignorados de propósito — ver o comentário
      // sobre identidade lá em cima. A partir da v3 não existe sala por nome:
      // entra-se numa CONVERSA, e o id dela é o nome da sala interna. Quem
      // decide se pode é a lista de membros no servidor, comparada com o uid da
      // SESSÃO — nunca com texto do cliente.
      if (typeof msg.conversa !== 'string') {
        safeSend(ws, JSON.stringify({ type: 'join-denied', reason: 'sem-conversa' }));
        return;
      }
      const conversa = conversaPorId(msg.conversa);
      // Mesma resposta pra "não existe" e "não é seu": não confirma id alheio.
      if (!conversa || !ehMembro(conversa, myUid)) {
        safeSend(ws, JSON.stringify({ type: 'join-denied', reason: 'nao-membro' }));
        return;
      }
      const nomeDaSala = conversa.id;

      myColor = limpaCor(msg.color);
      myNameColor = limpaIdentificador(msg.nameColor);
      myNameEffect = limpaIdentificador(msg.nameEffect);

      // Entrar de novo na MESMA conexão sem ter saído da anterior deixava um
      // fantasma na sala velha (ver `saiDaSalaAtual`). O cliente de verdade
      // nunca faz isso — ele recarrega a página, o que abre um socket novo —
      // mas nada impedia um cliente qualquer de fazer, e o estrago era
      // permanente. Sai só DEPOIS das checagens: um join recusado não pode
      // arrancar a pessoa da sala em que ela já estava.
      if (currentRoom) saiDaSalaAtual();

      currentRoom = nomeDaSala;
      if (!rooms.has(currentRoom)) rooms.set(currentRoom, new Map());
      const room = rooms.get(currentRoom);
      // O dono da sala interna é o dono do grupo (conversa direta não tem).
      // Reafirmado a cada entrada: a posse pode ter passado a outra pessoa.
      const donoDoGrupo = conversa.dono && contaPorUid(conversa.dono);
      if (donoDoGrupo) roomOwners.set(currentRoom, { uid: conversa.dono, nome: donoDoGrupo.nome });
      else roomOwners.delete(currentRoom);
      const souDono = ehODono(currentRoom, myUid, myName);
      const silenciado = roomForcedMutes.get(currentRoom)?.has(minhaChaveDeModeracao) || false;

      // Avisa aos que já estão na sala que alguém novo chegou
      // Espectador está na SALA mas não na CHAMADA: lê e escreve no chat, vê
      // quem está lá, e não monta conexão de mídia com ninguém. É o que faz
      // sentido num celular (que não aguenta mesh de WebRTC mas aguenta muito
      // bem uma tela de chat) e pra quem só quer acompanhar.
      //
      // Quem decide não montar conexão é o CLIENTE dos dois lados — o servidor
      // só carrega a informação. É o mesmo desenho do `screen-state`: aqui não
      // há como saber, e fingir que sabe é que dá divergência.
      const souEspectador = !!msg.espectador;

      const joinedPayload = JSON.stringify({ type: 'peer-joined', id: myId, uid: myUid || undefined, arroba: myArroba || undefined, name: myName, avatar: myAvatar, color: myColor, nameColor: myNameColor, nameEffect: myNameEffect, owner: souDono, forcedMute: silenciado, status: statusParaOutros(minhaChave), bio: minhaBio, espectador: souEspectador });
      for (const [, peer] of room) {
        safeSend(peer.ws, joinedPayload);
      }
      // Manda pro novo membro a lista de quem já está (com nome/avatar de cada
      // um). Vai junto o estado de mic/surdez: sem isso quem entra depois via
      // todo mundo como "microfone ligado, ouvindo" até a pessoa mexer no
      // botão, e o ícone da sidebar ficava mentindo o tempo todo.
      safeSend(ws, JSON.stringify({
        type: 'room-info',
        id: myId,
        uid: myUid || undefined,
        arroba: myArroba || undefined,
        owner: nomeDoDono(currentRoom),
        youAreOwner: souDono,
        forcedMute: silenciado,
        conversa: conversa.id,
        tipo: conversa.tipo,
        icone: urlDoIconeDeGrupo(conversa),
        peers: [...room.entries()].map(([id, peer]) => ({
          id, uid: peer.uid || undefined, arroba: peer.arroba || undefined, name: peer.name, avatar: peer.avatar,
          color: peer.color || null, nameColor: peer.nameColor || null, nameEffect: peer.nameEffect || null,
          muted: !!peer.muted, deafened: !!peer.deafened, atividade: peer.atividade || null,
          status: peer.status || STATUS_PADRAO, bio: peer.bio || '',
          owner: ehODono(currentRoom, peer.uid, peer.name),
          forcedMute: !!peer.forcedMute,
          espectador: !!peer.espectador
        }))
      }));

      // Histórico de chat da sala (últimas mensagens salvas em disco)
      const history = loadHistoryPayload(currentRoom);
      if (history.length) {
        safeSend(ws, JSON.stringify({ type: 'chat-history', messages: history }));
      }
      // Depois do histórico: a barra de fixado só faz sentido com as mensagens
      // já na tela pra ela poder rolar até lá.
      if (roomPins.has(currentRoom)) {
        safeSend(ws, JSON.stringify({ type: 'chat-pin', pinned: roomPins.get(currentRoom) }));
      }
      // O quadro branco que já estava desenhado. Vai em UMA mensagem com todos
      // os traços — mandar um por um seria uma rajada de centenas de frames na
      // entrada de cada pessoa.
      const quadroAtual = roomBoards.get(currentRoom);
      if (quadroAtual && quadroAtual.tracos.length) {
        safeSend(ws, JSON.stringify({ type: 'quadro-estado', tracos: quadroAtual.tracos }));
      }
      const roletasDaSala = (confDeSalas.get(currentRoom) || {}).roletas;
      if (roletasDaSala && roletasDaSala.length) {
        safeSend(ws, JSON.stringify({ type: 'roletas', roletas: roletasDaSala }));
      }

      room.set(myId, { ws, name: myName, uid: myUid, arroba: myArroba, chave: minhaChave, chaveMod: minhaChaveDeModeracao, avatar: myAvatar, color: myColor, nameColor: myNameColor, nameEffect: myNameEffect, muted: silenciado, deafened: false, forcedMute: silenciado, status: statusParaOutros(minhaChave), bio: minhaBio, espectador: souEspectador });
      picoDePessoas = Math.max(picoDePessoas, [...rooms.values()].reduce((t, r) => t + r.size, 0));
      avisaChamada(currentRoom);
      return;
    }

    // Aparência trocada com a call já rolando. Sem isso a escolha só valeria
    // na próxima vez que a pessoa entrasse numa sala, e "mudei a cor e não
    // mudou nada" parece bug.
    if (msg.type === 'profile' && currentRoom) {
      const room = rooms.get(currentRoom);
      if (!room) return;
      myColor = limpaCor(msg.color);
      myNameColor = limpaIdentificador(msg.nameColor);
      myNameEffect = limpaIdentificador(msg.nameEffect);
      const me = room.get(myId);
      if (me) { me.color = myColor; me.nameColor = myNameColor; me.nameEffect = myNameEffect; }
      const payload = JSON.stringify({ type: 'profile', from: myId, color: myColor, nameColor: myNameColor, nameEffect: myNameEffect });
      for (const [id, peer] of room) {
        if (id !== myId) safeSend(peer.ws, payload);
      }
      return;
    }

    // "Fulano está digitando..." — efêmero, não persiste nada e não volta pra
    // quem mandou. O cliente é quem controla a frequência (manda no começo e
    // um "parou" depois de uma pausa); aqui é só repasse.
    if (msg.type === 'typing' && currentRoom) {
      const room = rooms.get(currentRoom);
      if (!room) return;
      const payload = JSON.stringify({ type: 'typing', from: myId, name: myName, typing: !!msg.typing });
      for (const [id, peer] of room) {
        if (id !== myId) safeSend(peer.ws, payload);
      }
      return;
    }

    // Estado do mic (mutado ou não) — repassa pra todo mundo da sala mostrar
    // o ícone certo na lista de participantes, sem persistir em disco.
    // Entrar na chamada (ou voltar pro chat) com a sala já aberta. Mesma
    // estrada do `mic-state`: guarda no peer (senão quem entrar depois vê o
    // estado errado) e repassa pros outros, sem eco pra quem mandou.
    if (msg.type === 'mode-state' && currentRoom) {
      const room = rooms.get(currentRoom);
      if (!room) return;
      const me = room.get(myId);
      if (me) me.espectador = !!msg.espectador;
      const payload = JSON.stringify({ type: 'mode-state', from: myId, espectador: !!msg.espectador });
      for (const [id, peer] of room) {
        if (id !== myId) safeSend(peer.ws, payload);
      }
      avisaChamada(currentRoom);
      return;
    }

    if (msg.type === 'mic-state' && currentRoom) {
      const room = rooms.get(currentRoom);
      if (!room) return;
      const me = room.get(myId);
      // Silenciado pelo dono não se dessilencia sozinho: sem essa linha, o
      // botão de mic do próprio cliente desfaria a moderação.
      if (me && me.forcedMute && !msg.muted) return;
      if (me) me.muted = !!msg.muted;
      const payload = JSON.stringify({ type: 'mic-state', from: myId, muted: !!msg.muted, viaDeafen: !!msg.viaDeafen, resync: !!msg.resync });
      for (const [id, peer] of room) {
        if (id !== myId) safeSend(peer.ws, payload);
      }
      return;
    }

    // Estado de "mutar áudio" (ensurdecer) — toca o som de aviso nos outros
    // participantes e acende o ícone de surdez na lista deles. Não muda nada
    // na conexão de mídia: quem ensurdece zera o ganho do próprio lado.
    if (msg.type === 'deafen-state' && currentRoom) {
      const room = rooms.get(currentRoom);
      if (!room) return;
      const me = room.get(myId);
      if (me) me.deafened = !!msg.deafened;
      const payload = JSON.stringify({ type: 'deafen-state', from: myId, deafened: !!msg.deafened, resync: !!msg.resync });
      for (const [id, peer] of room) {
        if (id !== myId) safeSend(peer.ws, payload);
      }
      return;
    }

    // Avisa a sala qual MediaStream é a tela compartilhada. Sem isso o
    // receptor tinha que adivinhar ("a primeira stream que chegou é a voz"), e
    // essa heurística erra quando alguém entra na sala com a apresentação já
    // rolando, ou quando o microfone foi negado e a voz nem existe.
    if (msg.type === 'screen-state' && currentRoom) {
      const room = rooms.get(currentRoom);
      if (!room) return;
      const streamId = typeof msg.streamId === 'string' ? msg.streamId.slice(0, 128) : null;
      if (!streamId) return;
      const payload = JSON.stringify({ type: 'screen-state', from: myId, streamId, active: !!msg.active });
      for (const [id, peer] of room) {
        if (id !== myId) safeSend(peer.ws, payload);
      }
      return;
    }

    // Câmera ligada/desligada. Existe pela MESMA razão do screen-state, e a
    // causa é a mesma: `pc.removeTrack` não dispara `ended` na track do outro
    // lado — o receptor só marca a track como muda. Sem este aviso, desligar a
    // câmera deixava a caixinha da pessoa congelada no último quadro pra sala
    // inteira, até ela sair da chamada. Foi exatamente o bug que a
    // transmissão de tela teve, e a câmera não tinha ganhado a correção.
    // Atividade: que jogo a pessoa está jogando e que música está tocando.
    // Vem do cliente porque é o único que sabe (a detecção roda no processo
    // principal do Electron), e por isso mesmo NENHUMA decisão de autorização
    // olha pra isso — é decoração, igual cor do nome.
    if (msg.type === 'activity-state' && currentRoom) {
      const room = rooms.get(currentRoom);
      if (!room) return;
      const atividade = limpaAtividade(msg.atividade);
      const eu = room.get(myId);
      if (eu) eu.atividade = atividade;
      const payload = JSON.stringify({ type: 'activity-state', from: myId, atividade });
      for (const [id, peer] of room) {
        if (id !== myId) safeSend(peer.ws, payload);
      }
      // Fora da chamada quem mostra a atividade é a lista de amigos, que lê
      // daqui — ver `atividadePorChave`.
      if (minhaChave) guardaAtividade(minhaChave, atividade);
      return;
    }

    // Status escolhido à mão (online / ausente / não perturbe). Anda pela
    // mesma estrada do mic e da surdez: broadcast, sem eco pra quem mandou, e
    // reanunciado no resync — sem isso uma oscilação de wifi devolveria todo
    // mundo pra "online" na tela dos outros.
    //
    // Escreve no MESMO lugar que a rota HTTP: se fossem dois estados, escolher
    // "não perturbe" na tela inicial e entrar numa sala voltaria pra online.
    if (msg.type === 'status-state' && currentRoom) {
      const room = rooms.get(currentRoom);
      if (!room) return;
      const status = limpaStatus(msg.status);
      if (!status) return;
      // Sem sessão a identidade é a MESMA pra todo mundo (`nome:Anônimo`), então
      // guardar o status ali faria a escolha de um anônimo aparecer em todos os
      // outros. Quem não tem conta simplesmente fica em "online".
      if (!session) return;
      guardaStatus(session, minhaChave, status);
      const eu = room.get(myId);
      const statusPublico = status === 'invisivel' ? 'offline' : status;
      if (eu) eu.status = statusPublico;
      const payload = JSON.stringify({ type: 'status-state', from: myId, status: statusPublico });
      for (const [id, peer] of room) {
        if (id !== myId) safeSend(peer.ws, payload);
      }
      return;
    }

    if (msg.type === 'camera-state' && currentRoom) {
      const room = rooms.get(currentRoom);
      if (!room) return;
      const payload = JSON.stringify({ type: 'camera-state', from: myId, active: !!msg.active });
      for (const [id, peer] of room) {
        if (id !== myId) safeSend(peer.ws, payload);
      }
      return;
    }

    // Aviso de eco: quem DETECTA é o outro lado (só ele consegue comparar o
    // som que mandou com o que voltou), mas quem precisa ver o aviso é a
    // pessoa com o microfone aberto perto da caixa de som. Por isso é
    // endereçado, e não broadcast — avisar a sala inteira que "alguém está com
    // eco" não ajudaria ninguém a resolver.
    if (msg.type === 'echo-warning' && currentRoom) {
      const room = rooms.get(currentRoom);
      const alvo = room && room.get(msg.to);
      if (alvo) safeSend(alvo.ws, JSON.stringify({ type: 'echo-warning', from: myId }));
      return;
    }

    // Repassa mensagens de sinalização (offer/answer/ice) só pro destinatário certo
    if (['offer', 'answer', 'ice-candidate'].includes(msg.type) && currentRoom) {
      const room = rooms.get(currentRoom);
      const target = room && room.get(msg.to);
      if (target) {
        safeSend(target.ws, JSON.stringify({ ...msg, from: myId }));
      }
    }

    // Chat: manda pra todo mundo na sala (inclusive quem escreveu, pra manter
    // uma única ordem de exibição pra todo mundo) e salva criptografado em
    // disco pra sobreviver a F5 e a reinícios do servidor.
    if (msg.type === 'chat' && currentRoom) {
      const room = rooms.get(currentRoom);
      if (!room) return;

      // Limite de ritmo (item 9 do ROADMAP). Não existia nenhum: o teto
      // teórico era 300 imagens × 5MB POR SALA, e o número de salas é livre.
      // Numa VPS de 954MB e 39GB, uma pessoa distraída com o dedo no Enter (ou
      // um cliente com defeito num laço) enchia disco e RAM sem esforço.
      //
      // Dois limites porque os dois custos são diferentes: texto custa banda e
      // atenção; imagem custa DISCO, e disco não volta sozinho.
      const agora = Date.now();
      historicoDeEnvio = historicoDeEnvio.filter(t => agora - t.em < JANELA_DE_IMAGEM_MS);
      const textosRecentes = historicoDeEnvio.filter(t => agora - t.em < JANELA_DE_TEXTO_MS).length;
      const imagensRecentes = historicoDeEnvio.filter(t => t.comImagem).length;
      const temImagem = !!(msg.image && typeof msg.image.data === 'string');
      if (textosRecentes >= LIMITE_DE_TEXTO || (temImagem && imagensRecentes >= LIMITE_DE_IMAGEM)) {
        safeSend(ws, JSON.stringify({
          type: 'chat-recusado',
          motivo: temImagem && imagensRecentes >= LIMITE_DE_IMAGEM ? 'imagens' : 'ritmo'
        }));
        return;
      }
      historicoDeEnvio.push({ em: agora, comImagem: temImagem });

      // As mensagens podem ter quebra de linha desde que o campo virou
      // textarea (Shift+Enter). Três ou mais seguidas viram duas: sem isso uma
      // mensagem de dez linhas em branco empurra o histórico inteiro pra fora
      // da tela de todo mundo, e isso fica salvo em disco.
      const text = typeof msg.text === 'string' ? msg.text.slice(0, 2000).replace(/\n{3,}/g, '\n\n').trim() : '';

      let imageId = null, imageMime = null, imageBuf = null;
      if (msg.image && typeof msg.image.data === 'string') {
        try { imageBuf = Buffer.from(msg.image.data, 'base64'); } catch { imageBuf = null; }
        const detectado = imageBuf && imageBuf.length > 0 && imageBuf.length <= MAX_IMAGE_BYTES
          ? sniffImageMime(imageBuf)
          : null;
        if (detectado && ALLOWED_IMAGE_MIME.has(detectado)) {
          imageId = saveImage(imageBuf);
          imageMime = detectado;
        } else {
          imageBuf = null;
        }
      }

      // GIF: viaja como URL (o arquivo nunca passa por aqui). O host é
      // conferido de novo aqui — o cliente confere pra não mostrar besteira, o
      // servidor confere porque é ele que grava.
      const gif = limpaUrlDeGif(msg.gif);
      const enquete = limpaEnquete(msg.enquete);

      // Arquivo já enviado por POST /api/arquivo: só vale pra quem enviou, na
      // conversa em que foi enviado, e uma vez.
      let arquivo = null;
      if (typeof msg.arquivoId === 'string') {
        const p = arquivosPendentes.get(msg.arquivoId);
        if (p && p.uid && p.uid === myUid && p.conversa === currentRoom) {
          arquivo = { id: msg.arquivoId, nome: p.nome, tam: p.tam };
          arquivosPendentes.delete(msg.arquivoId);
        }
      }

      if (!text && !imageId && !gif && !enquete && !arquivo) return;

      // --- Sussurro: mensagem só pra quem foi escolhido ------------------
      //
      // EFÊMERO de propósito, e essa é a decisão que segura o resto: nada vai
      // pro disco, então não existe histórico privado pra filtrar por pessoa
      // na hora de servir — que é exatamente onde um erro vazaria conversa
      // fechada pra sala inteira. Quem não estava online não recebe, e isso é
      // dito na tela de quem manda.
      //
      // Texto só: aceitar imagem obrigaria a gravar o arquivo (`saveImage`),
      // e aí o sussurro passaria pelo disco pela porta dos fundos.
      if (Array.isArray(msg.para) && msg.para.length) {
        if (!text) return;
        // Ids da própria sala, no máximo dez, sem repetição e sem você mesmo
        // (você já recebe de volta, logo abaixo).
        const destinos = [...new Set(msg.para)]
          .filter(id => typeof id === 'string' && id !== myId && room.has(id))
          .slice(0, 10);
        if (!destinos.length) return;
        const ts = Date.now();
        const id = crypto.randomBytes(6).toString('hex');
        const nomes = destinos.map(d => room.get(d).name);
        const payload = JSON.stringify({
          type: 'chat', id, from: myId, uid: myUid || undefined, name: myName,
          text, ts, sussurro: true, sussurroPara: nomes,
          // A menção continua valendo dentro do sussurro, mas só entre quem
          // recebe: quem não está na lista nunca vê a mensagem.
          mentions: undefined, mentionUids: undefined
        });
        for (const destino of destinos) safeSend(room.get(destino).ws, payload);
        safeSend(ws, payload);
        return;
      }

      // Se for resposta a outra mensagem, guarda só um retrato (id/nome/
      // trecho) mandado pelo cliente — assim a citação sobrevive mesmo que
      // a mensagem original seja apagada ou já tenha saído do histórico.
      let replyTo = null;
      if (msg.replyTo && typeof msg.replyTo === 'object') {
        const rid = typeof msg.replyTo.id === 'string' ? msg.replyTo.id.slice(0, 32) : null;
        if (rid) {
          replyTo = {
            id: rid,
            name: typeof msg.replyTo.name === 'string' ? msg.replyTo.name.slice(0, 64) : '',
            text: typeof msg.replyTo.text === 'string' ? msg.replyTo.text.slice(0, 200) : ''
          };
        }
      }

      const ts = Date.now();
      const id = crypto.randomBytes(6).toString('hex');
      const mencao = achaMencoes(text, room);
      appendMessage(currentRoom, { id, from: myId, uid: myUid || undefined, name: myName, text: text || undefined, imageId, imageMime, gif: gif || undefined, arquivo: arquivo || undefined, enquete: enquete || undefined, replyTo, ts,
        mentions: mencao.nomes.length ? mencao.nomes : undefined,
        mentionUids: mencao.uids.length ? mencao.uids : undefined });

      const conversaDoChat = conversaPorId(currentRoom);
      if (conversaDoChat) {
        const previaDoAviso = text ? text.replace(/\s+/g, ' ').slice(0, 80)
          : (imageBuf ? '[imagem]' : (arquivo ? '[arquivo]' : (gif ? '[GIF]' : (enquete ? '[enquete]' : ''))));
        avisaUids(conversaDoChat.membros, { type: 'mensagem', conversa: conversaDoChat.id, autor: myName, uid: myUid || undefined, previa: previaDoAviso, ts });
      }

      const payload = JSON.stringify({
        type: 'chat', id, from: myId, uid: myUid || undefined, name: myName,
        text: text || undefined,
        image: imageBuf ? { mime: imageMime, data: imageBuf.toString('base64') } : undefined,
        gif: gif || undefined,
        arquivo: arquivo || undefined,
        enquete: enquete || undefined,
        replyTo: replyTo || undefined,
        ts,
        mentions: mencao.nomes.length ? mencao.nomes : undefined,
        mentionUids: mencao.uids.length ? mencao.uids : undefined
      });
      for (const [, peer] of room) {
        safeSend(peer.ws, payload);
      }
      return;
    }

    // Editar mensagem: só quem mandou pode, mesma regra do apagar. A menção é
    // recalculada — senão, editar pra citar alguém não notificaria ninguém, e
    // editar pra TIRAR a citação deixaria a menção antiga presa no arquivo.
    if (msg.type === 'chat-edit' && currentRoom) {
      const room = rooms.get(currentRoom);
      if (!room) return;
      const id = typeof msg.id === 'string' ? msg.id.slice(0, 64) : null;
      const texto = typeof msg.text === 'string' ? msg.text.slice(0, 2000).replace(/\n{3,}/g, '\n\n').trim() : '';
      if (!id) return;
      const entry = editMessageEntry(currentRoom, id, myName, myUid, texto);
      if (!entry) return;
      const mencao = achaMencoes(entry.text, room);
      if (mencao.nomes.length) entry.mentions = mencao.nomes; else delete entry.mentions;
      if (mencao.uids.length) entry.mentionUids = mencao.uids; else delete entry.mentionUids;
      // A entrada já foi gravada dentro do editMessageEntry; regravar aqui é
      // o que fixa a menção nova no disco.
      const todas = readRoomEntries(currentRoom);
      const i = todas.findIndex(e => entryId(e) === id);
      if (i !== -1) { todas[i] = entry; writeRoomEntries(currentRoom, todas); }
      const payload = JSON.stringify({
        type: 'chat-edit', id, text: entry.text || undefined,
        editedAt: entry.editedAt,
        mentions: mencao.nomes.length ? mencao.nomes : undefined,
        mentionUids: mencao.uids.length ? mencao.uids : undefined
      });
      for (const [, peer] of room) safeSend(peer.ws, payload);
      return;
    }

    // Reagir com emoji. A lista de emojis é fechada no servidor: o valor fica
    // salvo em disco e vai pra sala inteira, não é lugar pra texto livre.
    // --- Ícone da sala e roletas -----------------------------------------
    //
    // As duas coisas que sobrevivem à sala esvaziar. Trocar exige ser dono,
    // pela mesma razão da senha: é uma coisa da sala, não de quem está nela
    // agora. (Em sala sem login todo mundo é "Anônimo" e portanto dono — o
    // mesmo limite conhecido do resto da moderação.)
    if ((msg.type === 'roleta-criar' || msg.type === 'roleta-editar'
      || msg.type === 'roleta-apagar' || msg.type === 'roleta-girar') && currentRoom) {
      const room = rooms.get(currentRoom);
      if (!room) return;
      const conf = confDaSala(currentRoom);
      conf.roletas = conf.roletas || [];

      if (msg.type === 'roleta-criar') {
        if (conf.roletas.length >= MAX_ROLETAS_POR_SALA) return;
        const nova = limpaRoleta(msg.roleta);
        if (!nova) return;
        nova.id = crypto.randomBytes(5).toString('hex');
        nova.criadaPor = myName;
        conf.roletas.push(nova);
      } else if (msg.type === 'roleta-editar') {
        const alvo = conf.roletas.find(r => r.id === msg.id);
        const nova = limpaRoleta(msg.roleta);
        if (!alvo || !nova) return;
        alvo.nome = nova.nome;
        alvo.opcoes = nova.opcoes;
      } else if (msg.type === 'roleta-apagar') {
        const antes = conf.roletas.length;
        conf.roletas = conf.roletas.filter(r => r.id !== msg.id);
        if (conf.roletas.length === antes) return;
      } else {
        // Girar: **quem sorteia é o servidor**. É a diferença entre a roleta e
        // o `/sortear` do chat — aqui ninguém pode forjar o resultado, e todo
        // mundo vê exatamente o mesmo giro.
        const alvo = conf.roletas.find(r => r.id === msg.id);
        if (!alvo || !alvo.opcoes.length) return;
        const indice = crypto.randomInt(alvo.opcoes.length);
        const payload = JSON.stringify({ type: 'roleta-resultado', id: alvo.id, indice, por: myName });
        for (const [, peer] of room) safeSend(peer.ws, payload);
        return;
      }

      saveConfDeSalas();
      const payload = JSON.stringify({ type: 'roletas', roletas: conf.roletas });
      for (const [, peer] of room) safeSend(peer.ws, payload);
      return;
    }

    // --- Atividades: quadro, soundboard, reação flutuante, cutucada -------
    //
    // Todas são RELAY: nada vai pro disco. O que elas têm em comum é serem
    // baratas de mandar e caras de aguentar em rajada — som e reação aparecem
    // (ou tocam) na tela de todo mundo. Por isso passam por um orçamento por
    // conexão, e o teto é generoso pra uso normal e curto pra spam.
    if (msg.type === 'quadro' && currentRoom) {
      const room = rooms.get(currentRoom);
      if (!room) return;
      if (!podeAgir('quadro', 40, 1000)) return;
      const traco = limpaTraco(msg.traco);
      if (!traco) return;
      const quadro = quadroDaSala(currentRoom);
      quadro.tracos.push(traco);
      quadro.pontos += (traco.p.length / 2) | 0;
      limpaTracosAntigos(quadro);
      const payload = JSON.stringify({ type: 'quadro', from: myId, traco });
      for (const [id, peer] of room) if (id !== myId) safeSend(peer.ws, payload);
      return;
    }

    if (msg.type === 'quadro-limpar' && currentRoom) {
      const room = rooms.get(currentRoom);
      if (!room) return;
      // "Limpar" é evento próprio e não "um traço vazio": assim a ordem de
      // chegada não decide se o quadro ficou limpo ou não.
      roomBoards.set(currentRoom, { tracos: [], pontos: 0 });
      const payload = JSON.stringify({ type: 'quadro-limpar', from: myId, name: myName });
      for (const [, peer] of room) safeSend(peer.ws, payload);
      return;
    }

    if (msg.type === 'som' && currentRoom) {
      const room = rooms.get(currentRoom);
      if (!room) return;
      // Um som a cada dois segundos por pessoa. O soundboard é a coisa mais
      // divertida da sala na primeira noite e a mais insuportável na segunda;
      // este número é metade do remédio (a outra é o silenciar por pessoa, que
      // é do lado de quem ouve).
      if (!podeAgir('som', 1, 2000)) return;
      const id = limpaIdentificador(msg.id);
      if (!id) return;
      // Volume escolhido por quem solta o som (0–100); vale igual para todos os ouvintes.
      const vol = Number.isFinite(Number(msg.vol)) ? Math.max(0, Math.min(100, Math.round(Number(msg.vol)))) : 40;
      const payload = JSON.stringify({ type: 'som', from: myId, name: myName, id, vol });
      for (const [, peer] of room) safeSend(peer.ws, payload);
      return;
    }

    if (msg.type === 'reacao' && currentRoom) {
      const room = rooms.get(currentRoom);
      if (!room) return;
      if (!podeAgir('reacao', 5, 3000)) return;
      const emoji = typeof msg.emoji === 'string' ? msg.emoji.slice(0, 8) : '';
      if (!emoji || !EMOJIS_PERMITIDOS.has(emoji)) return;
      const payload = JSON.stringify({ type: 'reacao', from: myId, name: myName, emoji });
      for (const [, peer] of room) safeSend(peer.ws, payload);
      return;
    }

    if (msg.type === 'cutucada' && currentRoom) {
      const room = rooms.get(currentRoom);
      if (!room) return;
      if (!podeAgir('cutucada', 1, 5000)) return;
      const alvo = room.get(typeof msg.to === 'string' ? msg.to : '');
      if (!alvo || msg.to === myId) return;
      safeSend(alvo.ws, JSON.stringify({ type: 'cutucada', from: myId, name: myName }));
      // Volta pra quem cutucou, pra ele saber que saiu.
      safeSend(ws, JSON.stringify({ type: 'cutucada-enviada', to: msg.to, name: alvo.name }));
      return;
    }

    // Voto de enquete. Mesma estrada da reação: o valor fica na própria
    // mensagem no disco, e a sala inteira recebe a contagem nova.
    if (msg.type === 'voto' && currentRoom) {
      const room = rooms.get(currentRoom);
      if (!room) return;
      const id = typeof msg.id === 'string' ? msg.id.slice(0, 64) : null;
      if (!id || !Number.isInteger(msg.opcao)) return;
      const votos = registraVoto(currentRoom, id, minhaChave, msg.opcao);
      if (!votos) return;
      const payload = JSON.stringify({ type: 'voto', id, votos });
      for (const [, peer] of room) safeSend(peer.ws, payload);
      return;
    }

    if (msg.type === 'chat-react' && currentRoom) {
      const room = rooms.get(currentRoom);
      if (!room) return;
      const id = typeof msg.id === 'string' ? msg.id.slice(0, 64) : null;
      const emoji = typeof msg.emoji === 'string' ? msg.emoji : null;
      if (!id || !emoji) return;
      const reactions = toggleReaction(currentRoom, id, myName, emoji);
      if (!reactions) return;
      const payload = JSON.stringify({ type: 'chat-react', id, reactions });
      for (const [, peer] of room) safeSend(peer.ws, payload);
      return;
    }

    // Moderação de sala (`room-kick`, `room-mute`, `room-set-password`) saiu na
    // v3: remover alguém é do dono do grupo, por HTTP (`/api/grupos/<id>/membros`).

    // Fixar mensagem: vive na memória da sala, some junto com ela — igual à
    // senha. Guarda um retrato (texto/autor) em vez de só o id, pra a barra
    // continuar dizendo alguma coisa se a mensagem original for apagada.
    if (msg.type === 'chat-pin' && currentRoom) {
      const room = rooms.get(currentRoom);
      if (!room) return;
      if (msg.id === null) {
        roomPins.delete(currentRoom);
      } else {
        const id = typeof msg.id === 'string' ? msg.id.slice(0, 64) : null;
        if (!id) return;
        const entries = readRoomEntries(currentRoom);
        const entry = entries.find(e => entryId(e) === id);
        if (!entry || entry.sys) return;
        roomPins.set(currentRoom, {
          id, name: entry.name,
          text: (entry.text || (entry.imageId ? '📷 Imagem' : '')).slice(0, 200),
          by: myName, ts: Date.now()
        });
      }
      const payload = JSON.stringify({ type: 'chat-pin', pinned: roomPins.get(currentRoom) || null });
      for (const [, peer] of room) safeSend(peer.ws, payload);
      return;
    }

    // Apagar mensagem: só quem mandou pode. Repassa o mesmo evento pra sala
    // inteira remover do próprio chat na hora, e some do arquivo em disco
    // também (junto com a imagem, se tinha).
    if (msg.type === 'chat-delete' && currentRoom) {
      const room = rooms.get(currentRoom);
      if (!room) return;
      const id = typeof msg.id === 'string' ? msg.id.slice(0, 64) : null;
      if (!id) return;
      const ok = deleteMessageEntry(currentRoom, id, myName, myUid);
      if (ok) {
        const payload = JSON.stringify({ type: 'chat-delete', id });
        for (const [, peer] of room) safeSend(peer.ws, payload);
        // Mensagem fixada que foi apagada não pode continuar na barra do topo
        // apontando pra nada.
        if (roomPins.get(currentRoom)?.id === id) {
          roomPins.delete(currentRoom);
          const semPin = JSON.stringify({ type: 'chat-pin', pinned: null });
          for (const [, peer] of room) safeSend(peer.ws, semPin);
        }
      }
      return;
    }
  });

  // Tirar esta conexão da sala em que ela está, avisando a sala. Existe como
  // função (em vez de morar dentro do `close`) porque um segundo `join` na
  // MESMA conexão também precisa disso: sem sair da sala anterior, o `myId`
  // ficava pendurado no Map dela pra sempre — o `close` só limpa a sala
  // corrente, então a antiga nunca esvaziava. Uma conexão só, entrando em N
  // salas, deixava N-1 fantasmas: gente que aparece em `/api/rooms` e no
  // `room-info` de quem entra, sala que nunca some, e memória que nunca volta
  // numa VPS que o OOM-killer já visitou.
  function saiDaSalaAtual() {
    if (!currentRoom || !rooms.has(currentRoom)) { currentRoom = null; return; }
    const room = rooms.get(currentRoom);
    const salaQueSai = currentRoom;
    room.delete(myId);
    const leftPayload = JSON.stringify({ type: 'peer-left', id: myId });
    for (const [, peer] of room) {
      safeSend(peer.ws, leftPayload);
    }
    if (room.size === 0) {
      rooms.delete(currentRoom);
      // Tudo que é de moderação morre com a sala. A senha NÃO: ela agora vive
      // em disco, e é o que impede recriar o nome pra ler o histórico.
      roomPins.delete(currentRoom);
      roomOwners.delete(currentRoom);
      roomBans.delete(currentRoom);
      roomForcedMutes.delete(currentRoom);
      roomBoards.delete(currentRoom);
    }
    currentRoom = null;
    avisaChamada(salaQueSai);
  }

  ws.on('close', () => {
    saiDaSalaAtual();
    if (avisosUid) {
      const conexoes = avisosPorUid.get(avisosUid);
      if (conexoes) { conexoes.delete(ws); if (!conexoes.size) avisosPorUid.delete(avisosUid); }
      // Sem canal e sem outro sinal de vida, a pessoa passa a offline pros
      // amigos; se ainda há outro canal dela aberto, nada muda.
      // Com carência: uma oscilação de rede reabre o canal em segundos, e
      // piscar "offline" pra todos os amigos a cada reconexão seria ruído.
      if (!avisosPorUid.has(avisosUid)) {
        const uidQueSaiu = avisosUid;
        setTimeout(() => {
          if (avisosPorUid.has(uidQueSaiu)) return;
          vistoPorChave.delete(uidQueSaiu);   // o "visto há pouco" não vale mais
          avisaAmigos(uidQueSaiu, { type: 'status-amigo', uid: uidQueSaiu, status: statusVisivelDe(uidQueSaiu) });
        }, AVISOS_GRACA_MS).unref();
      }
    }
  });
});

const HEARTBEAT_INTERVAL_MS = 25000;
const heartbeatTimer = setInterval(() => {
  wss.clients.forEach((ws) => {
    if (ws.isAlive === false) return ws.terminate();
    ws.isAlive = false;
    ws.ping();
  });
}, HEARTBEAT_INTERVAL_MS);
wss.on('close', () => clearInterval(heartbeatTimer));

server.listen(PORT, () => {
  console.log(`Servidor rodando na porta ${PORT}`);
});
