// Entender (BELTR-PRT-18, contrato H6): % de palabras mal de Parakeet en 30 audios cortos de internet
// (FLEURS es_419 test, CC-BY 4.0; fuente y licencia de cada uno en el MANIFIESTO.md de los datos) por el
// camino del dictado. Falla si sube del umbral de tests/banco-pulido/umbrales.json. Los largos (MLS, 2 min)
// van en `npm run test:lento`; las 13 grabaciones de Txonso, en tests/banco-entender.test.mjs.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { UMBRALES, faltaModelo, hay, medirWer } from './banco-pulido/comun.mjs';

const saltar = faltaModelo() || (!hay('cortos') && 'faltan los audios de internet (PRONTO_INTERNET)');

test('entender: % de palabras mal en 30 audios cortos de internet', { skip: saltar, timeout: 20 * 60_000 }, (t) => {
  const wer = medirWer(assert, t, ['cortos']);
  assert.ok(Number.isFinite(wer.cortos), 'falta el WER de cortos');
  assert.ok(wer.cortos <= UMBRALES.cortos.werMax, `cortos: WER ${wer.cortos} % > ${UMBRALES.cortos.werMax} %`);
});
