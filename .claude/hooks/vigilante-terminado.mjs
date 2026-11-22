#!/usr/bin/env node
// Vigilante de «terminado» genérico (hook Stop, BELTR-SIS-13; asyncRewake desde BELTR-SIS-21).
// Se copia a <proyecto>/.claude/hooks/vigilante-terminado.mjs y lee <proyecto>/.claude/vigilante.json:
//   { "nombre": "Pronto 2", "comando": "npm test", "vigilado": ["src", "tests", "package.json"], "minutos": 12 }
//
// Hook Stop con `async` + `asyncRewake` (BELTR-SIS-21, asesorado por Fable 5.2): corre en segundo plano,
// no bloquea la caja de mensajes y solo despierta a Claude (código 2) con ROJO o ERROR. Ver main().
// `--correr --id X` es el corredor: un proceso aparte por carpeta que pasa las pruebas y guarda el
// resultado en %TEMP%/vigilante-<hash>.json; manda el aviso de Windows.
// Huella: con git, HEAD + `git status` + `git diff HEAD` + archivos nuevos sin seguimiento; sin git,
// ruta+tamaño+fecha de cada archivo vigilado.
import { spawn, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readdirSync, readFileSync, renameSync, statSync, unlinkSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const RAIZ = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const SALTAR = new Set(['node_modules', '.git', 'dist', 'build', '.next', '__pycache__', '.venv', 'venv', 'coverage', '.dart_tool']);

export function leerConfig(raiz = RAIZ) {
  const c = JSON.parse(readFileSync(path.join(raiz, '.claude', 'vigilante.json'), 'utf8'));
  if (typeof c.comando !== 'string' || !c.comando.trim()) throw new Error('vigilante.json sin "comando"');
  const vigilado = Array.isArray(c.vigilado) && c.vigilado.length ? c.vigilado : ['.'];
  return { nombre: c.nombre || path.basename(raiz), comando: c.comando, vigilado, minutos: Number(c.minutos) || 12 };
}

// Sin NODE_TEST_CONTEXT: si quien llama corre dentro de `node --test`, el hijo informaría al padre y saldría con 0.
const { NODE_TEST_CONTEXT: _fuera, ...ENTORNO } = process.env;
const correr = (raiz, cmd, ms) => spawnSync(cmd, {
  cwd: raiz, encoding: 'utf8', shell: true, timeout: ms, maxBuffer: 64 * 1024 * 1024, env: ENTORNO,
});

// Corre las pruebas con tope de tiempo matando el ÁRBOL entero al pasarse. spawnSync con timeout
// solo mata cmd.exe y deja huérfanos (node, dart de Flutter) agarrando la carpeta.
const CORREDOR = `
const { spawn, spawnSync } = require('node:child_process');
const [cmd, ms] = process.argv.slice(1);
const win = process.platform === 'win32';
const p = spawn(cmd, { shell: true, detached: !win, windowsHide: true });
let fuera = false;
p.stdout.on('data', (d) => process.stdout.write(d));
p.stderr.on('data', (d) => process.stderr.write(d));
const t = setTimeout(() => {
  fuera = true;
  if (win) spawnSync('taskkill', ['/PID', String(p.pid), '/T', '/F'], { windowsHide: true });
  else { try { process.kill(-p.pid, 'SIGKILL'); } catch {} }
}, Number(ms));
p.on('close', (c) => { clearTimeout(t); process.exit(fuera ? 124 : (c ?? 1)); });
`;
const correrConTope = (raiz, cmd, ms) => {
  const r = spawnSync(process.execPath, ['-e', CORREDOR, cmd, String(ms)], {
    cwd: raiz, encoding: 'utf8', timeout: ms + 60_000, maxBuffer: 64 * 1024 * 1024, env: ENTORNO, windowsHide: true,
  });
  return r.status === 124 ? { ...r, status: null, error: new Error('tope') } : r;
};

/** Huella del estado del código vigilado. Nunca vacía: un commit cambia la huella (entra HEAD)
 *  y un archivo nuevo sin seguimiento también (entran su tamaño y su fecha). */
export function huella(raiz, vigilado) {
  const lista = vigilado.map((v) => `"${v}"`).join(' ');
  const st = correr(raiz, `git status --porcelain -- ${lista}`, 30_000);
  if (st.status === 0) {
    const h = createHash('sha1');
    h.update(correr(raiz, 'git rev-parse HEAD', 30_000).stdout ?? '').update('\0').update(st.stdout).update('\0');
    h.update(correr(raiz, `git diff HEAD -- ${lista}`, 30_000).stdout ?? '');
    const nuevos = (correr(raiz, `git ls-files -o --exclude-standard -- ${lista}`, 30_000).stdout ?? '').split(/\r?\n/).filter(Boolean).sort();
    for (const rel of nuevos) {
      try { const s = statSync(path.join(raiz, rel)); h.update(`\0${rel}|${s.size}|${s.mtimeMs}`); } catch { /* borrado entre medias */ }
    }
    return 'git:' + h.digest('hex');
  }
  // Sin git: ruta + tamaño + fecha de cada archivo.
  const h = createHash('sha1');
  const visitar = (rel) => {
    let s;
    try { s = statSync(path.join(raiz, rel)); } catch { return; }
    if (s.isDirectory()) {
      for (const n of readdirSync(path.join(raiz, rel)).sort()) if (!SALTAR.has(n)) visitar(path.join(rel, n));
    } else h.update(`${rel}|${s.size}|${s.mtimeMs}\n`);
  };
  for (const v of vigilado) visitar(v);
  return 'sin-git:' + h.digest('hex');
}

/** Archivos de pruebas que fallaron, sacados de la salida TAP de `node --test`. */
export function archivosQueFallan(salida) {
  const archivos = new Set();
  for (const b of salida.split(/\r?\n(?=\s*not ok \d+)/).slice(1)) {
    const m = b.match(/location:\s*'([^']+?\.(?:mjs|js|cjs|ts)):\d+:\d+'/);
    if (m) archivos.add(m[1].replace(/\\\\/g, '\\'));
  }
  return [...archivos];
}

/** Lo que ve Claude cuando frena: qué falla y por qué, corto. */
export function resumenFallo(salida) {
  const lineas = salida.split(/\r?\n/);
  const utiles = lineas.filter((l) => /not ok|FAIL|Error|error:|expected|actual|assert|location:/.test(l));
  return (utiles.length ? utiles : lineas).slice(-40).join('\n');
}

function estado(raiz) {
  const fichero = path.join(os.tmpdir(), `vigilante-${createHash('sha1').update(raiz).digest('hex').slice(0, 12)}.json`);
  return {
    leer: () => { try { return JSON.parse(readFileSync(fichero, 'utf8')); } catch { return {}; } },
    // Escritura atómica (tmp + rename): varios procesos leen y escriben este archivo a la vez.
    guardar: (v) => {
      const tmp = `${fichero}.${process.pid}.${Date.now()}.tmp`;
      try { writeFileSync(tmp, JSON.stringify(v)); } catch { return; }
      for (let i = 0; i < 5; i++) {
        try { renameSync(tmp, fichero); return; } catch { const t = Date.now() + 50; while (Date.now() < t) { /* espera EPERM */ } }
      }
      try { unlinkSync(tmp); } catch { /* nada */ }
    },
  };
}


// Aviso de Windows (toast) con el resultado, para que Txonso vea cuándo acaban las pruebas: lo que un
// hook en segundo plano le dice a Claude no se ve en pantalla. Uno por resultado (lo lanza el corredor).
export function avisarWindows(titulo, texto) {
  if (process.platform !== 'win32' || process.env.VIGILANTE_SIN_AVISO) return;
  const esc = (s) => String(s).replace(/[<>&'"]/g, (c) => ({ '<': '&lt;', '>': '&gt;', '&': '&amp;', "'": '&apos;', '"': '&quot;' }[c]));
  const xml = `<toast><visual><binding template="ToastGeneric"><text>${esc(titulo)}</text><text>${esc(texto)}</text></binding></visual></toast>`;
  const ps = [
    '[Windows.UI.Notifications.ToastNotificationManager,Windows.UI.Notifications,ContentType=WindowsRuntime]>$null',
    '[Windows.Data.Xml.Dom.XmlDocument,Windows.Data.Xml.Dom,ContentType=WindowsRuntime]>$null',
    '$x=New-Object Windows.Data.Xml.Dom.XmlDocument',
    `$x.LoadXml('${xml.replace(/'/g, "''")}')`,
    '$id=\'{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\\WindowsPowerShell\\v1.0\\powershell.exe\'',
    '[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier($id).Show([Windows.UI.Notifications.ToastNotification]::new($x))',
  ].join(';');
  try {
    spawn('powershell.exe', ['-NoProfile', '-NonInteractive', '-WindowStyle', 'Hidden', '-Command', ps], { detached: true, stdio: 'ignore', windowsHide: true }).unref();
  } catch { /* sin aviso no pasa nada */ }
}

/** Corredor: pasa las pruebas y guarda el resultado. 0 = VERDE, 1 = ROJO. */
export function correrPruebas(raiz = RAIZ) {
  let cfg;
  try { cfg = leerConfig(raiz); } catch (e) { console.log(`ERROR: no puedo leer .claude/vigilante.json (${e.message}).`); return 0; }
  const { leer, guardar } = estado(raiz);
  const h = huella(raiz, cfg.vigilado);
  const ms = cfg.minutos * 60_000;
  const t0 = Date.now();
  const r = correrConTope(raiz, cfg.comando, ms);
  if (r.error || r.status === null) {
    const motivo = `«${cfg.comando}» no arrancó o pasó de ${cfg.minutos} min`;
    guardar({ ...leer(), error: h, motivoError: motivo });
    console.log(`ERROR: ${motivo}. No es un fallo de las pruebas.`);
    avisarWindows(`Pruebas de ${cfg.nombre}: ERROR`, `${motivo}. Nadie las ha comprobado.`);
    return 0;
  }
  const seg = () => Math.round((Date.now() - t0) / 1000);
  const salida = `${r.stdout}\n${r.stderr}`;
  // Si el código cambió mientras corrían, el resultado es de un código que ya no existe: no se guarda.
  const rancio = () => huella(raiz, cfg.vigilado) !== h;
  const verde = (nota = '') => {
    if (rancio()) { console.log('VERDE (código viejo): el código cambió mientras corrían las pruebas; no lo doy por bueno.'); return 0; }
    guardar({ ...leer(), verde: h, rojo: undefined, resumen: undefined, error: undefined, rojosSeguidos: 0 });
    console.log(`VERDE: pruebas de ${cfg.nombre} pasan («${cfg.comando}», ${seg()} s)${nota}.`);
    avisarWindows(`Pruebas de ${cfg.nombre}: VERDE`, `Pasan en ${seg()} s${nota}.`);
    return 0;
  };
  if (r.status === 0) return verde();
  // Reintento de inestables: por archivo con node --test; si no, todo el comando solo si fue corto (<2 min).
  const archivos = archivosQueFallan(salida);
  const otraVez = archivos.length
    ? archivos.map((a) => correrConTope(raiz, `node --test "${a}"`, ms)).every((x) => x.status === 0)
    : Date.now() - t0 < 2 * 60_000 && correrConTope(raiz, cfg.comando, ms).status === 0;
  if (otraVez) return verde(`; fallaron una vez y pasaron al repetir (inestables${archivos.length ? `: ${archivos.join(', ')}` : ''})`);
  const resumen = resumenFallo(salida);
  if (rancio()) { console.log(`ROJO (código viejo): fallaron, pero el código cambió mientras corrían.\n\n${resumen}`); return 1; }
  const s = leer();
  guardar({ ...s, rojo: h, resumen, rojosSeguidos: (s.rojosSeguidos || 0) + 1 });
  console.log(`ROJO: pruebas de ${cfg.nombre} fallan («${cfg.comando}», ${seg()} s).\n\n${resumen}`);
  avisarWindows(`Pruebas de ${cfg.nombre}: ROJO`, resumen.split(/\r?\n/).find((l) => l.trim()) || 'Fallan.');
  return 1;
}

const vivo = (pid) => { try { process.kill(pid, 0); return true; } catch (e) { return e.code === 'EPERM'; } };

/** Arranca el corredor en un proceso aparte (uno por carpeta). Devuelve su id. */
export function arrancarFondo(raiz = RAIZ, h = '') {
  const { leer, guardar } = estado(raiz);
  const s = leer();
  if (s.enCurso?.pid && vivo(s.enCurso.pid) && s.resultado?.id !== s.enCurso.id) {
    return s.enCurso.id; // ya hay uno corriendo: se espera a ese
  }
  const id = `${Date.now()}`;
  const hijo = spawn(process.execPath, [fileURLToPath(import.meta.url), '--correr', '--id', id], {
    cwd: raiz, detached: true, stdio: 'ignore', windowsHide: true, env: ENTORNO,
  });
  hijo.unref();
  guardar({ ...leer(), enCurso: { id, inicio: Date.now(), pid: hijo.pid, huella: h } });
  return id;
}

/** Espera el resultado del corredor `id`. null = se acabó el tiempo o murió. */
export async function esperarResultado(raiz, id, maxMs, pasoMs = 2000) {
  const { leer } = estado(raiz);
  const t0 = Date.now();
  for (;;) {
    const s = leer();
    if (s.resultado?.id === id) return s.resultado;
    if (s.enCurso?.id === id && s.enCurso.pid && !vivo(s.enCurso.pid)) {
      await new Promise((r) => setTimeout(r, 500));
      const s2 = leer();
      return s2.resultado?.id === id ? s2.resultado : null;
    }
    if (Date.now() - t0 >= maxMs) return null;
    await new Promise((r) => setTimeout(r, pasoMs));
  }
}

const decir = (txt) => { process.stdout.write(txt); process.stderr.write(txt); };

/**
 * Hook Stop con `async` + `asyncRewake` (forma de Lydia: sale con código 2, pero sin bloquear la caja de
 * mensajes). Claude Code lo corre en segundo plano y solo despierta a Claude si sale con 2.
 * - Código ya en verde → 0, sin correr nada.
 * - ROJO o ERROR ya conocidos de este código y no avisados en esta ventana → 2 al momento.
 * - Si no, arranca el corredor (o se engancha al que ya corre) y espera. VERDE → 0. ROJO/ERROR → 2.
 * - Freno blando: desde el 3.er ROJO seguido pide parar y explicar; desde el 6.º ya no despierta (solo aviso de Windows).
 */
export async function main(raiz = RAIZ, entrada = {}, { pasoMs = 2000 } = {}) {
  let cfg;
  try { cfg = leerConfig(raiz); } catch (e) { console.log(`Vigilante: no puedo leer .claude/vigilante.json (${e.message}); no compruebo.`); return 0; }
  const { leer, guardar } = estado(raiz);
  const sesion = entrada.session_id || 'sin-sesion';
  const h = huella(raiz, cfg.vigilado);
  const s0 = leer();
  if (h === s0.verde) return 0;

  const avisar = (tipo) => {
    const s = leer();
    const avisados = s.avisados || {};
    if (avisados[sesion] === `${tipo}:${h}`) return 0;
    guardar({ ...s, avisados: { ...avisados, [sesion]: `${tipo}:${h}` } });
    if (tipo === 'error') {
      decir(`ERROR del vigilante de ${cfg.nombre}: ${s.motivoError || 'las pruebas no llegaron a correr'}. ` +
        'No es un fallo de las pruebas, pero nadie las ha comprobado: díselo a Txonso.\n');
      return 2;
    }
    const n = s.rojosSeguidos || 1;
    if (n >= 6) return 0; // ya basta de despertar: queda el aviso de Windows
    const cabeza = n >= 3
      ? `ROJO por ${n}.ª vez seguida en ${cfg.nombre}. Para: no toques más código y explica a Txonso qué falla y qué has probado.`
      : `ROJO: las pruebas de ${cfg.nombre} fallan («${cfg.comando}»). Arréglalas antes de dar esto por terminado. Si el fallo no viene de tus cambios, dilo a Txonso.`;
    decir(`${cabeza}\n\n${s.resumen || ''}\n`);
    return 2;
  };
  if (h === s0.rojo) return avisar('rojo');
  if (h === s0.error) return avisar('error');

  // Se rinde 30 s antes del timeout del hook, que pone el instalador: (minutos*2+5) min.
  const limite = Date.now() + (cfg.minutos * 2 + 5) * 60_000 - 30_000;
  // Máx. 2 vueltas: si había un corredor con código viejo, se espera a ese y luego se prueba el actual.
  for (let vuelta = 0; vuelta < 2; vuelta++) {
    const id = arrancarFondo(raiz, h);
    const r = await esperarResultado(raiz, id, Math.max(0, limite - Date.now()), pasoMs);
    if (huella(raiz, cfg.vigilado) !== h) return 0; // el código cambió: lo comprobará el próximo Stop
    const s = leer();
    if (s.verde === h) return 0;
    if (s.rojo === h) return avisar('rojo');
    if (s.error === h) return avisar('error');
    if (!r) {
      guardar({ ...s, error: h, motivoError: `las pruebas no terminaron en ${cfg.minutos * 2 + 5} min (proceso colgado o muerto)` });
      return avisar('error');
    }
  }
  return 0;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2);
  if (args.includes('--correr')) {
    const id = args.includes('--id') ? args[args.indexOf('--id') + 1] : null;
    const lineas = [];
    const log = console.log;
    if (id) console.log = (...x) => { lineas.push(x.join(' ')); log(...x); };
    const codigo = correrPruebas(RAIZ);
    if (id) {
      const { leer, guardar } = estado(RAIZ);
      guardar({ ...leer(), resultado: { id, codigo, texto: lineas.join('\n') } });
    }
    process.exit(codigo);
  }
  let entrada = {};
  try { entrada = JSON.parse(readFileSync(0, 'utf8') || '{}'); } catch { /* sin entrada */ }
  process.exit(await main(RAIZ, entrada));
}
