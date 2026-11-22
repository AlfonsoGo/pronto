# Texto en vivo: lo que falta (BELTR-PRT-10)

> Estado 20-nov-2026. Diseño en `docs/ARQUITECTURA.md`, Contrato v15. Código: `src-tauri/src/vivo.rs`.

## Hecho
- El texto final sale siempre del audio entero, con Parakeet y con Whisper, por el mismo camino que sin
  texto en vivo (`state::transcribe_heard`). Lo en vivo es solo vista previa en la Flow bar. Ya no se cose
  por trozos (`ensamblar`, `coser`, `al_soltar` y `Engine::stitches` fuera; el hallazgo 6 desaparece).
- Banco de 13 WAV de Txonso (`vivo::banco_vivo`, con ticks de 8 s): 13/13 iguales con los dos motores.
- Al soltar, el dictado no espera al tick a medias: con Parakeet van a la vez (ticks de 4 s como mucho),
  con Whisper el tick se corta (`abort_callback`). Tras soltar no empieza ninguna decodificación.
- Hallazgos 1, 2, 3 y 5 arreglados, cada uno con su test (`vivo::tests`, `tests/flowbar-vivo.test.mjs`).

## Falta
- Regla 3 con poco margen: de soltar a texto con un tick de Parakeet a medias, p50 988-1034 ms para 15 s
  (límite 1068), +5-6 % sobre sin texto en vivo (938-978 ms; hoy este PC va ≈ 8 % más lento que la línea
  base de 890 ms). sherpa-onnx no deja cortar una decodificación; lo que queda es no hacer el tick a la
  vez (esperarlo cuesta +300 ms) o un reconocedor aparte (≈ 670 MB más, no fue más rápido). Decidir.
- Repetir el banco de 13 WAV con los ticks de 4 s (lo pasé con 8 s).
- Hallazgo 4: con Whisper en idioma automático, cada trozo de la vista previa decide su idioma. Solo afecta
  a la vista previa.
- Probarlo con el micrófono de verdad (las baterías no usan el micro).
