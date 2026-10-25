// Utilidades de los tests de «lo que no se negocia» (CLAUDE.md § NO SE NEGOCIA). No es un test:
// `node --test` solo recoge los `*.test.mjs`.
import { spawnSync } from 'node:child_process';
import { readdirSync, readFileSync, existsSync } from 'node:fs';
import { join, resolve, relative, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

export const RAIZ = resolve(fileURLToPath(new URL('../../', import.meta.url)));
export const SRC_TAURI = join(RAIZ, 'src-tauri');

/**
 * Lanza `cargo test <filtro>` en src-tauri y devuelve la salida y el recuento. Si otro `cargo` tiene
 * el directorio de compilación bloqueado, cargo espera solo («Blocking waiting for file lock»).
 */
export function cargoTest(filtro, { env = {}, timeoutMs = 30 * 60_000 } = {}) {
  const r = spawnSync('cargo', ['test', filtro, '--', '--nocapture', '--color', 'never'], {
    cwd: SRC_TAURI,
    env: { ...process.env, ...env, CARGO_TERM_COLOR: 'never' },
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
    timeout: timeoutMs,
    shell: process.platform === 'win32',
  });
  const salida = `${r.stdout ?? ''}\n${r.stderr ?? ''}`;
  let pasados = 0;
  let fallados = 0;
  for (const m of salida.matchAll(/test result: \w+\. (\d+) passed; (\d+) failed/g)) {
    pasados += Number(m[1]);
    fallados += Number(m[2]);
  }
  return { ok: r.status === 0, estado: r.status, salida, pasados, fallados, ejecutados: pasados + fallados };
}

/** Falla con la salida de cargo si algo no pasó o si el filtro no ejecutó ningún test. */
export function exigirCargoVerde(assert, r, filtro) {
  const ls = r.salida.split(/\r?\n/);
  // Las líneas útiles y, tras cada «panicked at», el mensaje del assert (va en la línea siguiente).
  const cola = ls.filter((l, i) => /^test |panicked|left:|right:|assertion|error(\[|:)|NSN_/.test(l) || /panicked at/.test(ls[i - 1] ?? '')).slice(-60).join('\n');
  assert.ok(r.ejecutados > 0, `cargo test ${filtro} no ejecutó ningún test (¿filtro mal escrito?)\n${cola}`);
  assert.equal(r.fallados, 0, `cargo test ${filtro}: ${r.fallados} test(s) fallaron\n${cola}`);
  assert.ok(r.ok, `cargo test ${filtro} terminó con estado ${r.estado}\n${cola}`);
}

/** Ficheros bajo `dir` (recursivo) con alguna de las extensiones. */
export function ficheros(dir, extensiones) {
  const out = [];
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, e.name);
    if (e.isDirectory()) out.push(...ficheros(p, extensiones));
    else if (extensiones.some((x) => e.name.endsWith(x))) out.push(p);
  }
  return out;
}

export const rel = (p) => relative(RAIZ, p).split(sep).join('/');

/**
 * Código Rust de producción: sin los bloques `#[cfg(test)]` (módulos y funciones de prueba) ni los
 * comentarios de línea completa. Lo que queda es lo que va dentro de Pronto.
 */
export function quitarTests(src) {
  const ls = src.split(/\r?\n/);
  const out = [];
  for (let i = 0; i < ls.length; i++) {
    if (ls[i].trim() !== '#[cfg(test)]') {
      if (!/^\s*\/\//.test(ls[i])) out.push(ls[i]);
      continue;
    }
    let j = i + 1;
    while (j < ls.length && ls[j].trim().startsWith('#[')) j++;
    const l = ls[j] ?? '';
    const sangria = l.match(/^\s*/)[0];
    if (l.trimEnd().endsWith(';') || (l.includes('{') && l.trimEnd().endsWith('}'))) { i = j; continue; }
    let k = j + 1;
    while (k < ls.length && ls[k] !== `${sangria}}`) k++;
    i = k;
  }
  return out.join('\n');
}

/** Carpeta de los submódulos de un módulo: `x.rs` → `x\`, `x\mod.rs` → `x\`. */
const carpetaDeModulo = (p) => (/[\\/]mod\.rs$/.test(p) ? join(p, '..') : p.replace(/\.rs$/, ''));

/**
 * Código Rust que va dentro de Pronto: cada `.rs` de `dir` sin sus bloques de prueba (`quitarTests`)
 * y sin los ficheros que solo se compilan en pruebas (`soloDePrueba`).
 */
export function rustDeProduccion(dir = join(SRC_TAURI, 'src')) {
  const pruebas = soloDePrueba(dir);
  return ficheros(dir, ['.rs'])
    // Fuera el fichero de prueba y sus submódulos (`x.rs` y `x/mod.rs` → todo lo que cuelga de `x/`).
    .filter((f) => !pruebas.some((p) => f === p || f.startsWith(carpetaDeModulo(p) + sep)))
    .map((f) => ({ fichero: rel(f), texto: quitarTests(readFileSync(f, 'utf8')) }));
}

/**
 * Ficheros `.rs` que solo se compilan en pruebas: los que se enganchan con `#[cfg(test)] mod x;`
 * (p. ej. `rompedor_sondas.rs`) y a los que nada más enlaza. Para no abrir agujeros, un fichero NO se
 * da por «de prueba» si también se declara sin `#[cfg(test)]`, si la declaración lleva `#[path]` o
 * cualquier otro `cfg` (`any(test, …)`, `not(test)`), o si algún `#[path = …]` o `include!(…)`
 * del código de producción lo nombra.
 */
export function soloDePrueba(dir) {
  const decl = /^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;/;
  const deTest = new Set();
  const deProduccion = new Set();
  const nombradosEnProduccion = [];
  for (const f of ficheros(dir, ['.rs'])) {
    const ls = readFileSync(f, 'utf8').split(/\r?\n/);
    // Los módulos de `x.rs` viven en `x/`; los de main.rs, lib.rs y mod.rs, en su misma carpeta.
    const base = /[\\/](main|lib|mod)\.rs$/.test(f) ? join(f, '..') : f.replace(/\.rs$/, '');
    for (let i = 0; i < ls.length; i++) {
      const m = ls[i].match(decl);
      if (!m) continue;
      const attrs = [];
      for (let j = i - 1; j >= 0 && ls[j].trim().startsWith('#['); j--) attrs.push(ls[j].trim());
      const destinos = [join(base, `${m[1]}.rs`), join(base, m[1], 'mod.rs')];
      const soloTest = attrs.includes('#[cfg(test)]') && attrs.every((a) => a === '#[cfg(test)]' || !/^#\[\s*(cfg|path)/.test(a));
      for (const d of destinos) (soloTest ? deTest : deProduccion).add(d);
    }
    const prod = quitarTests(ls.join('\n'));
    for (const m of prod.matchAll(/#\[\s*path\s*=\s*"([^"]+)"|include(?:_str|_bytes)?!\s*\(\s*(?:concat!\s*\()?[^)]*?"([^"]+)"/g)) nombradosEnProduccion.push(m[1] ?? m[2]);
  }
  const nombre = (p) => p.split(/[\\/]/).pop();
  return [...deTest].filter((p) => !deProduccion.has(p) && existsSync(p) && !nombradosEnProduccion.some((n) => nombre(n) === nombre(p)));
}

/** Chrome para Playwright (headless): `PRIMERIZO_CHROME` o el de sistema. */
export function rutaChrome() {
  const candidatos = [process.env.PRIMERIZO_CHROME, 'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe', 'C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe'];
  return candidatos.find((c) => c && existsSync(c)) ?? null;
}

/** Páginas del hub (ui/hub.js `PAGES`) y pestañas de Configuración (ui/hub/pages/settings.js `TABS`). */
export const PAGINAS = ['home', 'meetings', 'insights', 'dictionary', 'snippets', 'style', 'transforms', 'notes', 'novedades'];
export const AJUSTES = ['general', 'engine', 'system', 'meetings', 'experimental', 'data'];

/** Sirve ui/ en un puerto libre de 127.0.0.1 y abre Chrome headless. */
export async function abrirHub() {
  const { servir } = await import('../../scripts/servir-ui.mjs');
  const { chromium } = await import('playwright-core');
  const servidor = await servir(0);
  const base = `http://127.0.0.1:${servidor.address().port}`;
  const browser = await chromium.launch({ executablePath: rutaChrome(), headless: true });
  return {
    base,
    browser,
    async cerrar() {
      await browser.close();
      await new Promise((ok) => servidor.close(ok));
    },
  };
}

/** Abre `ruta` del hub y espera a que la página haya pintado (sin animaciones pendientes). */
export async function cargar(page, url) {
  await page.goto(url, { waitUntil: 'load' });
  await page.waitForFunction(() => {
    const app = document.querySelector('#app');
    return app && app.children.length > 0 && !document.querySelector('.cargando, [aria-busy="true"]');
  }, null, { timeout: 15_000 }).catch(() => {});
  await page.waitForTimeout(700);
}
