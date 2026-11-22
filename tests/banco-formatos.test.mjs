// Banco de formatos (BELTR-PRT-15, contrato en specs/features/pulido-en-directo.md): 40 dictados con
// correos, teléfonos, símbolos, webs, horas, euros y % (tests/banco-formatos/casos.tsv), dichos por voces
// locales (generar.mjs: voz de Windows + Piper; nunca por los altavoces). Dicta cada uno con Parakeet por
// el camino del dictado (`transcribe_heard` + `text::process`) y cuenta los bien escritos: sin pulir y,
// si Ollama responde, con el pulido encendido. Falla si alguno baja de tests/banco-formatos/umbrales.json
// o si el pulido estropea un caso que sin pulir estaba bien (regla 7: el pulido no toca cifras ni cuentas).
// Sin modelo Parakeet, se salta. Sin Ollama, solo mide sin pulir.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { RAIZ, cargoTest, exigirCargoVerde } from './no-se-negocia/_comun.mjs';
import { leerCasos, generar } from './banco-formatos/generar.mjs';

const BANCO = join(RAIZ, 'tests', 'banco-formatos');
const AUDIO = join(BANCO, 'audio');
const UMBRALES = JSON.parse(readFileSync(join(BANCO, 'umbrales.json'), 'utf8'));
const MODELO = process.env.PRONTO_PARAKEET_DIR || join(process.env.APPDATA ?? '', 'Pronto2', 'models', 'parakeet');
const faltaModelo = ['encoder.int8.onnx', 'decoder.int8.onnx', 'joiner.int8.onnx', 'tokens.txt'].filter((f) => !existsSync(join(MODELO, f)));
const MODELO_PULIDO = process.env.PRONTO_FORMATOS_PULIDO ?? 'qwen3:1.7b';

/** ¿Está bien escrito? Contiene alguno de los aceptados (sin mirar mayúsculas) y ninguno de los prohibidos. */
export function bienEscrito(texto, caso) {
  const t = texto.toLowerCase().replace(/ | /g, ' ');
  const tiene = (x) => t.includes(x.toLowerCase().replace(/ | /g, ' '));
  return caso.aceptados.some(tiene) && !caso.prohibidos.some(tiene);
}

async function ollamaResponde() {
  try {
    const r = await fetch('http://127.0.0.1:11434/api/tags', { signal: AbortSignal.timeout(1500) });
    const j = await r.json();
    return (j.models ?? []).some((m) => m.name === MODELO_PULIDO);
  } catch {
    return false;
  }
}

const saltar = faltaModelo.length ? `no está el modelo Parakeet en ${MODELO}; define PRONTO_PARAKEET_DIR` : false;

test('las reglas de formatos aciertan los 40 casos escritos tal cual se dicen (sin audio)', () => {
  // Comprobación barata del criterio: cada caso tiene al menos un aceptado y el id es único.
  const casos = leerCasos();
  assert.equal(casos.length, 40);
  assert.equal(new Set(casos.map((c) => c.id)).size, 40);
  for (const c of casos) assert.ok(c.aceptados.length > 0, `caso ${c.id} sin aceptados`);
  assert.ok(bienEscrito('Escríbeme a alfonso@gmail.com.', casos[0]));
  assert.ok(!bienEscrito('La nota lleva un asterisco *', casos.find((c) => c.id === '20')));
});

test('banco de formatos: correos, teléfonos, símbolos, webs, horas, euros y % bien escritos', { skip: saltar, timeout: 20 * 60_000 }, async (t) => {
  const casos = leerCasos();
  if (!existsSync(AUDIO) || readdirSync(AUDIO).filter((f) => f.endsWith('.wav')).length !== casos.length) generar(AUDIO);
  const conPulido = await ollamaResponde();
  if (!conPulido) t.diagnostic(`Ollama no responde o no tiene ${MODELO_PULIDO}: solo se mide sin pulir`);
  const env = { PRONTO_FORMATOS_DIR: AUDIO, PRONTO_NSN_MODELO: MODELO, PRONTO_BANCO_DICCIONARIO: join(RAIZ, 'tests', 'banco-entender', 'diccionario.txt') };
  if (conPulido) env.PRONTO_BANCO_PULIDO = MODELO_PULIDO;
  const r = cargoTest('banco_pulido::tests::banco_formatos', { env });
  exigirCargoVerde(assert, r, 'banco_formatos');
  assert.ok(!r.salida.includes('FORMATOS_OMITIDO'), 'el banco de Rust no midió');
  const salida = new Map();
  for (const m of r.salida.matchAll(/^FORMATO\t(\S+)\t([^\t]*)\t([^\r\n]*)$/gm)) salida.set(m[1], { sin: m[2], con: m[3] });
  assert.equal(salida.size, casos.length, `se esperaban ${casos.length} casos en la salida`);

  let sin = 0, con = 0;
  const estropeados = [];
  const porCategoria = {};
  for (const c of casos) {
    const o = salida.get(c.id);
    const okSin = bienEscrito(o.sin, c);
    const okCon = conPulido ? bienEscrito(o.con, c) : okSin;
    sin += okSin;
    con += okCon;
    porCategoria[c.categoria] ??= { n: 0, bien: 0 };
    porCategoria[c.categoria].n++;
    porCategoria[c.categoria].bien += okSin;
    if (okSin && !okCon) estropeados.push(`${c.id}: «${o.sin}» → «${o.con}»`);
    if (!okSin) t.diagnostic(`mal ${c.id} (${c.categoria}): «${o.sin}» · se esperaba ${c.aceptados[0]}`);
  }
  t.diagnostic(`FORMATOS sin pulir ${sin}/${casos.length} · con pulido ${conPulido ? `${con}/${casos.length}` : 'no medido'}`);
  t.diagnostic(Object.entries(porCategoria).map(([k, v]) => `${k} ${v.bien}/${v.n}`).join(' · '));
  assert.ok(sin >= UMBRALES.sinPulidoMin, `formatos sin pulir: ${sin}/40 < ${UMBRALES.sinPulidoMin}`);
  if (conPulido) {
    assert.ok(con >= UMBRALES.conPulidoMin, `formatos con pulido: ${con}/40 < ${UMBRALES.conPulidoMin}`);
    assert.deepEqual(estropeados, [], `el pulido estropeó formatos que estaban bien:\n  ${estropeados.join('\n  ')}`);
  }
});

// ── Voz real de Txonso (BELTR-PRT-30) ─────────────────────────────────────────────────────────────────
// Correos dichos por Txonso, fuera del repo (es su voz): «correos-noche» (los 7 que dictó el 20-nov) y
// «correos-voz» (las 15 frases de su GUION.md, cuando las grabe). Cada carpeta lleva su casos.tsv. Se mide
// con Parakeet y con Whisper medium, sin pulir, con el diccionario del banco y con «arroba», «Gmail» y
// «Hotmail» añadidos (¿ayuda guardarlas?). Falla si alguna cifra baja de umbrales-voz.json.
const DATOS_VOZ = process.env.PRONTO_PRUEBAS_DATOS || 'C:/Users/Txonso/dev/pronto-pruebas/datos';
const UMBRALES_VOZ = JSON.parse(readFileSync(join(BANCO, 'umbrales-voz.json'), 'utf8'));
const WHISPER_MEDIUM = join(process.env.APPDATA ?? '', 'Pronto2', 'models', 'whisper', 'ggml-medium-q5_0.bin');
const DICCIONARIOS = { base: join(RAIZ, 'tests', 'banco-entender', 'diccionario.txt'), correos: join(BANCO, 'diccionario-correos.txt') };

for (const conjunto of ['correos-noche', 'correos-voz']) {
  const dir = join(DATOS_VOZ, conjunto);
  const hay = existsSync(join(dir, 'casos.tsv')) && readdirSync(dir).some((f) => /^\d+\.wav$/.test(f));
  const motivo = faltaModelo.length ? saltar : !hay ? `sin grabaciones en ${dir}${conjunto === 'correos-voz' ? ': falta que Txonso grabe las 15 frases de GUION.md' : ''}` : false;
  test(`banco de formatos con la voz de Txonso (${conjunto}): correos con y sin «arroba», «gmail», «hotmail» en el diccionario`, { skip: motivo, timeout: 20 * 60_000 }, (t) => {
    const casos = leerCasos(join(dir, 'casos.tsv')).filter((c) => existsSync(join(dir, `${c.id}.wav`)));
    const motores = ['parakeet', ...(existsSync(WHISPER_MEDIUM) ? ['whisper-medium'] : [])];
    const fallos = [];
    for (const motor of motores) {
      for (const [dic, ruta] of Object.entries(DICCIONARIOS)) {
        const env = { PRONTO_FORMATOS_DIR: dir, PRONTO_NSN_MODELO: MODELO, PRONTO_BANCO_DICCIONARIO: ruta };
        if (motor !== 'parakeet') env.PRONTO_FORMATOS_MOTOR = motor;
        const r = cargoTest('banco_pulido::tests::banco_formatos', { env });
        exigirCargoVerde(assert, r, `banco_formatos ${motor}`);
        const salida = new Map([...r.salida.matchAll(/^FORMATO\t(\S+)\t([^\t]*)\t/gm)].map((m) => [m[1], m[2]]));
        let bien = 0;
        for (const c of casos) {
          const o = salida.get(c.id) ?? '';
          const ok = bienEscrito(o, c);
          bien += ok;
          if (!ok) t.diagnostic(`${motor} · ${dic} · mal ${c.id}: «${o}»`);
        }
        const pct = Math.round((bien / casos.length) * 100);
        t.diagnostic(`VOZ_REAL ${conjunto} ${motor} diccionario=${dic}: ${bien}/${casos.length} (${pct} %)`);
        const min = UMBRALES_VOZ[conjunto]?.[`${motor}-${dic}`];
        if (typeof min === 'number' && bien < min) fallos.push(`${motor} ${dic}: ${bien}/${casos.length} < ${min}`);
      }
    }
    assert.deepEqual(fallos, [], 'los correos con la voz de Txonso empeoraron');
  });
}
