# FR-16 · Compromisos por interlocutor

> Origen: decidido · planificado · Objetivo al que sirve: G4

## Qué hace (lenguaje del usuario)
Pronto sabe cómo se llama la otra persona de la reunión («Ellos»), apunta quién se comprometió a qué y para cuándo, y antes de la siguiente reunión con esa persona me recuerda «la última vez quedasteis en…».

## Requisitos (cada uno comprobable; `/rumbo` da un veredicto por línea)
- R1. En una reunión el usuario puede poner nombre a «Ellos»; ese nombre sustituye a «Ellos» en la transcripción, las notas y la exportación a Markdown de esa reunión.
- R2. Cada compromiso guarda su responsable («Tú» o el nombre del interlocutor) y su fecha: el plazo tal como se dijo y, si se puede, la fecha concreta calculada desde el día de la reunión; si no se dijo, queda vacío y nunca se inventa.
- R3. Las reuniones con una misma persona quedan agrupadas por interlocutor y los compromisos abiertos de esa persona se pueden ver juntos.
- R4. Al empezar una reunión con una persona que tiene compromisos abiertos, Pronto muestra un recordatorio «La última vez quedasteis en…» con esos compromisos y sus fechas.
- R5. Un compromiso marcado como hecho no vuelve a aparecer en los recordatorios.
- R6. Los nombres y los compromisos se guardan solo en el PC y se generan con IA local.
- R7. La persona se reconoce por texto (nombre escrito, lo dicho en la reunión o la lista de participantes), nunca por los rasgos de su voz: reconocer por la voz sería identificación biométrica de alto riesgo (dictamen `specs/cumplimiento-ia/2026-10-22-previo.md`).

## Fuera de alcance
- Identificar por la voz quién es cada interlocutor.
- Enviar recordatorios por correo, calendario o mensajería.
- Reuniones con más de un interlocutor distinguido por separado (hoy solo hay «Tú» y «Ellos»).

## Dónde vive
- (sin construir)
