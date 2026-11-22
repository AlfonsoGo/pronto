# FR-31 · Exportar subtítulos SRT

> Origen: Spokenly · Ticket: PRNT-49 · Investigación: `docs/ideas-glaido-wispr-2026-11-09.md` · REA: observabilidad no aplica
> Estado: PROPUESTA (9-nov-2026). Nada construido. Lo que diga REA se apunta como visto/inferido en `clon-wispr-flow/`.

## Qué hace (lenguaje del usuario)
Del texto de un archivo o una reunión saco un .srt con tiempos.

## Requisitos
- R1. Tiempos desde las marcas por palabra del motor.
- R2. Líneas de 42 caracteres máximo, 2 líneas por bloque.
- R3. Guardar con el diálogo de Windows (aviso si la carpeta sube a la nube).
- R4. Test: el SRT abre en un reproductor y los tiempos crecen siempre.

## NO SE NEGOCIA que toca
- Regla 1.

## Dónde vivirá
- Por decidir al construir (leer `docs/ARQUITECTURA.md` antes).
