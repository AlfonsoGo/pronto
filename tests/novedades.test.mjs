// Novedades: cada versión que se publica trae las suyas, completas y en los dos idiomas, y su texto de GitHub cabe entero
// en el aviso de actualización de Pronto (`updater::plain_notes`: 600 caracteres, 12 líneas, sin enlaces).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { RELEASES, githubBody } from '../ui/hub/novedades.js';
import { ICON_NAMES } from '../ui/shared/icons.js';
import { PAGINAS, AJUSTES } from './no-se-negocia/_comun.mjs';

const leer = (p) => readFileSync(new URL(`../${p}`, import.meta.url), 'utf8');
const versionConf = JSON.parse(leer('src-tauri/tauri.conf.json')).version;
const versionCargo = leer('src-tauri/Cargo.toml').match(/^version\s*=\s*"([^"]+)"/m)[1];

// Lo que deja `updater::plain_notes` de un Markdown (misma lógica, para medir lo que verá el usuario).
function enAviso(md) {
  const out = [];
  for (const raw of md.split('\n')) {
    const l = raw.trim();
    if (!l) { if (out.length && out.at(-1)) out.push(''); continue; }
    if (l.toLowerCase().includes('.exe')) continue;
    let s = l;
    if (l.startsWith('#')) { s = l.replace(/^#+/, '').trim(); if (!out.length && /^pronto\s+v?\d/i.test(s)) continue; }
    else if (l.startsWith('- ')) s = `• ${l.slice(2)}`;
    s = s.replace(/\[([^\]]*)\]\([^)]*\)/g, '$1').replace(/(\*\*|__)(.+?)(\*\*|__)/g, '$2');
    out.push(s);
  }
  while (out.length && !out.at(-1)) out.pop();
  return out;
}

test('novedades: la versión que se compila tiene sus novedades y es la primera de la lista', () => {
  assert.equal(versionConf, versionCargo, 'tauri.conf.json y Cargo.toml no dicen la misma versión');
  assert.equal(RELEASES[0].version, versionConf, `Falta la entrada de la ${versionConf} al principio de ui/hub/novedades.js`);
  assert.ok(RELEASES[0].highlights.length >= 1, 'La última versión necesita al menos un destacado');
});

test('novedades: versiones únicas, de la más nueva a la más vieja, con mes AAAA-MM', () => {
  const n = (v) => v.split('.').map(Number).reduce((a, x) => a * 1000 + x, 0);
  for (let i = 1; i < RELEASES.length; i++) assert.ok(n(RELEASES[i - 1].version) > n(RELEASES[i].version), `${RELEASES[i - 1].version} antes que ${RELEASES[i].version}`);
  for (const r of RELEASES) assert.match(r.month, /^\d{4}-(0[1-9]|1[0-2])$/, `${r.version}: mes «${r.month}»`);
});

test('novedades: todo en castellano e inglés, con icono existente y «Probar» que lleva a una pantalla real', () => {
  const fallos = [];
  const destinos = new Set([...PAGINAS, 'novedades']);
  for (const r of RELEASES) {
    if (!r.es?.title || !r.en?.title) fallos.push(`${r.version}: título`);
    if (!!r.es?.intro !== !!r.en?.intro) fallos.push(`${r.version}: intro en un solo idioma`);
    for (const h of r.highlights) {
      if (!h.es?.t || !h.es?.d || !h.en?.t || !h.en?.d) fallos.push(`${r.version}: destacado «${h.es?.t}» sin los dos idiomas`);
      if (!ICON_NAMES.includes(h.icon)) fallos.push(`${r.version}: icono «${h.icon}» no existe`);
      if (h.probar?.page && !destinos.has(h.probar.page)) fallos.push(`${r.version}: «Probar» a la página «${h.probar.page}»`);
      if (h.probar?.settings && !AJUSTES.includes(h.probar.settings)) fallos.push(`${r.version}: «Probar» a Configuración «${h.probar.settings}»`);
      if (h.probar?.hint && (!h.probar.hint.es || !h.probar.hint.en)) fallos.push(`${r.version}: pista de «${h.es.t}» sin los dos idiomas`);
      if (h.es.breve && h.es.breve.length > 70) fallos.push(`${r.version}: breve de «${h.es.t}» pasa de 70`);
    }
    for (const i of [...r.improved, ...r.fixed]) if (!i.es || !i.en) fallos.push(`${r.version}: «${i.es || i.en}» sin los dos idiomas`);
  }
  assert.deepEqual(fallos, []);
});

test('novedades: el texto de GitHub de la última versión cabe entero en el aviso de actualización', () => {
  const lineas = enAviso(githubBody(RELEASES[0].version));
  const texto = lineas.join('\n');
  assert.ok(lineas.length <= 12, `${lineas.length} líneas (máx. 12):\n${texto}`);
  assert.ok(texto.length <= 600, `${texto.length} caracteres (máx. 600):\n${texto}`);
  assert.match(texto, /Novedades/, 'el aviso tiene que decir dónde ver todo');
});
