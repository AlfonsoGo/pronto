# Pronto

**Dictado de voz por IA, open source, para escribir mucho más rápido.**
_Hecho por **Beltran Labs**._

Pronto convierte tu voz en texto y lo escribe **directamente en cualquier
aplicación** que tengas abierta (el navegador, el editor de código, el chat, el
correo…). Todo el reconocimiento ocurre **en tu propio equipo**: tu voz no sale
de tu ordenador. Y cuanto más lo usas, **mejor te entiende**, porque aprende de
tus propias correcciones.

> Estado actual: **MVP scaffold para Windows** (escritorio primero). El esqueleto
> de la app, las interfaces de plataforma y el motor de automejora ya están en
> su sitio. Ver [estado actual](#estado-actual) y [ROADMAP.md](ROADMAP.md).

---

## ¿Qué hace especial a Pronto?

- **🗣️ Dictado GLOBAL.** Mantén pulsado un atajo de teclado (por defecto
  `Ctrl + Alt + Espacio`), habla y suéltalo: el texto aparece donde tengas el
  cursor, **en la app que sea**. No hace falta tener Pronto en primer plano.
- **🔒 100 % on-device y privado.** La transcripción usa
  [whisper.cpp](https://github.com/ggml-org/whisper.cpp) ejecutándose en tu CPU.
  Sin servidores, sin cuotas, sin enviar tu audio a la nube. Funciona sin
  conexión.
- **🇪🇸 Español primero.** Pensado y afinado para el español desde el primer día;
  el soporte multilingüe (empezando por inglés) llega después y reutiliza el
  mismo motor (ver [ROADMAP.md](ROADMAP.md)).
- **🧠 Automejora: aprende de tus correcciones.** Cuando corriges una
  transcripción, Pronto detecta *qué* palabra falló y *cómo* la querías, y la
  arregla automáticamente la próxima vez. Construye tu **diccionario personal**
  (nombres propios, jerga técnica, marcas…) y lo usa también para **sesgar** al
  reconocedor (*biasing* vía `initial_prompt`).
- **⚡ Rápido y ligero.** Push-to-talk con baja latencia, medidor de nivel de
  audio en vivo y modelos que equilibran precisión y velocidad en CPU.
- **🆓 Open source (licencia MIT propuesta).** Inspecciónalo, modifícalo,
  contribúyelo.

---

## Capturas conceptuales

Mientras pulimos las capturas reales, así se siente usar Pronto:

```
┌──────────────────────────────┐
│            Pronto          ●│  ← ventana compacta + icono de bandeja
├──────────────────────────────┤
│                              │
│        ◖  Listo para        │   Estado: idle / grabando /
│           dictar  ◗         │   transcribiendo / insertando
│                              │
│   ▁▂▅▇▆▃▁  (nivel de audio)  │   medidor en vivo mientras hablas
│                              │
│   Atajo:  Ctrl + Alt + Esp.  │
│                              │
│   Último dictado:            │
│   "Reunión con García a las  │   ← editable: si lo corriges,
│    cinco en la sala azul"    │      Pronto APRENDE del cambio
│                              │
└──────────────────────────────┘
```

**Flujo de uso (push-to-talk):**

```
  Mantienes        Sueltas el        whisper.cpp        diccionario      el texto
  el atajo   ──▶   atajo y se   ──▶  transcribe   ──▶   aprendido    ──▶ se inserta
  y hablas         para de grabar    en isolate         corrige           en la app
                                                        (+ LLM opc.)      con foco
```

**Bucle de automejora:**

```
  Dictas  ──▶  Pronto escribe  ──▶  TÚ corriges  ──▶  aprende el par
    ▲                                                    "raw → corregido"
    │                                                          │
    └──────────  la próxima vez ya lo escribe bien  ◀──────────┘
```

---

## El stack elegido

Todo en **Flutter (Dart, null-safe sound)**, **sin generación de código**
(nada de `build_runner`, `freezed`, `drift` ni `@riverpod`). El estado se maneja
con **Riverpod usando providers manuales**.

| Pieza | Tecnología | Para qué |
|---|---|---|
| Reconocimiento de voz | **whisper.cpp vía `dart:ffi`** | Transcripción on-device en CPU, modelos GGML (ver [BUILD_WHISPER.md](BUILD_WHISPER.md)) |
| Captura de audio | **`record`** | Micrófono → PCM mono 16 kHz (normalizado a float32 para Whisper) |
| Atajo global + inyección de texto | **`win32` + `ffi`** | Atajo a nivel de sistema (low-level keyboard hook) e inserción de texto en la app con foco (portapapeles/`SendInput` Unicode) |
| Estado | **Riverpod (`flutter_riverpod ^2.5`)** | Providers/Notifiers manuales, sin codegen |
| Persistencia | **`sqlite3` + `sqlite3_flutter_libs`** | Diccionario aprendido, vocabulario y log de dictados |
| Ventana / overlay / bandeja / autostart | **`window_manager` + `tray_manager` + `launch_at_startup`** | Ventana compacta, icono de bandeja y arranque con el sistema |
| Post-corrección LLM (opcional) | **`http`** (Ollama local o nube) | Limpieza opcional del texto, *gateada* por confianza para evitar parafraseo |

> Las versiones exactas de los paquetes están en [`pubspec.yaml`](pubspec.yaml).

---

## Estado actual

**MVP scaffold (Windows).** Lo que ya existe en este repositorio:

- ✅ **Arranque de la app** y ventana compacta con `window_manager`
  (`lib/main.dart`, `lib/src/core/app.dart`, `theme.dart`).
- ✅ **Interfaces (contratos) de la capa de plataforma**, agnósticas del SO, para
  poder portar luego sin tocar el resto de la app:
  `WhisperEngine`, `AudioCapture`, `GlobalHotkeyService`, `TextInjector`
  (en `lib/src/platform/`).
- ✅ **Orquestador del pipeline de dictado** completo en Riverpod
  (`DictationController` + `DictationState`): hotkey → grabar → transcribir →
  diccionario → (LLM opcional) → insertar → registrar.
- ✅ **Motor de automejora** funcional y testeado: alineamiento de palabras
  (Needleman-Wunsch), similitud (Levenshtein, Jaro-Winkler), extracción de
  pares de corrección con filtros anti-falsos-positivos, diccionario
  determinista y construcción del `initial_prompt` de *biasing*
  (`lib/src/features/learning/`, con tests en `test/learning/`).
- ✅ **Utilidades de audio** (PCM16 → float32) y **configuración** central
  (`lib/src/core/`).

**Pendiente (siguiente fase, ver [ROADMAP.md](ROADMAP.md)):** las
implementaciones concretas que el `factory` por plataforma ya espera —
`WhisperEngineFfi` (FFI a whisper.cpp), `AudioCaptureRecord`,
`WindowsHotkeyService`, `WindowsTextInjector`, `SqliteLearningRepository` y la
pantalla principal (`HomeScreen`)— para cerrar el pipeline de punta a punta.

---

## Arranque rápido

1. **Requisitos.** Flutter (canal estable) con soporte de escritorio Windows
   activado, y las herramientas de compilación nativa. Detalles paso a paso en
   **[SETUP.md](SETUP.md)**.
2. **whisper.cpp y el modelo.** Compila la librería nativa y descarga un modelo
   GGML (por defecto `ggml-small.bin`; para máxima calidad,
   `ggml-large-v3-turbo.bin`). Guía completa en **[BUILD_WHISPER.md](BUILD_WHISPER.md)**.
3. **Dependencias y ejecución:**
   ```bash
   flutter pub get
   flutter run -d windows
   ```
4. **Dicta.** Mantén pulsado `Ctrl + Alt + Espacio`, habla, suelta. El texto
   aparecerá donde tengas el cursor.

> ¿Algún paquete no resuelve? `flutter pub upgrade --major-versions` y revisa
> las notas de [SETUP.md](SETUP.md).

---

## Estructura de carpetas

El código vive bajo `lib/src/`, organizado por **capas** y por **features**.
Dentro de `lib/` los imports son **relativos**; para paquetes de pub se usa
`package:`.

```
lib/
├─ main.dart                     # Punto de entrada: ventana + ProviderScope
└─ src/
   ├─ core/                      # Núcleo transversal (sin dependencias de UI ni SO)
   │  ├─ app.dart                #   Widget raíz (MaterialApp)
   │  ├─ config.dart             #   Constantes y umbrales (modelo, idioma, automejora)
   │  ├─ theme.dart              #   Tema Material 3
   │  └─ audio_utils.dart        #   Conversión PCM16 → float32
   │
   ├─ common/                    # Widgets y utilidades de UI reutilizables
   │
   ├─ platform/                  # Capa de plataforma: INTERFACES + factory
   │  ├─ whisper_engine.dart     #   Contrato del reconocedor (+ TranscriptResult)
   │  ├─ audio_capture.dart      #   Contrato de captura de micrófono
   │  ├─ global_hotkey_service.dart  # Contrato del atajo global (HotkeyCombo/Event)
   │  ├─ text_injector.dart      #   Contrato de inserción de texto
   │  └─ platform_services.dart  #   Providers Riverpod que eligen la impl. por SO
   │
   ├─ features/                  # Funcionalidades, cada una en su carpeta
   │  ├─ dictation/              #   Orquestación del pipeline (Controller + State)
   │  ├─ learning/               #   Automejora: diff, similitud, diccionario, biasing
   │  ├─ transcription/          #   Implementación FFI de whisper.cpp
   │  ├─ audio_capture/          #   Implementación con `record`
   │  └─ home/                   #   Pantalla principal
   │
   └─ data/                      # Persistencia (SQLite): repositorios concretos
```

**La idea clave:** todo lo dependiente del sistema operativo está detrás de las
interfaces de `platform/`. Al añadir macOS, Linux, Android o iOS **solo cambia
la capa `platform/`** (y sus implementaciones); el resto de la app se reutiliza
tal cual. Ver [ROADMAP.md](ROADMAP.md).

---

## Cómo funciona la automejora (en breve)

1. **Aprende del diff.** Al corregir un dictado, se alinean la transcripción
   cruda y tu versión final (Needleman-Wunsch) y se extraen pares
   `raw → corregido`. Filtros estrictos descartan stopwords, números,
   reescrituras semánticas y meros ajustes de mayúsculas/acentos, para que el
   diccionario no aprenda basura.
2. **Corrige de forma determinista.** Los pares que superan una frecuencia
   mínima se aplican palabra por palabra (respetando mayúsculas) — instantáneo y
   con **cero alucinación**.
3. **Sesga al reconocedor.** Tu vocabulario más frecuente alimenta el
   `initial_prompt` de Whisper para que acierte nombres propios y jerga desde el
   principio.
4. **(Opcional) Pulido con LLM.** Una post-corrección con un LLM local (Ollama)
   o en la nube, **gateada por la confianza** del reconocedor y con protección
   anti-parafraseo. Desactivada por defecto.

---

## Licencia

**MIT © 2026 Beltran Labs.** Pronto es libre y comunitario: puedes usar,
modificar y distribuir el proyecto bajo los términos de la licencia
[MIT](LICENSE).

---

## Contribuir

¡Bienvenidas las contribuciones! La hoja de ruta y los puntos donde más ayuda
hace falta están en [ROADMAP.md](ROADMAP.md). Mantén el estilo del proyecto:
Dart null-safe sound, comentarios y UI **en español**, imports relativos dentro
de `lib/`, **sin codegen** y Riverpod con providers manuales.
