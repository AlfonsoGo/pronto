Rompedor · 2026-10-22 · dictado · 1 altas, 3 medias, 6 bajas · Arreglado: todo lo reproducido con sonda (68707a4; sus sondas pasan sin `#[ignore]`); la segunda pasada del 23-oct confirma
Datos: solo bases en memoria y carpetas temporales propias (borradas al salir); `%APPDATA%\Pronto2\pronto2.db` 270 336 B, mtime 2026-10-15 21:29:15 antes y después → restaurados ✅

Sondas: `src-tauri/src/rompedor_dictado.rs`. Lanzar con
`cargo test rompedor_dictado_ -- --include-ignored --nocapture --test-threads=1` (`CARGO_TARGET_DIR=C:/Users/usuario/dev/pt-b`).
Sin `--include-ignored`: 7 pasan, 8 ignoradas (las roturas). La de 1 M de filas pide `PRONTO_ROMPEDOR_DICTADO_1M=1`.
Medidas en perfil `test` (debug).

## ALTA

1. **«quiero decir» al final de una frase normal borra el dictado entero, y como el texto queda vacío no se pega ni se guarda en el historial ni su audio** (`src-tauri/src/text.rs:261` disparador `quiero\s+decir` sin contexto, `:291-306` corta desde el último fin de frase; `src-tauri/src/state.rs:584-586`: `nothing_heard` sale con «No se ha oído nada» ANTES de `finish_dictation`, así que no hay fila ni WAV: incumple la regla 5).
   Reproducción: `rompedor_dictado_quiero_decir_vacia_el_dictado` (limpieza media, Casual, como de fábrica).
   Evidencia:
   ```
   «Eso es justo lo que quiero decir.» -> «»
   «No sé muy bien lo que quiero decir» -> «»
   «Me has entendido mal, no es lo que quiero decir.» -> «»
   «Lo que te quiero decir es que mañana no puedo.» -> «Es que mañana no puedo»
   «Creo que eso está mejor dicho así.» -> «Así»
   ```
   Arreglo propuesto: (a) en `transcribe_and_paste`, si `raw` no está vacío pero la tubería deja el texto vacío, guardar el dictado (texto vacío + `raw`, con audio) como se hace con una transcripción fallida, y avisar «Se borró todo con "borra eso"/"quiero decir"» en vez de «No se ha oído nada»; (b) en `apply_backtrack`, no tratar como retroceso «quiero decir»/«mejor dicho» cuando no les sigue nada (o cuando van precedidos de «lo que», «es», «está») y no aplicar el retroceso si dejaría vacío un dictado sin otra frase previa.

## MEDIA

2. **«Suena igual» cambia palabras correctas y corrientes: con «Ollama» en el diccionario (el caso real de los tests de `text.rs`) cada «llama» se pega como «Ollama»; «Sara» como «Zara», «vaca» como «Baca»** (`src-tauri/src/text.rs:713-725` `near_key`: holgura de vocal inicial `yama`↔`oyama`; `:683-708` `sound_pass`; `:384-437` `COMMON_WORDS` no tiene «llama», «Sara», «vaca»).
   Reproducción: `rompedor_dictado_suena_igual_cambia_palabras_correctas`.
   Evidencia:
   ```
   dicc ["Ollama"]: «Mi madre me llama todos los días.» -> «Mi madre me Ollama todos los días» (arreglos 1)
   dicc ["Ollama"]: «Te llama Marta.» -> «Te Ollama Marta» (arreglos 1)
   dicc ["Zara"]: «Ayer cené con Sara y su hermano.» -> «Ayer cené con Zara y su hermano» (arreglos 1)
   dicc ["Baca"]: «La vaca está en el prado.» -> «La Baca está en el prado» (arreglos 1)
   ```
   Arreglo propuesto: la pasada fonética solo sobre palabras que no existan en un léxico español/inglés amplio (no la lista corta de `COMMON_WORDS`); `near_key` solo cuando la palabra encontrada no sea una palabra real («llama», «llamas», «yama»); como mínimo añadir conjugaciones de «llamar» y los homófonos v/b, s/z más frecuentes. Regresión: la sonda.

3. **Tras una transcripción fallida, «Copiar último» (Alt+Shift+X) vacía el portapapeles del usuario y dice «Copiado»; «Pegar último» pega nada** (`src-tauri/src/db.rs:1110-1116` `last_history_text`: `ORDER BY id DESC LIMIT 1` sin filtrar `text = ''` ni `transform`, aunque el doc dice «(no transform)»; `src-tauri/src/state.rs:119-124` `copy_last` → `inject::copy_text` (`inject.rs:355-357`) escribe sin guardar ni restaurar el portapapeles; la fila vacía la crea `keep_failed_transcription`, `state.rs:677`).
   Reproducción: `rompedor_dictado_ultimo_tras_transcripcion_fallida`.
   Evidencia:
   ```
   tras fallo: last_history_text = Some("")
   tras transform: last_history_text = Some("RESULTADO DEL TRANSFORM")
   ```
   Arreglo propuesto: `SELECT text FROM history WHERE text <> '' AND transform IS NULL ORDER BY id DESC LIMIT 1` (o decidir si el transform cuenta y corregir el doc); en `copy_last`, error «Aún no hay ninguna transcripción» si el texto está vacío.

4. **Un snippet cuyo disparador lleva una palabra del diccionario no se expande nunca** (`src-tauri/src/text.rs:56-58`: el diccionario va antes y cambia la palabra por un marcador protegido —también en el ajuste de mayúsculas, aunque no cambie nada, `:582`—; `:754-755` y `:766` comparan el disparador con un texto que ya tiene marcadores).
   Reproducción: `rompedor_dictado_snippet_con_palabra_del_diccionario` (palabra «Txonso», snippet «firma txonso»).
   Evidencia:
   ```
   sin diccionario: "Un saludo,\nTxonso Beltrán\nBeltranLabs"
   con «Txonso» en el diccionario: "Firma Txonso" (snippets 0)
   dentro de la frase: "Pon al final la firma Txonso y envíalo"
   ```
   Arreglo propuesto: buscar disparadores sobre el texto con los marcadores ya restaurados (o aplicar snippets antes del diccionario y proteger sus expansiones), y no proteger un ajuste de mayúsculas que no cambia nada (`found == text`).

## BAJA

5. **«No pulse enter» se toma por la orden: quita «pulse enter» del texto y pulsa Enter** (`src-tauri/src/text.rs:228`: la regex no mira un «no» delante). Reproducción: `rompedor_dictado_no_pulse_enter_pulsa_enter` (con «pulsa enter» activado). Evidencia: `"Revisa el formulario y no pulse enter."` → `-> "Revisa el formulario y no", press_enter=true`. Arreglo propuesto: no disparar si la palabra anterior es «no»/«don't»/«nunca».

6. **Buscar en el historial «óscar», «ángel», «último» no encuentra «Óscar», «Ángel», «ÚLTIMO»** (`src-tauri/src/db.rs:1102`: `LIKE` de SQLite solo pliega mayúsculas ASCII). Reproducción: `rompedor_dictado_buscar_no_pliega_mayusculas_acentuadas`. Evidencia: `buscar "óscar" -> []`, `"ángel" -> []`, `"último" -> []`; control `"Oscar" -> ["oscar sin tilde"]`. Arreglo propuesto: columna o función plegada (`lower` Unicode registrada con `create_scalar_function` + `text::fold`) para buscar.

7. **Un U+E000 (uso privado) en el texto crudo se toma por un marcador abierto y se come todo hasta el siguiente reemplazo del diccionario** (`src-tauri/src/text.rs:787-815` `restore_placeholders`; el texto de entrada no se limpia de `PH_OPEN`/`PH_CLOSE`/`U+E100..`). Reproducción: `rompedor_dictado_caracter_privado_se_come_texto`. Evidencia: `"Hola \u{E000} esto se pierde entero hasta que diga txonso y sigo."` → `"Hola Txonso y sigo"`. Baja: Parakeet/Whisper no deberían emitir uso privado. Arreglo propuesto: quitar `U+E000..U+E10F` del texto al entrar en `process`.

8. **Un NUL dentro del texto corta lo que se pega** (`src-tauri/src/inject.rs:214-218` `text_formats`: CF_UNICODETEXT se lee hasta el primer NUL). Reproducción: `rompedor_dictado_portapapeles_formatos` (compila con `rustc` el fragmento real de `inject.rs` de «// Formatos estándar» a «/// Formatos registrados», sin tocar el portapapeles). Evidencia: `texto "hola\0mundo" -> lo que lee una app (hasta el primer NUL) "hola"`; la tubería deja pasar el NUL (`"hola\0mundo" -> "Hola\0mundo"`). Baja: solo desde un snippet o reemplazo con NUL. Arreglo propuesto: quitar `\0` en `process` o en `text_formats`.

9. **El coste de la tubería crece lineal con el diccionario: compila una regex por regla en CADA dictado** (`src-tauri/src/text.rs:573-578` `dictionary_inner`, y `sound_map` en `:652-678`). Reproducción: `rompedor_dictado_diccionario_grande_coste` (frase de 16 palabras; cada palabra con 1 variante). Evidencia (debug): `0 palabras: p50 0.1 ms · 50: 124.8 ms · 200: 478.9 ms · 1000: 2466.0 ms`. La regla 3 da «resto de la tubería ~0,3 ms». La medida en release no se pudo hacer (ver Huecos), así que la cifra real es menor: por eso baja. Arreglo propuesto: cachear las regex compiladas por versión del diccionario (recompilar solo al cambiarlo) o una sola `RegexSet`/Aho-Corasick.

10. **Más de 16 disparadores de retroceso en un dictado dejan el resto en el texto pegado** (`src-tauri/src/text.rs:274`: tope de 16 vueltas). Reproducción: `rompedor_dictado_entradas_hostiles_en_toda_la_tuberia`. Evidencia: `"borra eso " × 40` → `"Borra eso borra eso … "` (24 «borra eso» pegados). Arreglo propuesto: si se agota el tope, quitar los disparadores restantes sin más vueltas.

## Lo que aguanta (ejecutado)

- Entradas §3 (vacío, espacio, NUL, RTL con U+202E, emoji con ZWJ y bandera, combinantes, `İ ı ǅ ß ŉ ﬀ ΐ`, solo signos, BOM/ZWSP/NBSP, 40 disparadores) × 4 niveles × 4 estilos × «pulsa enter»: sin pánico, sin marcadores internos en la salida, emojis ZWJ intactos (`rompedor_dictado_entradas_hostiles_en_toda_la_tuberia`, `rompedor_dictado_casos_de_longitud_variable`).
- Snippets que se expanden a otros disparadores (`uno→dos`, `dos→uno`, `tres→tres tres`): una sola pasada, `"dos uno tres tres"`, sin bucle. Disparadores solo-signos (`...`, `¿?`) se descartan.
- Diccionario con aspecto de regex (`a.b`, `(`, `.*`, `\d+`, reemplazo `X$1`): literal, sin expandir `$1`. Palabra en blanco y reemplazo vacío: ignorados.
- 1 MB de texto (52 palabras de diccionario, 20 snippets, Entusiasta): None 4,7 s, Medium 5,7 s, High 10,0 s (debug), sin pánico ni cuelgue.
- Huella de voz: `text_words` con emoji ZWJ, combinante, CJK, token vacío: rangos en límites de carácter; `decide` con 2533 aciertos fabricados: sin pánico.
- Historial: búsqueda con `%`, `_`, `\`, `%_\` literal; borrar ids `0`, `-1`, `i64::MIN`, `i64::MAX` sin error ni tocar otras filas.
- Historial con 1 M de filas (debug): primera página 0,3 ms; offset 999 950 185 ms; buscar sin resultado 410-460 ms; último 0,0 ms; añadir 0,1 ms; borrar 0,0 ms.
- Portapapeles (fragmento real compilado aparte): `save_formats` quita duplicados (`13,13` → `13`; `0xC0F1` ×2 → 1), salta privados/GDI (`0x200..0x3FF`, `2`), guarda formatos de 0 bytes; 200 MB entran y salen enteros; `restore_formats` devuelve exactamente los que fallan; 100 000 emojis ZWJ ida y vuelta exacta con su NUL final.

## Huecos

- **Máquina de estados del atajo (`state.rs` `Controller`) sin ejecutar.** Es privada del módulo `state`, necesita `AppHandle`, abre el micrófono (`Recorder::start`) y su `stop()` pega con `SendInput`; el hook (`hotkey.rs::handle`) lee `GetAsyncKeyState`. No se puede alcanzar desde `rompedor_dictado.rs` sin tocar otros ficheros ni usar teclado/micro reales. Lo que se ve leyendo, sin reproducir:
  - Toque de `Win+Alt` y, dentro de 500 ms, `Win+Alt+Espacio`: el `Win+Alt` del segundo atajo llega antes como `HoldDown` → `lock_hands_free` (`state.rs:229`), y el `Espacio` llega como `HandsFree` en manos libres → `stop()` (`:236`): para y pega al instante.
  - `Esc` a los 19 min de manos libres descarta todo sin historial ni audio (`state.rs:432-437`). FR-01 R4 lo pide así, pero choca con la regla 5.
  - Soltar a ≥ 500 ms con un micro lento de abrir (Bluetooth) puede dejar menos de 300 ms de audio y se descarta en silencio (`state.rs:389-397`).
  - Secuencias raras (soltar antes de pulsar, dos `HoldDown`, `Cmd::Finished` en cualquier fase): en la tabla de `on_hotkey`/`on_cmd` (`state.rs:225-294`) caen en `_ => {}` o vuelven a reposo. No se ve rotura.
- **`inject.rs` con el portapapeles real, sin ejecutar (prohibido):**
  - Si `write_text` falla tras `EmptyClipboard`, `paste_text` (`inject.rs:367-374`) sale con `?`: el portapapeles queda vacío hasta el siguiente pegado y el error dice «Copiado, pero no se pudo pegar…» (`state.rs:603`) aunque no se copió nada.
  - «Copiar último» dentro de los 600 ms tras un pegado: el hilo de restauración (`inject.rs:398-405`) machaca lo copiado.
  - `write_hglobal` (`inject.rs:277`) restaura un formato de 0 bytes como 1 byte sin inicializar (`GMEM_MOVEABLE` sin `ZEROINIT`).
- `finish_dictation`, `keep_dictation`, `keep_failed_transcription`, `nothing_heard` (`state.rs`) y `pronounce::verdict`/`purge_pending` son privadas: no se pueden sondear desde aquí. Las cubre `nsn_experiencia`. Con `keepAudioDays = 0` una transcripción fallida no deja nada (`state.rs:674`). Está documentado como elección del usuario.
- Medida en release del coste del diccionario (BAJA 9): `cargo test --release` en `pt-b` falla compilando `whisper-rs-sys` (cmake, `exit code: 1`, aviso de Vulkan). Queda una `pt-b/release` a medias.
- Huella con audio real (`voiceprint::correct` con muestras de voz): no probada. Solo rangos y `decide` con aciertos fabricados.
