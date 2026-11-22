# FR-27 · Tapar datos personales al exportar

> Origen: Gladia (PII redaction, API) · Ticket: PRNT-48 · Investigación: `docs/ideas-glaido-wispr-2026-11-09.md` · REA: observabilidad no aplica (API de nube: sin REA)
> Estado: PROPUESTA (9-nov-2026). Nada construido. Lo que diga REA se apunta como visto/inferido en `clon-wispr-flow/`.

## Qué hace (lenguaje del usuario)
Al exportar o copiar unas notas puedo marcar «tapar datos» y salen sin correos, teléfonos ni cuentas.

## Requisitos
- R1. Detección local por patrones: correo, teléfono, IBAN, DNI/NIE; nombres propios con lista del diccionario.
- R2. Se sustituye por [correo], [teléfono]…; el original no se toca.
- R3. Lista blanca de lo que se tapa; se prueba el negativo (que no quede nada).
- R4. Test: notas con cada tipo y con formatos raros.

## NO SE NEGOCIA que toca
- Regla 1: todo local; Gladia es solo la idea.

## Dónde vivirá
- Por decidir al construir (leer `docs/ARQUITECTURA.md` antes).
