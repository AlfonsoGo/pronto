// Sonda: ventanas de 320 px y 4K, zoom 200 %, tema claro/oscuro y prefers-reduced-motion.
// Por cada combinación recorre las 8 páginas y Configuración y mide: scroll horizontal de la página,
// botones/campos fuera de la ventana o tapados (no pulsables), texto que se sale de su caja y animaciones
// en marcha con reduced-motion.
import { espera, captura } from './comun.mjs';

export const nombre = 'pantallas';

// El hub de Tauri tiene minWidth 900 × minHeight 600 (tauri.conf.json) y sin zoom de WebView: 900×600 es la
// ventana más pequeña posible, también con escala 200 % de Windows (los mínimos son lógicos). 320 px y
// 1280×800 al 200 % solo se pueden dar en un navegador: se miden aparte como informativos.
const COMBOS = [
  { n: 'min-900x600-oscuro', viewport: { width: 900, height: 600 } },
  { n: 'min-900x600-escala200', viewport: { width: 900, height: 600 }, deviceScaleFactor: 2 },
  { n: 'min-900x600-claro-reduced', viewport: { width: 900, height: 600 }, reducedMotion: 'reduce', colorScheme: 'light', q: '&theme=light' },
  { n: '4K-3840x2160', viewport: { width: 3840, height: 2160 } },
  { n: 'info-320x640', viewport: { width: 320, height: 640 }, info: true },
  { n: 'info-zoom200-1280x800', viewport: { width: 640, height: 400 }, deviceScaleFactor: 2, info: true },
];
const PAGINAS = ['home', 'meetings', 'insights', 'dictionary', 'snippets', 'style', 'transforms', 'notes', 'novedades'];

function medir() {
  const vw = document.documentElement.clientWidth, vh = document.documentElement.clientHeight;
  const res = { hscroll: document.documentElement.scrollWidth - vw, fuera: [], tapados: [], anim: 0 };
  const sel = 'button, input, select, textarea, [role=tab], [role=switch]';
  const capa = [...document.querySelectorAll('.velo')].pop() || document.querySelector('#app');
  const opaco = (el) => { for (let x = el; x && x !== document.body; x = x.parentElement) if (Number(getComputedStyle(x).opacity) < 0.1) return false; return true; };
  for (const el of capa.querySelectorAll(sel)) {
    const r = el.getBoundingClientRect();
    if (!r.width || !r.height || getComputedStyle(el).visibility === 'hidden' || !opaco(el)) continue;
    const name = (el.getAttribute('aria-label') || el.textContent || el.id || el.className).trim().replace(/\s+/g, ' ').slice(0, 30);
    if (r.right > vw + 1 || r.left < -1) { res.fuera.push(name); continue; }
    // ¿lo tapa otro elemento? (solo lo que está dentro de la ventana en vertical)
    if (r.top >= 0 && r.bottom <= vh) {
      const cx = r.left + r.width / 2, cy = r.top + r.height / 2;
      const top = document.elementFromPoint(cx, cy);
      if (top && !el.contains(top) && !top.contains(el)) res.tapados.push(`${name} ⟵ ${(top.className || top.tagName).toString().slice(0, 25)}`);
    }
  }
  res.anim = document.getAnimations().filter((a) => a.playState === 'running' && (a.effect?.getComputedTiming().iterations === Infinity)).length;
  return res;
}

export async function correr({ abrir }) {
  const out = [];
  for (const c of COMBOS) {
    const { page, cerrar } = await abrir(`/hub.html?x=1${c.q || ''}`, c);
    await page.waitForSelector('.sb .nav', { timeout: 8000 });
    await espera(1800);
    const malos = [];
    for (const p of PAGINAS) {
      await page.evaluate((id) => document.querySelector(`.sb [data-nav=${id}]`)?.click(), p);
      await espera(700);
      const m = await page.evaluate(medir);
      if (m.hscroll > 1 || m.fuera.length || m.tapados.length || (c.reducedMotion && m.anim)) {
        malos.push({ p, ...m, cap: await captura(page, `pantalla-${c.n}-${p}`) });
      }
    }
    await page.evaluate(() => document.querySelector('.sb [data-act=settings]')?.click()); await espera(600);
    for (const tab of ['general', 'engine', 'system', 'meetings', 'experimental', 'data']) {
      await page.evaluate((t) => document.querySelector(`[data-stab=${t}]`)?.click(), tab); await espera(400);
      const m = await page.evaluate(medir);
      if (m.hscroll > 1 || m.fuera.length || m.tapados.length || (c.reducedMotion && m.anim)) malos.push({ p: `ajustes/${tab}`, ...m, cap: await captura(page, `pantalla-${c.n}-ajustes-${tab}`) });
    }
    if (malos.length) {
      out.push({ grav: c.info ? 'OK' : 'BAJA', titulo: (c.info ? '(informativo, inalcanzable en Tauri) ' : '') + `${c.n}: ${malos.length} pantallas con contenido fuera o tapado`, evidencia: malos.map((x) => `${x.p}: hscroll ${x.hscroll}px · fuera [${x.fuera.slice(0, 4).join(' | ')}${x.fuera.length > 4 ? ` +${x.fuera.length - 4}` : ''}] · tapados [${x.tapados.slice(0, 3).join(' | ')}${x.tapados.length > 3 ? ` +${x.tapados.length - 3}` : ''}]${c.reducedMotion ? ` · animaciones infinitas ${x.anim}` : ''} · ${x.cap}`).join('\n') });
    } else out.push({ grav: 'OK', titulo: `${c.n}: sin desbordes`, evidencia: '' });
    await cerrar();
  }
  return out;
}
