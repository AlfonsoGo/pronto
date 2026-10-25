# FR-03 · Diccionario por voz y huella de voz

> Origen: observado-en-código · Objetivo al que sirve: G1 (también G3 y G5)

## Qué hace (lenguaje del usuario)
Añado una palabra a mi diccionario, la digo en voz alta y Pronto aprende cómo la oye y cómo suena mi voz al decirla; la próxima vez que la dicte la escribe bien aunque el motor la oiga distinta. Las grabaciones se quedan en mi PC.

## Requisitos (cada uno comprobable; `/rumbo` da un veredicto por línea)
- R1. «Grabar pronunciación» graba del micrófono elegido y para sola tras 900 ms de silencio después de ≥ 200 ms de voz seguida, o a los 5 s; el umbral de voz es el ruido de los primeros 300 ms + 0,15, con mínimo 0,22.
- R2. Antes de transcribir la grabación se sube de volumen hasta pico 0,5 (×20 como mucho); con pico < 0,0005 el error es «No llega sonido del micrófono. ¿Lo tienes silenciado?».
- R3. Solo se graba una pronunciación a la vez y se rechaza con motor no listo, con un dictado en curso («Hay un dictado en curso. Termínalo antes de grabar la pronunciación») o con una reunión en curso; si empieza un dictado, la grabación se descarta.
- R4. El resultado dice si lo oído ya está cubierto: `ok` (ya la escribe bien), `covered` (ya la corrige un reemplazo o el ajuste de mayúsculas/tildes) o `new` (se guarda como variante); si no se entiende nada el error es «No se ha entendido nada. Prueba otra vez, más cerca del micrófono».
- R5. Cada palabra del diccionario guarda variantes (lo que oyó el motor, normalizadas, sin repetir) y muestras de voz (recortadas a la voz, 0,2-2,5 s, 16 kHz mono); la interfaz recomienda 3 grabaciones por palabra.
- R6. Una grabación sin guardar con la palabra se borra a las 24 h; al borrar una palabra se borran en la misma operación sus variantes, sus muestras y sus ficheros de audio.
- R7. Al dictar, un tramo de 1 a 3 palabras se reescribe con la palabra del diccionario solo si suena como sus muestras (coste DTW < 6,3 con 1 muestra, < 6,4 con 2 o más), cubre ≥ 50 % de cada palabra cambiada, no son todas palabras corrientes ni otra entrada del diccionario y ninguna otra entrada casa a menos de 0,35 de margen.
- R8. La huella prefiere no corregir a corregir mal: con 50 palabras × 3 muestras sobre 13 s de audio tarda ≈ 35 ms y no produjo falsos positivos en las baterías medidas (44 frases sintéticas y 174 dictados reales).
- R9. Las muestras de voz y las variantes se guardan solo en el PC (`%APPDATA%\Pronto2\voz` y la base de datos); el nombre de un fichero de muestra solo admite letras, cifras, `-` y `_` más `.wav`, nunca una ruta.
- R10. Con muestras sueltas dichas por el usuario la huella debe reconocer la mayoría de las veces que dice la palabra dentro de una frase (fallo conocido 18-oct: con muestras recortadas de frases reales el recall medido fue ≈ 10 %, con voz sintética suelta ≈ 85 %; falta medirlo con 3 grabaciones sueltas reales y, si es bajo, recalibrar `MAX_COST`).

## Fuera de alcance
- Hotwords o sesgo del vocabulario dentro del motor (Parakeet greedy no los admite; ver FR-04).
- Aprender solo de las correcciones del usuario tras pegar (FR-12) y el informe de palabras falladas (FR-14).
- Reemplazos de texto del diccionario y su orden en la tubería (FR-02).

## Dónde vive
- `src-tauri/src/pronounce.rs:23-33` (tiempos y umbrales), `:73-109` (parada automática), `:113-190` (grabar y transcribir), `:194-222` (muestra pendiente, 24 h), `:226-233` (subir volumen), `:236-251` (veredicto)
- `src-tauri/src/text.rs:498-516` (`ok` / `covered` / `new` / `empty`), `:486-493` (normalizar lo oído)
- `src-tauri/src/voiceprint.rs:41-55` (límites y costes), `:558-601` (decidir qué se cambia), `:629-661` (aplicar), `:688-724` (banco de palabras con muestras)
- `src-tauri/src/state.rs:532-541` (huella antes de la tubería)
- `src-tauri/src/db.rs:56-72` (carpeta y nombre válido de muestra), `:191-221` (tablas `word_variants`, `word_samples`), `:343-389` (variantes y muestras pendientes), `:698-719` (listar), `:830-844` (borrar palabra y todo lo suyo)
- `ui/hub/pages/dictionary.js:14` (3 grabaciones recomendadas)
