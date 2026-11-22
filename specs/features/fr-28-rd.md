# FR-28 · Conexión MCP local con las reuniones

> Origen: Wispr Notetaker (MCP) y Glaido (tools) · Ticket: PRNT-48 · Investigación: `docs/ideas-glaido-wispr-2026-11-09.md` · REA: observabilidad media
> Estado: PROPUESTA (9-nov-2026). Nada construido. Lo que diga REA se apunta como visto/inferido en `clon-wispr-flow/`.

## Qué hace (lenguaje del usuario)
Claude Code o Claude Desktop pueden leer mis reuniones de Pronto si yo lo enciendo.

## Requisitos
- R1. Apagado de fábrica. Servidor MCP por stdio, solo lectura, solo reuniones (nunca dictados).
- R2. Al encenderlo, aviso: «la IA que conectes puede mandar este texto a su nube».
- R3. Herramientas: listar reuniones, leer notas, buscar.
- R4. Test: apagado no abre nada; encendido no devuelve dictados.

## NO SE NEGOCIA que toca
- Regla 1: DECIDE TXONSO antes de construir (el dato puede salir por la IA conectada).

## Dónde vivirá
- Por decidir al construir (leer `docs/ARQUITECTURA.md` antes).
