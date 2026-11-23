// BELTR-PRT-41: la 2.3.2 se publicó con el instalador NSIS de `tauri build` en vez del de Inno Setup.
// Instalaba en otra carpeta (%LOCALAPPDATA%\Pronto), el actualizador reabría la copia vieja y el aviso
// de la misma versión volvía en cada arranque. Estas pruebas cierran cada puerta por la que pasó.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { problemasDelInstalador, INNO_MARKER } from '../scripts/comprobar-instalador.mjs';

const leer = (p) => readFileSync(new URL(`../${p}`, import.meta.url), 'utf8');
const conf = JSON.parse(leer('src-tauri/tauri.conf.json'));
const version = conf.version;

const exe = (...partes) => Buffer.concat([Buffer.from('MZ', 'latin1'), ...partes.map((p) => (Buffer.isBuffer(p) ? p : Buffer.from(p, 'latin1')))]);

test('instalador: `tauri build` no fabrica instaladores (solo se hace con installer/build_installer.ps1)', () => {
  assert.equal(conf.bundle?.active, false, 'tauri.conf.json → bundle.active tiene que ser false: el NSIS de Tauri no es el instalador de Pronto');
});

test('instalador: la comprobación antes de subir rechaza el NSIS y acepta el de Inno con su versión', () => {
  const nsis = exe('...Nullsoft Install System v3.10...', Buffer.from(version, 'utf16le'));
  assert.match(problemasDelInstalador(nsis, version).join('\n'), /no es el instalador de Inno Setup \(es un NSIS/);
  const inno = exe(`...${INNO_MARKER}6.7.0)...`, Buffer.from(version, 'utf16le'));
  assert.deepEqual(problemasDelInstalador(inno, version), []);
  assert.match(problemasDelInstalador(inno, '9.9.9').join('\n'), /no lleva la versión 9\.9\.9/);
  assert.match(problemasDelInstalador(Buffer.from('hola'), version).join('\n'), /falta «MZ»/);
});

test('instalador: si ya está construido, el de installer/dist pasa la comprobación', (t) => {
  const f = new URL(`../installer/dist/Pronto-Setup-${version}.exe`, import.meta.url);
  if (!existsSync(f)) return t.skip(`sin installer/dist/Pronto-Setup-${version}.exe`);
  assert.deepEqual(problemasDelInstalador(readFileSync(f), version), []);
});

test('instalador: el ayudante instala en la carpeta del Pronto que reabre', () => {
  const ps = leer('src-tauri/src/update_helper.ps1');
  assert.match(ps, /\$appDir = \[System\.IO\.Path\]::GetDirectoryName\(\$Exe\)/);
  assert.match(ps, /\$setupArgs = @\([^)]*'\/DIR="' \+ \$appDir/);
});

test('instalador: Inno arregla los PC con la copia NSIS de la 2.3.2', () => {
  const iss = leer('installer/pronto2.iss');
  assert.match(iss, /#define NsisKeyName "Pronto"/, 'entrada de desinstalación que deja el NSIS');
  assert.match(iss, /DefaultDirName=\{code:DirPorDefecto\}/, 'si solo está la copia NSIS, se instala encima');
  assert.match(iss, /while IsFileInUse\(Exe\) or OtraCopiaEnUso do/, 'espera también a que se cierre la otra copia');
  assert.match(iss, /procedure CurStepChanged[\s\S]*CopiasAlDia;/, 'tras instalar, la otra copia queda al día');
  assert.match(iss, /AppId=\{#MyAppId\}/);
  assert.match(iss, /#define MyAppId "\{\{B7E5B2A0-9C4D-4E1A-9F3B-7A2D6E8C1F40\}\}"/, 'el AppId de siempre');
});
