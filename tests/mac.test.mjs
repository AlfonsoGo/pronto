// Pronto 2 para Mac: lo que se puede comprobar desde Windows (BELTR-PRT-20, specs/mac/CONTRATO.md).
//
// 1. Cada función de la versión de Windows (specs/mac/funciones.json) tiene su estado en Mac en
//    specs/mac/ESTADO.md: «hecho», «degradado» o «no», y las dos últimas con su motivo.
// 2. El Rust compila para macOS (aarch64-apple-darwin), también los tests: 0 errores. Es la guarda que
//    avisa si alguien añade código de Windows sin su hermano de Mac (decisión D2). En el Mac no se
//    hace: allí ya se compila de verdad (scripts/mac/probar.sh).
// 3. Regla 1 en Mac: el código propio del Mac solo nombra hosts de la lista de specs/PRD.md §5.2.
// 4. La página «Permisos del Mac» del hub (maqueta con ?mac) pasa axe (regla 6) y esconde los botones
//    de ventana de Windows.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const RAIZ = join(dirname(fileURLToPath(import.meta.url)), '..');
const require = createRequire(import.meta.url);

test('mac 1: cada función de Windows tiene su estado en Mac, y su motivo si no está entera', () => {
  const funciones = JSON.parse(readFileSync(join(RAIZ, 'specs/mac/funciones.json'), 'utf8'));
  const estado = readFileSync(join(RAIZ, 'specs/mac/ESTADO.md'), 'utf8');
  // Filas de la tabla: | F01 | nombre | hecho | cómo / motivo |
  const filas = new Map();
  for (const l of estado.split(/\r?\n/)) {
    const m = /^\|\s*(F\d{2})\s*\|[^|]*\|\s*(hecho|degradado|no)\s*\|([^|]*)\|/.exec(l);
    if (m) filas.set(m[1], { estado: m[2], nota: m[3].trim() });
  }
  const faltan = funciones.filter((f) => !filas.has(f.id)).map((f) => f.id);
  assert.deepEqual(faltan, [], `funciones sin estado en ESTADO.md: ${faltan.join(', ')}`);
  const sinMotivo = [...filas].filter(([, v]) => v.estado !== 'hecho' && v.nota.length < 15).map(([k]) => k);
  assert.deepEqual(sinMotivo, [], `«degradado» o «no» sin motivo escrito: ${sinMotivo.join(', ')}`);
  const sobran = [...filas.keys()].filter((id) => !funciones.some((f) => f.id === id));
  assert.deepEqual(sobran, [], `ESTADO.md tiene funciones que no están en funciones.json: ${sobran.join(', ')}`);
});

test('mac 2: el Rust compila para macOS sin errores (también los tests)', { skip: process.platform === 'darwin' && 'en el Mac se compila de verdad', timeout: 40 * 60_000 }, async () => {
  const { checkMac } = await import('../scripts/mac/check.mjs');
  const r = checkMac({ tests: true });
  const detalle = r.salida.split(/\r?\n/).filter((l) => / error/.test(l)).slice(0, 40).join('\n');
  assert.equal(r.total, 0, `cargo check para aarch64-apple-darwin: ${r.total} errores ${JSON.stringify(r.errores)}\n${detalle}`);
});

test('mac 3: el código propio del Mac solo nombra hosts permitidos (regla 1)', () => {
  const PERMITIDOS = ['huggingface.co', 'ollama.com', 'github.com', 'api.github.com', '127.0.0.1', 'localhost'];
  const dir = join(RAIZ, 'src-tauri/src/mac');
  const fallos = [];
  for (const f of readdirSync(dir).filter((x) => x.endsWith('.rs'))) {
    const src = readFileSync(join(dir, f), 'utf8');
    for (const m of src.matchAll(/\bhttps?:\/\/([a-z0-9.-]+)/gi)) {
      const host = m[1].toLowerCase();
      if (!PERMITIDOS.some((h) => host === h || host.endsWith(`.${h}`))) fallos.push(`${f}: ${host}`);
    }
  }
  assert.deepEqual(fallos, [], `hosts fuera de specs/PRD.md §5.2: ${fallos.join(', ')}`);
});

test('mac 4: la página de permisos del Mac es accesible y el marco es el de macOS', { timeout: 120_000 }, async (t) => {
  const { abrirHub, cargar, rutaChrome } = await import('./no-se-negocia/_comun.mjs');
  if (!rutaChrome()) return t.skip('no hay Chrome (define PRIMERIZO_CHROME)');
  const AXE = require.resolve('axe-core/axe.min.js');
  const hub = await abrirHub();
  const fallos = [];
  try {
    const page = await hub.browser.newPage({ viewport: { width: 1180, height: 780 } });
    for (const tema of ['dark', 'light']) {
      for (const q of ['mac', 'mac=ok']) {
        await cargar(page, `${hub.base}/hub.html?${q}&page=permisos&theme=${tema}`);
        const info = await page.evaluate(() => ({
          os: document.documentElement.dataset.os,
          wc: getComputedStyle(document.querySelector('.wc')).display,
          tarjetas: document.querySelectorAll('.perm').length,
          menu: !!document.querySelector('.sb [data-nav="permisos"]'),
        }));
        if (info.os !== 'macos') fallos.push(`${q}: el hub no se marca como macOS`);
        if (info.wc !== 'none') fallos.push(`${q}: se ven los botones de ventana de Windows`);
        if (info.tarjetas !== 4) fallos.push(`${q}: ${info.tarjetas} tarjetas de permisos (se esperaban 4)`);
        if (!info.menu) fallos.push(`${q}: «Permisos del Mac» no está en el menú`);
        await page.addScriptTag({ path: AXE });
        const v = await page.evaluate(async () => (await window.axe.run(document, { runOnly: { type: 'tag', values: ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'] }, resultTypes: ['violations'] })).violations
          .filter((x) => x.impact === 'serious' || x.impact === 'critical').map((x) => `${x.impact} ${x.id}`));
        for (const x of v) fallos.push(`[${tema} ${q}] ${x}`);
      }
    }
    // Sin ?mac (Windows) no hay página de permisos en el menú y se ven los botones de ventana.
    await cargar(page, `${hub.base}/hub.html?page=home`);
    const win = await page.evaluate(() => ({ menu: !!document.querySelector('.sb [data-nav="permisos"]'), wc: getComputedStyle(document.querySelector('.wc')).display }));
    if (win.menu) fallos.push('Windows: sale «Permisos del Mac» en el menú');
    if (win.wc === 'none') fallos.push('Windows: no se ven los botones de ventana');
  } finally {
    await hub.cerrar();
  }
  assert.deepEqual(fallos, []);
});
