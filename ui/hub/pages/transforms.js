// Transforms: reescribir el texto seleccionado con un modelo local (Ollama). [spec §2.3 Transforms]
import { icon } from '../../shared/icons.js';
import { invoke, state } from '../api.js';
import { t, esc, $, toast, errText, keysHtml, hotkeyText, confirmDialog } from '../util.js';
import { pageHeader, selectHtml } from '../ui.js';

const DEFAULTS = {
  Polish: 'Reescribe el texto para que sea más claro y conciso. Mantén el mismo idioma, el tono y el significado. Corrige gramática y puntuación. Devuelve solo el texto resultante, sin explicaciones.',
  'Prompt Engineer': 'Convierte las ideas desordenadas del texto en un prompt de IA limpio y optimizado: rol, tarea, puntos clave y tono, en secciones breves. Mantén el idioma del texto. Devuelve solo el prompt.',
};
const SUB = () => ({
  Polish: t('Mejora la claridad y la concisión'),
  'Prompt Engineer': t('Crea prompts óptimos'),
});
// El atajo sale SIEMPRE de los ajustes (hotkeyTransform). Con una tecla que no es un dígito solo el Transform 1 lo tiene;
// con un dígito (p. ej. Win + Alt + 1) los dos siguientes usan 2 y 3.
function hkIdx(n) {
  const base = state.settings && state.settings.hotkeyTransform;
  if (!base) return null;
  if (Number(n) === 1) return base;
  return /^\d$/.test(base.key || '') ? { ...base, key: String(n) } : null;
}
const hkLabel = (n) => { const h = hkIdx(n); return h ? hotkeyText(h) : null; };

export default {
  id: 'transforms',
  mount(root) {
    let list = [], editing = null, ollama = null, alive = true;
    root.innerHTML = '<div class="page-inner" id="tr-root"></div>';
    const el = $('#tr-root', root);

    function ollamaBanner() {
      if (!ollama) return '';
      const model = state.settings.ollamaModel;
      if (!ollama.reachable) {
        return `<div class="aviso amar"><div class="in"><span class="ic-l">${icon('triangle-alert', 20)}</span>
          <div class="tx"><b>${esc(t('Ollama no responde'))}</b>
            <span>${t('Los Transforms usan un modelo local con Ollama. Ábrelo desde el menú Inicio o ejecuta {a} en una terminal; después descarga el modelo con {b}.', {
              a: '<code>ollama serve</code>', b: `<code>ollama pull ${esc(model)}</code>` })}</span></div>
          <button class="btn btn-secondary btn-sm" data-act="ollama-retry">${icon('refresh-cw', 14)}${esc(t('Reintentar'))}</button></div></div>`;
      }
      const has = ollama.models.some((m) => m === model || m.split(':')[0] === model.split(':')[0] && !model.includes(':'));
      if (!has) {
        return `<div class="aviso amar"><div class="in"><span class="ic-l">${icon('triangle-alert', 20)}</span>
          <div class="tx"><b>${esc(t('Falta el modelo {m}', { m: model }))}</b>
            <span>${t('Descárgalo con {a} o elige otro en Configuración › Experimental.', { a: `<code>ollama pull ${esc(model)}</code>` })}</span></div>
          <button class="btn btn-secondary btn-sm" data-act="ollama-retry">${icon('refresh-cw', 14)}${esc(t('Reintentar'))}</button></div></div>`;
      }
      return `<div class="ollama-ok"><span class="badge ok"><i class="dot"></i>${esc(t('Ollama listo'))} · ${esc(model)}</span></div>`;
    }

    function renderList() {
      const sub = SUB();
      el.innerHTML = `${pageHeader(t('Transforms'), {
        sub: t('Transforma tu texto para que <em>suene como quieras</em>.'),
      })}
        <div class="feat-chips">${[t('Mantén tu tono'), t('Hacer más conciso'), t('Reformular para mayor claridad'), t('Reorganizar para mejor lectura'), t('Optimiza el texto para LLM')]
          .map((x) => `<span class="chip static">${icon('check', 13)}${esc(x)}</span>`).join('')}</div>
        <div id="tr-ollama">${ollamaBanner()}</div>
        <h2 class="sec-t">${esc(t('Mis Transforms'))}</h2>
        <div class="tr-grid">
          ${list.map((x) => `<button class="tr-card tarj" data-open="${x.id}"><span class="in">
              <span class="tr-ic">${icon(x.name === 'Prompt Engineer' ? 'sparkles' : 'wand-sparkles', 20)}</span>
              <span class="tr-n"><b>${esc(x.name)}</b><small>${esc(sub[x.name] || (x.prompt.length > 60 ? x.prompt.slice(0, 58) + '…' : x.prompt) || t('Atajo personalizado de Transform'))}</small></span>
              ${x.hotkeyIndex && hkIdx(x.hotkeyIndex) ? `<span class="tr-k">${keysHtml(hkIdx(x.hotkeyIndex))}</span>` : `<span class="tr-k muted xs">${esc(t('Sin atajo'))}</span>`}
            </span></button>`).join('')}
          <button class="tr-card tr-new tarj" data-open="new"><span class="in">
            <span class="tr-ic">${icon('plus', 20)}</span>
            <span class="tr-n"><b>${esc(t('Crea el tuyo'))}</b><small>${esc(t('Sube tu propio prompt'))}</small></span></span></button>
        </div>
        <p class="muted tr-tip">${t('Selecciona texto en cualquier app y pulsa {k} (el atajo del Transform). Pronto lo reescribe en tu equipo con {a} y lo pega en su sitio.', { k: keysHtml(state.settings.hotkeyTransform), a: '<b>Ollama</b>' })}</p>`;
    }

    function renderEditor() {
      const x = editing, isNew = !x.id;
      const used = new Set(list.filter((o) => o.id !== x.id && o.hotkeyIndex).map((o) => o.hotkeyIndex));
      el.innerHTML = `
        <button class="back" data-act="back">${icon('chevron-left', 17)}${esc(t('Transforms'))}</button>
        ${pageHeader(isNew ? t('Crea el tuyo') : x.name, { sub: isNew ? t('Configura un atajo de teclado para aplicar este Transform.') : (SUB()[x.name] || '') })}
        <div id="tr-ollama">${ollamaBanner()}</div>
        <div class="tr-form tarj"><div class="in">
          <div class="field"><label for="t-name">${esc(t('Nombre'))}</label>
            <input class="input" id="t-name" maxlength="40" placeholder="${esc(t('Modo jefe'))}" value="${esc(x.name || '')}" ${x.builtin ? 'readonly' : ''}></div>
          <div class="field"><span class="lab">${esc(t('Atajo'))}</span>
            ${selectHtml('hk', [{ value: '', label: t('Sin atajo') }, ...[1, 2, 3].filter((n) => hkLabel(n) || x.hotkeyIndex === n).map((n) => ({ value: n, label: `${hkLabel(n) || t('Sin atajo')}${used.has(n) ? ' · ' + t('en uso') : ''}` }))], x.hotkeyIndex || '', { label: t('Atajo') })}
            <span class="hint">${esc(t('Selecciona texto en cualquier app y pulsa el atajo para aplicar este Transform.'))}${hkIdx(1) && !/^\d$/.test((state.settings.hotkeyTransform || {}).key || '') ? ' ' + esc(t('Con este atajo solo el primer Transform tiene tecla propia; cámbialo en Atajos.')) : ''}</span></div>
          <div class="field"><label for="t-prompt">${esc(t('Prompt'))}</label>
            <textarea class="textarea" id="t-prompt" rows="8" placeholder="${esc(t('¿Cómo quieres que este Transform modifique tu texto?'))}">${esc(x.prompt || '')}</textarea>
            <span class="hint">${esc(t('Describe qué debe hacer con el texto seleccionado. El modelo devuelve solo el resultado.'))}</span></div>
          <p class="form-err" id="t-err" role="alert" hidden></p>
          <div class="form-actions">
            ${x.builtin ? `<button class="btn btn-ghost" data-act="reset">${icon('rotate-ccw', 15)}${esc(t('Restablecer valores predeterminados'))}</button>` : ''}
            ${!isNew && !x.builtin ? `<button class="btn btn-danger" data-act="delete">${icon('trash-2', 15)}${esc(t('Eliminar'))}</button>` : ''}
            <span class="sp"></span>
            <button class="btn btn-ghost" data-act="back">${esc(t('Cancelar'))}</button>
            <button class="btn btn-primary" data-act="save">${esc(t('Guardar'))}</button>
          </div></div></div>`;
    }

    async function checkOllama() {
      try { ollama = await invoke('ollama_status'); } catch (_) { ollama = { reachable: false, models: [] }; }
      if (!alive) return;
      const box = $('#tr-ollama', el); if (box) box.innerHTML = ollamaBanner();
    }
    async function load() {
      try { list = await invoke('list_transforms'); } catch (e) { toast(errText(e), { kind: 'err' }); list = []; }
      if (!alive) return;
      editing ? renderEditor() : renderList();
    }

    root.addEventListener('click', async (e) => {
      const open = e.target.closest('[data-open]');
      if (open) {
        editing = open.dataset.open === 'new' ? { id: 0, name: '', prompt: '', builtin: false, hotkeyIndex: null }
          : { ...list.find((x) => String(x.id) === open.dataset.open) };
        renderEditor(); el.scrollIntoView({ block: 'start' }); $('.page-scroll')?.scrollTo({ top: 0 });
        return;
      }
      const b = e.target.closest('[data-act]'); if (!b) return;
      const act = b.dataset.act;
      if (act === 'back') { editing = null; renderList(); }
      else if (act === 'ollama-retry') { b.disabled = true; await checkOllama(); }
      else if (act === 'reset') { $('#t-prompt', el).value = DEFAULTS[editing.name] || ''; toast(t('Configuración restablecida'), { kind: 'info' }); }
      else if (act === 'delete') {
        const ok = await confirmDialog({ title: t('¿Eliminar este Transform?'), body: t("'{x}' se eliminará permanentemente.", { x: esc(editing.name) }) });
        if (!ok) return;
        try { await invoke('delete_transform', { id: editing.id }); toast(t('Transform eliminado')); editing = null; await load(); }
        catch (err) { toast(errText(err), { kind: 'err' }); }
      } else if (act === 'save') {
        const err = $('#t-err', el); err.hidden = true;
        const name = $('#t-name', el).value.trim(), prompt = $('#t-prompt', el).value.trim();
        const idx = $('[data-select=hk]', el).value;
        const showErr = (m) => { err.textContent = m; err.hidden = false; };
        if (!name) { showErr(t('Ponle un nombre.')); return; }
        if (!prompt) { showErr(t('Escribe el prompt.')); return; }
        const hotkeyIndex = idx ? Number(idx) : null;
        const clash = hotkeyIndex && list.find((o) => o.id !== editing.id && o.hotkeyIndex === hotkeyIndex);
        if (clash) { showErr(t('Ese atajo ya lo usa «{x}».', { x: clash.name })); return; }
        b.disabled = true;
        try {
          await invoke('save_transform', { transform: { id: editing.id || 0, name, prompt, builtin: !!editing.builtin, hotkeyIndex } });
          toast(t('Transform guardado'));
          editing = null; await load();
        } catch (er) { showErr(errText(er) || t('No se pudo guardar, inténtalo de nuevo')); b.disabled = false; }
      }
    });

    renderList();
    load();
    checkOllama();
    return () => { alive = false; };
  },
};
