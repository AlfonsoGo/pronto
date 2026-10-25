# FR-01 · Dictado con atajo y pegado

> Origen: observado-en-código · Objetivo al que sirve: G2 (también G5)

## Qué hace (lenguaje del usuario)
Mantengo Win+Alt, hablo, suelto, y el texto aparece escrito en la app donde estaba escribiendo, sin perder lo que tenía copiado. Si no quiero mantener la tecla, uso manos libres; si me arrepiento, pulso Esc.

## Requisitos (cada uno comprobable; `/rumbo` da un veredicto por línea)
- R1. Mantener `Win + Alt` (atajo por defecto, configurable) empieza a grabar al instante; al soltar tras ≥ 500 ms se para, se transcribe y el texto se pega en la app que tenía el foco.
- R2. Una pulsación de `Win + Alt` de menos de 500 ms es un toque: se espera 500 ms a un segundo toque, que pasa a manos libres sin perder lo grabado; si no llega, se descarta con el mensaje exacto «No lo toques. Mantén pulsado Win + Alt.» (el atajo sale de los ajustes).
- R3. `Win + Alt + Espacio` arranca en manos libres; en manos libres, volver a pulsar `Win + Alt`, `Win + Alt + Espacio` o el botón ■ de la barra termina y pega.
- R4. `Esc` durante la grabación la descarta sin transcribir ni pegar (la barra dice «Cancelado»); fuera de una grabación `Esc` llega intacto a la app.
- R5. Una grabación de menos de 300 ms se ignora sin error ni pegado; con el pico de audio por debajo de 0,0005 sale «¿Tienes el micrófono silenciado?»; con transcripción vacía, «No se ha oído nada».
- R6. Un dictado dura como máximo 20 min: a los 19 min avisa «Queda 1 minuto: la sesión termina a los 20 minutos» y a los 20 min para y pega solo.
- R7. Al pegar se respeta el portapapeles del usuario: se guarda antes, se pega con Ctrl+V una vez sueltas Win/Alt/Shift/Ctrl (espera máx. 2 s) y se restaura tras max(600 ms, 400 ms + 1 ms por cada 4 caracteres), tope 3 s; si hay dos pegados seguidos se devuelve el portapapeles original, no el texto del primero; un texto de 600 caracteres o más NO se restaura y sale el aviso «Texto largo: también queda en el portapapeles (Ctrl+V)».
- R8. La transcripción se guarda en el historial antes de pegar; si el pegado falla el error es «No se pudo pegar» / «No se copió nada; el texto está en el historial (…)», el portapapeles del usuario vuelve a como estaba y el texto no se pierde.
- R9. `Alt + Shift + Z` pega la última transcripción con texto que no sea un Transform (error «Aún no hay ninguna transcripción» si no hay ninguna, y el portapapeles no se toca) y `Alt + Shift + X` la copia al portapapeles con aviso «Copiado».
- R10. Si falta el modelo de voz y el motor no está listo ni cargando, el atajo no graba: la barra avisa «Falta el modelo de voz» / «Ábrelo en Pronto».

## Fuera de alcance
- La limpieza, el diccionario y el estilo del texto (FR-02) y el pulido con IA (FR-05).
- Elegir motor de voz (FR-04) y la posición de la barra (FR-06).
- Grabar reuniones (FR-07); `Win + Alt + M` solo descarta en silencio el dictado «mantener» que empezó al pulsar `Win + Alt`.

## Dónde vive
- `src-tauri/src/state.rs:27` (mínimo 300 ms), `:29` (500 ms), `:31-32` (20 y 19 min), `:41` (pico 0,0005)
- `src-tauri/src/state.rs:223-235` (toque, manos libres, parar, Esc, pegar última), `:295-310` (toque y mensaje «No lo toques»), `:322-330` (falta modelo), `:380-403` (parar, <300 ms, silencio), `:430` (cancelar), `:561-603` (guardar y pegar, aviso de texto largo)
- `src-tauri/src/hotkey.rs:397-482` (hook: mantener, manos libres, Esc, pegar/copiar última), `:324-345` (neutralizar Win/Alt)
- `src-tauri/src/inject.rs:17-26` (restauración), `:154-162` (espera a soltar modificadores), `:210-259` (pegar y restaurar), `:21` (600 caracteres)
- `src-tauri/src/audio.rs:16` (tope de 20 min del búfer), `:56-80` (grabadora)
- `src-tauri/src/models.rs:179-184` (atajos por defecto)
