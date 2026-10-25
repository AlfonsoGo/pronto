#!/usr/bin/env node
// fases.mjs — la parte determinista de /fases. Sin dependencias (Node ≥ 20).
//
//   node fases.mjs detectar  [dir] [--json]            → qué proyecto es (stack, puerto, datos, zonas, sistemas)
//   node fases.mjs instalar  [dir] [--sistemas S2,S5] [--tipo nuevo|desplegado] [--pre-push]
//                                                       → copia y adapta las plantillas sin pisar nada
//   node fases.mjs verificar [dir] [--json]            → juzga cada sistema contra su evidencia y reescribe la
//                                                         tabla de estado de specs/FASES.md
//   node fases.mjs publicar  [dir] [--sin-tests] [--json] → PUEDE PUBLICARSE / NO (sale 0 / 1)
//
// `instalar` solo funciona desde la skill (necesita plantillas/); el resto también desde la copia que
// se instala en el proyecto (.claude/fases/fases.mjs).
import { spawnSync } from 'node:child_process';
import { appendFileSync, copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { basename, dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const AQUI = dirname(fileURLToPath(import.meta.url));
const PLANTILLAS = resolve(AQUI, '..', 'plantillas');
const IGNORAR = new Set(['node_modules', '.git', '.next', '.nuxt', '.svelte-kit', 'dist', 'build', 'out', 'coverage',
  '.venv', 'venv', '__pycache__', '.turbo', '.vercel', '.claude', 'specs', 'e2e', 'tests', 'test', '__tests__', 'rompedor-backup']);
const CRITICO = /(auth|login|logout|session|sesion|password|contrase|signup|registro|payment|pago|stripe|billing|factur|checkout|invoice|tenant|organi[sz]|admin|webhook|llm|openai|anthropic|prompt|gpt)/i;
const FUENTE = /\.(m?[jt]sx?|py|go|rb|php|svelte|vue|astro)$/;
// Zona crítica por contenido: el nombre del fichero no basta (un servidor.mjs puede llevar todo el login).
const CRITICO_CONTENIDO = /(scrypt|bcrypt|argon2|set-cookie|sesi[oó]n|session|password|contraseña|stripe|checkout|jwt|tenant_id|organization_id|openai|anthropic|['"`]\/(login|entrar|auth|pago|pagos|billing)['"`])/i;
const esCritico = (f, r) => CRITICO.test(r) || (FUENTE.test(f) && CRITICO_CONTENIDO.test((leer(f) ?? '').slice(0, 300_000)));
const MARCADOR = /<(?:[^<>\n]*\s[^<>\n]*|PROY-N|RUTA_DATOS|rutas|Proyecto|nuevo\|desplegado|crítico\|normal)>/;

// ───────────────────────── utilidades
const leer = (f) => { try { return readFileSync(f, 'utf8'); } catch { return null; } };
const leerJson = (f) => { try { return JSON.parse(readFileSync(f, 'utf8')); } catch { return null; } };
const rel = (dir, f) => relative(dir, f).split(sep).join('/');
const mtime = (f) => { try { return statSync(f).mtimeMs; } catch { return 0; } };

function recorrer(dir, { profundidad = 6, max = 4000 } = {}) {
  const fuera = [];
  const pila = [[dir, 0]];
  while (pila.length && fuera.length < max) {
    const [d, p] = pila.pop();
    let entradas = [];
    try { entradas = readdirSync(d, { withFileTypes: true }); } catch { continue; }
    for (const e of entradas) {
      if (e.isDirectory()) { if (!IGNORAR.has(e.name) && !e.name.startsWith('.') && p < profundidad) pila.push([join(d, e.name), p + 1]); }
      else fuera.push(join(d, e.name));
    }
  }
  return fuera;
}

function ultimoInforme(dir, carpeta) {
  const d = join(dir, carpeta);
  if (!existsSync(d)) return null;
  const mds = readdirSync(d).filter((n) => n.endsWith('.md')).sort();
  if (!mds.length) return null;
  const f = join(d, mds.at(-1));
  return { fichero: f, texto: leer(f) ?? '', ms: mtime(f) };
}

function seccion(texto, titulo) {
  const lineas = texto.split(/\r?\n/);
  const i = lineas.findIndex((l) => /^#{1,3}\s/.test(l) && titulo.test(l));
  if (i < 0) return '';
  const fin = lineas.findIndex((l, j) => j > i && /^#{1,3}\s/.test(l));
  return lineas.slice(i + 1, fin < 0 ? undefined : fin).join('\n');
}

// ───────────────────────── detectar
export function detectar(dir) {
  dir = resolve(dir);
  const pkg = leerJson(join(dir, 'package.json'));
  const deps = { ...(pkg?.dependencies ?? {}), ...(pkg?.devDependencies ?? {}) };
  const scripts = pkg?.scripts ?? {};
  const pyTxt = (leer(join(dir, 'pyproject.toml')) ?? '') + (leer(join(dir, 'requirements.txt')) ?? '');
  const tipo = pkg ? 'node' : pyTxt ? 'python' : 'otro';
  const pm = existsSync(join(dir, 'pnpm-lock.yaml')) ? 'pnpm' : existsSync(join(dir, 'yarn.lock')) ? 'yarn'
    : existsSync(join(dir, 'bun.lockb')) || existsSync(join(dir, 'bun.lock')) ? 'bun' : 'npm';
  const framework = ['next', 'astro', '@sveltejs/kit', '@remix-run/node', 'nuxt', 'vite', 'express', 'fastify', 'hono']
    .find((f) => deps[f]) ?? (/fastapi/i.test(pyTxt) ? 'fastapi' : /django/i.test(pyTxt) ? 'django' : /flask/i.test(pyTxt) ? 'flask' : null);
  const ficheros = recorrer(dir);
  const fuentes = ficheros.filter((f) => FUENTE.test(f));

  const comandoDev = scripts.dev ?? scripts.start ?? null;
  let puerto = Number((comandoDev ?? '').match(/(?:-p|--port)[ =](\d{2,5})/)?.[1] ?? (comandoDev ?? '').match(/PORT=(\d{2,5})/)?.[1]) || null;
  if (!puerto) for (const f of fuentes.slice(0, 400)) {
    const m = (leer(f) ?? '').match(/PORT\s*(?:\?\?|\|\|)\s*['"]?(\d{2,5})/);
    if (m) { puerto = Number(m[1]); break; }
  }
  puerto ??= { next: 3000, astro: 4321, vite: 5173, '@sveltejs/kit': 5173, nuxt: 3000, '@remix-run/node': 3000, fastapi: 8000, django: 8000, flask: 5000 }[framework] ?? 3000;

  const runnerTest = ['vitest', 'jest', '@playwright/test', 'mocha', 'ava'].find((r) => deps[r])
    ?? (/node --test/.test(scripts.test ?? '') ? 'node:test' : /pytest/i.test(pyTxt) ? 'pytest' : null);

  const datos = [
    ...ficheros.filter((f) => /\.(db|sqlite3?)$/.test(f)),
    ...ficheros.filter((f) => /[\\/]data[\\/][^\\/]+\.json$/.test(f)),
    ...['prisma/schema.prisma', 'drizzle.config.ts', 'drizzle.config.js', 'supabase/config.toml'].map((f) => join(dir, f)).filter(existsSync),
  ].map((f) => rel(dir, f));

  const indiciosDespliegue = ['vercel.json', '.vercel', 'netlify.toml', 'fly.toml', 'render.yaml', 'Dockerfile', 'docker-compose.yml', 'Procfile', 'app.yaml']
    .filter((f) => existsSync(join(dir, f)));
  const flujos = join(dir, '.github', 'workflows');
  if (existsSync(flujos)) for (const f of readdirSync(flujos)) if (/deploy|release|prod/i.test(f + (leer(join(flujos, f)) ?? ''))) { indiciosDespliegue.push(`.github/workflows/${f}`); break; }

  const ia = Object.keys(deps).some((d) => /^(openai|@anthropic-ai\/|ai$|@ai-sdk\/|langchain|@langchain\/|@google\/(generative-ai|genai)|ollama|replicate|groq-sdk)/.test(d))
    || /\b(openai|anthropic|langchain|google-genai|groq)\b/i.test(pyTxt);

  const zonasCriticas = fuentes.filter((f) => esCritico(f, rel(dir, f))).map((f) => rel(dir, f)).slice(0, 40);
  const agentes = existsSync(join(dir, '.claude', 'agents')) ? readdirSync(join(dir, '.claude', 'agents')).filter((n) => n.endsWith('.md')) : [];
  const claude = leer(join(dir, 'CLAUDE.md'));
  return {
    dir, nombre: pkg?.name ?? basename(dir), tipo, pm, framework, comandoDev: comandoDev ? `${pm} run ${scripts.dev ? 'dev' : 'start'}` : null,
    puerto, runnerTest, scriptTest: scripts.test ? `${pm} run test` : null, playwright: !!(deps['playwright-core'] || deps.playwright || deps['@playwright/test']),
    datos, desplegado: indiciosDespliegue.length > 0, indiciosDespliegue, ia, zonasCriticas, git: existsSync(join(dir, '.git')),
    existentes: {
      prd: existsSync(join(dir, 'specs', 'PRD.md')), fases: existsSync(join(dir, 'specs', 'FASES.md')),
      claudeMd: claude !== null, noSeNegocia: /^## NO SE NEGOCIA/m.test(claude ?? ''), agentes,
      rumbo: existsSync(join(dir, '.claude', 'skills', 'rumbo', 'SKILL.md')), e2e: existsSync(join(dir, 'e2e')),
    },
  };
}

// ───────────────────────── instalar
const PIEZAS = {
  F0: [['specs/FASES.md', 'specs/FASES.md']],
  S1: [['specs/PRD.md', 'specs/PRD.md'], ['specs/features/fr-01-rd.md', 'specs/features/fr-01-rd.md']],
  S2: [['.claude/skills/rumbo/SKILL.plantilla.md', '.claude/skills/rumbo/SKILL.md']],
  S4: [],
  S3: [],
  S5: [['.claude/agents/rompedor.md', '.claude/agents/rompedor.md']],
  S6: [['.claude/agents/primerizo.md', '.claude/agents/primerizo.md'], ['.claude/hooks/primerizo-guardia.mjs', '.claude/hooks/primerizo-guardia.mjs'],
    ['.claude/hooks/primerizo-guardia.test.mjs', '.claude/hooks/primerizo-guardia.test.mjs'], ['.claude/primerizo/navegador.mjs', '.claude/primerizo/navegador.mjs'],
    ['.claude/agents/recorredor.md', '.claude/agents/recorredor.md']],
  S7: [['.claude/agents/triador.md', '.claude/agents/triador.md']],
};

export function instalar(dir, { sistemas = Object.keys(PIEZAS), tipo = null, prePush = false } = {}) {
  if (!existsSync(PLANTILLAS)) throw new Error('instalar se ejecuta desde la skill /fases (no encuentro plantillas/).');
  dir = resolve(dir);
  const d = detectar(dir);
  const fecha = new Date().toISOString().slice(0, 10);
  const tipoFinal = tipo ?? (d.desplegado ? 'desplegado' : 'nuevo');
  const sustituir = (texto) => texto
    .replaceAll('http://localhost:3000', `http://localhost:${d.puerto}`)
    .replaceAll('`<RUTA_DATOS>`', d.datos.length ? d.datos.map((x) => `\`${x}\``).join(', ') : '`<RUTA_DATOS>`')
    .replace('Tipo: <nuevo|desplegado>', `Tipo: ${tipoFinal}`)
    .replace('Perfil: <crítico|normal>', `Perfil: ${d.zonasCriticas.length || d.ia ? 'crítico' : 'normal'}`)
    .replace('Rama de trabajo: <fases/AAAA-MM-DD>', `Rama de trabajo: ${d.git ? `fases/${fecha}` : '(sin git: se trabaja en sitio)'}`);
  const resultado = { creado: [], igual: [], conflicto: [], pendiente: [], tipo: tipoFinal };
  const poner = (destRel, contenido) => {
    const dest = join(dir, destRel);
    mkdirSync(dirname(dest), { recursive: true });
    const actual = leer(dest);
    if (actual === null) { writeFileSync(dest, contenido); resultado.creado.push(destRel); }
    else if (actual === contenido) resultado.igual.push(destRel);
    else if (destRel === 'specs/PRD.md' || destRel === 'specs/FASES.md' || destRel.startsWith('specs/features/')) resultado.igual.push(`${destRel} (ya existe, se respeta)`);
    else { writeFileSync(`${dest}.fases-nuevo`, contenido); resultado.conflicto.push(destRel); }
    if (/<RUTA_DATOS>/.test(leer(dest) ?? '')) resultado.pendiente.push(`${destRel} (<RUTA_DATOS>)`);
  };

  for (const s of ['F0', ...sistemas.filter((s) => s !== 'F0')]) {
    for (const [origen, destino] of PIEZAS[s] ?? []) {
      if (destino === 'specs/features/fr-01-rd.md' && existsSync(join(dir, 'specs', 'features')) && readdirSync(join(dir, 'specs', 'features')).length) continue;
      const texto = readFileSync(join(PLANTILLAS, origen), 'utf8');
      poner(destino, /\.(md)$/.test(destino) ? sustituir(texto) : texto);
    }
    if (s === 'S4') {
      const claudeMd = join(dir, 'CLAUDE.md');
      const actual = leer(claudeMd);
      if (/^## NO SE NEGOCIA/m.test(actual ?? '')) resultado.igual.push('CLAUDE.md (NO SE NEGOCIA ya está)');
      else {
        const bloque = readFileSync(join(PLANTILLAS, 'CLAUDE-no-se-negocia.md'), 'utf8');
        if (actual === null) writeFileSync(claudeMd, `# ${d.nombre}\n\n${bloque}`);
        else appendFileSync(claudeMd, `${actual.endsWith('\n') ? '' : '\n'}\n${bloque}`);
        resultado.creado.push('CLAUDE.md § NO SE NEGOCIA');
      }
      mkdirSync(join(dir, 'tests', 'no-se-negocia'), { recursive: true });
    }
  }
  const copia = join(dir, '.claude', 'fases', 'fases.mjs');
  mkdirSync(dirname(copia), { recursive: true });
  copyFileSync(fileURLToPath(import.meta.url), copia);
  if (prePush && d.git) {
    const gancho = join(dir, '.git', 'hooks', 'pre-push');
    if (existsSync(gancho)) resultado.conflicto.push('.git/hooks/pre-push (ya hay uno: no se toca)');
    else { writeFileSync(gancho, '#!/bin/sh\nnode .claude/fases/fases.mjs publicar || { echo "fases: la puerta de publicar dice NO"; exit 1; }\n', { mode: 0o755 }); resultado.creado.push('.git/hooks/pre-push'); }
  }
  return { ...resultado, deteccion: d };
}

// ───────────────────────── verificar
const E = { activo: '✅', parcial: '🟨', ausente: '⬜', noAplica: '➖' };

function umbral(fases) { return Number((fases.match(/Sesiones sin error[^*\n]*\*\*\s*(\d+(?:[.,]\d+)?)\s*%/)?.[1] ?? '95').replace(',', '.')); }

export function verificar(dir, { correrTests = true } = {}) {
  dir = resolve(dir);
  const fases = leer(join(dir, 'specs', 'FASES.md')) ?? '';
  const prd = leer(join(dir, 'specs', 'PRD.md'));
  const claude = leer(join(dir, 'CLAUDE.md')) ?? '';
  const r = {};
  const fila = (estado, evidencia = [], falta = []) => ({ estado, evidencia, falta });

  // F0
  const tipo = fases.match(/^Tipo:\s*(nuevo|desplegado)\s*$/m)?.[1];
  const perfil = fases.match(/^Perfil:\s*(crítico|normal)\s*$/m)?.[1];
  r.F0 = !fases ? fila('ausente', [], ['specs/FASES.md']) : tipo && perfil ? fila('activo', [`Tipo ${tipo} · Perfil ${perfil}`]) : fila('parcial', ['specs/FASES.md'], ['Tipo y Perfil']);

  // S1
  if (!prd) r.S1 = fila('ausente', [], ['specs/PRD.md']);
  else {
    const falta = [];
    const objetivos = prd.split(/\r?\n/).filter((l) => /^\|\s*G\d+\s*\|/.test(l) && !MARCADOR.test(l) && l.split('|').slice(2, 4).every((c) => c.trim()));
    if (!objetivos.length) falta.push('objetivo con métrica');
    if (!seccion(prd, /No-objetivos/i).split(/\r?\n/).some((l) => /^-\s+\S/.test(l) && !MARCADOR.test(l))) falta.push('no-objetivos');
    if (!seccion(prd, /Caminos críticos/i).split(/\r?\n/).some((l) => /^\d+\.\s+\S/.test(l) && !MARCADOR.test(l))) falta.push('caminos críticos');
    const fdir = join(dir, 'specs', 'features');
    const rds = existsSync(fdir) ? readdirSync(fdir).filter((n) => n.endsWith('.md')).filter((n) => (leer(join(fdir, n)) ?? '').split(/\r?\n/).some((l) => /^-\s*R\d+\.\s*\S/.test(l) && !MARCADOR.test(l))) : [];
    if (!rds.length) falta.push('RD con requisitos R1.');
    r.S1 = fila(falta.length ? 'parcial' : 'activo', [`PRD: ${objetivos.length} objetivos`, `${rds.length} RDs`], falta);
  }

  // S4
  const bloque = seccion(claude, /^## NO SE NEGOCIA/);
  if (!/^## NO SE NEGOCIA/m.test(claude)) r.S4 = fila('ausente', [], ['CLAUDE.md § NO SE NEGOCIA']);
  else {
    const reglas = bloque.split(/\r?\n/).filter((l) => /^\|\s*\d+\s*\|/.test(l));
    const sinTest = [];
    for (const l of reglas) {
      const ruta = [...l.matchAll(/`(tests\/[^`]+)`/g)].at(-1)?.[1];
      const prefijo = ruta?.replace(/\.\*$/, '').replace(/\*.*$/, '');
      const carpeta = prefijo ? join(dir, dirname(prefijo)) : null;
      const ok = carpeta && existsSync(carpeta) && readdirSync(carpeta).some((n) => n.startsWith(basename(prefijo)));
      if (!ok || MARCADOR.test(l)) sinTest.push(l.split('|')[1].trim());
    }
    r.S4 = fila(reglas.length && !sinTest.length ? 'activo' : 'parcial', [`${reglas.length} reglas`], sinTest.length ? [`test o umbral en reglas ${sinTest.join(', ')}`] : []);
  }

  // S2
  const rumbo = ultimoInforme(dir, 'specs/rumbo');
  const veredicto = rumbo?.texto.match(/^Rumbo:\s*(🟢|🟡|🔴)/mu)?.[1];
  if (!existsSync(join(dir, '.claude', 'skills', 'rumbo', 'SKILL.md'))) r.S2 = fila('ausente', [], ['skill rumbo']);
  else r.S2 = veredicto === '🟢' || veredicto === '🟡' ? fila('activo', [`${rel(dir, rumbo.fichero)}: ${veredicto}`])
    : fila('parcial', rumbo ? [`${rel(dir, rumbo.fichero)}: ${veredicto ?? 'sin veredicto'}`] : ['skill instalada'], [veredicto === '🔴' ? 'volver a 🟢' : 'ejecutar /rumbo']);

  // S3
  const zonas = seccion(fases, /^## Zonas/);
  const zonasOk = zonas && !/<rutas>/.test(zonas) && zonas.split(/\r?\n/).filter((l) => /^\|\s*(crítica|normal|menor)\s*\|/.test(l)).length >= 1;
  const seg = ultimoInforme(dir, 'specs/seguridad');
  const segM = seg?.texto.match(/^Seguridad · críticos (\d+) · altos (\d+)/mu);
  const f3 = [];
  if (!zonasOk) f3.push('rellenar Zonas en FASES.md');
  if (!segM) f3.push('informe en specs/seguridad/ con la línea "Seguridad · críticos N · altos N"');
  r.S3 = fila(!f3.length ? 'activo' : zonasOk || segM ? 'parcial' : 'ausente', segM ? [`${rel(dir, seg.fichero)}: críticos ${segM[1]} · altos ${segM[2]}`] : [], f3);

  // S5
  const rompedorMd = leer(join(dir, '.claude', 'agents', 'rompedor.md'));
  const rom = ultimoInforme(dir, 'specs/rompedor');
  const altas = rom?.texto.match(/^Rompedor ·.*?(\d+)\s*altas/mu)?.[1];
  const restaurado = /restaurados?\s*✅/u.test(rom?.texto ?? '');
  if (!rompedorMd) r.S5 = fila('ausente', [], ['agente rompedor']);
  else {
    const f5 = [];
    if (rompedorMd.includes('<RUTA_DATOS>')) f5.push('rellenar <RUTA_DATOS> en rompedor.md');
    if (altas === undefined) f5.push('ejecutar el rompedor');
    else if (altas !== '0') f5.push(`${altas} altas abiertas`);
    if (rom && !restaurado) f5.push('datos restaurados ✅ en el informe');
    r.S5 = fila(f5.length ? 'parcial' : 'activo', rom ? [`${rel(dir, rom.fichero)}: ${altas} altas`] : ['agente instalado'], f5);
  }

  // S6
  const guardiaTest = join(dir, '.claude', 'hooks', 'primerizo-guardia.test.mjs');
  if (!existsSync(join(dir, '.claude', 'agents', 'primerizo.md')) && !existsSync(join(dir, '.claude', 'agents', 'recorredor.md'))) r.S6 = fila('ausente', [], ['primerizo y recorredor']);
  else {
    const f6 = [], ev6 = [];
    if (correrTests && existsSync(guardiaTest)) {
      const t = spawnSync(process.execPath, ['--test', guardiaTest], { cwd: dir, encoding: 'utf8' });
      if (t.status === 0) ev6.push('guardia: tests verdes'); else f6.push('tests de la guardia en rojo');
    } else if (!existsSync(guardiaTest)) f6.push('guardia del primerizo');
    const pri = ultimoInforme(dir, 'specs/primerizo');
    if (!pri) f6.push('informe del primerizo');
    else if (!/^Guardia:.*Bloqueado/mu.test(pri.texto)) f6.push('el informe del primerizo no prueba la guardia (línea "Guardia: Bloqueado…")');
    else ev6.push(`${rel(dir, pri.fichero)}: guardia en vivo ✅`);
    const lj = leerJson(join(dir, 'e2e', 'results', 'latest.json'));
    const pct = Number(lj?.sesiones_sin_error_pct);
    const u = umbral(fases);
    if (!lj) f6.push('e2e/results/latest.json (recorredor)');
    else if (!(pct >= u)) f6.push(`sesiones sin error ${pct} % < ${u} %`);
    else ev6.push(`sesiones sin error ${pct} % ≥ ${u} %`);
    r.S6 = fila(f6.length ? 'parcial' : 'activo', ev6, f6);
  }

  // S7
  const fuentes = seccion(fases, /^## Fuentes de clientes/);
  const registro = leer(join(dir, 'specs', 'clientes', 'registro.md')) ?? '';
  if (tipo === 'nuevo' && /➖/u.test(fuentes)) r.S7 = fila('noAplica', ['sin clientes todavía']);
  else if (/^\|\s*\d{4}-\d{2}-\d{2}/m.test(registro)) r.S7 = fila('activo', ['specs/clientes/registro.md']);
  else r.S7 = fila(existsSync(join(dir, '.claude', 'agents', 'triador.md')) ? 'parcial' : 'ausente', [], ['fuentes reales y una pasada del triador']);

  if (fases) escribirEstado(dir, fases, r);
  return r;
}

const NOMBRES = { F0: 'F0 Diagnóstico', S1: 'S1 Plan y prueba', S4: 'S4 No se negocia', S2: 'S2 Rumbo', S3: 'S3 Seguridad y puertas', S5: 'S5 Rompedor', S6: 'S6 Ojos de cliente', S7: 'S7 Bucle de cliente' };
function tabla(r) {
  return ['| Sistema | Estado | Evidencia | Falta |', '|---|---|---|---|',
    ...Object.keys(NOMBRES).map((k) => `| ${NOMBRES[k]} | ${E[r[k].estado]} | ${r[k].evidencia.join(' · ')} | ${r[k].falta.join(' · ')} |`)].join('\n');
}
function escribirEstado(dir, fases, r) {
  const nuevo = fases.replace(/<!-- estado:inicio -->[\s\S]*?<!-- estado:fin -->/, `<!-- estado:inicio -->\n${tabla(r)}\n<!-- estado:fin -->`);
  if (nuevo !== fases) writeFileSync(join(dir, 'specs', 'FASES.md'), nuevo);
}

// ───────────────────────── publicar
function horaDelCodigo(dir) {
  const fuera = [':(exclude)specs', ':(exclude)e2e/results', ':(exclude).claude', ':(exclude)rompedor-backup'];
  let ms = 0;
  if (existsSync(join(dir, '.git'))) {
    const log = spawnSync('git', ['log', '-1', '--format=%ct', '--', '.', ...fuera], { cwd: dir, encoding: 'utf8' });
    ms = Number(log.stdout.trim()) * 1000 || 0;
    const sucios = spawnSync('git', ['status', '--porcelain', '--', '.', ...fuera], { cwd: dir, encoding: 'utf8' }).stdout.split(/\r?\n/).filter(Boolean);
    for (const l of sucios) ms = Math.max(ms, mtime(join(dir, l.slice(3).trim().replace(/^"|"$/g, ''))));
  } else for (const f of recorrer(dir)) if (FUENTE.test(f)) ms = Math.max(ms, mtime(f));
  return ms;
}

function zonaCriticaTocada(dir, fases) {
  const zonas = seccion(fases, /^## Zonas/).split(/\r?\n/).find((l) => /^\|\s*crítica\s*\|/.test(l)) ?? '';
  const rutas = [...zonas.split('|')[3]?.matchAll(/`?([\w./\-[\]*]+)`?/g) ?? []].map((m) => m[1].replace(/\*.*$/, '')).filter((x) => x && x !== '<rutas>');
  let cambiados = [];
  if (existsSync(join(dir, '.git'))) {
    const tag = spawnSync('git', ['describe', '--tags', '--match', 'fases-publicado-*', '--abbrev=0'], { cwd: dir, encoding: 'utf8' }).stdout.trim();
    const args = tag ? ['diff', '--name-only', tag] : ['ls-files'];
    cambiados = spawnSync('git', args, { cwd: dir, encoding: 'utf8' }).stdout.split(/\r?\n/).filter(Boolean);
    cambiados.push(...spawnSync('git', ['status', '--porcelain'], { cwd: dir, encoding: 'utf8' }).stdout.split(/\r?\n/).filter(Boolean).map((l) => l.slice(3)));
  } else cambiados = recorrer(dir).filter((f) => FUENTE.test(f)).map((f) => rel(dir, f));
  return [...new Set(cambiados)].filter((c) => FUENTE.test(c) && (rutas.some((r) => c.startsWith(r)) || esCritico(join(dir, c), c)));
}

export function publicar(dir, { correrTests = true } = {}) {
  dir = resolve(dir);
  const fases = leer(join(dir, 'specs', 'FASES.md')) ?? '';
  const codigo = horaDelCodigo(dir);
  const frescura = (ms) => ms >= codigo;
  const pasos = [];
  const paso = (nombre, ok, detalle) => pasos.push({ nombre, ok, detalle });

  const rumbo = ultimoInforme(dir, 'specs/rumbo');
  const v = rumbo?.texto.match(/^Rumbo:\s*(🟢|🟡|🔴)/mu)?.[1];
  paso('Rumbo', !!rumbo && (v === '🟢' || v === '🟡') && frescura(rumbo.ms),
    !rumbo ? 'no hay informe de /rumbo' : !frescura(rumbo.ms) ? `informe ${v} anterior al último cambio de código: pasa /rumbo cambios` : `${v} (${basename(rumbo.fichero)})`);

  const pkg = leerJson(join(dir, 'package.json'));
  if (!correrTests) paso('Tests', true, 'omitidos (--sin-tests)');
  else if (pkg?.scripts?.test) {
    const pm = detectar(dir).pm;
    const t = spawnSync(`${pm} run test`, { cwd: dir, encoding: 'utf8', shell: true });
    paso('Tests', t.status === 0, `${pm} run test → salida ${t.status}\n${(t.stdout + t.stderr).trim().split(/\r?\n/).slice(-6).join('\n')}`);
  } else paso('Tests', false, 'el proyecto no tiene script "test"');
  const v4 = verificar(dir, { correrTests: false }).S4;
  paso('No se negocia', v4.estado === 'activo', v4.estado === 'activo' ? v4.evidencia.join(' · ') : v4.falta.join(' · ') || 'falta el bloque');

  const seg = ultimoInforme(dir, 'specs/seguridad');
  const s = seg?.texto.match(/^Seguridad · críticos (\d+) · altos (\d+)/mu);
  paso('Seguridad', !!s && s[1] === '0' && s[2] === '0' && frescura(seg.ms),
    !s ? 'no hay informe en specs/seguridad/' : !frescura(seg.ms) ? 'escaneo anterior al último cambio: pasa /claude-security' : `críticos ${s[1]} · altos ${s[2]}`);

  const lj = leerJson(join(dir, 'e2e', 'results', 'latest.json'));
  const u = umbral(fases);
  const pct = Number(lj?.sesiones_sin_error_pct);
  const ljMs = mtime(join(dir, 'e2e', 'results', 'latest.json'));
  paso('Recorredor', !!lj && pct >= u && frescura(ljMs),
    !lj ? 'no hay e2e/results/latest.json' : !frescura(ljMs) ? 'resultados anteriores al último cambio: relanza el recorredor' : `${pct} % sin error (umbral ${u} %)`);

  const criticos = zonaCriticaTocada(dir, fases);
  if (criticos.length) {
    const rom = ultimoInforme(dir, 'specs/rompedor');
    const altas = rom?.texto.match(/^Rompedor ·.*?(\d+)\s*altas/mu)?.[1];
    paso('Rompedor (zona crítica tocada)', altas === '0' && frescura(rom.ms),
      `${criticos.slice(0, 5).join(', ')}${criticos.length > 5 ? '…' : ''} → ${!rom ? 'sin informe' : !frescura(rom.ms) ? 'informe anterior al cambio' : `${altas} altas`}`);
  } else paso('Rompedor', true, 'el cambio no toca zona crítica');

  return { puede: pasos.every((p) => p.ok), pasos };
}

// ───────────────────────── CLI
const esPrincipal = process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (esPrincipal) {
  const [, , orden, ...resto] = process.argv;
  const flags = resto.filter((a) => a.startsWith('--'));
  const dir = resto.find((a) => !a.startsWith('--') && !/^S\d|^nuevo|^desplegado/.test(a)) ?? process.cwd();
  const valor = (f) => resto[resto.indexOf(f) + 1];
  const json = flags.includes('--json');
  const salida = (o, texto) => console.log(json ? JSON.stringify(o, null, 2) : texto);
  try {
    if (orden === 'detectar') { const d = detectar(dir); salida(d, Object.entries(d).map(([k, v]) => `${k}: ${typeof v === 'object' ? JSON.stringify(v) : v}`).join('\n')); }
    else if (orden === 'instalar') {
      const r = instalar(dir, { sistemas: flags.includes('--sistemas') ? valor('--sistemas').split(',') : undefined, tipo: flags.includes('--tipo') ? valor('--tipo') : null, prePush: flags.includes('--pre-push') });
      salida(r, [`Tipo: ${r.tipo} · puerto ${r.deteccion.puerto} · datos: ${r.deteccion.datos.join(', ') || '(no detectados)'}`,
        `Creado (${r.creado.length}): ${r.creado.join(', ') || '—'}`, `Ya estaba (${r.igual.length}): ${r.igual.join(', ') || '—'}`,
        `Conflicto → *.fases-nuevo (${r.conflicto.length}): ${r.conflicto.join(', ') || '—'}`, `Con <marcadores> por rellenar: ${r.pendiente.join(', ') || '—'}`].join('\n'));
    } else if (orden === 'verificar') { const r = verificar(dir); salida(r, tabla(r)); }
    else if (orden === 'publicar') {
      const r = publicar(dir, { correrTests: !flags.includes('--sin-tests') });
      salida(r, [...r.pasos.map((p) => `${p.ok ? '✅' : '❌'} ${p.nombre}: ${p.detalle}`), '', r.puede ? 'PUEDE PUBLICARSE' : 'NO SE PUEDE PUBLICAR'].join('\n'));
      process.exit(r.puede ? 0 : 1);
    } else { console.error('Órdenes: detectar | instalar | verificar | publicar  [dir]  (ver cabecera del script)'); process.exit(2); }
  } catch (e) { console.error(`fases: ${e.message}`); process.exit(2); }
}
