# Abre la version de PRUEBAS de Pronto (la de la rama, compilada en release) sobre una COPIA de tus datos.
#   - Tus datos reales (%APPDATA%\Pronto2) no se tocan: la primera vez se copian a la carpeta de pruebas.
#   - No toca el arranque con Windows ni el Pronto instalado. Los modelos de voz se leen de %APPDATA%.
#   - Usa el mismo atajo (Win + Alt): hay que cerrar antes el Pronto instalado y Wispr Flow.
#
# Uso (doble clic en «Probar Pronto (pruebas).cmd», o):
#   powershell -ExecutionPolicy Bypass -File scripts\probar-pronto.ps1
#   ... -Reiniciar    vuelve a copiar tus datos reales encima de los de prueba
param([switch]$Reiniciar)
$ErrorActionPreference = 'Stop'

$base    = Join-Path $env:USERPROFILE 'dev\pronto-pruebas'
$datos   = Join-Path $base 'datos'
$exe     = Join-Path $base 'pronto.exe'
$build   = Join-Path $env:USERPROFILE 'dev\pt\release\pronto.exe'
$real    = Join-Path $env:APPDATA 'Pronto2'
$instalado = Join-Path $env:LOCALAPPDATA 'Programs\Pronto\pronto.exe'

function Pronto-En-Marcha {
    Get-Process pronto -ErrorAction SilentlyContinue
}

Write-Host ''
Write-Host '  Pronto - version de PRUEBAS' -ForegroundColor Cyan
Write-Host "  Datos de prueba: $datos" -ForegroundColor DarkGray
Write-Host ''

# 1) Ningun Pronto abierto (comparten el atajo y la instancia unica).
while (Pronto-En-Marcha) {
    $p = (Pronto-En-Marcha | Select-Object -First 1).Path
    if ($p -and ($p -ieq $exe)) { Write-Host '  La version de pruebas ya esta abierta (mira la bandeja).' -ForegroundColor Yellow; exit 0 }
    Write-Host '  Hay un Pronto abierto. Cierralo desde la bandeja (clic derecho en el icono > Salir)' -ForegroundColor Yellow
    Read-Host '  y pulsa Enter para seguir'
}
if (Get-Process 'Wispr Flow', 'WisprFlow' -ErrorAction SilentlyContinue) {
    Write-Host '  Wispr Flow esta abierto y usa el mismo atajo: cierralo para probar.' -ForegroundColor Yellow
}

# 2) El exe de pruebas: el ultimo compilado.
if (-not (Test-Path $build)) { throw "No hay version compilada en $build. Pide a Claude que la compile." }
New-Item -ItemType Directory -Force $base | Out-Null
if (-not (Test-Path $exe) -or (Get-Item $build).LastWriteTime -gt (Get-Item $exe).LastWriteTime) {
    Copy-Item $build $exe -Force
    Write-Host "  Version de pruebas actualizada ($((Get-Item $exe).LastWriteTime))." -ForegroundColor Green
}

# 3) Copia de tus datos (solo la primera vez, o con -Reiniciar). Con Pronto cerrado, la base y su -wal coinciden.
if ($Reiniciar -and (Test-Path $datos)) {
    $viejo = "$datos-$(Get-Date -Format yyyyMMdd-HHmmss)"
    Rename-Item $datos $viejo
    Write-Host "  Datos de prueba anteriores apartados en $viejo" -ForegroundColor DarkGray
}
if (-not (Test-Path (Join-Path $datos 'pronto2.db'))) {
    New-Item -ItemType Directory -Force $datos | Out-Null
    foreach ($f in 'pronto2.db', 'pronto2.db-wal', 'pronto2.db-shm') {
        $src = Join-Path $real $f
        if (Test-Path $src) { Copy-Item $src (Join-Path $datos $f) -Force }
    }
    foreach ($d in 'audio', 'voz') {
        $src = Join-Path $real $d
        if (Test-Path $src) { Copy-Item $src (Join-Path $datos $d) -Recurse -Force }
    }
    Write-Host '  Copiados tu historial, audio, diccionario y ajustes a la carpeta de pruebas.' -ForegroundColor Green
}

# 4) Abrir la de pruebas apuntando a la copia.
$env:PRONTO_DATOS = $datos
Start-Process -FilePath $exe -WorkingDirectory $base
Write-Host ''
Write-Host '  Abierta. Para volver a tu Pronto de siempre: cierra esta desde la bandeja (Salir)' -ForegroundColor Cyan
Write-Host '  y abre Pronto desde el menu Inicio.' -ForegroundColor Cyan
Write-Host ''
