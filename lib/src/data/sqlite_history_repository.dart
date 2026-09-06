import 'package:sqlite3/sqlite3.dart';

import '../features/history/dictation_entry.dart';
import '../features/history/history_repository.dart';
import 'sql_functions.dart';

/// El historial sobre `dictation_log`.
///
/// No abre la base: recibe una función que la da ya abierta, migrada y con
/// [registerProntoFunctions] puesto. La apertura de `pronto.db` sigue estando
/// en un solo sitio ([SqliteLearningRepository]), que es lo que evita dos
/// migraciones compitiendo por el mismo fichero.
class SqliteHistoryRepository implements HistoryRepository {
  SqliteHistoryRepository(this._open);

  final Future<Database> Function() _open;

  /// Todas las columnas que lee la pantalla, en un sitio.
  static const String _columns = '''
    id, ts, raw, final, mode, engine, format_path, format_model,
    format_notice, translated, app_name, window_title, spoken_ms,
    transcribe_ms, format_ms, cost_usd, source
  ''';

  @override
  Future<int> log(DictationRecord r) async {
    final db = await _open();
    final stmt = db.prepare('''
      INSERT INTO dictation_log (
        raw, final, avg_logprob, ts, source, mode, engine, format_path,
        format_model, format_notice, translated, app_name, window_title,
        spoken_ms, transcribe_ms, format_ms, cost_usd
      ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    ''');
    try {
      stmt.execute([
        r.raw,
        r.finalText,
        r.avgLogProb,
        r.at.millisecondsSinceEpoch,
        r.source.dbValue,
        r.mode,
        r.engine,
        r.formatPath,
        r.formatModel,
        r.formatNotice,
        r.translated ? 1 : 0,
        r.appName,
        r.windowTitle,
        r.spoken?.inMilliseconds,
        r.transcribe?.inMilliseconds,
        r.format?.inMilliseconds,
        r.costUsd,
      ]);
      return db.lastInsertRowId;
    } finally {
      stmt.dispose();
    }
  }

  @override
  Future<List<DictationEntry>> page(
    HistoryQuery query, {
    int limit = 50,
    int offset = 0,
  }) async {
    final db = await _open();
    final w = _where(query);
    final stmt = db.prepare(
      'SELECT $_columns FROM dictation_log ${w.sql} '
      'ORDER BY ts DESC, id DESC LIMIT ? OFFSET ?',
    );
    try {
      return stmt
          .select([...w.args, limit, offset])
          .map(_toEntry)
          .toList(growable: false);
    } finally {
      stmt.dispose();
    }
  }

  @override
  Future<List<DictationEntry>> all(HistoryQuery query) async {
    final db = await _open();
    final w = _where(query);
    final stmt = db.prepare(
      'SELECT $_columns FROM dictation_log ${w.sql} ORDER BY ts DESC, id DESC',
    );
    try {
      return stmt.select(w.args).map(_toEntry).toList(growable: false);
    } finally {
      stmt.dispose();
    }
  }

  @override
  Future<int> count(HistoryQuery query) async {
    final db = await _open();
    final w = _where(query);
    final stmt =
        db.prepare('SELECT count(*) AS n FROM dictation_log ${w.sql}');
    try {
      return stmt.select(w.args).first['n'] as int;
    } finally {
      stmt.dispose();
    }
  }

  @override
  Future<HistoryStats> stats(HistoryQuery query) async {
    final db = await _open();
    final w = _where(query);
    // Las palabras se cuentan en Dart, con la MISMA función que usa la ficha de
    // cada entrada: dos definiciones de «palabra» darían dos totales distintos
    // para el mismo texto. Solo se traen las columnas que suman.
    final stmt = db.prepare(
      'SELECT ts, raw, final, mode, app_name, spoken_ms, transcribe_ms, '
      'format_ms, cost_usd FROM dictation_log ${w.sql}',
    );
    try {
      final rows = stmt.select(w.args);
      var words = 0;
      var spokenMs = 0;
      var processingMs = 0;
      var cost = 0.0;
      final perDay = <String, int>{};
      final perMode = <String, int>{};
      final perApp = <String, int>{};

      for (final row in rows) {
        final finalText = (row['final'] as String?) ?? '';
        final raw = (row['raw'] as String?) ?? '';
        words += countWords(finalText.isEmpty ? raw : finalText);
        spokenMs += (row['spoken_ms'] as int?) ?? 0;
        processingMs += ((row['transcribe_ms'] as int?) ?? 0) +
            ((row['format_ms'] as int?) ?? 0);
        cost += (row['cost_usd'] as num?)?.toDouble() ?? 0.0;

        final day = _dayKey(row['ts'] as int?);
        perDay[day] = (perDay[day] ?? 0) + 1;
        final mode = row['mode'] as String?;
        if (mode != null && mode.isNotEmpty) {
          perMode[mode] = (perMode[mode] ?? 0) + 1;
        }
        final app = row['app_name'] as String?;
        if (app != null && app.isNotEmpty) {
          perApp[app] = (perApp[app] ?? 0) + 1;
        }
      }

      return HistoryStats(
        total: rows.length,
        words: words,
        spoken: Duration(milliseconds: spokenMs),
        processing: Duration(milliseconds: processingMs),
        costUsd: cost,
        perDay: perDay,
        perMode: perMode,
        perApp: perApp,
      );
    } finally {
      stmt.dispose();
    }
  }

  @override
  Future<List<String>> distinctModes() async {
    final db = await _open();
    return db
        .select(
          "SELECT DISTINCT mode FROM dictation_log "
          "WHERE mode IS NOT NULL AND mode <> '' AND source = 'dictado' "
          'ORDER BY mode',
        )
        .map((r) => r['mode'] as String)
        .toList(growable: false);
  }

  @override
  Future<void> deleteEntry(int id) async {
    final db = await _open();
    final stmt = db.prepare('DELETE FROM dictation_log WHERE id = ?');
    try {
      stmt.execute([id]);
    } finally {
      stmt.dispose();
    }
  }

  @override
  Future<int> deleteAll() async {
    final db = await _open();
    db.execute('DELETE FROM dictation_log');
    return db.updatedRows;
  }

  @override
  Future<int> purgeOlderThan(Duration age, {DateTime? now}) async {
    // Cero días borraría el historial entero al arrancar. La retención
    // «para siempre» se dice con un cero en los ajustes, así que aquí ese cero
    // tiene que significar exactamente lo contrario de borrar.
    if (age <= Duration.zero) return 0;
    final db = await _open();
    final limite = (now ?? DateTime.now()).subtract(age);
    final stmt = db.prepare('DELETE FROM dictation_log WHERE ts < ?');
    try {
      stmt.execute([limite.millisecondsSinceEpoch]);
      return db.updatedRows;
    } finally {
      stmt.dispose();
    }
  }

  @override
  Future<void> updateFinalText(int id, String text) async {
    final db = await _open();
    final stmt =
        db.prepare('UPDATE dictation_log SET final = ? WHERE id = ?');
    try {
      stmt.execute([text, id]);
    } finally {
      stmt.dispose();
    }
  }

  /// El `WHERE` del filtro, con sus parámetros enlazados (nunca interpolados).
  _Where _where(HistoryQuery q) {
    // La prueba del asistente (PRNT-13) se guarda pero no es historial: se
    // descarta aquí, en el único sitio por el que pasan todas las consultas.
    final clauses = <String>["source = 'dictado'"];
    final args = <Object?>[];

    final text = q.text.trim();
    if (text.isNotEmpty) {
      final patron = '%${_escapeLike(foldForSearch(text))}%';
      clauses.add(
        '(pronto_fold(raw) LIKE ? ESCAPE \'\\\' '
        "OR pronto_fold(coalesce(final, '')) LIKE ? ESCAPE '\\' "
        "OR pronto_fold(coalesce(app_name, '')) LIKE ? ESCAPE '\\')",
      );
      args.addAll([patron, patron, patron]);
    }
    if (q.from != null) {
      clauses.add('ts >= ?');
      args.add(q.from!.millisecondsSinceEpoch);
    }
    if (q.to != null) {
      clauses.add('ts <= ?');
      args.add(q.to!.millisecondsSinceEpoch);
    }
    if (q.mode != null) {
      clauses.add('mode = ?');
      args.add(q.mode);
    }
    if (q.onlyFailed) {
      clauses.add("format_notice IS NOT NULL AND format_notice <> ''");
    }

    return _Where('WHERE ${clauses.join(' AND ')}', args);
  }

  /// `%` y `_` escritos por el usuario son texto, no comodines.
  String _escapeLike(String value) => value
      .replaceAll('\\', '\\\\')
      .replaceAll('%', '\\%')
      .replaceAll('_', '\\_');

  static String _dayKey(int? ts) {
    if (ts == null) return '';
    final d = DateTime.fromMillisecondsSinceEpoch(ts);
    final m = d.month.toString().padLeft(2, '0');
    final day = d.day.toString().padLeft(2, '0');
    return '${d.year}-$m-$day';
  }

  static DictationEntry _toEntry(Row row) => DictationEntry(
        id: row['id'] as int,
        at: DateTime.fromMillisecondsSinceEpoch((row['ts'] as int?) ?? 0),
        raw: (row['raw'] as String?) ?? '',
        finalText: (row['final'] as String?) ?? '',
        mode: row['mode'] as String?,
        engine: row['engine'] as String?,
        formatPath: row['format_path'] as String?,
        formatModel: row['format_model'] as String?,
        formatNotice: row['format_notice'] as String?,
        appName: row['app_name'] as String?,
        windowTitle: row['window_title'] as String?,
        spoken: _ms(row['spoken_ms']),
        transcribe: _ms(row['transcribe_ms']),
        format: _ms(row['format_ms']),
        costUsd: (row['cost_usd'] as num?)?.toDouble(),
        translated: (row['translated'] as int?) == 1,
        source: DictationSource.fromDb(row['source']),
      );

  static Duration? _ms(Object? value) =>
      value is int ? Duration(milliseconds: value) : null;
}

class _Where {
  final String sql;
  final List<Object?> args;

  const _Where(this.sql, this.args);
}
