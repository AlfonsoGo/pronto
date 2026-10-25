# Pendiente (apuntado el 13-oct-2026, actualizado el 22-oct-2026)

> **22-oct-2026:** Pronto 2 pasó por `/fases auto` en la rama `fases/2026-10-22` (sin publicar). Lo que queda para
> mañana, en orden, está en `specs/FASES.md` § «PARA MAÑANA». De la lista del 19-oct: puntos 1 (motor sin repintar) y
> 2 («Pantalla 7») ARREGLADOS en la rama; 3-7 siguen abiertos (decisiones de Txonso en `specs/clientes/registro.md`).

## 1. HECHO (15-oct, contrato v11): diccionario por huella de voz
Falta: que Txonso grabe 3 veces alguna palabra en la app y comprobar el recall con muestras sueltas reales
(con muestras recortadas de frases salió ~10 %, con sintéticas sueltas ~85 %). Si es bajo, recalibrar MAX_COST.

### Pedido original
Pedido de Txonso: «tú puedas decir la palabra y lo que significa, para que si detecta lo mismo la
próxima vez, el mismo audio, sepa qué significa. Yo hablo y esa entonación, ese tono de voz, esa onda,
lo asocia a esa palabra».

Hoy (v9) se graba la palabra, Parakeet la transcribe («Cholso») y se guarda ese TEXTO como variante.
Lo que pide es guardar la HUELLA del audio y, al dictar, reconocer ese trozo de audio por cómo suena,
aunque Parakeet escriba otra cosa distinta cada vez.
Ideas a estudiar:
- Varias grabaciones por palabra (3-5) → huella acústica (MFCC / embedding del encoder de Parakeet)
  y, al dictar, buscar ese patrón en el audio con DTW o similitud de embeddings; si casa, forzar la
  palabra en ese tramo del texto (con los timestamps de tokens de sherpa-onnx).
- Alternativa: hotwords/contextual biasing de sherpa-onnx (modified_beam_search + hotwords con BPE)
  usando las grabaciones solo para validar.
- Medir siempre con WAVs reales de Txonso antes de darlo por bueno.

## 2. HECHO (15-oct): «no escucho nada» aunque Txonso sí habló
Arreglado en pronounce.rs: voz SEGUIDA ≥200 ms, 900 ms de cola, 5 s, umbral mínimo 0,22 y subida de volumen.

### Detalle original
Al grabar la pronunciación a veces dice que no se ha oído nada. Sospecha: el umbral de voz de
`pronounce.rs` (ruido de los primeros 250 ms + 0,15, mínimo 0,3 en `rms_to_level`) es alto para su
micro: en sus dictados el pico medido fue 0,02-0,13. Revisar con su nivel real; quizá usar el mismo
criterio que el dictado normal o no cortar por silencio hasta 4 s.

## 3. Sin ver en vivo
- El hub tiene que salir delante al abrir Pronto ya abierto (AllowSetForegroundWindow en `main.rs`).

## Estado
Pronto 2.1.0 instalado en el PC SIN: arreglo de «Yama», arreglo del «no escucho nada» y huella de voz (todo en el código, 202 tests verdes). Falta instalarlo.
Todo sin commit ni push en BELTRANLABS2\ProntoV2. Publicar solo cuando Txonso diga «sube».

## HECHO (17-oct-2026) · modelo más rápido para Meets (pedido de Txonso)
Problema de partida: qwen3.6 (35B MoE A3B, 23 GB) no cabe en la RTX 4070 Ti (12 GB): solo el 42 % de sus capas
va a la GPU. Resultado: `NOTES_MODELS` (ollama.rs) pasa a `gemma4:12b-it-qat` > qwen3.6 > qwen2.5:7b > …; la
elección a mano de otro modelo se respeta igual que antes. Repartir etiquetas y notas entre dos modelos NO compensa
(ver abajo): un solo modelo para pulir, etiquetar y resumir.

### Medidas (Ollama 127.0.0.1:11434, sin otra carga; 4 hilos de Parakeet; contexto 8192)
Conversación larga c (10,5 min, 108 turnos), segundos por fase y por minuto de reunión (sin transcribir):
| Modelo | En GPU | Genera | Carga | Pulir | Etiquetar | Notas | Total | s/min |
|---|---|---|---|---|---|---|---|---|
| gemma4:12b-it-qat (7,2 GB) | 100 % | 44 tok/s | 7,0 | 7,1 | 39-50 | 66-70 | 124-130 | 11,8-12,3 |
| gemma4:26b-a4b-it-qat (15 GB) | 58 % | 42 tok/s | 18 | 9-10 | 38-40 | 75 | 141-142 | 13,4-13,5 |
| qwen2.5:7b (4,7 GB) | 100 % | 76 tok/s | 4,1 | 8,6 | 28,5 | 25,9 | 67 | 6,4 |
| qwen3.6 (23 GB) | 42 % | 39 tok/s | 25 | 12 | 43-48 | 71-74 | 151-158 | 14,4-15,0 |
Con 8 de los 16 hilos de CPU ocupados por otro programa (pasa en el uso real): gemma 12B 145 s (13,8 s/min);
qwen3.6 228 s (21,7 s/min), porque gran parte de sus capas corren en CPU. Con 3 GB de VRAM reservados
(`OLLAMA_GPU_OVERHEAD`) no cambió nada (qwen3.6 sigue en 42 % GPU: 133 s). La carga cambia mucho según la caché
del disco: gemma 12B 4-7 s en caliente y 30-37 s en frío; qwen3.6 14-30 s.
En reposo qwen3.6 sale a ~13 min por hora de Meet, no a 35: los 35 min de Txonso son compatibles con el PC cargado.
Conversaciones cortas (carga incluida): a (2,2 min) 48 s gemma 12B, 62 s gemma 26B, 24 s qwen2.5, 70 s qwen3.6;
b (1,3 min) 27-72 s, 100-141 s, 16-19 s, 50-67 s.

### Calidad frente a la verdad (a×2, b×2, c×2-4 repeticiones por modelo; la transcripción es idéntica)
| Modelo | Tareas con responsable y plazo | Decisiones | Preguntas | Quién dijo qué |
|---|---|---|---|---|
| gemma 12B | a 4/4; b 3/4 (2/2: falta «preguntar informes»); c 6-7/8 | las 3 de c y las 2 de a; 0-2 de más (reuniones/envíos que no son decisiones) | 0-1 mal por reunión (c: «¿quién prepara los anuncios?» sin resolver aunque se contestó) | 0 cambios |
| gemma 26B | a 4/4; b 4/4 (2/2); c 7/8 con Marta y Jorge duplicados | pierde «lanzar el 15 de noviembre» (4 de 4) | 2-3 mal por reunión en c (contestadas dadas por sin resolver) | 0 cambios |
| qwen2.5:7b | a 4/4; b 3/4; c 6/8 (copia literal «Yo envío…») | a y b incompletas; hasta 3 de más | pierde la de la imprenta (b); 1 mal en c | 0 cambios |
| qwen3.6 | a 4/4; b 4/4 y 3/4; c 7/8 (4 de 4) | completas salvo 2 de 8 veces; 0-4 de más | 0-1 mal | 0 cambios |
Las tareas que salen llevan siempre responsable y plazo correctos (ninguna inventada, ninguna con otro hablante). El
fallo común es «preguntar al cliente informes semanales/mensuales»: `fix_speakers` lo da por listado porque comparte
«presupuesto» con «Enviar el presupuesto» (regla «mismo responsable y una palabra de peso en común»).
Repartir: pulir+etiquetar con qwen2.5:7b y notas con el 26B: c 132 s (12,6 s/min), 8/8 tareas pero 2 preguntas mal
marcadas, y en b una pregunta perdida; ahorra ~8 s y gasta 15 s más de carga. Con gemma 12B → qwen3.6 no se probó
porque etiquetar tarda lo mismo en los dos (39-50 s frente a 43-48 s). Todos generan ~40 tok/s: el tiempo manda el
número de tokens que se escriben, no el modelo.

### Siguiente (no hecho, fuera de este pedido)
- Mejoras 2-5 de Meets siguen sin implementar (eco por audio, nombre de «Ellos», calibración con Meet real, notas
  antes de terminar).

## HECHO (17-oct-2026) · contrato v12.2: etiquetas compactas, solape y tarea «preguntar informes»
Las tres mejoras de «Siguiente» de arriba (ver el texto del contrato v12.2 en el informe; ARQUITECTURA.md sin tocar).
- `notes/turns.rs`: formato compacto `5 r 4` (p/r/pr/d/t/o) con parser tolerante (`parse_compact`) y respaldo al JSON
  de v12; `label_prompt` enseña `LABEL_CONTEXT` = 6 turnos de contexto a cada lado del bloque de 40 (sin etiquetarlos);
  un «propongo… ¿te parece bien?» ya no pierde su pregunta.
- `notes.rs`: `label_turns` sin cambios de firma, pero su `llm` ya no fuerza JSON (`meeting.rs`: `llm_labels`, tope de
  1024 tokens); `fix_speakers` compara la ACCIÓN (`same_action`: raíz del verbo en infinitivo) y no una palabra común, y
  una tarea rescatada con «lo» sin tema lleva la pregunta a la que contesta (`with_topic`); una pregunta que la síntesis
  da por abierta sigue contestada si la respuesta resuelve algo (`evasive` detecta «no lo sé», «todavía no»…).
- Batería nueva `bateria_etiquetas` (`PRONTO_LABELS_JSON=<salida_*.json> [PRONTO_ROUNDS=n]`): fase de etiquetas sola,
  formato antiguo frente a nuevo alternados, con tokens de Ollama. `PRONTO_LLM_TIMEOUT` (s) en `bateria_meets`.

### Medidas (gemma4:12b-it-qat; conversación c = 10,5 min, 104 turnos, 3 bloques)
Etiquetas solas, alternando antiguo/nuevo en el mismo rato (`bateria_etiquetas`, 2 rondas):
| Formato | Llamadas | Tokens de entrada | Tokens escritos | Tiempo |
|---|---|---|---|---|
| JSON por turno, sin contexto | 3 | 4209 | 2724 | 64,7 s (2.ª ronda; la 1.ª salió a 343 s por la GPU compartida) |
| Compacto con contexto de 6 turnos | 3 | 5197 | 451 | 11,1-12,0 s |
Escribe un 83 % menos y el contexto cuesta ~1000 tokens de entrada (se procesan en <1 s). Ahorro ≈ 50 s por 10,5 min.
Batería completa (2 repeticiones por conversación, misma transcripción; antes = mediciones de la mañana con la GPU libre; columna de total = s por minuto de reunión sin contar la carga del modelo, que cambia mucho con la caché del disco):
| Conv. | Etiquetar antes → ahora | Total sin carga antes → ahora | Tareas con responsable y plazo | Preguntas mal |
|---|---|---|---|---|
| c | 39-50 s → 11,7-11,9 s | 11,1-11,7 → 7,4-7,7 s/min | 6-7/8 → 8/8 y 7/8 | 1 y 0 → 0 y 0 |
| b | 3,5-9 s → 1,5 s | 17,2 → 16,7-16,9 s/min | 3/4 y 3/4 → 4/4 y 4/4 | 0 y 0 → 0 y 1 |
| a | 14,5 s → 1,9 s | 18,7 → 12,0 s/min (1 repetición; antes 2) | 4/4 → 4/4 | 0 → 0 |
Sin cambios de hablante (0 de 222/127/1048 palabras al otro, salvo 2 de 1048 en c, igual que antes), 0 inversiones de orden.
Decisiones: 3/3 en c salvo una repetición (2/3: el modelo perdió «lanzar el 15», fase de notas, no tocada). La pregunta
«¿quién prepara los anuncios?» queda contestada en las 6 repeticiones nuevas (antes, sin resolver en 1 de 2).
Cuidado al comparar tiempos: durante las pruebas la GPU la compartían procesos Python con CUDA de otro trabajo de
Txonso (hubo tandas a 0,4-7 tok/s en vez de 44); por eso la comparación limpia son los tokens y las rondas alternadas.
Causa real de la pregunta «sin resolver»: en los registros el enlace pregunta-respuesta de las etiquetas existía
(salía «SIN RESOLVER» junto a la respuesta); la marcaba `questions_with_status` porque las notas por bloques (5000
caracteres, otro troceado que sigue sin solape) la daban por abierta. El solape de las etiquetas ayuda a que una
pregunta del final de un bloque vea su respuesta; la regla de `evasive` cubre además el corte de los bloques de notas.
Pendiente: «¿Te parece bien?» y preguntas sin signos dependen de cómo etiquete el modelo; solape en los bloques de notas.

## HECHO (17-oct-2026, contrato v13) · Whisper como segundo motor de voz (Pronto 2.2)

Hecho como se planeó abajo; detalle y medidas en `docs/ARQUITECTURA.md` § Contrato v13. Queda: probarlo en uso real
(dictado y un Meet con Whisper elegido) y, si va bien, publicar la 2.2.

### Plan aprobado

Objetivo: más precisión donde el equipo lo permita, sin dejar de funcionar en casi todos los PCs.
- **Dictado:** Parakeet sigue por defecto (va en cualquier CPU y procesa solo lo hablado). Whisper
  large-v3-turbo como «motor preciso» opcional, ofrecido solo si Pronto detecta GPU con Vulkan y ≥ 3 GB de VRAM.
- **Meets:** Whisper turbo en la fase 2 (va en segundo plano, la latencia no importa). Con GPU, turbo; sin GPU,
  elegir entre turbo en CPU (más lento) o Parakeet, según lo que diga la comparativa.
- **PCs flojos:** Parakeet. Whisper small/base en español suele quedar por debajo; no se ofrece.
- **Motor en Rust:** whisper.cpp con backend Vulkan (NVIDIA, AMD e Intel, incluidas integradas); CUDA solo vale
  para NVIDIA. Para compilar hace falta el Vulkan SDK (y libclang para los bindings); hoy no están en el equipo.
  Alternativa sin GPU: Whisper en sherpa-onnx, que ya usamos (solo CPU).
- **Modelo aparte:** turbo int8 ~0,8 GB, descarga bajo demanda, solo si el usuario lo activa.
- **Vigilar:** Whisper procesa ventanas de 30 s (un dictado corto cuesta casi lo mismo que uno largo) y se
  inventa frases en silencios («Subtítulos realizados por…»): VAD antes y filtro de alucinaciones después.
- **Necesita Meets:** tiempos por palabra (Whisper los da con `word_timestamps`; comprobar que valen para
  quitar el eco y unir las pistas como los de Parakeet).
- **Antes de decidir:** comparativa medida Parakeet vs Whisper (base, small, medium, large-v3, large-v3-turbo;
  GPU fp16 y turbo en CPU int8) en `scratchpad\asr\resultados.md` de la sesión del 17-oct: WER/CER, tiempo por
  minuto, VRAM, carga, latencia en 5 s y 30 s, y 12 dictados reales lado a lado.

## PARA EL 19-OCT-2026 · lo que pidió Txonso tras probar la 2.2.0

Lo que más le gusta: **Whisper large-v3** («va de locos»).

1. **Motor: tras descargar un modelo sigue saliendo «Descargar».** Hay que salir de la pestaña y volver para ver
   «Usar». El `whisper://progress` con `done` marca `present` y repinta, pero algo falla en la instalada (¿el evento
   no llega con `state: 'done'`, o `current` es nulo?). Mirar `ui/hub/pages/settings.js` (listener) y
   `modeldl.rs::whisper_start`.
   **Peor aún (Txonso, 18-oct):** tras reiniciar el PC y abrir Pronto, los modelos ya bajados vuelven a salir con
   «Descargar». Al pulsar, «descarga» en segundos: el descargador ve el fichero bueno y se lo salta. En disco están
   bien (`%APPDATA%\Pronto2\models\whisper`: large-v3 1081140203, turbo 574041195, medium 539212467 bytes, tamaños
   exactos), así que `WhisperModel::present()` debería dar `true`. Sospecha: la UI pinta con datos viejos o
   `asr_options` falla o llega tarde al arrancar. Mirar con el registro de la app instalada qué devuelve
   `asr_options` nada más arrancar. Seguramente es el mismo fallo que el de arriba.
2. **Monitor de la barra: sale «Pantalla 7» con solo dos pantallas.** `screenNum` saca el número de
   `\.\DISPLAY7`, y Windows no los numera seguidos. Numerar por orden de la lista (1, 2…) y mostrar el nombre
   amigable si lo hay (`flowbar.rs::friendly_names`).
3. **Selector de polish al terminar el dictado.** Ya existe en Estilo → «Pulir cada dictado con IA local»
   (`aiPolish`). Enseñárselo; si no lo encuentra, ponerlo más a mano (¿también en Motor o en la barra?).
4. **Motor «recomendado para tu PC».** Marcar el que mejor va según GPU/VRAM/RAM, con porcentajes de velocidad,
   tamaño y aciertos, en tarjetas estilo awards (podio, medalla) respetando el diseño del hub.
5. **Actualizador.** Comprobar que al subir la nueva versión los que tienen la 2.0.x reciben la 2.2.0 (Txonso
   decía 2.1; la instalada es 2.2.0 y lo publicado en GitHub es 2.0.2). Y que en un PC nuevo haga lo mismo que
   hicimos aquí desde cero: detectar RAM, VRAM y GPU, recomendar, y no ofrecer lo que no cabe (Ollama/gemma,
   Whisper). Que se adapte a cualquier equipo.
6. **Siguiente versión grande: macOS (Apple).** Solo apuntado.
7. **Polish que entienda la frase.** Hoy (`polish.rs::build_prompt`) solo corrige puntuación, tildes, mayúsculas y
   concordancia, quita muletillas y tiene prohibido añadir o quitar contenido. Txonso quiere que además arregle
   frases mal construidas y palabras que el motor oyó mal, deduciendo lo que quiso decir por el contexto
   («cuenta» → «cuento», nombres del diccionario). Plan: pasarle el diccionario, permitir cambiar palabras que suenan
   parecido y reordenar, prohibir inventar ideas nuevas; medir con dictados reales que no se pasa de la raya.
   **Como interruptor aparte** (pedido de Txonso): el polish normal sigue igual y debajo va otro, p. ej. «Entender lo
   que quise decir», que se pone o se quita. Puede tardar más (modelo más grande o prompt más largo, plazo propio
   mayor que los 6 s). Apagado de fábrica. Ajuste nuevo en `models.rs` (`ai_polish_intent: bool`) y fila en Estilo.
