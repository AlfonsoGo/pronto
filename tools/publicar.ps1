# Promueve a PRODUCCIÓN: sube la versión, compila el instalador (canal prod,
# SIN badge DEV), commitea y publica la release en GitHub.
# Ejecutar SOLO cuando hayas probado en local con tools\pronto-dev.ps1 y dado el OK.
#
#   Uso:  powershell -ExecutionPolicy Bypass -File tools\publicar.ps1 -Version 0.6.0 [-Notas ruta\notas.md]
#
# Antes de ejecutar: el código aprobado debe estar en la rama main
# (merge de dev -> main). Ver DESARROLLO.md.
param(
    [Parameter(Mandatory = $true)][string]$Version,
    [string]$Notas = ''
)
$ErrorActionPreference = 'Stop'
$proj = Split-Path $PSScriptRoot -Parent
Set-Location $proj

# 1) Sube la versión en pubspec (x.y.z+N -> $Version, build +1).
$pubspecPath = Join-Path $proj 'pubspec.yaml'
$pub = Get-Content $pubspecPath -Raw
if ($pub -notmatch '(?m)^\s*version:\s*([0-9]+\.[0-9]+\.[0-9]+)\+([0-9]+)') {
    throw "No pude leer la version del pubspec"
}
$build = [int]$Matches[2] + 1
$pub = $pub -replace '(?m)^(\s*version:\s*)[0-9]+\.[0-9]+\.[0-9]+\+[0-9]+', "`${1}$Version+$build"
Set-Content $pubspecPath $pub -Encoding utf8 -NoNewline
Write-Host "Version -> $Version+$build" -ForegroundColor Cyan

# 2) Compila el instalador de PRODUCCIÓN (canal prod, sin dart-define dev).
& powershell -ExecutionPolicy Bypass -File (Join-Path $proj 'installer\build_installer.ps1')
if ($LASTEXITCODE -ne 0) { throw "build_installer fallo" }

# 3) Commit + push + release en GitHub.
$exe = Join-Path $proj "dist\Pronto-Setup-$Version.exe"
if (-not (Test-Path $exe)) { throw "No existe el instalador $exe" }
& git add -A
& git commit -m "Pronto $Version"
& git push origin main
$gh = (Get-Command gh -ErrorAction Ignore).Source
if (-not $gh) { $gh = Join-Path $env:LOCALAPPDATA 'gh\bin\gh.exe' }
if ($Notas -and (Test-Path $Notas)) {
    & $gh release create "v$Version" $exe --target main --title "Pronto $Version" --notes-file $Notas
}
else {
    & $gh release create "v$Version" $exe --target main --title "Pronto $Version" --notes "Pronto $Version"
}
Write-Host "PUBLICADO v$Version" -ForegroundColor Green
