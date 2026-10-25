# FR-15 · Ajuste nocturno y motor recomendado para tu PC

> Origen: decidido · planificado · Objetivo al que sirve: G1, G2

## Qué hace (lenguaje del usuario)
De noche Pronto reprocesa los dictados del día para ver qué motor, umbral y palabras guía me van mejor y se ajusta solo, siempre midiendo contra el banco de FR-11. Además me dice qué motor es el mejor para mi PC según mi tarjeta gráfica y mi memoria, y no me ofrece lo que no cabe.

## Requisitos (cada uno comprobable; `/rumbo` da un veredicto por línea)
- R1. Cada noche, con el PC encendido y sin dictado ni reunión en curso, Pronto reprocesa los dictados del día que conservan su audio con cada motor candidato instalado.
- R2. Elige motor, umbral y hotwords por la medida del banco (FR-11) y nunca usa los 50 dictados del juez para ajustar.
- R3. Solo aplica un cambio si baja el WER medido en el banco y la latencia sigue dentro del objetivo de FR-11 (< 0,25 s en 15 s de dictado).
- R4. Cada cambio automático queda anotado con fecha, valor anterior, valor nuevo y la medida que lo justificó, y el usuario puede deshacerlo con un botón.
- R5. Configuración → Motor marca el motor recomendado para este PC según GPU, VRAM y RAM, con tarjetas tipo podio (medalla) que muestran porcentaje de velocidad, tamaño y aciertos, respetando el diseño del hub.
- R6. No se ofrece lo que no cabe: los modelos de Whisper según la VRAM y los modelos de IA local (Ollama, gemma) según la memoria disponible salen como «No cabe en este PC» con el motivo, o no se ofrecen.
- R7. En un PC nuevo Pronto detecta RAM, VRAM y GPU por sí solo, recomienda y adapta lo que ofrece a ese equipo sin intervención del usuario.
- R8. El reprocesado y las medidas se hacen en el PC; ningún audio ni texto sale de él.

## Fuera de alcance
- Construir el banco de voz y el juez (FR-11).
- La versión para macOS.
- Descargar un modelo sin que el usuario lo pida.

## Dónde vive
- (sin construir)
