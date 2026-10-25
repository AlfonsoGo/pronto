// Configuración: modal 960x640 con sidebar interna. [spec §2.4]
import { icon } from '../../shared/icons.js';
import { invoke, listen, state, saveSettings, subscribe, notify, applySettingsEffects, IS_MOCK, checkUpdate, installUpdate, updateBusy, startModelDownload } from '../api.js';
import {
  t, esc, $, $$, toast, errText, openModal, confirmDialog, keysHtml, hotkeyText, sameHotkey, getLang,
} from '../util.js';
import { mountCleanup } from './cleanup.js';
import { switchHtml, selectHtml } from '../ui.js';
import { aiSlot, aiMount } from '../ai.js';

export const VERSION = 'Pronto v2.3.0';
/** Versión real del ejecutable (la da `app_info`, la misma que la barra lateral; si falta, `update_status`); `VERSION`
 *  solo es el respaldo. */
const versionLabel = () => {
  const v = (state.app && state.app.version) || state.update.currentVersion;
  return v ? `Pronto v${v}` : VERSION;
};
// Lo de uso diario arriba; lo que pide saber de motores o de IA, en «Avanzado» (pruebas con primerizos, 24-oct-2026).
// Los id no cambian: los usan los enlaces (`?settings=`), «Cómo se usa» y los tests.
import { shortensRetention } from '../retencion.js';

const TABS = () => [
  { group: t('Configuración'), items: [
    { id: 'general', icon: 'sliders-horizontal', label: t('Lo básico') },
    { id: 'system', icon: 'monitor', label: t('Ventana y sonidos') },
    { id: 'meetings', icon: 'disc-2', label: t('Reuniones') },
    { id: 'data', icon: 'shield-check', label: t('Privacidad y datos') },
  ] },
  { group: t('Avanzado'), items: [
    { id: 'engine', icon: 'cpu', label: t('Reconocimiento de voz') },
    { id: 'experimental', icon: 'wand-sparkles', label: t('Limpieza e IA') },
  ] },
];
const flat = () => TABS().flatMap((g) => g.items);

const row = (title, desc, control, extra = '') =>
  `<div class="srow ${extra}"><div class="srow-t"><b>${title}</b>${desc ? `<span>${desc}</span>` : ''}</div><div class="srow-c">${control}</div></div>`;
const group = (title, rows) => `<section class="sgroup">${title ? `<h3 class="overline">${esc(title)}</h3>` : ''}<div class="scard">${rows}</div></section>`;
// Cada botón dice qué cambia: había nueve «Cambiar» iguales y los primerizos pulsaban el que no era.
const changeBtn = (act, label) => `<button class="btn btn-secondary btn-sm" data-act="${act}">${esc(label)}</button>`;

// ------------------------------------------------------------------ pestañas
function engineChip() {
  const e = state.engine;
  if (e.error) return `<span class="badge err"><i class="dot"></i>${esc(t('Error'))}</span>`;
  if (e.loading) return `<span class="badge vio"><i class="dot pulse"></i>${esc(t('Cargando…'))}</span>`;
  return e.ready ? `<span class="badge ok"><i class="dot"></i>${esc(t('Listo'))}</span>` : `<span class="badge"><i class="dot"></i>${esc(t('Detenido'))}</span>`;
}
const ollamaNote = () => `<div class="aviso vio"><div class="in"><span class="ic-l">${icon('info', 18)}</span><div class="tx"><b>${esc(t('IA local opcional (Ollama)'))}</b>
  <span>${esc(t('Se usa para pulir y reescribir textos, resumir reuniones y responder preguntas sobre ellas. Si no la instalas, el dictado y la transcripción funcionan igual. Puede ir más lento según tu PC.'))}
  ${esc(t('Con el botón de abajo, Pronto la instala y la deja lista por ti.'))}</span></div></div></div>${aiSlot('card')}`;
// ¿La URL de Ollama es de este PC (loopback)? 0.0.0.0 no cuenta. El host lo saca el parser WHATWG (el mismo
// que usa el navegador): uno casero se deja engañar por `http://evil.example\@localhost`. Lo que no parsea, remoto.
const isLoopbackUrl = (u) => {
  const s = String(u || '').trim();
  try {
    const host = new URL(/^[a-z][a-z0-9+.-]*:\/\//i.test(s) ? s : 'http://' + s).hostname.toLowerCase();
    // Como `is_loopback_host` del backend (updater.rs): todo 127.0.0.0/8 (el parser ya lo deja en decimal con puntos, también
    // «127.1» o «0x7f.1») y la IPv4 mapeada («[::ffff:127.x.y.z]» sale como «[::ffff:7fxx:yyyy]»). [rompedor 23-oct, hub BAJA 6]
    return ['localhost', '[::1]'].includes(host) || /^127(\.\d{1,3}){3}$/.test(host) || /^\[::ffff:7f[0-9a-f]{2}:[0-9a-f]{1,4}\]$/.test(host);
  } catch (_) { return false; }
};
// Regla 1 (decisión de Txonso, 23-oct-2026): Ollama solo en este PC. Una dirección de otro equipo es un error
// de validación: no se guarda (el backend la rechaza también, `aisetup::checked_local_url`).
const REMOTE_MSG = () => t('Pronto solo usa Ollama en este PC (localhost o 127.0.0.1). Escribe una dirección de este equipo: con otra, tus dictados y reuniones tendrían que salir del PC.');
const remoteOllamaNote = (url) => `<div class="aviso rojo" id="oll-remote" role="alert"${isLoopbackUrl(url) ? ' hidden' : ''}><div class="in"><span class="ic-l">${icon('triangle-alert', 18)}</span><div class="tx"><span>${esc(REMOTE_MSG())}</span></div></div></div>`;
// Marca el campo de la URL como válido o no y enseña u oculta el error de debajo.
function showOllamaUrlError(root, bad) {
  const n = $('#oll-remote', root); if (n) n.hidden = !bad;
  const inp = $('[data-text="ollamaUrl"]', root); if (inp) { if (bad) inp.setAttribute('aria-invalid', 'true'); else inp.removeAttribute('aria-invalid'); }
}
const langName = (l) => ({ auto: t('Español e inglés (lo detecta solo)'), es: t('Solo español'), en: t('Solo inglés') }[l] || l);

// ------------------------------------------------------------------ pestaña Motor (Contrato v13)
// `asr_options` dice qué modelos de Whisper caben en la tarjeta de este PC y cuáles están bajados.
let asrOpts = null, asrDl = null; // asrDl: progreso de la descarga de Whisper en curso ({file, overallPercent, state})
// Cambio de motor pedido y aún sin terminar de cargar: hasta el siguiente estado «no cargando» del motor.
let switching = false;
const PARAKEET = {
  id: 'parakeet', name: 'NVIDIA Parakeet TDT 0.6B v3', mb: 670, wer: 5.7, speed: 4, cpu: true,
  desc: 'Funciona en cualquier PC, en el procesador. Detecta solo el idioma (25 idiomas europeos).',
};
// Etiqueta y consejo de cada modelo, solo con lo medido (pedido de Txonso el 24-oct-2026: las etiquetas de antes no
// decían lo que importa). `juez`: % de palabras mal en 10 dictados reales (specs/juez-g1/2026-10-23.md; turbo por su
// camino rápido, specs/velocidad/2026-10-23.md). `reu`: % en reuniones de prueba troceadas (docs/asr-comparativa.md).
// `frase`: tiempo por frase tras soltar el atajo (specs/velocidad/ y el juez). `null` = sin medir.
const MEDIDO = {
  parakeet: { tag: 'Recomendado · el más probado', cls: 'ok', juez: 12.4, reu: 5.7, frase: '0,35-0,55 s',
    tip: 'El de fábrica: funciona en cualquier PC, sin tarjeta gráfica, y es el que más tiempo lleva en Pronto. Falla algo más que los Whisper grandes.' },
  'large-v3-turbo': { tag: 'El más rápido', cls: 'vio', juez: 9.9, reu: 4.2, frase: '0,2 s',
    tip: 'Si tienes una gráfica potente, este es el mejor para ti: el más rápido y con menos errores que Parakeet.' },
  medium: { tag: 'Menos errores', cls: 'vio', juez: 8.1, reu: 3.4, frase: '0,24-0,29 s',
    tip: 'El que menos falló en las pruebas, con dictados y con reuniones. Un poco más lento que turbo.' },
  'large-v3': { tag: 'Solo con gráfica grande', cls: '', juez: 8.7, reu: 3.8, frase: '0,37-0,48 s',
    tip: 'Acierta casi como medium, pero tarda casi el doble que turbo y ocupa el doble de memoria de vídeo. En una prueba repitió una frase en bucle.' },
  small: { tag: 'Para gráficas pequeñas', cls: '', juez: null, reu: 4.0, frase: null,
    tip: 'Ligero (1 GB de memoria de vídeo). En reuniones de prueba falló poco, pero no se ha medido con dictados reales.' },
  base: { tag: 'Sin probar', cls: '', juez: null, reu: null, frase: null,
    tip: 'El más pequeño (60 MB). No se ha medido en Pronto: úsalo solo si no cabe ningún otro.' },
};
const pct = (w) => String(w).replace('.', ',');
function measured(m) {
  const d = MEDIDO[m.id];
  if (!d) return m.wer == null ? t('errores sin medir') : t('{w} % de errores en reuniones', { w: pct(m.wer) });
  const errs = d.juez != null ? t('{a} % de errores en dictados reales · {b} % en reuniones', { a: pct(d.juez), b: pct(d.reu) })
    : d.reu != null ? t('{w} % de errores en reuniones', { w: pct(d.reu) }) : t('errores sin medir');
  return d.frase ? `${errs} · ${t('{s} por frase', { s: d.frase })}` : errs;
}
function engineInUse(s) {
  return s.asrEngine === 'whisper' ? (s.whisperModel || 'large-v3-turbo') : 'parakeet';
}
function modelRow(m, s) {
  const chosen = engineInUse(s) === m.id;
  const loaded = chosen && (state.engine.active || 'parakeet') === (m.id === 'parakeet' ? 'parakeet' : 'whisper');
  const present = m.id === 'parakeet' ? !!(state.model && state.model.complete) : m.present;
  const busy = asrDl && asrDl.file === m.file && (asrDl.state === 'downloading' || asrDl.state === 'verifying');
  const where = m.cpu ? 'CPU' : t('tarjeta gráfica ({g} GB)', { g: m.minVramGb });
  const meta = `${m.mb >= 1000 ? (m.mb / 1000).toFixed(1).replace('.', ',') + ' GB' : m.mb + ' MB'} · ${where} · ${measured(m)}`;
  let control;
  if (chosen) control = state.engine.loading || switching ? `<span class="badge vio"><i class="dot pulse"></i>${esc(t('Cargando…'))}</span>`
    : `<span class="badge ok"><i class="dot"></i>${esc(loaded ? t('En uso') : t('Elegido'))}</span>`;
  else if (!m.cpu && !m.available) control = `<span class="muted" title="${esc(m.reason || '')}">${esc(t('No cabe en este PC'))}</span>`;
  else if (busy) control = `<span class="muted">${esc(asrDl.state === 'verifying' ? t('Comprobando…') : t('Descargando… {p} %', { p: asrDl.overallPercent || 0 }))}</span>`;
  else if (!present) control = `<button class="btn btn-secondary btn-sm" data-act="dl-model" data-id="${esc(m.id)}" ${asrOpts && asrOpts.downloading ? 'disabled' : ''}>${icon('download', 14)}${esc(t('Descargar'))}</button>`;
  // Con una carga en marcha, «Usar» de todas las filas queda apagado: cada clic más lanzaba otra carga
  // del modelo (+670 MB cada una) (rompedor 22-oct-c, carga BAJA 1).
  else control = `<button class="btn btn-secondary btn-sm" data-act="use-model" data-id="${esc(m.id)}" ${state.engine.loading || switching ? 'disabled' : ''}>${esc(t('Usar'))}</button>`;
  const why = !m.cpu && !m.available && m.reason ? `<br>${esc(m.reason)}.` : '';
  const d = MEDIDO[m.id];
  const tag = d ? ` <span class="badge ${d.cls}">${esc(t(d.tag))}</span>` : '';
  return row(esc(m.name) + tag, `${esc(t(d ? d.tip : m.desc))}<br><span class="mono-meta">${esc(meta)}</span>${why}`, control);
}
function modelListHtml(s) {
  if (!asrOpts) return row(t('Modelos de voz'), t('Comprobando tu equipo…'), '');
  return [PARAKEET, ...asrOpts.whisper].map((m) => modelRow(m, s)).join('');
}
function engineSummary(s) {
  const id = engineInUse(s);
  const active = state.engine.active || 'parakeet';
  const gpu = asrOpts && asrOpts.gpu ? `${asrOpts.gpu} · ${String(asrOpts.vramGb.toFixed(1)).replace('.', ',')} GB` : t('ninguna compatible');
  const name = id === 'parakeet' ? PARAKEET.name : ((asrOpts && asrOpts.whisper.find((m) => m.id === id)) || { name: 'Whisper' }).name;
  const loading = state.engine.loading || switching;
  const fellBack = !loading && id !== 'parakeet' && active === 'parakeet' && state.engine.ready;
  const text = loading ? t('Cargando {n}…', { n: name }) : fellBack ? t('{n} no se pudo cargar; se usa Parakeet.', { n: name }) : name;
  // Si el motor no pudo cargar (carpeta del modelo incompleta o dañada), el porqué va aquí mismo.
  const why = !loading && state.engine.error ? `<br>${esc(state.engine.error)}` : '';
  return row(t('En uso'), esc(text) + why, `<span class="eng-chip">${engineChip()}</span>`)
    + row(t('Tarjeta gráfica'), esc(gpu), '');
}
function paintEngine(root) {
  const list = root && root.querySelector('#asr-list');
  if (list) {
    // Repintar la lista tira el botón enfocado («Usar» → «Cargando…») y el foco caía al <body>: el siguiente Tab volvía al
    // principio del modal. Se devuelve el foco a la misma fila (su botón, o el chip «Cargando…» con tabindex=-1). [rompedor 23-oct, hub BAJA 5]
    const a = document.activeElement;
    const fila = a && list.contains(a) ? a.closest('.srow') : null;
    const idx = fila ? Array.prototype.indexOf.call(list.children, fila) : -1;
    list.innerHTML = modelListHtml(state.settings);
    const nueva = idx >= 0 ? list.children[idx] : null;
    if (nueva) {
      const dest = nueva.querySelector('button:not([disabled])') || nueva.querySelector('.srow-c .badge, .srow-c .muted') || nueva;
      if (!dest.matches('button')) dest.tabIndex = -1;
      dest.focus({ preventScroll: true });
    }
  }
  const sum = root && root.querySelector('#asr-sum');
  if (sum) sum.innerHTML = engineSummary(state.settings);
}
async function loadAsr(root) {
  try { asrOpts = await invoke('asr_options'); } catch (_) { asrOpts = { gpu: null, vramGb: 0, whisper: [], downloading: null }; }
  paintEngine(root);
}
listen('whisper://progress', (p) => {
  asrDl = p;
  if (asrOpts) {
    asrOpts.downloading = p.state === 'downloading' || p.state === 'verifying' ? p.file : null;
    const m = asrOpts.whisper.find((x) => x.file === p.file);
    if (p.state === 'done' && m) { m.present = true; toast(t('{n} descargado. Ya puedes usarlo.', { n: m.name })); }
  }
  if (p.state === 'error') toast(p.error || t('No se pudo descargar el modelo'), { kind: 'err' });
  if (current) paintEngine(document.getElementById('set-body'));
  // Al terminar, el backend manda: se vuelve a pedir la lista para no pintar «Descargar» con datos viejos.
  if (p.state === 'done' && current) loadAsr(document.getElementById('set-body'));
});

// ------------------------------------------------------------------ monitor de la barra (Contrato v8)
// `list_monitors` es asíncrono: la fila se pinta con lo último que se sabe y se rehace al llegar la lista.
let monitors = null;
const monLabel = (m) => [t('Pantalla {n}', { n: m.number }), m.name, `${m.width}×${m.height}`, m.primary ? t('principal') : '']
  .filter(Boolean).join(' · ');
function monitorRowHtml(s) {
  const pin = s.flowBarMonitor || '';
  const list = monitors || [];
  const listed = !pin || list.some((m) => m.device.toLowerCase() === pin.toLowerCase());
  const missing = !listed && !!monitors;          // con la lista cargada, el fijado no está conectado
  const opts = [{ value: '', label: t('Seguir al cursor') }, ...list.map((m) => ({ value: m.device, label: monLabel(m) }))];
  if (!listed) opts.push({ value: pin, label: missing ? t('Pantalla fijada (no conectada)') : t('Pantalla fijada') });
  const desc = !pin ? t('La barra sale en el monitor donde estás al dictar. También puedes fijarla con el candado de la propia barra.')
    : missing ? t('Ese monitor no está conectado: mientras tanto, la barra sigue al cursor.')
    : t('La barra se queda en ese monitor aunque trabajes o dictes en otro.');
  return row(t('Monitor de la barra'), esc(desc), selectHtml('flowBarMonitor', opts, pin, { label: t('Monitor de la barra') }), 'mon-row');
}
async function loadMonitors(body) {
  try { monitors = (await invoke('list_monitors')) || []; } catch (_) { monitors = monitors || []; }
  const r = $('.mon-row', body);
  if (r && state.settings) r.outerHTML = monitorRowHtml(state.settings);
}

// ------------------------------------------------------------------ actualizaciones (Contrato v7)
function updateLine() {
  const u = state.update, v = u.info ? u.info.version : '';
  if (u.checking) return esc(t('Buscando…'));
  if (updateBusy()) return esc(t('Actualizando a {v}…', { v }));
  if (v && (u.state === 'available' || u.state === 'error')) return esc(t('Hay una versión nueva: {v}', { v }));
  if (u.state === 'upToDate') {
    const at = u.checkedAt ? new Date(u.checkedAt) : null;
    const hhmm = at && !isNaN(at) ? at.toLocaleTimeString(getLang() === 'en' ? 'en-US' : 'es-ES', { hour: '2-digit', minute: '2-digit' }) : '';
    return esc(hhmm ? t('Estás al día · comprobado a las {h}', { h: hhmm }) : t('Estás al día.'));
  }
  return esc(t('Todavía no se ha buscado en esta sesión.'));
}

function updateGroupHtml() {
  const u = state.update, s = state.settings;
  const newer = u.info && (u.state === 'available' || u.state === 'error');
  const btns = `${newer ? `<button class="btn btn-primary btn-sm" data-act="update-install" ${updateBusy() ? 'disabled' : ''}>${icon('download', 14)}${esc(t('Actualizar ahora'))}</button>` : ''}
    <button class="btn btn-secondary btn-sm" data-act="update-check" ${u.checking || updateBusy() ? 'disabled' : ''} style="margin-left:8px">${icon('refresh-cw', 14)}${esc(t('Buscar ahora'))}</button>`;
  return group(t('Actualizaciones'), [
    row(esc(t('Buscar actualizaciones automáticamente')), esc(t('Al abrir Pronto y cada 4 horas. Solo se instala cuando tú lo pides.')), switchHtml('autoUpdate', s.autoUpdate !== false, t('Buscar actualizaciones automáticamente'))),
    row(esc(t('Versión instalada: {v}', { v: (state.app && state.app.version) || u.currentVersion || VERSION.replace('Pronto v', '') })), updateLine(), btns),
  ].join('')).replace('<section class="sgroup">', '<section class="sgroup" id="upd-group">');
}

const VIEWS = {
  general(s) {
    return group('', [
      row(t('Tecla para dictar'), t('Mantén {keys} y habla.', { keys: keysHtml(s.hotkeyHold) }), changeBtn('shortcuts', t('Cambiar tecla'))),
      row(t('Micrófono'), esc(s.micName || t('El que use Windows')), changeBtn('mic', t('Cambiar micrófono'))),
      row(t('Idioma en el que hablas'), esc(langName(s.language)), changeBtn('langs', t('Cambiar idioma'))),
      row(t('Idioma de la aplicación'), t('Selecciona tu idioma preferido para Pronto.'),
        selectHtml('appLanguage', [{ value: 'es', label: 'Español' }, { value: 'en', label: 'English' }], s.appLanguage, { label: t('Idioma de la aplicación') })),
      row(t('Nombre'), t('Se usa en el saludo de la pantalla de inicio.'),
        `<input class="input" data-text="firstName" maxlength="40" value="${esc(s.firstName || '')}" placeholder="${esc(t('Tu nombre'))}" aria-label="${esc(t('Nombre'))}" autocomplete="off" spellcheck="false">`),
      row(t('Decir «pulsa enter»'), t('Si acabas un dictado diciendo «pulsa enter», Pronto pulsa Enter por ti (por ejemplo, para enviar un mensaje).'), switchHtml('pressEnterCommand', s.pressEnterCommand, t('Decir «pulsa enter»'))),
    ].join(''));
  },
  engine(s) {
    return `<div class="aviso vio"><div class="in"><span class="ic-l">${icon('cpu', 18)}</span><div class="tx"><b>${esc(t('Qué programa convierte tu voz en texto'))}</b>
      <span>${esc(t('Si no sabes cuál elegir, deja el recomendado.'))} ${esc(t('Parakeet va en cualquier PC. Whisper se equivoca menos con nombres y puntuación, pero necesita tarjeta gráfica. Tu diccionario se le pasa a Whisper como pista. Los modelos se bajan una vez y todo sigue siendo local.'))}</span></div></div></div>`
      + group(t('Ahora'), `<div id="asr-sum">${engineSummary(s)}</div>`)
      + group(t('Modelos'), `<div id="asr-list">${modelListHtml(s)}</div>`)
      + group(t('Avanzado'), row(t('Ruta del modelo Parakeet'), t('Carpeta con encoder, decoder, joiner y tokens.'),
        `<input class="input wide" data-text="modelDir" value="${esc(s.modelDir)}" aria-label="${esc(t('Ruta del modelo'))}" autocomplete="off" spellcheck="false">`, 'stack'));
  },
  system(s) {
    const hidden = s.flowBarHiddenUntil && new Date(s.flowBarHiddenUntil) > new Date();
    const hhmm = hidden ? new Date(s.flowBarHiddenUntil).toLocaleTimeString(getLang() === 'en' ? 'en-US' : 'es-ES', { hour: '2-digit', minute: '2-digit' }) : '';
    return group(t('Configuración de la aplicación'), [
      row(t('Abrir Pronto al encender el ordenador'), t('Pronto se abre solo con Windows y espera en la bandeja, junto al reloj.'), switchHtml('launchAtLogin', s.launchAtLogin, t('Abrir Pronto al encender el ordenador'))),
      row(t('Mostrar siempre la barra de dictado'), hidden ? t('Oculta hasta las {h}.', { h: hhmm }) : t('La barrita flotante de abajo se queda visible aunque no estés dictando.'), switchHtml('showFlowBar', s.showFlowBar, t('Mostrar siempre la barra de dictado'))),
      monitorRowHtml(s),
    ].join('')) + group(t('Sonido'), [
      row(t('Sonidos de dictado y notificaciones'), t('Un pitido corto al empezar y al terminar.'), switchHtml('sounds', s.sounds, t('Sonidos de dictado y notificaciones'))),
      row(t('Silenciar la música mientras dictas'), t('Silencia el audio del sistema mientras grabas y lo restaura después.'), switchHtml('muteMusic', s.muteMusic, t('Silenciar la música mientras dictas'))),
    ].join('')) + group(t('Apariencia'), [
      row(t('Fondo oscuro o claro'), t('Elige si el panel se ve con fondo oscuro o con fondo claro.'),
        selectHtml('theme', [{ value: 'dark', label: t('Oscuro') }, { value: 'light', label: t('Claro') }], s.theme, { label: t('Fondo oscuro o claro') })),
    ].join('')) + updateGroupHtml();
  },
  meetings(s) {
    const cur = s.meetingAuto || (s.meetingDetect === false ? 'off' : 'auto');
    const opts = [
      { v: 'auto', label: t('Automático'), sub: t('Empieza al detectar Teams, Zoom, Meet… y para al colgar') },
      { v: 'ask', label: t('Preguntar antes'), sub: t('Te avisa al detectar una reunión y tú decides si tomar notas') },
      { v: 'off', label: t('Desactivado'), sub: t('Solo toma notas cuando tú las inicias') },
    ];
    return `<section class="sgroup"><h3 class="overline">${esc(t('Grabar reuniones automáticamente'))}</h3><div class="opt-list" role="radiogroup" aria-label="${esc(t('Grabar reuniones automáticamente'))}">${opts.map((o) => `<button class="opt ${cur === o.v ? 'on' : ''}" role="radio" aria-checked="${cur === o.v}" data-auto="${o.v}"><span class="radio"></span><span class="opt-t"><b>${esc(o.label)}</b><small>${esc(o.sub)}</small></span></button>`).join('')}</div></section>` + group('', [
      row(t('Tecla para grabar una reunión'), t('Empieza o termina la reunión con {keys}.', { keys: keysHtml(s.hotkeyMeeting) }),
        `<button class="btn btn-secondary btn-sm" data-act="capture-meeting" id="cap-meeting">${esc(t('Cambiar tecla'))}</button>`),
      row(t('Guardar el audio de las reuniones'), t('Conserva en este equipo el audio de tu micrófono y el del sistema. Por defecto se descarta al terminar de transcribir.'), switchHtml('meetingKeepAudio', s.meetingKeepAudio, t('Guardar el audio de las reuniones'))),
    ].join('')) + ollamaNote() + group(t('Resumen · Ollama'), [
      row(t('Modelo del resumen'), t('El modelo de Ollama que resume la reunión y extrae tareas y decisiones.'),
        `<input class="input" data-text="ollamaModel" list="ollama-models" value="${esc(s.ollamaModel)}" aria-label="${esc(t('Modelo del resumen'))}" autocomplete="off" spellcheck="false"><datalist id="ollama-models"></datalist>`),
      row(t('Estado'), '<span id="oll-state" class="muted">—</span>', `<button class="btn btn-secondary btn-sm" data-act="ollama-check">${icon('refresh-cw', 14)}${esc(t('Comprobar'))}</button>`),
    ].join('')) + `<p class="consent" id="oll-meet-note" hidden>${icon('info', 14)}${esc(t('Sin Ollama las reuniones se transcriben, pero no se resumen. Puedes instalarlo cuando quieras; puede ir más lento según tu PC.'))}</p><div class="aviso amar"><div class="in"><span class="ic-l">${icon('info', 18)}</span><div class="tx"><span>${esc(t('Pide siempre consentimiento antes de transcribir a otras personas. Todo se procesa en este equipo.'))}</span></div></div></div>`;
  },
  experimental(s) {
    return ollamaNote() + '<div id="cleanup-host"></div>' + group(t('Reescribir texto seleccionado'), [
      row(t('Reescribir con IA'), t('Seleccionas un texto en cualquier programa, pulsas una tecla y Pronto lo reescribe (más corto, más claro…). Necesita la IA local.'),
        `<button class="btn btn-secondary btn-sm" data-act="open-transforms">${esc(t('Abrir Reescribir'))}</button>`),
    ].join('')) + group(t('IA local · Ollama'), [
      row(t('Modelo'), t('El modelo de Ollama con el que se reescribe el texto.'),
        `<input class="input" data-text="ollamaModel" list="ollama-models" value="${esc(s.ollamaModel)}" aria-label="${esc(t('Modelo'))}" autocomplete="off" spellcheck="false"><datalist id="ollama-models"></datalist>`),
      row(t('URL de Ollama'), t('Dónde escucha Ollama (por defecto, en este mismo equipo).'),
        `<input class="input" data-text="ollamaUrl" value="${esc(s.ollamaUrl)}" aria-label="${esc(t('URL de Ollama'))}" autocomplete="off" spellcheck="false">`),
      row(t('Estado'), '<span id="oll-state" class="muted">—</span>', `<button class="btn btn-secondary btn-sm" data-act="ollama-check">${icon('refresh-cw', 14)}${esc(t('Comprobar'))}</button>`),
    ].join('')) + remoteOllamaNote(s.ollamaUrl);
  },
  data(s) {
    const days = [0, 7, 30, 90, 180, 365].map((n) => ({ value: n, label: n === 0 ? t('Siempre') : t('{n} días', { n }) }));
    const audio = [0, 1, 7, 14, 30].map((n) => ({ value: n, label: n === 0 ? t('No guardar audio') : n === 1 ? t('1 día') : t('{n} días', { n }) }));
    return `<div class="aviso verde"><div class="in"><span class="ic-l">${icon('lock', 20)}</span>
      <div class="tx"><b>${esc(t('100 % local'))}</b><span>${esc(t('Tu voz, tus transcripciones, el diccionario y las notas viven solo en este equipo. Pronto no envía nada a Internet.'))}</span></div></div></div>` +
      group(t('Historial'), [
        row(t('Conservar el historial'), s.historyRetentionDays ? t('Los dictados de más de {n} días se borran solos.', { n: s.historyRetentionDays }) : t('Se guardan todos tus dictados, en este PC.'), selectHtml('historyRetentionDays', days, s.historyRetentionDays, { label: t('Conservar el historial') })),
        row(t('Audio guardado'), t('Cuántos días se guarda la grabación de tu voz, para volver a escucharla o volver a pasarla a texto.'), selectHtml('keepAudioDays', audio, s.keepAudioDays, { label: t('Audio guardado') })),
        row(t('Borrar historial'), t('Elimina todas las transcripciones y su audio. No se puede deshacer.'), `<button class="btn btn-danger btn-sm" data-act="clear-history">${icon('trash-2', 14)}${esc(t('Borrar historial'))}</button>`),
      ].join('')) +
      group(t('Motor de voz'), [
        row(t('Estado del motor'), esc(state.engine.error || t('Elige el motor y el modelo en Configuración → Reconocimiento de voz.')), `<span id="eng-chip">${engineChip()}</span>`),
      ].join('')) +
      group(t('Registro de errores'), [
        row(t('Registro de errores'), t('Si algo falla, Pronto lo apunta en un fichero de este equipo: fecha, versión y qué falló, sin tus dictados ni tus textos. No se envía a ningún sitio.'),
          `<button class="btn btn-secondary btn-sm" data-act="open-error-log">${icon('file-text', 14)}${esc(t('Abrir registro de errores'))}</button>`),
      ].join(''));
  },
};

// ------------------------------------------------------------------ modal principal
let current = null; // {ctl, tab}

export function openSettings(tab = 'general', opts = {}) {
  const dialog = opts.dialog || null;
  const ids = flat().map((x) => x.id);
  if (!ids.includes(tab)) tab = 'general';
  if (current) { current.go(tab); if (dialog) openDialog(dialog, current); return current.ctl; }

  const nav = TABS().map((g) => `<div class="rotulo set-g">${esc(g.group)}</div>${g.items.map((i) =>
    `<button class="set-tab" role="tab" data-stab="${i.id}" aria-selected="false">${icon(i.icon, 18)}<span>${esc(i.label)}</span></button>`).join('')}`).join('');

  const ctl = openModal({
    width: 'min(960px, calc(100vw - 48px))', height: 'min(640px, calc(100vh - 48px))', cls: 'settings',
    raw: `<div class="set">
      <nav class="set-nav" role="tablist" aria-orientation="vertical" aria-label="${esc(t('Configuración'))}">${nav}
        <div class="set-foot"><span>${esc(versionLabel())}</span><span class="set-eng" title="${esc(t('Estado del motor'))}">${engineChip()}</span></div></nav>
      <section class="set-main">
        <header class="set-hd"><h2 id="set-title"></h2><button class="icon-btn" data-close aria-label="${esc(t('Cerrar'))}">${icon('x', 18)}</button></header>
        <div class="set-scroll" id="set-body"></div></section></div>`,
    onClose: () => { unsub(); if (offCleanup) offCleanup(); current = null; },
  });
  const body = $('#set-body', ctl.el), title = $('#set-title', ctl.el);
  ctl.modal.setAttribute('aria-labelledby', 'set-title');
  let active = tab, offAi = null, offCleanup = null;

  function renderTab() {
    const info = flat().find((x) => x.id === active);
    title.textContent = info.label;
    $$('.set-tab', ctl.el).forEach((b) => { const on = b.dataset.stab === active; b.setAttribute('aria-selected', on); b.tabIndex = on ? 0 : -1; });
    // Si el foco estaba dentro de la pestaña (p. ej. el botón «Usar» de Motor, que tras guardar repinta todo por
    // `settings://changed`), se devuelve al mismo control o, si ya no existe, al de la misma fila: sin esto caía al <body>.
    // (rompedor 23-oct, hub BAJA 5)
    const a = document.activeElement;
    const volver = a && body.contains(a) ? {
      sel: a.dataset && a.dataset.act ? `[data-act="${a.dataset.act}"]${a.dataset.id ? `[data-id="${a.dataset.id}"]` : ''}` : null,
      idx: a.closest('.srow') ? $$('.srow', body).indexOf(a.closest('.srow')) : -1,
    } : null;
    if (offCleanup) { offCleanup(); offCleanup = null; }
    body.innerHTML = VIEWS[active](state.settings);
    body.scrollTop = 0;
    const ch = $('#cleanup-host', body); if (ch) offCleanup = mountCleanup(ch, { aiCta: false });
    if (volver) {
      let d = volver.sel && $(`${volver.sel}:not([disabled])`, body);
      if (!d && volver.idx >= 0) {
        const f = $$('.srow', body)[volver.idx];
        d = f && (f.querySelector('button:not([disabled]),select,input') || f.querySelector('.srow-c .badge,.srow-c .muted') || f);
      }
      if (d) { if (!d.matches('button,select,input,textarea')) d.tabIndex = -1; d.focus({ preventScroll: true }); }
    }
    if (active === 'experimental' || active === 'meetings') ollamaCheck(false);
    if (active === 'system') loadMonitors(body);
    if (active === 'engine') loadAsr(body);
    if (offAi) offAi();
    offAi = aiMount(body, (kind) => { if (kind === 'ready') { ollamaCheck(false); if (offCleanup) offCleanup.recheck(); } });
  }
  function go(id) { active = id; renderTab(); }
  current = { ctl, go, get tab() { return active; }, rerender: renderTab };

  async function ollamaCheck(announce) {
    const out = $('#oll-state', body);
    if (out) out.textContent = t('Comprobando…');
    let r;
    try { r = await invoke('ollama_status'); } catch (_) { r = { reachable: false, models: [] }; }
    const o = $('#oll-state', body);
    if (!o) return;
    const note = $('#oll-meet-note', body); if (note) note.hidden = r.reachable;
    const dl = $('#ollama-models', body);
    if (dl) dl.innerHTML = (r.models || []).map((m) => `<option value="${esc(m)}">`).join('');
    const model = state.settings.ollamaModel;
    if (!r.reachable) o.innerHTML = `<span class="err-t">${esc(t('Ollama no responde. Ejecuta «ollama serve» o abre la app de Ollama.'))}</span>`;
    else if (!r.models.includes(model)) o.innerHTML = `<span class="warn-t">${esc(t('Ollama responde, pero falta el modelo «{m}» (ollama pull {m}).', { m: model }))}</span>`;
    else o.innerHTML = `<span class="ok-t">${esc(t('Ollama listo con «{m}».', { m: model }))}</span>`;
    if (announce) toast(r.reachable ? t('Ollama responde') : t('Ollama no responde'), { kind: r.reachable ? 'ok' : 'err' });
  }

  // Acortar cuánto se conserva borra al momento y para siempre (rompedor 25-oct, ALTA 1): con el teclado, cada flecha del
  // desplegable lanzaba `change` y la primera ya purgaba. Se vuelve al valor guardado y solo se aplica tras confirmar.
  let askingRetention = false;
  async function confirmRetention(sel, key) {
    const next = Number(sel.value);
    sel.value = String(state.settings[key]);
    if (askingRetention) return;
    askingRetention = true;
    const history = key === 'historyRetentionDays';
    const ok = await confirmDialog({
      title: history ? t('¿Borrar los dictados de más de {n} días?', { n: next }) : next === 0 ? t('¿Dejar de guardar el audio?')
        : next === 1 ? t('¿Borrar el audio de más de 1 día?') : t('¿Borrar el audio de más de {n} días?', { n: next }),
      body: esc(history ? t('Los dictados más antiguos se borran ahora y para siempre, con su audio.')
        : t('El audio más antiguo se borra ahora y para siempre: esos dictados ya no se podrán escuchar ni volver a pasar a texto.')),
      confirmText: t('Sí, borrar'),
    });
    askingRetention = false;
    if (!ok) { sel.focus(); return; }
    sel.value = String(next);
    changeSetting(key, next);
  }

  async function changeSetting(key, value) {
    // Una dirección de Ollama que no es de este PC no se guarda: error de validación, sin llamar al backend.
    // Configuración guarda el campo al cerrarse (Esc, ×, clic fuera): el error de debajo ya no se vería, así que
    // además sale como aviso suelto (rompedor 23-oct, hub MEDIA 3).
    if (key === 'ollamaUrl' && !isLoopbackUrl(value)) {
      showOllamaUrlError(body, true);
      toast(REMOTE_MSG(), { kind: 'err', ms: 8000 });
      return;
    }
    try {
      await saveSettings({ [key]: value });
      if (key === 'appLanguage') {
        document.dispatchEvent(new CustomEvent('pronto:lang'));
        const keep = active; ctl.close(); openSettings(keep); return;
      }
      if (key === 'theme') renderTab();
      // Los desplegables no daban señal de haberse guardado (Rosario, al dejar de guardar el audio).
      if (['theme', 'historyRetentionDays', 'keepAudioDays', 'flowBarMonitor', 'ollamaModel', 'ollamaUrl', 'modelDir', 'firstName'].includes(key)) toast(t('Guardado'));
      if (key === 'historyRetentionDays') renderTab();
      if (key === 'ollamaUrl') showOllamaUrlError(body, false);
      if (key === 'ollamaModel' || key === 'ollamaUrl') ollamaCheck(false);
    } catch (_) { renderTab(); }
  }

  ctl.el.addEventListener('click', async (e) => {
    const tb = e.target.closest('[data-stab]');
    if (tb) { go(tb.dataset.stab); return; }
    const sw = e.target.closest('[data-switch]');
    if (sw) {
      const k = sw.dataset.switch, v = !state.settings[k];
      sw.setAttribute('aria-checked', v);
      changeSetting(k, v);
      return;
    }
    const au = e.target.closest('[data-auto]');
    if (au) { changeSetting('meetingAuto', au.dataset.auto).then(() => { if (active === 'meetings') renderTab(); }); return; }
    const b = e.target.closest('[data-act]');
    if (!b) return;
    const act = b.dataset.act;
    if (act === 'shortcuts' || act === 'mic' || act === 'langs') openDialog(act, current);
    else if (act === 'ollama-check') ollamaCheck(true);
    else if (act === 'open-transforms') { ctl.close(); document.dispatchEvent(new CustomEvent('pronto:navigate', { detail: 'transforms' })); }
    else if (act === 'open-error-log') {
      try { await invoke('open_error_log'); } catch (er) { toast(errText(er), { kind: 'err' }); }
    }
    else if (act === 'dl-model') {
      const id = b.dataset.id;
      if (id === 'parakeet') { startModelDownload(); return; }
      const m = asrOpts && asrOpts.whisper.find((x) => x.id === id);
      asrDl = { file: m ? m.file : '', state: 'downloading', overallPercent: 0 };
      if (asrOpts) asrOpts.downloading = asrDl.file;
      paintEngine(body);
      try { await invoke('whisper_download_start', { id }); } catch (er) { asrDl = null; if (asrOpts) asrOpts.downloading = null; paintEngine(body); toast(errText(er), { kind: 'err' }); }
    }
    else if (act === 'use-model') {
      if (state.engine.loading || switching) return; // una sola carga a la vez
      const id = b.dataset.id;
      const patch = id === 'parakeet' ? { asrEngine: 'parakeet' } : { asrEngine: 'whisper', whisperModel: id };
      switching = true;
      try {
        await saveSettings(patch);
        toast(id === 'parakeet' ? t('Volviendo a Parakeet…') : t('Cargando {n}: la primera vez tarda hasta 40 s. Mientras, sigues dictando con el motor anterior.', { n: (asrOpts.whisper.find((x) => x.id === id) || {}).name || 'Whisper' }));
      } catch (_) { switching = false; }
      paintEngine(body);
      // Red de seguridad: si el motor no llegó a avisar de que carga, «Usar» no se queda apagado.
      setTimeout(() => { if (switching && !state.engine.loading) { switching = false; paintEngine(body); } }, 5000);
    }
    else if (act === 'update-check') {
      try {
        const r = await checkUpdate();
        const newer = r.info && (r.state === 'available' || r.state === 'error');
        toast(newer ? t('Hay una versión nueva: {v}', { v: r.info.version }) : t('Estás al día.'), { kind: 'info' });
      } catch (er) { toast(errText(er), { kind: 'err' }); }
    }
    else if (act === 'update-install') installUpdate();
    else if (act === 'capture-meeting') {
      b.disabled = true; b.textContent = t('Pulsa una combinación…');
      try {
        const hk = await invoke('capture_hotkey');
        if (hk && (hk.key || hk.ctrl || hk.alt || hk.shift || hk.win)) { await saveSettings({ hotkeyMeeting: hk }); toast(t('Tecla guardada')); }
      } catch (er) { const m = errText(er); if (!/cancel/i.test(m)) toast(m, { kind: 'err' }); }
      if (document.contains(b)) renderTab();
    }
    else if (act === 'clear-history') {
      const ok = await confirmDialog({
        title: t('¿Borrar todo el historial?'),
        body: t('Se eliminarán todas las transcripciones y su audio. Una vez eliminado, no se puede recuperar.'),
        confirmText: t('Borrar historial'),
      });
      if (!ok) return;
      try { await invoke('clear_history'); toast(t('Historial borrado')); document.dispatchEvent(new CustomEvent('pronto:history-cleared')); }
      catch (er) { toast(errText(er), { kind: 'err' }); }
    }
  });
  ctl.el.addEventListener('change', (e) => {
    const sel = e.target.closest('[data-select]');
    if (sel) {
      const k = sel.dataset.select;
      const num = ['historyRetentionDays', 'keepAudioDays'].includes(k);
      if (num && shortensRetention(k, state.settings[k], Number(sel.value))) { confirmRetention(sel, k); return; }
      if (k === 'flowBarMonitor') changeSetting(k, sel.value || null);   // '' = seguir al cursor
      else changeSetting(k, num ? Number(sel.value) : sel.value);
      return;
    }
    const tx = e.target.closest('[data-text]');
    if (tx) {
      const k = tx.dataset.text; let v = tx.value.trim();
      if (k === 'modelDir' && !v) { tx.value = state.settings.modelDir; return; }
      if (k === 'ollamaModel' && !v) { tx.value = state.settings.ollamaModel; return; }
      if (k === 'ollamaUrl' && !v) { tx.value = state.settings.ollamaUrl; return; }
      if (v !== state.settings[k]) changeSetting(k, v);
    }
  });
  // El error de «Ollama solo en este PC» sigue lo que se escribe, no el valor guardado antes (rompedor 23-oct, hub MEDIA 3).
  ctl.el.addEventListener('input', (e) => {
    if (e.target.matches('[data-text="ollamaUrl"]')) showOllamaUrlError(body, !isLoopbackUrl(e.target.value));
  });
  ctl.el.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && e.target.matches('[data-text]')) { e.preventDefault(); e.target.blur(); }
    // Alt+↑/↓ en un desplegable lo abre (Windows): no cambia de pestaña (rompedor 25-oct-b, BAJA 5).
    if (e.altKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown') && !e.target.matches('select, input, textarea')) {
      e.preventDefault();
      const all = flat().map((x) => x.id), i = all.indexOf(active);
      go(all[(i + (e.key === 'ArrowDown' ? 1 : -1) + all.length) % all.length]);
      const x = $(`[data-stab="${active}"]`, ctl.el); if (x) x.focus();
    } else if ((e.key === 'ArrowDown' || e.key === 'ArrowUp') && e.target.matches('.set-tab')) {
      e.preventDefault();
      const all = flat().map((x) => x.id), i = all.indexOf(active);
      go(all[(i + (e.key === 'ArrowDown' ? 1 : -1) + all.length) % all.length]);
      $(`[data-stab="${active}"]`, ctl.el).focus();
    }
  });

  const unsub = subscribe((what) => {
    if (what === 'settings-ext') renderTab();
    else if (what === 'update') { const g = $('#upd-group', body); if (g) g.outerHTML = updateGroupHtml(); }
    else if (what === 'engine') {
      if (!state.engine.loading) switching = false;
      const c = $('.set-eng', ctl.el); if (c) c.innerHTML = engineChip();
      const d = $('#eng-chip', body); if (d) d.innerHTML = engineChip();
      paintEngine(body);
    }
  });

  renderTab();
  // El modal enfoca su primer campo al abrirse; en «Limpieza e IA» está abajo y la pestaña se abría a media altura.
  requestAnimationFrame(() => requestAnimationFrame(() => { body.scrollTop = 0; }));
  if (dialog) openDialog(dialog, current);
  return ctl;
}

/** Abre la tecla, el micrófono o el idioma sin Configuración detrás (desde «Cómo se usa» y la portada). */
export function openSettingsDialog(kind) { openDialog(kind, current); }

function openDialog(kind, cur) {
  if (kind === 'shortcuts') openShortcutsDialog(cur);
  else if (kind === 'mic') openMicDialog(cur);
  else if (kind === 'langs') openLangDialog(cur);
}

// ------------------------------------------------------------------ diálogo de atajos
function openShortcutsDialog(cur) {
  const ROWS = () => [
    { key: 'hotkeyHold', title: t('Dictar'), desc: t('Mantén pulsadas las teclas mientras hablas; al soltarlas, se escribe.') },
    { key: 'hotkeyHandsFree', title: t('Dictar sin mantener (manos libres)'), desc: t('Pulsa una vez para empezar a dictar y otra para terminar.'),
      hint: () => t('También se activa tocando dos veces {keys}, la tecla de dictar.', { keys: keysHtml(state.settings.hotkeyHold) }) },
    { key: 'hotkeyPasteLast', title: t('Pegar la última transcripción'), desc: t('Vuelve a pegar tu último dictado en la app con foco.') },
    { key: 'hotkeyCopyLast', title: t('Copiar la última transcripción'), desc: t('Copia tu último dictado al portapapeles.') },
    { key: 'hotkeyTransform', title: t('Reescribir texto seleccionado'), desc: t('Reescribe con la IA local el texto que tengas seleccionado (opcional).') },
    { key: 'hotkeyMeeting', title: t('Grabar reunión'), desc: t('Empieza o termina la grabación de una reunión: tu micrófono y el sonido del ordenador.') },
    { fixed: 'Esc', title: t('Cancelar'), desc: t('Descarta el dictado en curso y las notificaciones.') },
  ];
  let capturing = null;

  const m = openModal({
    title: t('Teclas de Pronto'), subtitle: esc(t('Pulsa «Cambiar» en la fila que quieras y después la tecla o combinación nueva.')), width: 620,
    body: '<div id="sc-list" class="sc-list"></div><p class="form-err" id="sc-warn" role="alert" hidden></p>',
    footer: `<button class="btn btn-ghost" data-act="reset" style="margin-right:auto">${icon('rotate-ccw', 15)}${esc(t('Restablecer'))}</button>
      <button class="btn btn-primary" data-close>${esc(t('Listo'))}</button>`,
    onClose: () => { if (cur && current) current.rerender(); },
  });
  const list = $('#sc-list', m.el);

  function render() {
    list.innerHTML = ROWS().map((r, i) => {
      const isCap = capturing === r.key;
      const keys = r.fixed ? `<span class="keys"><kbd>${esc(r.fixed)}</kbd></span>` : isCap ? `<span class="cap-hint"><i class="dot pulse"></i>${esc(t('Pulsa una tecla o combinación…'))}</span>` : keysHtml(state.settings[r.key]);
      return `<div class="sc-row ${isCap ? 'cap' : ''}"><div class="sc-t"><b>${esc(r.title)}</b><span>${esc(r.desc)}</span>${r.hint && !isCap ? `<em>${r.hint()}</em>` : ''}</div>
        <div class="sc-k">${keys}${r.fixed ? '' : `<button class="btn btn-secondary btn-sm" data-cap="${r.key}" aria-label="${esc(isCap ? t('Esc para cancelar') : t('Cambiar: {x}', { x: r.title }))}" ${capturing && !isCap ? 'disabled' : ''}>${esc(isCap ? t('Esc para cancelar') : t('Cambiar'))}</button>`}</div></div>`;
    }).join('');
    const keys = ['hotkeyHold', 'hotkeyHandsFree', 'hotkeyPasteLast', 'hotkeyCopyLast', 'hotkeyTransform', 'hotkeyMeeting'];
    let dup = null;
    keys.forEach((a, i) => keys.slice(i + 1).forEach((b) => { if (sameHotkey(state.settings[a], state.settings[b])) dup = [a, b]; }));
    const w = $('#sc-warn', m.el);
    w.hidden = !dup;
    if (dup) w.textContent = t('Dos acciones comparten el mismo atajo ({k}). Cambia una de ellas.', { k: hotkeyText(state.settings[dup[0]]) });
  }

  async function capture(key) {
    capturing = key; render();
    try {
      const hk = await invoke('capture_hotkey');
      if (hk && (hk.key || hk.ctrl || hk.alt || hk.shift || hk.win)) { await saveSettings({ [key]: hk }); toast(t('Tecla guardada')); }
    } catch (e) {
      const msg = errText(e);
      if (!/cancel/i.test(msg)) toast(msg, { kind: 'err' });
    }
    capturing = null;
    if (document.contains(m.el)) render();
  }

  m.el.addEventListener('click', async (e) => {
    const c = e.target.closest('[data-cap]');
    if (c) { if (capturing) return; capture(c.dataset.cap); return; }
    const r = e.target.closest('[data-act=reset]');
    if (r) {
      try {
        const s = await invoke('reset_shortcuts');
        if (s) { state.settings = s; applySettingsEffects(s); notify('settings'); }
        render(); toast(t('Atajos restablecidos'));
      } catch (er) { toast(errText(er), { kind: 'err' }); }
    }
  });
  render();
}

// ------------------------------------------------------------------ diálogo de micrófono
async function openMicDialog(cur) {
  const m = openModal({
    title: t('Micrófono'), subtitle: esc(t('Elige qué micrófono usa Pronto para dictar.')), width: 520,
    body: `<div class="opt-list" id="mic-list" role="radiogroup" aria-label="${esc(t('Micrófono'))}"><div class="skel"></div><div class="skel"></div></div>`,
    footer: `<button class="btn btn-primary" data-close>${esc(t('Listo'))}</button>`,
    onClose: () => { if (cur && current) current.rerender(); },
  });
  let mics = [];
  try { mics = await invoke('list_mics'); } catch (e) { toast(errText(e), { kind: 'err' }); }
  const list = $('#mic-list', m.el);
  const render = () => {
    const sel = state.settings.micName;
    const opts = [{ v: null, label: t('El que use Windows'), sub: t('Pronto usa el micrófono que Windows tenga elegido') }, ...mics.map((n) => ({ v: n, label: n }))];
    list.innerHTML = opts.map((o, i) => `<button class="opt ${sel === o.v ? 'on' : ''}" role="radio" aria-checked="${sel === o.v}" data-i="${i}">
      <span class="radio"></span><span class="opt-t"><b>${esc(o.label)}</b>${o.sub ? `<small>${esc(o.sub)}</small>` : ''}</span></button>`).join('')
      + (mics.length ? '' : `<p class="muted xs">${esc(t('No se detectó ningún micrófono.'))}</p>`);
    list._opts = opts;
  };
  render();
  list.addEventListener('click', async (e) => {
    const b = e.target.closest('[data-i]'); if (!b) return;
    const o = list._opts[Number(b.dataset.i)];
    if (state.settings.micName === o.v) return;
    try { await saveSettings({ micName: o.v }); render(); toast(t('Micrófono guardado')); } catch (_) { /* ya avisado */ }
  });
}

// ------------------------------------------------------------------ diálogo de idiomas
// Con Parakeet el idioma se detecta solo: decirlo sin que suene a que el ajuste «no sirve» (Rosario, 24-oct-2026:
// «marco "Solo inglés", me lo guarda y me dice que no cambia nada»).
// Marcos, después: «¿Qué es Whisper? ¿Lo tengo o no?». Sin nombres de motores: qué pasa en su PC ahora mismo.
const langNote = () => ((state.settings.asrEngine || 'parakeet') === 'parakeet'
  ? t('No hace falta cambiar nada: Pronto ya distingue solo si hablas en español o en inglés.')
  : t('Pronto escuchará en el idioma que elijas aquí.'));

function openLangDialog(cur) {
  const opts = [
    { v: 'auto', label: t('Español e inglés (lo detecta solo)'), sub: t('Hablas en uno u otro y Pronto lo distingue. Si duda, español.') },
    { v: 'es', label: t('Solo español') }, { v: 'en', label: t('Solo inglés') },
  ];
  const m = openModal({
    title: t('Idioma en el que hablas'), width: 520,
    subtitle: esc(t('Elige en qué idioma vas a dictar.')),
    body: `<div class="opt-list" id="lang-list" role="radiogroup" aria-label="${esc(t('Idioma en el que hablas'))}"></div>
      <p class="muted sm lang-note">${esc(langNote())}</p>`,
    footer: `<button class="btn btn-primary" data-close>${esc(t('Listo'))}</button>`,
    onClose: () => { if (cur && current) current.rerender(); },
  });
  const list = $('#lang-list', m.el);
  const render = () => {
    const sel = state.settings.language;
    list.innerHTML = opts.map((o) => `<button class="opt ${sel === o.v ? 'on' : ''}" role="radio" aria-checked="${sel === o.v}" data-v="${o.v}">
      <span class="radio"></span><span class="opt-t"><b>${esc(o.label)}</b>${o.sub ? `<small>${esc(o.sub)}</small>` : ''}</span></button>`).join('');
  };
  render();
  list.addEventListener('click', async (e) => {
    const b = e.target.closest('[data-v]'); if (!b) return;
    if (state.settings.language === b.dataset.v) return;
    try { await saveSettings({ language: b.dataset.v }); render(); toast(t('Idioma guardado')); } catch (_) { /* ya avisado */ }
  });
}

export const settingsOpen = () => !!current;
