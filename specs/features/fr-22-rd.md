# FR-22 · Importar el diccionario de Wispr Flow

> Origen: Glaido (lo ofrece al instalar) · Ticket: PRNT-46 · Investigación: `docs/ideas-glaido-wispr-2026-11-09.md` · REA: observabilidad alta (Wispr instalado: app.asar)
> Estado: PROPUESTA (9-nov-2026). Nada construido. Lo que diga REA se apunta como visto/inferido en `clon-wispr-flow/`.

## Qué hace (lenguaje del usuario)
Si vengo de Wispr Flow, Pronto me ofrece traer mis palabras del diccionario sin teclearlas otra vez.

## Requisitos
- R1. Detecta Wispr Flow instalado en este PC y lee su diccionario LOCAL (formato a sacar con REA: dónde y cómo lo guarda).
- R2. Previsualizar y confirmar antes de escribir nada; todo en una transacción.
- R3. Si el diccionario de Wispr solo vive en su nube, la función se queda en importar un fichero exportado, sin red.
- R4. Test: fichero con duplicados, vacío y con caracteres raros (U+2028, RTL).

## NO SE NEGOCIA que toca
- Regla 1: nada de iniciar sesión en Wispr ni llamar a su servidor.

## Dónde vivirá
- Por decidir al construir (leer `docs/ARQUITECTURA.md` antes).
