// Sonda del 25-oct (b): la confirmación al acortar «Conservar el historial» / «Audio guardado» (settings.js
// confirmRetention / shortensRetention) y la tarjeta «Lo nuevo» acumulada (novedades.js pendingReleases) con valores raros
// guardados. Mismo backend que novedades.mjs: mock.js detrás de un __TAURI__ falso (IS_MOCK=false) + registro de llamadas.
import { scriptBackendHostil, espera } from './comun.mjs';

export const nombre = 'retencion';

const REGISTRO = `
  window.__log = [];
  const inv0 = window.__TAURI__.core.invoke;
  window.__TAURI__.core.invoke = async (c, a) => {
    window.__log.push([c, a === undefined ? null : JSON.parse(JSON.stringify(a))]);
    return inv0(c, a);
  };
`;
const LIMPIO = scriptBackendHostil({ envenenar: false, extra: REGISTRO });
const VISTA = 'pronto.novedades.vista';

const guardados = (page, k) => page.evaluate((k) => {
  // save_settings manda el objeto entero: solo cuentan los que cambian `k` respecto al anterior.
  const s = window.__log.filter((x) => x[0] === 'save_settings').map((x) => x[1].settings[k]);
  return s;
}, k);
const velos = (page) => page.$$eval('.velo:not(.out)', (v) => v.length);
const dialogos = (page) => page.$$eval('.velo:not(.out) .modal-hd h2', (v) => v.map((x) => x.textContent.trim()));
const foco = (page) => page.evaluate(() => {
  const a = document.activeElement; if (!a) return 'null';
  return `${a.tagName.toLowerCase()}${a.dataset?.select ? `[data-select=${a.dataset.select}]` : ''}${a.hasAttribute?.('data-ok') ? '[data-ok]' : ''}${a.hasAttribute?.('data-close') ? '[data-close]' : ''} «${(a.textContent || '').trim().slice(0, 20)}»`;
});
const valor = (page, k) => page.$eval(`[data-select=${k}]`, (s) => s.value).catch(() => 'sin select');

export async function correr({ abrir }) {
  const out = [];
  const caso = async (titulo, ruta, fn, opts = {}) => {
    const p = await abrir(ruta, { hostil: LIMPIO, ...opts });
    await p.page.waitForSelector('.sb .nav', { timeout: 8000 });
    await espera(400);
    try { await fn(p); } catch (e) { out.push({ grav: 'MEDIA', titulo: `${titulo}: la sonda falló`, evidencia: String(e.message || e).split('\n').slice(0, 6).join(' | ') }); }
    const g = p.guardas();
    if (g.errores.length) out.push({ grav: 'MEDIA', titulo: `${titulo}: error JS sin capturar`, evidencia: g.errores.join('\n'), cubreErrores: true });
    await p.cerrar();
  };
  const H = 'historyRetentionDays', A = 'keepAudioDays';
  const listo = async (page, k) => { await page.waitForSelector(`[data-select=${k}]`); await page.focus(`[data-select=${k}]`); };

  // 1 · Teclado: «↓ ↓ Enter» (quien recorre el desplegable cerrado y pulsa Enter para «aceptar»), sin pausas y con pausas
  for (const [n, pausa] of [['sin pausa', 0], ['con 300 ms', 300]]) {
    await caso(`↓ ↓ Enter ${n}`, '/hub.html?settings=data', async ({ page }) => {
      await listo(page, H);
      await page.keyboard.press('ArrowDown'); if (pausa) await espera(pausa);
      const f1 = await foco(page);
      await page.keyboard.press('ArrowDown'); if (pausa) await espera(pausa);
      await page.keyboard.press('Enter');
      await espera(600);
      const S = await guardados(page, H);
      out.push({ grav: S.length ? 'ALTA' : 'OK', titulo: `«Conservar el historial»: ↓ ↓ Enter (${n}) desde «Siempre»`,
        evidencia: `foco tras 1.ª ↓=${f1} · save_settings(${H})=${JSON.stringify(S)} · valor=${await valor(page, H)} · diálogos=${JSON.stringify(await dialogos(page))}` });
    });
  }

  // 1b · ¿Qué botón tiene el foco al abrirse la confirmación? (antes y después del primer frame)
  await caso('foco inicial de la confirmación', '/hub.html?settings=data', async ({ page }) => {
    await listo(page, H);
    const f = await page.evaluate(async () => {
      const s = document.querySelector('[data-select=historyRetentionDays]');
      s.value = '7'; s.dispatchEvent(new Event('change', { bubbles: true }));
      await new Promise((r) => setTimeout(r, 0));
      const a0 = document.activeElement; const sinc = a0 && (a0.hasAttribute('data-ok') ? 'Sí, borrar' : a0.textContent.trim().slice(0, 20));
      await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
      const a1 = document.activeElement; const tras = a1 && (a1.hasAttribute('data-ok') ? 'Sí, borrar' : a1.textContent.trim().slice(0, 20));
      return { sinc, tras };
    });
    out.push({ grav: f.sinc === 'Sí, borrar' ? 'BAJA' : 'OK', titulo: 'foco al abrir «¿Borrar los dictados…?»: antes del primer frame está en «Sí, borrar»',
      evidencia: `antes del frame=${f.sinc} · tras el frame=${f.tras}` });
  });

  // 2 · Ratón: elegir, cancelar, volver a elegir y confirmar
  await caso('ratón: cancelar y volver a elegir', '/hub.html?settings=data', async ({ page }) => {
    await page.waitForSelector(`[data-select=${H}]`);
    await page.selectOption(`[data-select=${H}]`, '30'); await espera(400);
    const d1 = await dialogos(page);
    await page.click('.velo:not(.out) .modal-ft [data-close]'); await espera(400);
    const v1 = await valor(page, H), f1 = await foco(page);
    await page.selectOption(`[data-select=${H}]`, '90'); await espera(400);
    const d2 = await dialogos(page);
    await page.click('.velo:not(.out) [data-ok]'); await espera(600);
    const S = await guardados(page, H);
    const v2 = await valor(page, H);
    const ok = d1.length === 1 && v1 === '0' && d2.length === 1 && v2 === '90' && S.at(-1) === 90 && !S.includes(30);
    out.push({ grav: ok ? 'OK' : 'MEDIA', titulo: 'ratón: 30 → Cancelar → 90 → Sí, borrar',
      evidencia: `diálogo 1=${JSON.stringify(d1)} · tras Cancelar valor=${v1} foco=${f1} · diálogo 2=${JSON.stringify(d2)} · save_settings=${JSON.stringify(S)} · valor final=${v2}` });
  });

  // 3 · Esc y clic en el velo de la confirmación: no guarda, Configuración sigue abierta
  for (const [n, accion] of [['Esc', async (page) => page.keyboard.press('Escape')], ['clic en el velo', async (page) => page.mouse.click(5, 5)]]) {
    await caso(`confirmación + ${n}`, '/hub.html?settings=data', async ({ page }) => {
      await page.waitForSelector(`[data-select=${A}]`);
      await page.selectOption(`[data-select=${A}]`, '0'); await espera(400);
      const d = await dialogos(page);
      await accion(page); await espera(500);
      const d2 = await dialogos(page); const nv = await velos(page);
      const S = await guardados(page, A);
      out.push({ grav: S.length ? 'ALTA' : nv !== 1 ? 'BAJA' : 'OK', titulo: `«Audio guardado» → «No guardar audio» y ${n}`,
        evidencia: `abierto=${JSON.stringify(d)} · tras ${n}=${JSON.stringify(d2)} velos abiertos=${nv} · save_settings(${A})=${JSON.stringify(S)} · valor=${await valor(page, A)} · foco=${await foco(page)}` });
    });
  }

  // 4 · Muchas flechas seguidas: un solo diálogo, nada guardado
  await caso('5 flechas seguidas', '/hub.html?settings=data', async ({ page }) => {
    await listo(page, A);
    for (let i = 0; i < 5; i++) await page.keyboard.press('ArrowUp');
    await espera(600);
    const d = await dialogos(page);
    const S = await guardados(page, A);
    out.push({ grav: S.length ? 'ALTA' : d.length > 2 ? 'BAJA' : 'OK', titulo: '«Audio guardado»: 5 × ↑ seguidas desde 14 días',
      evidencia: `diálogos=${JSON.stringify(d)} · save_settings(${A})=${JSON.stringify(S)} · valor=${await valor(page, A)}` });
  });

  // 5 · Confirmar 7 y seguir: más corto pregunta otra vez, más largo guarda sin preguntar
  await caso('confirmar y seguir', '/hub.html?settings=data', async ({ page }) => {
    await page.waitForSelector(`[data-select=${H}]`);
    await page.selectOption(`[data-select=${H}]`, '30'); await espera(300);
    await page.click('.velo:not(.out) [data-ok]'); await espera(500);
    await page.selectOption(`[data-select=${H}]`, '7'); await espera(300);
    const d = await dialogos(page);
    await page.click('.velo:not(.out) .modal-ft [data-close]'); await espera(400);
    await page.selectOption(`[data-select=${H}]`, '0'); await espera(400);
    const d2 = await dialogos(page);
    const S = await guardados(page, H);
    out.push({ grav: JSON.stringify(S) === '[30,0]' && d.length === 1 && d2.length === 0 ? 'OK' : 'MEDIA', titulo: '30 (sí) → 7 (cancelar) → Siempre',
      evidencia: `diálogo al ir a 7=${JSON.stringify(d)} · al ir a Siempre=${JSON.stringify(d2)} · save_settings=${JSON.stringify(S)}` });
  });

  // 6 · Otro sitio cambia los ajustes con la confirmación abierta (settings://changed → renderTab)
  await caso('ajustes cambiados con la confirmación abierta', '/hub.html?settings=data', async ({ page }) => {
    await page.waitForSelector(`[data-select=${H}]`);
    await page.selectOption(`[data-select=${H}]`, '7'); await espera(300);
    await page.evaluate(async () => { const m = await window.__hostil.m(); const s = await m.invoke('get_settings'); await m.invoke('save_settings', { settings: { ...s, theme: s.theme === 'light' ? 'dark' : 'light' } }); });
    await espera(400);
    const d = await dialogos(page);
    await page.click('.velo:not(.out) .modal-ft [data-close]').catch(() => {}); await espera(400);
    out.push({ grav: (await foco(page)).startsWith('body') ? 'BAJA' : 'OK', titulo: 'cancelar tras un cambio de ajustes externo: ¿dónde queda el foco?',
      evidencia: `diálogos=${JSON.stringify(d)} · foco tras Cancelar=${await foco(page)} · valor=${await valor(page, H)}` });
  });

  // 7 · Alt+↓ en el desplegable (atajo estándar de Windows para abrirlo) y escribir «3» (type-ahead)
  await caso('Alt+↓ en el desplegable', '/hub.html?settings=data', async ({ page }) => {
    await listo(page, H);
    const tab0 = await page.$eval('.set-tab[aria-selected=true], .set-tab.active', (b) => b.dataset.stab).catch(() => '?');
    await page.keyboard.press('Alt+ArrowDown'); await espera(400);
    const tab1 = await page.$eval('.set-tab[aria-selected=true], .set-tab.active', (b) => b.dataset.stab).catch(() => '?');
    out.push({ grav: tab0 !== tab1 ? 'BAJA' : 'OK', titulo: 'Alt+↓ con el foco en «Conservar el historial» cambia de pestaña en vez de abrir la lista',
      evidencia: `pestaña antes=${tab0} · después=${tab1} · foco=${await foco(page)}` });
  });
  await caso('type-ahead «3»', '/hub.html?settings=data', async ({ page }) => {
    await listo(page, H);
    await page.keyboard.press('3'); await espera(400);
    const d = await dialogos(page);
    out.push({ grav: 'OK', titulo: 'escribir «3» con el foco en «Conservar el historial»', evidencia: `diálogos=${JSON.stringify(d)} · save=${JSON.stringify(await guardados(page, H))}` });
  });

  // 8 · Textos de la confirmación con 1 día y en inglés
  await caso('confirmación «1 día»', '/hub.html?settings=data', async ({ page }) => {
    await page.waitForSelector(`[data-select=${A}]`);
    await page.selectOption(`[data-select=${A}]`, '1'); await espera(400);
    const d = await dialogos(page);
    const opcion = await page.$eval(`[data-select=${A}] option[value="1"]`, (o) => o.textContent.trim());
    out.push({ grav: d.some((x) => /1 días|1 days/.test(x)) ? 'BAJA' : 'OK', titulo: 'el título de la confirmación con 1 día', evidencia: `opción=«${opcion}» · diálogo=${JSON.stringify(d)}` });
  });
  await caso('confirmación «1 día» en inglés', '/hub.html?settings=data&lang=en', async ({ page }) => {
    await page.waitForSelector(`[data-select=${A}]`); await espera(400);
    await page.selectOption(`[data-select=${A}]`, '1'); await espera(400);
    const d = await dialogos(page);
    out.push({ grav: d.some((x) => /1 días|1 days/.test(x)) ? 'BAJA' : 'OK', titulo: 'el título de la confirmación con 1 día (inglés)', evidencia: `diálogo=${JSON.stringify(d)}` });
  });

  // 9 · Valor guardado que no está en la lista (p. ej. 3 días, de una versión anterior o editado a mano)
  await caso('valor fuera de la lista', '/hub.html?settings=data', async ({ page }) => {
    await page.evaluate(async () => { const m = await window.__hostil.m(); const s = await m.invoke('get_settings'); await m.invoke('save_settings', { settings: { ...s, keepAudioDays: 3, historyRetentionDays: 45 } }); });
    await espera(600);
    const vis = await page.$$eval(`[data-select=${A}], [data-select=${H}]`, (v) => v.map((s) => `${s.dataset.select}: value=«${s.value}» se ve «${s.selectedOptions[0]?.textContent.trim() ?? ''}»`));
    out.push({ grav: 'OK', titulo: 'keepAudioDays=3 / historyRetentionDays=45 guardados: qué enseña el desplegable', evidencia: vis.join(' · ') });
  });

  // 10 · «Lo nuevo» acumulado con valores raros en pronto.novedades.vista
  for (const raro of [null, '', '2.3', '2.3.0', '2.2.9', '9.9.9', '2.3.1 ', ' 2.3.0', '2.3.0-beta', 'null', '<img src=x onerror=__pwnHit(1)>', '2.3.99999', '{"v":"2.3.0"}']) {
    await caso(`Lo nuevo con vista=${JSON.stringify(raro)}`, '/hub.html', async ({ page, pwn }) => {
      await espera(500);
      const r = await page.evaluate(() => ({
        modal: [...document.querySelectorAll('.velo:not(.out) .modal-hd h2')].map((x) => x.textContent.trim()),
        versiones: [...document.querySelectorAll('.velo:not(.out) .nv-modal-v')].map((x) => x.textContent.trim()),
        tarjetas: document.querySelectorAll('.velo:not(.out) .nv-modal .nv-grid > li').length,
      }));
      const v = await page.evaluate((k) => localStorage.getItem(k), VISTA);
      const p = await pwn();
      out.push({ grav: p.hits.length ? 'ALTA' : r.modal.length !== 1 && raro !== '2.3.1 ' ? 'BAJA' : 'OK', titulo: `«Lo nuevo» con ${VISTA}=${JSON.stringify(raro)}`,
        evidencia: `modal=${JSON.stringify(r.modal)} · secciones=${JSON.stringify(r.versiones)} · tarjetas=${r.tarjetas} · guardado después=${JSON.stringify(v)} · pwn=${JSON.stringify(p.hits)}` });
    }, { init: raro === null ? `localStorage.removeItem(${JSON.stringify(VISTA)})` : `if (!sessionStorage.getItem('sembrado')) { localStorage.setItem(${JSON.stringify(VISTA)}, ${JSON.stringify(raro)}); sessionStorage.setItem('sembrado', '1'); }` });
  }

  return out;
}
