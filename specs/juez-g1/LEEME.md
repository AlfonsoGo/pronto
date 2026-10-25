# Juez de G1 · 10 dictados (preparado el 23-oct-2026, sin grabar)

Para medir cuántas palabras falla Pronto con tu voz (objetivo G1 del PRD: menos del 2 %) hace falta un «juez»: dictados
tuyos con su texto correcto, que nunca se usan para ajustar nada. Decidiste 10 en vez de 50.

Como el texto de cada frase ya está escrito abajo, **no tienes que corregir nada a mano**: solo leerlas en voz alta
con Pronto. El texto de referencia es el de esta página. Unos 5 minutos.

## Antes de empezar (1 minuto)
1. Cierra Wispr Flow (usa el mismo atajo).
2. En el hub de Pronto: **Configuración → Audio guardado**, deja al menos 7 días (si está en «No guardar», el juez no
   tiene audio y no sirve).
3. Abre un Bloc de notas vacío: ahí van a caer los dictados.

## Graba las 10 (unos 4 minutos)
Mantén **Win + Alt**, lee la frase con tu voz y ritmo normales (sin vocalizar de más), suelta. Una frase por dictado,
en este orden. Si te trabas o dices otra palabra, no repitas: apúntalo al lado del número en «Lo que dije de otra forma».

| # | Frase |
|---|---|
| 1 | Mañana a las diez tengo una reunión con el equipo de BeltranLabs para revisar el presupuesto del trimestre. |
| 2 | Pásame el enlace del repositorio de Pronto en GitHub y lo reviso esta tarde con calma. |
| 3 | El modelo de Whisper large tarda unos dos segundos en cargar, pero Parakeet es bastante más rápido. |
| 4 | Recuérdame llamar a Alfonso el jueves veintisiete antes de las cinco y media. |
| 5 | He subido la nueva versión del instalador y el actualizador ya la detecta sin problemas. |
| 6 | Necesito que el cliente confirme el pedido de trescientas cuarenta unidades antes del viernes. |
| 7 | Vamos a probar Claude con el prompt nuevo y comparamos el resultado con el de Ollama en local. |
| 8 | La factura de octubre son mil doscientos cincuenta euros más IVA, pagaderos a treinta días. |
| 9 | Oye, al final no hace falta mover la demo: la dejamos para el lunes por la mañana. |
| 10 | Abre el hub, entra en el diccionario y añade Txonso, Prontov2 y Wispr Flow como palabras nuevas. |

**Lo que dije de otra forma** (déjalo vacío si leíste bien):
- …

## Cuando acabes
Dile a Claude «juez grabado». Lo que hace él, sin tocar tu base: copia `pronto2.db` y su `-wal` a una carpeta de
trabajo, saca los 10 últimos dictados con su audio, compara con la tabla de arriba y te da el % de palabras falladas
por motor (Parakeet y los Whisper que tengas). Esos 10 quedan marcados como juez y no se usan para ajustar nada
(FR-11 R2).

---

## Aparte: huella de voz de una palabra (unos 2 minutos)
Mide si Pronto reconoce una palabra del diccionario por tu forma de decirla (FR-03 R10).
1. Hub → **Diccionario** → elige una palabra que Pronto suela escribir mal (por ejemplo, «Txonso» o «Wispr»).
2. Pulsa el micrófono de su fila (**Grabar pronunciación**) y dila **3 veces**, con unos segundos entre cada una.
3. Dile a Claude «huella grabada» y qué palabra era.
