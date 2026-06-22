# Reconstruye el instalador de Pronto para Windows, de forma reproducible.
#   - Compila el Release de Flutter
#   - Copia el modelo Whisper junto al ejecutable
#   - ANONIMIZA tu nombre de usuario en los binarios (rutas de build embebidas)
#   - Empaqueta el instalador .exe con Inno Setup
#
# Uso:  powershell -ExecutionPolicy Bypass -File installer\build_installer.ps1
$ErrorActionPreference = 'Stop'

# --- Rutas (ajusta si cambian) ---
$proj = Split-Path $PSScriptRoot -Parent
# Version tomada del pubspec ("0.1.0+1" -> "0.1.0"): el instalador y su nombre de
# fichero suben solos al cambiar la version del proyecto.
$pubspec = Get-Content (Join-Path $proj 'pubspec.yaml') -Raw
$ver = if ($pubspec -match '(?m)^\s*version:\s*([0-9]+\.[0-9]+\.[0-9]+)') { $Matches[1] } else { '0.1.0' }
Write-Host "Version: $ver" -ForegroundColor Cyan
# Detecta herramientas sin rutas personales (PATH primero; si no, ubicaciones por defecto)
$flutter = (Get-Command flutter -ErrorAction Ignore).Source
if (-not $flutter) { $flutter = Join-Path $env:USERPROFILE 'dev\flutter\bin\flutter.bat' }
$iscc = (Get-Command ISCC -ErrorAction Ignore).Source
if (-not $iscc) { $iscc = Join-Path $env:LOCALAPPDATA 'InnoSetup6\ISCC.exe' }
$cmakeBin = Join-Path $env:USERPROFILE 'dev\cmake\bin'
$rel   = Join-Path $proj 'build\windows\x64\runner\Release'
$model = Join-Path $proj 'native\whisper\models\ggml-small.bin'

$env:Path = "$(Split-Path $flutter);$cmakeBin;" + $env:Path

Write-Host "== 1/4  flutter build windows --release ==" -ForegroundColor Cyan
& $flutter build windows --release
if ($LASTEXITCODE -ne 0) { throw "flutter build fallo" }

Write-Host "== 2/4  copiar modelo junto al exe ==" -ForegroundColor Cyan
$models = Join-Path $rel 'models'
if (-not (Test-Path $models)) { New-Item -ItemType Directory $models | Out-Null }
Copy-Item $model (Join-Path $models 'ggml-small.bin') -Force

# Runtime de Visual C++ junto al .exe: sin esto la app instala pero NO arranca
# en un PC limpio (sin VS / sin el redistribuible). App-local = sin admin.
# Incluye vcomp140.dll (OpenMP): ggml-cpu.dll/ggml-base.dll dependen de ella;
# sin ella whisper NO carga en un PC limpio.
# ponytail: copiadas de System32 (presentes en la maquina de build); si algun
# dia compilas en una sin ellas, apunta a VC\Redist\MSVC\*\x64\Microsoft.VC*.CRT.
foreach ($d in 'msvcp140.dll','vcruntime140.dll','vcruntime140_1.dll','vcomp140.dll') {
    $src = Join-Path $env:WINDIR "System32\$d"
    if (Test-Path $src) { Copy-Item $src (Join-Path $rel $d) -Force; Write-Host "   + $d" }
    else { throw "Falta $d en System32: el instalador no arrancaria en PCs limpios" }
}

Write-Host "== 3/4  scrub de privacidad (usuario -> mascara) ==" -ForegroundColor Cyan
# El AOT de Dart y las DLLs de whisper embeben rutas de build con tu usuario.
# Las reemplazamos byte a byte por una mascara de la MISMA longitud (sin recompilar).
$u = $env:USERNAME
$mask = if ($u.Length -ge 6) { 'public'.PadRight($u.Length, '_').Substring(0, $u.Length) } else { 'x' * $u.Length }
$pat = [System.Text.Encoding]::ASCII.GetBytes($u)
$rep = [System.Text.Encoding]::ASCII.GetBytes($mask)
$targets = @('data\app.so','whisper.dll','ggml.dll','ggml-base.dll','ggml-cpu.dll') | ForEach-Object { Join-Path $rel $_ }
foreach ($f in $targets) {
    if (-not (Test-Path $f)) { continue }
    $d = [System.IO.File]::ReadAllBytes($f); $n = 0
    for ($i = 0; $i -le $d.Length - $pat.Length; $i++) {
        $ok = $true
        for ($j = 0; $j -lt $pat.Length; $j++) { if ($d[$i+$j] -ne $pat[$j]) { $ok = $false; break } }
        if ($ok) { for ($j = 0; $j -lt $rep.Length; $j++) { $d[$i+$j] = $rep[$j] }; $n++; $i += $pat.Length - 1 }
    }
    if ($n -gt 0) { [System.IO.File]::WriteAllBytes($f, $d) }
    Write-Host ("   {0,-14} {1} reemplazos" -f [IO.Path]::GetFileName($f), $n)
}

Write-Host "== 4/4  empaquetar instalador (Inno Setup) ==" -ForegroundColor Cyan
& $iscc /Qp "/DAppVersion=$ver" (Join-Path $PSScriptRoot 'pronto.iss')
if ($LASTEXITCODE -ne 0) { throw "ISCC fallo" }

$out = Join-Path $proj "dist\Pronto-Setup-$ver.exe"
Write-Host ("LISTO -> $out  (" + [math]::Round((Get-Item $out).Length/1MB,1) + " MB)") -ForegroundColor Green