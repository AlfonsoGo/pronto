// Sonda: textos sin traducir al inglés. Dos vías:
//  1) estática: cada t('literal') del hub (ui/hub.js, ui/hub/**) que no está en ui/hub/en.js y parece español;
//  2) en vivo: cada página y diálogo con ?lang=en, texto visible con palabras o letras de español.
import { readdir, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { AQUI, espera, captura } from './comun.mjs';

export const nombre = 'ingles';
const UI = join(AQUI, '..', '..', 'ui');
const ES = /[áéíóúñ¿¡]|\b(de|que|el|la|los|las|para|con|una|un|por|sin|tus|tu|está|aquí|nada|guardar|cancelar|eliminar|reunión|dictado|nota|notas)\b/i;

async function ficheros(dir) {
  const out = [];
  for (const e of await readdir(dir, { withFileTypes: true })) {
    const p = join(dir, e.name);
    if (e.isDirectory()) out.push(...await ficheros(p)); else if (e.name.endsWith('.js') && e.name !== 'en.js') out.push(p);
  }
  return out;
}

export async function correr({ abrir }) {
  const out = [];
  const EN = (await import(pathToFileURL(join(UI, 'hub', 'en.js')).href)).default;
  const src = [join(UI, 'hub.js'), ...await ficheros(join(UI, 'hub'))];
  const faltan = new Map();
  for (const f of src) {
    const txt = await readFile(f, 'utf8');
    const lineas = txt.split('\n');
    lineas.forEach((l, i) => {
      for (const m of l.matchAll(/\bt\(\s*(['"])((?:\.|(?!\1).)*)\1/g)) {
        const k = m[2].replace(/\'/g, "'").replace(/\\"/g, '"').replace(/\n/g, '\n');
        if (!(k in EN) && ES.test(k)) faltan.set(k, `${f.slice(UI.length - 2).split(String.fromCharCode(92)).join('/')}:${i + 1}`);
      }
    });
  }
  out.push({ grav: faltan.size ? 'BAJA' : 'OK', titulo: `t('…') sin entrada en en.js: ${faltan.size}`, evidencia: [...faltan].map(([k, w]) => `${w}  «${k.slice(0, 90)}»`).join('\n') });

  // en vivo
  const rutas = ['home', 'insights', 'dictionary', 'snippets', 'style', 'notes', 'transforms', 'meetings'].map((p) => `/hub.html?lang=en&page=${p}`)
    .concat(['general', 'system', 'engine', 'experimental', 'data', 'meetings', 'about'].map((s) => `/hub.html?lang=en&settings=${s}`))
    .concat(['/hub.html?lang=en&model=missing', '/hub.html?lang=en&update=available', '/hub.html?lang=en&engine=error&ollama=none&page=transforms', '/hub.html?lang=en&empty=1&page=meetings']);
  const vivos = new Map();
  for (const r of rutas) {
    const p = await abrir(r);
    await p.page.waitForSelector('.sb .nav', { timeout: 8000 }).catch(() => {});
    await espera(900);
    const tx = await p.page.evaluate(() => {
      const w = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
      const o = []; let n;
      while ((n = w.nextNode())) { const s = n.textContent.trim(); const e = n.parentElement; if (s && e && e.offsetParent !== null && !e.closest('textarea,input,.h-text,.nt-card,.mrow,.tx,.md-body,kbd,code')) o.push(s); }
      document.querySelectorAll('[aria-label],[title],[placeholder]').forEach((e) => ['aria-label', 'title', 'placeholder'].forEach((a) => { const v = e.getAttribute(a); if (v) o.push(`@${a}=${v}`); }));
      return o;
    });
    for (const s of tx) if (ES.test(s) && !/Pronto|Parakeet|Txonso|Bilbao|Alfonso/.test(s)) vivos.set(s.slice(0, 100), r);
    await p.cerrar();
  }
  out.push({ grav: vivos.size ? 'BAJA' : 'OK', titulo: `texto en español visible con el hub en inglés: ${vivos.size}`, evidencia: [...vivos].slice(0, 40).map(([s, r]) => `${r}  «${s}»`).join('\n') });
  return out;
}
