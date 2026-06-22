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

  // --- Post-corrección LLM (opcional, off por defecto) ---
  /// Por debajo de esta confianza (avgLogProb) se permite la post-corrección.
  /// Por encima (texto muy seguro) se salta para evitar parafraseo.
  static const double llmConfidenceGate = -0.35;

  /// Si el LLM cambió más de esta fracción de palabras-contenido, se descarta
  /// su salida (protección contra alucinación).
  static const double llmMaxContentDrift = 0.15;
}
