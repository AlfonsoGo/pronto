# Pronto 2 para Mac · prepara el .zip que Txonso le pasa al amigo que prueba en su Mac (sin GitHub).
#
#   powershell -ExecutionPolicy Bypass -File scripts\mac\empaquetar-para-amigo.ps1
#
# Mete en un .zip el código del último commit de esta rama (nada sin guardar, nada de %APPDATA%) y lo
# deja en el Escritorio. El amigo lo descomprime y, en la Terminal del Mac, ejecuta:
#   cd ~/Downloads/pronto-mac && bash scripts/mac/probar.sh
$ErrorActionPreference = 'Stop'
$raiz = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
Set-Location $raiz
$sha = (git rev-parse --short HEAD).Trim()
$rama = (git rev-parse --abbrev-ref HEAD).Trim()
if (git status --porcelain) { Write-Warning 'Hay cambios sin guardar en un commit: el .zip lleva solo lo del último commit.' }
$destino = Join-Path ([Environment]::GetFolderPath('Desktop')) "pronto-mac-$sha.zip"
git archive --format=zip --prefix=pronto-mac/ --add-virtual-file="pronto-mac/.commit:$sha ($rama)" -o $destino HEAD
$mb = [math]::Round((Get-Item $destino).Length / 1MB, 1)
Write-Host "Listo: $destino ($mb MB)."
Write-Host 'Mándaselo al amigo junto con docs/mac/PROBAR.md (va dentro del .zip).'
