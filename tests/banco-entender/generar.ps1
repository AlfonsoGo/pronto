# Genera las 20 frases del banco (frases.txt) con la voz TTS de Windows en español, directo a fichero
# (16 kHz, 16 bits, mono): nunca suena por los altavoces. Salida: audio/01.wav … audio/20.wav, y cada
# texto de referencia en audio/01.txt … Lo llama tests/banco-entender.test.mjs si faltan.
param([string]$Salida = (Join-Path $PSScriptRoot 'audio'))
Add-Type -AssemblyName System.Speech
New-Item -ItemType Directory -Force $Salida | Out-Null
$frases = Get-Content -Encoding UTF8 (Join-Path $PSScriptRoot 'frases.txt') | Where-Object { $_.Trim() }
$fmt = New-Object System.Speech.AudioFormat.SpeechAudioFormatInfo(16000, [System.Speech.AudioFormat.AudioBitsPerSample]::Sixteen, [System.Speech.AudioFormat.AudioChannel]::Mono)
$i = 0
foreach ($f in $frases) {
  $i++
  $n = '{0:D2}' -f $i
  $s = New-Object System.Speech.Synthesis.SpeechSynthesizer
  $voz = $s.GetInstalledVoices() | Where-Object { $_.VoiceInfo.Culture.Name -like 'es-*' } | Select-Object -First 1
  if ($voz) { $s.SelectVoice($voz.VoiceInfo.Name) }
  $s.Rate = 0
  $s.SetOutputToWaveFile((Join-Path $Salida "$n.wav"), $fmt)
  $s.Speak($f)
  $s.SetOutputToNull()
  $s.Dispose()
  [System.IO.File]::WriteAllText((Join-Path $Salida "$n.txt"), $f, (New-Object System.Text.UTF8Encoding($false)))
}
Write-Output "$i frases en $Salida"
