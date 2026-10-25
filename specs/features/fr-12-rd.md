# FR-12 · Aprender de las correcciones

> Origen: decidido · planificado · Objetivo al que sirve: G1

## Qué hace (lenguaje del usuario)
Si después de pegar corrijo una palabra a mano, Pronto se da cuenta y me propone enseñársela (diccionario y huella de voz) para no fallar la próxima vez, sin guardar nada más de lo que escribo.

## Requisitos (cada uno comprobable; `/rumbo` da un veredicto por línea)
- R1. Unos segundos después de pegar, Pronto relee el campo de texto de la app donde pegó mediante UI Automation y compara lo leído con el texto que pegó.
- R2. Si el usuario cambió una palabra por otra, Pronto propone «Añadir «X» al diccionario» (y a la huella de voz si el dictado conserva su audio) y no guarda nada hasta que el usuario acepta.
- R3. Solo se guarda la palabra corregida y la que Pronto había escrito; el resto del texto del campo y cualquier texto de otras apps se descarta tras comparar y nunca se guarda ni se registra.
- R4. Si el campo no se puede leer (app sin UI Automation, campo de contraseña) no se propone nada y no sale ningún error.
- R5. Tras aceptar la propuesta, la palabra se escribe bien en el siguiente dictado que la contenga.
- R6. El usuario puede apagar esta función y, apagada, no lee ningún campo.
- R7. Nada de lo leído sale del PC.

## Fuera de alcance
- Informe de palabras más falladas (FR-14).
- Cambios de frases enteras o de estilo: solo cambios de palabra.
- Grabar la pronunciación en el momento (FR-03; aquí solo se propone).

## Dónde vive
- (sin construir)
