// «Activar IA local»: estado compartido, aviso previo y los trozos de interfaz (tarjeta y botón) que
// usan Configuración, Estilo, Transforms y Reuniones. Contrato v6 en docs/ARQUITECTURA.md.
//
// Uso en una página:
//   plantilla:   ${aiSlot('card')}  (tarjeta completa)  o  ${aiSlot('cta')}  (botón/progreso en línea)
//   al montar:   const off = aiMount(root, (kind, prev) => { if (kind === 'ready') volverAComprobar(); });
// El resto (clics, progreso, errores) lo gestiona este módulo.
import { invoke, listen } from './api.js';
import { icon } from '../shared/icons.js';
import { esc, t, openModal, toast, errText } from './util.js';

const MODEL = 'qwen3:1.7b';
const END = new Set(['done', 'error', 'cancelled']);

export const ai = { status: null, progress: null, plan: [], starting: false };
const subs = new Set();
const notify = () => subs.forEach((fn) => { try { fn(); } catch (e) { console.error(e); } });
export const aiSubscribe = (fn) => { subs.add(fn); return () => subs.delete(fn); };

let listening = false;
function ensureListening() {
  if (listening) return;
  listening = true;
  listen('ai://progress', (p) => {
    if (!p) return;
    ai.progress = p; ai.starting = false;
    if (ai.status) ai.status = { ...ai.status, running: !END.has(p.phase) };
    if (p.phase === 'done') toast(t('IA local lista'));
    if (p.phase === 'cancelled') toast(t('Activación cancelada'));
    if (END.has(p.phase)) refreshAi(); else notify();
  });
}

export async function refreshAi() {
  try { ai.status = await invoke('ai_setup_status'); } catch (_) { /* sin backend: se queda como estaba */ }
  notify();
  return ai.status;
}

/** idle | running | error | ready */
export function aiKind() {
  const s = ai.status, p = ai.progress;
  if (ai.starting || (s && s.running)) return 'running';
  if (s && s.ready) return 'ready';
  if (p && p.phase === 'error') return 'error';
  if (s && s.phase === 'error') return 'error';
  return 'idle';
}
const errMsg = () => (ai.progress && ai.progress.phase === 'error' ? ai.progress.message : (ai.status && ai.status.message) || '');

// ------------------------------------------------------------------ aviso previo y arranque
const mb = (n) => Math.round((Number(n) || 0) / 1048576);

function consent() {
  const s = ai.status || {};
  const hasOllama = !!s.ollamaInstalled;
  const what = hasOllama
    ? t('Ya tienes Ollama. Pronto descargará el modelo {m} (≈ 1,4 GB) y lo dejará listo.', { m: `<b>${MODEL}</b>` })
    : t('Pronto instalará Ollama (≈ 1,5 GB) y el modelo {m} (≈ 1,4 GB) en tu equipo.', { m: `<b>${MODEL}</b>` });
  return new Promise((resolve) => {
    let res = false;
    const m = openModal({
      title: t('Activar la IA local'), width: 480,
      body: `<div class="ai-consent"><p>${what}</p>
        <ul>
          <li>${esc(t('Usa la CPU y la memoria de tu PC; en equipos modestos puede ir más lento.'))}</li>
          <li>${esc(t('Sirve para pulir, corregir y resumir textos y reuniones.'))}</li>
          <li>${esc(t('El dictado normal no la necesita: funciona igual sin esto.'))}</li>
          ${hasOllama ? '' : `<li>${esc(t('Ollama se instala solo para tu usuario, sin pedir permisos de administrador.'))}</li>`}
        </ul></div>`,
      footer: `<button class="btn btn-ghost" data-close>${esc(t('Ahora no'))}</button>
        <button class="btn btn-primary" data-ok>${icon('download', 16)}${esc(t('Instalar'))}</button>`,
      onClose: () => resolve(res),
    });
    m.el.querySelector('[data-ok]').addEventListener('click', () => { res = true; m.close(); });
  });
}

/** Pide confirmación y lanza la activación. Devuelve true si se lanzó. */
let asking = false; // aviso previo abierto: un doble clic no abre otro ni lanza la activación dos veces
export async function aiActivate() {
  ensureListening();
  if (asking || aiKind() === 'running') return false;
  asking = true;
  try {
    await refreshAi();
    if (!(await consent())) return false;
  } finally { asking = false; }
  const s = ai.status || {};
  ai.plan = [];
  if (!s.ollamaInstalled) ai.plan.push('download-ollama', 'install-ollama');
  if (!s.ollamaRunning) ai.plan.push('start-ollama');
  if (!s.modelReady) ai.plan.push('pull-model');
  ai.progress = { phase: 'check', received: 0, total: 0, message: 'Comprobando…' };
  ai.starting = true;
  notify();
  try { await invoke('ai_setup_start'); }
  catch (e) {
    ai.starting = false;
    ai.progress = { phase: 'error', received: 0, total: 0, message: errText(e) };
    notify();
  }
  return true;
}

async function aiCancel() {
  try { await invoke('ai_setup_cancel'); } catch (e) { toast(errText(e), { kind: 'err' }); }
}

// ------------------------------------------------------------------ marcado
const STEP = {
  'download-ollama': 'Descargar Ollama', 'install-ollama': 'Instalar Ollama', 'start-ollama': 'Arrancar Ollama', 'pull-model': 'Descargar el modelo',
};

function runningHtml() {
  const p = ai.progress || { phase: 'check', received: 0, total: 0, message: 'Comprobando…' };
  const known = p.total > 0;
  const pct = known ? Math.min(100, Math.floor((p.received * 100) / p.total)) : 0;
  const bytes = known && (p.phase === 'download-ollama' || p.phase === 'pull-model')
    ? t('{a} de {b} MB', { a: mb(p.received), b: mb(p.total) }) + ` · ${pct} %` : '';
  const idx = ai.plan.indexOf(p.phase);
  const steps = ai.plan.length > 1
    ? `<ol class="ai-steps" aria-label="${esc(t('Pasos'))}">${ai.plan.map((ph, i) =>
      `<li class="${i < idx ? 'done' : i === idx ? 'on' : ''}">${i < idx ? icon('check', 13) : `<i>${i + 1}</i>`}${esc(t(STEP[ph]))}</li>`).join('')}</ol>` : '';
  const slow = p.phase === 'install-ollama' ? `<p class="ai-hint">${esc(t('Puede tardar un par de minutos. No hace falta que hagas nada.'))}</p>` : '';
  const lock = p.phase === 'install-ollama';
  return `${steps}
    <div class="mc-prog ai-prog"><div class="mc-bar ${known ? '' : 'indet'}" data-ai-bar role="progressbar" aria-label="${esc(t('Activando la IA local'))}" aria-valuemin="0" aria-valuemax="100" ${known ? `aria-valuenow="${pct}"` : ''}><i style="--p:${known ? pct : 100}%"></i></div>
      <div class="mc-meta"><b data-ai-label>${esc(t(p.message || ''))}</b><span data-ai-bytes>${esc(bytes)}</span></div></div>
    ${slow}
    <button class="btn btn-ghost btn-sm ai-cancel" data-act="ai-cancel" ${lock ? `disabled title="${esc(t('No se puede cancelar mientras se instala.'))}"` : ''}>${esc(t('Cancelar'))}</button>`;
}

function errorHtml(retryLabel) {
  return `<div class="aviso rojo" role="alert"><div class="in"><span class="ic-l">${icon('triangle-alert', 18)}</span>
    <div class="tx"><b>${esc(t('No se pudo activar la IA local'))}</b><span>${esc(errMsg())}</span></div></div></div>
    <button class="btn btn-primary" data-act="ai-start">${icon('refresh-cw', 16)}${esc(retryLabel)}</button>`;
}

function cardHtml() {
  const k = aiKind(), s = ai.status || {};
  let title = t('Activar IA local'), body = '', desc = t('Pule y corrige tus textos, resume reuniones y responde preguntas sobre ellas, todo en tu PC.');
  if (k === 'ready') {
    title = t('IA local lista');
    desc = t('Ollama y el modelo {m} están listos en tu equipo.', { m: MODEL });
    body = `<span class="badge ok"><i class="dot"></i>${esc(t('Lista'))}</span>`;
  } else if (k === 'running') {
    title = t('Activando la IA local…');
    body = runningHtml();
  } else if (k === 'error') {
    body = errorHtml(t('Reintentar'));
  } else {
    const note = s.ollamaInstalled ? t('Ya tienes Ollama; solo falta descargar el modelo {m} (≈ 1,4 GB).', { m: MODEL })
      : t('Instala Ollama (≈ 1,5 GB) y el modelo {m} (≈ 1,4 GB). El dictado normal no la necesita.', { m: MODEL });
    body = `<button class="btn btn-primary" data-act="ai-start">${icon('sparkles', 16)}${esc(t('Activar IA local'))}</button><p class="ai-note">${esc(note)}</p>`;
  }
  return `<div class="ai-card tarj ${k}"><div class="in">
    <span class="mc-ic" aria-hidden="true">${icon(k === 'ready' ? 'circle-check' : 'wand-sparkles', 22)}</span>
    <div class="mc-tx"><h2>${esc(title)}</h2><p>${esc(desc)}</p>${body}</div></div></div>`;
}

function ctaHtml() {
  const k = aiKind();
  if (k === 'ready') return `<span class="badge ok"><i class="dot"></i>${esc(t('IA local lista'))}</span>`;
  if (k === 'running') return `<div class="ai-cta">${runningHtml()}</div>`;
  if (k === 'error') return `<div class="ai-cta"><span class="err-t">${esc(errMsg())}</span>
    <button class="btn btn-primary btn-sm" data-act="ai-start">${icon('refresh-cw', 14)}${esc(t('Activar de nuevo'))}</button></div>`;
  return `<button class="btn btn-primary btn-sm" data-act="ai-start">${icon('sparkles', 14)}${esc(t('Activar IA local'))}</button>`;
}

const slotHtml = (variant) => (variant === 'cta' ? ctaHtml() : cardHtml());
const slotKey = (variant) => `${aiKind()}|${ai.progress && ai.progress.phase}|${ai.plan.join()}|${errMsg()}|${ai.status && ai.status.ollamaInstalled}`;

/** Marcador que se coloca en la plantilla de una página. variant: 'card' | 'cta'. */
export function aiSlot(variant = 'card') {
  return `<div class="ai-slot" data-ai-slot="${variant}" data-ai-key="${esc(slotKey(variant))}">${slotHtml(variant)}</div>`;
}

function fill(root) {
  root.querySelectorAll('[data-ai-slot]').forEach((el) => {
    const variant = el.dataset.aiSlot, key = slotKey(variant);
    if (el.dataset.aiKey !== key) {
      el.dataset.aiKey = key;
      el.innerHTML = slotHtml(variant);
      return;
    }
    // Misma fase: solo se mueve la barra y los números (así la animación no se corta).
    const p = ai.progress, bar = el.querySelector('[data-ai-bar]');
    if (!p || !bar) return;
    const known = p.total > 0, pct = known ? Math.min(100, Math.floor((p.received * 100) / p.total)) : 0;
    if (known) { bar.querySelector('i').style.setProperty('--p', pct + '%'); bar.setAttribute('aria-valuenow', pct); }
    const bytes = el.querySelector('[data-ai-bytes]');
    if (bytes) bytes.textContent = known && (p.phase === 'download-ollama' || p.phase === 'pull-model')
      ? t('{a} de {b} MB', { a: mb(p.received), b: mb(p.total) }) + ` · ${pct} %` : '';
    const label = el.querySelector('[data-ai-label]');
    if (label) label.textContent = t(p.message || '');
  });
}

/**
 * Conecta una página: pinta los huecos `aiSlot`, atiende sus botones y llama a `onKind(nuevo, anterior)`
 * cuando cambia entre idle · running · error · ready. Devuelve la función para desconectar.
 */
export function aiMount(root, onKind) {
  ensureListening();
  let last = aiKind();
  if (!root._aiBound) {
    root._aiBound = true;
    root.addEventListener('click', (e) => {
      const b = e.target.closest('[data-act]');
      if (!b || !root.contains(b)) return;
      if (b.dataset.act === 'ai-start') { e.stopPropagation(); aiActivate(); }
      else if (b.dataset.act === 'ai-cancel') { e.stopPropagation(); aiCancel(); }
    });
  }
  const off = aiSubscribe(() => {
    if (!root.isConnected) { off(); return; }
    fill(root);
    const k = aiKind();
    if (k !== last) { const prev = last; last = k; if (onKind) onKind(k, prev); }
  });
  refreshAi();
  return off;
}
