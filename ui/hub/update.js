// Tarjeta de «Pronto X.Y.Z disponible» (Contrato v7 · actualizaciones). Vive en el marco del hub, encima de
// la página, así que se ve en todas las páginas. Las notas de la release llegan como texto plano y aquí
// se escapan igualmente (nada del texto de GitHub se interpreta como HTML).
import { icon } from '../shared/icons.js';
import { state, subscribe, installUpdate, dismissUpdate } from './api.js';
import { t, esc, formatDec } from './util.js';

const SHOWN = ['available', 'downloading', 'verifying', 'installing', 'error'];
const mb = (n) => formatDec((Number(n) || 0) / 1048576, 1);

let slot = null;
let expanded = false;
let lastKey = '';

function progressHtml(u) {
  if (u.state === 'downloading') {
    const p = u.percent || 0;
    return `<div class="mc-prog" role="status"><div class="mc-bar" role="progressbar" aria-label="${esc(t('Descarga de la actualización'))}" aria-valuemin="0" aria-valuemax="100" aria-valuenow="${p}"><i style="--p:${p}%"></i></div>
      <div class="mc-meta"><b>${p} %</b><span>${esc(t('{a} de {b} MB', { a: mb(u.doneBytes), b: mb(u.totalBytes) }))}</span></div></div>`;
  }
  const text = u.state === 'verifying' ? t('Verificando la descarga…') : t('Instalando… Pronto se cierra un momento y se abre solo.');
  return `<div class="mc-prog" role="status"><div class="mc-bar" role="progressbar" aria-label="${esc(text)}" aria-valuemin="0" aria-valuemax="100" aria-valuenow="100"><i class="busy" style="--p:100%"></i></div>
    <div class="mc-meta"><span>${esc(text)}</span></div></div>`;
}

function html() {
  const u = state.update, info = u.info;
  if (!info || !SHOWN.includes(u.state)) return '';
  const calm = u.state === 'available' || u.state === 'error';
  if (calm && u.dismissed === info.version) return '';

  const busy = !calm;
  const notes = String(info.notes || '').trim();
  const long = notes.length > 150 || notes.split('\n').length > 3;
  const sub = t('Tienes la {cur}. Se instala sola y Pronto se vuelve a abrir.', { cur: esc(u.currentVersion || '') });
  const failed = u.state === 'error' || (u.error && calm);
  const err = failed && u.error
    ? `<div class="aviso rojo" role="alert"><div class="in"><span class="ic-l">${icon('triangle-alert', 18)}</span><div class="tx"><b>${esc(t('No se pudo actualizar'))}</b><span>${esc(u.error)}</span></div></div></div>`
    : '';
  const buttons = busy ? '' : `<div class="upd-btns">
      <button class="btn btn-primary btn-sm" data-act="upd-install">${icon(failed ? 'refresh-cw' : 'download', 15)}${esc(failed ? t('Reintentar') : t('Actualizar ahora'))}</button>
      <button class="btn btn-ghost btn-sm" data-act="upd-later">${esc(t('Más tarde'))}</button></div>`;

  return `<section class="upd tarj" aria-label="${esc(t('Actualización disponible'))}"><div class="in">
    <span class="mc-ic" aria-hidden="true">${icon('download', 22)}</span>
    <div class="upd-tx">
      <h2>${esc(t('Pronto {v} disponible', { v: info.version }))}</h2>
      <p class="upd-sub">${sub}</p>
      ${notes && !busy ? `<div class="upd-notes${expanded ? ' open' : ''}" id="upd-notes">${esc(notes)}</div>
        ${long ? `<button class="upd-more" data-act="upd-more" aria-expanded="${expanded}" aria-controls="upd-notes">${esc(expanded ? t('Ver menos') : t('Ver novedades'))}</button>` : ''}` : ''}
      ${busy ? progressHtml(u) : err}
    </div>
    ${buttons}
  </div></section>`;
}

export function renderUpdateBanner() {
  if (!slot) return;
  const h = html();
  const key = h;
  if (key === lastKey) return;
  lastKey = key;
  slot.innerHTML = h;
  slot.hidden = !h;
}

/** Engancha la tarjeta al hueco `#upd-slot` del marco (se llama en cada reconstrucción del marco). */
export function mountUpdateBanner(el) {
  slot = el;
  lastKey = '';
  if (!slot) return;
  slot.addEventListener('click', (e) => {
    const b = e.target.closest('[data-act]');
    if (!b) return;
    if (b.dataset.act === 'upd-install') installUpdate();
    else if (b.dataset.act === 'upd-later') dismissUpdate();
    else if (b.dataset.act === 'upd-more') { expanded = !expanded; lastKey = ''; renderUpdateBanner(); }
  });
  renderUpdateBanner();
}

subscribe((what) => { if (what === 'update') renderUpdateBanner(); });
