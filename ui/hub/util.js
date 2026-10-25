// Utilidades del hub: i18n, formato, toasts, modales, menús. Sin dependencias.
import { icon } from '../shared/icons.js';
import EN from './en.js';

// ------------------------------------------------------------------ i18n
let lang = 'es';
export function setLang(l) {
  lang = l === 'en' ? 'en' : 'es';
  document.documentElement.lang = lang;
}
export const getLang = () => lang;
const locale = () => (lang === 'en' ? 'en-US' : 'es-ES');

/** t('Texto en español {n}', {n:3}) — la clave ES es el texto; EN es opcional. */
export function t(es, vars) {
  let s = (lang === 'en' && EN[es]) || es;
  if (vars) s = s.replace(/\{(\w+)\}/g, (m, k) => (k in vars ? vars[k] : m));
  return s;
}

export const esc = (s) =>
  String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

export const $ = (sel, root = document) => root.querySelector(sel);
export const $$ = (sel, root = document) => Array.from(root.querySelectorAll(sel));
export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
export function debounce(fn, ms = 250) {
  let h;
  const d = (...a) => { clearTimeout(h); h = setTimeout(() => fn(...a), ms); };
  d.cancel = () => clearTimeout(h);
  d.flush = (...a) => { clearTimeout(h); fn(...a); };
  return d;
}
// Guardados pendientes (p. ej. el autoguardado de una nota) que hay que vaciar antes de cerrar la ventana.
const pendingSaves = new Set();
/** Registra fn() -> Promise que guarda lo pendiente ya. Devuelve la función para quitarla. */
export function onBeforeClose(fn) { pendingSaves.add(fn); return () => pendingSaves.delete(fn); }
/** Lanza todos los guardados pendientes y espera a que acaben. Devuelve true si todos se guardaron; false si alguno
 *  falló (cada fn puede devolver false) o si pasó `ms`. Rompedor 23-oct, hub MEDIA 1: antes cortaba a los 1,5 s y la
 *  ventana se cerraba con el último `save_note` sin mandar cuando el anterior iba lento (base ocupada, antivirus).
 *  Ahora espera al guardado en vuelo y al pendiente; el tope de 30 s solo evita que una base colgada deje la ventana
 *  imposible de cerrar. */
export function flushPending(ms = 30000) {
  const all = [...pendingSaves].map((fn) => { try { return Promise.resolve(fn()).then((r) => r !== false, () => false); } catch (_) { return Promise.resolve(false); } });
  return Promise.race([Promise.all(all).then((rs) => rs.every(Boolean)), sleep(ms).then(() => false)]);
}
/** Texto sin tildes ni mayúsculas, para que «cancion» encuentre «Canción» (NFD + quitar los acentos combinados). */
export const fold = (s) => String(s ?? '').normalize('NFD').replace(/\p{M}/gu, '').toLowerCase();
export const errText = (e) => (typeof e === 'string' ? e : e && e.message ? e.message : t('Algo salió mal.'));

// ------------------------------------------------------------------ formato
export function formatCompact(n) {
  n = Number(n) || 0;
  const dec = lang === 'en' ? '.' : ',';
  if (n >= 1e6) return (n / 1e6).toFixed(1).replace('.', dec) + ' M';
  if (n >= 1000) return (n / 1000).toFixed(1).replace('.', dec) + ' K';
  return String(Math.round(n));
}
export const formatInt = (n) =>
  String(Math.round(Number(n) || 0)).replace(/\B(?=(\d{3})+(?!\d))/g, lang === 'en' ? ',' : '.');
export const formatDec = (n, d = 1) => new Intl.NumberFormat(locale(), { maximumFractionDigits: d, minimumFractionDigits: 0 }).format(Number(n) || 0);

export function formatTime(iso) {
  const d = new Date(iso);
  let h = d.getHours();
  const m = String(d.getMinutes()).padStart(2, '0');
  const ap = h >= 12 ? 'pm' : 'am';
  h = h % 12 || 12;
  return `${h}:${m} ${ap}`;
}
const startOfDay = (d) => { const x = new Date(d); x.setHours(0, 0, 0, 0); return x; };
export function dayLabel(iso) {
  const d = startOfDay(iso), today = startOfDay(new Date());
  const diff = Math.round((today - d) / 86400000);
  if (diff === 0) return t('Hoy');
  if (diff === 1) return t('Ayer');
  return d.toLocaleDateString(locale(), { day: 'numeric', month: 'long', year: 'numeric' });
}
export const dayKey = (iso) => { const d = new Date(iso); return `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`; };
export function shortDate(iso) {
  const d = new Date(iso), diff = Math.round((startOfDay(new Date()) - startOfDay(d)) / 86400000);
  if (diff === 0) return t('Hoy') + ' · ' + formatTime(iso);
  if (diff === 1) return t('Ayer');
  if (diff < 7) return d.toLocaleDateString(locale(), { weekday: 'long' });
  return d.toLocaleDateString(locale(), { day: 'numeric', month: 'short', year: d.getFullYear() === new Date().getFullYear() ? undefined : 'numeric' });
}
export const plural = (n, one, other) => (n === 1 ? one : other);

// ------------------------------------------------------------------ atajos
const KEY_LABEL = {
  Space: () => t('Espacio'), Escape: () => 'Esc', Enter: () => 'Enter', Tab: () => 'Tab', Backspace: () => t('Retroceso'),
  ArrowUp: () => '↑', ArrowDown: () => '↓', ArrowLeft: () => '←', ArrowRight: () => '→',
};
export function hotkeyParts(hk) {
  if (!hk) return [];
  const p = [];
  if (hk.ctrl) p.push('Ctrl');
  if (hk.win) p.push('Win');
  if (hk.alt) p.push('Alt');
  if (hk.shift) p.push('Shift');
  if (hk.key) p.push(KEY_LABEL[hk.key] ? KEY_LABEL[hk.key]() : String(hk.key).length === 1 ? String(hk.key).toUpperCase() : hk.key);
  return p;
}
export const hotkeyText = (hk) => hotkeyParts(hk).join(' + ');
export function keysHtml(hk) {
  return `<span class="keys">${hotkeyParts(hk).map((k) => `<kbd>${esc(k)}</kbd>`).join('<span class="plus">+</span>')}</span>`;
}
export const sameHotkey = (a, b) =>
  !!a && !!b && !!a.ctrl === !!b.ctrl && !!a.alt === !!b.alt && !!a.shift === !!b.shift && !!a.win === !!b.win && (a.key || null) === (b.key || null);

// ------------------------------------------------------------------ toasts
let toastBox;
export function toast(msg, opts = {}) {
  const { kind = 'ok', ms = 2600 } = opts;
  if (!toastBox) {
    toastBox = document.createElement('div');
    toastBox.className = 'toasts';
    toastBox.setAttribute('role', 'status');
    toastBox.setAttribute('aria-live', 'polite');
    document.body.appendChild(toastBox);
  }
  const el = document.createElement('div');
  el.className = `toast ${kind}`;
  el.innerHTML = `${icon(kind === 'err' ? 'circle-x' : kind === 'info' ? 'info' : 'circle-check', 18)}<span>${esc(msg)}</span>`;
  toastBox.appendChild(el);
  while (toastBox.children.length > 3) toastBox.firstChild.remove();
  setTimeout(() => { el.classList.add('out'); setTimeout(() => el.remove(), 320); }, ms);
}

// ------------------------------------------------------------------ modales
const stack = [];
const FOCUSABLE = 'button:not([disabled]),[href],input:not([disabled]),select:not([disabled]),textarea:not([disabled]),[tabindex]:not([tabindex="-1"])';

document.addEventListener('keydown', (e) => {
  const top = stack[stack.length - 1];
  if (!top) return;
  if (e.key === 'Escape' && !top.noEsc) {
    if (menuEl) return;
    e.preventDefault(); e.stopPropagation(); top.dismiss();
  } else if (e.key === 'Tab') {
    const f = $$(FOCUSABLE, top.modal).filter((x) => x.offsetParent !== null);
    if (!f.length) return;
    const first = f[0], last = f[f.length - 1];
    if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); }
    else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
    else if (!top.modal.contains(document.activeElement)) { e.preventDefault(); first.focus(); }
  }
}, true);

/** Abre un modal. opts: {title, subtitle, body, footer, width, cls, raw, onClose, noEsc, noX, height, dirty}.
 *  dirty() -> true si hay cambios sin guardar: Esc o un clic en el velo preguntan antes de descartarlos. */
export function openModal(opts) {
  const { title = '', subtitle = '', body = '', footer = '', width = 520, height, cls = '', raw = '', onClose, noEsc, noX, dirty } = opts;
  const prev = document.activeElement;
  const velo = document.createElement('div');
  velo.className = 'velo';
  const id = 'm' + Math.random().toString(36).slice(2, 8);
  velo.innerHTML = `<div class="modal ${cls}" role="dialog" aria-modal="true" ${title ? `aria-labelledby="${id}"` : ''}
      style="width:${typeof width === 'number' ? width + 'px' : width};${height ? `height:${height};` : ''}">
    <div class="modal-core">${raw || `
      <div class="modal-hd"><div><h2 id="${id}">${esc(title)}</h2>${subtitle ? `<p>${subtitle}</p>` : ''}</div>
        ${noX ? '' : `<button class="icon-btn" data-close data-x aria-label="${esc(t('Cerrar'))}">${icon('x', 18)}</button>`}</div>
      <div class="modal-bd">${body}</div>
      ${footer ? `<div class="modal-ft">${footer}</div>` : ''}`}
    </div></div>`;
  document.body.appendChild(velo);
  const modal = velo.firstElementChild;
  let closed = false, asking = false;
  const ctl = {
    el: velo, modal, core: modal.firstElementChild, noEsc,
    bd: $('.modal-bd', velo), ft: $('.modal-ft', velo),
    close(result) {
      if (closed) return;
      closed = true;
      const i = stack.indexOf(ctl); if (i >= 0) stack.splice(i, 1);
      velo.classList.add('out');
      setTimeout(() => velo.remove(), 210);
      if (prev && prev.focus && document.contains(prev)) { try { prev.focus({ preventScroll: true }); } catch (_) { /* */ } }
      if (onClose) onClose(result);
    },
    // Esc o clic en el velo: cierra, salvo que haya cambios sin guardar y no se confirme descartarlos.
    dismiss() {
      if (closed || asking) return;
      if (!dirty || !dirty()) { ctl.close(); return; }
      asking = true;
      confirmDialog({ title: t('¿Descartar los cambios?'), body: t('Lo que has escrito no se ha guardado.'), confirmText: t('Descartar') })
        .then((ok) => { asking = false; if (ok) ctl.close(); });
    },
  };
  stack.push(ctl);
  velo.addEventListener('mousedown', (e) => { if (e.target === velo && !noEsc) ctl.dismiss(); });
  // La × del encabezado pregunta antes de descartar (como Esc y el clic fuera); «Cancelar» y los demás [data-close] descartan a propósito.
  // (rompedor 23-oct, hub BAJA 3)
  velo.addEventListener('click', (e) => { const c = e.target.closest('[data-close]'); if (c) { if (c.hasAttribute('data-x')) ctl.dismiss(); else ctl.close(); } });
  requestAnimationFrame(() => {
    const auto = $('[autofocus],[data-autofocus]', modal) || $('input:not([type=hidden]),textarea', modal);
    (auto || $('.modal-ft .btn-primary,.modal-ft .btn', modal) || $('[data-close]', modal) || modal).focus({ preventScroll: true });
  });
  return ctl;
}
export const hasModal = () => stack.length > 0;

/** Diálogo de confirmación. Devuelve Promise<boolean>. */
export function confirmDialog({ title, body = '', confirmText, cancelText, danger = true }) {
  return new Promise((resolve) => {
    let res = false;
    const m = openModal({
      title, subtitle: body, width: 440,
      footer: `<button class="btn btn-ghost" data-close>${esc(cancelText || t('Cancelar'))}</button>
        <button class="btn ${danger ? 'btn-danger-solid' : 'btn-primary'}" data-ok>${esc(confirmText || t('Sí, eliminar'))}</button>`,
      onClose: () => resolve(res),
    });
    $('.modal-bd', m.el).remove();
    $('[data-ok]', m.el).addEventListener('click', () => { res = true; m.close(); });
    if (danger) $('[data-ok]', m.el).focus();
  });
}

// ------------------------------------------------------------------ menú contextual
let menuEl = null, menuOff = null;
export function closeMenu() {
  if (menuEl) { menuEl.remove(); menuEl = null; }
  if (menuOff) { menuOff(); menuOff = null; }
}
/** items: [{title}|{sep:true}|{label, icon, sel, danger, disabled, onClick}] */
export function openMenu(anchor, items, opts = {}) {
  closeMenu();
  const { align = 'right', up = false } = opts;
  const el = document.createElement('div');
  el.className = 'menu';
  el.setAttribute('role', 'menu');
  el.innerHTML = items.map((it, i) => {
    if (it.title) return `<div class="m-title">${esc(it.title)}</div>`;
    if (it.sep) return '<div class="m-sep"></div>';
    return `<button class="m-item ${it.danger ? 'danger' : ''} ${it.sel ? 'sel' : ''}" role="menuitem" data-i="${i}" ${it.disabled ? 'disabled' : ''}>
      ${it.icon ? icon(it.icon, 17) : ''}<span>${esc(it.label)}</span>${it.sel ? `<span class="sp">${icon('check', 16, 'chk')}</span>` : ''}</button>`;
  }).join('');
  document.body.appendChild(el);
  menuEl = el;
  const r = anchor.getBoundingClientRect();
  const w = el.offsetWidth, h = el.offsetHeight;
  let left = align === 'right' ? r.right - w : r.left;
  left = Math.max(8, Math.min(left, innerWidth - w - 8));
  let top = up ? r.top - h - 8 : r.bottom + 8;
  if (top + h > innerHeight - 8) top = Math.max(8, r.top - h - 8);
  if (top < 8) top = 8;
  el.style.left = left + 'px'; el.style.top = top + 'px';

  el.addEventListener('click', (e) => {
    const b = e.target.closest('.m-item'); if (!b) return;
    const it = items[+b.dataset.i];
    closeMenu();
    // El foco vuelve al botón que abrió el menú antes de ejecutar la acción: un modal que abra la acción lo recordará
    // (y al cancelarlo el foco no cae al <body>). [rompedor 23-oct, hub BAJA 5]
    if (anchor.isConnected && anchor.focus) anchor.focus({ preventScroll: true });
    if (it && it.onClick) it.onClick();
  });
  const btns = $$('.m-item:not([disabled])', el);
  const onKey = (e) => {
    if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); const a = anchor; closeMenu(); a.focus && a.focus(); }
    else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      const i = btns.indexOf(document.activeElement);
      const n = e.key === 'ArrowDown' ? (i + 1) % btns.length : (i - 1 + btns.length) % btns.length;
      btns[n].focus();
    } else if (e.key === 'Tab') closeMenu();
  };
  const onDown = (e) => { if (!el.contains(e.target) && !anchor.contains(e.target)) closeMenu(); };
  document.addEventListener('keydown', onKey, true);
  document.addEventListener('mousedown', onDown, true);
  window.addEventListener('resize', closeMenu);
  menuOff = () => {
    document.removeEventListener('keydown', onKey, true);
    document.removeEventListener('mousedown', onDown, true);
    window.removeEventListener('resize', closeMenu);
  };
  if (btns[0]) btns[0].focus({ preventScroll: true });
  return { close: closeMenu };
}
export const menuOpen = () => !!menuEl;
