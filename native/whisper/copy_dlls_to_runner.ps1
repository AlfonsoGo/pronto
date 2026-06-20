# Copia las DLLs nativas de whisper (whisper.dll + ggml*.dll) al directorio del
# runner de Flutter para Windows. Útil para ejecuciones de desarrollo
# (`flutter run -d windows`), donde quieres las DLLs junto al .exe rápidamente.
#
# Para builds de Release / instalador, la regla install() añadida a
# windows/CMakeLists.txt ya incluye estas DLLs en el bundle automáticamente
# (ver docs/BUILD_WHISPER.md). Este script es el atajo manual para desarrollo.
#
# Uso:
#   ./native/whisper/copy_dlls_to_runner.ps1            # Debug por defecto
#   ./native/whisper/copy_dlls_to_runner.ps1 -Config Release

param(
  [ValidateSet('Debug', 'Release')]
  [string]$Config = 'Debug'
)

$ErrorActionPreference = 'Stop'

# .../native/whisper -> .../native -> .../Pronto
$projectRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$src = Join-Path $PSScriptRoot 'bin'
$dst = Join-Path $projectRoot "build\windows\x64\runner\$Config"

if (-not (Test-Path $src)) {
  throw "No existe '$src'. Compila primero whisper.dll con build_whisper.ps1."
}

$dlls = Get-ChildItem -Path (Join-Path $src '*.dll') -ErrorAction SilentlyContinue
if (-not $dlls) {
  throw "No hay archivos .dll en '$src'. ¿Falló la compilación de whisper?"
}

if (-not (Test-Path $dst)) {
  New-Item -ItemType Directory -Force -Path $dst | Out-Null
}

Copy-Item -Path (Join-Path $src '*.dll') -Destination $dst -Force
Write-Host "OK: copiadas $($dlls.Count) DLL(s) a '$dst'." -ForegroundColor Green
$dlls | ForEach-Object { Write-Host "  - $($_.Name)" }
