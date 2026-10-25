#!/usr/bin/env node
// Guardia PreToolUse del agente `primerizo` (sistema 6, «ojos de cliente nuevo»).
// El primerizo vale porque NO sabe nada de la app. Esta guardia lo mantiene ciego:
//   · Bash solo puede ejecutar el navegador: `node .claude/primerizo/navegador.mjs ...`
//     (sin encadenar, sin redirigir, sin subshells).
//   · Read solo puede abrir sus propias capturas: <tmp>/primerizo-*.png
//   · Cualquier otra herramienta, bloqueada.
// Salida 0 = permitido · salida 2 = bloqueado (el motivo va por stderr y lo lee el agente).
//
// Dónde se engancha: en el frontmatter de `.claude/agents/primerizo.md`. Claude Code se SALTA en silencio los hooks del
// frontmatter de un agente de proyecto mientras la carpeta no tenga aceptado el diálogo de confianza (24-oct-2026: el
// primerizo leyó CLAUDE.md así; 25-oct-2026: tampoco se aplicaban con `claude -p`, ni con --agent ni como subagente). Por eso
// también va registrada en `.claude/settings.json` con `--solo-primerizo`: allí salta para todas las sesiones y solo actúa
// cuando la entrada trae `agent_type: "primerizo"` (campo común de la entrada de los hooks). Sin esa marca, una entrada sin
// `agent_type` (la sesión principal) se trataba como primerizo y se bloqueaba entera.
import { readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, resolve } from 'node:path';

const NAVEGADOR = /^node\s+\.claude[\\/]+primerizo[\\/]+navegador\.mjs(\s|$)/;
const METACARACTERES = /[;&|`<>\n\r$\\]/;
const CAPTURA = /^primerizo-[\w-]+\.png$/i;

export function decidir({ tool_name, tool_input, agent_type }, { soloPrimerizo = false } = {}) {
  tool_input = tool_input ?? {};
  // Registrada en settings.json, la guardia ve también la sesión principal y los demás agentes: a esos no los toca.
  if (soloPrimerizo ? agent_type !== 'primerizo' : agent_type !== undefined && agent_type !== 'primerizo') return null;
  if (tool_name === 'Bash') {
    const cmd = String(tool_input.command ?? '').trim();
    if (!NAVEGADOR.test(cmd)) return 'Bash solo puede ejecutar `node .claude/primerizo/navegador.mjs <orden>`. Usa la app como un cliente: abrir, clic, escribir, tecla, ver.';
    if (METACARACTERES.test(cmd.replace(NAVEGADOR, ""))) return 'Nada de encadenar, redirigir ni subshells (; & | ` < > $ ni barra invertida). Una orden del navegador por llamada.';
    // Fuera de comillas, bash expandiría comodines y ~ a nombres de ficheros del PC (rompedor 25-oct-b, BAJA 3).
    if (/[*?[\]{}~]/.test(cmd.replace(/"[^"]*"|'[^']*'/g, ''))) return 'Pon entre comillas lo que escribes o pulsas: sin comillas, * ? [ ] { } ~ se convierten en nombres de ficheros.';
    return null;
  }
  if (tool_name === 'Read') {
    const ruta = resolve(String(tool_input.file_path ?? ''));
    const mismaCarpeta = dirname(ruta).toLowerCase() === resolve(tmpdir()).toLowerCase();
    if (mismaCarpeta && CAPTURA.test(basename(ruta))) return null;
    return 'Solo puedes abrir tus propias capturas (las rutas que imprime el navegador). El código, los specs y la documentación están fuera de tu alcance: tu valor es no conocerlos.';
  }
  return `La herramienta ${tool_name} está bloqueada para el primerizo.`;
}

const esPrincipal = process.argv[1] && resolve(process.argv[1]) === resolve(new URL(import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1'));
if (esPrincipal) {
  let entrada;
  try { entrada = JSON.parse(readFileSync(0, 'utf8')); }
  catch { process.stderr.write('Guardia del primerizo: entrada ilegible, bloqueo por defecto.'); process.exit(2); }
  let motivo;
  // Un fallo interno con el primerizo bloquea (falla cerrada); para cualquier otra sesión, deja pasar (rompedor 25-oct, MEDIA 2).
  try { motivo = decidir(entrada ?? {}, { soloPrimerizo: process.argv.includes('--solo-primerizo') }); }
  catch (e) { motivo = entrada?.agent_type === 'primerizo' || !process.argv.includes('--solo-primerizo') ? `entrada inesperada (${e.message})` : null; }
  if (motivo) { process.stderr.write(`Bloqueado: ${motivo}`); process.exit(2); }
  process.exit(0);
}
