// Pronto 2 · Hub (ventana principal): marco, navegación y eventos.
// Estructura y textos de Wispr Flow [docs/wispr-ui-spec.md §2]; aspecto de Pronto (Cristal etéreo).
import { icon } from './shared/icons.js';
import {
  invoke, listen, win, state, loadSettings, refreshEngine, refreshModel, applyModelProgress, refreshStats, notify, applySettingsEffects, IS_MOCK, subscribe,
  refreshUpdate, applyUpdateAvailable, applyUpdateProgress,
} from './hub/api.js';
import { t, esc, $, $$, toast, closeMenu, hasModal, errText, flushPending, openModal, keysHtml } from './hub/util.js';
import home from './hub/pages/home.js';
import insights from './hub/pages/insights.js';
import dictionary from './hub/pages/dictionary.js';
import snippets from './hub/pages/snippets.js';
import style from './hub/pages/style.js';
import transforms from './hub/pages/transforms.js';
import notes from './hub/pages/notes.js';
import meetings from './hub/pages/meetings.js';
import novedades, { maybeShowWhatsNew, unseen as novedadesSinVer } from './hub/pages/novedades.js';
import { initMeetingState, live as liveMeeting } from './hub/meeting.js';
import { openSettings, openSettingsDialog } from './hub/pages/settings.js';
import { mountUpdateBanner } from './hub/update.js';

const PAGES = { home, meetings, insights, dictionary, snippets, style, transforms, notes, novedades };
// Menú pensado para quien no sabe de informática (pruebas con tres primerizos, 24-oct-2026): solo lo de la primera semana.
// Estadísticas se abre desde la tarjeta de cifras de Dictado y Transforms desde Configuración → IA en tu PC.
const NAV = () => [
  { id: 'home', label: t('Dictado'), icon: 'mic' },
  { id: 'meetings', label: t('Reuniones'), icon: 'disc-2' },
  { id: 'dictionary', label: t('Mis palabras'), icon: 'book-a' },
  { id: 'snippets', label: t('Atajos de texto'), icon: 'scissors' },
  { id: 'style', label: t('Cómo escribe'), icon: 'type' },
  { id: 'notes', label: t('Notas'), icon: 'notebook-pen' },
];

const ALIAS = {
  dictation: 'home', dictado: 'home', history: 'home', historial: 'home', inicio: 'home',
  notetaker: 'meetings', meeting: 'meetings', meets: 'meetings', meet: 'meetings', reuniones: 'meetings', 'notas de reunión': 'meetings', 'notas de reunion': 'meetings',
  stats: 'insights', estadisticas: 'insights', 'estadísticas': 'insights',
  diccionario: 'dictionary', words: 'dictionary', palabras: 'dictionary', 'mis palabras': 'dictionary',
  fragmentos: 'snippets', atajos: 'snippets', 'atajos de texto': 'snippets', estilo: 'style', reescribir: 'transforms',
  scratchpad: 'notes', notas: 'notes', 'bloc de notas': 'notes', notes: 'notes',
  whatsnew: 'novedades', changelog: 'novedades', news: 'novedades',
};
const SETTINGS_ALIAS = {
  shortcuts: ['general', 'shortcuts'], atajos: ['general', 'shortcuts'], hotkeys: ['general', 'shortcuts'],
  microphone: ['general', 'mic'], mic: ['general', 'mic'], microfono: ['general', 'mic'], 'micrófono': ['general', 'mic'],
  languages: ['general', 'langs'], idiomas: ['general', 'langs'],
  meetings: ['meetings'], meets: ['meetings'], reuniones: ['meetings'], notetaker: ['meetings'],
  general: ['general'], system: ['system'], sistema: ['system'], experimental: ['experimental'],
  data: ['data'], privacy: ['data'], privacidad: ['data'], datos: ['data'],
  engine: ['engine'], motor: ['engine'], whisper: ['engine'], parakeet: ['engine'],
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
      <button class="brand" data-nav="home" aria-label="Pronto"><img src="assets/brand/pronto_mark.png" alt="" width="24" height="24"><span class="lbl">Pronto</span>${state.app.test ? `<span class="dev-tag lbl">${esc(t('PRUEBAS'))}</span>` : ''}</button>
      <nav class="nav-list">${NAV().map((n) => `<button class="nav" data-nav="${n.id}">${icon(n.icon, 20)}<span class="lbl">${esc(n.label)}</span>${n.id === 'meetings' ? '<i class="rec-dot nav-rec" id="nav-rec" hidden></i>' : ''}</button>`).join('')}</nav>
      <div class="sb-fill"></div>
      <button class="eng tarj" id="eng-card" data-act="open-data">
        <span class="in">
          <span class="eng-hd"><i class="eng-ic">${icon('shield-check', 17)}</i><b>${esc(t('100 % local'))}</b></span>
          <span class="eng-tx">${esc(t('Tu voz no sale del equipo'))}</span>
          <span class="badge ${ev.cls}" id="eng-badge"><i class="dot ${ev.dot}"></i>${esc(ev.text)}</span>
        </span></button>
      <button class="eng-mini" id="eng-mini" data-act="open-data" aria-label="${esc(t('100 % local · Tu voz no sale del equipo'))}">${icon('shield-check', 20)}<i class="dot ${ev.dot}"></i></button>
      <div class="sb-div"></div>
      <button class="nav" data-act="settings">${icon('settings', 20)}<span class="lbl">${esc(t('Configuración'))}</span></button>
      <button class="nav" data-act="help">${icon('circle-help', 20)}<span class="lbl">${esc(t('Cómo se usa'))}</span></button>
      ${novedadesNavHtml()}
      ${state.app.version ? `<div class="sb-ver lbl">v${esc(state.app.version)}${state.app.test ? ` · ${esc(t('pruebas'))}` : ''}</div>` : ''}
    </aside>
    <main class="main"><div class="upd-slot" id="upd-slot" hidden></div><div class="page-shell carcasa"><div class="nucleo page-core"><div class="page-scroll" id="page" tabindex="0"></div></div></div></main>
  </div>`;
}

// «Novedades» en la parte de abajo (no en el menú de la primera semana), con un punto mientras la versión instalada traiga
// algo que aún no se ha visto.
function novedadesNavHtml() {
  const nuevo = novedadesSinVer();
  return `<button class="nav" data-nav="novedades" id="nav-novedades" ${nuevo ? `aria-label="${esc(t('Novedades (hay cosas nuevas)'))}"` : ''}>${icon('sparkles', 20)}<span class="lbl">${esc(t('Novedades'))}</span>${nuevo ? '<i class="nv-dot" aria-hidden="true"></i>' : ''}</button>`;
}

function updateNovedadesNav() {
  const b = $('#nav-novedades'); if (!b) return;
  const active = b.classList.contains('active');
  b.outerHTML = novedadesNavHtml();
  if (active) highlightNav();
  applySidebar(app.classList.contains('sb-collapsed'));
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

// Una página puede dejar `cleanup.canLeave = async () => boolean` para preguntar antes de irse con algo sin guardar
// (Transforms: rompedor 23-oct, hub MEDIA 2). `force` (rehacer el marco, abrir una reunión) no pregunta.
async function navigate(id, { force = false } = {}) {
  id = String(id || 'home').toLowerCase();
  id = ALIAS[id] || id;
  if (!PAGES[id]) id = 'home';
  if (id === current.id && !force) { const p = $('#page'); if (p) p.scrollTo({ top: 0, behavior: 'smooth' }); return; }
  if (!force && current.cleanup && current.cleanup.canLeave) {
    const salir = current;
    if (!(await current.cleanup.canLeave()) || current !== salir) return;
  }
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
// «Cómo se usa»: los tres pasos del dictado y, debajo, la tecla, el micrófono y el idioma. Antes era un menú «Ayuda» con
// tres ajustes que abría además Configuración detrás; ninguno de los tres primerizos encontró ahí cómo dictar.
function helpDialog() {
  const s = state.settings;
  const step = (n, title, body) => `<li class="howto-step"><span class="howto-n" aria-hidden="true">${n}</span><div><b>${title}</b><span>${body}</span></div></li>`;
  const m = openModal({
    title: t('Cómo se usa Pronto'), width: 560,
    body: `<ol class="howto">
      ${step(1, esc(t('Pon el cursor donde quieras escribir')), esc(t('En cualquier programa: WhatsApp, Word, el correo, el navegador…')))}
      ${step(2, t('Mantén pulsadas {keys} y habla', { keys: keysHtml(s.hotkeyHold) }), esc(t('Las teclas a la vez, todo el rato que hables.')))}
      ${step(3, esc(t('Suelta las teclas')), esc(t('El texto se escribe solo donde estaba el cursor. Además queda guardado en «Dictado».')))}
      </ol>
      <div class="howto-cfg">
        <button class="btn btn-secondary btn-sm" data-help="shortcuts">${icon('keyboard', 15)}${esc(t('Cambiar tecla'))}</button>
        <button class="btn btn-secondary btn-sm" data-help="mic">${icon('mic', 15)}${esc(t('Cambiar micrófono'))}</button>
        <button class="btn btn-secondary btn-sm" data-help="langs">${icon('globe', 15)}${esc(t('Cambiar idioma'))}</button>
      </div>`,
    footer: `<button class="btn btn-primary" data-close>${esc(t('Entendido'))}</button>`,
  });
  m.el.addEventListener('click', (e) => {
    const b = e.target.closest('[data-help]'); if (!b) return;
    m.close(); openSettingsDialog(b.dataset.help);
  });
}

function toggleSidebar() {
  const c = !app.classList.contains('sb-collapsed');
  lsSet('pronto.sb', c ? '1' : '0');
  applySidebar(c);
}

// Cerrar espera al guardado en vuelo y al pendiente (rompedor 23-oct, hub MEDIA 1). 'guardando': esperando; 'fallo': se avisó de que
// algo no se guardó y la siguiente × cierra igualmente (la ventana no puede quedarse sin poder cerrarse si la base está rota).
let cierre = null;
async function windowAction(kind) {
  try {
    const w = win();
    if (kind === 'min') await w.minimize();
    else if (kind === 'max') await w.toggleMaximize();
    else if (kind === 'close') {
      if (cierre === 'guardando') return;
      if (cierre === 'fallo') { await w.close(); return; }
      cierre = 'guardando';
      // Si el guardado tarda, se ve que se está guardando y por qué la ventana sigue abierta.
      const aviso = setTimeout(() => toast(t('Guardando antes de cerrar…'), { kind: 'info', ms: 8000 }), 400);
      let ok = true;
      try { ok = await flushPending(); } finally { clearTimeout(aviso); }
      if (!ok) {
        cierre = 'fallo';
        toast(t('No se pudo guardar lo último (por ejemplo, una nota). Pulsa cerrar otra vez para salir igualmente.'), { kind: 'err', ms: 8000 });
        return;
      }
      cierre = null;
      await w.close(); // cerrar = destruir el hub (libera su WebView2); antes se guarda lo pendiente
    }
  } catch (e) { cierre = null; console.error(e); }
}
// Alt+F4, apagar Windows…: no da tiempo a esperar, pero los guardados pendientes salen ya.
addEventListener('pagehide', () => { flushPending(); });
addEventListener('beforeunload', () => { flushPending(); });

app.addEventListener('click', (e) => {
  const wc = e.target.closest('[data-wc]');
  if (wc) { windowAction(wc.dataset.wc); return; }
  if (e.target.closest('#sb-toggle')) { toggleSidebar(); return; }
  const nav = e.target.closest('[data-nav]');
  if (nav) { navigate(nav.dataset.nav); return; }
  const act = e.target.closest('.sb [data-act]');
  if (!act) return;
  if (act.dataset.act === 'settings') openSettings('general');
  else if (act.dataset.act === 'help') helpDialog();
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
  // Escuchar el motor ANTES de preguntar su estado: si el aviso «listo» llegara entre la consulta y
  // el listen (la de GitHub puede tardar al encender el PC), el hub se quedaba en «Cargando el motor…».
  await listen('engine://status', (e) => { state.engine = e; updateEngineCard(); notify('engine'); });
  await Promise.all([refreshEngine(), refreshModel(), refreshStats(), initMeetingState(), refreshUpdate()]);
  // Red de seguridad: mientras no esté listo ni en error, se vuelve a preguntar cada 2 s.
  const enginePoll = setInterval(() => {
    if (state.engine.ready || state.engine.error) { clearInterval(enginePoll); return; }
    refreshEngine().then(updateEngineCard);
  }, 2000);
  try { state.app = await invoke('app_info'); } catch { /* versión desconocida: no se enseña */ }
  buildShell();
  subscribe((what) => { if (what === 'meeting') updateRecDot(); });

  const qs = new URLSearchParams(location.search);
  if (qs.get('meeting')) ctx.openMeeting = Number(qs.get('meeting'));
  navigate(qs.get('page') || 'home');
  if (qs.get('settings')) openSettingsFrom(qs.get('settings'));
  // «Lo nuevo» tras actualizar, una vez por versión. En la maqueta solo con ?novedades (si no, taparía las páginas en los tests).
  if (!IS_MOCK || qs.has('novedades')) {
    const veteran = (state.stats?.dictations || 0) > 0 || (state.stats?.meetings || 0) > 0;
    setTimeout(() => maybeShowWhatsNew(ctx, { veteran, force: IS_MOCK, blocked: hasModal() || !!qs.get('meeting') || !!qs.get('settings') || !['home', null].includes(qs.get('page')) }), 500);
  }
  document.addEventListener('pronto:novedades-vistas', updateNovedadesNav);

  listen('hub://navigate', (p) => { const page = (p && p.page) || 'home'; if (p && p.id != null) ctx.openMeeting = Number(p.id); if (String(page).toLowerCase() === 'settings') openSettings('general'); else navigate(page, { force: ctx.openMeeting != null }); });
  listen('hub://settings', (p) => openSettingsFrom(p && p.tab));
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
  document.addEventListener('pronto:navigate', (e) => navigate(e.detail));
  document.addEventListener('pronto:history-cleared', () => { if (current.id === 'home') navigate('home', { force: true }); refreshStats(); });
}

boot();

if (IS_MOCK) console.info('[Pronto hub] backend simulado (mock). Parámetros: ?page=insights ?settings=system ?theme=light ?ollama=off ?empty=1 ?engine=error ?model=missing ?model=error ?update=available ?update=error ?update=none');
