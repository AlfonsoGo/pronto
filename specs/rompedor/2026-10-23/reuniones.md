Rompedor · 2026-10-23 · reuniones · 0 altas, 5 medias, 5 bajas · Arreglado (23-oct): MEDIA 1 (`audio.rs` `WavReader::open_recovering`, `notes.rs` `transcribe_words`), MEDIA 2 (`auto.rs` `step_browsers`, `detect.rs` `scan`), MEDIA 3 (`detect.rs` `title_service`/`candidates`, `meeting.rs` `spawn_detector`; Slack, Webex, Jitsi y Whereby solo se avisan en «ask»), MEDIA 4 (`notes.rs` `polish_turns`, cortacircuitos en `meeting.rs` `process_meeting`), MEDIA 5 (`meetingdb.rs` `fail_summary_status`/`close_orphan_meetings`, `meetings.js` aviso), BAJA 6 (`meetingdb.rs` `carry_done`), BAJA 8 (`auto.rs`), BAJA 9 (`notes/turns.rs`). Sin arreglar: BAJA 7 (export Markdown), BAJA 10 (`db.rs`) y los Huecos
Datos: solo bases en memoria (`Db::open_memory`) y WAV en `%TEMP%` (borrados) · `%APPDATA%\Pronto2\pronto2.db` 270336 B, mtime 15-oct 21:29:15 antes y después · restaurados ✅ | Servidor usado: ninguno (Ollama simulado con cierres; el real ni se tocó)

Rama `fases/2026-10-22` (commit 2f78f7f + árbol compartido con 6 rompedores). Sondas en `src-tauri/src/rompedor_reuniones.rs`,
las de hoy con prefijo `rompedor_reuniones_2_`. Las que rompen llevan `#[ignore = "rompedor: …"]`:
`CARGO_TARGET_DIR=C:/Users/usuario/dev/pt-f cargo test rompedor_reuniones_2_ -- --include-ignored --nocapture --test-threads=1`.
`rompedor_reuniones_2_solapes_extremos` tarda ~7 min (perfil `test`). Sin red, sin Ollama real, sin dispositivos, sin motor real.
Sin `--include-ignored`: 16 pasan, 11 ignoradas (10 roturas + 1 medida de apoyo).

Huecos:
- **Suspensión con una reunión abierta**: no se puede suspender el PC de Txonso. Si `Instant` cuenta el tiempo suspendido
  (QueryPerformanceCounter en Windows; no verificado aquí), al despertar `fill_silence` (`meeting.rs:417-430`) escribe
  todo el hueco de golpe. Medido (`rompedor_reuniones_2_coste_de_una_hora_de_silencio`): 1 h de silencio = 115 MB por
  pista, 3,7 s de escritura y 7,2 s de fase 2 (perfil test, 0 trozos al motor). Una noche de 10 h son ~2,3 GB entre las
  dos pistas. Un fin de semana de 60 h son ~13,8 GB y pasan del tope de 4 GB del WAV (37 h, `audio.rs:309`; el lector
  sigue leyendo hasta el final del fichero, así que no se pierde). Mientras escribe, el hilo no mira `stop`. No hay tope
  de duración ni aviso en una manual olvidada (R3: nunca se para sola, por diseño). Si el disco se llena, también fallan
  las escrituras de la base.
- **Teams colgado con el micro abierto**: la automática sigue al registro (`detect.rs:178-182`). Si la app se congela
  sin soltar el micro, o el registro se queda con `LastUsedTimeStop = 0` tras un cierre forzoso, la automática no se
  para nunca. No se puede reproducir sin la app real.
- **Arranque automático fallido sin aviso**: si `start_auto` falla (las dos pistas no abren), `meeting.rs:823-825` solo
  hace `eprintln!`. En modo «Automático» el usuario cree que se graba y no se graba nada. Sin `AppHandle` en las pruebas.
- **Chrome con PiP automático de Meet**: al cambiar de pestaña, Chrome puede abrir una ventana flotante de Meet. Si su
  título es el de la sala, la MEDIA 2 no pasaría en Meet (en Teams o Zoom web sí). No comprobado.
- Hueco de la primera pasada sin cambios: `close_orphan_meetings` sigue corriendo después de `hotkey::start` y
  `spawn_controller` (`main.rs:226-231`). Una reunión empezada con el atajo en ese hueco sale `failed` con
  `ended_at = started_at` hasta pararla. Es benigno: al pararla se corrige.
- Índice FTS con 120 000 filas (`rompedor_reuniones_2_fts_con_muchas_reuniones`): `replace_segments` tarda 130 ms,
  `save_summary` 50 ms y buscar «presu» 291 ms. `meeting_id UNINDEXED` obliga a recorrer el índice en cada borrado, pero
  hoy no es una rotura.
- Todo medido en el perfil `test` (debug). No se usó el motor real.

## ALTA

Ninguna confirmada.

## MEDIA

1. **Tras un apagón o un pantallazo azul, si la cabecera del WAV queda a ceros, la reunión se queda sin notas para
   siempre aunque el audio siga en el disco** (`src-tauri/src/notes.rs:66-72`: el arreglo de anoche solo trata como
   vacía una pista de ≤ 44 B. `src-tauri/src/audio.rs:356-357`: «no es un WAV». `src-tauri/src/meeting.rs:597` y `:625`:
   `complete = false` y `PARTIAL_TRANSCRIPT` en cada intento).
   Cómo pasa: NTFS suele dejar a ceros las páginas que no se volcaron antes de un corte (los típicos ficheros «llenos de
   NUL»). La página de la cabecera es la que más se reescribe (`sync_header` cada 5 s, `audio.rs:318-329`) y no se hace
   `fsync`.
   Reproducción: `rompedor_reuniones_2_cabecera_a_ceros_bloquea_las_notas`. Dos pistas de 2 min. La primera página
   (4 KB) de «Ellos» se pone a ceros. Después, tres veces «Reintentar» como hace `meeting.rs`.
   Evidencia: `intento 1..3: turnos=30 fallos=["…them.wav): no es un WAV"] final=false`. `«Ellos»: 3872044 B, 959910
   muestras con audio tras la página a ceros`. Con las dos cabeceras a ceros: `me -> Err(…no es un WAV)`. La reunión
   entera no se transcribe nunca, aunque el 99,9 % de las muestras está intacto.
   Arreglo propuesto: los WAV de reunión los escribe siempre Pronto con una cabecera fija de 44 B (PCM16 mono 16 kHz).
   Si `WavReader::open` falla en un `meeting_*.wav`, leer desde el byte 44 con ese formato. Si una pista falla dos
   veces seguidas con el mismo error, dar por final la transcripción de las pistas buenas y avisar (lo mismo se propuso
   en la primera pasada y no se hizo).

2. **Cambiar de pestaña durante una videollamada en el navegador corta la reunión automática: ese rato no se graba y la
   llamada queda partida en dos reuniones** (`src-tauri/src/detect.rs:111-119`: solo vale el título y Windows solo da el
   de la pestaña ACTIVA de cada ventana. `src-tauri/src/auto.rs:81-88`: sin ese título 20 s → `Stop`, sin bloquear).
   Reproducción: `rompedor_reuniones_2_cambiar_de_pestana_corta_la_reunion`. Llamada de 30 min (micro del navegador en
   uso todo el rato). De 10:00 a 13:00 el usuario mira el correo de Outlook (Edge, Teams web) o una hoja de cálculo
   (Chrome, Meet) en otra pestaña de la misma ventana. Se simula el detector cada 3 s con `detect_all` + `AutoMachine`.
   Evidencia: `Edge: reuniones [(3, 618, "Microsoft Teams (Edge)"), (780, 1800, "Microsoft Teams (Edge)")]; grabado 1635
   s de 1800 s`. Igual en Chrome con Meet. Salen **2 reuniones y 162 s de llamada sin grabar** en el modo de fábrica
   (R2). Además, cada trozo se resume aparte.
   Arreglo propuesto: el título solo para EMPEZAR. Una vez en marcha una automática de navegador, seguir al navegador:
   pararla cuando ese exe suelte el micro durante `END_GRACE_MS`, no cuando desaparezca el título.

3. **Otra ventana del mismo navegador con el título de Teams, Zoom o Jitsi hace grabar como reunión una llamada de
   Discord o WhatsApp Web. Incumple PRD §1 «decidido» («No graba reuniones de Discord … se limita a Teams, Zoom y Meet»)**
   (`src-tauri/src/detect.rs:115-118`: vale cualquier ventana visible de ese navegador. `:63-65`: el filtro de Discord
   solo quita la ventana cuyo título dice «discord». `:76-87`: `contains("| microsoft teams")`, `starts_with("zoom meeting")`
   y `starts_with("jitsi meet")` también casan con páginas que no son llamadas).
   Reproducción: `rompedor_reuniones_2_otra_ventana_con_teams_graba_discord`. Chrome con el micro en uso para una llamada
   de Discord web o WhatsApp Web, y otra ventana de Chrome con Teams web en el chat o el calendario (abierto todo el día en
   muchas oficinas), la página de producto de Teams, un tutorial de YouTube o un artículo.
   Evidencia: `Discord web + Teams en el chat: [(3, 60, "Microsoft Teams (Chrome)")]`. Lo mismo con `WhatsApp Web +
   Teams en el calendario`, `Discord web + página de Teams`, `Discord web + tutorial en YouTube` («Zoom Meeting: tutorial
   para principiantes - YouTube») → `Zoom (Chrome)` y `WhatsApp Web + artículo` («Jitsi Meet, la alternativa gratis a
   Zoom - Xataka») → `Jitsi (Chrome)`. En «Automático» empieza a grabar micro y altavoces de una llamada privada sin
   preguntar. De paso: `slack.exe → ["Slack (huddle)"]`, `CiscoCollabHost.exe` y `atmgr.exe → ["Webex"]` se graban solos,
   también fuera de la lista del PRD (Teams, Zoom, Meet).
   Arreglo propuesto: en el navegador, solo títulos de llamada en curso: Meet con código de sala (ya está), Teams con
   «Reunión…/Meeting…/Llamada… | Microsoft Teams» (no «Chat |», «Calendario |»…), Zoom solo «Zoom Meeting» o «Zoom
   Webinar» exactos (sin texto detrás) y Jitsi solo `meet.jit.si`/«| Jitsi Meet». Si cualquier ventana de ese navegador
   es de Discord o WhatsApp, no disparar sola: avisar como en «ask». Quitar Slack, Webex, Jitsi y Whereby del modo
   automático, o pasar el PRD a «ask» para ellos.

4. **Si Ollama se cuelga a mitad del pulido, la fase 2 encadena un plazo de 240 s por turno: unas 7 h con la reunión
   «procesando», sin poder borrarla, reintentarla ni actualizar Pronto** (`src-tauri/src/notes.rs:181-187`: solo para
   si falla la PRIMERA petición; las siguientes hacen `continue`. `src-tauri/src/meeting.rs:660`: `timeout` de 240 s;
   `:670`: calentamiento de 900 s. `commands.rs:696`, `meeting.rs:746` y `updater.rs:903`: lo que se bloquea mientras).
   Reproducción: `rompedor_reuniones_2_ollama_colgado_a_mitad_del_pulido`. Reunión de 1 h (400 turnos, uno de cada cuatro
   necesita pulido). El «Ollama» contesta a la primera petición y a partir de ahí cada llamada devuelve el error de
   plazo.
   Evidencia: `pulido: Ok(1), 100 llamadas, 99 colgadas; etiquetas: 1 llamadas (err=true); notas: 1 llamadas (err=true);
   fase 2 con Ollama colgado ≈ 900 s de calentamiento + 101 × 240 s = 7.0 h`. Las etiquetas y las notas sí paran al
   primer fallo. La regla 4 se cumple llamada a llamada (todas tienen plazo), pero la función degradada arrastra a borrar,
   reintentar y actualizar. Con Ollama CAÍDO (conexión rechazada) está bien: `rompedor_reuniones_2_ollama_caido_la_reunion_se_guarda`
   deja guardados los 20 turnos, todos etiquetados por reglas, `final=true`, y las notas fallan con «No se pudo contactar
   con Ollama».
   Arreglo propuesto: en `polish_turns`, cortar en el primer error de plazo o de conexión, no solo en el primero de la
   lista (o tras 2 fallos seguidos), y dejar `polish_done` sin marcar para el siguiente «Regenerar». Opcional: un plazo
   total de la fase 2 con Ollama.

5. **Un «Regenerar» que falla esconde las notas buenas que ya había: resumen, tareas y marcas desaparecen del hub hasta
   que Ollama vuelva** (`src-tauri/src/meeting.rs:752` y `:614`: `pending`. `:731-738` `fail_summary` y
   `src-tauri/src/meetingdb.rs:362` `close_orphan_meetings`: `failed`. `ui/hub/pages/meetings.js:261-265`: con `failed`
   solo pinta «No se pudo generar el resumen»).
   Reproducción: `rompedor_reuniones_2_regenerar_fallido_esconde_las_notas`. Notas guardadas con una tarea marcada.
   «Regenerar» con (a) Ollama caído, o (b) Pronto cerrado (o el PC apagado) mientras regenera.
   Evidencia: `Ollama caído al regenerar: estado «failed», en la base: resumen Some("Se aprobó el contrato."), tareas
   [("Enviar el contrato", true)]`. El caso (b) da lo mismo. Los datos siguen en la base (el export los saca), pero el
   hub no los enseña y la tarea no se puede marcar. Basta con que el modelo devuelva basura («El modelo no sacó nada…»).
   Arreglo propuesto: al regenerar, si falla y ya había resumen, volver a `done` (o a un estado `stale` que pinte las
   notas con un aviso «No se pudo regenerar»). `close_orphan_meetings` no debe pasar a `failed` una reunión `pending`
   que ya tenía resumen.

## BAJA

6. **«Regenerar» sigue desmarcando las tareas hechas que el modelo redacta con otras palabras: el arreglo de anoche
   solo cubre el texto idéntico** (`src-tauri/src/meetingdb.rs:228-236`: `task_key` exacto; el arreglo propuesto era un
   parecido ≥ 0,8).
   Reproducción: `rompedor_reuniones_2_regenerar_con_tareas_parafraseadas`. Tres tareas marcadas. Al regenerar, el
   modelo (temperatura 0,1 y 0,5) las vuelve a escribir.
   Evidencia: `[("Enviar contrato a Ana", false), ("Llamar al proveedor y confirmar el precio", false), ("Revisar el
   presupuesto del trimestre.", true)]`: 2 de 3 marcas perdidas.
   Arreglo propuesto: conservar `done` si `notes::similarity(viejo, nuevo) ≥ 0.8` (y mismo responsable si lo hay),
   emparejando cada tarea vieja una sola vez.

7. **El export sigue dejando pasar Markdown activo: títulos con `===`, listas numeradas, casillas `[x]` falsas, títulos
   dentro de una lista y enlaces por referencia** (`src-tauri/src/summary.rs:611-616`: `md_inert` solo quita `![`, `](`,
   `<` y los enlaces automáticos. `:628-643`: `md_block` solo escapa `# > - * + |` al principio de línea y `md_line` no
   escapa nada al principio).
   Reproducción: `rompedor_reuniones_2_exportar_markdown_hostil`. Se guardan notas hostiles y el export se pasa por
   CommonMark (`markdown_it`, sin filtrar esquemas, como Obsidian o Notion).
   Evidencia: el resumen `…\nPagar hoy 5.000 € a ES00 0000\n===` sale como
   `<h1>Se revisó la factura. Pagar hoy 5.000 € a ES00 0000</h1>`. `1. Transferir ya` sale como `<ol>`. Un punto clave
   `[x] Transferencia de 5.000 € aprobada` sale como `- [x] …`, una casilla marcada en GFM. `## Decisiones` sale como
   `<li><h2>Decisiones</h2></li>`. `[factura]: //ejemplo.invalid/f` en un punto clave hace que la decisión «Pagar la
   [factura]» sea `<a href="//ejemplo.invalid/f">factura</a>`. La fuente realista es la salida del modelo (el anclaje
   la filtra en parte), así que es defensa en profundidad.
   Arreglo propuesto: escapar con `\` todo carácter de puntuación ASCII de Markdown (``\ ` * _ { } [ ] ( ) # + - . ! | < > = :``)
   en `md_inert`, no solo al principio de línea. Así no queda ni referencia, ni casilla, ni `===`, ni `1.`.

8. **Poner la detección en «No» con una automática en marcha (aunque se vuelva a «Automático» al momento) la deja
   grabando para siempre, aunque prometió «Se para solo al colgar»** (`src-tauri/src/auto.rs:71-74`: `Off` reinicia la
   máquina y olvida `auto_app`. `:90-101`: al volver, `Recording::Auto` sin `auto_app` se trata como manual).
   Reproducción: `rompedor_reuniones_2_modo_off_deja_la_automatica_sin_parar`. Teams cuelga a los 10 min. La detección
   pasa a «No» en t=30 s (y en el segundo caso vuelve a «Automático» en t=33 s).
   Evidencia: `se para sola en None s (colgó en 600 s)` en los dos casos, simulados durante 2 h.
   Arreglo propuesto: con `Off`, no olvidar una automática en marcha (seguir parándola al colgar) o pararla al cambiar
   el modo. Si `recording == Auto` y no hay `auto_app`, tomar `in_use.first()` como su app, no tratarla como manual.

9. **Cuando las dos personas hablan a la vez y sin pausas, el reparto de turnos es cuadrático: 40 min tardan 271 s
   (perfil test) y 3 h tardarían más de 1 h** (`src-tauri/src/notes/turns.rs:317-333`, `drop_echo_turns`: por cada palabra
   del turno del micro recorre TODAS las palabras de «Ellos» activas en el turno con `sounds_alike` (Levenshtein). Con un
   turno largo, n × n).
   Reproducción: `rompedor_reuniones_2_solapes_extremos` (más `…_desglose`). Las dos pistas hablan a la vez sin pausas
   de 700 ms y con vocabularios distintos (un vídeo con voz de fondo mientras el usuario explica, un intérprete, o ruido
   transcrito como palabras).
   Evidencia: `5 min 4.9 s · 10 min 19.0 s · 20 min 78.7 s · 40 min 271.0 s` (×4 por cada vez que se dobla la duración;
   «crecimiento respecto a lineal ×6.9»). Con un punto cada 3 palabras: 10 min 13,0 s y 20 min 56,5 s. Desglose a 10 min:
   `drop_echo_turns 17.2 s` frente a `remove_echo 0.0`, `remove_echo_tails 0.2` e `interleave 0.00`. El arreglo de
   `interleave` de anoche funciona (3 h en 1,8 s). Además, 40 min de solape salen en solo 3 turnos.
   Arreglo propuesto: en `drop_echo_turns`, comparar cada palabra solo con las de «Ellos» activas a ±`ECHO_WINDOW_MS` de
   ella (`active` con `partition_point`, como en `remove_echo`), no con todas las del turno, o partir antes los turnos
   del micro de más de 30 s.

10. **Tras actualizar, una reunión de la versión instalada que quedó huérfana (Pronto o el PC se cayeron) pierde su
    audio: «Reintentar» no lo encuentra y «Eliminar» lo deja en el disco** (`src-tauri/src/meetingdb.rs:355-358`:
    `ended_at = started_at`. `src-tauri/src/meeting.rs:316-317` y `src-tauri/src/db.rs:70-76`: el WAV antiguo
    `meeting_{id}_{who}.wav` solo vale si se modificó antes de `ended_at + 10 min`).
    Reproducción: `rompedor_reuniones_2_huerfana_antigua_pierde_su_audio`. WAV con el nombre antiguo, escrito por última
    vez a los 40 min; después, `close_orphan_meetings`.
    Evidencia: `ended_at = started_at: true; WAV de 40 min tras «Eliminar»/«Reintentar»: no se reconoce como suyo (se queda
    en el disco y la fase 2 no lo ve)`. Hoy no hay ningún `meeting_*.wav` en `%APPDATA%\Pronto2\audio` (solo se listaron
    los nombres), así que a Txonso no le afecta ahora. Sí afectaría a quien tenga reuniones `failed` con audio de la
    versión publicada.
    Arreglo propuesto: con `ended_at == started_at` (huérfana), usar como fin la fecha de modificación del propio WAV, o
    aceptar el nombre antiguo sin ventana si la reunión es la última con ese id en la base. En `close_orphan_meetings`,
    tomar la duración del tamaño del WAV.

## Arreglos de la primera pasada: confirmados

Sondas de la primera pasada, ya sin `#[ignore]`, repetidas hoy. Todas pasan:
- ✅ MEDIA 1, pista vacía. `rompedor_reuniones_pista_vacia_bloquea_las_notas_para_siempre`: `WAV de «Ellos»: 44 B` (ahora
  la cabecera se vuelca al crear), `intento 1..3: turnos=30 fallos=[] final=true`. Atacado: se rompe con una cabecera
  a ceros (MEDIA 1 de hoy).
- ✅ MEDIA 2, detección por subcadena. `rompedor_reuniones_falsos_positivos_de_la_deteccion`: TeamSpeak, ZoomIt, `#teams |
  … Discord` y WhatsApp más un artículo de zoom dan `[]`. Atacado: se rompe con otras ventanas de Teams, Zoom o Jitsi y
  al cambiar de pestaña (MEDIAS 2 y 3 de hoy).
- ✅ MEDIA 3, reparto de turnos de 3 h. `rompedor_reuniones_tres_horas_de_interrupciones`: `conversation() 1.8 s`
  (antes 165 s), `sin partir: 0`. Atacado: se rompe con solapes continuos, aunque en `drop_echo_turns`, no en
  `interleave` (BAJA 9).
- ✅ BAJA 4, la manual no se para sola. `rompedor_reuniones_manual_tras_parar_la_auto_se_para_sola`: `t=None`. Atacado:
  falla al pasar a «No» (BAJA 8). La suspensión y Teams colgado van a Huecos.
- ✅ BAJA 5, export. `rompedor_reuniones_exportar_con_texto_hostil`: el título queda en una línea y sin `## Tareas`, y
  `](javascript:` sale escapado. Atacado: deja pasar `===`, `1.`, `[x]` y enlaces por referencia (BAJA 7).
- ✅ BAJA 6, regenerar. `rompedor_reuniones_regenerar_desmarca_tareas`: `antes: true; después: true`. Atacado: falla si
  la tarea se redacta distinto (BAJA 6) y el regenerar fallido esconde las notas (MEDIA 5).
- ✅ BAJA 7, palabras cortas. `rompedor_reuniones_busqueda_ignora_palabras_cortas`: `«IA» -> 1; «Q3» -> 1; «plan IA» -> ["Lunes"]`.
- ✅ En verde, sin cambios: WAV raros, dos reuniones seguidas, títulos y búsqueda hostiles, Ollama hostil (sin pánico),
  listas largas (1000 elementos, 0,35 s) y resumen en bucle (≤ 0,7 s).

Cambios de hoy revisados: `meetingdb.rs` en transacción (0e5c4d2): `replace_segments`, `save_summary` y `delete_meeting`
son atómicos, y un borrado a mitad de fase 2 da `Ok(false)` sin resucitar. Sin roturas. `any_processing` (1a0b4f7): el
actualizador espera de verdad, pero eso agrava la MEDIA 4 (hasta ~7 h esperando). Motor (b4e0d58): la fase 2 sigue
cediendo al dictado (`background_turn`). No se vio que un trozo de reunión pueda fallar por el dictado (espera, no
devuelve error).
