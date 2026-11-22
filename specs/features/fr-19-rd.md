# FR-19 · Aviso de micro mudo

> Origen: Wispr Flow (whats-new 9-oct-2026) · Ticket: PRNT-46 · Investigación: `docs/ideas-glaido-wispr-2026-11-09.md` · REA: observabilidad alta
> Estado: PROPUESTA (9-nov-2026). Nada construido. Lo que diga REA se apunta como visto/inferido en `clon-wispr-flow/`.

## Qué hace (lenguaje del usuario)
Si empiezo a dictar y el micro no recoge sonido, la barra me avisa al momento, no después de soltar.

## Requisitos
- R1. En los primeros 600 ms de un dictado, si el nivel no pasa del umbral de ruido, la barra dice «No te oigo: revisa el micro» y nombra el micro en uso.
- R2. El aviso no corta el dictado: si luego llega voz, el aviso se quita solo.
- R3. Un micro silenciado por Windows o desconectado se distingue de «hablas bajo» cuando el sistema lo dice.
- R4. Test: audio de silencio → aviso; audio con voz a los 300 ms → sin aviso.

## NO SE NEGOCIA que toca
- Regla 5 (un dictado nunca se pierde): el aviso no descarta audio.

## Dónde vivirá
- Por decidir al construir (leer `docs/ARQUITECTURA.md` antes).
