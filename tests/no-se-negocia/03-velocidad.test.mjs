// Regla 3 · Velocidad (CLAUDE.md § NO SE NEGOCIA).
// Transcribir ~15 s de audio con el motor por defecto (Parakeet TDT 0.6B v3 int8) no puede empeorar
// más de un 20 % la línea base medida. Audio: datos/dictado-15s.wav (14,2 s, voz TTS de Windows
// generada a fichero, nunca por los altavoces; se regenera con datos/generar-dictado.ps1). Modelo:
// %APPDATA%\Pronto2\models\parakeet (solo lectura) o PRONTO_PARAKEET_DIR. Sin modelo, se salta.
//
// Línea base medida el 22-oct-2026 en el PC de Txonso (AMD Ryzen 7 5800X3D, 4 hilos de dictado),
// `cargo test` (perfil de pruebas): 4 tandas de 5 transcripciones tras calentar, medianas de 870,
// 887, 890 y 901 ms → 890 ms. El resto de la tubería tras transcribir (huella de voz + limpieza y
// estilo del texto) tarda ~0,3 ms (~11 ms la primera vez). Si cambia el PC, vuelve a medir y
// actualiza este número y la fila 3 de CLAUDE.md con el visto bueno de Txonso; nunca lo subas para
// que el test pase.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { join } from 'node:path';
import { RAIZ, cargoTest, exigirCargoVerde } from './_comun.mjs';

export const LINEA_BASE_MS = 890;
export const MARGEN = 1.2;

const DATOS = join(RAIZ, 'tests', 'no-se-negocia', 'datos');
const WAV = join(DATOS, 'dictado-15s.wav');
const MODELO = process.env.PRONTO_PARAKEET_DIR || join(process.env.APPDATA ?? '', 'Pronto2', 'models', 'parakeet');
const FICHEROS_MODELO = ['encoder.int8.onnx', 'decoder.int8.onnx', 'joiner.int8.onnx', 'tokens.txt'];
const faltaModelo = FICHEROS_MODELO.filter((f) => !existsSync(join(MODELO, f)));

function asegurarWav() {
  if (existsSync(WAV) || process.platform !== 'win32') return existsSync(WAV);
  spawnSync('powershell', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', join(DATOS, 'generar-dictado.ps1'), WAV], { encoding: 'utf8' });
  return existsSync(WAV);
}

const mediana = (v) => {
  const s = [...v].sort((a, b) => a - b);
  return s.length % 2 ? s[(s.length - 1) / 2] : (s[s.length / 2 - 1] + s[s.length / 2]) / 2;
};

const saltar = faltaModelo.length
  ? `no está el modelo Parakeet en ${MODELO} (faltan ${faltaModelo.join(', ')}); define PRONTO_PARAKEET_DIR`
  : false;

test(`03 velocidad: transcribir ~15 s con Parakeet ≤ ${LINEA_BASE_MS} ms × ${MARGEN} (p50 de 5)`, { skip: saltar, timeout: 30 * 60_000 }, (t) => {
  assert.ok(asegurarWav(), `no hay audio de prueba en ${WAV} y no se pudo generar`);
  const r = cargoTest('nsn_velocidad', { env: { PRONTO_NSN_WAV: WAV, PRONTO_NSN_MODELO: MODELO, PRONTO_NSN_RUNS: '5' } });
  exigirCargoVerde(assert, r, 'nsn_velocidad');
  const linea = (k) => (r.salida.match(new RegExp(`^${k}=(.*)$`, 'm')) ?? [])[1];
  assert.ok(!r.salida.includes('NSN_OMITIDO'), 'el test de Rust no midió (faltan variables)');
  const asr = linea('NSN_TRANSCRIPCION_MS').split(',').map(Number);
  const post = linea('NSN_TUBERIA_MS').split(',').map(Number);
  assert.equal(asr.length, 5, 'se esperaban 5 medidas');
  const p50 = mediana(asr);
  t.diagnostic(`audio ${linea('NSN_AUDIO_S')} s · transcripción ${asr.join(', ')} ms → p50 ${p50.toFixed(0)} ms (límite ${(LINEA_BASE_MS * MARGEN).toFixed(0)}) · resto de la tubería p50 ${mediana(post).toFixed(1)} ms`);
  t.diagnostic(`texto: ${linea('NSN_TEXTO')}`);
  assert.ok(p50 <= LINEA_BASE_MS * MARGEN, `transcribir 15 s tarda ${p50.toFixed(0)} ms (p50), más que la línea base ${LINEA_BASE_MS} ms × ${MARGEN} = ${(LINEA_BASE_MS * MARGEN).toFixed(0)} ms`);
});

// Texto en vivo (BELTR-PRT-10, encendido de fábrica): mientras se habla la Flow bar enseña una vista
// previa; al soltar, el texto final sale del audio entero por el mismo camino que sin texto en vivo
// (`state::transcribe_heard`). Se mide de soltar a texto final en el peor caso: el tick más largo del
// texto en vivo acaba de empezar al soltar. El dictado no lo espera (con Parakeet van a la vez, ticks de
// 4 s como mucho) y no puede pasar del mismo límite que sin texto en vivo; además, el texto final tiene
// que ser el mismo palabra a palabra que sin texto en vivo.
test(`03 velocidad: con texto en vivo, de soltar a texto ≤ ${LINEA_BASE_MS} ms × ${MARGEN} (peor caso, p50 de 5)`, { skip: saltar, timeout: 30 * 60_000 }, (t) => {
  assert.ok(asegurarWav(), `no hay audio de prueba en ${WAV} y no se pudo generar`);
  const r = cargoTest('nsn_vivo_soltar_hasta_texto', { env: { PRONTO_NSN_WAV: WAV, PRONTO_NSN_MODELO: MODELO, PRONTO_NSN_RUNS: '5' } });
  exigirCargoVerde(assert, r, 'nsn_vivo_soltar_hasta_texto');
  const linea = (k) => (r.salida.match(new RegExp(`^${k}=(.*)$`, 'm')) ?? [])[1];
  assert.ok(!r.salida.includes('NSN_OMITIDO'), 'el test de Rust no midió (faltan variables)');
  const entero = linea('NSN_ENTERO_MS').split(',').map(Number);
  const peor = linea('NSN_VIVO_PEOR_MS').split(',').map(Number);
  assert.equal(peor.length, 5, 'se esperaban 5 medidas');
  t.diagnostic(`de soltar a texto: sin texto en vivo p50 ${mediana(entero).toFixed(0)} ms · con texto en vivo y un tick a medias p50 ${mediana(peor).toFixed(0)} ms (${peor.join(', ')}) · tick p50 ${linea('NSN_TICK_P50_MS')} ms, el mayor ${linea('NSN_TICK_MAYOR')} · palabras iguales ${linea('NSN_PALABRAS_IGUALES')}`);
  t.diagnostic(`texto final: ${linea('NSN_TEXTO_VIVO')}`);
  const [iguales, total] = linea('NSN_PALABRAS_IGUALES').split('/').map(Number);
  assert.equal(iguales, total, `con texto en vivo, el texto final del dictado cambia:\n  sin texto en vivo: ${linea('NSN_TEXTO_ENTERO')}\n  con texto en vivo: ${linea('NSN_TEXTO_VIVO')}`);
  assert.ok(mediana(peor) <= LINEA_BASE_MS * MARGEN, `con texto en vivo, de soltar a texto tarda ${mediana(peor).toFixed(0)} ms (peor caso, p50), más que ${(LINEA_BASE_MS * MARGEN).toFixed(0)} ms`);
});
