# FR-05 · IA local: pulido, Transforms y activación en un botón

> Origen: observado-en-código · Objetivo al que sirve: G1 (también G3 y G5)

## Qué hace (lenguaje del usuario)
Con un botón, «Activar IA local», Pronto instala Ollama y un modelo pequeño en mi PC. Con eso puedo pulir cada dictado (puntuación, tildes, mayúsculas) y transformar el texto seleccionado con un atajo, siempre sin que nada salga de mi equipo y sin que un fallo de la IA me impida pegar.

## Requisitos (cada uno comprobable; `/rumbo` da un veredicto por línea)
- R1. «Activar IA local» hace solo lo que falte, en este orden: descargar `OllamaSetup.exe` desde `https://ollama.com/download/OllamaSetup.exe`, instalarlo en silencio sin administrador, arrancar Ollama (espera hasta 90 s a su API) y bajar `qwen3:1.7b`; antes muestra un aviso con «Instalar» / «Ahora no»; «Cancelar» corta la descarga, la bajada del modelo y la espera de arranque, un instalador ya lanzado no se mata (el botón se desactiva durante «Instalando Ollama…») y solo hay una activación a la vez.
- R2. El instalador de Ollama no se ejecuta si pesa menos de 50 MB o si su firma Authenticode no es válida o el firmante no empieza por «Ollama»; el error es «No se instaló Ollama: la firma del instalador descargado no es válida (…)».
- R3. Todas las llamadas a Ollama van por `http` a `ollamaUrl`; si esa dirección no es de este equipo (`localhost`, `127.0.0.1`, `::1`) y no responde, no se instala ni arranca nada y el error dice que revise la dirección en Configuración; ningún texto ni audio sale del PC (la única descarga externa es la de Ollama y su modelo).
- R3b. Con «Pulir cada dictado» encendido, si Ollama está instalado pero parado, Pronto lo arranca oculto al iniciarse y al empezar a dictar (un arranque a la vez, espera hasta 60 s), solo con una `ollamaUrl` loopback, buscando `ollama.exe` en sus rutas de instalación o en carpetas absolutas del PATH y con `OLLAMA_HOST` en la dirección de loopback guardada (la IPv4 tal cual, `[::1]` si es IPv6, `127.0.0.1` para `localhost`) y su puerto, incluido el de por defecto del esquema; el cortacircuitos del pulido solo se reinicia si lo acaba de arrancar Pronto (decisión de Txonso 24-oct-2026; seguridad 25-oct M1, B1, B2). `src-tauri/src/polish.rs` (`warm_up`), `src-tauri/src/aisetup.rs` (`ensure_running_ex`, `path_dirs`, `ollama_host_env`).
- R3c. Un plazo de generación vencido abre el cortacircuitos del pulido como «atascado»: no se cierra porque `/api/tags` conteste: la primera vez, el vigilante prueba cada 5 s una generación mínima del mismo modelo (`num_predict` 1, plazo 3 s, a la misma URL loopback) y se cierra si sale, sin borrar los fallos; si el pulido de verdad vuelve a vencer, se cumple la ventana entera, que crece (60 s, 2 min…); un solo vigilante por dirección; solo un pulido bueno cierra el circuito y borra sus fallos (las notas, preguntas, Transforms y precargas no lo tocan), y la sonda de 1 token cuenta solo plazos vencidos seguidos, no fallos de conexión; un fallo de conexión se sigue cerrando en cuanto Ollama responde (rompedor 25-oct, MEDIA 1). `src-tauri/src/ollama.rs` (`GenError::stalled`, `circuit::trip_stalled`), test `rompedor_pulido_ollama_vivo_generacion_atascada`.
- R4. Al terminar, los ajustes de modelo que no estén en Ollama pasan a `qwen3:1.7b` (uno elegido a mano que sí esté descargado se respeta) y «Pulir cada dictado» no se enciende solo; la tarjeta muestra «IA local lista».
- R5. Con «Pulir cada dictado con IA local» activado, los dictados de 3 palabras o más pasan por Ollama después de la tubería de texto y antes de guardar y pegar; los de menos de 3 palabras y los que expandieron un snippet no se pulen.
- R6. El pulido tiene temperatura 0, `think:false` y un plazo duro de 6 s por defecto (ajustable entre 0,5 y 60 s); si Ollama falla, vence el plazo o la respuesta mide más del doble o menos de la mitad del original, se pega el texto sin pulir y sale el aviso «Pegado sin pulir: Ollama no respondió».
- R7. El prompt de pulido solo corrige puntuación, tildes, mayúsculas y concordancia, quita muletillas y repeticiones, no añade ni quita contenido, no traduce y trata el dictado como dato: una pregunta u orden dictada se devuelve corregida, nunca contestada.
- R8. En el historial `text` guarda el texto pulido y `rawText` el texto local previo, para poder deshacer; «Reintentar» no pule.
- R9. Un Transform actúa sobre la selección actual (Ctrl+C, Ollama, pegar): de fábrica «Polish» (`Alt + Shift + P`) y «Prompt Engineer»; los de fábrica no se pueden borrar; con más de 1000 palabras seleccionadas el error es «Texto demasiado largo para transformar (máx. 1000 palabras)» y sin selección «Selecciona texto primero»; el portapapeles del usuario se restaura si el flujo se aborta.
- R10. Para las notas de reunión se usa el mejor modelo instalado de `gemma4:12b-it-qat` > `qwen3.6` > `qwen2.5:7b` > `qwen3:8b` > `qwen3:4b` > `Qwen3-4B-GGUF:Q4_K_M`; un modelo distinto del de fábrica elegido a mano se respeta.

## Fuera de alcance
- Que el pulido entienda la frase, cambie palabras mal oídas y use el diccionario (FR-13).
- Recomendar o esconder modelos según GPU/VRAM/RAM (FR-15).
- Resumen y etiquetado de reuniones (FR-07).

## Dónde vive
- `src-tauri/src/polish.rs:10-14` (límites), `:18-31` (prompt), `:33-57` (validación), `:60-71` (petición), `:91-95` (cuándo aplica), `:97-117` (aplicar y tono «muy casual»)
- `src-tauri/src/ollama.rs:41-83` (petición, `think:false`, errores), `:88-101` (quitar `<think>`), `:117-130` (modelo de notas), `:133-145` (¿responde y qué modelos tiene?)
- `src-tauri/src/aisetup.rs:25-34` (URL y banderas del instalador), `:113-125` (¿está instalado?), `:128-134` (dirección local), `:136-159` (estado), `:262-269` (firma), `:273-298` (instalar), `:300-323` (arrancar), `:424-459` (bajar modelo), `:464-487` (orquestación), `:515-524` (adoptar modelo), `:541-587` (hilo y cancelar)
- `src-tauri/src/state.rs:569-606` (pulir y avisar), `:637-680` (Transform)
- `src-tauri/src/db.rs:253-258` (Transforms de fábrica)
- `ui/hub/ai.js:12-15` (modelo y estado), `:54-100` (aviso previo y activar), `:102-104` (cancelar), `:136` (tarjeta); `ui/hub/pages/style.js:76-105` (interruptor y modelo del pulido)
