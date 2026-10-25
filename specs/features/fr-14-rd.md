# FR-14 · Informe de palabras más falladas

> Origen: decidido · planificado · Objetivo al que sirve: G1

## Qué hace (lenguaje del usuario)
En Estadísticas veo las 10 palabras que más he tenido que corregir esta semana, y con un botón «Enseñar» le digo a Pronto cómo se dicen para que no vuelva a fallarlas.

## Requisitos (cada uno comprobable; `/rumbo` da un veredicto por línea)
- R1. Estadísticas tiene una sección «Palabras más falladas» con como máximo 10 palabras, ordenadas de más a menos corregidas en los últimos 7 días móviles.
- R2. Una palabra cuenta como corregida cuando el usuario la cambió a mano tras pegar y Pronto lo detectó (FR-12); sin FR-12 la sección no tiene datos.
- R3. Cada fila muestra la palabra correcta, cuántas veces se corrigió y qué había escrito Pronto.
- R4. El botón «Enseñar» de cada fila abre «Grabar pronunciación» (FR-03) con esa palabra ya puesta; tras guardarla, la fila sale de la lista cuando ya no se corrige.
- R5. Sin correcciones en la semana sale un estado vacío explicativo y con menos de 10 palabras se muestran solo las que haya.
- R6. Solo se guardan las palabras corregidas y su recuento, nunca el texto completo de los dictados ni de otras apps.
- R7. El informe se calcula en el PC y no sale de él.

## Fuera de alcance
- Detectar las correcciones (FR-12) y grabar la pronunciación (FR-03).
- Estadísticas de reuniones.
- Informes de otros periodos distintos de la semana.

## Dónde vive
- (sin construir)
