// Fábrica de páginas tipo lista (Diccionario y Fragmentos): tabs, búsqueda plegable, orden,
// selección múltiple y borrado. [spec §2.3 Dictionary / Snippets]
import { icon } from '../../shared/icons.js';
import { invoke } from '../api.js';
import { t, esc, $, $$, toast, errText, openMenu, confirmDialog, debounce } from '../util.js';
import { pageHeader, tabsHtml, tabsKeys, emptyState, heroBanner, searchBtn, highlight } from '../ui.js';

/**
 * cfg: {
 *  id, title, listCmd, deleteCmd, searchPlaceholder, sortOptions:[{id,label}], defaultSort, sortFn(sort)->cmp,
 *  tabFilter(tab,item)->bool, banner:{titleHtml, body, cta, deco, extra}, noun:{one,other}, addLabel,
 *  rowMain(item,q)->html, rowActs(item)->html, onAct(act,item,helpers)->bool, haystack(item)->string,
 *  phrase(item)->string, openEditor(item|null, {done, items}), emptyTitle, emptyBody, deleteTitle
 * }
 */
export function makeVocabPage(cfg) {
  return {
    id: cfg.id,
    mount(root) {
      let items = [], tab = 'all', sort = cfg.defaultSort, query = '', searchOpen = false, loaded = false;
      const selected = new Set();
      let alive = true;

      root.innerHTML = `<div class="page-inner">
        ${pageHeader(cfg.title, { actions: `<button class="btn btn-primary" data-act="add">${icon('plus', 16)}${esc(cfg.addLabel)}</button>` })}
        <div id="v-banner">${heroBanner({ ...cfg.banner, ctaAct: 'add', cls: 'hero-sm' })}</div>
        <div id="v-tabs"></div><div id="v-bulk"></div><div id="v-list"></div></div>`;
      const elTabs = $('#v-tabs', root), elBulk = $('#v-bulk', root), elList = $('#v-list', root);

      const visible = () => {
        const q = query.toLowerCase();
        return items.filter((i) => cfg.tabFilter(tab, i) && (!q || cfg.haystack(i).toLowerCase().includes(q))).sort(cfg.sortFn(sort));
      };

      function renderTabs() {
        const tools = (searchOpen
          ? `<div class="searchbar compact"><span class="sb-ic">${icon('search', 16)}</span>
              <input class="sb-in" data-search-input type="search" placeholder="${esc(cfg.searchPlaceholder)}" value="${esc(query)}" aria-label="${esc(cfg.searchPlaceholder)}" autocomplete="off" spellcheck="false">
              <button class="icon-btn" data-act="clear-search" aria-label="${esc(t('Cerrar búsqueda'))}">${icon('x', 15)}</button></div>`
          : searchBtn()) +
          `<button class="icon-btn" data-act="sort" aria-label="${esc(t('Ordenar por'))}" title="${esc(t('Ordenar por'))}">${icon('arrow-up-down', 18)}</button>`;
        elTabs.innerHTML = tabsHtml([{ id: 'all', label: t('Todos') }, { id: 'personal', label: t('Personal') }], tab, tools);
        if (searchOpen) { const i = $('[data-search-input]', elTabs); if (wantFocus && i) { i.focus(); i.setSelectionRange(i.value.length, i.value.length); } }
      }
      let wantFocus = false;

      function renderBulk() {
        const n = selected.size;
        elBulk.innerHTML = n ? `<div class="bulk" role="toolbar" aria-label="${esc(t('Acciones en lote'))}">
          <span class="bulk-n">${esc(t('{n} seleccionados', { n }))}</span>
          <button class="btn btn-ghost btn-sm" data-act="bulk-clear">${esc(t('Cancelar'))}</button>
          <button class="btn btn-danger-solid btn-sm" data-act="bulk-delete">${icon('trash-2', 15)}${esc(t('Eliminar {n}', { n }))}</button></div>` : '';
        elList.classList.toggle('sel-mode', n > 0);
      }

      function renderList() {
        if (!loaded) { elList.innerHTML = `<div class="skel-list">${'<div class="skel"></div>'.repeat(4)}</div>`; return; }
        const rows = visible();
        if (!rows.length) {
          elList.innerHTML = query
            ? emptyState('search', t('No se encontraron resultados'), t('No se encontraron palabras que coincidan con tu búsqueda'))
            : emptyState(cfg.emptyIcon || 'book-a', cfg.emptyTitle, cfg.emptyBody);
          return;
        }
        elList.innerHTML = `<div class="lista vlist">${rows.map((i) => `
          <div class="vrow ${selected.has(i.id) ? 'is-sel' : ''}" data-id="${i.id}">
            <button class="check" role="checkbox" aria-checked="${selected.has(i.id)}" data-act="select" aria-label="${esc(t('Seleccionar'))}">${icon('check', 12)}</button>
            <div class="v-main">${cfg.rowMain(i, query)}</div>
            <div class="v-acts">${cfg.rowActs ? cfg.rowActs(i) : ''}
              <button class="icon-btn" data-act="edit" aria-label="${esc(t('Editar'))}" title="${esc(t('Editar'))}">${icon('pencil', 16)}</button>
              <button class="icon-btn danger" data-act="delete" aria-label="${esc(t('Eliminar'))}" title="${esc(t('Eliminar'))}">${icon('trash-2', 16)}</button>
            </div></div>`).join('')}</div>`;
      }

      function renderAll() { renderTabs(); renderBulk(); renderList(); }

      async function load() {
        try { items = await invoke(cfg.listCmd); } catch (e) { toast(errText(e), { kind: 'err' }); items = []; }
        loaded = true;
        const ids = new Set(items.map((i) => i.id));
        [...selected].forEach((id) => { if (!ids.has(id)) selected.delete(id); });
        if (alive) renderAll();
      }

      async function removeOne(it) {
        const ok = await confirmDialog({
          title: t(cfg.deleteTitle),
          body: t("'{phrase}' se eliminará permanentemente.", { phrase: esc(cfg.phrase(it)) }),
        });
        if (!ok) return;
        try {
          await invoke(cfg.deleteCmd, { id: it.id });
          selected.delete(it.id);
          toast(t('"{x}" eliminada', { x: cfg.phrase(it) }));
          await load();
        } catch (e) { toast(errText(e) || t('No se pudo eliminar "{x}"', { x: cfg.phrase(it) }), { kind: 'err' }); }
      }

      async function removeMany() {
        const n = selected.size;
        const ok = await confirmDialog({
          title: n === 1 ? t(cfg.bulkTitleOne) : t(cfg.bulkTitleMany, { n }),
          body: n === 1 ? t(cfg.bulkBodyOne) : t(cfg.bulkBodyMany),
          confirmText: t('Sí, eliminar'),
        });
        if (!ok) return;
        let failed = 0;
        for (const id of [...selected]) {
          try { await invoke(cfg.deleteCmd, { id }); selected.delete(id); } catch (_) { failed++; }
        }
        toast(failed ? t('No se pudieron eliminar {n}', { n: failed }) : (n === 1 ? t(cfg.bulkDoneOne) : t(cfg.bulkDoneMany, { n })), { kind: failed ? 'err' : 'ok' });
        await load();
      }

      const helpers = {
        reload: load,
        patch(id, p) { const it = items.find((x) => x.id === id); if (it) Object.assign(it, p); renderList(); },
      };

      root.addEventListener('click', (e) => {
        const b = e.target.closest('[data-act],[data-tab]');
        if (!b) return;
        if (b.dataset.tab) { tab = b.dataset.tab; selected.clear(); renderAll(); return; }
        const act = b.dataset.act;
        const row = b.closest('.vrow');
        const it = row ? items.find((x) => String(x.id) === row.dataset.id) : null;
        if (act === 'add') cfg.openEditor(null, { done: load, items });
        else if (act === 'example') cfg.openEditor(null, { done: load, items, prefill: cfg.examples[Number(b.dataset.i)] });
        else if (act === 'edit' && it) cfg.openEditor(it, { done: load, items });
        else if (act === 'delete' && it) removeOne(it);
        else if (act === 'select' && it) { selected.has(it.id) ? selected.delete(it.id) : selected.add(it.id); renderBulk(); row.classList.toggle('is-sel', selected.has(it.id)); b.setAttribute('aria-checked', selected.has(it.id)); }
        else if (act === 'bulk-clear') { selected.clear(); renderBulk(); renderList(); }
        else if (act === 'bulk-delete') removeMany();
        else if (act === 'toggle-search') { searchOpen = true; wantFocus = true; renderTabs(); }
        else if (act === 'clear-search') { searchOpen = false; wantFocus = false; query = ''; renderTabs(); renderList(); }
        else if (act === 'sort') {
          openMenu(b, [{ title: t('Ordenar por') }, ...cfg.sortOptions.map((o) => ({
            label: o.label, sel: sort === o.id, onClick: () => { sort = o.id; renderList(); },
          }))]);
        } else if (it && cfg.onAct) cfg.onAct(act, it, helpers);
      });
      root.addEventListener('input', (e) => {
        if (e.target.matches('[data-search-input]')) { query = e.target.value.trim(); wantFocus = true; renderList(); }
      });
      root.addEventListener('keydown', (e) => {
        if (e.key === 'Escape' && e.target.matches('[data-search-input]')) { e.stopPropagation(); $('[data-act=clear-search]', root).click(); }
      });
      tabsKeys(root, (id) => { tab = id; selected.clear(); renderAll(); });

      renderAll();
      load();
      return () => { alive = false; };
    },
  };
}

export { highlight };
