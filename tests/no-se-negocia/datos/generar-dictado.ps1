# Genera dictado-15s.wav (16 kHz, 16 bits, mono) con la voz TTS de Windows, directo a fichero:
# nunca suena por los altavoces. Lo usa 03-velocidad.test.mjs si el WAV no existe.
param([string]$Salida = (Join-Path $PSScriptRoot 'dictado-15s.wav'))
Add-Type -AssemblyName System.Speech
$s = New-Object System.Speech.Synthesis.SpeechSynthesizer
$voz = $s.GetInstalledVoices() | Where-Object { $_.VoiceInfo.Culture.Name -like 'es-*' } | Select-Object -First 1
if ($voz) { $s.SelectVoice($voz.VoiceInfo.Name) }
$s.Rate = 0
$fmt = New-Object System.Speech.AudioFormat.SpeechAudioFormatInfo(16000, [System.Speech.AudioFormat.AudioBitsPerSample]::Sixteen, [System.Speech.AudioFormat.AudioChannel]::Mono)
$s.SetOutputToWaveFile($Salida, $fmt)
$s.Speak('Hola equipo, la reunión del jueves se mueve al viernes a las diez de la mañana. Por favor, revisad el presupuesto del proyecto antes de entonces y traed las dudas apuntadas. Si alguien no puede venir, que me avise hoy.')
$s.SetOutputToNull()
$s.Dispose()
Write-Output $Salida
