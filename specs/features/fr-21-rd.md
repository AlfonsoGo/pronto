# FR-21 · Etiquetas en fragmentos

> Origen: Glaido (docs, revisión Spokenly 26-sep-2026) · Ticket: PRNT-46 · Investigación: `docs/ideas-glaido-wispr-2026-11-09.md` · REA: observabilidad media (Glaido no instalado)
> Estado: PROPUESTA (9-nov-2026). Nada construido. Lo que diga REA se apunta como visto/inferido en `clon-wispr-flow/`.

## Qué hace (lenguaje del usuario)
En un fragmento puedo poner {fecha} o {portapapeles} y Pronto los cambia por la fecha de hoy o lo que tengo copiado al pegarlo.

## Requisitos
- R1. Etiquetas cerradas: {fecha}, {hora}, {portapapeles}. Una etiqueta desconocida se pega tal cual.
- R2. {portapapeles} usa el contenido de ANTES del dictado (Pronto usa el portapapeles para pegar).
- R3. Fecha en el formato del idioma de Windows.
- R4. Test: fragmento con las tres etiquetas y portapapeles vacío o con imagen.

## NO SE NEGOCIA que toca
- Regla 5: el portapapeles del usuario se restaura igual que hoy.

## Dónde vivirá
- Por decidir al construir (leer `docs/ARQUITECTURA.md` antes).
