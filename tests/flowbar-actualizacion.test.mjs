// Aviso de versión nueva en la Flow bar (PRNT-44): violeta y respirando en reposo, «Actualización lista» al
// pasar el ratón, y nunca mientras se dicta, se descarga o tras «Más tarde».
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { abrirHub, cargar } from './no-se-negocia/_comun.mjs';
import { scriptFlowbarFalso } from '../e2e/rompedor/comun.mjs';

const PENDIENTE = { currentVersion: '2.3.1', state: 'available', dismissed: null, info: { version: '2.4.0' } };

let hub;
before(async () => { hub = await abrirHub(); });
after(async () => { await hub.cerrar(); });

async function abrir(opciones = {}, settings = null) {
  const page = await hub.browser.newPage({ viewport: { width: 360, height: 300 }, ...opciones });
  await page.addInitScript(scriptFlowbarFalso(settings));
  await cargar(page, `${hub.base}/flowbar.html`);
  await page.waitForSelector('#pill', { state: 'attached' });
  return page;
}
const emitir = (page, n, p) => page.evaluate(([a, b]) => window.__fb.emit(a, b), [n, p]);
const aviso = (page) => page.$eval('#hot', (h) => h.dataset.upd);
const animacion = (page) => page.$eval('#pill', (p) => getComputedStyle(p).animationName);

test('barra: sin versión pendiente no cambia nada', async () => {
  const page = await abrir();
  assert.equal(await aviso(page), 'false');
  assert.equal(await animacion(page), 'none');
  assert.equal(await page.$eval('#sat-update', (b) => b.hidden), true);
  await page.close();
});

test('barra: con versión pendiente cambia de color y respira', async () => {
  const page = await abrir();
  const antes = await page.$eval('#pill', (p) => getComputedStyle(p).backgroundColor);
  await emitir(page, 'update://changed', PENDIENTE);
  assert.equal(await aviso(page), 'true');
  assert.equal(await animacion(page), 'respira');
  // El color cambia con una transición: se espera a que cambie (leído al instante fallaba a ratos).
  await page.waitForFunction((a) => getComputedStyle(document.querySelector('#pill')).backgroundColor !== a, antes, { timeout: 2000 }).catch(() => {});
  assert.notEqual(await page.$eval('#pill', (p) => getComputedStyle(p).backgroundColor), antes);
  assert.equal(await page.$eval('#pill', (p) => p.getAttribute('aria-label')), 'Actualización lista: Pronto 2.4.0');
  await page.close();
});

test('barra: al pasar el ratón dice «Actualización lista» y el botón abre Pronto', async () => {
  const page = await abrir();
  await emitir(page, 'update://changed', PENDIENTE);
  await page.hover('#pill', { force: true }); // respira: nunca está «quieta» para Playwright
  await page.waitForFunction(() => document.getElementById('tip').classList.contains('on'));
  assert.equal(await page.textContent('#tip'), 'Actualización lista: Pronto 2.4.0');

  const ultimaZona = await page.evaluate(async () => {
    await new Promise((r) => setTimeout(r, 20));
    return window.__fb.calls.filter((c) => c[0] === 'set_flowbar_hitbox').at(-1)[1].rect;
  });
  const boton = await page.$eval('#sat-update', (b) => { const r = b.getBoundingClientRect(); return { izq: r.left, der: r.right }; });
  assert.ok(ultimaZona.x <= boton.izq && boton.der <= ultimaZona.x + ultimaZona.width, 'el botón entra en la zona pulsable');

  await page.click('#sat-update');
  assert.ok(await page.evaluate(() => window.__fb.calls.some((c) => c[0] === 'open_hub')));
  assert.equal(await page.evaluate(() => window.__fb.calls.some((c) => c[0] === 'start_dictation')), false, 'no empieza a dictar');
  await page.close();
});

test('barra: dictando no respira y al acabar vuelve', async () => {
  const page = await abrir();
  await emitir(page, 'update://changed', PENDIENTE);
  await emitir(page, 'dictation://state', { state: 'listening' });
  assert.equal(await aviso(page), 'false');
  assert.equal(await animacion(page), 'none');
  await emitir(page, 'dictation://state', { state: 'idle' });
  assert.equal(await aviso(page), 'true');
  await page.close();
});

test('barra: «Más tarde», descarga y versión retirada la apagan; un fallo al instalar la enciende', async () => {
  const page = await abrir();
  await emitir(page, 'update://changed', PENDIENTE);
  await emitir(page, 'update://changed', { ...PENDIENTE, dismissed: '2.4.0' });
  assert.equal(await aviso(page), 'false');
  assert.equal(await page.$eval('#sat-update', (b) => b.hidden), true);

  await emitir(page, 'update://changed', PENDIENTE);
  await emitir(page, 'update://progress', { state: 'downloading', doneBytes: 1, totalBytes: 2, percent: 50 });
  assert.equal(await aviso(page), 'false');
  await emitir(page, 'update://progress', { state: 'error', error: 'sin red' });
  assert.equal(await aviso(page), 'true');

  await emitir(page, 'update://changed', { ...PENDIENTE, state: 'upToDate', info: null });
  assert.equal(await aviso(page), 'false');
  await page.close();
});

test('barra: con movimiento reducido se queda violeta, sin respirar', async () => {
  const page = await abrir({ reducedMotion: 'reduce' });
  await emitir(page, 'update://changed', PENDIENTE);
  assert.equal(await aviso(page), 'true');
  assert.equal(await animacion(page), 'none');
  await page.close();
});

test('barra: oculta por el usuario, no se ve el aviso', async () => {
  const page = await abrir({}, { showFlowBar: false, appLanguage: 'es' });
  await emitir(page, 'update://changed', PENDIENTE);
  assert.equal(await page.$eval('#hot', (h) => getComputedStyle(h).visibility), 'hidden');
  await page.close();
});

test('barra: una versión con HTML se pinta como texto y los estados raros no rompen', async () => {
  const page = await abrir();
  for (const p of [null, {}, { info: null }, { state: 'available', info: {} }, { state: '__proto__', info: { version: 'x' } }]) await emitir(page, 'update://changed', p);
  await emitir(page, 'update://changed', { ...PENDIENTE, info: { version: '<img src=x onerror=alert(1)>' } });
  await page.hover('#pill', { force: true }); // respira: nunca está «quieta» para Playwright
  await page.waitForFunction(() => document.getElementById('tip').classList.contains('on'));
  assert.equal(await page.$('#tip img'), null);
  await page.close();
});

test('hub: «Más tarde» avisa al backend, quita la tarjeta y el panel arranca sin errores', async () => {
  const page = await hub.browser.newPage({ viewport: { width: 1280, height: 800 } });
  const errores = [];
  page.on('pageerror', (e) => errores.push(e.message));
  await cargar(page, `${hub.base}/hub.html?update=available`);
  const titulo = page.getByRole('heading', { name: 'Pronto 2.0.2 disponible' });
  await titulo.waitFor();
  await page.getByRole('button', { name: 'Más tarde' }).click();
  await titulo.waitFor({ state: 'hidden' });
  // La tarjeta se va al instante y el aviso al backend llega después: se espera a que quede guardado.
  await page.waitForFunction(async () => (await (await import('/hub/api.js')).invoke('update_status')).dismissed === '2.0.2', null, { timeout: 3000 }).catch(() => {});
  assert.equal(await page.evaluate(async () => (await (await import('/hub/api.js')).invoke('update_status')).dismissed), '2.0.2');
  assert.deepEqual(errores, []);
  await page.close();
});
