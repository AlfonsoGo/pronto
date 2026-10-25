// Sonda: la Flow bar (ui/flowbar.html + flowbar.js) con un backend falso controlable (window.__fb.emit).
// Eventos hostiles: avisos, errores de dictado, estados de reunión y ajustes con HTML/JS, textos de
// 100 000 caracteres, acciones desconocidas, niveles NaN/Infinity, estados inventados, ráfagas de 2 000
// cambios de estado. Mide: nodos [data-pwn], JS ejecutado, errores JS, tarjeta fuera de la ventana de 360×150.
import { scriptFlowbarFalso, espera, captura } from './comun.mjs';

export const nombre = 'flowbar';
const X = (k) => `<img src=x data-pwn="${k}" onerror="__pwnHit('${k}')"><b data-pwn="b-${k}">negrita</b>`;

export async function correr({ abrir }) {
  const out = [];
  const settingsHostiles = {
    hotkeyHold: { ctrl: false, alt: true, shift: false, win: true, key: X('hotkey') },
    showFlowBar: 'no', flowBarHiddenUntil: 'mañana', flowBarMonitor: X('monitor'), appLanguage: X('lang'),
  };
  const { page, pwn, cerrar } = await abrir('/flowbar.html', { viewport: { width: 360, height: 150 }, init: scriptFlowbarFalso(settingsHostiles) });
  await page.waitForSelector('#pill');
  const emit = (n, p) => page.evaluate(([a, b]) => window.__fb.emit(a, b), [n, p]);
  const hover = () => page.evaluate(() => window.__fb.emit('flowbar://hover', { hover: true }));

  // 1 · tooltips con el atajo hostil (hover) y candado con el monitor hostil
  await hover(); await page.hover('#pill').catch(() => {}); await espera(300);
  await page.hover('#sat-lock').catch(() => {}); await espera(200);

  // 2 · avisos y errores con HTML
  await emit('dictation://notice', { title: X('notice-title'), body: X('notice-body'), action: X('notice-action') }); await espera(250);
  const accion = await page.$eval('#card-act', (b) => ({ hidden: b.hidden, text: b.textContent }));
  for (const kind of ['tap', 'nomic', 'noaudio', 'empty', 'paste', 'ollama', 'ia', 'select', 'otro']) {
    await emit('dictation://state', { state: 'error', kind, message: X('err-' + kind) }); await espera(60);
  }
  await emit('meeting://state', { state: 'done', meetingId: 7, message: X('meeting-done') }); await espera(200);
  await emit('meeting://state', { state: 'error', message: X('meeting-error') }); await espera(200);
  await emit('meeting://state', { state: X('meeting-state') }); await espera(100);
  await emit('settings://changed', { ...settingsHostiles, appLanguage: 'en' }); await espera(100);

  // 3 · basura: estados inventados, payloads nulos, niveles imposibles, acciones raras
  for (const p of [null, {}, { state: 'foo' }, { state: '__proto__' }, { state: 'constructor' }, { state: 'toString' }]) await emit('dictation://state', p);
  await emit('meeting://state', null); await emit('meeting://state', { state: 'recording' });
  for (const l of [NaN, Infinity, -5, '0.5', null, 1e308]) await emit('dictation://level', { level: l });
  for (const l of [NaN, Infinity, -5, 'x']) await emit('meeting://level', { mic: l, system: l });
  for (const a of ['open_meeting:abc', 'open_meeting:1e309', 'open_meeting:', '__proto__', 'constructor']) await emit('dictation://notice', { title: 't', action: a });
  await emit('dictation://state', { state: 'listening' });
  await espera(400);
  const ondas = await page.$eval('#waves', (w) => w.style.getPropertyValue('--audio-scale'));

  // 4 · ráfaga de 2 000 cambios de estado
  await page.evaluate(() => { const st = ['listening', 'processing', 'polishing', 'polished', 'done', 'cancelled', 'error', 'handsfree', 'idle']; for (let i = 0; i < 2000; i++) window.__fb.emit('dictation://state', { state: st[i % st.length], message: 'm' }); });
  await emit('dictation://state', { state: 'idle' }); await emit('meeting://state', { state: 'idle' }); await espera(300);
  const final = await page.$eval('#pill', (p) => p.dataset.s);

  // 5 · aviso de 100 000 caracteres: ¿cabe la tarjeta en la ventana de 360×150?
  await emit('dictation://notice', { title: 'T'.repeat(100000), body: 'palabra '.repeat(12500) }); await espera(300);
  const caja = await page.$eval('#card', (c) => { const r = c.getBoundingClientRect(); return { top: Math.round(r.top), bottom: Math.round(r.bottom), left: Math.round(r.left), right: Math.round(r.right), h: Math.round(r.height) }; });
  const capLargo = (caja.top < 0 || caja.bottom > 150 || caja.right > 360) ? await captura(page, 'flowbar-aviso-largo') : '';
  // aviso normal con acción paste_last después de uno largo
  await emit('dictation://notice', { title: 'No se pudo pegar', body: 'Haz clic en un cuadro de texto y pega con Ctrl + V.', action: 'paste_last' }); await espera(300);
  const cajaN = await page.$eval('#card', (c) => { const r = c.getBoundingClientRect(); return { top: Math.round(r.top), bottom: Math.round(r.bottom) }; });

  const r = await pwn();
  const calls = await page.evaluate(() => window.__fb.calls.map((c) => c[0]));
  if (r.hits.length) out.push({ grav: 'ALTA', titulo: 'Flow bar ejecuta JS que llega del backend', evidencia: r.hits.join(', ') });
  if (r.nodos.length) out.push({ grav: 'ALTA', titulo: 'Flow bar interpreta HTML del backend', evidencia: r.nodos.join(', ') });
  if (!r.hits.length && !r.nodos.length) out.push({ grav: 'OK', titulo: 'Flow bar: ningún texto del backend se interpreta como HTML', evidencia: 'avisos, 9 tipos de error, reunión done/error, ajustes, tooltips' });
  if (!accion.hidden) out.push({ grav: 'BAJA', titulo: 'Un aviso con una acción desconocida pinta un botón con el texto crudo de la acción (que no hace nada)', evidencia: `action=<img…> → botón visible «${accion.text.slice(0, 60)}…» (flowbar.js:265 resolveAction devuelve [str, null])` });
  if (capLargo) out.push({ grav: 'BAJA', titulo: 'Un aviso muy largo se sale de la ventana de 360×150 (título y cuerpo sin límite de líneas)', evidencia: `tarjeta ${JSON.stringify(caja)} con stage 360×150 · ${capLargo}` });
  out.push({ grav: ondas === '' || /^\d/.test(ondas) ? 'OK' : 'BAJA', titulo: `ondas tras niveles imposibles: --audio-scale=«${ondas}»`, evidencia: '' });
  out.push({ grav: final === 'rest' || final === 'ready' ? 'OK' : 'BAJA', titulo: `ráfaga de 2 000 estados → estado final «${final}»`, evidencia: `aviso normal tras el largo: ${JSON.stringify(cajaN)} · comandos: ${[...new Set(calls)].join(', ')}` });
  await cerrar();

  // 6 · prefers-reduced-motion: ¿animaciones infinitas en escucha / procesando?
  const rm = await abrir('/flowbar.html', { viewport: { width: 360, height: 150 }, reducedMotion: 'reduce', init: scriptFlowbarFalso() });
  await rm.page.waitForSelector('#pill');
  const anims = {};
  for (const st of ['listening', 'processing', 'polishing']) {
    await rm.page.evaluate((s) => window.__fb.emit('dictation://state', { state: s }), st); await espera(300);
    anims[st] = await rm.page.evaluate(() => document.getAnimations().filter((a) => a.playState === 'running' && a.effect?.getComputedTiming().iterations === Infinity).map((a) => a.animationName || 'anim'));
  }
  const todas = Object.values(anims).flat(), tot = todas.filter((n) => n !== 'spin-fade').length;
  out.push({ grav: tot ? 'BAJA' : 'OK', titulo: `reduced-motion: ${tot} animaciones infinitas decorativas (el spinner de progreso, ${todas.length - tot}, se admite como esencial)`, evidencia: JSON.stringify(anims) });
  await rm.cerrar();
  return out;
}
