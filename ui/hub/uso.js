// Aviso de las estadísticas de uso (PRNT-43): sale una vez a quien aún no ha decidido, con «Sí» y «No» al mismo nivel y sin
// nada marcado. Hasta que decide no se envía nada. Se cambia cuando se quiera en Configuración → Privacidad y datos.
import { state, setUsageStats } from './api.js';
import { t, esc, openModal, hasModal } from './util.js';

export function maybeAskUsage({ blocked = false } = {}) {
  if (blocked || hasModal() || !state.settings || state.settings.usageStats != null) return false;
  const m = openModal({
    title: t('¿Nos ayudas a mejorar Pronto?'),
    width: 520, noX: true, noEsc: true, cls: 'uso-modal',
    body: `<div id="uso-texto"><p>${esc(t('Si aceptas, Pronto enviará a Beltran Labs, una vez al día: el día, la versión, las palabras dictadas, el tiempo de dictado, el número de dictados, el número y los minutos de tus reuniones y el programa donde más has dictado, solo si es uno muy conocido, como Word o Chrome (los demás salen como «otro»). Cada PC se identifica con un código al azar, sin tu nombre ni tu correo.'))}</p>
      <p><b>${esc(t('Nunca lo que dices o escribes.'))}</b> ${esc(t('Ni tu audio, ni el contenido de tus reuniones, ni los títulos de tus ventanas. No estamos en tus conversaciones.'))}</p>
      <p>${esc(t('Se guarda 24 meses en nuestro servidor, sin tu IP. Puedes cambiarlo cuando quieras en Configuración → Privacidad y datos. Más detalle en pronto.beltranlabs.com/privacidad.'))}</p></div>`,
    footer: `<button class="btn btn-secondary" data-uso="no">${esc(t('No, gracias'))}</button><button class="btn btn-secondary" data-uso="si">${esc(t('Sí, acepto'))}</button>`,
  });
  m.modal.setAttribute('aria-describedby', 'uso-texto');
  m.el.addEventListener('click', async (e) => {
    const b = e.target.closest('[data-uso]');
    if (!b) return;
    m.close();
    try { await setUsageStats(b.dataset.uso === 'si'); } catch (_) { /* ya avisado; se vuelve a preguntar al abrir */ }
  });
  return true;
}
