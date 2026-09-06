import 'dictation_entry.dart';

/// Lo que se guarda al terminar un dictado.
///
/// Va aparte de [DictationEntry] porque al escribir todavía no hay `id`, y
/// porque quien dicta no tiene por qué saber cómo se lee el historial.
class DictationRecord {
  final String raw;
  final String finalText;
  final DateTime at;
  final DictationSource source;
  final String? mode;
  final String? engine;
  final String? formatPath;
  final String? formatModel;
  final String? formatNotice;
  final String? appName;
  final String? windowTitle;
  final Duration? spoken;
  final Duration? transcribe;
  final Duration? format;
  final double? costUsd;

  /// Lo pegado es una traducción de lo dictado (ver [DictationEntry.translated]).
  final bool translated;

  /// Confianza media del motor, tal cual la devuelve el motor.
  ///
  /// ⚠ Hoy **siempre vale 0**: Parakeet —el único motor desde la 0.7.0— no la
  /// calcula (ver `AppConfig`), y nadie la lee. La columna sigue en el esquema
  /// porque está escrita en las bases de los usuarios desde la 0.7 y quitarla
  /// costaría una migración destructiva para no ganar nada.
  final double avgLogProb;

  const DictationRecord({
    required this.raw,
    required this.finalText,
    required this.at,
    this.source = DictationSource.dictado,
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
    this.avgLogProb = 0.0,
  });
}

/// Qué trozo del historial se quiere ver.
///
/// Los filtros vacíos son «no filtres por esto», nunca «filtra por vacío».
class HistoryQuery {
  /// Texto a buscar. Se compara sin tildes y sin distinguir mayúsculas, contra
  /// el crudo, el final y el nombre de la app.
  final String text;

  /// Rango de fechas, ambos extremos incluidos.
  final DateTime? from;
  final DateTime? to;

  /// Modo exacto.
  final String? mode;

  /// Solo los dictados en los que el formateo dejó un aviso.
  final bool onlyFailed;

  const HistoryQuery({
    this.text = '',
    this.from,
    this.to,
    this.mode,
    this.onlyFailed = false,
  });

  bool get isEmpty =>
      text.trim().isEmpty && from == null && to == null && mode == null &&
      !onlyFailed;

  HistoryQuery copyWith({
    String? text,
    DateTime? from,
    DateTime? to,
    String? mode,
    bool? onlyFailed,
    bool clearFrom = false,
    bool clearTo = false,
    bool clearMode = false,
  }) =>
      HistoryQuery(
        text: text ?? this.text,
        from: clearFrom ? null : (from ?? this.from),
        to: clearTo ? null : (to ?? this.to),
        mode: clearMode ? null : (mode ?? this.mode),
        onlyFailed: onlyFailed ?? this.onlyFailed,
      );
}

/// Los números del historial, ya agregados.
///
/// Se calculan sobre el MISMO filtro que la lista: si estás mirando septiembre,
/// las estadísticas son de septiembre. Un total que no responde al filtro de
/// arriba se lee como el total de todo y engaña.
class HistoryStats {
  final int total;
  final int words;

  /// Cuánto se habló en total (suma de lo medido; los dictados que no lo
  /// tengan apuntado no suman).
  final Duration spoken;

  /// Cuánto tardó la máquina: motor más formateo.
  final Duration processing;

  /// Gasto de nube acumulado, en dólares.
  final double costUsd;

  /// Dictados por día (`yyyy-MM-dd`), por Modo y por aplicación.
  final Map<String, int> perDay;
  final Map<String, int> perMode;
  final Map<String, int> perApp;

  const HistoryStats({
    required this.total,
    required this.words,
    required this.spoken,
    required this.processing,
    required this.costUsd,
    required this.perDay,
    required this.perMode,
    required this.perApp,
  });

  static const empty = HistoryStats(
    total: 0,
    words: 0,
    spoken: Duration.zero,
    processing: Duration.zero,
    costUsd: 0,
    perDay: {},
    perMode: {},
    perApp: {},
  );
}

/// Persistencia del historial de dictados.
abstract class HistoryRepository {
  /// Guarda un dictado recién terminado y devuelve su `id`, para que una
  /// corrección posterior arregle esa misma fila.
  Future<int> log(DictationRecord record);

  /// Una página de la lista, de la más nueva a la más vieja.
  Future<List<DictationEntry>> page(
    HistoryQuery query, {
    int limit = 50,
    int offset = 0,
  });

  /// Cuántas entradas cumplen el filtro (para «N de M»).
  Future<int> count(HistoryQuery query);

  /// Los números del filtro actual.
  Future<HistoryStats> stats(HistoryQuery query);

  /// Los Modos que aparecen de verdad en el historial, ordenados. Vacío hasta
  /// que los Modos existan (PRNT-3): el desplegable no se enseña si no hay
  /// nada que elegir.
  Future<List<String>> distinctModes();

  /// Borra una entrada.
  Future<void> deleteEntry(int id);

  /// Borra el historial entero (incluidas las pruebas). Devuelve cuántas filas
  /// cayeron.
  Future<int> deleteAll();

  /// Borra lo más viejo que [age]. Devuelve cuántas filas cayeron.
  ///
  /// [age] cero significa NO borrar nada: la retención «infinita» tiene que
  /// costar un gesto explícito, no salir de un cero mal leído.
  Future<int> purgeOlderThan(Duration age, {DateTime? now});

  /// Cambia el texto final de una entrada (corrección a mano).
  Future<void> updateFinalText(int id, String text);

  /// Todas las entradas que cumplen el filtro, para exportar.
  Future<List<DictationEntry>> all(HistoryQuery query);
}
