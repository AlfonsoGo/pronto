# Guía de instalación y primer arranque (Windows 11)

Esta guía te lleva, paso a paso, desde un Windows 11 **sin nada instalado** hasta
ver Pronto funcionando. Todos los comandos están pensados para **PowerShell**
(la app por defecto: pulsa `Win`, escribe `PowerShell`, abre *Windows PowerShell*).

> Convención: el símbolo `PS>` indica el *prompt* de PowerShell; **no lo copies**,
> copia solo el comando que va detrás.

> En esta guía, `C:\ruta\a\Pronto` representa la carpeta donde hayas clonado el
> proyecto. Sustitúyela por la ruta real de tu equipo.

---

## 0) Resumen de lo que vas a instalar

1. **Flutter SDK** — el framework y sus herramientas de línea de comandos.
2. **Visual Studio 2022** con la carga *Desktop development with C++* —
   **imprescindible** para compilar apps de escritorio Windows con Flutter
   (Flutter usa el compilador MSVC y CMake de esa carga).
3. Las **carpetas de plataforma** del proyecto (`windows/`), que aún no existen.
4. Las **dependencias** del `pubspec.yaml`.
5. El motor **whisper.cpp** (`whisper.dll`) y el **modelo** de voz.

Tiempo aproximado: 30-60 min (la descarga de Visual Studio es la parte larga).

---

## 1) Instalar Flutter SDK y Visual Studio 2022

### 1.1 Instalar Flutter

**Opción A — winget (recomendada, la más sencilla):**

```powershell
PS> winget install --id Google.Flutter -e
```

Cierra y vuelve a abrir PowerShell al terminar para que el `PATH` se refresque.

> Si `winget` no está disponible, actualiza *App Installer* desde Microsoft Store,
> o usa la Opción B.

**Opción B — descarga manual:**

1. Descarga el `.zip` estable desde
   <https://docs.flutter.dev/get-started/install/windows/desktop>
2. Descomprímelo en una ruta **sin espacios ni permisos de administrador**,
   por ejemplo `C:\src\flutter` (NO uses `C:\Program Files\`):

   ```powershell
   PS> New-Item -ItemType Directory -Force C:\src | Out-Null
   PS> Expand-Archive -Path "$env:USERPROFILE\Downloads\flutter_windows_*-stable.zip" -DestinationPath C:\src
   ```

3. Añade `C:\src\flutter\bin` al `PATH` de tu usuario (persistente):

   ```powershell
   PS> $flutterBin = "C:\src\flutter\bin"
   PS> $userPath   = [Environment]::GetEnvironmentVariable("Path", "User")
   PS> if ($userPath -notlike "*$flutterBin*") {
   PS>   [Environment]::SetEnvironmentVariable("Path", "$userPath;$flutterBin", "User")
   PS> }
   ```

   Cierra y vuelve a abrir PowerShell para que el cambio tenga efecto.

Comprueba que `flutter` se encuentra en el `PATH`:

```powershell
PS> flutter --version
```

### 1.2 Instalar Visual Studio 2022 con la carga de C++ (OBLIGATORIO)

Flutter **no puede compilar apps Windows** sin esta carga de trabajo.

**Opción A — winget:**

```powershell
PS> winget install --id Microsoft.VisualStudio.2022.Community -e --override "--add Microsoft.VisualStudio.Workload.NativeDesktop --includeRecommended --passive --norestart"
```

**Opción B — instalador interactivo:**

1. Descarga *Visual Studio 2022 Community* (gratis) desde
   <https://visualstudio.microsoft.com/downloads/>
2. En el **Visual Studio Installer**, marca la carga
   **«Desktop development with C++»** (*Desarrollo para el escritorio con C++*)
   y deja los componentes recomendados.
3. Instala y, si lo pide, **reinicia** el equipo.

> No confundas *Visual Studio 2022* (el IDE) con *Visual Studio Code*.
> Para compilar apps Windows hace falta **Visual Studio 2022** (por el toolchain
> de C++). VS Code es opcional como editor.

### 1.3 Verificar con `flutter doctor`

```powershell
PS> flutter doctor
```

Para esta app **solo necesitas en verde** estas dos líneas:

- `[√] Flutter`
- `[√] Visual Studio - develop Windows apps`

Es **normal y se puede ignorar** que aparezcan en rojo/aviso *Android toolchain*,
*Chrome*, *Android Studio* o la firma de licencias de Android: no compilamos para
Android ni web en este proyecto.

Si quieres ver el detalle de algún problema:

```powershell
PS> flutter doctor -v
```

---

## 2) Generar las carpetas de plataforma que faltan (`windows/`)

El proyecto ya trae `lib/` y `pubspec.yaml`, pero **no** la carpeta `windows/`
(el *runner* nativo). La generamos con `flutter create` apuntando al directorio
actual con el punto final (`.`):

```powershell
PS> cd C:\ruta\a\Pronto
PS> flutter create --platforms=windows .
```

- El `--platforms=windows` crea **solo** la plataforma Windows.
- El **punto final** (`.`) significa «el directorio actual».
- `flutter create` sobre un proyecto existente **añade** las carpetas de
  plataforma y **no toca tu `lib/`** ni tu lógica de negocio. Sí puede regenerar
  ficheros de andamiaje como `pubspec.yaml`, `analysis_options.yaml`,
  `.gitignore`, `.metadata` o `README.md`.

> **IMPORTANTE — revisa que no pisa tus archivos.** Antes de continuar:
>
> - Si usas Git, haz `git status` y revisa el *diff* de `pubspec.yaml`,
>   `analysis_options.yaml` y `.gitignore`. Restaura con `git checkout -- <archivo>`
>   cualquiera que se haya sobrescrito con valores que no quieras perder.
> - Si **no** usas Git, haz una copia de seguridad de esos ficheros antes de
>   ejecutar el comando:
>
>   ```powershell
>   PS> Copy-Item pubspec.yaml, analysis_options.yaml, .gitignore .\_backup\ -Force
>   ```
>
> Tras el comando deberías ver una carpeta nueva `windows\` con el *runner*.

---

## 3) Descargar las dependencias

```powershell
PS> flutter pub get
```

Si alguna versión del `pubspec.yaml` no resuelve (conflictos de *constraints*):

```powershell
PS> flutter pub upgrade --major-versions
```

> **Nota:** las versiones fijadas en `pubspec.yaml` son **orientativas**
> (verificadas a mediados de 2026). Es esperable que con el tiempo haya que
> subirlas. `flutter pub upgrade --major-versions` actualiza los *constraints* a
> las últimas compatibles y reescribe el `pubspec.yaml`; revisa el resultado y
> haz `flutter pub get` de nuevo si fuera necesario.

---

## 4) Compilar whisper.dll y descargar el modelo

El motor de voz (whisper.cpp) y el modelo **no vienen** con el repositorio:
hay que compilar la DLL nativa y descargar el modelo aparte.

Sigue la guía dedicada: **[BUILD_WHISPER.md](./BUILD_WHISPER.md)**.

Cuando termines, recuerda **dos copias** clave:

1. **Las DLLs nativas** (`whisper.dll` y las que dependa, p. ej. `ggml*.dll`)
   van junto al ejecutable, en la carpeta `Release` del *runner*:

   ```text
   C:\ruta\a\Pronto\build\windows\x64\runner\Release\
   ```

   > Esa carpeta `Release\` se genera al compilar (paso 5). Si aún no existe,
   > compila primero y copia las DLLs después. Para depurar (`Debug`) la ruta
   > equivalente es `...\runner\Debug\`.

2. **El modelo** (por defecto `ggml-small.bin`) va en la **carpeta de datos**
   de la app, dentro de un subdirectorio `models\`. La app la crea en el primer
   arranque; su ruta típica es:

   ```text
   %APPDATA%\Pronto\Pronto\models\ggml-small.bin
   ```

   > Esa es la ruta de *Application Support* en Windows. Los segmentos
   > `Pronto\Pronto` derivan del CompanyName/ProductName del ejecutable; si dudas,
   > arranca la app una vez, abre la carpeta de datos
   > desde la propia app o busca con:
   >
   > ```powershell
   > PS> Get-ChildItem "$env:APPDATA" -Recurse -Filter "models" -Directory -ErrorAction SilentlyContinue | Where-Object FullName -like "*Pronto*"
   > ```
   >
   > Si prefieres otro modelo (p. ej. `ggml-large-v3-turbo.bin` para máxima
   > calidad), colócalo ahí y ajusta `defaultModelFile` en
   > `lib\src\core\config.dart`.

---

## 5) Ejecutar la app y los tests

### 5.1 Arrancar la app (escritorio Windows)

```powershell
PS> flutter run -d windows
```

La primera compilación tarda más (compila el *runner* C++). Cuando arranque
verás la ventana de Pronto. El **atajo global por defecto** para dictar es
**`Ctrl + Alt + Espacio`** (mantener pulsado mientras hablas, *push-to-talk*).

> En la sesión de `flutter run`: pulsa `r` para *hot reload*, `R` para *hot
> restart* y `q` para salir.

Para generar un ejecutable de *release*:

```powershell
PS> flutter build windows --release
```

El `.exe` queda en
`C:\ruta\a\Pronto\build\windows\x64\runner\Release\`
(recuerda que las DLLs de whisper y dependencias deben estar **en esa misma
carpeta**).

### 5.2 Ejecutar los tests

```powershell
PS> flutter test
```

---

## 6) Solución de problemas

### Permisos de micrófono
Si la app no captura audio:

1. Abre **Configuración → Privacidad y seguridad → Micrófono**.
2. Activa **«Acceso al micrófono»** y **«Permitir que las aplicaciones de
   escritorio accedan al micrófono»**.
3. Atajo rápido para abrir esa pantalla:

   ```powershell
   PS> Start-Process "ms-settings:privacy-microphone"
   ```

4. Reinicia Pronto después de cambiar el permiso.

### SmartScreen / «Windows protegió tu PC»
Al ejecutar un `.exe` propio sin firmar, Windows puede mostrar SmartScreen:

- Pulsa **«Más información» → «Ejecutar de todos modos»**.
- Es esperable en binarios sin firma digital; no significa que haya un problema.
- También puedes desbloquear el archivo:

  ```powershell
  PS> Unblock-File "C:\ruta\a\Pronto\build\windows\x64\runner\Release\pronto.exe"
  ```

### El atajo global no funciona / está en conflicto
El atajo por defecto es **`Ctrl + Alt + Espacio`**. Si no responde:

- Otra app puede tener registrado ese atajo (al registrarlo, la app avisa si ya
  está en uso). Cierra esa app o cambia el atajo en la configuración de Pronto.
- Algunos atajos los captura el sistema/teclados de fabricante; prueba a cambiar
  la combinación.
- Para que pueda **escribir en cualquier app** (inyección de texto global), no
  ejecutes la app de destino *como administrador* mientras Pronto corre sin
  privilegios elevados (una app elevada ignora la entrada de una no elevada).

### «No se encuentra la DLL» / la app no carga el motor de voz
Si al iniciar el dictado falla la carga de `whisper.dll`:

- Asegúrate de que `whisper.dll` (y sus dependencias `ggml*.dll`) están **en la
  misma carpeta que el `.exe`** (`...\runner\Release\` o `...\runner\Debug\`).
- Verifica que la **arquitectura coincide** (DLL de 64 bits para la build x64).
- Comprueba que el modelo existe en la carpeta de datos (ver paso 4); sin modelo
  la app arranca pero no transcribe.
- Si compilaste whisper.cpp con aceleración (p. ej. CUDA/Vulkan), copia también
  las DLLs de esa dependencia.

### Otros
- `flutter doctor` en rojo solo en Android/web/Chrome: **ignóralo**, no afecta a
  la build de Windows.
- Errores raros de compilación tras cambiar dependencias:

  ```powershell
  PS> flutter clean
  PS> flutter pub get
  PS> flutter run -d windows
  ```
