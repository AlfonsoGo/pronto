#!/usr/bin/env node
// Sirve ui/ en estático para abrir el hub en un navegador con el backend simulado (ui/shared/mock.js).
// Lo usan los tests de accesibilidad, el recorredor y el primerizo. Solo escucha en 127.0.0.1.
//
//   node scripts/servir-ui.mjs [puerto]     (por defecto 4173 o PRONTO_UI_PUERTO)
//   → http://127.0.0.1:4173/hub.html
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { extname, join, normalize, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const RAIZ = resolve(fileURLToPath(new URL('../ui/', import.meta.url)));
const TIPOS = {
  '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.mjs': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8', '.json': 'application/json', '.svg': 'image/svg+xml', '.png': 'image/png',
  '.ico': 'image/x-icon', '.wav': 'audio/wav', '.woff2': 'font/woff2', '.ttf': 'font/ttf',
};

export function servir(puerto = Number(process.env.PRONTO_UI_PUERTO ?? 4173)) {
  const servidor = createServer(async (req, res) => {
    const ruta = decodeURIComponent(new URL(req.url, 'http://x').pathname);
    const fichero = normalize(join(RAIZ, ruta === '/' ? 'hub.html' : ruta));
    if (fichero !== RAIZ && !fichero.startsWith(RAIZ + sep)) { res.writeHead(403).end(); return; }
    try {
      const datos = await readFile(fichero);
      res.writeHead(200, { 'Content-Type': TIPOS[extname(fichero)] ?? 'application/octet-stream', 'Cache-Control': 'no-store' });
      res.end(datos);
    } catch {
      res.writeHead(404).end('no encontrado');
    }
  });
  return new Promise((ok) => servidor.listen(puerto, '127.0.0.1', () => ok(servidor)));
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const puerto = Number(process.argv[2] ?? process.env.PRONTO_UI_PUERTO ?? 4173);
  await servir(puerto);
  console.log(`hub en http://127.0.0.1:${puerto}/hub.html (backend simulado)`);
}
