// Atajos de texto (antes «Fragmentos»): frase corta -> texto largo. [spec §2.3 Snippets]
import { icon } from '../../shared/icons.js';
import { invoke } from '../api.js';
import { t, esc, $, toast, errText, openModal } from '../util.js';
import { waveDeco } from '../ui.js';
import { makeVocabPage, highlight } from './vocab.js';

const MAX = 4000;

const EXAMPLES = () => [
  { trigger: t('correo de presentación'), expansion: t('Hola, me encantaría encontrar un momento para conversar más tarde...') },
  { trigger: t('mi LinkedIn'), expansion: 'https://www.linkedin.com/in/tu-perfil/' },
  { trigger: t('indicación de reescritura'), expansion: t('Reescribe esto para que sea más conciso...') },
];

export function openSnippetEditor(item, { done, items = [], prefill } = {}) {
  const editing = !!item;
  const init = item || prefill || { trigger: '', expansion: '' };
  let saving = false; // un guardado en vuelo: Enter/Ctrl+Enter repetido no lo lanza otra vez
  const m = openModal({
    title: editing ? t('Editar atajo de texto') : t('Agregar atajo de texto'),
    width: 540,
    body: `
      <div class="field"><label for="s-trig">${esc(t('Lo que dices'))}</label>
        <input class="input" id="s-trig" autocomplete="off" spellcheck="false" maxlength="120" placeholder="${esc(t('Por ejemplo: mi dirección'))}">
        <span class="hint">${esc(t('Cuando digas esta frase al dictar, Pronto escribe en su lugar el texto de abajo.'))}</span></div>
      <div class="field"><label for="s-exp">${esc(t('Lo que se escribe'))}</label>
        <textarea class="textarea" id="s-exp" rows="6" maxlength="${MAX}" placeholder="${esc(t('El texto entero, por ejemplo: Calle Mayor 12, 3.º B, 28013 Madrid'))}"></textarea>
        <div class="counter"><span id="s-cnt">0/${MAX}</span></div></div>
      <p class="form-err" id="s-err" role="alert" hidden></p>`,
    footer: `<button class="btn btn-ghost" data-close>${esc(t('Cancelar'))}</button>
      <button class="btn btn-primary" id="s-ok">${esc(editing ? t('Guardar cambios') : t('Guardar atajo'))}</button>`,
    dirty: () => trig.value !== init.trigger || exp.value !== init.expansion,
  });
  const el = m.el, trig = $('#s-trig', el), exp = $('#s-exp', el), err = $('#s-err', el), cnt = $('#s-cnt', el);
  trig.value = init.trigger; exp.value = init.expansion;
  const count = () => { cnt.textContent = `${exp.value.length}/${MAX}`; cnt.parentElement.classList.toggle('warn', exp.value.length > MAX * 0.9); };
  exp.addEventListener('input', count); count();
  const showErr = (x) => { err.textContent = x; err.hidden = false; };
  async function save() {
    if (saving) return;
    err.hidden = true;
    const trigger = trig.value.trim(), expansion = exp.value;
    if (!trigger) { showErr(t('Escribe la frase que vas a decir.')); trig.focus(); return; }
    if (!expansion.trim()) { showErr(t('Escribe el texto que se escribirá.')); exp.focus(); return; }
    if (items.some((x) => (!editing || x.id !== item.id) && x.trigger.toLowerCase() === trigger.toLowerCase())) {
      showErr(t('Ya tienes un atajo con la frase "{x}". Usa otra frase o edita ese atajo.', { x: trigger })); return;
    }
    const btn = $('#s-ok', el); btn.disabled = true; saving = true;
    try {
      if (editing) { await invoke('update_snippet', { id: item.id, trigger, expansion }); toast(t('Atajo actualizado')); }
      else { await invoke('add_snippet', { trigger, expansion }); toast(t('"{x}" se agregó correctamente', { x: trigger })); }
      m.close();
      if (done) done();
    } catch (e) { showErr(errText(e)); btn.disabled = false; saving = false; }
  }
  $('#s-ok', el).addEventListener('click', save);
  el.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && e.target.id === 's-trig') { e.preventDefault(); exp.focus(); }
    else if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) { e.preventDefault(); save(); }
  });
}

const SORTS = () => [
  { id: 'newest', label: t('Más recientes primero') },
  { id: 'oldest', label: t('Más antiguos primero') },
  { id: 'alpha', label: t('Alfabético (A-Z)') },
];

export default makeVocabPage({
  id: 'snippets',
  get title() { return t('Atajos de texto'); },
  listCmd: 'list_snippets', deleteCmd: 'delete_snippet',
  get addLabel() { return t('Agregar atajo'); },
  get searchPlaceholder() { return t('Busca en tus atajos'); },
  get sortOptions() { return SORTS(); },
  defaultSort: 'newest',
  get examples() { return EXAMPLES(); },
  get banner() {
    return {
      titleHtml: t('Dices una frase y se escribe <em>un texto entero</em>.'),
      body: t('Tu dirección, tu firma, tu teléfono, un correo que repites: guárdalo una vez y, al dictar, di la frase. Para que escriba bien un nombre, usa «Mis palabras».'),
      deco: waveDeco(),
      extra: `<div class="hero-chips">${EXAMPLES().slice(0, 2).map((x, i) =>
        `<button class="chip" data-act="example" data-i="${i}">${icon('scissors', 14)}${esc(x.trigger)}</button>`).join('')}</div>`,
    };
  },
  tabFilter: () => true,
  haystack: (s) => s.trigger + ' ' + s.expansion,
  phrase: (s) => s.trigger,
  sortFn(sort) {
    const byNew = (a, b) => String(b.createdAt).localeCompare(String(a.createdAt));
    if (sort === 'oldest') return (a, b) => -byNew(a, b);
    if (sort === 'alpha') return (a, b) => a.trigger.localeCompare(b.trigger, undefined, { sensitivity: 'base' });
    return byNew;
  },
  rowMain(s, q) {
    return `<div class="sn"><div class="sn-t">${highlight(s.trigger, q)}</div><div class="sn-e">${highlight(s.expansion, q)}</div></div>`;
  },
  openEditor: openSnippetEditor,
  deleteTitle: '¿Eliminar este atajo de texto?',
  bulkTitleOne: '¿Eliminar 1 atajo?', bulkTitleMany: '¿Eliminar {n} atajos?',
  bulkBodyOne: 'El atajo seleccionado se eliminará de forma permanente.', bulkBodyMany: 'Los atajos seleccionados se eliminarán de forma permanente.',
  bulkDoneOne: '1 atajo eliminado', bulkDoneMany: '{n} atajos eliminados',
  emptyIcon: 'scissors',
  get emptyTitle() { return t('Aún no hay atajos de texto'); },
  get emptyBody() { return t('Guarda textos que repites a menudo y actívalos con una frase corta.'); },
});
