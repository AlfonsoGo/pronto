// Rompedor · piezas comunes de las sondas del hub y la flow bar (Chrome headless + backend simulado).
// Cada sonda exporta `nombre` y `correr({ abrir, base, captura, registro })` y devuelve una lista de hallazgos
// { grav: 'ALTA'|'MEDIA'|'BAJA'|'OK', titulo, evidencia }. Ninguna petición sale de 127.0.0.1: todo lo demás se
// aborta y se anota como hallazgo.
import { chromium } from 'playwright-core';
import { mkdir } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { servir } from '../../scripts/servir-ui.mjs';

export const AQUI = dirname(fileURLToPath(import.meta.url));
export const CAPTURAS = join(AQUI, 'capturas');
const CHROME = process.env.E2E_CHROME ?? 'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe';

/** Textos hostiles (catálogo de entradas). */
export const HOSTIL = {
  img: '<img src=x data-pwn="entrada" onerror="__pwnHit(\'entrada\')">',
  svg: '<svg/onload=__pwnHit("svg") data-pwn="svg">',
  plantilla: '{{7*7}} ${7*7} <%= 7*7 %>',
  js: 'javascript:__pwnHit("js")',
  rtl: '\u202Eesrever\u202C עברית مرحبا',
  zwj: '👩‍👩‍👧‍👦🏳️‍🌈👨🏽‍💻',
  nul: 'antes\u0000después',
  comillas: '"\'><b data-pwn="comillas">x</b>',
  largo: 'a'.repeat(100000),
};

/** Backend hostil: un window.__TAURI__ falso que delega en mock.js y envenena cada texto que devuelve o emite. */
export function scriptBackendHostil({ envenenar = true, extra = '' } = {}) {
  return `(() => {
  const P = (k) => '<img src=x data-pwn="' + k + '" onerror="__pwnHit(\\'' + k + '\\')"><b data-pwn="b-' + k + '">{{7*7}}</b>';
  const NO = new Set(['state','phase','kind','stage','theme','appLanguage','language','asrEngine','whisperModel','cleanupLevel','meetingAuto',
    'key','createdAt','updatedAt','startedAt','endedAt','publishedAt','checkedAt','date','flowBarHiddenUntil','flowBarMonitor','device',
    'source','speaker','askedBy','status','tab','page','personal','work','email','other','tone','id','dir','modelDir','file','who']);
  const NO_CMD = new Set(['get_history_audio','get_word_sample_audio','capture_hotkey']);
  const ON = ${envenenar ? 'true' : 'false'};
  const poison = (v, k) => {
    if (!ON) return v;
    if (typeof v === 'string') return NO.has(k) ? v : v + P(k || 'raiz');
    if (Array.isArray(v)) return v.map((x) => poison(x, k));
    if (v && typeof v === 'object') { const o = {}; for (const [kk, vv] of Object.entries(v)) o[kk] = poison(vv, kk); return o; }
    return v;
  };
  let mp = null; const m = () => (mp = mp || import('/shared/mock.js'));
  window.__hostil = { poison, m };
  window.__calls = []; window.__lento = window.__lento || {};
  const ventana = { minimize: async () => {}, toggleMaximize: async () => {}, hide: async () => {}, close: async () => { window.__calls.push('ventana.close'); }, isMaximized: async () => false };
  window.__TAURI__ = {
    core: { invoke: async (cmd, args) => {
      const mod = await m();
      window.__calls.push(cmd);
      if (window.__lento[cmd]) await new Promise((r) => setTimeout(r, window.__lento[cmd]));
      try { const r = await mod.invoke(cmd, args); return NO_CMD.has(cmd) ? r : poison(r, cmd); }
      catch (e) { throw poison(typeof e === 'string' ? e : String(e && e.message || e), 'error-' + cmd); }
    } },
    event: { listen: async (ev, cb) => (await m()).listen(ev, (e) => cb({ event: e.event, payload: poison(e.payload, ev) })) },
    window: { getCurrentWindow: () => ventana },
  };
  ${extra}
})();`;
}

/** Flow bar con un backend falso controlable: window.__fb.emit(evento, payload) y window.__fb.calls. */
export const scriptFlowbarFalso = (settings = null) => `(() => {
  const h = {}; const calls = [];
  const S = ${JSON.stringify(settings)} || { hotkeyHold: { ctrl: false, alt: true, shift: false, win: true, key: null }, showFlowBar: true, flowBarHiddenUntil: null, flowBarMonitor: null, appLanguage: 'es' };
  window.__fb = { calls, S, emit: (n, p) => (h[n] || []).slice().forEach((f) => f({ event: n, payload: p })) };
  window.__TAURI__ = {
    core: { invoke: async (cmd, args) => { calls.push([cmd, args]); if (cmd === 'get_settings') return JSON.parse(JSON.stringify(window.__fb.S)); if (cmd === 'meeting_status') return { active: false, elapsedMs: 0 }; if (cmd === 'list_transforms') return window.__fb.transforms || []; return null; } },
    event: { listen: async (n, cb) => { (h[n] = h[n] || []).push(cb); return () => {}; } },
  };
})();`;

export async function arrancar() {
  if (!existsSync(CHROME)) throw new Error(`No existe Chrome en ${CHROME}`);
  await mkdir(CAPTURAS, { recursive: true });
  const servidor = await servir(Number(process.env.ROMPEDOR_PUERTO ?? 4181)); // 4181 (nunca 4173: es del recorredor)
  const port = servidor.address().port;
  const base = `http://127.0.0.1:${port}`;
  const browser = await chromium.launch({ executablePath: CHROME, headless: true, args: ['--autoplay-policy=no-user-gesture-required', '--mute-audio'] });
  return { base, port, browser, cerrar: async () => { await browser.close(); await new Promise((ok) => servidor.close(ok)); } };
}

const primera = (e) => String((e && e.message) || e).split('\n').map((l) => l.trim()).find(Boolean) ?? 'error';

/** Abre una página con guardas: errores JS, console.error, diálogos (alert de XSS) y peticiones fuera de 127.0.0.1. */
export function fabrica(browser, base, registro) {
  return async function abrir(ruta, opts = {}) {
    const { viewport = { width: 1280, height: 800 }, colorScheme = 'dark', reducedMotion = 'no-preference', hostil = false,
      init = '', deviceScaleFactor = 1 } = opts;
    const ctx = await browser.newContext({ viewport, colorScheme, reducedMotion, deviceScaleFactor, locale: 'es-ES',
      permissions: ['clipboard-read', 'clipboard-write'] });
    const errores = [], consola = [], fuera = [], dialogos = [];
    await ctx.route('**/*', (r) => {
      const u = new URL(r.request().url());
      if (u.protocol === 'data:' || u.protocol === 'blob:' || u.hostname === '127.0.0.1') return r.continue();
      fuera.push(r.request().url()); return r.abort();
    });
    await ctx.addInitScript(() => { window.__pwn = []; window.__pwnHit = (f) => window.__pwn.push(String(f)); });
    if (hostil) await ctx.addInitScript(typeof hostil === 'string' ? hostil : scriptBackendHostil());
    if (init) await ctx.addInitScript(init);
    const page = await ctx.newPage();
    page.on('pageerror', (e) => {
      const donde = String(e.stack || '').split('\n').map((l) => l.match(/127\.0\.0\.1:\d+\/([^?#)\s]+):(\d+):\d+/)).filter(Boolean).slice(0, 2).map((m) => `ui/${m[1]}:${m[2]}`).join(' ← ');
      errores.push(primera(e) + (donde ? ` @ ${donde}` : ''));
    });
    page.on('console', (m) => { if (m.type() === 'error') consola.push(m.text().slice(0, 300)); });
    page.on('response', (r) => { if (r.status() >= 400) consola.push(`HTTP ${r.status()} ${r.url().replace(/^https?:\/\/127\.0\.0\.1:\d+/, '')}`); });
    page.on('dialog', (d) => { dialogos.push(d.message()); d.dismiss().catch(() => {}); });
    page.on('request', (rq) => { const u = new URL(rq.url()); if (!['data:', 'blob:'].includes(u.protocol) && u.hostname !== '127.0.0.1') fuera.push(rq.url()); });
    await page.goto(base + ruta);
    const pwn = () => page.evaluate(() => ({
      hits: [...new Set(window.__pwn)],
      nodos: [...new Set([...document.querySelectorAll('[data-pwn]')].map((n) => `${n.tagName.toLowerCase()}[${n.getAttribute('data-pwn')}]`))],
    }));
    const guardas = () => ({ errores: [...new Set(errores)], consola: [...new Set(consola)], fuera: [...new Set(fuera)], dialogos });
    registro.push(guardas);
    return { page, ctx, pwn, guardas, cerrar: () => ctx.close() };
  };
}

export async function captura(page, nombre) {
  const f = join(CAPTURAS, `${nombre}.png`);
  try { await page.screenshot({ path: f, fullPage: false }); } catch (_) { /* */ }
  return `e2e/rompedor/capturas/${nombre}.png`;
}

export const espera = (ms) => new Promise((r) => setTimeout(r, ms));
