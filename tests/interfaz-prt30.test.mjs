// Interfaz (BELTR-PRT-30, contrato en specs/features/prt-30-ningun-dictado-perdido.md), en Chrome headless
// sobre la maqueta (backend simulado), sin Tauri. Sin Chrome, se salta.
// - H5 · Texto en vivo a un clic desde la página Dictado (antes: Configuración → General, 3 clics).
// - H7 · Candado de la Flow bar: el ratón va de la barra al candado a varias velocidades y por varios caminos,
//   con el sondeo del cursor de `flowbar.rs` simulado (hover según el rectángulo pulsable). Meta: 100 %.
// - H6 · Interfaz adaptable: el hub en 5 tamaños de ventana (del mínimo, 900×600, a 1920×1080) y la Flow
//   bar en todos sus estados (con textos largos y escalas de Windows 100-200 %): 0 piezas cortadas, fuera de
//   la ventana o encima de otra.
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { abrirHub, cargar, rutaChrome, PAGINAS, AJUSTES } from './no-se-negocia/_comun.mjs';
import { scriptFlowbarFalso } from '../e2e/rompedor/comun.mjs';

const saltar = !rutaChrome() && 'no hay Chrome (PRIMERIZO_CHROME)';
let hub;
before(async () => { if (!saltar) hub = await abrirHub(); });
after(async () => { if (hub) await hub.cerrar(); });

const cerrarAvisos = async (page) => {
  for (const b of await page.$$('dialog[open] [data-act="close"], dialog[open] .modal-close')) await b.click().catch(() => {});
};

// ── H5 · Texto en vivo ────────────────────────────────────────────────────────────────────────────────
test('texto en vivo: el interruptor está en la página Dictado, a 1 clic, y es el mismo ajuste que en Configuración', { skip: saltar }, async () => {
  const page = await hub.browser.newPage({ viewport: { width: 1180, height: 780 } });
  await cargar(page, `${hub.base}/hub.html`);
  await cerrarAvisos(page);
  assert.equal(await page.evaluate(() => document.querySelector('.nav.active')?.textContent?.trim()), 'Dictado', 'el hub abre en Dictado');
  const sw = page.locator('#app [data-switch=liveText]').first();
  assert.ok(await sw.isVisible(), 'el interruptor del texto en vivo no está a la vista al abrir el hub');
  assert.equal(await sw.getAttribute('aria-checked'), 'true', 'de fábrica está encendido');
  assert.ok((await sw.getAttribute('aria-label'))?.length > 5, 'el interruptor tiene nombre para el lector de pantalla');
  await sw.click(); // el único clic
  await page.waitForTimeout(150);
  assert.equal(await page.locator('#app [data-switch=liveText]').first().getAttribute('aria-checked'), 'false', 'un clic lo apaga');
  assert.match(await page.textContent('#h-live-state'), /Desactivado/);
  // Configuración → General enseña el mismo estado.
  await page.locator('[data-act="settings"]').first().click();
  await page.waitForTimeout(250);
  assert.equal(await page.locator('[role=dialog] [data-switch=liveText], dialog [data-switch=liveText]').first().getAttribute('aria-checked'), 'false', 'Configuración enseña el mismo estado');
  await page.close();
});

test('texto en vivo: con teclado también (foco en el interruptor y Espacio)', { skip: saltar }, async () => {
  const page = await hub.browser.newPage({ viewport: { width: 1180, height: 780 } });
  await cargar(page, `${hub.base}/hub.html`);
  await cerrarAvisos(page);
  await page.locator('#app [data-switch=liveText]').first().focus();
  await page.keyboard.press('Space');
  await page.waitForTimeout(150);
  assert.equal(await page.locator('#app [data-switch=liveText]').first().getAttribute('aria-checked'), 'false');
  await page.close();
});

// ── H7 · Candado ──────────────────────────────────────────────────────────────────────────────────────
const AJUSTES_BARRA = { hotkeyHold: { ctrl: false, alt: true, shift: false, win: true, key: null }, showFlowBar: true, flowBarHiddenUntil: null, flowBarMonitor: null, appLanguage: 'es' };

/** Abre la Flow bar con el backend simulado y el sondeo del cursor de `flowbar.rs`: hover = el ratón está
 *  dentro del último rectángulo pulsable que mandó la barra (`set_flowbar_hitbox`); solo avisa al cambiar. */
async function barra(scale = 1) {
  const ctx = await hub.browser.newContext({ viewport: { width: 360, height: 300 }, deviceScaleFactor: scale });
  const page = await ctx.newPage();
  await page.addInitScript(scriptFlowbarFalso(AJUSTES_BARRA));
  await page.goto(`${hub.base}/flowbar.html`, { waitUntil: 'load' });
  await page.waitForSelector('#pill', { state: 'attached' });
  await page.waitForTimeout(200);
  let dentro = false;
  const sondeo = async (x, y) => {
    const r = await page.evaluate(() => {
      const c = window.__fb.calls.filter((c) => c[0] === 'set_flowbar_hitbox').pop();
      return c ? c[1].rect : null;
    });
    const ahora = !!r && x >= r.x && x <= r.x + r.width && y >= r.y && y <= r.y + r.height;
    if (ahora !== dentro) {
      dentro = ahora;
      await page.evaluate((h) => window.__fb.emit('flowbar://hover', { hover: h }), ahora);
    }
  };
  const mover = async (x, y) => { await page.mouse.move(x, y); await sondeo(x, y); };
  return { page, ctx, mover };
}

const centro = (b) => [b.x + b.width / 2, b.y + b.height / 2];

/** Un intento: entra en la píldora en reposo, espera a que salgan los satélites y va al candado en `pasos`
 *  pasos de `ms`, pasando por `via` (fracción del camino → desplazamiento vertical). ¿Llega y lo pulsa? */
async function intento(b, { pasos, ms, via }) {
  const { page, mover } = b;
  const pill = await page.$eval('#pill', (e) => e.getBoundingClientRect().toJSON());
  const [px, py] = centro(pill);
  await mover(px, 300 - 2);
  await mover(px, py);
  await page.waitForTimeout(250); // la barra crece y salen los satélites
  const lock = await page.$eval('#sat-lock', (e) => e.getBoundingClientRect().toJSON());
  const [lx, ly] = centro(lock);
  for (let i = 1; i <= pasos; i++) {
    const f = i / pasos;
    await mover(px + (lx - px) * f, py + (ly - py) * f + via(f));
    await page.waitForTimeout(ms);
  }
  const visible = await page.evaluate(([x, y]) => {
    const el = document.elementFromPoint(x, y);
    const lock = document.querySelector('#sat-lock');
    return !!el && (el === lock || lock.contains(el)) && getComputedStyle(lock).opacity === '1' && document.querySelector('#hot').dataset.s === 'ready';
  }, [lx, ly]);
  const antes = await page.evaluate(() => window.__fb.calls.filter((c) => c[0] === 'pin_flowbar').length);
  await page.mouse.down();
  await page.mouse.up();
  await page.waitForTimeout(60);
  const pulso = (await page.evaluate(() => window.__fb.calls.filter((c) => c[0] === 'pin_flowbar').length)) > antes;
  // Fuera de la barra para el siguiente intento: vuelve a reposo.
  await mover(20, 20);
  await page.waitForTimeout(250);
  return visible && pulso;
}

const VELOCIDADES = [
  { nombre: 'rápido', pasos: 4, ms: 8 },
  { nombre: 'normal', pasos: 12, ms: 16 },
  { nombre: 'despacio', pasos: 30, ms: 25 },
  { nombre: 'muy despacio', pasos: 50, ms: 40 },
];
const CAMINOS = [
  { nombre: 'recto', via: () => 0 },
  { nombre: 'por arriba', via: (f) => -14 * Math.sin(Math.PI * f) },
  { nombre: 'por abajo', via: (f) => 6 * Math.sin(Math.PI * f) },
];

test('candado: el ratón llega al candado con la barra a la vista en el 100 % de los intentos', { skip: saltar, timeout: 120_000 }, async (t) => {
  const fallos = [];
  let total = 0;
  for (const scale of [1, 1.5]) {
    const b = await barra(scale);
    for (const v of VELOCIDADES) {
      for (const c of CAMINOS) {
        total++;
        if (!(await intento(b, { ...v, via: c.via }))) fallos.push(`${v.nombre}, ${c.nombre}, escala ${scale * 100} %`);
      }
    }
    await b.ctx.close();
  }
  const pct = Math.round(((total - fallos.length) / total) * 100);
  t.diagnostic(`CANDADO ${total - fallos.length}/${total} intentos llegan (${pct} %)`);
  assert.deepEqual(fallos, [], `la barra se escondió antes de llegar al candado (${pct} %):\n  ${fallos.join('\n  ')}`);
});

// ── H6 · Interfaz adaptable ───────────────────────────────────────────────────────────────────────────
/** En la página: piezas cortadas (fuera de la ventana o del contenedor que las recorta), texto que se sale de
 *  su botón y controles encima de otros. Solo mira lo visible. */
function revisarPiezas(raiz) {
  const vw = document.documentElement.clientWidth, vh = document.documentElement.clientHeight;
  const problemas = [];
  const nombre = (el) => (el.id ? `#${el.id}` : el.tagName.toLowerCase() + (el.className && typeof el.className === 'string' ? '.' + el.className.trim().split(/\s+/).slice(0, 2).join('.') : '')) + (el.textContent?.trim() ? ` «${el.textContent.trim().slice(0, 30)}»` : '');
  const visible = (el) => {
    if (!el.getClientRects().length) return false;
    // Dentro de un <details> cerrado no se pinta (Chrome le deja medidas aunque no se vea).
    for (let d = el.parentElement?.closest('details'); d; d = d.parentElement?.closest('details')) {
      if (!d.open && !el.closest('summary')) return false;
    }
    for (let p = el; p; p = p.parentElement) {
      const cs = getComputedStyle(p);
      if (cs.display === 'none' || cs.visibility === 'hidden' || Number(cs.opacity) === 0) return false;
    }
    const b = el.getBoundingClientRect();
    return b.width > 2 && b.height > 2;
  };
  const recorte = (el) => {
    const r = { l: 0, r: vw, t: -Infinity, b: Infinity };
    let vertical = true;
    for (let p = el.parentElement; p && p !== document.body; p = p.parentElement) {
      const cs = getComputedStyle(p);
      if (/(auto|scroll|hidden|clip)/.test(cs.overflowX)) {
        const b = p.getBoundingClientRect();
        r.l = Math.max(r.l, b.left); r.r = Math.min(r.r, b.right);
      }
      // En vertical solo cuenta el primer contenedor: si se desplaza (scroll), lo de abajo está más abajo, no cortado.
      if (/(auto|scroll)/.test(cs.overflowY)) vertical = false;
      if (vertical && /(hidden|clip)/.test(cs.overflowY)) {
        const b = p.getBoundingClientRect();
        r.t = Math.max(r.t, b.top); r.b = Math.min(r.b, b.bottom);
      }
    }
    return r;
  };
  const doc = document.scrollingElement;
  if (doc.scrollWidth > vw + 1) problemas.push(`la página tiene scroll horizontal (${doc.scrollWidth} > ${vw})`);
  const sel = 'button, a[href], input, select, textarea, [role=switch], [role=tab], kbd, h1, h2, h3, .pill, .card, .sat';
  const piezas = [...document.querySelectorAll(raiz.map((r) => `${r} :is(${sel})`).join(','))].filter(visible).filter((el) => !el.closest('.sr, .sr-only, .visually-hidden'));
  for (const el of piezas) {
    const b = el.getBoundingClientRect();
    const c = recorte(el);
    if (b.right > c.r + 1 || b.left < c.l - 1) problemas.push(`${nombre(el)} cortado en horizontal: [${Math.round(b.left)}, ${Math.round(b.right)}] fuera de [${Math.round(c.l)}, ${Math.round(c.r)}]`);
    else if (b.bottom > c.b + 1 || b.top < c.t - 1) problemas.push(`${nombre(el)} cortado en vertical: [${Math.round(b.top)}, ${Math.round(b.bottom)}] fuera de [${Math.round(c.t)}, ${Math.round(c.b)}]`);
    const cs = getComputedStyle(el);
    if ((el.tagName === 'BUTTON' || el.matches('[role=tab]')) && cs.overflowX === 'visible' && cs.whiteSpace !== 'normal' && el.scrollWidth > el.clientWidth + 2) problemas.push(`${nombre(el)}: el texto se sale del botón (${el.scrollWidth} > ${el.clientWidth})`);
  }
  const flota = (el) => {
    for (let p = el; p && p !== document.body; p = p.parentElement) if (/sticky|fixed/.test(getComputedStyle(p).position)) return p;
    return null;
  };
  const controles = piezas.filter((el) => el.matches('button, a[href], input, select, textarea, [role=switch], [role=tab]'));
  for (let i = 0; i < controles.length; i++) {
    for (let j = i + 1; j < controles.length; j++) {
      const [a, b] = [controles[i], controles[j]];
      if (a.contains(b) || b.contains(a)) continue;
      // Una barra que flota a propósito (sticky/fixed) tapa lo que pasa por debajo al desplazar: no es un solape.
      if (flota(a) !== flota(b)) continue;
      const ra = a.getBoundingClientRect(), rb = b.getBoundingClientRect();
      const w = Math.min(ra.right, rb.right) - Math.max(ra.left, rb.left), h = Math.min(ra.bottom, rb.bottom) - Math.max(ra.top, rb.top);
      if (w > 3 && h > 3) {
        // Solo cuenta si de verdad se tapan donde se ve: el de encima en el centro del cruce no es ninguno de los dos (o es el otro).
        const x = Math.max(ra.left, rb.left) + w / 2, y = Math.max(ra.top, rb.top) + h / 2;
        if (x < 0 || y < 0 || x > vw || y > vh) continue;
        problemas.push(`${nombre(a)} y ${nombre(b)} se solapan (${Math.round(w)}×${Math.round(h)} px)`);
      }
    }
  }
  return problemas;
}

const TAMANOS = [[900, 600], [1024, 700], [1180, 780], [1440, 900], [1920, 1080]];

test('hub adaptable: 0 piezas cortadas, fuera de la ventana o solapadas en 5 tamaños (páginas y Configuración)', { skip: saltar, timeout: 15 * 60_000 }, async (t) => {
  const problemas = [];
  let vistas = 0;
  for (const [w, h] of TAMANOS) {
    const page = await hub.browser.newPage({ viewport: { width: w, height: h } });
    const rutas = [...PAGINAS.map((p) => [`page=${p}`, ['#app']]), ...AJUSTES.map((s) => [`settings=${s}`, ['dialog[open]', '[role=dialog]']])];
    for (const [q, raiz] of rutas) {
      await cargar(page, `${hub.base}/hub.html?${q}`);
      if (!q.startsWith('settings')) await cerrarAvisos(page);
      vistas++;
      for (const p of await page.evaluate(revisarPiezas, raiz)) problemas.push(`${w}×${h} ${q}: ${p}`);
    }
    await page.close();
  }
  t.diagnostic(`HUB ${vistas} vistas revisadas · ${problemas.length} problemas`);
  assert.deepEqual(problemas, [], `piezas descolocadas en el hub:\n  ${problemas.slice(0, 40).join('\n  ')}`);
});

const LARGO = 'Esto es un aviso larguísimo que no cabría en una sola línea de la barra, con palabras como transcripción, reintentar e historial.';
const ESTADOS_BARRA = [
  ['reposo', async () => {}],
  ['hover', async (p) => p.evaluate(() => window.__fb.emit('flowbar://hover', { hover: true }))],
  ...['listening', 'handsfree', 'processing', 'polishing', 'polished', 'done', 'cancelled'].map((s) => [s, async (p) => p.evaluate((s) => window.__fb.emit('dictation://state', { state: s }), s)]),
  ['error largo', async (p) => p.evaluate((m) => window.__fb.emit('dictation://state', { state: 'error', message: m }), LARGO)],
  ['aviso largo', async (p) => p.evaluate((m) => window.__fb.emit('dictation://notice', { title: m, body: m, action: 'paste_last' }), LARGO)],
  ['texto en vivo largo', async (p) => p.evaluate((m) => { window.__fb.emit('dictation://state', { state: 'listening' }); window.__fb.emit('dictation://live', { fixed: m + ' ' + m, partial: m }); }, LARGO)],
  ['vista previa larga', async (p) => p.evaluate((m) => { window.__fb.emit('dictation://state', { state: 'preview' }); window.__fb.emit('transform://preview', { text: (m + ' ').repeat(6), name: 'Polish', warning: m }); }, LARGO)],
  ['reunión', async (p) => p.evaluate(() => window.__fb.emit('meeting://state', { state: 'recording' }))],
  ['transcribiendo (tarda)', async (p) => { await p.evaluate(() => window.__fb.emit('dictation://state', { state: 'processing' })); await p.waitForTimeout(1100); }],
];

test('Flow bar adaptable: todos los estados caben en la barra (textos largos, escalas 100-200 %)', { skip: saltar, timeout: 5 * 60_000 }, async (t) => {
  const problemas = [];
  for (const scale of [1, 1.25, 1.5, 2]) {
    for (const [nombre, poner] of ESTADOS_BARRA) {
      const ctx = await hub.browser.newContext({ viewport: { width: 360, height: 300 }, deviceScaleFactor: scale });
      const page = await ctx.newPage();
      await page.addInitScript(scriptFlowbarFalso(AJUSTES_BARRA));
      await page.goto(`${hub.base}/flowbar.html`, { waitUntil: 'load' });
      await page.waitForSelector('#pill', { state: 'attached' });
      await page.waitForTimeout(150);
      await poner(page);
      await page.waitForTimeout(400);
      for (const p of await page.evaluate(revisarPiezas, ['#stage'])) problemas.push(`${scale * 100} % ${nombre}: ${p}`);
      await ctx.close();
    }
  }
  t.diagnostic(`FLOWBAR ${ESTADOS_BARRA.length * 4} vistas revisadas · ${problemas.length} problemas`);
  assert.deepEqual(problemas, [], `piezas descolocadas en la Flow bar:\n  ${problemas.slice(0, 40).join('\n  ')}`);
});

// ── H2 · La Flow bar dice que sigue procesando ────────────────────────────────────────────────────────
test('Flow bar: si el final tarda más de 1 s, la píldora dice «Transcribiendo…» con letras hasta que acaba', { skip: saltar }, async () => {
  const b = await barra(1);
  const { page } = b;
  await page.evaluate(() => window.__fb.emit('dictation://state', { state: 'processing' }));
  await page.waitForTimeout(300);
  assert.equal(await page.$eval('#pill', (e) => e.dataset.s), 'processing', 'al soltar, ondas y rueda como siempre');
  await page.waitForTimeout(900);
  assert.equal(await page.$eval('#pill', (e) => e.dataset.s), 'transcribing');
  const r = await page.evaluate(() => {
    const l = document.querySelector('#label'), p = document.querySelector('#pill');
    const bl = l.getBoundingClientRect(), bp = p.getBoundingClientRect();
    return { texto: l.textContent, visible: getComputedStyle(l).display !== 'none', dentro: bl.left >= bp.left && bl.right <= bp.right + 0.5, aria: p.getAttribute('aria-label') };
  });
  assert.equal(r.texto, 'Transcribiendo…');
  assert.ok(r.visible && r.dentro, `el rótulo no cabe en la píldora: ${JSON.stringify(r)}`);
  assert.match(r.aria, /sigue procesando/);
  await page.evaluate(() => window.__fb.emit('dictation://state', { state: 'done' }));
  await page.waitForTimeout(100);
  assert.equal(await page.$eval('#pill', (e) => e.dataset.s), 'done', 'al acabar, el rótulo se va');
  await b.ctx.close();
});
