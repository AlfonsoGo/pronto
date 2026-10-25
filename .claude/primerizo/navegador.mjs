#!/usr/bin/env node
// Navegador del agente `primerizo`: la ÚNICA forma que tiene de tocar la app.
// Mantiene un Chromium headless vivo entre llamadas (CDP en 127.0.0.1) para que el
// primerizo navegue como un cliente: cada orden actúa, saca captura y describe la pantalla.
//
//   node .claude/primerizo/navegador.mjs abrir <url>          (solo localhost/127.0.0.1)
//   node .claude/primerizo/navegador.mjs clic "<texto visible>"
//   node .claude/primerizo/navegador.mjs escribir "<etiqueta o placeholder>" "<texto>"
//   node .claude/primerizo/navegador.mjs tecla <Enter|Tab|Escape|...>
//   node .claude/primerizo/navegador.mjs atras | ver | cerrar
//
// Requiere `playwright-core` (o `playwright` / `@playwright/test`) en el proyecto y un navegador: `PRIMERIZO_CHROME`, el
// Chromium de Playwright si está instalado, o el Chrome / Edge del sistema (ver `rutaNavegador`).
import { spawn } from 'node:child_process';
import { rutaNavegador } from './ruta-navegador.mjs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const PUERTO = Number(process.env.PRIMERIZO_PUERTO ?? 9333);
const CDP = `http://127.0.0.1:${PUERTO}`;
const [, , orden, ...args] = process.argv;

function cargarPlaywright() {
  const req = createRequire(join(process.cwd(), 'package.json'));
  for (const nombre of ['playwright-core', 'playwright', '@playwright/test']) {
    try { return req(nombre).chromium; } catch {}
  }
  salir('Falta Playwright en el proyecto: npm i -D playwright-core && npx playwright install chromium');
}

function salir(mensaje, codigo = 1) { console.error(mensaje); process.exit(codigo); }
const esperar = (ms) => new Promise((r) => setTimeout(r, ms));
const vivo = () => fetch(`${CDP}/json/version`).then((r) => r.ok, () => false);

async function conectar(chromium, arrancarSiHaceFalta) {
  if (!(await vivo())) {
    if (!arrancarSiHaceFalta) salir('No hay navegador abierto. Empieza con: abrir http://localhost:3000');
    // PRIMERIZO_CHROME: ruta a otro Chromium/Chrome si el de esta versión de Playwright no está instalado.
    const exe = rutaNavegador(chromium);
    if (!exe) salir('No encuentro ningún navegador: instala Chrome, o `npx playwright install chromium`, o define PRIMERIZO_CHROME.');
    spawn(exe, [
      '--headless=new', `--remote-debugging-port=${PUERTO}`, '--remote-debugging-address=127.0.0.1',
      `--user-data-dir=${join(tmpdir(), 'primerizo-perfil')}`, '--window-size=1366,900', 'about:blank',
    ], { detached: true, stdio: 'ignore' }).unref();
    for (let i = 0; i < 50 && !(await vivo()); i++) await esperar(200);
  }
  const navegador = await chromium.connectOverCDP(CDP);
  const contexto = navegador.contexts()[0] ?? (await navegador.newContext());
  const paginas = contexto.pages();
  return paginas.at(-1) ?? (await contexto.newPage());
}

function soloLocal(url) {
  let u;
  try { u = new URL(url); } catch { salir(`URL no válida: ${url}`); }
  if (!['localhost', '127.0.0.1', '[::1]'].includes(u.hostname)) salir('Solo se permite la app en localhost.');
  return u.href;
}

async function localizar(pagina, texto) {
  const candidatos = [
    pagina.getByRole('button', { name: texto }), pagina.getByRole('link', { name: texto }),
    pagina.getByLabel(texto), pagina.getByPlaceholder(texto), pagina.getByText(texto),
  ];
  for (const c of candidatos) {
    const visible = c.filter({ visible: true }).first();
    if (await visible.count()) return visible;
  }
  salir(`No veo nada en pantalla que diga «${texto}». Eso también es un hallazgo: anótalo.`, 3);
}

async function describir(pagina) {
  await pagina.waitForLoadState('domcontentloaded').catch(() => {});
  await esperar(400);
  const captura = join(tmpdir(), `primerizo-${Date.now()}.png`);
  await pagina.screenshot({ path: captura, fullPage: true });
  const datos = await pagina.evaluate(() => {
    const visible = (el) => !!(el.offsetWidth || el.offsetHeight || el.getClientRects().length);
    const nombre = (el) => (el.getAttribute('aria-label') || el.innerText || el.value || el.placeholder || '').trim().replace(/\s+/g, ' ').slice(0, 80);
    const etiqueta = (el) => (el.labels?.[0]?.innerText || el.getAttribute('aria-label') || el.placeholder || el.name || '').trim();
    return {
      texto: document.body.innerText.replace(/\n{3,}/g, '\n\n').slice(0, 4000),
      botones: [...document.querySelectorAll('button, [role=button], input[type=submit]')].filter(visible).map(nombre).filter(Boolean),
      enlaces: [...document.querySelectorAll('a[href]')].filter(visible).map(nombre).filter(Boolean),
      campos: [...document.querySelectorAll('input:not([type=hidden]):not([type=submit]), textarea, select')].filter(visible).map((el) => `${etiqueta(el) || '(sin etiqueta)'} [${el.type || el.tagName.toLowerCase()}]`),
    };
  });
  console.log([
    `URL: ${pagina.url()}`, `Título: ${await pagina.title()}`, `Captura: ${captura}`,
    `Botones: ${datos.botones.join(' · ') || '(ninguno)'}`,
    `Enlaces: ${datos.enlaces.join(' · ') || '(ninguno)'}`,
    `Campos: ${datos.campos.join(' · ') || '(ninguno)'}`,
    '--- Texto visible ---', datos.texto,
  ].join('\n'));
}

const chromium = cargarPlaywright();
if (orden === 'cerrar') {
  if (await vivo()) { const n = await chromium.connectOverCDP(CDP); await n.newBrowserCDPSession().then((s) => s.send('Browser.close')).catch(() => {}); }
  console.log('Navegador cerrado.');
  process.exit(0);
}
if (!['abrir', 'clic', 'escribir', 'tecla', 'atras', 'ver'].includes(orden)) salir('Órdenes: abrir <url> · clic "<texto>" · escribir "<campo>" "<texto>" · tecla <tecla> · atras · ver · cerrar');

const pagina = await conectar(chromium, orden === 'abrir');
try {
  if (orden === 'abrir') await pagina.goto(soloLocal(args[0] ?? ''), { timeout: 20000 });
  if (orden === 'clic') await (await localizar(pagina, args[0] ?? '')).click({ timeout: 8000 });
  if (orden === 'escribir') await (await localizar(pagina, args[0] ?? '')).fill(args[1] ?? '', { timeout: 8000 });
  if (orden === 'tecla') await pagina.keyboard.press(args[0] ?? 'Enter');
  if (orden === 'atras') await pagina.goBack({ timeout: 8000 });
  if (!pagina.url().startsWith('about:')) soloLocal(pagina.url());
} catch (e) {
  console.log(`⚠ La acción falló: ${e.message.split('\n')[0]} — anótalo como hallazgo si un cliente se toparía con ello.`);
}
await describir(pagina);
process.exit(0);
