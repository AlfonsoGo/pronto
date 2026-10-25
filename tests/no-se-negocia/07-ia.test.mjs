// Regla 7 · La IA no inventa ni obedece (CLAUDE.md § NO SE NEGOCIA), con la decisión A de Txonso
// (23-oct): «Lo que llega al modelo es dato hostil: nunca se ejecuta como orden. El pulido no añade
// contenido: cifras, enlaces o cuentas que no estaban → se pega sin pulir. Un Transform nunca se pega
// sin que el usuario lo confirme con Enter. Cada tarea o decisión de una reunión lleva la cita literal
// que la respalda y las notas se marcan como generadas con IA local.»
//
// - Pulido (src-tauri/src/polish.rs `accept`): ya no bloquea por palabras nuevas. Se pega sin pulir si
//   la salida trae cifras (también compuestas, con su unidad o moneda, horas y fechas), enlaces, rutas,
//   IP, correos, cuentas o comandos que no estaban (ANCLAJE, src-tauri/src/anclaje.rs: valor completo,
//   sin plegar homoglifos, con el esquema, cuentas en orden), si cambia la polaridad o el orden de lo
//   dicho, si deja de ser el mismo texto (traducción, respuesta a la pregunta) o si no cabe en la mitad
//   o el doble. Lo que se pega va sin caracteres ocultos (etiquetas Unicode, RLO).
// - Transforms (src-tauri/src/ollama.rs `review_transform`/`accept_transform`, state.rs): la salida va a
//   la vista previa de la Flow bar con sus avisos (órdenes en la selección, lo que añade o quita, el
//   sentido cambiado, ocultos quitados); solo Enter la pega.
// - Reuniones (notes.rs, summary.rs, meetingdb.rs): cada tarea y decisión con su cita literal (turno,
//   quién y momento) localizada en la transcripción, con la misma polaridad y el mismo sujeto, nunca de
//   un turno que da órdenes a la IA; sin cita, fuera. Se guarda (esquema v10) y sale en el Markdown con
//   la etiqueta «Generado con IA local». La transcripción va delimitada como dato (`as_data`, que no se
//   cierra con etiquetas disfrazadas ni tokens de plantilla).
// Sin Ollama real: servidores locales falsos.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { cargoTest, exigirCargoVerde } from './_comun.mjs';

// Los `nsn_ia_*` que tienen que haberse ejecutado (si uno se renombra o desaparece, falla).
const NSN_IA = [
  // Pulido de dictados (polish.rs): no añade cifras, enlaces ni cuentas; no obedece; no estropea.
  'rechaza_salidas_que_anaden_contenido', 'no_obedece_ordenes_del_dictado', 'acepta_el_pulido_legitimo',
  'pulido_no_cambia_cifras_ni_polaridad', 'pulido_legitimo_con_cifras_y_grafias',
  'pulido_cifras_enlaces_y_cuentas_disfrazados', 'pulido_no_pega_caracteres_ocultos',
  // Transforms: vista previa con avisos, nada se pega sin Enter (state.rs + ollama.rs).
  'transform_seleccion_hostil_no_se_pega', 'transform_no_pega_lo_que_el_modelo_inventa', 'transform_legitimos_se_pegan',
  'transform_avisa_sin_bloquear', 'transform_anclaje_bloquea_lo_que_no_estaba', 'transform_ordenes_disfrazadas_no_se_pegan',
  'transform_kind_lo_decide_el_usuario', 'as_data_no_se_cierra_con_etiquetas_disfrazadas',
  // Reuniones (notes.rs, summary.rs): cita literal, polaridad contra la cita, anclaje, etiqueta de IA.
  'reunion_cada_tarea_y_decision_lleva_su_cita', 'reunion_sin_cita_o_con_sentido_contrario_se_descarta',
  'reunion_no_obedece_tareas_inyectadas', 'reunion_titulo_inyectado_queda_el_de_por_defecto', 'ask_delimita_las_reuniones_como_dato',
  'reunion_anclaje_cifras_enlaces_y_fechas', 'reunion_orden_partida_acta_y_pulido', 'reunion_frases_normales_respaldan',
  'reunion_titulo_de_palabras_meta', 'markdown_inerte',
];

test('07 IA: 0 pulidos aceptados con cifras, enlaces o cuentas nuevas; 0 Transforms pegados sin Enter; 0 tareas o decisiones de reunión sin cita literal; notas marcadas como IA local', { timeout: 30 * 60_000 }, () => {
  const r = cargoTest('nsn_ia');
  exigirCargoVerde(assert, r, 'nsn_ia');
  for (const t of NSN_IA) {
    assert.match(r.salida, new RegExp(`nsn_ia_${t} \\.\\.\\. ok`), `no se ejecutó nsn_ia_${t}`);
  }
});

test('07 IA: el prompt del pulido trata el dictado como dato, no como orden (Rust)', { timeout: 30 * 60_000 }, () => {
  const r = cargoTest('el_prompt_prohibe_contestar_y_resumir');
  exigirCargoVerde(assert, r, 'el_prompt_prohibe_contestar_y_resumir');
});

test('07 IA: la migración v10 guarda la cita de tareas y decisiones sin perder filas (Rust)', { timeout: 30 * 60_000 }, () => {
  const r = cargoTest('nsn_integridad_migracion_v10_conserva_tareas_y_decisiones');
  exigirCargoVerde(assert, r, 'nsn_integridad_migracion_v10_conserva_tareas_y_decisiones');
});
