// Bloc de notas: cuadrícula de notas y editor con guardado automático. [spec §2.3 Scratchpad]
import { icon } from '../../shared/icons.js';
import { invoke } from '../api.js';
import { t, esc, $, toast, errText, debounce, sleep, shortDate, confirmDialog, onBeforeClose, fold } from '../util.js';
import { pageHeader, emptyState, highlight } from '../ui.js';

const split = (text) => {
  const lines = String(text || '').split('\n');
  const i = lines.findIndex((l) => l.trim());
  if (i < 0) return { title: '', rest: '' };
  return { title: lines[i].trim(), rest: lines.slice(i + 1).join('\n').trim() };
};

export default {
  id: 'notes',
  mount(root) {
    let notes = [], query = '', cur = null, status = 'saved', alive = true, loaded = false, dirty = false;
    root.innerHTML = '<div class="page-inner" id="nt-root"></div>';
    const el = $('#nt-root', root);

    const visible = () => {
      const q = fold(query); // sin tildes ni mayúsculas (rompedor 23-oct, hub BAJA 9)
      return notes.filter((n) => !q || fold(n.text).includes(q));
    };

    function renderList() {
      const rows = visible();
      el.innerHTML = `${pageHeader(t('Notas'), {
        actions: `<button class="btn btn-primary" data-act="new">${icon('plus', 16)}${esc(t('Nueva nota'))}</button>`,
      })}
        <div class="searchbar"><span class="sb-ic">${icon('search', 17)}</span>
          <input class="sb-in" data-search-input type="search" placeholder="${esc(t('Busca en tus notas'))}" value="${esc(query)}" aria-label="${esc(t('Busca en tus notas'))}" autocomplete="off" spellcheck="false">
          <button class="icon-btn" data-act="clear-search" style="visibility:${query ? 'visible' : 'hidden'}" aria-label="${esc(t('Limpiar búsqueda'))}">${icon('x', 16)}</button></div>
        <div id="nt-grid">${gridHtml(rows)}</div>`;
    }
    function gridHtml(rows) {
      if (!loaded) return `<div class="skel-list">${'<div class="skel"></div>'.repeat(3)}</div>`;
      if (!rows.length) return query ? emptyState('search', t('No hay notas que coincidan'), '') : emptyState('notebook-pen', t('Aún no hay notas'), t('Crea una nota para guardar ideas, listas o borradores. Se guarda sola.'));
      return `<div class="overline rec">${esc(t('Recientes'))}</div><div class="nt-grid">${rows.map((n) => {
        const { title, rest } = split(n.text);
        return `<article class="nt-card tarj" data-id="${n.id}"><div class="in" tabindex="0" role="link" data-act="open">
          <h3 class="${rest ? '' : 'solo'}">${highlight(title || t('Nota sin título'), query)}</h3>
          ${rest ? `<p>${highlight(rest, query)}</p>` : ''}
          <footer><span>${esc(shortDate(n.updatedAt))}</span>
            <button class="icon-btn danger" data-act="del" aria-label="${esc(t('Eliminar nota'))}" title="${esc(t('Eliminar nota'))}">${icon('trash-2', 15)}</button></footer></div></article>`;
      }).join('')}</div>`;
    }
    function updateGrid() { const g = $('#nt-grid', el); if (g) g.innerHTML = gridHtml(visible()); }

    function statusHtml() {
      return status === 'saving' ? `<span class="muted xs">${esc(t('Guardando…'))}</span>`
        : status === 'error' ? `<span class="err-t xs">${esc(t('No se pudo guardar'))}</span>`
        : `<span class="muted xs ok-sv">${icon('check', 13)}${esc(t('Guardado'))}</span>`;
    }
    function renderEditor() {
      el.innerHTML = `
        <div class="ed-bar">
          <button class="back" data-act="back">${icon('chevron-left', 17)}${esc(t('Bloc de notas'))}</button>
          <span class="sp"></span><span id="nt-status">${statusHtml()}</span>
          <button class="icon-btn" data-act="copy" aria-label="${esc(t('Copiar'))}" title="${esc(t('Copiar'))}">${icon('copy', 17)}</button>
          <button class="icon-btn danger" data-act="del-cur" aria-label="${esc(t('Eliminar nota'))}" title="${esc(t('Eliminar nota'))}">${icon('trash-2', 17)}</button>
        </div>
        <div class="tarj ed"><div class="in"><textarea id="nt-text" class="nt-text" placeholder="${esc(t('Escribe o dicta aquí…'))}" spellcheck="true" aria-label="${esc(t('Texto de la nota'))}"></textarea></div></div>`;
      const ta = $('#nt-text', el);
      ta.value = cur.text;
      ta.focus();
      ta.addEventListener('input', () => { cur.text = ta.value; dirty = true; status = 'saving'; setStatus(); autosave(); });
    }
    function setStatus() { const s = $('#nt-status', el); if (s) s.innerHTML = statusHtml(); }

    // Los guardados van en fila: el siguiente espera al anterior y usa el id que devolvió
    // (si no, una nota nueva guardada dos veces a la vez se crearía dos veces).
    let saving = Promise.resolve();
    const save = () => { const c = cur; dirty = false; saving = saving.then(() => saveNote(c)); return saving; };
    // Devuelve false si el guardado falló. Rompedor 23-oct, hub BAJA 1: `save()` pone `dirty = false` antes de saber si guardó;
    // si falla se vuelve a poner a true, para que cerrar (o el siguiente intento) lo reintente en vez de dar la nota por guardada.
    const saveNote = async (c) => {
      if (!c) return true;
      if (!c.id && !c.text.trim()) { status = 'saved'; setStatus(); return true; }
      let ok = true;
      try {
        const n = await invoke('save_note', { id: c.id || null, text: c.text });
        c.id = n.id; c.updatedAt = n.updatedAt;
        const i = notes.findIndex((x) => x.id === n.id);
        if (i >= 0) notes[i] = n; else notes.unshift(n);
        status = 'saved';
      } catch (e) { status = 'error'; ok = false; if (c === cur) dirty = true; toast(errText(e), { kind: 'err' }); }
      if (alive) setStatus();
      return ok;
    };
    const autosave = debounce(save, 600);
    // Cerrar la ventana justo después de escribir: se guarda ya, sin esperar al autoguardado, y la ventana espera a que
    // acabe el guardado que ya iba en vuelo y el de lo último (los guardados van en fila) sin tope de 1,5 s: rompedor 23-oct,
    // hub MEDIA 1 (con la base lenta la ventana se destruía antes de mandar lo último). Si falla, un segundo intento tras
    // 400 ms; si vuelve a fallar devuelve false y la ventana avisa en vez de cerrarse en silencio. (Con la página ya
    // desmontada `cur` sigue apuntando a la nota que quedó en fila.)
    const pendiente = () => !!cur && dirty;
    const offClose = onBeforeClose(async () => {
      for (let intento = 0; intento < 2; intento++) {
        if (pendiente()) { autosave.cancel(); await save(); } else await saving;
        if (!pendiente()) return true;
        if (intento === 0) await sleep(400);
      }
      return !pendiente();
    });

    async function load() {
      try { notes = await invoke('list_notes'); } catch (e) { toast(errText(e), { kind: 'err' }); notes = []; }
      loaded = true;
      if (alive && !cur) updateGrid();
    }
    async function closeEditor() {
      autosave.cancel();
      // Si el guardado falla, la nota sigue abierta con el aviso «No se pudo guardar» (volver a la lista la daría por perdida).
      if (!(await save())) return;
      cur = null; status = 'saved';
      await load();
      renderList();
    }
    async function del(n) {
      const ok = await confirmDialog({ title: t('¿Seguro que quieres eliminar esta nota?'), body: t('Una vez eliminado, no se puede recuperar') });
      if (!ok) return false;
      try { await invoke('delete_note', { id: n.id }); notes = notes.filter((x) => x.id !== n.id); toast(t('Nota eliminada')); return true; }
      catch (e) { toast(errText(e), { kind: 'err' }); return false; }
    }

    root.addEventListener('click', async (e) => {
      const b = e.target.closest('[data-act]'); if (!b) return;
      const act = b.dataset.act;
      const card = b.closest('.nt-card');
      const n = card ? notes.find((x) => String(x.id) === card.dataset.id) : null;
      if (act === 'new') { cur = { id: null, text: '' }; renderEditor(); }
      else if (act === 'open' && n) { cur = { ...n }; status = 'saved'; renderEditor(); }
      else if (act === 'del' && n) { e.stopPropagation(); if (await del(n)) updateGrid(); }
      else if (act === 'back') closeEditor();
      else if (act === 'copy') {
        if (!cur.text.trim()) return;
        try { await invoke('copy_text', { text: cur.text }); toast(t('Copiado al portapapeles')); } catch (er) { toast(errText(er), { kind: 'err' }); }
      } else if (act === 'del-cur') {
        autosave.cancel();
        await saving;
        if (!cur) return;
        if (!cur.id) { cur = null; renderList(); return; }
        if (await del(cur)) { cur = null; renderList(); }
      } else if (act === 'clear-search') { query = ''; renderList(); $('[data-search-input]', el).focus(); }
    });
    root.addEventListener('keydown', (e) => {
      if ((e.key === 'Enter' || e.key === ' ') && e.target.matches('.nt-card > .in')) { e.preventDefault(); e.target.click(); }
      else if (e.key === 'Escape' && cur && !e.defaultPrevented && !document.querySelector('.velo')) closeEditor();
      else if (e.key === 'Escape' && e.target.matches('[data-search-input]')) { query = ''; renderList(); }
    });
    root.addEventListener('input', (e) => {
      if (e.target.matches('[data-search-input]')) {
        query = e.target.value.trim(); updateGrid();
        const x = $('[data-act=clear-search]', el); if (x) x.style.visibility = query ? 'visible' : 'hidden';
      }
    });

    renderList();
    load();
    return () => { alive = false; if (cur) { autosave.cancel(); save(); } saving.then(() => { if (!pendiente()) offClose(); }); };
  },
};
