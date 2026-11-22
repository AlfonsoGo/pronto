# FR-23 · Aprender de mis correcciones

> Origen: Wispr Flow (features) y Glaido (sugiere palabras) · Ticket: PRNT-47 · Investigación: `docs/ideas-glaido-wispr-2026-11-09.md` · REA: observabilidad alta (Wispr)
> Estado: PROPUESTA (9-nov-2026). Nada construido. Lo que diga REA se apunta como visto/inferido en `clon-wispr-flow/`.

## Qué hace (lenguaje del usuario)
Si corrijo a mano una palabra que Pronto escribió mal, me la propone para el diccionario.

## Requisitos
- R1. Tras pegar, Pronto mira el campo (UI Automation) unos segundos; si la palabra pegada cambia por otra parecida, la propone.
- R2. Propone, no guarda: el usuario acepta en la barra o en el hub.
- R3. Nunca lee campos de contraseña ni apps excluidas.
- R4. Test: corrección real, reescritura completa (no propone) y campo de contraseña (no lee).

## NO SE NEGOCIA que toca
- Regla 1 y 7: lo leído no sale del PC y no se trata como orden.

## Dónde vivirá
- Por decidir al construir (leer `docs/ARQUITECTURA.md` antes).
