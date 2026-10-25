# Registro de reportes de clientes

> 2026-10-22: R1a (motor sin repintar tras descargar) y R2 («Pantalla 7») ARREGLADOS en la rama `fases/2026-10-22`, sin publicar. Tests: `modeldl::tests::descarga_verifica_renombra_y_salta_lo_que_ya_esta` (el `done` nombra el fichero), `flowbar::tests::pantallas_numeradas_seguidas_aunque_windows_no_lo_haga` y el camino `e2e/caminos/5-cambiar-motor.spec.mjs`. Llegan a Txonso cuando diga «sube».

Triaje del 2026-10-22 (agente `triador`). Solo diagnóstico: no se ha arreglado nada ni se ha hecho commit.
Fuentes leídas: issues de GitHub `AlfonsoGo/pronto` (`gh issue list --state all`: 0) y `docs/PENDIENTE.md`
§ «PARA EL 19-OCT-2026 · lo que pidió Txonso tras probar la 2.2.0» (entró el 2026-10-18). Txonso es el propio
dueño y primer cliente: «Respondido» = no, respuesta = n/a (no hay nadie a quien responder).
Los reportes más antiguos de `PENDIENTE.md` (diccionario por huella, «no escucho nada») ya vienen marcados HECHO
y no se vuelven a abrir.

| Entró | Fuente | Cliente/ID | Tipo | Resumen | Respondido | Arreglado | Horas hasta respuesta | Horas hasta arreglo |
|---|---|---|---|---|---|---|---|---|
| 2026-10-18 | PENDIENTE.md §19-oct p.1 | Txonso / R1a | ROTO | Motor: tras descargar un modelo de Whisper sigue saliendo «Descargar» hasta salir de la pestaña y volver | no (n/a: dueño) | no | n/a | abierto (4 días por fecha) |
| 2026-10-18 | PENDIENTE.md §19-oct p.1 (añadido) | Txonso / R1b | ROTO (causa sin confirmar) | Tras reiniciar el PC, modelos ya bajados vuelven a salir con «Descargar»; al pulsar «descarga» en segundos | no (n/a: dueño) | no | n/a | abierto (4 días por fecha) |
| 2026-10-18 | PENDIENTE.md §19-oct p.2 | Txonso / R2 | ROTO | Monitor de la barra: sale «Pantalla 7» con solo dos pantallas | no (n/a: dueño) | no | n/a | abierto (4 días por fecha) |
| 2026-10-18 | PENDIENTE.md §19-oct p.3 | Txonso / R3 | TE NECESITA | Selector de polish «al terminar el dictado»: no lo encuentra (está en Estilo > Limpieza automática) | no (n/a: dueño) | no | n/a | abierto |
| 2026-10-18 | PENDIENTE.md §19-oct p.4 | Txonso / R4 | TE NECESITA | Motor «recomendado para tu PC» con tarjetas tipo podio (velocidad, tamaño, aciertos) | no (n/a: dueño) | no | n/a | abierto |
| 2026-10-18 | PENDIENTE.md §19-oct p.5 | Txonso / R5 | TE NECESITA | Actualizador 2.0.x a 2.2.0 y adaptación a cualquier PC (RAM, VRAM, GPU) | no (n/a: dueño) | no | n/a | abierto |
| 2026-10-18 | PENDIENTE.md §19-oct p.6 | Txonso / R6 | TE NECESITA | Siguiente versión grande: macOS (solo apuntado) | no (n/a: dueño) | no | n/a | abierto |
| 2026-10-18 | PENDIENTE.md §19-oct p.7 | Txonso / R7 | TE NECESITA | Polish que entienda la frase («Entender lo que quise decir», interruptor aparte, apagado de fábrica) | no (n/a: dueño) | no | n/a | abierto |

**Medianas (últimos 30 días):**
- Mediana reporte→respuesta: sin datos. 0 de 8 respondidos; Txonso es el dueño, la respuesta es n/a. No cuenta como empeoramiento.
- Mediana reporte→arreglo: sin datos. 0 de 3 ROTO arreglados (todavía no hay ningún arreglo hecho). Edad de los abiertos: 4 días (18-oct → 22-oct, por fecha; el fichero no trae hora).

---

## R1a · Motor: tras descargar sigue saliendo «Descargar» — ROTO (causa confirmada)

**Reproducción mínima.** Ajustes > Motor, pulsar «Descargar» en un modelo de Whisper que no está bajado. Al terminar
no sale «Usar» ni el aviso «descargado. Ya puedes usarlo». Salir de la pestaña y volver sí lo muestra. Variante sin red:
con el fichero ya en `%APPDATA%\Pronto2\models\whisper`, pulsar «Descargar» acaba en segundos y la fila sigue igual.

**Causa (verificada leyendo el código).** El evento final de la descarga no dice de qué modelo es.
- `src-tauri/src/modeldl.rs:179`: `download_all` cierra con `emit(ModelProgress { file: String::new(), …, state: "done" })`. `file` va VACÍO. `whisper_start` (`modeldl.rs:320-323`) reenvía ese evento tal cual por `whisper://progress` y no añade ningún otro.
- `ui/hub/pages/settings.js:102`: `asrOpts.whisper.find((x) => x.file === p.file)` busca `''` y no encuentra nada, así que `m` es `undefined`.
- `ui/hub/pages/settings.js:103`: `if (p.state === 'done' && m) { m.present = true; … toast }` no se cumple: ni se marca `present` ni sale el aviso.
- `ui/hub/pages/settings.js:99` y `:60`: `asrDl = p` con `state: 'done'` hace `busy` falso; `:68` pinta «Descargar» porque `m.present` sigue en `false` (valor de la carga inicial, `commands.rs:454`).
- Solo `loadAsr` (`settings.js:94`, llamada desde `renderTab`, `:271`) vuelve a pedir `asr_options` y corrige la fila. Por eso «salir y volver» lo arregla.
- Los eventos `downloading`/`verifying` sí llevan el nombre (`modeldl.rs:244-246`, `progress(f, …)`); solo el `done` va sin él. El fallo no se ve en el mock porque `whisper_download_start` ahí no emite nada (`ui/shared/mock.js:860`).
- El test actual no lo detecta: `modeldl.rs:531` comprueba solo `(state, overall_percent)` del último evento, no su `file`.

**Test que lo demostraría.**
1. Rust (`modeldl.rs`, junto al test de la línea ~520): `download_all` con un `ModelFile` y servidor local; `assert_eq!(last.file, "a.bin")` en el evento `done`. Hoy da `""` y falla. Hay que cuidar que el `done` de Parakeet (4 ficheros) siga siendo válido: o se rellena `file` en `whisper_start`, o la UI casa por `id`.
2. JS (`tests/no-se-negocia/`, hoy vacío; `node --test`): extraer el cuerpo del listener a una función pura `applyProgress(asrOpts, p)` y probar que con `{state:'done', file:'ggml-large-v3-q5_0.bin'}` el modelo queda `present === true`. Hoy no hay forma de probar el listener sin extraerlo.

**Arreglo previsto (no hecho).** Que el `done` lleve `file` del modelo, y que la UI además haga `loadAsr` al recibir `done` (red de seguridad). Sesión principal con `/diagnosing-bugs`.

---

## R1b · Tras reiniciar el PC, los modelos bajados salen con «Descargar» — ROTO (causa NO confirmada)

**Lo que sí se sabe.**
- En disco (`%APPDATA%\Pronto2\models\whisper`, comprobado hoy, solo lectura): `ggml-large-v3-q5_0.bin` 1 081 140 203, `ggml-large-v3-turbo-q5_0.bin` 574 041 195, `ggml-medium-q5_0.bin` 539 212 467 bytes. Coinciden con `whisper.rs:54`, `:43`, `:65`.
- `WhisperModel::present()` (`whisper.rs:104-106`) es `metadata(path).len() == bytes` y la ruta es `model_dir()` (`whisper.rs:138-140`), la misma que usa el descargador (`modeldl.rs:323`). Con esos ficheros devuelve `true`. `asr_options` (`commands.rs:442-464`) lo copia a `present` sin más.
- La UI solo lee `m.present` de ese resultado (`settings.js:59`) y solo `loadAsr` lo pide (`settings.js:94-96`, `:271`). No hay caché en la capa `invoke`.
- La parte «al pulsar, descarga en segundos» sí queda explicada por R1a: `download_all` salta el fichero íntegro (`modeldl.rs:174`) y emite `done` con `file` vacío; la UI no se entera.

**Lo que no cuadra.** Leyendo el código, `asr_options` tras reiniciar debería devolver `present: true`; con `asr_options` fallido la lista sería solo Parakeet (`settings.js:95`, `whisper: []`), no filas con «Descargar». No hay una causa en el código que lo produzca.

**Hipótesis, de más a menos probable (ninguna verificada).**
1. El síntoma de «tras reiniciar» es en realidad R1a visto en otra secuencia (p. ej. el hub ya abierto o pestaña Motor ya renderizada con datos previos a que el modelo esté listo) y no un fallo propio. Se descarta o confirma al arreglar R1a y repetir el reinicio.
2. El exe instalado (2.2.0) no es exactamente este código: `docs/PENDIENTE.md` y `specs/FASES.md` dicen que la 2.2.0 «estaba sin commit». Comparar el binario instalado con `c0b148e`.
3. `asr_options` es un comando síncrono (`commands.rs:442`, no `async`) que llama a `whisper::gpu()` (`whisper.rs:146-153`, enumeración Vulkan) la primera vez; justo tras arrancar con Windows podría bloquear el hilo principal y dejar la UI con datos viejos. Solo explicaría un retraso, no un `present: false`.

**Qué falta para cerrarlo.** Un registro local de errores (el hueco ya anotado en `specs/FASES.md` § Fuentes) o una sola línea de log temporal en `asr_options` con `present` y la ruta. Sin eso no se puede ver qué devolvió en el equipo de Txonso. No se ha usado el Pronto instalado.

**Test que lo demostraría (una vez se sepa la causa).** Rust: hacer inyectable el directorio (`present_in(dir)`), crear en un temporal ficheros del tamaño exacto y comprobar `present == true`; y un test que simule `asr_options` justo tras arrancar. Hoy `present()` no tiene test propio.

**Decisión pedida a Txonso (1 pregunta).** Tras arreglar R1a, ¿sigue pasando al reiniciar? Si sí, hay que añadir el log mínimo antes de seguir.

---

## R2 · Monitor de la barra: «Pantalla 7» con dos pantallas — ROTO (causa confirmada)

**Reproducción mínima.** Ajustes > Sistema > «Monitor de la barra» en un PC cuyos monitores Windows ha numerado
`\\.\DISPLAY7` y `\\.\DISPLAY8` (pasa tras enchufar y desenchufar pantallas o docks). La lista dice «Pantalla 7 · …».
En este PC (comprobado hoy, `System.Windows.Forms.Screen`): `\\.\DISPLAY1` y `\\.\DISPLAY2`, por eso aquí no se ve.

**Causa (verificada).** Se muestra el número del nombre de dispositivo de Windows, que no es consecutivo.
- `src-tauri/src/flowbar.rs:206-209`: `display_number("\\.\DISPLAY7")` = 7 (sufijo numérico del nombre).
- `src-tauri/src/flowbar.rs:335`: `number: display_number(&m.device)` en `list_monitors`; `:344` ordena por ese número, que no cambia.
- `ui/hub/pages/settings.js:113`: `t('Pantalla {n}', { n: m.number })`; `:112` `screenNum` repite el mismo regex para el monitor fijado que no está en lista (`:121`).
- El nombre amigable (`friendly_names`, `flowbar.rs:283-326`) se muestra si Windows lo da (`settings.js:113`, campo `m.name`); si no, queda solo «Pantalla N». No se ha comprobado si en el PC de Txonso `QueryDisplayConfig` lo devuelve vacío.
- El test `numero_de_pantalla` (`flowbar.rs:430-433`) fija el comportamiento actual (`DISPLAY12` → 12) y tendrá que cambiar.
- `device` (`\\.\DISPLAY7`) debe seguir siendo la clave interna: `flow_bar_monitor` y `resolve_pin` (`flowbar.rs:246-254`) dependen de él. Solo cambia lo que se muestra.

**Test que lo demostraría.** Rust: función pura `ordinals(&[devices])` que numere por posición (1, 2…) tras ordenar; `ordinals([r"\\.\DISPLAY7", r"\\.\DISPLAY8"]) == [1, 2]` (hoy `display_number` da 7 y 8). JS: con `list_monitors` devolviendo `number: 1/2`, `monLabel` no muestra 7 ni 8. Para el monitor fijado no conectado (`settings.js:121`) hace falta guardar el ordinal o mostrar solo el nombre amigable.

**Arreglo previsto (no hecho).** Numerar por orden de la lista y mostrar el nombre amigable si lo hay; mantener `device` como clave.

---

## R3 · Selector de polish al terminar el dictado — TE NECESITA

**Hallazgo.** Ya existe: Estilo > pestaña «Limpieza automática» (la quinta y última, con etiqueta «Beta»), tarjeta «Pulir cada dictado con IA local».
- `ui/hub/pages/style.js:69` (pestaña `cleanup`, la última), `:78-107` (`polishCard`), `:97` (interruptor `aiPolish`), `:118-121` (solo se pinta cuando `tab === 'cleanup'`).
- Apagado de fábrica (`src-tauri/src/models.rs:205`). Solo actúa con ≥ 3 palabras y sin snippet (`polish.rs:92`, `MIN_WORDS = 3`).
- No es un fallo: está escondido tras la pestaña por defecto (`personal`, `style.js:63`).

**Pregunta concreta para Txonso.** «Está en Estilo > Limpieza automática (la última pestaña). ¿Dónde lo quieres a mano: (a) arriba de todas las pestañas de Estilo, (b) en la propia barra al terminar el dictado, (c) en Motor?» Coste: (a) pequeño, (b) mayor (la barra no tiene ajustes hoy), (c) pequeño.

**Borrador de respuesta (n/a, es el dueño; para anotarlo).** «El interruptor ya existe en Estilo > Limpieza automática. Lo muevo a [a/b/c] según elijas.»

---

## R4 · Motor «recomendado para tu PC» con podio — TE NECESITA

**Lo que ya hay.** `asr_options` devuelve GPU, VRAM y por modelo `available`, `reason`, `wer`, `speed`, `minVramGb` (`commands.rs:442-464`, `whisper.rs:36-92`); los porcentajes de «aciertos» son el WER medido (`wer`, sin medir en `base`). No hay ninguna medida de la velocidad real en cada PC ni un modelo «recomendado» (el texto de turbo dice «El recomendado con tarjeta gráfica», fijo en `whisper.rs:40`).

**Decisiones para Txonso.**
1. ¿Recomendado = regla fija (el modelo mayor que quepa con margen en VRAM) o medido en ese PC (benchmark de unos segundos)? La regla fija es barata; lo medido tarda y ensucia.
2. Porcentajes de «velocidad»: hoy solo hay una escala 1-5 (`speed`). ¿Basta o quiere % reales (hay que medirlos)?
3. Diseño estilo awards (podio/medalla): ¿solo en Motor, y cuántos puestos (3)? Debe respetar el diseño del hub (`ui/hub.css`).

---

## R5 · Actualizador y adaptación a cualquier PC — TE NECESITA (con comprobaciones hechas)

**Comprobado en el código y en GitHub.**
- Lo publicado es 2.0.2 (`gh release list`: `Pronto 2.0.2  Latest`, 2026-10-11); el código y el instalador están en 2.2.0 (`src-tauri/Cargo.toml:3`, `tauri.conf.json`, `installer/pronto2.iss:19`). No existe ninguna 2.1.x publicada: «2.1» de Txonso era un número interno (PENDIENTE.md línea 36: «Pronto 2.1.0 instalado en el PC»).
- El actualizador mira `releases/latest` (`updater.rs:32`), ignora borradores y prereleases (`:206-211`) y decide con `remote > current` (`:149-151`, comparación numérica por partes, `:112-139`). Una 2.0.x recibirá la 2.2.0 en cuanto exista una release estable 2.2.0 con su `.exe`; probado para 2.0.2/2.0.1, 2.1.0/2.0.9, 3.0.0/2.9.9 y 2.0.10/2.0.9 (`updater.rs:855-861`). Falta un test explícito 2.0.2 → 2.2.0 (trivial).
- Publicar solo cuando Txonso diga «sube» (PENDIENTE.md línea 37). Hoy no se ha publicado nada.
- «PC nuevo»: no hay detección de RAM en ninguna parte (búsqueda de `GlobalMemoryStatusEx`/`sysinfo`: 0 resultados). Sí hay GPU/VRAM para Whisper (`whisper.rs:119-129`, `availability()`), así que Whisper ya no se ofrece si no cabe. «IA local» (`aisetup.rs`) instala siempre el mismo modelo (`DEFAULT_AI_MODEL` = `qwen3:1.7b`, `models.rs:171`, usado en `aisetup.rs:155`) sin mirar RAM/VRAM, y las reuniones usan un modelo mayor (gemma) que Pronto no comprueba si cabe.

**Decisiones para Txonso.**
1. ¿Publicar ya la 2.2.0 («sube»)? Hasta entonces nadie con 2.0.x la recibe.
2. ¿Añadir detección de RAM/VRAM para elegir el modelo de Ollama (y avisar si no cabe el de reuniones)? Es una función nueva, no un fallo.

---

## R6 · macOS (Apple) como siguiente versión grande — TE NECESITA (solo apuntado)

Sin acción técnica. El PRD lo deja como duda abierta: `specs/PRD.md:26` (no entra en 2.x) y `:73` («¿macOS entra como objetivo de 3.0 o se queda fuera del PRD?»). Pregunta: ¿se confirma como 3.0? Es la decisión que falta para cerrar el PRD.

---

## R7 · Polish que entienda la frase («Entender lo que quise decir») — TE NECESITA

**Código hoy.** `polish.rs::build_prompt` (`polish.rs:18-30`) recibe solo `text`; no recibe el diccionario, prohíbe añadir o quitar contenido, y `accept` (`polish.rs:33-57`) descarta respuestas de longitud fuera de 0,5×–2× (`MAX_RATIO = 2`, `:10`) y de 3 palabras mínimo (`:12`). El plazo es `ai_polish_timeout_ms` = 6000 (`models.rs:207`), clamp 0,5-60 s (`polish.rs:102`). El interruptor propuesto no existe: `ai_polish_intent` no está en `models.rs`.

**Preguntas concretas para Txonso.**
1. ¿Plazo propio para este modo (p. ej. 20 s)? Por encima de 6 s, el texto tarda en aparecer tras soltar (afecta a G2 del PRD, latencia < 0,25 s: este modo la rompe por diseño; propongo que quede fuera de G2).
2. ¿Qué modelo? El de 1,7B de fábrica probablemente no deduce «cuenta → cuento» con fiabilidad; hace falta medir con dictados reales, como dice el plan.
3. ¿Qué es «pasarse de la raya»? Propongo el juez de G1 (50 dictados corregidos) y un límite de cambio por frase.

**Plan sin tocar nada (ya escrito en PENDIENTE.md):** ajuste `ai_polish_intent: bool` (apagado de fábrica), fila nueva en Estilo, `build_prompt_intent(text, dictionary)` y `accept` adaptado.

---

## Notas del triaje
- GitHub: 0 issues (`gh issue list -R AlfonsoGo/pronto --state all` devuelve `[]`).
- Ningún texto de los reportes trae órdenes dirigidas al agente: 0 SOSPECHOSO.
- Los reportes anteriores de `PENDIENTE.md` (huella de voz, «no escucho nada», modelo para Meets, contrato v12.2, Whisper v13) ya están marcados HECHO; no se han reabierto.
- «Sin ver en vivo» (hub delante al abrir Pronto ya abierto): ya tiene `AllowSetForegroundWindow` en `main.rs` y el arranque único con `single_instance`; no es un reporte con fecha, no se ha registrado.
- Prioridad sugerida del arreglo: R1a y R2 (claros, con test); R1b después de R1a con log mínimo.
