// Acortar cuánto se conserva el historial o el audio borra al momento y para siempre: nunca sin confirmar (reglas 2 y 5;
// rompedor 25-oct ALTA 1: con el teclado, cada flecha del desplegable guardaba y purgaba).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { shortensRetention } from '../ui/hub/retencion.js';
import { abrirHub, cargar, rutaChrome } from './no-se-negocia/_comun.mjs';

test('retención: qué cuenta como acortar', () => {
  const H = 'historyRetentionDays', A = 'keepAudioDays';
  assert.equal(shortensRetention(H, 0, 7), true);     // siempre → 7 días
  assert.equal(shortensRetention(H, 30, 7), true);
  assert.equal(shortensRetention(H, 7, 30), false);
  assert.equal(shortensRetention(H, 30, 0), false);   // → siempre
  assert.equal(shortensRetention(H, 30, 30), false);
  assert.equal(shortensRetention(A, 14, 7), true);
  assert.equal(shortensRetention(A, 14, 1), true);
  assert.equal(shortensRetention(A, 14, 0), true);    // → no guardar audio
  assert.equal(shortensRetention(A, 0, 7), false);
  assert.equal(shortensRetention(A, 7, 14), false);
  assert.equal(shortensRetention('theme', 1, 0), false);
});

test('retención: una flecha en el desplegable pregunta y no guarda; Cancelar deja el valor guardado', { skip: !rutaChrome() && 'no hay Chrome (define PRIMERIZO_CHROME)', timeout: 120_000 }, async () => {
  const hub = await abrirHub();
  try {
    const page = await (await hub.browser.newContext({ viewport: { width: 1180, height: 780 } })).newPage();
    for (const [sel, tecla, antes] of [['historyRetentionDays', 'ArrowDown', '0'], ['keepAudioDays', 'ArrowUp', '14']]) {
      await cargar(page, `${hub.base}/hub.html?settings=data`);
      await page.waitForSelector(`[data-select=${sel}]`);
      await page.focus(`[data-select=${sel}]`);
      await page.keyboard.press(tecla);
      await page.waitForSelector('.velo [data-ok]', { timeout: 5000 });
      assert.equal(await page.$eval(`[data-select=${sel}]`, (s) => s.value), antes, `${sel}: la flecha cambió el valor sin confirmar`);
      const foco = await page.evaluate(() => document.activeElement && document.activeElement.hasAttribute('data-ok'));
      assert.equal(foco, false, `${sel}: el diálogo de borrar empieza con el foco en «Sí, borrar»`);
      await page.click('.velo:not(.out) .modal-ft [data-close]');
      await page.waitForTimeout(400);
      assert.equal(await page.$eval(`[data-select=${sel}]`, (s) => s.value), antes, `${sel}: Cancelar no dejó el valor guardado`);
    }
  } finally {
    await hub.cerrar();
  }
});
