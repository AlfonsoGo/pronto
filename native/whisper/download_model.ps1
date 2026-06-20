<#
.SYNOPSIS
    Descarga un modelo GGML de Whisper desde Hugging Face para Pronto.

.DESCRIPTION
    Descarga el modelo GGML (.bin) desde el repositorio oficial
    ggerganov/whisper.cpp en Hugging Face y lo guarda en la carpeta de datos de
    la app (para que Pronto lo encuentre) o en native/whisper/models/.

    IMPORTANTE: whisper.cpp usa el formato GGML (.bin), NO GGUF. No mezcles
    modelos de llama.cpp (GGUF) con whisper.cpp.

    Donde lo busca la app:
        getApplicationSupportDirectory()/models/<archivo>.bin
    En Windows eso resuelve normalmente a:
        %APPDATA%\<Organizacion>\<Producto>\models\
    (la Organizacion/Producto los define windows/runner/Runner.rc del proyecto
    Flutter; por defecto algo como %APPDATA%\Pronto\Pronto\models\).

    El archivo por defecto que carga la app es 'ggml-small.bin'
    (ver lib/src/core/config.dart -> AppConfig.defaultModelFile). Si descargas
    'large-v3-turbo', cambia ese valor o renombra/ajusta segun corresponda.

.PARAMETER Model
    Modelo a descargar: 'small' (equilibrio, por defecto) o 'large-v3-turbo'
    (maxima calidad, mas pesado y lento en CPU).

.PARAMETER Destination
    Destino: 'app' (carpeta de datos de la app, recomendado para uso real) o
    'repo' (native/whisper/models/, util para pruebas locales). Por defecto: app.

.PARAMETER Quantized
    Si se indica con large-v3-turbo, descarga la variante cuantizada q5_0
    (mucho mas ligera, ligera perdida de precision).

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File native\whisper\download_model.ps1 -Model small

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File native\whisper\download_model.ps1 -Model large-v3-turbo -Destination app

.EXAMPLE
    # Variante cuantizada (mas ligera):
    powershell -ExecutionPolicy Bypass -File native\whisper\download_model.ps1 -Model large-v3-turbo -Quantized
#>

[CmdletBinding()]
param(
    [ValidateSet('small', 'large-v3-turbo')]
    [string]$Model = 'small',

    [ValidateSet('app', 'repo')]
    [string]$Destination = 'app',

    [switch]$Quantized
)

$ErrorActionPreference = 'Stop'

function Write-Step ([string]$m) { Write-Host "==> $m" -ForegroundColor Cyan }
function Write-Ok   ([string]$m) { Write-Host "    OK  $m" -ForegroundColor Green }
function Fail       ([string]$m) { Write-Host "ERROR: $m" -ForegroundColor Red; exit 1 }

# --- Resolucion del nombre de archivo GGML ------------------------------------
# Repositorio oficial de modelos: https://huggingface.co/ggerganov/whisper.cpp
$baseUrl = 'https://huggingface.co/ggerganov/whisper.cpp/resolve/main'

switch ($Model) {
    'small' {
        # No hay variante cuantizada habitual que ofrezcamos aqui para small.
        $fileName = 'ggml-small.bin'
    }
    'large-v3-turbo' {
        $fileName = if ($Quantized) { 'ggml-large-v3-turbo-q5_0.bin' } else { 'ggml-large-v3-turbo.bin' }
    }
}

# ?download=true fuerza la descarga del binario crudo (no la pagina HTML).
$url = "$baseUrl/$fileName`?download=true"

# --- Resolucion de la carpeta destino -----------------------------------------
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition  # native/whisper

if ($Destination -eq 'repo') {
    $destDir = Join-Path $ScriptDir 'models'
} else {
    # Carpeta de datos de la app. Flutter (getApplicationSupportDirectory) usa en
    # Windows %APPDATA%\<Organizacion>\<Producto>. No podemos leer esos valores
    # del runner desde aqui sin parsear el proyecto, asi que pedimos confirmacion
    # del nombre de carpeta y por defecto usamos el patron del paquete "pronto".
    $orgProduct = 'Pronto\Pronto'
    $appData = $env:APPDATA
    if (-not $appData) { Fail 'No se pudo leer %APPDATA%.' }
    $destDir = Join-Path (Join-Path $appData $orgProduct) 'models'

    Write-Host ''
    Write-Host 'NOTA: la carpeta de datos exacta depende de la Organizacion/Producto' -ForegroundColor Yellow
    Write-Host '      definidos en windows/runner (Runner.rc / CMakeLists). Se usara:' -ForegroundColor Yellow
    Write-Host "      $destDir" -ForegroundColor Yellow
    Write-Host '      Si la app no encuentra el modelo, copia el .bin a la carpeta'   -ForegroundColor Yellow
    Write-Host '      "models" que la app crea al arrancar (mira los logs de inicio).' -ForegroundColor Yellow
    Write-Host ''
}

if (-not (Test-Path $destDir)) {
    New-Item -ItemType Directory -Path $destDir -Force | Out-Null
}

$destPath = Join-Path $destDir $fileName

Write-Step "Descargando modelo Whisper '$Model'"
Write-Host  "    Archivo : $fileName"
Write-Host  "    URL     : $url"
Write-Host  "    Destino : $destPath"
Write-Host  ''

if (Test-Path $destPath) {
    $sizeMb = [math]::Round((Get-Item $destPath).Length / 1MB, 1)
    Write-Host "    Ya existe ($sizeMb MB). Se sobrescribira." -ForegroundColor Yellow
}

# --- Descarga -----------------------------------------------------------------
# Usamos curl.exe si esta disponible (mejor para ficheros grandes y reanudacion);
# si no, Invoke-WebRequest.
$curl = Get-Command curl.exe -ErrorAction SilentlyContinue
try {
    if ($curl) {
        Write-Step 'Descargando con curl...'
        & curl.exe -L --fail --progress-bar -o $destPath $url
        if ($LASTEXITCODE -ne 0) { Fail "curl devolvio codigo $LASTEXITCODE." }
    } else {
        Write-Step 'Descargando con Invoke-WebRequest...'
        $ProgressPreference = 'Continue'
        Invoke-WebRequest -Uri $url -OutFile $destPath -UseBasicParsing
    }
} catch {
    Fail "Fallo la descarga: $($_.Exception.Message)"
}

if (-not (Test-Path $destPath) -or (Get-Item $destPath).Length -lt 1MB) {
    Fail 'La descarga parece incompleta o vacia. Reintenta.'
}

$finalMb = [math]::Round((Get-Item $destPath).Length / 1MB, 1)
Write-Ok "Modelo descargado: $destPath ($finalMb MB)"
Write-Host ''

# --- Recordatorio de configuracion --------------------------------------------
$defaultExpected = 'ggml-small.bin'
if ($fileName -ne $defaultExpected) {
    Write-Host 'RECUERDA:' -ForegroundColor Cyan
    Write-Host "  La app carga por defecto '$defaultExpected' (AppConfig.defaultModelFile en"
    Write-Host '  lib/src/core/config.dart). Para usar este modelo, actualiza ese valor a:'
    Write-Host "      static const String defaultModelFile = '$fileName';" -ForegroundColor White
    Write-Host ''
}
Write-Host 'Recuerda fijar el idioma a "es" (language: "es") al transcribir,' -ForegroundColor Cyan
Write-Host 'y que el audio debe ir a 16 kHz mono float32.' -ForegroundColor Cyan
