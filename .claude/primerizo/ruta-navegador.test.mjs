// node --test .claude/primerizo/ruta-navegador.test.mjs
// El primerizo tiene que arrancar aunque no esté el Chromium de Playwright (24-oct-2026: no arrancaba y el PC tenía Chrome).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { rutaNavegador } from './ruta-navegador.mjs';

const CHROME = 'C:/Program Files/Google/Chrome/Application/chrome.exe';
const EDGE = 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe';
const PW = 'C:/Users/x/AppData/Local/ms-playwright/chromium-1248/chrome-win64/chrome.exe';
const pw = { executablePath: () => PW };
const hay = (...rutas) => (r) => rutas.includes(r);

test('sin el Chromium de Playwright, usa el Chrome del sistema', () => {
  assert.equal(rutaNavegador(pw, {}, hay(CHROME)), CHROME);
});
test('sin Chrome, usa Edge', () => {
  assert.equal(rutaNavegador(pw, {}, hay(EDGE)), EDGE);
});
test('PRIMERIZO_CHROME manda si existe; si no, sigue buscando', () => {
  assert.equal(rutaNavegador(pw, { PRIMERIZO_CHROME: 'D:/c.exe' }, hay('D:/c.exe', CHROME)), 'D:/c.exe');
  assert.equal(rutaNavegador(pw, { PRIMERIZO_CHROME: 'D:/no.exe' }, hay(CHROME)), CHROME);
});
test('el de Playwright, si está instalado, va antes que el del sistema', () => {
  assert.equal(rutaNavegador(pw, {}, hay(PW, CHROME)), PW);
});
test('playwright-core sin navegadores (executablePath lanza) no rompe la búsqueda', () => {
  assert.equal(rutaNavegador({ executablePath: () => { throw new Error('sin navegadores'); } }, {}, hay(CHROME)), CHROME);
});
test('sin ningún navegador devuelve null (navegador.mjs lo explica y para)', () => {
  assert.equal(rutaNavegador(pw, {}, () => false), null);
});
