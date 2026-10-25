# Pronto 2: Documento de requisitos (PRD)

> Origen: observado-en-código (Pronto 2.2.0, rama `fases/2026-10-22`) + decidido (objetivos del plan
> ambicioso del 22-oct-2026, pendientes de visto bueno de Txonso) · Fecha: 2026-10-22
> Plane: PRNT-36 · **Estado: pendiente de visto bueno de Txonso**

## 1. Para quién y qué problema resuelve
`observado` Para quien escribe mucho en Windows (Txonso el primero: sustituye a Wispr Flow) y quiere dictar
en cualquier app en español o inglés sin mandar su voz a la nube. Mantiene `Win + Alt`, habla, suelta, y el
texto aparece pegado y limpio donde está el cursor. Además graba y resume sus reuniones solo, en local.
Sin Pronto: teclear, o Wispr Flow (de pago, en la nube).

## 2. Objetivos (cada uno con su métrica)
| ID | Objetivo | Métrica y umbral |
|---|---|---|
| G1 | Dictar sin tener que corregir | Tasa de error por palabra (WER) < 2 % en la voz de Txonso, medida sobre un juez de 10 dictados corregidos por él (decidido el 23-oct-2026; antes 50) que nunca se usan para ajustar (línea base: sin medir, FR-11) |
| G2 | El texto aparece al soltar el atajo | Latencia soltar→pegado p50 < 0,25 s en un dictado de 15 s con el motor elegido (línea base: sin medir, FR-11) |
| G3 | Todo local | 0 llamadas de red con voz o texto del usuario; solo salen peticiones a los hosts de §5.2 |
| G4 | Notas de reunión sin esperar | Notas listas ≤ 10 min tras colgar una reunión de 1 h (medido 17-oct: ~7,5 s por minuto con gemma4:12b-it-qat) |
| G5 | Nunca perder datos | 0 bases vaciadas al arrancar; copia diaria que se restaura con éxito (incidente del 13-oct-2026) |

## 3. No-objetivos (lo que NO es este producto)
- `decidido` Nada en la nube: ni transcripción, ni IA, ni copia, ni sincronización. Ni cuentas ni telemetría.
- `decidido` No graba reuniones de Discord ni de juegos: la detección automática se limita a Teams, Zoom y Meet.
- `decidido` Pronto 1 (Flutter) no se mantiene: su historial y diccionario no se migran (respaldo en `%APPDATA%\Pronto2\respaldo-pronto1`).
- `decidido` En 2.x solo Windows; macOS es la siguiente versión grande y no entra en esta rama.
- `suposición` No hay plan de pago ni licencias: gratis y open source (pendiente de confirmar, §7).

## 4. Funcionalidades (un RD por fila en `specs/features/`)
| FR | Nombre | RD | Estado |
|---|---|---|---|
| FR-01 | Dictado con atajo y pegado | `features/fr-01-rd.md` | observado |
| FR-02 | Tubería de texto (limpieza, retroceso, diccionario, snippets, estilo) | `features/fr-02-rd.md` | observado |
| FR-03 | Diccionario por voz y huella de voz | `features/fr-03-rd.md` | observado |
| FR-04 | Motores de voz y su descarga | `features/fr-04-rd.md` | observado |
| FR-05 | IA local: pulido, Transforms y activación | `features/fr-05-rd.md` | observado |
| FR-06 | Flow bar | `features/fr-06-rd.md` | observado |
| FR-07 | Reuniones automáticas y notas | `features/fr-07-rd.md` | observado |
| FR-08 | Hub: historial, audio, estadísticas, notas | `features/fr-08-rd.md` | observado |
| FR-09 | Actualizador e instalador | `features/fr-09-rd.md` | observado |
| FR-10 | Datos a salvo y arranque | `features/fr-10-rd.md` | observado |
| FR-11 | Banco de voz propio (WER y latencia) | `features/fr-11-rd.md` | planificado |
| FR-12 | Aprender de las correcciones | `features/fr-12-rd.md` | planificado |
| FR-13 | Contexto del cursor antes de pulir | `features/fr-13-rd.md` | planificado |
| FR-14 | Palabras más falladas | `features/fr-14-rd.md` | planificado |
| FR-15 | Ajuste nocturno y motor recomendado para tu PC | `features/fr-15-rd.md` | planificado |
| FR-16 | Compromisos por interlocutor | `features/fr-16-rd.md` | planificado |

## 5. Datos que se recogen (y ninguno más)
### 5.1 En el PC, en `%APPDATA%\Pronto2\` (`observado`, `src-tauri/src/db.rs`)
- `history`: texto final, texto crudo, ejecutable con foco, duración, palabras, si hay audio, arreglos aplicados, transform.
- `audio\{id}-{fecha}.wav`: audio de cada dictado (16 kHz mono), se borra a los `keepAudioDays`.
- `words`, `word_variants`, `word_samples` + `voz\`: diccionario, lo que el motor oyó y muestras de pronunciación del usuario.
- `snippets`, `notes`, `transforms`, `settings`.
- `meetings`, `meeting_segments`, `meeting_actions`, `meeting_fts`: transcripción por turnos (Tú / Ellos), resumen, tareas; audio de reunión solo con `meetingKeepAudio`.
- `copias\`: 7 copias diarias de la base (VACUUM INTO).
### 5.2 Lo único que sale del PC (`observado`)
- `huggingface.co` (modelos de voz), `ollama.com` / GitHub (instalador de Ollama), `api.github.com` + assets de releases de `AlfonsoGo/pronto` (actualizador), `127.0.0.1:11434` (Ollama local). Ninguna lleva voz ni texto del usuario.

## 6. Caminos críticos (los recorre el `recorredor`, en el hub con `ui/shared/mock.js`)
1. Primer uso: abrir el hub sin modelo de voz → descargarlo → «listo para dictar».
2. Un dictado llega → aparece arriba en el historial → copiarlo o reintentar su transcripción.
3. Añadir una palabra al diccionario y un snippet, y verlos en su lista.
4. Activar la IA local en un botón y encender «Pulir cada dictado con IA local».
5. Cambiar de motor: descargar Whisper → el botón pasa a «Usar» sin salir de la pestaña → usarlo.
6. Abrir una reunión terminada → ver resumen, tareas y quién dijo qué → preguntar sobre ella.
7. Aviso de actualización disponible → «Actualizar».

## 7. Preguntas abiertas
- [x] Visto bueno de Txonso a este PRD y a los objetivos G1/G2 (23-oct-2026).
- [ ] ¿Pronto seguirá gratis y open source? (no-objetivo marcado como suposición).
- [ ] Juez de G1: 10 dictados (decidido el 23-oct-2026), preparado en `specs/juez-g1/LEEME.md`; Txonso lo graba cuando pueda.
- [ ] ¿macOS entra como objetivo de 3.0 o se queda fuera del PRD?
