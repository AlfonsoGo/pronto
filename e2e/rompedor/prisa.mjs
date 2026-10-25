// Sonda: usuario con prisa. Doble/triple clic y doble Enter en cada botón que escribe (guardar, descargar,
// activar IA, actualizar, borrar), navegar a mitad de una descarga, Configuración abierta y cerrada en bucle,
// Escape/Tab en modales, cambio de idioma a mitad, cerrar el hub justo después de escribir, backend lento.
// El backend es mock.js envuelto por un __TAURI__ falso SIN veneno que apunta cada comando en window.__calls.
import { scriptBackendHostil, espera, captura } from './comun.mjs';

export const nombre = 'prisa';
const LIMPIO = scriptBackendHostil({ envenenar: false });
const mock = (page, cmd, args) => page.evaluate(async ([c, a]) => (await import('/shared/mock.js')).invoke(c, a), [cmd, args]);
const calls = (page, cmd) => page.evaluate((c) => window.__calls.filter((x) => x === c).length, cmd);

export async function correr({ abrir }) {
  const out = [];
  const caso = async (titulo, ruta, fn, opts = {}) => {
    const p = await abrir(ruta, { hostil: LIMPIO, ...opts });
    await p.page.waitForSelector('.sb .nav', { timeout: 8000 });
    await espera(300);
    try { await fn(p); } catch (e) { out.push({ grav: 'MEDIA', titulo: `${titulo}: la sonda falló`, evidencia: String(e.message || e).split('\n')[0] }); }
    const g = p.guardas();
    if (g.errores.length) out.push({ grav: 'MEDIA', titulo: `${titulo}: error JS sin capturar`, evidencia: g.errores.join('\n'), cubreErrores: true });
    await p.cerrar();
  };

  // 1 · Doble Enter en «Agregar palabra» → ¿dos palabras iguales?
  await caso('diccionario doble Enter', '/hub.html?page=dictionary', async ({ page }) => {
    const antes = (await mock(page, 'list_words')).length;
    await page.click('#page .ph [data-act=add]'); await page.waitForSelector('#w-word');
    await page.fill('#w-word', 'Zarandaja');
    await page.press('#w-word', 'Enter'); await page.press('#w-word', 'Enter').catch(() => {}); await page.keyboard.press('Enter');
    await espera(600);
    const ws = (await mock(page, 'list_words')).filter((w) => w.word === 'Zarandaja');
    const n = await calls(page, 'add_word');
    if (ws.length > 1) out.push({ grav: 'MEDIA', titulo: `Doble Enter en «Agregar palabra» guarda la palabra ${ws.length} veces`, evidencia: `add_word llamado ${n}×; list_words: ${antes} → ${antes + ws.length} («Zarandaja» ×${ws.length}). ${await captura(page, 'diccionario-doble-enter')}` });
    else out.push({ grav: 'OK', titulo: 'diccionario doble Enter: 1 palabra', evidencia: `add_word ${n}×` });
  });

  // 2 · Ctrl+Enter doble en «Agregar fragmento»
  await caso('fragmento doble Ctrl+Enter', '/hub.html?page=snippets', async ({ page }) => {
    await page.click('#page .ph [data-act=add]'); await page.waitForSelector('#s-trig');
    await page.fill('#s-trig', 'mi frase rápida'); await page.fill('#s-exp', 'expansión');
    await page.keyboard.press('Control+Enter'); await page.keyboard.press('Control+Enter'); await page.keyboard.press('Control+Enter');
    await espera(600);
    const ss = (await mock(page, 'list_snippets')).filter((s) => s.trigger === 'mi frase rápida');
    if (ss.length > 1) out.push({ grav: 'MEDIA', titulo: `Ctrl+Enter repetido en «Agregar fragmento» lo guarda ${ss.length} veces`, evidencia: `add_snippet ${await calls(page, 'add_snippet')}×; «mi frase rápida» ×${ss.length}` });
    else out.push({ grav: 'OK', titulo: 'fragmento Ctrl+Enter repetido: 1', evidencia: `add_snippet ${await calls(page, 'add_snippet')}×` });
  });

  // 3 · Triple clic en Guardar del Transform y en Agregar palabra (ratón)
  await caso('triple clic guardar', '/hub.html?page=transforms', async ({ page }) => {
    await page.click('#page [data-open=new]'); await page.waitForSelector('#t-name');
    await page.fill('#t-name', 'Triple'); await page.fill('#t-prompt', 'p');
    await page.click('#page [data-act=save]', { clickCount: 3 }).catch(() => {});
    await espera(700);
    const n = (await mock(page, 'list_transforms')).filter((x) => x.name === 'Triple').length;
    out.push({ grav: n > 1 ? 'MEDIA' : 'OK', titulo: `Transform triple clic en Guardar → ${n} guardado(s)`, evidencia: `save_transform ${await calls(page, 'save_transform')}×` });
    await page.click('.sb [data-nav=dictionary]'); await espera(400);
    await page.click('#page .ph [data-act=add]'); await page.waitForSelector('#w-word');
    await page.fill('#w-word', 'Triclic'); await page.click('#w-ok', { clickCount: 3 }).catch(() => {}); await espera(600);
    const m = (await mock(page, 'list_words')).filter((w) => w.word === 'Triclic').length;
    out.push({ grav: m > 1 ? 'MEDIA' : 'OK', titulo: `Agregar palabra triple clic → ${m}`, evidencia: `add_word ${await calls(page, 'add_word')}×` });
  });

  // 4 · Activar IA local: doble clic → ¿dos avisos previos apilados y dos ai_setup_start?
  await caso('activar IA doble clic', '/hub.html?page=transforms&ollama=none', async ({ page }) => {
    await page.waitForSelector('#page [data-act=ai-start]', { timeout: 4000 });
    await page.click('#page [data-act=ai-start]', { clickCount: 2 });
    await espera(700);
    const modales = await page.$$eval('.velo', (v) => v.length);
    let starts = 0;
    for (let i = 0; i < modales; i++) { const ok = await page.$('.velo:last-of-type .modal-ft .btn-primary'); if (ok) { await ok.click(); await espera(400); } }
    starts = await calls(page, 'ai_setup_start');
    if (modales > 1 || starts > 1) out.push({ grav: 'BAJA', titulo: `Doble clic en «Activar IA local» abre ${modales} avisos previos apilados y lanza ai_setup_start ${starts}×`, evidencia: await captura(page, 'ia-doble-aviso') });
    else out.push({ grav: 'OK', titulo: 'activar IA doble clic: 1 aviso', evidencia: `modales ${modales}, ai_setup_start ${starts}×` });
    // navegar a mitad de la activación y volver
    await page.click('.sb [data-nav=home]'); await espera(1500); await page.click('.sb [data-nav=transforms]'); await espera(800);
    const visible = await page.$('#page [data-ai-bar], #page .ai-cta, #page [data-act=ai-cancel]');
    out.push({ grav: visible ? 'OK' : 'BAJA', titulo: 'volver a Transforms durante la activación muestra el progreso', evidencia: visible ? 'sí' : await captura(page, 'ia-progreso-perdido') });
  });

  // 5 · Descargar modelo de voz: triple clic, navegar a mitad, cambiar idioma a mitad
  await caso('descarga modelo', '/hub.html?model=missing', async ({ page }) => {
    await page.waitForSelector('#page [data-act=model-dl]', { timeout: 4000 });
    await page.click('#page [data-act=model-dl]', { clickCount: 3 });
    await espera(800);
    await page.click('.sb [data-nav=insights]'); await espera(800); await page.click('.sb [data-nav=home]'); await espera(500);
    const prog = await page.$eval('#page', (p) => p.innerText.match(/\d+\s?%/)?.[0] || '');
    await page.click('.sb [data-act=settings]'); await espera(400);
    await page.click('[data-stab=general]').catch(() => {});
    const sel = await page.$('[data-select=appLanguage]');
    if (sel) { await sel.selectOption('en'); await espera(800); }
    await page.keyboard.press('Escape'); await espera(300);
    await page.waitForFunction(() => !document.querySelector('#page [data-act=model-dl]') && !/\d+\s?%/.test(document.querySelector('#page')?.innerText || ''), null, { timeout: 20000 }).catch(() => {});
    const n = await calls(page, 'model_download_start');
    const st = await mock(page, 'model_status');
    out.push({ grav: st.complete ? 'OK' : 'MEDIA', titulo: `descarga del modelo con triple clic + navegar + idioma a mitad → ${st.state}`, evidencia: `model_download_start ${n}×, progreso al volver «${prog}», idioma cambiado: ${!!sel}` });
  });

  // 6 · Actualizar ahora: triple clic
  await caso('actualizar triple clic', '/hub.html?update=available', async ({ page }) => {
    await page.waitForSelector('[data-act=upd-install]', { timeout: 4000 });
    await page.click('[data-act=upd-install]', { clickCount: 3 }).catch(() => {});
    await espera(400);
    const n = await calls(page, 'update_install');
    out.push({ grav: n > 1 ? 'BAJA' : 'OK', titulo: `«Actualizar ahora» triple clic → update_install ${n}×`, evidencia: '' });
  });

  // 7 · Configuración abierta/cerrada en bucle (Escape, X, fondo) + Escape/Tab en modal
  await caso('ajustes en bucle', '/hub.html', async ({ page }) => {
    for (let i = 0; i < 40; i++) {
      await page.click('.sb [data-act=settings]');
      const how = i % 3;
      if (how === 0) await page.keyboard.press('Escape');
      else if (how === 1) await page.click('.velo [data-close]').catch(() => page.keyboard.press('Escape'));
      else await page.mouse.click(5, 5);
    }
    await espera(600);
    const velos = await page.$$eval('.velo', (v) => v.length);
    await page.click('.sb [data-act=settings]'); await espera(300);
    for (const t of ['engine', 'system', 'data', 'general']) await page.click(`[data-stab=${t}]`);
    const tabs = await page.$$eval('[data-stab]', (v) => v.length);
    // Tab x 40 dentro del modal: ¿se escapa el foco?
    let fuera = 0;
    for (let i = 0; i < 40; i++) { await page.keyboard.press(i % 7 === 6 ? 'Shift+Tab' : 'Tab'); if (!(await page.evaluate(() => !!document.activeElement.closest('.velo')))) fuera++; }
    await page.keyboard.press('Escape'); await espera(300);
    const quedan = await page.$$eval('.velo', (v) => v.length);
    out.push({ grav: velos || quedan || fuera ? 'MEDIA' : 'OK', titulo: 'Configuración 40× abrir/cerrar + 40 Tab', evidencia: `velos colgados ${velos}/${quedan}, pestañas ${tabs}, foco fuera del modal ${fuera}/40` });
  });

  // 8 · Escribir una nota y cerrar el hub al instante (botón × de la ventana)
  await caso('nota + cerrar', '/hub.html?page=notes', async ({ page }) => {
    await page.click('#page [data-act=new]'); await page.waitForSelector('#nt-text');
    await page.type('#nt-text', 'Idea que no quiero perder', { delay: 5 });
    await page.click('[data-wc=close]');
    await espera(50);
    const orden = await page.evaluate(() => window.__calls.filter((c) => c === 'save_note' || c === 'ventana.close'));
    const antesDeCerrar = orden.indexOf('ventana.close') >= 0 && !orden.slice(0, orden.indexOf('ventana.close')).includes('save_note');
    if (antesDeCerrar) out.push({ grav: 'ALTA', titulo: 'Cerrar el hub justo después de escribir en una nota pierde lo escrito', evidencia: `orden de llamadas: ${JSON.stringify(orden)} (ventana.close sin save_note antes; el autoguardado espera 600 ms y la ventana se destruye) ${await captura(page, 'nota-cerrar')}` });
    else out.push({ grav: 'OK', titulo: 'nota guardada antes de cerrar', evidencia: JSON.stringify(orden) });
  });

  // 9 · Nota nueva con backend lento: Atrás mientras el primer guardado está en vuelo → ¿dos notas?
  await caso('nota backend lento', '/hub.html?page=notes', async ({ page }) => {
    await page.evaluate(() => { window.__lento.save_note = 1500; });
    const antes = (await mock(page, 'list_notes')).length;
    await page.click('#page [data-act=new]'); await page.waitForSelector('#nt-text');
    await page.type('#nt-text', 'Nota única', { delay: 5 });
    await espera(750);            // el autoguardado ya salió y aún no ha vuelto
    await page.click('#page [data-act=back]');
    await espera(3500);
    const ns = (await mock(page, 'list_notes')).filter((n) => n.text === 'Nota única');
    if (ns.length > 1) out.push({ grav: 'MEDIA', titulo: `Nota nueva + «Atrás» con el guardado en vuelo crea ${ns.length} notas iguales`, evidencia: `save_note ${await calls(page, 'save_note')}×; notas ${antes} → ${antes + ns.length}. ${await captura(page, 'nota-duplicada')}` });
    else out.push({ grav: 'OK', titulo: 'nota con backend lento: 1', evidencia: `${ns.length}` });
  });

  // 10 · Título de reunión: escribir y que llegue un evento de reunión (p. ej. Teams detectado)
  await caso('titulo reunion + evento', '/hub.html?page=meetings', async ({ page }) => {
    const res = [];
    for (let i = 0; i < 5; i++) {
      const txt = `Título que estoy escribiendo ${i}`;
      await page.click('#page .mrow'); await page.waitForSelector('#md-title');
      await page.click('#md-title'); await page.fill('#md-title', txt);
      await page.evaluate(async () => { const m = await import('/shared/mock.js'); await m.invoke('start_meeting', { title: null }).catch(() => {}); });
      await espera(1200);
      const val = await page.$eval('#md-title', (x) => x.value).catch(() => '(sin campo)');
      const guardado = (await mock(page, 'list_meetings', { query: null, limit: 100, offset: 0 })).some((m) => m.title === txt);
      res.push({ val, pisado: val !== txt, guardado });
      if (val !== txt && !res.cap) res.cap = await captura(page, 'titulo-reunion-pisado');
      await page.evaluate(async () => { const m = await import('/shared/mock.js'); await m.invoke('stop_meeting').catch(() => {}); });
      await espera(300);
      await page.click('#page [data-act=back]').catch(() => {}); await espera(500);
    }
    const pis = res.filter((r) => r.pisado);
    const perdido = pis.filter((r) => !r.guardado);
    if (perdido.length) out.push({ grav: 'ALTA', titulo: `Título de reunión a medio escribir perdido por un evento de reunión (${perdido.length}/5)`, evidencia: `${JSON.stringify(res)} ${res.cap || ''}` });
    else out.push({ grav: 'OK', titulo: `título de reunión + evento «empieza otra reunión»: la vista salta a la nueva en ${pis.length}/5, pero lo escrito se guarda (focusout al quitar el campo)`, evidencia: JSON.stringify(res) });
  });

  // 11 · Título de reunión + «Atrás» al momento
  await caso('titulo reunion + atras', '/hub.html?page=meetings', async ({ page }) => {
    await page.evaluate(() => { window.__lento.rename_meeting = 400; });
    await page.click('#page .mrow'); await page.waitForSelector('#md-title');
    await page.fill('#md-title', 'Nuevo título');
    await page.click('#page [data-act=back]');
    await espera(1200);
    const lista = await page.$eval('#page', (p) => p.innerText.includes('Nuevo título'));
    const guardado = (await mock(page, 'list_meetings', { query: null, limit: 100, offset: 0 })).some((m) => m.title === 'Nuevo título');
    out.push({ grav: 'OK', titulo: 'título + Atrás (backend 400 ms)', evidencia: `guardado en backend: ${guardado}; la lista muestra el nuevo título: ${lista}` });
  });

  // 12 · Escape / clic fuera con un fragmento largo a medio escribir
  await caso('escape fragmento', '/hub.html?page=snippets', async ({ page }) => {
    await page.click('#page .ph [data-act=add]'); await page.waitForSelector('#s-exp');
    await page.fill('#s-trig', 'borrador'); await page.fill('#s-exp', 'Párrafo largo escrito con calma. '.repeat(40));
    await page.mouse.click(4, 400);  // clic en el fondo oscuro
    await espera(400);
    const abierto = !!(await page.$('#s-exp'));
    if (!abierto) out.push({ grav: 'MEDIA', titulo: 'Un clic fuera (o Esc) cierra el editor de fragmento/palabra/atajos y descarta lo escrito sin preguntar', evidencia: `1 280 caracteres escritos en «Expansión» → clic en el velo → modal cerrado, nada guardado (add_snippet ${await calls(page, 'add_snippet')}×)` });
  });

  // 13 · Borrar: doble clic en «Sí, eliminar» y en la papelera
  await caso('borrar doble', '/hub.html?page=notes', async ({ page }) => {
    await page.click('#page .nt-card [data-act=del]'); await page.waitForSelector('[data-ok]');
    await page.click('[data-ok]', { clickCount: 2 }).catch(() => {});
    await espera(500);
    const n = await calls(page, 'delete_note');
    out.push({ grav: n > 1 ? 'BAJA' : 'OK', titulo: `«Sí, eliminar» doble clic → delete_note ${n}×`, evidencia: '' });
  });

  // 14 · Audio corrupto del backend (WAV dañado) al pulsar reproducir
  await caso('audio corrupto', '/hub.html', async ({ page }) => {
    await page.evaluate(() => {
      const inv = window.__TAURI__.core.invoke;
      window.__TAURI__.core.invoke = (c, a) => (c === 'get_history_audio' ? Promise.resolve('!!no-es-base64!!') : inv(c, a));
    });
    const b = await page.$('#page [data-act=play]:not([disabled])');
    if (b) { await b.evaluate((x) => x.click()); await espera(1500); }
    const toastTx = await page.$$eval('.toast', (t) => t.map((x) => x.textContent).join(' | '));
    out.push({ grav: 'OK', titulo: 'audio corrupto', evidencia: `toast: ${toastTx || '(ninguno)'}` });
  });

  // 15 · Configuración: escribir la URL/modelo de Ollama y cerrar con Esc / X / clic fuera sin pulsar Enter
  await caso('ajuste de texto + cerrar', '/hub.html', async ({ page }) => {
    const res = [];
    for (const [i, how] of ['Escape', 'X', 'fondo'].entries()) {
      await page.click('.sb [data-act=settings]'); await espera(300);
      await page.click('[data-stab=experimental]'); await espera(300);
      const url = `http://127.0.0.1:1143${i}`;
      await page.fill('[data-text=ollamaUrl]', url);
      if (how === 'Escape') await page.keyboard.press('Escape');
      else if (how === 'X') await page.click('.velo [data-close]');
      else await page.mouse.click(3, 3);
      await espera(500);
      res.push({ how, guardado: (await mock(page, 'get_settings')).ollamaUrl === url });
    }
    const perdidos = res.filter((r) => !r.guardado);
    out.push({ grav: perdidos.length ? 'MEDIA' : 'OK', titulo: `URL de Ollama escrita y Configuración cerrada sin Enter: ${perdidos.length ? 'se pierde con ' + perdidos.map((r) => r.how).join(', ') : 'se guarda siempre'}`, evidencia: JSON.stringify(res) });
  });

  {
    const { res, cap } = await resaltado({ abrir });
    const roto = Object.entries(res).filter(([, v]) => v !== 'Tom & Jerry <3 "hola"');
    out.push({ grav: roto.length ? 'BAJA' : 'OK', titulo: 'buscar una letra que aparece en una entidad (a→&amp;, l→&lt;, o→&quot;) rompe la entidad y la tarjeta muestra «&amp;» literal', evidencia: `${JSON.stringify(res)} ${cap}` });
  }
  return out;
}

// 16 · (añadido) resaltado de la búsqueda sobre texto ya escapado: buscar «amp» / «lt» / «quot» en una nota con & < "
export async function resaltado({ abrir }) {
  const p = await abrir('/hub.html?page=notes', { hostil: LIMPIO });
  await p.page.waitForSelector('.sb .nav');
  await mock(p.page, 'save_note', { id: null, text: 'Tom & Jerry <3 "hola"' });
  await p.page.click('.sb [data-nav=home]'); await espera(300); await p.page.click('.sb [data-nav=notes]'); await espera(500);
  const res = {};
  for (const q of ['a', 'l', 'o']) {
    await p.page.fill('#page [data-search-input]', q); await espera(300);
    res[q] = await p.page.$eval('#page .nt-card h3', (h) => h.textContent).catch(() => '(sin tarjeta)');
  }
  const cap = await captura(p.page, 'resaltado-entidades');
  await p.cerrar();
  return { res, cap };
}
