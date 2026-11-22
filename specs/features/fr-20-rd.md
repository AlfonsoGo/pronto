# FR-20 · No actualizar mientras trabajas

> Origen: Wispr Flow (whats-new 4-sep-2026) · Ticket: PRNT-46 · Investigación: `docs/ideas-glaido-wispr-2026-11-09.md` · REA: observabilidad alta
> Estado: PROPUESTA (9-nov-2026). Nada construido. Lo que diga REA se apunta como visto/inferido en `clon-wispr-flow/`.

## Qué hace (lenguaje del usuario)
Pronto nunca se reinicia para actualizarse mientras dicto o hay una reunión: espera a que deje el PC quieto.

## Requisitos
- R1. «Actualizar ahora» y la actualización automática esperan si hay dictado, reunión grabando o notas procesándose.
- R2. Espera a 2 min sin teclado ni ratón (GetLastInputInfo) antes de instalar sola.
- R3. La barra dice «Actualización lista: se instala cuando acabes».
- R4. Test: con reunión activa, el instalador no se lanza; al terminar y pasar el reposo, sí.

## NO SE NEGOCIA que toca
- Regla 2 (integridad): nunca cortar una reunión ni una migración a medias.

## Dónde vivirá
- Por decidir al construir (leer `docs/ARQUITECTURA.md` antes).
