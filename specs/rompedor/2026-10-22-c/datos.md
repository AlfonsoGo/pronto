Rompedor · 2026-10-22 · datos · 1 altas, 3 medias, 4 bajas · Arreglado: ALTA 1 (-wal sin identidad: sqlite_sequence que baja, página distinta y FTS con meta), MEDIA 2 (copia con base_id propio + copia_de y aviso que pide apartar -wal/-shm), MEDIA 3 (principal de 0 B con -wal se aparta), MEDIA 4 y BAJA 6 (se decodifica la celda de meta), BAJA 5 (versión de -wal desconocida se aparta), BAJA 7 (borrados uno a uno no hacen sospechosa la copia), BAJA 8 (sin disparadores en clear/purge, journal_size_limit, -wal leído marco a marco, purge_audio por índice) · 23-oct, sin commit
Datos: copia de `%APPDATA%\Pronto2\pronto2.db` (270336 B) y su `-wal` (1825192 B) en el scratchpad de la sesión, marcados de solo lectura y borrados al terminar. `pronto2.db` 270336 B / mtime 2026-10-15T19:29:15.377Z, `-wal` 1825192 B / 2026-10-22T15:05:33.070Z y `-shm` 32768 B / 2026-10-22T14:53:51.039Z, iguales antes y después. Ningún `apartado-*` nuevo en `%APPDATA%\Pronto2`: restaurados ✅

Zona: DATOS, identidad de base y `-wal`. Rama `fases/2026-10-22`, commit 19b9574. Las líneas citadas son las de 19b9574. Mientras corría, otros rompedores hicieron dos commits (587b3b2, 10d0675) que no tocan `db.rs`. Otra sesión dejó `db.rs` modificado sin commit: `register_fold` en `:495` y `list_history`/`last_history_text`. Eso no toca la guarda, pero en el árbol de trabajo las líneas a partir de `:495` van +16 y a partir de `:961` +17. Las últimas pasadas de las sondas se compilaron con ese árbol, y salen igual que con 19b9574 limpio.
Sondas: `src-tauri/src/rompedor_datos.rs`, prefijo `rompedor_datos_`.
- `cargo test rompedor_datos_ -- --test-threads=1`: 2 ok y 10 ignoradas. Las 2 que pasan quedan como regresión: `wal_propio_no_se_aparta` y `base_real_copiada`.
- `cargo test rompedor_datos_ -- --ignored --skip rendimiento --skip wal_enorme --test-threads=1 --nocapture`: las 8 roturas fallan como se espera.
- Las medidas se lanzan solas con `--ignored`: `rompedor_datos_rendimiento_disparadores` (`PRONTO_ROMPEDOR_N`) y `rompedor_datos_wal_enorme` (`PRONTO_ROMPEDOR_WAL_MB`).

**Pregunta principal: ¿algún `-wal` propio se clasifica como ajeno? En lo que hace Pronto normalmente, no.** Pasan todos estos casos con los datos idénticos, sin aviso y sin `apartado-*` (`rompedor_datos_wal_propio_no_se_aparta`):
- varias transacciones con un volcado PASSIVE parcial (un lector con una instantánea vieja);
- `wal_checkpoint(RESTART)` completo seguido de una sola escritura (sales nuevas y marcos viejos detrás, -wal de 1,2 MB);
- RESTART a medias (`(1, 232, 212)`);
- `VACUUM` en modo WAL;
- apagón con marcos de una transacción sin confirmar (-wal de 164 KB → 1,4 MB) más medio marco al final;
- base nueva que nunca se volcó (principal de 4096 B sin `base_id`), abierta tres veces;
- `-wal` con solo escrituras FTS (0 apariciones de `base_id`: pasa por la vista);
- base v8 que migra a v9 con el volcado TRUNCATE bloqueado por un lector;
- ruta `📁 Pronto & co; ñ=1 %41`: el propio se respeta y el ajeno se aparta.

La base real (copia) se abre en 110 ms: 493 filas iguales a su vista con `-wal`, `base_id` escrito y volcado, y `-wal` a 0 B. Ninguna ruta abre la base sin pasar por `open_at`: el único otro `Connection::open*` de `src-tauri/src/` es `voiceprint.rs:936`, de solo lectura y dentro de una prueba. `wal_identity` no se sale de rango, no entra en bucle y no hace panic con ninguna de las 17 variantes de `-wal` manipulado (sondas 5 y 6). Las roturas vienen de entradas raras, de restaurar una copia y de los `-wal` que no llevan identidad.

## ALTA

1. **Un `-wal` ajeno SIN identidad (escrito por Pronto ≤ 2.0.2, el del incidente del 13-oct) sigue tapando la base buena, y el primer volcado la machaca. Pasa con la base ya identificada y en el primer arranque de 2.0.3 sobre una base de antes. No hay aviso.** (`src-tauri/src/db.rs:785-797`: con `wal_id = None` decide la vista. Si el principal tiene `base_id` y la vista enseña el mismo (`:795`, `Some(_) => None`), da el `-wal` por propio. Si el principal es de antes, solo lo aparta si la vista tiene 0 filas y nunca tuvo datos (`:796-797`). `:931-933`: la base de antes se vuelca con `TRUNCATE` en el mismo arranque.)
   Reproducción: `rompedor_datos_wal_ajeno_sin_identidad`.
   1. Una plantilla v9 sin `meta` hace de base de 2.0.2.
   2. La intrusa es la plantilla con 4 dictados escritos por una conexión sin disparadores y sin volcar.
   3. La buena es la plantilla con 20 dictados, volcados.
   4. Se copia el `-wal` de la intrusa junto a la buena y se abre con `Db::open_at`.

   Evidencia:
   - `-wal intruso: 49472 B, 'base_id' dentro: 0`
   - `buena_ya_identificada: id_principal=Some(…) buenas=20 visto_al_abrir=4 quick_check=ok aviso=false tras_cerrar=4 apartados=0`
   - `buena_de_antes_primer_arranque_2_0_3: id_principal=None buenas=20 visto_al_abrir=4 quick_check=ok aviso=false tras_cerrar=4`

   Los 20 dictados desaparecen del fichero principal y no queda nada apartado. La vista sale coherente (mismas páginas raíz) y `quick_check` no lo detecta. La base de `%APPDATA%` de Txonso es justo una base de antes con un `-wal` de 2.0.2 (1,8 MB), así que la ventana es su primer arranque con 2.0.3.

   Variante, `rompedor_datos_wal_con_otro_tamano_de_pagina`: el `-wal` ajeno sin identidad viene de una base con página de 1024 B. La vista lo da por bueno y `quick_check` falla (`Fragmentation of 3072 bytes reported as 0 on page 3`). `set_aside_base` aparta entera la base BUENA y Pronto arranca vacío con «está dañada» (`1024_sin_identidad: aviso=true intacta=false`).

   Arreglo propuesto:
   - (a) Invariante de propio, que vale con o sin `base_id`: un `-wal` propio nunca baja `sqlite_sequence`. Si para alguna tabla `seq(vista) < seq(principal)`, es ajeno. En la reproducción, 4 < 20.
   - (b) Si el tamaño de página del `-wal` (cabecera, byte 8) ≠ el del principal, es ajeno sin más.
   - (c) Hacer que toda escritura lleve `meta`, incluidas las FTS directas (en la misma transacción que su fila o con un `UPDATE meta` explícito). Así «principal con `base_id` + `-wal` con marcos y sin identidad» = ajeno, sin pasar por la vista.
   - Añadir este caso a `tests/no-se-negocia/02-integridad`.

## MEDIA

2. **Restaurar una copia de `copias\` como dice el propio aviso de Pronto, con el `-wal` de la misma base al lado (lo normal: «Salir» no cierra la conexión), hace que Pronto declare la base dañada y arranque vacío.** (`db.rs:782-784`: copia y `-wal` llevan el mismo `base_id` (`VACUUM INTO`, `:1225`), así que `(Some(_), Some(_)) => None` aplica los marcos de la base viva sobre la disposición de páginas de la copia. `:919-926` → `set_aside_base`. `:859`, y `main.rs:63`: «cierra Pronto y cópiala encima de …», sin decir nada del `-wal`/`-shm`.)
   Reproducción: `rompedor_datos_copia_restaurada_con_wal_de_la_misma_base`. Base con 400 dictados, copia del día con `daily_backup`, cambios de hoy y Pronto muere sin volcar. Luego `copy copia → pronto2.db` y `Db::open_at`.
   Evidencia, en los 3 casos:

   | Caso | -wal | Resultado |
   |---|---|---|
   | `con_paginas_libres` | 452 marcos | aviso «está dañada (malformed database schema (<uuid>))», `filas=2` (solo los transforms de fábrica), `history tras cerrar=Ok(0)`, `apartados=1` |
   | `sin_paginas_libres` | 453 marcos | igual |
   | `solo_un_cambio_sin_crecer` | 2 marcos: un `update_history_text` | igual |

   No se pierde nada: copia y `-wal` quedan en `apartado-*`. Pero el usuario que restaura ve Pronto vacío y un aviso que le manda repetir lo mismo. Al segundo intento funciona, porque el `-wal` ya es el de la base nueva y la guarda lo aparta. Es la MEDIA 3 de `-b`, con otro síntoma.
   Arreglo propuesto: que la copia diaria tenga identidad propia. Tras `VACUUM INTO`, en la copia `UPDATE meta SET value=<uuid nuevo> WHERE key='base_id'` + `copia_de=<id original>`. Así el `-wal` viejo es ajeno para la copia restaurada y la comparación de copias usa `copia_de`. Además, o en su lugar, «Restaurar copia…» en el hub, que aparte `-wal`/`-shm` con la conexión cerrada. Corregir el texto de `:859` para que diga que se aparten `pronto2.db-wal` y `-shm`.

3. **`pronto2.db` de 0 bytes con su `-wal` con datos: SQLite BORRA el `-wal` al abrir y Pronto arranca con una base vacía, sin aviso y sin apartar nada.** (`db.rs:903`: el apartado de la base de 0 B se salta si hay `-wal` > 0. `:767`: la guarda sale sin mirar. `:906`: `existed=false`, así que tampoco hay `quick_check`. SQLite, en `pagerOpenWalIfPresent`, borra el `-wal` de una base de 0 páginas.)
   Reproducción: `rompedor_datos_base_0_bytes_con_wal_propio`. 20 dictados volcados, 10 en el `-wal`, Pronto muere y `pronto2.db` se trunca a 0 B (sincronizador, disco lleno). Luego `Db::open_at`.
   Evidencia: `Ok: history=0 quick_check=ok aviso=None; -wal original (177192 B) conservado en apartado: false; apartados=0; -wal en su sitio ahora: 0 B (¿el original? false)`. Lo único que quedaba de la base, el `-wal` con los cambios desde el último volcado, se destruye sin rastro y la copia diaria no se hace (base sin datos). El disparador es raro, pero en este proyecto ya ha habido bases de 0 B (`-b` BAJA 11).
   Arreglo propuesto: en `open_at`, principal de 0 B + `-wal` > 0 → `set_aside_all(path, &["", "-wal", "-shm"])` con aviso y `LEEME`, antes de abrir con SQLite (igual que el caso sin `-wal`).

4. **Una nota (o un dictado, snippet…) que contenga `base_id` pegado a un UUID hace que Pronto aparte su propio `-wal` al siguiente arranque tras un apagón: todo lo de la sesión desaparece de Pronto y el aviso dice «Tus datos están intactos».** (`db.rs:621-632`: busca el texto `base_id` en TODOS los bytes de los marcos confirmados y se queda con la ÚLTIMA aparición seguida de 36 caracteres con forma de UUID. Los marcos de una transacción van por número de página: las páginas de desbordamiento de una fila larga van detrás de la página de `meta`.)
   Reproducción: `rompedor_datos_nota_con_base_id_aparta_su_wal`. 20 dictados volcados. Luego un dictado y una nota `"relleno "×1200 + "apunte de depuración base_id0f0e0d0c-0b0a-4908-8706-050403020100 fin"`. Pronto muere sin volcar y se reabre.
   Evidencia:
   - `nota_larga_desbordada: aviso=Some("…es de otra base (identidad 0f0e0d0c-0b0a-4908-8706-050403020100; la tuya es cb3…") apartados=1 nota_visible=false dictado_visible=false`
   - la misma nota corta (sin desbordar): `aviso=None` → depende de la disposición de páginas.

   Se aparta todo lo escrito desde el último volcado, que se hace una vez por arranque (`daily_backup` → PASSIVE): toda la sesión. La versión «ajeno → apartado» de esto es hostil (una nota preparada), pero el mismo escaneo también acepta restos de celdas borradas (ver BAJA 6).
   Arreglo propuesto: no buscar texto. Leer el `rootpage` de `meta` en el principal (`immutable`), mirar solo los marcos de ese `pgno` y decodificar la celda (cabecera de registro: clave `'base_id'`, valor TEXT de 36). Si el principal no tiene `meta`, quedarse con la ruta de la vista.

## BAJA

5. **Un `-wal` con la versión de cabecera ≠ 3007000 (y la suma bien) deja a Pronto sin arrancar, con un error que culpa a otro programa.** (`db.rs:585`: `wal_identity` lo da por vacío (`(0, None)`), pero SQLite devuelve `SQLITE_CANTOPEN`. `:912-918`: «¿la tiene abierta otro programa, o su -wal/-shm?».)
   Reproducción: `rompedor_datos_wal_manipulado`, variante `versión 3007001 (suma bien)`.
   Evidencia: `Err(no se pudo abrir …\v13\pronto2.db (¿la tiene abierta otro programa, o su -wal/-shm?); no se ha tocado nada: unable to open database file)`. Hace falta recalcular la suma de la cabecera: es manipulación o un SQLite futuro, no un bit suelto.
   Arreglo propuesto: si la cabecera tiene la suma bien y una versión desconocida, `wal_identity` lo señala como «ilegible» → se aparta con aviso, o al menos sale un mensaje claro.

6. **Si el `base_id` desaparece de la vista (otro programa borra `meta` o la fila) y se regenera, el nuevo vive solo en el `-wal`: con un apagón antes del volcado de mantenimiento, Pronto aparta su propio `-wal`.** (`db.rs:539-541`: id nuevo. `:931`: solo se vuelca si `guard.main_id.is_none()`, y aquí el principal conserva el viejo.)
   Reproducción: `rompedor_datos_meta_borrada_fuera`. Con un programa externo, `DELETE FROM meta` o `DROP TABLE meta` + 1 fila. Abre Pronto (id nuevo), añade un dictado, muere sin volcar y se reabre.
   Evidencia: `delete_meta: id_principal_antes=Some("dd79…") … base_id_nuevo=Some("0698…"); tras dictado+apagón: aviso=Some("…es de otra base (identidad 0698…") conserva=false apartados=1`, y lo mismo con `drop_meta`. La primera apertura sí lo acepta, pero porque el texto de la celda borrada sigue en la página (`wal_identity` lee restos de celdas borradas). En producción `daily_backup` → `checkpoint()` vuelca nada más arrancar, así que la ventana es corta.
   Arreglo propuesto: volcar con `TRUNCATE` también cuando `ensure_identity` escribe un id distinto del de `guard.main_id`. Lo arregla de raíz el arreglo de la MEDIA 4 (leer la celda viva).

7. **Borrar dictados uno a uno (más de la mitad) marca la copia diaria como «sospechosa»: las copias normales dejan de rotar y guardan los dictados borrados indefinidamente.** Rompe la promesa de `-b` MEDIA 6: «lo borrado sale de ellas en `keep` días». (`db.rs:1216-1219`: «borrado del usuario» solo con `meta.borrado_masivo`, que escriben `clear_history` y `purge_history` (`:1141`, `:1164`) pero no `delete_history` (`:1118-1129`).)
   Reproducción: `rompedor_datos_copias_borrado_uno_a_uno`. 20 dictados, copia normal del 01-oct, se borran 15 con `delete_history` y luego `daily_backup`.
   Evidencia: `copia de hoy=…pronto2-20261022-sospechosa.db sospechosa=true; la copia normal del 01-oct sigue con 15 dictados borrados`. Además saldrá cada día el aviso «Tu base tiene muchos menos datos…» (`main.rs:63`).
   Arreglo propuesto: con el mismo `base_id`, considerar borrado del usuario todo `sqlite_sequence` que no ha bajado respecto a la copia (una base nueva o tapada empieza de 0). O que `delete_*` también sellen `borrado_masivo`/`ultimo_borrado`.

8. **Coste de la identidad: «Borrar historial» de 100 000 filas ×35 más lento. El `-wal` crece a 24 MB y no encoge nunca, y `wal_identity` lee el `-wal` entero en memoria en cada arranque.** (`db.rs:554-568`: los disparadores por fila desactivan la optimización de `DELETE` sin `WHERE` y suman 100 000 `UPDATE meta`. `:574`: `std::fs::read` del fichero entero. No hay `journal_size_limit`.)
   Reproducción: `rompedor_datos_rendimiento_disparadores` y `rompedor_datos_wal_enorme` (sección siguiente). El hilo que borra tiene el mutex de la base ~0,6 s (debug): un dictado que llegue a la vez espera.
   Arreglo propuesto:
   - en `clear_history`/`purge_history`, que ya escriben `meta`, no hacen falta los disparadores por fila (`DROP TRIGGER temp.pronto_toque_DELETE_history` y volver a crearlo, o subir `escrituras` una vez por transacción desde `Db`);
   - `PRAGMA journal_size_limit = 4194304`;
   - leer el `-wal` marco a marco (`BufReader`) en vez de entero.

## Medidas de rendimiento

Perfil `test` (debug: Rust y SQLite sin optimizar), Ryzen 7 5800X3D, disco local. «Sin disparadores» es la misma base y el mismo SQL de 7b7d46c: `add_history` no cambió y su `clear_history` era `DELETE FROM history` sin transacción, comprobado con `git show 19b9574`. Va con una conexión sin los `TEMP TRIGGER`.

| Operación (100 000 filas) | 19b9574 (con disparadores) | 7b7d46c (sin) |
|---|---|---|
| `add_history` autocommit, total | 14,93 s | 9,99 s |
| … p50 / p95 por dictado | 115,7 / 184,4 µs | 75,6 / 129,1 µs |
| inserción en 1 transacción | 3,49 s | 2,28 s |
| «Borrar historial» | **0,628 s** | **0,018 s** |
| `-wal` tras «Borrar historial» | 4,2 MB → **24,1 MB** (no encoge) | — |
| arranque con ese `-wal` propio | `open_at` 638 ms (sin aviso) | apertura cruda + COUNT 37 ms |

Contadores y casos grandes:
- `meta.escrituras` llega a 300 001: no hay riesgo de desbordamiento real.
- `-wal` enorme (250 MB, debug):
  - base identificada: `open_at` 9618 ms, pico de memoria 21 → 265 MB, el `-wal` se queda en 250 MB (cada arranque lo vuelve a leer); la apertura cruda tarda 368 ms;
  - base de antes: `open_at` 11756 ms (incluye el `TRUNCATE` de 250 MB), el `-wal` queda en 0.
- `wal_identity` en **release**: copia literal de `db.rs:529-534` y `:573-634` en un microbench temporal, -wal sintético con sumas válidas, el mejor de 3:

  | -wal | marcos | tiempo |
  |---|---|---|
  | 2 MB | 500 | 3,7 ms |
  | 24 MB | 6100 | 45,5 ms |
  | 250 MB | 63 620 | 505,5 ms (más 250 MB de RAM pasajera) |

  Lineal y sin bucles.
- Con 17 `-wal` manipulados (0, 1, 31, 32 B, 5000 B aleatorios, solo cabecera, cabecera + 10 B, suma mala, pgno 0, marcos válidos LE/BE, página 1000, página 65536 + basura, versión distinta, ajeno, ajeno con id cambiado con y sin sumas): `open_at` en 4-16 ms, sin panic.
  - Sin la suma recalculada, el `-wal` falsificado se ignora igual que lo ignora SQLite.
  - Con la suma recalculada, se acepta y se aplica (`filas=26`). Es lo mismo que haría SQLite, así que no es una rotura de la guarda: quien puede escribir el `-wal` puede escribir la base.

## Huecos
- **Sin comparación en un worktree de 7b7d46c**: no se compiló una segunda copia del crate (whisper/Vulkan, directorio de compilación compartido con otros rompedores). Se sustituye por el A/B de la misma prueba con SQL idéntico, comprobado en el diff. Las cifras absolutas son de debug. Solo `wal_identity` está medido en release.
- `-wal` big-endian real (escrito en una máquina BE): solo una cabecera construida (`0x377f0683`), que se lee bien.
- «Dos `base_id`»: imposible con `key TEXT PRIMARY KEY`. No se probó una `meta` recreada sin clave por un programa externo.
- Segunda instancia leyendo el `-wal` mientras la primera escribe (lectura rota en `wal_identity`): no probado. El single-instance lo evita.
- Copia restaurada que SÍ pase `quick_check` con páginas mezcladas en silencio: no conseguida en 3 disposiciones (siempre «malformed schema»). No se descarta con otras disposiciones.
- Cobertura de disparadores en tablas que se creen en ejecución (hoy no hay ninguna) y en escrituras FTS sueltas de `meetingdb.rs`: solo el caso de la sonda (g). No se auditó cada llamada.
