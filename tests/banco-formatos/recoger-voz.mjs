// Recoge las grabaciones de correos de Txonso (BELTR-PRT-30) para el banco de formatos con voz real.
// Txonso dicta con la versión de pruebas las 15 frases de GUION.md, en orden. Este script copia los
// 15 últimos audios del historial de pruebas (o los 15 desde `--desde N`, id del primero) a la carpeta del
// conjunto como 01.wav … 15.wav, con su frase en 01.txt … 15.txt. Solo lee los audios: no toca la base.
//   node tests/banco-formatos/recoger-voz.mjs [--desde 324]
import { copyFileSync, existsSync, readdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { leerCasos } from './generar.mjs';

const DATOS = process.env.PRONTO_PRUEBAS_DATOS || 'C:/Users/Txonso/dev/pronto-pruebas/datos';
const AUDIO = join(DATOS, 'audio');
const DESTINO = process.env.PRONTO_CORREOS_VOZ || join(DATOS, 'correos-voz');

const casos = leerCasos(join(DESTINO, 'casos.tsv'));
const i = process.argv.indexOf('--desde');
const desde = i > 0 ? Number(process.argv[i + 1]) : null;
const wavs = readdirSync(AUDIO)
  .filter((f) => /^\d+-.*\.wav$/.test(f))
  .map((f) => ({ f, id: Number(f.split('-')[0]) }))
  .sort((a, b) => a.id - b.id);
const elegidos = desde ? wavs.filter((w) => w.id >= desde).slice(0, casos.length) : wavs.slice(-casos.length);
if (elegidos.length < casos.length) {
  console.error(`Solo hay ${elegidos.length} audios; hacen falta ${casos.length}. ¿Grabaste las 15 frases?`);
  process.exit(1);
}
casos.forEach((c, k) => {
  const w = elegidos[k];
  copyFileSync(join(AUDIO, w.f), join(DESTINO, `${c.id}.wav`));
  writeFileSync(join(DESTINO, `${c.id}.txt`), c.frase + '\n');
  console.log(`${c.id} ← dictado ${w.id}: ${c.frase}`);
});
console.log(`Listo: ${casos.length} correos en ${DESTINO}${existsSync(join(DESTINO, 'GUION.md')) ? '' : ' (falta GUION.md)'}.`);
