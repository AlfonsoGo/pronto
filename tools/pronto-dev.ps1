# Compila Pronto como build LOCAL de PRUEBAS (canal "dev") y lo ejecuta.
# NO publica nada en GitHub. La app se marca con un badge naranja "DEV".
#
#   Uso:  powershell -ExecutionPolicy Bypass -File tools\pronto-dev.ps1
#
# Para probar en local antes de subir a producción (ver DESARROLLO.md).
$ErrorActionPreference = 'Stop'

$proj = Split-Path $PSScriptRoot -Parent
$flutter = (Get-Command flutter -ErrorAction Ignore).Source
if (-not $flutter) { $flutter = Join-Path $env:USERPROFILE 'dev\flutter\bin\flutter.bat' }
$cmakeBin = Join-Path $env:USERPROFILE 'dev\cmake\bin'
$env:Path = "$(Split-Path $flutter);$cmakeBin;" + $env:Path

$rel = Join-Path $proj 'build\windows\x64\runner\Release'
$parakeetSrc = Join-Path $proj 'native\parakeet'
$whisperModel = Join-Path $proj 'native\whisper\models\ggml-small.bin'

# Id de build legible (sha corto de git u hora) para verlo en el badge.
$sha = (& git -C $proj rev-parse --short HEAD 2>$null)
$buildId = if ($sha) { "$sha".Trim() } else { (Get-Date -Format 'HHmm') }

Write-Host "== Compilando Pronto DEV ($buildId) ==" -ForegroundColor Cyan
& $flutter build windows --release "--dart-define=PRONTO_CHANNEL=dev" "--dart-define=PRONTO_BUILD_ID=$buildId"
if ($LASTEXITCODE -ne 0) { throw "flutter build fallo" }

Write-Host "== Copiando modelos + runtime junto al exe ==" -ForegroundColor Cyan
$models = Join-Path $rel 'models'
$parakeetDst = Join-Path $models 'parakeet'
if (-not (Test-Path $parakeetDst)) { New-Item -ItemType Directory $parakeetDst -Force | Out-Null }
Copy-Item (Join-Path $parakeetSrc '*') $parakeetDst -Recurse -Force
if (Test-Path $whisperModel) { Copy-Item $whisperModel (Join-Path $models 'ggml-small.bin') -Force }
foreach ($d in 'msvcp140.dll', 'vcruntime140.dll', 'vcruntime140_1.dll', 'vcomp140.dll') {
    $src = Join-Path $env:WINDIR "System32\$d"
    if (Test-Path $src) { Copy-Item $src (Join-Path $rel $d) -Force }
}

# Instancia única: cerramos cualquier Pronto (prod o dev) para poder arrancar.
Get-Process pronto -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 600

Write-Host "== Lanzando Pronto DEV ==" -ForegroundColor Green
Start-Process (Join-Path $rel 'pronto.exe')
Write-Host "Pronto DEV en marcha (badge 'DEV - $buildId'). No se ha publicado nada."
Write-Host "Cuando lo apruebes, promociona a produccion con: tools\publicar.ps1 -Version X.Y.Z"
