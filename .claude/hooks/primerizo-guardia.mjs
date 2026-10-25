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
// primerizo leyó CLAUDE.md así). Por eso la guardia también vale registrada en un settings.json: allí salta para todas las
// sesiones y solo actúa cuando la entrada trae `agent_type: "primerizo"` (campo común de la entrada de los hooks).
import { readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, resolve } from 'node:path';

const NAVEGADOR = /^node\s+\.claude[\\/]+primerizo[\\/]+navegador\.mjs(\s|$)/;
const METACARACTERES = /[;&|`<>\n\r]|\$\(/;
const CAPTURA = /^primerizo-[\w-]+\.png$/i;

export function decidir({ tool_name, tool_input = {}, agent_type }) {
  // Registrada en settings.json, la guardia ve también la sesión principal y los demás agentes: a esos no los toca.
  if (agent_type !== undefined && agent_type !== 'primerizo') return null;
  if (tool_name === 'Bash') {
    const cmd = String(tool_input.command ?? '').trim();
    if (!NAVEGADOR.test(cmd)) return 'Bash solo puede ejecutar `node .claude/primerizo/navegador.mjs <orden>`. Usa la app como un cliente: abrir, clic, escribir, tecla, ver.';
    if (METACARACTERES.test(cmd)) return 'Nada de encadenar, redirigir ni subshells (; & | ` < > $( ). Una orden del navegador por llamada.';
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
  const motivo = decidir(entrada);
  if (motivo) { process.stderr.write(`Bloqueado: ${motivo}`); process.exit(2); }
  process.exit(0);
}
