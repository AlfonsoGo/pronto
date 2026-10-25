#!/usr/bin/env node
// Recorredor: lanza cada camino crítico (e2e/caminos/*.spec.mjs) R veces en Chrome headless contra el hub con el
// backend simulado (ui/shared/mock.js), mide cada paso y escribe e2e/results/latest.json + copia con fecha.
//
//   node e2e/correr.mjs [camino ...]        (R=5 por defecto; E2E_R=3 para cambiarlo)
//   Chrome: C:\Program Files\Google\Chrome\Application\chrome.exe (o E2E_CHROME). No instala navegadores.
import { chromium } from 'playwright-core';
import { createServer } from 'node:net';
import { readdir, writeFile, mkdir } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { join, dirname } from 'node:path';
import { servir } from '../scripts/servir-ui.mjs';

const AQUI = dirname(fileURLToPath(import.meta.url));
const R = Number(process.env.E2E_R ?? 5);
const PRESUPUESTO_MS = 3000; // SUPOSICIÓN: CLAUDE.md no fija presupuesto de UI; el agente prescribe 3 s por paso.
const CHROME = process.env.E2E_CHROME ?? 'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe';

const libre = (p) => new Promise((ok) => {
  const s = createServer().once('error', () => ok(false)).once('listening', () => s.close(() => ok(true)));
  s.listen(p, '127.0.0.1');
});
const p95 = (xs) => {
  if (!xs.length) return 0;
  const o = [...xs].sort((a, b) => a - b);
  return o[Math.min(o.length - 1, Math.ceil(0.95 * o.length) - 1)];
};
const primeraLinea = (e) => String((e && e.message) || e).split('\n').map((l) => l.trim()).find(Boolean) ?? 'error';

if (!existsSync(CHROME)) { console.error(`No existe Chrome en ${CHROME}`); process.exit(2); }
const puerto = (await libre(4173)) ? 4173 : 0;
const servidor = await servir(puerto);
const port = servidor.address().port;
const BASE = `http://127.0.0.1:${port}/hub.html`;

const filtro = process.argv.slice(2);
const ficheros = (await readdir(join(AQUI, 'caminos'))).filter((f) => f.endsWith('.spec.mjs')).sort();
const caminos = [];
for (const f of ficheros) {
  const m = await import(pathToFileURL(join(AQUI, 'caminos', f)).href);
  if (!filtro.length || filtro.includes(m.nombre)) caminos.push(m);
}

const browser = await chromium.launch({ executablePath: CHROME, headless: true });
const resultado = { fecha: new Date().toISOString(), base: BASE, repeticiones: R, caminos: {}, sesiones_sin_error_pct: 0 };
let pasadasTotal = 0, pasadasOk = 0;
console.log(`recorredor · base ${BASE} · R=${R} · presupuesto ${PRESUPUESTO_MS} ms/paso (suposición)\n`);

try {
  for (const camino of caminos) {
    const porPaso = new Map(); // nombre -> { orden, medidas: [{ms, estado, causa}] }
    const todos = [];
    let fallos = 0;
    for (let r = 1; r <= R; r++) {
      const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 }, locale: 'es-ES', permissions: ['clipboard-read', 'clipboard-write'] });
      const page = await ctx.newPage();
      const erroresPagina = [];
      page.on('pageerror', (e) => erroresPagina.push(primeraLinea(e)));
      const url = (params = '') => `${BASE}${params ? '?' + params : ''}`;
      let roto = false;
      const paso = async (nombre, fn, { timeout = 10000 } = {}) => {
        if (roto) return;
        if (!porPaso.has(nombre)) porPaso.set(nombre, { orden: porPaso.size, medidas: [] });
        const t0 = performance.now();
        try {
          await fn({ timeout });
          const ms = Math.round(performance.now() - t0);
          porPaso.get(nombre).medidas.push({ ms, estado: ms > PRESUPUESTO_MS ? 'lento' : 'ok' });
          todos.push(ms);
        } catch (e) {
          const ms = Math.round(performance.now() - t0);
          roto = true;
          porPaso.get(nombre).medidas.push({ ms, estado: 'roto', causa: primeraLinea(e) });
          todos.push(ms);
        }
      };
      const t0 = performance.now();
      try { await camino.correr({ page, url, paso }); }
      catch (e) { // fallo fuera de un paso: se anota como paso roto «(fuera de paso)»
        roto = true;
        if (!porPaso.has('(fuera de paso)')) porPaso.set('(fuera de paso)', { orden: porPaso.size, medidas: [] });
        porPaso.get('(fuera de paso)').medidas.push({ ms: 0, estado: 'roto', causa: primeraLinea(e) });
      }
      if (roto) fallos++;
      console.log(`  ${camino.nombre} #${r}: ${roto ? 'ROTO ' : 'ok   '} ${Math.round(performance.now() - t0)} ms${erroresPagina.length ? ` · errores de página: ${erroresPagina.join(' | ')}` : ''}`);
      await ctx.close();
    }
    const pasos = [...porPaso.entries()].sort((a, b) => a[1].orden - b[1].orden).map(([paso, { medidas }]) => {
      const rotos = medidas.filter((m) => m.estado === 'roto');
      const ms = p95(medidas.map((m) => m.ms));
      const estado = rotos.length ? 'roto' : ms > PRESUPUESTO_MS ? 'lento' : 'ok';
      const o = { paso, estado, ms };
      if (rotos.length) o.causa = `${rotos.length}/${R} pasadas: ${rotos[0].causa}`;
      return o;
    });
    resultado.caminos[camino.nombre] = { pasadas: R, fallos, p95_ms: p95(todos), pasos };
    pasadasTotal += R; pasadasOk += R - fallos;
  }
} finally {
  await browser.close();
  servidor.close();
}

resultado.sesiones_sin_error_pct = pasadasTotal ? Math.round((pasadasOk / pasadasTotal) * 1000) / 10 : 0;
const d = new Date();
const dos = (n) => String(n).padStart(2, '0');
const sello = `${d.getFullYear()}-${dos(d.getMonth() + 1)}-${dos(d.getDate())}-${dos(d.getHours())}${dos(d.getMinutes())}`;
await mkdir(join(AQUI, 'results'), { recursive: true });
const json = JSON.stringify(resultado, null, 2) + '\n';
await writeFile(join(AQUI, 'results', 'latest.json'), json);
await writeFile(join(AQUI, 'results', `${sello}.json`), json);

console.log('\ncamino · pasadas · fallos · p95 ms · peor paso');
for (const [n, c] of Object.entries(resultado.caminos)) {
  const peor = c.pasos.reduce((a, b) => (b.ms > a.ms ? b : a), c.pasos[0] ?? { paso: '-', ms: 0 });
  console.log(`${n} · ${c.pasadas} · ${c.fallos} · ${c.p95_ms} · ${peor.paso} (${peor.ms} ms)`);
  for (const p of c.pasos) console.log(`    [${p.estado}] ${p.paso} ${p.ms} ms${p.causa ? ' — ' + p.causa : ''}`);
}
console.log(`\nsesiones_sin_error_pct: ${resultado.sesiones_sin_error_pct}  (${pasadasOk}/${pasadasTotal} pasadas sin paso roto)`);
console.log(`resultados: e2e/results/latest.json y e2e/results/${sello}.json`);
process.exit(0);
