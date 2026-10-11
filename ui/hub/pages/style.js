// Estilo: tono por tipo de app + limpieza automática. [spec §2.3 Style]
import { icon } from '../../shared/icons.js';
import { state, saveSettings, invoke } from '../api.js';
import { t, esc, $, $$, toast, errText } from '../util.js';
import { pageHeader, tabsHtml, tabsKeys, switchHtml, selectHtml } from '../ui.js';
import { aiSlot, aiMount } from '../ai.js';

const CTX = ['personal', 'work', 'email', 'other'];
const TONES = ['formal', 'casual', 'veryCasual', 'excited'];

// <h> = mayúscula inicial, <c>/<clg> = puntuación (se resaltan en violeta)
const PREVIEWS = {
  personal: {
    formal: '<h>O</h>ye<c>,</c> ¿tienes libre mañana para almorzar? <h>H</h>agámoslo a las 12 si te queda bien<c>.</c>',
    casual: '<h>O</h>ye, ¿tienes libre mañana para almorzar? <h>H</h>agámoslo a las 12 si te queda bien',
    veryCasual: '<h>o</h>ye, ¿tienes libre mañana para almorzar? <h>h</h>agámoslo a las 12 si te queda bien',
    excited: '<h>H</h>ola<clg>,</clg> ¿tienes tiempo para almorzar mañana? Nos vemos a las 12 si te queda bien<clg>!</clg>',
  },
  work: {
    formal: '<h>H</h>ey<c>,</c> si tienes un momento<c>,</c> hablemos sobre los excelentes resultados<c>.</c>',
    casual: '<h>H</h>ey<c>,</c> si tienes un momento hablemos sobre los excelentes resultados',
    veryCasual: '<h>o</h>ye, si tienes tiempo, hablemos sobre los excelentes resultados',
    excited: '<h>H</h>ey<c>,</c> si tienes un momento<c>,</c> hablemos sobre los excelentes resultados<clg>!</clg>',
  },
  email: {
    formal: '<h>H</h>ola Alex<c>,</c><br><br><h>Q</h>ué bueno hablar contigo hoy. <h>E</h>spero con ganas nuestra próxima charla<c>.</c><br><br>Saludos<c>,</c><br>Mary',
    casual: '<h>H</h>ola Alex<c>,</c> qué bueno hablar contigo hoy. <h>E</h>spero con ganas nuestra próxima charla<c>.</c><br><br>Saludos<c>,</c><br>Mary',
    veryCasual: '<h>h</h>ola alex, fue genial hablar contigo hoy. <h>e</h>spero con ganas nuestra próxima charla<br><br>saludos,<br>mary',
    excited: '<h>H</h>ola Alex<c>,</c><br><br><h>Q</h>ué bueno hablar contigo hoy. <h>E</h>spero con ganas nuestra próxima charla<clg>!</clg><br><br>Saludos<c>,</c><br>Mary',
  },
  other: {
    formal: '<h>H</h>asta ahora estoy disfrutando la nueva rutina de ejercicio<c>.</c><br><br><h>T</h>engo muchas ganas del entrenamiento de mañana<c>,</c> sobre todo después de una noche completa de descanso<c>.</c>',
    casual: '<h>H</h>asta ahora estoy disfrutando la nueva rutina de ejercicio.<br><br><h>T</h>engo muchas ganas del entrenamiento de mañana, sobre todo después de una noche completa de descanso',
    veryCasual: '<h>h</h>asta ahora estoy disfrutando la nueva rutina de ejercicio<br><br><h>t</h>engo muchas ganas de entrenar mañana, sobre todo después de una noche completa de descanso',
    excited: '<h>H</h>asta ahora estoy disfrutando la nueva rutina de ejercicio<clg>!</clg><br><br><h>T</h>engo muchas ganas del entrenamiento de mañana<clg>,</clg> sobre todo después de una noche completa de descanso<clg>!</clg>',
  },
};
const rich = (s) => s.replace(/<h>(.*?)<\/h>/g, '<span class="hl">$1</span>').replace(/<(c|clg)>(.*?)<\/\1>/g, '<span class="hp">$2</span>');

const TONE_INFO = () => ({
  formal: { name: t('Formal.'), sub: t('Mayúsculas + puntuación') },
  casual: { name: t('Casual'), sub: t('Mayúsculas + menos puntuación') },
  veryCasual: { name: t('muy casual'), sub: t('Sin mayúsculas + menos puntuación') },
  excited: { name: t('¡Entusiasta!'), sub: t('Más exclamaciones') },
});
const CTX_INFO = () => ({
  personal: { title: t('Este estilo se aplica en mensajeros personales'), apps: 'WhatsApp · Telegram · Discord' },
  work: { title: t('Este estilo se aplica en mensajeros de trabajo'), apps: 'Slack · Microsoft Teams' },
  email: { title: t('Este estilo se aplica en todas las aplicaciones principales de correo'), apps: 'Outlook · Thunderbird · Gmail' },
  other: { title: t('Este estilo se aplica en todas las demás aplicaciones'), apps: t('Navegador, editores, notas y el resto') },
});

const RAW = 'Eh, bueno, pues, mañana a las diez, no, mejor a las once tenemos que que revisar el el informe';
const LEVELS = () => [
  { id: 'none', name: t('Ninguno'), desc: t('Transcribe exactamente lo que dijiste, incluidos los errores.'), out: RAW },
  { id: 'light', name: t('Ligero'), desc: t('Quita muletillas («eh», «mmm», «um») y espacios sobrantes.'), out: 'Bueno, pues, mañana a las diez, no, mejor a las once tenemos que que revisar el el informe' },
  { id: 'medium', name: t('Medio'), desc: t('Además aplica correcciones habladas («no, mejor…») y quita palabras repetidas.'), out: 'Bueno, pues, mañana a las once tenemos que revisar el informe', def: true },
  { id: 'high', name: t('Alto'), desc: t('Además quita los arranques en falso del principio («bueno, pues», «a ver,»).'), out: 'Mañana a las once tenemos que revisar el informe' },
];

export default {
  id: 'style',
  mount(root) {
    let tab = 'personal';
    let oll = null, alive = true;
    const tabs = () => [
      { id: 'personal', label: t('Mensajes personales') }, { id: 'work', label: t('Mensajes de trabajo') },
      { id: 'email', label: t('Correo') }, { id: 'other', label: t('Otro') },
      { id: 'cleanup', label: t('Limpieza automática'), badge: 'Beta' },
    ];
    root.innerHTML = `<div class="page-inner">${pageHeader(t('Estilo'), { sub: t('Elige cómo suena lo que dictas según la app en la que escribes.') })}
      <div id="st-tabs"></div><div id="st-body"></div></div>`;
    const elTabs = $('#st-tabs', root), elBody = $('#st-body', root);


    // ---- Pulido con IA local (aiPolish) ---------------------------------------------------------
    const FAST = 'qwen3:1.7b';
    function polishCard() {
      const s = state.settings, on = !!s.aiPolish, cur = s.aiPolishModel || FAST;
      const secs = Math.max(1, Math.round((s.aiPolishTimeoutMs || 6000) / 1000));
      const models = [...new Set([...(oll && oll.models ? oll.models : []), cur])];
      const label = (m) => m + (m === FAST ? ' · ' + t('más rápido') : '');
      let warn = '';
      if (oll && !oll.reachable) {
        warn = `<div class="aviso amar"><div class="in"><span class="ic-l">${icon('triangle-alert', 18)}</span><div class="tx"><b>${esc(t('Ollama no responde'))}</b>
          <span>${esc(t('Pronto puede instalarlo y arrancarlo por ti. Mientras tanto, los dictados se pegan sin pulir.'))}</span>${aiSlot('cta')}</div>
          <button class="btn btn-secondary btn-sm" data-act="pol-check">${icon('refresh-cw', 14)}${esc(t('Reintentar'))}</button></div></div>`;
      } else if (oll && !oll.models.includes(cur)) {
        warn = `<div class="aviso amar"><div class="in"><span class="ic-l">${icon('triangle-alert', 18)}</span><div class="tx"><b>${esc(t('Falta el modelo {m}', { m: cur }))}</b>
          <span>${esc(t('Pronto puede descargarlo por ti (usa {m}), o elige otro de la lista.', { m: FAST }))}</span>${aiSlot('cta')}</div>
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
    function refreshPolish() { const c = $('#polish-card', root); if (c) c.outerHTML = polishCard(); }
    async function checkOllama() {
      try { oll = await invoke('ollama_status'); } catch (_) { oll = { reachable: false, models: [] }; }
      if (alive) refreshPolish();
    }

    function render() {
      elTabs.innerHTML = tabsHtml(tabs(), tab);
      const s = state.settings;
      if (tab === 'cleanup') {
        const lv = LEVELS(), cur = s.cleanupLevel || 'medium';
        elBody.innerHTML = `
          ${polishCard()}
          <div class="aviso vio"><div class="in"><span class="ic-l">${icon('sparkles', 20)}</span>
            <div class="tx"><b>${esc(t('La limpieza automática se aplica a todos tus dictados'))}</b>
            <span>${esc(t('Elige el nivel de limpieza que se aplica cada vez, en todas las aplicaciones. Tu dictado original nunca se pierde: lo guarda el historial.'))}</span></div></div></div>
          <div class="caja orig"><span class="rotulo">${esc(t('Lo que dices'))}</span><p>${esc(RAW)}</p></div>
          <div class="levels" role="radiogroup" aria-label="${esc(t('Nivel de limpieza'))}">${lv.map((l) => `
            <button class="lvl tarj ${cur === l.id ? 'on' : ''}" role="radio" aria-checked="${cur === l.id}" data-level="${l.id}"><span class="in">
              <span class="lvl-hd"><span class="radio"></span><b>${esc(l.name)}</b>${l.def ? `<span class="badge vio">${esc(t('Por defecto'))}</span>` : ''}</span>
              <span class="lvl-d">${esc(l.desc)}</span>
              <span class="lvl-o">${esc(l.out)}${l.id === 'none' ? '' : '.'}</span></span></button>`).join('')}</div>`;
        if (!oll) checkOllama(); else refreshPolish();
        return;
      }
      const info = CTX_INFO()[tab], ti = TONE_INFO(), cur = s.styles[tab] || 'casual';
      elBody.innerHTML = `
        <div class="aviso vio"><div class="in"><span class="ic-l">${icon('type', 20)}</span>
          <div class="tx"><b>${esc(info.title)}</b><span>${esc(info.apps)}</span></div></div></div>
        <div class="tones" role="radiogroup" aria-label="${esc(t('Tono'))}">${TONES.map((tn) => `
          <button class="tone tarj ${cur === tn ? 'on' : ''}" role="radio" aria-checked="${cur === tn}" data-tone="${tn}"><span class="in">
            <span class="tone-hd"><span class="tone-n"><b>${esc(ti[tn].name)}</b><small>${esc(ti[tn].sub)}</small></span><span class="radio"></span></span>
            <span class="tone-pv caja">${rich(PREVIEWS[tab][tn])}</span></span></button>`).join('')}</div>`;
    }

    root.addEventListener('change', async (e) => {
      const sel = e.target.closest('[data-select=aiPolishModel]');
      if (!sel) return;
      try { await saveSettings({ aiPolishModel: sel.value }); toast(t('Modelo guardado')); } catch (_) { /* ya avisado */ }
      refreshPolish();
    });
    root.addEventListener('click', async (e) => {
      const sw = e.target.closest('[data-switch=aiPolish]');
      if (sw) {
        try { await saveSettings({ aiPolish: !state.settings.aiPolish }); toast(state.settings.aiPolish ? t('Pulido con IA activado') : t('Pulido con IA desactivado')); } catch (_) { /* */ }
        refreshPolish(); return;
      }
      if (e.target.closest('[data-act=pol-check]')) { await checkOllama(); return; }
      const tb = e.target.closest('[data-tab]');
      if (tb) { tab = tb.dataset.tab; render(); return; }
      const tone = e.target.closest('[data-tone]'), lvl = e.target.closest('[data-level]');
      try {
        if (tone) {
          if (state.settings.styles[tab] === tone.dataset.tone) return;
          await saveSettings({ styles: { ...state.settings.styles, [tab]: tone.dataset.tone } });
          render(); toast(t('Estilo guardado'));
        } else if (lvl) {
          if (state.settings.cleanupLevel === lvl.dataset.level) return;
          await saveSettings({ cleanupLevel: lvl.dataset.level });
          render(); toast(t('Nivel de limpieza guardado'));
        }
      } catch (_) { render(); }
    });
    tabsKeys(root, (id) => { tab = id; render(); });
    render();
    const offAi = aiMount(root, (kind) => { if (kind === 'ready' && tab === 'cleanup') checkOllama(); });
    return () => { alive = false; offAi(); };
  },
};
