# Línea base (antes de tocar nada) · 20-nov-2026 · BELTR-PRT-13

PC: Ryzen 7 5800X3D + RTX 4070 Ti. Motor de fábrica Parakeet (int8, CPU). Pulido: Ollama `qwen3:1.7b`
(el de fábrica), plazo 6 s, modelo cargado antes de medir (como hace la app al arrancar y al empezar a
dictar). Código: commit `09365fd` + cambios sin commit de otras ventanas (no tocan el dictado).

## 1. De soltar el atajo a texto listo para pegar, con el pulido encendido

Camino de `state::transcribe_and_paste`: audio entero → `transcribe_heard` → `text::process` → pulido →
diccionario (`banco_pulido::soltar_sin_vivo`). Sin el pegado en sí (unos ms).

| Conjunto | Dictados | p50 | p90 |
|---|---|---|---|
| ~15 s (12,8-17 s; FLEURS juntado, `internet\quince`) | 10 | **1178 ms** | **1444 ms** |
| ~2 min (110-127 s; MLS, `internet\largos`) | 10 | **12 992 ms** | **14 092 ms** |

Sin pulido, los mismos 15 s salen en ~900 ms: el pulido suma ~250 ms en 15 s y ~5 s en 2 min (el
modelo reescribe todo el texto al soltar). De los 10 de 15 s, el modelo no cambió ninguno (Parakeet ya
puntúa bien); de los 10 largos, 3.

## 2. % de palabras mal (WER), Parakeet, texto final

Banco de `banco_entender` (minúsculas, sin signos; las cifras no se pasan a palabras).

| Conjunto | Muestras | Palabras | WER |
|---|---|---|---|
| Internet cortos: FLEURS es_419 test (CC-BY 4.0) | 30 | 616 | **6,2 %** |
| Internet largos: MLS español test, 7-8 frases seguidas del mismo hablante (CC-BY 4.0) | 10 | 2842 | **5,4 %** |
| Las 13 grabaciones de Txonso (referencias provisionales) | 13 | 134 | **10,4 %** (umbral 10,45) |

Fuente, licencia e id de cada audio: `C:\Users\Txonso\dev\pronto-pruebas\datos\internet\MANIFIESTO.md`.

## 3. Formatos: 40 dictados (`tests/banco-formatos/casos.tsv`)

Voces locales (Helena de Windows + Piper davefx, sharvard hombre y mujer, claude-mx), 8 cada una. No hay
grabaciones de voz real con estos casos: los 40 van con voz local.

| | Sin pulir | Con pulido |
|---|---|---|
| Bien escritos | **13/40 (33 %)** | **13/40 (33 %)** |

Por tipo (sin pulir): correo 0/8 · teléfono 3/8 · símbolo 2/6 · web 1/5 · hora 0/5 · euro 3/4 · % 4/4.
Parakeet ya escribe cifras («600123456», «15%», «49,99 euros»), pero deja «arroba», «barra», «y media»,
«asterisco» y «almohadilla» en palabras y escribe los teléfonos con guiones. El pulido no cambia nada.

## 4. Clics para encender o apagar el pulido desde que se abre el hub

**3**: Configuración → pestaña «Limpieza e IA» → interruptor. La Flow bar no enseña si está encendido.
