#!/usr/bin/env node
// Texto de la release de GitHub sacado de ui/hub/novedades.js (la misma fuente que la página «Novedades» del hub).
//
//   node scripts/notas-release.mjs            → la versión de src-tauri/tauri.conf.json
//   node scripts/notas-release.mjs 2.3.0      → otra versión
//
// Se pega tal cual como descripción de la release (o `gh release create … --notes-file -`).
import { readFileSync } from 'node:fs';
import { githubBody } from '../ui/hub/novedades.js';

const conf = JSON.parse(readFileSync(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8'));
process.stdout.write(githubBody(process.argv[2] || conf.version));
