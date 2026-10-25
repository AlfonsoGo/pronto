# FR-08 · Hub: historial, audio guardado, reintentar, estadísticas y notas

> Origen: observado-en-código · Objetivo al que sirve: G5 (también G1)

## Qué hace (lenguaje del usuario)
En el hub veo todo lo que he dictado, puedo buscarlo, copiarlo, volver a oírlo y, si salió mal, volver a transcribir su audio. También veo cuánto dicto y a qué velocidad, y tengo un bloc de notas que se guarda solo.

## Requisitos (cada uno comprobable; `/rumbo` da un veredicto por línea)
- R1. El historial lista los dictados del más nuevo al más viejo, agrupados por día, de 40 en 40 con «Cargar más», y de cada uno muestra hora, app, texto, nº de palabras y segundos; un dictado nuevo aparece arriba al instante con el aviso «Nuevo dictado: N palabras».
- R2. La búsqueda encuentra por texto y por nombre de app sin distinguir mayúsculas; sin resultados dice «No se encontraron transcripciones para "…"».
- R3. Cada dictado se puede copiar, desplegar, reproducir y eliminar (tras «¿Seguro que quieres eliminar esta transcripción?», borrando también su audio); también existe vaciar todo el historial.
- R4. Cada dictado guarda su audio (WAV 16 kHz mono) `%APPDATA%\Pronto2\audio\{id}.wav` durante `keepAudioDays` días (14 de fábrica; 0 = no guardar audio y borrar el existente); sin audio, el botón de reproducir está desactivado con «El audio ya no está disponible».
- R5. «Reintentar transcripción» vuelve a transcribir el audio guardado con el motor, el diccionario y los ajustes actuales, actualiza el texto del dictado (sin pegar y sin pulir con IA) y no se ofrece sin audio; los errores son «Este dictado no tiene audio guardado» y «El audio guardado está dañado».
- R6. `historyRetentionDays` (0 = conservar siempre) borra los dictados más viejos y sus audios al arrancar y al guardar el ajuste.
- R7. Estadísticas: palabras totales, ppm (palabras entre minutos de audio dictado), racha de días seguidos (cuenta desde ayer si hoy aún no hay dictado), racha más larga, palabras de los últimos 7 días, nº de dictados, mapa de calor de 365 días, las 8 apps con más palabras y los «Arreglos de Pronto» (diccionario, snippets, muletillas).
- R8. Los Transforms quedan en el historial con su nombre como etiqueta, pero no cuentan como dictado en las estadísticas.
- R9. El Bloc de notas permite crear, editar, buscar, copiar y eliminar notas con guardado automático a los 600 ms de dejar de escribir («Guardando…» / «Guardado» / «No se pudo guardar»), ordenadas por última edición.
- R10. Todo el historial, el audio y las notas se guardan solo en el PC (SQLite y `%APPDATA%\Pronto2`).

## Fuera de alcance
- Las notas de reuniones y su página (FR-07).
- Informe de palabras más falladas con botón «enseñar» (FR-14) y medidas de latencia y WER (FR-11).
- Copia de seguridad y migraciones de la base (FR-10).

## Dónde vive
- `ui/hub/pages/home.js:10` (40 por página), `:83-99` (fila del historial), `:101-132` (lista y «Cargar más»), `:134-147` (cargar y búsqueda), `:170-195` (reproducir), `:197-221` (reintentar y eliminar), `:223-262` (acciones), `:283-292` (dictado nuevo)
- `ui/hub/pages/insights.js:58-149` (Estadísticas: tiles, mapa de calor, apps, arreglos)
- `ui/hub/pages/notes.js:68-84` (guardado automático 600 ms), `:86-116` (lista, borrar, copiar)
- `src-tauri/src/commands.rs:127-205` (historial, estadísticas, reintentar, audio)
- `src-tauri/src/db.rs:477-497` (añadir), `:512-519` (reintento: `polished = 0`), `:521-534` (búsqueda), `:545-578` (borrar, vaciar, retención), `:612-628` (purga de audio), `:632-693` (estadísticas e insights), `:924-957` (notas), `:1045-1053` (racha)
- `src-tauri/src/state.rs:577-596` (guardar antes de pegar), `:617-634` (guardar audio)
