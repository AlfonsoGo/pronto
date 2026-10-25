Rompedor · 2026-10-23 · carga · 0 altas, 2 medias, 3 bajas · Arreglado: MEDIA 2 (cortacircuitos del pulido, `ollama.rs` `circuit` + `polish.rs`), BAJA 3 (plazo vencido en el cuerpo → «no respondió a tiempo»), BAJA 2 a medias (la purga de «Guardar audio» ya va en un hilo, `commands.rs`; `purge_audio` sigue lento en `db.rs`) y hub BAJA (doble clic en Reintentar: el segundo ve el mismo error, `commands.rs`). Sin arreglar: MEDIA 1 (cancelar un Transform no suelta Ollama: ureq 2 no deja cortar el socket y la marca vive en `state.rs`) y BAJA 1 (`asr.rs`, de otro). 23-oct
Datos: sin datos reales tocados. Bases y audio en `%TEMP%` (`pronto-rompedor-carga-purga-<pid>`, `pronto2-pruebas-audio-<pid>`), borrados al salir. Parakeet de `%APPDATA%\Pronto2\models\parakeet`, solo lectura. `%APPDATA%\Pronto2\pronto2.db`: 270 336 B, mtime 2026-10-15 21:29:15.377 antes y después. `-wal`: 1 825 192 B, mtime 2026-10-22 17:05:33.070 antes y después → restaurados ✅ | Servidor usado: servidores TCP falsos en 127.0.0.1 (Ollama de un hueco, colgado, parado, con goteo). Ollama real no tocado.

Zona: carga, concurrencia, Transform y pulido con Ollama lento o colgado, reintentos, cargas del motor y purga. Rama `fases/2026-10-22` (2f78f7f; los arreglos de la primera pasada están en b4e0d58). Las sondas están en `src-tauri/src/rompedor_carga.rs`, al final, en la sección «segunda pasada (2026-10-23)».
Lanzar: `cargo test rompedor_carga_ -- --test-threads=1 --nocapture`. Añade `--include-ignored` para las rotas. Motor real:
`PRONTO_ROMPEDOR_MODELO=%APPDATA%\Pronto2\models\parakeet PRONTO_ROMPEDOR_WAV=tests/no-se-negocia/datos/dictado-15s.wav`.

**CPU compartida.** Seis rompedores compilaban y probaban a la vez, con la carga de CPU entre el 29 % y el 94 % según el momento. Cada medida del motor se compara con su ronda «solo» del mismo momento, nunca con los 890 ms de CLAUDE.md:
- `rompedor_carga_motor_linea_base` (motor solo, 14,2 s de audio, 15 dictados): p50/p95 **1004/1059 ms** con la CPU al 57 % (cumple ≤ 1068 en ese momento) y **1376/1479 ms** con la CPU al 29-80 %.
- Rondas «solo» dentro de otras sondas: 974-1480 ms.

Medidas en debug. El motor es nativo y tarda lo mismo. SQLite va sin optimizar, así que la purga sería más rápida en release.

## Huecos

- **Carreras con el portapapeles al cancelar un Transform.** Leídas en el código; sin reproducción. Reproducirlas exige el portapapeles real y `SendInput`, que están prohibidos. Además, `Controller` necesita `AppHandle` y `tauri` no tiene la feature `test`.
  - (a) **Cancelar mientras se copia la selección.** `inject::capture_selection` (`inject.rs:540-585`) no se puede cancelar: espera hasta 2 s a que se suelten los modificadores, manda Ctrl+C y espera hasta 900 ms. Si el atajo de dictar llega en ese hueco, `cancel_transform` (`state.rs:546-554`) hace `restore_pending()`: devuelve el portapapeles original O y vacía `PENDING`. Después llega el Ctrl+C del Transform, el portapapeles se queda con la selección S y ya nadie guarda O. El pegado del dictado nuevo lee S como «original» y restaura S. Resultado: **O se pierde (regla 5)**. Si la restauración cae antes del Ctrl+C, `capture_selection` toma O como selección, porque el número de secuencia cambió por la propia restauración (`inject.rs:555-567`).
  - (b) **Cancelar justo después de la última comprobación.** La comprobación es `state.rs:929`. Si la cancelación llega después, `paste_text` sigue adelante (`state.rs:932`). Si la cancelación cae entre `write_text(result)` y el Ctrl+V, `restore_pending` ya ha devuelto O y **el Ctrl+V pega en la app el portapapeles del usuario**, que puede ser una contraseña. Ese hueco es `wait_modifiers_released`, hasta 2 s si el usuario ya está pulsando los modificadores del atajo. Si la cancelación llega antes de que `paste_text` mire `PENDING`, el Transform cancelado pega su resultado igualmente, en medio del dictado nuevo.
  - (c) Si `Finished` y `HoldDown` llegan a la vez, `select!` puede atender antes `HoldDown`. Entonces `cancel_transform` restaura O antes de los 600 ms y el Bloc de notas (que lee tarde) pega O en vez del resultado.
  - Arreglo propuesto: que `cancel_transform` no restaure si hay un pegado en curso. Hacer la cancelación y el tramo «comprobar → escribir → Ctrl+V» exclusivos entre sí (un `Mutex` compartido, o la marca `cancelled` mirada dentro de `paste_text` bajo `PENDING`). Que `capture_selection` mire la marca antes del Ctrl+C. Y si ya se mandó el Ctrl+C, que sea el hilo del Transform, y no el controlador, quien restaure al acabar.
- **`LoadQueue` con cargas pedidas desde dos hilos** (`asr.rs:265-293`). `modeldl.rs:511` y `:569` llaman a `load` desde el hilo de la descarga, mientras `apply_settings` lo hace desde el principal. Entre `generation += 1` y `submit` puede colarse otra llamada. Si la de época 2 entra en espera y la de época 1 la sustituye, las dos acaban obsoletas. Ningún modelo se aplica y el estado se queda en `loading: true` para siempre. La ventana es de microsegundos. Sin reproducción: `LoadQueue` es privada y `load_then` necesita `AppHandle`. Arreglo propuesto: incrementar la generación y hacer el `submit` bajo el mismo cerrojo, o que `submit` solo sustituya la que espera si la nueva época es mayor.
- **Una carga del motor que se cuelga bloquea todas las siguientes.** Antes, cada carga iba en su propio hilo. Ahora la cola es de capacidad uno. Si `create_any` no vuelve (un driver de GPU con Whisper, o el modelo en un recurso de red colgado), volver a Parakeet se queda en espera para siempre. Sin reproducción: no hay forma segura de colgar `create_any` en pruebas.
- **Doble clic en «Reintentar»** (`commands.rs:229-236`). Si el primer reintento falla («No se ha oído nada», motor sin cargar), el segundo devuelve `Ok(item)` sin cambios y el hub puede mostrarlo como un éxito. Sin reproducción: `retry_history` necesita `tauri::State`.
- **Ollama real con un modelo grande en frío.** Si el cliente corta a los 45 s, no sé si Ollama aborta también la carga del modelo. Si la aborta, el siguiente Transform vuelve a empezar la carga y nunca llega. No probado: solo se permite `/api/tags` contra el Ollama real.
- **Esc no cancela un Transform** (`state.rs:209-211`). Solo lo cancela el atajo de dictar. Esc solo se captura grabando (`hotkey::set_recording`). Visto leyendo el código.
- **Whisper** (VRAM, trozos de 30 s en el reintento): sin probar, solo Parakeet.

## MEDIA

1. **Cancelar un Transform no suelta Ollama: el pulido del dictado nuevo y el Transform siguiente hacen cola detrás del cancelado** (`src-tauri/src/state.rs:546-554` `cancel_transform` solo pone la marca; `state.rs:899-901` la marca se mira **después** de `transform_text`; `src-tauri/src/ollama.rs:17-19` y `:33-36`: la petición sigue abierta hasta que Ollama responde o vencen los 45 s).
   El arreglo de la ALTA 1 libera el controlador, pero el hilo `pronto-transform` sigue esperando a Ollama con la conexión abierta. Ollama sigue generando para un Transform que ya nadie quiere. Si Ollama atiende una petición a la vez (`OLLAMA_NUM_PARALLEL=1`), lo siguiente queda detrás:
   - el pulido del dictado nuevo vence su plazo y el texto se pega 6 s tarde y sin pulir;
   - el Transform que se lance después vence el suyo, aunque solo habría tardado 30 s.

   Es justo el caso que el arreglo quería resolver, un Ollama lento. Con Ollama colgado da igual: también fallaría.
   Reproducción: `rompedor_carga_cancelar_transform_no_suelta_ollama` (ignorada). Usa un Ollama falso de un hueco: 30 s por Transform y 0,3 s por pulido, y abandona la petición si el cliente cierra, como Ollama. Las funciones son las reales: A es `state::transform_text_within` con `TRANSFORM_TIMEOUT` (lo que sigue corriendo el hilo cancelado). A los 2,8 s llega `polish::apply` (el dictado nuevo, con el plazo de fábrica de 6 s), y después B, otro Transform.
   Evidencia:
   - `PULIDO con Ollama libre: 335 ms, pulido=true`
   - `PULIDO del dictado nuevo: 6004 ms (solo: 335), pulido=false fallo=true`
   - `TRANSFORM B lanzado a los 8.8 s: 45014 ms → Err("Ollama no respondió a tiempo") (solo tardaría 30 s)`
   - `TRANSFORM A (cancelado): siguió 30093 ms … → Ok(())`
   - Diario del Ollama falso: `transform (llegó 0.3 s): generando de 0.3 a 30.4 s; respondida`

   Con varios cancelados seguidos (Win+Alt+2 cancela el 1, porque `HoldDown` llega antes que `Transform(2)`), se encadenan hasta 45 s de peticiones fantasma cada uno.
   Arreglo propuesto: que la petición se pueda cortar. Con ureq 2 bloqueante, se puede guardar el `TcpStream` (o usar un `Agent` propio con un conector que lo exponga) y hacer `shutdown(Both)` desde `cancel_transform`. Otra opción: leer en un hilo y que el hilo del Transform espere con `select!` entre la respuesta y la marca. Y que la marca se mire **antes** de llamar a Ollama (justo tras `capture_selection`). Test: con el Ollama falso de un hueco, al cancelar A a 1 s, el diario dice «A abandonada» antes de 1,5 s y el pulido de un dictado lanzado a 2,8 s tarda ≤ 1 s.

2. **Con el pulido activado y Ollama colgado o parado, cada dictado paga el plazo entero antes de pegar, siempre: no hay cortacircuitos** (`src-tauri/src/state.rs:771`: `polish::apply` va en `finish_dictation` antes de `paste`; `src-tauri/src/polish.rs:215-231`; `src-tauri/src/ollama.rs:18`: `timeout_connect` de 3 s).
   Si Ollama se cuelga (acepta la conexión y no contesta), cada dictado tarda 6 s más (`ai_polish_timeout_ms` de fábrica). Si Ollama está parado (no arrancó con Windows, o el usuario lo cerró desde la bandeja), cada dictado tarda **2 s más**. Windows reintenta el SYN contra un puerto cerrado de loopback durante ~2 s antes de dar `ECONNREFUSED`, y el `timeout_connect` de 3 s no lo corta. No aprende del primer fallo: el dictado 50 espera lo mismo que el primero. Se cumple la letra de la regla 4 (pega sin pulir), pero no su sentido («solo se degrada esa función») ni el de la regla 3 («el texto aparece al soltar el atajo»). La 2.0.2 activa la IA local con un botón, así que el caso es habitual: PC recién encendido con Ollama aún sin arrancar.
   Reproducción: `rompedor_carga_pulido_con_ollama_colgado_o_parado` (ignorada). `polish::apply` real, 4 dictados seguidos contra un servidor colgado y contra un puerto cerrado.
   Evidencia:
   - `COLGADO, 4 dictados seguidos: [6017.0, 6007.0, 6004.0, 6002.0]`
   - `PARADO (puerto cerrado), 4 dictados: [2021.0, 2031.0, 2049.0, 2047.0]`

   Arreglo propuesto: un cortacircuitos en `polish`. Tras un fallo de conexión o de plazo, saltarse el pulido (sin esperar) durante N minutos, o hasta que un `GET /api/tags` en segundo plano vuelva a responder, y avisarlo una sola vez en la Flow bar. Para loopback, conectar con un plazo de 200-300 ms (`TcpStream::connect_timeout`): Ollama en local, si está, acepta al instante. Test: con Ollama parado, el segundo dictado con pulido activado no espera más de 50 ms en `polish::apply`; con Ollama colgado, el segundo tampoco.

## BAJA

1. **Un dictado justo después de pulsar «Reintentar transcripción» espera un trozo entero de 60 s: 4,1 s en vez de 1,0 s** (`src-tauri/src/asr.rs:360-364`: el primer trozo del reintento coge el motor con prioridad y no espera; `asr.rs:15` `MAX_CHUNK_SECS = 60`).
   Es lo que queda de la MEDIA 1 de la primera pasada, y el comentario del arreglo lo acepta («lo peor que espera ahora un dictado es lo que quede del trozo en curso»). Aun así, pasa la regla 3 entre 3 y 6 veces en el caso natural: el usuario pulsa Reintentar en el hub y se va a dictar. En manos libres, además, el primer trozo empieza aunque haya un dictado grabando.
   Reproducción: `rompedor_carga_dictado_justo_tras_pulsar_reintentar` (ignorada). Reintento de 120 s y, 1 s después, un dictado de 15 s.
   Evidencia:
   - `SOLO esta ronda: p50=1020 max=1048 ms · DICTADO 1 s después de «Reintentar»: 4118 ms`
   - En `rompedor_carga_dictado_durante_reintento_largo`, el primer dictado tarda 3823, 4674, 5512 y 6494 ms en 4 rondas, frente a una ronda solo de 1023-1480 ms.

   Arreglo propuesto: el reintento en trozos de 15 s (Parakeet transcribe igual de bien; el peor caso pasa a ~1 s) y el primer trozo también por `background_turn(Duration::ZERO)`, que espera a que no haya un dictado grabando.

2. **Bajar «Guardar audio» con mucho audio guardado congela el hilo principal 3 s y retiene la base hasta 0,6 s** (`src-tauri/src/commands.rs:84-87`: `apply_settings` llama a `purge_audio` dentro de `save_settings`, un comando síncrono que va en el hilo principal; `src-tauri/src/db.rs:1492-1502`: un `UPDATE` por fila con el cerrojo cogido, y por cada fichero `existing_audio_path`, que llama dos veces a `audio_dir()`, que hace `create_dir_all`).
   La purga horaria normal va bien: 31-44 ms con 100 000 filas y nada que borrar. Lo que pesa es la purga de golpe: al bajar los días, o al arrancar tras unas vacaciones (`main.rs:79`).
   Reproducción: `rompedor_carga_purga_masiva_y_dictado` (ignorada). Base temporal con 100 000 dictados y N audios caducados con su WAV de 1 KB. `purge_audio(7)` mientras otro hilo guarda un dictado cada 20 ms (`add_history`, lo que va antes de pegar).
   Evidencia:
   - 1400 caducados: `PURGA … 1051 ms`, guardar un dictado máx. **188 ms**.
   - 6000 caducados: **2915-3517 ms**, guardar máx. **500-595 ms**.

   Arreglo propuesto: que `apply_settings` lance la purga en un hilo y no en el comando. En `purge_audio`, un solo `UPDATE … WHERE created_at < ?1 AND has_audio = 1 RETURNING id, created_at` dentro de una transacción, y resolver `audio_dir()` una vez fuera del bucle.

3. **Si el plazo vence leyendo el cuerpo, la Flow bar dice «Respuesta de Ollama ilegible: Se produjo un error durante el intento de conexión…»** (`src-tauri/src/ollama.rs:307`). `is_timeout` (`ollama.rs:316-330`) solo se aplica a los fallos de `send_json`. El plazo funciona: corta a los 3 s. Lo que falla es el aviso: echa la culpa a una respuesta «ilegible» y sale con el texto del sistema entero. Afecta al Transform, al pulido y a las notas de reunión.
   Reproducción: `rompedor_carga_plazo_vencido_en_el_cuerpo_dice_ilegible` (ignorada). Un servidor que manda las cabeceras y medio JSON y se calla. Plazo de 2 s.
   Evidencia: `"Respuesta de Ollama ilegible: Se produjo un error durante el intento de conexión ya que la parte conectada no respondió adecuadamente tras un periodo de tiempo, … (os error 10060) at line 1 column 12"`. Con un goteo del cuerpo en trozos: `"Respuesta de Ollama ilegible: timed out reading response"`.
   Arreglo propuesto: en `into_json().map_err`, si el `io::Error` es `TimedOut`/`WouldBlock`/10060 (lo mismo que mira `is_timeout`), devolver «Ollama no respondió a tiempo».

## Arreglos de la primera pasada: confirmados

- ✅ **ALTA 1 (Transform con Ollama colgado deja el dictado muerto 10 min).**
  - `rompedor_carga_transform_con_ollama_colgado`: `TRANSFORM terminó en 3007 ms: Some("Ollama no respondió a tiempo") · fase 2 esperó Ok(3013.4) ms` (antes 600 017 ms). El mensaje ya no miente.
  - Plazo duro de verdad: `rompedor_carga_plazo_transform_con_goteo` corta un goteo del cuerpo a los 3010 ms y uno de cabeceras a los 3003 ms, con plazo de 3 s.
  - Los tests del arreglo pasan: `state::carga_transform` (5/5: el atajo interrumpe, `Finished` con id viejo no toca el dictado nuevo, aviso de fin también con pánico, plazo 30-45 s).
  - Leído en el código: `HoldDown`/`HandsFree`/`UiStart` en `Polishing` cancelan y empiezan a grabar, y el `match` mira la fase ya cambiada (`state.rs:259-264`, `:321-324`). Los ids de `Finished` cuadran: `stop`, `start_transform` y `cancel_transform` suben `job`, y `finish_applies` exige el último y la fase `Processing`/`Polishing`.
  - Lo que queda está arriba: MEDIA 1 y las carreras con el portapapeles en Huecos.
- ✅ **MEDIA 1 (reintento largo bloquea el dictado 90 s).**
  - `rompedor_carga_dictado_durante_reintento_largo` con 1200 s de audio: reintento **102 506 ms**; primer dictado **3823 ms** (antes 89 756); siguientes 966-991 ms, frente a una ronda solo de 1023/1085. Pico de memoria +414 MB (antes +811).
  - Con 300 s, 3 de 4 rondas pasan: dictados uno a uno `[5512, 1328, 1439, 1662, 1446]` y `[4674, 1120, 1082, 1054, 1045]`. La primera ronda falló la aserción de «los siguientes ≤ 1,5 × solo»: p50 1524 ms, con la CPU subiendo del 29 al 80 % por otros agentes. No tengo los valores uno a uno de esa ronda, así que lo atribuyo al ruido sin poder demostrarlo.
  - Doble clic: `commands::tests` (2/2: el segundo espera al primero, y un pánico no deja la marca). Sin interbloqueo: `retry_wait` suelta el cerrojo en `Condvar::wait`, `RetryGuard::drop` lo coge solo para quitar el id, y `spawn_blocking` usa el pool de tokio.
  - Lo que queda: BAJA 1.
- ✅ **BAJA 1 (N cambios de motor = N modelos a la vez).** Confirmado por el test unitario `asr::tests::las_cargas_del_motor_van_de_una_en_una_y_gana_la_ultima` y leyendo `load_then`/`run_load` (`asr.rs:265-329`). La sonda `rompedor_carga_cargas_del_motor_a_la_vez` llama a `create_recognizer` directamente y no pasa por la cola (`load_then` necesita `AppHandle`): no sirve para confirmarlo de punta a punta. Huecos: dos hilos a la vez y una carga colgada.
- ✅ **BAJA 2 (la purga de audio en cada dictado).** `rompedor_carga_guardar_dictado_con_100k_y_hub`: guardar sin hub p50/p95/máx **24/25/27 ms** (antes 54/71/72 en debug). `purge_audio` (64 ms en debug) ya no está en el camino. Test `guardar_el_audio_de_un_dictado_no_purga_el_historial` ✅. Con el hub pidiendo estadísticas, guardar sigue en 199/307/372 ms (antes 177/246/382). Eso no era la purga: es `stats`+`insights` reteniendo el cerrojo de la base (544 ms por vuelta), y queda para la zona de datos.
- ✅ **Hueco «`active` atascada por un pánico».** `run_guarded` manda siempre `Finished`, también con pánico dentro del aviso (`state::carga_transform::el_aviso_de_fin_llega_siempre_aunque_el_hilo_entre_en_panico`). Fuzz de la tubería: `rompedor_carga_fuzz_tuberia_sin_panico`, 3000 casos, 0 pánicos.

## Confirmado de antes (no cuenta)

- **BAJA 8 del informe -b (dictado con la fase 2 en marcha)** sigue. `rompedor_carga_todo_a_la_vez` (ignorada): solo p50/p95 974/1073 ms; todo a la vez **1698/2180/2486 ms** (14 trozos de fase 2 y 26 pronunciaciones).

## Aguanta (sondas en verde)

- **Reintento con dictados seguidos: no se cuelga ni se come los dictados** (`rompedor_carga_reintento_con_dictados_seguidos`). Un reintento de 180 s tarda 16,8 s solo. Con un dictado cada 3 s (pausas de 1,5 s, menos que la gracia de 3 s) durante 42,6 s, acaba a los 58,5 s: no avanza mientras dura la ráfaga, como está diseñado, y termina en cuanto hay hueco. Dictados durante el reintento: p50 1342 ms (solo 1361), salvo el primero, de 4643 ms (BAJA 1).
- **Plazo de 45 s del Transform**: duro también con goteo del cuerpo y de cabeceras (arriba).
- **Purga horaria**: 31-44 ms con 100 000 filas y nada que borrar, una vez por hora, fuera del dictado. `spawn_audio_purge` solo se lanza una vez, desde `spawn_controller`.
- **Línea base de la regla 3 en este momento**: `rompedor_carga_motor_linea_base` p50 1004 ms con la CPU al 57 % (cumple); 1376 ms con la CPU al 29-80 %. La diferencia es de la máquina compartida, no de la app.
