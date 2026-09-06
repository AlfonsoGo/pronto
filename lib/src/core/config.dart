/// Constantes y valores por defecto de Pronto.
class AppConfig {
  AppConfig._();

  static const String appName = 'Pronto';

  /// Repositorio de GitHub donde se publican las releases. Lo usa el
  /// auto-actualizador in-app. REQUIERE que el repo/releases sean PÚBLICOS
  /// (la app consulta la API pública sin token).
  static const String githubOwner = 'AlfonsoGo';
  static const String githubRepo = 'pronto';

  // --- Whisper ---
  /// Idioma por defecto (español primero; multilingüe = cambiar este valor).
  static const String defaultLanguage = 'es';

  /// Modelo de arranque (equilibrio precisión/velocidad en CPU).
  /// Para máxima calidad: 'ggml-large-v3-turbo.bin'.
  static const String defaultModelFile = 'ggml-small.bin';

  /// Hilos por defecto para whisper.cpp (0 = auto = núcleos disponibles).
  static const int defaultThreads = 0;

  /// URL de descarga del modelo Whisper (bajo demanda, si el usuario elige el
  /// motor Whisper y no está empaquetado). Mirror oficial en Hugging Face.
  static const String whisperModelUrl =
      'https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin';

  // --- Parakeet (motor alternativo, sherpa-onnx) ---
  /// Subcarpeta dentro de `models/` donde vive el modelo Parakeet (encoder/
  /// decoder/joiner/tokens). Es el motor POR DEFECTO y va en el instalador.
  static const String parakeetModelDir = 'parakeet';

  /// Base de descarga del modelo Parakeet (sherpa-onnx NeMo Parakeet TDT 0.6b
  /// v3 int8) en Hugging Face. El splash descarga aquí los ficheros que falten
  /// la primera vez que se abre la app sin modelo empaquetado.
  static const String parakeetModelBaseUrl =
      'https://huggingface.co/csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/resolve/main';

  /// Ficheros que componen el modelo Parakeet (deben estar los cuatro).
  static const List<String> parakeetModelFiles = [
    'encoder.int8.onnx',
    'decoder.int8.onnx',
    'joiner.int8.onnx',
    'tokens.txt',
  ];

  /// SHA-256 esperado de cada fichero del modelo Parakeet. El descargador
  /// verifica el hash de cada `.part` recién bajado ANTES de renombrarlo al
  /// nombre final; si no coincide, borra el `.part` y falla (no se carga un
  /// modelo sin verificar). Calculados sobre los ficheros de `native/parakeet/`
  /// que van en el instalador (mismo mirror de Hugging Face que la descarga).
  static const Map<String, String> parakeetModelSha256 = {
    'encoder.int8.onnx':
        'acfc2b4456377e15d04f0243af540b7fe7c992f8d898d751cf134c3a55fd2247',
    'decoder.int8.onnx':
        '179e50c43d1a9de79c8a24149a2f9bac6eb5981823f2a2ed88d655b24248db4e',
    'joiner.int8.onnx':
        '3164c13fc2821009440d20fcb5fdc78bff28b4db2f8d0f0b329101719c0948b3',
    'tokens.txt':
        'd58544679ea4bc6ac563d1f545eb7d474bd6cfa467f0a6e2c1dc1c7d37e3c35d',
  };

  // --- Automejora ---
  /// Frecuencia mínima para que un par de corrección se active (anti-ruido).
  static const int minCorrectionFreq = 3;

  /// Distancia Levenshtein normalizada por encima de la cual NO se considera
  /// una corrección de la "misma palabra" sino una reescritura semántica.
  static const double maxCorrectionDistance = 0.6;

  /// Si la edición cambió más de esta fracción del texto, se asume reescritura
  /// completa y no se extraen pares de corrección.
  static const double maxRewriteRatio = 0.5;

  /// Límite aproximado del initial_prompt (~224 tokens ≈ ~800-900 chars).
  static const int initialPromptMaxChars = 820;

  /// Nº máximo de términos de vocabulario en el initial_prompt.
  static const int initialPromptMaxTerms = 45;

  // --- Biasing del motor (hotwords de sherpa-onnx) ---
  /// Nº máximo de términos que se vuelcan al fichero de hotwords.
  ///
  /// MUY por encima de [initialPromptMaxTerms]: el initial_prompt de Whisper
  /// competía por una ventana de ~224 tokens, pero los hotwords son una lista
  /// aparte y no gastan contexto. Un tope bajo aquí CIEGA el biasing en
  /// silencio: el vocabulario aprendido existe en la BD pero nunca llega al
  /// motor, que es justo la avería que este fichero viene a arreglar.
  static const int hotwordsMaxTerms = 500;

  /// Peso del biasing. El valor por defecto de sherpa-onnx es 1.5; subirlo
  /// fuerza más la coincidencia pero aumenta los falsos positivos.
  static const double hotwordsScore = 2.0;

  /// Longitud mínima de un término para entrar en los hotwords. Las palabras
  /// muy cortas disparan falsos positivos por todas partes.
  static const int hotwordsMinTermLength = 3;

  /// Fichero de hotwords que Pronto genera dentro de la carpeta del modelo.
  static const String hotwordsFile = 'hotwords.txt';

  /// Vocabulario BPE necesario para tokenizar los hotwords. Se genera desde el
  /// tokenizer del modelo original y viaja junto al modelo.
  static const String bpeVocabFile = 'bpe.vocab';

  // --- Formateo del dictado (opcional, off por defecto) ---
  //
  // HISTORIA, para que no se vuelva a montar la misma trampa:
  // hasta la 0.7.1 la post-corrección LLM estaba gateada por confianza
  // (`llmConfidenceGate = -0.35`, se saltaba si el motor iba seguro). Pero
  // [ParakeetEngine] — el ÚNICO motor desde la 0.7.0 — devuelve `avgLogProb: 0`
  // fijo, y `0 >= -0.35` siempre. Resultado: el formateo NUNCA se ejecutaba,
  // aunque el usuario lo encendiera. La puerta se ha quitado entera: mientras
  // ningún motor emita una confianza de verdad, un gate por confianza es un
  // control inerte, no una protección.

  /// Tope de espera de una llamada de formateo. Más corto que el de traducción
  /// porque el formateo está en el camino crítico del pegado: si tarda más, se
  /// pega el texto sin formatear en vez de dejar al usuario mirando la píldora.
  static const Duration formatTimeout = Duration(seconds: 12);

  // Los topes del guardia de deriva NO viven aquí: son propiedad del Preset
  // (ADR 0004), y están en `FormatPreset.limpio`. Se midieron sobre 443
  // dictados formateados al estilo «Limpio» y solo valen para ese trabajo:
  // «Nota» reestructura en viñetas a propósito y con el tope de Limpio se le
  // descartaría su salida buena. Mientras fueran globales, ese fallo estaba
  // garantizado en cuanto existiera el segundo Preset.

  /// Modelo local de formateo con el que se ARRANCA cuando no se sabe nada más.
  ///
  /// ⚠ Es un último recurso, no una propuesta: desde el PRNT-18 la lista de
  /// modelos sale de preguntarle a Ollama qué tiene (`/api/tags`, ver
  /// [OllamaCatalog]) y lo que Pronto propone es siempre algo instalado. Este
  /// valor solo sobrevive en unos ajustes que nunca se tocaron.
  ///
  /// Historia: hasta la 0.9 el nombre se escribía a mano y no se comprobaba
  /// nunca. `qwen3:4b`, el defecto de la 0.7.1, no existía en el registro
  /// público de Ollama: encender el Formateo daba 404 y en pantalla parecía
  /// que la función no servía.
  static const String defaultFormatLocalModel = 'qwen2.5:7b';

  /// Raíz del proveedor de nube (compatible OpenAI). OpenRouter a propósito:
  /// es el único cuyo gasto ve el vigilante que ya corre en el VPS2.
  static const String openRouterBaseUrl = 'https://openrouter.ai/api';

  /// Modelo de nube por defecto. El más barato de la lista blanca
  /// (`kCloudPrices`): un mes de dictado normal cuesta menos de un céntimo.
  static const String defaultFormatCloudModel = 'openai/gpt-5-nano';

  /// Tope de gasto de nube por DÍA, en dólares.
  ///
  /// Los números que lo justifican: con el volumen real de Txonso (~161
  /// dictados/mes ≈ 8/día) un día cuesta entre 0,0007 $ y 0,004 $ según el
  /// modelo. 0,10 $ deja unas 25 veces el uso normal y aun así corta un bucle
  /// enloquecido en el sitio. El tope de 2 $/día del vigilante del VPS son
  /// CINCUENTA MESES de uso normal: no puede detectar nada, y por eso el freno
  /// está aquí dentro y el vigilante es solo la red de último recurso.
  static const double defaultCloudDayCapUsd = 0.10;

  /// Tope de gasto de nube por MES, en dólares. Un mes normal son céntimos.
  static const double defaultCloudMonthCapUsd = 1.00;

  /// Tope POR DICTADO: por encima de esto no se manda a la nube (se pega sin
  /// formatear). Es el tope "por llamada": sin él, un solo dictado disparatado
  /// se comería el tope del día de una sentada.
  static const int cloudMaxChars = 4000;

  /// Lo más alto que se puede poner un tope de gasto, en dólares.
  ///
  /// Existe por un resbalón de dedo: teclear `100` donde se quería `1,00` fija
  /// cien dólares al día sin que nada chille. Cien veces el tope por defecto ya
  /// es de sobra para cualquier uso imaginable de dictado.
  static const double cloudMaxCapUsd = 10.0;

  // --- Historial (PRNT-16) ---
  /// Días que se guarda el Historial de fábrica.
  ///
  /// **No es infinito a propósito** (ADR 0009). En `pronto.db`, sin cifrar,
  /// está todo lo que el usuario ha dictado nunca: correos, nombres de
  /// clientes, lo que dijera en voz alta sin pensar. Tres meses son de sobra
  /// para el uso real del historial —buscar lo de la semana pasada, sacar el
  /// corpus, ver las estadísticas del mes— y quien quiera guardarlo todo lo
  /// pone a 0 a mano, que es una decisión, no un olvido.
  static const int defaultHistoryRetentionDays = 90;

  /// Tope de la retención, en días (unos diez años). Por encima de esto el
  /// número es un resbalón de dedo, no una intención.
  static const int maxHistoryRetentionDays = 3650;

  // --- Traducción al dictar (español ⇄ inglés, opcional) ---
  /// Modelo por defecto para traducir. Se usa contra el mismo servidor LLM que
  /// la post-corrección ([AppSettings.llmBaseUrl]); Ollama local por defecto.
  static const String defaultTranslationModel = 'qwen2.5:7b';

  /// Idioma en el que se pega el dictado por defecto (el "idioma por defecto"
  /// que alterna el atajo de idioma).
  static const String defaultOutputLanguage = 'es';

  /// Tope de espera de una traducción. Más generoso que la post-corrección
  /// porque un modelo local traduciendo un párrafo largo tarda más.
  static const Duration translationTimeout = Duration(seconds: 45);

  /// Tope de espera del precalentado del modelo. Se dispara al empezar a grabar
  /// y solo sirve para adelantar la carga: si tarda más, se abandona sin ruido.
  static const Duration translationWarmUpTimeout = Duration(seconds: 90);

  /// Longitud máxima de un texto a traducir. Por encima no se traduce (se
  /// pegaría el original) en vez de dejar la píldora colgada minutos.
  static const int translationMaxChars = 4000;
}
