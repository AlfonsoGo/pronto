// Cómo escribe (antes «Estilo»): tono por tipo de app. [spec §2.3 Style]
// 24-oct-2026: los cuatro tipos de app pasan de pestañas grises (nadie las veía como opciones) a tarjetas grandes con sus
// apps y el tono elegido; la limpieza automática y el pulido con IA se van a Configuración → Limpieza e IA (`cleanup.js`).
import { icon } from '../../shared/icons.js';
import { state, saveSettings } from '../api.js';
import { t, esc, $, toast, getLang } from '../util.js';
import { pageHeader, tabsKeys, toneName } from '../ui.js';

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
    formal: '<h>H</h>ola<c>,</c> si tienes un momento<c>,</c> hablemos sobre los excelentes resultados<c>.</c>',
    casual: '<h>H</h>ola<c>,</c> si tienes un momento hablemos sobre los excelentes resultados',
    veryCasual: '<h>o</h>ye, si tienes tiempo, hablemos sobre los excelentes resultados',
    excited: '<h>H</h>ola<c>,</c> si tienes un momento<c>,</c> hablemos sobre los excelentes resultados<clg>!</clg>',
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
// Ejemplos en inglés para el hub en inglés (rompedor 23-oct, hub BAJA 8): mismas marcas <h> (mayúscula) y <c>/<clg> (puntuación).
const PREVIEWS_EN = {
  personal: {
    formal: "<h>H</h>ey<c>,</c> are you free for lunch tomorrow? <h>L</h>et's do 12 if that works for you<c>.</c>",
    casual: "<h>H</h>ey, are you free for lunch tomorrow? <h>L</h>et's do 12 if that works for you",
    veryCasual: "<h>h</h>ey, are you free for lunch tomorrow? <h>l</h>et's do 12 if that works for you",
    excited: "<h>H</h>i<clg>,</clg> are you free for lunch tomorrow? See you at 12 if that works for you<clg>!</clg>",
  },
  work: {
    formal: "<h>H</h>ey<c>,</c> if you have a moment<c>,</c> let's talk about the excellent results<c>.</c>",
    casual: "<h>H</h>ey<c>,</c> if you have a moment let's talk about the excellent results",
    veryCasual: "<h>h</h>ey, if you have time, let's talk about the excellent results",
    excited: "<h>H</h>ey<c>,</c> if you have a moment<c>,</c> let's talk about the excellent results<clg>!</clg>",
  },
  email: {
    formal: "<h>H</h>i Alex<c>,</c><br><br><h>I</h>t was great talking to you today. <h>I</h>'m looking forward to our next chat<c>.</c><br><br>Best<c>,</c><br>Mary",
    casual: "<h>H</h>i Alex<c>,</c> great talking to you today. <h>I</h>'m looking forward to our next chat<c>.</c><br><br>Best<c>,</c><br>Mary",
    veryCasual: "<h>h</h>i alex, it was great talking to you today. <h>l</h>ooking forward to our next chat<br><br>best,<br>mary",
    excited: "<h>H</h>i Alex<c>,</c><br><br><h>I</h>t was great talking to you today. <h>I</h>'m looking forward to our next chat<clg>!</clg><br><br>Best<c>,</c><br>Mary",
  },
  other: {
    formal: "<h>S</h>o far I am enjoying the new exercise routine<c>.</c><br><br><h>I</h> am looking forward to tomorrow's workout<c>,</c> especially after a full night of rest<c>.</c>",
    casual: "<h>S</h>o far I'm enjoying the new exercise routine.<br><br><h>I</h>'m looking forward to tomorrow's workout, especially after a full night of rest",
    veryCasual: "<h>s</h>o far i'm enjoying the new exercise routine<br><br><h>l</h>ooking forward to training tomorrow, especially after a full night of rest",
    excited: "<h>S</h>o far I'm enjoying the new exercise routine<clg>!</clg><br><br><h>I</h>'m looking forward to tomorrow's workout<clg>,</clg> especially after a full night of rest<clg>!</clg>",
  },
};
const previews = () => (getLang() === 'en' ? PREVIEWS_EN : PREVIEWS);
const rich = (s) => s.replace(/<h>(.*?)<\/h>/g, '<span class="hl">$1</span>').replace(/<(c|clg)>(.*?)<\/\1>/g, '<span class="hp">$2</span>');

const TONE_INFO = () => ({
  formal: { name: toneName('formal'), sub: t('Mayúsculas y todos los puntos y comas') },
  casual: { name: toneName('casual'), sub: t('Mayúsculas y menos puntuación') },
  veryCasual: { name: toneName('veryCasual'), sub: t('Sin mayúsculas y con menos puntuación') },
  excited: { name: toneName('excited'), sub: t('Más signos de exclamación') },
});
const CTX_INFO = () => ({
  personal: { name: t('WhatsApp y mensajes'), icon: 'message-square', apps: 'WhatsApp · Telegram · Discord' },
  work: { name: t('Chats de trabajo'), icon: 'users', apps: 'Slack · Microsoft Teams' },
  email: { name: t('Correo'), icon: 'mail', apps: 'Outlook · Thunderbird · Gmail' },
  other: { name: t('Todo lo demás'), icon: 'monitor', apps: t('Word, el navegador, las notas y el resto') },
});


export default {
  id: 'style',
  mount(root) {
    let tab = 'personal';
    root.innerHTML = `<div class="page-inner">${pageHeader(t('Cómo escribe'), { sub: t('Elige si lo que dictas sale formal o más cercano. Pronto lo aplica según el programa en el que escribas.') })}
      <p class="st-step">${esc(t('1. Elige dónde escribes'))}</p><div id="st-tabs"></div>
      <p class="st-step">${esc(t('2. Elige cómo quieres que salga'))}</p><div id="st-body"></div></div>`;
    const elTabs = $('#st-tabs', root), elBody = $('#st-body', root);

    function render() {
      const s = state.settings, ci = CTX_INFO(), ti = TONE_INFO();
      elTabs.innerHTML = `<div class="ctx-grid" role="tablist" aria-label="${esc(t('Dónde escribes'))}">${CTX.map((c) => {
        const on = c === tab;
        return `<button class="ctx tarj ${on ? 'on' : ''}" role="tab" data-tab="${c}" aria-selected="${on}" tabindex="${on ? 0 : -1}"><span class="in">
          <span class="ctx-ic" aria-hidden="true">${icon(ci[c].icon, 20)}</span>
          <span class="ctx-t"><b>${esc(ci[c].name)}</b><small>${esc(ci[c].apps)}</small></span>
          <span class="ctx-cur">${esc(ti[s.styles[c] || 'casual'].name)}</span></span></button>`;
      }).join('')}</div>`;
      const cur = s.styles[tab] || 'casual';
      elBody.innerHTML = `
        <div class="tones" role="radiogroup" aria-label="${esc(t('Cómo sale en {x}', { x: ci[tab].name }))}">${TONES.map((tn) => `
          <button class="tone tarj ${cur === tn ? 'on' : ''}" role="radio" aria-checked="${cur === tn}" data-tone="${tn}"><span class="in">
            <span class="tone-hd"><span class="tone-n"><b>${esc(ti[tn].name)}</b><small>${esc(ti[tn].sub)}</small></span><span class="radio"></span></span>
            <span class="tone-pv caja">${rich(previews()[tab][tn])}</span>${cur === tn ? `<span class="tone-on">${icon('check', 14)}${esc(t('Elegido'))}</span>` : ''}</span></button>`).join('')}</div>
        <p class="muted sm st-note">${esc(t('Solo cambia mayúsculas y puntuación: no añade saludos, despedidas ni palabras que no hayas dicho.'))}</p>`;
    }

    root.addEventListener('click', async (e) => {
      const tb = e.target.closest('[data-tab]');
      if (tb) { tab = tb.dataset.tab; render(); return; }
      const tone = e.target.closest('[data-tone]');
      if (!tone) return;
      if (state.settings.styles[tab] === tone.dataset.tone) return;
      try {
        await saveSettings({ styles: { ...state.settings.styles, [tab]: tone.dataset.tone } });
        render(); toast(t('Guardado: {a} en {b}', { a: TONE_INFO()[tone.dataset.tone].name, b: CTX_INFO()[tab].name }));
      } catch (_) { render(); }
    });
    tabsKeys(root, (id) => { tab = id; render(); });
    render();
  },
};
