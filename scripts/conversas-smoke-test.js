#!/usr/bin/env node
// Teste de fumaça das conversas (v3), ponta a ponta contra um servidor de
// verdade: conversa direta, grupos, canal de avisos, chamada e toque, mensagens
// de sistema, salas por nome desligadas e convite por link.
//
// Uso:
//   WORDLE_PALAVRA=crane TOQUE_MS=1500 AVISOS_GRACA_MS=300 PORT=8099 node server.js &
//   node scripts/conversas-smoke-test.js http://localhost:8099
//
// O servidor TEM que subir com `WORDLE_PALAVRA=crane` (palavra fixa do dia) e `TOQUE_MS` pequeno (o toque de produção dura
// 30s) e, de preferência, `AVISOS_GRACA_MS`. Essas duas variáveis só existem
// pra teste.
//
// A suíte cria contas, amizades, grupos e histórico. Rode contra um servidor
// DESCARTÁVEL (cópia do server.js numa pasta temporária — o `chatdata/` mora
// ao lado do arquivo). Por isso ela se recusa a falar com host que não seja
// local, a menos que se passe `--permite-remoto`.
//
// Precisa do módulo `ws` (o mesmo do servidor).

const args = process.argv.slice(2);
const permiteRemoto = args.includes('--permite-remoto');
const base = (args.find(a => !a.startsWith('--')) || 'http://localhost:8099').replace(/\/$/, '')
  .replace(/^ws/, 'http');
const wsUrl = base.replace(/^http/, 'ws');
const host = new URL(base).hostname;
if (!permiteRemoto && !['localhost', '127.0.0.1', '[::1]'].includes(host)) {
  console.error(`Recusado: ${host} não é local. Esta suíte cria contas e grupos.`);
  process.exit(2);
}

const WebSocket = require('ws');

let falhas = 0;
let total = 0;
function confere(cond, nome, detalhe) {
  total++;
  if (cond) console.log('  OK  ' + nome);
  else { falhas++; console.log('  FALHOU  ' + nome + (detalhe ? '  -> ' + detalhe : '')); }
}
const secao = t => console.log('\n' + t);
const espera = ms => new Promise(r => setTimeout(r, ms));

async function http(metodo, caminho, corpo, cookie) {
  const r = await fetch(base + caminho, {
    method: metodo,
    headers: { 'Content-Type': 'application/json', ...(cookie ? { Cookie: cookie } : {}) },
    body: corpo ? JSON.stringify(corpo) : undefined
  });
  const texto = await r.text();
  let json = null; try { json = JSON.parse(texto); } catch { /* não é JSON */ }
  return { s: r.status, j: json, h: r.headers };
}

const sufixo = Math.random().toString(36).slice(2, 7);
async function criaConta(prefixo) {
  const usuario = `${prefixo}${sufixo}`;
  const r = await http('POST', '/api/conta/criar', { usuario, senha: 'senha-de-teste-123', nome: prefixo });
  if (r.s !== 200) throw new Error(`não consegui criar ${usuario}: ${r.s} ${JSON.stringify(r.j)}`);
  const ck = (r.h.get('set-cookie') || '').split(';')[0];
  const perfil = await http('GET', '/api/perfil/' + usuario, null, ck);
  return { usuario, ck, uid: perfil.j.uid, nome: prefixo };
}

function abreWs(cookie) {
  return new Promise((resolve, reject) => {
    const w = new WebSocket(wsUrl, { headers: cookie ? { Cookie: cookie } : {} });
    w.msgs = [];
    w.fechou = null;
    w.on('message', d => { try { w.msgs.push(JSON.parse(d)); } catch { /* ignora */ } });
    w.on('close', c => { w.fechou = c; });
    w.on('open', () => resolve(w));
    w.on('error', reject);
  });
}
const envia = (w, o) => w.send(JSON.stringify(o));
const todos = (w, tipo, f) => w.msgs.filter(m => m.type === tipo && (!f || f(m)));
const primeiro = (w, tipo, f) => todos(w, tipo, f)[0];

// 1x1 PNG mínimo (assinatura + bytes): basta para o `sniffImageMime`.
const PNG = Buffer.concat([Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]), Buffer.alloc(40, 1)]);

async function main() {
  const A = await criaConta('ana'), B = await criaConta('bob'), C = await criaConta('cai'), D = await criaConta('duda');
  const contas = [A, B, C, D];
  for (const o of [B, C]) {
    await http('POST', '/api/amigos/pedir', { usuario: o.usuario }, A.ck);
    await http('POST', '/api/amigos/responder', { uid: A.uid, aceita: true }, o.ck);
  }
  // C e D são amigos também: serve para "membro adiciona amigo dele".
  await http('POST', '/api/amigos/pedir', { usuario: D.usuario }, C.ck);
  await http('POST', '/api/amigos/responder', { uid: C.uid, aceita: true }, D.ck);

  // --- A1/A2: conversa direta e grupos ----------------------------------
  secao('A1/A2  conversas e grupos (HTTP)');
  let r = await http('GET', '/api/conversas');
  confere(r.s === 401, 'sem sessão: 401');
  r = await http('POST', '/api/conversas/dm', { uid: D.uid }, A.ck);
  confere(r.s === 403, 'não-amigo não abre conversa direta (403)');
  r = await http('POST', '/api/conversas/dm', { uid: B.uid }, A.ck);
  confere(r.s === 200 && r.j.conversa.tipo === 'dm', 'amigo abre conversa direta');
  const dm = r.j.conversa;
  r = await http('POST', '/api/conversas/dm', { uid: A.uid }, B.ck);
  confere(r.j && r.j.conversa.id === dm.id, 'conversa direta é única por par');
  r = await http('POST', '/api/grupos', { membros: [B.uid, D.uid] }, A.ck);
  confere(r.s === 403, 'grupo com não-amigo recusado (403)');
  r = await http('POST', '/api/grupos', { membros: [] }, A.ck);
  confere(r.s === 400, 'grupo sem ninguém recusado (400)');
  r = await http('POST', '/api/grupos', { membros: [B.uid, C.uid] }, A.ck);
  confere(r.s === 200 && r.j.conversa.membros.length === 3 && r.j.conversa.dono === A.uid, 'grupo criado com 3 membros, dono = quem criou');
  const g = r.j.conversa;
  confere(g.nome.includes('ana') && g.nome.includes('bob'), 'nome padrão lista os membros', g.nome);
  r = await http('POST', '/api/grupos/' + g.id, { nome: 'Os Cria' }, B.ck);
  confere(r.s === 200 && r.j.conversa.nome === 'Os Cria', 'qualquer membro renomeia');
  r = await http('POST', '/api/grupos/' + g.id, { nome: '' }, B.ck);
  confere(r.s === 400, 'nome vazio recusado');
  r = await http('POST', '/api/grupos/' + g.id, { nome: 'x'.repeat(61) }, B.ck);
  confere(r.s === 400, 'nome com 61 caracteres recusado');
  r = await http('POST', '/api/grupos/' + g.id, { nome: 'Invasor' }, D.ck);
  confere(r.s === 404, 'não-membro não mexe no grupo (404)');
  r = await http('POST', `/api/grupos/${g.id}/icone`, { gif: 'https://exemplo.com/x.gif' }, A.ck);
  confere(r.s === 400, 'GIF fora do provedor recusado');
  r = await http('POST', `/api/grupos/${g.id}/icone`, { gif: 'https://media.giphy.com/m/x.gif' }, A.ck);
  confere(r.s === 200 && /giphy/.test(r.j.conversa.icone), 'ícone por GIF');
  r = await http('POST', `/api/grupos/${g.id}/icone`, { data: PNG.toString('base64') }, A.ck);
  confere(r.s === 200 && /\/icone\?v=/.test(r.j.conversa.icone), 'ícone por imagem');
  const urlIcone = r.j.conversa.icone;
  let rr = await fetch(base + urlIcone, { headers: { Cookie: C.ck } });
  confere(rr.status === 200 && rr.headers.get('content-type') === 'image/png', 'ícone servido a membro');
  rr = await fetch(base + urlIcone, { headers: { Cookie: D.ck } });
  confere(rr.status === 404, 'ícone negado a não-membro');
  r = await http('POST', `/api/grupos/${g.id}/icone`, { data: Buffer.from('isto não é imagem').toString('base64') }, A.ck);
  confere(r.s === 400, 'bytes que não são imagem recusados');
  r = await http('DELETE', `/api/grupos/${g.id}/icone`, null, A.ck);
  confere(r.s === 200 && r.j.conversa.icone === null, 'ícone removido');
  r = await http('POST', `/api/grupos/${g.id}/membros`, { uids: [D.uid] }, A.ck);
  confere(r.s === 403, 'só adiciona amigo de quem adiciona (403)');
  r = await http('POST', `/api/grupos/${g.id}/membros`, { uids: [D.uid] }, C.ck);
  confere(r.s === 200 && r.j.conversa.membros.length === 4, 'membro adiciona amigo dele');
  r = await http('DELETE', `/api/grupos/${g.id}/membros/${D.uid}`, null, B.ck);
  confere(r.s === 403, 'não-dono não remove (403)');

  // --- A3: entrar pelo WebSocket -----------------------------------------
  secao('A3  entrar na conversa (WebSocket)');
  let w = await abreWs(D.ck);
  // D ainda está no grupo (foi adicionado acima): testa com um não-membro de verdade.
  const E = await criaConta('eva');
  let we = await abreWs(E.ck);
  envia(we, { type: 'join', conversa: g.id });
  await espera(200);
  confere(!!primeiro(we, 'join-denied', m => m.reason === 'nao-membro'), 'não-membro recusado (nao-membro)');
  we.close();
  const wa0 = await abreWs(A.ck);
  envia(wa0, { type: 'join', room: 'sala-antiga' });
  await espera(200);
  confere(!!primeiro(wa0, 'join-denied', m => m.reason === 'sem-conversa'), 'join por nome de sala recusado (sem-conversa)');
  wa0.close();
  const wn = await abreWs();
  envia(wn, { type: 'join', conversa: g.id });
  await espera(200);
  confere(!!primeiro(wn, 'join-denied', m => m.reason === 'nao-membro'), 'sem sessão recusado');
  wn.close(); w.close();

  // --- A4: canal de avisos ------------------------------------------------
  secao('A4  canal de avisos');
  const nv = await abreWs();
  envia(nv, { type: 'avisos' });
  await espera(150);
  confere(!!primeiro(nv, 'avisos-negado'), 'sem sessão: avisos-negado');
  nv.close();
  const av = {};
  for (const c of [A, B, C, D]) { av[c.nome] = await abreWs(c.ck); envia(av[c.nome], { type: 'avisos' }); }
  await espera(300);
  confere(!!primeiro(av.ana, 'avisos-ok'), 'avisos-ok');
  confere(!!primeiro(av.ana, 'status-amigo', m => m.uid === B.uid && m.status !== 'offline'), 'amigo online aparece');
  const g2 = (await http('POST', '/api/grupos', { membros: [B.uid] }, A.ck)).j.conversa;
  await espera(200);
  confere(!!primeiro(av.ana, 'conversa-nova', m => m.conversa.id === g2.id) && !!primeiro(av.bob, 'conversa-nova', m => m.conversa.id === g2.id), 'conversa-nova para os membros');
  confere(!primeiro(av.cai, 'conversa-nova', m => m.conversa.id === g2.id), 'quem é de fora não recebe');
  await http('POST', '/api/grupos/' + g2.id, { nome: 'Novo Nome' }, B.ck);
  await espera(200);
  const mud = todos(av.ana, 'conversa-mudou').pop();
  confere(mud && mud.conversa.nome === 'Novo Nome' && mud.conversa.membros.length === 2, 'conversa-mudou com nome e membros');
  await http('POST', '/api/grupos/' + g2.id + '/membros', { uids: [C.uid] }, A.ck);
  await espera(200);
  confere(!!primeiro(av.cai, 'conversa-mudou', m => m.conversa.id === g2.id), 'quem entrou no grupo é avisado');
  // chamada + mensagem
  const ra = await abreWs(A.ck), rb = await abreWs(B.ck);
  envia(ra, { type: 'join', conversa: g2.id });
  await espera(250);
  const ch = todos(av.bob, 'chamada', m => m.conversa === g2.id);
  confere(ch.length === 1 && ch[0].ativa && ch[0].dentro.join() === A.uid, 'chamada ativa avisada');
  envia(rb, { type: 'join', conversa: g2.id, espectador: true });
  envia(rb, { type: 'mode-state', espectador: true });
  await espera(250);
  confere(todos(av.cai, 'chamada', m => m.conversa === g2.id).length === 1, 'espectador não muda a chamada');
  envia(rb, { type: 'mode-state', espectador: false });
  await espera(250);
  confere(todos(av.cai, 'chamada', m => m.conversa === g2.id).pop().dentro.length === 2, 'sair do modo espectador entra na chamada');
  envia(ra, { type: 'chat', text: 'oi turma' });
  await espera(250);
  const aviso = primeiro(av.cai, 'mensagem', m => m.previa === 'oi turma');
  confere(aviso && aviso.conversa === g2.id && aviso.autor === 'ana', 'mensagem chega a quem está fora da sala');
  await http('POST', '/api/status', { status: 'ocupado' }, A.ck);
  await espera(200);
  confere(!!primeiro(av.bob, 'status-amigo', m => m.uid === A.uid && m.status === 'ocupado'), 'status do amigo chega');
  await http('POST', '/api/atividade', { atividade: { jogo: { nome: 'Jogo de Teste' } } }, A.ck);
  await espera(200);
  confere(!!todos(av.bob, 'atividade-amigo').pop(), 'atividade do amigo chega');
  ra.close(); rb.close();
  await espera(300);
  const fim = todos(av.cai, 'chamada', m => m.conversa === g2.id).pop();
  confere(fim && !fim.ativa && fim.dentro.length === 0, 'chamada termina quando todos saem');

  // --- A5: chamada e toque -------------------------------------------------
  secao('A5  chamada e toque');
  const g3 = (await http('POST', '/api/grupos', { membros: [B.uid, C.uid] }, A.ck)).j.conversa;
  await espera(150);
  envia(av.duda, { type: 'ligar', conversa: g3.id });
  await espera(150);
  confere(!!primeiro(av.duda, 'toque-erro', m => m.motivo === 'nao-membro'), 'não-membro não liga');
  envia(av.ana, { type: 'ligar', conversa: g3.id });
  await espera(250);
  const tb = primeiro(av.bob, 'toque', m => m.conversa === g3.id);
  const tc = primeiro(av.cai, 'toque', m => m.conversa === g3.id);
  confere(!!tb && !!tc && tb.quem.uid === A.uid, 'os de fora recebem o toque de quem ligou');
  confere(!primeiro(av.ana, 'toque', m => m.conversa === g3.id), 'quem liga não recebe toque');
  const ch0 = primeiro(av.ana, 'chamada', m => m.conversa === g3.id && Array.isArray(m.chamando) && m.chamando.length);
  confere(ch0 && ch0.chamando.includes(B.uid) && ch0.chamando.includes(C.uid), 'evento chamada lista quem está tocando (chamando)');
  const duracao = tb ? tb.duracaoMs : 30000;
  confere(duracao <= 10000, 'servidor com TOQUE_MS pequeno', `duracaoMs=${duracao}; suba o servidor com TOQUE_MS=1500`);
  const xa = await abreWs(A.ck), xb = await abreWs(B.ck);
  envia(xa, { type: 'join', conversa: g3.id });
  await espera(200);
  envia(xb, { type: 'join', conversa: g3.id });
  await espera(250);
  confere(!!primeiro(av.bob, 'toque-fim', m => m.conversa === g3.id && m.motivo === 'atendeu'), 'entrar na chamada atende o toque');
  envia(av.cai, { type: 'recusar', conversa: g3.id });
  await espera(200);
  confere(!!primeiro(av.cai, 'toque-fim', m => m.conversa === g3.id && m.motivo === 'recusou'), 'recusar encerra o próprio toque');
  confere(!primeiro(av.ana, 'toque-fim', m => m.conversa === g3.id), 'recusar não afeta quem está na chamada');
  envia(av.cai, { type: 'tocar-de-novo', conversa: g3.id, uid: B.uid });
  await espera(150);
  confere(!!primeiro(av.cai, 'toque-erro', m => m.motivo === 'nao-esta-na-chamada'), 'só quem está dentro toca de novo');
  envia(xa, { type: 'tocar-de-novo', conversa: g3.id, uid: B.uid });
  await espera(150);
  confere(!!primeiro(xa, 'toque-erro', m => m.motivo === 'alvo-na-chamada'), 'não toca para quem já está dentro');
  envia(xa, { type: 'tocar-de-novo', conversa: g3.id, uid: C.uid });
  await espera(200);
  confere(todos(av.cai, 'toque', m => m.conversa === g3.id).length === 2, 'tocar de novo chega ao alvo');
  envia(xa, { type: 'tocar-de-novo', conversa: g3.id, uid: C.uid });
  await espera(150);
  confere(!!primeiro(xa, 'toque-erro', m => m.motivo === 'limite'), 'limite de uma vez a cada 10s por alvo');
  await espera(duracao + 400);
  confere(!!primeiro(av.cai, 'toque-fim', m => m.conversa === g3.id && m.motivo === 'tempo'), 'sem resposta: toque acaba por tempo');
  xa.close(); xb.close();
  await espera(300);
  // cancelado: liga, entra, sai antes do tempo
  const dm2 = (await http('POST', '/api/conversas/dm', { uid: C.uid }, A.ck)).j.conversa;
  const ya = await abreWs(A.ck);
  envia(ya, { type: 'ligar', conversa: dm2.id });
  envia(ya, { type: 'join', conversa: dm2.id });
  await espera(200);
  ya.close();
  await espera(300);
  confere(!!primeiro(av.cai, 'toque-fim', m => m.conversa === dm2.id && m.motivo === 'cancelado'), 'chamada sem ninguém dentro cancela o toque');
  // snapshot: canal aberto no meio do toque
  const za = await abreWs(A.ck);
  envia(za, { type: 'ligar', conversa: dm2.id });
  await espera(150);
  const av2 = await abreWs(C.ck); envia(av2, { type: 'avisos' });
  await espera(250);
  confere(!!primeiro(av2, 'toque', m => m.conversa === dm2.id), 'canal novo recebe o toque em andamento');
  av2.close(); za.close();

  // --- A6: mensagens de sistema --------------------------------------------
  secao('A6  mensagens de sistema');
  const wh = await abreWs(B.ck);
  envia(wh, { type: 'join', conversa: g3.id });
  await espera(300);
  const hist = primeiro(wh, 'chat-history');
  const sist = hist ? hist.messages.filter(m => m.type === 'sistema') : [];
  confere(sist.some(m => m.sys === 'grupo' && /criou o grupo/.test(m.texto)), 'criação do grupo no histórico');
  confere(!sist.some(m => m.sys === 'perdida'), 'sem linha "não atendeu" (quem perdeu vê só "Você perdeu uma chamada")');
  confere(sist.some(m => m.sys === 'chamada' && /durou/.test(m.texto)), 'duração da chamada no histórico');
  const alvoSys = sist[0];
  if (alvoSys) {
    envia(wh, { type: 'chat-delete', id: alvoSys.id });
    envia(wh, { type: 'chat-edit', id: alvoSys.id, text: 'adulterado' });
    envia(wh, { type: 'chat-react', id: alvoSys.id, emoji: '👍' });
    envia(wh, { type: 'chat-pin', id: alvoSys.id });
    await espera(300);
    const wh2 = await abreWs(B.ck);
    envia(wh2, { type: 'join', conversa: g3.id });
    await espera(300);
    const depois = (primeiro(wh2, 'chat-history') || { messages: [] }).messages.filter(m => m.type === 'sistema');
    const igual = depois.find(m => m.id === alvoSys.id);
    confere(depois.length === sist.length && igual && igual.texto === alvoSys.texto, 'mensagem de sistema não apaga nem edita');
    confere(!todos(wh, 'chat-pin').length && !todos(wh, 'chat-react').length, 'não fixa nem reage');
    wh2.close();
  }
  wh.close();

  // --- remoção do grupo: kicked --------------------------------------------
  secao('A2/A3  remover e sair (efeito na conexão)');
  const kb = await abreWs(B.ck);
  envia(kb, { type: 'join', conversa: g.id });
  await espera(250);
  r = await http('DELETE', `/api/grupos/${g.id}/membros/${B.uid}`, null, A.ck);
  await espera(300);
  confere(r.s === 200, 'dono remove membro');
  confere(!!primeiro(kb, 'kicked', m => m.motivo === 'removido') && kb.fechou === 4003, 'removido recebe kicked e a conexão fecha');
  const kb2 = await abreWs(B.ck);
  envia(kb2, { type: 'join', conversa: g.id });
  await espera(200);
  confere(!!primeiro(kb2, 'join-denied', m => m.reason === 'nao-membro'), 'removido não reentra');
  kb2.close();
  r = await http('POST', `/api/grupos/${g.id}/sair`, null, A.ck);
  confere(r.s === 200, 'dono sai');
  r = await http('GET', '/api/conversas', null, C.ck);
  const gg = r.j.conversas.find(x => x.id === g.id);
  confere(gg && gg.dono === C.uid, 'posse passa ao membro mais antigo', gg ? gg.dono : 'grupo sumiu');
  r = await http('GET', '/api/conversas', null, A.ck);
  confere(!r.j.conversas.find(x => x.id === g.id), 'quem saiu não vê mais o grupo');

  // --- A7: salas por nome desligadas ----------------------------------------
  secao('A7  salas por nome desligadas');
  r = await http('GET', '/api/rooms', null, A.ck);
  confere(r.s === 410, '/api/rooms responde 410');
  const sa = await abreWs(A.ck), sb = await abreWs(C.ck);
  envia(sa, { type: 'join', conversa: g3.id });
  envia(sb, { type: 'join', conversa: g3.id });
  await espera(250);
  const idc = todos(sa, 'peer-joined').pop();
  const info = primeiro(sb, 'room-info');
  confere(info && info.locked === undefined && info.conversa === g3.id, 'room-info sem senha de sala');
  if (idc) {
    envia(sa, { type: 'room-kick', id: idc.id });
    envia(sa, { type: 'room-mute', id: idc.id, muted: true });
    envia(sa, { type: 'room-set-password', password: 'x' });
    await espera(300);
  }
  confere(sb.fechou === null && !todos(sb, 'kicked').length && !todos(sb, 'force-mute').length && !todos(sb, 'room-password').length, 'room-kick, room-mute e room-set-password ignorados');
  sa.close(); sb.close();

  // --- A8: convite por link ---------------------------------------------------
  secao('A8  convite por link');
  const g4 = (await http('POST', '/api/grupos', { membros: [B.uid] }, A.ck)).j.conversa;
  r = await http('POST', `/api/grupos/${g4.id}/convite`, null, B.ck);
  confere(r.s === 403, 'só o dono gera o convite');
  r = await http('POST', `/api/grupos/${g4.id}/convite`, null, A.ck);
  const t1 = r.j && r.j.convite;
  confere(r.s === 200 && /^[0-9a-f]{48}$/.test(t1 || ''), 'dono gera o token');
  r = await http('GET', '/api/conversas', null, B.ck);
  const gb = r.j.conversas.find(x => x.id === g4.id);
  confere(gb && gb.convite === undefined && gb.temConvite === true, 'membro não vê o token');
  r = await http('GET', '/api/convites/' + t1, null, D.ck);
  confere(r.s === 200 && r.j.membros === 2 && r.j.jaMembro === false, 'prévia do convite');
  r = await http('POST', '/api/convites/' + t1, null, D.ck);
  confere(r.s === 200 && r.j.conversa.membros.length === 3, 'abrir o link logado entra no grupo');
  r = await http('POST', '/api/convites/' + t1, null, D.ck);
  confere(r.s === 200 && r.j.conversa.membros.length === 3, 'abrir de novo não duplica');
  r = await http('POST', `/api/grupos/${g4.id}/convite`, null, A.ck);
  const t2 = r.j.convite;
  r = await http('POST', '/api/convites/' + t1, null, E.ck);
  confere(t2 !== t1 && r.s === 404, 'token novo invalida o antigo');
  await http('DELETE', `/api/grupos/${g4.id}/convite`, null, A.ck);
  r = await http('POST', '/api/convites/' + t2, null, E.ck);
  confere(r.s === 404, 'convite revogado não entra');
  r = await http('POST', '/api/convites/' + t2);
  confere(r.s === 401, 'convite sem sessão: 401');


  // --- P1: encaminhar e favoritar ------------------------------------------------
  secao('P1  encaminhar e favoritar mensagens');
  const wf = await abreWs(A.ck);
  envia(wf, { type: 'join', conversa: dm.id });
  await espera(300);
  envia(wf, { type: 'chat', text: 'mensagem para encaminhar' });
  await espera(300);
  const original = primeiro(wf, 'chat', m => m.text === 'mensagem para encaminhar');
  confere(!!original, 'mensagem original chegou');
  r = await http('POST', `/api/conversas/${g4.id}/encaminhar`, { origem: dm.id, mensagem: original.id, extra: 'olha isso' }, A.ck);
  confere(r.s === 200, 'encaminhar para outra conversa (200)');
  r = await http('POST', `/api/conversas/${g4.id}/encaminhar`, { origem: dm.id, mensagem: 'nao-existe' }, A.ck);
  confere(r.s === 404, 'encaminhar mensagem inexistente (404)');
  r = await http('POST', `/api/conversas/${g4.id}/encaminhar`, { origem: dm.id, mensagem: original.id }, C.ck);
  confere(r.s === 404, 'não-membro não encaminha (404)');
  const wg = await abreWs(B.ck);
  envia(wg, { type: 'join', conversa: g4.id });
  await espera(400);
  const histFwd = primeiro(wg, 'chat-history');
  const enc = histFwd && histFwd.messages.find(m => m.fwd);
  confere(enc && enc.text === 'mensagem para encaminhar' && enc.fwd.nome === 'ana' && enc.name === 'ana', 'histórico traz a encaminhada com o autor original');
  confere(histFwd && histFwd.messages.some(m => m.text === 'olha isso' && !m.fwd), 'mensagem opcional vem como texto comum');
  r = await http('POST', '/api/favoritos', { conversa: dm.id, mensagem: original.id }, A.ck);
  confere(r.s === 200, 'favoritar (200)');
  await http('POST', '/api/favoritos', { conversa: dm.id, mensagem: original.id }, A.ck);
  r = await http('GET', '/api/favoritos', null, A.ck);
  confere(r.s === 200 && r.j.favoritos.length === 1 && r.j.favoritos[0].texto === 'mensagem para encaminhar', 'favorito listado, sem duplicar');
  r = await http('GET', '/api/favoritos', null, B.ck);
  confere(r.s === 200 && r.j.favoritos.length === 0, 'favorito é por pessoa');
  r = await http('POST', '/api/favoritos', { conversa: dm.id, mensagem: original.id }, C.ck);
  confere(r.s === 404, 'não-membro não favorita (404)');
  r = await http('DELETE', `/api/favoritos/${dm.id}/${original.id}`, null, A.ck);
  r = await http('GET', '/api/favoritos', null, A.ck);
  confere(r.j.favoritos.length === 0, 'desfavoritar remove');
  wf.close(); wg.close();

  // --- P3: status personalizado --------------------------------------------------
  secao('P3  status personalizado');
  const wsc = await abreWs(B.ck);
  envia(wsc, { type: 'avisos' });
  await espera(300);
  r = await http('POST', '/api/status-custom', { texto: 'codando', emoji: '💻', minutos: 60 }, A.ck);
  confere(r.s === 200 && r.j.statusCustom && r.j.statusCustom.texto === 'codando' && r.j.statusCustom.ate > Date.now(), 'define status com prazo');
  await espera(300);
  confere(!!primeiro(wsc, 'status-custom-amigo', m => m.uid === A.uid && m.statusCustom && m.statusCustom.emoji === '💻'), 'amigo recebe o aviso');
  r = await http('GET', '/api/perfil/' + A.usuario, null, B.ck);
  confere(r.j.statusCustom && r.j.statusCustom.texto === 'codando', 'cartão do perfil traz o status');
  r = await http('POST', '/api/status-custom', { texto: 'abc', emoji: 'xx', minutos: 7 }, A.ck);
  confere(r.s === 200 && r.j.statusCustom.emoji === '' && r.j.statusCustom.ate === null, 'emoji com letra é descartado; prazo desconhecido = sem prazo');
  r = await http('POST', '/api/status-custom', { texto: '', emoji: '' }, A.ck);
  r = await http('GET', '/api/perfil/' + A.usuario, null, B.ck);
  confere(!r.j.statusCustom, 'vazio limpa o status');
  r = await http('POST', '/api/status-custom', { texto: 'x' });
  confere(r.s === 401, 'sem sessão: 401');
  wsc.close();

  // --- P4: Wordle ----------------------------------------------------------------
  secao('P4  Wordle (palavra do dia no servidor)');
  // O Wordle é só de grupos: um grupo novo de A e B (C fica de fora).
  r = await http('POST', '/api/grupos', { membros: [B.uid] }, A.ck);
  const gw4 = r.j.conversa;
  r = await http('POST', `/api/conversas/${dm.id}/wordle`, null, A.ck);
  confere(r.s === 400, 'Wordle em conversa direta recusado (400)');
  r = await http('POST', '/api/wordle/tentar', { palavra: 'crate', dia: '99999_r7' }, A.ck);
  confere(r.s === 400 && r.j.codigo === 'rodada', 'rodada inventada recusada (400)');
  r = await http('GET', '/api/wordle', null, A.ck);
  confere(r.s === 200 && r.j.max === 6 && r.j.tentativas.length === 0 && r.j.resposta === undefined, 'estado inicial sem resposta');
  r = await http('POST', '/api/wordle/tentar', { palavra: 'abc' }, A.ck);
  confere(r.s === 400 && r.j.codigo === 'curta', 'palavra curta recusada');
  r = await http('POST', '/api/wordle/tentar', { palavra: 'qqqqq' }, A.ck);
  confere(r.s === 400 && r.j.codigo === 'invalida', 'palavra fora da lista recusada');
  // A resposta do teste é "crane" (WORDLE_PALAVRA no servidor).
  r = await http('POST', '/api/wordle/tentar', { palavra: 'crate' }, A.ck);
  confere(r.s === 200 && r.j.tentativas[0].cores === 'gggxg' && !r.j.fim, 'cores: crate contra crane = gggxg');
  r = await http('POST', '/api/wordle/tentar', { palavra: 'eager' }, A.ck);
  confere(r.s === 200 && /^[gyx]{5}$/.test(r.j.tentativas[1].cores), 'segunda tentativa devolve cores');
  r = await http('GET', '/api/wordle', null, B.ck);
  confere(r.j.tentativas.length === 0, 'estado é por pessoa');
  const gw = await http('POST', `/api/conversas/${gw4.id}/wordle`, null, A.ck);
  confere(gw.s === 200 && gw.j.existia === false, 'cria a mensagem do Wordle na conversa');
  r = await http('POST', `/api/conversas/${gw4.id}/wordle`, null, B.ck);
  confere(r.s === 200 && r.j.existia === true && r.j.id === gw.j.id, 'repetir /wordle reaproveita a mensagem do dia');
  r = await http('GET', `/api/conversas/${gw4.id}/wordle`, null, B.ck);
  confere(r.s === 200 && r.j.hoje.length === 1 && r.j.hoje[0].cores.length === 2 && JSON.stringify(r.j).indexOf('crate') < 0, 'grupo mostra só cores, nunca as letras');
  r = await http('POST', `/api/conversas/${gw4.id}/wordle`, null, C.ck);
  confere(r.s === 404, 'não-membro não mexe no Wordle da conversa (404)');
  const wgB = await abreWs(B.ck);
  envia(wgB, { type: 'join', conversa: gw4.id, espectador: true });
  await espera(300);
  r = await http('POST', '/api/wordle/tentar', { palavra: 'crane' }, A.ck);
  await espera(300);
  confere(primeiro(wgB, 'wordle-grupo', m => m.dia === r.j.dia) && !JSON.stringify(todos(wgB, 'wordle-grupo')).includes('crane'),
    'quem está no grupo é avisado da jogada (sem a palavra)');
  wgB.close();
  confere(r.s === 200 && r.j.fim === 'ganhou' && r.j.resposta === 'crane', 'acertar encerra e revela a resposta');
  r = await http('POST', '/api/wordle/tentar', { palavra: 'crate' }, A.ck);
  confere(r.s === 409 && r.j.codigo === 'fim', 'depois de terminar não aceita mais tentativa');
  r = await http('GET', `/api/conversas/${gw4.id}/wordle`, null, B.ck);
  confere(r.j.streak >= 1, 'sequência do grupo conta o dia de hoje');

  // --- P7: soundboard (volume do emissor viaja no evento) -----------------------
  secao('P7  soundboard');
  const ws1 = await abreWs(A.ck), ws2 = await abreWs(B.ck);
  envia(ws1, { type: 'join', conversa: dm.id });
  envia(ws2, { type: 'join', conversa: dm.id });
  await espera(400);
  envia(ws1, { type: 'som', id: 'buzina', vol: 25 });
  await espera(300);
  confere(primeiro(ws2, 'som', m => m.id === 'buzina') && primeiro(ws2, 'som').vol === 25, 'o outro ouve o som com o volume de quem soltou');
  await espera(2100);
  envia(ws1, { type: 'som', id: 'pop', vol: 999 });
  await espera(300);
  confere(primeiro(ws2, 'som', m => m.id === 'pop').vol === 100, 'volume acima de 100 é limitado a 100');
  await espera(2100);
  envia(ws1, { type: 'som', id: 'laser' });
  await espera(300);
  confere(primeiro(ws2, 'som', m => m.id === 'laser').vol === 40, 'sem volume: usa o padrão (40)');
  ws1.close(); ws2.close();

  // --- P9: perfil com jogos, insígnias, amigos em comum e mural -----------------
  secao('P9  perfil: jogos, insígnias, mural');
  // Amizade A-D só aqui no fim: C e D passam a ter A em comum.
  await http('POST', '/api/amigos/pedir', { usuario: D.usuario }, A.ck);
  await http('POST', '/api/amigos/responder', { uid: A.uid, aceita: true }, D.ck);
  r = await http('GET', '/api/perfil/' + D.usuario, null, C.ck);
  confere(r.s === 200 && r.j.mutuos.length === 1 && r.j.mutuos[0].uid === A.uid, 'amigos em comum aparecem (Ana é amiga de Cai e de Duda)');
  await http('POST', '/api/amigos/remover', { uid: D.uid }, A.ck);
  r = await http('GET', '/api/perfil/' + A.usuario, null, B.ck);
  confere(r.s === 200 && r.j.amigoDeMim === true && r.j.jogos.some(j => j.nome === 'Jogo de Teste' && j.dias === 1), 'amigo vê a coleção de jogos (jogo detectado antes entrou)');
  confere(Array.isArray(r.j.insignias) && Array.isArray(r.j.mutuos), 'perfil traz insígnias e amigos em comum');
  r = await http('GET', '/api/perfil/' + A.usuario, null, D.ck);
  confere(r.s === 200 && r.j.amigoDeMim === false && r.j.jogos.length === 0 && r.j.mutuos.length === 0, 'quem não é amigo não vê jogos nem amigos em comum');
  r = await http('GET', '/api/perfil/' + A.usuario, null, A.ck);
  confere(r.j.amigoDeMim === true && r.j.mutuos.length === 0, 'no próprio perfil não há "amigos em comum"');
  r = await http('POST', '/api/mural/' + A.usuario, { texto: '  oi   do   B  ' }, B.ck);
  confere(r.s === 200 && r.j.recados[0].texto === 'oi do B' && r.j.recados[0].de === B.uid, 'amigo deixa recado e o texto é normalizado');
  const idRecado = r.j.recados[0].id;
  r = await http('POST', '/api/mural/' + A.usuario, { texto: 'de novo' }, B.ck);
  confere(r.s === 429, 'dois recados seguidos do mesmo autor: 429');
  r = await http('POST', '/api/mural/' + A.usuario, { texto: 'intruso' }, D.ck);
  confere(r.s === 403, 'quem não é amigo não escreve no mural (403)');
  r = await http('GET', '/api/mural/' + A.usuario, null, D.ck);
  confere(r.s === 403, 'quem não é amigo não lê o mural (403)');
  r = await http('GET', '/api/mural/' + A.usuario, null, A.ck);
  confere(r.s === 200 && r.j.recados.length === 1, 'o dono lê o próprio mural');
  r = await http('POST', '/api/mural/' + A.usuario, { texto: '   ' }, A.ck);
  confere(r.s === 400, 'recado vazio é recusado');
  r = await http('DELETE', `/api/mural/${A.usuario}/${idRecado}`, null, D.ck);
  confere(r.s === 403, 'estranho não apaga recado');
  r = await http('DELETE', `/api/mural/${A.usuario}/${idRecado}`, null, A.ck);
  confere(r.s === 200 && r.j.recados.length === 0, 'o dono do mural apaga recado');
  r = await http('GET', '/api/mural/' + A.usuario);
  confere(r.s === 401, 'mural sem sessão: 401');

  // --- limpeza -----------------------------------------------------------------
  for (const c of Object.values(av)) c.close();
  for (const c of [...contas, E]) await http('POST', '/api/conta/apagar', { senha: 'senha-de-teste-123' }, c.ck);

  console.log(`\n${total - falhas}/${total} checagens OK`);
  process.exit(falhas ? 1 : 0);
}

main().catch(e => { console.error('Erro no teste:', e); process.exit(1); });
