# Compilar e integrar whisper.cpp en Pronto (Windows)

Esta guia explica, paso a paso y en espanol, como compilar **whisper.cpp**
(motor de voz on-device, formato **GGML**), integrar las librerias en el runner
de Windows de Pronto y descargar/colocar el modelo de reconocimiento.

- whisper.cpp fijado al tag estable **v1.9.1** (mediados de 2026).
- Repositorio fuente: <https://github.com/ggml-org/whisper.cpp>
- Modelos GGML oficiales: <https://huggingface.co/ggerganov/whisper.cpp>

---

## 1. Prerrequisitos

| Herramienta | Detalle |
|---|---|
| **Visual Studio 2022** | Carga de trabajo *"Desktop development with C++"* (instala MSVC, el SDK de Windows y CMake). |
| **CMake** | En el `PATH`. Lo incluye VS 2022; si no, descargalo de <https://cmake.org/download/>. |
| **Git** | En el `PATH`. <https://git-scm.com/download/win> |
| **Flutter** (Windows desktop) | Para compilar la app. `flutter config --enable-windows-desktop`. |

Comprueba que las herramientas estan disponibles:

```powershell
git --version
cmake --version
```

> Si la configuracion de CMake falla por no encontrar el compilador, abre la
> consola **"x64 Native Tools Command Prompt for VS 2022"** y ejecuta los
> scripts desde ahi.

---

## 2. Compilar whisper.cpp

Desde la raiz del proyecto:

```powershell
powershell -ExecutionPolicy Bypass -File native\whisper\build_whisper.ps1
```

El script `native\whisper\build_whisper.ps1`:

1. Comprueba que `git` y `cmake` estan presentes.
2. Clona `ggml-org/whisper.cpp` fijando el tag **v1.9.1** (o el que indiques con
   `-Tag`).
3. Configura con CMake:

   ```powershell
   cmake -B native\whisper\whisper.cpp\build -S native\whisper\whisper.cpp `
     -DBUILD_SHARED_LIBS=ON `
     -DCMAKE_BUILD_TYPE=Release `
     -DWHISPER_BUILD_EXAMPLES=OFF `
     -DWHISPER_BUILD_TESTS=OFF
   ```

4. Compila en Release:

   ```powershell
   cmake --build native\whisper\whisper.cpp\build --config Release -j
   ```

5. Copia **whisper.dll** y todas las **ggml\*.dll** (p.ej. `ggml.dll`,
   `ggml-base.dll`, `ggml-cpu.dll`) a `native\whisper\bin\`.

### Aceleracion por GPU (Vulkan) — opcional

Por defecto se compila para **CPU** (maxima compatibilidad). Para activar la GPU
con Vulkan:

1. Instala el **SDK de Vulkan**: <https://vulkan.lunarg.com/>
2. Ejecuta:

   ```powershell
   powershell -ExecutionPolicy Bypass -File native\whisper\build_whisper.ps1 -Vulkan
   ```

   Esto anade `-DGGML_VULKAN=ON` a la configuracion de CMake. Generara
   `ggml-vulkan.dll` adicional, que tambien se copiara a `bin\`.

---

## 3. Integrar las DLLs en el runner de Windows

El `.exe` de la app carga `whisper.dll` (y sus dependencias `ggml*.dll`) en
tiempo de ejecucion mediante `dart:ffi`. **Las DLLs deben estar junto al `.exe`.**

Tras `flutter build windows`, el ejecutable Release queda en:

```
build\windows\x64\runner\Release\pronto.exe
```

### Opcion A — Copia manual (rapida)

```powershell
flutter build windows
Copy-Item native\whisper\bin\*.dll build\windows\x64\runner\Release\ -Force
```

Para depuracion (`flutter run -d windows`), copia tambien a la carpeta Debug:

```powershell
Copy-Item native\whisper\bin\*.dll build\windows\x64\runner\Debug\ -Force
```

### Opcion B — Automatizar en CMake (post-build, recomendado)

Edita `windows\runner\CMakeLists.txt` y anade, despues de la definicion del
target `${BINARY_NAME}` (el ejecutable), un paso post-build que copie las DLLs:

```cmake
# --- Pronto: copiar las DLLs de whisper.cpp junto al .exe ---
# Las DLLs (whisper.dll + ggml*.dll) las genera native/whisper/build_whisper.ps1
# y quedan en native/whisper/bin/.
file(GLOB WHISPER_DLLS "${CMAKE_SOURCE_DIR}/../native/whisper/bin/*.dll")
foreach(WHISPER_DLL ${WHISPER_DLLS})
  add_custom_command(TARGET ${BINARY_NAME} POST_BUILD
    COMMAND ${CMAKE_COMMAND} -E copy_if_different
      "${WHISPER_DLL}" "$<TARGET_FILE_DIR:${BINARY_NAME}>"
    COMMENT "Copiando ${WHISPER_DLL} junto al ejecutable de Pronto")
endforeach()
```

> Nota: en el proyecto Flutter de Windows, `${CMAKE_SOURCE_DIR}` apunta a
> `windows/`, por eso se usa `../native/whisper/bin`. Ajusta la ruta si tu
> estructura difiere. Tras esto, las DLLs se copian automaticamente en cada
> `flutter build windows` / `flutter run -d windows`.

> Como alternativa, puedes usar la lista `INSTALL` del CMakeLists del runner
> (la seccion que ya copia los plugins) anadiendo las DLLs de whisper para que
> el bundle de instalacion las incluya.

---

## 4. Descargar y colocar el modelo

whisper.cpp usa modelos **GGML** (`.bin`), **no GGUF**.

```powershell
# Modelo equilibrado (por defecto), a la carpeta de datos de la app:
powershell -ExecutionPolicy Bypass -File native\whisper\download_model.ps1 -Model small

# Maxima calidad (mas pesado/lento en CPU):
powershell -ExecutionPolicy Bypass -File native\whisper\download_model.ps1 -Model large-v3-turbo

# Variante cuantizada (mas ligera) de large-v3-turbo:
powershell -ExecutionPolicy Bypass -File native\whisper\download_model.ps1 -Model large-v3-turbo -Quantized
```

### Donde debe ir el `.bin`

La app busca el modelo en:

```
getApplicationSupportDirectory()/models/<archivo>.bin
```

En Windows, `getApplicationSupportDirectory()` (del paquete `path_provider`)
resuelve a `%APPDATA%\<Organizacion>\<Producto>`, segun la
Organizacion/Producto definidos en `windows\runner` (`Runner.rc` /
`CMakeLists.txt`). Por defecto, para el paquete `pronto`, suele ser algo como:

```
%APPDATA%\Pronto\Pronto\models\
```

La app **crea automaticamente** la subcarpeta `models\` al arrancar
(ver `appSupportDirProvider` en `lib/src/platform/platform_services.dart`). Si
no estas seguro de la ruta exacta, arranca la app una vez y revisa los logs de
inicio, o copia el `.bin` a esa carpeta `models\`.

- `download_model.ps1 -Destination app` (por defecto) lo coloca en esa carpeta.
- `download_model.ps1 -Destination repo` lo coloca en `native\whisper\models\`
  (util para pruebas; en ese caso pasa la ruta explicita a la app).

### Archivo por defecto que carga la app

La constante `AppConfig.defaultModelFile` en `lib/src/core/config.dart` vale
`'ggml-small.bin'`. Si descargas `large-v3-turbo`, actualiza esa constante:

```dart
static const String defaultModelFile = 'ggml-large-v3-turbo.bin';
```

---

## 5. Notas importantes

- **Formato**: whisper.cpp usa **GGML** (`.bin`), no GGUF. No uses modelos de
  llama.cpp.
- **Audio**: el motor espera **16 kHz, mono, `float32`** normalizado a
  `[-1, 1]` (ver `WhisperEngine.transcribe` en
  `lib/src/platform/whisper_engine.dart`).
- **Idioma**: fija siempre `language: 'es'` (espanol primero). El parametro por
  defecto de la interfaz ya es `'es'`.
- **DLLs junto al `.exe`**: `whisper.dll` no se carga si sus dependencias
  `ggml*.dll` no estan en la misma carpeta.
- **Distribucion**: al empaquetar la app, incluye todas las `*.dll` de
  `native\whisper\bin\` (mas el modelo si lo quieres preinstalar, aunque lo
  habitual es descargarlo en el primer arranque).

---

## 6. Comandos resumidos (copia y pega)

```powershell
# Compilar whisper.cpp (CPU)
powershell -ExecutionPolicy Bypass -File native\whisper\build_whisper.ps1

# (Opcional) con GPU Vulkan
powershell -ExecutionPolicy Bypass -File native\whisper\build_whisper.ps1 -Vulkan

# Descargar modelo
powershell -ExecutionPolicy Bypass -File native\whisper\download_model.ps1 -Model small

# Compilar la app y copiar DLLs
flutter build windows
Copy-Item native\whisper\bin\*.dll build\windows\x64\runner\Release\ -Force
```
