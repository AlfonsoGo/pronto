// Sonda del 25-oct (c): `confirmDialog` con `danger` empieza con el foco en «Cancelar» (rompedor 25-oct-b, BAJA 1). ¿En
// TODOS los diálogos de borrar del hub? Para cada uno: dónde está el foco justo al abrirse (antes del primer frame, por un
// MutationObserver), dónde tras los frames, y si un Enter inmediato llama al comando de borrar.
import { scriptBackendHostil, espera } from './comun.mjs';

export const nombre = 'borrar-foco';

const REGISTRO = `
  window.__log = [];
  const inv0 = window.__TAURI__.core.invoke;
  window.__TAURI__.core.invoke = async (c, a) => { window.__log.push(c); return inv0(c, a); };
  window.__focoAlAbrir = [];
  new MutationObserver((ms) => {
    for (const m of ms) for (const n of m.addedNodes) if (n.classList && n.classList.contains('velo')) {
      const a = document.activeElement;
      window.__focoAlAbrir.push(a ? (a.hasAttribute('data-ok') ? 'Sí' : a.hasAttribute('data-close') ? 'Cancelar' : a.tagName) : 'null');
    }
  }).observe(document, { childList: true, subtree: true });
`;
const LIMPIO = scriptBackendHostil({ envenenar: false, extra: REGISTRO });
const focoAhora = (page) => page.evaluate(() => {
  const a = document.activeElement; if (!a) return 'null';
  return a.hasAttribute('data-ok') ? 'Sí' : a.hasAttribute('data-close') && !a.hasAttribute('data-x') ? 'Cancelar' : `${a.tagName.toLowerCase()} «${(a.textContent || '').trim().slice(0, 20)}»`;
});

export async function correr({ abrir }) {
  const out = [];
  const caso = async (titulo, ruta, abrirDialogo, cmd) => {
    const p = await abrir(ruta, { hostil: LIMPIO });
    const { page } = p;
    try {
      await page.waitForSelector('.sb .nav', { timeout: 8000 });
      await espera(500);
      await abrirDialogo(page);
      await page.waitForSelector('.velo:not(.out) [data-ok]', { timeout: 5000 });
      await page.keyboard.press('Enter'); // sin pausa
      await espera(60); const foco60 = await focoAhora(page);
      await espera(500);
      const alAbrir = await page.evaluate(() => window.__focoAlAbrir.slice(-1)[0]);
      const titulo2 = await page.$$eval('.velo .modal-hd h2', (v) => v.map((x) => x.textContent.trim()));
      const llamadas = await page.evaluate((c) => window.__log.filter((x) => x === c).length, cmd);
      const sigue = await page.$$eval('.velo:not(.out) [data-ok]', (v) => v.length);
      out.push({ grav: llamadas ? 'ALTA' : alAbrir !== 'Cancelar' ? 'BAJA' : 'OK', titulo: `${titulo}: foco al abrir y Enter inmediato`,
        evidencia: `diálogo=${JSON.stringify(titulo2)} · foco al abrir=${alAbrir} · ${cmd} llamado=${llamadas} · diálogo abierto tras Enter=${sigue} · foco a 60 ms=${foco60} · a 560 ms=${await focoAhora(page)}` });
    } catch (e) {
      out.push({ grav: 'OK', titulo: `${titulo}: no se pudo llegar al diálogo (hueco)`, evidencia: String(e.message || e).split('\n')[0] });
    }
    await p.cerrar();
  };

  await caso('Configuración → Borrar historial', '/hub.html?settings=data', async (page) => {
    await page.waitForSelector('[data-act=clear-history]'); await page.focus('[data-act=clear-history]'); await page.keyboard.press('Enter');
  }, 'clear_history');
  await caso('Inicio → ⋯ → Eliminar dictado', '/hub.html', async (page) => {
    await page.waitForSelector('.hrow [data-act=more]', { state: 'attached' }); await page.focus('.hrow [data-act=more]'); await page.keyboard.press('Enter');
    await page.waitForSelector('.menu .m-item.danger'); await page.click('.menu .m-item.danger');
  }, 'delete_history');
  await caso('Notas → Eliminar nota', '/hub.html?page=notes', async (page) => {
    await page.waitForSelector('[data-act=del]'); await page.click('[data-act=del]');
  }, 'delete_note');
  await caso('Reuniones → detalle → Eliminar reunión', '/hub.html?page=meetings', async (page) => {
    const l = await page.evaluate(() => window.__TAURI__.core.invoke('list_meetings', { query: null, limit: 1, offset: 0 }));
    await page.evaluate((id) => document.dispatchEvent(new CustomEvent('pronto:navigate', { detail: 'meetings' })), l[0].id);
    await page.waitForSelector('[data-id]', { state: 'attached' }); await page.click('.mrow[data-id], [data-id]');
    await page.waitForSelector('[data-act=del-detail]', { state: 'attached', timeout: 5000 }); await page.focus('[data-act=del-detail]'); await page.keyboard.press('Enter');
  }, 'delete_meeting');
  for (const [pg, cmd] of [['dictionary', 'delete_word'], ['snippets', 'delete_snippet']]) {
    await caso(`${pg} → eliminar fila`, `/hub.html?page=${pg}`, async (page) => {
      await page.waitForSelector('[data-act=delete]', { timeout: 3000 }); await page.click('[data-act=delete]');
    }, cmd);
  }
  await caso('Transforms → editar uno propio → Eliminar', '/hub.html?page=transforms', async (page) => {
    await page.evaluate(() => window.__TAURI__.core.invoke('save_transform', { transform: { id: 0, name: 'Rompedor', prompt: 'x', builtin: false, hotkeyIndex: null } }));
    await page.evaluate(() => document.dispatchEvent(new CustomEvent('pronto:navigate', { detail: 'home' })));
    await page.evaluate(() => document.dispatchEvent(new CustomEvent('pronto:navigate', { detail: 'transforms' })));
    await espera(500);
    const ids = await page.$$eval('[data-open]', (v) => v.map((x) => x.dataset.open + ':' + x.textContent.trim().slice(0, 30)));
    const el = await page.$$('[data-open]'); const i = ids.findIndex((x) => /Rompedor/.test(x)); if (i < 0) throw new Error('sin transform propio: ' + ids.join(' | ')); await el[i].click();
    await page.waitForSelector('[data-act=delete]', { timeout: 3000 }); await page.click('[data-act=delete]');
  }, 'delete_transform');
  // Cancelar antes del primer frame: el requestAnimationFrame de openModal (util.js) enfoca el diálogo ya cerrado.
  {
    const p = await abrir('/hub.html?settings=data', { hostil: LIMPIO });
    const { page } = p;
    await page.waitForSelector('[data-act=clear-history]'); await espera(400); await page.focus('[data-act=clear-history]');
    const r = await page.evaluate(async () => {
      document.querySelector('[data-act=clear-history]').click();
      for (let i = 0; i < 20 && !document.querySelector('.velo:not(.out) [data-ok]'); i++) await new Promise((ok) => setTimeout(ok, 0));
      const c = document.querySelector('.velo:not(.out) .modal-ft [data-close]'); if (!c) return 'sin diálogo';
      c.click();
      const justo = document.activeElement?.dataset?.act || document.activeElement?.tagName;
      await new Promise((ok) => setTimeout(ok, 500));
      const a = document.activeElement;
      return { justo, luego: a.dataset?.act || a.tagName.toLowerCase() };
    });
    out.push({ grav: r.luego === 'clear-history' ? 'OK' : 'BAJA', titulo: 'Configuración → Borrar historial → Cancelar antes del primer frame: ¿vuelve el foco al botón?',
      evidencia: "foco justo al cancelar=" + r.justo + " · a 500 ms=" + r.luego });
    await p.cerrar();
  }
  return out;
}
