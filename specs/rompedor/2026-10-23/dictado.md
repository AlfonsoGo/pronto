Rompedor · 2026-10-23 · dictado · 1 altas, 6 medias, 7 bajas · Arreglado: ALTA 1, MEDIA 2, 3, 4, 5 y 7, BAJA 8, 9, 10, 11 y 12 (esta, a medias: el frío tras arrancar sigue) — sondas sin #[ignore]; MEDIA 4 con léxico ampliado a mano, no uno de verdad. Pendiente: MEDIA 6 (agente de datos), BAJA 13 y 14
Datos: solo bases en memoria y carpetas temporales propias (borradas al salir: 0 restos `pronto2-rompedor-dictado-*` en `%TEMP%`); `%APPDATA%\Pronto2\pronto2.db` 270 336 B / 15-oct 21:29:15 y `-wal` 1 825 192 B / 22-oct 17:05:33, iguales antes y después → restaurados ✅. Motor real en solo lectura (`%APPDATA%\Pronto2\models\parakeet`); WAV propios generados con la voz TTS de Windows («Helena», es-ES) directo a fichero en el scratchpad de la sesión, nunca por altavoces | Servidor usado: ninguno

Rama `fases/2026-10-22` (2f78f7f). Sondas en `src-tauri/src/rompedor_dictado.rs`, las de esta pasada con prefijo `rompedor_dictado_b_`:
`cargo test rompedor_dictado_ -- --include-ignored --nocapture --test-threads=1` (`CARGO_TARGET_DIR=C:/Users/usuario/dev/pt-b`).
Sin `--include-ignored`: 19 pasan, 13 ignoradas (las roturas). Las del motor real piden `PRONTO_ROMPEDOR_MODELO=%APPDATA%\Pronto2\models\parakeet` y `PRONTO_ROMPEDOR_VOZ=<carpeta con w_ollama_0..2.wav, w_asana_0..2.wav, w_pronto_0..2.wav, s_ollama.wav, s_asana.wav, s_pronto.wav>` (se generan con `System.Speech` a 16 kHz mono, como `tests/no-se-negocia/datos/generar-dictado.ps1`); sin ellas imprimen «OMITIDA» y pasan.
Medidas en perfil `test` (debug) y con otros 6 rompedores compilando en la máquina: los tiempos absolutos son orientativos; donde importa se da la razón frente a una medida hecha a la vez (antes/ahora sobre la misma base).

Huecos:
- **Máquina de estados del atajo y pegado real**: igual que en la primera pasada. `Controller`, `transcribe_and_paste`, `nothing_heard`, `emptied_by_pipeline` y `keep_emptied_dictation` son privadas de `state` y piden `AppHandle`, micro o `SendInput`. Lo de esas funciones se ve leyendo; solo se ejecuta lo público.
- **Arreglos de `inject.rs` de anoche sin ejecutar** (portapapeles real prohibido): `copy_text` cancela el pegado pendiente y restaura si falla, `paste_text` devuelve el portapapeles si `write_text` falla y `GMEM_ZEROINIT`. Se leyeron (`inject.rs:361-372`, `:376-402`, `:281`) y no se ve rotura. Sigue pasando la sonda de las funciones puras (`text_formats` ya quita el NUL).
- **Ronda «solo»**: no se pudo medir con la máquina libre. Había 6 rompedores compilando en paralelo. El MB entero por la tubería salió en 8,8-9,6 s (frente a 4,7-5,7 s en la primera pasada). No se puede decir si es carga o regresión: falta repetirlo con la máquina libre. Por eso las medidas de abajo comparan siempre frente a un control medido a la vez.
- **Release**: no medido (la primera pasada ya vio que `cargo test --release` en `pt-b` falla en `whisper-rs-sys`). No se cambió el perfil para no recompilar las dependencias de los demás rompedores.
- **Huella con la voz de una persona**: probada con voz TTS. La muestra y la frase son de la misma «voz», como pasa con el usuario y su propia voz, pero no es una voz humana real.
- Búsqueda del bloc de notas: la hace el hub en JS (`snippets.js`/`notes.js`), fuera de `db.rs`. No se probó.

## ALTA

1. **El retroceso sigue comiéndose frases normales: «quiero decir» como verbo, «Quiero decir que…» tras otra frase, «no quiero decir» + casi cualquier palabra, «mejor dicho» tras un verbo, «borra eso»/«scratch that»/«no wait» dentro de una frase** (`src-tauri/src/text.rs:301-304` `BACKTRACK_CONTEXT_BEFORE` es una lista corta; `:308` `BACKTRACK_NEGATION_AFTER` también; `:330-334` con pausa delante (frase anterior acabada en punto) no se mira el contexto, así que «X. Quiero decir que…» borra la frase X entera; `:312-315`/`:324` «borra eso»/«scratch that» no piden nada detrás ni delante). El arreglo de anoche solo cubre las 5 frases de la sonda. Se pega el texto sin esas palabras, sin aviso. Lo oído sí queda en `raw_text` del historial, así que no es pérdida total. Es la misma rotura de la ALTA 1 de anoche, por otra puerta.
   Reproducción: `rompedor_dictado_b_retroceso_se_come_frases_normales` (Medium y High, Casual).
   Evidencia (13 de 13 frases pierden palabras):
   ```
   «Hoy quiero decir algo importante.» -> «Algo importante»
   «Antes de empezar quiero decir que estoy muy contento.» -> «Que estoy muy contento»
   «Primero quiero decir gracias a todos.» -> «Gracias a todos»
   «Yo también quiero decir una cosa.» -> «Una cosa»
   «Os cuento una cosa. Quiero decir que mañana no vengo.» -> «Que mañana no vengo»
   «Gracias por venir. Quiero decir unas palabras.» -> «Unas palabras»
   «Mira, no quiero decir adiós todavía.» -> «Adiós todavía»
   «Yo no quiero decir su nombre.» -> «Su nombre»
   «Esto suena mejor dicho en voz alta.» -> «En voz alta»
   «Mi hijo siempre borra eso de la pizarra.» -> «De la pizarra»
   «There is no wait at the restaurant.» -> «At the restaurant»
   «My cat likes to scratch that old sofa.» -> «Old sofa»
   ```
   Arreglo propuesto: invertir el criterio. Que una corrección solo cuente si va aislada por pausas (coma, punto o «no,» delante y coma o pausa detrás: «…lunes, quiero decir, el martes») en vez de contar salvo que la palabra anterior esté en una lista. «Quiero decir» seguido de «que», de un artículo/determinante o de un verbo en infinitivo no es corrección. «borra eso» y «scratch that» solo cuentan como orden al final del dictado o entre pausas, no con sujeto delante («mi hijo borra eso», «to scratch that»). «no wait» solo con coma («no, wait»). Las 13 frases de la sonda sirven de regresión.

## MEDIA

2. **«no, mejor dicho» deja «Dicho,» pegado al principio** (`src-tauri/src/text.rs:291`: la alternativa `no,?\s+mejor` va antes que `mejor\s+dicho` en la regex y gana la coincidencia más a la izquierda. Lo que queda detrás empieza por «dicho»). «No, mejor dicho, …» es la forma más corriente de corregirse.
   Reproducción: `rompedor_dictado_b_no_mejor_dicho_deja_dicho`.
   Evidencia:
   ```
   «Quedamos el lunes, no, mejor dicho, el martes.» -> «Dicho, el martes»
   «Es para mañana no, mejor dicho, para el viernes.» -> «Dicho, para el viernes»
   «Llámame a las cinco. No, mejor dicho, a las seis.» -> «Dicho, a las seis»
   ```
   Arreglo propuesto: añadir `no,?\s+mejor\s+dicho` como alternativa propia, delante de `no,?\s+mejor`. Si no, después de casar «no, mejor», tragarse un «dicho» que venga justo detrás.

3. **El ajuste de mayúsculas del diccionario pisa palabras corrientes: con «Pronto» (el nombre de la app) en el diccionario, cada «pronto» se pega «Pronto»; igual «llama» con «Llama», «cursor», «claqueta», «luna», «mistral»** (`src-tauri/src/text.rs:604-611` `case_rule_ok` solo mira `is_common_phrase` (la lista corta `COMMON_WORDS`), no el léxico de `is_real_word` que 68707a4 añadió solo a «suena igual»; `:695` crea la regla `Case` y `:791` aplica la forma del diccionario a cada aparición). Es un hueco hermano de la MEDIA 2 de anoche, por otra puerta, y no lo vio la primera pasada.
   Reproducción: `rompedor_dictado_b_mayusculas_pisan_palabras_corrientes` (6 de 6); también sale con el motor real en `rompedor_dictado_b_huella_pisa_palabras_reales` («Llegaré pronto a casa. Nos vemos pronto.» → «Llegaré Pronto a casa. Nos vemos Pronto.»).
   Evidencia:
   ```
   dicc [Pronto]: «Llegaré pronto a casa, hasta pronto.» -> «Llegaré Pronto a casa, hasta Pronto» (arreglos 2)
   dicc [Llama]: «Mi madre me llama todos los días.» -> «Mi madre me Llama todos los días» (arreglos 1)
   dicc [Cursor]: «Mueve el cursor a la derecha.» -> «Mueve el Cursor a la derecha» (arreglos 1)
   dicc [Luna]: «Esta noche hay luna llena.» -> «Esta noche hay Luna llena» (arreglos 1)
   ```
   Arreglo propuesto: en `case_rule_ok` (o al aplicar la regla `Case`), no poner mayúscula a una aparición en minúsculas si la palabra es real (`is_real_word`). La regla solo debería restaurar tildes y mayúsculas internas («beltranlabs» → «BeltranLabs»), no la inicial de una palabra que existe. Además, los arreglos de estas reglas no deberían sumar en «Arreglos de Pronto».

4. **«Suena igual» sigue pisando palabras reales que no están en el léxico embebido: «sana» → «Asana», «lisa» → «Elisa», «riel» → «Ariel», «cana» → «Ocaña», «Sarah» → «Zara»** (`src-tauri/src/lexico_es.txt`: ~1000 formas; `src-tauri/src/text.rs:592-599` `is_real_word` solo conoce eso, las formas regulares de sus verbos y `COMMON_WORDS`; `:914-926` `near_key` (vocal inicial) acepta cualquier palabra que no esté ahí; el inglés solo tiene ~150 palabras en `COMMON_WORDS`).
   Reproducción: `rompedor_dictado_b_suena_igual_fuera_del_lexico` (5 de 5).
   Evidencia:
   ```
   dicc [Asana]: «Mi abuela ya está sana.» -> «Mi abuela ya está Asana» (arreglos 1)
   dicc [Elisa]: «La mesa es lisa.» -> «La mesa es Elisa» (arreglos 1)
   dicc [Ariel]: «El tren salió del riel.» -> «El tren salió del Ariel» (arreglos 1)
   dicc [Ocaña]: «Le salió una cana.» -> «Le salió una Ocaña» (arreglos 1)
   dicc [Zara]: «I met Sarah yesterday.» -> «I met Zara yesterday» (arreglos 1)
   ```
   Arreglo propuesto: un léxico de verdad (decenas de miles de formas; p. ej. una lista de frecuencias del español y otra del inglés embebidas comprimidas) en vez de una lista a mano. Hasta tenerlo, `near_key` solo cuando la palabra encontrada empieza por «y»/«ll» o la forma que falta es exactamente una variante grabada, no con cualquier palabra a la que le falte la vocal inicial.

5. **La huella de voz cambia palabras reales por la del diccionario y se come la de al lado: con «Asana» grabada 3 veces, «también está sana» se pega «también Asana»** (`src-tauri/src/voiceprint.rs:595` `decide` solo descarta `is_common_phrase(found)`, no `is_real_word`; el arreglo de anoche no tocó este camino, que corre antes de la tubería en cada dictado con muestras, `state.rs:638`; `span_for` `voiceprint.rs:534-555` cubre dos palabras). Motor real.
   Reproducción: `rompedor_dictado_b_huella_pisa_palabras_reales` (con `PRONTO_ROMPEDOR_MODELO` y `PRONTO_ROMPEDOR_VOZ`; 3 muestras `w_asana_*.wav` como «Grabar pronunciación» y la frase `s_asana.wav`, misma voz).
   Evidencia:
   ```
   Asana: crudo "Mi abuela ya está sana y su hermana también está sana."
      huella "Mi abuela ya está sana y su hermana también Asana." cambios [("está sana", "Asana", 3.2178917)]
      final "Mi abuela ya está Asana y su hermana también Asana"
   ```
   (El primer «sana» lo cambia «suena igual», MEDIA 4; el segundo, la huella, que además se lleva «está». Con «Ollama» la huella no tocó «llama»: el coste no bajó del umbral).
   Arreglo propuesto: en `decide`, descartar también el tramo si alguna de sus palabras es real (`crate::text::is_real_word`, hoy privada) y no es ya una forma de la entrada. Y no extender el tramo a una palabra entera que no forma parte de la palabra del diccionario («está»).

6. **La búsqueda del historial con `fold()` es ×16 más lenta y bloquea la conexión: con 100 000 dictados, un dictado que termina mientras el usuario busca en el hub espera ~1,5 s (debug) para guardar en el historial antes de pegar** (`src-tauri/src/db.rs:1308` `fold(text) LIKE … OR fold(app) LIKE …`: función de Rust por fila, con dos `String` nuevos por fila, sobre toda la tabla cuando hay pocos resultados; `db.rs:23` una sola `Mutex<Connection>`; `state.rs:786` `finish_dictation` guarda ANTES de pegar (regla 5) y `state.rs:635`/`:648` leen diccionario y snippets al empezar, por la misma conexión; `ui/hub/pages/home.js:147` busca a los 250 ms de dejar de teclear). Choca con la regla 3: el texto ya no aparece al soltar el atajo.
   Reproducción: `rompedor_dictado_b_busqueda_frena_el_dictado` (base temporal con 100 000 filas, un hilo buscando como el hub y diez dictados simulados: `list_words` + `list_snippets` + `add_history`) y `rompedor_dictado_b_buscar_coste_fold` (antes/ahora sobre la misma base, medidos a la vez).
   Evidencia (debug, con carga):
   ```
   antes (LIKE) «zzz» sin resultado: p50 96.9 ms · ahora (fold): p50 1671.0 ms  ×17.2
   antes (LIKE) «presupuesto»: p50 0.1 ms · ahora (fold): p50 0.9 ms  ×11.6
   dictado sin buscar: p50 0.3 ms, max 5.2 ms
   dictado mientras se busca (7 búsquedas): p50 1539.4 ms, p90 1665.6 ms, max 1794.9 ms
   ```
   Arreglo propuesto: guardar el texto plegado una vez, en una columna `search` (rellenada en `add_history`/`update` y con una migración para las filas viejas) o en una tabla FTS5 con `remove_diacritics`. Así la búsqueda vuelve a ser `LIKE` (o `MATCH`) sobre texto ya plegado. Además, buscar con una conexión propia de solo lectura (WAL lo permite) para no bloquear la del dictado.

7. **Regla 5: un dictado con voz floja que Parakeet transcribe vacío se tira sin fila ni audio, aunque el micro no esté mudo** (`src-tauri/src/state.rs:41` `SILENCE_PEAK = 0.0005`; `:715-717` `nothing_heard` devuelve «No se ha oído nada» antes de cualquier `keep_*`; `keep_failed_transcription` solo se usa cuando el motor da `Err`, `:691-701`). Si se sube el volumen, el mismo audio sí se transcribe: con su WAV guardado, «Reintentar transcripción» (o Whisper) lo recuperaría.
   Reproducción: `rompedor_dictado_b_voz_floja_sale_vacia` (motor real; el WAV de 15 s de `tests/no-se-negocia/datos` multiplicado por un factor). El descarte de `state.rs:715` no se ejecuta en la sonda (privada, pide `AppHandle`). El camino es directo y lo cubre `state::nsn_experiencia::dictado_vacio_no_se_confunde_con_micro_silenciado` (`nothing_heard("")` → «No se ha oído nada»).
   Evidencia:
   ```
   ×0.005: pico 0.0040 (umbral mudo 0.0005) -> 40 palabras: "Hola equipo. La reunión del jueves se mueve…"
   ×0.002: pico 0.0016 (umbral mudo 0.0005) -> 0 palabras: ""
   ×0.001: pico 0.0008 (umbral mudo 0.0005) -> 0 palabras: ""
   ```
   Arreglo propuesto: si `raw` sale vacío y el audio pasa el umbral de mudo con algo de voz (p. ej. `voiceprint::voice_range` o un RMS mínimo), guardarlo como transcripción fallida (`keep_failed_transcription`, con su audio) y avisar «No se ha entendido; el audio queda en el historial». Y en «Reintentar», normalizar el volumen antes de transcribir (como hace `pronounce::boost`).

## BAJA

8. **❌ BAJA 10 de la primera pasada sin arreglar: con más de 16 correcciones en un dictado, las restantes se quedan tal cual** (`src-tauri/src/text.rs:355` `while applied < 16`). Ahora es más realista: manos libres dura hasta 20 min. Reproducción: `rompedor_dictado_b_tope_16_retrocesos`. Evidencia: `20 correcciones -> quedan 4 sin aplicar; final: "… Quedamos el lunes, quiero decir, el martes. Quedamos el lunes, quiero decir, el martes."`. Arreglo propuesto: el tope solo contra bucles (p. ej. tantas vueltas como disparadores encuentre la regex al principio, o hasta que el texto deje de encogerse), no un 16 fijo.

9. **Regresión de 68707a4: una corrección sin pausa justo detrás de un pronombre ya no se aplica y «quiero decir» queda en el texto** (`src-tauri/src/text.rs:301-304`: «tu», «yo», «te», «eso»… están en `BACKTRACK_CONTEXT_BEFORE`). Reproducción: `rompedor_dictado_b_correccion_tras_pronombre`. Evidencia: `«Cómpralo tú quiero decir yo.» -> «Cómpralo tú quiero decir yo»`, `«Lo hago yo quiero decir tú.» -> «Lo hago yo quiero decir tú»` (control: `«Dáselo a ella y a ti quiero decir a él.» -> «A él»`). Arreglo propuesto: con el criterio de pausas de la ALTA 1 no hace falta la lista. Si se mantiene, quitar de ella los pronombres tónicos («tú», «yo», «él», «ella»), que no pueden ir delante de «quiero decir» como complemento.

10. **Una pregunta con «borra eso» se pega como «?»** (`src-tauri/src/text.rs:321-342`: el «¿» cuenta como texto delante, así que no queda vacío y no pasa por «Se borró todo el dictado», `state.rs:704`). Reproducción: `rompedor_dictado_b_pregunta_borra_eso`. Evidencia: `«¿Juan borra eso?» -> «?»` y `«¿Borra eso?» -> «?»`. Arreglo propuesto: no tratar como orden un disparador dentro de una pregunta (`¿…?`). Y si lo que queda tras el retroceso no tiene letras ni cifras, tratarlo como vacío.

11. **El arreglo de BAJA 5 se pasa: un «No.» como respuesta en la frase anterior anula «Pulsa enter», que se pega literal** (`src-tauri/src/text.rs:263-266`: toma la palabra anterior sin mirar si hay un fin de frase entre medias). Reproducción: `rompedor_dictado_b_no_punto_pulsa_enter` («pulsa enter» activado). Evidencia: `"¿Lo mando así? No. Pulsa enter." -> "¿Lo mando así? No. Pulsa enter." press_enter=false`. Arreglo propuesto: solo es negación si no hay `.`/`?`/`!`/`…` entre la palabra anterior y la orden.

12. **❌ BAJA 9 de la primera pasada, arreglada a medias: el diccionario se recompila entero en el primer dictado tras arrancar y tras CADA cambio del diccionario, y los snippets recompilan su regex en todos los dictados** (`src-tauri/src/text.rs:752-765`: caché de una sola versión, que se compila en el camino del dictado; `:1038` `whole_word_re(trigger)` por snippet y por dictado, sin caché; grabar una pronunciación añade una variante y cambia la versión). Reproducción: `rompedor_dictado_b_diccionario_en_frio` y `rompedor_dictado_b_tuberia_15s_coste` (texto real del WAV de 15 s). Evidencia (debug): `1000 palabras: frío 4088.8 ms, caliente 13.8 ms, tras grabar una variante 3972.5 ms`. Tubería sobre el dictado de 15 s: `sin diccionario p50 0.15 ms · 20 palabras 0.84 ms · 10 snippets 15.20 ms · 20 palabras + 10 snippets 15.81 ms`. Arreglo propuesto: compilar el diccionario y los snippets al cargarlos o cambiarlos (hilo aparte, al arrancar y en `add_word`/`update_word`/`add_snippet`…), no en el dictado. Cachear también las regex de los snippets. Además, `nsn_velocidad` mide la tubería con `words: &[]` y `snippets: &[]` (`asr.rs:845`), así que la regla 3 no ve este coste.

13. **Un disparador de snippet escrito sin tilde no se expande nunca, porque Parakeet escribe con tilde** (`src-tauri/src/text.rs:940-948` `normalize_trigger` y `:1038` `whole_word_re` no pliegan tildes). Reproducción: `rompedor_dictado_b_snippet_sin_tilde`. Evidencia: snippet «mi direccion»: `"Mi dirección." -> "Mi dirección"`, `"Te paso mi dirección ahora." -> "Te paso mi dirección ahora"`. Arreglo propuesto: comparar disparador y texto plegados (`fold`), como ya hace el ajuste de mayúsculas (`phrase_re(.., fold = true)`).

14. **La búsqueda con tildes (68707a4) no encuentra textos con tildes combinantes (NFD) ni busca bien con ellas** (`src-tauri/src/text.rs:432-449` `base_letter`/`fold` no normalizan Unicode; `db.rs:498` `register_fold`). Baja: Parakeet escribe en NFC; el NFD puede llegar de un Transform o de un texto pegado en el buscador. Reproducción: `rompedor_dictado_b_buscar_combinantes`. Evidencia: `buscar "óscar" -> ["Óscar en NFC"]` (no sale `"O\u{301}scar llega tarde"`), `buscar "begoña" -> []`, `buscar "O\u{301}scar" -> ["O\u{301}scar llega tarde"]` (no sale el NFC). Arreglo propuesto: en `fold`, quitar las marcas combinantes (U+0300–U+036F) después de bajar a minúsculas, o normalizar a NFD y quitarlas (`unicode-normalization`).

## Arreglos de la primera pasada: confirmados

Se repitieron las 15 sondas de la primera pasada (`rompedor_dictado_*` sin `_b_`, todas pasan) y los tests del arreglo (`text::`, `nsn_experiencia`, `voiceprint::`, `pronounce::`, `inject::`: 62 pasan, 0 fallan).

- ✅ ALTA 1 «quiero decir» vacía el dictado: las 5 frases salen intactas (`«Eso es justo lo que quiero decir.» -> «Eso es justo lo que quiero decir»`). Un dictado que la tubería vacía queda en el historial (`emptied_by_pipeline`, `state.rs:704`; lo cubre `nsn_experiencia`). **Pero se salta por otras frases: ALTA 1 de este informe.**
- ✅ MEDIA 2 «suena igual» con «llama», «Sara», «vaca»: `«Te llama Marta.» -> «Te llama Marta» (arreglos 0)`. **Pero se salta con palabras fuera del léxico (MEDIA 4), con el ajuste de mayúsculas (MEDIA 3) y con la huella de voz (MEDIA 5).**
- ✅ MEDIA 3 copiar/pegar último tras una transcripción fallida: `last_history_text = Some("Dictado bueno de antes")` tras el fallo y tras un transform. La parte del portapapeles (`copy_text` restaura) no se ejecutó (Huecos).
- ✅ MEDIA 4 snippet con palabra del diccionario: `"Un saludo,\nTxonso Beltrán\nBeltranLabs" (snippets 1)`, también dentro de la frase.
- ✅ BAJA 5 «no pulse enter»: `press_enter=false` y el texto se conserva. **Se pasa con «No. Pulsa enter.» (BAJA 11).**
- ✅ BAJA 6 búsqueda con mayúsculas acentuadas: `"óscar" -> [.., "Óscar llega a las tres"]`, `"ángel" -> ["Ángel no viene"]`, `"último" -> ["ÚLTIMO aviso"]`. **Pero es ×16 más lenta y frena el dictado (MEDIA 6), y no cubre el NFD (BAJA 14).**
- ✅ BAJA 7 U+E000 se come texto: `-> "Hola \u{e000} esto se pierde entero hasta que diga Txonso y sigo"`.
- ✅ BAJA 8 NUL corta el pegado: `text_formats` deja `"holamundo"` (el NUL sigue en el texto del historial, `"hola\0mundo" -> "Hola\0mundo"`, pero ya no se pega cortado).
- ❌ BAJA 9 coste del diccionario: en caliente sí (1000 palabras: p50 16,4 ms frente a 2466 ms), pero en frío y tras cada cambio sigue igual, y los snippets no tienen caché (BAJA 12).
- ❌ BAJA 10 tope de 16 retrocesos: sin cambios (BAJA 8).

## Lo que aguanta (ejecutado)

- Correcciones de verdad tras el arreglo: `«Quedamos el lunes, quiero decir, el martes.» -> «El martes»`, `«Llego a las cinco quiero decir a las seis.» -> «A las seis»`, `«Quedamos el lunes, perdón, quiero decir, el martes.» -> «El martes»`, `«Quedamos el lunes. Quiero decir, el martes.» -> «El martes»`, `«Te lo mando a ti, no, quiero decir a él.» -> «A él»`. «digo» y «no, perdón» no son disparadores (FR-02 R4 no los promete) y se dejan tal cual: `«Quedamos el lunes, digo, el martes.»` intacto.
- Snippets con diccionario: `«llamar a begona»` con «Begoña» en el diccionario no rompe nada, solo pone la tilde. Las expansiones siguen opacas (una expansión no dispara otro snippet).
- Entradas hostiles §3 en toda la tubería (sonda de la primera pasada): sin pánico, sin marcadores en la salida; U+E000/U+E001/U+E10x del usuario vuelven tal cual.
- Huella con «Ollama» (3 muestras que se oyen «Oyama.»): no tocó «me llama» ni «te llama» con el motor real.
- Coste de la huella con 9 muestras sobre 15 s de audio (debug, con carga): p50 472-539 ms. Orientativo: en release será mucho menor; no se pudo medir (Huecos).
- Motor real con poco volumen: el WAV de 15 s se transcribe entero hasta un pico de 0,004.
