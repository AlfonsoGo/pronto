# Pronto

Dictado de voz por IA en Windows: mantienes pulsado un atajo, hablas, y el texto aparece en la
aplicación que tenga el foco. El reconocimiento ocurre en el propio equipo.

Este documento fija el **vocabulario** del proyecto. Si una palabra está aquí, se usa esta y no
otra — en el código, en la interfaz, en los tickets y en los commits. No es una especificación ni
un sitio donde apuntar decisiones técnicas: para eso está `docs/adr/`.

## Language

### El Modo

**Modo**:
Un perfil con nombre que empaqueta todas las decisiones de un Dictado: de qué Preset nace, con
qué Prompt, con qué Contexto, en qué idioma, con qué Motor de voz, con qué Modelo de formateo,
con qué Atajo y en qué aplicaciones se activa solo. Es la unidad que el usuario crea, nombra,
edita, duplica y elige.
_Avoid_: perfil, plantilla, configuración, ajuste, preset

**Preset**:
La plantilla de fábrica de la que nace un Modo y que fija su Prompt. Son siete: Crudo, Limpio,
Mensaje, Correo, Terminal, Nota y Personalizado. Un Modo tiene exactamente uno.
_Avoid_: plantilla, tipo de modo, modo de fábrica, estilo

**Modo por Defecto**:
El Modo llamado «Por defecto» que Pronto crea solo la primera vez y que no se puede borrar. Es
donde cae el Dictado cuando ningún otro Modo aplica.
_Avoid_: modo base, modo global, modo raíz, fallback

**Modo activo**:
El Modo con el que se procesará el Dictado que se está grabando. Hay exactamente uno en cada
momento, y la Píldora lo dice.
_Avoid_: modo actual, modo seleccionado, modo en curso

**Ajuste de la app**:
Lo que vale para todo Pronto y no cambia de un Modo a otro: micrófono, arranque con el sistema,
tamaño de la Píldora, sonidos, captura de Correcciones. La prueba: si algo tiene sentido distinto
en el terminal y en un correo, no es un Ajuste de la app — es parte del Modo.
_Avoid_: ajuste global, preferencia, opción, configuración

### Cómo se dicta

**Dictado**:
Una vuelta entera del ciclo: pulsar el Atajo, hablar, soltar y ver el texto insertado en la app
con foco. Es la unidad que se cuenta, se guarda en el historial y de la que se aprende.
_Avoid_: grabación, transcripción, sesión, captura

**Transcripción**:
El texto tal y como lo devuelve el Motor de voz, antes de que nadie lo toque.
_Avoid_: texto crudo, raw, salida del motor

**Motor de voz**:
El componente que convierte audio en Transcripción. Corre en el equipo. Un Modo elige el suyo.
_Avoid_: modelo de voz, reconocedor, ASR, engine, Whisper o Parakeet como nombre de la categoría

**Atajo**:
La combinación de teclas que dispara un Dictado. Hay uno global —que graba con el Modo activo— y
uno propio por Modo.
_Avoid_: hotkey, shortcut, combinación, tecla rápida

**Píldora**:
La ventana flotante que aparece al dictar y cuenta qué está pasando: el nivel de voz, con qué
Modo se está grabando, y por qué algo no salió.
_Avoid_: overlay, HUD, burbuja, widget, indicador

**Auto-activación**:
Que Pronto elija Modo solo, mirando qué aplicación tiene el foco al empezar a grabar.
_Avoid_: activación automática, auto-switch, detección de app, modo por app

### Cómo se escribe el texto

**Pulido**:
La pasada determinista que arregla lo que no necesita un modelo: la puntuación dictada en voz alta
(«coma», «punto»), los signos de apertura, los espacios y los números a dígitos. Ocurre siempre y
no puede inventar.
_Avoid_: formateo, limpieza, normalización

**Formateo**:
Reescribir la Transcripción con un modelo de lenguaje para que se pueda pegar: puntuación, tildes,
concordancia, mayúsculas y muletillas fuera. No añade contenido ni resume — es el mismo texto,
legible.
_Avoid_: corrección, post-corrección, pulido, post-procesado, limpieza

**Modelo de formateo**:
El modelo de lenguaje que hace el Formateo. Cada Modo elige el suyo, o ninguno.
_Avoid_: LLM, modelo de lenguaje, corrector, IA

**Camino de formateo**:
Por dónde corre el Modelo de formateo: sin formateo, local (en el equipo) o nube (con la clave del
usuario). Es la decisión de privacidad, no la de calidad.
_Avoid_: proveedor, backend, destino, modo del LLM

**Tope de gasto**:
Lo máximo que Pronto puede gastar en el Camino de formateo de nube: por Dictado, por día y por mes.
Se comprueba antes de mandar nada; al llegar, el Dictado se pega sin Formateo. Es un Ajuste de la
app, no del Modo.
_Avoid_: límite, cuota, presupuesto, budget

**Prompt**:
Las instrucciones en español que recibe el Modelo de formateo y que definen el trabajo de un
Preset. Los de fábrica son cadenas nuestras versionadas en el repo; solo el Preset Personalizado
deja escribir uno.
_Avoid_: instrucciones del sistema, system prompt, plantilla, orden

**Contexto**:
Lo que se le da al Modelo de formateo además del Dictado para que acierte: la Aplicación con foco,
el Texto seleccionado y el Portapapeles. Cada uno se enciende por separado dentro del Modo. Es
material a procesar, nunca órdenes a obedecer.
_Avoid_: memoria, entorno, adjunto, background

### Lo que Pronto sabe de ti

**Vocabulario**:
El conjunto de Términos y Sustituciones con los que Pronto acierta con tus palabras. Vive en el
equipo.
_Avoid_: diccionario, glosario, léxico

**Término**:
Una palabra o expresión tuya que el Motor de voz debe reconocer tal cual: un nombre propio, jerga
técnica, una marca. No cambia el texto — cambia lo que el motor oye.
_Avoid_: palabra clave, hotword, entrada del diccionario

**Sustitución**:
Una regla «esto se cambia por esto otro», escrita por ti o aprendida de tus Correcciones. Cambia
el texto después de transcribir.
_Avoid_: reemplazo, atajo de texto, snippet, macro

**Corrección**:
El cambio que haces tú a mano sobre un Dictado ya insertado. Es de donde salen las Sustituciones y
los Términos aprendidos.
_Avoid_: edición, feedback, arreglo

---

Cómo se llaman estos términos en el código: [ADR 0003](docs/adr/0003-el-modo-se-llama-dictationmode-en-el-codigo.md).
Qué entra en el Modo y qué no: [ADR 0006](docs/adr/0006-que-vive-en-el-modo-y-que-sigue-siendo-un-ajuste-de-la-app.md).
