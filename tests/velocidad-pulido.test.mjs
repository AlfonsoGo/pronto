// Velocidad con el pulido encendido (BELTR-PRT-16, contrato H3 en specs/features/pulido-en-directo.md):
// de soltar el atajo a texto listo, en dictados de ~15 s de internet (FLEURS), dichos a ritmo real con la
// sesión de vivo de la app (vista previa + segmentos confirmados + pulidor de fondo) y Ollama de verdad.
// Versión rápida (3 dictados); la completa, con los de 2 min, va en `npm run test:lento`.
// Se salta sin modelo Parakeet, sin los audios de internet o sin Ollama con el modelo del pulido.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { UMBRALES, faltaModelo, hay, ollamaResponde, medirVelocidad, MODELO_PULIDO } from './banco-pulido/comun.mjs';

const saltar = faltaModelo() || (!hay('quince') && 'faltan los audios de internet (PRONTO_INTERNET)');

// Versión rápida: 4 dictados, en directo y por el camino de antes (audio entero al soltar, sin sesión de
// vivo) en la misma tanda, para que la carga del PC (un juego, otra compilación) afecte a los dos igual.
// El en directo no puede ser más lento que el de antes (con un 15 % de margen por el ruido de 4 muestras)
// ni pasar de 2,5 s; la meta de 1,5 s p50 se mide con los 10 dictados en `npm run test:lento`.
test('pulido en directo: 15 s de dictado no tardan más que antes en salir pulidos', { skip: saltar, timeout: 20 * 60_000 }, async (t) => {
  if (!(await ollamaResponde())) return t.skip(`Ollama no responde o no tiene ${MODELO_PULIDO}`);
  const { cifra } = medirVelocidad(assert, t, { sets: ['quince'], modo: 'ambos', max: 4 });
  const [directo, antes] = [cifra('QUINCE_DIRECTO_P50'), cifra('QUINCE_HOY_P50')];
  assert.ok(Number.isFinite(directo) && Number.isFinite(antes), 'faltan las cifras p50');
  assert.ok(directo <= Math.max(antes * 1.15, UMBRALES.quince.p50Max), `15 s con pulido: en directo p50 ${directo} ms, antes ${antes} ms`);
  assert.ok(directo <= 2500, `15 s con pulido: p50 ${directo} ms > 2500 ms`);
  // Mismo audio: en directo no puede entender peor que antes (el texto anticipado puede cambiar alguna palabra).
  const [werDirecto, werAntes] = [cifra('QUINCE_DIRECTO_WER'), cifra('QUINCE_HOY_WER')];
  assert.ok(werDirecto <= werAntes + 1, `15 s: WER en directo ${werDirecto} % > antes ${werAntes} % + 1`);
});
