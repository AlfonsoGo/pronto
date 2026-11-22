// Regla 5 · Experiencia: un dictado nunca se pierde (CLAUDE.md § NO SE NEGOCIA).
// La app entera (atajo, micrófono, pegado con Ctrl+V) no se puede lanzar en una batería sin ratón ni
// teclado, así que se prueba la pieza real más cercana: `state::dictar_con` (transcribir → historial +
// WAV → pegar, con el pegado inyectado y un motor de mentira), `state::finish_dictation` y
// `state::keep_failed_transcription`. Comprueba que, si el pegado, el pulido o la transcripción fallan, el
// dictado queda en el historial con su audio (`has_audio` y WAV legible con todas las muestras).
// BELTR-PRT-30 (prueba de Txonso del 20-nov con el PC cargado): motor que devuelve vacío o entra en pánico,
// carga simulada (8 dictados a la vez con un motor lento), soltar mientras se confirma un segmento o se
// anticipa el texto, Ollama colgado, y el atajo pulsado mientras se transcribe el dictado anterior.
// Con los audios reales de esa noche (fuera del repo, voz de Txonso) y Whisper medium: 0 vacíos; y con
// ruidos sin voz (zumbido, respiración, teclado, ruido blanco, ceros), 0 frases inventadas.
// No cubierto aquí: la restauración del portapapeles (toca el portapapeles real del usuario).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { cargoTest, exigirCargoVerde } from './_comun.mjs';

const NUEVOS = [
  'nsn_experiencia_ningun_dictado_se_pierde',
  // Rompedor 22-oct-c, ALTA 1: un dictado que la tubería deja vacío queda en el historial con su audio.
  'nsn_experiencia_dictado_vaciado_por_la_tuberia_no_se_pierde',
  'nsn_experiencia_motor_vacio_o_que_falla_no_pierde_el_dictado',
  'nsn_experiencia_carga_simulada_y_pegado_fallido_no_pierden_nada',
  'nsn_experiencia_segmento_o_anticipo_vacios_con_voz_se_vuelven_a_transcribir',
  'nsn_experiencia_soltar_mientras_se_confirma_o_se_anticipa',
  'nsn_experiencia_ollama_colgado_pega_sin_pulir_y_guarda',
  'nsn_experiencia_dictar_mientras_se_transcribe_el_anterior',
  'nsn_experiencia_la_voz_clara_no_se_tira_por_no_speech',
  // Revisión del 22-nov (cola única): el anterior pega tras la grabación nueva y la marca de ocupado aguanta.
  'nsn_experiencia_el_anterior_espera_a_la_grabacion_nueva_para_pegar',
  'nsn_experiencia_la_marca_de_ocupado_sigue_con_dictados_pendientes',
];

test('05 experiencia: 0 dictados perdidos si falla el pegado, el pulido o la transcripción, con carga o soltando a destiempo', { timeout: 30 * 60_000 }, () => {
  const r = cargoTest('nsn_experiencia');
  exigirCargoVerde(assert, r, 'nsn_experiencia');
  for (const n of NUEVOS) assert.match(r.salida, new RegExp(`${n} \\.\\.\\. ok`), `no pasó (o no corrió) ${n}`);
});

const VACIOS = process.env.PRONTO_VACIOS_DIR || 'C:/Users/Txonso/dev/pronto-pruebas/datos/vacios-20nov';
const WHISPER = join(process.env.APPDATA ?? '', 'Pronto2', 'models', 'whisper', 'ggml-medium-q5_0.bin');
const saltar = !existsSync(VACIOS) ? `sin los audios de la noche del 20-nov en ${VACIOS}` : !existsSync(WHISPER) ? `no está Whisper medium en ${WHISPER}` : false;

test('05 experiencia: los dictados reales que salían vacíos con Whisper salen con texto (y el silencio, vacío)', { skip: saltar, timeout: 30 * 60_000 }, (t) => {
  const r = cargoTest('whisper_no_deja_vacia_la_voz_real', { env: { PRONTO_VACIOS_DIR: VACIOS } });
  for (const l of r.salida.split(/\r?\n/).filter((l) => /^VACIOS_/.test(l))) t.diagnostic(l);
  exigirCargoVerde(assert, r, 'whisper_no_deja_vacia_la_voz_real');
  assert.ok(!r.salida.includes('VACIOS_OMITIDO'), 'el test de Rust no midió');
  assert.match(r.salida, /VACIOS_FALLOS=0/);
});
