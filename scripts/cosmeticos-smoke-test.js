#!/usr/bin/env node
// Smoke test for profile cosmetics (catalog, files, saving, broadcast).
// Start the server with COSMETICOS_DIR pointing at a prepared folder
// (scripts/cosmeticos-prepara.py) with at least one item of each type.
// Usage: NODE_PATH=<dir>/node_modules node scripts/cosmeticos-smoke-test.js http://localhost:8099
const { WebSocket } = require('ws');

const BASE = process.argv[2] || 'http://localhost:8099';
const host = new URL(BASE).hostname;
if (!['localhost', '127.0.0.1', '::1'].includes(host)) {
  console.error('Refusing to run against a non-local host:', host);
  process.exit(2);
}
let ok = 0;
let falhas = 0;
const confere = (cond, nome) => {
  if (cond) { ok++; } else { falhas++; console.log('FALHOU:', nome); }
};
const req = async (metodo, caminho, corpo, ck) => {
  const r = await fetch(BASE + caminho, {
    method: metodo,
    headers: { 'content-type': 'application/json', ...(ck ? { cookie: ck } : {}) },
    body: corpo ? JSON.stringify(corpo) : undefined,
  });
  let j = null;
  try { j = await r.json(); } catch { /* not json */ }
  return { s: r.status, j, c: r.headers.get('set-cookie') };
};
const conta = async u => {
  const r = await req('POST', '/api/conta/criar', { usuario: u, senha: 'senha-de-teste-123', nome: u });
  const ck = (r.c || '').split(';')[0];
  const p = await req('GET', '/api/perfil/' + u, null, ck);
  return { ck, uid: p.j && p.j.uid };
};

(async () => {
  const sufixo = Date.now().toString(36).slice(-5);
  const A = await conta('cosa' + sufixo);
  const B = await conta('cosb' + sufixo);

  const cat = await req('GET', '/api/cosmeticos');
  confere(cat.s === 200 && Array.isArray(cat.j.itens), 'catálogo responde lista');
  const itens = cat.j.itens || [];
  const de = t => itens.find(i => i.tipo === t);
  for (const t of ['decoracao', 'placa', 'efeito', 'moldura']) confere(!!de(t), `catálogo tem ${t}`);
  confere(itens.every(i => !('url' in i) && !('sku' in i)), 'catálogo sem URL/sku do Discord');
  confere(itens.every(i => /\?v=/.test(i.arquivo)), 'arquivo leva versão');

  const arq = await fetch(BASE + de('decoracao').arquivo);
  confere(arq.status === 200 && arq.headers.get('content-type') === 'image/webp', 'arquivo servido como webp');
  confere(/immutable/.test(arq.headers.get('cache-control') || ''), 'arquivo com cache immutable');
  const fora = await fetch(BASE + '/cosmeticos/..%2Fserver.js');
  confere(fora.status === 404, 'não sai da pasta');
  const json = await fetch(BASE + '/cosmeticos/catalogo.json');
  confere(json.status === 404, 'só webp/png');

  confere((await req('GET', '/api/cosmeticos/pessoas')).s === 401, 'pessoas exige sessão');
  confere((await req('POST', '/api/conta/cosmeticos', {})).s === 401, 'salvar exige sessão');

  const ruim = await req('POST', '/api/conta/cosmeticos', { decoracao: 'nao-existe' }, A.ck);
  confere(ruim.s === 400, 'id desconhecido recusado');
  const tipoErrado = await req('POST', '/api/conta/cosmeticos', { placa: de('decoracao').id }, A.ck);
  confere(tipoErrado.s === 400, 'item no tipo errado recusado');

  // B escuta o canal de avisos: a mudança de A tem que chegar.
  const ws = new WebSocket(BASE.replace(/^http/, 'ws'), { headers: { Cookie: B.ck } });
  const msgs = [];
  ws.on('message', d => { try { msgs.push(JSON.parse(d)); } catch { /* ignore */ } });
  await new Promise(r => ws.on('open', r));
  ws.send(JSON.stringify({ type: 'avisos' }));
  await new Promise(r => setTimeout(r, 400));

  const escolha = {
    decoracao: de('decoracao').id,
    placa: de('placa').id,
    efeito: de('efeito').id,
    moldura: de('moldura').id,
    nome: { fonte: 'bangers', efeito: 'neon', cor: '#EC4899', cor2: 'lixo', extra: 1 },
    tema: { cor1: '#3b0764', cor2: '#a855f7' },
  };
  const salvo = await req('POST', '/api/conta/cosmeticos', escolha, A.ck);
  confere(salvo.s === 200, 'salvar responde 200');
  const c = salvo.j && salvo.j.cosmeticos;
  confere(c && c.decoracao.id === escolha.decoracao && c.decoracao.arquivo === de('decoracao').arquivo, 'decoração com arquivo do catálogo');
  confere(c && c.nome.fonte === 'bangers' && c.nome.cor === '#ec4899' && !('cor2' in c.nome) && !('extra' in c.nome), 'estilo do nome limpo');
  confere(c && c.tema.cor1 === '#3b0764', 'tema guardado');
  await new Promise(r => setTimeout(r, 400));
  const aviso = msgs.find(m => m.type === 'cosmeticos' && m.uid === A.uid);
  confere(aviso && aviso.cosmeticos && aviso.cosmeticos.placa.id === escolha.placa, 'aviso chega a outra pessoa');

  const pessoas = await req('GET', '/api/cosmeticos/pessoas', null, B.ck);
  confere(pessoas.s === 200 && pessoas.j.pessoas[A.uid] && pessoas.j.pessoas[A.uid].efeito.id === escolha.efeito, 'pessoas lista a escolha');
  const perfil = await req('GET', '/api/perfil/cosa' + sufixo, null, B.ck);
  confere(perfil.j && perfil.j.cosmeticos && perfil.j.cosmeticos.moldura.id === escolha.moldura, 'perfil público leva cosméticos');

  const limpo = await req('POST', '/api/conta/cosmeticos', {}, A.ck);
  confere(limpo.s === 200 && limpo.j.cosmeticos === null, 'escolha vazia limpa');
  const depois = await req('GET', '/api/cosmeticos/pessoas', null, B.ck);
  confere(!depois.j.pessoas[A.uid], 'sem cosméticos some da lista');

  ws.close();
  console.log(`${ok}/${ok + falhas} checagens OK`);
  process.exit(falhas ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });
