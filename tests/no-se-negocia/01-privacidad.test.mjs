// Regla 1 · Privacidad: todo local (CLAUDE.md § NO SE NEGOCIA).
// Solo hay red hacia los hosts de specs/PRD.md §5.2 y loopback; Ollama en loopback por defecto.
// Un host nuevo NO se añade aquí en silencio: se para y se avisa (CLAUDE.md).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { RAIZ, SRC_TAURI, cargoTest, exigirCargoVerde, ficheros, rel, rustDeProduccion, soloDePrueba, rutaChrome, abrirHub, cargar, PAGINAS, AJUSTES } from './_comun.mjs';

// specs/PRD.md §5.2: huggingface.co (modelos), ollama.com / GitHub (instalador de Ollama),
// api.github.com + assets de releases de AlfonsoGo/pronto (actualizador), Ollama local.
const HOSTS_PRD = ['huggingface.co', 'ollama.com', 'github.com', 'api.github.com'];
// Los assets de las releases de GitHub se sirven desde *.githubusercontent.com (redirección).
const SUFIJOS_PRD = ['.githubusercontent.com'];
const LOOPBACK = ['127.0.0.1', 'localhost', '[::1]'];
// Únicos módulos de Rust que hablan por la red, y para qué (specs/PRD.md §5.2).
const MODULOS_DE_RED = {
  'src-tauri/src/ollama.rs': 'Ollama local',
  'src-tauri/src/aisetup.rs': 'instalador de Ollama (ollama.com) y Ollama local',
  'src-tauri/src/modeldl.rs': 'modelos de voz (huggingface.co)',
  'src-tauri/src/updater.rs': 'actualizador (api.github.com + releases)',
};

// Cadenas con forma de dominio que NO son red (revisadas a mano; cada una con su motivo).
const NO_SON_RED = {
  'amara.org': 'src-tauri/src/whisper.rs KNOWN: frase que Whisper alucina en silencios y se borra del texto',
  'meet.google.com': 'src-tauri/src/detect.rs title_service: patrón del título/URL de una pestaña para reconocer Google Meet (no se conecta a nada)',
};

const permitido = (host) => {
  const h = host.toLowerCase().replace(/:\d+$/, '');
  return HOSTS_PRD.includes(h) || LOOPBACK.includes(h) || SUFIJOS_PRD.some((s) => h.endsWith(s));
};

/** Hosts y dominios fuera de PRD §5.2 que nombra el código Rust de producción. */
function hostsFuera(rust) {
  const fuera = [];
  for (const { fichero, texto } of rust) {
    for (const m of texto.matchAll(/https?:\/\/([^/\s"'`<>)?#\\]*)/g)) {
      const host = m[1];
      if (!host && texto[m.index + m[0].length] === '"') continue; // solo el esquema («"https://"»), p. ej. para validar
      if (!host || host.startsWith('{')) fuera.push(`${fichero}: URL construida en tiempo de ejecución «${m[0]}» (revisa a dónde va)`);
      else if (!permitido(host)) fuera.push(`${fichero}: ${host}`);
    }
    // Dominios sueltos en cadenas («github.com», «.githubusercontent.com»…).
    for (const m of texto.matchAll(/"\.?((?:[a-z0-9-]+\.)+(?:com|co|net|org|io|dev|ai|app|me|cloud|xyz|hf))"/gi)) {
      if (!permitido(m[1]) && !SUFIJOS_PRD.includes(`.${m[1]}`) && !(m[1].toLowerCase() in NO_SON_RED)) fuera.push(`${fichero}: dominio ${m[1]}`);
    }
  }
  return fuera;
}

test('01 privacidad: el código de Pronto solo nombra hosts de PRD §5.2 o loopback', () => {
  const fuera = hostsFuera(rustDeProduccion());
  // Script del actualizador (lo lanza Pronto): no descarga nada por su cuenta.
  const ps1 = readFileSync(join(SRC_TAURI, 'src', 'update_helper.ps1'), 'utf8');
  if (/https?:\/\/|Invoke-WebRequest|Invoke-RestMethod|WebClient|Start-BitsTransfer|curl/i.test(ps1)) fuera.push('src-tauri/src/update_helper.ps1: hace peticiones de red');
  assert.deepEqual(fuera, [], `Hosts fuera de specs/PRD.md §5.2 (no los añadas a la lista sin el visto bueno de Txonso):\n${fuera.join('\n')}`);
});

// Los módulos que solo se compilan en pruebas (`#[cfg(test)] mod x;`, p. ej. las sondas del
// rompedor con sus servidores en 127.0.0.1 y URLs hostiles) no son código de Pronto. Este test
// comprueba con un árbol de mentira que esa exclusión no abre agujeros: cada mutante da rojo.
test('01 privacidad: solo se excluyen los módulos que solo se compilan en pruebas', () => {
  const dir = mkdtempSync(join(tmpdir(), 'pronto-nsn-privacidad-'));
  const montar = (ficherosRs) => {
    rmSync(dir, { recursive: true, force: true });
    for (const [n, t] of Object.entries(ficherosRs)) {
      mkdirSync(join(dir, n, '..'), { recursive: true });
      writeFileSync(join(dir, n), t);
    }
    return hostsFuera(rustDeProduccion(dir));
  };
  const rs = (...lineas) => `${lineas.join('\n')}\n`;
  const sondas = rs('pub fn x() { let _ = "https://evil.test/a"; }');
  try {
    const limpio = { 'main.rs': rs('mod db;', '#[cfg(test)]', 'mod sondas;'), 'db.rs': rs('pub fn y() {}'), 'sondas.rs': sondas, 'sondas/sub.rs': sondas };
    assert.deepEqual(montar(limpio), [], 'un módulo `#[cfg(test)] mod x;` no es código de producción');
    assert.deepEqual(soloDePrueba(dir).map((p) => p.split(/[\\/]/).pop()), ['sondas.rs']);
    const mutantes = {
      'host en producción': { ...limpio, 'db.rs': rs('pub fn y() { let _ = "https://evil.test/b"; }') },
      'declarado sin cfg(test)': { ...limpio, 'main.rs': rs('mod db;', 'mod sondas;') },
      'enlazado además con #[path] desde producción': { ...limpio, 'main.rs': rs('mod db;', '#[cfg(test)]', 'mod sondas;', '#[path = "sondas.rs"]', 'mod otra;') },
      'cfg(test) con #[path] a un fichero de producción': { ...limpio, 'main.rs': rs('#[cfg(test)]', '#[path = "db.rs"]', 'mod sondas;'), 'db.rs': rs('pub fn y() { let _ = "https://evil.test/c"; }') },
      'cfg(any(test, …))': { ...limpio, 'main.rs': rs('mod db;', '#[cfg(any(test, feature = "x"))]', 'mod sondas;') },
      'cfg(not(test))': { ...limpio, 'main.rs': rs('mod db;', '#[cfg(not(test))]', 'mod sondas;') },
      'include! desde producción': { ...limpio, 'db.rs': rs('include!("sondas.rs");') },
    };
    for (const [nombre, arbol] of Object.entries(mutantes)) {
      assert.notDeepEqual(montar(arbol), [], `mutante «${nombre}»: el host no se detectó`);
    }
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('01 privacidad: solo los módulos previstos abren conexiones de red', () => {
  const red = /\bureq::|std::net::|\bTcpStream\b|\bUdpSocket\b|\breqwest\b|\bhyper::|WinHttp|URLDownloadToFile|tauri_plugin_http/;
  const nuevos = rustDeProduccion().filter(({ fichero, texto }) => red.test(texto) && !(fichero in MODULOS_DE_RED)).map(({ fichero }) => fichero);
  assert.deepEqual(nuevos, [], `Módulos con red no previstos en specs/PRD.md §5.2:\n${nuevos.join('\n')}`);
  // Ninguna dependencia de red aparte de ureq (que solo usan esos módulos).
  const cargo = readFileSync(join(SRC_TAURI, 'Cargo.toml'), 'utf8');
  const deps = ['reqwest', 'hyper', 'isahc', 'surf', 'attohttpc', 'curl', 'tauri-plugin-http', 'tauri-plugin-websocket', 'tungstenite'].filter((d) => new RegExp(`^${d}\\s*=`, 'm').test(cargo));
  assert.deepEqual(deps, [], `Dependencias de red nuevas en Cargo.toml: ${deps.join(', ')}`);
});

test('01 privacidad: la interfaz no carga nada de fuera del PC', () => {
  const fuera = [];
  for (const f of ficheros(join(RAIZ, 'ui'), ['.js', '.html', '.css'])) {
    const src = readFileSync(f, 'utf8');
    const pats = [
      /\b(?:fetch|EventSource|WebSocket|sendBeacon|importScripts)\s*\(\s*[`'"]((?:https?|wss?):)?\/\/[^`'"]+/g,
      /\bimport\s*(?:\(|[^'"`;]*from\s*)\s*[`'"]https?:\/\/[^`'"]+/g,
      /\b(?:src|href|action|poster|data)\s*=\s*["'`]https?:\/\/[^"'`]+["'`][^>]*>/g,
      /url\(\s*["']?https?:\/\/[^)]+\)/g,
      /@import\s+(?:url\()?["']?https?:\/\//g,
      /new\s+XMLHttpRequest/g,
    ];
    for (const p of pats) for (const m of src.matchAll(p)) {
      // Un <a href> externo lo abre el usuario con un clic; no es una petición de Pronto.
      if (/^href\s*=/.test(m[0]) && /^<a\s/i.test(src.slice(src.lastIndexOf('<', m.index), m.index))) continue;
      fuera.push(`${rel(f)}: ${m[0].slice(0, 100)}`);
    }
  }
  assert.deepEqual(fuera, [], `La interfaz pide recursos de fuera:\n${fuera.join('\n')}`);
});

test('01 privacidad: Ollama en loopback por defecto y el actualizador solo baja de GitHub (Rust)', () => {
  const r = cargoTest('nsn_privacidad');
  exigirCargoVerde(assert, r, 'nsn_privacidad');
});

// Decisión de Txonso (23-oct-2026): Ollama solo en este PC. Una URL que no sea de este PC (loopback) es un error
// de validación: Ajustes lo dice y NO la guarda (el backend la rechaza también: `aisetup::checked_local_url`,
// probado en `nsn_privacidad_*`). 0.0.0.0 no cuenta como este PC, y `evil.example\@localhost` tampoco (para el
// navegador, \ cierra la autoridad: el host es evil.example).
test('01 privacidad: con Ollama fuera de este PC, Ajustes lo rechaza y no lo guarda', { skip: !rutaChrome() && 'no hay Chrome (PRIMERIZO_CHROME)', timeout: 120_000 }, async () => {
  const hub = await abrirHub();
  try {
    const ctx = await hub.browser.newContext({ viewport: { width: 1180, height: 780 } });
    const page = await ctx.newPage();
    await cargar(page, `${hub.base}/hub.html?settings=experimental`);
    const guardada = () => page.evaluate(async () => (await import('/shared/mock.js')).invoke('get_settings').then((s) => s.ollamaUrl));
    const error = page.locator('#oll-remote');
    assert.equal(await error.isVisible(), false, 'con Ollama en este PC no hay error');
    const url = page.getByRole('textbox', { name: 'URL de Ollama' });
    let ultima = await guardada();
    assert.equal(ultima, 'http://localhost:11434');
    for (const [dir, rechazada] of [['http://192.168.1.50:11434', true], ['http://127.0.0.1:11434', false], ['http://0.0.0.0:11434', true], ['http://[::1]:11434', false], ['http://ollama.example.com', true], ['http://localhost:11434', false], ['http://evil.example\\@localhost:11434', true]]) {
      await url.fill(dir);
      await url.press('Enter');
      await page.waitForTimeout(300);
      assert.equal(await error.isVisible(), rechazada, `${dir}: error ${rechazada ? 'oculto' : 'visible'}`);
      assert.equal(await url.getAttribute('aria-invalid'), rechazada ? 'true' : null, `${dir}: el campo ${rechazada ? 'debe marcarse' : 'no debe marcarse'} como no válido`);
      if (rechazada) {
        assert.match(await error.innerText(), /Pronto solo usa Ollama en este PC/);
        assert.equal(await guardada(), ultima, `${dir}: se rechaza y no se guarda`);
      } else {
        assert.equal(await guardada(), dir, `${dir}: es de este PC y se guarda`);
        ultima = dir;
      }
    }
    // El backend (aquí simulado igual que commands.rs) tampoco la acepta aunque la UI no la parara.
    const directo = await page.evaluate(async () => {
      const m = await import('/shared/mock.js');
      const s = await m.invoke('get_settings');
      try { await m.invoke('save_settings', { settings: { ...s, ollamaUrl: 'http://192.168.1.50:11434' } }); return 'guardó'; } catch (e) { return String(e); }
    });
    assert.match(directo, /^Pronto solo usa Ollama en este PC/);
    assert.equal(await guardada(), ultima);
  } finally {
    await hub.cerrar();
  }
});

test('01 privacidad: el hub no hace ninguna petición fuera de 127.0.0.1', { skip: !rutaChrome() && 'no hay Chrome (PRIMERIZO_CHROME)', timeout: 180_000 }, async () => {
  const hub = await abrirHub();
  try {
    const ctx = await hub.browser.newContext({ viewport: { width: 1180, height: 780 } });
    const page = await ctx.newPage();
    const fuera = new Set();
    page.on('request', (req) => {
      const u = req.url();
      if (u.startsWith('data:') || u.startsWith('blob:') || u.startsWith(hub.base + '/')) return;
      fuera.add(u);
    });
    const rutas = [...PAGINAS.map((p) => `hub.html?page=${p}`), ...AJUSTES.map((s) => `hub.html?settings=${s}`), 'hub.html?update=available', 'flowbar.html'];
    for (const r of rutas) await cargar(page, `${hub.base}/${r}`);
    assert.deepEqual([...fuera], [], `El hub pidió recursos fuera del PC:\n${[...fuera].join('\n')}`);
  } finally {
    await hub.cerrar();
  }
});
