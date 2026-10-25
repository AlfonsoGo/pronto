// Regla 5 · Experiencia: un dictado nunca se pierde (CLAUDE.md § NO SE NEGOCIA).
// La app entera (atajo, micrófono, pegado con Ctrl+V) no se puede lanzar en una batería sin ratón ni
// teclado, así que se prueba la pieza real más cercana: `state::finish_dictation` (pulido → historial
// + WAV → pegado, con el pegado inyectado) y `state::keep_failed_transcription`. Comprueba que, si el
// pegado o el pulido fallan, o falla la transcripción, el dictado queda en el historial con su audio
// (`has_audio` y WAV legible con todas las muestras). El audio va a una carpeta temporal.
// No cubierto aquí: la restauración del portapapeles (toca el portapapeles real del usuario).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { cargoTest, exigirCargoVerde } from './_comun.mjs';

test('05 experiencia: 0 dictados perdidos si falla el pegado, el pulido o la transcripción', { timeout: 30 * 60_000 }, () => {
  const r = cargoTest('nsn_experiencia');
  exigirCargoVerde(assert, r, 'nsn_experiencia');
  assert.match(r.salida, /nsn_experiencia_ningun_dictado_se_pierde \.\.\. ok/);
  // Rompedor 22-oct-c, ALTA 1: un dictado que la tubería deja vacío queda en el historial con su audio.
  assert.match(r.salida, /nsn_experiencia_dictado_vaciado_por_la_tuberia_no_se_pierde \.\.\. ok/);
});
