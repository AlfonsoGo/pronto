# Reconstruye el instalador de Pronto 2 (Windows) de forma reproducible.
#   1) cargo build --release en src-tauri (salvo -SkipBuild)
#   2) reune en installer\stage SOLO lo necesario (pronto.exe + DLL que pida en ejecucion)
#   3) comprueba las dependencias reales del exe (tabla de importaciones)
#   4) anonimiza el nombre de usuario embebido en rutas de build
#   5) empaqueta con Inno Setup -> installer\dist\Pronto-Setup-<version>.exe
#
# Uso:
#   powershell -ExecutionPolicy Bypass -File installer\build_installer.ps1
#   ... -SkipBuild                       (usa el pronto.exe que ya haya en target\release)
#   ... -TargetDir C:\ruta\target        (carpeta target de cargo; por defecto src-tauri\target)
#   ... -TestSandbox -TestDir C:\ruta    (variante de PRUEBA: otro AppId/nombre/carpeta)
#   ... -OutDir C:\ruta                  (carpeta de salida; por defecto installer\dist)
#
# La version sale de src-tauri\tauri.conf.json ("version").
param(
    [switch]$SkipBuild,
    [string]$TargetDir,
    [switch]$TestSandbox,
    [string]$TestDir,
    [string]$OutDir
)
$ErrorActionPreference = 'Stop'

$proj  = Split-Path $PSScriptRoot -Parent
$tauri = Join-Path $proj 'src-tauri'
$conf  = Get-Content (Join-Path $tauri 'tauri.conf.json') -Raw | ConvertFrom-Json
$ver   = [string]$conf.version
if ($ver -notmatch '^\d+\.\d+\.\d+$') { throw "Version invalida en tauri.conf.json: '$ver'" }
Write-Host "Version: $ver" -ForegroundColor Cyan
if ($TestSandbox -and -not $TestDir) { throw "-TestSandbox necesita -TestDir <carpeta de pruebas>" }

# --- Inno Setup: PATH, %LOCALAPPDATA%\Programs\Inno Setup N, Program Files* ---
function Find-Iscc {
    $c = Get-Command ISCC -ErrorAction Ignore
    if ($c) { return $c.Source }
    $cands = @()
    if ($env:LOCALAPPDATA) {
        $cands += Get-ChildItem (Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup *\ISCC.exe') -ErrorAction Ignore |
            Sort-Object FullName -Descending | ForEach-Object FullName
    }
    foreach ($pf in @($env:ProgramFiles, ${env:ProgramFiles(x86)})) {
        if ($pf) {
            $cands += Get-ChildItem (Join-Path $pf 'Inno Setup *\ISCC.exe') -ErrorAction Ignore |
                Sort-Object FullName -Descending | ForEach-Object FullName
        }
    }
    return ($cands | Select-Object -First 1)
}
$iscc = Find-Iscc
if (-not $iscc) { throw "No encuentro ISCC.exe (Inno Setup). Instalalo o ponlo en el PATH." }
Write-Host "ISCC: $iscc" -ForegroundColor DarkGray

if (-not $TargetDir) { $TargetDir = Join-Path $tauri 'target' }
$relDir = Join-Path $TargetDir 'release'

# --- 1) Compilar ---
if ($SkipBuild) {
    Write-Host "== 1/5  compilacion omitida (-SkipBuild) ==" -ForegroundColor Cyan
}
else {
    Write-Host "== 1/5  cargo build --release ==" -ForegroundColor Cyan
    Push-Location $tauri
    try {
        & cargo build --release --target-dir $TargetDir
        if ($LASTEXITCODE -ne 0) { throw "cargo build fallo" }
    }
    finally { Pop-Location }
}
$exeSrc = Join-Path $relDir 'pronto.exe'
if (-not (Test-Path $exeSrc)) { throw "No existe $exeSrc (compila sin -SkipBuild)." }

# --- 2) Stage ---
Write-Host "== 2/5  stage (solo lo necesario) ==" -ForegroundColor Cyan
$stage = Join-Path $PSScriptRoot 'stage'
if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
New-Item -ItemType Directory $stage | Out-Null
Copy-Item $exeSrc (Join-Path $stage 'pronto.exe') -Force
# DLL junto al exe (hoy no hay: sherpa-onnx, SQLite y WebView2Loader van enlazados estaticamente).
# Si algun dia aparecen, se incluyen aqui; el paso 3 comprueba que no falte ninguna.
Get-ChildItem $relDir -Filter *.dll -File -ErrorAction Ignore | ForEach-Object {
    Copy-Item $_.FullName (Join-Path $stage $_.Name) -Force
    Write-Host "   + $($_.Name)"
}

# --- 3) Dependencias reales (tabla de importaciones del PE, normales y delay-load) ---
Write-Host "== 3/5  comprobar dependencias del exe ==" -ForegroundColor Cyan
function Get-PeImports([string]$path) {
    $d = [IO.File]::ReadAllBytes($path)
    $pe = [BitConverter]::ToInt32($d, 0x3c)
    $nsec = [BitConverter]::ToUInt16($d, $pe + 6)
    $opt = $pe + 24
    $dd = $opt + $(if ([BitConverter]::ToUInt16($d, $opt) -eq 0x20b) { 112 } else { 96 })
    $secs = $opt + [BitConverter]::ToUInt16($d, $pe + 20)
    $S = for ($i = 0; $i -lt $nsec; $i++) {
        $o = $secs + $i * 40
        [pscustomobject]@{ VS = [BitConverter]::ToUInt32($d, $o + 8); VA = [BitConverter]::ToUInt32($d, $o + 12);
                           RS = [BitConverter]::ToUInt32($d, $o + 16); RO = [BitConverter]::ToUInt32($d, $o + 20) }
    }
    function R2O([uint32]$rva) {
        foreach ($s in $S) { if ($rva -ge $s.VA -and $rva -lt ($s.VA + [Math]::Max($s.VS, $s.RS))) { return [int]($rva - $s.VA + $s.RO) } }
        return -1
    }
    $names = New-Object System.Collections.Generic.HashSet[string]
    foreach ($kind in 'imp', 'delay') {
        $idx = if ($kind -eq 'imp') { 1 } else { 13 }
        $rva = [BitConverter]::ToUInt32($d, $dd + $idx * 8)
        if ($rva -eq 0) { continue }
        $o = R2O $rva
        while ($o -ge 0) {
            if ($kind -eq 'imp') { $nm = [BitConverter]::ToUInt32($d, $o + 12); $step = 20 }
            else { $nm = [BitConverter]::ToUInt32($d, $o + 4); $step = 32 }
            if ($nm -eq 0) { break }
            $n = R2O $nm
            $e = [Array]::IndexOf($d, [byte]0, $n)
            [void]$names.Add([Text.Encoding]::ASCII.GetString($d, $n, $e - $n).ToLower())
            $o += $step
        }
    }
    return $names
}
$sys32 = Join-Path $env:WINDIR 'System32'
$missing = @()
foreach ($f in Get-ChildItem $stage -Filter *.exe -File) {
    foreach ($dll in (Get-PeImports $f.FullName | Sort-Object)) {
        if ($dll -like 'api-ms-win-*' -or $dll -like 'ext-ms-win-*') { continue }
        if (Test-Path (Join-Path $stage $dll)) { continue }
        if (Test-Path (Join-Path $sys32 $dll)) { continue }
        $missing += $dll
    }
}
if ($missing.Count -gt 0) {
    throw ("El exe importa DLL que no estan en el stage ni en System32: " + ($missing -join ', ') +
           ". Copialas a $relDir (o ajusta el script) y repite.")
}
Write-Host "   OK: todas las importaciones son del sistema o estan en el stage" -ForegroundColor DarkGray

# --- 4) Scrub de privacidad: usuario -> mascara de la MISMA longitud ---
Write-Host "== 4/5  scrub de privacidad (usuario en rutas de build) ==" -ForegroundColor Cyan
$u = $env:USERNAME
if ($u -and $u.Length -ge 2) {
    $mask = if ($u.Length -ge 6) { 'public'.PadRight($u.Length, '_').Substring(0, $u.Length) } else { 'x' * $u.Length }
    $latin1 = [Text.Encoding]::GetEncoding(28591)   # 1 byte <-> 1 char, sin perdidas
    Get-ChildItem $stage -Include *.exe, *.dll -Recurse -File | ForEach-Object {
        $txt = $latin1.GetString([IO.File]::ReadAllBytes($_.FullName))
        $n = ([regex]::Matches($txt, [regex]::Escape($u))).Count
        if ($n -gt 0) {
            [IO.File]::WriteAllBytes($_.FullName, $latin1.GetBytes($txt.Replace($u, $mask)))
        }
        Write-Host ("   {0,-14} {1} reemplazos" -f $_.Name, $n)
    }
}

# --- 5) Inno Setup ---
Write-Host "== 5/5  empaquetar instalador (Inno Setup) ==" -ForegroundColor Cyan
if (-not $OutDir) { $OutDir = Join-Path $PSScriptRoot 'dist' }
New-Item -ItemType Directory $OutDir -Force | Out-Null
$isccArgs = @('/Qp', "/DAppVersion=$ver", "/DStageDir=$stage", "/DOutDir=$OutDir")
if ($TestSandbox) { $isccArgs += @('/DTestSandbox=1', "/DTestDir=$TestDir") }
$isccArgs += (Join-Path $PSScriptRoot 'pronto2.iss')
& $iscc @isccArgs
if ($LASTEXITCODE -ne 0) { throw "ISCC fallo" }

$name = if ($TestSandbox) { "ProntoTest-Setup-$ver.exe" } else { "Pronto-Setup-$ver.exe" }
$out = Join-Path $OutDir $name
Write-Host ("LISTO -> $out  (" + [math]::Round((Get-Item $out).Length / 1MB, 1) + " MB)") -ForegroundColor Green
