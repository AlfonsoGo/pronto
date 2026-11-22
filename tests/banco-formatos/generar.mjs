// Genera el audio del banco de formatos (casos.tsv) con voces locales, directo a fichero (nunca por los
// altavoces): la voz de Windows en español (Helena) y tres voces de Piper (davefx, sharvard hombre y mujer,
// claude de México), por turnos. Salida: audio/NN.wav (16 kHz, 16 bits, mono) y audio/NN.txt con la frase.
// Uso: node tests/banco-formatos/generar.mjs [carpeta]
// Piper: PRONTO_PIPER=carpeta con venv/ y voces/ (por defecto C:\Users\Txonso\dev\pronto-pruebas\piper;
// instalación: python -m venv venv && venv\Scripts\pip install piper-tts; voces de
// huggingface.co/rhasspy/piper-voices). Sin Piper, todas las frases van con la voz de Windows.
import { existsSync, mkdirSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const AQUI = dirname(fileURLToPath(import.meta.url));
const SALIDA = process.argv[2] || join(AQUI, 'audio');
const PIPER = process.env.PRONTO_PIPER || 'C:/Users/Txonso/dev/pronto-pruebas/piper';
const PY = join(PIPER, 'venv', 'Scripts', 'python.exe');
const VOCES = [
  { nombre: 'helena' },
  { nombre: 'davefx', modelo: 'es_ES-davefx-medium.onnx' },
  { nombre: 'sharvard-h', modelo: 'es_ES-sharvard-medium.onnx', hablante: 0 },
  { nombre: 'sharvard-m', modelo: 'es_ES-sharvard-medium.onnx', hablante: 1 },
  { nombre: 'claude-mx', modelo: 'es_MX-claude-high.onnx' },
];

export function leerCasos(ruta = join(AQUI, 'casos.tsv')) {
  return readFileSync(ruta, 'utf8')
    .split(/\r?\n/)
    .filter((l) => l.trim() && !l.startsWith('#'))
    .map((l) => {
      const [id, categoria, frase, aceptados, prohibidos] = l.split('\t');
      const lista = (s) => (s ?? '').split(' || ').map((x) => x.trim()).filter(Boolean);
      return { id, categoria, frase, aceptados: lista(aceptados), prohibidos: lista(prohibidos) };
    });
}

export function vozDe(i) {
  const v = VOCES[i % VOCES.length];
  if (v.modelo && !existsSync(PY)) return VOCES[0];
  return v;
}

function conHelena(frase, wav) {
  const ps = `Add-Type -AssemblyName System.Speech
$s = New-Object System.Speech.Synthesis.SpeechSynthesizer
$v = $s.GetInstalledVoices() | Where-Object { $_.VoiceInfo.Culture.Name -like 'es-*' } | Select-Object -First 1
if ($v) { $s.SelectVoice($v.VoiceInfo.Name) }
$fmt = New-Object System.Speech.AudioFormat.SpeechAudioFormatInfo(16000, [System.Speech.AudioFormat.AudioBitsPerSample]::Sixteen, [System.Speech.AudioFormat.AudioChannel]::Mono)
$s.SetOutputToWaveFile($env:PRONTO_WAV, $fmt)
$s.Speak($env:PRONTO_FRASE)
$s.SetOutputToNull(); $s.Dispose()`;
  const r = spawnSync('powershell', ['-NoProfile', '-Command', ps], { env: { ...process.env, PRONTO_WAV: wav, PRONTO_FRASE: frase }, encoding: 'utf8' });
  if (r.status !== 0) throw new Error(`voz de Windows: ${r.stderr}`);
}

function conPiper(v, frase, wav) {
  const crudo = `${wav}.22k.wav`;
  const args = ['-m', 'piper', '-m', join(PIPER, 'voces', v.modelo), '-f', crudo];
  if (v.hablante !== undefined) args.push('-s', String(v.hablante));
  const r = spawnSync(PY, args, { input: frase, encoding: 'utf8', env: { ...process.env, PYTHONUTF8: '1', PYTHONIOENCODING: 'utf-8' } });
  if (r.status !== 0 || !existsSync(crudo)) throw new Error(`piper ${v.nombre}: ${r.stderr}`);
  const f = spawnSync('ffmpeg', ['-y', '-loglevel', 'error', '-i', crudo, '-ar', '16000', '-ac', '1', '-sample_fmt', 's16', wav], { encoding: 'utf8' });
  rmSync(crudo, { force: true });
  if (f.status !== 0) throw new Error(`ffmpeg: ${f.stderr}`);
}

export function generar(salida = SALIDA) {
  mkdirSync(salida, { recursive: true });
  const casos = leerCasos();
  casos.forEach((c, i) => {
    const wav = join(salida, `${c.id}.wav`);
    const v = vozDe(i);
    if (v.modelo) conPiper(v, c.frase, wav);
    else conHelena(c.frase, wav);
    writeFileSync(join(salida, `${c.id}.txt`), c.frase);
  });
  writeFileSync(join(salida, 'voces.txt'), casos.map((c, i) => `${c.id}\t${vozDe(i).nombre}`).join('\n') + '\n');
  return casos.length;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  console.log(`${generar()} frases en ${SALIDA}`);
}
