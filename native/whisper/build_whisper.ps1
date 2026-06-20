<#
.SYNOPSIS
    Compila whisper.cpp (GGML) como DLL compartida para Pronto en Windows.

.DESCRIPTION
    Clona ggml-org/whisper.cpp fijando un tag estable (v1.9.1), configura con
    CMake (BUILD_SHARED_LIBS=ON, CMAKE_BUILD_TYPE=Release), compila en Release y
    copia whisper.dll + ggml*.dll a native/whisper/bin/.

    Requisitos:
      - Visual Studio 2022 con la carga de trabajo "Desktop development with C++".
      - CMake en el PATH (lo incluye VS 2022 o instalalo aparte).
      - Git en el PATH.

    La GPU (Vulkan) viene COMENTADA: actívala solo si tienes el SDK de Vulkan
    instalado (ver parametro -Vulkan mas abajo).

.PARAMETER Tag
    Tag de whisper.cpp a compilar. Por defecto v1.9.1 (estable, 2026).

.PARAMETER Vulkan
    Si se indica, activa la aceleracion por GPU (-DGGML_VULKAN=ON). Requiere el
    SDK de Vulkan (https://vulkan.lunarg.com/). Por defecto: CPU (recomendado
    para máxima compatibilidad).

.PARAMETER Jobs
    Numero de trabajos de compilacion en paralelo. Por defecto: nucleos logicos.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File native\whisper\build_whisper.ps1

.EXAMPLE
    # Con aceleracion GPU (requiere SDK de Vulkan):
    powershell -ExecutionPolicy Bypass -File native\whisper\build_whisper.ps1 -Vulkan
#>

[CmdletBinding()]
param(
    [string]$Tag = 'v1.9.1',
    [switch]$Vulkan,
    [int]$Jobs = [Environment]::ProcessorCount
)

# Aborta a la primera de cambio; trata los errores nativos con cuidado.
$ErrorActionPreference = 'Stop'

function Write-Step  ([string]$m) { Write-Host "==> $m" -ForegroundColor Cyan }
function Write-Ok    ([string]$m) { Write-Host "    OK  $m" -ForegroundColor Green }
function Write-Warn2 ([string]$m) { Write-Host "    !!  $m" -ForegroundColor Yellow }
function Fail        ([string]$m) { Write-Host "ERROR: $m" -ForegroundColor Red; exit 1 }

# --- Rutas base (todo relativo a este script) ---------------------------------
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition  # native/whisper
$SrcDir    = Join-Path $ScriptDir 'whisper.cpp'                      # clon
$BuildDir  = Join-Path $SrcDir 'build'                              # build CMake
$BinDir    = Join-Path $ScriptDir 'bin'                            # salida DLLs

Write-Step "Pronto - compilacion de whisper.cpp ($Tag)"
Write-Host  "    Origen : $SrcDir"
Write-Host  "    Salida : $BinDir"
Write-Host  ""

# --- Comprobacion de herramientas ---------------------------------------------
Write-Step 'Comprobando herramientas (git, cmake)...'
$git   = Get-Command git   -ErrorAction SilentlyContinue
$cmake = Get-Command cmake -ErrorAction SilentlyContinue
if (-not $git)   { Fail 'No se encontro "git" en el PATH. Instala Git: https://git-scm.com/download/win' }
if (-not $cmake) { Fail 'No se encontro "cmake" en el PATH. Instala CMake o el componente C++ de Visual Studio 2022.' }
Write-Ok "git   -> $($git.Source)"
Write-Ok "cmake -> $($cmake.Source)"

# Aviso sobre el compilador (CMake elegira el generador por defecto; en Windows
# con VS 2022 instalado sera "Visual Studio 17 2022"). No abortamos si no se
# detecta, pero avisamos.
if (-not (Get-Command cl.exe -ErrorAction SilentlyContinue)) {
    Write-Warn2 'No se detecto cl.exe en el PATH. CMake intentara localizar Visual Studio 2022 automaticamente.'
    Write-Warn2 'Si la configuracion falla, ejecuta este script desde "x64 Native Tools Command Prompt for VS 2022".'
}
Write-Host ''

# --- Clonado / actualizacion del repositorio ----------------------------------
if (Test-Path $SrcDir) {
    Write-Step "El repositorio ya existe. Actualizando al tag $Tag..."
    & git -C $SrcDir fetch --tags --force
    if ($LASTEXITCODE -ne 0) { Fail 'Fallo "git fetch".' }
    & git -C $SrcDir checkout --force $Tag
    if ($LASTEXITCODE -ne 0) { Fail "No se pudo hacer checkout del tag $Tag." }
} else {
    Write-Step "Clonando ggml-org/whisper.cpp (tag $Tag)..."
    & git clone --depth 1 --branch $Tag https://github.com/ggml-org/whisper.cpp.git $SrcDir
    if ($LASTEXITCODE -ne 0) {
        Write-Warn2 "No se pudo clonar el tag $Tag directamente. Clonando rama por defecto y haciendo checkout..."
        & git clone https://github.com/ggml-org/whisper.cpp.git $SrcDir
        if ($LASTEXITCODE -ne 0) { Fail 'Fallo el clonado del repositorio.' }
        & git -C $SrcDir fetch --tags
        & git -C $SrcDir checkout --force $Tag
        if ($LASTEXITCODE -ne 0) { Fail "No se pudo hacer checkout del tag $Tag tras el clonado." }
    }
}
Write-Ok 'Codigo fuente listo.'
Write-Host ''

# --- Configuracion CMake ------------------------------------------------------
Write-Step 'Configurando con CMake...'

# Opciones base: DLL compartida (whisper.dll + ggml*.dll) en Release.
$cmakeArgs = @(
    '-B', $BuildDir,
    '-S', $SrcDir,
    '-DBUILD_SHARED_LIBS=ON',
    '-DCMAKE_BUILD_TYPE=Release',
    # No compilamos ejemplos/tests: solo necesitamos las librerias.
    '-DWHISPER_BUILD_EXAMPLES=OFF',
    '-DWHISPER_BUILD_TESTS=OFF'
)

# --- ACELERACION GPU (Vulkan) -------------------------------------------------
# Por defecto desactivada (CPU). Para activarla:
#   1) Instala el SDK de Vulkan: https://vulkan.lunarg.com/
#   2) Ejecuta el script con -Vulkan
# Linea equivalente (comentada) si lo prefieres fijo:
#   $cmakeArgs += '-DGGML_VULKAN=ON'
if ($Vulkan) {
    Write-Warn2 'Aceleracion GPU activada (-DGGML_VULKAN=ON). Requiere el SDK de Vulkan instalado.'
    $cmakeArgs += '-DGGML_VULKAN=ON'
} else {
    Write-Host  '    (GPU desactivada: compilacion CPU. Usa -Vulkan para activar GGML_VULKAN.)'
}

& cmake @cmakeArgs
if ($LASTEXITCODE -ne 0) { Fail 'Fallo la configuracion de CMake.' }
Write-Ok 'Configuracion completada.'
Write-Host ''

# --- Compilacion --------------------------------------------------------------
Write-Step "Compilando en Release (jobs: $Jobs)..."
& cmake --build $BuildDir --config Release -j $Jobs
if ($LASTEXITCODE -ne 0) { Fail 'Fallo la compilacion.' }
Write-Ok 'Compilacion completada.'
Write-Host ''

# --- Copia de las DLLs --------------------------------------------------------
Write-Step "Copiando DLLs a $BinDir ..."
if (-not (Test-Path $BinDir)) { New-Item -ItemType Directory -Path $BinDir -Force | Out-Null }

# Con MSVC + multi-config, los binarios suelen quedar en build/bin/Release.
# Buscamos en varias rutas conocidas por robustez frente a cambios de layout.
$searchRoots = @(
    (Join-Path $BuildDir 'bin\Release'),
    (Join-Path $BuildDir 'bin'),
    (Join-Path $BuildDir 'Release'),
    $BuildDir
) | Where-Object { Test-Path $_ }

# whisper.dll + todas las ggml*.dll (ggml.dll, ggml-base.dll, ggml-cpu.dll, etc.)
$dlls = @()
foreach ($root in $searchRoots) {
    $dlls += Get-ChildItem -Path $root -Recurse -Filter '*.dll' -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -like 'whisper*.dll' -or $_.Name -like 'ggml*.dll' }
}
# Deduplica por nombre quedandote con la primera coincidencia (rutas mas especificas primero).
$dlls = $dlls | Sort-Object Name -Unique

if (-not $dlls -or $dlls.Count -eq 0) {
    Fail "No se encontro ninguna whisper.dll/ggml*.dll en $BuildDir. Revisa la salida de la compilacion."
}

foreach ($dll in $dlls) {
    Copy-Item -Path $dll.FullName -Destination $BinDir -Force
    Write-Ok "copiada $($dll.Name)"
}
Write-Host ''

Write-Step 'Listo. DLLs disponibles en:'
Write-Host  "    $BinDir"
Get-ChildItem $BinDir -Filter '*.dll' | ForEach-Object { Write-Host "      - $($_.Name)" }
Write-Host ''
Write-Host 'SIGUIENTE PASO:' -ForegroundColor Cyan
Write-Host '  Copia estas DLLs junto al ejecutable de la app, p.ej.:'
Write-Host '    build\windows\x64\runner\Release\'
Write-Host '  (ver docs\BUILD_WHISPER.md para automatizarlo en CMakeLists).'
Write-Host ''
Write-Host '  Descarga un modelo con: native\whisper\download_model.ps1 -Model small'
