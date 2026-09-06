import 'dart:async';

import 'package:sqlite3/sqlite3.dart';

import '../features/learning/learning_repository.dart';
import 'pronto_database.dart';

/// Implementación con SQLite del [LearningRepository].
///
/// Usa `package:sqlite3` (con `sqlite3_flutter_libs` para bundlear la librería
/// nativa en Windows). La base de datos `pronto.db` se guarda en el directorio
/// de soporte de la aplicación.
///
/// La apertura es PEREZOSA: la BD no se abre hasta la primera operación, y la
/// instancia se cachea para reutilizarla. Todas las consultas usan parámetros
/// enlazados (prepared statements), nunca interpolación de strings.
///
/// Abrir es MÁS que abrir un fichero: [abrirBaseDeDatosDePronto] copia la
/// carpeta de datos y migra el esquema si hace falta (PRNT-20). Lo que pasó
/// queda en [informe], que es lo que se le enseña al usuario tras actualizar.
class SqliteLearningRepository implements LearningRepository {
  /// Instancia cacheada de la BD (se crea en la primera llamada a [_db]).
  Database? _database;

  /// Future en curso de apertura, para evitar abrir la BD dos veces si llegan
  /// varias operaciones concurrentes antes de terminar la primera apertura.
  Future<Database>? _opening;

  final Completer<InformeApertura> _informe = Completer<InformeApertura>();

  /// Lo que pasó al abrir la base: si se migró, dónde quedó la copia previa y
  /// qué falló, si algo falló. Se completa en la primera apertura.
  Future<InformeApertura> get informe => _informe.future;

  /// Fuerza la apertura (y con ella la copia y la migración) sin esperar a que
  /// alguien dicte. Se llama al arrancar: migrar tiene que pasar ANTES de que
  /// nada toque la base, no la primera vez que el usuario habla.
  Future<InformeApertura> asegurarAbierta() async {
    await _db();
    return informe;
  }

  /// Devuelve la BD abriéndola de forma perezosa la primera vez, con su copia
  /// previa y su migración. Cachea la instancia.
  Future<Database> _db() {
    final cached = _database;
    if (cached != null) return Future.value(cached);

    // Si ya hay una apertura en curso, reutilízala (evita carreras).
    return _opening ??= _open();
  }

  Future<Database> _open() async {
    try {
      final apertura = await abrirBaseDeDatosDePronto();
      _database = apertura.db;
      if (!_informe.isCompleted) _informe.complete(apertura.informe);
      return apertura.db;
    } catch (e) {
      // Si falla la apertura, limpia el future en curso para reintentar luego.
      _opening = null;
      // El informe se completa igual: si no, quien lo espere para enseñar el
      // aviso se quedaría colgado para siempre.
      if (!_informe.isCompleted) {
        _informe.complete(InformeApertura(errorApertura: '$e'));
      }
      throw StateError('No se pudo abrir la base de datos de aprendizaje: $e');
    } finally {
      // Una vez resuelta (con éxito o no), deja de marcar la apertura en curso.
      _opening = null;
    }
  }

  @override
  Future<void> bumpCorrection(String raw, String corrected) async {
    final db = await _db();
    final now = DateTime.now().millisecondsSinceEpoch;
    final stmt = db.prepare('''
      INSERT INTO corrections (raw, corrected, freq, last_seen)
      VALUES (?, ?, 1, ?)
      ON CONFLICT(raw, corrected) DO UPDATE SET
        freq = freq + 1,
        last_seen = excluded.last_seen
    ''');
    try {
      stmt.execute([raw, corrected, now]);
    } finally {
      stmt.dispose();
    }
  }

  @override
  Future<List<CorrectionEntry>> activeCorrections({int minFreq = 3}) async {
    final db = await _db();
    final stmt = db.prepare('''
      SELECT raw, corrected, freq
      FROM corrections
      WHERE freq >= ?
      ORDER BY freq DESC
    ''');
    try {
      final ResultSet rows = stmt.select([minFreq]);
      return [
        for (final Row row in rows)
          CorrectionEntry(
            raw: row['raw'] as String,
            corrected: row['corrected'] as String,
            freq: row['freq'] as int,
          ),
      ];
    } finally {
      stmt.dispose();
    }
  }

  @override
  Future<void> bumpVocab(String term) async {
    final db = await _db();
    final stmt = db.prepare('''
      INSERT INTO vocab_terms (term, freq)
      VALUES (?, 1)
      ON CONFLICT(term) DO UPDATE SET
        freq = freq + 1
    ''');
    try {
      stmt.execute([term]);
    } finally {
      stmt.dispose();
    }
  }

  @override
  Future<List<String>> topVocab({int limit = 45}) async {
    final db = await _db();
    final stmt = db.prepare('''
      SELECT term
      FROM vocab_terms
      ORDER BY freq DESC
      LIMIT ?
    ''');
    try {
      final ResultSet rows = stmt.select([limit]);
      return [
        for (final Row row in rows) row['term'] as String,
      ];
    } finally {
      stmt.dispose();
    }
  }

  @override
  Future<void> logDictation({
    required String raw,
    required String finalText,
    required double avgLogProb,
  }) async {
    final db = await _db();
    final ts = DateTime.now().millisecondsSinceEpoch;
    final stmt = db.prepare('''
      INSERT INTO dictation_log (raw, final, avg_logprob, ts)
      VALUES (?, ?, ?, ?)
    ''');
    try {
      stmt.execute([raw, finalText, avgLogProb, ts]);
    } finally {
      stmt.dispose();
    }
  }

  /// Cierra la base de datos y libera la instancia cacheada.
  ///
  /// No forma parte de [LearningRepository]; útil al apagar la app o en tests.
  void dispose() {
    _database?.dispose();
    _database = null;
  }
}
