// Configuración: modal 960x640 con sidebar interna. [spec §2.4]
import { icon } from '../../shared/icons.js';
import { invoke, state, saveSettings, subscribe, notify, applySettingsEffects, IS_MOCK } from '../api.js';
import {
  t, esc, $, $$, toast, errText, openModal, confirmDialog, keysHtml, hotkeyText, sameHotkey, getLang,
} from '../util.js';
import { switchHtml, selectHtml } from '../ui.js';

export const VERSION = 'Pronto v2.0.0';
const TABS = () => [
  { group: t('Configuración'), items: [
    { id: 'general', icon: 'sliders-horizontal', label: t('General') },
    { id: 'system', icon: 'monitor', label: t('Sistema') },
    { id: 'meetings', icon: 'disc-2', label: t('Notas de reunión') },
    { id: 'experimental', icon: 'flask-conical', label: t('Experimental') },
  ] },
  { group: t('Privacidad'), items: [{ id: 'data', icon: 'shield-check', label: t('Datos y privacidad') }] },
];
const flat = () => TABS().flatMap((g) => g.items);

const row = (title, desc, control, extra = '') =>
  `<div class="srow ${extra}"><div class="srow-t"><b>${title}</b>${desc ? `<span>${desc}</span>` : ''}</div><div class="srow-c">${control}</div></div>`;
const group = (title, rows) => `<section class="sgroup">${title ? `<h3 class="overline">${esc(title)}</h3>` : ''}<div class="scard">${rows}</div></section>`;
const changeBtn = (act) => `<button class="btn btn-secondary btn-sm" data-act="${act}">${esc(t('Cambiar'))}</button>`;

// ------------------------------------------------------------------ pestañas
function engineChip() {
  const e = state.engine;
  if (e.error) return `<span class="badge err"><i class="dot"></i>${esc(t('Error'))}</span>`;
  if (e.loading) return `<span class="badge vio"><i class="dot pulse"></i>${esc(t('Cargando…'))}</span>`;
  return e.ready ? `<span class="badge ok"><i class="dot"></i>${esc(t('Listo'))}</span>` : `<span class="badge"><i class="dot"></i>${esc(t('Detenido'))}</span>`;
}
const ollamaNote = () => `<div class="aviso vio"><div class="in"><span class="ic-l">${icon('info', 18)}</span><div class="tx"><b>${esc(t('IA local opcional (Ollama)'))}</b>
  <span>${esc(t('Se usa para pulir textos (Polish y «Pulir cada dictado»), resumir reuniones y responder preguntas sobre ellas. Si no la instalas, el dictado y la transcripción funcionan igual. Puede ir más lento según tu PC.'))}
  ${t('Instálalo desde ollama.com y ejecuta {a}.', { a: '<code>ollama pull qwen2.5:7b</code>' })}</span></div></div></div>`;
const langName = (l) => ({ auto: t('Detección automática'), es: t('Español'), en: t('Inglés') }[l] || l);

const VIEWS = {
  general(s) {
    return group('', [
      row(t('Atajos'), t('Mantén {keys} y habla.', { keys: keysHtml(s.hotkeyHold) }), changeBtn('shortcuts')),
      row(t('Micrófono'), esc(s.micName || t('Predeterminado del sistema')), changeBtn('mic')),
      row(t('Idiomas de dictado'), esc(langName(s.language)), changeBtn('langs')),
      row(t('Idioma de la aplicación'), t('Selecciona tu idioma preferido para Pronto.'),
        selectHtml('appLanguage', [{ value: 'es', label: 'Español' }, { value: 'en', label: 'English' }], s.appLanguage, { label: t('Idioma de la aplicación') })),
      row(t('Nombre'), t('Se usa en el saludo de la pantalla de inicio.'),
        `<input class="input" data-text="firstName" maxlength="40" value="${esc(s.firstName || '')}" placeholder="${esc(t('Tu nombre'))}" aria-label="${esc(t('Nombre'))}" autocomplete="off" spellcheck="false">`),
    ].join(''));
  },
  system(s) {
    const hidden = s.flowBarHiddenUntil && new Date(s.flowBarHiddenUntil) > new Date();
    const hhmm = hidden ? new Date(s.flowBarHiddenUntil).toLocaleTimeString(getLang() === 'en' ? 'en-US' : 'es-ES', { hour: '2-digit', minute: '2-digit' }) : '';
    return group(t('Configuración de la aplicación'), [
      row(t('Abrir la aplicación al iniciar sesión'), t('Pronto arranca en la bandeja del sistema con Windows.'), switchHtml('launchAtLogin', s.launchAtLogin, t('Abrir la aplicación al iniciar sesión'))),
      row(t('Mostrar la Flow Bar en todo momento'), hidden ? t('Oculta hasta las {h}.', { h: hhmm }) : t('La píldora flotante se queda visible abajo, aunque no estés dictando.'), switchHtml('showFlowBar', s.showFlowBar, t('Mostrar la Flow Bar en todo momento'))),
    ].join('')) + group(t('Sonido'), [
      row(t('Sonidos de dictado y notificaciones'), t('Un pitido corto al empezar y al terminar.'), switchHtml('sounds', s.sounds, t('Sonidos de dictado y notificaciones'))),
      row(t('Silenciar la música mientras dictas'), t('Silencia el audio del sistema mientras grabas y lo restaura después.'), switchHtml('muteMusic', s.muteMusic, t('Silenciar la música mientras dictas'))),
    ].join('')) + group(t('Apariencia'), [
      row(t('Tema'), t('Oscuro por defecto; el claro usa el mismo cristal violeta.'),
        selectHtml('theme', [{ value: 'dark', label: t('Oscuro') }, { value: 'light', label: t('Claro') }], s.theme, { label: t('Tema') })),
    ].join(''));
  },
  meetings(s) {
    const cur = s.meetingAuto || (s.meetingDetect === false ? 'off' : 'auto');
    const opts = [
      { v: 'auto', label: t('Automático'), sub: t('Empieza al detectar Teams, Zoom, Meet… y para al colgar') },
      { v: 'ask', label: t('Preguntar antes'), sub: t('Te avisa al detectar una reunión y tú decides si tomar notas') },
      { v: 'off', label: t('Desactivado'), sub: t('Solo toma notas cuando tú las inicias') },
    ];
    return `<section class="sgroup"><h3 class="overline">${esc(t('Notas de reunión automáticas'))}</h3><div class="opt-list" role="radiogroup" aria-label="${esc(t('Notas de reunión automáticas'))}">${opts.map((o) => `<button class="opt ${cur === o.v ? 'on' : ''}" role="radio" aria-checked="${cur === o.v}" data-auto="${o.v}"><span class="radio"></span><span class="opt-t"><b>${esc(o.label)}</b><small>${esc(o.sub)}</small></span></button>`).join('')}</div></section>` + group('', [
      row(t('Atajo para las notas de reunión'), t('Empieza o termina la reunión con {keys}.', { keys: keysHtml(s.hotkeyMeeting) }),
        `<button class="btn btn-secondary btn-sm" data-act="capture-meeting" id="cap-meeting">${esc(t('Cambiar'))}</button>`),
      row(t('Guardar el audio de las reuniones'), t('Conserva en este equipo el audio de tu micrófono y el del sistema. Por defecto se descarta al terminar de transcribir.'), switchHtml('meetingKeepAudio', s.meetingKeepAudio, t('Guardar el audio de las reuniones'))),
    ].join('')) + ollamaNote() + group(t('Resumen · Ollama'), [
      row(t('Modelo del resumen'), t('El modelo de Ollama que resume la reunión y extrae tareas y decisiones.'),
        `<input class="input" data-text="ollamaModel" list="ollama-models" value="${esc(s.ollamaModel)}" aria-label="${esc(t('Modelo del resumen'))}" autocomplete="off" spellcheck="false"><datalist id="ollama-models"></datalist>`),
      row(t('Estado'), '<span id="oll-state" class="muted">—</span>', `<button class="btn btn-secondary btn-sm" data-act="ollama-check">${icon('refresh-cw', 14)}${esc(t('Comprobar'))}</button>`),
    ].join('')) + `<p class="consent" id="oll-meet-note" hidden>${icon('info', 14)}${esc(t('Sin Ollama las reuniones se transcriben, pero no se resumen. Puedes instalarlo cuando quieras; puede ir más lento según tu PC.'))}</p><div class="aviso amar"><div class="in"><span class="ic-l">${icon('info', 18)}</span><div class="tx"><span>${esc(t('Pide siempre consentimiento antes de transcribir a otras personas. Todo se procesa en este equipo.'))}</span></div></div></div>`;
  },
  experimental(s) {
    return group('', [
      row(t('Comando «pulsa enter»'), t('Pulsa Enter automáticamente cuando digas «pulsa enter» al final de un dictado.'), switchHtml('pressEnterCommand', s.pressEnterCommand, t('Comando «pulsa enter»'))),
    ].join('')) + ollamaNote() + group(t('Transforms · Ollama'), [
      row(t('Modelo'), t('El modelo de Ollama con el que se reescribe el texto.'),
        `<input class="input" data-text="ollamaModel" list="ollama-models" value="${esc(s.ollamaModel)}" aria-label="${esc(t('Modelo'))}" autocomplete="off" spellcheck="false"><datalist id="ollama-models"></datalist>`),
      row(t('URL de Ollama'), t('Dónde escucha Ollama (por defecto, en este mismo equipo).'),
        `<input class="input" data-text="ollamaUrl" value="${esc(s.ollamaUrl)}" aria-label="${esc(t('URL de Ollama'))}" autocomplete="off" spellcheck="false">`),
      row(t('Estado'), '<span id="oll-state" class="muted">—</span>', `<button class="btn btn-secondary btn-sm" data-act="ollama-check">${icon('refresh-cw', 14)}${esc(t('Comprobar'))}</button>`),
    ].join(''));
  },
  data(s) {
    const days = [0, 7, 30, 90, 180, 365].map((n) => ({ value: n, label: n === 0 ? t('Siempre') : t('{n} días', { n }) }));
    const audio = [0, 1, 7, 14, 30].map((n) => ({ value: n, label: n === 0 ? t('No guardar audio') : n === 1 ? t('1 día') : t('{n} días', { n }) }));
    return `<div class="aviso verde"><div class="in"><span class="ic-l">${icon('lock', 20)}</span>
      <div class="tx"><b>${esc(t('100 % local'))}</b><span>${esc(t('Tu voz, tus transcripciones, el diccionario y las notas viven solo en este equipo. Pronto no envía nada a Internet.'))}</span></div></div></div>` +
      group(t('Historial'), [
        row(t('Conservar el historial'), t('Las transcripciones más antiguas se eliminan solas.'), selectHtml('historyRetentionDays', days, s.historyRetentionDays, { label: t('Conservar el historial') })),
        row(t('Audio guardado'), t('Cuántos días se conserva el audio para reproducirlo o reintentar la transcripción.'), selectHtml('keepAudioDays', audio, s.keepAudioDays, { label: t('Audio guardado') })),
        row(t('Borrar historial'), t('Elimina todas las transcripciones y su audio. No se puede deshacer.'), `<button class="btn btn-danger btn-sm" data-act="clear-history">${icon('trash-2', 14)}${esc(t('Borrar historial'))}</button>`),
      ].join('')) +
      group(t('Motor de voz'), [
        row(t('Estado del motor'), esc(state.engine.error || t('NVIDIA Parakeet TDT 0.6B v3 · todo en tu CPU')), `<span id="eng-chip">${engineChip()}</span>`),
        row(t('Ruta del modelo'), t('Carpeta con encoder, decoder, joiner y tokens.'),
          `<input class="input wide" data-text="modelDir" value="${esc(s.modelDir)}" aria-label="${esc(t('Ruta del modelo'))}" autocomplete="off" spellcheck="false">`, 'stack'),
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
        <div class="set-foot"><span>${esc(VERSION)}</span><span class="set-eng" title="${esc(t('Estado del motor'))}">${engineChip()}</span></div></nav>
      <section class="set-main">
        <header class="set-hd"><h2 id="set-title"></h2><button class="icon-btn" data-close aria-label="${esc(t('Cerrar'))}">${icon('x', 18)}</button></header>
        <div class="set-scroll" id="set-body"></div></section></div>`,
    onClose: () => { unsub(); current = null; },
  });
  const body = $('#set-body', ctl.el), title = $('#set-title', ctl.el);
  ctl.modal.setAttribute('aria-labelledby', 'set-title');
  let active = tab;

  function renderTab() {
    const info = flat().find((x) => x.id === active);
    title.textContent = info.label;
    $$('.set-tab', ctl.el).forEach((b) => { const on = b.dataset.stab === active; b.setAttribute('aria-selected', on); b.tabIndex = on ? 0 : -1; });
    body.innerHTML = VIEWS[active](state.settings);
    body.scrollTop = 0;
    if (active === 'experimental' || active === 'meetings') ollamaCheck(false);
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

  async function changeSetting(key, value) {
    try {
      await saveSettings({ [key]: value });
      if (key === 'appLanguage') {
        document.dispatchEvent(new CustomEvent('pronto:lang'));
        const keep = active; ctl.close(); openSettings(keep); return;
      }
      if (key === 'theme') renderTab();
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
    else if (act === 'capture-meeting') {
      b.disabled = true; b.textContent = t('Pulsa una combinación…');
      try {
        const hk = await invoke('capture_hotkey');
        if (hk && (hk.key || hk.ctrl || hk.alt || hk.shift || hk.win)) await saveSettings({ hotkeyMeeting: hk });
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
      changeSetting(k, num ? Number(sel.value) : sel.value);
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
  ctl.el.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && e.target.matches('[data-text]')) { e.preventDefault(); e.target.blur(); }
    if (e.altKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
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
    else if (what === 'engine') {
      const c = $('.set-eng', ctl.el); if (c) c.innerHTML = engineChip();
      const d = $('#eng-chip', body); if (d) d.innerHTML = engineChip();
    }
  });

  renderTab();
  if (dialog) openDialog(dialog, current);
  return ctl;
}

function openDialog(kind, cur) {
  if (kind === 'shortcuts') openShortcutsDialog(cur);
  else if (kind === 'mic') openMicDialog(cur);
  else if (kind === 'langs') openLangDialog(cur);
}

// ------------------------------------------------------------------ diálogo de atajos
function openShortcutsDialog(cur) {
  const ROWS = () => [
    { key: 'hotkeyHold', title: t('Pulsar para hablar'), desc: t('Mantén pulsado para decir algo breve.') },
    { key: 'hotkeyHandsFree', title: t('Modo manos libres'), desc: t('Dicta sin mantener pulsado: pulsa este atajo para empezar y para terminar.'),
      hint: () => t('También: toca dos veces {keys}', { keys: keysHtml(state.settings.hotkeyHold) }) },
    { key: 'hotkeyPasteLast', title: t('Pegar la última transcripción'), desc: t('Vuelve a pegar tu último dictado en la app con foco.') },
    { key: 'hotkeyCopyLast', title: t('Copiar la última transcripción'), desc: t('Copia tu último dictado al portapapeles.') },
    { key: 'hotkeyTransform', title: t('Transform'), desc: t('Aplica Polish al texto seleccionado.') },
    { key: 'hotkeyMeeting', title: t('Notas de reunión'), desc: t('Empieza o termina la reunión con micrófono y audio del sistema.') },
    { fixed: 'Esc', title: t('Cancelar'), desc: t('Descarta el dictado en curso y las notificaciones.') },
  ];
  let capturing = null;

  const m = openModal({
    title: t('Atajos'), subtitle: esc(t('Elige tus atajos preferidos para usar Pronto.')), width: 620,
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
        <div class="sc-k">${keys}${r.fixed ? '' : `<button class="btn btn-secondary btn-sm" data-cap="${r.key}" ${capturing && !isCap ? 'disabled' : ''}>${esc(isCap ? t('Esc para cancelar') : t('Cambiar'))}</button>`}</div></div>`;
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
      if (hk && (hk.key || hk.ctrl || hk.alt || hk.shift || hk.win)) await saveSettings({ [key]: hk });
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
    const opts = [{ v: null, label: t('Predeterminado del sistema'), sub: t('Detección automática') }, ...mics.map((n) => ({ v: n, label: n }))];
    list.innerHTML = opts.map((o, i) => `<button class="opt ${sel === o.v ? 'on' : ''}" role="radio" aria-checked="${sel === o.v}" data-i="${i}">
      <span class="radio"></span><span class="opt-t"><b>${esc(o.label)}</b>${o.sub ? `<small>${esc(o.sub)}</small>` : ''}</span></button>`).join('')
      + (mics.length ? '' : `<p class="muted xs">${esc(t('No se detectó ningún micrófono.'))}</p>`);
    list._opts = opts;
  };
  render();
  list.addEventListener('click', async (e) => {
    const b = e.target.closest('[data-i]'); if (!b) return;
    const o = list._opts[Number(b.dataset.i)];
    try { await saveSettings({ micName: o.v }); render(); } catch (_) { /* ya avisado */ }
  });
}

// ------------------------------------------------------------------ diálogo de idiomas
function openLangDialog(cur) {
  const opts = [
    { v: 'auto', label: t('Detección automática'), sub: t('Español, inglés y 23 idiomas europeos más') },
    { v: 'es', label: t('Español') }, { v: 'en', label: t('Inglés') },
  ];
  const m = openModal({
    title: t('Idiomas de dictado'), width: 520,
    subtitle: esc(t('Elige el idioma en el que dictas.')),
    body: `<div class="opt-list" id="lang-list" role="radiogroup" aria-label="${esc(t('Idiomas de dictado'))}"></div>
      <div class="aviso vio"><div class="in"><span class="ic-l">${icon('info', 18)}</span>
      <div class="tx"><span>${esc(t('Parakeet TDT v3 detecta el idioma por sí solo (25 idiomas europeos, español e inglés incluidos). Este ajuste se guarda, pero hoy no cambia el motor.'))}</span></div></div></div>`,
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
    try { await saveSettings({ language: b.dataset.v }); render(); } catch (_) { /* ya avisado */ }
  });
}

export const settingsOpen = () => !!current;
