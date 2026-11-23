// Comprueba un instalador ANTES de subirlo a una release:
//   node scripts/comprobar-instalador.mjs installer/dist/Pronto-Setup-2.3.3.exe
// Tiene que ser el de Inno Setup (installer/build_installer.ps1) y llevar la versión de
// src-tauri/tauri.conf.json. El NSIS de `tauri build` instala en otra carpeta y el actualizador
// de Pronto reabría la copia vieja: así salió la 2.3.2 (BELTR-PRT-41).
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

export const INNO_MARKER = 'Inno Setup Setup Data (';

/** Devuelve la lista de problemas (vacía = se puede subir). */
export function problemasDelInstalador(buf, version) {
  const out = [];
  if (buf.length < 2 || buf[0] !== 0x4d || buf[1] !== 0x5a) out.push('no es un ejecutable de Windows (falta «MZ»)');
  if (!buf.includes(Buffer.from(INNO_MARKER, 'latin1'))) {
    const nsis = buf.includes(Buffer.from('Nullsoft', 'latin1'));
    out.push(`no es el instalador de Inno Setup${nsis ? ' (es un NSIS, el de `tauri build`)' : ''}: hazlo con installer/build_installer.ps1`);
  }
  if (version && !buf.includes(Buffer.from(version, 'utf16le'))) out.push(`no lleva la versión ${version} de tauri.conf.json`);
  return out;
}

function versionDelProyecto() {
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
  return JSON.parse(readFileSync(path.join(root, 'src-tauri', 'tauri.conf.json'), 'utf8')).version;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const file = process.argv[2];
  if (!file) {
    console.error('Uso: node scripts/comprobar-instalador.mjs <Pronto-Setup-x.y.z.exe>');
    process.exit(2);
  }
  const version = versionDelProyecto();
  const problemas = problemasDelInstalador(readFileSync(file), version);
  if (problemas.length) {
    console.error(`NO SUBIR ${file}:\n- ${problemas.join('\n- ')}`);
    process.exit(1);
  }
  console.log(`OK: ${file} es el instalador de Inno de la ${version}.`);
}
