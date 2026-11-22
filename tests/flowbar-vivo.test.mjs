// Texto en vivo (BELTR-PRT-10): mientras se habla, la Flow bar enseña lo fijado y, en gris, lo
// provisional (`dictation://live`); se va al pegar o con un error, y el ajuste «Texto en vivo» del hub
// (encendido de fábrica) lo apaga. Sin Chrome, se salta.
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { abrirHub, cargar, rutaChrome } from './no-se-negocia/_comun.mjs';
import { scriptFlowbarFalso } from '../e2e/rompedor/comun.mjs';

const saltar = !rutaChrome() && 'no hay Chrome (PRIMERIZO_CHROME)';
let hub;
before(async () => { if (!saltar) hub = await abrirHub(); });
after(async () => { if (hub) await hub.cerrar(); });

async function abrir(settings = null) {
  const page = await hub.browser.newPage({ viewport: { width: 360, height: 300 } });
  await page.addInitScript(scriptFlowbarFalso(settings));
  // `cargar` espera al #app del hub, que la barra no tiene: aquí basta con que haya cargado y escuche.
  await page.goto(`${hub.base}/flowbar.html`, { waitUntil: 'load' });
  await page.waitForSelector('#pill', { state: 'attached' });
  await page.waitForTimeout(100);
  return page;
}
const emitir = (page, n, p) => page.evaluate(([a, b]) => window.__fb.emit(a, b), [n, p]);
const visible = (page) => page.$eval('#vivo', (e) => !e.hidden && getComputedStyle(e).display !== 'none');
const texto = (page) => page.$eval('#vivo', (e) => e.textContent);

test('barra: las palabras llegan mientras se habla; lo provisional va en gris', { skip: saltar }, async () => {
  const page = await abrir();
  await emitir(page, 'dictation://live', { fixed: 'antes', partial: 'de dictar' });
  assert.equal(await visible(page), false, 'sin dictado en curso no se enseña nada');

  await emitir(page, 'dictation://state', { state: 'listening' });
  assert.equal(await visible(page), false, 'aún no hay palabras');
  await emitir(page, 'dictation://live', { fixed: '', partial: 'Hola equipo' });
  assert.equal(await visible(page), true);
  assert.equal(await texto(page), 'Hola equipo');
  await emitir(page, 'dictation://live', { fixed: 'Hola equipo.', partial: 'La reunión del jueves' });
  assert.equal(await page.textContent('#vivo-fijo'), 'Hola equipo.');
  assert.equal(await page.textContent('#vivo-prov'), ' La reunión del jueves');
  const color = (sel) => page.$eval(sel, (e) => getComputedStyle(e).color);
  assert.notEqual(await color('#vivo-prov'), await color('#vivo-fijo'), 'lo provisional se distingue de lo fijado');
  assert.equal(await page.$eval('#vivo', (e) => e.getAttribute('aria-hidden')), 'true', 'el lector de pantalla no lee cada tick');
  assert.equal(await page.$eval('#vivo', (e) => getComputedStyle(e).pointerEvents), 'none', 'no tapa clics: la barra es click-through');

  // Al soltar se sigue viendo mientras se transcribe el final; al pegar se va.
  await emitir(page, 'dictation://state', { state: 'processing' });
  assert.equal(await visible(page), true);
  await emitir(page, 'dictation://live', { fixed: 'tarde', partial: '' });
  assert.equal(await page.textContent('#vivo-fijo'), 'Hola equipo.', 'un tick que llega tras soltar no cambia nada');
  await emitir(page, 'dictation://state', { state: 'done' });
  assert.equal(await visible(page), false);

  // El siguiente dictado empieza en blanco.
  await emitir(page, 'dictation://state', { state: 'listening' });
  assert.equal(await visible(page), false);
  await page.close();
});

test('barra: un error lo quita y manda la tarjeta; pasar a manos libres no lo borra', { skip: saltar }, async () => {
  const page = await abrir();
  await emitir(page, 'dictation://state', { state: 'listening' });
  await emitir(page, 'dictation://live', { fixed: 'Uno dos.', partial: 'tres' });
  await emitir(page, 'dictation://state', { state: 'handsfree' });
  assert.equal(await texto(page), 'Uno dos. tres');
  await emitir(page, 'dictation://notice', { title: 'Queda 1 minuto' });
  assert.equal(await visible(page), false, 'la tarjeta va en el mismo sitio: manda ella');
  await emitir(page, 'dictation://state', { state: 'error', message: 'No se pudo transcribir' });
  assert.equal(await visible(page), false);
  await page.close();
});

// Revisión de ea34a16 (hallazgo 2): un Transform emite «polishing» sin pasar por «listening»; la barra
// enseñaba encima el texto del dictado anterior.
test('barra: un Transform no enseña el texto del dictado anterior', { skip: saltar }, async () => {
  const page = await abrir();
  for (const fin of [{ state: 'done' }, { state: 'cancelled' }, { state: 'error', message: 'No se pudo transcribir' }, { state: 'idle' }]) {
    await emitir(page, 'dictation://state', { state: 'listening' });
    await emitir(page, 'dictation://live', { fixed: 'Dictado anterior.', partial: 'sigue' });
    assert.equal(await visible(page), true);
    await emitir(page, 'dictation://state', fin);
    await emitir(page, 'dictation://state', { state: 'polishing' });
    assert.equal(await visible(page), false, `tras «${fin.state}», el Transform enseña el dictado anterior`);
    await emitir(page, 'dictation://state', { state: 'processing' });
    assert.equal(await visible(page), false, `tras «${fin.state}», lo de antes queda en la barra`);
  }
  await page.close();
});

test('barra: lo dictado es texto, nunca HTML', { skip: saltar }, async () => {
  const page = await abrir();
  await emitir(page, 'dictation://state', { state: 'listening' });
  for (const p of [null, {}, { fixed: 1, partial: null }]) await emitir(page, 'dictation://live', p);
  await emitir(page, 'dictation://live', { fixed: '<img src=x onerror=alert(1)>', partial: '<b>hola</b>' });
  assert.equal(await page.$('#vivo img, #vivo b'), null);
  assert.equal(await texto(page), '<img src=x onerror=alert(1)> <b>hola</b>');
  await page.close();
});

test('barra: con «Texto en vivo» apagado no se enseña', { skip: saltar }, async () => {
  const page = await abrir({ showFlowBar: true, appLanguage: 'es', liveText: false });
  await emitir(page, 'dictation://state', { state: 'listening' });
  await emitir(page, 'dictation://live', { fixed: 'Hola.', partial: 'qué tal' });
  assert.equal(await visible(page), false);
  await page.close();
});

test('hub: «Texto en vivo» sale encendido en Configuración y se puede apagar', { skip: saltar }, async () => {
  const page = await hub.browser.newPage({ viewport: { width: 1280, height: 800 } });
  const errores = [];
  page.on('pageerror', (e) => errores.push(e.message));
  await cargar(page, `${hub.base}/hub.html?settings=general`);
  const interruptor = page.getByRole('switch', { name: 'Texto en vivo' });
  await interruptor.waitFor();
  assert.equal(await interruptor.getAttribute('aria-checked'), 'true', 'encendido de fábrica');
  await interruptor.focus();
  await page.keyboard.press('Enter');
  await page.waitForFunction(() => document.querySelector('[data-switch="liveText"]').getAttribute('aria-checked') === 'false');
  // El backend simulado tarda 20-60 ms al azar en cada orden: se espera a que el guardado llegue.
  const guardado = await page.waitForFunction(async () => (await (await import('/hub/api.js')).invoke('get_settings')).liveText === false, null, { timeout: 5_000 })
    .then(() => true, () => false);
  assert.ok(guardado, 'el ajuste se guarda (liveText: false)');
  assert.deepEqual(errores, []);
  await page.close();
});
