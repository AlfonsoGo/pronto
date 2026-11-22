// Interruptor del pulido con IA (BELTR-PRT-17, contrato en specs/features/pulido-en-directo.md):
// - en la página Dictado (la que se abre con el hub), a 1 clic desde que se abre; antes eran 3
//   (Configuración → Limpieza e IA → interruptor);
// - su estado se ve también en la Flow bar (punto violeta y aviso al pasar el ratón).
// Maqueta del hub (backend simulado), sin Tauri. Sin Chrome, se salta.
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { abrirHub, cargar, rutaChrome } from './no-se-negocia/_comun.mjs';
import { scriptFlowbarFalso } from '../e2e/rompedor/comun.mjs';

const saltar = !rutaChrome() && 'no hay Chrome (PRIMERIZO_CHROME)';
let hub;
before(async () => { if (!saltar) hub = await abrirHub(); });
after(async () => { if (hub) await hub.cerrar(); });

/** Clics que hacen falta, desde que se abre el hub, para llegar a un interruptor del pulido visible. */
async function clicsHastaElInterruptor(page) {
  const visible = async () => page.locator('[data-switch=aiPolish]').first().isVisible().catch(() => false);
  if (await visible()) return 1; // el propio clic del interruptor
  return Infinity;
}

test('pulido: el interruptor está en la página Dictado, a 1 clic, y guarda el ajuste', { skip: saltar }, async () => {
  const page = await hub.browser.newPage({ viewport: { width: 1280, height: 800 } });
  await cargar(page, `${hub.base}/hub.html`);
  // Se cierra cualquier aviso de bienvenida («Lo nuevo», estadísticas): no cuenta como clic del interruptor.
  for (const b of await page.$$('dialog[open] [data-act="close"], dialog[open] .modal-close')) await b.click().catch(() => {});
  assert.equal(await page.evaluate(() => document.querySelector('.nav.active')?.textContent?.trim()), 'Dictado', 'el hub abre en Dictado');
  assert.equal(await clicsHastaElInterruptor(page), 1, 'el interruptor del pulido no está a la vista al abrir el hub');

  const sw = page.locator('#app [data-switch=aiPolish]').first();
  assert.equal(await sw.getAttribute('aria-checked'), 'false');
  assert.ok((await sw.getAttribute('aria-label'))?.includes('Pulir'), 'el interruptor tiene nombre para el lector de pantalla');
  await sw.click();
  await page.waitForTimeout(150);
  assert.equal(await page.locator('#app [data-switch=aiPolish]').first().getAttribute('aria-checked'), 'true', 'un clic lo enciende');
  assert.match(await page.textContent('#h-polish-state'), /Activado/);

  // El mismo ajuste en Configuración → Limpieza e IA (siguen siendo el mismo).
  await page.locator('[data-act="settings"]').first().click();
  await page.locator('[role="tab"]', { hasText: 'Limpieza e IA' }).first().click();
  await page.waitForTimeout(150);
  assert.equal(await page.locator('dialog [data-switch=aiPolish], .modal [data-switch=aiPolish], [role=dialog] [data-switch=aiPolish]').first().getAttribute('aria-checked'), 'true', 'Configuración enseña el mismo estado');
  await page.close();
});

test('pulido: con teclado también se enciende (Tab hasta el interruptor y Espacio)', { skip: saltar }, async () => {
  const page = await hub.browser.newPage({ viewport: { width: 1280, height: 800 } });
  await cargar(page, `${hub.base}/hub.html`);
  await page.locator('#app [data-switch=aiPolish]').first().focus();
  await page.keyboard.press('Space');
  await page.waitForTimeout(150);
  assert.equal(await page.locator('#app [data-switch=aiPolish]').first().getAttribute('aria-checked'), 'true');
  await page.close();
});

test('Flow bar: enseña si el pulido está encendido', { skip: saltar }, async () => {
  for (const on of [true, false]) {
    const page = await hub.browser.newPage({ viewport: { width: 360, height: 300 } });
    await page.addInitScript(scriptFlowbarFalso({ hotkeyHold: { ctrl: false, alt: true, shift: false, win: true, key: null }, showFlowBar: true, flowBarHiddenUntil: null, flowBarMonitor: null, appLanguage: 'es', aiPolish: on }));
    await page.goto(`${hub.base}/flowbar.html`, { waitUntil: 'load' });
    await page.waitForSelector('#pill', { state: 'attached' });
    await page.waitForTimeout(150);
    assert.equal(await page.$eval('#hot', (e) => e.dataset.polish), on ? 'on' : 'off');
    const label = await page.$eval('#pill', (e) => e.getAttribute('aria-label'));
    assert.equal(/pulido con IA/.test(label), on, `aria-label de la barra: ${label}`);
    const punto = await page.$eval('#pill', (e) => getComputedStyle(e, '::before').content);
    assert.equal(punto !== 'none' && punto !== 'normal', on, 'el punto del pulido solo sale encendido');
    // Al cambiar el ajuste en el hub, la barra se entera (settings://changed).
    await page.evaluate((v) => window.__fb.emit('settings://changed', { ...window.__fb.S, aiPolish: v }), !on);
    await page.waitForTimeout(50);
    assert.equal(await page.$eval('#hot', (e) => e.dataset.polish), on ? 'off' : 'on');
    await page.close();
  }
});
