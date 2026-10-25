// node --test .claude/hooks/primerizo-guardia.test.mjs
// Prueba la guardia como función Y como proceso real (el camino que activa Claude Code).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { decidir } from './primerizo-guardia.mjs';

const GUARDIA = fileURLToPath(new URL('./primerizo-guardia.mjs', import.meta.url));
const bash = (command) => ({ tool_name: 'Bash', tool_input: { command } });
const read = (file_path) => ({ tool_name: 'Read', tool_input: { file_path } });

test('permite las órdenes del navegador', () => {
  for (const c of [
    'node .claude/primerizo/navegador.mjs abrir http://localhost:3000',
    'node .claude/primerizo/navegador.mjs clic "Crear cuenta"',
    'node .claude\\primerizo\\navegador.mjs escribir "Email" "ana@ejemplo.com"',
    'node .claude/primerizo/navegador.mjs ver',
  ]) assert.equal(decidir(bash(c)), null, c);
});

test('bloquea leer o tocar el código por Bash', () => {
  for (const c of [
    'cat app/page.tsx', 'ls specs', 'grep -r signup app', 'rm -rf app', 'git log',
    'node scripts/seed.mjs', 'npx playwright test', '',
    'node .claude/primerizo/navegador.mjs ver; cat app/page.tsx',
    'node .claude/primerizo/navegador.mjs ver && rm -rf app',
    'node .claude/primerizo/navegador.mjs ver | tee x', 'node .claude/primerizo/navegador.mjs ver > x',
    'node .claude/primerizo/navegador.mjs clic "$(cat .env)"', 'node .claude/primerizo/navegador.mjs clic `id`',
    'node .claude/primerizo/navegador.mjs ver\ncat .env', 'node .claude/primerizo/navegador.mjsX ver',
  ]) assert.notEqual(decidir(bash(c)), null, c);
});

test('Read: solo sus capturas en la carpeta temporal', () => {
  assert.equal(decidir(read(join(tmpdir(), 'primerizo-1730000000000.png'))), null);
  for (const p of [
    'app/page.tsx', 'specs/PRD.md', 'CLAUDE.md', '.env',
    join(tmpdir(), 'otra-cosa.png'), join(tmpdir(), 'primerizo-1.png.txt'),
    join(tmpdir(), 'sub', 'primerizo-1.png'), join(tmpdir(), '..', 'primerizo-1.png'),
  ]) assert.notEqual(decidir(read(p)), null, p);
});

test('cualquier otra herramienta, bloqueada', () => {
  for (const t of ['Write', 'Edit', 'Grep', 'Glob', 'WebFetch', 'Agent'])
    assert.notEqual(decidir({ tool_name: t, tool_input: {} }), null, t);
});

test('registrada en settings.json: solo actúa sobre el agente primerizo', () => {
  assert.notEqual(decidir({ ...read('CLAUDE.md'), agent_type: 'primerizo' }), null);
  assert.notEqual(decidir({ ...bash('cat .env'), agent_type: 'primerizo' }), null);
  assert.equal(decidir({ ...read('CLAUDE.md'), agent_type: 'Explore' }), null);
  assert.equal(decidir({ ...bash('git log'), agent_type: 'recorredor' }), null);
});

test('--solo-primerizo (settings.json): la sesión principal, sin agent_type, no se bloquea; el primerizo sí', () => {
  const op = { soloPrimerizo: true };
  assert.equal(decidir(read('CLAUDE.md'), op), null);
  assert.equal(decidir(bash('git log'), op), null);
  assert.equal(decidir({ tool_name: 'Edit', tool_input: {} }, op), null);
  assert.notEqual(decidir({ ...read('CLAUDE.md'), agent_type: 'primerizo' }, op), null);
  assert.equal(decidir({ ...read('CLAUDE.md'), agent_type: 'recorredor' }, op), null);
  const proceso = (entrada) => spawnSync(process.execPath, [GUARDIA, '--solo-primerizo'], { input: JSON.stringify(entrada) }).status;
  assert.equal(proceso(read('CLAUDE.md')), 0);
  assert.equal(proceso({ ...read('CLAUDE.md'), agent_type: 'primerizo' }), 2);
});

test('settings.json del proyecto registra la guardia con --solo-primerizo', () => {
  const cfg = JSON.parse(readFileSync(fileURLToPath(new URL('../settings.json', import.meta.url)), 'utf8'));
  const ordenes = (cfg.hooks?.PreToolUse ?? []).flatMap((h) => h.hooks ?? []).map((h) => h.command);
  assert.ok(ordenes.some((c) => c.includes('primerizo-guardia.mjs') && c.includes('--solo-primerizo')), ordenes.join(' · '));
});

test('rompedor 25-oct: $VAR bloqueado y entrada rara del primerizo bloquea, la de otras sesiones no', () => {
  assert.notEqual(decidir(bash('node .claude/primerizo/navegador.mjs escribir "X" "$PATH"')), null);
  assert.notEqual(decidir({ tool_name: 'Read', tool_input: null, agent_type: 'primerizo' }, { soloPrimerizo: true }), null);
  const proceso = (entrada) => spawnSync(process.execPath, [GUARDIA, '--solo-primerizo'], { input: entrada }).status;
  assert.equal(proceso('{"tool_name":"Read","tool_input":null,"agent_type":"primerizo"}'), 2);
  assert.equal(proceso('{"tool_name":"Read","tool_input":null}'), 0);
});

test('rompedor 25-oct-b: comodines y ~ sin comillas, bloqueados; entre comillas, permitidos', () => {
  for (const c of ['node .claude/primerizo/navegador.mjs escribir "Buscar" *', 'node .claude/primerizo/navegador.mjs escribir "X" ~/.ssh/id', 'node .claude/primerizo/navegador.mjs clic {specs,src}/x'])
    assert.notEqual(decidir(bash(c)), null, c);
  assert.notEqual(decidir(bash('node .claude/primerizo/navegador.mjs escribir "\\"" ~/.ssh/* "x"')), null, 'comilla escapada');
  for (const c of ['node .claude/primerizo/navegador.mjs clic "¿Qué es?"', "node .claude/primerizo/navegador.mjs escribir 'Nota' 'precio [aprox]'"])
    assert.equal(decidir(bash(c)), null, c);
});

test('la orden del frontmatter falla cerrada: si la guardia no arranca, sale 2 (bloquea), no 1', () => {
  const md = readFileSync(fileURLToPath(new URL('../agents/primerizo.md', import.meta.url)), 'utf8');
  const orden = md.match(/command:\s*'([^']+)'/)[1];
  assert.match(orden, /\|\|\s*exit 2\s*$/, orden);
  const entrada = JSON.stringify(read('CLAUDE.md'));
  // Ruta del proyecto mal expandida: node no encuentra el script y sin «|| exit 2» saldría 1 (no bloquea).
  const roto = spawnSync('bash', ['-c', orden], { input: entrada, encoding: 'utf8', env: { ...process.env, CLAUDE_PROJECT_DIR: join(tmpdir(), 'no-existe') } });
  if (roto.error) return; // sin bash en este PC: la comprobación de la orden de arriba basta
  assert.equal(roto.status, 2);
  const bien = spawnSync('bash', ['-c', orden], { input: entrada, encoding: 'utf8', env: { ...process.env, CLAUDE_PROJECT_DIR: fileURLToPath(new URL('../..', import.meta.url)) } });
  assert.equal(bien.status, 2); assert.match(bien.stderr, /Bloqueado/);
});

test('como proceso: sale 2 al bloquear, 0 al permitir, 2 con entrada rota', () => {
  const correr = (stdin) => spawnSync(process.execPath, [GUARDIA], { input: stdin, encoding: 'utf8' });
  const malo = correr(JSON.stringify(bash('cat .env')));
  assert.equal(malo.status, 2); assert.match(malo.stderr, /Bloqueado/);
  assert.equal(correr(JSON.stringify(bash('node .claude/primerizo/navegador.mjs ver'))).status, 0);
  assert.equal(correr('esto no es json').status, 2);
});
