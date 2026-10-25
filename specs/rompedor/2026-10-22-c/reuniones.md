Rompedor · 2026-10-22 · reuniones · 0 altas, 3 medias, 4 bajas · Arreglado: todo lo reproducido con sonda (6da4117; sus sondas pasan sin `#[ignore]`); la segunda pasada del 23-oct confirma
Datos: solo bases en memoria (`Db::open_memory`) y WAV en `%TEMP%` (borrados) · `%APPDATA%\Pronto2\pronto2.db` 270336 B, mtime 1792092555 (15-oct 21:29) antes y después · restaurados ✅

Rama `fases/2026-10-22`. Sondas en `src-tauri/src/rompedor_reuniones.rs` (prefijo `rompedor_reuniones_`). Las que rompen
llevan `#[ignore = "rompedor: …"]`: `cargo test rompedor_reuniones_ -- --include-ignored --nocapture --test-threads=1`.
`rompedor_reuniones_tres_horas_de_interrupciones` tarda ~3 min en el perfil `test`. Sin red, sin Ollama, sin dispositivos.
No se repite lo de `2026-10-22.md` ni `2026-10-22-b.md`.

## ALTA

Ninguna confirmada.

## MEDIA

1. **Una pista de 0 bytes deja la reunión sin notas para siempre. Es una regresión del arreglo de hoy (`transcript_complete`)** (`src-tauri/src/notes.rs:63`: `WavReader::open` falla con un WAV vacío. `src-tauri/src/meeting.rs:551-561`: la pista cuenta como `failed`. `:501-503`: `complete = false`. `:589`: `PARTIAL_TRANSCRIPT` y salida).
   Cómo pasa: `WavStream` guarda la cabecera y el audio en un búfer de 8 KB, unos 0,25 s (`audio.rs:291-295`). Si Pronto o el PC se caen antes del primer volcado, el WAV se queda en 0 B. Lo mismo pasa si el hilo de esa pista se cuelga al abrir el dispositivo: `meeting.rs:201` lo da por perdido sin pararlo (`join`), y el fichero creado se queda en 0 B. La otra pista está sana.
   Reproducción: `rompedor_reuniones_pista_vacia_bloquea_las_notas_para_siempre`. Pista «Tú» de 2 min bien cerrada. Pista «Ellos» creada con `WavStream::create_new`, con 2000 muestras escritas y un `mem::forget` (como si el proceso muriera). Después, tres veces «Reintentar» tal como lo hacen `meeting.rs:510-569` y `:584-602`.
   Evidencia: `WAV de «Ellos»: 0 B`. Los 3 intentos dan `turnos=30 fallos=["…failed to fill whole buffer"] final=false`. La reunión queda `failed` con el aviso «reintenta el resumen», pero reintentar no sirve nunca. Antes de hoy, las notas salían de la pista buena. Con `meetingKeepAudio=false`, el audio tampoco se borra nunca.
   Arreglo propuesto: tratar un WAV de menos de 44 B (o sin bloque `data`) como pista vacía, `Ok((vec![], 0, 0))`, y no como fallo. Reservar `failed` para errores transitorios del motor. Además, si la misma pista falla con el mismo error en dos intentos seguidos, dar la transcripción por final con las pistas buenas y avisar.

2. **La detección automática graba TeamSpeak, ZoomIt y Discord o WhatsApp Web como reuniones. Incumple PRD §1, decisión «No graba reuniones de Discord ni de juegos: … se limita a Teams, Zoom y Meet»** (`src-tauri/src/detect.rs:68-72`: `app.contains("zoom")` y `contains("teams")` buscan una subcadena del exe. `:45-49` y `:79-84`: basta una palabra suelta en el título de *cualquier* ventana visible del navegador que usa el micro).
   Reproducción: `rompedor_reuniones_falsos_positivos_de_la_deteccion`.
   Evidencia: `TeamSpeak.exe → ["Microsoft Teams"]` («teamspeak» contiene «teams»). `ZoomIt64.exe → ["Zoom"]`. Chrome con el título `#teams | Mi servidor - Discord` → `["Microsoft Teams (Chrome)"]`. Chrome en una llamada de WhatsApp Web con otra ventana titulada «Cómo hacer zoom en fotos» → `["Zoom (Chrome)"]`. En el modo `auto` (el de fábrica, R2), esto empieza a grabar el micro y el audio del sistema sin que nadie lo pida.
   Arreglo propuesto: comparar el exe normalizado con una lista exacta (`zoom`, `ms-teams`, `msteams`, `teams`, `ciscocollabhost`, `webex`, `slack`). En el navegador, mirar solo la ventana en primer plano o la que tiene el audio, con patrones de URL o título propios de cada servicio (`Meet - xxx-xxxx-xxx`, `| Microsoft Teams`, `Zoom Meeting`), no una palabra suelta.

3. **En una reunión larga con muchas interrupciones, encadenar los turnos es cuadrático y, pasadas 1000 particiones, deja de partir: el resto de la reunión queda con quién dijo qué fuera de orden** (`src-tauri/src/notes/turns.rs:343`: `while changed && guard < 1_000`. `:346-370`: cada partición vuelve a recorrer todos los pares `i×j` desde el principio y reordena).
   Reproducción: `rompedor_reuniones_tres_horas_de_interrupciones`. Reunión de 3 h: «Ellos» habla en turnos de 7 s con 1 s de pausa y «Tú» mete una frase de 3 palabras en mitad de cada uno.
   Evidencia: `palabras: tú 4047, ellos 26980; interrupciones 1349; turnos 2698; conversation() 165.0 s (perfil test); sin partir: 349 (la primera en 8003 s)`. Desde 2 h 13 min, cada intervención del usuario queda dentro de un turno de «Ellos» que sigue después. La transcripción, las preguntas con su respuesta y «quién dijo qué» salen mal en el último tercio. Con el modelo falso, `label_turns` tarda 0,3 s y `understand` 1,0 s: el cuello es `interleave`.
   Arreglo propuesto: un solo barrido ordenado por tiempo. Para cada turno, buscar con `partition_point` los turnos de la otra persona que empiezan dentro de él y partirlo en todos esos puntos de una vez. Es O(n log n) y sin tope de iteraciones. Si se deja un tope, debe cubrir la reunión entera (por ejemplo, el número de turnos), no 1000 fijas.

## BAJA

4. **R3: una reunión empezada a mano justo después de parar a mano la automática se para sola al colgar** (`src-tauri/src/auto.rs:66-86`: con `recording` a `true` y `auto_app` puesto, la máquina no distingue la manual nueva de la automática anterior).
   Reproducción: `rompedor_reuniones_manual_tras_parar_la_auto_se_para_sola`. Empieza la automática de Teams. El usuario la para y empieza otra a mano antes del siguiente vistazo (3 s). Teams cuelga a los 60 s.
   Evidencia: `la reunión manual se para sola en t=Some(78000) ms`.
   Arreglo propuesto: pasar a `step` el id de la reunión en curso, o su `auto`, en vez de `recording: bool`, y olvidar `auto_app` si el id cambia.

5. **El Markdown exportado no escapa nada: un título o un texto con saltos de línea crea secciones y tareas falsas, y los enlaces `[x](javascript:…)` y el HTML pasan tal cual** (`src-tauri/src/summary.rs:616`: `# {title}`. `:621-680`: listas y transcripción sin escapar. `src-tauri/src/meetingdb.rs:191-192`: el título que genera el modelo solo pasa por `trim`).
   Reproducción: `rompedor_reuniones_exportar_con_texto_hostil`.
   Evidencia: con el título del modelo `Revisión\n\n## Tareas\n\n- [x] Transferir 5.000 € a ES00 0000`, el export sale con `# Revisión` / `## Tareas` / `- [x] Transferir…` antes de la fecha. Un punto clave con `\n## Decisiones\n- Pagar hoy` crea una sección «Decisiones» falsa. La transcripción conserva `[la factura](javascript:alert(1)) <img src=x onerror=alert(1)>`. El hub sí escapa (`esc`), así que el problema aparece al pegar en Notion u Obsidian.
   Arreglo propuesto: el título en una sola línea (cambiar `\s+` por un espacio al guardarlo y al exportar). En el export, escapar `[ ] ( ) < > # * _ \`` al principio de línea y en el texto, y no dejar saltos de línea dentro de un elemento de lista.

6. **«Regenerar» desmarca las tareas que el usuario ya había hecho, aunque salgan iguales** (`src-tauri/src/meetingdb.rs:194-199`: `DELETE` y nuevo `INSERT` con `done = 0`).
   Reproducción: `rompedor_reuniones_regenerar_desmarca_tareas`.
   Evidencia: `hecha antes de regenerar: true; después: false`.
   Arreglo propuesto: antes de borrar, guardar las tareas marcadas y volver a marcar las nuevas que se parezcan (`similarity ≥ 0.8`).

7. **Buscar reuniones ignora las palabras de 1 y 2 letras («IA», «Q3», «UX»), y con una búsqueda mixta salen reuniones que no las contienen** (`src-tauri/src/meetingdb.rs:44`: `chars().count() >= 3`. `:262`: `fts_query` sin esas palabras. `:273`: `LIKE` solo en el título).
   Reproducción: `rompedor_reuniones_busqueda_ignora_palabras_cortas`.
   Evidencia: `«IA» -> 0; «Q3» -> 0; «plan IA» -> ["Martes", "Lunes"]`. «Martes» solo habla del plan de vacaciones.
   Arreglo propuesto: con 2 caracteres, buscar sin prefijo (`"ia"`), o recurrir a `LIKE` también sobre `meeting_segments.text` si la consulta solo tiene palabras cortas.

## En verde (sondas que pasan)

- `rompedor_reuniones_wav_raros_de_reunion`: reunión de 0 s (44 B) → `Ok(0 palabras)`. Una muestra, byte impar, cabecera de 4 GB, cabecera a 0 y cierre brusco a 8 s → se leen las 144000 muestras. WAV a 48 kHz → error claro.
- `rompedor_reuniones_dos_seguidas_no_comparten_audio`: dos reuniones seguidas tienen WAV distintos (`meeting_{id}-{fecha}`), y borrar la primera no toca los de la segunda.
- `rompedor_reuniones_busqueda_y_titulos_hostiles`: un título de 10 000 caracteres con RLO, ZWJ, `e\u{301}`, ZWSP y NUL se relee igual. FTS con `-`, `content:`, `meeting_id:`, `AND NOT`, paréntesis, llaves, `' OR 1=1 --`, `%`, `_` y `\` no da ningún error, ni en `list_meetings` ni en `rank_meetings`. Un título de 1 MB se exporta.
- `rompedor_reuniones_ollama_hostil_no_revienta_la_fase_2`: números de turno 0, enormes, negativos, `1e300` o en texto, JSON con tipos cambiados, `[`×16 000, `{`×8 000, bloques ``` cortados: ningún pánico en pulido, etiquetas ni notas.
- `rompedor_reuniones_coste_de_listas_largas_del_modelo` y `rompedor_reuniones_resumen_en_bucle_del_modelo`: 1000 elementos por lista, 0,17 s. Un resumen en bucle de 8 KB («jaja…», «é…»), ≤ 0,02 s.

## Huecos

- `start`, `stop`, `process_meeting` y `retry_summary` piden `AppHandle`, y el crate no tiene la feature `tauri/test`: no se pueden ejecutar. **Reintentar dos veces a la vez** y **borrar mientras procesa** solo se revisaron leyendo el código: `Processing::begin` (`meeting.rs:94`) y `commands.rs:654` los protegen. Ventana sin probar: si el hilo de «Reintentar» llega antes que el de `stop`, en una reunión automática no sale «Notas listas» y no se descarta aunque sea corta (`meeting.rs:250` frente a `:720`).
- **El actualizador puede cerrar Pronto con la fase 2 en marcha**: `busy_reason` (`updater.rs:678-686`) solo mira la grabación y el dictado. Al arrancar, `close_orphan_meetings` la deja `failed` y hay que reintentar a mano. No ejecutado: `busy_reason` es privado.
- **`close_orphan_meetings` con una reunión ya grabando**: el atajo y el controlador arrancan antes que `db_maintenance` (`main.rs:221-229`, copia diaria y después cierre de huérfanas). Una reunión empezada con el atajo en ese hueco quedaría `failed` con `ended_at = started_at` hasta pararla. Sin reproducir: hace falta la app.
- **Relleno de huecos tras una suspensión**: `meeting.rs:398-410` reserva `vec![0.0; behind]` de una vez. Tras 8 h de suspensión con una reunión abierta, eso sería ~1,8 GB por pista si `Instant` cuenta el tiempo suspendido. Sin dispositivo ni suspensión en la prueba.
- **Hilo de pista colgado** (`meeting.rs:201`): no se para, no se espera al parar y su WAV puede escribirse mientras la fase 2 lo lee. Sin dispositivo.
- **Hub**: con una transcripción parcial (`transcriptFinal=false`), `ui/hub/pages/meetings.js:340` dice «Transcripción en vivo de una versión anterior», un texto que no encaja con el arreglo de hoy. No se ejecutó el hub.
- **Respuestas del modelo de cientos de KB**: con 200 KB, las notas tardaron más de 9 min (sonda abortada). A 12 KB, el tamaño posible con `num_predict` 2048 (`meeting.rs:624`), son 6,2 s en el perfil `test`. No se buscó qué paso es superlineal.
- Medidas en el perfil `test` (debug), no release. No se usaron el motor real (Parakeet/Whisper) ni WAV de más de 4 GB (~37 h, `audio.rs:303`, `u32` saturado).
