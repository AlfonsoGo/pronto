// Sonda: textos hostiles ESCRITOS POR EL USUARIO en cada campo (diccionario, fragmentos, notas, Transforms,
// URL de Ollama, buscadores, título de reunión, «Pregunta a tus reuniones»). Comprueba: nada se interpreta
// como HTML/JS, lo guardado en el backend es exactamente lo escrito (salvo el recorte visible de maxlength),
// y no salta ningún error de JS.
import { HOSTIL, espera, captura } from './comun.mjs';

export const nombre = 'entradas';

const mock = (page, cmd, args) => page.evaluate(async ([c, a]) => (await import('/shared/mock.js')).invoke(c, a), [cmd, args]);

export async function correr({ abrir }) {
  const out = [];
  const nodos = new Set(), ejec = new Set(), integridad = [];
  const { page, pwn, cerrar } = await abrir('/hub.html');
  await page.waitForSelector('.sb .nav');
  const mira = async (donde) => { const r = await pwn(); r.nodos.forEach((n) => nodos.add(`${n} @ ${donde}`)); r.hits.forEach((h) => ejec.add(`${h} @ ${donde}`)); };
  const nav = async (id) => { await page.click(`.sb [data-nav=${id}]`); await espera(500); };

  for (const [k, v] of Object.entries(HOSTIL)) {
    // ---- diccionario
    await nav('dictionary');
    await page.click('#page .ph [data-act=add]'); await page.waitForSelector('#w-word');
    await page.fill('#w-word', v); await page.click('#w-ok'); await espera(400);
    await mira(`diccionario/${k}`);
    const words = await mock(page, 'list_words');
    const w = words.find((x) => x.word === v.trim().slice(0, 120));
    if (!w) integridad.push(`diccionario/${k}: no se guardó tal cual (len ${v.length}) → ${JSON.stringify(words.slice(-1)[0]?.word || '').slice(0, 60)}`);
    if (await page.$('.velo')) { await page.keyboard.press('Escape'); await espera(250); }

    // ---- fragmentos
    await nav('snippets');
    await page.click('#page .ph [data-act=add]'); await page.waitForSelector('#s-trig');
    await page.fill('#s-trig', ('t-' + k + v).slice(0, 120)); await page.fill('#s-exp', v); await page.click('#s-ok'); await espera(400);
    await mira(`fragmentos/${k}`);
    const sn = (await mock(page, 'list_snippets')).find((x) => x.trigger === ('t-' + k + v).slice(0, 120).trim());
    if (!sn) integridad.push(`fragmentos/${k}: no se guardó`);
    else if (sn.expansion !== v.slice(0, 4000)) integridad.push(`fragmentos/${k}: expansión distinta (guardado ${sn.expansion.length} car., escrito ${v.length})`);
    if (await page.$('.velo')) { await page.keyboard.press('Escape'); await espera(250); }

    // ---- notas
    await nav('notes');
    await page.click('#page [data-act=new]'); await page.waitForSelector('#nt-text');
    await page.fill('#nt-text', v); await espera(150);
    await page.click('#page [data-act=back]'); await espera(500);
    await mira(`notas/${k}`);
    const nt = (await mock(page, 'list_notes')).find((x) => x.text === v);
    if (!nt) integridad.push(`notas/${k}: el texto guardado no es el escrito`);

    // ---- Transforms
    await nav('transforms');
    await page.click('#page [data-open=new]'); await page.waitForSelector('#t-name');
    await page.fill('#t-name', v); await page.fill('#t-prompt', v); await page.click('#page [data-act=save]'); await espera(500);
    await mira(`transforms/${k}`);
    const tr = (await mock(page, 'list_transforms')).find((x) => x.name === v.slice(0, 40).trim());
    if (!tr) integridad.push(`transforms/${k}: no se guardó (¿error visible? ${await page.$eval('#t-err', (e) => e.hidden ? 'no' : e.textContent).catch(() => '—')})`);
    else if (tr.prompt !== v.trim()) integridad.push(`transforms/${k}: prompt distinto`);
    if (tr) { await page.click(`#page [data-open="${tr.id}"]`).catch(() => {}); await espera(300); await mira(`transforms/${k}/editor`); await page.click('#page [data-act=back]').catch(() => {}); }

    // ---- buscadores (historial, diccionario, notas, reuniones)
    await nav('home');
    if (await page.$('#page [data-act=toggle-search]')) await page.click('#page [data-act=toggle-search]');
    await page.fill('#page [data-search-input]', v.slice(0, 2000)); await espera(500); await mira(`buscar-historial/${k}`);
    await nav('notes');
    await page.fill('#page [data-search-input]', v.slice(0, 2000)); await espera(300); await mira(`buscar-notas/${k}`);
    await nav('meetings');
    if (await page.$('#page [data-act=toggle-search]')) { await page.click('#page [data-act=toggle-search]'); await page.fill('#page [data-search-input]', v.slice(0, 2000)); await espera(600); await mira(`buscar-reuniones/${k}`); }

    // ---- reunión: título y pregunta
    await nav('meetings');
    const fila = await page.$('#page .mrow');
    if (fila) {
      await fila.click(); await page.waitForSelector('#md-title', { timeout: 3000 }).catch(() => {});
      if (await page.$('#md-title')) {
        await page.fill('#md-title', v); await page.keyboard.press('Enter'); await espera(400);
        await mira(`reunion-titulo/${k}`);
      }
      if (await page.$('#ask-in')) { await page.fill('#ask-in', v); await page.press('#ask-in', 'Enter'); await espera(1500); await mira(`pregunta/${k}`); }
    }
  }

  // ---- URL de Ollama: javascript:, HTML y el «\@» que esquiva el aviso de «sale del PC»
  await page.click('.sb [data-act=settings]'); await espera(400);
  await page.click('[data-stab=experimental]'); await espera(500);
  for (const u of [HOSTIL.js, HOSTIL.img, HOSTIL.largo.slice(0, 5000)]) {
    await page.fill('[data-text=ollamaUrl]', u); await page.press('[data-text=ollamaUrl]', 'Enter'); await espera(500);
    await mira(`ollamaUrl/${u.slice(0, 15)}`);
  }
  const casos = ['http://evil.example\\@localhost:11434', 'http://192.168.1.5:11434', 'http://localhost@evil.example'];
  const avisos = [];
  for (const u of casos) {
    await page.fill('[data-text=ollamaUrl]', u); await page.press('[data-text=ollamaUrl]', 'Enter'); await espera(400);
    const r = await page.evaluate((x) => ({ aviso: !document.querySelector('#oll-remote').hidden, hostReal: new URL(x).hostname }), u);
    avisos.push({ u, ...r });
  }
  const malo = avisos.filter((a) => !a.aviso && !['localhost', '127.0.0.1', '[::1]'].includes(a.hostReal));
  if (malo.length) {
    out.push({ grav: 'MEDIA', titulo: 'URL de Ollama remota SIN el aviso «tus dictados salen del PC» (backslash antes de @)', evidencia: malo.map((a) => `${a.u} → host WHATWG «${a.hostReal}», aviso visible: ${a.aviso}`).join('\n') + '\n' + await captura(page, 'ollama-url-sin-aviso') });
  }
  await page.keyboard.press('Escape');

  if (ejec.size) out.push({ grav: 'ALTA', titulo: 'entrada del usuario EJECUTADA como JS', evidencia: [...ejec].join('\n') });
  if (nodos.size) out.push({ grav: 'ALTA', titulo: 'entrada del usuario interpretada como HTML', evidencia: [...nodos].join('\n') });
  if (!ejec.size && !nodos.size) out.push({ grav: 'OK', titulo: 'ninguna entrada hostil se interpreta como HTML/JS', evidencia: `${Object.keys(HOSTIL).length} cargas × 9 campos` });
  if (integridad.length) out.push({ grav: 'MEDIA', titulo: 'lo guardado no es lo escrito', evidencia: integridad.join('\n') });
  else out.push({ grav: 'OK', titulo: 'lo guardado coincide con lo escrito (NUL, RTL, ZWJ, 100 000 car.)', evidencia: 'diccionario, fragmentos, notas, Transforms' });
  await cerrar();
  return out;
}
