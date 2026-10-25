// Sonda del 25-oct: lo nuevo desde 67ce120 en el hub. Novedades (página, tarjeta «Lo nuevo», botones Probar, localStorage),
// «Cómo se usa», Configuración → Limpieza e IA (cleanup.js dentro de Configuración) y los desplegables de Privacidad y
// datos. Backend: mock.js sin veneno detrás de un window.__TAURI__ falso (así IS_MOCK es false y el hub sigue el camino
// de la app de verdad: «Lo nuevo» sale sola si hay dictados) + registro de argumentos (window.__log).
import { createRequire } from 'node:module';
import { scriptBackendHostil, espera } from './comun.mjs';

const AXE = createRequire(import.meta.url).resolve('axe-core/axe.min.js');

export const nombre = 'novedades';

const REGISTRO = `
  window.__log = [];
  const inv0 = window.__TAURI__.core.invoke;
  window.__TAURI__.core.invoke = async (c, a) => {
    window.__log.push([c, a === undefined ? null : JSON.parse(JSON.stringify(a))]);
    const r = await inv0(c, a);
    if (c === 'get_stats' && window.__stats) return { ...r, ...window.__stats };
    return r;
  };
`;
const LIMPIO = scriptBackendHostil({ envenenar: false, extra: REGISTRO });
const saves = (page) => page.evaluate(() => window.__log.filter((x) => x[0] === 'save_settings').map((x) => x[1].settings));
const titulos = (page) => page.$$eval('.velo:not(.out) .modal-hd h2', (v) => v.map((x) => x.textContent.trim()));
const foco = (page) => page.evaluate(() => { const a = document.activeElement; return a ? `${a.tagName.toLowerCase()}${a.id ? '#' + a.id : ''}${a.dataset && a.dataset.select ? '[data-select=' + a.dataset.select + ']' : ''}${a.dataset && a.dataset.nav ? '[data-nav=' + a.dataset.nav + ']' : ''}` : 'null'; });
const VISTA = 'pronto.novedades.vista';

export async function correr({ abrir }) {
  const out = [];
  const caso = async (titulo, ruta, fn, opts = {}) => {
    const p = await abrir(ruta, { hostil: LIMPIO, ...opts });
    await p.page.waitForSelector('.sb .nav', { timeout: 8000 });
    await espera(500);
    try { await fn(p); } catch (e) { out.push({ grav: 'MEDIA', titulo: `${titulo}: la sonda falló`, evidencia: String(e.message || e).split('\n').slice(0, 6).join(' | ') }); }
    const g = p.guardas();
    if (g.errores.length) out.push({ grav: 'MEDIA', titulo: `${titulo}: error JS sin capturar`, evidencia: g.errores.join('\n'), cubreErrores: true });
    await p.cerrar();
  };

  // ── A · Privacidad y datos: una flecha en «Conservar el historial» / «Audio guardado» con el teclado
  for (const [sel, tecla] of [['historyRetentionDays', 'ArrowDown'], ['keepAudioDays', 'ArrowUp']]) {
    await caso(`teclado en ${sel}`, '/hub.html?settings=data', async ({ page }) => {
      await page.waitForSelector(`[data-select=${sel}]`);
      await page.focus(`[data-select=${sel}]`);
      const antes = await page.$eval(`[data-select=${sel}]`, (s) => s.value);
      await page.keyboard.press(tecla);
      await espera(500);
      const f1 = await foco(page);
      await page.keyboard.press(tecla);
      await espera(500);
      const f2 = await foco(page);
      const S = (await saves(page)).map((s) => s[sel]);
      out.push({ grav: S.length ? 'MEDIA' : 'OK', titulo: `${sel}: ${tecla} con el desplegable cerrado guarda (y purga) al momento`,
        evidencia: `valor antes=${antes} · save_settings(${sel})=${JSON.stringify(S)} · foco tras 1.ª flecha=${f1} · tras 2.ª=${f2}` });
    });
  }

  // ── B · «Lo nuevo» en el camino de la app (IS_MOCK=false)
  await caso('Lo nuevo: veterano, una vez', '/hub.html', async ({ page }) => {
    const t1 = await titulos(page);
    const v1 = await page.evaluate((k) => localStorage.getItem(k), VISTA);
    await page.reload(); await page.waitForSelector('.sb .nav'); await espera(500);
    const t2 = await titulos(page);
    out.push({ grav: t1.length === 1 && t2.length === 0 ? 'OK' : 'MEDIA', titulo: 'Lo nuevo sale una vez (veterano)', evidencia: `1.ª apertura=${JSON.stringify(t1)} vista=${v1} · 2.ª=${JSON.stringify(t2)}` });
  });
  await caso('Lo nuevo: solo reuniones', '/hub.html?page=meetings', async ({ page }) => {
    await page.evaluate((k) => { localStorage.removeItem(k); window.__stats = { dictations: 0, totalWords: 0 }; }, VISTA);
    // segundo arranque con el mismo almacenamiento y 0 dictados (quien solo graba reuniones)
    await page.addInitScript(() => { window.__stats = { dictations: 0, totalWords: 0 }; });
    await page.goto(page.url().replace(/\?.*$/, '')); await page.waitForSelector('.sb .nav'); await espera(600);
    const t = await titulos(page);
    const v = await page.evaluate((k) => localStorage.getItem(k), VISTA);
    const stats = await page.evaluate(() => window.__log.filter((x) => x[0] === 'get_stats').length);
    out.push({ grav: t.length === 0 && v === '2.3.1' ? 'BAJA' : 'OK', titulo: 'quien solo usa reuniones (0 dictados) nunca ve «Lo nuevo»: se marca vista sin enseñarla',
      evidencia: `get_stats llamadas=${stats} · modales=${JSON.stringify(t)} · ${VISTA}=${v} · (get_stats no trae «meetings»: state.stats.meetings es undefined)` });
  });

  const PROBAR = ['2.3.1#0', '2.3.0#0', '2.3.0#1', '2.3.0#2', '2.3.0#3', '2.3.0#4', '2.0.2#0', '2.0.2#1'];
  for (const k of PROBAR) {
    await caso(`Probar ${k}`, '/hub.html?page=novedades', async ({ page }) => {
      await page.$$eval('details.nv-old', (d) => d.forEach((x) => { x.open = true; }));
      await page.click(`[data-try="${k}"]`);
      await espera(1400);
      const res = await page.evaluate(() => ({
        nav: document.querySelector('.sb .nav.active')?.dataset.nav || null,
        modales: [...document.querySelectorAll('.velo:not(.out) .modal-hd h2')].map((x) => x.textContent.trim()),
        spot: !!document.querySelector('.nv-spot'),
        toast: [...document.querySelectorAll('.toast')].map((x) => x.textContent.trim()).join(' | ').slice(0, 60),
      }));
      out.push({ grav: 'OK', titulo: `Probar ${k}`, evidencia: JSON.stringify(res) });
    });
  }

  await caso('doble clic Probar 2.3.0#2', '/hub.html?page=novedades', async ({ page }) => {
    await page.$$eval('details.nv-old', (d) => d.forEach((x) => { x.open = true; }));
    await page.dblclick('[data-try="2.3.0#2"]');
    await espera(1800);
    const t = await titulos(page);
    out.push({ grav: t.length > 1 ? 'BAJA' : 'OK', titulo: 'doble clic en Probar (Mis palabras)', evidencia: `modales=${JSON.stringify(t)}` });
  });

  await caso('Probar encadenado', '/hub.html?page=novedades', async ({ page }) => {
    await page.$$eval('details.nv-old', (d) => d.forEach((x) => { x.open = true; }));
    await page.click('[data-try="2.3.0#2"]');
    await espera(100);
    await page.evaluate(() => document.dispatchEvent(new CustomEvent('pronto:navigate', { detail: 'home' })));
    await espera(1500);
    const res = await page.evaluate(() => ({ nav: document.querySelector('.sb .nav.active')?.dataset.nav, modales: [...document.querySelectorAll('.velo:not(.out) .modal-hd h2')].map((x) => x.textContent.trim()) }));
    out.push({ grav: res.modales.length && res.nav !== 'dictionary' ? 'BAJA' : 'OK', titulo: 'Probar (Mis palabras) y cambiar de página antes de 450 ms', evidencia: JSON.stringify(res) });
  });

  // ── C · Teclado: «Novedades» del menú con Enter mientras tiene el punto (el botón se repinta con outerHTML)
  await caso('Novedades con teclado', '/hub.html?page=meetings', async ({ page }) => {
    const punto = !!(await page.$('#nav-novedades .nv-dot'));
    await page.focus('#nav-novedades');
    await page.keyboard.press('Enter');
    await espera(600);
    const f = await foco(page);
    await page.focus('.sb [data-nav=dictionary]');
    await page.keyboard.press('Enter');
    await espera(600);
    const fOtro = await foco(page);
    out.push({ grav: punto && f === 'body' ? 'BAJA' : 'OK', titulo: 'Enter en «Novedades» (con punto) tira el foco a <body>',
      evidencia: `punto=${punto} foco tras Enter en Novedades=${f} · tras Enter en Mis palabras=${fOtro}` });
  });

  // ── D · localStorage que lanza (WebView con el almacenamiento roto)
  await caso('localStorage roto', '/hub.html?page=novedades', async ({ page }) => {
    await page.click('.sb [data-nav=home]'); await espera(500);
    const punto = !!(await page.$('#nav-novedades .nv-dot'));
    out.push({ grav: punto ? 'BAJA' : 'OK', titulo: 'localStorage roto: el punto «hay cosas nuevas» no se va nunca', evidencia: `punto tras visitar Novedades=${punto}` });
  }, { init: `Storage.prototype.getItem = function () { throw new Error('roto'); }; Storage.prototype.setItem = function () { throw new Error('roto'); };` });

  // ── E · «Cómo se usa» → Cambiar tecla / micrófono / idioma sin Configuración detrás
  for (const k of ['shortcuts', 'mic', 'langs']) {
    await caso(`Cómo se usa → ${k}`, '/hub.html?page=meetings', async ({ page }) => {
      await page.click('.sb [data-act=help]');
      await page.waitForSelector(`.velo:not(.out) [data-help=${k}]`);
      await page.click(`[data-help=${k}]`);
      await espera(600);
      const t = await titulos(page);
      await page.keyboard.press('Escape');
      await espera(500);
      const t2 = await titulos(page);
      out.push({ grav: t.length === 1 && !t2.length ? 'OK' : 'BAJA', titulo: `Cómo se usa → ${k}`, evidencia: `abierto=${JSON.stringify(t)} tras Esc=${JSON.stringify(t2)} foco=${await foco(page)}` });
    });
  }

  // ── F · Configuración → Limpieza e IA: el interruptor y el nivel no se guardan dos veces
  await caso('Limpieza e IA en Configuración', '/hub.html?settings=experimental', async ({ page }) => {
    await page.waitForSelector('#cleanup-host [data-switch=aiPolish]');
    await page.click('#cleanup-host [data-switch=aiPolish]');
    await espera(500);
    await page.click('#cleanup-host [data-level=high]');
    await espera(500);
    const S = await saves(page);
    out.push({ grav: S.length === 2 ? 'OK' : 'MEDIA', titulo: 'pulido + nivel desde Configuración: un guardado por cambio', evidencia: JSON.stringify(S.map((s) => ({ aiPolish: s.aiPolish, cleanupLevel: s.cleanupLevel }))) });
  });

  // ── G · axe (WCAG 2.2 AA, graves y críticas) en lo nuevo: «Cómo se usa», Novedades y «Lo nuevo», oscuro y claro
  for (const [n, ruta, prep] of [
    ['Cómo se usa', '/hub.html?page=meetings', async (page) => { await page.click('.sb [data-act=help]'); await page.waitForSelector('.velo:not(.out) [data-help]'); }],
    ['Cómo se usa (claro)', '/hub.html?page=meetings&theme=light', async (page) => { await page.click('.sb [data-act=help]'); await page.waitForSelector('.velo:not(.out) [data-help]'); }],
    ['Novedades', '/hub.html?page=novedades', async (page) => { await page.$$eval('details.nv-old', (d) => d.forEach((x) => { x.open = true; })); }],
    ['Novedades (claro)', '/hub.html?page=novedades&theme=light', async (page) => { await page.$$eval('details.nv-old', (d) => d.forEach((x) => { x.open = true; })); }],
    ['Lo nuevo (claro)', '/hub.html?theme=light', async (page) => { await page.waitForSelector('.nv-modal'); }],
    ['Configuración → Limpieza e IA (claro)', '/hub.html?settings=experimental&theme=light', async (page) => { await page.waitForSelector('#cleanup-host [data-switch=aiPolish]'); }],
  ]) {
    await caso(`axe ${n}`, ruta, async ({ page }) => {
      await prep(page); await espera(900);
      await page.addScriptTag({ path: AXE });
      const v = await page.evaluate(async () => (await window.axe.run(document, { runOnly: { type: 'tag', values: ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'] }, resultTypes: ['violations'] }))
        .violations.filter((x) => x.impact === 'serious' || x.impact === 'critical')
        .map((x) => ({ id: x.id, n: x.nodes.length, ej: x.nodes.slice(0, 2).map((d) => d.target.join(' ') + ' · ' + d.failureSummary.slice(0, 160)) })));
      out.push({ grav: v.length ? 'MEDIA' : 'OK', titulo: `axe ${n}: ${v.length} violaciones graves`, evidencia: JSON.stringify(v) });
    }, { colorScheme: ruta.includes('light') ? 'light' : 'dark' });
  }

  return out;
}
