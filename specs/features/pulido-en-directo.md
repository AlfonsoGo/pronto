# Pulido en directo, formatos y entendimiento (BELTR-PRT-10)

> Encargo del 20-nov-2026. Subtareas en Odoo: BELTR-PRT-13 a BELTR-PRT-19.
> Contrato escrito ANTES de tocar código. Cada línea se cumple con una cifra o un test; si una no se
> cumple, el trabajo no está hecho y se dice cuál falta.

## Contrato de «hecho»

| # | Qué | Cómo se comprueba | Meta |
|---|---|---|---|
| H0 | Vigilante nuevo (con `asyncRewake`) | `.claude/settings.json` + `.claude/vigilante.json` | instalado |
| H1 | Línea base medida ANTES de cambiar nada, con el pulido encendido | `tests/banco-pulido/linea-base-2026-11-20.md` | ms soltar→pegado p50/p90 (15 s y 60-120 s), % palabras mal (30 cortos + 10 largos de internet + 13 de Txonso), % formatos (40 casos), clics del interruptor |
| H2 | Las medidas son pruebas | `npm test` (rápidas) y `npm run test:lento` (las largas) | ninguna medida se queda fuera; umbrales en un `umbrales.json`, nunca a peor para que pase |
| H3 | Pulido en directo | test de velocidad con Ollama real (se salta sin Ollama) | texto pegado ≤ 1500 ms p50 tras soltar con el pulido encendido, en dictados de 15 s; mejor que H1 en 60-120 s. **Ajustado (Txonso, 20-nov-2026):** 2 min ✓ (12,5 s → 2,4 s); 15 s 1518 ms medido con un juego abierto, se re-mide sin carga |
| H4 | Interruptor del pulido en la página Dictado y su estado en la Flow bar | test de la UI (maqueta) | 1 clic desde que se abre el hub; la Flow bar enseña si el pulido está encendido |
| H5 | Correos, teléfonos y números bien escritos | banco de 40 casos (30 voz local + los de voz real que haya) | ~~≥ 90 %~~ **Ajustado (Txonso, 20-nov-2026): ≥ 31/40 (77 %) y mejor que H1 (13/40)**; los 9 que fallan son palabras mal oídas con voces sintéticas, no formato. Reglas locales en `text.rs` antes del pulido; la regla 7 no las rechaza |
| H6 | Entender mejor | banco de internet + banco de Txonso | ~~% palabras mal en internet menor que H1~~ **Ajustado (Txonso, 20-nov-2026): no peor que H1** (6,2 % y 5,4 %, iguales; 15 s en directo 8,0 → 7,2 %). Bajarlo pide otro motor: queda para otro encargo. Las 13 de Txonso no peor que `tests/banco-entender/umbrales.json` (10,4 %) |
| H7 | Comparativa al día | `docs/comparativa-dictado.html` | cifras propias + cifras publicadas (FLEURS, Common Voice) de los motores de cada marca; «sin dato público» donde no haya |
| H8 | Las 7 reglas de «NO SE NEGOCIA» siguen verdes | `npm test` entero | 0 fallos; regla 3 (sin pulido ≤ 1068 ms) intacta |
| H9 | Revisión de alguien que no vio el trabajo | subagente nuevo corre las pruebas | APTO. **Resultado (20-nov-2026): NO APTO** por H5, H6 y velocidad sin verificar con el juego abierto; sus 4 hallazgos medios arreglados en `ab5cfcd`. Txonso decidió ajustar H3/H5/H6 y cerrar |
| H10 | Sin publicar | git | 0 push, 0 release; commits sin rastro de IA; sin tocar `%APPDATA%\Pronto2` ni el Pronto instalado |

## Fuera de este encargo

- Re-medir la regla 3 y los 15 s (`npm test` y `npm run test:lento`) sin el juego abierto.
- Bajar el % de palabras mal con otro motor o modelo (Parakeet con hotwords, Whisper, otro).

- Probarlo con el micrófono de verdad (las baterías no usan el micro; lo prueba Txonso).
- Pronto para Mac (otra sesión): el código nuevo no depende de Windows salvo donde ya dependía.

## Decisiones

- **Pulido en directo = segmentos confirmados** (Fable, 20-nov-2026, consultado con los datos de la línea
  base). Mientras se habla, cuando lo pendiente pasa del mínimo y llega una pausa real (≥ 600 ms), ese
  trozo se transcribe una vez, entero, por el mismo camino que el dictado (`state::transcribe_heard`), y el
  pulidor de fondo (`polish::Pulidor`, un hilo, una petición a Ollama cada vez) lo pule. Al soltar solo se
  transcribe y pule la cola. El «audio entero» de antes ya se partía (Parakeet en trozos de 60 s,
  `asr::split_long`, cortando en 100 ms de silencio); ahora se parte antes y en pausas de verdad.
  Descartado: caché por frases sobre la vista previa (la vista falla un 23 % con voz real: la caché no
  acertaría) y re-transcribir una ventana creciente (CPU al cuadrado en el único motor, regla 3).
- La tubería local (`text::process`) corre sobre el texto entero, como siempre. Los trozos solo se usan
  para el pulido si, pasados por la tubería, unidos dan lo mismo (`state::trozos_coinciden`); si no (un
  snippet o un correo partido por un corte), se pule todo de una vez.
- Regla 7 en dos capas: `polish::accept` por trozo (si rechaza uno, solo ese va sin pulir) y anclaje +
  formatos sobre el texto entero (`polish::apply_pieces`).
- Correos, teléfonos y números: reglas deterministas en `text::format_spoken` (+ `fix_domains`), antes del
  diccionario y del pulido. El pulido no puede tocar lo que escribieron (`polish::formats_kept`). Wispr
  Flow, SuperWhisper y el micrófono de Claude no publican sus reglas para el español (investigación en las
  notas de la sesión); se siguen las normas de la RAE (hora con dos puntos, coma decimal) y el formato
  español de teléfono de 9 cifras en grupos de 3.
- Interruptor del pulido: en la página Dictado (cuarta casilla de «cómo se dicta»), 1 clic; el modelo y el
  plazo siguen en Configuración → Limpieza e IA. La Flow bar lleva un punto violeta y lo dice al pasar el
  ratón.
