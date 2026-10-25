# FR-02 · Tubería de texto

> Origen: observado-en-código · Objetivo al que sirve: G1

## Qué hace (lenguaje del usuario)
Lo que dicto sale limpio y escrito a mi manera: sin muletillas, con mis palabras bien puestas, mis atajos de texto expandidos y el tono que toca según la app donde pego (formal en el correo, casual en el chat). Todo se hace en mi PC, sin IA.

## Requisitos (cada uno comprobable; `/rumbo` da un veredicto por línea)
- R1. Los pasos se aplican siempre en este orden: limpieza, retroceso, «pulsa enter», diccionario, snippets, estilo; el texto final nunca contiene los marcadores internos de protección.
- R2. La limpieza tiene cuatro niveles (`none`, `light`, `medium` por defecto, `high`): `none` deja el texto tal cual; `light` quita muletillas en español e inglés («eh», «em», «mmm», «um», «uh», «hmm», «erm») y «o sea» solo al inicio de la frase; `medium` añade palabras repetidas seguidas («que que» pasa a «que») sin tocar «muy muy», «no no», «sí sí»; `high` añade arranques en falso al inicio («bueno, pues», «a ver,»).
- R3. Una muletilla entre signos («¿eh?») no deja signos colgando: «Hola Marta, ¿eh? Te confirmo…» pasa a «Hola Marta. Te confirmo…».
- R4. ~~Quitado por Txonso el 23-oct-2026: el retroceso podía confundir; lo que se dicta se pega tal cual.~~ Antes: con nivel `medium` o `high`, «no, mejor…», «mejor dicho», «quiero decir», «no, quiero decir», «scratch that», «borra eso» o «no, wait» borran la frase anterior (desde el último fin de frase); «No, mejor no» al principio del dictado se deja intacto como respuesta normal. Las correcciones («quiero decir», «mejor dicho», «no, mejor…», «no, wait») solo actúan si les sigue la corrección y no van pegadas a una palabra que las haga parte de la frase («lo que quiero decir», «te quiero decir», «está mejor dicho»), así que nunca dejan vacío un dictado; solo «borra eso»/«scratch that» pueden vaciarlo, y entonces el dictado queda en el historial con lo oído y su audio (regla 5).
- R5. Las entradas del diccionario con reemplazo se sustituyen por palabra completa y sin distinguir mayúsculas; las frases más largas ganan a las cortas («san francisco» antes que «san»); una palabra sin reemplazo solo ajusta mayúsculas y tildes (pone tildes, nunca las quita: «verá» no pasa a «Vera»).
- R6. Una palabra que suena igual que una del diccionario (clave fonética española, claves de 4 o más letras, nunca palabras corrientes ni claves que suenen a dos entradas distintas) se escribe como la del diccionario («Chonzo» pasa a «Txonso»); una palabra que existe en español (léxico embebido y formas regulares de sus verbos) o un nombre de pila distinto no se cambia nunca («llama» no pasa a «Ollama», «Sara» no pasa a «Zara»).
- R7. Si el texto dictado entero (minúsculas, sin puntuación de los extremos) coincide con el disparador de un snippet, sale exactamente su expansión, sin punto añadido; también se expande dentro de una frase, por palabra completa y con el disparador más largo primero; el disparador se compara con el texto ya corregido por el diccionario («firma txonso» se expande aunque «Txonso» esté en el diccionario).
- R8. El estilo se elige por la app con foco: `whatsapp`, `telegram`, `discord`, `signal`, `messenger` = Mensajes personales; `slack`, `teams`, `ms-teams`, `zoom` = trabajo; `outlook`, `thunderbird` o título con «Gmail»/«Outlook» = correo; el resto = otros. De fábrica: personales Casual, trabajo Casual, correo Formal, otros Casual.
- R9. Formal pone mayúscula inicial y punto final; Casual, mayúscula inicial y sin punto final en frases sueltas (conserva «?» y «!»); Muy casual, todo en minúscula y sin punto final; Entusiasta cambia el punto final por «!» en frases de hasta 10 palabras; el estilo nunca toca lo que el usuario escribió (reemplazos del diccionario y expansiones de snippets): «Muy casual» no lo pasa a minúsculas.
- R10. Con «pulsa enter» activado, decir «pulsa/presiona enter» al final del dictado quita la orden del texto y pulsa Enter tras pegar (150 ms después); una orden negativa («no pulse enter», «don't press enter») no pulsa.

## Fuera de alcance
- Corrección con IA (FR-05).
- Dar de alta palabras y su voz (FR-03).
- Hotwords del diccionario para Parakeet: no existen; en Whisper el diccionario va como texto previo (FR-04).

## Dónde vive
- `src-tauri/src/text.rs:43-61` (orden de la tubería), `:73-93` (niveles de limpieza), `:95-159` (muletillas), `:162-191` (repeticiones), `:193-218` (arranques en falso), `:220-235` («pulsa enter»), `:237-248` (espacios), `:252-305` (retroceso)
- `src-tauri/src/text.rs:540-609` (diccionario), `:614-723` (clave fonética y pasada «suena igual»), `:734-781` (snippets), `:898-928` (estilo), `:933-944` (categoría de app)
- `src-tauri/src/state.rs:544-558` (opciones de la tubería), `:488-495` (tono por categoría), `:608-612` (Enter tras pegar)
- `src-tauri/src/models.rs:24-50` (niveles y tonos de fábrica)
