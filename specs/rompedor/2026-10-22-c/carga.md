Rompedor · 2026-10-22 · carga · 1 altas, 1 medias, 2 bajas · Arreglado: ALTA 1, MEDIA 1, BAJA 1 y BAJA 2 (23-oct, rama fases/2026-10-22; sin commit)
Datos: sin datos reales tocados (bases y audio en `%TEMP%`, borrados al salir; Parakeet de `%APPDATA%\Pronto2\models\parakeet` solo lectura, en pruebas `modeldl::verify` no escribe marca) · `%APPDATA%\Pronto2\pronto2.db` 270 336 B, mtime 2026-10-15 21:29:15.377 antes y después; `-wal` 1 825 192 B, mtime 2026-10-22 17:05:33 antes y después → restaurados ✅

Zona: carga, concurrencia y pánicos. Sondas en `src-tauri/src/rompedor_carga.rs` (prefijo `rompedor_carga_`).
Lanzar: `cargo test rompedor_carga_ -- --test-threads=1 --nocapture` (`--ignored` para las rotas). Motor real:
`PRONTO_ROMPEDOR_MODELO=%APPDATA%\Pronto2\models\parakeet PRONTO_ROMPEDOR_WAV=tests/no-se-negocia/datos/dictado-15s.wav`.
Micrófono: `PRONTO_ROMPEDOR_MIC=1`. Sin esas variables, esas sondas no miden y pasan.

**CPU compartida.** Otros agentes compilaban y probaban a la vez (carga de CPU 76-100 %). La línea base de esta pasada
(motor solo, 14,2 s de audio, 3 rondas): p50/p95 **969/1275**, **1166/1426** y **1190/1383 ms**. Las rondas «solo» dentro
de otras sondas van de 874 a 1487 ms. El 890 ms de CLAUDE.md es con el PC en reposo. Comparo cada medida con su ronda
«solo» del mismo momento, no con 890. El motor (sherpa-onnx) es nativo y su tiempo es igual en debug. SQLite y la huella
de voz se midieron también con `opt-level=3` en `pronto`, `libsqlite3-sys` y `sha2` (el perfil release entero no compila
en un directorio aparte: `whisper-rs-sys` falla con la ruta larga).

## ALTA

1. **Un Transform con Ollama colgado deja el dictado muerto y la fase 2 de las reuniones parada 10 minutos** (`src-tauri/src/state.rs:439-456`, `src-tauri/src/ollama.rs:236-237`, `src-tauri/src/state.rs:278`, `src-tauri/src/asr.rs:129-135`).
   `start_transform` pone `active=true` y `Phase::Polishing`. Solo vuelve a reposo con `Cmd::Finished`, que el hilo `pronto-transform` manda cuando `ollama::generate` acaba, y el plazo es de 600 s. Mientras tanto, el controlador ignora el atajo (`_ => {}`: `HoldDown` en `Polishing`). `UiStart` solo vale en `Idle`. `wait_dictation_released` no deja empezar trozos de la fase 2. Incumple la regla 4: si Ollama no responde, solo debería degradarse la IA, no el dictado. Pasa igual con un Ollama vivo pero lento: un modelo grande en CPU, o uno que se está cargando.
   Reproducción: `rompedor_carga_transform_con_ollama_colgado` (ignorada). Un servidor TCP en 127.0.0.1 acepta la conexión y no contesta nunca. `ollama::generate` real corre en un hilo `pronto-transform` con la marca puesta, como `start_transform`. En otro hilo, `Engine::transcribe_background_timed` (fase 2) con la marca compartida.
   Evidencia: `TRANSFORM terminó en 600017 ms: Some("No se pudo contactar con Ollama en http://127.0.0.1:54903") · fase 2 esperó Ok(600017.262) ms`. Son 10 min sin dictado ni fase 2. El mensaje, además, es falso: sí contactó. La parte del controlador (ignora el atajo hasta `Finished`) sale de leer `state.rs:226-292`. Ejecutarla necesita `AppHandle`.
   Arreglo propuesto: plazo propio del Transform (p. ej. 30-45 s, como `ai_polish_timeout_ms`) en `run_transform_job`, con `restore_pending()` al vencer. Y no bloquear el atajo durante `Polishing`: que `HoldDown` cancele el Transform (que se descarte su resultado por `ui_epoch`) y empiece a dictar. Test: con Ollama colgado, un `HoldDown` 1 s después de lanzar el Transform empieza a grabar, y `active` vuelve a `false` en ≤ el plazo.

## MEDIA

1. **«Reintentar transcripción» de un dictado largo bloquea el dictado hasta que acaba: 90 s con 20 min de audio** (`src-tauri/src/commands.rs:193-208` → `state::transcribe_processed` → `src-tauri/src/asr.rs:262-267` `transcribe_timed`, la cola con prioridad; `asr.rs:289-311`: el cerrojo `busy` se queda cogido en todos los trozos de 60 s seguidos).
   El reintento del hub entra por la misma cola que el dictado y no suelta `busy` entre trozos. Un dictado pulsado durante el reintento espera a que se transcriba todo el audio. No pone `active`: la fase 2 sí se le cuela. Un doble clic en «Reintentar» encola dos.
   Reproducción: `rompedor_carga_dictado_durante_reintento_largo` (ignorada). Parakeet real. Un hilo transcribe N s de audio (el WAV de 15 s repetido) con `transcribe_timed`, como `retry_history`. 1,5 s después se dictan 5 × 15 s.
   Evidencia: 1200 s de audio: reintento **90 345 ms**, primer dictado **89 756 ms**, los siguientes p50 978 ms. 120 s de audio: reintento 10 076 ms, primer dictado **9 584 ms**. Memoria: pico +811 MB (20 min) y +424 MB (2 min) sobre el motor cargado.
   Arreglo propuesto: que `retry_history` (y `pronounce.rs:183`) vayan por una cola sin prioridad, o que `transcribe_locked` suelte y vuelva a coger `busy` entre trozos de 60 s cediendo a `dictation_waiting > 0`, como hace `transcribe_background_timed`. Desactivar «Reintentar» mientras hay uno en marcha. Test: dictado de 15 s ≤ 1068 ms + un trozo (≈ 4 s) con un reintento de 20 min en marcha.

## BAJA

1. **Cambiar de motor varias veces seguidas carga varios modelos a la vez: +670 MB por cada uno** (`src-tauri/src/asr.rs:205-249`: cada `load_then` lanza un `pronto-asr-load`, y la generación solo descarta la carga obsoleta al terminar, `asr.rs:221`; `src-tauri/src/commands.rs:56-64`; `ui/hub/pages/settings.js:74-79`: solo la fila elegida muestra «Cargando…», los botones «Usar» de las demás siguen activos).
   Reproducción: `rompedor_carga_cargas_del_motor_a_la_vez`. Lo que hace cada hilo de carga (`create_recognizer` de Parakeet), 1 vez y luego 4 en paralelo, con un muestreo del working set cada 50 ms.
   Evidencia: x1 pico +677 MB (21,3 s, con el SHA-256 en frío). x4 a la vez: pico **+2 644 MB** en 3,9 s, 4 bien. Pasa con Whisper (sin modelo o sin GPU cae a Parakeet) y, con Whisper elegido, con cada cambio de idioma (`asr.rs:42-51`). En un portátil de 8 GB, 6-8 clics rápidos agotan la RAM. No probado con Whisper (VRAM).
   Arreglo propuesto: una sola carga a la vez en `load_then`: si hay una en marcha, guardar solo «la última pedida» y lanzarla al acabar. Y desactivar «Usar» en todas las filas mientras `loading || switching`.

2. **Cada dictado recorre el historial entero antes de pegar (`purge_audio`): 26 ms con 100 000 dictados, y hasta 195 ms si el hub está pidiendo estadísticas** (`src-tauri/src/state.rs:737` dentro de `save_audio` ← `keep_dictation`, que va antes de pegar; `src-tauri/src/db.rs:1262-1278`: `WHERE has_audio = 1 AND datetime(created_at) < …`, sin índice usable, con el mutex de la base cogido).
   Reproducción: `rompedor_carga_guardar_dictado_con_100k_y_hub`. Base temporal con 100 000 filas (cada 5 min hacia atrás; las de la última semana con audio). Se guarda un dictado (`add_history` + `state::save_audio` con 15 s de audio), solo y con un hilo «hub» en bucle (`stats` + `insights` + `list_history(50)`).
   Evidencia (opt-level 3): `purge_audio` sola **26,2 ms**. Guardar p50/p95/máx 34/45/46 ms solo y **125/181/195 ms** con el hub (el hub tarda 341/391/400 ms por vuelta). En debug: 60 ms, 54/71/72 y 177/246/382 ms.
   Arreglo propuesto: purgar el audio una vez al arrancar y cada hora (o al cambiar `keepAudioDays`), no en cada dictado. Si se queda, `WHERE has_audio = 1 AND created_at < ?` con la fecha calculada en Rust (usa `idx_history_created`) y un índice parcial `ON history(created_at) WHERE has_audio = 1`.

## Confirmado de antes (no cuenta)

- **BAJA 8 del informe -b (dictado corto tras un trozo de la fase 2)**: sigue. `rompedor_carga_todo_a_la_vez` (ignorada), 1-2 s de atajo, 15 s de audio, 12 dictados. Ronda solo p50/p95 874/920 ms. Solo con la fase 2: **1037/1399/1496 ms**. Las pronunciaciones del hub en bucle (65 en la ronda) no lo empeoran: 895/977 ms frente a 975/1031. La tubería de texto en otro hilo tampoco: 918/1166 frente a 1050/1521. Todo junto: 1468/1941 ms frente a 1013/1129 (otra ronda 3019/3529 frente a 1487/1593, con la CPU saturada por otros).

## Aguanta (sondas en verde)

- **Pánicos (hueco del informe -b)**: `rompedor_carga_fuzz_tuberia_sin_panico` pasa por lo mismo que corren `pronto-transcribe` y `pronto-transform` con lo que devuelve el motor u Ollama: `voiceprint::correct_with_bank` (con banco y DTW 1 de cada 10), `text::process` (todos los niveles y tonos, diccionario con variantes y snippets al azar), `polish::accept`, `text::pronounce_status`, `whisper::prompt_for` y `ollama::accept_transform` (3 tipos, con `<think>`). Las entradas tienen İ, ẞ, ligaduras, combinantes, emoji, U+E000/E001, NUL, ZWSP, BOM, `$1`, metacaracteres de regex y code points al azar. **33 000 casos (semillas 22102026 y 987654321): 0 pánicos.**
- **Cerrojos**: todo `parking_lot`, así que un pánico no envenena nada. Repasado a mano el orden de bloqueo: `Engine` (recognizer → status en `transcribe_locked`; nunca al revés), `AppState.settings` (ninguna lectura anidada con escritura; las guardas de los `if` caen antes del bloque), `meeting` (active → detected; nada al revés), `hotkey` (CONFIG/STATE/CAPTURE nunca anidados al revés), `inject` (PENDING → TOKEN en los dos sitios). Ningún orden inverso.
- **Reunión de 60 min en trozos de 30 s** (`rompedor_carga_reunion_larga_memoria`): memoria plana tras calentar. RSS 1109-1110 MB y privada 1228 MB desde el trozo 20 hasta el 120. Handles 213 → 211. Trozo p50 2,2-2,7 s, sin deriva. Una de 3 h, extrapolada: igual.
- **1000 cancelaciones** (`rompedor_carga_1000_cancelaciones_del_microfono`, `Recorder::start` + `stop`, lo que hace Esc): abrir y cerrar p50/p95/máx 19/26/95 ms, 0 fallos. Hilos 11 → 11, handles 240 → 242, privada 4 → 5 MB. 50 `Recorder` más abandonados sin `stop` (el `Drop`): sus hilos terminan. El audio no se guardó.
- **Huella de voz con diccionario** (`rompedor_carga_huella_de_voz_con_diccionario`, opt-level 3): 15 s de dictado con 2 / 30 / 90 muestras: **23 / 27 / 49 ms**. 300 s: 265 / 519 / 1035 ms. Txonso tiene 2 muestras (`%APPDATA%\Pronto2\voz`, solo listado). El test de la regla 3 (`asr.rs:670`) la mide con el diccionario vacío; con 2 muestras suma ~23 ms. No rompe el umbral.
- **Motor + pronunciación**: comparten la cola con prioridad sin hambre (65 pronunciaciones intercaladas con 12 dictados, sin esperas apreciables).
- 100 000 dictados con el hub pidiendo estadísticas: el guardado no se degrada más allá de BAJA 2. El resto (`stats`/`insights`/búsqueda) ya está en el primer informe; no lo repito.

## Huecos

- **Marca `active` atascada por un pánico** (`state.rs:415-423`, `:449-457`, sin `catch_unwind`): sigue sin pánico alcanzable tras 33 000 casos. Si lo hubiera, el efecto es seguro por la semántica de Rust: no se manda `Cmd::Finished`, quedan `Processing`/`Polishing` y `active=true` hasta reiniciar, y la fase 2 espera 21 min por trozo. Lo barato es envolver el cuerpo de los dos hilos en `catch_unwind` y mandar `Finished` siempre (o una guarda con `Drop` que lo mande). El mismo efecto, sin pánico y con plazo, es la ALTA 1.
- **Hilo de transcripción colgado en el portapapeles**: `inject.rs:259` `GetClipboardData` sobre un formato con render diferido de una app colgada puede no volver nunca, y entonces `Processing` se queda para siempre. No probado: haría falta poner algo en el portapapeles real.
- **Whisper**: cargas simultáneas (VRAM) y dictado con Whisper durante la fase 2, sin probar. Solo Parakeet.
- **Controlador**: que el atajo se ignora en `Processing`/`Polishing` sale de leer `state.rs:226-292`. Ejecutar `Controller` necesita `AppHandle`, y `tauri` no tiene la feature `test`.
- **Perfil release completo**: no compila con `--profile` propio en `pt-c` (`whisper-rs-sys` falla). Usé `opt-level=3` solo en `pronto`, `libsqlite3-sys` y `sha2`.
- **Regla 3 bajo CPU compartida**: con otros procesos pesados (aquí, otros agentes), el motor solo da p50 1166-1190 ms, por encima de 1068. No es un fallo de la app, pero el umbral supone un PC en reposo. Sin reproducción en reposo en esta pasada.
