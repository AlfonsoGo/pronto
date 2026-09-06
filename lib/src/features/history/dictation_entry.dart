/// El vocabulario del Historial: un dictado ya ocurrido y lo que se sabe de él.
///
/// Es la lectura de `dictation_log`. Lo que se ESCRIBE al dictar es
/// [DictationRecord] (ver `history_repository.dart`): son dos formas distintas
/// a propósito, porque al guardar todavía no hay `id` ni fecha de la base.
library;

/// De dónde salió la fila. El Historial enseña solo [dictado].
enum DictationSource {
  /// Lo que el usuario dictó de verdad, para pegarlo en algún sitio.
  dictado,

  /// La prueba de micrófono del asistente de instalación (PRNT-13). Se guarda
  /// —sirve para diagnosticar una instalación que no arranca— pero no es
  /// historial de nadie: no aparece en la lista ni cuenta en las estadísticas.
  prueba;

  static DictationSource fromDb(Object? value) =>
      value == 'prueba' ? DictationSource.prueba : DictationSource.dictado;

  String get dbValue => name;
}

/// Un dictado del historial.
class DictationEntry {
  final int id;

  /// Cuándo se dictó.
  final DateTime at;

  /// Lo que oyó el motor, sin tocar.
  final String raw;

  /// Lo que se acabó pegando. Vacío cuando se pegó el crudo tal cual.
  final String finalText;

  /// El Modo con el que se dictó (PRNT-3). `null` en todo lo dictado antes de
  /// que los Modos existieran: es «no se sabe», no «Por defecto».
  final String? mode;

  /// Motor de voz (`parakeet`, `whisper`).
  final String? engine;

  /// Camino de formateo: `none`, `local` o `cloud`.
  final String? formatPath;

  /// Modelo con el que se formateó.
  final String? formatModel;

  /// Por qué el formateo no hizo lo que se esperaba. Es SIEMPRE una cadena
  /// nuestra: la respuesta cruda del proveedor no habla con la voz del sistema.
  final String? formatNotice;

  /// Aplicación en la que aterrizó el texto y título de su ventana.
  final String? appName;
  final String? windowTitle;

  /// Cuánto se habló, cuánto tardó el motor y cuánto el formateo.
  final Duration? spoken;
  final Duration? transcribe;
  final Duration? format;

  /// Lo que costó la llamada de nube, en dólares (OpenRouter factura en
  /// dólares; convertirlo a euros aquí sería inventarse un tipo de cambio).
  final double? costUsd;

  /// Lo que se pegó era una traducción de lo dictado. Cuando es cierto, el
  /// crudo y el final están en idiomas distintos: su diferencia NO es una
  /// corrección de la que se pueda aprender.
  final bool translated;

  final DictationSource source;

  const DictationEntry({
    required this.id,
    required this.at,
    required this.raw,
    required this.finalText,
    this.mode,
    this.engine,
    this.formatPath,
    this.formatModel,
    this.formatNotice,
    this.appName,
    this.windowTitle,
    this.spoken,
    this.transcribe,
    this.format,
    this.costUsd,
    this.translated = false,
    this.source = DictationSource.dictado,
  });

  /// Copia con algún campo cambiado. Existe para que añadir una columna no
  /// obligue a reescribir a mano una lista de quince campos en cada sitio que
  /// rehace una entrada.
  DictationEntry copyWith({String? finalText}) => DictationEntry(
        id: id,
        at: at,
        raw: raw,
        finalText: finalText ?? this.finalText,
        mode: mode,
        engine: engine,
        formatPath: formatPath,
        formatModel: formatModel,
        formatNotice: formatNotice,
        appName: appName,
        windowTitle: windowTitle,
        spoken: spoken,
        transcribe: transcribe,
        format: format,
        costUsd: costUsd,
        translated: translated,
        source: source,
      );

  /// El texto que de verdad se pegó: el final si lo hubo, si no el crudo.
  String get pastedText => finalText.isEmpty ? raw : finalText;

  /// ¿Hubo algo que contarle al usuario sobre el formateo de este dictado?
  bool get failed => (formatNotice ?? '').isNotEmpty;

  /// Palabras del texto pegado.
  int get wordCount => countWords(pastedText);
}

/// Palabras de un texto: bloques separados por espacios en blanco.
///
/// Es la definición que usa TODA la pantalla —la cifra de la entrada, el total
/// de las estadísticas y el tiempo ahorrado— para que dos números que salen del
/// mismo texto no se contradigan.
int countWords(String text) {
  final t = text.trim();
  if (t.isEmpty) return 0;
  return t.split(RegExp(r'\s+')).length;
}
