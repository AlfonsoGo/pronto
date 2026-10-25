// Control: axe sobre páginas del hub SIN __TAURI__ falso (mock puro) y con el viewport de tests/no-se-negocia/06.
import { createRequire } from 'node:module';
import { espera } from './comun.mjs';

export const nombre = 'axe-control';
const require = createRequire(import.meta.url);
const AXE = require.resolve('axe-core/axe.min.js');

export async function correr({ abrir }) {
  const out = [];
  for (const [vp, r] of [[{ width: 1180, height: 780 }, '/hub.html?page=dictionary'], [{ width: 1280, height: 800 }, '/hub.html?page=dictionary']]) {
    const p = await abrir(r, { viewport: vp });
    await p.page.waitForSelector('.sb .nav'); await espera(1500);
    await p.page.addScriptTag({ path: AXE });
    const v = await p.page.evaluate(async () => (await window.axe.run(document, { runOnly: { type: 'tag', values: ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'] }, resultTypes: ['violations'] }))
      .violations.filter((x) => x.impact === 'serious' || x.impact === 'critical')
      .map((x) => ({ id: x.id, n: x.nodes.slice(0, 3).map((n) => ({ t: n.target.join(' '), html: n.html.slice(0, 260), msg: n.failureSummary.slice(0, 200) })) })));
    out.push({ grav: v.length ? 'MEDIA' : 'OK', titulo: `axe mock puro ${r} ${vp.width}×${vp.height}: ${v.length} violaciones graves`, evidencia: JSON.stringify(v, null, 1) });
    await p.cerrar();
  }
  return out;
}
