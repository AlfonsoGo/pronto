# FR-06 · Flow bar

> Origen: observado-en-código · Objetivo al que sirve: G2 (también G5)

## Qué hace (lenguaje del usuario)
Una píldora flotante abajo al centro de la pantalla me dice si Pronto escucha, transcribe o ha terminado, sin quitarme nunca el foco de lo que estoy escribiendo. Puedo fijarla en un monitor con un candado para que no estorbe al compartir pantalla.

## Requisitos (cada uno comprobable; `/rumbo` da un veredicto por línea)
- R1. La barra es una ventana de 360×150 px lógicos, siempre encima, anclada abajo al centro del área de trabajo (justo sobre la barra de tareas) del monitor donde está el cursor al empezar a dictar.
- R2. La barra nunca roba el foco ni aparece en Alt+Tab; el hueco transparente de la ventana no bloquea los clics al escritorio (solo es pulsable la zona de la píldora, sondeada a ~30 Hz).
- R3. La píldora refleja el estado del dictado: reposo, «Escuchando», manos libres, «Transcribiendo», «Puliendo el texto» (Polish), «Listo, texto pulido», «Texto pegado», «Cancelado» y error con tarjeta de 4 s con el mensaje; durante una reunión muestra el estado de reunión y vuelve a él al acabar un dictado.
- R4. El candado de la barra (24 px, junto a Polish, visible al pasar el ratón, violeta si está cerrado) fija la barra en el monitor donde está («Fijar la barra en este monitor» / «Soltar la barra»); mientras está fijada, dictar en otro monitor no la mueve.
- R5. Si el monitor fijado no está conectado, la barra sigue al cursor sin borrar el ajuste y vuelve sola al monitor fijado (≈ 2 s) al reconectarlo o cambiar resolución o escala.
- R6. El monitor fijado se reencuentra aunque Windows renumere las pantallas: primero nombre más rectángulo, luego solo rectángulo, luego solo nombre.
- R7. En Ajustes → Sistema la fila «Monitor de la barra» ofrece «Seguir al cursor» (de fábrica) y cada monitor conectado con su número, nombre comercial, resolución y «principal»; los números van seguidos en el orden de la lista: 1, 2… (fallo conocido 18-oct: sale «Pantalla 7» con solo dos pantallas porque el número se saca de `\\.\DISPLAY7`; debe numerarse por orden y mostrar el nombre amigable si lo hay).
- R8. El clic derecho en la píldora abre un menú nativo con «Ocultar 1 hora», «Configuración», «Micrófono» (lista con el predeterminado del sistema), «Historial de transcripciones» y «Pegar última transcripción».
- R9. «Ocultar 1 hora» oculta la barra en reposo 60 min, sigue apareciendo al dictar y vuelve sola a la hora, también tras reiniciar Pronto; con el ajuste `showFlowBar` desactivado en reposo no se ve nunca.
- R10. Un aviso de la barra (por ejemplo «Falta el modelo de voz», «Pegado sin pulir: Ollama no respondió») sale como tarjeta con su acción si la tiene («Abrir Pronto», «Pegar último texto», «Copiar», «Configuración», «Grabar», «Detener») y se cierra sola.

## Fuera de alcance
- Qué se dicta, se transcribe o se pega (FR-01 y FR-02).
- Las notas de reunión que la barra anuncia («Notas listas»; FR-07).
- Barra distinta por app o varias barras a la vez.

## Dónde vive
- `src-tauri/src/flowbar.rs:43` (360×150), `:57-64` (visible en reposo), `:82-111` (estilos sin foco, mostrar), `:122-138` (monitor destino), `:155-176` (colocar), `:181-189` (mantener fijada), `:206-209` y `:335` (número de pantalla sacado de `\\.\DISPLAYn`: origen del fallo conocido), `:246-254` (reencontrar monitor), `:283-346` (nombres amigables y lista), `:351-397` (sondeo y zona pulsable)
- `src-tauri/src/state.rs:138-155` (ocultar 1 hora), `:125-135` (refrescar visibilidad)
- `src-tauri/src/commands.rs:525-539` (zona pulsable, menú, ocultar), `:545-560` (candado y lista de monitores)
- `src-tauri/src/tray.rs:37` (textos del menú), `:106-119` (menú contextual)
- `ui/flowbar.js:20-35` (tamaños por estado), `:160-170` (estado y etiquetas), `:225-232` (mensajes), `:246-262` (acciones de las tarjetas), `:396-416` (candado)
- `ui/hub/pages/settings.js:111-126` (fila «Monitor de la barra»; `:112` `screenNum` y `:113` `monLabel` pintan «Pantalla N»)
