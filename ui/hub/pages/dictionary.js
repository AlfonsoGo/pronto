// Diccionario: vocabulario personal y reglas de corrección. [spec §2.3 Dictionary]
import { icon } from '../../shared/icons.js';
import { invoke } from '../api.js';
import { t, esc, $, toast, errText, openModal } from '../util.js';
import { switchHtml, waveDeco } from '../ui.js';
import { makeVocabPage, highlight } from './vocab.js';

export function openWordEditor(item, { done, items = [] } = {}) {
  const editing = !!item;
  let correct = editing ? !!item.replacement : false;
  const m = openModal({
    title: editing ? (item.replacement ? t('Editar reemplazo') : t('Editar palabra')) : t('Agregar al vocabulario'),
    width: 480,
    body: `
      <div class="field" id="f-word"><label for="w-word" id="l-word"></label>
        <input class="input" id="w-word" autocomplete="off" spellcheck="false" maxlength="120"></div>
      <div class="field" id="f-rep" hidden><label for="w-rep">${esc(t('Corregir ortografía'))}</label>
        <input class="input" id="w-rep" autocomplete="off" spellcheck="false" maxlength="120"
          placeholder="${esc(t('Cómo debe escribirse'))}"></div>
      <div class="swrow"><div><b>${esc(t('Corrige un error de ortografía'))}</b>
        <span class="muted">${esc(t('Pronto reemplazará siempre la versión incorrecta por la correcta cada vez que la dictes.'))}</span></div>
        ${switchHtml('correct', correct, t('Corrige un error de ortografía'))}</div>
      <p class="form-err" id="w-err" role="alert" hidden></p>`,
    footer: `<button class="btn btn-ghost" data-close>${esc(t('Cancelar'))}</button>
      <button class="btn btn-primary" id="w-ok">${esc(editing ? t('Guardar cambios') : t('Agregar palabra'))}</button>`,
  });
  const el = m.el;
  const wordIn = $('#w-word', el), repIn = $('#w-rep', el), err = $('#w-err', el), sw = $('[data-switch=correct]', el);
  wordIn.value = editing ? item.word : '';
  repIn.value = editing && item.replacement ? item.replacement : '';
  function sync() {
    $('#f-rep', el).hidden = !correct;
    $('#l-word', el).textContent = correct ? t('Error ortográfico') : (editing ? t('Editar palabra') : t('Agregar una palabra nueva'));
    wordIn.placeholder = correct ? t('Cómo lo transcribe Pronto') : t('Nombre, jerga o término');
    sw.setAttribute('aria-checked', correct);
  }
  sync();
  sw.addEventListener('click', () => { correct = !correct; sync(); (correct ? repIn : wordIn).focus(); });
  const showErr = (m2) => { err.textContent = m2; err.hidden = false; };

  async function save() {
    err.hidden = true;
    const word = wordIn.value.trim(), rep = repIn.value.trim();
    if (!word) { showErr(t('Escribe una palabra.')); wordIn.focus(); return; }
    if (correct && !rep) { showErr(t('Escribe cómo debe corregirse.')); repIn.focus(); return; }
    if (items.some((x) => (!editing || x.id !== item.id) && x.word.toLowerCase() === word.toLowerCase())) {
      showErr(t('"{x}" ya existe en tu diccionario personal', { x: word })); return;
    }
    const btn = $('#w-ok', el); btn.disabled = true;
    try {
      if (editing) {
        await invoke('update_word', { id: item.id, word, replacement: correct ? rep : null });
        toast(correct ? t('Se actualizó "{a}" → "{b}"', { a: word, b: rep }) : t('Palabra actualizada'));
      } else {
        await invoke('add_word', { word, replacement: correct ? rep : null });
        toast(correct ? t('"{a}" se agregó como reemplazo de "{b}"', { a: rep, b: word }) : t('"{x}" se agregó correctamente', { x: word }));
      }
      m.close();
      if (done) done();
    } catch (e) { showErr(errText(e)); btn.disabled = false; }
  }
  $('#w-ok', el).addEventListener('click', save);
  el.addEventListener('keydown', (e) => { if (e.key === 'Enter' && e.target.matches('input')) { e.preventDefault(); save(); } });
}

const SORTS = () => [
  { id: 'starred', label: t('Favoritos primero') },
  { id: 'newest', label: t('Más recientes primero') },
  { id: 'oldest', label: t('Más antiguos primero') },
  { id: 'alpha', label: t('Alfabético (A-Z)') },
];

export default makeVocabPage({
  id: 'dictionary',
  get title() { return t('Diccionario'); },
  listCmd: 'list_words', deleteCmd: 'delete_word',
  get addLabel() { return t('Agregar nuevo'); },
  get searchPlaceholder() { return t('Busca en tu vocabulario'); },
  get sortOptions() { return SORTS(); },
  defaultSort: 'newest',
  get banner() {
    return {
      titleHtml: t('Pronto escribe como lo haces <em>tú</em>.'),
      body: t('Pronto aprende tus palabras y nombres únicos, de forma automática o manual. Agrega términos personales, jerga de tu empresa, nombres de clientes o vocabulario de tu industria.'),
      cta: t('Agregar palabra'), deco: waveDeco(),
    };
  },
  tabFilter: (tab, w) => (tab === 'personal' ? w.source !== 'auto' : true),
  haystack: (w) => w.word + ' ' + (w.replacement || ''),
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
        : `<span class="w-new">${highlight(w.word, q)}</span>`}`;
  },
  rowActs: (w) => `<button class="icon-btn ${w.starred ? 'on' : ''}" data-act="star" aria-pressed="${!!w.starred}" aria-label="${esc(t('Destacar'))}" title="${esc(t('Destacar'))}">${icon('star', 16).replace('fill="none"', w.starred ? 'fill="currentColor"' : 'fill="none"')}</button>`,
  async onAct(act, w, { patch }) {
    if (act === 'star') {
      try { await invoke('toggle_word_star', { id: w.id }); patch(w.id, { starred: !w.starred }); }
      catch (e) { toast(errText(e), { kind: 'err' }); }
    }
  },
  openEditor: openWordEditor,
  deleteTitle: '¿Eliminar del vocabulario personal?',
  bulkTitleOne: '¿Eliminar 1 palabra?', bulkTitleMany: '¿Eliminar {n} palabras?',
  bulkBodyOne: 'La palabra seleccionada se eliminará permanentemente.', bulkBodyMany: 'Las palabras seleccionadas se eliminarán permanentemente.',
  bulkDoneOne: '1 palabra eliminada', bulkDoneMany: '{n} palabras eliminadas',
  emptyIcon: 'book-a',
  get emptyTitle() { return t('Tu diccionario está vacío'); },
  get emptyBody() { return t('Agrega nombres propios, jerga o términos de tu sector para que Pronto los escriba bien.'); },
});
