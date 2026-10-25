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
