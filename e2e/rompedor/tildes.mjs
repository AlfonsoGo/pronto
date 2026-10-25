// Sonda: buscadores del hub que filtran en el propio hub (notas, fragmentos, diccionario) con y sin tildes.
// El del historial pliega tildes y mayúsculas en el backend desde 68707a4; ¿y estos?
import { scriptBackendHostil, espera } from './comun.mjs';

export const nombre = 'tildes';
const LIMPIO = scriptBackendHostil({ envenenar: false });
const mock = (page, cmd, args) => page.evaluate(async ([c, a]) => (await import('/shared/mock.js')).invoke(c, a), [cmd, args]);

export async function correr({ abrir }) {
  const out = [];
  const QS = ['canción', 'cancion', 'CANCIÓN', 'Canción', 'ancion', 'NCIÓN'];
  const sitios = [
    ['notes', (t) => ['save_note', { id: null, text: t }], null, '#page .nt-card'],
    ['snippets', (t) => ['add_snippet', { trigger: 'letra', expansion: t }], '#page [data-act=toggle-search]', '#page .sn'],
    ['dictionary', (t) => ['add_word', { word: t, replacement: null }], '#page [data-act=toggle-search]', '#page .w-new'],
  ];
  for (const [pag, sembrar, abreBusca, sel] of sitios) {
    const p = await abrir(`/hub.html?page=${pag}`, { hostil: LIMPIO });
    const { page } = p;
    await page.waitForSelector('.sb .nav');
    const [c, a] = sembrar('Canción de la ponencia'); await mock(page, c, a);
    await page.click('.sb [data-nav=home]'); await espera(300); await page.click(`.sb [data-nav=${pag}]`); await espera(600);
    if (abreBusca) { await page.click(abreBusca); await espera(200); }
    const r = {};
    for (const q of QS) {
      await page.fill('#page [data-search-input]', q); await espera(450);
      r[q] = await page.$$eval(sel, (els) => els.filter((e) => /Canción de la ponencia/.test(e.textContent)).length);
    }
    const no = QS.filter((q) => !r[q]);
    out.push({ grav: no.length ? 'BAJA' : 'OK', titulo: `buscador de ${pag} con tildes: ${no.length ? 'no encuentra «Canción de la ponencia» con ' + no.map((x) => `«${x}»`).join(', ') : 'encuentra con todas'}`, evidencia: JSON.stringify(r) });
    await p.cerrar();
  }
  return out;
}
