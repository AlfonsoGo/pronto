Rompedor · 2026-10-23 · hub · 0 altas, 6 medias, 10 bajas · Arreglado: MEDIA 1-6 y BAJA 1, 3, 5, 6, 8, 9 (más la parte del hub de BAJA 2); sin tocar: BAJA 2 en Rust, 4 (velo sobre la barra de título), 7 (meetings.js), 10 (Rust) y los Huecos (ver «Arreglado» al final)
Datos: sin datos reales (backend simulado + bases temporales `Tmp` de Rust, borradas solas) · `%APPDATA%\Pronto2\pronto2.db` 270336 B, mtime 2026-10-15 21:29:15.377 antes y después (no se abre) · restaurados ✅
Servidor usado: `scripts/servir-ui.mjs` en http://127.0.0.1:4181 (Chrome headless, `playwright-core`) · Rust con `CARGO_TARGET_DIR=C:/Users/usuario/dev/pt-g` · 0 peticiones fuera de 127.0.0.1 · sin commit

Zona: `ui/hub.html`, `ui/hub.js`, `ui/hub/**`, `ui/shared/**` y los comandos IPC que llaman. Rama `fases/2026-10-22` @ 2f78f7f.
Ejecutar: `node e2e/rompedor/correr.mjs [segunda|estados|tildes|ingles|axe-control|prisa|entradas|xss-backend|flowbar|pantallas]`
(con `ROMPEDOR_SIN_AXE=1`, `estados` se salta los 70 pases de axe, ~13 min). Rust:
`cargo test rompedor_hub_ -- --ignored --nocapture --test-threads=1`. Salidas: `e2e/rompedor/capturas/salida-1a.txt`
(primera pasada repetida), `salida-2a.txt`, `salida-2b.txt`, `salida-2c.txt`, `salida-axe.txt` (ignoradas por git, como las capturas).
`e2e/rompedor/comun.mjs`: el servidor de las sondas pasa de puerto libre a 4181 (`ROMPEDOR_PUERTO`).

## Huecos

- **Doble «Reintentar transcripción» con el backend real** (`src-tauri/src/commands.rs:231-236`): el segundo clic espera al primero
  y devuelve `Ok(get_history(id))`. Si el primero falla («No se ha oído nada», «El audio guardado está dañado», motor sin cargar),
  el segundo devuelve el dictado SIN cambios como éxito y el hub enseña «Transcripción actualizada» junto al error. No ejecutado:
  `retry_history`, `retry_claim` y `retry_wait` piden `tauri::State` o son privados de `commands`, y el crate no activa la feature
  `test` de tauri; desde `rompedor_sondas.rs` no se pueden llamar. La parte del hub (dos llamadas) sí está reproducida (BAJA 2).
- **Alt+F4 / «Salir» de la bandeja con un editor modal a medias** (fragmento, palabra): con un modal abierto la × de la ventana
  queda tapada (BAJA 4), así que solo se llega por Alt+F4 o la bandeja; ahí `pagehide`/`beforeunload` solo vacían la nota
  (`flushPending`), no los modales: lo escrito se perdería sin preguntar. No ejecutable sin Tauri (en Chrome se simuló con un
  clic por JS en la × tapada: `ventana.close` 1×, `add_snippet` 0×).
- **¿Llega el `save_note` de `pagehide` al backend en WebView2?** En Chrome sale (beacon visto, caso A3). En Tauri 2 el IPC va por
  `fetch` al protocolo `ipc://`; si WebView2 cancela las peticiones al destruir la ventana, ni siquiera el arreglo de anoche
  alcanza en Alt+F4. No ejecutable sin Tauri.
- **Dos pestañas sobre la misma nota**: el hub real es una sola ventana (`tray.rs:50-71` reutiliza la etiqueta `hub`; una segunda
  creación a la vez falla con «label already exists»), y en el navegador cada pestaña tiene su propio mock en memoria: no hay
  forma realista de reproducirlo. Para quien lo mire: `Db::save_note` (`db.rs:1809`) con el `id` de una nota borrada la vuelve a
  crear con otro id, en silencio.
- **Cambio de carpeta del modelo + cambio de idioma con Whisper mientras carga**: `apply_settings` (`commands.rs:47-60`) manda
  la carpeta vieja en la segunda carga; la nueva época descarta la primera y `adopt_model_dir` no se llama: el cambio de carpeta
  se perdería sin aviso. No ejecutable: `Engine::load_then` y `apply_settings` piden `AppHandle`. «Usar» ya no deja lanzarlo
  (apagado durante la carga, comprobado), pero el diálogo de Idiomas no está apagado.
- «Usar» con el motor real colgado (sin ningún `engine://status`): el mock siempre manda `ready` a los 1,6 s; no se pudo probar.
- Lectores de pantalla reales (NVDA/Narrador): solo axe y teclado.

## MEDIA

1. **Cerrar el hub con un autoguardado de la nota aún en vuelo pierde lo último que se escribió** (`ui/hub/util.js:39`
   `flushPending(ms = 1500)`, `ui/hub/pages/notes.js:75` y `:91`). Los guardados van en fila (arreglo de anoche contra la nota
   duplicada) y el de cierre espera al anterior; `flushPending` corta a los 1,5 s y cierra la ventana. Si el `save_note` que ya
   iba en vuelo tarda más (base ocupada por la copia diaria, una reunión escribiendo, antivirus), el guardado con lo último
   nunca llega a mandarse: en Tauri la ventana ya no existe.
   Reproducción: `node e2e/rompedor/correr.mjs segunda` → «nota: cerrar con un guardado en vuelo (2500 ms)»: nota nueva, escribir
   «Primera idea», esperar 700 ms (el autoguardado sale), escribir « y la segunda que importa», × de la ventana.
   Evidencia: `save_note mandados antes de ventana.close: ["Primera idea"]; ventana.close a 4172 ms, el save_note con el texto
   completo sale a 4696 ms`. Con 800 ms de base sí sale antes (`["Primera idea","Primera idea y la segunda que importa"]`).
   Captura `e2e/rompedor/capturas/nota-cerrar-en-vuelo-2500.png`.
   Arreglo propuesto: en el `onBeforeClose` de notas mandar ya `save_note` con el texto actual sin esperar a la fila (si la nota
   aún no tiene id, esperar al primer guardado sin tope y luego actualizar), o que `windowAction('close')` espere a la fila de
   notas sin el corte de 1,5 s (con «Guardando…» a la vista).

2. **El editor de Transforms descarta el prompt sin preguntar** (`ui/hub/pages/transforms.js:121`; sin `dirty` como los modales).
   El arreglo «confirmar antes de descartar» de c5527f9 solo llegó a los modales de palabra y fragmento. En Transforms un prompt
   escrito con calma se pierde con un clic en el menú lateral, «Cancelar» o «‹ Transforms».
   Reproducción: `correr.mjs segunda` → «descartar: Transforms»: «Crea el tuyo», nombre «Modo jefe», prompt de 624 caracteres,
   salir por cada vía. Evidencia: `{"menú lateral":{"pregunta":false,"guardado":false},"Cancelar":{"pregunta":false,"guardado":false},
   "«‹ Transforms»":{"pregunta":false,"guardado":false}}` · `capturas/transform-descartado.png`.
   Arreglo propuesto: guardar una foto del formulario al abrir y, si cambió, pedir `confirmDialog('¿Descartar los cambios?')` en
   `back` y en la navegación (`hub.js` `navigate` → un `canLeave()` de la página actual).

3. **URL de Ollama de otro equipo + Esc: se guarda y el aviso «tus dictados salen del PC» no se llega a ver**
   (`ui/hub/pages/settings.js:319`; aviso en `:50`). El aviso solo se recalcula en el `change` del campo, y Configuración guarda
   el campo al cerrarse (Esc, ×, clic fuera: comprobado en `prisa` caso 15). Escribir la URL y cerrar con Esc deja Ollama apuntando
   fuera del PC sin que el único aviso de la regla 1 aparezca (ni un toast). Además, mientras se escribe, el aviso enseña el estado
   de la URL anterior.
   Reproducción: `correr.mjs segunda` → «aviso URL + Esc» (un `MutationObserver` apunta si `#oll-remote` llega a estar visible).
   Evidencia: `ollamaUrl guardada «http://192.168.1.50:11434»; aviso visible en algún momento: false; avisos (toast): []; modales
   abiertos: 0`. Variante en vivo: `http://evil.example\@localhost:11434: escribiendo false → tras Enter true`.
   Arreglo propuesto: recalcular `#oll-remote` en `input`, y al guardar una URL no local desde el cierre del modal, un toast de
   aviso (`kind: 'err'`) con el mismo texto.

4. **Doble clic en «Cargar más» pinta dictados repetidos** (`ui/hub/pages/home.js:134-142`, `load(false)` sin bandera de carga
   en vuelo; `offset = items.length` se lee antes del `await`). Las dos peticiones piden la misma página y las dos se concatenan.
   Reproducción: `correr.mjs estados` → «Cargar más: doble clic (100 000)» (backend con 100 000 dictados que pagina como
   `db.rs:1300`, `list_history` a 700 ms). Evidencia: `filas 120, distintas 80; list_history 3×` · `capturas/cargar-mas-duplicados.png`.
   Arreglo propuesto: `if (loading) return;` al entrar en `load(false)` y apagar el botón mientras carga.

5. **Buscar mientras «Cargar más» está en vuelo mete en los resultados dictados que no coinciden** (`ui/hub/pages/home.js:141`
   `items = reset ? page : items.concat(page)` sin comprobar que la respuesta es de la consulta actual; `doSearch` en `:147`).
   La página que llega tarde se pega a los resultados de la búsqueda.
   Reproducción: `correr.mjs estados` → «Cargar más + buscar»: «Cargar más» con la base a 1,2 s y, al momento, buscar
   «número 99999». Evidencia: `resultados 41, que no contienen la búsqueda 40: «Dictado número 99960 con algo de texto…»` ·
   `capturas/buscar-mezcla.png`.
   Arreglo propuesto: un contador de petición (`const req = ++seq; … if (req !== seq) return;`) en `load`.

6. **Regla 6: violaciones graves de axe fuera de lo que mira el test** (`ui/shared/cristal.css:184` y `ui/hub/pages/dictionary.js:312`).
   - `color-contrast` (serious, WCAG 1.4.3 AA) en `.btn-danger-solid`: blanco sobre `--rojo-fuerte` `#E5484D` = **3,91:1** (< 4,5).
     Es el botón de todos los `confirmDialog` («Sí, eliminar», «Descartar», borrar historial…), en tema oscuro y claro.
   - `label-content-name-mismatch` (serious, WCAG 2.5.3 A) en el botón de muestras de voz de la lista del Diccionario: texto visible
     «1 de 3», nombre accesible «1 grabación de tu voz. Grábala 2 veces más…» (el texto visible no está en el nombre).
   `tests/no-se-negocia/06-accesibilidad.test.mjs` no los ve: no abre diálogos, y la página del Diccionario la pasa a 1180×780,
   donde axe no cuenta esa fila; a 1280×800 (ventana algo mayor o maximizada) sí.
   Reproducción: `correr.mjs estados` (axe en 18 estados + 16 modales × 2 temas) y `correr.mjs axe-control` (mock puro, sin
   `__TAURI__` falso). Evidencia: `[dark] ¿descartar los cambios? → serious color-contrast: .btn-danger-solid`, `[light] confirmar
   borrado → serious color-contrast: .btn-danger-solid`, `axe mock puro /hub.html?page=dictionary 1280×800: label-content-name-mismatch
   div[data-id="3"] > .v-main > .v-voice`; a 1180×780: 0. El resto: 0 graves en base vacía, sin modelo, modelo con error, motor con
   error, actualización disponible/con error, sin Ollama, Ollama sin modelo, inglés, editores de palabra/fragmento/nota/Transform,
   detalle de reunión y su transcripción, menú «⋯», atajos, micrófono, idiomas, aviso de IA y la IA activándose.
   Arreglo propuesto: `--rojo-fuerte` ≥ 4,5:1 con blanco (p. ej. `#C8333A`, 5,3:1) o texto oscuro; en `.v-voice`, que el nombre
   empiece por lo visible («1 de 3 grabaciones de tu voz. …»). Y que el test 06 abra al menos un `confirmDialog` y use también 1280×800.

## BAJA

1. **Si el autoguardado de una nota falla, cerrar no lo reintenta** (`ui/hub/pages/notes.js:75`: `save()` pone `dirty = false`
   antes de saber si guardó). Una nota nueva cuyo primer guardado falló se pierde entera al cerrar el hub.
   Reproducción: `correr.mjs segunda` → «nota: guardado fallido + cerrar» (`save_note` falla una vez). Evidencia: `estado antes de
   cerrar «No se pudo guardar»; save_note antes de ventana.close: 1 (el fallido); nota en la base: false`.
   Arreglo: volver a poner `dirty = true` en el `catch` de `saveNote`.
2. **«Reintentar transcripción» se puede lanzar dos veces sobre el mismo dictado** (`ui/hub/pages/home.js:200`, sin marca de
   «reintentando» ni ítem apagado). Con un audio largo el aviso dura 2,6 s y no queda nada que diga que sigue en marcha.
   Reproducción: `correr.mjs segunda` → «reintentar ×2» (`retry_history` a 2,5 s). Evidencia: `retry_history 2× sobre id 1000; avisos
   durante: ["Reintentando la transcripción…","Reintentando la transcripción…"]; al final: ["Transcripción actualizada","Transcripción
   actualizada"]`. Con el backend real el segundo espera al primero (b4e0d58) y, si el primero falla, da un falso «actualizada» (Huecos).
   Arreglo: un `Set` de ids reintentando en `home.js` (ítem apagado y un spinner en la fila) y, en `commands.rs:233`, que el que espera
   devuelva el resultado del primero (o un `Err` si falló), no la fila sin cambios.
3. **La × y «Cancelar» del editor de fragmento/palabra descartan sin preguntar; Esc y el clic fuera sí preguntan**
   (`ui/hub/util.js:186`, `[data-close]` → `ctl.close()` en vez de `ctl.dismiss()`). Reproducción: `correr.mjs segunda` →
   «descartar: velo, Esc, ×, Cancelar». Evidencia: velo y Esc → `["Agregar fragmento","¿Descartar los cambios?"]`; `tras ×: modales
   abiertos []; tras «Cancelar»: []` (1 320 caracteres perdidos). Arreglo: la × del encabezado llama a `dismiss()` («Cancelar» puede
   quedarse como descarte explícito).
4. **Con cualquier modal abierto el velo tapa la barra de título: no se puede mover, minimizar, maximizar ni cerrar la ventana con
   el ratón** (`ui/shared/cristal.css:262` `.velo{position:fixed;inset:0;z-index:100}`). Reproducción: `correr.mjs segunda` → «barra de
   título bajo el velo» (Configuración abierta, `elementFromPoint` sobre cada botón). Evidencia: `[["min","tapado por .velo"],["max",
   "tapado por .velo"],["close","tapado por .velo"]]; zona de arrastre: tapada por .velo` · `capturas/velo-tapa-titulo.png`.
   Arreglo: `.velo{top: <alto de .hdr>}` o subir `.hdr` por encima del velo.
5. **El foco cae al `<body>` tras acciones de teclado que repintan** (regla 6, «todo operable con teclado»):
   «Usar» en Motor (`ui/hub/pages/settings.js:359` `paintEngine` rehace la lista con el botón enfocado dentro) y «Eliminar transcripción»
   desde el menú «⋯» del historial (`ui/hub/util.js:244-245`, el menú se cierra y la fila desaparece). El siguiente Tab vuelve al
   principio del modal o de la página. Reproducción: `correr.mjs segunda` → «Usar: foco con teclado» (`foco tras Enter: body; tras Tab:
   button «General»`) y `correr.mjs estados` → «foco: borrar dictado con teclado» (`foco al final body`). Borrar una nota con el teclado
   sí deja el foco en la tarjeta siguiente. Arreglo: tras repintar, enfocar el botón equivalente (o el chip «Cargando…» con
   `tabindex=-1`); tras borrar una fila, la siguiente o el botón «⋯» de la siguiente.
6. **Aviso «tus dictados salen del PC» con direcciones que sí son de este PC** (`ui/hub/pages/settings.js:44-47`: solo
   `localhost`, `127.0.0.1`, `[::1]`; el backend, `updater.rs:291` `is_loopback_host`, acepta todo 127/8 y `::ffff:127.x`).
   Reproducción: `correr.mjs segunda` → «aviso URL de Ollama» (30 variantes). Evidencia: `http://[::ffff:127.0.0.1]:11434 →
   [::ffff:7f00:1]`, `http://127.0.0.2:11434 → 127.0.0.2` con aviso. Las 30 variantes que salen del PC sí avisan (0 sin aviso).
   Arreglo: misma regla que `is_loopback_host` (127.0.0.0/8 y IPv4 mapeada).
7. **Título de reunión + Enter repetido + Tab → `rename_meeting` 2×** (`ui/hub/pages/meetings.js:491` focusout; `d.title` se
   actualiza tras el `await`, así que un segundo `focusout` en vuelo vuelve a mandarlo). Inocuo salvo dos toasts «Título
   actualizado». Reproducción: `correr.mjs segunda` → «reunión: título + Enter ×3» (`rename_meeting` a 400 ms) → `2×`.
   Arreglo: comparar con el último valor mandado, no con `d.title`.
8. **Textos sin traducir con el hub en inglés**. Claves `t()` sin entrada en `ui/hub/en.js`: `ui/hub/pages/insights.js:140`
   «Correcciones desde tu diccionario» (title), `ui/hub/pages/transforms.js:125` «'{x}' se eliminará permanentemente.» (cuerpo de
   «¿Eliminar este Transform?»), `ui/hub/pages/vocab.js:96` «No se pudo eliminar "{x}"». Y los ejemplos de la página Estilo
   (`ui/hub/pages/style.js:14-17`, «¿tienes libre mañana para almorzar?…») no pasan por `t()`. Reproducción: `correr.mjs ingles`
   (estática + en vivo con `?lang=en` en 8 páginas, 7 pestañas y 4 estados). Arreglo: añadir las 3 claves y unos ejemplos en inglés.
9. **Los buscadores de Bloc de notas, Fragmentos y Diccionario no pliegan tildes** (`ui/hub/pages/notes.js:22`,
   `ui/hub/pages/vocab.js:32`: `toLowerCase().includes(q)`). El del historial sí desde 68707a4. Reproducción: `correr.mjs tildes`.
   Evidencia (las tres páginas): `{"canción":1,"cancion":0,"CANCIÓN":1,"Canción":1,"acion":0}`. Arreglo: comparar sobre
   `s.normalize('NFD').replace(/\p{M}/gu,'').toLowerCase()` (y lo mismo en `highlight` para que marque).
10. **El buscador de reuniones no encuentra el título si cambian tildes o mayúsculas no ASCII** (`src-tauri/src/meetingdb.rs:314`
    `m.title LIKE ?2`: el título no está en `meeting_fts`, y `LIKE` solo pliega ASCII). Una reunión sin notas todavía (manual, o
    con la fase 2 fallida) solo se encuentra por su título exacto. Reproducción: sonda
    `rompedor_sondas::rompedor_hub_buscar_reunion_por_titulo_con_tildes` (`#[ignore]`). Evidencia: `«Óscar»: historial 1, reuniones 1 |
    «óscar»: historial 1, reuniones 0 | «oscar»: historial 1, reuniones 0 | «OSCAR»: historial 1, reuniones 0 | «revision»: historial 1,
    reuniones 0 | «Revisión»: historial 1, reuniones 1 | «REVISIÓN»: historial 1, reuniones 0`. Arreglo: `fold(m.title) LIKE ?` con la
    función `fold` que ya registra `init` para el historial.

## Lo que aguantó (con reproducción)

- **XSS: 0.** `xss-backend` (backend que envenena cada texto y evento, 3 variantes): veneno como texto en 126/126 pantallas, 0 nodos
  `[data-pwn]`, 0 `onerror`; `entradas` (9 cargas × 9 campos): 0, y lo guardado = lo escrito. Resaltado con 17 consultas hostiles
  (`&`, `<`, `"`, `amp`, `lt`, `.*`, `(`, `\`, `$&`, `İ`, `ss`, `<img`, `mark`…) × 5 textos hostiles (HTML vivo, entidades ya
  escritas, regex, `İstanbul ß ǅ`) en Notas, Fragmentos y Diccionario: 0 entidades a la vista, 0 HTML vivo, 0 `__pwnHit`.
- **Dobles guardados: 1 llamada en todo**: editar palabra + Enter ×3 (`update_word` 1×), editar fragmento + Ctrl+Enter ×3
  (`update_snippet` 1×), «Pregunta a tus reuniones» + Enter ×3 (`ask_meetings` 1×), «Borrar historial» doble clic (`clear_history` 1×),
  «Usar» doble clic (`save_settings` 1×), más los de `prisa`.
- **«Usar» durante una carga**: con un backend que avisa `loading` al instante y tarda 12 s, 0 botones encendidos a los 0,5/4/6/9 s
  y `save_settings` 1×; sin ningún aviso del motor, el temporizador de 5 s los vuelve a encender (5,4 s: `[false]`).
- **Confirmar antes de descartar**: clic en el velo y Esc preguntan; Esc sobre la pregunta la cierra y el editor sigue con sus 1 320
  caracteres; Esc en un editor sin cambios cierra sin preguntar y devuelve el foco a «Agregar nuevo».
- **Nota**: con un guardado en vuelo de 800 ms lo último sale antes de cerrar; al descargar la página (`pagehide`) el `save_note`
  sale; papelera + «Cancelar» no deja el estado colgado.
- **Aviso de IA fuera del PC**: 0 de 30 variantes que salen del PC sin aviso (`\@`, `@`, `#@`, `?@`, `/@`, `http:host`, `%2f@`,
  `nip.io`, `0.0.0.0`, LAN, tabuladores, `ws:`, `file:`).
- **100 000 dictados**: primera página 175 ms; «Cargar más» ×40 → 1 640 filas, p95 795 ms, máx 860 ms, 0 repetidos (con un clic
  cada vez); buscar tecleando con 1 640 filas pintadas: 1,6 s para 11 letras a 30 ms + 0,8 s de espera. Estadísticas con
  2,4 M palabras / 12 M correcciones: sin desbordes (las etiquetas de mes rebasan su caja a propósito y se leen bien).
- **Estados raros sin errores JS**: base vacía, sin modelo, modelo con error, motor con error, sin Ollama, Ollama sin modelo,
  actualización disponible/con error, inglés: 0 `pageerror` en todas las sondas.

## Arreglos de la primera pasada: confirmados

Repetidos con las mismas sondas (`node e2e/rompedor/correr.mjs`, salida `capturas/salida-1a.txt`: 0 ALTA · 0 MEDIA · 0 BAJA · 32 OK)
más los casos nuevos de `segunda`.

- ✅ ALTA 1 · cerrar el hub justo después de escribir en una nota: `["save_note","ventana.close"]` (pero ver MEDIA 1: con un guardado
  anterior de > 1,5 s aún falla).
- ✅ MEDIA 1 · URL de Ollama con `\@` sin aviso: `http://evil.example\@localhost:11434` avisa tras guardar (y 29 variantes más; ver MEDIA 3
  para el caso de cerrar con Esc).
- ✅ MEDIA 2 · doble Enter / Ctrl+Enter repetido: `add_word 1×`, `add_snippet 1×`.
- ✅ MEDIA 3 · nota nueva + «Atrás» con el guardado en vuelo: 1 nota.
- ✅ MEDIA 4 · renombrar reunión + «Atrás»: sin `pageerror`, guardado en backend y la lista muestra el título nuevo.
- ✅ MEDIA 5 · clic fuera / Esc descartan sin preguntar: ahora preguntan (ver BAJA 3 para la ×).
- ✅ BAJA 1 · doble clic en «Activar IA local»: 1 aviso, `ai_setup_start 1×`.
- ✅ BAJA 2 · Flow bar con acción desconocida: sin botón.
- ✅ BAJA 3 · audio dañado: un solo aviso, «No se pudo reproducir el audio».
- ✅ BAJA 4 · resaltado que rompe entidades: `{"a":"Tom & Jerry <3 \"hola\"","l":…,"o":…}` y 17 consultas hostiles más sin entidades.

## Arreglado (23-oct, implementador del hub)

Comprobado con `node e2e/rompedor/correr.mjs` (todas las sondas, `ROMPEDOR_SIN_AXE=1`): 0 ALTA · 0 MEDIA; `node --test tests/no-se-negocia/06-accesibilidad.test.mjs` 3/3 y `npm run e2e` 35/35.

- MEDIA 1 · `ui/hub/util.js` `flushPending` espera sin el corte de 1,5 s (tope de seguridad 30 s) y devuelve true/false; `notes.js` `onBeforeClose` espera al guardado en vuelo y al pendiente y reintenta una vez; `ui/hub.js` `windowAction('close')` enseña «Guardando antes de cerrar…» y, si falla, avisa y la siguiente × cierra igualmente.
- MEDIA 2 · `transforms.js`: foto del formulario al abrir; `‹ Transforms`/Cancelar y el menú lateral (`hub.js` `navigate` + `cleanup.canLeave`) piden «¿Descartar los cambios?».
- MEDIA 3 · `settings.js`: el aviso `#oll-remote` se recalcula en `input`; si se guarda una URL no local con Configuración ya cerrada sale un toast de error con el mismo texto.
- MEDIA 4 y 5 · `home.js` `load`: guardia `moreBusy` y contador `seq` (respuesta vieja descartada); el foco pasa al primer dictado nuevo.
- MEDIA 6 · `cristal.css`: `--rojo-fuerte` #C8333A (5,26:1) y hover #D13C43 (4,73:1); `dictionary.js`: nombre accesible de `.v-voice` empieza por «{n} de {m}». El test 06 pasa ahora a 1180×780, 1280×800 y 900×600 (mínimo de la ventana) y abre 13 diálogos/editores; a 900×600 salieron dos fallos reales más, arreglados: `hub.css` (barra lateral con `flex-shrink:0`, tarjeta «100 % local» mínima con poco alto → `target-size`) e `insights.js` (`.hm-scroll` con `tabindex=0` y `role=region` → `scrollable-region-focusable`).
- BAJA 1 · `notes.js`: `dirty = true` si el guardado falla; «Atrás» no sale si el guardado falla. BAJA 2 (solo el hub) · `home.js`: `retrying` apaga el ítem. BAJA 3 · la × del encabezado llama a `dismiss()`. BAJA 5 · foco tras «Usar» (`settings.js` `renderTab`/`paintEngine`) y tras cancelar/borrar desde «⋯» (`util.js` `openMenu` devuelve el foco al botón; `home.js` `remove`). BAJA 6 · `isLoopbackUrl` acepta 127.0.0.0/8 y IPv4 mapeada. BAJA 8 · claves en `en.js` y ejemplos de Estilo en inglés. BAJA 9 · `fold` (NFD + sin marcas) en notas, fragmentos, diccionario y `highlight`.
- Sondas adaptadas: `tildes.mjs` (la consulta «acion» no es subcadena de «canción»: ahora «ancion» y «NCIÓN»), `segunda.mjs` (Transforms confirma «Descartar» para volver a la lista), `estados.mjs` (la confirmación abre con «Cancelar» enfocado: comprueba cancelar y borrar con Tab + Enter).
