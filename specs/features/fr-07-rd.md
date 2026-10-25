# FR-07 · Reuniones automáticas y notas (Meets)

> Origen: observado-en-código · Objetivo al que sirve: G4 (también G3 y G5)

## Qué hace (lenguaje del usuario)
Cuando entro en una videollamada Pronto lo detecta, graba mi micrófono y el audio del sistema por separado y, al colgar, prepara solo las notas: transcripción con quién dijo qué, resumen, tareas con responsable y plazo, decisiones y preguntas. Después puedo preguntarle cosas sobre mis reuniones. Todo corre en mi PC.

## Requisitos (cada uno comprobable; `/rumbo` da un veredicto por línea)
- R1. Cada 3 s Pronto mira en el registro de Windows (`ConsentStore\microphone`) qué apps usan el micrófono y reconoce Zoom, Microsoft Teams, Webex y Slack (huddle) por el nombre EXACTO de su ejecutable (`TeamSpeak.exe` no es Teams, `ZoomIt64.exe` no es Zoom), y navegadores (Chrome, Edge, Firefox, Brave, Opera, Vivaldi, Arc) solo si una ventana de ese mismo navegador lleva el título propio de la videollamada («Meet - abc-defg-hij», «… | Microsoft Teams», «Zoom Meeting», «… | Jitsi Meet», «… | Whereby»): una palabra suelta («Cómo hacer zoom en fotos», «#teams») no cuenta; Discord nunca cuenta (PRD §1); Pronto no se detecta a sí mismo.
- R2. El ajuste `meetingAuto` tiene tres modos: `auto` (de fábrica) empieza sola con título «Reunión de {App} · HH:MM» y el aviso «Meets: grabando {App}» / «Se para solo al colgar.»; `ask` avisa una vez por reunión con «¿Grabar esta reunión en Meets?»; `off` no hace nada.
- R3. Una reunión automática se para sola cuando su app lleva 20 s sin usar el micrófono; las iniciadas a mano (`Win + Alt + M` o el botón) nunca se paran solas; si el usuario la para a mano con la llamada en marcha, esa app no vuelve a disparar hasta soltar el micrófono 6 s seguidos; solo hay una reunión a la vez («Ya hay una reunión en curso»).
- R4. Fase 1: un hilo por fuente (micrófono = «Tú», audio del sistema por loopback = «Ellos») escribe `meeting_{id}_me.wav` y `meeting_{id}_them.wav` (PCM16 mono 16 kHz) alineados al inicio (huecos de ≥ 200 ms rellenos con silencio) con la cabecera en disco desde que se crea, el audio volcado al primer segundo y la cabecera reescrita cada 5 s; un WAV sin muestras es una pista vacía, no fallida; durante la reunión no se transcribe nada; si una pista falla sale «Meets con una sola pista» y se sigue con la otra; si fallan las dos, no se crea la reunión y el error empieza «No se pudo capturar audio».
- R5. Al parar (o con «Reintentar») arranca sola la fase 2 en segundo plano, una por reunión a la vez, con avance `transcribing`, `polishing`, `labeling`, `understanding` y `done` (porcentaje por fase) sin retrasar los dictados; al acabar una reunión automática sale el aviso «Notas listas» con el título y la acción de abrirla.
- R6. La transcripción final usa cada pista entera con tiempos por palabra, la huella de voz y el diccionario; quita el eco del altavoz en el micrófono, parte los turnos por pausas de 0,7 s, une los turnos seguidos de la misma persona, descarta trozos basura («S.», menos de 2 caracteres alfanuméricos) y los del micrófono que apenas sobresalen del ruido.
- R7. Con Ollama se pulen como mucho el 25 % de los turnos (mínimo 8), solo los defectuosos y guardando el original en `rawText`, y se etiqueta cada turno (pregunta, respuesta, propuesta, decisión, tarea, otro) en bloques de 40 con 6 turnos de contexto a cada lado; sin Ollama las etiquetas salen solo por reglas; si Ollama está instalado pero parado se arranca solo (hasta 30 s).
- R8. Las notas incluyen título, resumen, tareas (con responsable y plazo si se dijeron), decisiones, temas, puntos clave, próximos pasos, preguntas abiertas, qué contó cada parte y las preguntas con su respuesta; usa una sola pasada si la conversación cabe en 16 000 caracteres y bloques de 5 000 si no; no inventa responsables, fechas ni respuestas; el título generado solo sustituye al automático, nunca a uno puesto a mano.
- R9. Ningún dato se pierde: el audio solo se borra tras guardar la transcripción final y siempre que `meetingKeepAudio` sea false; si la fase 2 falla, la reunión queda `failed` con su audio para reintentar; una reunión que quedó grabando al cerrarse la app se cierra al arrancar con resumen `failed` y reintentable; no se puede borrar una reunión en curso ni mientras se generan sus notas; una reunión automática de menos de 60 s con menos de 2 turnos se borra sin resumen ni aviso («Descartada: reunión demasiado corta»).
- R10. «Preguntar sobre tus reuniones» contesta con Ollama local usando las 4 reuniones mejor puntuadas por la pregunta (o las 3 últimas) o solo la elegida, con un máximo de 20 000 caracteres de contexto; pregunta vacía da «Escribe una pregunta» y sin reuniones «Aún no hay reuniones guardadas»; cada reunión se exporta a Markdown; ni el audio, ni la transcripción, ni las notas salen del PC (motor de voz local y Ollama en `ollamaUrl`).

## Fuera de alcance
- Nombre real de «Ellos» y compromisos por interlocutor (FR-16).
- Transcripción en directo durante la reunión (retirada en el contrato v12).
- Elegir motor de voz o modelo de IA (FR-04 y FR-05).
- Listado y edición en el hub de las reuniones (FR-08 cubre historial y notas de dictado).

## Dónde vive
- `src-tauri/src/detect.rs:16-36` (clave del registro, navegadores y palabras), `:58-97` (reuniones en curso), `:152-` (lectura del registro)
- `src-tauri/src/auto.rs:12-17` (20 s y 6 s), `:59-136` (máquina de estados), `:140-142` (título)
- `src-tauri/src/meeting.rs:42-43` (descarte), `:145-160` (empezar, a mano y sola), `:166-227` (dos pistas), `:230-255` (parar), `:258-269` (descartar), `:272-281` (atajo), `:538-651` (fase 2 y aviso «Notas listas»), `:653-681` (fallo y reintento), `:684-724` (preguntar), `:731-767` (detector)
- `src-tauri/src/notes.rs:31-46` (constantes), `:56-107` (palabras por pista), `:120-138` (eco y turnos), `:145-186` (pulido), `:195-225` (etiquetas), `:442-489` (comprensión)
- `src-tauri/src/notes/turns.rs:25-29` (pausa de 0,7 s), `:174-317` (eco), `:521-560` (candidatos y aceptación del pulido), `:805` (contexto de etiquetas)
- `src-tauri/src/summary.rs:131` (líneas de conversación), `:198-280` (prompts), `:532` (contexto de preguntas), `:594` (Markdown)
- `src-tauri/src/meetingdb.rs:156-200` (guardar notas y título), `:286-316` (borrar y reuniones huérfanas), `:319-340` (ranking para preguntar)
- `src-tauri/src/commands.rs:581-660` (comandos de reuniones y borrado protegido), `src-tauri/src/main.rs:50` (cerrar huérfanas al arrancar)
- `src-tauri/src/segment.rs:35` (trozos finales de 1,2 s de silencio y 30 s)
