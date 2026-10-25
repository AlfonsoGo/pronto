// Sonda: datos hostiles QUE LLEGAN DEL BACKEND. Un __TAURI__ falso envuelve mock.js y añade
// `<img src=x onerror=…><b data-pwn>` a cada texto libre que devuelve un comando o trae un evento
// (transcripciones, apps, palabras, snippets, notas, Transforms, títulos y frases de reunión, notas de la
// release, errores, micros, modelos de Ollama…). Recorre todas las páginas, diálogos y pestañas.
// Si algo se interpreta como HTML aparece un nodo [data-pwn] y, si llega a ejecutarse, window.__pwn.
import { espera, captura } from './comun.mjs';

export const nombre = 'xss-backend';

async function clicSi(page, sel, ms = 450) {
  const el = await page.$(sel);
  if (!el) return false;
  try { await el.click({ timeout: 1500 }); } catch (_) { return false; }
  await espera(ms);
  return true;
}

export async function correr({ abrir }) {
  const out = [];
  const visto = new Map(); // nodo -> dónde
  const textos = new Map(); // dónde -> nº de cargas visibles como TEXTO (prueba de que el veneno llegó a pantalla)
  const anota = async (p, donde) => {
    const r = await p.pwn();
    textos.set(donde, await p.page.evaluate(() => document.body.innerText.split('data-pwn').length - 1));
    // ¿se pidió /x? (un <img src=x> llegó a existir aunque no quede en el DOM ni se ejecutara su onerror)
    const px = await p.page.evaluate(() => performance.getEntriesByType('resource').filter((e) => /\/x$/.test(e.name)).length);
    if (px && !visto.has('PETICION:/x')) visto.set('PETICION:/x', donde);
    for (const n of [...r.nodos, ...r.hits.map((h) => 'EJECUTADO:' + h)]) if (!visto.has(n)) visto.set(n, donde);
  };
  const variantes = [
    '/hub.html?update=available',
    '/hub.html?update=error&model=error&ai=error&ollama=none',
    '/hub.html?engine=error&ollama=nomodel',
  ];
  for (const ruta of variantes) {
    const { page, pwn, cerrar } = await abrir(ruta, { hostil: true });
    page.on('request', (rq) => { if (/\/x$/.test(rq.url()) && !visto.has('PETICION:/x')) visto.set('PETICION:/x', ruta + ' (page.on request)'); });
    const p = { pwn, page };
    await page.waitForSelector('.sb .nav', { timeout: 8000 });
    await espera(800);
    await anota(p, ruta + ' · arranque');
    // eventos hostiles empujados por el «backend»
    await page.evaluate(async () => {
      const m = await window.__hostil.m();
      m.emit('dictation://result', { id: 9001, createdAt: new Date().toISOString(), text: 'dictado nuevo', rawText: 'crudo', app: 'Notepad', durationMs: 1200, words: 2, hasAudio: true });
      m.emit('dictation://state', { state: 'error', kind: 'paste', message: 'mensaje del backend' });
      m.emit('engine://status', { ready: false, loading: false, modelDir: 'C:\\x', error: 'fallo del motor' });
      m.emit('update://available', { version: '9.9.9', tag: 'v9.9.9', title: 'Pronto 9.9.9', notes: 'notas\nlinea2\nlinea3\nlinea4', assetName: 'x.exe', size: 1, pageUrl: 'javascript:__pwnHit("pageUrl")', publishedAt: new Date().toISOString() });
      m.emit('ai://progress', { phase: 'error', received: 0, total: 0, message: 'fallo de la IA' });
      m.emit('model://progress', { file: 'encoder', doneBytes: 1, totalBytes: 2, overallPercent: 50, state: 'error', error: 'fallo de descarga' });
    });
    await espera(900);
    await anota(p, ruta + ' · eventos');
    await clicSi(page, '[data-act=upd-more]');
    await anota(p, ruta + ' · notas de la release desplegadas');

    for (const nav of ['home', 'meetings', 'insights', 'dictionary', 'snippets', 'style', 'transforms', 'notes']) {
      await clicSi(page, `.sb [data-nav=${nav}]`, 900);
      await anota(p, `${ruta} · ${nav}`);
      // abrir el primer elemento editable / detalle de cada página
      for (const sel of ['#page [data-act=open]', '#page .mrow', '#page [data-act=edit]', '#page .vrow', '#page .note-card', '#page [data-act=expand]', '#page .tr-card', '#page [data-tab]:not([aria-selected=true])']) {
        if (await clicSi(page, sel, 700)) {
          await anota(p, `${ruta} · ${nav} · ${sel}`);
          // pestañas internas (resumen / transcripción) y menú «más»
          for (const t of await page.$$('#page [role=tab]')) { try { await t.click({ timeout: 800 }); await espera(400); await anota(p, `${ruta} · ${nav} · pestaña`); } catch (_) { /* */ } }
          await page.keyboard.press('Escape'); await espera(300);
        }
      }
      if (await clicSi(page, '#page [data-act=toggle-search]', 400)) {
        await page.fill('#page [data-search-input]', 'a').catch(() => {});
        await espera(600); await anota(p, `${ruta} · ${nav} · búsqueda`);
      }
    }
    // Configuración: todas las pestañas y los diálogos de micro, idiomas y atajos
    await clicSi(page, '.sb [data-act=settings]', 700);
    for (const tab of await page.$$eval('[data-stab]', (xs) => xs.map((x) => x.dataset.stab))) {
      await clicSi(page, `[data-stab="${tab}"]`, 700);
      await anota(p, `${ruta} · ajustes · ${tab}`);
      for (const b of ['[data-act=ollama-check]', '[data-act=pol-check]', '[data-act=update-check]']) if (await clicSi(page, b, 1200)) await anota(p, `${ruta} · ajustes · ${tab} · ${b}`);
    }
    await page.keyboard.press('Escape'); await espera(300);
    for (const d of ['mic', 'langs', 'shortcuts']) {
      await page.evaluate(async (dd) => { const m = await window.__hostil.m(); m.emit('hub://settings', { tab: dd }); }, d === 'langs' ? 'idiomas' : d);
      await espera(900); await anota(p, `${ruta} · diálogo ${d}`);
      await page.keyboard.press('Escape'); await espera(250); await page.keyboard.press('Escape'); await espera(250);
    }
    const r = await pwn();
    if (r.nodos.length || r.hits.length) await captura(page, `xss-backend-${variantes.indexOf(ruta)}`);
    await cerrar();
  }
  const ejecutados = [...visto.entries()].filter(([n]) => n.startsWith('EJECUTADO:'));
  const nodos = [...visto.entries()].filter(([n]) => !n.startsWith('EJECUTADO:') && !n.startsWith('PETICION:'));
  const pet = [...visto.entries()].filter(([n]) => n.startsWith('PETICION:'));
  if (pet.length) out.push({ grav: 'ALTA', titulo: 'un <img src=x> del backend llegó a crearse (petición /x)', evidencia: pet.map(([n, d]) => `${n} @ ${d}`).join('\n') });
  if (ejecutados.length) out.push({ grav: 'ALTA', titulo: 'JS inyectado desde el backend se EJECUTA', evidencia: ejecutados.map(([n, d]) => `${n} @ ${d}`).join('\n') });
  if (nodos.length) out.push({ grav: 'ALTA', titulo: 'HTML del backend interpretado como marcado', evidencia: nodos.map(([n, d]) => `${n} @ ${d}`).join('\n') });
  // control positivo: el detector ve una inyección real (ROMPEDOR_SIN_CONTROL=1 lo salta)
  if (!process.env.ROMPEDOR_SIN_CONTROL)
  {
    const { page, pwn, cerrar } = await abrir('/hub.html', { hostil: true });
    await page.waitForSelector('.sb .nav', { timeout: 8000 });
    await page.evaluate(() => document.body.insertAdjacentHTML('beforeend', `<img src=x data-pwn="control" onerror="__pwnHit('control')">`));
    await espera(500);
    const r = await pwn();
    out.push({ grav: r.hits.includes('control') ? 'OK' : 'MEDIA', titulo: 'control positivo del detector', evidencia: JSON.stringify(r) });
    await cerrar();
  }
  const conTexto = [...textos.entries()].filter(([, n]) => n > 0);
  out.push({ grav: 'OK', titulo: `veneno visible como texto en ${conTexto.length}/${textos.size} pantallas`, evidencia: `p. ej. ${conTexto.slice(0, 3).map(([d, n]) => `${n}× ${d}`).join(' · ')}` });
  if (!visto.size) out.push({ grav: 'OK', titulo: 'Ningún texto del backend se interpreta como HTML', evidencia: `${variantes.length} variantes, 8 páginas + detalle + búsqueda + 6 pestañas de ajustes + 3 diálogos + 6 eventos envenenados` });
  return out;
}
