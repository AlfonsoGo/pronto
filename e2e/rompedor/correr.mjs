#!/usr/bin/env node
// Rompedor del hub y la flow bar: corre las sondas e2e/rompedor/*.mjs (salvo comun/correr) en Chrome headless
// contra ui/ servido en un puerto libre con el backend simulado. Ninguna petición sale de 127.0.0.1.
//
//   node e2e/rompedor/correr.mjs [sonda ...]
// Imprime cada hallazgo (ALTA/MEDIA/BAJA/OK) y, por sonda, los errores JS sin capturar, console.error,
// diálogos (alert) y peticiones fuera de 127.0.0.1 que recogieron las guardas. Código de salida 1 si hay ALTA.
import { readdir } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';
import { join } from 'node:path';
import { AQUI, arrancar, fabrica, captura } from './comun.mjs';

const filtro = process.argv.slice(2);
const sondas = (await readdir(AQUI)).filter((f) => f.endsWith('.mjs') && !['comun.mjs', 'correr.mjs'].includes(f)).sort();
const { base, browser, cerrar } = await arrancar();
console.log(`rompedor · ${base} · ${new Date().toISOString()}\n`);
let altas = 0;
const resumen = { ALTA: 0, MEDIA: 0, BAJA: 0, OK: 0 };
try {
  for (const f of sondas) {
    const m = await import(pathToFileURL(join(AQUI, f)).href);
    if (filtro.length && !filtro.includes(m.nombre)) continue;
    const registro = [];
    const t0 = Date.now();
    let hallazgos;
    try { hallazgos = await m.correr({ abrir: fabrica(browser, base, registro), base, captura }); }
    catch (e) { hallazgos = [{ grav: 'MEDIA', titulo: 'la sonda se rompió', evidencia: String(e && e.stack || e).slice(0, 800) }]; }
    // guardas de todas las páginas que abrió la sonda
    const g = { errores: new Set(), consola: new Set(), fuera: new Set(), dialogos: new Set() };
    for (const fn of registro) { const x = fn(); for (const k of Object.keys(g)) x[k].forEach((v) => g[k].add(v)); }
    if (g.fuera.size) hallazgos.push({ grav: 'ALTA', titulo: 'petición fuera de 127.0.0.1', evidencia: [...g.fuera].join('\n') });
    if (g.dialogos.size) hallazgos.push({ grav: 'ALTA', titulo: 'diálogo (alert/confirm) abierto por contenido', evidencia: [...g.dialogos].join('\n') });
    if (g.errores.size && !hallazgos.some((h) => h.cubreErrores)) hallazgos.push({ grav: 'MEDIA', titulo: 'error JS sin capturar (pageerror)', evidencia: [...g.errores].join('\n') });
    console.log(`━━ ${m.nombre}  (${((Date.now() - t0) / 1000).toFixed(1)} s)`);
    for (const h of hallazgos) {
      resumen[h.grav] = (resumen[h.grav] || 0) + 1;
      if (h.grav === 'ALTA') altas++;
      console.log(`  [${h.grav}] ${h.titulo}`);
      if (h.evidencia) console.log(String(h.evidencia).split('\n').map((l) => '      ' + l).join('\n'));
    }
    if (g.consola.size) console.log(`  (console.error: ${[...g.consola].slice(0, 8).join(' | ')})`);
    console.log('');
  }
} finally {
  await cerrar();
}
console.log(`resumen · ${resumen.ALTA} ALTA · ${resumen.MEDIA} MEDIA · ${resumen.BAJA} BAJA · ${resumen.OK} OK`);
process.exit(altas ? 1 : 0);
