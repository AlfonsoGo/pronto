// Común a las pruebas del pulido en directo y del entendimiento con audios de internet (BELTR-PRT-10,
// contrato en specs/features/pulido-en-directo.md). No es un test: node --test solo recoge *.test.mjs.
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { RAIZ, cargoTest, exigirCargoVerde } from '../no-se-negocia/_comun.mjs';

export const UMBRALES = JSON.parse(readFileSync(join(RAIZ, 'tests', 'banco-pulido', 'umbrales.json'), 'utf8'));
export const MODELO = process.env.PRONTO_PARAKEET_DIR || join(process.env.APPDATA ?? '', 'Pronto2', 'models', 'parakeet');
export const DICCIONARIO = join(RAIZ, 'tests', 'banco-entender', 'diccionario.txt');
export const MODELO_PULIDO = process.env.PRONTO_PULIDO_MODELO ?? 'qwen3:1.7b';

// Audios de internet (FLEURS y MLS, CC-BY 4.0): fuera del repo, con su manifiesto (fuente y licencia).
// PRONTO_INTERNET=carpeta con cortos/, largos/ y quince/.
export const INTERNET = process.env.PRONTO_INTERNET || 'C:/Users/Txonso/dev/pronto-pruebas/datos/internet';
export const conjunto = (n) => join(INTERNET, n);
export const hay = (n) => existsSync(conjunto(n)) && readdirSync(conjunto(n)).some((f) => f.endsWith('.wav'));

export const faltaModelo = () => {
  const f = ['encoder.int8.onnx', 'decoder.int8.onnx', 'joiner.int8.onnx', 'tokens.txt'].filter((x) => !existsSync(join(MODELO, x)));
  return f.length ? `no está el modelo Parakeet en ${MODELO}; define PRONTO_PARAKEET_DIR` : false;
};

export async function ollamaResponde() {
  try {
    const r = await fetch('http://127.0.0.1:11434/api/tags', { signal: AbortSignal.timeout(1500) });
    return ((await r.json()).models ?? []).some((m) => m.name === MODELO_PULIDO);
  } catch {
    return false;
  }
}

/** Lanza el banco de velocidad (`banco_pulido::banco_velocidad_pulido`) y devuelve sus cifras por conjunto y modo. */
export function medirVelocidad(assert, t, { sets, modo = 'directo', max = null }) {
  const env = {
    PRONTO_NSN_MODELO: MODELO,
    PRONTO_BANCO_DICCIONARIO: DICCIONARIO,
    PRONTO_BANCO_PULIDO: MODELO_PULIDO,
    PRONTO_VELOCIDAD_SETS: sets.map((n) => `${n}=${conjunto(n)}`).join(';'),
    PRONTO_VELOCIDAD_MODO: modo,
  };
  if (max) env.PRONTO_VELOCIDAD_MAX = String(max);
  const r = cargoTest('banco_pulido::tests::banco_velocidad_pulido', { env, timeoutMs: 90 * 60_000 });
  exigirCargoVerde(assert, r, 'banco_velocidad_pulido');
  assert.ok(!r.salida.includes('VELOCIDAD_OMITIDO'), 'el banco de Rust no midió');
  for (const m of r.salida.matchAll(/^VELOCIDAD_RESUMEN .*$/gm)) t.diagnostic(m[0].replace('VELOCIDAD_RESUMEN ', ''));
  const cifra = (k) => Number((r.salida.match(new RegExp(`^VELOCIDAD_${k}=(.*)$`, 'm')) ?? [])[1]);
  return { cifra, salida: r.salida };
}

/** Lanza el banco de entendimiento sobre conjuntos de internet y devuelve el WER de cada uno. */
export function medirWer(assert, t, sets) {
  const r = cargoTest('banco_entender::tests::banco_entender', {
    env: {
      PRONTO_BANCO_SETS: sets.map((n) => `${n}=${conjunto(n)}`).join(';'),
      PRONTO_BANCO_DICCIONARIO: DICCIONARIO,
      PRONTO_NSN_MODELO: MODELO,
      PRONTO_BANCO_SIN_VISTA: '1',
    },
  });
  exigirCargoVerde(assert, r, 'banco_entender');
  for (const m of r.salida.matchAll(/^BANCO_RESUMEN .*$/gm)) t.diagnostic(m[0].replace('BANCO_RESUMEN ', ''));
  const wer = {};
  for (const n of sets) wer[n] = Number((r.salida.match(new RegExp(`^BANCO_${n.toUpperCase()}_PARAKEET_WER=(.*)$`, 'm')) ?? [])[1]);
  return wer;
}
