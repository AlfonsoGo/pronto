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
  ollamaModel: string,               // qwen3:1.7b (antes qwen2.5:7b; ver Contrato v6)
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

Cada dictado guarda su WAV 16 kHz mono en `%APPDATA%\Pronto2\audio\{id}-{fecha}.wav`; se purga a los
`keepAudioDays`. Sirve para «Reintentar transcripción» y «Reproducir» [spec §2.3].
`{fecha}` son las cifras de `created_at` (reuniones: de `started_at`): una base nueva que reutilice los id no
pisa el audio de otra, y nunca se sobrescribe un WAV (si existe, se aparta a `audio\apartado-<fecha-hora>\`).
Los de antes de la 2.2.x (`{id}.wav`) se leen solo si su fecha de modificación cuadra con la del dictado.

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
  texto primero» / transform demasiado largo). `noaudio` salta si el pico del audio es < 0,0005
  (micrófono silenciado) sin llegar a transcribir; **`empty`** («No se ha oído nada») si llegó audio pero
  la transcripción sale vacía.
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
- Audio: `%APPDATA%\Pronto2\audio\{id}-{fecha}.wav` (PCM16 mono 16 kHz). `retry_history` re-transcribe y aplica la
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
- `meetingKeepAudio` guarda `audio\meeting_{id}-{fecha}_me.wav` y `_them.wav` (PCM16 16 kHz, escritura en streaming);
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

Ollama + `qwen3:1.7b` (desde el Contrato v6; antes `qwen2.5:7b`) es opcional y puede ir más lento según el PC. Aviso «IA local opcional (Ollama)» en
Configuración › Notas de reunión y › Experimental; nota bajo «Pulir cada dictado con IA» al activarlo; aviso
discreto «Sin Ollama las reuniones se transcriben, pero no se resumen…» en la página y en los ajustes de Notas
de reunión cuando `ollama_status` dice `reachable: false` (en el mock, `?ollama=off`).

## Pruebas

Unitarias sin red (`cargo test modeldl`): SHA-256, tabla de ficheros, rutas y precedencia, estado, JSON de los
tipos y la descarga completa contra un servidor HTTP local (verificación, renombrado, salto de lo ya
descargado, hash erróneo, 404 y descarga corta). `#[ignore]` real:
`PRONTO_DL_DIR=<carpeta> PRONTO_WAV="a.wav;b.wav" cargo test descarga_real -- --ignored --nocapture`
descarga de verdad, vuelve a calcular los hashes, carga el modelo y transcribe los WAV.


---

# Contrato v6 · IA local

Pedido por Txonso el 11-oct-2026: Ollama (pulido, Transforms, resúmenes y «Pregunta sobre tus reuniones») deja de
pedir `ollama serve` / `ollama pull` a mano. Un botón, **«Activar IA local»**, lo instala todo. Módulo nuevo
`aisetup.rs`; la interfaz está en `ui/hub/ai.js` y se usa desde Configuración, Estilo, Transforms y Notas de reunión.

## Qué hace

1. **Siempre el modelo pequeño `qwen3:1.7b`** (`models::DEFAULT_AI_MODEL`) para `ollamaModel` y `aiPolishModel`.
   Migración en `Db::load_settings`: un valor guardado que sea **exactamente** `qwen2.5:7b` (el antiguo de fábrica,
   `LEGACY_AI_MODEL`) pasa a `qwen3:1.7b`; cualquier otro, elegido a mano, se respeta. Qwen3 «razona»: todas las
   llamadas a Ollama (`ollama::generate_with`: Polish, Transforms, resúmenes y Ask) envían `"think": false` y además
   `ollama::strip_think` quita cualquier `<think>…</think>` de la respuesta (también un `<think>` sin cerrar o un
   `</think>` suelto).
2. **`ai_setup_start`** lanza un único hilo (`pronto-aisetup`), cancelable, que hace solo lo que falte:
   - **Detección**: Ollama «está» si responde en `ollamaUrl`, o existe `%LOCALAPPDATA%\Programs\Ollama\ollama.exe`
     (o `%ProgramFiles%\Ollama`), o hay `ollama.exe` en el `PATH`.
   - **Ollama no instalado** → `download-ollama`: `https://ollama.com/download/OllamaSetup.exe` (redirige a GitHub,
     ≈ 1,5 GB) a `%TEMP%\Pronto-IA-<pid>-<ms>\OllamaSetup.exe` con progreso en bytes → `install-ollama`:
     **verifica la firma Authenticode** (`WinVerifyTrust`, sin interfaz ni comprobación de revocación para no colgarse sin
     red) y que el firmante empiece por «Ollama» (el oficial es «Ollama Inc.»); si no, aborta con
     «No se instaló Ollama: la firma del instalador descargado no es válida (…)» y no lo ejecuta. Después lo ejecuta
     con `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP-` (Inno Setup; `PrivilegesRequired=lowest`, instala en
     `%LOCALAPPDATA%\Programs\Ollama`, **sin administrador**), espera a que termine (máx. 20 min, código de salida 0) y
     borra la carpeta temporal. Un tamaño < 50 MB o una descarga corta (Content-Length) se rechazan.
   - **Ollama instalado pero sin responder** (o recién instalado) → `start-ollama`: lanza `ollama app.exe` (la
     aplicación de bandeja que arranca el servidor, como tras una instalación normal) o, si no existe, `ollama.exe
     serve`, oculto (`CREATE_NO_WINDOW`), y espera hasta 90 s a `GET /api/tags`. Recién instalado el propio instalador
     lo lanza (su `[Run]` no lleva `skipifsilent`), así que se le dan 8 s antes de abrirlo nosotros. Si `ollamaUrl` no
     es local (`localhost`, `127.0.0.1`, `::1`) y no responde, **no** instala ni arranca nada: error claro.
   - **Falta el modelo** (alguno de `ollamaModel` / `aiPolishModel` no está en Ollama y tampoco `qwen3:1.7b`) →
     `pull-model`: `POST {ollamaUrl}/api/pull {"model":"qwen3:1.7b","stream":true}` leyendo el NDJSON; el progreso es la
     suma de `completed`/`total` de todas las capas (cada `digest`).
   - **Al terminar** (`done`): los ajustes de modelo que **no** estén en Ollama pasan a `qwen3:1.7b` (uno elegido a mano
     y descargado se respeta); `aiPolish` («Pulir cada dictado») **no se toca**.
3. **`ai_setup_cancel`** levanta una bandera: corta la descarga y el `pull` (Ollama conserva lo bajado y reanuda), y la
   espera del arranque. Un instalador ya en marcha **no se mata** (a medias dejaría Ollama roto): termina y el proceso
   se detiene justo después (`cancelled`). La interfaz desactiva «Cancelar» durante `install-ollama`.
4. Una sola activación a la vez (si ya hay una, `ai_setup_start` no hace nada).

## Comandos

| comando | args | devuelve |
|---|---|---|
| `ai_setup_start` | — | `()` (vuelve enseguida; el avance llega por `ai://progress`) |
| `ai_setup_cancel` | — | `()` |
| `ai_setup_status` | — | `AiSetupStatus` (asíncrono: consulta a Ollama, hasta 3 s si no responde) |

## Eventos

| evento | payload |
|---|---|
| `ai://progress` | `AiProgress` (~7 por segundo mientras baja; uno por cambio de fase; uno final) |
| `settings://changed` | (existente) al final, si hubo que dejar `qwen3:1.7b` en los ajustes |

## Tipos

```ts
AiProgress = {
  phase: "check" | "download-ollama" | "install-ollama" | "start-ollama" | "pull-model" | "done" | "error" | "cancelled",
  received: number, total: number,   // bytes de la fase (descarga de Ollama o del modelo); 0 = no aplica / no se sabe
  message: string                    // texto en español; en "error", el motivo
}
AiSetupStatus = {
  running: boolean,
  phase: string, received: number, total: number, message: string,   // la en curso, o la última si fue error/cancelled
  ollamaInstalled: boolean, ollamaRunning: boolean,
  model: "qwen3:1.7b", modelReady: boolean,   // los modelos de los ajustes están en Ollama
  ready: boolean                              // Ollama responde y modelReady
}
```

`check` y `cancelled` son añadidos respecto al pedido original (`check` = comprobando antes de saber qué falta).

## Interfaz

- `ui/hub/ai.js`: estado compartido (`ai`), `aiSlot('card' | 'cta')` para las plantillas, `aiMount(root, onKind)` para
  conectar la página (clics, progreso en vivo sin repintar la barra, `onKind` al pasar entre `idle · running · error ·
  ready`) y `aiActivate()`.
- **Aviso previo** (diálogo del hub): instalará Ollama (≈ 1,5 GB) y `qwen3:1.7b` (≈ 1,4 GB), usa CPU/RAM y en equipos
  modestos puede ir más lento, sirve para pulir, corregir y resumir, y el dictado normal no lo necesita. «Instalar» /
  «Ahora no». Si Ollama ya está, el texto solo habla del modelo.
- **Tarjeta** (`card`): Configuración › Experimental y › Notas de reunión (bajo el aviso «IA local opcional»). Estados:
  activar · pasos y barra con «X de Y MB · N %» (indeterminada al instalar/arrancar) · «IA local lista» · error con
  «Reintentar».
- **En línea** (`cta`): dentro de los avisos «Ollama no responde» / «Falta el modelo» de Estilo › Limpieza (tarjeta de
  pulido) y de Transforms, y bajo la nota «Sin Ollama…» de Notas de reunión. Al llegar a `ready` la página vuelve a
  comprobar `ollama_status` y el aviso desaparece.
- Mock (`ui/shared/mock.js`): `?ollama=none` recorre las cinco fases (~25 s), `?ollama=off` arranca y ya,
  `?ollama=nomodel` solo baja el modelo, `?ai=error` hace fallar el primer intento al 40 % de la descarga de Ollama.

## Pruebas

Unitarias sin red (`cargo test aisetup`): nombres de modelo y modelos faltantes, direcciones locales, banderas del
instalador, JSON del evento, suma de capas del `pull`, `pull` y descarga contra un servidor HTTP local (NDJSON,
error, corte, cancelación, 404), firma (fichero sin firmar y binario de Microsoft rechazados; el `ollama.exe`
instalado, si existe, aceptado), espera de la API. `#[ignore]` reales:
`cargo test firma_real -- --ignored --nocapture` (descarga el instalador a `%TEMP%`, valida la firma de «Ollama
Inc.» y comprueba que un byte añadido la invalida; **no lo ejecuta**), `cargo test arranque_real -- --ignored`
(`ollama serve` oculto en otro puerto, `OLLAMA_HOST=127.0.0.1:11999`) y `PRONTO_PULL_MODEL=all-minilm cargo test
pull_real -- --ignored --nocapture` (baja un modelo diminuto al Ollama local y lo borra con `DELETE /api/delete`).

---

# Contrato v7 · actualizaciones

Pedido por Txonso el 11-oct-2026: Pronto 2 busca solo las versiones nuevas en GitHub y se actualiza con un
botón. Módulo nuevo `updater.rs` (+ `update_helper.ps1`, el ayudante, incrustado con `include_str!`); la UI es
`ui/hub/update.js` (tarjeta) y un grupo «Actualizaciones» en Configuración › Sistema. **No se toca
`installer/`**: el instalador ya trae `skipifsilent` en su `[Run]`, `CloseApplications=no` y espera él mismo
hasta ~20 s a que `pronto.exe` quede libre.

## Qué hace

1. **Buscar** (hilo `pronto-updater`): 20 s después de arrancar y luego cada 4 h, solo si `Settings.autoUpdate`
   (por defecto `true`; los ajustes viejos cargan con el valor por defecto). `GET
   https://api.github.com/repos/AlfonsoGo/pronto/releases/latest` con `User-Agent: Pronto/<versión>
   (actualizador; +https://github.com/AlfonsoGo/pronto)`, `Accept: application/vnd.github+json` y
   `X-GitHub-Api-Version`. Sin red o error HTTP: se calla y reintenta a los 30 min; con 403/429 (límite de la
   API, 60/h por IP sin token) a la hora. Nada bloquea el arranque. «Buscar ahora» (`update_check`) sí enseña
   el error al usuario.
2. **Comparar**: el tag (`v2.0.2`) y `CARGO_PKG_VERSION` se leen como semver (`v`/`V` opcional, partes que
   faltan = 0, `+build` ignorado, prerelease menor que la estable). Se ignoran borradores, `prerelease: true`,
   tags con `-rc.1` y releases sin ningún `.exe` (tratadas como «al día»). Se toma el **primer asset `.exe`**.
3. **Avisar**: evento `update://available`, tarjeta en el hub y elemento «Actualizar a X.Y.Z» (arriba) más
   tooltip «Pronto · actualizar a X.Y.Z» en la bandeja. No hay notificación nativa de Windows (haría falta un
   plugin y su permiso; con el hub cerrado el aviso es el menú y el tooltip de la bandeja). Pulsar el
   elemento abre el hub e inicia la instalación.
4. **Instalar** (`update_install`, solo si hay una versión pendiente y nadie dicta ni hay una reunión
   grabándose): hilo `pronto-update`:
   - descarga a `%TEMP%\pronto-update\<asset>.part` (solo URLs `https` de `github.com`, `*.github.com` o
     `*.githubusercontent.com`), emite `update://progress` (~8/s);
   - **verifica** tamaño == `size` de la API, cabecera `MZ` y SHA-256 == `digest: "sha256:..."` de la API
     (si la API no lo da, solo tamaño y firma `MZ`); si falla, borra el `.part` y sale un error en español;
   - escribe `pronto_update.ps1` y lanza **PowerShell oculto** (`CREATE_NO_WINDOW`, sin parpadeo) con
     `-ProntoPid -Installer -Exe -WorkDir`; espera hasta 10 s a que el ayudante cree `helper-started`; si no
     llega, **no cierra Pronto** y avisa del error;
   - tras 0,6 s llama a `app.exit(0)` (cierre limpio).
5. **El ayudante** (`update_helper.ps1`, log en `%TEMP%\pronto-update\update.log`): espera a que muera el PID de
   Pronto (máx. 45 s, luego lo fuerza) → ejecuta el instalador con `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART
   /NOCANCEL /SP- /LOG=setup.log` y espera → escribe `result.txt` (`ok` / `error <código>`) → **abre
   `pronto.exe` una sola vez** (la ruta del exe que estaba en marcha; si ya hay un `pronto.exe` en esa ruta, no
   abre otro; hasta 8 intentos hasta verlo vivo). Se reabre también si el instalador falló (la versión de
   siempre sigue en su sitio), para no dejar al usuario sin Pronto. Se reabre sin `--hidden`: tras actualizar
   se ve el hub.
6. **Sin doble apertura**: el `[Run] ... postinstall skipifsilent` del `.iss` no abre la app en modo silencioso,
   así que la única reapertura es la del ayudante. Con Pronto 1 → 2 pasa igual: el ayudante de Pronto 1 reabre
   su `pronto.exe` y el instalador no lo hace.
7. **Si falla la instalación** silenciosa: el siguiente arranque (el de la versión vieja que reabrió el
   ayudante) lee y borra `result.txt`; si dice `error N`, el estado lleva `error` («La última actualización no se
   pudo instalar (código N)… Sigues con tu versión de siempre…») y la tarjeta enseña el aviso rojo con
   «Reintentar». Diagnóstico: `%TEMP%\pronto-update\update.log` y `setup.log` (el de Inno).
8. Limpieza: al arrancar se borran los `.exe`/`.part` de `%TEMP%\pronto-update` (el log y el resultado se
   conservan).
9. **Compilaciones de desarrollo** (`...\target\debug|release\pronto.exe`): buscan y avisan, pero no se
   autoinstalan (instalarían la versión de GitHub en la carpeta del instalador y reabrirían un exe que no se ha
   tocado). Se reconocen por la ruta o por `deps\` y `.fingerprint\` al lado (vale con otro `target-dir`).
   Una compilación de depuración no se autoinstala nunca; en una de `release`, `PRONTO_UPDATE_ALLOW_DEV=1`
   lo permite (solo pruebas). Justo antes de lanzar el ayudante y salir se vuelve a comprobar que no se
   esté dictando ni grabando una reunión.

## Comandos

| comando | args | devuelve |
|---|---|---|
| `update_status` | — | `UpdateStatus` (sin red) |
| `update_check` | — | `UpdateStatus`; `Err(String)` si no hay conexión / GitHub limita / respuesta ilegible |
| `update_install` | — | `()` (vuelve enseguida; avance por `update://progress`). `Err` en los rechazos previos (dictando, reunión en curso, sin versión pendiente, compilación de desarrollo); el mismo texto sale por `update://progress` con `state: "error"` |

## Eventos

| evento | payload |
|---|---|
| `update://available` | `UpdateInfo` (cada consulta que encuentra una versión nueva; la UI lo deduplica por versión) |
| `update://progress` | `UpdateProgress` (~8/s al descargar; uno al verificar; `installing` justo antes de cerrar; `error`) |

## Tipos

```ts
Settings.autoUpdate: boolean            // «Buscar actualizaciones automáticamente», por defecto true
UpdateInfo = {
  version: string,                      // «2.0.2»
  tag: string,                          // «v2.0.2»
  title: string,                        // nombre de la release (o «Pronto 2.0.2»)
  notes: string,                        // cuerpo markdown -> texto plano (sin HTML ni enlaces, «•» en las listas, sin
                                        // la línea de descarga ni el título «Pronto X.Y.Z»), ≤ 600 caracteres y 12
                                        // líneas, con «…» si se recortó. La UI lo escapa igualmente.
  pageUrl: string, publishedAt: string | null,
  assetName: string, size: number       // el instalador; su URL y su hash no salen del backend
}
UpdateStatus = {
  currentVersion: string,               // versión efectiva de este ejecutable
  state: "idle" | "upToDate" | "available" | "downloading" | "verifying" | "installing" | "error",
  checking: boolean, checkedAt: string | null,   // ISO de la última consulta que funcionó
  info: UpdateInfo | null,              // la versión pendiente (con state available/downloading/…/error)
  doneBytes: number, totalBytes: number, percent: number,
  error: string | null                  // fallo de descarga/verificación/instalación o de la última instalación
}
UpdateProgress = {
  state: "downloading" | "verifying" | "installing" | "error",
  doneBytes: number, totalBytes: number, percent: number, error?: string
}
```

`state: "error"` solo significa «la instalación de `info` falló»; un fallo de una consulta no lo cambia. «Más
tarde» no existe en el backend: la UI recuerda `dismissed = <versión>` en memoria (`state.update.dismissed`) y no
vuelve a mostrar la tarjeta de esa versión hasta el próximo arranque (o hasta pulsar «Buscar ahora»); el
elemento de la bandeja y el botón de Configuración siguen disponibles.

## Interfaz

- **Tarjeta** (`ui/hub/update.js`, hueco `#upd-slot` en `<main>`, encima de la página: visible en todas las
  páginas; estilo de «Falta el modelo de voz», `.upd` en `hub.css`): «Pronto X.Y.Z disponible» · «Tienes la
  A.B.C. Se instala sola y Pronto se vuelve a abrir.» · notas (3 líneas, «Ver novedades» las despliega) ·
  «Actualizar ahora» / «Más tarde». Descargando: barra con «N %» y «X de Y MB»; verificando: «Verificando la
  descarga…»; instalando: «Instalando… Pronto se cierra un momento y se abre solo.» (sin botones). Con error:
  aviso rojo y «Reintentar».
- **Configuración › Sistema › Actualizaciones**: interruptor «Buscar actualizaciones automáticamente», fila
  «Versión instalada: X» con el estado («Estás al día · comprobado a las 18:04», «Hay una versión nueva: X»…),
  «Actualizar ahora» (si la hay) y «Buscar ahora». El pie del modal usa la versión real del ejecutable.
- Estado compartido: `state.update` (`api.js`: `refreshUpdate`, `checkUpdate`, `installUpdate`,
  `dismissUpdate`, `applyUpdateAvailable`, `applyUpdateProgress`).
- Mock (`ui/shared/mock.js`): `?update=available` (tarjeta ya visible; «Actualizar ahora» simula descarga,
  verificación e instalación), `?update=event` (aparece a los 3 s), `?update=error` (la primera instalación falla
  al 40 % con un error de SHA-256), `?update=offline` («Buscar ahora» falla), sin parámetro o `?update=none`
  («Buscar ahora» responde «Estás al día»). Las notas de ejemplo llevan `<b>` y `<script>` para comprobar el
  escapado.

## Variables de entorno (solo pruebas, sin efecto si no se definen)

| variable | efecto |
|---|---|
| `PRONTO_FAKE_VERSION=2.0.0` | este ejecutable se comporta como esa versión (para ver el aviso contra la release real) |
| `PRONTO_UPDATE_API=http://127.0.0.1:PUERTO/latest` | otra URL para `releases/latest`; con ella también se aceptan descargas `http://127.0.0.1` |
| `PRONTO_UPDATE_ALLOW_DEV=1` | permite autoinstalar desde `target\release` (en depuración, nunca) |

## Pruebas

Unitarias sin red (`cargo test updater`): lectura y comparación de versiones (semver, prereleases, `2.0.10 >
2.0.9`), JSON de la release real de v2.0.1 (primer `.exe`, digest, tamaño), borradores/prereleases/sin `.exe`,
JSON roto, nombres de asset peligrosos, URLs permitidas, notas en texto plano y recorte, `digest` (mayúsculas,
algoritmos ajenos, longitud), verificación de tamaño/`MZ`/SHA-256, descarga y consulta contra un servidor HTTP
local (verifica, borra el `.part` si el hash o el tamaño fallan, 404, sin conexión, User-Agent propio), JSON de los
tipos, `result.txt` del ayudante y un solo punto de reapertura en el script. `#[ignore]` reales:
`cargo test ultima_release_real -- --ignored --nocapture` (consulta a GitHub, solo lectura),
`PRONTO_DL_DIR=<carpeta> cargo test instalador_real_de_github -- --ignored --nocapture` (baja y verifica el
`.exe` de la última release) y `ayudante_real` (ejecuta el ayudante con un instalador de prueba:
`PRONTO_E2E_INSTALLER` = un `ProntoTest-Setup-*.exe` compilado con `/DTestSandbox=1 /DTestDir=<carpeta>`,
`PRONTO_E2E_EXE` = el `pronto.exe` de esa carpeta (`<TestDir>\default-dir`), `PRONTO_E2E_DIR` = carpeta de
trabajo, y opcionalmente `PRONTO_E2E_PID` = proceso al que esperar).

Extremo a extremo (hecho a mano el 11-oct-2026, sin tocar la instalación real): una copia de `pronto.exe` en la
carpeta de una instalación de pruebas (`/DTestSandbox=1`), con `PRONTO_UPDATE_API` apuntando a un servidor local
que imita `releases/latest` (tag `v9.9.9`, digest y tamaño reales del instalador de pruebas),
`WEBVIEW2_USER_DATA_FOLDER` propio y `--remote-debugging-port` para llamar a `update_check` y `update_install`
desde la webview del hub. Resultado: aviso en la tarjeta, descarga verificada, cierre limpio, instalación silenciosa
sobre la carpeta de pruebas, y un único `pronto.exe` reabierto.

# Contrato v8 · barra fija en un monitor

Pedido por Txonso el 13-oct-2026: fijar la Flow bar en un monitor (candado en la propia barra) para que no
moleste al compartir pantalla o al trabajar en el sitio donde aparece.

## Ajustes nuevos (`Settings`, camelCase, opcionales; los ajustes viejos cargan con `null`)
- `flowBarMonitor: string | null`: nombre de dispositivo Win32 (`"\\.\DISPLAY2"`). `null` = sigue al cursor.
  Si ese monitor no está conectado, la barra sigue al cursor sin borrar el ajuste y vuelve sola (~2 s) al
  reconectarlo.
- `flowBarMonitorRect: { left, top, right, bottom } | null`: rectángulo físico del monitor al fijarlo, para
  reencontrarlo si Windows renumera. Lo pone el backend (`save_settings` lo recalcula al cambiar `flowBarMonitor`).
- Orden para reencontrarlo (`flowbar::resolve_pin`): nombre + rectángulo, solo rectángulo, solo nombre.

## Comandos
- `pin_flowbar({ pinned: bool }) -> Settings`: `true` fija la barra en el monitor donde está; `false` la suelta.
  Guarda, recoloca y emite `settings://changed`.
- `list_monitors() -> MonitorInfo[]` `{ device, number, name, primary, width, height, current }` (nombre comercial
  del EDID, px físicos; `current` = la barra está ahí).

## Colocación
`flowbar::place` → monitor fijado y conectado; si no, el del cursor al empezar a dictar. El sondeo de 30 Hz
recoloca la barra fijada cada ~2 s si se ha movido (reconexión, resolución, escala), con `SWP_NOACTIVATE`.
UI: satélite candado de 24 px junto a Polish (aparece con el hover, violeta si está cerrado) y fila «Monitor de
la barra» en Ajustes → Sistema, sincronizadas por `settings://changed`.

# Contrato v9 · diccionario por voz

Pedido por Txonso el 13-oct-2026: «poner la palabra, grabar cómo se pronuncia y que lo grabe el sistema».
Parakeet (greedy_search) no admite vocabulario: el diccionario aprende qué escribe Pronto al oír la palabra y lo
corrige. Módulo `pronounce.rs`; tabla `word_variants` (migración v6).

## Grabar (`pronounce_record { word, replacement? } -> Pronunciation`)
Micrófono de `settings.micName`, mono 16 kHz. Para sola tras 900 ms de silencio después de ≥200 ms de voz SEGUIDA (umbral =
ruido de los primeros 300 ms + 0,15, mínimo 0,22 de `rms_to_level`) o a los 5 s; los ruidos sueltos (clic,
respiración) ya no la paran antes de hablar (15-oct-2026). Antes de transcribir sube el volumen de una grabación
floja hasta pico 0,5 (×20 como mucho); `pronounce_stop { cancel? }` la
para o la descarta. Una a la vez; se rechaza si el motor no está listo o hay un dictado o una reunión en curso; si
empieza un dictado, se descarta. Transcribe con `Engine::transcribe`. No guarda nada.
`Pronunciation { heard, variant, status: "new"|"ok"|"covered", common, durationMs }`: `ok` = ya la entiende,
`covered` = ya la corrige un reemplazo o el ajuste de mayúsculas/tildes, `new` = se guarda como variante.

## Variantes y tubería
- `word_variants(id, word_id → words.id, heard, created_at)` + índice. `foreign_keys` no está activo:
  `delete_word` borra sus variantes en la misma transacción. `Word.variants: WordVariant[] { id, heard }`.
- Comandos: `add_word`/`update_word` aceptan `variants?: string[]` (en `update_word` deja exactamente esa lista);
  `add_word_variant { wordId, heard }`; `delete_word_variant { id }`.
- Eventos: `pronounce://level { level }` (~30/s) y `pronounce://state { state: recording|transcribing|idle }`.
- `text::dictionary_inner` (dictado y reuniones), de más larga a más corta, palabra completa, sin mayúsculas:
  par a mano `word → replacement`; variante → `replacement` o `word`; palabra suelta → ajusta mayúsculas y tildes
  (pone tildes, no las quita; nunca palabras corrientes); pasada final «suena igual» con clave fonética española
  (claves ≥4 letras, nunca palabras corrientes ni claves ambiguas). Solo cuentan como arreglo si cambian el texto.

# Contrato v10 · reuniones en dos fases

Pedido por Txonso el 13-oct-2026: grabar el audio en dos hilos en paralelo y después transcribir y entender la
reunión con Ollama. Módulo `notes.rs` (fase 2 sin `AppHandle`).

## Fase 1 · captura (durante la reunión)
Un hilo por fuente (mic = «Tú», loopback = «Ellos») escribe SIEMPRE el audio a
`%APPDATA%\Pronto2\audio\meeting_{id}-{fecha}_me.wav` y `meeting_{id}-{fecha}_them.wav` (PCM16 mono 16 kHz), alineados al inicio
(huecos rellenos con silencio, `GAP_FILL_SAMPLES`). La cabecera se reescribe cada 5 s y al cerrar;
`audio::WavReader` lee hasta el final sin fiarse de ella. La transcripción en vivo (`segment::LIVE`, 700 ms /
20 s) es solo vista previa (`meeting://segment`) y plan B si el audio falla.

## Fase 2 · comprensión (al parar o con `retry_summary`; hilo `pronto-notes`, una por reunión a la vez)
1. Si `transcript_final = 0` y hay WAV: transcribe cada pista entera (`segment::FINAL`, 1,2 s de silencio, trozos
   ≤30 s cortados en el punto más silencioso), limpieza + diccionario, sin basura. Mezcla cronológica quitando el
   eco (trozo del micro que coincide en tiempo y en ≥50 % de palabras con el sistema). `replace_segments` sustituye
   la vista previa y pone `transcript_final = 1`; si `meetingKeepAudio` es false, los WAV se borran entonces. Si
   falla, quedan la vista previa y el audio para reintentar.
2. Ollama en dos pasadas (`notes::understand`): notas por bloques de ~5.000 caracteres y síntesis final. JSON con
   reintento y salida de emergencia. Se contrasta con la transcripción: fuera ejemplos del prompt, «no hay…»,
   responsables y fechas no dichos y puntos sin apoyo; lo que la síntesis olvida vuelve desde los bloques.
El dictado sigue teniendo prioridad (`transcribe_background`). Una reunión en fase 2 no se puede borrar ni relanzar.

## Eventos y tipos
- `meeting://progress { meetingId, stage: transcribing|understanding|done, percent }`; `meeting://state` y
  `meeting://summary` como antes.
- `MeetingDetail` añade `keyPoints`, `nextSteps`, `openQuestions: string[]` y `transcriptFinal: boolean`;
  `ActionItem` añade `due: string | null`. Base de datos v7: columnas `key_points`, `next_steps`, `open_questions`,
  `transcript_final` en `meetings` y `due` en `meeting_actions`.

## Coste
32 KB/s por pista (~230 MB por hora con las dos, borrados al terminar si no se guarda el audio). Fase 2 ≈ 16–18×
tiempo real por pista (~7 min para 1 h de reunión) + ~3 s por bloque con qwen3:1.7b.
Prueba real: `PRONTO_MEETING_DIR=<carpeta con me.wav y them.wav> cargo test fase_2_real -- --ignored --nocapture`.

# Contrato v11 · huella de voz

Pedido por Txonso el 14-oct-2026: «que tú puedas decir la palabra y lo que significa, para que si detecta lo
mismo la próxima vez, el mismo audio, sepa qué significa». El diccionario reconoce la palabra por cómo SUENA su
voz, no por el texto que escribe Parakeet. Módulo `voiceprint.rs`; tabla `word_samples` (migración v8).

## Grabar
`pronounce_record` (v9) guarda además la grabación recortada a la voz como muestra pendiente
(`%APPDATA%\Pronto2\voz\pendiente-{ms}.wav`, 16 kHz mono; recorte al bloque con más energía, 0,2-2,5 s).
`Pronunciation` añade `sample: string | null` y `sampleMs`. Las pendientes sin palabra se borran a las 24 h.

## Datos y comandos
- `word_samples(id, word_id → words.id, file, duration_ms, created_at)`; al enganchar, el fichero pasa a
  `w{wordId}-{ms}.wav`. `delete_word` borra filas y ficheros. `Word.samples: { id, durationMs, createdAt }[]`.
- `add_word { …, newSamples? }`, `update_word { …, keepSamples?, newSamples? }`, `add_word_sample { wordId,
  sample }`, `delete_word_sample { id }`, `get_word_sample_audio { id? | pending? } -> WAV base64 | null`.

## Tubería
Dictado y `retry_history`: `Engine::transcribe_timed` → `voiceprint::correct` → `text::process` (el historial
guarda en `rawText` lo que oyó Parakeet). Reuniones: fase 2, cada trozo. Rasgos MFCC c1-c13 de 40 bandas log-mel
(25/10 ms, FFT propia) + deltas, CMN con la media del dictado. DTW de subsecuencia simétrica (Sakoe-Chiba P=1)
sobre el dictado entero. Se cambia un tramo solo si: coste < 6,3 (1 muestra) / 6,4 (2+); cubre ≥ 50 % de cada
palabra cambiada; 1-3 palabras, no todas corrientes ni de otra entrada; el texto no es ya la palabra; ninguna otra
entrada casa a menos de 0,35. Conservador a propósito: prefiere no corregir a corregir mal.

## Interfaz
Editor y «Grabar pronunciación»: bloque «Tu voz · n de 3» con ▶ y × por grabación; en la lista, insignia 🎙.

## Medidas (15-oct-2026)
Coste: 50 palabras × 3 muestras sobre 13 s ≈ 35 ms. Voz real de Txonso (muestras recortadas de frases, 174
dictados): precisión 100 %, recall 10-11 %. Voz sintética con muestras sueltas: 21/24 reconocidas, 0 falsos
positivos en 44 frases. Baterías `--ignored`: `bateria_voz_real`, `bateria_voz_sintetica`, `coste_50_palabras`.

# Contrato v12 · Meets

Pedido por Txonso el 16-oct-2026. «Notas de reunión» pasa a llamarse **Meets** en la interfaz (es/en); los
nombres internos no cambian.

## Fase 1 · solo captura
Un hilo por pista (mic = «Tú», loopback = «Ellos») escribe su WAV alineado (`meeting_{id}-{fecha}_{me|them}.wav`). Durante
el Meet NO se transcribe nada: la interfaz enseña tiempo y nivel. Ya no existen `meeting://segment` ni la vista
previa. Si una pista no se puede grabar (al empezar o a mitad), aviso y se sigue con la otra.

## Fase 2 (al parar o con `retry_summary`) · `meeting://progress { meetingId, stage, percent, step, steps: 4 }`
1. `transcribing`: cada pista con `segment::FINAL` + `transcribe_background_timed` (huella de voz por rangos) →
   palabras con inicio y fin. Eco palabra a palabra (±300 ms y sonido parecido a una del sistema; rachas de hasta 3
   palabras mal oídas entre eco si la palabra del sistema no tenía ya su eco). Turnos por pausas de 0,7 s.
   Encadenado por tiempo real: un turno con sustancia (≥3 palabras o ≥1 s) que entra a mitad parte el del otro y el
   resto queda `overlap`. Se juntan turnos seguidos de la misma persona a menos de 1,6 s.
2. `polishing` (una vez por reunión, con Ollama): turnos defectuosos (idioma cambiado, palabra imposible,
   repetición, frase cortada, ritmo, tiempos raros, inicio pisado), como mucho el 25 % (mínimo 8), con el turno
   anterior y el siguiente de contexto. Se acepta solo si: largo ×0,6-1,3, sin cifras ni nombres nuevos, ≤1-2
   palabras nuevas y sin perder más del 40 %. El original queda en `rawText`.
3. `labeling`: reglas + modelo en bloques de 40 (`pregunta | respuesta | propuesta | decision | tarea | otro`,
   `replyTo`).
4. `understanding`: notas por bloques y síntesis que dice quién dijo qué. `questions` y `saidByMe`/`saidByThem`
   salen de las etiquetas, no del modelo. Arreglos deterministas: responsable = quien se comprometió en primera
   persona; compromisos olvidados entran como tareas con plazo; fuera decisiones y tareas sacadas de preguntas.
   Modelo: `notes_model` (qwen3.6 > qwen2.5:7b > …), cargado antes con plazo de 900 s.
Transcripción a 4 hilos y un solo reconocedor (más hilos o un segundo reconocedor no compensaron, medido).

## Tipos y datos (base v9)
`Segment += { rawText, kind, overlap, replyTo }`; `Question = { text, askedBy: "me"|"them", atMs, answered,
answer }`; `MeetingDetail += { questions, saidByMe, saidByThem }`. Columnas `meeting_segments.raw_text, kind,
overlap, reply_to`; `meetings.questions, said_me, said_them, polish_done`. Las reuniones viejas cargan vacías.

## Medidas (16-oct-2026, voces TTS)
0 turnos fuera de orden; 0 de 349 palabras atribuidas al otro en las conversaciones cortas y 2 de 1048 en la de
10,5 min; ninguna palabra propia borrada como eco. Transcripción 7-13 s por minuto de reunión; qwen3.6 ≈ 25 s por
minuto para pulir, etiquetar y resumir, más ~45 s de carga (≈35 min para una hora, en segundo plano).
Batería: `PRONTO_MEETS_DIR=… cargo test --release bateria_meets -- --ignored --nocapture`.

# Contrato v12.1 · modelo de Meets

Pedido por Txonso el 17-oct-2026 (las notas tardaban demasiado). Solo cambia qué modelo de Ollama hace pulido,
etiquetas y notas. `ollama::notes_model`: si el usuario eligió a mano un modelo distinto del de fábrica, ese; si
no, el primero instalado de `NOTES_MODELS`: `gemma4:12b-it-qat` > `qwen3.6` > `qwen2.5:7b` > `qwen3:8b` >
`qwen3:4b` > `Qwen3-4B-GGUF:Q4_K_M`. Un solo modelo para todo (repartir no compensa, medido).

Medido en RTX 4070 Ti de 12 GB: gemma4:12b-it-qat (7,2 GB, 100 % GPU) iguala en calidad a qwen3.6 y tarda
11,8-12,3 s por minuto de reunión frente a 14,4-15,0 (13,8 frente a 21,7 con la CPU ocupada); qwen3.6 (23 GB) corre
un 58 % en CPU. Descartados gemma4:26b-a4b (pierde decisiones, duplica tareas) y qwen2.5:7b (el doble de rápido
pero se deja tareas y preguntas). Todos generan ~40 tok/s: manda el número de tokens escritos.
Batería: `PRONTO_MEETS_DIR=… [PRONTO_MEETS_ONLY=b] [PRONTO_OLLAMA_MODEL=…] [PRONTO_OLLAMA_FAST_MODEL=…] cargo
test --release bateria_meets -- --ignored --nocapture`.

# Contrato v12.2 · etiquetas compactas

Pedido por Txonso el 17-oct-2026. Solo cambian la fase `labeling` y tres arreglos de `understanding`; ni tipos ni base.
- Formato (`notes/turns.rs`): una línea por turno `n letra [a]` (p pregunta, r respuesta + nº de la pregunta, pr
  propuesta, d decisión, t tarea, o otro): ~7 tokens por turno frente a ~25 del JSON (-83 %). Sin `format: json`,
  `num_predict` 1024 (`meeting::LABEL_MAX_TOKENS`). `parse_compact` tolerante; respaldo al JSON de v12 y a las reglas.
- Solape: bloques de 40 turnos con `LABEL_CONTEXT` = 6 turnos antes y después marcados «no lo etiquetes»; cada turno se
  etiqueta una vez y una respuesta puede apuntar a una pregunta del bloque anterior.
- `fix_speakers`: un compromiso se da por listado solo si comparte la ACCIÓN (`same_action`, raíz del verbo), no un
  nombre común; «Se lo pregunto…» ya no se confunde con «Enviar el presupuesto». `questions_with_status` + `evasive`:
  una pregunta con respuesta real no vuelve a quedar «sin resolver».
- Medidas (gemma4:12b-it-qat, conversación de 10,5 min): etiquetas 62-65 s → 11-12 s; fase 2 sin carga del modelo
  11,1-11,7 → 7,4-7,7 s por minuto de reunión; tareas con responsable y plazo 6-7/8 → 8/8 y 7/8.

# Contrato v12.3 · comprensión de la conversación entera

Pedido por Txonso el 17-oct-2026 tras dos Meets reales («las funciones van bien, pero hay que intentar que comprenda
mejor la reunión»). Solo cambia la fase `understanding`; ni tipos públicos ni base.
- `summary::conversation_lines` sustituye a `prompt_lines`: fuera los asentimientos («Mm-hmm», «Sí, sí», y lo de ≤ 2
  palabras solapado) y las intervenciones seguidas de la misma persona en una línea (< 10 s sin nadie en medio); un
  trozo corto que acaba en punto lleva «…». Sin etiquetas de tipo en el prompt (en charla informal confundían).
- Pasada única (`summary::full_prompt`) si la conversación unida cabe en `notes::FULL_CHARS` = 16.000 caracteres
  (~20 min; entra con la respuesta en `NUM_CTX` 8192). Si no, las dos pasadas de siempre (`by_blocks`).
- Claves nuevas en las dos rutas: `aportes_usuario`, `aportes_otro` (lo que contó cada parte, redactado; un párrafo
  se parte en frases) e `intercambios` (`{pregunta, pregunto, respuesta|null}`).
- Contraste: un aporte sin nada exclusivo de su parte y con ≥ 2 rasgos exclusivos de la otra cambia de lista; sin
  aportes del modelo, `turns::said_points`. Cada pregunta del modelo se apoya en lo hablado, casa con un turno (de
  ahí minuto y, si casa mejor con la otra parte, quién preguntó) y se descarta si solo repite la pregunta recibida
  (`echoed_question`) o si ni ella ni su turno suenan a pregunta. Respuesta inventada fuera; evasiva = sin resolver.
  Si el modelo da la clave `intercambios` (aunque vacía) manda ella; las etiquetas solo sin ella.
- Temas sin contraste de palabras (son etiquetas). «Yo lo que te iba a decir» ya no cuenta como compromiso.
- Medidas (gemma4:12b-it-qat): Meets reales 16 y 17 (2-3 min) 11-14 s de comprensión; reuniones de trabajo a y b
  20 s. b: 2 → 4 preguntas con respuesta; sin preguntas ni tareas falsas en la charla informal.
  Batería: `PRONTO_NOTES_JSON=f1.json;f2.json [PRONTO_FULL_CHARS=0] [PRONTO_RAW=1] cargo test --release bateria_notas
  -- --ignored --nocapture` (ficheros `{"segments": [...]}`).

# Contrato v13 · Whisper como segundo motor de voz (Pronto 2.2)

Pedido por Txonso el 17-oct-2026 («más lento pero más eficaz… que funcione en casi todos los PCs»).
- Motor: OpenAI Whisper large-v3-turbo q5_0 (`ggml-large-v3-turbo-q5_0.bin`, 574 MB, SHA-256 de Hugging Face) vía
  whisper.cpp (`whisper-rs` 0.16, feature `vulkan`): GPU NVIDIA, AMD o Intel. Módulo `whisper.rs`.
- `asr.rs`: `Recognizer::{Parakeet, Whisper}` y `Choice` (`settings.asrEngine` = `parakeet` | `whisper` + idioma). Con
  Whisper elegido pero sin modelo, sin GPU Vulkan con ≥ 3 GB (`whisper::availability`) o sin AVX2, carga Parakeet:
  nunca se queda sin dictado. `EngineStatus.active` dice qué motor está cargado. Trozos de 28 s (ventana de 30 s).
- Whisper: greedy, temperatura 0, `no_context`, sin segmentos con prob. de silencio > 0,6, calentamiento al cargar
  (Vulkan compila sus shaders ~20-40 s la primera vez), texto previo = frase puntuada de ejemplo + hasta 40 palabras
  del diccionario (`state::vocabulary`, destacadas primero; sin él a veces no puntúa). `drop_hallucinations` quita
  frases de subtítulos («Subtítulos realizados por…») y bucles («qué pasa» ×9).
- Tiempos por token con DTW (cabezas de large-v3-turbo; sin flash attention): sin DTW, 29 de 263 palabras de Meets
  al otro hablante por eco. `turns::remove_echo_tails` quita la frase corta (≤ 3 palabras) con la que empieza un
  turno del micro si repite lo último del otro («… de noviembre.» → «Noviembre. Genial…»): 29 → 0 en la reunión a.
- Ajustes → General → Motor de voz: selector, descarga (`whisper_download_start`, evento `whisper://progress`) y
  qué motor está en uso. Comando `asr_options` (GPU, VRAM, disponible y por qué no, modelo presente).
- Compilación: `src-tauri/.cargo/config.toml` (Vulkan SDK, libclang de `pip install libclang`, `GGML_NATIVE=OFF` con
  AVX2, runtime estático `/MT` como sherpa-onnx, carpeta `target` corta `%USERPROFILE%\dev\pt` por el límite de
  rutas). `build.rs`: `vulkan-1.dll` con carga diferida; `whisper::gpu` la busca antes de tocar Vulkan.
- Medidas (RTX 4070 Ti): dictado 2 s → 0,15 s, 8 s → 0,4 s, 30 s → 1,2 s (Parakeet CPU: 0,4 / 0,7 / 3,4 s). Meets:
  5,5 s por minuto por pista (Parakeet 9,4-10,5). WER (pistas TTS, faster-whisper): turbo 4,4 % frente a 7,0 %.
  En CPU no sirve (minutos por dictado). Detalle en `docs/asr-comparativa.md`.
  Pruebas: `PRONTO_WAVS=a.wav;b.wav cargo test --release whisper_real -- --ignored --nocapture` y
  `PRONTO_ASR=whisper PRONTO_MEETS_DIR=… cargo test --release bateria_meets -- --ignored --nocapture`.
- v13.1 (17-oct, noche): pestaña **Configuración → Motor** con Parakeet y cinco Whisper (`whisper::MODELS`:
  large-v3-turbo 574 MB/3 GB VRAM, large-v3 1,1 GB/4 GB, medium 539 MB/2 GB, small 190 MB/1 GB, base 60 MB/1 GB),
  cada uno con tamaño, CPU/GPU, errores medidos, velocidad y «Usar» / «Descargar» / «No cabe en este PC».
  Ajuste nuevo `whisperModel`; `asr_options` devuelve la lista; `whisper_download_start(id)`, una descarga a la vez.
  DTW con las cabezas de cada modelo. Al cambiar de motor el anterior sigue dictando hasta que el nuevo está listo.
  `aisetup::ensure_running`: si Ollama está instalado pero parado, las notas de Meets lo arrancan (hasta 30 s).

# Contrato v14 · Dictado rápido con Whisper (24-oct-2026, sin publicar)

Pedido de Txonso: más velocidad sin perder calidad. Medidas en `specs/velocidad/2026-10-23.md`.
- `whisper::Whisper` tiene dos contextos del mismo modelo: `fast` (atención flash, sin DTW) para el dictado y
  `precise` (DTW) para `Engine::transcribe_background_timed` (fase 2 de Meets) y `Engine::transcribe_timed_precise`.
  `precise` se precarga en el hilo de carga, después de dar el motor por listo (`on_ok`: ya se puede dictar), si
  caben los dos (`room_for_two`: VRAM ≥ `pair_vram_gb` × 0,95, lo medido por modelo + 1,5 GB; turbo 3,8 GB,
  `specs/velocidad/2026-10-24.md`); si no, la primera vez que se pide (antes, `verify()` del `.bin`). Si no se puede
  cargar, se usan los tiempos aproximados de `fast` (regla 4). El estado del latido sigue la misma cuenta.
- `state::transcribe_processed_as`: si `Engine::rough_timings()` y `voiceprint::hears` oye una palabra con
  muestras (solo mira el audio, antes de transcribir), el dictado va directo por `transcribe_timed_precise` (la
  huella necesita los tiempos de DTW): una sola pasada. En «Reintentar transcripción», por
  `transcribe_timed_polite_precise` (`Kind::RetryPrecise`: cede a los dictados entre trozos). Si esa pasada falla
  o sale vacía, se transcribe por el camino rápido y no se corrige con la huella (regla 5).
- `Engine::keep_warm` → `Warmth`: latido de la GPU (`Whisper::pulse`, estado aparte) cada 200 ms mientras se
  graba, con tope de 2 min; `Controller::finish_recording` lo suelta antes de transcribir.
- `src-tauri/vendor/whisper-rs-sys` (con `[patch.crates-io]`): whisper.cpp no repite el encoder tras la detección
  de idioma con «auto» (`PRONTO.md`). Carpeta de compilación corta obligatoria (MAX_PATH): con
  `CARGO_TARGET_DIR=C:/Users/usuario/dev/pt-v` falla; `pt` o `v` sí.
  `whisper::tests::vendor_whisper_sin_cambios_ajenos` fija la huella SHA-256 del árbol vendor (sin `PRONTO.md` ni
  `.cargo-ok`, saltos de línea normalizados) y que Cargo.lock lo tome de la ruta: con `[patch]` por ruta Cargo no
  guarda checksum. Al actualizar el vendor a propósito, se cambia la constante.

# Contrato v14.1 · Whisper solo en español o inglés (24-oct-2026, sin publicar)

Pedido de Txonso: con medium y el idioma automático le salía en portugués; quiere que Whisper (todos los tamaños)
elija solo entre español e inglés, con español cuando dude. Medidas en `specs/velocidad/2026-10-24.md` §3.
- Ajuste `language` sin cambios (`auto` | `es` | `en`). `whisper::whisper_language`: `auto` (o vacío, o cualquier
  valor raro) se pasa a whisper.cpp como la lista `es,en`; un código de dos letras, tal cual; `libre` (solo bancos)
  = la detección de Whisper entre sus 99 idiomas. Parakeet no cambia.
- Segundo parche `[Pronto]` en `whisper_full_with_state`: si el idioma lleva comas, se detecta como con `auto`
  (mismo encoder reutilizado, sin pasada extra) y se elige el más probable de la lista; uno que no sea el primero
  solo gana con probabilidad ≥ 0,85 (inglés real: 0,89-0,99; un «Sí» o español con acento: 0,72-0,82).
- `vendor/whisper-rs-sys/build.rs`: el crate copiaba whisper.cpp a `OUT_DIR` una sola vez y solo vigilaba
  `wrapper.h`, así que un cambio en el parche no llegaba al binario. Ahora vigila `whisper.cpp/{src,include,ggml}` y
  copia los ficheros que difieran (`sync_changed`).
- Pruebas: `whisper::tests::automatico_es_espanol_o_ingles` y el banco `banco_idioma`
  (`PRONTO_WAVS=…; PRONTO_IDIOMA_ESPERADO=es|en cargo test --release banco_idioma -- --ignored --nocapture --test-threads=1`).

