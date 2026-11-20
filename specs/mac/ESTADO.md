# Pronto 2 para Mac — estado de cada función

Fuente de la lista: `specs/mac/funciones.json` (72 funciones de la versión de Windows: FR-01 a FR-10, FR-17 y
FR-18; las FR-11 a FR-16 están planificadas y aún no existen en Windows). `tests/mac.test.mjs` falla si una
función no tiene fila aquí o si «degradado»/«no» no lleva su motivo.

**Resumen: 66 hechas · 3 degradadas · 3 no (con motivo).** «Hecho» quiere decir escrito y
comprobado al compilar para Mac desde Windows; probarlo en un Mac de verdad es `scripts/mac/probar.sh` +
`docs/mac/PROBAR.md`.

| Id | Función | Mac | Cómo / motivo |
|---|---|---|---|
| F01 | Dictado manteniendo el atajo (grabar y pegar al soltar) | hecho | Mantener fn/🌐 (CGEventTap, `mac/hotkey.rs`) y pegar con ⌘V (`mac/inject.rs`). |
| F02 | Toque corto y segundo toque para manos libres | hecho | Lógica común de `state.rs`; el gancho Mac manda los mismos eventos. |
| F03 | Modo manos libres con atajo propio y boton de parar | hecho | fn + Espacio por defecto. |
| F04 | Esc cancela la grabacion sin pegar | hecho | Esc se traga durante la grabación, igual que en Windows. |
| F05 | Descarte de grabaciones cortas y aviso de microfono silenciado | hecho | Común: el mismo código en Windows y Mac. |
| F06 | Limite de 20 min por dictado con aviso a los 19 | hecho | Común. |
| F07 | Pegado en la app activa con restauracion del portapapeles | hecho | NSPasteboard: se guarda y devuelve todo (cada elemento y cada tipo); lo de Pronto lleva la marca Transient. |
| F08 | Dictado guardado en historial antes de pegar y reintento si falla el pegado | hecho | Común (regla 5). |
| F09 | Pegar el ultimo dictado con Alt+Shift+Z | hecho | ⌥⇧Z por defecto. |
| F10 | Aviso de falta de modelo de voz | hecho | Común. |
| F11 | Seleccion de microfono y atajo configurables | hecho | Micrófono por cpal (Core Audio); el atajo se graba con el mismo modo captura (incluye fn). |
| F12 | Limpieza de muletillas en 4 niveles | hecho | Común: el mismo código en Windows y Mac. |
| F13 | Diccionario con reemplazo de palabras | hecho | Común: el mismo código en Windows y Mac. |
| F14 | Coincidencia fonetica del diccionario | hecho | Común: el mismo código en Windows y Mac. |
| F15 | Atajos de texto (snippets) | hecho | Común: el mismo código en Windows y Mac. |
| F16 | Estilo de escritura segun la app con foco | hecho | App con NSWorkspace y título por Accesibilidad; nombres de Mac traducidos a los de Windows (Mail, Outlook, Zoom, Teams…). |
| F17 | Tonos Formal, Casual, Muy casual y Entusiasta | hecho | Común: el mismo código en Windows y Mac. |
| F18 | Orden de voz pulsa enter | hecho | Enter sintético con CGEventPost. |
| F19 | Diccionario por voz (grabar la pronunciacion de una palabra) | hecho | Común (micrófono por cpal). |
| F20 | Variantes y muestras de pronunciacion por palabra | hecho | Común: el mismo código en Windows y Mac. |
| F21 | Reconocimiento por huella de voz (DTW) | hecho | Común: el mismo código en Windows y Mac. |
| F22 | Borrado automatico de muestras pendientes | hecho | Común: el mismo código en Windows y Mac. |
| F23 | Motor de voz Parakeet de fabrica | hecho | Parakeet en la CPU de los chips M (sherpa-onnx trae librerías para macOS arm64). Core ML no se activa hasta medirlo en un Mac (Fable: suele ir peor con el transducer). |
| F24 | Descarga de modelos con verificacion SHA-256 | hecho | Común; modelos en ~/Library/Application Support/Pronto2/models. |
| F25 | Catalogo de motores Whisper con filtro de CPU y VRAM | hecho | En Mac no hay AVX2 ni VRAM aparte: memoria unificada (`mac/whisper.rs`). |
| F26 | Cambiar de motor (Usar y recarga) | hecho | Whisper con Metal (feature `metal` de whisper-rs, librería de shaders incrustada). |
| F27 | Deteccion de modelos ya presentes en disco | hecho | Común: el mismo código en Windows y Mac. |
| F28 | Reconocimiento con diccionario como texto previo | hecho | Común: el mismo código en Windows y Mac. |
| F29 | Activar la IA local (Ollama) en un boton | degradado | En Mac el botón abre la página oficial de Ollama (ollama.com/download/mac) y Pronto espera a que responda en 127.0.0.1:11434; la instalación la hace la persona (decisión D3). |
| F30 | Verificacion de firma del instalador de Ollama | no | No hace falta: en Mac Pronto no baja ni ejecuta el instalador de Ollama; lo instala la persona y lo comprueba macOS (Ollama está notarizado por Apple). |
| F31 | Ollama solo en este equipo (loopback) y arranque oculto | hecho | Loopback común (regla 1); se arranca con `open -g -a Ollama` u `ollama serve`. |
| F32 | Cortacircuitos del pulido si Ollama falla | hecho | Común: el mismo código en Windows y Mac. |
| F33 | Pulido de cada dictado con IA local | hecho | Común: el mismo código en Windows y Mac. |
| F34 | Guarda contra invenciones (cifras, enlaces, cuentas) | hecho | Común: el mismo código en Windows y Mac. |
| F35 | Deshacer el pulido (texto crudo conservado) | hecho | Común: el mismo código en Windows y Mac. |
| F36 | Transforms Polish (Alt+Shift+P) y Prompt Engineer con confirmacion Enter | hecho | ⌥⇧P; la vista previa se confirma con Enter, que el gancho Mac traga solo con la vista previa abierta. |
| F37 | Flow bar: pildora flotante sin foco sobre la barra de tareas | hecho | Ventana Tauri transparente encima de todo, en todos los Escritorios y a pantalla completa (`mac/flowbar.rs`). No es un NSPanel: si se pulsa, Pronto devuelve el foco a la app antes de pegar (D8). |
| F38 | Estados de la pildora (escuchando, transcribiendo, puliendo) | hecho | Común: el mismo código en Windows y Mac. |
| F39 | Fijar la barra a un monitor y seguir al cursor | hecho | Monitores por CoreGraphics/NSScreen; la barra sigue al cursor. |
| F40 | Menu contextual de la barra (ocultar 1 hora, microfono, configuracion) | hecho | Común: el mismo código en Windows y Mac. |
| F41 | Avisos de la barra como tarjeta | hecho | Común: el mismo código en Windows y Mac. |
| F42 | Deteccion automatica de Zoom, Teams y Meet por uso del microfono | degradado | Apps de escritorio (Zoom, Teams, FaceTime, Webex, Slack…) por el uso del micro por proceso (Core Audio). Una llamada en el navegador (Meet) solo se avisa como dudosa y nunca se graba sola: leer el título de la pestaña pediría el permiso de Grabación de pantalla, que Pronto no pide. |
| F43 | Modos de reunion automatica (auto, preguntar, manual) | hecho | Común: el mismo código en Windows y Mac. |
| F44 | Parada automatica de la reunion al dejar de usar el microfono | hecho | Común: el mismo código en Windows y Mac. |
| F45 | Grabacion de reunion por dos pistas (microfono y audio del sistema) | hecho | Micro + audio del sistema con el «process tap» de Core Audio (cpal) desde macOS 14.6; en 14.0-14.5 la reunión graba solo el micrófono. |
| F46 | Atajo Win+Alt+M para iniciar o parar una reunion a mano | hecho | fn + M por defecto. |
| F47 | Transcripcion final con quien dijo que (Tu / Ellos) | hecho | Común (Whisper con Metal o Parakeet). |
| F48 | Notas de reunion: resumen, tareas, decisiones y citas | hecho | Común: el mismo código en Windows y Mac. |
| F49 | Reintentar el procesado de una reunion | hecho | Común: el mismo código en Windows y Mac. |
| F50 | Preguntar sobre tus reuniones con IA local | hecho | Común: el mismo código en Windows y Mac. |
| F51 | Audio de reunion opcional y borrado seguro | hecho | Común: el mismo código en Windows y Mac. |
| F52 | Historial de dictados agrupado por dia con busqueda | hecho | Común: el mismo código en Windows y Mac. |
| F53 | Copiar, reproducir y eliminar dictados | hecho | Común: el mismo código en Windows y Mac. |
| F54 | Audio guardado de cada dictado | hecho | Común: el mismo código en Windows y Mac. |
| F55 | Reintentar transcripcion de un dictado | hecho | Común: el mismo código en Windows y Mac. |
| F56 | Retencion del historial y del audio con proteccion al acortar | hecho | Común: el mismo código en Windows y Mac. |
| F57 | Estadisticas (palabras, ppm, racha) | hecho | Común: el mismo código en Windows y Mac. |
| F58 | Bloc de notas con guardado automatico | hecho | Común: el mismo código en Windows y Mac. |
| F59 | Aviso de version nueva y actualizacion con un clic | degradado | Avisa de la versión nueva; «Actualizar» abre la página de descarga del release en vez de instalar solo (decisión D9: sin notarizar no se cambia la app en caliente). |
| F60 | Actualizador automatico con comprobacion de tamano y firma | no | Sin instalación automática en Mac (D9). Se hará al notarizar con Apple Developer (docs/mac/REPARTIR.md). |
| F61 | Aviso si la ultima actualizacion fallo | no | No aplica en Mac: no hay instalación automática que pueda fallar (D9). |
| F62 | Instalador sin administrador con el mismo AppId | hecho | En Mac es un .dmg (arrastrar a Aplicaciones, sin administrador), construido por `.github/workflows/mac.yml`. |
| F63 | Base de datos unica local y migraciones versionadas | hecho | Común: el mismo código en Windows y Mac. |
| F64 | Copia diaria de la base (7 copias) | hecho | Común; las copias van a ~/Library/Application Support/Pronto2/copias. |
| F65 | Migracion de ajustes antiguos | hecho | Común: el mismo código en Windows y Mac. |
| F66 | Una sola instancia que enseña el hub abierto | hecho | tauri-plugin-single-instance; abrir Pronto otra vez desde Finder enseña el panel (RunEvent::Reopen). |
| F67 | Iniciar con el sistema | hecho | LaunchAgent en ~/Library/LaunchAgents (`mac/autostart.rs`, decisión D11). |
| F68 | Icono de bandeja; cerrar el hub no cierra Pronto | hecho | Icono plantilla en la barra de menús; sin icono en el Dock salvo con el panel abierto (D10). |
| F69 | Purga de audio e historial antiguos al arrancar | hecho | Común: el mismo código en Windows y Mac. |
| F70 | Registro de errores local (sin datos del usuario) | hecho | Común; registro en ~/Library/Application Support/Pronto2/logs. |
| F71 | Panel claro con menu lateral, configuracion y modo claro/oscuro | hecho | Mismo panel con el semáforo de ventana de macOS, atajos con ⌘⌥⇧ fn y página «Permisos del Mac». |
| F72 | Novedades dentro de la app y tarjeta Lo nuevo | hecho | Común: el mismo código en Windows y Mac. |

## Lo que no se puede comprobar desde Windows

| Qué | Motivo | Dónde se comprueba |
|---|---|---|
| Enlazar el ejecutable (frameworks, librerías de sherpa-onnx y whisper.cpp) | Solo el enlazador de Apple, en un Mac | `probar.sh` paso 6 y GitHub Actions |
| Compilar el C/C++/Objective-C (whisper.cpp con Metal, onnxruntime) | Hace falta el SDK de macOS | ídem |
| Que cada API de Apple se comporte como dice su documentación (tap de teclado, pegado, Core Audio, NSWindow) | Solo en ejecución | `docs/mac/PROBAR.md` pasos 2-15 |
| Permisos de macOS (TCC) y Gatekeeper | Solo en un Mac con la app firmada | `docs/mac/PROBAR.md` pasos 1-5 |
| Regla 3 (velocidad) en un chip M | Hace falta el chip | `probar.sh` paso 5 |
