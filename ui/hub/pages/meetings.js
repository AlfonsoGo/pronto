// Meets (Contrato v12; antes «Notas de reunión»): lista, tarjeta de grabación, Ask y detalle con
// Resumen / Transcripción. Durante la reunión solo se graba (tiempo y nivel, sin texto); al parar, la
// fase 2 avanza por pasos y la transcripción llega con hablante, hora, etiquetas y turnos pulidos.
// Estructura de Wispr (Notetaker/**, hub_notetaker_*, hub_meetings_*); acabado de Pronto.
import { icon } from '../../shared/icons.js';
import { invoke, subscribe } from '../api.js';
import {
  t, esc, $, $$, toast, errText, debounce, formatTime, dayLabel, dayKey, openMenu, confirmDialog, getLang, formatInt,
} from '../util.js';
import { pageHeader, emptyState, tabsHtml, tabsKeys, searchBtn, highlight } from '../ui.js';
import { aiSlot, aiMount } from '../ai.js';
import { live, elapsedMs, formatClock, formatDuration, markStarted, markStopped, expectManual } from '../meeting.js';

const GEN_STEPS = () => [t('Leyendo la transcripción…'), t('Resumiendo la reunión…'), t('Extrayendo tareas y decisiones…')];
const spinner = () => '<i class="spin" aria-hidden="true"></i>';
/** «Regenerar» fallidos con notas buenas que se conservan (id -> motivo): se avisa sin esconder las notas
 *  (rompedor 23-oct, reuniones MEDIA 5). Vive mientras la app esté abierta; se borra al reintentar. */
const regenFailed = new Map();

/** Pasos de la fase 2 (Contrato v12): etapa del evento, peso en la barra total y textos. */
const STEPS = () => [
  { stage: 'transcribing', w: 55, label: t('Transcribir las dos pistas y unir la conversación'), short: t('Transcribiendo… {p} %') },
  { stage: 'polishing', w: 10, label: t('Pulir lo que no se entendió bien'), short: t('Repasando lo que no se entendió…') },
  { stage: 'labeling', w: 10, label: t('Marcar preguntas, propuestas, decisiones y tareas'), short: t('Etiquetando la conversación…') },
  { stage: 'understanding', w: 25, label: t('Resumir quién dijo qué'), short: t('Entendiendo la reunión…') },
];
/** Fase 2 de una reunión: paso en curso, texto corto para la lista y avance total. */
function phase(id) {
  const p = live.progress.get(id);
  if (!p) return null;
  const steps = STEPS();
  const idx = steps.findIndex((s) => s.stage === p.stage);
  if (idx < 0) return null;
  const pct = Math.max(0, Math.min(100, Number(p.percent) || 0));
  const total = steps.slice(0, idx).reduce((a, s) => a + s.w, 0) + Math.round(steps[idx].w * pct / 100);
  return { stage: p.stage, idx, pct, label: t(steps[idx].short, { p: pct }), total };
}
const pendingLabel = (id) => { const ph = phase(id); return ph ? ph.label : t('Generando resumen…'); };

/** Etiqueta de lo que escribe el modelo (decisión A de Txonso, 23-oct; art. 50.2 del Reglamento de IA). */
const aiLabel = () => t('Generado con IA local');
/** Cita literal de lo dicho que respalda una tarea o una decisión (decisión A de Txonso, 23-oct):
 *  quién lo dijo, cuándo (lleva a ese turno de la transcripción) y lo que se oyó. Las notas de antes
 *  no la tienen y se ven como siempre. */
function citaHtml(c, indent) {
  if (!c || !c.text) return '';
  const who = c.speaker === 'me' ? t('Tú') : t('Los demás');
  const at = Number(c.atMs) || 0;
  return `<p class="q-a mt-cita"${indent ? ' style="margin:0 0 9px 34px"' : ''}><b>${esc(who)}</b> <button class="q-time" data-act="goto" data-start="${at}"
    title="${esc(t('Ver en la transcripción'))}" aria-label="${esc(t('Ver la cita en la transcripción, minuto {t}', { t: formatClock(at) }))}">${formatClock(at)}</button> «${esc(c.text)}»</p>`;
}

/** Texto de respuesta de Ask: párrafos y listas con «- ». */
function mdLite(text) {
  const out = []; let list = null;
  String(text || '').split('\n').forEach((raw) => {
    const l = raw.trim();
    if (/^[-*•]\s+/.test(l)) { (list = list || []).push(l.replace(/^[-*•]\s+/, '')); return; }
    if (list) { out.push(`<ul>${list.map((x) => `<li>${esc(x)}</li>`).join('')}</ul>`); list = null; }
    if (l) out.push(`<p>${esc(l)}</p>`);
  });
  if (list) out.push(`<ul>${list.map((x) => `<li>${esc(x)}</li>`).join('')}</ul>`);
  return out.join('');
}

export default {
  id: 'meetings',
  mount(root, ctx) {
    let list = [], query = '', searchOpen = false, wantFocus = false, loaded = false, ollamaDown = false;
    let detail = null, tab = 'summary', tq = '', tqOpen = false, detailErr = null;
    let ask = { state: 'idle', q: '', a: '' };
    let alive = true, timer = null, genTimer = null, genIdx = 0, detailStage = null;

    root.innerHTML = '<div class="page-inner" id="mt-root"></div>';
    const el = $('#mt-root', root);
    const scroller = () => root; // #page es el contenedor con scroll

    const isLiveId = (id) => live.active && live.meeting && live.meeting.id === id;

    // ------------------------------------------------------------ Ask (barra flotante)
    function askHtml() {
      const scoped = !!detail;
      const ph = scoped ? t('Pregunta sobre esta reunión') : t('Pregunta sobre tus reuniones');
      let card = '';
      if (ask.state === 'thinking') {
        card = `<div class="ask-card" role="status"><div class="ask-q">${esc(ask.q)}</div>
          <div class="ask-think"><span class="dots"><i></i><i></i><i></i></span>${esc(t('Pensando'))}</div></div>`;
      } else if (ask.state === 'done') {
        card = `<div class="ask-card" aria-label="${esc(t('Respuesta de Ask'))}"><div class="ask-hd"><span class="ask-q">${esc(ask.q)}</span>
          <span class="ask-acts"><button class="icon-btn" data-act="ask-copy" aria-label="${esc(t('Copiar respuesta'))}" title="${esc(t('Copiar respuesta'))}">${icon('copy', 16)}</button>
          <button class="icon-btn" data-act="ask-close" aria-label="${esc(t('Cerrar'))}">${icon('x', 16)}</button></span></div>
          <div class="ask-a">${mdLite(ask.a)}</div>
          <p class="muted ask-ai" style="margin:8px 0 0;font-size:11.5px">${icon('sparkles', 12)} ${esc(aiLabel())}</p></div>`;
      } else if (ask.state === 'error') {
        card = `<div class="ask-card err"><div class="ask-hd"><span class="ask-q">${esc(ask.a)}</span>
          <button class="icon-btn" data-act="ask-close" aria-label="${esc(t('Cerrar'))}">${icon('x', 16)}</button></div></div>`;
      }
      return `<div class="ask-dock" id="ask-dock">${card}
        <form class="ask-bar" id="ask-form" autocomplete="off">
          <span class="ask-ic">${icon('sparkles', 18)}</span>
          <input class="ask-in" id="ask-in" type="text" placeholder="${esc(ph)}" aria-label="${esc(ph)}" ${ask.state === 'thinking' ? 'disabled' : ''} maxlength="400" spellcheck="false">
          <button class="ask-send" type="submit" aria-label="${esc(t('Preguntar'))}" hidden>${icon('arrow-up', 16)}</button>
        </form></div>`;
    }
    function renderAsk(keepValue = true) {
      const dock = $('#ask-dock', el); if (!dock) return;
      const prev = $('#ask-in', el); const v = keepValue && prev ? prev.value : '';
      const had = document.activeElement === prev;
      dock.outerHTML = askHtml();
      const inp = $('#ask-in', el);
      if (inp) { inp.value = v; syncSend(); if (had && !inp.disabled) inp.focus(); }
    }
    function syncSend() {
      const inp = $('#ask-in', el), b = $('.ask-send', el);
      if (inp && b) b.hidden = !inp.value.trim();
    }
    async function doAsk() {
      const inp = $('#ask-in', el); const q = (inp && inp.value || '').trim();
      if (!q || ask.state === 'thinking') return;
      ask = { state: 'thinking', q, a: '' }; renderAsk(false);
      const scope = detail ? detail.id : null;
      try {
        const a = await invoke('ask_meetings', { question: q, meetingId: scope });
        if (!alive) return;
        ask = { state: 'done', q, a: String(a || '').trim() || t('No encontré nada sobre eso en tus reuniones.') };
      } catch (e) { ask = { state: 'error', q, a: errText(e) || t('Algo salió mal. Inténtalo de nuevo.') }; }
      if (alive) { renderAsk(false); const inp2 = $('#ask-in', el); if (inp2) inp2.focus(); }
    }

    // ------------------------------------------------------------ lista
    function liveCardHtml() {
      if (!live.active || !live.meeting) return '';
      const m = live.meeting;
      return `<div class="tarj live-card" id="live-card"><div class="in">
        <div class="live-row">
          <button class="live-main" data-act="open-live" aria-label="${esc(t('Abrir la reunión en curso'))}">
            <span class="rec-dot" aria-hidden="true"></span>
            <span class="live-t"><b>${esc(m.title || t('Reunión sin título'))}</b><small>${live.auto ? esc(t('Iniciada automáticamente')) : esc(t('Grabando'))}${m.app ? ' · ' + esc(m.app) : ''}</small></span>
          </button>
          <span class="live-timer" data-timer>${formatClock(elapsedMs())}</span>
          <span class="live-meters" aria-hidden="true">
            <span class="meter" data-meter="mic"><em>${esc(t('Tú'))}</em><i><b></b></i></span>
            <span class="meter" data-meter="system"><em>${esc(t('Los demás'))}</em><i><b></b></i></span>
          </span>
          <button class="btn btn-secondary stop-btn" data-act="stop" aria-label="${esc(t('Dejar de escuchar'))}">${icon('square', 13)}${esc(t('Detener'))}</button>
        </div>
        <p class="live-last muted">${esc(t('Grabando tu voz y la de los demás por separado. Durante la reunión no se procesa nada: la transcripción y las notas se hacen al terminar.'))}</p>
      </div></div>`;
    }

    function rowHtml(m) {
      const status = m.summaryStatus === 'pending' ? `<span class="badge vio" data-badge>${spinner()}${esc(pendingLabel(m.id))}</span>`
        : m.summaryStatus === 'failed' ? `<span class="badge err">${icon('triangle-alert', 12)}${esc(t('Falló el resumen'))}</span>`
        : m.actionCount ? `<span class="chip-n" title="${esc(t('Tareas pendientes'))}">${icon('list-checks', 14)}${esc(m.actionCount === 1 ? t('1 tarea') : t('{n} tareas', { n: m.actionCount }))}</span>` : '';
      const meta = [formatTime(m.startedAt), formatDuration(m.durationMs), m.app].filter(Boolean).join(' · ');
      const prev = m.summaryStatus === 'done' ? m.preview : m.summaryStatus === 'pending' ? t('El resumen se está generando…') : m.summaryStatus === 'failed' ? t('La transcripción está guardada. Reintenta el resumen.') : (m.preview || t('Aún no hay resumen'));
      return `<div class="mrow" role="link" tabindex="0" data-id="${m.id}">
        <span class="m-av">${m.app ? esc(m.app.charAt(0).toUpperCase()) : icon('mic', 17)}</span>
        <div class="m-text"><div class="m-title">${highlight(m.title, query)}</div><div class="m-meta">${esc(meta)}</div><div class="m-prev">${highlight(prev, query)}</div></div>
        <div class="m-trail">${status}${m.summaryStatus === 'failed' ? `<button class="btn btn-secondary btn-sm" data-act="row-retry">${icon('rotate-ccw', 14)}${esc(t('Reintentar resumen'))}</button>` : ''}<button class="icon-btn m-more" data-act="row-more" aria-label="${esc(t('Más opciones de «{x}»', { x: m.title }))}">${icon('ellipsis', 18)}</button></div></div>`;
    }

    function listBodyHtml() {
      const rows = list.filter((m) => !isLiveId(m.id));
      if (!loaded) return `<div class="skel-list">${'<div class="skel"></div>'.repeat(3)}</div>`;
      if (!rows.length) {
        if (query) return emptyState('search', t('No se encontraron notas para "{q}"', { q: query }), '');
        if (live.active) return '';
        return emptyState('disc-2', t('Pronto toma notas de tus reuniones para que puedas estar presente'),
          t('Graba tu voz y la de los demás por separado y, al terminar, transcribe, une la conversación y saca quién dijo qué, preguntas, decisiones y tareas. Nada sale de este ordenador.'),
          `<button class="pill" data-act="start" style="margin-top:10px"><span>${esc(t('Grabar reunión'))}</span><span class="circ">${icon('arrow-up-right', 16)}</span></button>`);
      }
      let html = searchOpen ? `<div class="searchbar"><span class="sb-ic">${icon('search', 17)}</span>
        <input class="sb-in" data-search-input type="search" placeholder="${esc(t('Busca en tus reuniones'))}" value="${esc(query)}" aria-label="${esc(t('Busca en tus reuniones'))}" autocomplete="off" spellcheck="false">
        <button class="icon-btn" data-act="clear-search" aria-label="${esc(t('Cerrar búsqueda'))}">${icon('x', 16)}</button></div>` : '';
      let g = '', first = true;
      rows.forEach((m) => {
        const k = dayKey(m.startedAt);
        if (k !== g) {
          if (g) html += '</div></section>';
          g = k;
          html += `<section class="day"><div class="day-hd"><span class="overline">${esc(dayLabel(m.startedAt))}</span>${first && !searchOpen ? searchBtn() : ''}</div><div class="lista mlist">`;
          first = false;
        }
        html += rowHtml(m);
      });
      html += '</div></section>';
      html += `<p class="consent">${icon('info', 14)}${esc(t('Pide siempre consentimiento antes de transcribir a otras personas.'))}</p>`;
      return html;
    }

    function renderList() {
      detail = null; tab = 'summary'; tq = ''; tqOpen = false; ask = { state: 'idle', q: '', a: '' };
      const startBtn = live.active ? '' : `<button class="pill" data-act="start"><span>${esc(t('Grabar reunión'))}</span><span class="circ">${icon('arrow-up-right', 16)}</span></button>`;
      el.innerHTML = `${pageHeader(t('Reuniones'), { sub: t('Graba tus reuniones y entiende quién dijo qué, sin salir de tu equipo.'), actions: startBtn })}
        <div id="m-live">${liveCardHtml()}</div><div id="m-oll">${ollamaNoteHtml()}</div><div id="m-list">${listBodyHtml()}</div>${askHtml()}`;
      applyLevels();
    }
    function ollamaNoteHtml() {
      return ollamaDown ? `<p class="consent oll-note">${icon('info', 14)}${esc(t('Sin la IA local las reuniones se pasan a texto, pero no se resumen. Puedes activarla cuando quieras; puede ir más lento según tu PC.'))}</p><div class="oll-cta">${aiSlot('cta')}</div>` : '';
    }
    async function checkOllama() {
      try { ollamaDown = !(await invoke('ollama_status')).reachable; } catch (_) { ollamaDown = true; }
      const b = $('#m-oll', el); if (alive && b) b.innerHTML = ollamaNoteHtml();
    }
    function refreshListBody() {
      const b = $('#m-list', el); if (!b) return;
      b.innerHTML = listBodyHtml();
      if (searchOpen && wantFocus) { const i = $('[data-search-input]', b); if (i) { i.focus(); i.setSelectionRange(i.value.length, i.value.length); } }
    }
    async function loadList() {
      try { list = await invoke('list_meetings', { query: query || null, limit: 100, offset: 0 }); }
      catch (e) { list = []; toast(errText(e), { kind: 'err' }); }
      loaded = true;
      if (alive && !detail) refreshListBody();
    }
    const searchDeb = debounce(loadList, 250);

    function applyLevels() {
      const lv = live.level || { mic: 0, system: 0 };
      ['mic', 'system'].forEach((k) => {
        const b = $(`[data-meter=${k}] b`, el); if (b) b.style.setProperty('--v', Math.min(1, Math.max(0.04, lv[k] || 0)).toFixed(3));
      });
    }

    // ------------------------------------------------------------ detalle
    function tabsRow() {
      const tools = tab === 'transcript' ? (tqOpen
        ? `<div class="searchbar compact"><span class="sb-ic">${icon('search', 16)}</span><input class="sb-in" data-tq type="search" placeholder="${esc(t('Buscar en la transcripción…'))}" value="${esc(tq)}" aria-label="${esc(t('Buscar en la transcripción…'))}" autocomplete="off" spellcheck="false">
          <button class="icon-btn" data-act="tq-close" aria-label="${esc(t('Cerrar búsqueda'))}">${icon('x', 15)}</button></div>`
        : `<button class="icon-btn" data-act="tq-open" aria-label="${esc(t('Buscar en la transcripción…'))}" title="${esc(t('Buscar en la transcripción…'))}">${icon('search', 18)}</button>`) : '';
      return tabsHtml([{ id: 'summary', label: t('Resumen') }, { id: 'transcript', label: t('Transcripción') }], tab, tools);
    }

    function metaLine(d) {
      const when = new Date(d.startedAt).toLocaleString(getLang() === 'en' ? 'en-US' : 'es-ES', { weekday: 'long', day: 'numeric', month: 'long', hour: '2-digit', minute: '2-digit' });
      const dur = isLiveId(d.id) ? t('En directo') : formatDuration(d.durationMs);
      return [when, dur, d.app].filter(Boolean).join(' · ');
    }

    function detailHeadHtml() {
      const d = detail, lv = isLiveId(d.id);
      return `<button class="back" data-act="back">${icon('chevron-left', 17)}${esc(t('Reuniones'))}</button>
        <div class="md-head"><div class="md-t">
          <input class="md-title-in" id="md-title" value="${esc(d.title)}" maxlength="120" aria-label="${esc(t('Título de la reunión'))}" spellcheck="false">
          <div class="md-meta">${lv ? '<span class="rec-dot sm"></span>' : ''}<span id="md-meta">${esc(metaLine(d))}</span>${lv ? `<span class="live-timer sm" data-timer>${formatClock(elapsedMs())}</span>` : ''}</div></div>
          <div class="md-actions">
            ${lv ? `<button class="btn btn-secondary btn-sm" data-act="stop">${icon('square', 12)}${esc(t('Detener'))}</button>` : ''}
            <button class="btn btn-secondary btn-sm" data-act="copy-md">${icon('copy', 15)}${esc(t('Copiar'))}</button>
            <button class="icon-btn danger" data-act="del-detail" aria-label="${esc(t('Eliminar reunión'))}" title="${esc(t('Eliminar reunión'))}">${icon('trash-2', 17)}</button>
          </div></div>`;
    }

    function summaryHtml() {
      const d = detail, st = d.summaryStatus, lv = isLiveId(d.id);
      if (lv || (st === 'none' && live.active && live.meeting && live.meeting.id === d.id)) {
        return `<div class="tarj"><div class="in listening"><div class="wave sm" aria-hidden="true">${Array.from({ length: 11 }, (_, i) => `<i style="--i:${i}"></i>`).join('')}</div>
          <div><h3>${esc(t('Grabando'))}</h3><p>${esc(t('Durante la reunión solo se graba. Cuando termines, Pronto transcribirá cada voz, unirá la conversación y sacará las notas.'))}</p></div></div></div>`;
      }
      if (st === 'pending') {
        // Con avance de la fase 2: paso, porcentaje y barra; sin él (aún no ha llegado), la animación de siempre.
        const ph = phase(d.id);
        const head = !ph ? t('Generando las notas…') : t('Paso {a} de {b}', { a: ph.idx + 1, b: STEPS().length });
        const line = !ph ? GEN_STEPS()[genIdx % 3] : ph.label;
        const bar = ph ? `<div class="mc-prog gen-prog"><div class="mc-bar" role="progressbar" aria-label="${esc(head)}" aria-valuemin="0" aria-valuemax="100" aria-valuenow="${ph.total}"><i style="--p:${ph.total}%"></i></div></div>`
          : '<div class="gen-lines" aria-hidden="true"><i></i><i></i><i></i></div>';
        // Lista de pasos: hecho ✓, en curso (con su %) y pendientes.
        const steps = ph ? `<ol class="mt-steps">${STEPS().map((s, i) => {
          const state = i < ph.idx ? 'done' : i === ph.idx ? 'now' : 'todo';
          const mark = state === 'done' ? icon('check', 12) : state === 'now' ? '<i class="spin" aria-hidden="true"></i>' : `<span>${i + 1}</span>`;
          return `<li class="${state}"><span class="st-dot" aria-hidden="true">${mark}</span><span class="st-t">${esc(s.label)}</span>${state === 'now' && s.stage !== 'understanding' ? `<span class="st-p">${ph.pct} %</span>` : ''}</li>`;
        }).join('')}</ol>` : '';
        const note = t('Todo se hace en tu PC: primero se pasa cada voz a texto y luego la IA local lo ordena y lo resume. En reuniones largas puede tardar unos minutos.');
        return `<div class="tarj"><div class="in gen" role="status"><span class="gen-ring" aria-hidden="true"></span>
          <div><h3>${esc(head)}</h3><p id="gen-step">${esc(line)}</p>${bar}${steps}</div></div></div>
          <p class="muted xs gen-note">${esc(note)}</p>`;
      }
      if (st === 'failed') {
        return `<div class="aviso rojo"><div class="in"><span class="ic-l">${icon('triangle-alert', 20)}</span>
          <div class="tx"><b>${esc(t('No se pudo generar el resumen'))}</b><span>${esc(t('La transcripción está guardada. Comprueba que la IA local esté activada y vuelve a intentarlo.'))}</span></div>
          <button class="btn btn-secondary btn-sm" data-act="retry">${icon('rotate-ccw', 14)}${esc(t('Reintentar resumen'))}</button></div></div>`;
      }
      if (st === 'none' || (!d.summary && !d.actionItems.length)) {
        return `<div class="tarj"><div class="in listening"><span class="empty-ic">${icon('sparkles', 24)}</span>
          <div><h3>${esc(d.segments.length < 2 ? t('La reunión fue demasiado corta para resumirla.') : t('Esta reunión no tiene resumen'))}</h3>
          ${d.segments.length >= 2 ? `<p>${esc(t('Genéralo ahora a partir de la transcripción.'))}</p><button class="btn btn-primary btn-sm" data-act="retry" style="margin-top:12px">${esc(t('Generar resumen'))}</button>` : ''}</div></div></div>`;
      }
      const done = d.actionItems.filter((a) => a.done).length;
      const regenWarn = regenFailed.has(d.id) ? `<div class="aviso amar" role="alert"><div class="in"><span class="ic-l">${icon('triangle-alert', 20)}</span>
          <div class="tx"><b>${esc(t('No se pudo regenerar las notas'))}</b><span>${esc(regenFailed.get(d.id) || t('Se conservan las notas anteriores.'))}</span></div>
          <button class="btn btn-secondary btn-sm" data-act="retry">${icon('rotate-ccw', 14)}${esc(t('Reintentar'))}</button></div></div>` : '';
      // Campos del Contrato v10 y v12: las reuniones anteriores no los tienen y sus secciones no se pintan.
      const keyPoints = d.keyPoints || [], nextSteps = d.nextSteps || [], openQs = d.openQuestions || [];
      const asked = d.questions || [], saidMe = d.saidByMe || [], saidThem = d.saidByThem || [];
      const bul = (items) => `<ul class="bul">${items.map((x) => `<li>${esc(x)}</li>`).join('')}</ul>`;
      const none = `<p class="muted">${esc(t('Nada pendiente.'))}</p>`;
      const who = (sp) => `<span class="who ${sp === 'me' ? 'me' : 'them'}">${esc(sp === 'me' ? t('Tú') : t('Los demás'))}</span>`;
      const questionsHtml = asked.length ? `<section class="tarj msec mt-gap"><div class="in"><div class="card-hd"><h3>${icon('circle-help', 17)}${esc(t('Preguntas'))}</h3>
          <span class="badge">${esc(t('{a} de {b} resueltas', { a: asked.filter((q) => q.answered).length, b: asked.length }))}</span></div>
          <ul class="mt-qs">${asked.map((q) => `<li><div class="q-hd">${who(q.askedBy)}<button class="q-time" data-act="goto" data-start="${q.atMs}" title="${esc(t('Ver en la transcripción'))}">${formatClock(q.atMs)}</button>
            <span class="q-st ${q.answered ? 'ok' : 'open'}">${q.answered ? icon('check', 12) + esc(t('Respondida')) : esc(t('Sin resolver'))}</span></div>
            <p class="q-t">${esc(q.text)}</p>${q.answer ? `<p class="q-a"><b>${esc(q.askedBy === 'me' ? t('Los demás') : t('Tú'))}:</b> ${esc(q.answer)}</p>` : ''}</li>`).join('')}</ul></div></section>` : '';
      const saidHtml = saidMe.length || saidThem.length ? `<section class="tarj msec mt-gap"><div class="in"><h3>${icon('message-square', 17)}${esc(t('Quién dijo qué'))}</h3>
          <div class="mt-said"><div>${who('me')}${saidMe.length ? bul(saidMe) : `<p class="muted">${esc(t('Nada destacable.'))}</p>`}</div>
          <div>${who('them')}${saidThem.length ? bul(saidThem) : `<p class="muted">${esc(t('Nada destacable.'))}</p>`}</div></div></div></section>` : '';
      return `
        ${regenWarn}
        <section class="tarj msec"><div class="in"><div class="card-hd"><h3>${icon('file-text', 17)}${esc(t('Resumen'))}</h3>
          <span class="badge mt-ai" title="${esc(t('El resumen, las tareas y las decisiones los escribe la IA de tu equipo; cada tarea y decisión lleva la cita de lo que se dijo.'))}">${icon('sparkles', 12)}${esc(aiLabel())}</span></div>
          <p class="m-sum">${esc(d.summary || '')}</p></div></section>
        ${saidHtml}
        <section class="tarj msec mt-gap"><div class="in"><div class="card-hd"><h3>${icon('list-checks', 17)}${esc(t('Tareas pendientes'))}</h3>
          ${d.actionItems.length ? `<span class="badge ${done === d.actionItems.length ? 'ok' : ''}">${esc(t('{a} de {b} hechas', { a: done, b: d.actionItems.length }))}</span>` : ''}</div>
          ${d.actionItems.length ? `<ul class="todo">${d.actionItems.map((a, i) => `<li><button class="todo-b ${a.done ? 'done' : ''}" role="checkbox" aria-checked="${!!a.done}" data-act="todo" data-i="${i}">
            <span class="check" aria-hidden="true" ${a.done ? 'aria-checked="true"' : ''}>${icon('check', 12)}</span><span class="todo-t">${esc(a.text)}</span>${a.due ? `<span class="due">${icon('clock', 12)}${esc(a.due)}</span>` : ''}${a.owner ? `<span class="owner">${esc(a.owner)}</span>` : ''}</button>${citaHtml(a.cita, true)}</li>`).join('')}</ul>`
            : `<p class="muted">${esc(t('No hay tareas pendientes para esta reunión'))}</p>`}</div></section>
        ${questionsHtml}
        <div class="msec-two mt-gap">
          <section class="tarj msec"><div class="in"><h3>${icon('circle-check', 17)}${esc(t('Decisiones'))}</h3>
            ${d.decisions.length ? `<ul class="bul">${d.decisions.map((x, i) => `<li>${esc(x)}${citaHtml((d.decisionCitas || [])[i])}</li>`).join('')}</ul>` : `<p class="muted">${esc(t('No se registraron decisiones.'))}</p>`}</div></section>
          <section class="tarj msec"><div class="in"><h3>${icon('hash', 17)}${esc(t('Temas'))}</h3>
            ${d.topics.length ? `<div class="topics">${d.topics.map((x) => `<span class="chip static">${esc(x)}</span>`).join('')}</div>` : `<p class="muted">${esc(t('Sin temas.'))}</p>`}</div></section>
        </div>
        ${keyPoints.length ? `<section class="tarj msec mt-gap"><div class="in"><h3>${icon('zap', 17)}${esc(t('Puntos clave'))}</h3>${bul(keyPoints)}</div></section>` : ''}
        ${nextSteps.length || openQs.length ? `<div class="msec-two mt-gap">
          <section class="tarj msec"><div class="in"><h3>${icon('arrow-up-right', 17)}${esc(t('Próximos pasos'))}</h3>${nextSteps.length ? bul(nextSteps) : none}</div></section>
          <section class="tarj msec"><div class="in"><h3>${icon('circle-help', 17)}${esc(t('Preguntas abiertas'))}</h3>${openQs.length ? bul(openQs) : none}</div></section>
        </div>` : ''}
        <div class="regen"><button class="btn btn-secondary btn-sm" data-act="retry" title="${esc(t('Vuelve a sacar el resumen y las tareas a partir de la transcripción'))}">${icon('rotate-ccw', 14)}${esc(t('Regenerar notas'))}</button></div>`;
    }

    /** Etiqueta de un turno (Contrato v12); «otro» no se pinta. */
    const KIND = () => ({ pregunta: t('Pregunta'), respuesta: t('Respuesta'), propuesta: t('Propuesta'), decision: t('Decisión'), tarea: t('Tarea') });
    const showRaw = new Set(); // turnos pulidos con el original a la vista (por inicio)
    function bubbleHtml(s, all) {
      const me = s.speaker === 'me';
      const kind = s.kind && KIND()[s.kind];
      const reply = s.replyTo != null && all && all[s.replyTo] ? all[s.replyTo] : null;
      const raw = !!s.rawText && showRaw.has(s.startMs);
      const end = s.endMs > s.startMs ? `–${formatClock(s.endMs)}` : '';
      return `<div class="bub ${me ? 'me' : 'them'}${s.overlap ? ' ov' : ''}" data-start="${s.startMs}"><div class="bub-hd"><b>${esc(me ? t('Tú') : t('Los demás'))}</b><time>${formatClock(s.startMs)}${end}</time>
        ${kind ? `<span class="bub-kind k-${esc(s.kind)}">${esc(kind)}</span>` : ''}
        ${s.overlap ? `<span class="bub-ov" title="${esc(t('Empezó mientras la otra persona aún hablaba'))}">${esc(t('a la vez'))}</span>` : ''}
        ${reply ? `<button class="bub-re" data-act="goto" data-start="${reply.startMs}" title="${esc(t('Ver la pregunta'))}">${esc(t('responde a {t}', { t: formatClock(reply.startMs) }))}</button>` : ''}
        ${s.rawText ? `<button class="bub-pol" data-act="raw" data-start="${s.startMs}" aria-pressed="${raw}" title="${esc(t('Este turno se entendía mal y se pulió con IA'))}">${icon('sparkles', 12)}${esc(raw ? t('Ver pulido') : t('Pulido · ver original'))}</button>` : ''}</div>
        <p>${highlight(raw ? s.rawText : s.text, tq)}</p></div>`;
    }
    function transcriptHtml() {
      const d = detail, lv = isLiveId(d.id);
      const all = d.segments;
      const q = tq.toLowerCase();
      const segs = all.filter((s) => !tq || s.text.toLowerCase().includes(q) || (s.rawText || '').toLowerCase().includes(q));
      if (lv) {
        // Fase 1: solo se graba; no hay texto hasta que termine.
        return `<div class="tx" id="tx"><div class="tx-empty"><span class="dots live"><i></i><i></i><i></i></span><p>${esc(t('Grabando. La transcripción aparecerá cuando termines la reunión.'))}</p></div></div>`;
      }
      if (!all.length) {
        const pending = d.summaryStatus === 'pending';
        return `<div class="tx" id="tx">${pending ? `<div class="tx-empty"><span class="dots live"><i></i><i></i><i></i></span><p>${esc(t('Transcribiendo cada voz por separado…'))}</p></div>` : emptyState('message-square', t('No se transcribió nada'), '')}</div>`;
      }
      if (!segs.length) return `<div class="tx" id="tx">${emptyState('search', t('No se encontró «{q}» en la transcripción', { q: tq }), '')}</div>`;
      // Transcripción sin la pasada final (versión anterior o fase 2 sin terminar).
      const preview = !d.transcriptFinal
        ? `<p class="consent tx-note">${icon('info', 14)}${esc(t('Transcripción incompleta: reinténtalo para completarla.'))}</p>` : '';
      return `<div class="tx" id="tx">${preview}${segs.map((s) => bubbleHtml(s, all)).join('')}</div>`;
    }

    function renderDetailBody() {
      const b = $('#md-body', el); if (!b) return;
      b.innerHTML = tab === 'summary' ? summaryHtml() : transcriptHtml();
      if (tab === 'summary' && detail.summaryStatus === 'pending') startGen(); else stopGen();
    }
    function startGen() {
      if (genTimer) return;
      genTimer = setInterval(() => {
        if (detail && phase(detail.id)) return; // el avance real manda
        genIdx++; const s = $('#gen-step', el);
        if (s) { s.style.opacity = '0'; setTimeout(() => { s.textContent = GEN_STEPS()[genIdx % 3]; s.style.opacity = '1'; }, 200); }
      }, 2600);
    }
    function stopGen() { if (genTimer) { clearInterval(genTimer); genTimer = null; } }

    function renderDetail() {
      stopGen();
      el.innerHTML = `${detailHeadHtml()}<div id="md-tabs">${tabsRow()}</div><div id="md-body"></div>${askHtml()}`;
      renderDetailBody();
    }

    async function openDetail(id, keepTab) {
      detailErr = null;
      try {
        const d = await invoke('get_meeting', { id });
        if (!alive) return;
        detail = d;
        if (!keepTab) tab = isLiveId(id) ? 'transcript' : 'summary';
        if (!keepTab) { ask = { state: 'idle', q: '', a: '' }; tq = ''; tqOpen = false; genIdx = 0; }
        renderDetail();
        root.scrollTo({ top: 0 });
      } catch (e) { toast(errText(e), { kind: 'err' }); detail = null; renderList(); loadList(); }
    }
    async function reloadDetail() {
      if (!detail) return;
      try {
        const d = await invoke('get_meeting', { id: detail.id });
        if (!alive || !detail || detail.id !== d.id) return;
        const statusChanged = d.summaryStatus !== detail.summaryStatus || d.title !== detail.title;
        detail = d;
        if (statusChanged || tab === 'summary') { const h = $('#md-meta', el); if (h) h.textContent = metaLine(d); renderDetailBody(); }
      } catch (_) { /* */ }
    }

    // ------------------------------------------------------------ acciones
    async function startMeeting() {
      try {
        expectManual();
        const m = await invoke('start_meeting', { title: null });
        markStarted(m);
        toast(t('Reunión en marcha: se graba tu micrófono y el sonido del ordenador. Las notas se hacen al terminar.'), { kind: 'info', ms: 3200 });
      } catch (e) { expectManual(false); toast(errText(e) || t('No se pudo iniciar la reunión.'), { kind: 'err' }); }
    }
    async function stopMeeting() {
      try {
        const m = await invoke('stop_meeting');
        markStopped(m);
        toast(t('Reunión guardada. Generando las notas…'));
        if (detail && detail.id === (m && m.id)) { await openDetail(m.id); tab = 'summary'; renderDetail(); }
        else { renderList(); loadList(); }
      } catch (e) { toast(errText(e), { kind: 'err' }); }
    }
    async function copyMarkdown(id) {
      try { const md = await invoke('export_meeting', { id }); await invoke('copy_text', { text: md }); toast(t('Notas copiadas como Markdown')); }
      catch (e) { toast(errText(e), { kind: 'err' }); }
    }
    async function deleteMeeting(id, title) {
      const ok = await confirmDialog({
        title: t('¿Eliminar reunión?'),
        body: t('Esto eliminará la nota y la transcripción de forma permanente. Esta acción no se puede deshacer'),
      });
      if (!ok) return false;
      try { await invoke('delete_meeting', { id }); toast(t('Reunión eliminada')); return true; }
      catch (e) { toast(errText(e) || t('No se pudo eliminar la reunión. Inténtalo de nuevo.'), { kind: 'err' }); return false; }
    }
    async function retrySummary(id) {
      try {
        await invoke('retry_summary', { id });
        regenFailed.delete(id);
        if (detail && detail.id === id) { detail.summaryStatus = 'pending'; genIdx = 0; renderDetailBody(); }
        else { const m = list.find((x) => x.id === id); if (m) { m.summaryStatus = 'pending'; refreshListBody(); } }
      } catch (e) { toast(errText(e), { kind: 'err' }); }
    }
    async function toggleTodo(i) {
      const a = detail.actionItems[i]; if (!a) return;
      a.done = !a.done; renderDetailBody();
      try { await invoke('toggle_action_item', { meetingId: detail.id, index: i }); }
      catch (e) { a.done = !a.done; renderDetailBody(); toast(errText(e), { kind: 'err' }); }
    }
    async function rename(input) {
      const d = detail; // tras el await la vista puede haber vuelto a la lista (detail = null) u otra reunión
      const v = input.value.trim();
      if (!v || v === d.title) { input.value = d.title; return; }
      try {
        await invoke('rename_meeting', { id: d.id, title: v }); d.title = v; toast(t('Título actualizado'), { ms: 1600 });
        const m = list.find((x) => x.id === d.id); if (m) { m.title = v; if (alive && !detail) refreshListBody(); }
      } catch (e) { input.value = d.title; toast(errText(e), { kind: 'err' }); }
    }

    // ------------------------------------------------------------ eventos del DOM
    root.addEventListener('click', async (e) => {
      const tb = e.target.closest('[data-tab]');
      if (tb && detail) { tab = tb.dataset.tab; $('#md-tabs', el).innerHTML = tabsRow(); renderDetailBody(); return; }
      const b = e.target.closest('[data-act]');
      const row = e.target.closest('.mrow');
      if (!b && row && !e.target.closest('.m-trail')) { openDetail(Number(row.dataset.id)); return; }
      if (!b) return;
      const act = b.dataset.act;
      if (act === 'start') startMeeting();
      else if (act === 'stop') stopMeeting();
      else if (act === 'open-live') { if (live.meeting) openDetail(live.meeting.id); }
      else if (act === 'back') { renderList(); loadList(); }
      else if (act === 'toggle-search') { searchOpen = true; wantFocus = true; refreshListBody(); }
      else if (act === 'clear-search') { searchOpen = false; wantFocus = false; if (query) { query = ''; loadList(); } else refreshListBody(); }
      else if (act === 'row-retry' && row) retrySummary(Number(row.dataset.id));
      else if (act === 'row-more' && row) {
        const m = list.find((x) => x.id === Number(row.dataset.id)); if (!m) return;
        openMenu(b, [
          { label: t('Abrir'), icon: 'arrow-up-right', onClick: () => openDetail(m.id) },
          { label: t('Copiar como Markdown'), icon: 'copy', onClick: () => copyMarkdown(m.id) },
          ...(m.summaryStatus === 'failed' ? [{ label: t('Reintentar resumen'), icon: 'rotate-ccw', onClick: () => retrySummary(m.id) }] : []),
          ...(m.summaryStatus === 'done' ? [{ label: t('Regenerar notas'), icon: 'rotate-ccw', onClick: () => retrySummary(m.id) }] : []),
          { sep: true },
          { label: t('Eliminar reunión'), icon: 'trash-2', danger: true, onClick: async () => { if (await deleteMeeting(m.id, m.title)) { list = list.filter((x) => x.id !== m.id); refreshListBody(); } } },
        ]);
      } else if (act === 'copy-md' && detail) copyMarkdown(detail.id);
      else if (act === 'del-detail' && detail) { if (await deleteMeeting(detail.id, detail.title)) { renderList(); loadList(); } }
      else if (act === 'retry' && detail) retrySummary(detail.id);
      else if (act === 'todo') toggleTodo(Number(b.dataset.i));
      else if (act === 'raw') { const k = Number(b.dataset.start); if (showRaw.has(k)) showRaw.delete(k); else showRaw.add(k); renderDetailBody(); }
      else if (act === 'goto') gotoTurn(Number(b.dataset.start));
      else if (act === 'tq-open') { tqOpen = true; $('#md-tabs', el).innerHTML = tabsRow(); const i = $('[data-tq]', el); if (i) i.focus(); }
      else if (act === 'tq-close') { tqOpen = false; tq = ''; $('#md-tabs', el).innerHTML = tabsRow(); renderDetailBody(); }
      else if (act === 'ask-close') { ask = { state: 'idle', q: '', a: '' }; renderAsk(); }
      else if (act === 'ask-copy') { try { await invoke('copy_text', { text: `${ask.a}\n\n— ${aiLabel()}` }); toast(t('Respuesta copiada')); } catch (er) { toast(errText(er) || t('No se pudo copiar la respuesta'), { kind: 'err' }); } }
    });
    root.addEventListener('submit', (e) => { if (e.target.id === 'ask-form') { e.preventDefault(); doAsk(); } });
    root.addEventListener('input', (e) => {
      if (e.target.id === 'ask-in') syncSend();
      else if (e.target.matches('[data-search-input]')) { query = e.target.value.trim(); wantFocus = true; searchDeb(); }
      else if (e.target.matches('[data-tq]')) { tq = e.target.value.trim(); renderDetailBody(); }
    });
    root.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') {
        if (e.target.id === 'md-title') { e.target.value = detail.title; e.target.blur(); }
        else if (e.target.matches('[data-search-input]')) { e.stopPropagation(); $('[data-act=clear-search]', el).click(); }
        else if (e.target.matches('[data-tq]')) { e.stopPropagation(); $('[data-act=tq-close]', el).click(); }
        else if (e.target.id === 'ask-in' && ask.state !== 'idle') { ask = { state: 'idle', q: '', a: '' }; renderAsk(); }
      } else if (e.key === 'Enter' && e.target.id === 'md-title') { e.preventDefault(); e.target.blur(); }
      else if ((e.key === 'Enter' || e.key === ' ') && e.target.matches('.mrow')) { e.preventDefault(); openDetail(Number(e.target.dataset.id)); }
    });
    root.addEventListener('focusout', (e) => { if (e.target.id === 'md-title' && detail) rename(e.target); });
    tabsKeys(root, (id) => { if (!detail) return; tab = id; $('#md-tabs', el).innerHTML = tabsRow(); renderDetailBody(); });

    // ------------------------------------------------------------ vivo
    timer = setInterval(() => {
      const c = formatClock(elapsedMs());
      $$('[data-timer]', el).forEach((n) => { n.textContent = c; });
    }, 1000);

    const refreshLive = debounce(() => {
      if (!alive) return;
      if (!detail) {
        const lc = $('#m-live', el); if (lc) lc.innerHTML = liveCardHtml();
        const h = $('.ph-actions', el); if (h) h.innerHTML = live.active ? '' : `<button class="pill" data-act="start"><span>${esc(t('Grabar reunión'))}</span><span class="circ">${icon('arrow-up-right', 16)}</span></button>`;
        applyLevels(); loadList();
      }
    }, 120);

    const unsub = subscribe((what, data) => {
      if (!alive) return;
      if (what === 'meeting-level') applyLevels();
      else if (what === 'meeting') {
        // Error con las notas anteriores conservadas (el estado sigue «done»): aviso sobre las notas.
        if (data && data.state === 'error' && data.meetingId != null && /conservan las notas anteriores/.test(data.message || '')) {
          regenFailed.set(data.meetingId, data.message);
          toast(t('No se pudo regenerar las notas'), { kind: 'err' });
        } else if (data && data.state === 'summarizing' && data.meetingId != null) regenFailed.delete(data.meetingId);
        if (!detail) refreshLive();
        else { rebuildHead(); reloadDetail(); }
      } else if (what === 'meeting-progress' && data) {
        if (detail && detail.id === data.meetingId) {
          // Al pasar a «entender» ya está guardada la transcripción final: se recarga el detalle.
          const stageChanged = detailStage !== data.stage; detailStage = data.stage;
          if (stageChanged) reloadDetail();
          else if (tab === 'summary' && detail.summaryStatus === 'pending') renderDetailBody();
        } else if (!detail) {
          const b = el.querySelector(`.mrow[data-id="${data.meetingId}"] [data-badge]`);
          if (b) b.innerHTML = `${spinner()}${esc(pendingLabel(data.meetingId))}`;
          else loadList();
        }
      } else if (what === 'meeting-summary' && data) {
        if (detail && detail.id === data.id) { detail = data; const h = $('#md-meta', el); if (h) h.textContent = metaLine(detail); renderDetailBody(); toast(t('Resumen listo')); }
        else if (!detail) loadList();
      }
    });
    function rebuildHead() {
      const head = $('.md-head', el); if (!head || !detail) return;
      const keep = $('#md-title', el); const focused = document.activeElement === keep;
      const tmp = document.createElement('div'); tmp.innerHTML = detailHeadHtml();
      const nh = tmp.querySelector('.md-head'); head.replaceWith(nh);
      if (focused) { const ti = $('#md-title', el); ti.focus(); }
    }
    /** Lleva a un turno de la transcripción (desde una pregunta o una respuesta) y lo resalta. */
    function gotoTurn(startMs) {
      if (!detail) return;
      if (tab !== 'transcript') { tab = 'transcript'; tq = ''; $('#md-tabs', el).innerHTML = tabsRow(); renderDetailBody(); }
      const bub = el.querySelector(`.bub[data-start="${startMs}"]`);
      if (!bub) return;
      bub.scrollIntoView({ behavior: 'smooth', block: 'center' });
      bub.classList.remove('flash'); void bub.offsetWidth; bub.classList.add('flash');
    }

    // ------------------------------------------------------------ arranque
    renderList();
    loadList();
    checkOllama();
    const offAi = aiMount(root, (kind) => { if (kind === 'ready') checkOllama(); });
    if (ctx.openMeeting) { const id = ctx.openMeeting; ctx.openMeeting = null; openDetail(id); }
    return () => {
      alive = false; unsub(); offAi(); clearInterval(timer); stopGen(); searchDeb.cancel(); refreshLive.cancel();
    };
  },
};
