// Pronto 2 · Hub (ventana principal): marco, navegación y eventos.
// Estructura y textos de Wispr Flow [docs/wispr-ui-spec.md §2]; aspecto de Pronto (Cristal etéreo).
import { icon } from './shared/icons.js';
import {
  invoke, listen, win, state, loadSettings, refreshEngine, refreshModel, applyModelProgress, refreshStats, notify, applySettingsEffects, IS_MOCK, subscribe,
  refreshUpdate, applyUpdateAvailable, applyUpdateProgress,
} from './hub/api.js';
import { t, esc, $, $$, toast, openMenu, closeMenu, hasModal, errText } from './hub/util.js';
import home from './hub/pages/home.js';
import insights from './hub/pages/insights.js';
import dictionary from './hub/pages/dictionary.js';
import snippets from './hub/pages/snippets.js';
import style from './hub/pages/style.js';
import transforms from './hub/pages/transforms.js';
import notes from './hub/pages/notes.js';
import meetings from './hub/pages/meetings.js';
import { initMeetingState, live as liveMeeting } from './hub/meeting.js';
import { openSettings } from './hub/pages/settings.js';
import { mountUpdateBanner } from './hub/update.js';

const PAGES = { home, meetings, insights, dictionary, snippets, style, transforms, notes };
const NAV = () => [
  { id: 'home', label: t('Dictado'), icon: 'mic' },
  { id: 'meetings', label: t('Notas de reunión'), icon: 'disc-2' },
  { id: 'insights', label: t('Estadísticas'), icon: 'chart-column' },
  { id: 'dictionary', label: t('Diccionario'), icon: 'book-a' },
  { id: 'snippets', label: t('Fragmentos'), icon: 'scissors' },
  { id: 'style', label: t('Estilo'), icon: 'type' },
  { id: 'transforms', label: t('Transforms'), icon: 'wand-sparkles' },
  { id: 'notes', label: t('Bloc de notas'), icon: 'notebook-pen' },
];
const ALIAS = {
  dictation: 'home', dictado: 'home', history: 'home', historial: 'home', inicio: 'home',
  notetaker: 'meetings', meeting: 'meetings', reuniones: 'meetings', 'notas de reunión': 'meetings', 'notas de reunion': 'meetings',
  stats: 'insights', estadisticas: 'insights', 'estadísticas': 'insights',
  diccionario: 'dictionary', words: 'dictionary', fragmentos: 'snippets', estilo: 'style',
  scratchpad: 'notes', notas: 'notes', 'bloc de notas': 'notes', notes: 'notes',
};
const SETTINGS_ALIAS = {
  shortcuts: ['general', 'shortcuts'], atajos: ['general', 'shortcuts'], hotkeys: ['general', 'shortcuts'],
  microphone: ['general', 'mic'], mic: ['general', 'mic'], microfono: ['general', 'mic'], 'micrófono': ['general', 'mic'],
  languages: ['general', 'langs'], idiomas: ['general', 'langs'],
  meetings: ['meetings'], reuniones: ['meetings'], notetaker: ['meetings'],
  general: ['general'], system: ['system'], sistema: ['system'], experimental: ['experimental'],
  data: ['data'], privacy: ['data'], privacidad: ['data'], datos: ['data'],
};

let current = { id: null, cleanup: null };
const lsGet = (k) => { try { return localStorage.getItem(k); } catch (_) { return null; } };
const lsSet = (k, v) => { try { localStorage.setItem(k, v); } catch (_) { /* */ } };

const app = $('#app');

// ------------------------------------------------------------------ marco
function engineView() {
  const e = state.engine;
  if (e.error) return { cls: 'err', text: t('Motor con error'), dot: 'err' };
  if (e.loading || !e.ready) return { cls: 'vio', text: t('Cargando el motor…'), dot: 'pulse' };
  return { cls: 'ok', text: t('Motor listo'), dot: 'ok' };
}

function shellHtml() {
  const ev = engineView();
  return `
  <header class="hdr" data-tauri-drag-region>
    <button class="hbtn" id="sb-toggle" aria-label="${esc(t('Plegar o desplegar la barra lateral'))}" title="${esc(t('Plegar o desplegar la barra lateral'))}">${icon('panel-left', 19)}</button>
    <div class="hdr-fill" data-tauri-drag-region></div>
    <div class="dict-pill" id="dict-pill" hidden role="status"></div>
    <div class="wc">
      <button class="wc-b" data-wc="min" aria-label="${esc(t('Minimizar'))}" title="${esc(t('Minimizar'))}">${icon('minus-win', 20)}</button>
      <button class="wc-b" data-wc="max" aria-label="${esc(t('Maximizar'))}" title="${esc(t('Maximizar'))}">${icon('square-win', 20)}</button>
      <button class="wc-b close" data-wc="close" aria-label="${esc(t('Cerrar'))}" title="${esc(t('Cerrar'))}">${icon('x-win', 20)}</button>
    </div>
  </header>
  <div class="body">
    <aside class="sb" aria-label="${esc(t('Navegación'))}">
      <button class="brand" data-nav="home" aria-label="Pronto"><img src="assets/brand/pronto_mark.png" alt="" width="24" height="24"><span class="lbl">Pronto</span></button>
      <nav class="nav-list">${NAV().map((n) => `<button class="nav" data-nav="${n.id}">${icon(n.icon, 20)}<span class="lbl">${esc(n.label)}</span>${n.id === 'meetings' ? '<i class="rec-dot nav-rec" id="nav-rec" hidden></i>' : ''}</button>`).join('')}</nav>
      <div class="sb-fill"></div>
      <button class="eng tarj" id="eng-card" data-act="open-data" aria-label="${esc(t('100 % local · Tu voz no sale del equipo'))}">
        <span class="in">
          <span class="eng-hd"><i class="eng-ic">${icon('shield-check', 17)}</i><b>${esc(t('100 % local'))}</b></span>
          <span class="eng-tx">${esc(t('Tu voz no sale del equipo'))}</span>
          <span class="badge ${ev.cls}" id="eng-badge"><i class="dot ${ev.dot}"></i>${esc(ev.text)}</span>
        </span></button>
      <button class="eng-mini" id="eng-mini" data-act="open-data" aria-label="${esc(t('100 % local · Tu voz no sale del equipo'))}">${icon('shield-check', 20)}<i class="dot ${ev.dot}"></i></button>
      <div class="sb-div"></div>
      <button class="nav" data-act="settings">${icon('settings', 20)}<span class="lbl">${esc(t('Configuración'))}</span></button>
      <button class="nav" data-act="help" aria-haspopup="menu">${icon('circle-help', 20)}<span class="lbl">${esc(t('Ayuda'))}</span></button>
    </aside>
    <main class="main"><div class="upd-slot" id="upd-slot" hidden></div><div class="page-shell carcasa"><div class="nucleo page-core"><div class="page-scroll" id="page" tabindex="-1"></div></div></div></main>
  </div>`;
}

function updateEngineCard() {
  const ev = engineView();
  const b = $('#eng-badge'); if (b) { b.className = `badge ${ev.cls}`; b.innerHTML = `<i class="dot ${ev.dot}"></i>${esc(ev.text)}`; }
  const m = $('#eng-mini .dot'); if (m) m.className = `dot ${ev.dot}`;
  const c = $('#eng-card'); if (c) c.dataset.state = ev.cls;
  const mm = $('#eng-mini'); if (mm) mm.dataset.state = ev.cls;
}

function updateRecDot() {
  const d = $('#nav-rec'); if (!d) return;
  d.hidden = !liveMeeting.active;
  const b = d.closest('.nav'); if (b) b.classList.toggle('is-rec', !!liveMeeting.active);
}

function applySidebar(collapsed) {
  app.classList.toggle('sb-collapsed', collapsed);
  $$('.sb .nav, .sb .brand', app).forEach((b) => {
    const l = $('.lbl', b);
    if (collapsed && l) b.title = l.textContent; else b.removeAttribute('title');
  });
  const tg = $('#sb-toggle'); if (tg) tg.setAttribute('aria-expanded', String(!collapsed));
}

function buildShell() {
  const collapsed = lsGet('pronto.sb') === '1';
  app.innerHTML = shellHtml();
  applySidebar(collapsed);
  updateEngineCard();
  updateRecDot();
  highlightNav();
  mountUpdateBanner($('#upd-slot'));
}

function highlightNav() {
  $$('.sb [data-nav]', app).forEach((b) => {
    if (b.classList.contains('brand')) return;
    const on = b.dataset.nav === current.id;
    b.classList.toggle('active', on);
    if (on) b.setAttribute('aria-current', 'page'); else b.removeAttribute('aria-current');
  });
}

// ------------------------------------------------------------------ navegación
const ctx = { navigate, openSettings };

function navigate(id, { force = false } = {}) {
  id = String(id || 'home').toLowerCase();
  id = ALIAS[id] || id;
  if (!PAGES[id]) id = 'home';
  if (id === current.id && !force) { const p = $('#page'); if (p) p.scrollTo({ top: 0, behavior: 'smooth' }); return; }
  closeMenu();
  if (current.cleanup) { try { current.cleanup(); } catch (e) { console.error(e); } }
  current = { id, cleanup: null };
  highlightNav();
  // Contenedor nuevo en cada visita: las páginas cuelgan sus listeners de `root` y, si se
  // reutilizara, se acumularían (el último clic lo atendería una página ya desmontada).
  const prevPage = $('#page');
  const page = prevPage.cloneNode(false);
  prevPage.replaceWith(page);
  try { current.cleanup = PAGES[id].mount(page, ctx) || null; }
  catch (e) { console.error(e); page.innerHTML = `<div class="page-inner"><div class="aviso rojo"><div class="in"><span class="ic-l">${icon('triangle-alert', 20)}</span><div class="tx"><b>${esc(t('Algo salió mal.'))}</b><span>${esc(errText(e))}</span></div></div></div></div>`; }
}

function openSettingsFrom(raw, extra = {}) {
  const key = String(raw || 'general').toLowerCase();
  const [tab, dialog] = SETTINGS_ALIAS[key] || ['general'];
  openSettings(tab, { dialog: extra.dialog || dialog });
}

// ------------------------------------------------------------------ acciones del marco
function helpMenu(anchor) {
  openMenu(anchor, [
    { title: t('Esenciales') },
    { label: t('Atajos'), icon: 'keyboard', onClick: () => openSettings('general', { dialog: 'shortcuts' }) },
    { label: t('Micrófono'), icon: 'mic', onClick: () => openSettings('general', { dialog: 'mic' }) },
    { label: t('Idiomas'), icon: 'globe', onClick: () => openSettings('general', { dialog: 'langs' }) },
  ], { align: 'left', up: true });
}

function toggleSidebar() {
  const c = !app.classList.contains('sb-collapsed');
  lsSet('pronto.sb', c ? '1' : '0');
  applySidebar(c);
}

async function windowAction(kind) {
  try {
    const w = win();
    if (kind === 'min') await w.minimize();
    else if (kind === 'max') await w.toggleMaximize();
    else if (kind === 'close') await w.hide(); // cerrar = ocultar a la bandeja
  } catch (e) { console.error(e); }
}

app.addEventListener('click', (e) => {
  const wc = e.target.closest('[data-wc]');
  if (wc) { windowAction(wc.dataset.wc); return; }
  if (e.target.closest('#sb-toggle')) { toggleSidebar(); return; }
  const nav = e.target.closest('[data-nav]');
  if (nav) { navigate(nav.dataset.nav); return; }
  const act = e.target.closest('.sb [data-act]');
  if (!act) return;
  if (act.dataset.act === 'settings') openSettings('general');
  else if (act.dataset.act === 'help') helpMenu(act);
  else if (act.dataset.act === 'open-data') openSettings('data');
});

// Atajos de teclado globales del hub
document.addEventListener('keydown', (e) => {
  if (hasModal()) return;
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'f') {
    const inp = $('#page [data-search-input]');
    if (inp) { e.preventDefault(); inp.focus(); inp.select(); return; }
    const btn = $('#page [data-act=toggle-search]');
    if (btn) { e.preventDefault(); btn.click(); }
  }
});

// ------------------------------------------------------------------ estado del dictado en la cabecera
function renderDictPill() {
  const el = $('#dict-pill'); if (!el) return;
  const s = state.dictation.state;
  const map = {
    listening: ['rojo', t('Escuchando…')], handsfree: ['rojo', t('Manos libres')],
    processing: ['vio', t('Transcribiendo…')], polishing: ['vio', t('Puliendo…')],
  };
  const m = map[s];
  el.hidden = !m;
  if (m) el.innerHTML = `<span class="pill-s ${m[0]}"><i class="dot pulse"></i>${esc(m[1])}</span>`;
}

// ------------------------------------------------------------------ arranque
async function rebuildAll() {
  const id = current.id || 'home';
  if (current.cleanup) { try { current.cleanup(); } catch (_) { /* */ } }
  current = { id: null, cleanup: null };
  buildShell();
  navigate(id, { force: true });
}

async function boot() {
  try {
    await loadSettings();
  } catch (e) {
    app.innerHTML = `<div class="fatal"><div class="aviso rojo"><div class="in"><span class="ic-l">${icon('triangle-alert', 22)}</span><div class="tx"><b>${esc(t('No se pudo cargar Pronto'))}</b><span>${esc(errText(e))}</span></div>
      <button class="btn btn-secondary btn-sm" id="retry">${esc(t('Reintentar'))}</button></div></div></div>`;
    $('#retry').addEventListener('click', () => location.reload());
    return;
  }
  await Promise.all([refreshEngine(), refreshModel(), refreshStats(), initMeetingState(), refreshUpdate()]);
  buildShell();
  subscribe((what) => { if (what === 'meeting') updateRecDot(); });

  const qs = new URLSearchParams(location.search);
  if (qs.get('meeting')) ctx.openMeeting = Number(qs.get('meeting'));
  navigate(qs.get('page') || 'home');
  if (qs.get('settings')) openSettingsFrom(qs.get('settings'));

  listen('hub://navigate', (p) => { const page = (p && p.page) || 'home'; if (p && p.id != null) ctx.openMeeting = Number(p.id); if (String(page).toLowerCase() === 'settings') openSettings('general'); else navigate(page, { force: ctx.openMeeting != null }); });
  listen('hub://settings', (p) => openSettingsFrom(p && p.tab));
  listen('engine://status', (e) => { state.engine = e; updateEngineCard(); notify('engine'); });
  listen('model://progress', applyModelProgress);
  listen('update://available', applyUpdateAvailable);
  listen('update://progress', applyUpdateProgress);
  listen('settings://changed', (s) => {
    if (!s) return;
    const langChanged = state.settings && s.appLanguage !== state.settings.appLanguage;
    state.settings = s; applySettingsEffects(s); notify('settings-ext');
    if (langChanged) rebuildAll();
  });
  listen('dictation://state', (d) => { state.dictation = d || { state: 'idle' }; renderDictPill(); });

  document.addEventListener('pronto:lang', () => rebuildAll());
  document.addEventListener('pronto:history-cleared', () => { if (current.id === 'home') navigate('home', { force: true }); refreshStats(); });
}

boot();

if (IS_MOCK) console.info('[Pronto hub] backend simulado (mock). Parámetros: ?page=insights ?settings=system ?theme=light ?ollama=off ?empty=1 ?engine=error ?model=missing ?model=error ?update=available ?update=error ?update=none');
