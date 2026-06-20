import 'dart:typed_data';

/// Resultado de una transcripción de Whisper.
class TranscriptResult {
  /// Texto transcrito (sin post-procesar).
  final String text;

  /// Logprob medio de los tokens (proxy de confianza).
  /// Más cercano a 0 = más confianza. Típicamente entre -1.0 y -0.1.
  /// Se usa para "gatear" la post-corrección con LLM.
  final double avgLogProb;

  /// Idioma detectado/usado (ISO 639-1, p.ej. "es").
  final String language;

  /// Duración del audio procesado, en milisegundos.
  final int audioMs;

  const TranscriptResult({
    required this.text,
    required this.avgLogProb,
    required this.language,
    this.audioMs = 0,
  });

  static const TranscriptResult empty =
      TranscriptResult(text: '', avgLogProb: 0, language: 'es');

  bool get isEmpty => text.trim().isEmpty;
}

/// Motor de reconocimiento de voz on-device.
///
/// Implementación principal: [WhisperEngineFfi] (whisper.cpp vía dart:ffi).
/// Está detrás de esta interfaz para poder cambiar de backend (p.ej. ONNX
/// en móvil) sin tocar el resto de la app.
abstract class WhisperEngine {
  /// Carga el modelo GGML desde [modelPath]. Pesado: cárgalo una vez y
  /// mantenlo vivo durante la sesión.
  Future<void> load(String modelPath);

  bool get isLoaded;

  /// Transcribe PCM mono 16 kHz float32 normalizado a [-1, 1].
  ///
  /// [initialPrompt] sesga el vocabulario (nombres propios, jerga) — máx
  /// ~224 tokens. [language] por defecto español.
  Future<TranscriptResult> transcribe(
    Float32List pcm16kMonoF32, {
    String language = 'es',
    String? initialPrompt,
    int? threads,
  });

  Future<void> dispose();
}
