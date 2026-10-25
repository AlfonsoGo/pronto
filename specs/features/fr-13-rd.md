# FR-13 · Contexto del cursor antes de pulir

> Origen: decidido · planificado · Objetivo al que sirve: G1

## Qué hace (lenguaje del usuario)
Antes de pulir un dictado con IA, Pronto puede mirar el texto que rodea al cursor para entender de qué hablo, y con otro interruptor aparte puede «entender lo que quise decir»: arreglar frases mal construidas y palabras que el motor oyó mal. Ambos van apagados de fábrica y nada sale de mi PC.

## Requisitos (cada uno comprobable; `/rumbo` da un veredicto por línea)
- R1. Hay un interruptor «Usar el texto alrededor del cursor» apagado de fábrica; apagado, Pronto no lee ningún campo de ninguna app.
- R2. Con el interruptor activo, justo antes de pulir un dictado Pronto lee con UI Automation el texto alrededor del cursor de la app donde se va a pegar y lo pasa al pulido como contexto: no se corrige y no aparece en el texto pegado.
- R3. El contexto leído solo viaja a Ollama en `ollamaUrl` local; no se guarda en el historial ni en registros y nunca sale del PC.
- R4. Si no se puede leer el campo (app sin UI Automation, campo de contraseña), el pulido sigue sin contexto y el dictado se pega igual.
- R5. «Entender lo que quise decir» es un interruptor aparte (`ai_polish_intent` en `models.rs`), apagado de fábrica, con su fila en Estilo bajo «Pulir cada dictado con IA local»; apagado, el pulido actual funciona exactamente igual.
- R6. Con «Entender lo que quise decir» activo, el pulido recibe el diccionario, arregla frases mal construidas y palabras mal oídas deduciendo la intención por el contexto («cuenta» pasa a «cuento»), puede cambiar palabras que suenan parecido y reordenar, y tiene prohibido añadir ideas nuevas.
- R7. «Entender lo que quise decir» tiene un plazo propio mayor que los 6 s del pulido normal; si vence, se pega el texto del pulido normal o sin pulir, con el aviso habitual.
- R8. Antes de darlo por bueno se mide con dictados reales del usuario que no se pasa de la raya (no inventa contenido ni cambia el significado).

## Fuera de alcance
- Aprender de las correcciones (FR-12).
- Leer el contexto en reuniones o en los Transforms.
- Elegir el modelo de IA (FR-05 y FR-15).

## Dónde vive
- (sin construir)
