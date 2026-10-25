// Regla 2 · Integridad de datos (CLAUDE.md § NO SE NEGOCIA).
// Con bases temporales (nunca %APPDATA%\Pronto2): base de esquema antiguo con filas → `Db::open_at`
// (migraciones) → mismas filas; copia diaria (VACUUM INTO) → se abre y tiene las mismas filas;
// abrir una base con datos nunca la deja vacía, un fichero dañado no se pisa con una base nueva y
// un -wal ajeno (el de una base recién creada, incidente del 13-oct) se aparta en vez de taparla.
// La guarda decide por identidad de base (meta.base_id): un -wal ajeno con datos se aparta, el propio
// tras un borrado legítimo no; un -shm cogido por otro programa no mueve nada; una base de antes sale
// con su base_id volcado en el fichero principal (rompedor 22-oct-b). Un -wal ajeno SIN identidad
// (escrito por Pronto ≤ 2.0.2) también se aparta, y uno con otro tamaño de página no aparta la base
// buena (rompedor 22-oct-c, datos ALTA 1), tampoco con más dictados que la buena; el -wal de la versión
// anterior (o de un editor) sin identidad es del usuario: se respeta, con una copia previa en
// respaldo-<fecha-hora>; y restaurar dos veces la misma copia enseña la copia (rompedor 23-oct, datos
// MEDIA 1-3).
// Las comprobaciones viven en src-tauri/src/db.rs (`mod nsn_integridad`).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { cargoTest, exigirCargoVerde } from './_comun.mjs';

test('02 integridad: 0 filas perdidas en migración y copia; copia restaurable', { timeout: 30 * 60_000 }, () => {
  const r = cargoTest('nsn_integridad');
  exigirCargoVerde(assert, r, 'nsn_integridad');
  for (const t of ['migracion_conserva_cada_fila', 'copia_diaria_restaurable', 'abrir_nunca_vacia_una_base_con_datos', 'wal_ajeno_no_tapa_la_base_buena', 'copia_diaria_no_rota_las_buenas_con_una_base_menguada',
    'wal_ajeno_con_datos_se_aparta', 'borrado_legitimo_no_aparta_su_wal', 'shm_bloqueado_no_mueve_nada', 'base_antigua_queda_con_base_id',
    'wal_ajeno_sin_identidad_se_aparta', 'wal_de_version_anterior_se_respeta_con_respaldo', 'copia_restaurada_dos_veces']) {
    assert.match(r.salida, new RegExp(`nsn_integridad_${t} \\.\\.\\. ok`), `no se ejecutó nsn_integridad_${t}`);
  }
});
