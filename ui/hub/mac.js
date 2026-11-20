// Pronto 2 · Hub en Mac: los botones de la ventana son los del propio macOS (semáforo rojo, ámbar y
// verde, `titleBarStyle: Overlay` en tauri.macos.conf.json), así que se esconden los de Windows y se deja
// sitio a la izquierda. El botón rojo pasa por el mismo cierre que la × de Windows: espera a que se
// guarde lo pendiente (regla 2) antes de cerrar.
import { IS_MAC } from './util.js';

export function initMac({ win, closeAction }) {
  if (!IS_MAC) return;
  document.documentElement.dataset.os = 'macos';
  let closing = false;
  try {
    const w = win();
    if (w && typeof w.onCloseRequested === 'function') {
      w.onCloseRequested(async (ev) => {
        if (closing) return; // el cierre que pide `closeAction` cuando ya se guardó: que siga
        ev.preventDefault();
        closing = true;
        try { await closeAction(); } finally { closing = false; }
      });
    }
  } catch (_) { /* maqueta sin ventana: nada que hacer */ }
}
