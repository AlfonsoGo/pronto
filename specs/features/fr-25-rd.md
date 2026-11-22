# FR-25 · Modo comando sin selección

> Origen: Wispr Flow (Command Mode) y Glaido · Ticket: PRNT-47 · Investigación: `docs/ideas-glaido-wispr-2026-11-09.md` · REA: observabilidad alta (Wispr)
> Estado: PROPUESTA (9-nov-2026). Nada construido. Lo que diga REA se apunta como visto/inferido en `clon-wispr-flow/`.

## Qué hace (lenguaje del usuario)
Sin seleccionar nada, digo «escríbeme un correo para…» y la IA local lo redacta; se pega solo si pulso Enter.

## Requisitos
- R1. Nuevo atajo; sin selección, la orden hablada genera texto con Ollama.
- R2. El resultado sale en la barra y solo se pega con Enter (como Reescribir).
- R3. Sin Ollama: aviso claro, nunca un pegado vacío.
- R4. Test: con selección se comporta como Reescribir; sin ella, genera.

## NO SE NEGOCIA que toca
- Regla 7: nunca se pega sin confirmar.

## Dónde vivirá
- Por decidir al construir (leer `docs/ARQUITECTURA.md` antes).
