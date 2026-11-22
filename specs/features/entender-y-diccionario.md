# Entender mientras hablas y respetar el diccionario (BELTR-PRT-11)

> 20-nov-2026. Pedido: que Pronto 2 entienda lo que se dice a medida que se habla, sin inventar, y que
> escriba bien las palabras guardadas del diccionario, igual o mejor que Wispr Flow, SuperWhisper y el
> micrófono de Claude.

## Contrato de «hecho»

Está hecho cuando se cumple todo esto, comprobado por un revisor que no vio el trabajo:

1. **Línea base medida y guardada** (antes de tocar nada) con el motor por defecto (Parakeet) sobre
   33 grabaciones: las 13 reales de Txonso y 20 frases sintéticas (voz de Windows) con palabras del
   diccionario. Cuatro cifras: WER (% de palabras mal), % de palabras del diccionario bien escritas,
   nº de palabras inventadas y ms de soltar a texto.
2. **El banco es una prueba** que pasa el vigilante (`npm test`): falla si el WER sube, si baja el % de
   palabras del diccionario, si aparece una palabra inventada más o si de soltar a texto pasa de 1068 ms
   (regla 3). Las 20 sintéticas van siempre; las 13 reales, solo en este PC (no entran en el repo).
3. **Mejora medible**: después del cambio, el % de palabras del diccionario bien escritas sube, el WER no
   empeora, las palabras inventadas no suben y la velocidad sigue en ≤ 1068 ms (p50, 15 s).
4. **Las 7 reglas NO SE NEGOCIA siguen en verde**: todo local, el pulido no añade contenido, velocidad,
   etc. (`tests/no-se-negocia/`).
5. **Texto de referencia de las 13 grabaciones** escrito y marcado como provisional hasta que Txonso lo
   confirme.
6. **Comparativa** de Wispr Flow, SuperWhisper, micrófono de Claude y Pronto 2 en una página en castellano
   llano, con fuentes públicas citadas; las cifras de Pronto, medidas aquí.
7. **Revisión independiente**: un subagente nuevo corre las pruebas y dice APTO.
8. Commit solo de lo de este ticket. Sin push ni publicar.

## Antes y después (20-nov-2026, Parakeet, `cargo test --release`)

| Cifra | Antes | Después |
|---|---|---|
| Palabras del diccionario bien escritas (20 frases TTS) | 12/25 (48 %) | 21/25 (84 %) |
| Lo mismo en la vista previa en vivo | 6/25 | 20/25 |
| WER frases TTS | 12,2 % | 5,6 % |
| Palabras inventadas frases TTS | 8 | 4 |
| WER voz real de Txonso (13 grabaciones, 134 palabras) | 10,4 % | 10,4 % |
| Inventadas voz real | 0 | 0 |
| Soltar → texto, p50 frases de 4 s | 286 ms | 292 ms (ruido) |
| Texto corriente con trampas (62 frases) cambiado por el diccionario | — | 0 |

Referencia: Whisper large-v3-turbo antes daba 17/25. Ficheros: `tests/banco-entender/linea-base-2026-11-20.txt` y
`despues-2026-11-20.txt`.

## Qué se hizo
- Pasada aproximada del diccionario (palabras partidas en 2-3 o casi iguales), con las guardas que propuso Fable
  como asesor (ver `docs/ARQUITECTURA.md`, Contrato v16).
- La vista previa en vivo ya pasa por el diccionario.
- El pulido de IA no puede deshacer una palabra del diccionario.
- Banco en `npm test` y prueba en contra con texto corriente.

## Pendiente
- **Txonso confirma las referencias** de sus 13 grabaciones (`C:\Users\Txonso\dev\pronto-pruebas\datos\referencias\`,
  marcadas «PROVISIONAL», con las dudas apuntadas en cada una).
- Grabar 15-20 frases reales de Txonso con palabras del diccionario: hoy el diccionario solo se mide con voz sintética.
- Quedan sin arreglar las palabras que Parakeet parte en palabras que existen («Pero aquí te» por «Parakeet») o
  acorta mucho («Cleta» por «Claqueta»). Siguiente paso posible: palabras con prioridad en el motor, solo con
  diccionario no vacío y audios cortos, detrás de un interruptor, si el banco lo justifica.
- En grabaciones de 15-16 s con mucho silencio, soltar → texto llega a 1,1 s en una pasada suelta (ya pasaba antes:
  1087 ms en la línea base). La regla 3 se mide con su propio audio (p50).

## Whisper por modelo (BELTR-PRT-12, 20-nov-2026)

Pedido de Txonso: dejar solo Parakeet y Whisper medium. Banco con los tres Whisper (`PRONTO_BANCO_WHISPER=medium,large-v3-turbo,large-v3`):

| Motor | Voz real: % mal | Voz real: inventadas | Frases TTS: % mal | Diccionario TTS | Soltar → texto (p50, real) |
|---|---|---|---|---|---|
| Parakeet | **10,4 %** | 0 | 5,6 % | 21/25 | 517 ms (CPU) |
| Whisper medium | 11,2 % | 0 | 7,1 % | 19/25 | 282 ms (GPU) |
| Whisper large-v3-turbo | 15,7 % | 1 | 4,6 % | 22/25 | 173 ms (GPU) |
| Whisper large-v3 | 14,9 % | 0 | 3,6 % | 21/25 | 324 ms (GPU) |

Con la voz de Txonso, medium es el Whisper que menos falla (turbo y large-v3 se comen palabras: 9 y 11 perdidas
frente a 3). Con voz sintética gana large-v3. Solo 134 palabras y referencias provisionales (sacadas de cruzar
Parakeet y turbo, así que no favorecen a medium): hay que confirmarlas. Datos en
`tests/banco-entender/whisper-modelos-2026-11-20.txt`.
Decisión de Txonso (20-nov): se quitan large-v3-turbo y large-v3; Whisper de fábrica pasa a medium (Contrato v16.1).
