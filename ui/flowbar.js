/* ───────────────────────────────────────────────────────────────────────────
   Pronto 2 · Flow bar  (ventana «flowbar», 360x300, transparente)
   Eventos: dictation://state · dictation://level · dictation://notice
            settings://changed · flowbar://hover (opcional)
            transform://preview (vista previa de un Transform: nada se pega sin Enter)
   Comandos: start_dictation · stop_dictation · cancel_dictation
             set_flowbar_hitbox · flowbar_context_menu · get_settings
             list_transforms · run_transform · pin_flowbar
   Sin window.__TAURI__ (navegador) corre un mock; con ?demo añade un panel
   de depuración fuera de la píldora.
   ─────────────────────────────────────────────────────────────────────────── */
(() => {
  'use strict';

  const TAURI = window.__TAURI__;
  const MOCK = !TAURI;
  const params = new URLSearchParams(location.search);
  const DEMO = MOCK && params.has('demo');

  // ── Constantes de la spec (§3.2, §3.3) ────────────────────────────────────
  const STAGE_W = 360, STAGE_H = 300;     // = flowbar::SIZE (lo comprueba un test de flowbar.rs)
  const MARGIN_B = 8;                       // margen inferior de la píldora
  const SIZE = {                            // [ancho, alto] en px CSS
    rest: [40, 8],
    ready: [50, 30],
    listening: [73, 30],
    handsfree: [102.5, 30],
    processing: [98, 30],
    polishing: [136, 30],
    polished: [96, 30],
    preview: [118, 30],                     // vista previa de un Transform (esperando Enter / Esc)
    done: [50, 30],
    cancelled: [96, 30],
    error: [91, 30],
    meeting: [94, 30],                      // reunión: fit-content (borde verde 2 px, ondas + ■)
    summarizing: [158, 30],
    notesdone: [116, 30],
  };
  const STATES = new Set(['idle', 'listening', 'handsfree', 'processing', 'polishing',
                          'polished', 'done', 'cancelled', 'error', 'preview']);
  const BAR_COUNT = 10;
  const GAIN = 5;                           // audio-scale = max(1, nivel * 5)
  const LEVEL_BOOST = 4;                  // el RMS del micro llega bajo (~0.05-0.25): se eleva antes de aplicar la ganancia
  const MAX_SCALE = 4;                      // tope para que la barra no se salga de la píldora de 30 px
  const AUTO_BACK = { done: 1100, cancelled: 1500, polished: 1900 };   // ms hasta volver a reposo
  const CARD_MS = 4000;
  const NOTES_MS = 2600;                    // «Notas listas» en la píldora
  const SUMMARY_MAX_MS = 90000;
  const PREVIEW_MAX_EXTRA = 5000;           // seguro: la vista previa se cierra aunque no llegue el «cancelled» del plazo             // seguro: «Generando notas…» no se queda para siempre                     // las tarjetas se cierran solas
  const HOLD = DEMO && params.has('hold');   // demo: no cierra solo (para capturas)
  const HIT_REST = 12, HIT_NORMAL = 6;      // margen del rectángulo pulsable

  // ── Puente IPC (real o mock) ──────────────────────────────────────────────
  const mockHandlers = {};
  const mockEmit = (name, payload) => (mockHandlers[name] || []).slice().forEach((f) => f({ event: name, payload }));
  const mockSettings = {
    hotkeyHold: { ctrl: false, alt: true, shift: false, win: true, key: null },
    hotkeyHandsFree: { ctrl: false, alt: true, shift: false, win: true, key: 'Space' },
    showFlowBar: true,
    flowBarHiddenUntil: null,
    flowBarMonitor: params.has('pinned') ? '\\\\.\\DISPLAY2' : null,      // ?pinned: candado cerrado
    language: 'auto',
    appLanguage: params.get('lang') === 'en' ? 'en' : 'es',
  };
  const mockMeeting = { active: false, start: 0 };
  const bridge = MOCK ? {
    invoke: async (cmd, args) => {
      console.log('[mock invoke]', cmd, args);
      if (cmd === 'get_settings') return JSON.parse(JSON.stringify(mockSettings));
      if (cmd === 'meeting_status') return { active: mockMeeting.active, elapsedMs: mockMeeting.active ? Date.now() - mockMeeting.start : 0 };
      if (cmd === 'list_transforms') return [{ id: 1, name: 'Polish', prompt: '', builtin: true, hotkeyIndex: 1 }];
      if (cmd === 'pin_flowbar') {
        mockSettings.flowBarMonitor = args && args.pinned ? '\\\\.\\DISPLAY1' : null;
        mockEmit('settings://changed', JSON.parse(JSON.stringify(mockSettings)));
        return JSON.parse(JSON.stringify(mockSettings));
      }
      return null;
    },
    listen: async (name, cb) => {
      (mockHandlers[name] = mockHandlers[name] || []).push(cb);
      return () => { mockHandlers[name] = (mockHandlers[name] || []).filter((f) => f !== cb); };
    },
  } : { invoke: TAURI.core.invoke, listen: TAURI.event.listen };

  const invoke = (cmd, args) => Promise.resolve(bridge.invoke(cmd, args)).catch((e) => {
    console.warn('[flowbar] invoke falló:', cmd, e);
    throw e;
  });
  const safe = (cmd, args) => invoke(cmd, args).catch(() => null);

  // ── DOM ───────────────────────────────────────────────────────────────────
  const $ = (id) => document.getElementById(id);
  const stage = $('stage'), hot = $('hot'), pill = $('pill'), wavesEl = $('waves'),
        spinnerEl = $('spinner'), labelEl = $('label'), fillEl = $('fill'), tip = $('tip'),
        card = $('card'), cardTitle = $('card-title'), cardBody = $('card-body'),
        cardAct = $('card-act'), cardIco = $('card-ico'), liveEl = $('live'),
        hitDbg = $('hitdbg'), btnStop = $('btn-stop'), satNotes = $('sat-notes'), satLock = $('sat-lock'),
        pvEl = $('preview'), pvTitle = $('pv-title'), pvWarn = $('pv-warn'), pvWarnText = $('pv-warn-text'),
        pvText = $('pv-text'), pvNote = $('pv-note'), pvHint = $('pv-hint'), pvPaste = $('pv-paste'), pvDiscard = $('pv-discard');

  for (let i = 0; i < BAR_COUNT; i++) {
    const b = document.createElement('i');
    b.className = 'bar';
    const h = i - (BAR_COUNT - 1) / 2;                       // distancia al centro
    b.style.setProperty('--i', String(i));
    b.style.setProperty('--bs', String(Math.max(0, 1 - (h * h) / 48)));   // campana
    wavesEl.appendChild(b);
  }
  for (let i = 0; i < 8; i++) {
    const s = document.createElement('i');
    s.style.setProperty('--i', String(i));
    spinnerEl.appendChild(s);
  }

  // ── Estado ────────────────────────────────────────────────────────────────
  let ui = 'idle';              // estado del dictado que pinta la barra
  let hover = false;
  let settings = null;
  let level = 0, audioScale = 1, raf = 0;
  let backTimer = 0, hoverTimer = 0, cardTimer = 0, hiddenTimer = 0, cardHideTimer = 0;
  let cardOn = false, cardAction = null, cardKind = '';
  let tipEl = null;
  // Vista previa de un Transform: último `transform://preview` y si se está enseñando
  let pvData = null, pvOn = false;
  // Notetaker
  let meetingActive = false, meetingStart = 0, meetingLevel = 0, trans = '', transTimer = 0, clockTimer = 0;

  const reduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)');

  // ── Atajos ────────────────────────────────────────────────────────────────
  const KEYNAMES = { Space: 'Espacio', Escape: 'Esc', Enter: 'Intro', Backspace: 'Retroceso', Tab: 'Tab' };
  function hotkeyParts(h) {
    if (!h) return ['Win', 'Alt'];
    const p = [];
    if (h.ctrl) p.push('Ctrl');
    if (h.win) p.push('Win');
    if (h.alt) p.push('Alt');
    if (h.shift) p.push('Shift');
    if (h.key) p.push(KEYNAMES[h.key] || String(h.key));
    return p.length ? p : ['Win', 'Alt'];
  }
  const holdKeys = () => hotkeyParts(settings && settings.hotkeyHold);

  function keysNode(parts) {
    const w = document.createElement('span');
    w.className = 'keys';
    parts.forEach((k, i) => {
      if (i) { const p = document.createElement('i'); p.textContent = '+'; w.appendChild(p); }
      const kb = document.createElement('kbd'); kb.textContent = k; w.appendChild(kb);
    });
    return w;
  }
  const txt = (s) => { const e = document.createElement('span'); e.textContent = s; return e; };

  // ── Visibilidad en reposo ─────────────────────────────────────────────────
  function restVisible() {
    if (settings && settings.showFlowBar === false) return false;
    const until = settings && settings.flowBarHiddenUntil ? Date.parse(settings.flowBarHiddenUntil) : 0;
    return !(until && until > Date.now());
  }
  function armHiddenTimer() {
    clearTimeout(hiddenTimer);
    const until = settings && settings.flowBarHiddenUntil ? Date.parse(settings.flowBarHiddenUntil) : 0;
    if (until && until > Date.now()) hiddenTimer = setTimeout(render, Math.min(until - Date.now() + 50, 2 ** 31 - 1));
  }

  // ── Render principal ──────────────────────────────────────────────────────
  // prioridad: dictado > aviso transitorio de notas > reunión en curso > reposo/hover
  const key = () => (ui !== 'idle' ? ui : trans ? trans : meetingActive ? 'meeting' : (hover ? 'ready' : 'rest'));
  const isRest = () => { const k = key(); return k === 'rest' || k === 'ready'; };
  const LABELS = { polishing: 'Polish', polished: 'Listo', preview: 'Vista previa', cancelled: 'Cancelado', error: 'Error',
    summarizing: 'Generando notas…', notesdone: 'Notas listas' };
  const ARIA = {
    rest: 'Pronto: reposo', meeting: 'Grabando la reunión', summarizing: 'Generando notas', notesdone: 'Notas listas', ready: 'Dictar', listening: 'Escuchando', handsfree: 'Dictado en manos libres',
    processing: 'Transcribiendo', polishing: 'Puliendo el texto', polished: 'Listo', done: 'Pegado',
    preview: 'Vista previa del resultado: Enter para pegar, Esc para descartar',
    cancelled: 'Cancelado', error: 'Error',
  };

  function render() {
    const k = key();
    const [w, h] = SIZE[k];
    const hidden = isRest() && !restVisible();
    pill.style.setProperty('--w', w + 'px');
    pill.style.setProperty('--h', h + 'px');
    pill.dataset.s = k;
    hot.dataset.s = k;
    hot.dataset.hidden = hidden ? 'true' : 'false';
    labelEl.textContent = LABELS[k] || '';
    pill.setAttribute('aria-label', ARIA[k] || 'Dictar');
    wavesEl.classList.toggle('live', live());
    btnStop.dataset.tip = k === 'meeting' ? 'Detener el Meet' : 'Finalizar y pegar';
    btnStop.setAttribute('aria-label', btnStop.dataset.tip);
    syncLock();
    syncClock();
    ensureLoop();
    refreshTip();
    scheduleHitbox();
  }

  // ── Ondas: nivel suavizado por frame (s = .85·s + .15·objetivo) ───────────
  const live = () => ui === 'listening' || ui === 'handsfree' || (ui === 'idle' && !trans && meetingActive);
  function loop() {
    raf = 0;
    const lv = Math.min(1, Math.max(0, key() === 'meeting' ? meetingLevel : level) * LEVEL_BOOST);
    const target = Math.max(1, Math.min(MAX_SCALE, lv * GAIN));
    audioScale = 0.85 * audioScale + 0.15 * target;
    wavesEl.style.setProperty('--audio-scale', audioScale.toFixed(3));
    if (live()) raf = requestAnimationFrame(loop);
    else { audioScale = 1; wavesEl.style.removeProperty('--audio-scale'); }
  }
  function ensureLoop() { if (live() && !raf) raf = requestAnimationFrame(loop); }

  // ── Progreso de Polish (7 s lineales) ─────────────────────────────────────
  function startProgress() {
    fillEl.classList.remove('go');
    void fillEl.offsetWidth;
    fillEl.classList.add('go');
  }
  function stopProgress() { fillEl.classList.remove('go'); }

  // ── Eventos de dictado ────────────────────────────────────────────────────
  function announce(s) { liveEl.textContent = ''; if (s) setTimeout(() => { liveEl.textContent = s; }, 30); }

  function onState(p) {
    if (!p || !STATES.has(p.state)) return;
    clearTimeout(backTimer);
    const st = p.state;
    ui = st;
    if (st === 'polishing') startProgress(); else stopProgress();
    if (st === 'listening' || st === 'handsfree') { if (st === 'listening') level = 0; hideCard(true); }
    if (st === 'preview') { hideCard(true); showPreview(); } else hidePreview();
    if (st === 'preview' && !HOLD) backTimer = setTimeout(() => { ui = 'idle'; hidePreview(); render(); },
      ((pvData && pvData.timeoutMs) || 60000) + PREVIEW_MAX_EXTRA);
    if (st !== 'error' && st !== 'idle' && cardKind === 'error') hideCard(true);
    if (st === 'error') showCard(Object.assign({ kind: 'error' }, errorCard(p)));
    if (AUTO_BACK[st] && !HOLD) backTimer = setTimeout(() => { ui = 'idle'; render(); }, AUTO_BACK[st]);
    if (st === 'error' && !HOLD) backTimer = setTimeout(() => { ui = 'idle'; render(); }, CARD_MS);
    const msg = { listening: 'Escuchando', handsfree: 'Manos libres: pulsa el atajo para finalizar y pegar',
      processing: 'Transcribiendo', polishing: 'Puliendo el texto', polished: 'Listo, texto pulido',
      done: 'Texto pegado', cancelled: 'Cancelado', error: (p.message || 'Error de dictado'),
      preview: previewAnnouncement() }[st];
    announce(msg || '');
    render();
    ensureLoop();
  }

  function errorCard(p) {
    const msg = p.message || '';
    const mk = (title, body) => ({ title, body: body && body !== title ? body : '' });
    switch (p.kind) {
      case 'tap': return mk(`No lo toques. Mantén pulsado ${holdKeys().join(' + ')}.`,
                            'Mantén pulsado mientras hablas y suelta para ver el texto.');
      case 'nomic': return mk('No se detectó ningún micrófono', msg);
      case 'noaudio': return mk('¿Tienes el micrófono silenciado?', msg || 'No se recibió audio.');   // pico casi 0
      case 'empty': return mk('No se ha oído nada', msg);                                              // llegó audio, sin texto
      case 'paste': return mk('No se pudo pegar', msg);
      case 'ollama': return mk('Polish no está disponible', msg);
      case 'ia': return mk('No se pegó la respuesta de la IA', msg);   // guarda de la regla 7 (Transforms)
      case 'select': return mk('Selecciona texto primero', msg);
      default: return mk(msg || 'Algo salió mal', '');
    }
  }

  // ── Vista previa de un Transform (decisión A de Txonso, 23-oct) ──────────
  // El resultado de la IA se enseña aquí y SOLO se pega con Enter (o «Pegar»); Esc (o «Descartar»)
  // lo descarta y el backend devuelve el portapapeles. Enter/Esc los recoge el gancho de teclado:
  // esta ventana no toma el foco, la app destino lo conserva para el pegado. Todo con textContent.
  const PV_TXT = {
    es: { title: (n) => (n ? n + ' · ' : '') + 'revisa antes de pegar', paste: 'Pegar (Enter)', discard: 'Descartar (Esc)',
          hint: [['Enter'], ' para pegar · ', ['Esc'], ' para descartar'], announce: 'Vista previa lista. Enter para pegar, Esc para descartar.' },
    en: { title: (n) => (n ? n + ' · ' : '') + 'review before pasting', paste: 'Paste (Enter)', discard: 'Discard (Esc)',
          hint: [['Enter'], ' to paste · ', ['Esc'], ' to discard'], announce: 'Preview ready. Enter to paste, Esc to discard.' },
  };
  const pvTxt = () => PV_TXT[settings && settings.appLanguage === 'en' ? 'en' : 'es'];
  function previewAnnouncement() {
    const w = pvData && pvData.warning ? ' ' + pvData.warning : '';
    return pvTxt().announce + w;
  }
  function fillPreview() {
    const tx = pvTxt(), d = pvData || {};
    pvTitle.textContent = tx.title(d.name ? String(d.name) : '');
    pvText.textContent = d.text ? String(d.text) : '';
    pvWarnText.textContent = d.warning ? String(d.warning) : '';
    pvWarn.hidden = !d.warning;
    pvNote.textContent = d.notice ? String(d.notice) : '';
    pvNote.hidden = !d.notice;
    pvHint.replaceChildren(...tx.hint.map((h) => (Array.isArray(h) ? keysNode(h) : txt(h))));
    pvPaste.setAttribute('aria-label', tx.paste);       // botones de icono: ✓ pegar, ✕ descartar
    pvDiscard.setAttribute('aria-label', tx.discard);
  }
  function showPreview() {
    fillPreview();
    pvOn = true;
    pvEl.hidden = false;
    pvText.scrollTop = 0;
    pvEl.style.animation = 'none'; void pvEl.offsetWidth; pvEl.style.animation = '';
    scheduleHitbox();
  }
  function hidePreview() {
    if (!pvOn) return;
    pvOn = false;
    pvEl.hidden = true;
    scheduleHitbox();
  }
  function onPreview(p) {
    if (!p) return;
    pvData = p;
    if (pvOn) {                                   // p. ej. «vuelve a la ventana…» con la vista previa abierta
      fillPreview();
      if (p.notice) announce(String(p.notice));
      scheduleHitbox();
    }
  }
  pvPaste.addEventListener('click', (e) => { e.stopPropagation(); safe('stop_dictation'); });
  pvDiscard.addEventListener('click', (e) => { e.stopPropagation(); safe('cancel_dictation'); });
  if ('ResizeObserver' in window) new ResizeObserver(scheduleHitbox).observe(pvEl);

  // ── Tarjeta de aviso ──────────────────────────────────────────────────────
  const ACTIONS = {
    paste_last: ['Pegar último texto', 'paste_last'],
    copy_last: ['Copiar', 'copy_last'],
    open_settings: ['Configuración', 'open_settings'],
    open_hub: ['Abrir Pronto', 'open_hub'],
    start_meeting: ['Grabar', 'start_meeting'],
    stop_meeting: ['Detener', 'stop_meeting'],
    open_notetaker: ['Ver', 'open_hub', { page: 'notetaker' }],
  };
  const ICON_ERR = '<circle cx="12" cy="12" r="10"/><path d="M12 8v4"/><path d="M12 16h.01"/>';
  const ICON_OK = '<circle cx="12" cy="12" r="10"/><path d="m9 12 2 2 4-4"/>';
  const ICON_INFO = '<circle cx="12" cy="12" r="10"/><path d="M12 16v-4"/><path d="M12 8h.01"/>';

  // «open_meeting:{id}» → botón «Ver» que abre esa reunión en el hub
  function resolveAction(action) {
    if (!action) return null;
    const str = String(action);
    if (str.startsWith('open_meeting:')) {
      const id = Number(str.slice('open_meeting:'.length));
      return ['Ver', 'open_hub', Number.isFinite(id) ? { page: 'notetaker', id } : { page: 'notetaker' }];
    }
    return Object.hasOwn(ACTIONS, str) ? ACTIONS[str] : null; // acción desconocida: sin botón
  }
  const isMeetingAction = (a) => a && (a[1] === 'stop_meeting' || (a[1] === 'open_hub' && a[2] && a[2].page === 'notetaker'));

  function showCard({ kind, title, body, action }) {
    clearTimeout(cardHideTimer);
    cardOn = true;
    cardKind = kind;
    card.hidden = false;
    const a = resolveAction(action);
    const ok = kind !== 'error' && isMeetingAction(a);          // avisos de notas: verde discreto
    card.className = 'card' + (kind === 'error' ? ' error' : ok ? ' ok' : '');
    card.setAttribute('role', kind === 'error' ? 'alert' : 'status');
    cardIco.innerHTML = kind === 'error' ? ICON_ERR : ok ? ICON_OK : ICON_INFO;
    cardTitle.textContent = title || '';
    cardBody.textContent = body || '';
    cardAction = a;
    cardAct.hidden = !a;
    if (a) cardAct.textContent = a[0];
    card.style.animation = 'none'; void card.offsetWidth; card.style.animation = '';
    armCardTimer(a && a[1] ? 6000 : CARD_MS);
    refreshTip();
    scheduleHitbox();
  }
  function armCardTimer(ms) { clearTimeout(cardTimer); if (HOLD) return; cardTimer = setTimeout(() => hideCard(), ms); }
  function hideCard(immediate) {
    clearTimeout(cardTimer);
    if (!cardOn) return;
    cardOn = false;
    scheduleHitbox();
    if (immediate) { card.hidden = true; refreshTip(); return; }
    card.classList.add('out');
    clearTimeout(cardHideTimer);
    cardHideTimer = setTimeout(() => { card.hidden = true; card.classList.remove('out'); refreshTip(); }, 170);
  }
  $('card-x').addEventListener('click', () => { hideCard(); if (ui === 'error') { clearTimeout(backTimer); ui = 'idle'; render(); } });
  cardAct.addEventListener('click', () => {
    const a = cardAction;
    hideCard();
    if (a && a[1]) safe(a[1], a[2] || {});
  });
  card.addEventListener('mouseenter', () => { if (cardOn) clearTimeout(cardTimer); });
  card.addEventListener('mouseleave', () => { if (cardOn) armCardTimer(2000); });
  if ('ResizeObserver' in window) new ResizeObserver(scheduleHitbox).observe(card);

  // ── Tooltip ───────────────────────────────────────────────────────────────
  function tipNodes(el) {
    const t = el.dataset.tip;
    if (t === 'dyn') {
      if (key() === 'ready') return [txt('Dictar'), keysNode(holdKeys())];
      if (ui === 'handsfree') return [txt('Pulsa'), keysNode(holdKeys()), txt('para finalizar y pegar')];
      if (key() === 'meeting') return [txt('Reunión · grabando'), keysNode([fmtElapsed()])];
      return null;
    }
    return t ? [txt(t)] : null;
  }
  function fmtElapsed() {
    const t = Math.max(0, Math.floor((Date.now() - meetingStart) / 1000));
    const h = Math.floor(t / 3600), m = Math.floor((t % 3600) / 60), sec = t % 60;
    const p2 = (n) => String(n).padStart(2, '0');
    return h ? `${h}:${p2(m)}:${p2(sec)}` : `${p2(m)}:${p2(sec)}`;
  }
  // reloj local: solo corre mientras hay reunión y se está mirando el tooltip
  function syncClock() {
    const need = meetingActive && !!tipEl;
    if (need && !clockTimer) clockTimer = setInterval(refreshTip, 1000);
    if (!need && clockTimer) { clearInterval(clockTimer); clockTimer = 0; }
  }
  function refreshTip() {
    syncClock();
    const nodes = tipEl && !cardOn ? tipNodes(tipEl) : null;
    if (!nodes) { tip.classList.remove('on'); return; }
    tip.replaceChildren(...nodes);
    const tw = tip.offsetWidth, th = tip.offsetHeight;
    const r = tipEl.getBoundingClientRect(), s = stage.getBoundingClientRect();
    const cx = tipEl === pill ? STAGE_W / 2 : r.left - s.left + r.width / 2;
    const pillTop = STAGE_H - MARGIN_B - SIZE[key()][1];
    const x = Math.max(4, Math.min(STAGE_W - 4 - tw, cx - tw / 2));
    tip.style.left = Math.round(x) + 'px';
    tip.style.top = Math.round(pillTop - 6 - th) + 'px';
    tip.classList.add('on');
  }
  stage.addEventListener('mouseover', (e) => {
    const el = e.target.closest ? e.target.closest('[data-tip]') : null;
    if (el !== tipEl) { tipEl = el; refreshTip(); }
  });
  stage.addEventListener('mouseleave', () => { tipEl = null; refreshTip(); });

  // ── Hover / clic ──────────────────────────────────────────────────────────
  function setHover(v) { if (hover === v) return; hover = v; render(); }
  hot.addEventListener('mouseenter', () => { clearTimeout(hoverTimer); setHover(true); });
  hot.addEventListener('mouseleave', () => { clearTimeout(hoverTimer); hoverTimer = setTimeout(() => setHover(false), 60); });
  document.documentElement.addEventListener('mouseleave', () => { clearTimeout(hoverTimer); setHover(false); tipEl = null; refreshTip(); });
  window.addEventListener('blur', () => { setHover(false); });

  pill.addEventListener('click', (e) => {
    if (e.target.closest && e.target.closest('.btn')) return;
    if (ui !== 'idle') return;
    if (key() === 'meeting') safe('open_hub', { page: 'notetaker' });
    else if (!trans) safe('start_dictation', { handsFree: true });
  });
  document.addEventListener('contextmenu', (e) => {
    e.preventDefault();
    if (e.target.closest && e.target.closest('#pill')) safe('flowbar_context_menu', {});
  });
  $('btn-cancel').addEventListener('click', (e) => { e.stopPropagation(); safe('cancel_dictation'); });
  btnStop.addEventListener('click', (e) => {
    e.stopPropagation();
    if (key() === 'meeting') safe('stop_meeting'); else safe('stop_dictation');
  });
  satNotes.addEventListener('click', (e) => { e.stopPropagation(); safe('start_meeting', {}); });
  $('sat-polish').addEventListener('click', async (e) => {
    e.stopPropagation();
    try {
      const list = (await invoke('list_transforms')) || [];
      const t = list.find((x) => x.builtin && /polish/i.test(x.name)) || list[0];
      if (!t) throw new Error('sin transforms');
      await invoke('run_transform', { id: t.id });
    } catch (err) {
      showCard({ kind: 'notice', title: 'Polish no está disponible', body: String((err && err.message) || err || '') });
    }
  });

  // ── Candado: fijar la barra en este monitor ───────────────────────────────
  // Cerrado = `flowBarMonitor` tiene un monitor (aunque ahora no esté conectado: el ajuste se
  // conserva y la barra sigue al cursor hasta que vuelva). Lo resuelve el backend.
  const LOCK_TXT = {
    es: { pin: 'Fijar la barra en este monitor', unpin: 'Soltar la barra', fail: 'No se pudo fijar la barra' },
    en: { pin: 'Pin the bar to this monitor', unpin: 'Unpin the bar', fail: 'Could not pin the bar' },
  };
  const lockTxt = () => LOCK_TXT[settings && settings.appLanguage === 'en' ? 'en' : 'es'];
  const pinned = () => !!(settings && settings.flowBarMonitor);
  function syncLock() {
    const on = pinned(), tx = lockTxt();
    satLock.setAttribute('aria-pressed', on ? 'true' : 'false');
    satLock.dataset.tip = on ? tx.unpin : tx.pin;
    satLock.setAttribute('aria-label', satLock.dataset.tip);
  }
  satLock.addEventListener('click', async (e) => {
    e.stopPropagation();
    const s = await safe('pin_flowbar', { pinned: !pinned() });
    if (s) applySettings(s);
    else showCard({ kind: 'error', title: lockTxt().fail, body: '' });
  });

  // ── Click-through: rectángulo pulsable ────────────────────────────────────
  let hitRaf = 0, lastHit = '';
  function scheduleHitbox() { if (!hitRaf) hitRaf = setTimeout(sendHitbox, 0); }   // no rAF: se pausa en ventanas ocultas

  function computeHitbox() {
    const rects = [];                              // [x0, y0, x1, y1]
    const grow = (r, m) => [r[0] - m, r[1] - m, r[2] + m, r[3] + m];
    const k = key();
    if (!(isRest() && !restVisible())) {
      const [w, h] = SIZE[k];
      const x = (STAGE_W - w) / 2, y = STAGE_H - MARGIN_B - h;
      const m = isRest() ? HIT_REST : HIT_NORMAL;
      rects.push(grow([x, y, x + w, y + h], m));
      if (k === 'ready') {                         // satélite Polish
        const sx = x + w + 6, sy = STAGE_H - MARGIN_B - 30;
        rects.push(grow([sx, sy, sx + 30, sy + 30], m));
        const nx = x - 6 - 30;                      // satélite Tomar notas (a la izquierda)
        rects.push(grow([nx, sy, nx + 30, sy + 30], m));
        const lx = sx + 30 + 6, ly = sy + 3;        // candado (24x24, a la derecha de Polish)
        rects.push(grow([lx, ly, lx + 24, ly + 24], m));
      }
    }
    if (cardOn && !card.hidden) {
      const r = card.getBoundingClientRect(), s = stage.getBoundingClientRect();
      rects.push(grow([r.left - s.left, r.top - s.top, r.right - s.left, r.bottom - s.top], HIT_NORMAL));
    }
    if (pvOn && !pvEl.hidden) {                    // vista previa: botones y rueda para leerla entera
      const r = pvEl.getBoundingClientRect(), s = stage.getBoundingClientRect();
      rects.push(grow([r.left - s.left, r.top - s.top, r.right - s.left, r.bottom - s.top], HIT_NORMAL));
    }
    if (!rects.length) return null;
    let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
    rects.forEach((r) => { x0 = Math.min(x0, r[0]); y0 = Math.min(y0, r[1]); x1 = Math.max(x1, r[2]); y1 = Math.max(y1, r[3]); });
    x0 = Math.max(0, Math.floor(x0)); y0 = Math.max(0, Math.floor(y0));
    x1 = Math.min(STAGE_W, Math.ceil(x1)); y1 = Math.min(STAGE_H, Math.ceil(y1));
    return { x: x0, y: y0, width: x1 - x0, height: y1 - y0 };
  }
  function sendHitbox() {
    hitRaf = 0;
    const rect = computeHitbox();
    const sig = JSON.stringify(rect);
    if (DEMO) paintHitDbg(rect);
    if (sig === lastHit) return;
    lastHit = sig;
    safe('set_flowbar_hitbox', { rect });
  }
  pill.addEventListener('transitionend', (e) => { if (e.propertyName === 'width') scheduleHitbox(); });

  // ── Ajustes ───────────────────────────────────────────────────────────────
  function applySettings(s) {
    if (!s) return;
    settings = s;
    armHiddenTimer();
    if (pvOn) fillPreview();
    render();
  }
  safe('get_settings').then(applySettings);
  setTimeout(() => syncMeeting(), 0);

  // ── Notetaker (meeting://…) ───────────────────────────────────────────────
  function setTrans(t, ms) {
    clearTimeout(transTimer);
    trans = t;
    if (t && ms && !HOLD) transTimer = setTimeout(() => { trans = ''; render(); }, ms);
  }
  async function syncMeeting() {                 // al entrar (o al recargar la barra) con una reunión ya en marcha
    const st = await safe('meeting_status');
    if (st && st.active) { meetingStart = Date.now() - (st.elapsedMs || 0); meetingActive = true; render(); }
  }
  function onMeeting(p) {
    if (!p) return;
    switch (p.state) {
      case 'recording':
        meetingActive = true; meetingStart = Date.now(); meetingLevel = 0;
        setTrans('');
        if (cardOn && cardAction && cardAction[1] === 'start_meeting') hideCard(true);
        announce('Grabando la reunión');
        render();
        syncMeeting();                           // el backend manda el tiempo real transcurrido
        break;
      case 'summarizing':
        meetingActive = false;
        setTrans('summarizing', SUMMARY_MAX_MS);
        announce('Generando notas');
        render();
        break;
      case 'done':
        meetingActive = false;
        setTrans('notesdone', NOTES_MS);
        // el backend manda además un notice «Notas listas» con open_meeting:{id}; aquí solo si no hay ya uno
        if (!(cardOn && cardAction && cardAction[1] === 'open_hub')) {
          showCard({ kind: 'notice', title: 'Notas listas', body: p.message || '',
                     action: p.meetingId != null ? 'open_meeting:' + p.meetingId : 'open_notetaker' });
        }
        announce('Notas listas');
        render();
        break;
      case 'error':
        meetingActive = false;
        setTrans('');
        showCard({ kind: 'error', title: 'No se pudieron generar las notas', body: p.message || '' });
        announce(p.message || 'Error en las notas del Meet');
        render();
        break;
      default:                                    // idle
        meetingActive = false;
        if (trans === 'summarizing') setTrans('');
        render();
    }
  }

  // ── Suscripciones ─────────────────────────────────────────────────────────
  const on = (name, fn) => bridge.listen(name, (e) => fn(e && e.payload)).catch((err) => console.warn('[flowbar] listen', name, err));
  on('dictation://state', onState);
  on('dictation://level', (p) => { if (p && typeof p.level === 'number') level = p.level; });
  on('dictation://notice', (p) => {
    if (!p || !p.title) return;
    if (pvOn) { onPreview(Object.assign({}, pvData, { notice: p.body ? p.title + ' · ' + p.body : p.title })); return; }
    showCard({ kind: 'notice', title: p.title, body: p.body, action: p.action });
  });
  on('transform://preview', onPreview);
  on('meeting://state', onMeeting);
  on('meeting://level', (p) => { if (p) meetingLevel = Math.max(Number(p.mic) || 0, Number(p.system) || 0); });
  on('settings://changed', applySettings);
  on('flowbar://hover', (p) => setHover(!!(p && p.hover)));   // opcional: lo puede emitir el backend si la ventana no recibe mouseleave

  reduceMotion.addEventListener && reduceMotion.addEventListener('change', render);
  render();

  // ── Modo demo (solo navegador + ?demo) ────────────────────────────────────
  function paintHitDbg(rect) {
    if (!stage.classList.contains('frame')) { hitDbg.hidden = true; return; }
    if (!rect) { hitDbg.hidden = true; return; }
    hitDbg.hidden = false;
    hitDbg.style.left = rect.x + 'px'; hitDbg.style.top = rect.y + 'px';
    hitDbg.style.width = rect.width + 'px'; hitDbg.style.height = rect.height + 'px';
  }

  if (DEMO) {
    document.body.classList.add('demo');
    document.body.dataset.bg = params.get('bg') || 'claro';
    stage.classList.add('frame');
    if (params.get('zoom')) stage.style.zoom = params.get('zoom');          // solo para revisar capturas

    const panel = document.createElement('div');
    panel.className = 'dbg';
    panel.innerHTML = '<h4>Estado</h4><div class="g" id="d-states"></div>' +
      '<h4>Avisos y errores</h4><div class="g" id="d-notices"></div>' +
      '<h4>Meets</h4><div class="g" id="d-meet"></div>' +
      '<h4>Voz</h4><label>Nivel <input id="d-level" type="range" min="0" max="1" step="0.01" value="0.35"><span class="val" id="d-lv">0.35</span></label>' +
      '<label><input id="d-sim" type="checkbox"> Voz simulada (30 Hz)</label>' +
      '<h4>Fondo de prueba</h4><div class="g" id="d-bg"></div>' +
      '<h4>Ajustes</h4><label><input id="d-show" type="checkbox" checked> showFlowBar</label>' +
      '<label><input id="d-frame" type="checkbox" checked> Ver ventana 360x300 y hitbox</label>';
    document.body.appendChild(panel);
    if (params.get('nopanel')) panel.style.display = 'none';

    const mk = (parent, label, fn) => {
      const b = document.createElement('button'); b.type = 'button'; b.textContent = label;
      b.addEventListener('click', fn); $(parent).appendChild(b); return b;
    };
    const send = (state, extra) => mockEmit('dictation://state', Object.assign({ state }, extra));
    ['idle', 'listening', 'handsfree', 'processing', 'polishing', 'polished', 'done', 'cancelled']
      .forEach((s) => mk('d-states', s, () => send(s)));
    mk('d-states', 'error', () => send('error', { message: 'Algo ha fallado al transcribir.' }));
    const sendPv = (extra) => { mockEmit('transform://preview', Object.assign({ name: 'Formal', timeoutMs: 60000,
      text: 'Estimada Ana:\n\nLe envío el informe del trimestre y quedamos el lunes para revisarlo con calma. ' +
            'Si le parece bien, podemos aprovechar para cerrar el presupuesto de marketing y el calendario de entregas.\n\nUn saludo.' }, extra)); send('preview'); };
    mk('d-states', 'preview', () => sendPv({}));
    mk('d-states', 'preview + aviso', () => sendPv({ warning: 'Ojo: la IA añadió enlaces que no estaban en el texto seleccionado.' }));
    mk('d-states', 'preview <b>', () => sendPv({ text: '<b>no es HTML</b> <img src=x onerror=alert(1)> ' + 'texto largo '.repeat(80) }));
    mk('d-notices', 'Aviso', () => mockEmit('dictation://notice', { title: 'Texto copiado', body: 'Tu última transcripción está en el portapapeles.' }));
    mk('d-notices', 'Aviso + acción', () => mockEmit('dictation://notice', { title: 'No se pudo pegar', body: 'Haz clic en un cuadro de texto y pega con Ctrl + V.', action: 'paste_last' }));
    mk('d-notices', 'Error: tap', () => send('error', { kind: 'tap', message: 'No lo toques. Mantén pulsado Win + Alt.' }));
    mk('d-notices', 'Error: sin audio', () => send('error', { kind: 'noaudio', message: 'No hemos recibido audio de «Micrófono (Realtek)».' }));
    mk('d-notices', 'Error: vacío', () => send('error', { kind: 'empty', message: 'No se ha oído nada' }));
    mk('d-notices', 'Error: sin micro', () => send('error', { kind: 'nomic', message: 'Conecta un micrófono o elige otro en Configuración.' }));
    mk('d-notices', 'Error: pegar', () => send('error', { kind: 'paste', message: 'Haz clic en un cuadro de texto y pulsa Ctrl + V para pegar.' }));
    const sendM = (state, extra) => {
      if (state === 'recording') { mockMeeting.active = true; mockMeeting.start = Date.now() - (extra && extra.elapsedMs || 0); }
      else if (state !== 'recording') mockMeeting.active = false;
      mockEmit('meeting://state', Object.assign({ state }, extra));
    };
    mk('d-meet', 'Empezar (recording)', () => sendM('recording'));
    mk('d-meet', 'Reunión 12:34', () => sendM('recording', { elapsedMs: 754000 }));
    mk('d-meet', 'summarizing', () => sendM('summarizing'));
    mk('d-meet', 'done', () => sendM('done', { message: 'Reunión de equipo · 3 tareas' }));
    mk('d-meet', 'error', () => sendM('error', { message: 'Ollama no responde en localhost:11434.' }));
    mk('d-meet', 'idle', () => sendM('idle'));
    mk('d-meet', 'Aviso: Tomando notas', () => { sendM('recording'); mockEmit('dictation://notice', { title: 'Reunión: grabando Zoom', body: 'Se para solo al colgar.', action: 'stop_meeting' }); });
    mk('d-meet', 'Aviso: Notas listas', () => { sendM('done'); mockEmit('dictation://notice', { title: 'Notas listas', body: 'Reunión de equipo · lunes', action: 'open_meeting:42' }); });
    mk('d-meet', 'Aviso: ¿Tomar notas?', () => mockEmit('dictation://notice', { title: '¿Grabar esta reunión?', body: 'Zoom', action: 'start_meeting' }));
    ['claro', 'oscuro', 'color', 'mixto'].forEach((b) => mk('d-bg', b, () => { document.body.dataset.bg = b; }));

    const lvl = $('d-level');
    const setLv = (v) => { lvl.value = v; $('d-lv').textContent = Number(v).toFixed(2); mockEmit('dictation://level', { level: Number(v) }); };
    lvl.addEventListener('input', () => setLv(lvl.value));
    let simT = 0, simTimer = 0;
    $('d-sim').addEventListener('change', (e) => {
      clearInterval(simTimer);
      if (e.target.checked) simTimer = setInterval(() => {
        simT += 1 / 30;
        const env = 0.5 + 0.5 * Math.sin(simT * 2.1);           // «frases» con pausas
        const v = Math.max(0, env * (0.1 + 0.16 * Math.abs(Math.sin(simT * 9.3))) + (Math.random() - 0.5) * 0.04);
        setLv(v.toFixed(3));
        mockEmit('meeting://level', { mic: Number(v), system: Math.max(0, Number(v) * 0.6) });
      }, 33);
    });
    $('d-show').addEventListener('change', (e) => { mockSettings.showFlowBar = e.target.checked; mockEmit('settings://changed', JSON.parse(JSON.stringify(mockSettings))); });
    $('d-frame').addEventListener('change', (e) => { stage.classList.toggle('frame', e.target.checked); scheduleHitbox(); lastHit = ''; scheduleHitbox(); });

    // arranque por URL: ?demo&state=handsfree&level=.4&bg=oscuro&notice=1&hover=1
    const st0 = params.get('state'), ms0 = params.get('mstate');
    if (ms0) setTimeout(() => sendM(ms0, params.get('elapsed') ? { elapsedMs: Number(params.get('elapsed')) } : (ms0 === 'done' ? { message: 'Reunión de equipo · 3 tareas' } : undefined)), 60);
    if (params.get('level')) { setLv(params.get('level')); mockEmit('meeting://level', { mic: Number(params.get('level')), system: 0 }); }
    if (params.get('sim')) { $('d-sim').checked = true; $('d-sim').dispatchEvent(new Event('change')); }
    if (st0 === 'preview') setTimeout(() => sendPv(params.get('warn') ? { warning: 'Ojo: la IA añadió importes que no estaban en el texto seleccionado.' } : {}), 80);
    else if (st0) setTimeout(() => {
      const kind = params.get('kind');
      send(st0, kind ? { kind, message: params.get('msg') || 'Mensaje de ejemplo.' } : (st0 === 'error' ? { message: 'Algo ha fallado.' } : {}));
    }, 80);
    if (params.get('notice')) setTimeout(() => mockEmit('dictation://notice', { title: 'Texto copiado', body: 'Tu última transcripción está en el portapapeles.', action: params.get('notice') === 'act' ? 'paste_last' : undefined }), 120);
    if (params.get('hover')) setTimeout(() => setHover(true), 60);
  }
})();
