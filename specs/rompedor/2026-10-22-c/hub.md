Rompedor · 2026-10-22 · hub · 1 altas, 5 medias, 4 bajas · Arreglado: todo lo reproducido con sonda (c5527f9; sus sondas pasan sin `#[ignore]`); la segunda pasada del 23-oct confirma
Datos: sin datos reales (backend simulado) · restaurados ✅

Zona: `ui/hub.html` + `ui/hub/**`, `ui/flowbar.html` + `ui/flowbar.js`. Servidor `scripts/servir-ui.mjs` en puerto libre
(nunca 4173), Chrome headless (`playwright-core`). Backend: `ui/shared/mock.js`, a veces envuelto por un `window.__TAURI__`
falso (`e2e/rompedor/comun.mjs` · `scriptBackendHostil`) que delega en el mock, apunta cada comando en `window.__calls`,
mete latencia a demanda (`window.__lento`) y, en modo hostil, añade `<img src=x onerror=…><b data-pwn>{{7*7}}</b>` a
cada texto libre que devuelve un comando o trae un evento. Guardas en cada página: `pageerror` (con `fichero:línea`),
`console.error`, respuestas HTTP ≥ 400, `dialog` y peticiones fuera de 127.0.0.1 (abortadas y contadas: **0**).
`%APPDATA%\Pronto2\pronto2.db`: 270336 B, mtime 2026-10-15 21:29:15.377 antes y después (no se abre). Sin commit.

Ejecutar: `node e2e/rompedor/correr.mjs [entradas|flowbar|pantallas|prisa|xss-backend]` (≈ 8 min todo; sale con 1 si hay ALTA).
Salida completa de la última pasada: `e2e/rompedor/capturas/salida.txt` (ignorada por git, como las capturas).

## ALTA

1. **Cerrar el hub justo después de escribir en una nota pierde lo escrito** (`ui/hub/pages/notes.js:84`,
   `ui/hub/pages/notes.js:137`, `ui/hub.js:186`). El Bloc de notas guarda con `debounce(save, 600)`; la limpieza de la
   página (que sí hace `save()`) solo corre al navegar, y el botón × llama directamente a `w.close()`, que destruye el
   WebView2 del hub (`src-tauri/src/main.rs:179-186`: «Cerrar el hub lo destruye»). Nada escucha `pagehide`/`beforeunload`.
   Lo tecleado (o dictado/pegado) en los últimos 600 ms se pierde; una nota nueva escrita de un tirón y cerrada al
   momento se pierde entera.
   Reproducción: `node e2e/rompedor/correr.mjs prisa` → caso 8: nueva nota, `type('Idea que no quiero perder')`, clic en
   `[data-wc=close]` → `window.__calls` = `["ventana.close"]` (ningún `save_note` antes). Captura `capturas/nota-cerrar.png`.
   Arreglo propuesto: en `windowAction('close')` (y en `pagehide`) llamar antes a la limpieza de la página actual
   (`current.cleanup()`), o exponer en notes un `flush()` (`autosave.flush()` + `await save()`) y esperarlo antes de `w.close()`.

## MEDIA

1. **URL de Ollama remota sin el aviso de «tus dictados salen del PC»** (`ui/hub/pages/settings.js:41-47`
   `isLoopbackUrl`, aviso en `:49`). El parser casero corta la autoridad por `/ ? #` pero no por `\`; con
   `http://evil.example\@localhost:11434` cree que el host es `localhost`, mientras que un parser WHATWG (el navegador,
   el crate `url`) da host `evil.example`. Regla 1 de CLAUDE.md (privacidad): el único aviso desaparece.
   Reproducción: `correr.mjs entradas` → «http://evil.example\@localhost:11434 → host WHATWG «evil.example», aviso
   visible: false» · `capturas/ollama-url-sin-aviso.png`. (`http://localhost@evil.example` y `192.168.1.5` sí avisan.)
   Arreglo propuesto: usar `new URL(u).hostname` (y en Rust `url::Url::parse(...).host_str()`; `src-tauri/src/aisetup.rs:127`
   `url_host` tiene la misma lógica — ver Huecos) y tratar como remoto todo lo que no parsee.

2. **Doble Enter / Ctrl+Enter repetido guarda la palabra o el fragmento varias veces** (`ui/hub/pages/dictionary.js:273`,
   `ui/hub/pages/snippets.js:57`). El botón se desactiva en `save()`, pero el atajo de teclado llama a `save()` sin mirar
   si ya hay un guardado en vuelo; la comprobación de duplicados usa la lista `items` de antes de abrir el modal.
   Reproducción: `correr.mjs prisa` casos 1-2 → «add_word llamado 3×; list_words: 8 → 11 («Zarandaja» ×3)»,
   «add_snippet 3×; «mi frase rápida» ×3» · `capturas/diccionario-doble-enter.png`. Con ratón (triple clic) sale 1: solo
   falla el teclado. En la BD real `words`/`snippets` no tienen UNIQUE (`src-tauri/src/db.rs:160-171`).
   Arreglo propuesto: bandera `saving` en `save()` (`if (saving) return; saving = true; … finally`) en ambos editores.

3. **Nota nueva + «Atrás» con el primer guardado en vuelo crea dos notas iguales** (`ui/hub/pages/notes.js:72-81`,
   `closeEditor` `:91-97`). `closeEditor` cancela el debounce y llama a `save()` mientras el autoguardado anterior
   (`cur.id` aún `null`) no ha vuelto: los dos hacen `save_note({ id: null })`. Lo mismo vale para la limpieza al navegar.
   Reproducción: `correr.mjs prisa` caso 9 (backend con `save_note` a 1,5 s): «save_note 2×; notas 6 → 8» ·
   `capturas/nota-duplicada.png`. Con backend rápido la ventana es de pocos ms (BD bloqueada, antivirus, disco lento).
   Arreglo propuesto: encadenar los guardados (`pending = pending.then(save)`) o guardar la promesa del primer
   `save_note` y que el siguiente espere su `id`.

4. **Renombrar una reunión y pulsar «Atrás» (o que empiece otra reunión) lanza un error JS sin capturar y la lista
   enseña el título viejo** (`ui/hub/pages/meetings.js:436-437`, disparado desde `:491` focusout y `:452` back).
   `rename()` lee `detail` después del `await`; si entretanto la vista volvió a la lista (`detail = null`), `detail.title = v`
   lanza dentro del `try` y el `catch` vuelve a leer `detail.title` → rechazo sin capturar; y `loadList()` ya pidió la lista
   antes de que el renombrado acabe.
   Reproducción: `correr.mjs prisa` casos 10-11 → `pageerror` «Cannot read properties of null (reading 'title') @
   ui/hub/pages/meetings.js:437»; «guardado en backend: true; la lista muestra el nuevo título: false».
   Arreglo propuesto: `const d = detail;` antes del `await` y usar `d`; tras renombrar, si la lista está a la vista,
   `loadList()` (o actualizar la fila).

5. **Un clic fuera del modal (o Esc) descarta lo escrito en los editores sin preguntar** (`ui/hub/util.js:167` mousedown en
   el velo, `:122` Escape; afecta a «Agregar/editar palabra», «Agregar/editar fragmento», etc.). 1 280 caracteres de
   expansión desaparecen con un clic en el fondo.
   Reproducción: `correr.mjs prisa` caso 12 → «modal cerrado, nada guardado (add_snippet 0×)».
   Arreglo propuesto: opción `dirty()` en `openModal`; si hay cambios, el clic en el velo no cierra y Esc pide
   confirmación (o se guarda un borrador).

## BAJA

1. **Doble clic en «Activar IA local» apila dos avisos previos y lanza `ai_setup_start` dos veces** (`ui/hub/ai.js:82-84`):
   la guarda `aiKind() === 'running'` se mira antes del `await refreshAi()`/`consent()`, así que el segundo clic pasa.
   Reproducción: `correr.mjs prisa` caso 4 → «abre 2 avisos previos apilados y lanza ai_setup_start 2×» ·
   `capturas/ia-doble-aviso.png`. Arreglo: bandera `asking` puesta antes del primer `await`.
2. **Flow bar: un aviso con una acción desconocida pinta un botón con el texto crudo de la acción y que no hace nada**
   (`ui/flowbar.js:275` `return ACTIONS[str] || [str, null]`). Se pinta con `textContent` (no hay XSS), pero un
   `action` inesperado del backend se ve como botón. Reproducción: `correr.mjs flowbar` → «botón visible «<img src=x …»».
   Arreglo: `ACTIONS[str] || null`.
3. **Audio de historial dañado: dos avisos, uno en inglés técnico** (`ui/hub/pages/home.js:180` y `:183`): el `error` del
   `<audio>` y el rechazo de `play()` muestran «No se pudo reproducir el audio» y «Failed to load because no supported
   source was found.». Reproducción: `correr.mjs prisa` caso 14 (base64 roto en `get_history_audio`).
   Arreglo: en el `catch` de `play()` usar el mismo texto traducido y no duplicar.
4. **El resaltado de la búsqueda rompe entidades HTML** (`ui/hub/ui.js:67-71` `highlight`): el regex corre sobre el texto
   ya escapado, así que buscar «a», «l» u «o» parte `&amp;`, `&lt;`, `&quot;` y la tarjeta enseña «Tom &amp; Jerry».
   No inyecta nada (las marcas solo envuelven texto). Reproducción: `correr.mjs prisa` (último caso) →
   `{"a":"Tom &amp; Jerry <3 \"hola\"","l":"Tom & Jerry &lt;3 …","o":"… &quot;hola&quot;"}` · `capturas/resaltado-entidades.png`.
   Arreglo: buscar sobre el texto sin escapar y escapar trozo a trozo (`split` por coincidencias).

## Lo que aguantó (con reproducción)

- **XSS desde el backend: 0.** `xss-backend`: 3 variantes (`?update=available`, `?update=error&model=error&ai=error&ollama=none`,
  `?engine=error&ollama=nomodel`), 8 páginas + detalle/pestañas + buscador + 6 pestañas de Configuración + diálogos de
  micro/idiomas/atajos + eventos `dictation://result|state`, `engine://status`, `update://available` (notas con HTML,
  `pageUrl: javascript:`), `ai://progress`, `model://progress` envenenados. El veneno se ve **como texto** en 126/126
  pantallas; 0 nodos `[data-pwn]`, 0 `onerror`, 0 peticiones a `/x`. Control positivo (inyección real) sí detectado.
- **XSS desde lo que escribe el usuario: 0.** `entradas`: 9 cargas (`<img onerror>`, `<svg onload>`, `{{7*7}}`/`${7*7}`,
  `javascript:`, comillas de ruptura, RTL U+202E, emojis ZWJ, NUL, 100 000 car.) × diccionario, fragmentos, notas, Transforms
  (nombre/prompt), buscadores de historial/notas/reuniones, título de reunión, «Pregunta a tus reuniones», URL de Ollama.
  Lo guardado = lo escrito (salvo los `maxlength` visibles 120/4000/40).
- **Flow bar**: avisos, 9 tipos de error, reunión `done`/`error`, ajustes y tooltips con HTML → todo `textContent`; estados
  inventados (`__proto__`, `constructor`…), payloads nulos, niveles `NaN/Infinity/-5/'x'`, ráfaga de 2 000 estados → sin
  errores, vuelve a reposo; aviso de 100 000 car. cabe en 360×150.
- **Prisa**: triple clic en Guardar (Transform/Agregar palabra), «Actualizar ahora» (1 `update_install`), descarga del modelo
  (1 `model_download_start`, navegar y cambiar a inglés a mitad → termina `done`), «Sí, eliminar» doble (1 `delete_note`),
  Configuración 40× abrir/cerrar por Esc/×/fondo (0 velos colgados) + 40 Tab (0 fugas de foco), URL de Ollama cerrando sin
  Enter (se guarda por Esc/×/fondo), volver a Transforms con la IA activándose (se ve el progreso).
- **Pantallas**: 900×600 (mínimo de `tauri.conf.json`), 900×600 a escala 200 %, claro + `prefers-reduced-motion`, 4K →
  sin scroll horizontal, sin controles fuera ni tapados; 0 animaciones infinitas decorativas con reduced-motion (el hub y la
  Flow bar; solo sigue el spinner de progreso de 8 barras, esencial).

## Huecos (solo con el backend real o fuera de esta zona)

- ALTA 1 asume que `w.close()` destruye el WebView2 sin dar tiempo al temporizador de 600 ms (`main.rs:179-186` lo dice;
  no ejecutado en Tauri). Igual con Alt+F4 o apagar Windows con el hub abierto.
- MEDIA 1 en el backend: `src-tauri/src/aisetup.rs:127-146` (`url_host`/`is_loopback_url`) parte igual por `/ ? #`; si el
  cliente HTTP de Ollama resuelve `\@` como WHATWG, el registro de «regla 1» (`log_if_remote`) tampoco salta. No ejecutado
  (zona backend).
- MEDIA 2 en la BD real: sin UNIQUE en `words`/`snippets` el duplicado se guardaría igual; con muestras de voz pendientes el
  segundo `add_word` fallaría y el error iría a un modal ya cerrado. No ejecutado.
- `listen(...).then((u) => offs.push(u))` (`home.js:283-293`, `insights.js:167`): si la página se desmonta antes de que el IPC
  de Tauri resuelva el `listen`, el oyente queda colgado (con `alive=false`). El mock resuelve al instante: no reproducible.
- Diferencias WebView2/Chrome: `focusout` al quitar un campo enfocado (de él depende que el título de reunión se guarde
  cuando una reunión nueva rehace la cabecera; en Chrome se guardó 3/3), portapapeles, `<audio>` con WAV dañado.
- 320 px y 1280×800 al 200 % se miden solo como informativos (`pantallas`): Tauri no deja bajar de 900×600 ni hacer zoom de
  WebView; a 320 px la barra lateral ocupa todo y los botones de cabecera y de Configuración quedan fuera.
- Historial con 100 000 filas: el mock pagina a 50 y no se puede sembrar a ese volumen sin tocar `mock.js`.
- Flow bar real: click-through/hitbox Win32, `flowbar_context_menu` y niveles `NaN` (serde los serializa como `null`).
