Rompedor · 2026-10-23 · datos · 0 altas, 4 medias, 2 bajas · Arreglado: MEDIA 1 (-wal sin meta junto a base identificada: lo decide la vista, con copia previa en respaldo-*; el aviso ya no dice «intactos»), MEDIA 2 (vista que reescribe los dictados del principal = ajeno; respaldo-* antes de aplicar un -wal sin identidad), MEDIA 3 (copia restaurada toma base_id propio al abrirla; página de meta de otra disposición = ajeno), MEDIA 4 (otro linaje solo es sospechoso si está menguado; la última normal del linaje anterior queda como -anterior.db, que no rota; el aviso nombra la sospechosa más nueva), BAJA 5 (las migraciones tocan meta dentro de su transacción; optimize/ANALYZE pasan por la vista), BAJA 6 (-wal sin principal se aparta con aviso); y dictado MEDIA 6 (búsqueda por conexión propia con fold_has) · 23-oct, sin commit
Datos: copia de `%APPDATA%\Pronto2\pronto2.db` (270336 B), su `-wal` (1825192 B) y `copias\pronto2-20261023.db` (327680 B) en `rompedor-backup/20261023-datos2/`, borrada al terminar. `pronto2.db` 270336 B / 2026-10-15 21:29:15.377, `-wal` 1825192 B / 2026-10-22 17:05:33.070 y `-shm` 32768 B / 2026-10-23 12:32:35.689: iguales al empezar y al terminar. 0 `apartado-*` en `%APPDATA%\Pronto2`: restaurados ✅ | Servidor usado: ninguno

Zona: DATOS, segunda pasada. Rama `fases/2026-10-22`. Se empezó en 2f78f7f; durante la pasada HEAD pasó a aeda142, que no toca `db.rs` ni `meetingdb.rs`. Las líneas citadas son las de aeda142. Ataco los arreglos de 0e5c4d2.
Sondas: `src-tauri/src/rompedor_datos.rs`, sección «SEGUNDA PASADA», prefijo `rompedor_datos_p2_`.
- `cargo test rompedor_datos_ -- --test-threads=1`: 15 ok y 8 ignoradas (6 roturas y 2 medidas).
- `cargo test rompedor_datos_p2_ -- --ignored --test-threads=1 --nocapture`: las 6 roturas fallan como se espera.
- Base real: `PRONTO_ROMPEDOR_DATOS_REAL=<carpeta con pronto2.db y -wal>` y `PRONTO_ROMPEDOR_DATOS_REAL_COPIA=<una copias\*.db de 2.0.2>`.

**Pregunta principal: ¿escribe Pronto algo legítimo que no toque `meta`? Hoy no.** `rompedor_datos_p2_cada_escritura_lleva_meta` (regresión, pasa) hace cada una de las 28 escrituras públicas de `Db`, sola en el `-wal` tras un `TRUNCATE`, con apagón y reapertura. Todas se reconocen como propias, sin aviso:
- historial: alta, texto, audio, borrado, purgas, «Borrar historial»;
- ajustes;
- diccionario y variantes;
- snippets, notas y transforms;
- reuniones: renombrar, tareas, duración, estado, huérfanas, `replace_segments`, `save_summary`, borrar.

El falso positivo aparece solo con escrituras que no pasan por los disparadores TEMP de esta conexión:
- Pronto 2.0.2;
- un editor SQLite;
- una migración de esquema de una versión futura, FTS `optimize` o `ANALYZE`.

Ver MEDIA 1 y BAJA 5.

Además aguanta:
- restaurar una copia de 2.0.2 (sin identidad) junto al `-wal` de 2.0.3. En 3 disposiciones (400 filas con y sin huecos, y base de 5 filas) sale «deja la base ilegible» → `-wal` apartado y se ve la copia. Sonda `rompedor_datos_p2_restaurar_copia_de_2_0_2`, que queda como regresión.
- Lo mismo con la base REAL: primer arranque de 2.0.3, 15 dictados, apagón y restaurar `copias\pronto2-20261023.db`. Se ve la copia (493 filas), con aviso y `quick_check=ok` (`rompedor_datos_p2_real_restaurar_copia_de_2_0_2`).
- Copias con identidad propia: 10 días de copias → quedan 7, 7 `base_id` distintos, todas con `copia_de` = la base, `quick_check` ok y ningún `-wal`/`-shm`/`.db-parcial` de sobra (`rompedor_datos_p2_copias_con_identidad_rotan`).
- El disparador `pronto_toque_DELETE_history` vuelve tras un `clear_history`/`purge_history` que falla a mitad (rollback de la tabla temporal). La siguiente escritura lleva `meta` (`rompedor_datos_p2_disparador_vuelve_tras_borrado_fallido`).

## MEDIA

1. **Un `-wal` con cambios del usuario escritos sin los disparadores (Pronto 2.0.2 tras haber usado 2.0.3, o un editor SQLite) se aparta entero al arrancar, y el aviso dice «Tus datos están intactos».** (`src-tauri/src/db.rs:953`: con el principal identificado, `(Some(m), WalMeta::Missing)` da el `-wal` por ajeno sin mirar nada más. `db.rs:960`: texto del aviso.)
   Reproducción: `rompedor_datos_p2_wal_sin_meta_de_version_anterior`.
   1. Base creada y cerrada con `Db::open_at`: identificada y volcada.
   2. Una conexión sin los disparadores TEMP (el SQL de `add_history` de 2.0.2, o un `UPDATE` como el de DB Browser) escribe y muere sin volcar. «Salir» no cierra la conexión, y 2.0.2 no volcaba nunca.
   3. `Db::open_at`.

   Evidencia, en los 3 casos `apartados=1`:

   | Caso | -wal | Resultado |
   |---|---|---|
   | `pronto_2_0_2_dicta_5` | 61832 B | `aviso=Some("…no era suyo: no lleva la identidad de tu base (5590dc13-…): lo escribió otra…") quick_check=ok se_ve_lo_escrito=false` |
   | `pronto_2_0_2_solo_ajustes` | 4152 B | igual |
   | `editor_externo_corrige_un_dictado` | 4152 B | igual |

   Los dictados de esa sesión desaparecen de Pronto. Están en `apartado-*`, pero el usuario no tiene forma de traerlos de vuelta. La Flow bar dice «Pronto apartó un fichero raro de la base… Tus datos están intactos» (`main.rs:200`). Pasa con quien vuelve a 2.0.2 tras actualizar (reinstalar, o un instalador que falla), o con quien corrige un dictado con un editor SQLite que muere o sigue abierto. Con la vista no habría duda: el mismo `base_id` (la página de `meta` es la del principal) y ningún contador baja.
   Arreglo propuesto:
   - con `Missing` y el principal identificado, decidir por la vista, como en `foreign_by_view`: mismo `base_id`, ningún `sqlite_sequence` baja y (nuevo) ningún `id` presente en las dos con distinto `created_at`, que Pronto nunca reescribe;
   - antes de aplicar un `-wal` dudoso, COPIAR (no mover) principal y `-wal` a `apartado-*`, para que un error en cualquiera de los dos sentidos se pueda deshacer;
   - el aviso no debe decir «intactos» si lo apartado tiene filas que no están en el principal: que diga cuántas y cómo recuperarlas.

2. **Primer arranque de 2.0.3 sobre una base de antes (sin `meta`): un `-wal` ajeno sin identidad con contadores IGUALES O MÁS ALTOS que la base sigue tapándola, sin aviso, y el `TRUNCATE` de ese mismo arranque la machaca.** Es el hueco que deja el arreglo (a) de la ALTA 1 de ayer. (`db.rs:978-983`: sin `base_id` en el principal, solo cuenta `lowered_seq` o «vista con 0 filas que nunca tuvo datos». `db.rs:1133-1135`: `checkpoint_truncate`, porque el principal no tenía id.)
   Reproducción: `rompedor_datos_p2_ajeno_sin_identidad_con_mas_filas`. Es la misma plantilla que `rompedor_datos_wal_ajeno_sin_identidad`, pero la intrusa tiene 30 dictados y la buena 20.
   Evidencia:
   - `buena_de_antes_primer_arranque_2_0_3: buenas=20 visto_al_abrir=30 aviso=None tras_cerrar: history=30 filas 'buena' que quedan=0 apartados=0`;
   - con la base ya identificada se aparta bien (`buena_ya_identificada: … aviso=Some(…) quedan=20`).

   La ventana es un solo arranque por instalación (el de la actualización) y necesita que un `-wal` ajeno esté ahí en ese momento. Pero es justo el estado de la base de Txonso (principal del 15-oct sin `meta` + `-wal` de 2.0.2), y lo que se pierde son todos los datos del fichero principal: solo quedan las copias diarias.
   Arreglo propuesto:
   - en el arranque que identifica una base de antes, copiar `pronto2.db` y `-wal` a `apartado-<fecha>-antes-de-identificar\` antes de abrir. Cuesta 2 MB en el PC de Txonso y cierra el caso del todo;
   - y/o la invariante `(id, created_at)` de la MEDIA 1: un `-wal` propio nunca cambia el `created_at` de un `id` que ya existe. Con fechas reales (nanosegundos) separa cualquier otra base.

3. **Restaurar DOS veces la misma copia (la segunda sin apartar el `-wal`): la base restaurada lleva el `base_id` de la copia, así que su `-wal` pasa por propio y se aplica sobre la copia.** Según haya habido un volcado entre medias, Pronto arranca vacío con «dañada» o la restauración no hace nada, en silencio. (`db.rs:952`, `(Some(_), WalMeta::Id(_)) => None`. Al abrir una copia restaurada no se le da identidad nueva: `db.rs:1133`, `id_now == main_id`.)
   Reproducción: `rompedor_datos_p2_restaurar_la_misma_copia_dos_veces`.
   1. 400 dictados y `daily_backup`.
   2. 1.ª restauración bien hecha.
   3. `daily_backup` (solo vuelca), 50 dictados, borrados y, en un caso, `checkpoint()` + más cambios.
   4. Apagón.
   5. 2.ª restauración de la MISMA copia, sin apartar el `-wal`.

   Evidencia:
   - `con_volcado_en_la_sesion: base_id de la copia=Some("e176a7ef-…") base_id de la base viva=Some("e176a7ef-…") -wal=1829312 B → aviso=Some("La base de datos (…) está dañada (PRAGMA quick_check: *** in database main ***…") ve=NI la copia NI lo de antes (2 filas frente a 402…) apartados=1`. Pronto arranca solo con los 2 transforms de fábrica.
   - `sin_volcado_en_la_sesion: … aviso=None quick_check=ok ve=lo de antes de restaurar apartados=0`: el usuario cree que ha restaurado y sigue viendo lo de después.

   Es la MEDIA 2 de ayer por otro camino: el arreglo dio identidad propia a la copia, pero al restaurarla esa identidad pasa a la base viva.
   Arreglo propuesto: al abrir un principal con `copia_de` que aún no se ha «adoptado», escribir un `base_id` nuevo, guardar `restaurada_de=<id de la copia>` (conservando `copia_de` para el linaje) y volcar con `TRUNCATE`. Así la copia de `copias\` y la base viva nunca comparten id. Y, como ya se propuso, «Restaurar copia…» en el hub, con la conexión cerrada y `-wal`/`-shm` apartados.

4. **Tras restaurar una copia sin identidad (las de 2.0.2, que son todas las de `copias\` durante la primera semana con 2.0.3) o tras un incidente que empieza base nueva, TODAS las copias diarias salen «sospechosas» para siempre.** Cada día sale el aviso «Tu base tiene muchos menos datos que la última copia», las copias normales no rotan, y el aviso de un incidente posterior recomienda restaurar la última copia normal, de ANTES de la restauración. (`db.rs:1437-1441`: `other_base` compara con la última copia NORMAL, que ya nunca cambia. `db.rs:1015-1025`, `latest_copy`: las normales antes que las sospechosas. `main.rs:59-64`.)
   Reproducción: `rompedor_datos_p2_copias_sospechosas_para_siempre`.
   1. Base de 2.0.2 + copia de 2.0.2.
   2. 2.0.3 con 3 copias normales (16-18 oct).
   3. Se restaura bien la copia de 2.0.2, o la base pasa a 0 B y Pronto empieza otra.
   4. 4 días de `daily_backup`.
   5. Luego otro incidente (0 B).

   Evidencia, en los dos casos:
   - `días siguientes: ["20261019:SOSPECHOSA", "20261020:SOSPECHOSA", "20261021:SOSPECHOSA", "20261022:SOSPECHOSA"]`;
   - `copias normales: ["pronto2-20261015.db", … "pronto2-20261018.db"]`;
   - `tras otro incidente el aviso recomienda Some("pronto2-20261018.db")`;
   - log: `la base (36 filas de datos, identidad Some("c514…")) no se parece a la última copia buena (44 filas, identidad Some("b3bf…"))`. El tamaño no es el motivo: es solo el linaje.

   Si el usuario sigue la recomendación, pierde todo lo dictado desde la restauración, aunque esté en las `-sospechosa` más nuevas. Con los meses, «la copia buena» es cada vez más vieja.
   Arreglo propuesto:
   - si la copia sospechosa del día anterior tiene el mismo linaje que la base y no menos filas, ese linaje queda establecido: la copia de hoy es normal y se avisa una sola vez por linaje nuevo, conservando aparte la última normal del linaje viejo;
   - `latest_copy` debe recomendar la copia más reciente del linaje de la base (o la más reciente de todas) y nombrar la otra como alternativa.

## BAJA

5. **Escrituras propias sin página de `meta`: una migración de esquema (la v10 que venga), FTS `optimize` o `ANALYZE`, con apagón justo después, se apartan como ajenas.** Latente: hoy Pronto no ejecuta ninguna fuera de las tandas con `meta`. (`db.rs:1152-1157`: cada migración confirma su propia transacción en `migrate` (`db.rs:305`) y `touch_meta` va después, en otra. `db.rs:586-598`: sin disparador en `sqlite_*` ni en `_data`/`_idx` de FTS. `db.rs:953`.)
   Reproducción: `rompedor_datos_p2_escrituras_propias_sin_meta`, con la conexión de Pronto (con sus disparadores), tras un `TRUNCATE`.
   Evidencia:
   - `migracion_v10_solo_esquema: user_version=10 -wal=16512 B → aviso=Some("…no lleva la identidad de tu base…") iguales=false apartados=1`: la columna y el `user_version` de la migración desaparecen. Se volvería a migrar, pero con un aviso falso de «otra base»;
   - `fts_optimize: -wal=8272 B → aviso=Some(…) apartados=1`;
   - `analyze: -wal=16512 B → aviso=Some(…) apartados=1`.

   Arreglo propuesto:
   - `touch_meta` dentro de la transacción de cada migración (antes de su `COMMIT`);
   - regla escrita en `touch_triggers`: toda tarea de mantenimiento futura (optimize, ANALYZE, VACUUM) va en una transacción que toque `meta`;
   - pasar `rompedor_datos_p2_cada_escritura_lleva_meta` a `tests/no-se-negocia/02-integridad`, para que una escritura nueva sin `meta` rompa la batería.

6. **Un `-wal` con datos junto a una `pronto2.db` que NO existe (antivirus en cuarentena, sincronizador, el usuario la mueve para restaurar otra) se borra al arrancar, sin apartarlo y sin aviso.** Es el caso hermano de la MEDIA 3 de ayer (0 B), que no quedó cubierto. (`db.rs:1097`: solo se mira `m.len() == 0` si el fichero existe. `db.rs:930-931`: la guarda sale con `file_len == 0`. SQLite borra el `-wal` de una base de 0 páginas al abrirla.)
   Reproducción: `rompedor_datos_p2_wal_huerfano_sin_base`. 20 dictados volcados, 10 en el `-wal`, apagón, `pronto2.db` se mueve fuera y `Db::open_at`.
   Evidencia: `-wal original 177192 B → aviso=None filas=2; conservado en apartado=false; -wal en su sitio ahora=0 B`. Ese `-wal` solo sirve para forense: sin su principal no se aplica a una copia con otra disposición.
   Arreglo propuesto: en `open_at`, si el principal no existe o tiene 0 B y hay `-wal` > 0, `set_aside_all(path, &["-wal", "-shm"])` con aviso antes de `Connection::open`.

## Arreglos de la primera pasada: confirmados
Sondas de ayer, que ya corren siempre (sin `#[ignore]`):
- ✅ **ALTA 1**, `-wal` ajeno sin identidad, en el caso reproducido.
  - `rompedor_datos_wal_ajeno_sin_identidad`: `buena_ya_identificada … aviso=true tras_cerrar=20` («no lleva la identidad») y `buena_de_antes_primer_arranque_2_0_3 … aviso=true tras_cerrar=20` («el contador de «history» baja de 20 a 4»).
  - Variante de página, `rompedor_datos_wal_con_otro_tamano_de_pagina`: `1024_sin_identidad: aviso=true intacta=true` (antes apartaba la base buena).
  - Queda el hueco de la MEDIA 2: contadores que no bajan.
- ✅ **MEDIA 2**, copia restaurada con el `-wal` de la misma base. `rompedor_datos_copia_restaurada_con_wal_de_la_misma_base`, en los 3 casos: `ve=la copia quick_check=ok`, con el `-wal` apartado. La copia lleva `base_id` propio y `copia_de`. Si se restaura dos veces la misma copia, ver la MEDIA 3.
- ✅ **MEDIA 3**, principal de 0 B con `-wal`. `rompedor_datos_base_0_bytes_con_wal_propio`: `-wal original (177192 B) conservado en apartado: true`, con aviso. Si el principal falta, ver la BAJA 6.
- ✅ **MEDIA 4**, nota con `base_id`+UUID. `rompedor_datos_nota_con_base_id_aparta_su_wal`: `nota_larga_desbordada: aviso=None nota_visible=true dictado_visible=true`, y lo mismo con la corta.
- ✅ **BAJA 5**, versión de `-wal` desconocida. `rompedor_datos_wal_manipulado`: `versión 3007001 (suma bien): apartado (aviso=true, filas=22)`, y Pronto arranca. Los otros 16 `-wal` manipulados, como se esperaba, en 5-24 ms. «2 marcos válidos (no-op)» y «big-endian válido» ahora se apartan, por la regla de `meta`: es lo correcto.
- ✅ **BAJA 6**, `meta` borrada por otro programa. `rompedor_datos_meta_borrada_fuera`: `tras dictado+apagón: aviso=None conserva=true`. Cambio de comportamiento: en la 1.ª apertura, lo escrito por el otro programa (`DELETE FROM meta` + 1 fila) ahora se aparta («enseña otra base (sin identidad)»). Es razonable con `meta` manipulada; sin tocar `meta`, ver la MEDIA 1.
- ✅ **BAJA 7**, borrados uno a uno. `rompedor_datos_copias_borrado_uno_a_uno`: `sospechosa=false`.
- ✅ **BAJA 8**, coste de la identidad (perfil `test`, debug):
  - `rompedor_datos_rendimiento_disparadores`: «Borrar historial» de 100 000 filas en 0,055 s (ayer 0,628 s; sin disparadores 0,026 s). El `-wal` se queda en 4 165 352 B (ayer crecía a 24 MB). Arranque con un `-wal` propio de 17 MB: `open_at` 379 ms (ayer 638);
  - `rompedor_datos_wal_enorme` (256 MB): `de_antes: -wal 249 MB → open_at 6160 ms; pico de memoria 23→23 MB` (ayer 21 → 265 MB).
- ✅ **Base real (copia)**, `rompedor_datos_base_real_copiada`: 493 filas iguales a la vista con `-wal`, `open_at` en 38 ms, `base_id` escrito y volcado, `-wal` a 0 B.

## Huecos
- **Segunda instancia** leyendo el `-wal` mientras la primera escribe: no probado (single-instance). Por el orden de los marcos (la página de `meta` va en la misma tanda que su commit), una lectura a medias da `frames=0` o una tanda completa, no `Missing`. No está demostrado.
- **`VACUUM INTO` que falla a mitad** (disco lleno): `db.rs:1449` sale con `?` sin borrar el `.db-parcial`, y al día siguiente solo se borra el de ese día. No se pudo provocar el fallo a mitad de escritura sin llenar un disco.
- **Editor SQLite abierto** mientras arranca Pronto: no probado. El `rename` del `-wal` debería fallar en Windows y Pronto no arrancaría, con un mensaje claro.
- **`settings_backup`**, creada en mitad de una sesión sin disparador: no se forzó la ruta «ajustes con pérdida». Por lectura de código, siempre va en la misma transacción que la escritura de `settings`, que sí lo tiene.
- **FTS automerge/crisismerge en una transacción propia**: no se encontró ninguna ruta que lo haga. Siempre va dentro de la tanda de la fila; solo se probó `optimize`.
- Quedan en `%TEMP%` dos carpetas `pronto2-rompedor-datos-restaurar-20528-…` y `-21208-…` de antes de esta pasada (no son mías: otros pid y una hora anterior). No las he tocado.
