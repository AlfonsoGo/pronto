// Texto en vivo con Whisper (BELTR-PRT-10): el texto final nunca puede ser peor que transcribir el audio
// entero. Prueba de Txonso del 19-nov con Whisper large-v3-turbo: a trozos inventaba («Va toda hostia
// esto. ¡Gracias!» donde el audio entero da «Va toda hostia esto.») y puntuaba cada corte. Modelo:
// %APPDATA%\Pronto2\models\whisper (solo lectura) y una GPU con Vulkan; sin modelo, se salta. Con
// PRONTO_VIVO_WAVS (rutas separadas por «;») prueba además esos audios. Enseña también lo que tarda de
// soltar a texto con un tick a medias (se corta con el abort_callback de whisper.cpp).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { cargoTest, exigirCargoVerde } from './no-se-negocia/_comun.mjs';

const MODELO = join(process.env.APPDATA ?? '', 'Pronto2', 'models', 'whisper', 'ggml-medium-q5_0.bin');
const saltar = !existsSync(MODELO) && `no está el modelo de Whisper en ${MODELO}`;

test('vivo: con Whisper, al soltar sale lo mismo que con el audio entero (sin frases inventadas)', { skip: saltar, timeout: 30 * 60_000 }, (t) => {
  const r = cargoTest('vivo_whisper_no_inventa_al_soltar', { env: { PRONTO_VIVO_WHISPER: '1' } });
  for (const l of r.salida.split(/\r?\n/).filter((l) => /^VIVO_(WAV|ENTERO|FINAL|MS)=/.test(l))) t.diagnostic(l);
  exigirCargoVerde(assert, r, 'vivo_whisper_no_inventa_al_soltar');
  assert.ok(!r.salida.includes('VIVO_OMITIDO'), 'el test de Rust no se ejecutó');
});
