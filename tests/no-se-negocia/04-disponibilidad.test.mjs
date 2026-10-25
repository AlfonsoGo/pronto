// Regla 4 · Disponibilidad (CLAUDE.md § NO SE NEGOCIA).
// 100 % de las llamadas ureq con timeout, y si Ollama no responde en su plazo el dictado se pega sin
// pulir. Nunca se toca el Ollama real (127.0.0.1:11434): servidores TCP locales en puertos libres.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { cargoTest, exigirCargoVerde, rustDeProduccion } from './_comun.mjs';

/** Trozo de código que construye una petición: desde el agente/llamada hasta el `;` que la cierra. */
function sentencia(texto, desde) {
  const fin = texto.indexOf(';', desde);
  return texto.slice(desde, fin < 0 ? texto.length : fin);
}

test('04 disponibilidad: toda llamada de red (ureq) lleva timeout', () => {
  const sin = [];
  let agentes = 0;
  for (const { fichero, texto } of rustDeProduccion()) {
    // Agentes: `AgentBuilder::new()...build()` con timeout de conexión y de lectura (o global).
    for (const m of texto.matchAll(/ureq::AgentBuilder::new\(\)/g)) {
      agentes++;
      const s = sentencia(texto, m.index);
      const conecta = /\.timeout_connect\(/.test(s) || /\.timeout\(/.test(s);
      const lee = /\.timeout_read\(/.test(s) || /\.timeout\(/.test(s);
      if (!conecta || !lee) sin.push(`${fichero}: agente sin ${conecta ? '' : 'timeout de conexión '}${lee ? '' : 'timeout de lectura'}\n    ${s.replace(/\s+/g, ' ').slice(0, 160)}`);
    }
    // Atajos que usan el agente por defecto de ureq, que NO tiene timeout.
    for (const m of texto.matchAll(/ureq::(get|post|put|delete|head|patch|request|request_url|agent)\s*\(/g)) {
      sin.push(`${fichero}: ureq::${m[1]}() usa el agente por defecto, sin timeout`);
    }
    for (const m of texto.matchAll(/AgentBuilder::new\(\)/g)) {
      if (texto.slice(Math.max(0, m.index - 6), m.index) !== 'ureq::') sin.push(`${fichero}: AgentBuilder importado sin prefijo; revisa su timeout a mano`);
    }
  }
  assert.ok(agentes > 0, 'no se encontró ningún agente ureq: ¿cambió la librería de red? Actualiza este test');
  assert.deepEqual(sin, [], `Llamadas de red sin timeout:\n${sin.join('\n')}`);
});

test('04 disponibilidad: Ollama colgado o caído → se pega sin pulir dentro del plazo (Rust)', { timeout: 30 * 60_000 }, () => {
  const r = cargoTest('nsn_disponibilidad');
  exigirCargoVerde(assert, r, 'nsn_disponibilidad');
  for (const t of ['ollama_que_no_responde_pega_sin_pulir_en_plazo', 'ollama_caido_pega_sin_pulir_al_momento', 'estado_de_ollama_no_cuelga']) {
    assert.match(r.salida, new RegExp(`nsn_disponibilidad_${t} \\.\\.\\. ok`), `no se ejecutó nsn_disponibilidad_${t}`);
  }
});
