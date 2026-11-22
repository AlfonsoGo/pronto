// Las medidas lentas del pulido en directo (BELTR-PRT-10; `npm run test:lento`, ~30 min):
// - velocidad con el pulido encendido en 10 dictados de ~15 s y 10 de ~2 min, dichos a ritmo real con la
//   sesión de vivo de la app (segmentos confirmados + pulidor de fondo) y Ollama de verdad;
// - % de palabras mal del texto final en esos dictados (el de 2 min sale de segmentos: no puede empeorar);
// - % de palabras mal de los 10 largos por el camino de siempre (audio entero).
// Umbrales en tests/banco-pulido/umbrales.json (línea base en linea-base-2026-11-20.md).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { UMBRALES, faltaModelo, hay, ollamaResponde, medirVelocidad, medirWer, MODELO_PULIDO } from '../banco-pulido/comun.mjs';

const saltar = faltaModelo() || (!(hay('quince') && hay('largos')) && 'faltan los audios de internet (PRONTO_INTERNET)');

test('lento: pulido en directo en 15 s y en 2 min (ms y % de palabras mal)', { skip: saltar, timeout: 90 * 60_000 }, async (t) => {
  if (!(await ollamaResponde())) return t.skip(`Ollama no responde o no tiene ${MODELO_PULIDO}`);
  const { cifra } = medirVelocidad(assert, t, { sets: ['quince', 'largos'], modo: 'directo' });
  const fallos = [];
  for (const [k, u] of [['QUINCE', UMBRALES.quince], ['LARGOS', UMBRALES.largos]]) {
    const [p50, p90, wer] = ['P50', 'P90', 'WER'].map((x) => cifra(`${k}_DIRECTO_${x}`));
    if (!(p50 <= u.p50Max)) fallos.push(`${k}: p50 ${p50} ms > ${u.p50Max}`);
    if (!(p90 <= u.p90Max)) fallos.push(`${k}: p90 ${p90} ms > ${u.p90Max}`);
    if (!(wer <= u.werDirectoMax)) fallos.push(`${k}: WER en directo ${wer} % > ${u.werDirectoMax} %`);
  }
  assert.deepEqual(fallos, [], `el pulido en directo empeoró:\n  ${fallos.join('\n  ')}`);
});

test('lento: % de palabras mal en 10 audios largos de internet (audio entero)', { skip: saltar, timeout: 60 * 60_000 }, (t) => {
  const wer = medirWer(assert, t, ['largos']);
  assert.ok(wer.largos <= UMBRALES.largos.werMax, `largos: WER ${wer.largos} % > ${UMBRALES.largos.werMax} %`);
});
