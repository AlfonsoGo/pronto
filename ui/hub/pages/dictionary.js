// Diccionario: vocabulario personal y reglas de corrección. [spec §2.3 Dictionary]
// Diccionario por voz (Contrato v9): «Grabar pronunciación» graba la palabra, enseña qué oye Pronto
// y guarda eso como variante que la tubería corrige a la palabra buena.
// Huella de voz (Contrato v11): cada grabación se guarda además como muestra de su voz («Tu voz»,
// se recomiendan 3); al dictar, Pronto reconoce la palabra por cómo suena.
import { icon } from '../../shared/icons.js';
import { invoke, listen } from '../api.js';
import { t, esc, $, toast, errText, openModal, formatDec } from '../util.js';
import { switchHtml, waveDeco } from '../ui.js';
import { makeVocabPage, highlight } from './vocab.js';

const sameHeard = (a, b) => a.trim().replace(/\s+/g, ' ').toLowerCase() === b.trim().replace(/\s+/g, ' ').toLowerCase();

/** Recomendado: 3 grabaciones de su voz por palabra. */
const VOICE_GOAL = 3;
const secs = (ms) => formatDec(Math.max(0, ms || 0) / 1000, 1) + ' s';

/** Texto de ayuda según cuántas grabaciones de su voz tiene la palabra. */
function voiceHint(n) {
  if (n >= VOICE_GOAL) return t('Pronto ya puede reconocerla por cómo suena tu voz.');
  if (n === 0) return t('Grábala 3 veces: así Pronto la reconoce por cómo suena tu voz, aunque la escriba distinto.');
  return VOICE_GOAL - n === 1 ? t('Grábala 1 vez más para que la reconozca mejor por tu voz.') : t('Grábala {n} veces más para que la reconozca mejor por tu voz.', { n: VOICE_GOAL - n });
}

/** Reproduce una muestra de voz (guardada por `id` o pendiente por `pending`); una a la vez. */
let voiceAudio = null;
async function playSample(s) {
  try {
    if (voiceAudio) { voiceAudio.pause(); voiceAudio = null; }
    const b64 = await invoke('get_word_sample_audio', s.id != null ? { id: s.id } : { pending: s.pending });
    if (!b64) { toast(t('Esa grabación ya no está disponible'), { kind: 'err' }); return; }
    voiceAudio = new Audio('data:audio/wav;base64,' + b64);
    await voiceAudio.play();
  } catch (e) { toast(t(errText(e)), { kind: 'err' }); }
}

/**
 * Bloque «Grabar pronunciación»: botón, grabación en curso (nivel y «Parar»), qué oye Pronto, las
 * variantes como chips con su × y las grabaciones de su voz («Tu voz», con ▶ y ×).
 * opts: { target() -> {word, replacement}, variants: [{id?, heard}], add(heard) -> Promise<variant>,
 *         remove(variant) -> Promise, samples: [{id?, pending?, durationMs}],
 *         addSample(file, ms) -> Promise<sample>, removeSample(sample) -> Promise, onEmptyWord(), autoStart }
 * Devuelve { list() -> variantes actuales, samples() -> grabaciones actuales, destroy() }.
 */
function pronouncer(host, opts) {
  let vars = (opts.variants || []).slice();
  let samples = (opts.samples || []).slice();
  let phase = 'idle'; // idle | recording | transcribing
  let alive = true;
  let cancelled = false;
  const unlisten = [];

  host.innerHTML = `
    <div class="pron-row">
      <button type="button" class="btn btn-secondary btn-sm" data-pron="rec">${icon('mic', 15)}<span data-pron-label></span></button>
      <div class="pron-live" data-pron-live hidden>
        <span class="rec-dot sm"></span><span data-pron-live-t></span>
        <span class="pron-meter" aria-hidden="true"><b></b></span>
        <button type="button" class="btn btn-ghost btn-sm" data-pron="stop">${icon('square', 11)}${esc(t('Parar'))}</button>
      </div>
    </div>
    <p class="pron-msg" data-pron-msg role="status" aria-live="polite"></p>
    <div class="pron-vars" data-pron-vars></div>
    <div class="pron-voice" data-pron-voice></div>`;
  const recBtn = $('[data-pron=rec]', host), live = $('[data-pron-live]', host), msg = $('[data-pron-msg]', host);
  const meter = $('.pron-meter b', host), varsBox = $('[data-pron-vars]', host), voiceBox = $('[data-pron-voice]', host);

  function renderVars() {
    const target = opts.target();
    const to = target.replacement || target.word;
    varsBox.innerHTML = vars.map((v, i) => `<span class="vchip" title="${esc(t('Pronto oye «{h}» y escribe «{w}»', { h: v.heard, w: to }))}">
      <span>${esc(v.heard)}</span><button type="button" data-pron-del="${i}" aria-label="${esc(t('Quitar «{x}»', { x: v.heard }))}">${icon('x', 12)}</button></span>`).join('');
    $('[data-pron-label]', host).textContent = vars.length || samples.length ? t('Grabar otra vez') : t('Grabar pronunciación');
  }

  function renderVoice() {
    const n = samples.length;
    voiceBox.innerHTML = `<div class="pv-hd"><span class="pv-ic">${icon('mic', 13)}</span><b>${esc(t('Tu voz'))}</b>
        <span class="pv-count ${n >= VOICE_GOAL ? 'ok' : ''}">${esc(t('{n} de {m}', { n, m: VOICE_GOAL }))}</span></div>
      <p class="pv-hint">${esc(voiceHint(n))}</p>
      ${n ? `<div class="pv-list">${samples.map((s, i) => `<span class="vchip pv-chip">
        <button type="button" class="pv-play" data-pv-play="${i}" aria-label="${esc(t('Escuchar la grabación {n}', { n: i + 1 }))}">${icon('play', 11)}</button>
        <span>${esc(t('Grabación {n}', { n: i + 1 }))} · ${esc(secs(s.durationMs))}</span>
        <button type="button" data-pv-del="${i}" aria-label="${esc(t('Borrar la grabación {n}', { n: i + 1 }))}">${icon('x', 12)}</button></span>`).join('')}</div>` : ''}`;
    $('[data-pron-label]', host).textContent = vars.length || samples.length ? t('Grabar otra vez') : t('Grabar pronunciación');
  }

  function setPhase(p, word) {
    phase = p;
    recBtn.hidden = p !== 'idle';
    live.hidden = p === 'idle';
    $('[data-pron=stop]', host).hidden = p !== 'recording';
    $('.pron-meter', host).hidden = p !== 'recording';
    const lt = $('[data-pron-live-t]', host);
    if (p === 'recording') lt.textContent = t('Di «{w}»…', { w: word });
    if (p === 'transcribing') lt.innerHTML = `<span class="spin"></span> ${esc(t('Transcribiendo…'))}`;
    meter.style.setProperty('--v', '.04');
  }

  function say(kind, html) {
    msg.className = 'pron-msg' + (kind ? ' ' + kind : '');
    msg.innerHTML = html;
  }

  const heardHtml = (h) => `${esc(t('Pronto oye:'))} <b>“${esc(h)}”</b>`;

  async function onResult(r, target) {
    const to = target.replacement || target.word;
    if (r.status === 'ok') { say('ok', `${heardHtml(r.variant)}. ${esc(t('Pronto ya la entiende bien.'))}`); return; }
    if (r.status === 'covered') { say('ok', `${heardHtml(r.variant)}, ${esc(t('que ya se corrige a «{w}».', { w: to }))}`); return; }
    if (vars.some((v) => sameHeard(v.heard, r.variant))) { say('', `${heardHtml(r.variant)}. ${esc(t('Esa variante ya estaba guardada.'))}`); return; }
    const v = await opts.add(r.variant);
    if (!alive) return;
    vars.push(v || { heard: r.variant });
    renderVars();
    say('ok', `${heardHtml(r.variant)}. ${esc(t('A partir de ahora lo escribirá «{w}».', { w: to }))}` +
      (r.common ? `<span class="warn-t">${esc(t('Ojo: «{h}» es una palabra corriente; se cambiará cada vez que la dictes.', { h: r.variant }))}</span>` : ''));
  }

  async function record() {
    if (phase !== 'idle') return;
    const target = opts.target();
    if (!target.word) { say('err', esc(t('Escribe primero la palabra.'))); if (opts.onEmptyWord) opts.onEmptyWord(); return; }
    cancelled = false;
    say('', '');
    setPhase('recording', target.replacement || target.word);
    try {
      const r = await invoke('pronounce_record', { word: target.word, replacement: target.replacement || null });
      if (!alive) return;
      setPhase('idle');
      // La grabación es también una muestra de su voz (aunque Pronto ya escriba bien la palabra).
      if (r.sample && opts.addSample) {
        // Si no se puede guardar la muestra, la variante sigue adelante igual.
        const smp = await opts.addSample(r.sample, r.sampleMs).catch((err) => { toast(t(errText(err)), { kind: 'err' }); return null; });
        if (!alive) return;
        if (smp) { samples.push(smp); renderVoice(); }
      }
      await onResult(r, target);
    } catch (e) {
      if (!alive) return;
      setPhase('idle');
      if (!cancelled) say('err', esc(t(errText(e))));
    }
  }

  host.addEventListener('click', async (e) => {
    const b = e.target.closest('[data-pron],[data-pron-del],[data-pv-play],[data-pv-del]');
    if (!b) return;
    if (b.dataset.pvPlay != null) { const smp = samples[Number(b.dataset.pvPlay)]; if (smp) playSample(smp); return; }
    if (b.dataset.pvDel != null) {
      const smp = samples[Number(b.dataset.pvDel)];
      if (!smp) return;
      try {
        if (opts.removeSample) await opts.removeSample(smp);
        samples = samples.filter((x) => x !== smp);
        renderVoice();
      } catch (err) { say('err', esc(t(errText(err)))); }
      return;
    }
    if (b.dataset.pron === 'rec') record();
    else if (b.dataset.pron === 'stop') invoke('pronounce_stop', { cancel: false }).catch(() => {});
    else if (b.dataset.pronDel != null) {
      const v = vars[Number(b.dataset.pronDel)];
      if (!v) return;
      try {
        await opts.remove(v);
        vars = vars.filter((x) => x !== v);
        renderVars();
        say('', '');
      } catch (err) { say('err', esc(t(errText(err)))); }
    }
  });

  listen('pronounce://level', (p) => { if (phase === 'recording' && p) meter.style.setProperty('--v', Math.max(0.04, Math.min(1, p.level)).toFixed(3)); })
    .then((u) => (alive ? unlisten.push(u) : u()));
  listen('pronounce://state', (p) => { if (p && p.state === 'transcribing' && phase === 'recording') setPhase('transcribing'); })
    .then((u) => (alive ? unlisten.push(u) : u()));

  renderVars();
  renderVoice();
  setPhase('idle');
  if (opts.autoStart) setTimeout(record, 250);

  return {
    list: () => vars.slice(),
    samples: () => samples.slice(),
    refresh: renderVars,
    destroy() {
      alive = false;
      if (phase !== 'idle') { cancelled = true; invoke('pronounce_stop', { cancel: true }).catch(() => {}); }
      if (voiceAudio) { voiceAudio.pause(); voiceAudio = null; }
      unlisten.forEach((u) => u());
    },
  };
}

export function openWordEditor(item, { done, items = [] } = {}) {
  const editing = !!item;
  let correct = editing ? !!item.replacement : false;
  let pron = null;
  let saving = false; // un guardado en vuelo: Enter repetido no lo lanza otra vez
  let base = null; // cómo estaba el formulario al abrirse (para saber si hay cambios sin guardar)
  const m = openModal({
    title: editing ? t('Editar palabra') : t('Agregar palabra'),
    width: 500,
    body: `
      <div class="field" id="f-word"><label for="w-word" id="l-word"></label>
        <input class="input" id="w-word" autocomplete="off" spellcheck="false" maxlength="120"></div>
      <div class="field" id="f-rep" hidden><label for="w-rep">${esc(t('Cómo se escribe bien'))}</label>
        <input class="input" id="w-rep" autocomplete="off" spellcheck="false" maxlength="120"
          placeholder="${esc(t('Por ejemplo: Iturbe-Zabaleta'))}"></div>
      <div class="swrow"><div><b>${esc(t('¿La escribe mal siempre de la misma forma? (opcional)'))}</b>
        <span class="muted">${esc(t('Actívalo y pon esa forma equivocada: Pronto la cambiará por la buena cada vez.'))}</span></div>
        ${switchHtml('correct', correct, t('¿La escribe mal siempre de la misma forma? (opcional)'))}</div>
      <div class="pron"><div class="pron-hd"><b>${esc(t('Cómo la dices (opcional)'))}</b>
        <span class="muted">${esc(t('Si quieres, grábala con tu voz (mejor 3 veces) para que Pronto la reconozca aunque la oiga distinta.'))}</span></div>
        <div id="w-pron"></div></div>
      <p class="form-err" id="w-err" role="alert" hidden></p>`,
    footer: `<button class="btn btn-ghost" data-close>${esc(t('Cancelar'))}</button>
      <button class="btn btn-primary" id="w-ok">${esc(editing ? t('Guardar cambios') : t('Guardar palabra'))}</button>`,
    onClose: () => { if (pron) pron.destroy(); },
    dirty: () => base !== null && snapshot() !== base,
  });
  const el = m.el;
  const wordIn = $('#w-word', el), repIn = $('#w-rep', el), err = $('#w-err', el), sw = $('[data-switch=correct]', el);
  wordIn.value = editing ? item.word : '';
  repIn.value = editing && item.replacement ? item.replacement : '';
  function sync() {
    $('#f-rep', el).hidden = !correct;
    $('#l-word', el).textContent = correct ? t('Cómo la escribe mal Pronto') : t('¿Cómo se escribe?');
    wordIn.placeholder = correct ? t('Por ejemplo: Iturbe Sabaleta') : t('Por ejemplo: Iturbe-Zabaleta');
    sw.setAttribute('aria-checked', correct);
  }
  sync();
  sw.addEventListener('click', () => { correct = !correct; sync(); if (pron) pron.refresh(); (correct ? repIn : wordIn).focus(); });
  const showErr = (m2) => { err.textContent = m2; err.hidden = false; };

  // Las variantes y grabaciones hechas aquí se guardan con el botón de guardar (Cancelar las descarta).
  pron = pronouncer($('#w-pron', el), {
    target: () => ({ word: wordIn.value.trim(), replacement: correct ? repIn.value.trim() : '' }),
    variants: editing ? item.variants || [] : [],
    add: async (heard) => ({ heard }),
    remove: async () => {},
    samples: editing ? item.samples || [] : [],
    addSample: async (file, ms) => ({ pending: file, durationMs: ms }),
    removeSample: async () => {},
    onEmptyWord: () => wordIn.focus(),
  });
  const snapshot = () => JSON.stringify([wordIn.value, correct, repIn.value, pron.list().map((v) => v.heard), pron.samples().map((x) => x.id ?? x.pending)]);
  base = snapshot();

  async function save() {
    if (saving) return;
    err.hidden = true;
    const word = wordIn.value.trim(), rep = repIn.value.trim();
    if (!word) { showErr(t('Escribe la palabra.')); wordIn.focus(); return; }
    if (correct && !rep) { showErr(t('Escribe cómo se escribe bien.')); repIn.focus(); return; }
    if (items.some((x) => (!editing || x.id !== item.id) && x.word.toLowerCase() === word.toLowerCase())) {
      showErr(t('"{x}" ya está en Mis palabras', { x: word })); return;
    }
    const variants = pron.list().map((v) => v.heard);
    const smp = pron.samples();
    const newSamples = smp.filter((x) => x.pending).map((x) => x.pending);
    const btn = $('#w-ok', el); btn.disabled = true; saving = true;
    try {
      if (editing) {
        const keepSamples = smp.filter((x) => x.id != null).map((x) => x.id);
        await invoke('update_word', { id: item.id, word, replacement: correct ? rep : null, variants, keepSamples, newSamples });
        toast(correct ? t('Se actualizó "{a}" → "{b}"', { a: word, b: rep }) : t('Palabra actualizada'));
      } else {
        await invoke('add_word', { word, replacement: correct ? rep : null, variants, newSamples });
        toast(correct ? t('"{a}" se agregó como reemplazo de "{b}"', { a: rep, b: word }) : t('"{x}" se agregó correctamente', { x: word }));
      }
      m.close();
      if (done) done();
    } catch (e) { showErr(t(errText(e))); btn.disabled = false; saving = false; }
  }
  $('#w-ok', el).addEventListener('click', save);
  el.addEventListener('keydown', (e) => { if (e.key === 'Enter' && e.target.matches('input')) { e.preventDefault(); save(); } });
}

/** «Grabar otra» desde la lista: graba al abrir y guarda cada variante y cada grabación al momento. */
export function openPronounceModal(w, { done } = {}) {
  const to = w.replacement || w.word;
  let changed = false;
  let pron = null;
  const m = openModal({
    title: t('Grabar pronunciación'),
    subtitle: esc(t('Di «{w}» como lo dirías al dictar.', { w: to })),
    width: 460,
    body: '<div class="pron" id="p-box"></div>',
    footer: `<button class="btn btn-primary" data-close>${esc(t('Listo'))}</button>`,
    onClose: () => { if (pron) pron.destroy(); if (changed && done) done(); },
  });
  pron = pronouncer($('#p-box', m.el), {
    target: () => ({ word: w.word, replacement: w.replacement || '' }),
    variants: w.variants || [],
    add: async (heard) => { const v = await invoke('add_word_variant', { wordId: w.id, heard }); changed = true; return v; },
    remove: async (v) => { await invoke('delete_word_variant', { id: v.id }); changed = true; },
    samples: w.samples || [],
    addSample: async (file) => { const smp = await invoke('add_word_sample', { wordId: w.id, sample: file }); changed = true; return smp; },
    removeSample: async (smp) => { await invoke('delete_word_sample', { id: smp.id }); changed = true; },
    autoStart: true,
  });
}

/** Cuántas grabaciones de su voz tiene la palabra (botón que abre «Grabar pronunciación»). */
function voiceHtml(w) {
  const n = (w.samples || []).length;
  if (!n) return '';
  const tip = `${n === 1 ? t('1 grabación de tu voz') : t('{n} grabaciones de tu voz', { n })}. ${voiceHint(n)}`;
  // rompedor 23-oct, hub MEDIA 6 (WCAG 2.5.3): el nombre accesible empieza por el texto visible («1 de 3»).
  const visible = t('{n} de {m}', { n, m: VOICE_GOAL });
  return `<button type="button" class="v-voice ${n >= VOICE_GOAL ? 'ok' : ''}" data-act="pron" title="${esc(tip)}" aria-label="${esc(`${visible}: ${tip}`)}">${icon('mic', 12)}<span>${esc(t('{n} de {m}', { n, m: VOICE_GOAL }))}</span></button>`;
}

const SORTS = () => [
  { id: 'starred', label: t('Favoritos primero') },
  { id: 'newest', label: t('Más recientes primero') },
  { id: 'oldest', label: t('Más antiguos primero') },
  { id: 'alpha', label: t('Alfabético (A-Z)') },
];

function variantsHtml(w) {
  const vs = w.variants || [];
  if (!vs.length) return '';
  const to = w.replacement || w.word;
  return `<span class="v-vars" title="${esc(t('Lo que oye Pronto y corrige a «{w}»', { w: to }))}"><span class="v-vars-ic">${icon('mic', 13)}</span>${vs.map((v) => `
    <span class="vchip"><span>${esc(v.heard)}</span><button type="button" data-act="var-del" data-vid="${v.id}" aria-label="${esc(t('Quitar «{x}»', { x: v.heard }))}">${icon('x', 11)}</button></span>`).join('')}</span>`;
}

export default makeVocabPage({
  id: 'dictionary',
  get title() { return t('Mis palabras'); },
  listCmd: 'list_words', deleteCmd: 'delete_word',
  get addLabel() { return t('Agregar palabra'); },
  get searchPlaceholder() { return t('Busca en tus palabras'); },
  get sortOptions() { return SORTS(); },
  defaultSort: 'newest',
  get banner() {
    return {
      titleHtml: t('Enseña a Pronto <em>tus</em> palabras.'),
      body: t('Nombres, apellidos, empresas o palabras de tu trabajo que Pronto escribe mal: pon cómo se escriben y las escribirá bien siempre. Para que una frase corta escriba un texto largo, usa «Atajos de texto».'),
      deco: waveDeco(),
    };
  },
  tabFilter: (tab, w) => (tab === 'personal' ? w.source !== 'auto' : true),
  haystack: (w) => [w.word, w.replacement || '', ...(w.variants || []).map((v) => v.heard)].join(' '),
  phrase: (w) => w.word,
  sortFn(sort) {
    const byNew = (a, b) => String(b.createdAt).localeCompare(String(a.createdAt));
    if (sort === 'oldest') return (a, b) => -byNew(a, b);
    if (sort === 'alpha') return (a, b) => a.word.localeCompare(b.word, undefined, { sensitivity: 'base' });
    if (sort === 'starred') return (a, b) => (b.starred ? 1 : 0) - (a.starred ? 1 : 0) || byNew(a, b);
    return byNew;
  },
  rowMain(w, q) {
    const auto = w.source === 'auto';
    return `<span class="v-src ${auto ? 'auto' : ''}" title="${esc(auto ? t('Pronto aprendió esta palabra automáticamente ✨') : t('Agregada por ti'))}">${icon(auto ? 'sparkles' : 'book-a', 15)}</span>
      ${w.replacement
        ? `<span class="w-old">${highlight(w.word, q)}</span><span class="w-arrow">${icon('arrow-up-right', 14)}</span><span class="w-new">${highlight(w.replacement, q)}</span>`
        : `<span class="w-new">${highlight(w.word, q)}</span>`}${voiceHtml(w)}${variantsHtml(w)}`;
  },
  rowActs: (w) => `<button class="icon-btn" data-act="pron" aria-label="${esc(t('Grabar pronunciación'))}" title="${esc(t('Grabar pronunciación'))}">${icon('mic', 16)}</button>
    <button class="icon-btn ${w.starred ? 'on' : ''}" data-act="star" aria-pressed="${!!w.starred}" aria-label="${esc(t('Destacar'))}" title="${esc(t('Destacar'))}">${icon('star', 16).replace('fill="none"', w.starred ? 'fill="currentColor"' : 'fill="none"')}</button>`,
  async onAct(act, w, { patch, reload }, btn) {
    if (act === 'star') {
      try { await invoke('toggle_word_star', { id: w.id }); patch(w.id, { starred: !w.starred }); }
      catch (e) { toast(errText(e), { kind: 'err' }); }
    } else if (act === 'pron') {
      openPronounceModal(w, { done: reload });
    } else if (act === 'var-del' && btn) {
      const v = (w.variants || []).find((x) => String(x.id) === btn.dataset.vid);
      if (!v) return;
      try {
        await invoke('delete_word_variant', { id: v.id });
        patch(w.id, { variants: w.variants.filter((x) => x !== v) });
        toast(t('Variante «{x}» quitada', { x: v.heard }));
      } catch (e) { toast(t(errText(e)), { kind: 'err' }); }
    }
  },
  openEditor: openWordEditor,
  deleteTitle: '¿Quitar de Mis palabras?',
  bulkTitleOne: '¿Eliminar 1 palabra?', bulkTitleMany: '¿Eliminar {n} palabras?',
  bulkBodyOne: 'La palabra seleccionada se eliminará permanentemente.', bulkBodyMany: 'Las palabras seleccionadas se eliminarán permanentemente.',
  bulkDoneOne: '1 palabra eliminada', bulkDoneMany: '{n} palabras eliminadas',
  emptyIcon: 'book-a',
  get emptyTitle() { return t('Aún no has agregado palabras'); },
  get emptyBody() { return t('Agrega nombres propios, jerga o términos de tu sector para que Pronto los escriba bien.'); },
});
