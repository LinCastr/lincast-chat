#!/usr/bin/env node
// Teste de interoperabilidade: o protótipo Rust transmitindo pra uma sala do
// servidor v1, e a PÁGINA ATUAL (public/index.html, num Chromium headless)
// assistindo como uma pessoa de verdade — entra na sala, clica em "Entrar na
// chamada" e recebe a mídia.
//
// Confere o que dá pra conferir sem tela nem placa de som:
//   - a conexão WebRTC entre os dois chega em `connected`;
//   - a página reconhece a stream como TELA (pelo `screen-state`), não voz;
//   - o áudio chega com nível de som de verdade (não silêncio).
//
// Roda as duas ordens de chegada, que exercitam caminhos diferentes da
// sinalização: protótipo primeiro (a página entra na chamada depois e quem
// oferece sai da comparação de ids) e página primeiro (o protótipo oferece
// pelo `room-info`).
//
// Uso (com o servidor v1 de pé e um app tocando som no PipeWire):
//   node teste-interop.js [urlDoServidor] [nomeDoApp]
// O vídeo da tela NÃO é testado aqui: precisa do portal do Wayland, ou seja,
// de uma sessão gráfica de verdade. Por isso o protótipo roda com --sem-tela.

'use strict';
const { spawn, execFileSync } = require('child_process');
const fs = require('fs');
const os = require('os');
const path = require('path');
const http = require('http');

const SERVIDOR = process.argv[2] || 'http://localhost:8099';
const APP = process.argv[3] || 'jogofalso';
const PROTO = path.join(__dirname, '..', 'target', 'debug', 'concord-proto');
const PORTA_CDP = 9333;

let falhas = 0;
function checa(ok, descricao, detalhe) {
  if (ok) console.log(`  ok    ${descricao}`);
  else { falhas++; console.log(`  FALHA ${descricao}${detalhe ? '\n          → ' + detalhe : ''}`); }
}
const espera = (ms) => new Promise(r => setTimeout(r, ms));

function achaChromium() {
  if (process.env.CHROMIUM) return process.env.CHROMIUM;
  for (const nome of ['google-chrome-stable', 'google-chrome', 'chromium', 'chromium-browser']) {
    try { return execFileSync('sh', ['-c', `command -v ${nome}`], { encoding: 'utf8' }).trim(); } catch { /* próximo */ }
  }
  return null;
}

const pegaJSON = (url) => new Promise((resolve, reject) => {
  http.get(url, (res) => {
    let d = '';
    res.on('data', c => d += c);
    res.on('end', () => { try { resolve(JSON.parse(d)); } catch (e) { reject(e); } });
  }).on('error', reject);
});

async function abrePagina(chromium) {
  const perfil = fs.mkdtempSync(path.join(os.tmpdir(), 'concord-interop-'));
  const nav = spawn(chromium, [
    '--headless', '--no-sandbox', '--mute-audio', '--disable-gpu',
    '--autoplay-policy=no-user-gesture-required',
    // Microfone falso, sem pedir permissão: é o que deixa "Entrar na chamada"
    // funcionar sem gente clicando.
    '--use-fake-ui-for-media-stream', '--use-fake-device-for-media-stream',
    `--remote-debugging-port=${PORTA_CDP}`, `--user-data-dir=${perfil}`, SERVIDOR
  ], { stdio: 'ignore' });
  let alvo;
  for (let i = 0; i < 60 && !alvo; i++) {
    try { alvo = (await pegaJSON(`http://localhost:${PORTA_CDP}/json`)).find(a => a.type === 'page'); } catch { /* subindo */ }
    if (!alvo) await espera(300);
  }
  if (!alvo) throw new Error('o Chromium não abriu a página');
  const ws = new WebSocket(alvo.webSocketDebuggerUrl);
  await new Promise((res, rej) => { ws.onopen = res; ws.onerror = () => rej(new Error('CDP')); });
  let id = 0;
  const pend = new Map();
  ws.addEventListener('message', (ev) => {
    const m = JSON.parse(ev.data);
    if (m.method === 'Page.javascriptDialogOpening') {
      console.log(`  (diálogo da página: "${m.params.message}")`);
      ws.send(JSON.stringify({ id: ++id, method: 'Page.handleJavaScriptDialog', params: { accept: true } }));
    }
    if (pend.has(m.id)) { pend.get(m.id)(m); pend.delete(m.id); }
  });
  const cmd = (method, params = {}) => new Promise(r => { const i = ++id; pend.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
  await cmd('Page.enable');
  const avalia = async (expr) => {
    const r = await cmd('Runtime.evaluate', { expression: expr, returnByValue: true, awaitPromise: true });
    if (r.result.exceptionDetails) throw new Error(r.result.exceptionDetails.exception?.description || 'erro no eval');
    return r.result.result.value;
  };
  for (let i = 0; i < 50; i++) {
    if (await avalia(`typeof joinRoom === 'function' && typeof handleSignal === 'function'`).catch(() => false)) break;
    await espera(200);
  }
  return { avalia, fecha: () => { try { nav.kill(); } catch { /* já foi */ } } };
}

// Com DISPLAY definido (um Xvfb, por exemplo), o protótipo manda também a tela
// pelo capturador X11 — é o caminho de vídeo inteiro menos o portal do Wayland.
const COM_VIDEO = !!process.env.DISPLAY;

function sobeProto(sala) {
  const tela = COM_VIDEO ? ['--x11'] : ['--sem-tela'];
  const p = spawn(PROTO, ['transmitir', '--servidor', SERVIDOR, '--sala', sala, ...tela, '--app', APP], {
    stdio: ['ignore', 'inherit', 'pipe'], env: { ...process.env, RUST_LOG: 'info' }
  });
  const log = [];
  p.stderr.on('data', d => { for (const l of d.toString().split('\n')) if (l.trim()) log.push(l.replace(/^\[[^\]]*\] ?/, '')); });
  return { p, log };
}

// Espera a conexão e mede o som recebido com um analisador de frequência na
// própria página. As estatísticas (`audioLevel`) não servem aqui: o Chromium só
// as calcula quando há placa de som puxando o áudio, e headless não tem.
async function mede(avalia) {
  let m = null;
  for (let i = 0; i < 40; i++) {
    const r = await avalia(`(async () => {
      const ids = Object.keys(peers);
      if (!ids.length) return null;
      const pc = peers[ids[0]];
      const rec = pc.getReceivers().find(r => r.track && r.track.kind === 'audio');
      if (pc.connectionState === 'connected' && rec && !window.__an) {
        const ctx = new AudioContext();
        const src = ctx.createMediaStreamSource(new MediaStream([rec.track]));
        const an = ctx.createAnalyser();
        an.fftSize = 16384;
        src.connect(an);
        window.__an = an; window.__ctx = ctx; window.__src = src;
      }
      let rms = 0, pico = 0;
      if (window.__an) {
        const t = new Float32Array(window.__an.fftSize);
        window.__an.getFloatTimeDomainData(t);
        rms = Math.sqrt(t.reduce((a, x) => a + x * x, 0) / t.length);
        const f = new Float32Array(window.__an.frequencyBinCount);
        window.__an.getFloatFrequencyData(f);
        let mi = 0; f.forEach((v, j) => { if (v > f[mi]) mi = j; });
        pico = mi * window.__ctx.sampleRate / window.__an.fftSize;
      }
      let quadros = 0, largura = 0, altura = 0;
      (await pc.getStats()).forEach(s => {
        if (s.type === 'inbound-rtp' && s.kind === 'video') { quadros = s.framesDecoded || 0; largura = s.frameWidth || 0; altura = s.frameHeight || 0; }
      });
      return JSON.stringify({ estado: pc.connectionState, ctx: window.__ctx && window.__ctx.state,
        rms, pico, telas: [...screenStreamIds], caixaDeVoz: !!firstStreamId[ids[0]],
        quadros, largura, altura, caixaDeTela: !!document.getElementById('wrap-' + ids[0] + '-screen') });
    })()`);
    if (r) {
      m = JSON.parse(r);
      if (m.estado === 'connected' && m.rms > 0.02 && (!COM_VIDEO || m.quadros > 30)) return m;
    }
    await espera(500);
  }
  return m;
}

async function cenario(chromium, nome, protoPrimeiro) {
  console.log(`\n${nome}`);
  const sala = 'interop-' + Math.random().toString(36).slice(2, 8);
  let proto, pagina;
  try {
    if (protoPrimeiro) { proto = sobeProto(sala); await espera(2000); }
    pagina = await abrePagina(chromium);
    await pagina.avalia(`document.getElementById('roomInput').value = ${JSON.stringify(sala)}; joinRoom(); true`);
    for (let i = 0; i < 30 && !(await pagina.avalia('!!myId')); i++) await espera(200);
    await pagina.avalia('entraNaChamada().then(() => true)');
    if (!protoPrimeiro) { await espera(1000); proto = sobeProto(sala); }
    const m = await mede(pagina.avalia);
    checa(m && m.estado === 'connected', 'a conexão WebRTC com o protótipo fica connected', JSON.stringify(m));
    checa(m && m.telas.some(t => t.startsWith('concord-tela-')), 'a página classifica a stream como tela (screen-state chegou)', JSON.stringify(m && m.telas));
    checa(m && !m.caixaDeVoz, 'a stream NÃO foi tomada como voz', '');
    checa(m && m.rms > 0.02, `o som do app chega com nível de verdade (RMS ${m ? m.rms.toFixed(3) : '-'})`, JSON.stringify(m));
    checa(m && Math.abs(m.pico - 440) < 10, `e é o som certo: pico em ${m ? m.pico.toFixed(0) : '-'} Hz (o app toca 440 Hz)`, '');
    if (COM_VIDEO && process.env.SALVA_QUADRO) {
      const png = await pagina.avalia(`(() => {
        const v = document.querySelector('video[id$="-screen"], [id$="-screen"] video');
        if (!v || !v.videoWidth) return '';
        const c = document.createElement('canvas'); c.width = v.videoWidth; c.height = v.videoHeight;
        c.getContext('2d').drawImage(v, 0, 0); return c.toDataURL('image/png');
      })()`);
      if (png) { fs.writeFileSync(process.env.SALVA_QUADRO, Buffer.from(png.split(',')[1], 'base64')); console.log(`  (quadro salvo em ${process.env.SALVA_QUADRO})`); }
    }
    if (COM_VIDEO) {
      checa(m && m.quadros > 30, `a página decodifica o vídeo da tela (${m ? m.quadros : 0} quadros, ${m ? m.largura + 'x' + m.altura : '-'})`, JSON.stringify(m));
      checa(m && m.caixaDeTela, 'a caixa de tela aparece na página (wrap-<id>-screen)', '');
    }
    if (falhas) console.log('  log do protótipo:\n    ' + proto.log.slice(-25).join('\n    '));
  } finally {
    if (pagina) pagina.fecha();
    if (proto) proto.p.kill('SIGINT');
    await espera(800);
  }
}

(async () => {
  const chromium = achaChromium();
  if (!chromium) { console.error('precisa de Chromium/Chrome no PATH'); process.exit(2); }
  if (!fs.existsSync(PROTO)) { console.error(`compile antes: cargo build (faltou ${PROTO})`); process.exit(2); }
  console.log(`Servidor: ${SERVIDOR}  ·  app capturado: ${APP}  ·  vídeo: ${COM_VIDEO ? 'X11 em ' + process.env.DISPLAY : 'não (sem DISPLAY)'}`);
  await cenario(chromium, 'Protótipo chega primeiro, a página entra na chamada depois', true);
  await cenario(chromium, 'Página na chamada primeiro, o protótipo chega depois', false);
  console.log(falhas ? `\n${falhas} falha(s).` : '\nTudo certo.');
  process.exit(falhas ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });
