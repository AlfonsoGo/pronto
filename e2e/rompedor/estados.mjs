// Sonda de estados raros, accesibilidad y volumen (segunda pasada 23-oct):
//  · axe (WCAG 2.2 A/AA, solo graves y críticas) en lo que el test de la regla 6 no abre: modales y editores,
//    menús, base vacía, sin modelo, motor con error, sin Ollama, actualización disponible, inglés;
//  · teclado: adónde va el foco tras cerrar un modal, borrar una nota o usar el menú «⋯»;
//  · 100 000 dictados (backend simulado que pagina como el real) y «Cargar más» con prisa.
import { createRequire } from 'node:module';
import { scriptBackendHostil, espera, captura } from './comun.mjs';

export const nombre = 'estados';
const require = createRequire(import.meta.url);
const AXE = require.resolve('axe-core/axe.min.js');
const ETIQUETAS = ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'];
const LIMPIO = scriptBackendHostil({ envenenar: false });

// 100 000 dictados: list_history pagina sobre filas sintéticas (clamp 1..1000 como db.rs), estadísticas enormes.
const CIEN_MIL = scriptBackendHostil({ envenenar: false, extra: `
  const N = 100000, base = Date.now();
  const fila = (i) => ({ id: N - i, createdAt: new Date(base - i * 60000).toISOString(), text: 'Dictado número ' + (N - i) + ' con algo de texto para que ocupe dos líneas en la tarjeta del historial y se vea cómo se pinta.',
    rawText: 'x', app: ['Chrome', 'Slack', 'Code'][i % 3], durationMs: 4000, words: 24, hasAudio: i < 2000 });
  const inv0 = window.__TAURI__.core.invoke;
  window.__TAURI__.core.invoke = async (c, a) => {
    if (c === 'list_history') {
      window.__calls.push(c);
      if (window.__lento[c]) await new Promise((r) => setTimeout(r, window.__lento[c]));
      const lim = Math.min(Math.max(a.limit, 1), 1000), off = Math.max(a.offset, 0);
      const q = (a.query || '').toLowerCase();
      const out = [];
      for (let i = off, k = 0; i < N && out.length < lim; i++) { const f = fila(i); if (!q || f.text.toLowerCase().includes(q)) { if (k++ >= 0) out.push(f); } }
      return out;
    }
    if (c === 'get_stats') { const r = await inv0(c, a); return { ...r, totalWords: 2400000, dictations: N, wordsThisWeek: 98765 }; }
    if (c === 'get_insights') { const r = await inv0(c, a); return { ...r, totalWords: 2400000, dictations: N, longestStreak: 1234, streakDays: 999, fixes: { dictionary: 1234567, snippets: 987654, fillers: 12345678 },
      topApps: r.topApps.map((x) => ({ ...x, words: x.words * 1000, count: x.count * 1000, app: x.app + ' con un nombre de aplicación muy largo de verdad.exe' })) }; }
    return inv0(c, a);
  };` });

async function axe(page) {
  await page.addScriptTag({ path: AXE });
  return page.evaluate(async (tags) => (await window.axe.run(document, { runOnly: { type: 'tag', values: tags }, resultTypes: ['violations'] })).violations
    .filter((x) => x.impact === 'serious' || x.impact === 'critical')
    .map((x) => `${x.impact} ${x.id}: ${x.nodes.slice(0, 2).map((n) => n.target.join(' ')).join(' | ')}${x.nodes.length > 2 ? ` (+${x.nodes.length - 2})` : ''}`), ETIQUETAS);
}
const foco = (page) => page.evaluate(() => { const a = document.activeElement; if (!a || a === document.body) return 'body'; return `${a.tagName.toLowerCase()}${a.dataset.act ? `[data-act=${a.dataset.act}]` : ''}${a.id ? '#' + a.id : ''} «${(a.getAttribute('aria-label') || a.textContent || '').trim().slice(0, 28)}»`; });

export async function correr({ abrir }) {
  const out = [];
  const caso = async (titulo, ruta, fn, opts = {}) => {
    const p = await abrir(ruta, { hostil: LIMPIO, ...opts });
    await p.page.waitForSelector('.sb .nav', { timeout: 8000 }).catch(() => {});
    await espera(700);
    try { await fn(p); } catch (e) { out.push({ grav: 'MEDIA', titulo: `${titulo}: la sonda falló`, evidencia: String(e.message || e).split('\n')[0] }); }
    const g = p.guardas();
    if (g.errores.length) out.push({ grav: 'MEDIA', titulo: `${titulo}: error JS sin capturar`, evidencia: g.errores.join('\n'), cubreErrores: true });
    await p.cerrar();
  };

  // ───────────────────────────── axe en estados y modales
  const fallosAxe = [];
  const pasa = async (etq, page) => { for (const v of await axe(page)) fallosAxe.push(`${etq} → ${v}`); };
  const ESTADOS = [
    ...['home', 'insights', 'dictionary', 'snippets', 'style', 'notes', 'transforms', 'meetings'].map((p) => `/hub.html?empty=1&page=${p}`),
    '/hub.html?model=missing', '/hub.html?model=error', '/hub.html?engine=error', '/hub.html?update=available', '/hub.html?update=error',
    '/hub.html?ollama=none&page=transforms', '/hub.html?ollama=none&page=meetings', '/hub.html?ollama=nomodel&page=transforms',
    '/hub.html?lang=en&page=home', '/hub.html?lang=en&page=meetings',
  ];
  const SIN_AXE = !!process.env.ROMPEDOR_SIN_AXE;
  if (SIN_AXE) { ESTADOS.length = 0; }
  for (const tema of ['dark', 'light']) {
    for (const r of ESTADOS) await caso(`axe ${r}`, `${r}&theme=${tema}`, async ({ page }) => pasa(`[${tema}] ${r}`, page));
  }
  // modales y vistas que no son una página
  const MODALES = [
    ['editor de palabra', '/hub.html?page=dictionary', async (page) => { await page.click('#page .ph [data-act=add]'); await page.waitForSelector('#w-word'); await page.fill('#w-word', 'x'); }],
    ['editar palabra con muestras', '/hub.html?page=dictionary', async (page) => { await page.click('#page [data-act=edit]'); await page.waitForSelector('#w-word'); }],
    ['editor de fragmento', '/hub.html?page=snippets', async (page) => { await page.click('#page .ph [data-act=add]'); await page.waitForSelector('#s-exp'); }],
    ['¿descartar los cambios?', '/hub.html?page=snippets', async (page) => { await page.click('#page .ph [data-act=add]'); await page.waitForSelector('#s-exp'); await page.fill('#s-exp', 'x'); await page.keyboard.press('Escape'); await espera(300); }],
    ['confirmar borrado', '/hub.html?page=notes', async (page) => { await page.click('#page .nt-card [data-act=del]'); await page.waitForSelector('[data-ok]'); }],
    ['editor de nota', '/hub.html?page=notes', async (page) => { await page.click('#page [data-act=new]'); await page.waitForSelector('#nt-text'); }],
    ['editor de Transform', '/hub.html?page=transforms', async (page) => { await page.click('#page [data-open=new]'); await page.waitForSelector('#t-prompt'); }],
    ['detalle de reunión', '/hub.html?page=meetings', async (page) => { await page.click('#page .mrow'); await page.waitForSelector('#md-title'); }],
    ['detalle de reunión · transcripción', '/hub.html?page=meetings', async (page) => { await page.click('#page .mrow'); await page.waitForSelector('#md-title'); const t = await page.$$('#md-tabs [role=tab]'); if (t[1]) await t[1].click(); await espera(300); }],
    ['menú ⋯ del historial', '/hub.html', async (page) => { await page.hover('#page .hrow'); await page.click('#page .hrow [data-act=more]'); await page.waitForSelector('.menu'); }],
    ['atajos', '/hub.html?settings=general', async (page) => { await page.click('[data-act=shortcuts]'); await espera(400); }],
    ['micrófono', '/hub.html?settings=general', async (page) => { await page.click('[data-act=mic]'); await espera(400); }],
    ['idiomas', '/hub.html?settings=general', async (page) => { await page.click('[data-act=langs]'); await espera(400); }],
    ['aviso previo de IA local', '/hub.html?page=transforms&ollama=none', async (page) => { await page.click('#page [data-act=ai-start]'); await page.waitForSelector('.velo .modal-ft .btn-primary'); }],
    ['IA activándose', '/hub.html?page=transforms&ollama=none', async (page) => { await page.click('#page [data-act=ai-start]'); await page.click('.velo .modal-ft .btn-primary'); await espera(1500); }],
    ['buscador del historial abierto', '/hub.html', async (page) => { await page.click('#page [data-act=toggle-search]'); await page.fill('#page [data-search-input]', 'zzzz'); await espera(600); }],
  ];
  if (SIN_AXE) MODALES.length = 0;
  MODALES.push(['página Diccionario sin modal (control)', '/hub.html?page=dictionary', async () => {}]);
  for (const [etq, ruta, prep] of MODALES) {
    for (const tema of ['dark', 'light']) await caso(`axe ${etq}`, `${ruta}${ruta.includes('?') ? '&' : '?'}theme=${tema}`, async ({ page }) => { await prep(page); await espera(350); await pasa(`[${tema}] ${etq}`, page); });
  }
  const unicos = [...new Set(fallosAxe)];
  out.push({ grav: unicos.length ? 'MEDIA' : 'OK', titulo: `axe (WCAG 2.2 AA) en ${ESTADOS.length} estados + ${MODALES.length} modales/vistas × 2 temas: ${unicos.length} violaciones graves/críticas`, evidencia: unicos.slice(0, 40).join('\n') });

  // ───────────────────────────── teclado y foco
  await caso('foco: borrar nota con teclado', '/hub.html?page=notes', async ({ page }) => {
    await page.focus('#page .nt-card [data-act=del]');
    await page.keyboard.press('Enter'); await page.waitForSelector('[data-ok]');
    await page.keyboard.press('Enter'); await espera(600);
    const f = await foco(page);
    await page.keyboard.press('Tab'); const f2 = await foco(page);
    out.push({ grav: f === 'body' ? 'BAJA' : 'OK', titulo: f === 'body' ? 'Borrar una nota (o palabra, fragmento, dictado) con el teclado deja el foco en <body>: el siguiente Tab vuelve al principio de la página' : `foco tras borrar nota: ${f}`, evidencia: `tras confirmar: ${f}; tras Tab: ${f2}` });
  });
  await caso('foco: borrar dictado con teclado', '/hub.html', async ({ page }) => {
    await page.focus('#page .hrow .h-text'); await page.keyboard.press('Tab'); await page.keyboard.press('Tab'); await page.keyboard.press('Tab');
    const enMas = await foco(page);
    await page.focus('#page .hrow [data-act=more]'); await page.keyboard.press('Enter'); await espera(300);
    const menu = await page.$('.menu');
    const fMenu = await foco(page);
    await page.keyboard.press('ArrowDown'); const f1 = await foco(page);
    const f2 = f1;
    await page.keyboard.press('Enter'); await espera(400);
    const conf = await page.$('[data-ok]');
    // La confirmación abre con «Cancelar» enfocado: Enter cancela y el foco debe volver al «⋯»; luego se repite y se confirma con Tab + Enter.
    let trasCancelar = null, filas0 = await page.$$eval('#page .hrow', (r) => r.length);
    if (conf) { await page.keyboard.press('Enter'); await espera(500); trasCancelar = await foco(page); }
    await page.keyboard.press('Enter'); await espera(300); await page.keyboard.press('ArrowDown'); await page.keyboard.press('Enter'); await espera(400);
    if (await page.$('[data-ok]')) { await page.keyboard.press('Tab'); await page.keyboard.press('Enter'); await espera(700); }
    const filas1 = await page.$$eval('#page .hrow', (r) => r.length);
    const fin = await foco(page);
    const mal = fin === 'body' || trasCancelar === 'body';
    out.push({ grav: !menu || !conf ? 'MEDIA' : mal ? 'BAJA' : 'OK', titulo: mal && menu && conf ? 'Tras «Eliminar transcripción» (o cancelarla) desde el menú ⋯ con el teclado, el foco cae al <body>: el siguiente Tab vuelve al principio de la página' : `menú ⋯ del historial con teclado: menú ${!!menu}, confirmación ${!!conf}; foco tras cancelar ${trasCancelar}; tras borrar ${fin} (filas ${filas0} → ${filas1})`, evidencia: `Tab desde el texto llega a ${enMas}; foco al abrir el menú ${fMenu}; ↓ ${f1}; ↓ ${f2}; tras cancelar ${trasCancelar}; tras borrar ${fin}` });
  });
  await caso('foco: Esc en editor vuelve al botón', '/hub.html?page=snippets', async ({ page }) => {
    await page.focus('#page .ph [data-act=add]'); await page.keyboard.press('Enter'); await page.waitForSelector('#s-exp');
    await page.keyboard.press('Escape'); await espera(400);
    const f = await foco(page);
    out.push({ grav: /data-act=add/.test(f) ? 'OK' : 'BAJA', titulo: `Esc en el editor de fragmento devuelve el foco a ${f}`, evidencia: '' });
  });

  // ───────────────────────────── 100 000 dictados
  {
    const p = await abrir('/hub.html', { hostil: CIEN_MIL });
    const t0 = Date.now();
    await p.page.waitForSelector('#page .hrow', { timeout: 15000 }).catch(() => {});
    const primera = Date.now() - t0;
    const tiempos = [];
    for (let i = 0; i < 40; i++) {
      const b = await p.page.$('#page [data-act=load-more]'); if (!b) break;
      const antes = await p.page.$$eval('#page .hrow', (r) => r.length);
      const a = Date.now(); await b.click();
      await p.page.waitForFunction((k) => document.querySelectorAll('#page .hrow').length > k, antes, { timeout: 15000 }).catch(() => {});
      tiempos.push(Date.now() - a);
    }
    const filas = await p.page.$$eval('#page .hrow', (r) => r.length);
    const ids = await p.page.$$eval('#page .hrow', (r) => r.map((x) => x.dataset.id));
    const dup = ids.length - new Set(ids).size;
    tiempos.sort((x, y) => x - y);
    const p95 = tiempos[Math.floor(tiempos.length * 0.95)] ?? 0, mx = tiempos[tiempos.length - 1] ?? 0;
    // tecleo en el buscador con 2 050 filas pintadas
    await p.page.click('#page [data-act=toggle-search]').catch(() => {});
    const a = Date.now(); await p.page.type('#page [data-search-input]', 'número 9999', { delay: 30 }); await espera(800);
    const tecleo = Date.now() - a;
    const res = await p.page.$$eval('#page .hrow', (r) => r.length);
    out.push({ grav: mx > 2000 || dup ? 'MEDIA' : 'OK', titulo: `100 000 dictados: primera página ${primera} ms; «Cargar más» ×${tiempos.length} → ${filas} filas, p95 ${p95} ms, máx ${mx} ms; duplicados ${dup}`, evidencia: `buscar «número 9999» tecleando: ${tecleo} ms → ${res} filas` });
    // estadísticas enormes: ¿desbordan?
    await p.page.click('.sb [data-nav=insights]'); await espera(1200);
    const desb = await p.page.evaluate(() => [...document.querySelectorAll('#page *')].filter((e) => e.children.length === 0 && e.textContent.trim() && e.scrollWidth > e.clientWidth + 2 && getComputedStyle(e).overflow !== 'hidden' && getComputedStyle(e).textOverflow !== 'ellipsis').map((e) => `${e.tagName.toLowerCase()}.${e.className} «${e.textContent.trim().slice(0, 30)}» ${e.scrollWidth}>${e.clientWidth}`).slice(0, 6));
    const fuera = await p.page.evaluate(() => { const pg = document.querySelector('#page').getBoundingClientRect(); return [...document.querySelectorAll('#page b, #page .big, #page .num')].filter((e) => { const r = e.getBoundingClientRect(); return r.right > pg.right + 1; }).map((e) => e.textContent.trim()).slice(0, 5); });
    out.push({ grav: desb.length || fuera.length ? 'BAJA' : 'OK', titulo: `Estadísticas con 100 000 dictados / 12 345 678 correcciones: ${desb.length + fuera.length ? 'textos que desbordan su caja' : 'sin desbordes'}`, evidencia: `${JSON.stringify(desb)} ${JSON.stringify(fuera)} ${desb.length || fuera.length ? await captura(p.page, 'estadisticas-100k') : ''}` });
    await p.cerrar();
  }

  // «Cargar más» con prisa: doble clic con la base lenta, y buscar mientras «Cargar más» está en vuelo
  await caso('Cargar más: doble clic', '/hub.html', async ({ page }) => {
    await page.evaluate(() => { window.__lento.list_history = 700; });
    const b = await page.waitForSelector('#page [data-act=load-more]', { timeout: 5000 }).catch(() => null);
    if (!b) { out.push({ grav: 'OK', titulo: 'Cargar más: el mock tiene < 50 dictados (se mira con 100 000 abajo)', evidencia: '' }); return; }
  }, { hostil: CIEN_MIL });
  await caso('Cargar más: doble clic (100 000)', '/hub.html', async ({ page }) => {
    await page.waitForSelector('#page [data-act=load-more]', { timeout: 8000 });
    await page.evaluate(() => { window.__lento.list_history = 700; });
    await page.click('#page [data-act=load-more]', { clickCount: 2 });
    await espera(2000);
    const ids = await page.$$eval('#page .hrow', (r) => r.map((x) => x.dataset.id));
    const dup = ids.length - new Set(ids).size;
    out.push({ grav: dup ? 'MEDIA' : 'OK', titulo: dup ? `Doble clic en «Cargar más» con la base lenta pinta ${dup} dictados repetidos (y el historial pide la misma página dos veces)` : 'Cargar más doble clic: sin repetidos', evidencia: `filas ${ids.length}, distintas ${new Set(ids).size}; list_history ${await page.evaluate(() => window.__calls.filter((c) => c === 'list_history').length)}× ${dup ? await captura(page, 'cargar-mas-duplicados') : ''}` });
  }, { hostil: CIEN_MIL });
  await caso('Cargar más + buscar', '/hub.html', async ({ page }) => {
    await page.waitForSelector('#page [data-act=load-more]', { timeout: 8000 });
    await page.evaluate(() => { window.__lento.list_history = 1200; });
    await page.click('#page [data-act=load-more]');
    await espera(100);
    await page.evaluate(() => { window.__lento.list_history = 100; });
    await page.click('#page [data-act=toggle-search]');
    await page.type('#page [data-search-input]', 'número 99999', { delay: 20 });
    await espera(2500);
    const filas = await page.$$eval('#page .hrow .h-text', (r) => r.map((x) => x.textContent));
    const ajenas = filas.filter((x) => !/número 99999/.test(x));
    out.push({ grav: ajenas.length ? 'MEDIA' : 'OK', titulo: ajenas.length ? `Buscar mientras «Cargar más» está en vuelo mezcla en los resultados ${ajenas.length} dictados que no coinciden` : 'Cargar más + buscar: resultados limpios', evidencia: `resultados ${filas.length}, que no contienen la búsqueda ${ajenas.length}: «${(ajenas[0] || '').slice(0, 50)}» ${ajenas.length ? await captura(page, 'buscar-mezcla') : ''}` });
  }, { hostil: CIEN_MIL });

  return out;
}
