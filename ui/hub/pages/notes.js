// Bloc de notas: cuadrícula de notas y editor con guardado automático. [spec §2.3 Scratchpad]
import { icon } from '../../shared/icons.js';
import { invoke } from '../api.js';
import { t, esc, $, toast, errText, debounce, shortDate, confirmDialog } from '../util.js';
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
    let notes = [], query = '', cur = null, status = 'saved', alive = true, loaded = false;
    root.innerHTML = '<div class="page-inner" id="nt-root"></div>';
    const el = $('#nt-root', root);

    const visible = () => {
      const q = query.toLowerCase();
      return notes.filter((n) => !q || n.text.toLowerCase().includes(q));
    };

    function renderList() {
      const rows = visible();
      el.innerHTML = `${pageHeader(t('Bloc de notas'), {
        badge: `<span class="badge vio beta">Beta</span>`,
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
        return `<article class="nt-card tarj" data-id="${n.id}"><div class="in" tabindex="0" role="button" data-act="open" aria-label="${esc(title || t('Nota sin título'))}">
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
      ta.addEventListener('input', () => { cur.text = ta.value; status = 'saving'; setStatus(); autosave(); });
    }
    function setStatus() { const s = $('#nt-status', el); if (s) s.innerHTML = statusHtml(); }

    const save = async () => {
      if (!cur) return;
      if (!cur.id && !cur.text.trim()) { status = 'saved'; setStatus(); return; }
      try {
        const n = await invoke('save_note', { id: cur.id || null, text: cur.text });
        cur.id = n.id; cur.updatedAt = n.updatedAt;
        const i = notes.findIndex((x) => x.id === n.id);
        if (i >= 0) notes[i] = n; else notes.unshift(n);
        status = 'saved';
      } catch (e) { status = 'error'; toast(errText(e), { kind: 'err' }); }
      if (alive) setStatus();
    };
    const autosave = debounce(save, 600);

    async function load() {
      try { notes = await invoke('list_notes'); } catch (e) { toast(errText(e), { kind: 'err' }); notes = []; }
      loaded = true;
      if (alive && !cur) updateGrid();
    }
    async function closeEditor() {
      autosave.cancel();
      await save();
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
        if (!cur.id) { cur = null; renderList(); return; }
        autosave.cancel();
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
    return () => { alive = false; if (cur) { autosave.cancel(); save(); } };
  },
};
