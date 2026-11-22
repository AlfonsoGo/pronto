# FR-29 · «¿Qué me he perdido?» en directo

> Origen: Wispr Notetaker · Ticket: PRNT-48 · Investigación: `docs/ideas-glaido-wispr-2026-11-09.md` · REA: observabilidad media
> Estado: PROPUESTA (9-nov-2026). Nada construido. Lo que diga REA se apunta como visto/inferido en `clon-wispr-flow/`.

## Qué hace (lenguaje del usuario)
En mitad de una reunión pregunto qué me he perdido y me resume lo último.

## Requisitos
- R1. Transcribir por trozos durante la llamada (hoy solo graba).
- R2. Resumen de los últimos N minutos con Ollama, con citas.
- R3. No frenar el dictado ni la regla 3 de velocidad.
- R4. Test: carga de CPU con reunión + dictado a la vez.

## NO SE NEGOCIA que toca
- Regla 3 y 4: si no cabe, se degrada y lo dice.

## Dónde vivirá
- Por decidir al construir (leer `docs/ARQUITECTURA.md` antes).
