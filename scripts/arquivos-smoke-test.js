#!/usr/bin/env node
// Teste de fumaça dos arquivos do chat (até 200MB), dos limites de avatar e
// banner (20MB), de apagar grupo e das listas de jogos do perfil.
//
// Uso (servidor DESCARTÁVEL, como a outra suíte):
//   WORDLE_PALAVRA=crane PORT=8099 node server.js &
//   NODE_PATH=<pasta>/node_modules node scripts/arquivos-smoke-test.js http://localhost:8099
const args = process.argv.slice(2);
const base = (args.find(a => !a.startsWith('--')) || 'http://localhost:8099').replace(/\/$/, '');
const wsUrl = base.replace(/^http/, 'ws');
const host = new URL(base).hostname;
if (!args.includes('--permite-remoto') && !['localhost', '127.0.0.1', '[::1]'].includes(host)) {
  console.error(`Recusado: ${host} não é local.`); process.exit(2);
}
const WebSocket = require('ws');
const crypto = require('crypto');

let falhas = 0, total = 0;
function confere(c, nome, d) { total++; if (c) console.log('  OK  ' + nome); else { falhas++; console.log('  FALHOU  ' + nome + (d ? '  -> ' + d : '')); } }
const secao = t => console.log('\n' + t);
const espera = ms => new Promise(r => setTimeout(r, ms));

async function http(metodo, caminho, corpo, cookie, bruto, extra) {
  const r = await fetch(base + caminho, {
    method: metodo,
    headers: { ...(bruto ? {} : { 'Content-Type': 'application/json' }), ...(cookie ? { Cookie: cookie } : {}), ...(extra || {}) },
    body: bruto || (corpo ? JSON.stringify(corpo) : undefined)
  });
  const t = await r.text(); let j = null; try { j = JSON.parse(t); } catch { /* não é JSON */ }
  return { s: r.status, j, h: r.headers, t };
}
const sufixo = Math.random().toString(36).slice(2, 7);
async function criaConta(p) {
  const usuario = `${p}${sufixo}`;
  const r = await http('POST', '/api/conta/criar', { usuario, senha: 'senha-de-teste-123', nome: p });
  if (r.s !== 200) throw new Error('conta: ' + r.s);
  const ck = (r.h.get('set-cookie') || '').split(';')[0];
  const perfil = await http('GET', '/api/perfil/' + usuario, null, ck);
  return { usuario, ck, uid: perfil.j.uid };
}
function abreWs(ck) {
  return new Promise((ok, no) => {
    const w = new WebSocket(wsUrl, { headers: { Cookie: ck } });
    w.msgs = []; w.on('message', d => { try { w.msgs.push(JSON.parse(d)); } catch { /* ignora */ } });
    w.on('open', () => ok(w)); w.on('error', no);
  });
}
const envia = (w, o) => w.send(JSON.stringify(o));
const gif = tam => { const b = Buffer.alloc(tam); b.write('GIF89a'); return b; };

(async () => {
  const A = await criaConta('arqa'), B = await criaConta('arqb'), C = await criaConta('arqc');
  await http('POST', '/api/amigos/pedir', { usuario: B.usuario }, A.ck);
  await http('POST', '/api/amigos/responder', { uid: A.uid, aceita: true }, B.ck);

  secao('limites de avatar e banner (20MB)');
  let r = await http('POST', '/api/conta/avatar', { data: gif(5 * 1024 * 1024).toString('base64') }, A.ck);
  confere(r.s === 200, 'avatar GIF de 5MB aceito (antes o teto era 2MB)', r.s + ' ' + r.t.slice(0, 80));
  r = await http('POST', '/api/conta/banner', { data: gif(15 * 1024 * 1024).toString('base64') }, A.ck);
  confere(r.s === 200, 'banner GIF de 15MB aceito', r.s + ' ' + r.t.slice(0, 80));
  r = await http('POST', '/api/conta/banner', { data: gif(21 * 1024 * 1024).toString('base64') }, A.ck);
  confere(r.s === 400, 'banner de 21MB recusado');

  secao('grupo e arquivo anexado');
  r = await http('POST', '/api/grupos', { membros: [B.uid] }, A.ck);
  const g = r.j.conversa;
  confere(r.s === 200 && g.id, 'grupo criado');
  const conteudo = crypto.randomBytes(6 * 1024 * 1024);
  r = await http('POST', `/api/arquivo?conversa=${encodeURIComponent(g.id)}&nome=${encodeURIComponent('relatório final.bin')}`, null, A.ck, conteudo, { 'Content-Type': 'application/octet-stream' });
  confere(r.s === 200 && r.j.arquivoId && r.j.tam === conteudo.length, 'upload de 6MB em streaming', r.s + ' ' + r.t.slice(0, 100));
  const arquivoId = r.j.arquivoId;
  r = await http('POST', `/api/arquivo?conversa=${encodeURIComponent(g.id)}&nome=x`, null, C.ck, Buffer.from('oi'));
  confere(r.s === 404, 'quem não é do grupo não envia arquivo');
  const status413 = await new Promise((ok) => {
    const rq = require('http').request(base + `/api/arquivo?conversa=${encodeURIComponent(g.id)}&nome=x`,
      { method: 'POST', headers: { Cookie: A.ck, 'Content-Length': String(201 * 1024 * 1024) } }, resp => { ok(resp.statusCode); resp.resume(); rq.destroy(); });
    rq.on('error', () => ok(0)); rq.write('x');
  });
  confere(status413 === 413, 'declarar mais de 200MB é recusado (413)', String(status413));

  const wa = await abreWs(A.ck), wb = await abreWs(B.ck), wc = await abreWs(C.ck);
  envia(wa, { type: 'join', conversa: g.id }); envia(wb, { type: 'join', conversa: g.id });
  await espera(400);
  envia(wb, { type: 'chat', arquivoId });
  await espera(400);
  confere(!wa.msgs.some(m => m.type === 'chat' && m.arquivo), 'arquivo de outra pessoa não vira mensagem (id só vale pra quem enviou)');
  envia(wa, { type: 'chat', text: 'segue o arquivo', arquivoId });
  await espera(500);
  const m = wb.msgs.find(x => x.type === 'chat' && x.arquivo);
  confere(m && m.arquivo.nome === 'relatório final.bin' && m.arquivo.tam === conteudo.length, 'mensagem chega com o arquivo anexado');

  r = await fetch(`${base}/api/arquivo/${encodeURIComponent(g.id)}/${arquivoId}`, { headers: { Cookie: B.ck } });
  const baixado = Buffer.from(await r.arrayBuffer());
  confere(r.status === 200 && baixado.equals(conteudo), 'membro baixa o arquivo idêntico');
  confere((r.headers.get('content-disposition') || '').includes('UTF-8'), 'nome vai no Content-Disposition');
  r = await fetch(`${base}/api/arquivo/${encodeURIComponent(g.id)}/${arquivoId}`, { headers: { Cookie: C.ck } });
  confere(r.status === 404, 'fora do grupo: 404 no download');

  envia(wb, { type: 'join', conversa: g.id });
  await espera(400);
  const hist = wb.msgs.filter(x => x.type === 'chat-history').flatMap(x => x.messages || []);
  confere(hist.some(x => x.arquivo && x.arquivo.id === arquivoId), 'histórico traz o anexo');

  secao('apagar grupo');
  r = await http('DELETE', '/api/grupos/' + encodeURIComponent(g.id), null, B.ck);
  confere(r.s === 403, 'membro que não é dono não apaga');
  r = await http('DELETE', '/api/grupos/' + encodeURIComponent(g.id), null, A.ck);
  confere(r.s === 200, 'dono apaga o grupo');
  r = await http('GET', '/api/conversas', null, B.ck);
  confere(!(r.j.conversas || []).some(c => c.id === g.id), 'grupo some da lista dos membros');
  r = await fetch(`${base}/api/arquivo/${encodeURIComponent(g.id)}/${arquivoId}`, { headers: { Cookie: A.ck } });
  confere(r.status === 404, 'arquivo some junto com o grupo');

  secao('listas de jogos do perfil');
  r = await http('PUT', '/api/conta/jogos', { favorito: { id: 1, nome: 'Outer Wilds' }, gosto: [{ id: 2, nome: 'Terraria' }, { id: 2, nome: 'repetido' }, { id: 'x', nome: 'ruim' }], quero: [] }, A.ck);
  confere(r.s === 200 && r.j.listas.gosto.length === 1 && r.j.listas.favorito.nome === 'Outer Wilds', 'salva listas e filtra lixo/duplicados');
  r = await http('GET', '/api/perfil/' + A.usuario, null, B.ck);
  confere(r.j.listas && r.j.listas.favorito && r.j.listas.favorito.id === 1, 'amigo vê as listas');
  r = await http('GET', '/api/perfil/' + A.usuario, null, C.ck);
  confere(r.j.listas && !r.j.listas.favorito, 'quem não é amigo não vê');
  r = await http('GET', '/api/jogos/busca?q=zelda', null, A.ck);
  confere(r.s === 503 || r.s === 200, 'busca de jogos responde (503 sem chave, 200 com)', String(r.s));
  r = await http('GET', '/api/jogos/busca?q=zelda');
  confere(r.s === 401, 'busca sem sessão: 401');

  [wa, wb, wc].forEach(w => w.close());
  console.log(`\n${total - falhas}/${total} checagens OK`);
  process.exit(falhas ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });
