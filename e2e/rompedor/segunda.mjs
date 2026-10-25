// Sonda de la segunda pasada (23-oct): ataca los arreglos del hub de c5527f9 y b4e0d58.
// Notas que se guardan al cerrar (con un guardado en vuelo, tras un fallo, al descargar la página), dobles
// guardados en los editores que quedaban, confirmar antes de descartar (×, Cancelar, cerrar la ventana,
// Transforms), aviso de IA fuera del PC con variantes de URL, resaltado con entradas hostiles en cada buscador,
// «Usar» durante una carga y su temporizador de 5 s, doble «Reintentar transcripción».
// Backend: mock.js envuelto por un __TAURI__ falso SIN veneno (comun.mjs) + un registro con argumentos y
// tiempos (window.__log) y fallos/latencias a demanda (window.__falla, window.__lento).
import { scriptBackendHostil, espera, captura } from './comun.mjs';

export const nombre = 'segunda';

// Registro con argumentos: [t, 'ini'|'fin'|'err', cmd, args]; la ventana cerrada queda como 'ventana.close'.
const REGISTRO = `
  window.__log = []; window.__falla = window.__falla || {};
  const inv0 = window.__TAURI__.core.invoke;
  window.__TAURI__.core.invoke = async (c, a) => {
    const t0 = performance.now();
    window.__log.push([Math.round(t0), 'ini', c, a === undefined ? null : JSON.parse(JSON.stringify(a))]);
    try { navigator.sendBeacon('/__rompedor/' + c, JSON.stringify(a ?? null)); } catch (_) {}
    if (window.__falla[c] > 0) { window.__falla[c]--; await new Promise((r) => setTimeout(r, window.__lento[c] || 0)); window.__log.push([Math.round(performance.now()), 'err', c]); throw 'Base de datos bloqueada (simulado)'; }
    try { const r = await inv0(c, a); window.__log.push([Math.round(performance.now()), 'fin', c]); return r; }
    catch (e) { window.__log.push([Math.round(performance.now()), 'err', c]); throw e; }
  };
  const w = window.__TAURI__.window.getCurrentWindow(); const cl = w.close;
  w.close = async () => { window.__log.push([Math.round(performance.now()), 'ini', 'ventana.close', null]); return cl(); };
`;
const LIMPIO = scriptBackendHostil({ envenenar: false, extra: REGISTRO });
const mock = (page, cmd, args) => page.evaluate(async ([c, a]) => (await import('/shared/mock.js')).invoke(c, a), [cmd, args]);
const log = (page) => page.evaluate(() => window.__log);
const n = (L, cmd) => L.filter((x) => x[1] === 'ini' && x[2] === cmd).length;
const velos = (page) => page.$$eval('.velo:not(.out)', (v) => v.length);
const titulos = (page) => page.$$eval('.velo:not(.out) .modal-hd h2', (v) => v.map((x) => x.textContent));
const toasts = (page) => page.$$eval('.toast', (t) => t.map((x) => x.textContent.trim()));

export async function correr({ abrir }) {
  const out = [];
  const caso = async (titulo, ruta, fn, opts = {}) => {
    const p = await abrir(ruta, { hostil: LIMPIO, ...opts });
    await p.page.route('**/__rompedor/**', (r) => r.fulfill({ status: 204 }));
    await p.page.waitForSelector('.sb .nav', { timeout: 8000 });
    await espera(300);
    try { await fn(p); } catch (e) { out.push({ grav: 'MEDIA', titulo: `${titulo}: la sonda falló`, evidencia: String(e.message || e).split('\n')[0] }); }
    const g = p.guardas();
    if (g.errores.length) out.push({ grav: 'MEDIA', titulo: `${titulo}: error JS sin capturar`, evidencia: g.errores.join('\n'), cubreErrores: true });
    await p.cerrar();
  };

  // ───────────────────────────── A · la nota se guarda al cerrar
  // A1 · Cerrar con el autoguardado anterior aún en vuelo (base ocupada 2,5 s): ¿sale el save_note con lo último antes de cerrar?
  for (const lento of [800, 2500]) {
    await caso(`nota: cerrar con un guardado en vuelo (${lento} ms)`, '/hub.html?page=notes', async ({ page }) => {
      await page.evaluate((ms) => { window.__lento.save_note = ms; }, lento);
      await page.click('#page [data-act=new]'); await page.waitForSelector('#nt-text');
      await page.type('#nt-text', 'Primera idea', { delay: 5 });
      await espera(700);                                 // el autoguardado (600 ms) ya salió y no ha vuelto
      await page.type('#nt-text', ' y la segunda que importa', { delay: 5 });
      await page.click('[data-wc=close]');
      await espera(lento * 2 + 800);
      const L = await log(page);
      const iClose = L.findIndex((x) => x[2] === 'ventana.close');
      const antes = L.slice(0, iClose).filter((x) => x[1] === 'ini' && x[2] === 'save_note').map((x) => x[3].text);
      const ok = antes.includes('Primera idea y la segunda que importa');
      const tClose = L[iClose] ? L[iClose][0] : null;
      const tUlt = (L.find((x) => x[1] === 'ini' && x[2] === 'save_note' && x[3].text === 'Primera idea y la segunda que importa') || [null])[0];
      out.push({ grav: ok ? 'OK' : 'MEDIA', titulo: ok ? `nota: con un guardado en vuelo de ${lento} ms, lo último sale antes de cerrar`
        : `Cerrar el hub con un autoguardado de la nota aún en vuelo (base lenta ${lento} ms) cierra la ventana sin mandar lo último que se escribió`,
      evidencia: `save_note mandados antes de ventana.close: ${JSON.stringify(antes)}; ventana.close a ${tClose} ms, el save_note con el texto completo sale a ${tUlt ?? 'nunca'} ms (después de cerrar: en Tauri la ventana ya no existe). ${ok ? '' : await captura(page, `nota-cerrar-en-vuelo-${lento}`)}` });
    });
  }

  // A2 · El autoguardado falla una vez (base bloqueada) y se cierra: ¿se reintenta al cerrar?
  await caso('nota: guardado fallido + cerrar', '/hub.html?page=notes', async ({ page }) => {
    await page.evaluate(() => { window.__falla.save_note = 1; });
    await page.click('#page [data-act=new]'); await page.waitForSelector('#nt-text');
    await page.type('#nt-text', 'Texto que falló al guardarse', { delay: 5 });
    await espera(1200);
    const estado = await page.$eval('#nt-status', (s) => s.textContent.trim());
    await page.click('[data-wc=close]'); await espera(400);
    const L = await log(page);
    const iClose = L.findIndex((x) => x[2] === 'ventana.close');
    const reintento = L.slice(0, iClose).filter((x) => x[1] === 'ini' && x[2] === 'save_note').length;
    const guardada = (await mock(page, 'list_notes')).some((x) => x.text === 'Texto que falló al guardarse');
    out.push({ grav: reintento >= 2 ? 'OK' : 'BAJA', titulo: reintento >= 2 ? 'nota: tras un guardado fallido, cerrar lo reintenta'
      : 'Si el autoguardado de una nota falla, cerrar el hub no lo reintenta: la nota se pierde con el aviso «No se pudo guardar» ya fuera de la vista',
    evidencia: `estado antes de cerrar «${estado}»; save_note antes de ventana.close: ${reintento} (el fallido); nota en la base: ${guardada}` });
  });

  // A3 · Escribir y descargar la página (Alt+F4 / apagar): ¿sale el save_note antes de que la página muera?
  await caso('nota: descargar la página', '/hub.html?page=notes', async ({ page }) => {
    const beacons = [];
    page.on('request', (r) => { if (r.url().includes('/__rompedor/save_note')) beacons.push(r.postData()); });
    await page.click('#page [data-act=new]'); await page.waitForSelector('#nt-text');
    await page.type('#nt-text', 'Antes del Alt+F4', { delay: 5 });
    await page.evaluate(() => { location.href = '/hub.html?page=home'; });
    await page.waitForSelector('.sb .nav', { timeout: 8000 }); await espera(500);
    const ok = beacons.some((b) => b && b.includes('Antes del Alt+F4'));
    out.push({ grav: ok ? 'OK' : 'MEDIA', titulo: ok ? 'nota: al descargar la página (pagehide) el save_note sale' : 'Al descargar la página (Alt+F4, apagar) la nota a medio escribir no se manda', evidencia: `save_note vistos: ${beacons.length}` });
  });

  // A4 · Papelera con lo último sin guardar y «Cancelar»: ¿queda «Guardando…» para siempre?
  await caso('nota: papelera + cancelar', '/hub.html?page=notes', async ({ page }) => {
    await page.click('#page .nt-card [data-act=open]'); await page.waitForSelector('#nt-text');
    await page.type('#nt-text', ' + añadido', { delay: 5 });
    await page.click('#page [data-act=del-cur]'); await page.waitForSelector('[data-ok]');
    await page.click('.velo .modal-ft .btn-ghost'); await espera(2500);
    const estado = await page.$eval('#nt-status', (s) => s.textContent.trim());
    const L = await log(page);
    out.push({ grav: /Guardando/.test(estado) ? 'BAJA' : 'OK', titulo: /Guardando/.test(estado)
      ? 'Bloc de notas: escribir, pulsar la papelera y cancelar deja «Guardando…» fijo y lo último sin guardar hasta la siguiente tecla'
      : 'nota: papelera + cancelar no deja el estado colgado', evidencia: `estado 2,5 s después: «${estado}»; save_note ${n(L, 'save_note')}×` });
  });

  // ───────────────────────────── B · sin dobles guardados (lo que no cubrió c5527f9)
  await caso('palabra: editar + Enter ×3', '/hub.html?page=dictionary', async ({ page }) => {
    await page.evaluate(() => { window.__lento.update_word = 400; });
    await page.click('#page [data-act=edit]').catch(async () => { await page.click('#page .vrow, #page .v-row'); });
    await page.waitForSelector('#w-word');
    await page.fill('#w-word', 'Txonso2');
    for (let i = 0; i < 3; i++) await page.press('#w-word', 'Enter').catch(() => {});
    await espera(900);
    const L = await log(page);
    out.push({ grav: n(L, 'update_word') > 1 ? 'MEDIA' : 'OK', titulo: `editar palabra + Enter ×3 → update_word ${n(L, 'update_word')}×`, evidencia: '' });
  });
  await caso('fragmento: editar + Ctrl+Enter ×3', '/hub.html?page=snippets', async ({ page }) => {
    await page.evaluate(() => { window.__lento.update_snippet = 400; });
    await page.click('#page [data-act=edit]').catch(() => {});
    await page.waitForSelector('#s-exp', { timeout: 3000 });
    await page.fill('#s-exp', 'nueva expansión');
    for (let i = 0; i < 3; i++) await page.keyboard.press('Control+Enter');
    await espera(900);
    const L = await log(page);
    out.push({ grav: n(L, 'update_snippet') > 1 ? 'MEDIA' : 'OK', titulo: `editar fragmento + Ctrl+Enter ×3 → update_snippet ${n(L, 'update_snippet')}×`, evidencia: '' });
  });
  await caso('reunión: título + Enter ×3', '/hub.html?page=meetings', async ({ page }) => {
    await page.evaluate(() => { window.__lento.rename_meeting = 400; });
    await page.click('#page .mrow'); await page.waitForSelector('#md-title');
    await page.fill('#md-title', 'Título nuevo de verdad');
    for (let i = 0; i < 3; i++) await page.keyboard.press('Enter');
    await page.click('#md-title').catch(() => {}); await page.keyboard.press('Tab');
    await espera(900);
    const L = await log(page);
    out.push({ grav: n(L, 'rename_meeting') > 1 ? 'BAJA' : 'OK', titulo: `título de reunión + Enter ×3 + Tab → rename_meeting ${n(L, 'rename_meeting')}×`, evidencia: '' });
  });
  await caso('preguntar a reuniones: Enter ×3', '/hub.html?page=meetings', async ({ page }) => {
    await page.evaluate(() => { window.__lento.ask_meetings = 600; });
    const inp = await page.$('#ask-in');
    if (!inp) { out.push({ grav: 'OK', titulo: 'preguntar: sin barra (Ollama apagado en el mock)', evidencia: '' }); return; }
    await page.fill('#ask-in', '¿Qué decidimos?');
    for (let i = 0; i < 3; i++) await page.press('#ask-in', 'Enter').catch(() => {});
    await espera(1200);
    const L = await log(page);
    out.push({ grav: n(L, 'ask_meetings') > 1 ? 'BAJA' : 'OK', titulo: `«Pregunta a tus reuniones» + Enter ×3 → ask_meetings ${n(L, 'ask_meetings')}×`, evidencia: '' });
  });
  await caso('borrar historial: doble «Sí»', '/hub.html?settings=data', async ({ page }) => {
    const b = await page.$('[data-act=clear-history]');
    if (!b) { out.push({ grav: 'OK', titulo: 'borrar historial: sin botón', evidencia: '' }); return; }
    await b.click(); await page.waitForSelector('[data-ok]');
    await page.click('[data-ok]', { clickCount: 2 }).catch(() => {}); await espera(500);
    const L = await log(page);
    out.push({ grav: n(L, 'clear_history') > 1 ? 'BAJA' : 'OK', titulo: `«Borrar historial» doble clic en confirmar → clear_history ${n(L, 'clear_history')}×`, evidencia: '' });
  });

  // B · «Reintentar transcripción» dos veces sobre el mismo dictado
  await caso('reintentar ×2', '/hub.html', async ({ page }) => {
    await page.evaluate(() => { window.__lento.retry_history = 2500; });
    await page.waitForSelector('#page .hrow [data-act=more]');
    const fila = await page.$eval('#page .hrow', (r) => r.dataset.id);
    for (let i = 0; i < 2; i++) {
      await page.hover(`#page .hrow[data-id="${fila}"]`); await espera(250);
      await page.click(`#page .hrow[data-id="${fila}"] [data-act=more]`);
      await espera(250);
      const item = await page.$('.menu .m-item:not([disabled]):not(.danger)');
      if (item) await item.click();
      await espera(300);
    }
    const durante = await toasts(page);
    await espera(3500);
    const L = await log(page);
    const ts = await toasts(page);
    out.push({ grav: n(L, 'retry_history') > 1 ? 'BAJA' : 'OK', titulo: n(L, 'retry_history') > 1
      ? `«Reintentar transcripción» se puede lanzar ${n(L, 'retry_history')} veces sobre el mismo dictado mientras el primero sigue en marcha (el menú no lo apaga ni hay indicador)`
      : 'reintentar ×2 → 1 retry_history',
    evidencia: `retry_history ${n(L, 'retry_history')}× sobre id ${fila}; avisos durante: ${JSON.stringify(durante)}; al final: ${JSON.stringify(ts)}` });
  });

  // ───────────────────────────── C · confirmar antes de descartar
  await caso('descartar: velo, Esc, ×, Cancelar', '/hub.html?page=snippets', async ({ page }) => {
    const r = {};
    const abre = async () => {
      await page.click('#page .ph [data-act=add]'); await page.waitForSelector('#s-exp');
      await page.fill('#s-trig', 'borrador'); await page.fill('#s-exp', 'Párrafo largo escrito con calma. '.repeat(40));
    };
    const cierra = async () => { while (await velos(page)) { await page.click('.velo:not(.out):last-of-type .modal-ft .btn-ghost, .velo:not(.out):last-of-type [data-close]').catch(() => {}); await espera(250); const c = await page.$('.velo:not(.out) [data-ok]'); if (c) { await c.click(); await espera(250); } } };
    await abre(); await page.mouse.click(4, 400); await espera(350); r.velo = await titulos(page); await cierra();
    await abre(); await page.keyboard.press('Escape'); await espera(350); r.esc = await titulos(page);
    await page.keyboard.press('Escape'); await espera(350); r.escEsc = { abiertos: await titulos(page), texto: (await page.$eval('#s-exp', (x) => x.value.length).catch(() => 0)) }; await cierra();
    await abre(); await page.click('.velo:not(.out) .modal-hd [data-close]'); await espera(350); r.equis = await titulos(page); await cierra();
    await abre(); await page.click('.velo:not(.out) .modal-ft .btn-ghost'); await espera(350); r.cancelar = await titulos(page); await cierra();
    const pregunta = (x) => Array.isArray(x) && x.some((s) => /Descartar/.test(s));
    out.push({ grav: pregunta(r.velo) && pregunta(r.esc) ? 'OK' : 'MEDIA', titulo: 'editor de fragmento con 1 280 car.: clic en el velo y Esc preguntan', evidencia: JSON.stringify({ velo: r.velo, esc: r.esc, escEsc: r.escEsc }) });
    out.push({ grav: pregunta(r.equis) ? 'OK' : 'BAJA', titulo: pregunta(r.equis) ? '× del editor pregunta' : 'La × del editor de fragmento/palabra cierra y descarta 1 280 caracteres sin preguntar (Esc y el velo sí preguntan)', evidencia: `tras ×: modales abiertos ${JSON.stringify(r.equis)}; tras «Cancelar»: ${JSON.stringify(r.cancelar)}` });
  });

  await caso('descartar: cerrar la ventana con un editor a medias', '/hub.html?page=snippets', async ({ page }) => {
    await page.click('#page .ph [data-act=add]'); await page.waitForSelector('#s-exp');
    await page.fill('#s-trig', 'borrador'); await page.fill('#s-exp', 'Lo que llevo escrito. '.repeat(30));
    // la × de la ventana está tapada por el velo del modal: se llega con el teclado/atajo de Windows (Alt+F4) o con el botón
    const tapada = await page.evaluate(() => { const b = document.querySelector('[data-wc=close]').getBoundingClientRect(); const e = document.elementFromPoint(b.x + b.width / 2, b.y + b.height / 2); return !e.closest('[data-wc=close]'); });
    await page.$eval('[data-wc=close]', (b) => b.click());
    await espera(400);
    const L = await log(page);
    const preg = (await titulos(page)).some((s) => /Descartar/.test(s));
    out.push({ grav: preg || n(L, 'ventana.close') === 0 ? 'OK' : 'BAJA', titulo: preg ? 'cerrar la ventana con un editor a medias pregunta'
      : 'Cerrar la ventana del hub con el editor de un fragmento (o palabra) a medio escribir lo descarta sin preguntar ni guardar',
    evidencia: `× de la ventana tapada por el velo: ${tapada}; ventana.close ${n(L, 'ventana.close')}×, add_snippet ${n(L, 'add_snippet')}×` });
  });

  await caso('descartar: Transforms', '/hub.html?page=transforms', async ({ page }) => {
    const prompt = 'Reescribe el texto como lo haría mi jefe: frases cortas, sin adornos, con una lista de tareas al final. '.repeat(6);
    const res = {};
    for (const via of ['menú lateral', 'Cancelar', '«‹ Transforms»']) {
      await page.click('#page [data-open=new]'); await page.waitForSelector('#t-prompt');
      await page.fill('#t-name', 'Modo jefe'); await page.fill('#t-prompt', prompt);
      if (via === 'menú lateral') { await page.click('.sb [data-nav=home]'); await espera(400); }
      else if (via === 'Cancelar') { await page.click('#page .form-actions [data-act=back]'); await espera(300); }
      else { await page.click('#page > * > .back, #page .back'); await espera(300); }
      res[via] = { pregunta: (await titulos(page)).some((s) => /Descartar/.test(s)), guardado: (await mock(page, 'list_transforms')).some((x) => x.name === 'Modo jefe') };
      // Con el arreglo la pregunta sale: se confirma «Descartar» para volver a la lista (antes se salía sin preguntar).
      while (await velos(page)) { const ok = await page.$('.velo:not(.out) [data-ok]'); if (ok) await ok.click(); else await page.keyboard.press('Escape'); await espera(300); }
      if (via === 'menú lateral') { await page.click('.sb [data-nav=transforms]'); await espera(500); }
    }
    const perdidos = Object.entries(res).filter(([, v]) => !v.pregunta && !v.guardado).map(([k]) => k);
    out.push({ grav: perdidos.length ? 'MEDIA' : 'OK', titulo: perdidos.length
      ? `Editor de Transform: un prompt de ${prompt.length} caracteres se descarta sin preguntar al salir por ${perdidos.join(', ')} (los modales de palabra y fragmento ya preguntan)`
      : 'Transforms pregunta antes de descartar', evidencia: JSON.stringify(res) + (perdidos.length ? ' ' + await captura(page, 'transform-descartado') : '') });
  });

  // ───────────────────────────── D · aviso de IA fuera del PC: variantes de URL
  await caso('aviso URL de Ollama', '/hub.html?settings=experimental', async ({ page }) => {
    const URLS = [
      'http://localhost:11434', 'http://127.0.0.1:11434', 'http://[::1]:11434', 'HTTP://LOCALHOST:11434', 'localhost:11434',
      'http://evil.example\\@localhost:11434', 'http://localhost@evil.example', 'http://localhost:11434@evil.example',
      'http://localhost.evil.example', 'http://evil.example#@localhost', 'http://evil.example?@localhost', 'http://evil.example/@localhost',
      'http:evil.example', 'http:/evil.example', 'http://evil.example\\.localhost', 'http://localhost%2f@evil.example',
      'http://127.0.0.1.nip.io', 'http://0x7f000001:11434', 'http://2130706433', 'http://127.1:11434', 'http://０x7f.0.0.1',
      'http://[::ffff:127.0.0.1]:11434', 'http://127.0.0.2:11434', 'http://localhost.:11434', 'http://0.0.0.0:11434',
      'http://192.168.1.5:11434', 'http://loc\talhost:11434', ' http://localhost:11434 ', 'ws://localhost:11434', 'file:///C:/x',
    ];
    const res = [];
    for (const u of URLS) {
      await page.fill('[data-text=ollamaUrl]', u);
      await page.dispatchEvent('[data-text=ollamaUrl]', 'input');
      const enVivo = await page.$eval('#oll-remote', (x) => !x.hidden).catch(() => null);
      await page.press('[data-text=ollamaUrl]', 'Enter'); await espera(250);
      const aviso = await page.$eval('#oll-remote', (x) => !x.hidden).catch(() => null);
      // destino real (lo que hará ureq/url = WHATWG); sin esquema, ureq no puede ni conectar
      const destino = await page.evaluate((s) => {
        const t = s.trim();
        try { const x = new URL(t); return /^https?:$/.test(x.protocol) ? x.hostname : `(${x.protocol} sin petición)`; } catch (_) { return '(no parsea: sin petición)'; }
      }, u);
      const loop = /^(localhost|127\.\d+\.\d+\.\d+|\[::1\]|\[::ffff:7f[0-9a-f]{2}:[0-9a-f]{1,4}\])$/i.test(destino);
      res.push({ u, enVivo, aviso, destino, loop });
    }
    const falsoLocal = res.filter((r) => !r.aviso && !r.loop && !/sin petición/.test(r.destino));
    const falsoRemoto = res.filter((r) => r.aviso && r.loop);
    const sinVivo = res.filter((r) => r.enVivo !== r.aviso);
    out.push({ grav: falsoLocal.length ? 'MEDIA' : 'OK', titulo: falsoLocal.length ? `URL de Ollama fuera del PC SIN aviso: ${falsoLocal.length}` : `aviso de IA fuera del PC: 0 URLs remotas sin aviso (${URLS.length} variantes)`, evidencia: falsoLocal.map((r) => `${r.u} → ${r.destino}`).join('\n') });
    if (falsoRemoto.length) out.push({ grav: 'BAJA', titulo: `Aviso «tus dictados salen del PC» con direcciones que son de este PC (${falsoRemoto.length}); el backend (is_loopback_host) las da por locales`, evidencia: falsoRemoto.map((r) => `${r.u} → ${r.destino}`).join('\n') });
    if (sinVivo.length) out.push({ grav: 'BAJA', titulo: `El aviso no cambia mientras se escribe la URL, solo al pulsar Enter/cerrar (${sinVivo.length} variantes)`, evidencia: sinVivo.slice(0, 5).map((r) => `${r.u}: escribiendo ${r.enVivo} → tras Enter ${r.aviso}`).join('\n') });
  });

  // D2 · URL remota escrita y Configuración cerrada con Esc (se guarda al cerrar): ¿llega a verse el aviso?
  await caso('aviso URL + Esc', '/hub.html?settings=experimental', async ({ page }) => {
    const vistos = [];
    await page.exposeFunction('__visto', (v) => vistos.push(v));
    await page.evaluate(() => { new MutationObserver(() => { const n = document.querySelector('#oll-remote'); if (n && !n.hidden && n.offsetParent) window.__visto('aviso'); }).observe(document.body, { subtree: true, childList: true, attributes: true }); });
    await page.fill('[data-text=ollamaUrl]', 'http://192.168.1.50:11434');
    await page.keyboard.press('Escape'); await espera(700);
    const guardada = (await mock(page, 'get_settings')).ollamaUrl;
    const ts = await toasts(page);
    const abierto = await velos(page);
    out.push({ grav: vistos.length ? 'OK' : 'MEDIA', titulo: vistos.length ? 'URL remota + Esc: el aviso llega a verse'
      : 'URL de Ollama de otro equipo escrita y Configuración cerrada con Esc (o × o clic fuera): se guarda y el aviso «tus dictados salen del PC» no se llega a ver',
    evidencia: `ollamaUrl guardada «${guardada}»; aviso visible en algún momento: ${vistos.length > 0}; avisos (toast): ${JSON.stringify(ts)}; modales abiertos: ${abierto}` });
  });

  // D3 · Con un modal abierto, la barra de título (mover, minimizar, cerrar) queda tapada por el velo
  await caso('barra de título bajo el velo', '/hub.html?settings=general', async ({ page }) => {
    const r = await page.evaluate(() => ['min', 'max', 'close'].map((k) => { const b = document.querySelector(`[data-wc=${k}]`); if (!b) return [k, 'sin botón']; const x = b.getBoundingClientRect(); const e = document.elementFromPoint(x.x + x.width / 2, x.y + x.height / 2); return [k, e && e.closest(`[data-wc=${k}]`) ? 'alcanzable' : `tapado por .${(e && e.className) || '?'}`]; }));
    const drag = await page.evaluate(() => { const d = document.querySelector('[data-tauri-drag-region]'); if (!d) return 'sin zona de arrastre'; const x = d.getBoundingClientRect(); const e = document.elementFromPoint(x.x + x.width / 2, x.y + 4); return e && e.closest('[data-tauri-drag-region]') ? 'alcanzable' : `tapada por .${(e && e.className) || '?'}`; });
    const tapados = r.filter(([, v]) => v !== 'alcanzable');
    out.push({ grav: tapados.length ? 'BAJA' : 'OK', titulo: tapados.length ? 'Con Configuración (o cualquier modal) abierta, el velo tapa la barra de título: no se puede minimizar, maximizar, cerrar ni mover la ventana con el ratón' : 'barra de título alcanzable con un modal abierto', evidencia: `${JSON.stringify(r)}; zona de arrastre: ${drag} ${tapados.length ? await captura(page, 'velo-tapa-titulo') : ''}` });
  });

  // ───────────────────────────── E · resaltado con entradas hostiles en cada buscador
  {
    const TEXTOS = ['Tom & Jerry <3 "hola" \'x\'', '<img src=x data-pwn="res" onerror="__pwnHit(\'res\')">', 'a.*b (c) [d] \\e $1 $& ^f|g', 'İstanbul ß STRASSE ǅ', '&amp; &lt;b&gt; &#39;'];
    const QS = ['a', '&', '<', '"', 'amp', 'lt', '.*', '(', '\\', '$&', 'İ', 'ss', 'img', 'onerror', '<img', 'x data', 'mark'];
    const sitios = [
      { ruta: '/hub.html?page=notes', sembrar: (t) => ['save_note', { id: null, text: t }], abrirBusca: null, sel: '#page .nt-card .in', inp: '#page [data-search-input]' },
      { ruta: '/hub.html?page=snippets', sembrar: (t) => ['add_snippet', { trigger: t.slice(0, 40), expansion: t }], abrirBusca: '#page [data-act=toggle-search], #page [data-act=search]', sel: '#page .sn', inp: '#page [data-search-input]' },
      { ruta: '/hub.html?page=dictionary', sembrar: (t) => ['add_word', { word: t.slice(0, 60), replacement: null }], abrirBusca: '#page [data-act=toggle-search], #page [data-act=search]', sel: '#page .w-new', inp: '#page [data-search-input]' },
    ];
    for (const s of sitios) {
      await caso(`resaltado ${s.ruta}`, s.ruta, async ({ page }) => {
        for (const t of TEXTOS) { const [c, a] = s.sembrar(t); await mock(page, c, a); }
        await page.click('.sb [data-nav=home]'); await espera(300); await page.click(`.sb [data-nav=${s.ruta.split('=')[1]}]`); await espera(600);
        if (s.abrirBusca) { const b = await page.$(s.abrirBusca); if (b) { await b.click(); await espera(200); } }
        const malos = [];
        for (const q of QS) {
          const i = await page.$(s.inp); if (!i) { malos.push(`sin buscador (${s.inp})`); break; }
          await i.fill(q); await espera(450);
          const r = await page.evaluate((sel) => [...document.querySelectorAll(sel)].map((e) => ({ tx: e.textContent, marks: e.querySelectorAll('mark').length, malHtml: !!e.querySelector('[data-pwn],img,script') })), s.sel);
          const ents = r.filter((x) => /&(amp|lt|gt|quot|#39);/.test(x.tx) && !x.tx.includes('&amp; &lt;b&gt;'));
          if (r.some((x) => x.malHtml)) malos.push(`q=${q}: HTML vivo`);
          if (ents.length) malos.push(`q=${q}: entidad a la vista «${ents[0].tx.slice(0, 60)}»`);
        }
        const pwn = await page.evaluate(() => window.__pwn.length);
        out.push({ grav: malos.length || pwn ? 'MEDIA' : 'OK', titulo: `resaltado en ${s.ruta.split('=')[1]}: ${malos.length || pwn ? 'roto' : 'sin entidades ni HTML'} (${QS.length} consultas × ${TEXTOS.length} textos)`, evidencia: malos.slice(0, 8).join('\n') + (pwn ? ` __pwn=${pwn}` : '') });
      });
    }
  }

  // ───────────────────────────── F · «Usar» durante una carga y el temporizador de 5 s
  await caso('Usar: doble clic y temporizador', '/hub.html?settings=engine', async ({ page }) => {
    await page.waitForFunction(() => document.querySelector('[data-act=use-model]') && !document.querySelector('[data-act=use-model][disabled]'), null, { timeout: 6000 });
    await page.click('[data-act=use-model]', { clickCount: 2 });
    await espera(200);
    const L = await log(page);
    const tras = await page.$$eval('[data-act=use-model]', (b) => b.map((x) => x.disabled));
    // Sin aviso de carga del motor (el mock no lo manda): a los 5 s se vuelve a encender.
    await espera(5400);
    const tras5 = await page.$$eval('[data-act=use-model]', (b) => b.map((x) => x.disabled));
    const resumen = await page.$eval('#asr-sum', (x) => x.innerText.replace(/\s+/g, ' ').slice(0, 120)).catch(() => '');
    out.push({ grav: n(L, 'save_settings') > 1 ? 'MEDIA' : 'OK', titulo: `«Usar» doble clic → save_settings ${n(L, 'save_settings')}×; botones apagados al momento: ${JSON.stringify(tras)}; a los 5,4 s sin aviso del motor: ${JSON.stringify(tras5)}`, evidencia: `resumen: «${resumen}»` });
  });

  await caso('Usar: carga real de 12 s', '/hub.html?settings=engine', async ({ page }) => {
    await page.waitForFunction(() => document.querySelector('[data-act=use-model]:not([disabled])'), null, { timeout: 6000 });
    // Backend como el real: save_settings → engine://status loading:true al instante, ready a los 12 s.
    await page.evaluate(() => {
      const inv = window.__TAURI__.core.invoke;
      window.__TAURI__.core.invoke = async (c, a) => {
        const r = await inv(c, a);
        if (c === 'save_settings') {
          const m = await import('/shared/mock.js');
          m.emit('engine://status', { ready: true, loading: true, active: 'parakeet', modelDir: '', error: null });
          setTimeout(() => m.emit('engine://status', { ready: true, loading: false, active: a.settings.asrEngine === 'whisper' ? 'whisper' : 'parakeet', modelDir: '', error: null }), 12000);
        }
        return r;
      };
    });
    await page.click('[data-act=use-model]');
    const muestras = [];
    for (const t of [500, 4000, 6000, 9000]) { await espera(t - (muestras.length ? [500, 4000, 6000, 9000][muestras.length - 1] : 0)); muestras.push([t, await page.$$eval('[data-act=use-model]', (b) => b.filter((x) => !x.disabled).length)]); }
    // un clic más durante la carga (por si alguno quedó encendido)
    const encendido = await page.$('[data-act=use-model]:not([disabled])');
    if (encendido) await encendido.click();
    await espera(3800);
    const L = await log(page);
    out.push({ grav: muestras.some(([, k]) => k > 0) ? 'MEDIA' : 'OK', titulo: `«Usar» con una carga de 12 s: botones encendidos durante la carga ${JSON.stringify(muestras)}; save_settings ${n(L, 'save_settings')}×`, evidencia: '' });
  });

  await caso('Usar: foco con teclado', '/hub.html?settings=engine', async ({ page }) => {
    await page.waitForFunction(() => document.querySelector('[data-act=use-model]:not([disabled])'), null, { timeout: 6000 });
    await page.focus('[data-act=use-model]:not([disabled])');
    await page.keyboard.press('Enter'); await espera(400);
    const foco = await page.evaluate(() => { const a = document.activeElement; return a === document.body ? 'body' : `${a.tagName.toLowerCase()}.${a.className}`.slice(0, 60) + (a.closest('.velo') ? ' (en el modal)' : ' (fuera del modal)'); });
    await page.keyboard.press('Tab'); await espera(100);
    const trasTab = await page.evaluate(() => { const a = document.activeElement; return `${a.tagName.toLowerCase()} «${(a.getAttribute('aria-label') || a.textContent || '').trim().slice(0, 30)}»`; });
    out.push({ grav: foco === 'body' ? 'BAJA' : 'OK', titulo: foco === 'body' ? 'Pulsar «Usar» con el teclado repinta la lista y el foco cae al <body>: el siguiente Tab salta al principio del modal' : `foco tras «Usar»: ${foco}`, evidencia: `foco tras Enter: ${foco}; tras Tab: ${trasTab}` });
  });

  return out;
}
