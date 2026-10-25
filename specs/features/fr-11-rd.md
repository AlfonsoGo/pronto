# FR-11 · Banco de voz propio

> Origen: decidido · planificado · Objetivo al que sirve: G1, G2

## Qué hace (lenguaje del usuario)
Pronto mide por su cuenta cuántas palabras se equivoca cada motor de voz con MI voz y cuánto tarda en pegar, usando mis propios dictados guardados, y lo compara contra un juez de 10 dictados que yo he corregido y que nunca se usan para ajustar nada.

## Requisitos (cada uno comprobable; `/rumbo` da un veredicto por línea)
- R1. Existe una pantalla (o comando) que mide el WER de cada motor instalado (Parakeet y cada modelo de Whisper presente) sobre los dictados guardados del usuario que conservan su audio, y muestra el resultado por motor con el nº de dictados y de palabras de referencia usados.
- R2. El juez son 10 dictados (23-oct-2026; antes 50) cuya transcripción corrigió el propio usuario a mano; quedan marcados como «juez» y ninguna función de ajuste (diccionario automático, umbrales, hotwords, ajuste nocturno de FR-15) los lee ni los usa.
- R3. Mientras el usuario no haya corregido 10 dictados, el banco no da cifra de WER y dice cuántos faltan («Faltan N dictados para el juez»).
- R4. Cada dictado guarda su latencia soltar atajo → texto pegado en ms, y el banco muestra la p50 y la p95 por motor.
- R5. El objetivo es WER < 2 % y latencia < 0,25 s en un dictado de 15 s; el banco marca cada motor como «cumple» o «no cumple» frente a esos dos números.
- R6. Cada medida se guarda con fecha, motor, modelo y versión de Pronto, para ver cómo evoluciona.
- R7. El banco solo mide: nunca cambia el motor, el diccionario ni ningún ajuste por sí solo (eso lo decide FR-15).
- R8. Los audios, las correcciones y los resultados no salen del PC.

## Fuera de alcance
- Elegir o cambiar de motor automáticamente (FR-15).
- Aprender de las correcciones tras pegar (FR-12).
- Medir la calidad de las notas de reunión.

## Dónde vive
- (sin construir)
