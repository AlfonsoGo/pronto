// Banco de entendimiento (BELTR-PRT-11, contrato en specs/features/entender-y-diccionario.md).
// Mide con Parakeet (motor de fábrica) cuatro cifras sobre dos conjuntos de grabaciones y falla si
// alguna empeora respecto a tests/banco-entender/umbrales.json:
//   - WER: % de palabras mal frente al texto de referencia.
//   - Diccionario: palabras guardadas que se dijeron y salen escritas tal cual.
//   - Inventadas: palabras que salen sin haberse dicho.
//   - ms de soltar a texto (regla 3: nunca más de 1068 ms).
// Y lo mismo para la vista previa del texto en vivo (lo que enseña la Flow bar al soltar).
//
// Conjuntos:
//   - sinteticas: 20 frases con palabras del diccionario (tests/banco-entender/frases.txt), voz TTS de
//     Windows generada a fichero (nunca por los altavoces) con tests/banco-entender/generar.ps1.
//   - reales: 13 grabaciones de Txonso. Solo en su PC (no entran en el repo): audio en
//     C:\Users\Txonso\dev\pronto-pruebas\datos\audio y referencias en …\datos\referencias, o
//     PRONTO_BANCO_REALES="carpeta_wav|carpeta_txt". Sin ellas, ese conjunto se salta.
// Modelo: %APPDATA%\Pronto2\models\parakeet (solo lectura) o PRONTO_PARAKEET_DIR. Sin modelo, se salta.
// Los umbrales solo se cambian con una medida nueva y nunca a peor para que pase.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { join } from 'node:path';
import { RAIZ, cargoTest, exigirCargoVerde } from './no-se-negocia/_comun.mjs';

const BANCO = join(RAIZ, 'tests', 'banco-entender');
const AUDIO = join(BANCO, 'audio');
const UMBRALES = JSON.parse(readFileSync(join(BANCO, 'umbrales.json'), 'utf8'));
const MODELO = process.env.PRONTO_PARAKEET_DIR || join(process.env.APPDATA ?? '', 'Pronto2', 'models', 'parakeet');
const FICHEROS_MODELO = ['encoder.int8.onnx', 'decoder.int8.onnx', 'joiner.int8.onnx', 'tokens.txt'];
const faltaModelo = FICHEROS_MODELO.filter((f) => !existsSync(join(MODELO, f)));
const REALES = process.env.PRONTO_BANCO_REALES || 'C:/Users/Txonso/dev/pronto-pruebas/datos/audio|C:/Users/Txonso/dev/pronto-pruebas/datos/referencias';
const [realesWav, realesTxt] = REALES.split('|');
const hayReales = existsSync(realesWav) && existsSync(realesTxt ?? realesWav);
const LIMITE_MS = 1068;

function asegurarAudio() {
  const frases = readFileSync(join(BANCO, 'frases.txt'), 'utf8').split(/\r?\n/).filter((l) => l.trim()).length;
  const hay = () => existsSync(AUDIO) && readdirSync(AUDIO).filter((f) => f.endsWith('.wav')).length === frases;
  if (hay() || process.platform !== 'win32') return hay();
  spawnSync('powershell', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', join(BANCO, 'generar.ps1'), AUDIO], { encoding: 'utf8' });
  return hay();
}

const saltar = faltaModelo.length ? `no está el modelo Parakeet en ${MODELO} (faltan ${faltaModelo.join(', ')}); define PRONTO_PARAKEET_DIR` : false;

test('banco de entendimiento: Parakeet no entiende peor ni inventa más (WER, diccionario, inventadas, ms)', { skip: saltar, timeout: 30 * 60_000 }, (t) => {
  assert.ok(asegurarAudio(), `no hay audio del banco en ${AUDIO} y no se pudo generar`);
  const sets = [`sinteticas=${AUDIO}`];
  if (hayReales) sets.push(`reales=${realesWav}|${realesTxt ?? realesWav}`);
  else t.diagnostic(`sin grabaciones reales en ${realesWav}: solo el conjunto sintético`);
  const r = cargoTest('banco_entender::tests::banco_entender', {
    env: { PRONTO_BANCO_SETS: sets.join(';'), PRONTO_BANCO_DICCIONARIO: join(BANCO, 'diccionario.txt'), PRONTO_NSN_MODELO: MODELO },
  });
  exigirCargoVerde(assert, r, 'banco_entender');
  assert.ok(!r.salida.includes('BANCO_OMITIDO'), 'el banco de Rust no midió (faltan variables)');
  const linea = (k) => (r.salida.match(new RegExp(`^BANCO_${k}=(.*)$`, 'm')) ?? [])[1];
  for (const m of r.salida.matchAll(/^BANCO_RESUMEN .*$/gm)) t.diagnostic(m[0].replace('BANCO_RESUMEN ', ''));

  const fallos = [];
  for (const conjunto of hayReales ? ['sinteticas', 'reales'] : ['sinteticas']) {
    const u = UMBRALES[conjunto];
    const k = `${conjunto.toUpperCase()}_PARAKEET`;
    const num = (x) => Number(linea(`${k}_${x}`));
    const par = (x) => linea(`${k}_${x}`).split('/').map(Number);
    const [dicBien, dicDichas] = par('DIC');
    const [vDicBien] = par('VISTA_DIC');
    const cifras = {
      wer: num('WER'),
      dic: dicBien,
      inventadas: num('INVENTADAS'),
      msP50: num('MS_P50'),
      vistaWer: num('VISTA_WER'),
      vistaDic: vDicBien,
      vistaInventadas: num('VISTA_INVENTADAS'),
    };
    for (const [k, v] of Object.entries(cifras)) assert.ok(Number.isFinite(v), `${conjunto}: falta la cifra ${k} en la salida del banco`);
    assert.equal(num('MUESTRAS'), u.muestras, `${conjunto}: se esperaban ${u.muestras} grabaciones con referencia`);
    assert.equal(dicDichas, u.dicDichas, `${conjunto}: cambió el nº de palabras del diccionario dichas (¿cambiaron las referencias?)`);
    if (cifras.wer > u.werMax) fallos.push(`${conjunto}: WER ${cifras.wer} % > ${u.werMax} %`);
    if (cifras.dic < u.dicMin) fallos.push(`${conjunto}: diccionario ${cifras.dic}/${dicDichas} < ${u.dicMin}`);
    if (cifras.inventadas > u.inventadasMax) fallos.push(`${conjunto}: ${cifras.inventadas} palabras inventadas > ${u.inventadasMax}`);
    if (cifras.vistaWer > u.vistaWerMax) fallos.push(`${conjunto}: vista previa WER ${cifras.vistaWer} % > ${u.vistaWerMax} %`);
    if (cifras.vistaDic < u.vistaDicMin) fallos.push(`${conjunto}: vista previa diccionario ${cifras.vistaDic}/${dicDichas} < ${u.vistaDicMin}`);
    if (cifras.vistaInventadas > u.vistaInventadasMax) fallos.push(`${conjunto}: vista previa ${cifras.vistaInventadas} inventadas > ${u.vistaInventadasMax}`);
    if (cifras.msP50 > LIMITE_MS) fallos.push(`${conjunto}: de soltar a texto p50 ${cifras.msP50} ms > ${LIMITE_MS} ms (regla 3)`);
    // Frases de ~4 s: ninguna puede pasar del límite de 15 s.
    if (conjunto === 'sinteticas' && num('MS_MAX') > LIMITE_MS) fallos.push(`${conjunto}: un dictado de ~4 s tardó ${num('MS_MAX')} ms > ${LIMITE_MS} ms`);
  }
  assert.deepEqual(fallos, [], `el banco de entendimiento empeoró:\n  ${fallos.join('\n  ')}`);
});

// Whisper medium (BELTR-PRT-30): el texto previo con «arroba, Gmail, Hotmail» y la regla nueva de silencio no
// pueden hacer que entienda peor. Sin el modelo, se salta.
const WHISPER = join(process.env.APPDATA ?? '', 'Pronto2', 'models', 'whisper', 'ggml-medium-q5_0.bin');
const saltarWhisper = saltar || (!existsSync(WHISPER) && `no está Whisper medium en ${WHISPER}`);

test('banco de entendimiento: Whisper medium no entiende peor (WER)', { skip: saltarWhisper, timeout: 30 * 60_000 }, (t) => {
  assert.ok(asegurarAudio(), `no hay audio del banco en ${AUDIO}`);
  const sets = [`sinteticas=${AUDIO}`];
  if (hayReales) sets.push(`reales=${realesWav}|${realesTxt ?? realesWav}`);
  const r = cargoTest('banco_entender::tests::banco_entender', {
    env: { PRONTO_BANCO_SETS: sets.join(';'), PRONTO_BANCO_DICCIONARIO: join(BANCO, 'diccionario.txt'), PRONTO_NSN_MODELO: MODELO, PRONTO_BANCO_WHISPER: 'medium', PRONTO_BANCO_SIN_VISTA: '1' },
  });
  exigirCargoVerde(assert, r, 'banco_entender (Whisper)');
  for (const m of r.salida.matchAll(/^BANCO_RESUMEN .*whisper.*$/gm)) t.diagnostic(m[0].replace('BANCO_RESUMEN ', ''));
  const fallos = [];
  for (const conjunto of hayReales ? ['sinteticas', 'reales'] : ['sinteticas']) {
    const wer = Number((r.salida.match(new RegExp(`^BANCO_${conjunto.toUpperCase()}_WHISPER_MEDIUM_WER=(.*)$`, 'm')) ?? [])[1]);
    assert.ok(Number.isFinite(wer), `${conjunto}: falta el WER de Whisper en la salida`);
    if (wer > UMBRALES.whisperMedium[conjunto].werMax) fallos.push(`${conjunto}: Whisper WER ${wer} % > ${UMBRALES.whisperMedium[conjunto].werMax} %`);
  }
  assert.deepEqual(fallos, [], `Whisper entiende peor: ${fallos.join(' · ')}`);
});
