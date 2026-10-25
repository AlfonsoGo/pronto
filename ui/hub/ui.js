// Piezas de marcado compartidas por las páginas del hub.
import { icon } from '../shared/icons.js';
import { esc, t, fold } from './util.js';

export function pageHeader(title, { sub = '', actions = '', badge = '', titleHtml = '' } = {}) {
  return `<div class="ph"><div class="ph-t"><h1 class="ptitle">${titleHtml || esc(title)}${badge}</h1>${sub ? `<p class="psub">${sub}</p>` : ''}</div>
    ${actions ? `<div class="ph-actions">${actions}</div>` : ''}</div>`;
}

/** tabs: [{id,label,badge?}] */
export function tabsHtml(tabs, active, tools = '') {
  return `<div class="tabs-row"><div class="tabs" role="tablist">${tabs.map((x) =>
    `<button class="tab" role="tab" data-tab="${x.id}" aria-selected="${x.id === active}" tabindex="${x.id === active ? 0 : -1}">${esc(x.label)}${x.badge ? `<span class="badge vio">${esc(x.badge)}</span>` : ''}</button>`).join('')}</div>${tools ? `<div class="tabs-tools">${tools}</div>` : ''}</div>`;
}

/** Navegación con flechas dentro de un tablist. */
export function tabsKeys(root, onSelect) {
  root.addEventListener('keydown', (e) => {
    const tab = e.target.closest('[role=tab]');
    if (!tab || (e.key !== 'ArrowRight' && e.key !== 'ArrowLeft')) return;
    const all = Array.from(root.querySelectorAll('[role=tab]'));
    const i = all.indexOf(tab);
    const n = all[(i + (e.key === 'ArrowRight' ? 1 : -1) + all.length) % all.length];
    e.preventDefault();
    onSelect(n.dataset.tab);
    requestAnimationFrame(() => { const x = root.querySelector(`[data-tab="${n.dataset.tab}"]`); if (x) x.focus(); });
  });
}

export const switchHtml = (key, on, label) =>
  `<button class="switch" role="switch" aria-checked="${!!on}" aria-label="${esc(label)}" data-switch="${esc(key)}"></button>`;

export const selectHtml = (key, options, value, { label = '' } = {}) =>
  `<div class="select"><select data-select="${esc(key)}" ${label ? `aria-label="${esc(label)}"` : ''}>${options.map((o) =>
    `<option value="${esc(o.value)}" ${String(o.value) === String(value) ? 'selected' : ''}>${esc(o.label)}</option>`).join('')}</select>${icon('chevron-down', 16)}</div>`;

export function emptyState(ic, title, body = '', action = '') {
  return `<div class="empty"><div class="empty-ic">${icon(ic, 26)}</div><h3>${esc(title)}</h3>${body ? `<p>${esc(body)}</p>` : ''}${action}</div>`;
}

/** Banner hero. opts: {titleHtml, body, cta, ctaAct, deco, closable, extra} */
export function heroBanner({ titleHtml, body = '', cta = '', ctaAct = '', closeAct = '', deco = '', extra = '', cls = '' }) {
  return `<div class="hero tarj ${cls}"><div class="in">
    <div class="hero-glow" aria-hidden="true"></div>
    <div class="hero-tx"><h2>${titleHtml}</h2>${body ? `<p>${body}</p>` : ''}${extra}
      ${cta ? `<button class="pill" data-act="${ctaAct}"><span>${esc(cta)}</span><span class="circ">${icon('arrow-up-right', 16)}</span></button>` : ''}</div>
    <div class="hero-deco" aria-hidden="true">${deco}</div>
    ${closeAct ? `<button class="icon-btn hero-x" data-act="${closeAct}" aria-label="${esc(t('Cerrar'))}">${icon('x', 16)}</button>` : ''}
  </div></div>`;
}

export const waveDeco = () => `<div class="wave">${Array.from({ length: 11 }, (_, i) => `<i style="--i:${i}"></i>`).join('')}</div>`;

/** Etiqueta con el nombre del tono de estilo. */
// «Casual» no es castellano llano (Rosario, 24-oct-2026: «¿Casual? ¿Qué es eso?»): «Cercano».
export const toneName = (tone) => ({ formal: t('Formal'), casual: t('Cercano'), veryCasual: t('Muy cercano'), excited: t('Con exclamaciones') }[tone] || tone);

// Con la palabra «Buscar» escrita: la lupa sola pasaba por adorno.
export const searchBtn = (act = 'toggle-search') =>
  `<button class="btn btn-ghost btn-sm search-btn" data-act="${act}">${icon('search', 16)}${esc(t('Buscar'))}</button>`;

export function searchBar({ placeholder, value = '', act = 'clear-search' }) {
  return `<div class="searchbar"><span class="sb-ic">${icon('search', 17)}</span>
    <input class="sb-in" data-search-input type="search" placeholder="${esc(placeholder)}" value="${esc(value)}" aria-label="${esc(placeholder)}" autocomplete="off" spellcheck="false">
    <button class="icon-btn" data-act="${act}" aria-label="${esc(t('Cerrar búsqueda'))}">${icon('x', 16)}</button></div>`;
}

/** Resalta la consulta: busca sobre el texto sin escapar y escapa trozo a trozo (así no parte «&amp;»).
 *  Rompedor 23-oct, hub BAJA 9: la búsqueda no distingue tildes ni mayúsculas («cancion» marca «Canción»); se busca en el
 *  texto plegado (`fold`) y se guarda de qué trozo del original viene cada letra para marcar el original, no el plegado. */
export function highlight(text, q) {
  const s = String(text ?? '');
  const fq = fold(q);
  if (!fq) return esc(s);
  let f = '';
  const ini = [], fin = [];   // por cada carácter plegado: dónde empieza y acaba en el original
  let i = 0;
  for (const ch of s) {
    const g = fold(ch);
    for (let k = 0; k < g.length; k++) { ini.push(i); fin.push(i + ch.length); }
    f += g; i += ch.length;
  }
  let out = '', pos = 0, at = f.indexOf(fq);
  while (at >= 0) {
    const a = ini[at];
    let b = fin[at + fq.length - 1];
    while (b < s.length && /\p{M}/u.test(s[b])) b++;   // el acento combinado que sigue a la última letra también se marca
    if (a >= pos) { out += esc(s.slice(pos, a)) + `<mark>${esc(s.slice(a, b))}</mark>`; pos = b; }
    at = f.indexOf(fq, at + fq.length);
  }
  return out + esc(s.slice(pos));
}
