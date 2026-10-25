// Regla 6 · Accesibilidad: el hub cumple WCAG 2.2 AA (CLAUDE.md § NO SE NEGOCIA).
// Sirve ui/ con el backend simulado (scripts/servir-ui.mjs), abre cada página y cada pestaña de
// Configuración en Chrome headless, en tema oscuro y claro, y pasa axe-core con las reglas WCAG
// 2.0/2.1/2.2 A y AA: 0 violaciones graves o críticas (páginas, diálogos y editores, a tres tamaños). Además, con el teclado: todas las secciones
// se alcanzan con Tab y cada parada de Tab tiene foco visible.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { abrirHub, cargar, rutaChrome, PAGINAS, AJUSTES } from './_comun.mjs';

const require = createRequire(import.meta.url);
const AXE = require.resolve('axe-core/axe.min.js');
const ETIQUETAS = ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'];
const saltar = !rutaChrome() && 'no hay Chrome (define PRIMERIZO_CHROME)';

// rompedor 23-oct, hub MEDIA 6: el test solo miraba las páginas a 1180×780 y se le escapaban el contraste del botón de
// los diálogos de confirmación y el nombre accesible del botón de voz del Diccionario (solo contado a partir de 1280 px).
// Ahora: tres tamaños (1180×780, 1280×800 y 900×600, el mínimo de la ventana en tauri.conf.json) y, además de las páginas,
// los diálogos y editores que abre el usuario (confirmaciones, descartar cambios, editores, menú «⋯», detalle de reunión).
const TAMANOS = [{ width: 1180, height: 780 }, { width: 1280, height: 800 }, { width: 900, height: 600 }];
const pasarAxe = (page) => page.evaluate(async (tags) => (await window.axe.run(document, { runOnly: { type: 'tag', values: tags }, resultTypes: ['violations'] })).violations
  .filter((x) => x.impact === 'serious' || x.impact === 'critical')
  .map((x) => `${x.impact} ${x.id}: ${x.nodes.slice(0, 3).map((n) => n.target.join(' ')).join(' | ')}${x.nodes.length > 3 ? ` (+${x.nodes.length - 3})` : ''}`), ETIQUETAS);
const espera = (ms) => new Promise((r) => setTimeout(r, ms));
// Diálogos y editores: [nombre, ruta, preparación que los abre]. Selectores de la maqueta (ui/shared/mock.js).
const ESTADOS = [
  ['editor de palabra', 'hub.html?page=dictionary', async (p) => { await p.click('#page .ph [data-act=add]'); await p.waitForSelector('#w-word'); await p.fill('#w-word', 'x'); }],
  ['editar palabra con muestras de voz', 'hub.html?page=dictionary', async (p) => { await p.click('#page [data-act=edit]'); await p.waitForSelector('#w-word'); }],
  ['editor de fragmento', 'hub.html?page=snippets', async (p) => { await p.click('#page .ph [data-act=add]'); await p.waitForSelector('#s-exp'); }],
  ['¿descartar los cambios? (confirmación)', 'hub.html?page=snippets', async (p) => { await p.click('#page .ph [data-act=add]'); await p.waitForSelector('#s-exp'); await p.fill('#s-exp', 'x'); await p.keyboard.press('Escape'); await p.waitForSelector('[data-ok]'); }],
  ['confirmar borrado de nota', 'hub.html?page=notes', async (p) => { await p.click('#page .nt-card [data-act=del]'); await p.waitForSelector('[data-ok]'); }],
  ['editor de nota', 'hub.html?page=notes', async (p) => { await p.click('#page [data-act=new]'); await p.waitForSelector('#nt-text'); }],
  ['editor de Transform', 'hub.html?page=transforms', async (p) => { await p.click('#page [data-open=new]'); await p.waitForSelector('#t-prompt'); }],
  ['detalle de reunión', 'hub.html?page=meetings', async (p) => { await p.click('#page .mrow'); await p.waitForSelector('#md-title'); }],
  ['menú ⋯ del historial', 'hub.html', async (p) => { await p.hover('#page .hrow'); await p.click('#page .hrow [data-act=more]'); await p.waitForSelector('.menu'); }],
  ['confirmar borrado de dictado', 'hub.html', async (p) => { await p.hover('#page .hrow'); await p.click('#page .hrow [data-act=more]'); await p.waitForSelector('.menu'); await p.keyboard.press('ArrowDown'); await p.keyboard.press('Enter'); await p.waitForSelector('[data-ok]'); }],
  ['atajos', 'hub.html?settings=general', async (p) => { await p.click('[data-act=shortcuts]'); await espera(400); }],
  ['micrófono', 'hub.html?settings=general', async (p) => { await p.click('[data-act=mic]'); await espera(400); }],
  ['idiomas', 'hub.html?settings=general', async (p) => { await p.click('[data-act=langs]'); await espera(400); }],
  ['lo nuevo tras actualizar', 'hub.html?novedades', async (p) => { await p.waitForSelector('.nv-modal'); }],
];

test('06 accesibilidad: 0 violaciones graves o críticas (axe, WCAG 2.2 AA) en cada página del hub', { skip: saltar, timeout: 600_000 }, async () => {
  const hub = await abrirHub();
  const fallos = [];
  try {
    for (const viewport of TAMANOS) {
      const ctx = await hub.browser.newContext({ viewport });
      const page = await ctx.newPage();
      const rutas = [...PAGINAS.map((p) => `hub.html?page=${p}`), ...AJUSTES.map((s) => `hub.html?settings=${s}`)];
      for (const tema of ['dark', 'light']) {
        for (const ruta of rutas) {
          await cargar(page, `${hub.base}/${ruta}&theme=${tema}`);
          await page.addScriptTag({ path: AXE });
          for (const x of await pasarAxe(page)) fallos.push(`[${tema} ${viewport.width}×${viewport.height}] ${ruta} → ${x}`);
        }
      }
      await ctx.close();
    }
  } finally {
    await hub.cerrar();
  }
  assert.deepEqual(fallos, [], `Violaciones de accesibilidad:\n${fallos.join('\n')}`);
});

test('06 accesibilidad: 0 violaciones graves o críticas (axe) en diálogos, confirmaciones y editores', { skip: saltar, timeout: 600_000 }, async () => {
  const hub = await abrirHub();
  const fallos = [];
  try {
    for (const viewport of TAMANOS) {
      const ctx = await hub.browser.newContext({ viewport });
      const page = await ctx.newPage();
      for (const tema of ['dark', 'light']) {
        for (const [nombre, ruta, abrir] of ESTADOS) {
          await cargar(page, `${hub.base}/${ruta}${ruta.includes('?') ? '&' : '?'}theme=${tema}`);
          try { await abrir(page); } catch (e) { fallos.push(`[${tema} ${viewport.width}×${viewport.height}] ${nombre} → no se pudo abrir: ${String(e.message).split('\n')[0]}`); continue; }
          await espera(350); // fin de las animaciones de entrada del modal
          await page.addScriptTag({ path: AXE });
          for (const x of await pasarAxe(page)) fallos.push(`[${tema} ${viewport.width}×${viewport.height}] ${nombre} → ${x}`);
        }
      }
      await ctx.close();
    }
  } finally {
    await hub.cerrar();
  }
  assert.deepEqual(fallos, [], `Violaciones de accesibilidad en diálogos:\n${fallos.join('\n')}`);
});

// Páginas que no están en el menú lateral y dónde está su entrada: [ruta del hub, selector del botón que la abre].
const FUERA_DEL_MENU = {
  insights: ['hub.html?page=home', '[data-act=open-insights]'],
  transforms: ['hub.html?settings=experimental', '[data-act=open-transforms]'],
};
const ESPERADO = { insights: /Estadísticas/, transforms: /Reescribir/ };

test('06 accesibilidad: todo el hub se recorre con el teclado y con foco visible', { skip: saltar, timeout: 300_000 }, async () => {
  const hub = await abrirHub();
  const fallos = [];
  try {
    const page = await (await hub.browser.newContext({ viewport: { width: 1180, height: 780 } })).newPage();
    for (const p of PAGINAS) {
      await cargar(page, `${hub.base}/hub.html?page=${p}`);
      // Sin transiciones: el estilo de foco se lee al instante.
      await page.addStyleTag({ content: '*,*::before,*::after{transition:none!important;animation:none!important}' });
      const alcanzados = new Set();
      for (let i = 0; i < 40; i++) {
        await page.keyboard.press('Tab');
        const r = await page.evaluate(async () => {
          const a = document.activeElement;
          if (!a || a === document.body) return null;
          // Foco visible = algo cambia a la vista (contorno, sombra, borde, fondo, color u opacidad)
          // en el elemento o en sus 3 antepasados (`:focus-within`) entre con foco y sin él.
          const foto = () => {
            const out = [];
            for (let e = a, k = 0; e && k < 4; e = e.parentElement, k++) {
              const c = getComputedStyle(e);
              out.push([c.outlineStyle, c.outlineWidth, c.outlineColor, c.boxShadow, c.borderColor, c.backgroundColor, c.color, c.opacity].join('|'));
            }
            return out.join('#');
          };
          const frame = () => new Promise((ok) => requestAnimationFrame(() => requestAnimationFrame(ok)));
          const con = foto();
          a.blur();
          await frame();
          const sin = foto();
          a.focus();
          await frame();
          const nombre = `${a.tagName.toLowerCase()}${a.id ? `#${a.id}` : ''}${a.className && typeof a.className === 'string' ? `.${a.className.trim().split(/\s+/).join('.')}` : ''} «${(a.getAttribute('aria-label') || a.textContent || '').trim().slice(0, 30)}»`;
          return { visible: con !== sin, nombre, nav: a.matches('[data-nav]') ? a.dataset.nav : null };
        });
        if (!r) continue;
        if (!r.visible) fallos.push(`${p}: sin foco visible en ${r.nombre}`);
        if (r.nav) alcanzados.add(r.nav);
      }
      const faltan = PAGINAS.filter((x) => !FUERA_DEL_MENU[x] && !alcanzados.has(x));
      if (faltan.length) fallos.push(`${p}: con Tab no se llega a ${faltan.join(', ')}`);
    }
    // Estadísticas y Transforms salieron del menú (panel claro, 24-oct-2026): se siguen alcanzando con Tab desde su sitio
    // nuevo, con foco visible, y Enter las abre.
    for (const [destino, [ruta, sel]] of Object.entries(FUERA_DEL_MENU)) {
      await cargar(page, `${hub.base}/${ruta}`);
      await page.addStyleTag({ content: '*,*::before,*::after{transition:none!important;animation:none!important}' });
      let llegado = false;
      for (let i = 0; i < 60 && !llegado; i++) {
        await page.keyboard.press('Tab');
        llegado = await page.evaluate((s) => !!document.activeElement && document.activeElement.matches(s), sel);
      }
      if (!llegado) { fallos.push(`${destino}: con Tab no se llega a su entrada (${ruta} → ${sel})`); continue; }
      await page.keyboard.press('Enter');
      const abierta = await page.waitForFunction((d) => document.querySelector(`.sb [data-nav]`) && !document.querySelector('.velo') && location && document.querySelector('#page h1'), destino, { timeout: 5000 }).then(() => true, () => false);
      const titulo = abierta ? await page.locator('#page h1').first().innerText() : '';
      if (!abierta || !ESPERADO[destino].test(titulo)) fallos.push(`${destino}: Enter en su entrada no la abre (título «${titulo}»)`);
    }
  } finally {
    await hub.cerrar();
  }
  assert.deepEqual([...new Set(fallos)], [], `Teclado y foco:\n${[...new Set(fallos)].join('\n')}`);
});
