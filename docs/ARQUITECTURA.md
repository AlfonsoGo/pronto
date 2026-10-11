# Pronto 2 — arquitectura y contrato

Clon local de Wispr Flow para Windows. **Interfaz y funcionamiento de Wispr Flow; colores y estética
de Pronto («Cristal etéreo»).** Todo corre en el equipo: ni audio ni texto salen de él.

- Backend: Rust + Tauri 2 (`src-tauri/`). Crate `pronto`.
- Interfaz: HTML/CSS/JS sin bundler (`ui/`), servida por Tauri. `window.__TAURI__` global
  (`withGlobalTauri: true`). Nada de CDN: fuentes y SVG van en `ui/assets/`.
- Motor de voz: NVIDIA Parakeet TDT 0.6B **v3** int8 (multilingüe, 25 idiomas europeos, español e
  inglés incluidos, con puntuación y mayúsculas) vía `sherpa-onnx` 1.13.8, `model_type =
  "nemo_transducer"`. Se reutiliza el modelo de Pronto 1 si está completo; si no, se descarga a
  `%APPDATA%\Pronto2\models\parakeet` (ver «Contrato v5»). Ficheros:
  `{encoder,decoder,joiner}.int8.onnx + tokens.txt`. La ruta es un ajuste (`model_dir`).
- Especificación visual y de comportamiento de Wispr Flow: `docs/wispr-ui-spec.md` (sacada con REA
  del `app.asar` 1.6.721) y las capturas descritas abajo.

## Ventanas

| label | fichero | qué es |
|---|---|---|
| `hub` | `ui/hub.html` | Ventana principal (sidebar + páginas). Sin decoración nativa: barra propia con min/max/cerrar. Cerrar = ocultar a la bandeja. |
| `flowbar` | `ui/flowbar.html` | La «Flow bar»: píldora flotante abajo al centro del monitor con foco, transparente, siempre encima, sin barra de tareas, **nunca roba el foco**. |

Bandeja del sistema: icono de Pronto, menú «Abrir Pronto», «Pegar última transcripción»,
«Micrófono ▸» (lista), «Salir».

## Funcionamiento (igual que Wispr Flow)

- **Atajo por defecto: mantener `Win + Alt`** («Hold Win + Alt and speak»). Mientras se mantiene,
  graba; al soltar, transcribe y pega en la app con foco.
- **Manos libres:** `Win + Alt + Espacio` arranca a grabar sin mantener; se para con el mismo atajo
  o con el botón ■ de la Flow bar.
- **Cancelar:** `Esc` durante la grabación descarta el audio (la Flow bar dice «Cancelado»).
- **Pegar última transcripción:** `Alt + Shift + Z`.
- Los atajos son configurables (Ajustes ▸ General ▸ Atajos). Se implementan con un hook de teclado
  de bajo nivel (`WH_KEYBOARD_LL`) en un hilo propio, porque `Win + Alt` solo modificadores no se
  puede registrar con `RegisterHotKey`. El hook se **traga** la tecla Win cuando forma parte del
  atajo para que no se abra el menú Inicio (truco: inyectar un `VK_E8` neutro antes de soltar Win).
- Grabaciones de menos de 300 ms se ignoran (toque accidental).
- **Pegado:** se guarda el portapapeles, se escribe el texto, `Ctrl+V` con `SendInput`, y se
  restaura el portapapeles a los 600 ms.
- **Silenciar música al dictar** (ajuste, por defecto activo): baja el volumen maestro mientras graba.
- **Sonidos** de inicio/fin (ajuste, por defecto activo), generados en `ui/assets/sounds/`.

### Tubería de texto (en este orden, en `text.rs`)

1. Transcripción cruda de Parakeet.
2. **Limpieza** (ajuste por Estilo): quitar muletillas en español e inglés («eh», «em», «mmm»,
   «um», «uh», «o sea» solo al inicio), espacios dobles, espacio antes de signos.
3. **Retroceso**: «no, mejor…» / «quiero decir» / «scratch that» borra la frase anterior (como
   Wispr «backtrack»). Solo si el ajuste está activo.
4. **Diccionario**: reemplazos (`de` → `a`, insensible a mayúsculas, por palabra completa) y las
   entradas sin reemplazo se pasan como *hotwords* al motor.
5. **Snippets**: si el texto dictado (normalizado: minúsculas, sin puntuación final) coincide con el
   disparador de un snippet, se sustituye por su expansión. También dentro de la frase.
6. **Estilo** por tipo de app (Wispr «Style»): Formal / Casual / Muy casual, por categoría
   (Mensajes personales, Mensajes de trabajo, Correo, Otros). Determinista y local:
   - Formal: mayúscula inicial, punto final, sin cambios.
   - Casual: mayúscula inicial, sin punto final en frases sueltas.
   - Muy casual: todo en minúscula, sin punto final.
   La categoría sale del ejecutable con foco (`whatsapp.exe`, `telegram.exe`, `discord.exe` →
   personales; `slack.exe`, `teams.exe` → trabajo; `outlook.exe`, `thunderbird.exe` o título con
   «Gmail»/«Outlook» → correo; resto → otros).

## Contrato IPC

Todos los comandos son `#[tauri::command]` y devuelven `Result<T, String>`. Nombres en snake_case,
argumentos en camelCase desde JS (`invoke('add_word', { word, replacement })`).

### Comandos

| comando | args | devuelve |
|---|---|---|
| `get_settings` | — | `Settings` |
| `save_settings` | `settings: Settings` | `()` |
| `list_mics` | — | `string[]` |
| `get_stats` | — | `Stats` |
| `list_history` | `query?: string, limit: number, offset: number` | `HistoryItem[]` |
| `delete_history` | `id: number` | `()` |
| `clear_history` | — | `()` |
| `copy_text` | `text: string` | `()` |
| `paste_last` | — | `()` |
| `list_words` | — | `Word[]` |
| `add_word` | `word: string, replacement?: string` | `Word` |
| `update_word` | `id, word, replacement?` | `()` |
| `delete_word` | `id` | `()` |
| `list_snippets` | — | `Snippet[]` |
| `add_snippet` | `trigger, expansion` | `Snippet` |
| `update_snippet` | `id, trigger, expansion` | `()` |
| `delete_snippet` | `id` | `()` |
| `list_notes` | — | `Note[]` |
| `save_note` | `id?: number, text: string` | `Note` |
| `delete_note` | `id` | `()` |
| `start_dictation` | `handsFree: bool` | `()` (botón de la Flow bar) |
| `stop_dictation` | — | `()` |
| `cancel_dictation` | — | `()` |
| `engine_status` | — | `EngineStatus` |
| `set_flowbar_hover` | `hover: bool` | `()` (para ensanchar la ventana al pasar el ratón) |

### Eventos (backend → todas las ventanas, `app.emit`)

| evento | payload |
|---|---|
| `dictation://state` | `{ state: "idle" \| "listening" \| "handsfree" \| "processing" \| "done" \| "cancelled" \| "error", message?: string }` |
| `dictation://level` | `{ level: number }` 0..1, ~30 por segundo mientras graba |
| `dictation://result` | `HistoryItem` (cada dictado nuevo, para que el hub lo pinte arriba) |
| `engine://status` | `EngineStatus` |
| `settings://changed` | `Settings` |

### Tipos (serde, `camelCase` en JSON)

```ts
Settings = {
  hotkeyHold: Hotkey,          // por defecto { win:true, alt:true, ctrl:false, shift:false, key:null }
  hotkeyHandsFree: Hotkey,     // { win:true, alt:true, key:"Space" }
  hotkeyPasteLast: Hotkey,     // { alt:true, shift:true, key:"Z" }
  micName: string | null,      // null = predeterminado del sistema
  language: "auto" | "es" | "en",
  appLanguage: "es" | "en",
  modelDir: string,
  launchAtLogin: boolean,
  showFlowBar: boolean,        // Flow bar visible en reposo (Wispr: «Show Flow bar at all times»)
  sounds: boolean,
  muteMusic: boolean,
  removeFillers: boolean,
  backtrack: boolean,
  styles: { personal: Tone, work: Tone, email: Tone, other: Tone },  // Tone = "formal"|"casual"|"veryCasual"
  historyRetentionDays: number // 0 = siempre
}
Hotkey = { ctrl: bool, alt: bool, shift: bool, win: bool, key: string | null }
HistoryItem = { id, createdAt: string /*ISO*/, text: string, rawText: string, app: string, durationMs: number, words: number }
Stats = { totalWords: number, wpm: number, streakDays: number, dictations: number, wordsThisWeek: number, byDay: {date: string, words: number}[] }
Word = { id, word: string, replacement: string | null, createdAt }
Snippet = { id, trigger: string, expansion: string, createdAt }
Note = { id, text: string, createdAt, updatedAt }
EngineStatus = { ready: boolean, loading: boolean, modelDir: string, error: string | null }
```

WPM = palabras / minutos de audio dictado (todo el historial). Racha = días seguidos hasta hoy con
al menos un dictado.

## Datos

SQLite en `%APPDATA%\Pronto2\pronto2.db` (`rusqlite` bundled). Tablas `history`, `words`,
`snippets`, `notes`, `settings` (una fila, JSON). Ajustes por defecto si no hay fila.

## Módulos Rust (`src-tauri/src/`)

| módulo | responsabilidad |
|---|---|
| `main.rs` | arranque Tauri, bandeja, ventanas, registro de comandos |
| `state.rs` | `AppState` (ajustes, motor, db, máquina de estados del dictado) |
| `audio.rs` | captura `cpal`, mezcla a mono, remuestreo a 16 kHz, nivel RMS |
| `asr.rs` | carga Parakeet en hilo aparte, `transcribe(&[f32]) -> String` |
| `hotkey.rs` | hook `WH_KEYBOARD_LL`, emite `HotkeyEvent::{HoldDown, HoldUp, HandsFree, Cancel, PasteLast}` |
| `inject.rs` | portapapeles + `SendInput` Ctrl+V; app con foco (exe y título) |
| `text.rs` | tubería de texto (con tests unitarios) |
| `db.rs` | SQLite y consultas |
| `media.rs` | silenciar/restaurar volumen maestro (`IAudioEndpointVolume`) |
| `commands.rs` | los `#[tauri::command]` del contrato |

## Estética (de Pronto, `cristal.dart`)

```
--violeta: #8B5CF6;  --violeta-hondo: #6D28D9;  --violeta-suave: #A78BFA;
--amarillo: #FDE047 (solo al 5 %, la única chispa cálida);  --verde: #34D399 (solo «hecho»);
--rojo: #F87171 (grabando / error);  --oled: #050505 (fondo);  --tinta: #F5F3FF;  --apagado: #9C93AE;
--cristal-pildora: rgba(14,11,20,.86);  --hilo: rgba(255,255,255,.08);  --hilo-fuerte: rgba(255,255,255,.14);
--brillo: inset 0 1px 1px rgba(255,255,255,.15);
radios: carcasa 32 (+6 aire → núcleo 26), tarjeta 24 (+5 → 19), caja 16, tecla 7, píldora 999.
Fuente: Plus Jakarta Sans (ui/assets/fonts). Orbes de fondo violeta y amarillo, desenfocados.
```

Los radios interiores son **concéntricos**: interior = exterior − aire. Es lo que separa el estilo de
su parodia.

## Desviaciones (backend Rust)

Cambios y decisiones respecto al contrato, tal como quedó implementado en `src-tauri/`.

**Módulos añadidos** a la tabla anterior: `models.rs` (tipos serde del contrato), `flowbar.rs`
(posición y visibilidad sin foco), `tray.rs` (bandeja) y `autostart.rs` («Iniciar con Windows»,
valor `HKCU\...\Run` con `--hidden`, que arranca sin abrir el hub).

**Contrato IPC**
- `set_flowbar_hover(hover, width?, height?)`: dos argumentos opcionales (px lógicos). Sin ellos:
  `hover=true` → 320x72, `hover=false` → 260x64. La ventana siempre queda abajo al centro del
  monitor en el que está.
- `Stats.byDay`: últimos 30 días, orden ascendente, **con ceros** (`date` = `YYYY-MM-DD` local).
  `wordsThisWeek` = últimos 7 días móviles. `streakDays` cuenta desde ayer si hoy aún no hay
  dictado (así no marca 0 cada mañana). `wpm` con un decimal.
- `HistoryItem.app` = ejecutable con la primera en mayúscula y sin `.exe` («Chrome», «Code»).
- `paste_last` devuelve error si no hay historial. Desde el hub pega en la ventana con foco, que
  será el propio hub; para copiar desde la UI está `copy_text`.
- Los mensajes de error de `dictation://state` van en español, sea cual sea `appLanguage`.

**Motor**
- Hotwords del diccionario: **no implementadas**. Con `greedy_search` sherpa las ignora; harían
  falta `modified_beam_search` + fichero de hotwords. Los reemplazos del diccionario sí funcionan
  (`text.rs`). Las entradas sin reemplazo hoy no hacen nada.
- `Settings.language` se guarda pero no se aplica: Parakeet v3 detecta el idioma solo.
- Audio de más de 60 s se parte en trozos cortando en el punto más silencioso; la grabación se
  corta a los 10 min.

**Atajos**
- `HoldDown` se emite en cuanto `Win + Alt` están pulsadas. Si luego llega `Espacio`, la grabación
  «mantener» pasa a manos libres (no se reinicia). Otras combinaciones que empiecen igual
  (p. ej. `Win+Alt+G` de Xbox) arrancan una grabación que se descarta por durar < 300 ms.
- El neutralizador `VK_E8` se inyecta al activarse el atajo (cubre Inicio y la barra de menús de
  Alt) y al tragar una tecla del atajo con Win/Alt pulsadas. Las teclas inyectadas por Pronto
  llevan `dwExtraInfo = 0x50524F32` y el hook las ignora.
- `Esc` se traga solo mientras hay una grabación (el flag lo pone el controlador).
- Antes de `Ctrl+V` se espera (máx. 2 s) a que se suelten Win/Alt/Shift/Ctrl; si no, la app
  recibiría `Win+Alt+Ctrl+V` al parar el modo manos libres.

**Texto**
- Más disparadores de retroceso además de los del contrato: «mejor dicho», «no, quiero decir»,
  «borra eso», «no, wait». «No, mejor…» al principio del dictado se deja intacto (es una respuesta
  normal: «No, mejor no»). El retroceso borra desde el último fin de frase anterior; sin puntuación
  previa borra todo lo anterior.
- Los reemplazos del diccionario y las expansiones de snippets se protegen del paso de estilo:
  «Muy casual» no los pasa a minúsculas.

**Sistema**
- Sonidos: `ui/assets/sounds/{start,stop}.wav` (generados con `scripts/gen_sounds.py`) van
  **incrustados** en el binario (`include_bytes!`) y se reproducen con `PlaySoundW`
  (`SND_MEMORY | SND_ASYNC`) desde Rust; así funcionan en la app empaquetada, donde `ui/` no existe
  en disco. La Flow bar no los toca.
- Silenciar música: se activa el *mute* del endpoint por defecto (no se baja el nivel), 180 ms
  después de empezar para que se oiga el pitido; solo se deshace si Pronto fue quien silenció.
- Cerrar cualquier ventana (hub o Flow bar) la oculta; la app solo termina con «Salir» de la
  bandeja.
- La máquina de estados vive en un único hilo controlador (`state.rs`) alimentado por canales:
  hotkeys, botones de la UI y fin de transcripción se serializan, sin bloqueos compartidos.

---

# Contrato v2 (manda sobre todo lo anterior donde choque)

Escrito tras `docs/wispr-ui-spec.md` (ingeniería inversa literal de Wispr Flow 1.6.721). Las
secciones de esa especificación se citan como **[spec §x]**. Tres agentes trabajan a la vez en
ficheros disjuntos: **backend** (`src-tauri/**`), **hub** (`ui/hub.*`, `ui/shared/**`) y
**flowbar** (`ui/flowbar.*`). Nadie toca ficheros de otro.

## Activación de atajos [spec §4.2]

- `DICTATION_DEBOUNCE = 500 ms`. Atajo «mantener» abajo → empieza a grabar **ya** (estado
  `listening`). Al soltar:
  - pulsado **≥ 500 ms** → para, transcribe y pega;
  - pulsado **< 500 ms** (toque) → espera 500 ms a un segundo toque. Si llega → pasa a **manos
    libres** (`handsfree`, sonido de bloqueo) sin perder lo grabado. Si no llega → descarta y emite
    `dictation://state { state:"error", message:"No lo toques. Mantén pulsado {atajo}." , kind:"tap" }`.
- `Win+Alt+Espacio` (atajo de manos libres) también entra en manos libres.
- En manos libres, pulsar otra vez el atajo «mantener» o el de manos libres = **terminar y pegar**.
  Botón ■ de la barra = igual. `Esc` o botón ✕ = cancelar.
- Atajos por defecto: mantener `Win+Alt` (el que Txonso usa en Wispr), manos libres `Win+Alt+Espacio`,
  pegar último `Alt+Shift+Z`, **copiar último `Alt+Shift+X`** (nuevo), **Transform 1 (Polish)
  `Win+Alt+1`** (nuevo), cancelar `Esc`.
- Duración máxima 20 min (aviso a los 19 con `message`).

## Estados del dictado (`dictation://state`)

`{ state, message?, kind? }` con `state ∈ idle | listening | handsfree | processing | done |
cancelled | error | polishing | polished` y `kind` opcional (`tap`, `nomic`, `noaudio`, `paste`,
`ollama`). `idle` = barra en reposo.

## Flow bar: ventana y click-through [spec §1, §3]

- La ventana `flowbar` es **fija de 360×150 px lógicos**, transparente, anclada abajo al centro
  del área de trabajo del monitor del cursor (margen 0: la píldora ya lleva su margen inferior de
  8 px dentro). Ya no cambia de tamaño. Se elimina `set_flowbar_hover`.
- Click-through: la UI informa del rectángulo pulsable en px CSS relativos a la ventana con
  `set_flowbar_hitbox { x, y, width, height }` (o `null` = nada pulsable). El backend sondea el
  cursor a ~30 Hz y llama a `set_ignore_cursor_events(!dentro)`. Así el hueco transparente no
  bloquea clics al escritorio.
- Clic derecho en la píldora → `flowbar_context_menu {}` → el backend abre un **menú nativo**
  (Tauri `Menu::popup`) con [spec §3.5]: «Ocultar 1 hora», «Configuración», separador, submenú
  «Micrófono» (radio con los dispositivos), separador, «Historial de transcripciones», «Pegar
  última transcripción».
- `hide_flowbar_for_hour` oculta la barra en reposo 60 min (sigue apareciendo al dictar).

## Comandos nuevos o cambiados

| comando | args | devuelve |
|---|---|---|
| `set_flowbar_hitbox` | `rect: {x,y,width,height} \| null` | `()` |
| `flowbar_context_menu` | — | `()` |
| `hide_flowbar_for_hour` | — | `()` |
| `open_hub` | `page?: string` | `()` (muestra y enfoca el hub; emite `hub://navigate {page}`) |
| `open_settings` | `tab?: string` | `()` (abre hub y emite `hub://settings {tab}`) |
| `copy_last` | — | `()` |
| `retry_history` | `id` | `HistoryItem` (re-transcribe el audio guardado; no pega) |
| `get_history_audio` | `id` | `string \| null` (WAV en base64, para reproducir con data URL) |
| `toggle_word_star` | `id` | `()` |
| `list_transforms` / `save_transform` / `delete_transform` | `Transform` | |
| `run_transform` | `id` | `()` (sobre la selección actual: Ctrl+C, Ollama, pegar) |
| `ollama_status` | — | `{ reachable: bool, models: string[] }` |
| `get_insights` | — | `Insights` |
| `reset_shortcuts` | — | `Settings` |
| `capture_hotkey` | — | `Hotkey` (bloquea hasta que se pulse una combinación y se suelte; Esc = cancela con error) |

## Eventos nuevos

`hub://navigate { page }`, `hub://settings { tab }`, `dictation://notice { title, body?, action? }`
(avisos de la barra tipo tarjeta, [spec §3.3 notificaciones]).

## Tipos nuevos o ampliados

```ts
Settings += {
  firstName: string,                 // «Hola de nuevo, {firstName}». Defecto: nombre de usuario de Windows
  hotkeyCopyLast: Hotkey,            // Alt+Shift+X
  hotkeyTransform: Hotkey,           // Win+Alt+1
  cleanupLevel: "none"|"light"|"medium"|"high",   // [spec §2.3 Auto cleanup], defecto "medium"
  pressEnterCommand: boolean,        // Experimental: «pulsa enter» al final → Enter
  ollamaUrl: string,                 // http://localhost:11434
  ollamaModel: string,               // qwen2.5:7b
  keepAudioDays: number,             // 14
  theme: "dark"|"light",             // defecto "dark" (Pronto)
  flowBarHiddenUntil: string|null,
}
Tone = "formal" | "casual" | "veryCasual" | "excited"   // excited: «!» en lugar de «.» final y en frases afirmativas cortas
HistoryItem += { hasAudio: boolean, transform?: string }
Word += { starred: boolean, source: "manual" | "auto" }
Transform = { id, name: string, prompt: string, builtin: boolean, hotkeyIndex: number|null }
  // de fábrica: «Polish» (claridad y concisión, mismo idioma) y «Prompt Engineer»
Insights = {
  totalWords, wpm, streakDays, longestStreak, dictations,
  heatmap: { date: string, words: number }[],      // últimos 365 días, con ceros
  topApps: { app: string, words: number, count: number }[],   // top 8
  fixes: { dictionary: number, snippets: number, fillers: number }
}
```

`cleanupLevel` (local y determinista, sin LLM): `none` = transcripción tal cual (solo estilo);
`light` = muletillas + espacios; `medium` = light + retroceso + palabras repetidas («que que»);
`high` = medium + quitar arranques en falso cortos («bueno, pues», «a ver,») al inicio.
Sustituye a `removeFillers`/`backtrack` (se mantienen por compatibilidad: si existen en la fila
guardada se migran a `cleanupLevel`).

## Audio guardado

Cada dictado guarda su WAV 16 kHz mono en `%APPDATA%\Pronto2\audio\{id}.wav`; se purga a los
`keepAudioDays`. Sirve para «Reintentar transcripción» y «Reproducir» [spec §2.3].

## Interfaz: reglas para los dos agentes de UI

- Estructura, textos y comportamiento: **los de Wispr** [spec §2, §3], en **español** (usa las
  traducciones ES de la spec; si no hay, traduce tú con el mismo tono).
- Aspecto: **Pronto «Cristal etéreo»** (sección «Estética» arriba), modo oscuro OLED por defecto,
  con tema claro opcional. Fuente Plus Jakarta Sans (`ui/assets/fonts/`). Ningún recurso de Wispr
  (ni sus SVG, ni fuentes, ni sonidos, ni su logo). Iconos: SVG inline estilo Lucide (trazo 1.75)
  dibujados a mano o copiados de Lucide (licencia ISC; deja el aviso en `ui/assets/LICENSES.md`).
- El logo es la marca de Pronto (`ui/assets/brand/pronto_mark.png`) + «Pronto».
- Fichero compartido de tokens: `ui/shared/cristal.css` (lo crea el agente **hub**; el de
  flowbar define los suyos en `flowbar.css` copiando los mismos valores, para no depender).
- Puente IPC: `window.__TAURI__.core.invoke` y `window.__TAURI__.event.listen`. Para poder abrir
  los HTML en un navegador sin Tauri, cada UI trae un **mock** (`ui/shared/mock.js` para el hub,
  dentro de `flowbar.js` para la barra) que se activa si `window.__TAURI__` no existe y simula
  datos y eventos. Así se pueden revisar con capturas.

## Desviaciones v2 (backend Rust)

Implementación del «Contrato v2» en `src-tauri/`. Todo lo no citado aquí sigue el contrato.

**Activación y estados**
- Debounce y doble toque viven en el controlador (`state.rs`), no en el hook. El hook estampa cada
  evento con su hora (`TimedEvent`), de modo que el arranque lento del micrófono no falsea los
  500 ms. Un toque suelto mantiene la grabación abierta 500 ms; el segundo toque pasa a manos
  libres sin perder audio y suena `lock.wav` (nuevo, `ui/assets/sounds/lock.wav`, en `scripts/gen_sounds.py`).
- Un toque sin segundo toque emite `error` con `kind:"tap"` y `message:"No lo toques. Mantén pulsado
  {atajo}."` (el atajo sale de los ajustes, p. ej. «Win + Alt»).
- Nuevos `kind`: además de `tap`, `nomic`, `noaudio`, `paste`, `ollama`, se usa **`select`** («Selecciona
  texto primero» / transform demasiado largo). `noaudio` también salta si el pico del audio es < 0,0005
  (micrófono silenciado) sin llegar a transcribir.
- Tope de 20 min y aviso a los 19 (re-emite `listening`/`handsfree` con `message` y manda un
  `dictation://notice`). El búfer de audio se limita a 20 min.
- El hook añade una red de seguridad: si se pierde el «up» de un modificador, `HoldUp` se emite con el
  siguiente evento de teclado. La tecla neutra `VK_E8` se inyecta en **cada** activación (también el
  segundo toque), por lo que el doble toque de `Win+Alt` no abre Inicio ni la barra de menús de Alt.
- Transform: si la tecla de `hotkeyTransform` es un dígito (`1`), ese y los dos siguientes lanzan
  Transform 1/2/3 (`hotkeyIndex`). Con otra tecla solo hay Transform 1.

**Flow bar**
- Ventana fija 360x150 lógicos (`tauri.conf.json` actualizado), margen 0, anclada al área de trabajo
  del monitor del cursor. `set_flowbar_hover` eliminado. `set_flowbar_hitbox` + sondeo a ~33 ms
  (solo con la barra visible) llaman a `set_ignore_cursor_events`; hasta que la UI informe de un
  rectángulo, toda la ventana es transparente a los clics.
- `flowbar_context_menu` abre un `Menu::popup` nativo con los ítems del contrato. Sus ids llevan el
  prefijo `ctx-` y se atienden en `Builder::on_menu_event`; los de la bandeja, en su propio manejador.
- `hide_flowbar_for_hour` guarda `flowBarHiddenUntil` (ISO) y la barra vuelve sola a la hora (también tras
  reiniciar). La visibilidad en reposo = `showFlowBar` y no estar oculta (`flowbar::rest_visible`).
- `open_hub(page?)` emite `hub://navigate {page}` solo si hay `page`; `open_settings(tab?)` emite siempre
  `hub://settings {tab}` (`tab` puede ser `null`). El menú contextual «Historial de transcripciones» navega a `page:"home"`.

**Ajustes**
- `firstName` por defecto = `USERNAME` de Windows con inicial mayúscula. `removeFillers`/`backtrack` se
  conservan en el JSON pero se ignoran; al leer una fila antigua sin `cleanupLevel` se migran
  (ambos → `medium`, solo muletillas → `light`, ninguno → `none`).
- `cleanupLevel` en `text.rs`: `none` = sin limpieza ni retroceso; `light` = muletillas y espacios;
  `medium` = + retroceso + palabras repetidas inmediatas («que que»; no toca «muy muy», «no no»…);
  `high` = + arranques en falso al inicio («bueno, pues», «a ver,»). `excited`: «!» en el final y en
  frases de hasta 10 palabras. `pressEnterCommand`: «pulsa/presiona enter», «press enter» al final
  se quita del texto y se pulsa Enter tras pegar.
- `keepAudioDays = 0` significa **no guardar audio** (y borra el existente).

**Datos**
- `PRAGMA user_version`: v1 = esquema original, v2 = columnas `has_audio`, `transform`, `fix_*`,
  `starred`, `source` y tabla `transforms` (con «Polish» y «Prompt Engineer», `hotkeyIndex` 1 y 2). Las
  migraciones son transaccionales y probadas con una base v1 con datos.
- Audio: `%APPDATA%\Pronto2\audio\{id}.wav` (PCM16 mono 16 kHz). `retry_history` re-transcribe y aplica la
  tubería actual (categoría de app deducida del nombre guardado) sin pegar. Se borran los WAV al eliminar
  historial y al caducar (`keepAudioDays`, al arrancar, al guardar ajustes y tras cada dictado).
- Un transform ejecutado añade una fila de historial con `transform` = nombre, `rawText` = selección original, 0 ms y 0
  palabras; **no cuenta** como dictado en `get_stats` ni en `get_insights`.
- `get_insights`: `heatmap` 365 días ascendente con ceros; `topApps` top 8 por palabras; `fixes` suma los
  reemplazos de diccionario, snippets y muletillas/repeticiones/arranques quitados por dictado.
- `save_transform(transform)` crea si `id` es 0 o no existe y devuelve el `Transform`; `delete_transform(id)`
  falla para los de fábrica. `builtin` no se puede modificar desde la UI.

**Ollama y comandos largos**
- `run_transform(id)` no bloquea: el controlador pasa a fase `Polishing` y trabaja en un hilo (Ctrl+C esperando
  el cambio de `GetClipboardSequenceNumber`, POST `{ollamaUrl}/api/generate` con `stream:false` y
  `"{prompt}\n\nTexto:\n{selección}"`, pegado y restauración del portapapeles original). Usa `ureq` 2 **sin TLS**:
  solo vale `http://` (Ollama local). Timeout 180 s.
- `retry_history`, `get_history_audio`, `ollama_status` y `capture_hotkey` son `async` con `spawn_blocking`.
  `capture_hotkey` expira a los 30 s («Tiempo agotado»); Esc devuelve error «Cancelado».

---

## Revisión y pruebas reales (Opus, 11-oct-2026)

Fallos encontrados al ejecutar la app de verdad y cómo quedaron:

1. **«state not managed» al abrir el hub (intermitente).** Las ventanas nacían desde
   `tauri.conf.json` y su página podía llamar a `get_settings` antes del `manage()` de `setup`.
   Ahora las ventanas llevan `"create": false` y se crean en `setup` justo después de `manage()`.
2. **La Flow bar desaparecía o podía robar el foco.** Cualquier API de ventana de Tauri que toque
   estilos (`show`, `hide`, `set_ignore_cursor_events`) hace que tao recalcule todos los estilos
   desde su estado interno y los reaplique con `ShowWindow(SW_SHOW)`: borraba `WS_EX_NOACTIVATE`
   y, como la habíamos mostrado con Win32 directo, la volvía a ocultar. Ahora es como la ventana
   «status» de Wispr: **siempre visible para el sistema** (`visible`, `focus: false`,
   `focusable: false`), la UI decide qué se pinta y el click-through se hace con
   `WS_EX_TRANSPARENT | WS_EX_LAYERED` directo, comprobando el estilo real en cada vuelta del
   sondeo. El sondeo emite `flowbar://hover` (sin él no llega `mouseleave`).
3. **El texto dictado no se pegaba.** Medido: tras soltar `Win + Alt`, la app destino veía `Alt↓`
   y `Alt↑` sin nada en medio, entraba en modo menú y se comía el `Ctrl+V` (pasa también sin
   Pronto). La tecla neutra `VK_E8` inyectada desde dentro del hook no llegaba en orden. Ahora la
   liberación de Alt/Win de un atajo se traga y otro hilo inyecta `E8↓ E8↑ tecla↑` en un solo
   `SendInput` (`hotkey::mask_release`). Probado con dos generadores de teclas distintos.
4. **Muletilla entre signos.** Parakeet escribe «¿eh?»; quitarla dejaba «Marta,? Te». Ahora una
   muletilla entre signos cierra la frase con punto y sustituye la coma colgante (test incluido).

Herramientas de depuración que quedan (solo en builds de depuración):
- `PRONTO_FAKE_WAV=ruta.wav` hace que el dictado use ese WAV (PCM16 mono 16 kHz) en vez del micro.
- `PRONTO_WAV="a.wav;b.wav" cargo test transcribe_wav -- --ignored --nocapture` transcribe WAVs.
- `inject` registra a qué ventana va cada `Ctrl+V` y qué teclas había pulsadas.

---

# Contrato v3 — Notetaker local (notas de reunión)

Pedido por Txonso el 11-oct-2026 tras probar Pronto 2 («me va bastante bien»). Es el Notetaker de
Wispr, pero **100 % local**: en Wispr va a la nube; aquí el audio se transcribe con Parakeet y el
resumen lo hace Ollama en el equipo. Fuente de verdad de la interfaz y los textos ES: los SCSS/TSX
de Wispr en `scratchpad/out/src_hub/src/renderer/pages/main/Notetaker/**`,
`components/Notetaker*`, `components/MeetingAvatar`, la ventana `meeting_recorder` y las claves
`hub_notetaker_*`, `hub_meetings_*`, `notetaker_*` de `out/es.json`. Ficheros disjuntos: el agente
**backend** toca `src-tauri/**`; el agente **UI** toca `ui/**`.

## Qué hace

1. **Grabar una reunión**: dos fuentes a la vez y por separado:
   - micrófono (el de Ajustes) → hablante **«Tú»**;
   - audio del sistema por **WASAPI loopback** del dispositivo de salida por defecto → **«Ellos»**.
   Separar por fuente sustituye a la diarización: no hacen falta modelos nuevos.
2. **Transcripción en directo por trozos**: cada fuente se corta en segmentos por silencio (VAD
   simple por energía; trozo máx. 20 s) y cada segmento se transcribe con el mismo motor Parakeet
   en un hilo aparte. Los segmentos llevan `{speaker, startMs, endMs, text}` y se emiten en vivo.
   Mientras haya reunión, el dictado normal sigue funcionando (el motor se comparte con un mutex;
   el dictado tiene prioridad).
3. **Al parar**: se guarda la transcripción completa y se pide a Ollama (`ollamaModel`) un resumen
   estructurado en el idioma de la reunión: título corto, resumen (3-5 frases), **tareas** (con
   responsable si se dice), **decisiones** y **temas**. Formato: JSON pedido con `format: "json"`.
   Si Ollama no responde, la reunión queda guardada con transcripción y `summaryStatus: "failed"`,
   y se puede reintentar.
4. **Preguntar sobre las reuniones** («Ask», [hub_notetaker_ask_*]): pregunta libre → Ollama con
   las transcripciones/resúmenes más relevantes (búsqueda por palabras en SQLite, máx. ~6k tokens
   de contexto) → respuesta en texto.
5. **Detección de reunión** (como el aviso de Wispr): cada 3 s se mira en el registro
   `HKCU\Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone`
   (y `NonPackaged`) qué apps tienen `LastUsedTimeStop == 0` (micro en uso ahora). Si es una app de
   reuniones (`zoom.exe`, `ms-teams.exe`, `teams.exe`, `webex*`, `slack.exe` en huddle, o un
   navegador cuyo título de ventana contenga «Meet», «Zoom», «Teams», «Jitsi», «Whereby») y no hay
   reunión grabándose → `dictation://notice` con `{title:"¿Tomar notas de esta reunión?",
   body: app, action: "start_meeting"}` (una vez por reunión detectada). Ajuste `meetingDetect`.
6. **Atajo** `Win + Alt + M` (como Wispr) = empezar / parar reunión. Ajuste `hotkeyMeeting`.

## Flow bar durante la reunión [spec §3.2 «reunión»]

Estado `meeting`: píldora `fit-content x 30` con **borde verde 2 px** (`#34D399`, el verde de
Pronto que dice «en marcha/hecho»), ondas verdes con el nivel combinado (mic + sistema) y botón ■
«Detener notas». El dictado normal puede ocurrir encima: al dictar la barra muestra el dictado y al
terminar vuelve a `meeting`. Satélite nuevo en reposo/ready: «Tomar notas» (icono de libreta) →
`start_meeting`.

## Comandos

| comando | args | devuelve |
|---|---|---|
| `start_meeting` | `title?: string` | `Meeting` |
| `stop_meeting` | — | `Meeting` (con `summaryStatus: "pending"`; el resumen llega por evento) |
| `meeting_status` | — | `{ active: bool, meeting?: Meeting, elapsedMs: number }` |
| `list_meetings` | `query?: string, limit, offset` | `MeetingListItem[]` |
| `get_meeting` | `id` | `MeetingDetail` |
| `rename_meeting` | `id, title` | `()` |
| `delete_meeting` | `id` | `()` |
| `retry_summary` | `id` | `()` |
| `ask_meetings` | `question: string, meetingId?: number` | `string` |
| `export_meeting` | `id` | `string` (Markdown: título, fecha, resumen, tareas, decisiones, transcripción con hablantes) |

## Eventos

| evento | payload |
|---|---|
| `meeting://state` | `{ state: "idle" \| "recording" \| "summarizing" \| "done" \| "error", meetingId?, message? }` |
| `meeting://segment` | `Segment` (en vivo) |
| `meeting://level` | `{ mic: number, system: number }` 0..1 ~15 Hz |
| `meeting://summary` | `MeetingDetail` (cuando termina el resumen) |

## Tipos

```ts
Meeting = { id, title: string, startedAt: string, endedAt: string|null, durationMs: number,
            app: string|null, summaryStatus: "none"|"pending"|"done"|"failed" }
MeetingListItem = Meeting & { preview: string, actionCount: number }
Segment = { meetingId, speaker: "me"|"them", startMs: number, endMs: number, text: string }
MeetingDetail = Meeting & {
  segments: Segment[],
  summary: string | null,
  actionItems: { text: string, owner: string|null, done: boolean }[],
  decisions: string[],
  topics: string[]
}
Settings += { hotkeyMeeting: Hotkey /* Win+Alt+M */, meetingDetect: boolean /* true */,
              meetingKeepAudio: boolean /* false: no se guarda el audio de reuniones */ }
```

`toggle_action_item(meetingId, index)` marca una tarea como hecha.

## Datos

Tablas nuevas (migración con `user_version`): `meetings`, `meeting_segments`, `meeting_actions`
(`meeting_id`, `idx`, `text`, `owner`, `done`), con FTS5 sobre segmentos y resumen para buscar y
para Ask. Las reuniones no cuentan en las estadísticas de dictado.

## Interfaz (agente UI)

- **Hub › Notas de reunión** (entrada de sidebar «Notetaker» → en ES «Notas de reunión», icono
  disco/libreta, entre Dictado y Estadísticas, como Wispr). Página: cabecera con botón primario
  «Iniciar notas» (o tarjeta en vivo verde con cronómetro, niveles Tú/Ellos y ■ si hay reunión),
  la barra «Pregunta sobre tus reuniones» (Ask), lista de reuniones agrupada por día con título,
  duración, app, vista previa y nº de tareas. Vacío con texto de Wispr adaptado («Pronto toma notas
  de tus reuniones para que puedas estar presente»).
- **Detalle de reunión** (vista dentro del hub): título editable, fecha/duración/app, pestañas
  **Resumen** (resumen, Tareas con casillas, Decisiones, Temas; estado «Generando resumen…» con
  spinner; botón «Reintentar» si falló) y **Transcripción** (burbujas por hablante «Tú»/«Ellos»
  con marca de tiempo, en vivo si se está grabando), acciones Copiar (Markdown de
  `export_meeting`), Preguntar sobre esta reunión, Eliminar.
- **Flow bar**: estado `meeting` y satélite «Tomar notas» (ver arriba); tarjeta de aviso con botón
  «Tomar notas» cuando llega el `notice` con `action: "start_meeting"`.
- **Ajustes › Notas de reunión** (pestaña nueva, como «Notetaker» en Wispr): detección automática,
  atajo, guardar audio de reuniones, modelo de resumen (lee `ollama_status`).
- Mock completo de todo lo anterior en `ui/shared/mock.js` y en el demo de la Flow bar.

## Desviaciones v3 (backend Rust, Notetaker)

Implementación del «Contrato v3» en `src-tauri/`. Módulos nuevos: `meeting.rs` (grabación, cola de
transcripción, resumen, Ask, hilo detector), `segment.rs` (VAD por energía), `summary.rs` (prompts,
parseo del JSON, contexto de Ask, Markdown), `meetingdb.rs` (consultas) y `detect.rs` (registro +
decisión pura). `audio.rs` gana `open_source` y `WavStream`; `ollama.rs`, `generate_raw`.

**Captura**
- Mic y loopback se abren con cpal 0.18 (`build_input_stream` sobre el dispositivo de salida activa
  `AUDCLNT_STREAMFLAGS_LOOPBACK` solo; **verificado**: abre a 48 kHz y entrega muestras). No hizo falta
  `IAudioClient` a mano. Cada fuente vive en su hilo (el `Stream` no es `Send`).
- El loopback no entrega paquetes en silencio: cada fuente sigue el **reloj de pared**; si pasan
  ≥ 200 ms sin datos se rellena con ceros, así los segmentos cierran y los tiempos siguen alineados.
- Segmentación (`segment.rs`): tramos de 20 ms; ruido de fondo adaptativo (calibra los primeros 300 ms y luego
  aprende solo del silencio); umbral de apertura ≈ 3,5× el ruido (mín. 0,004) con 40 ms seguidos; cierre a
  700 ms de silencio; 200 ms de pre-roll; máx. 20 s; descarta < 160 ms de voz. Los cortes por máximo
  son contiguos (sin hueco).
- Una sola inferencia a la vez en `Engine` (mutex). `transcribe` (dictado) tiene prioridad:
  `transcribe_background` cede el turno mientras haya un dictado esperando. La reunión no se detiene
  por un dictado, pero un trozo ya en marcha no se interrumpe (el dictado espera como mucho lo que dure ese trozo, ≤ 20 s de audio).
- `meetingKeepAudio` guarda `audio\meeting_{id}_me.wav` y `_them.wav` (PCM16 16 kHz, escritura en streaming);
  se borran con `delete_meeting`.
- Durante una reunión el dictado **no silencia** el audio del sistema (`muteMusic`), porque el loopback
  quedaría mudo.

**Resumen y datos**
- Migración `user_version` 3: `meetings`, `meeting_segments`, `meeting_actions` y `meeting_fts` (FTS5,
  `unicode61 remove_diacritics`, una fila por trozo `seg` + una `sum` con título/resumen/tareas).
  Probada desde una base v2 con datos. `decisions` y `topics` van como JSON en `meetings`.
- Resumen: `format:"json"`, `num_ctx` 8192, prompt en el idioma detectado (es/en por palabras vacías).
  Más de 14 000 caracteres de transcripción → se resume por trozos de 10 000 y luego en conjunto. Un
  segundo intento si el JSON no se puede leer. Parseo tolerante: texto alrededor, bloques ```json,
  comas finales, claves en español/inglés, respuesta cortada (se cierra a mano); sin nada aprovechable → `failed`.
- El título generado solo sustituye al automático («Reunión dd/mm hh:mm»), nunca a uno puesto a mano.
- `start_meeting`, `stop_meeting`, `meeting_status` y `ask_meetings` son `async` (`spawn_blocking`).
  `stop_meeting` devuelve enseguida con `summaryStatus:"pending"`; el hilo de resumen espera a que se
  vacíe la cola de transcripción. Reuniones que quedaron grabando al cerrar la app se cierran al arrancar
  (`summaryStatus:"failed"`, reintentable). Reunión sin transcripción → resumen `failed` con mensaje.
- `ask_meetings`: con `meetingId` usa esa reunión; sin él, las 4 mejor puntuadas por FTS/bm25 (o las 3
  últimas) con solo las líneas que mencionan palabras de la pregunta; máx. 20 000 caracteres de contexto.
- Las reuniones no cuentan en estadísticas (tablas aparte). `delete_meeting` falla si está en curso.

**Detección y Flow bar**
- `detect.rs` lee `ConsentStore\microphone` (+ `NonPackaged`) cada 3 s (`LastUsedTimeStart != 0` y
  `LastUsedTimeStop == 0` = en uso). Reconoce Zoom, Teams (incl. `ms-teams`/`MSTeams_…`), Webex/CiscoCollab,
  Slack (huddle) y navegadores con título Meet/Zoom/Teams/Jitsi/Whereby (palabra completa: «Meetup» no cuenta).
  Excluye Pronto. Un aviso (`dictation://notice`, `action:"start_meeting"`, `body` = app) por reunión:
  se rearma tras 6 s sin ninguna detectada. Nunca avisa con una reunión grabándose o `meetingDetect` apagado.
- `hotkeyMeeting` (Win+Alt+M) arma `mask_pending` como los demás. Como `Win+Alt` ya habrá empezado un
  dictado «mantener», ese dictado se descarta en silencio al llegar la `M`.
- `dictation://state` emite `meeting` al empezar la reunión si no hay dictado, y vuelve a `meeting`
  (en lugar de `idle`) al terminar cualquier dictado/transform mientras dura.
- `meeting://level` ~15 Hz con `{mic, system}`; `meeting://segment` por cada trozo; `meeting://state`
  (`recording|summarizing|done|error`) y `meeting://summary` (`MeetingDetail`, también cuando falla, con `summaryStatus:"failed"`).

## Desviaciones v3.1 (backend Rust, Notetaker automático)

- **`meetingAuto: "auto" | "ask" | "off"`** (defecto `auto`). `meetingDetect` se conserva en la estructura solo por
  compatibilidad y va siempre sincronizado (`meetingDetect = meetingAuto != off`). Migración al leer ajustes: si la fila
  no tiene `meetingAuto`, `meetingDetect` true→`auto`, false→`off`. Si la UI cambia solo el booleano, `apply_settings`
  lo traduce; si cambia `meetingAuto`, manda este.
- **Máquina pura** en `auto.rs` (`AutoMachine::step(now_ms, mode, recording, apps_en_uso)`, probada con tiempos
  simulados a pasos de 3 s). `detect::detect_all` devuelve todas las apps de reunión en uso; el detector (3 s) solo
  ejecuta la acción. `auto`: empieza sola con título «Reunión de {App} · HH:MM» (provisional: el título del resumen
  lo sustituye), `app` rellenada y notice `Tomando notas de {App}` / «Se pararán solas al colgar.» con
  `action:"stop_meeting"`. `ask`: el aviso de antes (`start_meeting`), una vez por reunión. `off`: nada.
- **Fin automático**: solo para reuniones iniciadas por el detector; se paran con `stop_meeting` cuando su app lleva
  **20 s** sin usar el micro (un corte menor no cuenta). Las manuales nunca se paran solas.
- **Sin re-disparo**: si el usuario para a mano una reunión (automática o manual) con la llamada en marcha, esa app queda
  bloqueada hasta que suelte el micrófono 6 s seguidos. Un arranque que falla también bloquea (no reintenta en bucle).
- **Notas listas**: al terminar el resumen de una reunión automática, notice `{title:"Notas listas", body:<título>,
  action:"open_meeting:{id}"}`. `open_hub(page?, id?)` emite `hub://navigate {page, id}` (`id` solo si se pasa); el menú
  de la Flow bar sigue abriendo `{page:"home"}`.
- **Descarte**: una reunión automática de < 60 s con < 2 trozos se borra tras vaciar la cola, sin resumen ni
  notice; se emite `meeting://state {state:"idle", meetingId, message:"Descartada: reunión demasiado corta"}`.
- **Trozos basura**: se descartan los de texto con < 2 caracteres alfanuméricos, o de una sola palabra de ≤ 2 letras
  que no sea una respuesta corta real (lista: sí, no, ya, ok, ah, oh, eh…; caso real «S.»), y los del micrófono cuya
  RMS de voz no supera 1,5× el umbral adaptativo (`AudioSegment::is_weak`). El loopback no se filtra por energía.
- **`Meeting.auto: bool`** (también en `MeetingListItem`, `MeetingDetail` y `meeting_status`): `true` si la arrancó la
  detección automática, `false` si fue a mano (botón, `Win+Alt+M` o el aviso «¿Tomar notas?»). Columna `meetings.auto`,
  migración `user_version` **4** (las reuniones antiguas quedan en `false`).

## Revisión Opus de la v3 / v3.1 (11-oct-2026)

Probado con la release real: reunión manual y automática (un `zoom.exe` de prueba que abre el
micro dispara la detección por el registro), transcripción en directo de «Ellos» por loopback,
voz real de Txonso por el RODE como «Tú», parada automática 22 s después de colgar, descarte de
una reunión de 12 s, resumen y Ask con `qwen2.5:7b`.

Cambio hecho en la revisión: el prompt del resumen etiquetaba «Tú»/«Ellos» y el modelo escribía
«la realice Tú» y se dejaba las tareas de la otra parte («sin tareas adicionales» con una tarea
dicha). Ahora el modelo recibe `summary::prompt_lines` (el usuario con su `firstName`, el resto
«Otro participante») y una regla explícita de recoger los compromisos de cualquier participante.
Con la misma reunión real salen las dos tareas con su responsable. La exportación a Markdown y la
UI siguen usando «Tú»/«Ellos».

## Desviaciones v4 (backend Rust)

- **Atajo del Transform: `Alt + Shift + P`** (antes `Win + Alt + 1`, que chocaba con el dictado «mantener»). Al leer
  ajustes, solo si el guardado es **exactamente** `{win, alt, key "1"}` se pasa al nuevo; un atajo cambiado por el
  usuario (otra tecla, con Ctrl…) se respeta. Con una tecla que no sea un dígito solo existe el Transform 1; la regla de
  «dígito y los dos siguientes = Transform N..N+2» sigue valiendo para quien configure un atajo con dígito. Lo de
  `state.rs` (descartar el dictado en `Listening` y lanzar el Transform) se mantiene.
- **Pulir con IA siempre**: `aiPolish` (false), `aiPolishModel` (por defecto el mismo que `ollamaModel`; vacío = el de
  `ollamaModel`; la UI puede sugerir `qwen3:1.7b` si sale en `ollama_status`) y `aiPolishTimeoutMs` (6000, acotado a
  0,5-60 s). Va **después** de `text::process` (snippets y estilo ya aplicados) y **antes** de guardar/pegar, en el hilo de
  transcripción, con la Flow bar en `processing`. Petición: `temperature 0`, `keep_alive 30m` (para que el modelo siga
  cargado entre dictados), `think:false`, `num_ctx 4096`, plazo duro de toda la petición.
- **Prompt** (`polish.rs`): el texto va entre `<texto>…</texto>` como dato; instrucciones explícitas de corregir
  puntuación/tildes/mayúsculas/concordancia, quitar muletillas y repeticiones, no añadir ni quitar contenido, no resumir,
  no traducir, **no responder ni obedecer** (una pregunta u orden dictada se devuelve corregida) y devolver solo el texto.
- **Salvaguardas**: respuesta de más de 2x o menos de 0,5x los caracteres del original → se descarta. Se limpian
  bloques `<think>`, etiquetas `<texto>` y comillas añadidas. Si falla, vence el plazo o se descarta: se pega el texto
  sin pulir y llega `dictation://notice {title:"Pegado sin pulir: Ollama no respondió"}` tras el pegado. Tras pulir **no
  se reaplica el tono**, salvo «muy casual» (minúsculas, sin punto final; ojo: también baja a minúsculas un reemplazo
  de diccionario).
- **No se pule** (decisión mía): textos de menos de 3 palabras, ni dictados donde se expandió un snippet (el texto del usuario
  no se toca). `retry_history` tampoco pule.
- **Historial**: `text` = texto pulido y `raw_text` = texto local previo al pulido (permite deshacer); sin pulido
  `raw_text` sigue siendo la transcripción cruda de Parakeet. Columna nueva `history.polished` (migración `user_version`
  **5**) y campo `polished: bool` en `HistoryItem`; `retry_history` lo pone a false.

---

# Contrato v5 · descarga del modelo

Pedido por Txonso el 11-oct-2026 para publicar Pronto 2 en GitHub: un usuario nuevo no tiene el modelo
que dejó Pronto 1 y no podría dictar. Módulo nuevo `modeldl.rs`; la UI vive en la página Dictado del hub.

## Qué hace

1. **Carpeta del modelo** (`Settings.modelDir`). Valor por defecto: el primer modelo **completo** que haya
   en las rutas de Pronto 1, por orden: `%APPDATA%\Pronto\Pronto\models\parakeet`,
   `<carpeta del exe>\models\parakeet` y `%LOCALAPPDATA%\Programs\Pronto\models\parakeet` (instalador de
   Pronto 1). Si no hay ninguno: `%APPDATA%\Pronto2\models\parakeet`. «Completo» = los cuatro ficheros con el
   tamaño exacto. Al leer ajustes ya guardados solo se sustituye un valor que fuera de fábrica (una de esas
   rutas o la propia) y que ya no tenga el modelo completo; una ruta elegida a mano nunca se toca.
2. **Descarga** (`model_download_start`): un solo hilo en segundo plano, sin bloquear la UI, una sola a la
   vez (si ya hay una en marcha no hace nada). Cada fichero de
   `https://huggingface.co/csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/resolve/main/<fichero>`
   (`encoder.int8.onnx` 652 184 281 B, `decoder.int8.onnx` 11 845 275 B, `joiner.int8.onnx` 6 355 277 B,
   `tokens.txt` 93 939 B; 670 478 772 B en total) se baja a `<fichero>.part`, se verifica el SHA-256 (los de
   `parakeetModelSha256` de Pronto 1) y se renombra. Si el hash no coincide o la descarga queda corta se borra
   el `.part` y se emite un error claro. No es reanudable. Los ficheros que ya están enteros se saltan.
3. **Al terminar bien** se llama a `Engine::load` con `modelDir`: el motor queda listo sin reiniciar.
4. **Dictar sin modelo**: si el motor no está listo ni cargando y falta el modelo, el atajo no graba; la Flow
   bar muestra el aviso `dictation://notice` «Falta el modelo de voz» / «Ábrelo en Pronto» con la acción
   `open_hub` (botón «Abrir Pronto»).
5. Con TLS: `ureq` se compila con la característica `tls` (rustls). Ollama sigue usándose solo por `http://`.

## Comandos

| comando | args | devuelve |
|---|---|---|
| `model_download_start` | — | `()` (vuelve enseguida; el avance llega por `model://progress`) |
| `model_status` | — | `ModelStatus` |

## Eventos

| evento | payload |
|---|---|
| `model://progress` | `ModelProgress` (~8 por segundo mientras baja; uno al verificar cada fichero; uno final) |
| `engine://status` | (existente) `loading` → `ready` tras la descarga |

## Tipos

```ts
ModelProgress = {
  file: string,                 // fichero en curso («» en done y error)
  doneBytes: number, totalBytes: number,   // de los CUATRO ficheros juntos
  overallPercent: number,       // 0..100 entero
  state: "downloading" | "verifying" | "done" | "error",
  error?: string                // en español, solo con state "error"
}
ModelStatus = {
  dir: string, complete: boolean, downloading: boolean,
  state: "idle" | "downloading" | "verifying" | "done" | "error",
  doneBytes: number, totalBytes: number, overallPercent: number,   // en disco (incluye el .part en curso)
  error?: string,
  files: { name: string, ok: boolean, bytes: number, size: number }[]   // ok = tamaño esperado; bytes = en disco
}
```

## Interfaz (hub › Dictado)

- Si `!engine.ready && !engine.loading && !modelStatus.complete`, encima del banner aparece la tarjeta
  «Falta el modelo de voz»: botón «Descargar modelo» → barra con porcentaje y «X de Y MB» (verificando:
  «Verificando la descarga…») → en error, el mensaje del backend y «Reintentar». Al llegar `done` la tarjeta
  desaparece y el motor pasa a «Cargando» y «Listo».
- Estado compartido: `state.model` (`api.js`: `refreshModel`, `applyModelProgress`, `startModelDownload`).
- Mock (`ui/shared/mock.js`): `?model=missing` muestra la tarjeta con una descarga simulada de ~12 s;
  `?model=error` hace además que el primer intento falle al 40 %.

## IA local opcional (solo interfaz, sin cambios de contrato)

Ollama + `qwen2.5:7b` es opcional y puede ir más lento según el PC. Aviso «IA local opcional (Ollama)» en
Configuración › Notas de reunión y › Experimental; nota bajo «Pulir cada dictado con IA» al activarlo; aviso
discreto «Sin Ollama las reuniones se transcriben, pero no se resumen…» en la página y en los ajustes de Notas
de reunión cuando `ollama_status` dice `reachable: false` (en el mock, `?ollama=off`).

## Pruebas

Unitarias sin red (`cargo test modeldl`): SHA-256, tabla de ficheros, rutas y precedencia, estado, JSON de los
tipos y la descarga completa contra un servidor HTTP local (verificación, renombrado, salto de lo ya
descargado, hash erróneo, 404 y descarga corta). `#[ignore]` real:
`PRONTO_DL_DIR=<carpeta> PRONTO_WAV="a.wav;b.wav" cargo test descarga_real -- --ignored --nocapture`
descarga de verdad, vuelve a calcular los hashes, carga el modelo y transcribe los WAV.
