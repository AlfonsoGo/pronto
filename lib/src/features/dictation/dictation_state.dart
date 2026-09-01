import '../translation/language_detect.dart';

enum DictationStatus {
  /// Modelo no cargado / falta configuración.
  uninitialized,
  idle,
  recording,
  transcribing,

  /// Traduciendo antes de pegar (solo si dictaste en un idioma distinto al de
  /// salida). La traducción de cortesía va en segundo plano y NO usa este estado.
  translating,
  injecting,
  error,
}

class DictationState {
  final DictationStatus status;

  /// Última transcripción CRUDA (lo que dijo Whisper, antes de corregir).
  final String? lastRaw;

  /// Último texto FINAL insertado (tras diccionario + LLM + traducción).
  final String? lastText;

  /// Idioma de [lastText].
  final TextLanguage? lastLang;

  /// La OTRA versión del último dictado: la traducción si se pegó el original,
  /// o el original si se pegó la traducción. `null` mientras no la haya.
  final String? lastAlt;

  /// Idioma de [lastAlt].
  final TextLanguage? lastAltLang;

  /// `true` si [lastText] es una TRADUCCIÓN y no lo que se dictó. Importa
  /// porque el aprendizaje solo puede comparar texto en el idioma dictado.
  final bool lastWasTranslated;

  /// Nivel de audio actual (0.0 - 1.0) para el medidor.
  final double level;

  /// Mensaje de error legible, si [status] == error.
  final String? error;

  const DictationState({
    this.status = DictationStatus.uninitialized,
    this.lastRaw,
    this.lastText,
    this.lastLang,
    this.lastAlt,
    this.lastAltLang,
    this.lastWasTranslated = false,
    this.level = 0.0,
    this.error,
  });

  bool get isBusy =>
      status == DictationStatus.recording ||
      status == DictationStatus.transcribing ||
      status == DictationStatus.translating ||
      status == DictationStatus.injecting;

  /// Nota: [error] NO usa el patrón `error ?? this.error`. Es intencional:
  /// cualquier transición que no pase un error lo LIMPIA (los errores son
  /// transitorios y deben desaparecer al iniciar una acción nueva, p. ej. al
  /// empezar a grabar de nuevo). No lo "arregles" a `error ?? this.error`.
  ///
  /// [clearAlt] borra la otra versión (y su idioma): hace falta al empezar un
  /// dictado nuevo que aún no tiene traducción, porque con `??` no se puede
  /// pasar de un valor a null.
  DictationState copyWith({
    DictationStatus? status,
    String? lastRaw,
    String? lastText,
    TextLanguage? lastLang,
    String? lastAlt,
    TextLanguage? lastAltLang,
    bool? lastWasTranslated,
    bool clearAlt = false,
    double? level,
    String? error,
  }) {
    return DictationState(
      status: status ?? this.status,
      lastRaw: lastRaw ?? this.lastRaw,
      lastText: lastText ?? this.lastText,
      lastLang: lastLang ?? this.lastLang,
      lastAlt: clearAlt ? null : (lastAlt ?? this.lastAlt),
      lastAltLang: clearAlt ? null : (lastAltLang ?? this.lastAltLang),
      lastWasTranslated: lastWasTranslated ?? this.lastWasTranslated,
      level: level ?? this.level,
      error: error,
    );
  }
}
