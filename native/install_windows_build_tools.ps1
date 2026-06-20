# ============================================================================
# Pronto — instalación del toolchain de compilación de Windows (REQUIERE ADMIN)
# ----------------------------------------------------------------------------
# Hace dos cosas que necesitan privilegios de administrador:
#   1) Activa el "Modo de desarrollador" de Windows (symlinks para plugins).
#   2) Instala Visual Studio 2022 Build Tools con la carga "Desktop
#      development with C++" (MSVC + Windows SDK + CMake) en modo silencioso.
#
# Ejecútalo como administrador (clic derecho -> "Ejecutar con PowerShell"); si
# no, se relanza solo pidiendo elevación (UAC) vía INSTALAR-requisitos-Windows.cmd.
#
# Progreso/registro en: native\install_buildtools.log
# ============================================================================

$ErrorActionPreference = 'Stop'
$log = Join-Path $PSScriptRoot 'install_buildtools.log'

function Write-Log($msg) {
  $line = "[{0}] {1}" -f (Get-Date -Format 'HH:mm:ss'), $msg
  Add-Content -Path $log -Value $line -Encoding utf8
  Write-Host $line
}

# Reinicia el log
Set-Content -Path $log -Value "START install_windows_build_tools" -Encoding utf8

try {
  # --- 1) Modo de desarrollador --------------------------------------------
  Write-Log "Activando Modo de desarrollador (Developer Mode)..."
  $devKey = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\AppModelUnlock'
  if (-not (Test-Path $devKey)) { New-Item -Path $devKey -Force | Out-Null }
  New-ItemProperty -Path $devKey -Name 'AllowDevelopmentWithoutDevLicense' `
    -PropertyType DWord -Value 1 -Force | Out-Null
  Write-Log "Developer Mode = ON."

  # --- 2) ¿Ya está instalado VS con C++? -----------------------------------
  $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
  $hasCpp = $false
  if (Test-Path $vswhere) {
    $found = & $vswhere -products * `
      -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
      -property installationPath
    if ($found) { $hasCpp = $true; Write-Log "VC++ ya presente en: $found" }
  }

  if (-not $hasCpp) {
    # --- Descargar el bootstrapper de VS Build Tools 2022 ------------------
    $bootstrap = Join-Path $env:TEMP 'vs_BuildTools.exe'
    Write-Log "Descargando vs_BuildTools.exe..."
    Invoke-WebRequest -Uri 'https://aka.ms/vs/17/release/vs_BuildTools.exe' `
      -OutFile $bootstrap
    Write-Log "Descargado. Instalando carga C++ (silencioso, puede tardar 10-30 min)..."

    $args = @(
      '--quiet', '--wait', '--norestart', '--nocache',
      '--add', 'Microsoft.VisualStudio.Workload.VCTools',
      '--add', 'Microsoft.VisualStudio.Component.VC.Tools.x86.x64',
      '--add', 'Microsoft.VisualStudio.Component.Windows11SDK.22621',
      '--includeRecommended'
    )
    $p = Start-Process -FilePath $bootstrap -ArgumentList $args -Wait -PassThru
    Write-Log "Instalador VS terminó con código $($p.ExitCode)."
    # 3010 = éxito pero requiere reinicio; lo tratamos como OK.
    if ($p.ExitCode -ne 0 -and $p.ExitCode -ne 3010) {
      throw "La instalación de VS Build Tools falló (código $($p.ExitCode))."
    }
  }

  # --- 3) Verificación final ------------------------------------------------
  if (Test-Path $vswhere) {
    $cppPath = & $vswhere -products * `
      -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
      -property installationPath
    if ($cppPath) {
      Write-Log "OK: VC++ instalado en $cppPath"
      Write-Log "INSTALL_RESULT=SUCCESS"
    } else {
      Write-Log "AVISO: vswhere no encontró VC++ tras la instalación."
      Write-Log "INSTALL_RESULT=PARTIAL"
    }
  } else {
    Write-Log "INSTALL_RESULT=SUCCESS_NO_VSWHERE"
  }
}
catch {
  Write-Log "ERROR: $($_.Exception.Message)"
  Write-Log "INSTALL_RESULT=ERROR"
}
finally {
  Write-Log "FIN."
}
