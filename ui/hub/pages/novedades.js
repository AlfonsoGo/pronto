// Novedades: lo que trae cada versión (fuente única en ../novedades.js) y la tarjeta «Lo nuevo» que sale una vez tras actualizar.
// Va dentro de la app, sin red (regla 1): lo que ves es lo que trae la versión que tienes instalada.
import { icon } from '../../shared/icons.js';
import { state } from '../api.js';
import { t, esc, $, getLang, openModal, toast } from '../util.js';
import { pageHeader } from '../ui.js';
import { RELEASES, releaseOf, hasHighlights } from '../novedades.js';

const SEEN_KEY = 'pronto.novedades.vista';
const L = (o) => (getLang() === 'en' && o.en ? o.en : o.es);

function monthName(month) {
  const d = new Date(`${month}-01T00:00:00`);
  return Number.isNaN(d.getTime()) ? '' : d.toLocaleDateString(getLang() === 'en' ? 'en-US' : 'es-ES', { month: 'long', year: 'numeric' });
}

// ------------------------------------------------------------------ visto / sin ver
function readSeen() { try { return { ok: true, v: localStorage.getItem(SEEN_KEY) }; } catch (_) { return { ok: false, v: null }; } }
export function markSeen(version = state.app?.version) {
  if (!version) return false;
  try { localStorage.setItem(SEEN_KEY, version); return localStorage.getItem(SEEN_KEY) === version; } catch (_) { return false; }
}
/** La versión instalada trae destacados que este usuario aún no ha visto (punto en el menú). */
export function unseen() {
  const v = state.app?.version;
  return !!v && hasHighlights(v) && readSeen().v !== v;
}

// ------------------------------------------------------------------ marcado
// `compact`: la tarjeta «Lo nuevo» enseña la línea corta (`breve`) para que quepan todas sin desplazarse.
function cardHtml(h, compact = false) {
  const x = L(h);
  const go = h.probar ? `data-try="${esc(h.key)}"` : '';
  const text = compact && x.breve ? x.breve.charAt(0).toUpperCase() + x.breve.slice(1) + '.' : x.d;
  return `<li class="nv-card${compact ? ' nv-card-sm' : ''}">
    <span class="nv-ic" aria-hidden="true">${icon(h.icon, 20)}</span>
    <div class="nv-card-tx">
      <h3>${esc(x.t)}${h.beta ? ` <span class="badge warn">${esc(t('Beta'))}</span>` : ''}</h3>
      <p>${esc(text)}</p>
      ${go ? `<button class="btn btn-secondary btn-sm" ${go} aria-label="${esc(t('Probar: {f}', { f: x.t }))}">${esc(t('Probar'))}${icon('arrow-up-right', 14)}</button>` : ''}
    </div></li>`;
}

function listHtml(title, ic, items) {
  if (!items.length) return '';
  return `<div class="nv-block"><h3>${icon(ic, 16)}${esc(title)}</h3><ul class="nv-list">${items.map((i) => `<li>${esc(L(i))}</li>`).join('')}</ul></div>`;
}

function releaseBody(r) {
  const x = L(r);
  return `${x.intro ? `<p class="nv-intro">${esc(x.intro)}</p>` : ''}
    ${r.highlights.length ? `<ul class="nv-grid">${r.highlights.map((h) => cardHtml(h)).join('')}</ul>` : ''}
    <div class="nv-cols">${listHtml(t('Mejoras'), 'sparkles', r.improved)}${listHtml(t('Arreglos'), 'circle-check', r.fixed)}</div>`;
}

function metaHtml(r) {
  const mine = r.version === state.app?.version;
  return `<span class="badge vio">${esc(t('Versión {v}', { v: r.version }))}</span>${mine ? `<span class="badge ok">${esc(t('La que tienes'))}</span>` : ''}<span class="nv-date">${esc(monthName(r.month))}</span>`;
}

function pageHtml() {
  const [last, ...old] = RELEASES;
  const id = (r) => `nv-${r.version.replace(/\./g, '-')}`;
  return `<div class="page-inner nv">
    ${pageHeader(t('Novedades'), { sub: esc(t('Lo que ha cambiado en cada versión de Pronto.')) })}
    <section class="tarj nv-rel" aria-labelledby="${id(last)}"><div class="in">
      <div class="nv-meta">${metaHtml(last)}</div>
      <h2 id="${id(last)}">${esc(L(last).title)}</h2>
      ${releaseBody(last)}
    </div></section>
    ${old.length ? `<h2 class="nv-old-h">${esc(t('Versiones anteriores'))}</h2>` : ''}
    ${old.map((r) => `<details class="nv-old caja"><summary>${icon('chevron-right', 16)}<span class="nv-old-v">${esc(r.version)}</span><span class="nv-old-t">${esc(L(r).title)}</span><span class="nv-date">${esc(countText(r))}</span></summary>
      <div class="nv-old-bd">${releaseBody(r)}</div></details>`).join('')}
  </div>`;
}

// Lo que trae una versión anterior, en el desplegable (todas salieron el mismo mes: la fecha no las distinguía).
function countText(r) {
  const n = r.highlights.length + r.improved.length, f = r.fixed.length;
  return [n ? t(n === 1 ? '1 novedad' : '{n} novedades', { n }) : '', f ? t(f === 1 ? '1 arreglo' : '{n} arreglos', { n: f }) : ''].filter(Boolean).join(' · ');
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
// «Probar» deja en la acción, no solo en la pantalla (prueba con un primerizo, 25-oct-2026: llegaba a «Mis palabras» y no
// sabía qué pulsar): abre lo que haga falta (`click`), resalta el control (`spot`) y explica el siguiente paso (`hint`).
async function tryFeature(btn, ctx) {
  const h = byKey(btn.dataset.try);
  if (!h || !h.probar) return;
  const p = h.probar;
  if (p.page) await ctx.navigate(p.page); else ctx.openSettings(p.settings);
  await sleep(450);
  if (p.click) { const c = $(p.click); if (c) { c.click(); await sleep(350); } }
  const spot = p.spot && $(p.spot);
  if (spot) {
    spot.classList.add('nv-spot');
    spot.scrollIntoView({ block: 'center', behavior: 'smooth' });
    try { spot.focus({ preventScroll: true }); } catch (_) { /* */ }
    setTimeout(() => spot.classList.remove('nv-spot'), 4200);
  }
  if (p.hint) toast(L(p.hint), { kind: 'info', ms: 7000 });
}

const byKey = (k) => { const [v, i] = String(k || '').split('#'); return releaseOf(v)?.highlights[Number(i)] || null; };

// ------------------------------------------------------------------ «Lo nuevo» (una vez por versión)
/** Tarjeta con los destacados de la versión instalada. `ctx` = { navigate, openSettings } del hub. */
export function openWhatsNew(ctx, version = state.app?.version) {
  const r = releaseOf(version);
  if (!r || !r.highlights.length) return null;
  markSeen(version);
  document.dispatchEvent(new CustomEvent('pronto:novedades-vistas'));
  const m = openModal({
    title: t('Lo nuevo en Pronto {v}', { v: r.version }),
    subtitle: esc(L(r).title),
    width: 640,
    cls: 'nv-modal',
    body: `<ul class="nv-grid nv-grid-1">${r.highlights.map((h) => cardHtml(h, true)).join('')}</ul>`,
    footer: `<button class="btn btn-ghost" data-all>${esc(t('Ver todas las novedades'))}</button><button class="btn btn-primary" data-close>${esc(t('Entendido'))}</button>`,
  });
  m.el.addEventListener('click', (e) => {
    const b = e.target.closest('[data-try],[data-all]');
    if (!b) return;
    m.close();
    if (b.hasAttribute('data-all')) ctx.navigate('novedades'); else tryFeature(b, ctx);
  });
  return m;
}

/**
 * Tras actualizar, al abrir el panel: si la versión trae destacados que no se han visto, se enseñan una vez. Quien acaba de
 * instalar (sin dictados ni reuniones) no tiene «novedades»: se marca como visto sin enseñar nada. Si no se puede guardar que
 * ya se vio, no se enseña (saldría en cada arranque). Devuelve true si la enseñó.
 */
export function maybeShowWhatsNew(ctx, { force = false, veteran = false, blocked = false } = {}) {
  const v = state.app?.version;
  if (!v || !hasHighlights(v)) return false;
  const seen = readSeen();
  if (!seen.ok || (seen.v === v && !force)) return false;
  if (!veteran && !force) { markSeen(v); return false; }
  if (blocked) return false; // otra ventana encima: el punto del menú lo recuerda
  if (!force && !markSeen(v)) return false;
  return !!openWhatsNew(ctx, v);
}

export default {
  id: 'novedades',
  mount(root, ctx) {
    root.innerHTML = pageHtml();
    if (markSeen()) document.dispatchEvent(new CustomEvent('pronto:novedades-vistas'));
    const onClick = (e) => {
      const b = e.target.closest('[data-try]');
      if (b) tryFeature(b, ctx);
    };
    root.addEventListener('click', onClick);
    return () => root.removeEventListener('click', onClick);
  },
};
