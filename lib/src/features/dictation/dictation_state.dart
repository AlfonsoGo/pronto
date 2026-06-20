enum DictationStatus {
  /// Modelo no cargado / falta configuración.
  uninitialized,
  idle,
  recording,
  transcribing,
  injecting,
  error,
}

class DictationState {
  final DictationStatus status;

  /// Última transcripción CRUDA (lo que dijo Whisper, antes de corregir).
  final String? lastRaw;

  /// Último texto FINAL insertado (tras diccionario + LLM).
  final String? lastText;

  /// Nivel de audio actual (0.0 - 1.0) para el medidor.
  final double level;

  /// Mensaje de error legible, si [status] == error.
  final String? error;

  const DictationState({
    this.status = DictationStatus.uninitialized,
    this.lastRaw,
    this.lastText,
    this.level = 0.0,
    this.error,
  });

  bool get isBusy =>
      status == DictationStatus.recording ||
      status == DictationStatus.transcribing ||
      status == DictationStatus.injecting;

  /// Nota: [error] NO usa el patrón `error ?? this.error`. Es intencional:
  /// cualquier transición que no pase un error lo LIMPIA (los errores son
  /// transitorios y deben desaparecer al iniciar una acción nueva, p. ej. al
  /// empezar a grabar de nuevo). No lo "arregles" a `error ?? this.error`.
  DictationState copyWith({
    DictationStatus? status,
    String? lastRaw,
    String? lastText,
    double? level,
    String? error,
  }) {
    return DictationState(
      status: status ?? this.status,
      lastRaw: lastRaw ?? this.lastRaw,
      lastText: lastText ?? this.lastText,
      level: level ?? this.level,
      error: error,
    );
  }
}
