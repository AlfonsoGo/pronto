# FR-04 · Motores de voz y su descarga

> Origen: observado-en-código · Objetivo al que sirve: G1 (también G3 y G2)

## Qué hace (lenguaje del usuario)
Pronto dicta con Parakeet (vale en cualquier PC) y, si mi tarjeta gráfica lo permite, con Whisper, que se equivoca menos. Elijo el motor en Configuración → Motor, descargo el que me falta y veo en todo momento cuál está en uso. Los modelos se bajan una vez y después todo funciona sin conexión.

## Requisitos (cada uno comprobable; `/rumbo` da un veredicto por línea)
- R1. Parakeet TDT 0.6B v3 int8 es el motor de fábrica y se considera instalado solo si sus cuatro ficheros tienen el tamaño exacto (`encoder.int8.onnx` 652 184 281 B, `decoder.int8.onnx` 11 845 275 B, `joiner.int8.onnx` 6 355 277 B, `tokens.txt` 93 939 B); si ya hay un modelo completo de Pronto 1 se reutiliza.
- R2. La descarga de un modelo baja cada fichero a `<fichero>.part`, comprueba su SHA-256, y solo si coincide lo renombra; con hash distinto, descarga corta o error de red borra el `.part` y avisa en español (por ejemplo «la descarga está dañada (el SHA-256 no coincide). Inténtalo de nuevo.»); los ficheros que ya están enteros se saltan; solo hay una descarga de Parakeet y una de Whisper a la vez y pedir otra de Whisper da «Ya se está descargando otro modelo; espera a que termine».
- R3. Al terminar bien la descarga de Parakeet el motor se carga sin reiniciar la app; al terminar la de un modelo de Whisper se carga solo si es el modelo elegido.
- R4. El catálogo de Whisper tiene cinco modelos con su tamaño exacto y SHA-256: large-v3-turbo 574 041 195 B (3 GB de VRAM), large-v3 1 081 140 203 B (4 GB), medium 539 212 467 B (2 GB), small 190 085 487 B (1 GB) y base 59 707 625 B (1 GB); cada uno se muestra con tamaño, dónde corre, % de errores y velocidad.
- R5. Un modelo de Whisper solo se ofrece si el procesador tiene AVX2 y hay una GPU Vulkan con al menos el 95 % de la VRAM que pide; si no, la fila dice «No cabe en este PC» con el motivo (por ejemplo «No hay una tarjeta gráfica compatible (Vulkan)»).
- R6. «Usar» guarda el motor y recarga; mientras carga, el motor anterior sigue dictando; si Whisper no puede cargar (sin modelo, sin GPU válida o sin AVX2) se carga Parakeet y el dictado nunca se queda sin motor; Configuración dice cuál está «En uso» y avisa cuando Whisper no se pudo cargar y se usa Parakeet.
- R7. Tras descargar un modelo, su fila pasa a «Usar» en cuanto termina, sin salir de la pestaña ni volver a entrar (fallo conocido 18-oct: sigue saliendo «Descargar» hasta cambiar de pestaña).
- R8. Tras reiniciar el PC y abrir Pronto, los modelos que ya están en disco con el tamaño exacto salen con «Usar», no con «Descargar» (fallo conocido 18-oct: salen con «Descargar» y al pulsar «descarga» en segundos porque el descargador ve el fichero bueno).
- R9. Con Whisper, el diccionario del usuario (hasta 40 palabras, las destacadas primero) se pasa como texto previo y se descartan las frases inventadas en silencios («Subtítulos realizados por…») y los bucles repetidos; con Parakeet el diccionario no entra al motor (solo actúa la tubería de texto).
- R10. El audio nunca sale del PC al transcribir: la única conexión de red de este módulo es bajar los modelos desde `huggingface.co`; un dictado tiene prioridad sobre una transcripción de fondo de una reunión.

## Fuera de alcance
- Medir WER propio y comparar motores con la voz del usuario (FR-11) y recomendar el motor para el PC con tarjetas tipo podio (FR-15).
- Pronunciación del diccionario y huella de voz (FR-03).
- La tarjeta «Falta el modelo de voz» del inicio solo cubre Parakeet (`ui/hub/pages/home.js:46-64`).

## Dónde vive
- `src-tauri/src/asr.rs:43-51` (elección de motor), `:175-216` (carga sin bloquear el dictado), `:225-248` (prioridad del dictado), `:314-327` (caída a Parakeet), `:351` (greedy, sin hotwords)
- `src-tauri/src/whisper.rs:36-92` (catálogo), `:99-130` (presente y disponible), `:133` (trozos de 28 s), `:185-197` (texto previo con diccionario), `:311` (alucinaciones)
- `src-tauri/src/modeldl.rs:18-32` (URL y ficheros de Parakeet con SHA y tamaños), `:96-106` (completo = tamaño exacto), `:168-181` (descarga y evento final `done` con `file` vacío), `:183-242` (`.part`, SHA-256, renombrado), `:264-290` (Parakeet), `:305-345` (Whisper, una a la vez)
- `src-tauri/src/commands.rs:52-55` (recargar al cambiar de motor), `:441-469` (`asr_options`)
- `ui/hub/pages/settings.js:47-107` (filas del motor, «Descargar»/«Usar»/«No cabe en este PC», escucha `whisper://progress`: `:102` busca el modelo por `file` y `modeldl.rs:179` emite `file` vacío en `done`, posible causa de R8), `:324-342` (descargar y usar)
