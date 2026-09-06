import '../translation/language_detect.dart';

enum DictationStatus {
  /// Modelo no cargado / falta configuración.
  uninitialized,
  idle,
  recording,
  transcribing,

  /// El texto dictado está SALIENDO DEL EQUIPO hacia el proveedor de nube para
  /// formatearlo. Es un estado propio y no un "pensando" más a propósito: es el
  /// único momento en que algo de un dictado sale del ordenador, y la Píldora
  /// tiene que decirlo con todas las letras. Solo se entra aquí cuando la
  /// petición ya va a salir de verdad.
  sendingToCloud,

  /// Traduciendo antes de pegar (solo si dictaste en un idioma distinto al de
  /// salida). La traducción de cortesía va en segundo plano y NO usa este estado.
  translating,
  injecting,
  error,
}

/// Para qué se está grabando.
///
/// No es un detalle de la interfaz: decide si el texto se pega en la aplicación
/// con foco, si entra en el Historial y si el Vocabulario aprende de él. Un
/// Dictado de prueba que se colara como Dictado de verdad ensuciaría el Corpus
/// con el que se mide todo lo demás.
enum DictationPurpose {
  /// Lo normal: se pega, se apunta en el Historial y se aprende de él.
  dictado,

  /// La prueba del asistente de instalación. NO se pega en ninguna aplicación
  /// (el texto se enseña dentro del asistente), NO cuenta como Historial y NO
  /// alimenta el aprendizaje.
  prueba,
}

class DictationState {
  final DictationStatus status;

  /// Para qué se graba ahora mismo.
  final DictationPurpose purpose;

  /// El texto del último Dictado de prueba. Vive aparte de [lastText] a
  /// propósito: si compartieran hueco, «copiar la última transcripción» y las
  /// Correcciones acabarían trabajando sobre una prueba.
  final String? lastTestText;

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
    this.purpose = DictationPurpose.dictado,
    this.lastTestText,
    this.lastRaw,
    this.lastText,
    this.lastLang,
    this.lastAlt,
    this.lastAltLang,
    this.lastWasTranslated = false,
    this.level = 0.0,
    this.error,
  });

  /// Hay un dictado en marcha y no se puede empezar otro.
  ///
  /// [DictationStatus.sendingToCloud] cuenta: si no contara, pulsar el atajo
  /// con el texto AÚN EN VUELO arrancaría un dictado nuevo, la píldora naranja
  /// desaparecería mientras el texto sigue fuera del equipo —el aviso que el
  /// usuario necesita— y quedarían dos formateos volando contra el mismo tope
  /// de gasto.
  bool get isBusy =>
      status == DictationStatus.recording ||
      status == DictationStatus.transcribing ||
      status == DictationStatus.sendingToCloud ||
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
    DictationPurpose? purpose,
    String? lastTestText,
    bool clearTestText = false,
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
      purpose: purpose ?? this.purpose,
      lastTestText:
          clearTestText ? null : (lastTestText ?? this.lastTestText),
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
