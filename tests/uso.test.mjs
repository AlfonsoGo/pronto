// Estadísticas de uso (PRNT-43): el aviso pregunta, nada queda marcado y el ajuste se puede retirar.
// La lista cerrada de lo que sale se prueba en `uso.rs` (`nsn_privacidad_uso_*`, dentro de la regla 1).
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { SRC_TAURI, abrirHub, cargar } from './no-se-negocia/_comun.mjs';

let hub;
before(async () => { hub = await abrirHub(); });
after(async () => { await hub.cerrar(); });

async function abrir(sufijo = 'uso') {
  const page = await hub.browser.newPage({ viewport: { width: 1280, height: 800 } });
  await cargar(page, `${hub.base}/hub.html?${sufijo}`);
  return page;
}
const NOMBRE = 'Enviar estadísticas de uso';

test('uso: el aviso no se va sin decidir y «Sí» y «No» pesan lo mismo', async () => {
  const page = await abrir();
  const dlg = page.getByRole('dialog');
  await dlg.getByRole('heading', { name: '¿Nos ayudas a mejorar Pronto?' }).waitFor();
  await dlg.getByText('No estamos en tus conversaciones.', { exact: false }).waitFor();
  const si = dlg.getByRole('button', { name: 'Sí, acepto' }), no = dlg.getByRole('button', { name: 'No, gracias' });
  assert.equal(await si.getAttribute('class'), await no.getAttribute('class'));
  await page.keyboard.press('Escape');
  await page.mouse.click(5, 5);
  await page.waitForTimeout(300);
  await dlg.waitFor();
  await page.close();
});

test('uso: «No» deja el interruptor apagado y sin código', async () => {
  const page = await abrir();
  await page.getByRole('button', { name: 'No, gracias' }).click();
  await page.getByRole('dialog').waitFor({ state: 'detached' });
  await page.getByRole('button', { name: 'Configuración' }).first().click();
  await page.getByRole('tab', { name: 'Privacidad y datos' }).click();
  const sw = page.getByRole('switch', { name: NOMBRE });
  assert.equal(await sw.getAttribute('aria-checked'), 'false');
  assert.equal(await page.locator('#uso-id').count(), 0);
  await page.close();
});

test('uso: «Sí» enseña el código en Ajustes y el interruptor lo retira', async () => {
  const page = await abrir();
  await page.getByRole('button', { name: 'Sí, acepto' }).click();
  await page.getByRole('dialog').waitFor({ state: 'detached' });
  await page.getByRole('button', { name: 'Configuración' }).first().click();
  await page.getByRole('tab', { name: 'Privacidad y datos' }).click();
  const sw = page.getByRole('switch', { name: NOMBRE });
  assert.equal(await sw.getAttribute('aria-checked'), 'true');
  await page.waitForFunction(() => document.querySelector('#uso-id')?.textContent);
  await sw.click();
  const aviso = page.getByRole('dialog').filter({ hasText: '¿Dejar de enviar estadísticas?' });
  await aviso.getByText('b7f3c9d2', { exact: false }).waitFor();
  await aviso.getByRole('button', { name: 'Cancelar' }).click();
  await aviso.waitFor({ state: 'detached' });
  assert.equal(await page.locator('#uso-id').count(), 1, 'cancelar no retira nada');
  await sw.click();
  await aviso.getByRole('button', { name: 'Sí, dejar de enviar' }).click();
  await page.locator('#uso-id').waitFor({ state: 'detached' });
  assert.equal(await page.getByRole('switch', { name: NOMBRE }).getAttribute('aria-checked'), 'false');
  await page.close();
});

test('uso: guardar otro ajuste no cambia la decisión (solo `set_usage_stats` la toca)', () => {
  const rs = readFileSync(join(SRC_TAURI, 'src', 'commands.rs'), 'utf8');
  assert.equal(rs.match(/new\.usage_stats = (prev|w)\.usage_stats;/g)?.length, 2, '`apply_settings` tiene que conservar la decisión');
  assert.equal(rs.match(/\.usage_sync\(/g)?.length, 1, 'el código solo se toca desde `set_usage_stats`');
  assert.match(rs.slice(rs.indexOf('pub fn set_usage_stats')), /^[\s\S]*?\.usage_sync\(/);
});

test('uso: sin ?uso la maqueta no pregunta', async () => {
  const page = await abrir('page=home');
  await page.waitForTimeout(300);
  assert.equal(await page.getByRole('dialog').count(), 0);
  await page.close();
});

test('uso: la app sabe a qué web de Umami enviar (si no, el ajuste no haría nada)', () => {
  const rs = readFileSync(join(SRC_TAURI, 'src', 'uso.rs'), 'utf8');
  assert.match(rs, /const WEBSITE_ID: &str = "[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}";/, 'falta el identificador de la app en Umami');
});
