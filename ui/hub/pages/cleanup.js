// Limpieza automática (niveles) y pulido con IA local (aiPolish). Vivía en Estilo como pestaña «Limpieza automática»; desde
// el 24-oct-2026 está en Configuración → Limpieza e IA: habla de modelos y de Ollama, y los primerizos no la entendían junto al tono.
import { icon } from '../../shared/icons.js';
import { state, saveSettings, invoke } from '../api.js';
import { t, esc, $, toast } from '../util.js';
import { switchHtml, selectHtml } from '../ui.js';
import { aiSlot } from '../ai.js';

const RAW = () => t('Eh, bueno, pues, mañana a las diez, no, mejor a las once tenemos que que revisar el el informe');
const LEVELS = () => [
  { id: 'none', name: t('Ninguno'), desc: t('Transcribe exactamente lo que dijiste, incluidos los errores.'), out: RAW() },
  { id: 'light', name: t('Ligero'), desc: t('Quita muletillas («eh», «mmm», «um») y espacios sobrantes.'), out: t('Bueno, pues, mañana a las diez, no, mejor a las once tenemos que que revisar el el informe') },
  { id: 'medium', name: t('Medio'), desc: t('Además aplica correcciones habladas («no, mejor…») y quita palabras repetidas.'), out: t('Bueno, pues, mañana a las once tenemos que revisar el informe'), def: true },
  { id: 'high', name: t('Alto'), desc: t('Además quita los arranques en falso del principio («bueno, pues», «a ver,»).'), out: t('Mañana a las once tenemos que revisar el informe') },
];
const FAST = 'qwen3:1.7b';

/** Pinta la limpieza y el pulido en `host` y atiende sus clics. Devuelve la función que lo desmonta.
 *  `aiCta: false` cuando la pantalla ya tiene su propio botón de «Activar IA local» (Configuración): dos a la vez se pisaban. */
export function mountCleanup(host, { aiCta = true } = {}) {
  let oll = null, alive = true;

  function polishCard() {
    const s = state.settings, on = !!s.aiPolish, cur = s.aiPolishModel || FAST;
    const secs = Math.max(1, Math.round((s.aiPolishTimeoutMs || 6000) / 1000));
    const models = [...new Set([...(oll && oll.models ? oll.models : []), cur])];
    const label = (m) => m + (m === FAST ? ' · ' + t('más rápido') : '');
    let warn = '';
    if (oll && !oll.reachable) {
      warn = `<div class="aviso amar"><div class="in"><span class="ic-l">${icon('triangle-alert', 18)}</span><div class="tx"><b>${esc(t('Ollama no responde'))}</b>
        <span>${esc(t('Pronto puede instalarlo y arrancarlo por ti. Mientras tanto, los dictados se pegan sin pulir.'))}</span>${aiCta ? aiSlot('cta') : ''}</div>
        <button class="btn btn-secondary btn-sm" data-act="pol-check">${icon('refresh-cw', 14)}${esc(t('Reintentar'))}</button></div></div>`;
    } else if (oll && !oll.models.includes(cur)) {
      warn = `<div class="aviso amar"><div class="in"><span class="ic-l">${icon('triangle-alert', 18)}</span><div class="tx"><b>${esc(t('Falta el modelo {m}', { m: cur }))}</b>
        <span>${esc(t('Pronto puede descargarlo por ti (usa {m}), o elige otro de la lista.', { m: FAST }))}</span>${aiCta ? aiSlot('cta') : ''}</div>
        <button class="btn btn-secondary btn-sm" data-act="pol-check">${icon('refresh-cw', 14)}${esc(t('Reintentar'))}</button></div></div>`;
    }
    return `<div class="tarj polish ${on ? 'on' : ''}" id="polish-card"><div class="in">
      <div class="pol-hd"><span class="pol-ic">${icon('wand-sparkles', 22)}</span>
        <div class="pol-t"><b>${esc(t('Pulir cada dictado con IA local'))}</b>
          <span>${esc(t('Ollama corrige puntuación, tildes y concordancia antes de pegar. No añade ni quita contenido.'))}</span></div>
        ${switchHtml('aiPolish', on, t('Pulir cada dictado con IA local'))}</div>
      ${on ? `<p class="pol-note">${esc(t('Añade una espera antes de pegar que depende de tu PC; si Ollama no responde en {s} s se pega sin pulir.', { s: secs }))}</p>` : ''}
      <div class="pol-ex" aria-label="${esc(t('Ejemplo'))}">
        <div class="caja"><span class="rotulo">${esc(t('Sin pulir'))}</span><p>eh pues mañana a las diez tenemos reunion con los de la imprenta y creo que hay que llevar los presupuesto</p></div>
        <span class="pol-arrow" aria-hidden="true">${icon('arrow-up-right', 18)}</span>
        <div class="caja on"><span class="rotulo">${esc(t('Pulido con IA'))}</span><p><span class="chg">Pues</span> mañana a las diez tenemos <span class="chg">reunión</span> con los de la imprenta y creo que hay que llevar los <span class="chg">presupuestos.</span></p></div>
      </div>
      <div class="pol-row"><div class="pol-m"><b>${esc(t('Modelo'))}</b><span>${esc(t('Más pequeño = más rápido. El modelo se ejecuta en tu equipo con Ollama.'))}</span></div>
        ${selectHtml('aiPolishModel', models.map((m) => ({ value: m, label: label(m) })), cur, { label: t('Modelo') })}</div>
      ${warn}
    </div></div>`;
  }
  function refreshPolish() { const c = $('#polish-card', host); if (c) c.outerHTML = polishCard(); }
  async function checkOllama() {
    try { oll = await invoke('ollama_status'); } catch (_) { oll = { reachable: false, models: [] }; }
    if (alive) refreshPolish();
  }

  function render() {
    const lv = LEVELS(), cur = state.settings.cleanupLevel || 'medium';
    host.innerHTML = `
      <section class="sgroup"><h3 class="overline">${esc(t('Limpieza automática'))}</h3>
      <div class="aviso vio"><div class="in"><span class="ic-l">${icon('sparkles', 20)}</span>
        <div class="tx"><b>${esc(t('La limpieza automática se aplica a todos tus dictados'))}</b>
        <span>${esc(t('Elige el nivel de limpieza que se aplica cada vez, en todas las aplicaciones. Tu dictado original nunca se pierde: lo guarda el historial.'))}</span></div></div></div>
      <div class="caja orig"><span class="rotulo">${esc(t('Lo que dices'))}</span><p>${esc(RAW())}</p></div>
      <div class="levels" role="radiogroup" aria-label="${esc(t('Nivel de limpieza'))}">${lv.map((l) => `
        <button class="lvl tarj ${cur === l.id ? 'on' : ''}" role="radio" aria-checked="${cur === l.id}" data-level="${l.id}"><span class="in">
          <span class="lvl-hd"><span class="radio"></span><b>${esc(l.name)}</b>${l.def ? `<span class="badge vio">${esc(t('Por defecto'))}</span>` : ''}</span>
          <span class="lvl-d">${esc(l.desc)}</span>
          <span class="lvl-o">${esc(l.out)}${l.id === 'none' ? '' : '.'}</span></span></button>`).join('')}</div></section>
      <section class="sgroup"><h3 class="overline">${esc(t('Pulir con IA'))}</h3>${polishCard()}</section>`;
  }

  const onChange = async (e) => {
    const sel = e.target.closest('[data-select=aiPolishModel]');
    if (!sel) return;
    e.stopPropagation(); // no es un ajuste de la pestaña: Configuración no debe guardarlo otra vez
    try { await saveSettings({ aiPolishModel: sel.value }); toast(t('Modelo guardado')); } catch (_) { /* ya avisado */ }
    refreshPolish();
  };
  const onClick = async (e) => {
    const sw = e.target.closest('[data-switch=aiPolish]');
    if (sw) {
      e.stopPropagation();
      try { await saveSettings({ aiPolish: !state.settings.aiPolish }); toast(state.settings.aiPolish ? t('Pulido con IA activado') : t('Pulido con IA desactivado')); } catch (_) { /* */ }
      refreshPolish(); return;
    }
    if (e.target.closest('[data-act=pol-check]')) { e.stopPropagation(); await checkOllama(); return; }
    const lvl = e.target.closest('[data-level]');
    if (!lvl) return;
    if (state.settings.cleanupLevel === lvl.dataset.level) return;
    try { await saveSettings({ cleanupLevel: lvl.dataset.level }); toast(t('Nivel de limpieza guardado')); } catch (_) { /* ya avisado */ }
    render();
  };
  host.addEventListener('change', onChange);
  host.addEventListener('click', onClick);
  render();
  checkOllama();
  return Object.assign(() => { alive = false; host.removeEventListener('change', onChange); host.removeEventListener('click', onClick); }, { recheck: checkOllama });
}
