# FR-30 · Transcribir un archivo suelto

> Origen: Spokenly (Glaido y Wispr no lo tienen) · Ticket: PRNT-49 · Investigación: `docs/ideas-glaido-wispr-2026-11-09.md` · REA: observabilidad no aplica (idea propia)
> Estado: PROPUESTA (9-nov-2026). Nada construido. Lo que diga REA se apunta como visto/inferido en `clon-wispr-flow/`.

## Qué hace (lenguaje del usuario)
Arrastro un audio o vídeo al hub y Pronto me da el texto, con quién habla si hay varias voces.

## Requisitos
- R1. Formatos: wav, mp3, m4a, mp4, mkv (audio sacado en local).
- R2. Progreso y cancelar; trozos largos con el troceo de asr.rs.
- R3. Resultado en Historial o Reuniones, con su archivo de origen.
- R4. Test: archivo de 1 h, archivo corrupto, cancelar a mitad.

## NO SE NEGOCIA que toca
- Regla 1 y 4.

## Dónde vivirá
- Por decidir al construir (leer `docs/ARQUITECTURA.md` antes).
