# Arquitectura de Pronto

> Dictado de voz por IA, on-device, español primero, dictado **global** en cualquier
> aplicación de Windows (escritorio primero), con un sistema de **automejora** que
> aprende de tus correcciones.
>
> Documento técnico que refleja el código real de `lib/`. Última revisión: 2026-06.

---

## 1. Visión general

Pronto es una app Flutter de escritorio (Windows en el MVP). El usuario mantiene
pulsado un atajo **global** (push-to-talk), habla, y al soltar el texto transcrito
se inserta en la aplicación que tenga el foco (editor, navegador, chat...). Todo el
reconocimiento ocurre **en local** con whisper.cpp vía FFI; no se envía audio a la
nube.

La pieza diferencial es el **lazo de automejora**: cuando el usuario corrige a mano
una transcripción, Pronto aprende un mapa determinista de reemplazos y un
vocabulario propio que sesga (biasing) las siguientes transcripciones mediante el
`initial_prompt` de Whisper.

Principios de diseño que vertebran el código:

1. **On-device por defecto.** Privacidad y latencia. El LLM de post-corrección es
   opcional y desactivado por defecto.
2. **Determinismo antes que IA generativa.** El diccionario aprendido (sin
   alucinación) se aplica SIEMPRE; el LLM solo entra, opcionalmente, cuando la
   confianza de Whisper es baja.
3. **Abstracción por plataforma.** Toda capacidad nativa (audio, atajo global,
   inyección de texto, motor de voz) vive detrás de una interface pura en
   `lib/src/platform`; el resto de la app no conoce Win32.
4. **Sin codegen.** Riverpod con providers manuales, SQLite a mano, modelos
   inmutables con `copyWith` escritos a mano.

---

## 2. Pipeline de dictado (tiempo real)

```
                    ┌──────────────────────────────────────────────────────────┐
                    │  GlobalHotkeyService  (low-level keyboard hook, Win32)     │
                    │  Ctrl+Alt+Espacio  →  HotkeyEvent(down) / HotkeyEvent(up)  │
                    └───────────────┬──────────────────────────┬───────────────┘
                          down ▼                       up ▼
                  ┌───────────────────────┐    ┌──────────────────────────────┐
                  │  startRecording()      │    │  stopAndProcess()            │
                  └───────────┬───────────┘    └───────────────┬──────────────┘
                              ▼                                 ▼
        ┌──────────────────────────────────┐      ┌──────────────────────────────┐
        │  AudioCapture (paquete `record`)  │      │  AudioCapture.stop()          │
        │  PCM16 · mono · 16 kHz · stream   │ ───► │  → Float32List (mono 16k)     │
        │  amplitude → medidor de la UI     │      │  pcm16ToFloat32([-1.0,1.0])   │
        └──────────────────────────────────┘      └───────────────┬──────────────┘
                                                          ▼
                                  ┌──────────────────────────────────────────────┐
                                  │  WhisperEngine.transcribe()  (whisper.cpp FFI)│
                                  │  · corre en un ISOLATE (no bloquea la UI)     │
                                  │  · language: "es"                             │
                                  │  · initial_prompt  ← LearningService          │
                                  │  → TranscriptResult{ text, avgLogProb, ... }  │
                                  └───────────────────────┬──────────────────────┘
                                                          ▼
                                  ┌──────────────────────────────────────────────┐
                                  │  LearningService.applyDictionary(text)        │
                                  │  reemplazos whole-word, case-aware,           │
                                  │  DETERMINISTA · cero alucinación              │
                                  └───────────────────────┬──────────────────────┘
                                                          ▼
                                  ┌──────────────────────────────────────────────┐
                                  │  [opcional] LlmCorrector.correct()            │
                                  │  SOLO si llmEnabled && avgLogProb < gate      │
                                  │  (gating por confianza; off por defecto)      │
                                  └───────────────────────┬──────────────────────┘
                                                          ▼
                                  ┌──────────────────────────────────────────────┐
                                  │  TextInjector.insert(finalText)               │
                                  │  Windows: clipboard-paste (Ctrl+V) por defecto│
                                  │  → escribe en la app con foco                 │
                                  └───────────────────────┬──────────────────────┘
                                                          ▼
                                  ┌──────────────────────────────────────────────┐
                                  │  Registro: state.lastRaw / state.lastText     │
                                  │  (queda disponible para corrección/auditoría) │
                                  └──────────────────────────────────────────────┘
```

Coreografía exacta en `DictationController.stopAndProcess()`:

1. `pcm = await _audio.stop()` → si está vacío, vuelve a `idle`.
2. `initialPrompt = _learning.buildInitialPrompt()`.
3. `result = await _engine.transcribe(pcm, language: 'es', initialPrompt: ...)`.
4. `finalText = _learning.applyDictionary(result.text)` (diccionario determinista).
5. Hook de LLM (comentado, post-MVP) gateado por `result.avgLogProb`.
6. `await _injector.insert(finalText)` (estado `injecting`).
7. Guarda `lastRaw = result.text` y `lastText = finalText`.

---

## 3. Lazo de automejora (aprendizaje)

```
   Usuario edita el último dictado en el panel/historial
                         │
                         ▼
   DictationController.submitCorrection(editedText)
        guard: raw != null && raw != editedText
                         │
                         ▼
   LearningService.recordEdit(raw, edited, avgLogProb)
                         │
        ┌────────────────┴───────────────────────────────────────────────┐
        ▼                                                                  │
   repo.logDictation(raw, finalText, avgLogProb)   ── auditoría/historial  │
                                                                           │
        ▼                                                                  │
   tokenizeWords(raw) , tokenizeWords(edited)                              │
        ▼                                                                  │
   alignWords(a, b)  ── Needleman-Wunsch (igual 0, sustituir 1, hueco 1)   │
        ▼                                                                  │
   FILTRO 1: changeRatio(ops) > maxRewriteRatio (0.5)?  → descarta todo    │
        ▼                            (reescritura completa, no correcciones)│
   para cada op == substitute:                                             │
        ▼                                                                  │
   _extractCorrection(source, target)  ── FILTROS ANTI-FALSOS-POSITIVOS:   │
        · normaliza (quita puntuación de bordes, minúsculas el origen)     │
        · FILTRO 2: src == tgtLower? → null (solo mayús/acentos = LLM)     │
        · FILTRO 3: src ∈ stopwords español? → null                       │
        · FILTRO 4: src es \d+? → null (números no son vocabulario)        │
        · FILTRO 5: ¿misma palabra mal oída?                               │
              normalizedLevenshtein(src,tgt) ≤ 0.6  OR  jaroWinkler ≥ 0.7  │
              si no → null (es reescritura semántica, no corrección)       │
        ▼                                                                  │
   repo.bumpCorrection(src, corrected)   (SQLite: ++freq)                  │
   repo.bumpVocab(corrected)             (SQLite: ++freq vocabulario)      │
        └──────────────────────────────────┬───────────────────────────────┘
                                            ▼
                          LearningService.refresh()
                          · activeMap ← activeCorrections(minFreq = 3)
                          · vocab     ← topVocab(limit = 45)
                                            ▼
              ┌─────────────────────────────┴──────────────────────────────┐
              ▼                                                              ▼
   applyDictionary(text)                                       buildInitialPrompt()
   (próxima transcripción)                                     (próxima transcripción;
                                                                términos frecuentes
                                                                AL FINAL, truncado)
```

Los dos productos del aprendizaje se reinyectan en el pipeline de dictado:

- El **diccionario** (`_activeMap`) en el paso 4 (`applyDictionary`).
- El **vocabulario** (`_vocab`) en el `initial_prompt` del paso 2/3, que sesga al
  propio Whisper hacia tus nombres propios y jerga.

Un par de corrección solo se vuelve activo cuando su frecuencia alcanza
`minCorrectionFreq` (3): el usuario debe corregir la misma equivocación varias
veces antes de que Pronto la dé por buena. Esto es otra capa anti-ruido.

---

## 4. Mapa de módulos

Estado real del árbol (lo IMPLEMENTADO vs. lo REFERENCIADO por el factory pero aún
no presente en disco):

```
lib/
  main.dart                         ✔ entrypoint, window_manager, ProviderScope
  src/
    core/                           ✔ implementado
      app.dart
      config.dart
      audio_utils.dart
      theme.dart
    platform/                       ✔ interfaces (puras) implementadas
      platform_services.dart        ✔ factory de providers por plataforma
      whisper_engine.dart           ✔ interface + TranscriptResult
      audio_capture.dart            ✔ interface
      global_hotkey_service.dart    ✔ interface + HotkeyCombo / TriggerMode
      text_injector.dart            ✔ interface + InjectionMode
      windows/                      ✗ PENDIENTE (referenciado por el factory)
        windows_hotkey_service.dart   → WindowsHotkeyService
        windows_text_injector.dart    → WindowsTextInjector
    features/
      learning/                     ✔ implementado por completo
        learning_service.dart
        learning_repository.dart    ✔ interface (impl SQLite en data/)
        word_diff.dart
        text_similarity.dart
      dictation/                    ✔ implementado
        dictation_controller.dart
        dictation_state.dart
      transcription/                ✗ PENDIENTE → WhisperEngineFfi
      audio_capture/                ✗ PENDIENTE → AudioCaptureRecord
      post_correction/              ✗ PENDIENTE (LlmCorrector, hook ya previsto)
      home/                         ✗ PENDIENTE → HomeScreen (referenciada por app.dart)
      settings/                     ✗ PENDIENTE
    data/                           ✗ PENDIENTE → SqliteLearningRepository
```

> Nota importante: `platform_services.dart` ya importa y construye
> `WhisperEngineFfi`, `AudioCaptureRecord`, `SqliteLearningRepository`,
> `WindowsHotkeyService` y `WindowsTextInjector`. Esas clases concretas son el
> contrato que las implementaciones pendientes deben cumplir; las interfaces que
> implementan ya están fijadas y NO deben cambiar. Lo mismo con `HomeScreen` en
> `app.dart`.

### 4.1 `core/` — fundamentos sin dependencias de plataforma

| Fichero | Responsabilidad | Símbolos clave |
|---|---|---|
| `app.dart` | Raíz de la UI Material 3. | `class ProntoApp extends ConsumerWidget` → `MaterialApp(home: HomeScreen())`. |
| `config.dart` | Constantes y umbrales del sistema (NO estado). | `AppConfig` (privado el ctor). Campos: `defaultLanguage='es'`, `defaultModelFile='ggml-small.bin'`, `defaultThreads=0`, `minCorrectionFreq=3`, `maxCorrectionDistance=0.6`, `maxRewriteRatio=0.5`, `initialPromptMaxChars=820`, `initialPromptMaxTerms=45`, `llmConfidenceGate=-0.35`, `llmMaxContentDrift=0.15`. |
| `audio_utils.dart` | Conversión de formato de audio. | `Float32List pcm16ToFloat32(Uint8List)` (divide por 32768.0, little-endian); `Uint8List concatChunks(List<Uint8List>)`. |
| `theme.dart` | Tema visual. | `ThemeData buildProntoTheme()` (Material 3, dark, seed `0xFF5B6CFF`). |

`config.dart` centraliza TODOS los umbrales del aprendizaje y del gating del LLM en
un único sitio; los servicios los leen, nunca los hardcodean.

### 4.2 `platform/` — interfaces puras (el corazón de la portabilidad)

Cada fichero define una **interface abstracta** sin ninguna dependencia de Win32.
Las implementaciones concretas viven aparte y se eligen en `platform_services.dart`.

**`whisper_engine.dart`** — motor de reconocimiento de voz on-device.
```dart
class TranscriptResult {
  final String text;        // texto crudo, sin post-procesar
  final double avgLogProb;  // proxy de confianza (~ -1.0..-0.1; cerca de 0 = más seguro)
  final String language;    // ISO 639-1, p.ej. "es"
  final int audioMs;
  static const TranscriptResult empty = ...;
  bool get isEmpty;
}
abstract class WhisperEngine {
  Future<void> load(String modelPath);
  bool get isLoaded;
  Future<TranscriptResult> transcribe(
    Float32List pcm16kMonoF32,
    { String language = 'es', String? initialPrompt, int? threads });
  Future<void> dispose();
}
```
Impl principal prevista: `WhisperEngineFfi` (whisper.cpp vía `dart:ffi`, en isolate).

**`audio_capture.dart`** — captura de micrófono en el formato que pide Whisper.
```dart
abstract class AudioCapture {
  Future<bool> hasPermission();
  Future<void> start();
  Future<Float32List> stop();      // mono 16 kHz float32
  Stream<double> get amplitude;    // 0.0..1.0 para el medidor de la UI
  Future<void> dispose();
}
```
Impl prevista: `AudioCaptureRecord` (paquete `record`).

**`global_hotkey_service.dart`** — atajo a nivel de SISTEMA + push-to-talk.
```dart
enum HotkeyEventType { down, up }
class HotkeyEvent { final HotkeyEventType type; }
class HotkeyCombo {
  final int virtualKey;            // Virtual-Key Code de Windows
  final bool ctrl, alt, shift, win;
  static const defaultCombo = HotkeyCombo(virtualKey: 0x20, ctrl: true, alt: true); // Ctrl+Alt+Espacio
  Map<String,dynamic> toJson();  factory HotkeyCombo.fromJson(...);
}
enum TriggerMode { hold, toggle }  // hold = push-to-talk (recomendado MVP)
abstract class GlobalHotkeyService {
  Future<void> register(HotkeyCombo combo, {TriggerMode mode = TriggerMode.hold});
  Future<void> unregister();
  Stream<HotkeyEvent> get events;
  Future<void> dispose();
}
```
Impl prevista: `WindowsHotkeyService` (low-level keyboard hook en isolate con bucle
de mensajes Win32). El `HotkeyCombo` ya serializa a JSON para persistir el atajo
del usuario.

**`text_injector.dart`** — inserción de texto a nivel de sistema.
```dart
enum InjectionMode {
  clipboardPaste,    // guardar portapapeles → poner texto → Ctrl+V → restaurar (por defecto)
  unicodeSendInput,  // SendInput KEYEVENTF_UNICODE carácter a carácter (terminales)
}
abstract class TextInjector {
  Future<void> insert(String text, {InjectionMode mode = InjectionMode.clipboardPaste});
}
```
Impl prevista: `WindowsTextInjector` (win32 FFI).

**`platform_services.dart`** — el **factory** (ver §5). Define los providers de
Riverpod que mapean cada interface a su implementación concreta según
`Platform.isWindows`, además de providers de rutas:
- `whisperEngineProvider` → `WhisperEngineFfi` (con `onDispose`).
- `textInjectorProvider`, `hotkeyServiceProvider` → impl Windows o
  `UnimplementedError` en otras plataformas.
- `audioCaptureProvider` → `AudioCaptureRecord`.
- `learningRepositoryProvider` → `SqliteLearningRepository`;
  `learningServiceProvider` → `LearningService(repo)`.
- `appSupportDirProvider` (FutureProvider, crea `…/models`) y
  `modelPathResolverProvider` (resuelve la ruta absoluta del `.bin` o `null`).

### 4.3 `features/learning/` — el motor de automejora

**`learning_repository.dart`** — contrato de persistencia.
```dart
class CorrectionEntry { final String raw, corrected; final int freq; }
abstract class LearningRepository {
  Future<void> bumpCorrection(String raw, String corrected);
  Future<List<CorrectionEntry>> activeCorrections({int minFreq = 3});
  Future<void> bumpVocab(String term);
  Future<List<String>> topVocab({int limit = 45});
  Future<void> logDictation({required String raw, required String finalText, required double avgLogProb});
}
```
Impl prevista: `SqliteLearningRepository` (en `data/`). En tests se usa una impl en
memoria.

**`learning_service.dart`** — la lógica de aprendizaje (sin dependencia de SQLite,
solo del repositorio).
```dart
class LearningService {
  LearningService(this._repo);
  Future<void> refresh();                                         // recarga caché desde el repo
  Future<void> recordEdit(String raw, String edited, {double avgLogProb = 0.0});
  String applyDictionary(String text);                            // reemplazos whole-word case-aware
  String buildInitialPrompt({String? base});                     // biasing de Whisper
}
```
Detalles relevantes del código:
- Mantiene una **caché en memoria** (`_activeMap`, `_vocab`) que se rellena en
  `refresh()` con `minFreq = AppConfig.minCorrectionFreq` y
  `limit = AppConfig.initialPromptMaxTerms`.
- `recordEdit` registra el dictado, tokeniza, alinea (Needleman-Wunsch), aplica el
  filtro de reescritura global (`changeRatio > maxRewriteRatio`) y por cada
  sustitución llama a `_extractCorrection`.
- `_extractCorrection` aplica los filtros anti-falsos-positivos (stopwords en
  español `_spanishStopwords`, números, “misma palabra mal oída” por Levenshtein
  normalizado ≤ 0.6 o Jaro-Winkler ≥ 0.7, e ignora cambios que sean solo
  mayúsculas/acentos/puntuación).
- `applyDictionary` reemplaza palabra completa con una regex que admite acentos
  (`[A-Za-zÀ-ÿ0-9]+`, porque `\b` de Dart no respeta Unicode) y preserva el patrón
  de mayúsculas del original (`_matchCase`: TODO MAYÚSCULAS / Capitalizada / minúsculas).
- `buildInitialPrompt` coloca los términos **más frecuentes al final** (Whisper pesa
  más los últimos tokens del prompt) y trunca por `initialPromptMaxChars`.

**`word_diff.dart`** — alineamiento de secuencias.
```dart
enum DiffOpType { equal, substitute, insert, delete }
class DiffOp { final DiffOpType type; final String? source, target; }
List<String> tokenizeWords(String text);          // split por \s+ (puntuación adherida)
List<DiffOp> alignWords(List<String> a, List<String> b);  // Needleman-Wunsch (DP + backtrack)
double changeRatio(List<DiffOp> ops);             // fracción de ops no-equal
```
`alignWords` implementa Needleman-Wunsch clásico: matriz de costes `(n+1)×(m+1)`
(igualdad 0, sustitución 1, hueco 1) y backtrack desde `(n,m)` reconstruyendo las
operaciones en orden.

**`text_similarity.dart`** — métricas de cadenas para los filtros.
```dart
int levenshtein(String a, String b);
double normalizedLevenshtein(String a, String b);   // 0=idénticas, 1=totalmente distintas
double jaro(String s1, String s2);
double jaroWinkler(String s1, String s2, {double prefixScale = 0.1});
```
`levenshtein` usa dos filas (memoria O(min)); Jaro-Winkler favorece prefijos
comunes (útil porque los errores de Whisper suelen mantener el inicio de la
palabra).

### 4.4 `features/dictation/` — el controlador (orquestador)

**`dictation_state.dart`** — estado inmutable.
```dart
enum DictationStatus { uninitialized, idle, recording, transcribing, injecting, error }
class DictationState {
  final DictationStatus status;
  final String? lastRaw;    // crudo de Whisper, antes de corregir
  final String? lastText;   // final insertado (tras diccionario + LLM)
  final double level;       // 0..1 para el medidor
  final String? error;
  bool get isBusy;          // recording | transcribing | injecting
  DictationState copyWith({...});   // OJO: error NO se conserva (siempre se reemplaza)
}
```

**`dictation_controller.dart`** — el cerebro del pipeline.
```dart
final dictationControllerProvider =
    NotifierProvider<DictationController, DictationState>(DictationController.new);

class DictationController extends Notifier<DictationState> {
  DictationState build();                          // lee deps con ref.read, schedule initialize()
  Future<void> initialize({String? modelPath});    // refresh aprendizaje, load modelo, registra hotkey
  Future<void> startRecording();                    // (hotkey down)
  Future<void> stopAndProcess();                    // (hotkey up) → todo el pipeline §2
  Future<void> submitCorrection(String editedText); // entrada del lazo de automejora §3
}
```
- Resuelve sus cinco dependencias por los providers del factory en `build()`
  (`whisperEngineProvider`, `audioCaptureProvider`, `textInjectorProvider`,
  `hotkeyServiceProvider`, `learningServiceProvider`).
- Se suscribe al `amplitude` del audio (medidor) y a `hotkeys.events`
  (`down → startRecording`, `up → stopAndProcess`). Cancela ambas en `onDispose`.
- `initialize` deja el estado en `idle` si el motor cargó, o `uninitialized` si no
  hay modelo; cualquier excepción → `error` con mensaje en español.
- `submitCorrection` ignora si no hay `lastRaw` o si el texto no cambió; tras
  aprender, llama a `refresh()` para que los cambios surtan efecto de inmediato.

### 4.5 Módulos pendientes (contratos ya fijados)

- **`features/transcription/` → `WhisperEngineFfi`**: implementa `WhisperEngine`
  con `dart:ffi` sobre `whisper.dll`, ejecutando `transcribe` en un **isolate**.
- **`features/audio_capture/` → `AudioCaptureRecord`**: implementa `AudioCapture`
  con el paquete `record` (stream PCM16 16k mono → acumula → `pcm16ToFloat32`).
- **`features/post_correction/` → `LlmCorrector`**: corrección opcional con LLM
  (Ollama local o nube vía `http`). El hook ya está previsto y comentado en
  `stopAndProcess`, gateado por `avgLogProb < AppConfig.llmConfidenceGate` y
  protegido por `AppConfig.llmMaxContentDrift`.
- **`features/home/` → `HomeScreen`**: ya referenciada por `app.dart`. UI con
  estado de dictado, medidor, último texto y entrada para `submitCorrection`.
- **`features/settings/`**: idioma, modelo, atajo (`HotkeyCombo`), modo de inyección,
  on/off del LLM. Persistencia con `shared_preferences`.
- **`data/` → `SqliteLearningRepository`**: implementa `LearningRepository` con
  `sqlite3` (sin codegen). Tablas para correcciones (`raw`, `corrected`, `freq`),
  vocabulario (`term`, `freq`) y log de dictados.

---

## 5. Estrategia de abstracción por plataforma

El patrón es **interface pura + factory de providers**:

1. Cada capacidad nativa se declara como `abstract class` en `lib/src/platform`
   (`WhisperEngine`, `AudioCapture`, `GlobalHotkeyService`, `TextInjector`,
   `LearningRepository`). Estas interfaces no importan `win32` ni nada específico.
2. Las implementaciones concretas viven en `features/<x>/`, `data/` o
   `platform/windows/`, e importan lo que necesiten (`dart:ffi`, `win32`, `record`,
   `sqlite3`).
3. `platform_services.dart` es el **único** punto que decide qué implementación se
   usa, normalmente con un `if (Platform.isWindows)`; para el resto de plataformas
   lanza `UnimplementedError` con mensaje claro en español.

```dart
final textInjectorProvider = Provider<TextInjector>((ref) {
  if (Platform.isWindows) return WindowsTextInjector();
  throw UnimplementedError('TextInjector solo implementado en Windows (MVP).');
});
```

**Por qué este diseño:**
- **Portabilidad real.** Portar a macOS/Linux/móvil = añadir una rama en el factory
  y una clase nueva, sin tocar `DictationController`, `LearningService` ni la UI.
- **Testabilidad.** El controlador depende de interfaces; en tests se inyectan
  dobles (de hecho, el repositorio de aprendizaje tiene una impl en memoria para
  los tests). Riverpod permite sobreescribir cualquier provider.
- **Acoplamiento mínimo.** El núcleo de la app (orquestación + aprendizaje) no sabe
  que existe Win32; eso queda confinado a `platform/windows/` y a los servicios
  concretos.
- **Fallo explícito.** En una plataforma no soportada el error es inmediato y
  legible, en vez de un comportamiento silencioso e incorrecto.

---

## 6. Decisiones de diseño y su razón

| Decisión | Razón |
|---|---|
| **FFI propio sobre whisper.cpp** (no un wrapper pub) | Control total de parámetros (`initial_prompt`, `language`, `threads`, `avgLogProb`), del ciclo de vida del modelo (cargar una vez, mantener vivo) y de la ejecución en **isolate**. Evita depender de un wrapper que se quede atrás respecto a whisper.cpp o que no exponga el logprob que necesitamos para el gating. |
| **Inyección por clipboard-paste** (no `SendInput` por defecto) | Robusto con texto largo y Unicode completo (acentos, ñ, emoji) y mucho más rápido que enviar tecla a tecla. Se guarda y restaura el portapapeles para no destruirlo. `unicodeSendInput` queda como modo alternativo para apps que ignoran Ctrl+V (terminales). |
| **Diccionario determinista ANTES del LLM** | El mapa aprendido es exacto, instantáneo y de **cero alucinación**. Garantiza que tus nombres propios/jerga siempre salen bien sin pasar por un modelo generativo. El LLM solo afina lo que el diccionario no cubre. |
| **Gating del LLM por confianza** (`avgLogProb < llmConfidenceGate`) | Si Whisper está muy seguro, el texto ya es bueno: dejar pasar el LLM solo arriesga **parafraseo** y latencia. Se invoca únicamente cuando la confianza es baja, y aun así se descarta su salida si cambia demasiado contenido (`llmMaxContentDrift`). LLM desactivado por defecto. |
| **Push-to-talk (hold)** como modo por defecto | Inicio/fin de habla nítidos (cero detección de silencio que falle), control total del usuario y privacidad: el micro solo graba mientras mantienes la tecla. `toggle` queda disponible. |
| **Atajo GLOBAL vía low-level keyboard hook** | Permite dictar en cualquier app sin que Pronto tenga el foco — su razón de ser. El hook corre en un isolate con bucle de mensajes Win32 para no bloquear la UI. |
| **SQLite sin codegen** (`sqlite3` a mano) | Consultas con agregación/frecuencias y `ORDER BY` que SQL hace de forma natural; persistencia fiable del perfil de aprendizaje. Sin `drift`/`build_runner`: menos dependencias, compilación más simple, control total del esquema y las migraciones. |
| **Riverpod con providers manuales** (sin `@riverpod`) | Sin codegen: el grafo de dependencias es explícito y legible, y el factory por plataforma es trivial de leer. |
| **Aprendizaje en dos productos (diccionario + initial_prompt)** | Atacan el problema en dos capas: el `initial_prompt` mejora la **fuente** (sesga a Whisper hacia tu vocabulario) y el diccionario corrige el **residuo** de forma determinista. |
| **Frecuencia mínima (`minCorrectionFreq = 3`) y filtros de similitud** | El usuario corrige cosas por muchos motivos (typos, cambios de idea). Solo se consolidan correcciones repetidas y que parezcan “la misma palabra mal oída”, no reescrituras semánticas. Evita envenenar el diccionario. |

---

## 7. Límites conocidos

- **Solo Windows en el MVP.** `textInjectorProvider` y `hotkeyServiceProvider`
  lanzan `UnimplementedError` fuera de Windows. Las ramas macOS/Linux/móvil están
  previstas pero no implementadas.
- **Implementaciones concretas pendientes.** `platform_services.dart` y `app.dart`
  ya referencian `WhisperEngineFfi`, `AudioCaptureRecord`, `SqliteLearningRepository`,
  `WindowsHotkeyService`, `WindowsTextInjector` y `HomeScreen`, pero esos ficheros
  aún no existen en disco; el proyecto no compila hasta crearlos respetando las
  interfaces ya fijadas.
- **Post-corrección LLM aún no conectada.** El hook está comentado en
  `stopAndProcess`; faltan `LlmCorrector` y el módulo `post_correction/`, además de
  un flag de settings que lo active.
- **`copyWith` no preserva `error`.** En `DictationState.copyWith`, `error` se
  asigna siempre desde el parámetro (que por defecto es `null`); por diseño cualquier
  transición de estado “limpia” el error previo, pero conviene tenerlo en cuenta al
  componer cambios.
- **Atajo por defecto fijo.** `HotkeyCombo.defaultCombo` (Ctrl+Alt+Espacio) puede
  colisionar con atajos de otras apps; el modelo ya soporta JSON para personalizarlo,
  pero falta la UI de settings y su persistencia. `register` debe lanzar si el atajo
  ya está en uso.
- **Calidad y latencia atadas al modelo.** El defecto es `ggml-small.bin` (equilibrio
  CPU); modelos mayores (`large-v3-turbo`) mejoran precisión a costa de latencia/RAM.
  Sin GPU, la transcripción de audios largos puede notarse.
- **El audio se acumula en memoria** hasta `stop()`; dictados muy largos consumen RAM
  proporcional a la duración (no hay streaming/transcripción incremental).
- **Latido del aprendizaje.** Las correcciones solo se aplican tras alcanzar
  `minFreq = 3` y tras un `refresh()`; hay una ventana en la que una corrección nueva
  todavía no surte efecto. Es intencionado (anti-ruido), pero es una limitación de UX.
- **`applyDictionary` opera por palabra suelta.** No corrige bigramas/expresiones de
  varias palabras ni contextos donde la misma palabra debe corregirse a veces sí y a
  veces no.
- **Sin diarización ni puntuación avanzada propia.** La puntuación depende del modelo
  Whisper y, opcionalmente, del LLM.
- **Permiso de micrófono dependiente del SO.** En Windows hay que habilitarlo en
  Configuración > Privacidad; la app lo detecta (`hasPermission`) pero no puede
  concederlo por el usuario.

---

## 8. Dependencias clave (pubspec)

`flutter_riverpod ^2.5` (estado), `ffi ^2.1` + `win32 ^5.5` (atajo global +
inyección), `record ^5.1` (audio PCM16 16k), `sqlite3 ^2.4` +
`sqlite3_flutter_libs` (persistencia sin codegen), `path` / `path_provider` /
`shared_preferences` (rutas y settings), `window_manager` / `tray_manager` /
`launch_at_startup` (ventana, bandeja, autostart), `http ^1.2` (LLM opcional).

Las versiones del `pubspec.yaml` son orientativas (verificadas a mediados de 2026);
ante un conflicto de resolución, ejecutar `flutter pub upgrade --major-versions` y
confirmar la API actual en pub.dev / Microsoft Learn antes de usarla.
