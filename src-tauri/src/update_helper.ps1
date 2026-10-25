# Ayudante de actualizacion de Pronto 2. Lo lanza updater.rs (PowerShell oculto) justo antes de que
# Pronto se cierre. Hace, en este orden:
#   1) avisa de que ha arrancado (helper-started) para que Pronto sepa que puede cerrarse;
#   2) espera a que termine el proceso de Pronto (y lo fuerza pasados 45 s);
#   3) ejecuta el instalador en silencio y espera a que acabe;
#   4) escribe result.txt ("ok" o "error <codigo>") para que Pronto avise si algo fallo;
#   5) vuelve a abrir pronto.exe UNA sola vez (si ya hay uno en marcha en esa ruta, no abre otro).
# El instalador de Pronto 2 lleva `skipifsilent` en su [Run], asi que en modo silencioso NO abre la
# app por su cuenta: la unica reapertura es la del paso 5. Si el instalador falla se reabre igualmente
# la version que habia, para no dejar al usuario sin Pronto.
param(
    [Parameter(Mandatory = $true)][int]$ProntoPid,
    [Parameter(Mandatory = $true)][string]$Installer,
    [Parameter(Mandatory = $true)][string]$Exe,
    [Parameter(Mandatory = $true)][string]$WorkDir
)
$ErrorActionPreference = 'Continue'
$log = Join-Path $WorkDir 'update.log'
function Log([string]$m) {
    # -LiteralPath: con -FilePath, unos corchetes en la carpeta ("tmp [1]") se toman como comodin.
    try { ((Get-Date -Format o) + '  ' + $m) | Out-File -LiteralPath $log -Append -Encoding utf8 } catch { }
}

# 1) Senal de vida para Pronto.
try { Set-Content -LiteralPath (Join-Path $WorkDir 'helper-started') -Value $PID -Encoding ascii } catch { }
Log "ayudante iniciado; Pronto pid=$ProntoPid exe=$Exe instalador=$Installer"

# 2) Esperar a que Pronto termine (max. 45 s); si no, forzarlo.
try { Wait-Process -Id $ProntoPid -Timeout 45 -ErrorAction Stop } catch { }
if (Get-Process -Id $ProntoPid -ErrorAction SilentlyContinue) {
    Log 'Pronto seguia abierto; se fuerza el cierre'
    Stop-Process -Id $ProntoPid -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 2
}

# 3) Instalar en silencio.
$code = -1
try {
    Unblock-File -LiteralPath $Installer -ErrorAction SilentlyContinue
    $setupLog = Join-Path $WorkDir 'setup.log'
    $setupArgs = @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/NOCANCEL', '/SP-', ('/LOG="' + $setupLog + '"'))
    $p = Start-Process -FilePath $Installer -ArgumentList $setupArgs -PassThru -Wait
    $code = $p.ExitCode
} catch {
    Log "no se pudo ejecutar el instalador: $_"
}
Log "el instalador termino con codigo $code"

# 4) Resultado para el proximo arranque.
try {
    $res = if ($code -eq 0) { 'ok' } else { "error $code" }
    Set-Content -LiteralPath (Join-Path $WorkDir 'result.txt') -Value $res -Encoding ascii
} catch { }
Start-Sleep -Milliseconds 800

# 5) Reabrir Pronto una sola vez. Cuenta como "en marcha" un pronto.exe de esa ruta (o uno cuya ruta
#    no se pueda leer, por prudencia: mejor no abrir dos).
function Test-ProntoRunning {
    foreach ($proc in @(Get-Process -Name 'pronto' -ErrorAction SilentlyContinue)) {
        $path = $null
        try { $path = $proc.Path } catch { }
        if (-not $path -or ($path -ieq $Exe)) { return $true }
    }
    return $false
}
for ($i = 0; $i -lt 8; $i++) {
    if (Test-ProntoRunning) { Log 'Pronto en marcha'; break }
    if (Test-Path -LiteralPath $Exe) {
        Log "abriendo $Exe (intento $i)"
        # ProcessStartInfo y no el parametro WorkingDirectory de Start-Process, que toma los corchetes de la carpeta
        # ("C:\Programas [Alfonso]\Pronto") como comodin y no abre nada.
        try {
            $psi = New-Object System.Diagnostics.ProcessStartInfo
            $psi.FileName = $Exe
            $psi.WorkingDirectory = [System.IO.Path]::GetDirectoryName($Exe)
            $psi.UseShellExecute = $true
            [void][System.Diagnostics.Process]::Start($psi)
        } catch { Log "no se pudo abrir: $_" }
    } else {
        Log "no existe $Exe"
    }
    Start-Sleep -Milliseconds 1500
}
Log 'ayudante terminado'
