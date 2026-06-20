# native/ — Componentes nativos de Pronto

Esta carpeta contiene todo lo necesario para compilar e integrar **whisper.cpp**
(motor de reconocimiento de voz on-device, formato **GGML**) en Pronto para
Windows escritorio.

> La guia completa y detallada esta en **[`docs/BUILD_WHISPER.md`](../docs/BUILD_WHISPER.md)**.
> Este README es el resumen rapido.

## Estructura

```
native/
└── whisper/
    ├── build_whisper.ps1     Compila whisper.cpp -> whisper.dll + ggml*.dll
    ├── download_model.ps1    Descarga modelos GGML desde Hugging Face
    ├── bin/                  (generado) DLLs compiladas
    ├── models/              (opcional) modelos descargados para pruebas
    └── whisper.cpp/         (generado) clon del repositorio fuente
```

> `bin/`, `models/` y `whisper.cpp/` se generan al ejecutar los scripts y estan
> ignorados por git (`.gitignore`).

## Prerrequisitos

1. **Visual Studio 2022** con la carga de trabajo
   *"Desktop development with C++"* (incluye MSVC y CMake).
2. **CMake** en el `PATH` (lo trae VS 2022, o instalalo aparte).
3. **Git** en el `PATH`.

## Pasos rapidos

```powershell
# 1) Compilar whisper.cpp (CPU). Genera native\whisper\bin\*.dll
powershell -ExecutionPolicy Bypass -File native\whisper\build_whisper.ps1

# 2) Descargar un modelo (small por defecto) a la carpeta de datos de la app
powershell -ExecutionPolicy Bypass -File native\whisper\download_model.ps1 -Model small

# 3) Compilar la app Flutter para Windows
flutter build windows

# 4) Copiar las DLLs junto al .exe de la app
Copy-Item native\whisper\bin\*.dll build\windows\x64\runner\Release\ -Force
```

> Para automatizar el paso 4 en cada compilacion (post-build de CMake), mira la
> seccion correspondiente en `docs/BUILD_WHISPER.md`.

## Notas clave

- **Formato GGML** (`.bin`), no GGUF.
- **Aceleracion GPU**: por defecto se compila para CPU. Para Vulkan, ejecuta
  `build_whisper.ps1 -Vulkan` (requiere el SDK de Vulkan).
- **Audio**: 16 kHz, mono, `float32` normalizado a `[-1, 1]`.
- **Idioma**: fija `language: 'es'` al transcribir (espanol primero).
- Las DLLs **whisper.dll** y **ggml\*.dll** deben estar junto al `.exe` o el
  cargador no encontrara el motor en tiempo de ejecucion.
