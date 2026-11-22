# Ningún dictado perdido, correos con voz real, velocidad, texto en vivo, interfaz y candado (BELTR-PRT-30)

> Encargo del 21-nov-2026 tras la prueba de Txonso con la versión de pruebas (20-nov, noche). Un solo
> ticket, sin subtareas. Contrato escrito ANTES de cambiar código. Cada línea se cumple con una cifra o
> un test; si una no se cumple, el trabajo no está hecho y se dice cuál falta.

## Contrato de «hecho»

| # | Qué | Cómo se comprueba | Antes (medido) | Meta |
|---|---|---|---|---|
| H1 | Ningún dictado con voz se queda sin texto | test Rust con los audios reales de esa noche (fuera del repo) + casos sintéticos en `npm test` | 6 de 32 dictados de la noche del 20-nov sin texto (293, 297, 298, 308, 309, 315), todos con voz | 0 con voz sin texto; 0 dictados fuera del historial con su audio |
| H2 | Soltar a destiempo, carga y Ollama colgado no pierden nada | tests Rust y `tests/no-se-negocia/05-experiencia` | sin pruebas de soltar mientras se confirma un segmento o se anticipa, ni con el motor que devuelve vacío o falla | 0 dictados perdidos en todos los casos; la Flow bar dice «Transcribiendo…» o «Puliendo…» hasta que acaba |
| H3 | Correos con la voz de Txonso | `tests/banco-formatos/` conjunto «voz real» (audios fuera del repo) con y sin «arroba», «gmail», «hotmail» en el diccionario | sin grabaciones de correos con referencia | guion listo; % medido con y sin diccionario cuando Txonso grabe (si no ha grabado, este punto queda «falta grabar») |
| H4 | Velocidad sin carga | `npm test` (regla 3) y `npm run test:lento` | 15 s con pulido p50 1518 ms (con un juego abierto) | ≤ 1500 ms p50 con pulido; regla 3 ≤ 1068 ms |
| H5 | Texto en vivo a un clic | test de la maqueta del hub | 3 clics (Configuración → General → interruptor) | 1 clic desde la página Dictado |
| H6 | Interfaz adaptable | Chrome headless en varios anchos (hub y Flow bar) | sin medir | 0 piezas cortadas, solapadas o fuera de la ventana |
| H7 | Candado de la Flow bar | Chrome headless: el ratón va de la barra al candado a varias velocidades | sin medir | 100 % de intentos llegan al candado con la barra a la vista |
| H8 | Las 7 reglas de «NO SE NEGOCIA» siguen verdes | `npm test` entero, sin el PC cargado | — | sale con 0 |
| H9 | Revisión de alguien que no vio el trabajo | subagente nuevo corre las pruebas | — | APTO |
| H10 | Sin publicar | git | — | 0 push, 0 release; commit sin rastro de IA y sin los cambios de otras ventanas; sin tocar `%APPDATA%\Pronto2` ni el Pronto instalado |

## Causa de los dictados sin texto (medida el 21-nov-2026)

Los 6 dictados vacíos se hicieron con Whisper medium e idioma «automático». Con el PC sin carga, Whisper
los vuelve a dejar vacíos; Parakeet los entiende todos. Con «automático», el whisper.cpp de Pronto elige
entre español e inglés y la probabilidad de «no hay voz» del segmento sale entre 0,62 y 0,86 con voz
clara. Pronto tiraba todo segmento por encima de 0,6 (`NO_SPEECH_MAX`) sin mirar si el texto era seguro.
El mismo audio con «es» fijo da 0,00. La seguridad del texto (log-probabilidad media de los tokens) era
de −0,06 a −0,37 en esos 6; el silencio que se inventa una frase da −6,66.

La regla de Whisper original (OpenAI) solo da un segmento por silencio si pasan las dos cosas a la vez:
«no hay voz» > 0,6 **y** log-probabilidad media < −1.

No era la GPU cargada: el fallo se repite sin carga. El audio sí se guardaba (regla 5 técnica), pero el
texto no aparecía y la Flow bar decía «No se entendió nada».

Línea base de Whisper medium antes del arreglo (21-nov, sin carga): sintéticas WER 7,1 %, reales 11,2 %.

## Decisiones

- **Whisper: un segmento es silencio solo si «no hay voz» > 0,6 y además su log-probabilidad media < −1**
  (`whisper::es_silencio`), la regla de Whisper original. Medido: los 6 vacíos salen con texto, el silencio
  sigue vacío y el WER de Whisper medium no cambia (sintéticas 7,1 %, reales 11,2 %).
- **Revisión del 22-nov (NO APTO, arreglado):** con «arroba, Gmail, Hotmail» en el texto previo y la regla de
  arriba, un zumbido sin voz salía «www.gmail.com» (regla 7). Medido con 9 ruidos y los 53 dictados reales de
  Txonso: el ruido que se inventa frases da −0,76 a −0,90 y la voz con «no hay voz» alto, −0,17 a −0,46. El
  corte pasa a −0,6 (`LOGPROB_SILENCIO`), lo de menos de −2 es basura siempre (`LOGPROB_BASURA`) y un trozo sin
  letras ni cifras («...») no es nada dicho. 48 casos (voz y ruido × 3 textos previos/idiomas): 0 fallos.
- **Red de seguridad en el dictado por segmentos** (`state::juntar_segmentos`): un segmento confirmado o un
  texto anticipado vacíos con voz en su audio se vuelven a transcribir al soltar; y si todo sale vacío con
  voz, se repite con el audio entero. El texto anticipado vacío ya no se guarda (`start_live`).
- **El atajo durante la transcripción del dictado anterior empieza otro** (`state::puede_empezar`). Antes se
  ignoraba y lo dicho se perdía sin grabarse: con el PC cargado, la transcripción tarda y Txonso volvía a
  pulsar. Tras la revisión del 22-nov (dos pegados a la vez podían pegar el mismo texto dos veces; el
  anterior no pegaba con el atajo del nuevo pulsado; `active` se apagaba con un dictado sin guardar), se
  eligió con Fable la **cola única** (`spawn_transcriptor`): un hilo transcribe y pega de uno en uno, en
  orden; el anterior espera a que acabe la grabación nueva para pegar (`AppState::esperar_grabacion`) y, si
  entretanto cambió la ventana de delante, no pega en ella; `active` solo se apaga sin grabación ni dictados
  pendientes (`AppState::liberar`); tras un toque o un cancelar con uno pendiente, la barra vuelve a
  «Transcribiendo». `inject::paste_text` ya no manda el Ctrl+V con Win/Alt/Ctrl/Mayús aún pulsadas: devuelve
  error y la barra dice que está en el historial.
- **La Flow bar dice «Transcribiendo…»** con letras si el final tarda más de 900 ms (estado `transcribing`).
- **Correos**: reglas nuevas en `text.rs` para lo que escribe Whisper con la voz de Txonso («x.arroba.y.com»,
  «x, arroba, y.es», «aroba», dominio partido «nova lumen.com»), y «arroba, Gmail, Hotmail» siempre en el
  texto previo de Whisper (`whisper::PROMPT_FIJAS`). Medido con los 7 correos de la noche del 20-nov: Whisper
  1/7 → 5/7 sin tocar el diccionario; guardarlas en el diccionario no añade nada; Parakeet 2/7 → 2/7 (oye
  «Marta. Ruiz arroba nova lumen.», sin «punto com»: no es formato).
- **Velocidad**: probado y **quitado** mirar el fin de la voz cada 100 ms (en vez de en cada tick de 700 ms)
  para anticipar más: en 15 s dio 906-1135 ms frente a 1073 (no estable) y en dos revisiones subió el WER en
  directo de 2 min (5,3 → 5,56 %). Sin cambios de velocidad en este ticket; la meta (≤ 1500 ms) ya se cumplía sin
  carga. Descartado bajar el mínimo de los segmentos (BELTR-PRT-10: empeora el WER).
- **Candado**: la zona del ratón en «ready» cubre todos los satélites con sus huecos, la salida espera 300 ms
  (antes 60) y, con el sondeo del cursor de `flowbar.rs` diciendo «dentro», un `mouseleave` del DOM no esconde
  la barra.

- **Regla 2, encontrado al pasar `npm test`** (`rompedor_dos_arranques_a_la_vez_sobre_base_nueva`, fallaba 1 de
  cada 5): dos arranques a la vez sobre una base nueva leían los dos la v0 fuera de transacción; el segundo
  dejaba `user_version` en 1 sobre una base ya en v10 y la base no volvía a abrir («duplicate column name»).
  Arreglo en `db.rs`: toda la subida de versión en una transacción `BEGIN IMMEDIATE` con la versión leída
  dentro, `ensure_v10_columns` con el candado y el paso a WAL reintentado ante «database is locked». 20/20.

## Resultado (22-nov-2026, PC sin carga, tras la revisión)

| # | Medida | Antes | Después | Test |
|---|---|---|---|---|
| H1 | Dictados con voz que salen sin texto (los 6 de la noche del 20-nov, Whisper medium) | 6 de 6 | 0 de 6 | `05-experiencia` + `whisper_no_deja_vacia_la_voz_real` |
| H1 | Ruido sin voz que sale con texto (9 ruidos + 1 silencio × 3 textos previos/idiomas) | — (el revisor sacó «www.gmail.com») | 0 de 30 (y la voz, 18 de 18 bien) | `whisper_no_deja_vacia_la_voz_real` |
| H2 | Casos de regla 5 con prueba | 0 | 11 pruebas, 0 dictados perdidos | `05-experiencia` |
| H3 | Correos bien escritos con la voz de Txonso (7 de la noche del 20-nov) | Whisper 1/7, Parakeet 2/7 | Whisper 5/7, Parakeet 2/7; con o sin «arroba/gmail/hotmail» en el diccionario, igual | `banco-formatos` (voz real); falta que Txonso grabe las 15 frases de `GUION.md` |
| H3 | % de palabras mal de Whisper medium (sintéticas / Txonso) | 7,1 / 11,2 % | 7,1 / 11,2 % | `banco-entender` (Whisper) |
| H4 | 15 s con pulido, soltar → pegado p50 | 1518 ms (con carga) | 1073-1135 ms sin carga (la diferencia era la carga, no el código); WER 7,2 % | `test:lento` (umbral 1500 ms) |
| H4 | 2 min con pulido, p50 / WER en directo | 1880 ms / 5,3 % | 1989 ms / 5,4 % (umbral 5,42) | `test:lento` |
| H4 | Regla 3 (15 s sin pulido, Parakeet) | 687 ms | 640-687 ms (límite 1068) | `03-velocidad` |
| H5 | Clics para el texto en vivo desde el hub | 3 | 1 | `interfaz-prt30` |
| H6 | Piezas cortadas/solapadas (hub 75 vistas, Flow bar 60) | 0 visibles (el test marcó 3 falsos: secciones cerradas y la barra flotante de Reuniones) | 0 | `interfaz-prt30` |
| H7 | Intentos que llegan al candado (24: 4 velocidades × 3 caminos × 2 escalas) | 11/24 (46 %) | 24/24 (100 %) | `interfaz-prt30` |

Además, arreglados por el camino para que `npm test` salga verde: la base que no volvía a abrir tras dos arranques a la vez (`db.rs`, regla 2) y dos pruebas inestables de otra ventana en `tests/flowbar-actualizacion.test.mjs` (leían antes de que acabara una transición o un guardado; ese fichero no entra en el commit de este ticket).

No probado con la app real: el pegado con teclas pulsadas y el cambio de ventana (necesitan teclado y ventanas de verdad); el controlador de la cola única solo se prueba por sus piezas (`dictar_con`, `esperar_grabacion`, `liberar`, `fin_a_la_vista`).

## Fuera de este encargo

- **Pitido o zumbido constante sin voz, con idioma «es» y palabras en el diccionario** → Whisper escribe un enlace
  que no se dijo («www.global.com», «www.beltranlab.com»; segunda revisión, 22-nov). Ya pasaba antes de este
  ticket (sin las palabras fijas sale igual o peor: es el eco del diccionario en el texto previo, y con «es» fijo
  Whisper nunca marca «no hay voz»). Necesita un detector de voz antes del motor (los tonos fijos no son voz):
  encargo aparte.

- Probarlo con el micrófono de verdad (las baterías no usan el micro; lo prueba Txonso).
- Pronto para Mac (otra sesión).
