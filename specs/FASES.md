# Fases — estado de los 7 sistemas

> Lo mantiene la skill `/fases`; la tabla de estado la reescribe `node .claude/fases/fases.mjs verificar`
> (no la edites a mano). Evidencia = una salida real o un fichero, nunca "hecho" a secas.

Plane: PRNT-36
Tipo: desplegado
Perfil: crítico
Rama de trabajo: fases/2026-10-22

<!-- estado:inicio -->
| Sistema | Estado | Evidencia | Falta |
|---|---|---|---|
| F0 Diagnóstico | ✅ | Tipo desplegado · Perfil crítico |  |
| S1 Plan y prueba | ✅ | PRD: 5 objetivos · 18 RDs |  |
| S4 No se negocia | ✅ | 7 reglas |  |
| S2 Rumbo | ✅ | specs/rumbo/2026-10-25-f.md: 🟢 |  |
| S3 Seguridad y puertas | ✅ | specs/seguridad/2026-10-25-f.md: críticos 0 · altos 0 |  |
| S5 Rompedor | ✅ | specs/rompedor/2026-10-25-f.md: 0 altas |  |
| S6 Ojos de cliente | ✅ | guardia: tests verdes · specs/primerizo/2026-10-25.md: guardia en vivo ✅ · sesiones sin error 100 % ≥ 95 % |  |
| S7 Bucle de cliente | ✅ | specs/clientes/registro.md |  |
<!-- estado:fin -->

Estados: ⬜ ausente · 🟨 parcial · ✅ activo (probado con evidencia) · ➖ no aplica

## Adaptación a una app de escritorio
Pronto es Rust/Tauri, no una web. Lo que en la plantilla es «la app en localhost» aquí es:
- **Hub en navegador**: `ui/hub.html` servido en estático con el backend simulado `ui/shared/mock.js`
  (`node scripts/servir-ui.mjs`, puerto 4173). Ahí trabajan el primerizo y el recorredor.
- **Backend**: tests de Rust (`cargo test` en `src-tauri/`) con base de datos temporal. Ahí trabaja el rompedor.
- **Prohibido** (pedido de Txonso): usar su ratón, sus altavoces o su Pronto instalado; parar Ollama;
  tocar `%APPDATA%\Pronto2` (datos reales) salvo leer copias; publicar o hacer push sin «sube».

## Umbrales del proyecto
- Sesiones sin error para publicar (recorredor): **95 %**
- Rompedor: publicar con **0 altas** abiertas
- Seguridad: publicar con **0 críticos y 0 altos**
- Rumbo: publicar con **🟢** (o 🟡 con la decisión escrita en *Preguntas abiertas* del PRD)

## Zonas (deuda de *slop* aceptada)
| Zona | Perfil (`estandar-pruebas.md` §6) | Rutas de este proyecto |
|---|---|---|
| crítica | 1–7 + experto-seguridad | `src-tauri/src/updater.rs` `src-tauri/src/update_helper.ps1` `installer/` `src-tauri/src/modeldl.rs` `src-tauri/src/aisetup.rs` (descargan y ejecutan) · `src-tauri/src/hotkey.rs` `src-tauri/src/inject.rs` (gancho de teclado y portapapeles) · `src-tauri/src/db.rs` `src-tauri/src/meetingdb.rs` (datos) · `src-tauri/src/ollama.rs` `src-tauri/src/polish.rs` `src-tauri/src/notes.rs` `src-tauri/src/notes/` `src-tauri/src/summary.rs` (entrada a LLM) · `src-tauri/src/meeting.rs` `src-tauri/src/detect.rs` `src-tauri/src/auto.rs` (graba a terceros) · `src-tauri/src/voiceprint.rs` `src-tauri/src/pronounce.rs` (voz del usuario) |
| normal | 1, 2, 3, 5, 7 | `src-tauri/src/asr.rs` `src-tauri/src/whisper.rs` `src-tauri/src/audio.rs` `src-tauri/src/segment.rs` `src-tauri/src/text.rs` `src-tauri/src/flowbar.rs` `src-tauri/src/tray.rs` `src-tauri/src/commands.rs` `src-tauri/src/state.rs` `src-tauri/src/models.rs` `src-tauri/src/main.rs` `src-tauri/src/autostart.rs` `src-tauri/src/media.rs` |
| menor | 1 + gates | `ui/` `docs/` `scripts/` |

## Fuentes de clientes (S7)
- Issues de GitHub `AlfonsoGo/pronto` (público) — conectado: sí, lectura con `gh` (0 issues a 22-oct-2026).
- Reportes de Txonso al probar cada versión — `docs/PENDIENTE.md` (es el primer cliente real).
- Descargas por release (`gh api …/releases`): v2.0.2 = 7, v2.0.1 = 2, v2.0.0 = 1 a 22-oct-2026.
- Sin registro de errores en la app (no hay fichero de log en `%APPDATA%\Pronto2`): hueco, ver Decisiones.

## VELOCIDAD · HECHO EL 24-OCT (sesión de seguimiento; nada subido)
Rama `fases/2026-10-22`, commits locales tras `182f8bf`. Detalle y cifras: `specs/velocidad/2026-10-24.md`.
1. ✅ `npm test` repetido: reglas 18/18, `cargo test` 570 ✅; al cerrar, 18/18 y 574 ✅ (64 ignorados: bancos y sondas con GPU).
2. ✅ `/fases` sobre lo nuevo. Rumbo 🟡 (`specs/rumbo/2026-10-24.md`), seguridad 0 críticos · 0 altos · 1 media ·
   4 bajas (`specs/seguridad/2026-10-24.md`), recorredor 35/35 (100 %), rompedor 0 altas · 1 media · 4 bajas
   (`specs/rompedor/2026-10-24.md`, 17 sondas en `src-tauri/src/rompedor_whisper.rs`). Arreglado:
   - la huella ya no tira un dictado si falla su pasada con DTW (regla 5) y hace una sola pasada (antes dos, +52 %);
   - «Reintentar transcripción» con huella cede a los dictados (seguridad MEDIA 1, test en `rompedor_carga`);
   - `verify()` del modelo antes de abrir el contexto con DTW (seguridad BAJA 2);
   - test con la huella SHA-256 del vendor de whisper.cpp y del `[patch]` en Cargo.lock (seguridad BAJA 1);
   - **el vendor no recompilaba whisper.cpp al cambiar su código** (`build.rs` copiaba una vez y solo vigilaba
     `wrapper.h`): arreglado, y comprobado que el parche del encoder de anoche sí estaba en el binario medido.
   Abierto (bajas, a Plane): latido a medias al soltar (+72 ms en ~1 de cada 3 dictados), `detect` doble de la
   huella (4-10 ms), `precise_failed` que no se reintenta hasta reiniciar, el Arc del motor viejo en el latido (≤ 120 s),
   `.gitattributes` para `vendor/** -text`, el WER del juez sin banco en el repo (rumbo H5).
3. ✅ Memoria medida (`whisper::tests::banco_memoria`): turbo con los dos contextos y el latido = 2,2-2,7 GB de vídeo.
   **Decisión:** `room_for_two` usa lo medido por modelo (`pair_vram_gb`) en vez del doble de `min_vram_gb`: turbo con
   latido y precarga desde tarjetas de 4 GB (antes 6), large-v3 desde 6 GB. Riesgo apuntado: en 4 GB con Ollama en la
   GPU, el contexto con DTW precargado le quita ~1 GB.
4. ✅ Meets con Whisper (dos reuniones sintéticas nuevas, Ollama apagado): 0 inversiones, 0 palabras al otro
   hablante, 0 palabras propias quitadas como eco; 3,8-5,0 s por minuto de reunión frente a 5,4-5,8 de Parakeet.
   Falta la parte de notas con Ollama (estaba apagado; no se arrancó).
5. ✅ **Whisper solo en español o inglés** (pedido de Txonso el 24-oct: medium le salía en portugués). Contrato v14.1.
   Con medium, 4 de 10 dictados cortos del juez salían en italiano o gallego; ahora 36/36 en español y 16/16 en
   inglés con los tres modelos; latencia igual (turbo p50 174 ms frente a 178). El texto del hub del diálogo de
   idiomas lo dice («Con Whisper, español o inglés»), aunque su aviso aún habla solo de Parakeet.
6. ✅ Pedidos de Txonso del 24-oct (tarde): **Parakeet sigue de fábrica** (ya lo era; su favorito). **Etiquetas de la
   pestaña Motor con lo medido**: cada modelo lleva una etiqueta, un consejo y sus cifras (errores en el juez y en
   reuniones, tiempo por frase): Parakeet «Recomendado · el más probado», turbo «El más rápido» («si tienes una gráfica
   potente, este es el mejor para ti»), medium «Menos errores», large-v3 «Solo con gráfica grande», small «Para
   gráficas pequeñas», base «Sin probar». **Idioma de Whisper según el ajuste** «Idiomas de dictado» que ya existía:
   Español/Inglés lo fijan, Automático elige entre los dos (ya funcionaba; el aviso del diálogo decía que no cambiaba
   nada y ahora lo explica). Polish con Parakeet: descartado por Txonso («a mí no me funcionaba»).
7. Txonso: probarlo en su PC con un instalador nuevo (no se ha hecho; regla: nada de usar su Pronto para probar).
8. Ideas sin hacer: Parakeet (motor sin GPU) sigue en 350-550 ms por frase; pasar `deepsec process` sobre el diff
   (cuesta dinero: OK de Txonso).

## SIGUIENTE (actualizado el 23-oct-2026 por la noche)
Rama `fases/2026-10-22` (último commit al cerrar el 23-oct: ver `git log`), nada publicado ni subido. Plane PRNT-36.
Hecho el 23-oct: todo el «PARA MAÑANA» del 22-oct salvo los pasos 4-7; segunda pasada de los 7 rompedores
(`specs/rompedor/2026-10-23.md`) y tercera tanda de arreglos (dictado, hub, reuniones, carga y descargas, datos).
Batería al cerrar: reglas 17/17 (velocidad p50 826 ms), recorrido 35/35, `cargo test` 504 ✅ y 1 intermitente
(`rompedor_dos_arranques_a_la_vez_sobre_base_nueva`: falla 1 de cada ~4 vueltas con carga; dos arranques a la vez sobre
una base nueva, que el candado de instancia única impide en la app: BAJA para la tercera pasada de datos).

1. ✅ Decisión 1 de Txonso (IA opción A) implementada el 23-oct por la noche. **Falta que Txonso pruebe en su PC la vista previa del Transform** (Enter y Esc no llegan a la app, el pegado tras Enter, la zona transparente de 300 px de la Flow bar no bloquea clics) con un instalador nuevo.
2. **Tercera pasada de los 7 rompedores** sobre el código final, hasta `0 altas`; informe consolidado nuevo con la fecha.
3. **Rumbo de los cambios** (`/rumbo cambios`), **escaneo de seguridad del diff** (`specs/seguridad/<fecha>.md`) y
   **recorredor** (`npm run e2e`): más nuevos que el último código.
4. **Primerizo** (S6): solo desde una sesión abierta EN `BELTRANLABS2\ProntoV2` (agentes y guardia).
5. **Actualizar los RD y `docs/ARQUITECTURA.md`** con lo cambiado el 22 y 23-oct (anclaje, `meta`/`base_id`, copias con
   identidad, `respaldo-*`, cortacircuitos de Ollama, plazos de descarga, retroceso del dictado, detección de reuniones).
   FR-02 R4: las correcciones sin pausa solo saltan si lo que sigue es del mismo tipo.
6. `node .claude/fases/fases.mjs publicar .` → PUEDE PUBLICARSE. Publicar sigue siendo de Txonso («sube»).

Efectos de los arreglos del 23-oct que Txonso debe conocer: un dictado inaudible con sonido (tos, teclado) deja una fila
con audio en el historial; una reunión de Teams en el navegador con nombre libre ya no se graba sola (a mano o por el
aviso); el rojo de los botones de borrar es más oscuro (#C8333A) por contraste; plazo de Transform de 45 s.

## Decisiones pendientes de Txonso
Respondidas el 23-oct-2026 por la noche («1A, 2 sí, 3 solo este PC, 4 sí, 5 sí, 6 sí, 7 nada, 8 y 9 luego, juez de 10»):
- ✅ **IA (regla 7): opción A.** Transform con vista previa en la Flow bar y Enter para pegar; tareas y decisiones de
  reunión con cita literal y etiqueta «Generado con IA local»; anclaje solo para cifras, enlaces e IBAN. Regla 7
  reescrita en `CLAUDE.md`. En construcción (23-oct noche).
- ✅ **PRD y reglas NO SE NEGOCIA aprobados** (`CLAUDE.md`, `specs/PRD.md` §7).
- ✅ **Ollama solo en este PC:** una URL remota se rechaza al guardar (regla 1 enmendada). En construcción.
- ✅ **Meets pregunta la primera vez** antes de grabar sola; luego graba sola como antes. En construcción.
- ✅ **Plazo de Transform de 45 s:** se queda.
- ✅ **Registro local de errores** (sin red, sin texto del usuario). En construcción.
- ✅ **Frases de `docs/ARQUITECTURA.md` sobre el app.asar de Wispr:** no se tocan (decisión de Txonso).
- ⏳ **Juez de G1 con 10 dictados** (antes 50): preparado en `specs/juez-g1/LEEME.md`, Txonso lo graba cuando pueda (~5 min).
- ⏳ **Huella de voz** (grabar 3 veces una palabra, FR-03 R10): en el mismo LEEME, ~2 min.
- ⏳ Cumplimiento IA: nombrar a «Ellos» por la voz = alto riesgo → hacerlo por texto o lista de participantes (sin decidir).
- ⏳ Triaje 22-oct (`specs/clientes/registro.md`): ideas de Txonso del 18-oct sin prisa (polish más a mano, motor
  recomendado con podio, actualizador con RAM/VRAM, macOS como 3.0, polish con intención).

## Registro (más reciente arriba)
- 2026-10-25 · rumbo (3597264) · 🟡 DERIVA LEVE: 0 ❌; FR-05 R3b (dirección de loopback de OLLAMA_HOST) y FR-08 R6b (foco al cerrar antes del primer frame) puestos al día; specs/rumbo/2026-10-25-d.md.
- 2026-10-25 · rumbo (f186b73→e753561) · 🟡 DERIVA LEVE: 0 ❌; FR-05 R3c y FR-08 R6b puestos al día con el vigilante de generación mínima, el foco en «Cancelar» y Alt+flecha; specs/rumbo/2026-10-25-c.md.
- 2026-10-25 · rumbo (236b251→f186b73, 2.3.1) · 🟡 DERIVA LEVE: 0 ❌; confirmación de retención, cortacircuitos «atascado» y Novedades acumuladas, ya en FR-05 R3c, FR-08 R6b, FR-18 R3/R6 y PRD §6 camino 8; specs/rumbo/2026-10-25-b.md.
- 2026-10-25 · revisión de la 2.3.0 publicada: puerta de publicar NO (rumbo, seguridad, recorredor y rompedor anteriores al código). Rumbo 🟡, seguridad 0/0 (1 media y 3 bajas arregladas), recorredor 40/40, rompedor 1 alta + 2 medias + 4 bajas arregladas; guardia del primerizo enganchada en .claude/settings.json (bloqueo comprobado). Sale como 2.3.1.
- 2026-10-24 · seguimiento de velocidad: npm test 18/18 + 570 ✅; rumbo 🟡, seguridad 0/0/1/4, recorredor 35/35, rompedor 0 altas/1 media/4 bajas sobre Whisper rápido; arreglos y v14.1 (Whisper solo es/en) en commits locales. Ver «VELOCIDAD · HECHO EL 24-OCT».
- 2026-10-23 noche · S5 · tercera tanda de arreglos guardada (9fd526a carga/descargas, e88fa38 dictado, 1ab89ae reuniones, 63c1d8b datos, b029c67 hub, 2658d44). Batería: reglas 17/17, recorrido 35/35, `cargo test` 504 ✅ + 1 intermitente (dos arranques a la vez). Quedan las 10 altas de IA a la espera de la decisión 1.
- 2026-10-23 tarde · S5 · carga (b4e0d58) y datos (0e5c4d2) arreglados; apagón a media tarde sin pérdidas (rama intacta, datos retomado). Batería en reposo: reglas 16/16 (velocidad p50 757 ms), `cargo test` 436 ✅ 0 ❌, recorrido 35/35. Lanzada la segunda pasada de los 7 rompedores (informes en `specs/rompedor/2026-10-23/`).
- 2026-10-23 · S5 · descargas terminado (ca4f00b): redirecciones a mano en actualizador, modelos, instalador de Ollama y Ollama, sin pánico de ureq; tope y plazo (PRNT-41). Arreglos pequeños (1a0b4f7): el actualizador espera a las notas de una reunión; texto de transcripción incompleta. Batería: `cargo test` 417 ✅ 0 ❌; recorrido 35/35; reglas 15/16: la 3 (velocidad) dio 2835 ms con la CPU al 100 % por otra sesión (Claqueta renderizando): repetir en reposo. Datos y carga, en curso.
- 2026-10-22 noche · tanda paralela: 7 rompedores (14 altas, 27 medias, 25 bajas). Arreglados y con commit: IA (anclaje: adversarial 21 %→100 %, falsos positivos →0 %), dictado, reuniones, hub. A medias: descargas (WIP). Pendientes: datos y carga. Parado a petición de Txonso; sigue en «PARA MAÑANA».
- 2026-10-22 · S2 · `/rumbo todo`: 🟡 deriva leve, 70 ✅ · 27 🟡 · 0 ❌ · 3 ⬜ en FR-01..10; FR-11..16 planificados (`specs/rumbo/2026-10-22.md`).
- 2026-10-22 · S6 · recorredor 100 % (35/35) tras arreglar la descarga de Whisper.
- 2026-10-22 · S5 · rompedor: 2 altas (base vacía del 13-oct reproducida; modelo dañado tumba Pronto) → arregladas con test en 7928069; segunda pasada en curso.
- 2026-10-22 · S7 · triaje: 8 reportes de Txonso (18-oct), 3 ROTO (R1a motor sin repintar y R2 «Pantalla 7» con causa confirmada; R1b sin confirmar), 5 TE NECESITA, 0 issues en GitHub.
- 2026-10-22 · cumplimiento IA previo · 🟢, 0 🔴, 3 🟠, 7 🟡.
- 2026-10-22 · línea base · `cargo test`: 224 pasan, 0 fallan, 20 ignorados (3 min 42 s con compilación).
- 2026-10-22 · F0 · rama `fases/2026-10-22` sobre el commit `c0b148e` (2.2.0 tal cual estaba sin commit); ticket Plane PRNT-36 en progreso; plantillas instaladas (`fases.mjs instalar --tipo desplegado`).
