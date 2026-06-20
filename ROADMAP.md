# Hoja de ruta de Pronto

Esta hoja de ruta describe cómo pasamos del scaffold actual a una app de dictado
de voz por IA, privada y multiplataforma. El principio rector es la
**arquitectura por capas**: toda la lógica dependiente del sistema operativo
vive detrás de las interfaces de `lib/src/platform/`. Por eso, al portar a un
nuevo sistema, **lo único que cambia de verdad es la capa `platform/`** (y sus
implementaciones); el pipeline de dictado, la automejora, el estado y la UI se
reutilizan.

Leyenda: ✅ hecho · 🔜 en curso / siguiente · ⬜ planificado

---

## Fase 0 — Scaffold actual ✅

El esqueleto que ya existe en el repositorio.

- ✅ Estructura por capas (`core` / `common` / `platform` / `features` / `data`).
- ✅ Arranque de la app y ventana compacta (`main.dart`, `core/app.dart`,
  `core/theme.dart`) con `window_manager`.
- ✅ **Contratos de plataforma** (agnósticos del SO):
  `WhisperEngine` (+ `TranscriptResult`), `AudioCapture`,
  `GlobalHotkeyService` (+ `HotkeyCombo`, `HotkeyEvent`, `TriggerMode`),
  `TextInjector` (+ `InjectionMode`).
- ✅ **Factory por plataforma** en `platform/platform_services.dart`
  (providers Riverpod manuales que eligen la implementación según el SO).
- ✅ **Orquestador del pipeline**: `DictationController` + `DictationState`
  (hotkey → grabar → transcribir → diccionario → LLM opc. → insertar → registrar).
- ✅ **Motor de automejora** completo y testeado: `word_diff` (Needleman-Wunsch),
  `text_similarity` (Levenshtein, Jaro, Jaro-Winkler), `LearningService`
  (extracción de correcciones, diccionario determinista, `initial_prompt`),
  contrato `LearningRepository`.
- ✅ Utilidades de audio (`core/audio_utils.dart`) y configuración central
  (`core/config.dart`).
- ✅ Tests unitarios del motor de aprendizaje (`test/learning/`).

**Se reutiliza en todas las fases siguientes:** `core/`, `features/learning/`,
`features/dictation/` y todos los contratos de `platform/`.

---

## Fase 1 — MVP Windows funcional de punta a punta 🔜

Objetivo: cerrar el pipeline **push-to-talk** real en Windows. Aquí se rellenan
las implementaciones concretas que el factory ya espera.

- 🔜 **`WhisperEngineFfi`** (`features/transcription/`): binding `dart:ffi` a
  whisper.cpp. Cargar modelo GGML una vez, transcribir PCM float32 mono 16 kHz,
  ejecutar la transcripción en un **isolate** para no bloquear la UI, exponer
  `avgLogProb` y soportar `initial_prompt` (biasing). Ver [BUILD_WHISPER.md](BUILD_WHISPER.md).
- 🔜 **`AudioCaptureRecord`** (`features/audio_capture/`): captura con `record`,
  conversión a float32 mono 16 kHz y stream de amplitud para el medidor.
- 🔜 **`WindowsHotkeyService`** (`platform/windows/`): atajo global a nivel de
  sistema mediante *low-level keyboard hook* (`SetWindowsHookEx` `WH_KEYBOARD_LL`)
  en un isolate con bucle de mensajes Win32; emite eventos down/up para
  push-to-talk.
- 🔜 **`WindowsTextInjector`** (`platform/windows/`): inserción en la app con
  foco vía win32 FFI (portapapeles + `Ctrl+V` con guardar/restaurar, y
  `SendInput` con `KEYEVENTF_UNICODE` como alternativa para terminales).
- 🔜 **`SqliteLearningRepository`** (`data/`): persistencia con `sqlite3` del
  diccionario, vocabulario y log de dictados.
- 🔜 **`HomeScreen`** (`features/home/`): estado del dictado, medidor de nivel,
  último dictado **editable** (para alimentar la automejora) y selector de
  modelo/atajo.
- 🔜 Primera carga del modelo (descarga/copia a la carpeta de datos) y mensajes
  claros si falta el modelo o el permiso de micrófono.

**Resultado:** dictado global real en Windows, en español, con diccionario
determinista funcionando.

---

## Fase 2 — Automejora completa ⬜

El motor base ya existe (Fase 0); aquí se completa y se expone.

- ⬜ **Diccionario aprendido** end-to-end: UI para revisar, editar y borrar pares
  de corrección y términos de vocabulario.
- ⬜ **Biasing afinado**: ajuste de cuántos términos y con qué orden van al
  `initial_prompt`; medir impacto en precisión.
- ⬜ **LLM local opcional** (`features/correction/` sobre `http`): post-corrección
  con Ollama local (o nube), **gateada por confianza** (`avgLogProb` <
  `llmConfidenceGate`) y con protección anti-parafraseo (`llmMaxContentDrift`).
  Desactivada por defecto.
- ⬜ Telemetría **local** opt-in (sin salir del equipo) para medir mejora real.

**Se reutiliza:** todo `features/learning/` y `core/config.dart` (ya define los
umbrales). Solo se añade UI y el corrector LLM.

---

## Fase 3 — Pulido y empaquetado (Windows) ⬜

- ⬜ **Instalador con Inno Setup** (`.exe`), empaquetando la app, las DLL de
  whisper.cpp y el modelo por defecto (o descarga en el primer arranque).
- ⬜ **Firma de código con Azure Trusted Signing** para evitar avisos de
  SmartScreen y dar confianza.
- ⬜ **Autostart** con `launch_at_startup` (arrancar minimizado).
- ⬜ **Bandeja del sistema** con `tray_manager`: mostrar/ocultar, pausar el
  atajo, salir; minimizar a bandeja en lugar de cerrar.
- ⬜ Onboarding (permisos, atajo, modelo) y ajustes persistidos.

**Se reutiliza:** toda la app; aquí solo se trabaja empaquetado, ventana y
bandeja.

---

## Fase 4 — Multilenguaje (inglés primero) ⬜

- ⬜ Selección de idioma en UI y por dictado; Whisper ya es multilingüe, basta
  con cambiar el `language` (hoy fijado a `es` en `core/config.dart`).
- ⬜ **Stopwords por idioma**: el `LearningService` hoy usa stopwords en español;
  se generaliza a un set por idioma para que la automejora no degrade en inglés.
- ⬜ Internacionalización de la **UI** (cadenas en inglés además de español).
- ⬜ Diccionario/vocabulario aprendido **por idioma**.

**Se reutiliza:** prácticamente todo. El motor de Whisper y la automejora ya son
agnósticos del idioma salvo las stopwords.

---

## Fase 5 — Multiplataforma ⬜

El objetivo de diseño se cobra aquí: **solo se implementa la capa `platform/`**
de cada SO, detrás de los contratos existentes (`WhisperEngine`, `AudioCapture`,
`GlobalHotkeyService`, `TextInjector`) y se añade la rama correspondiente en el
factory `platform_services.dart`.

### macOS
- ⬜ **Atajo global**: API de *Accessibility* / event taps.
- ⬜ **Inyección de texto**: `CGEvent` (`CGEventCreateKeyboardEvent` /
  `CGEventKeyboardSetUnicodeString`).
- ⬜ Gestionar permisos de *Accessibility* y micrófono.

### Linux
- ⬜ **Atajo global** e **inyección** vía **portal de escritorio**
  (`xdg-desktop-portal`, p. ej. `GlobalShortcuts` y `RemoteDesktop`), con
  conciencia de Wayland vs. X11.

### Android
- ⬜ **IME (teclado de entrada)**: Pronto como método de entrada que inserta el
  texto donde haya foco; el "atajo global" pasa a ser el botón de micrófono del
  teclado.
- ⬜ Reconsiderar el backend de voz (FFI a whisper.cpp en ARM u ONNX) detrás del
  mismo contrato `WhisperEngine`.

### iOS
- ⬜ **Keyboard extension**: misma idea que Android, dentro del modelo de
  extensiones de iOS y sus límites de memoria.

**Se reutiliza en todas:** `core/`, `features/learning/`, `features/dictation/`,
la persistencia `data/` (con su ruta por plataforma) y los contratos. **Solo
cambia `platform/` y sus implementaciones nativas.**

---

## Riesgos clave y mitigaciones

| Riesgo | Por qué importa | Mitigación |
|---|---|---|
| **UIPI / apps elevadas (Windows)** | Una app no elevada no puede inyectar entrada en ventanas de procesos elevados (administrador), así que el dictado "no escribe" ahí. | Documentarlo; ofrecer ejecutar Pronto elevado si el usuario lo necesita; valorar `ChangeWindowMessageFilter`/`uiAccess` y degradar con un mensaje claro cuando el destino está elevado. |
| **Foco de ventana** | El texto debe ir a la app que tenía el foco *antes* de disparar el atajo, no a Pronto; un cambio de foco lo manda al sitio equivocado. | Inyectar sin robar foco (la ventana de Pronto no se activa al dictar); con el atajo global el foco del usuario no cambia. En portapapeles, guardar/restaurar foco y contenido. |
| **Distribución de DLL nativas** | whisper.cpp se distribuye como DLL/`.so`/`.dylib`; faltarlas o mezclar arquitecturas (x64/ARM64) rompe el arranque. Por eso `*.dll`, `*.bin`, etc. están en `.gitignore`. | Empaquetar las DLL correctas en el instalador (Fase 3) y resolver la ruta en runtime; instrucciones reproducibles de compilación en [BUILD_WHISPER.md](BUILD_WHISPER.md); modelo descargado/copiado a la carpeta de datos. |
| **Parafraseo del LLM** | Un LLM de post-corrección puede "reescribir" y cambiar el significado, no solo corregir erratas (alucinación). | LLM **off por defecto**; *gate* por confianza (`llmConfidenceGate`): solo actúa cuando Whisper está poco seguro; descartar la salida si cambia más palabras-contenido de lo permitido (`llmMaxContentDrift`); el **diccionario determinista** sigue siendo la corrección principal (cero alucinación). |
| **Falsos positivos en la automejora** | Aprender pares erróneos degradaría las transcripciones futuras. | Filtros del `LearningService`: frecuencia mínima (`minCorrectionFreq`), descartar reescrituras (`maxRewriteRatio`), exigir similitud de palabra (`maxCorrectionDistance` / Jaro-Winkler) y excluir stopwords y números. |
| **Permisos de micrófono / latencia** | Sin permiso no hay audio; demasiada latencia arruina la experiencia. | Comprobar permiso antes de grabar y avisar con mensaje claro; transcribir en isolate; elegir modelo según CPU (`small` por defecto, `large-v3-turbo` para calidad). |
