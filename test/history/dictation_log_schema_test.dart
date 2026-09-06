import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:pronto/src/data/schema_migrations.dart';
import 'package:sqlite3/sqlite3.dart';

import '../data/sqlite_for_tests.dart';

/// La migración 2 (PRNT-16) ensancha `dictation_log`. Es EXACTAMENTE el caso
/// que el PRNT-20 dijo que iba a romper: `CREATE TABLE IF NOT EXISTS` no añade
/// columnas a una tabla que ya existe, así que la instalación nueva nacería
/// bien y la actualizada se quedaría sin ellas.
void main() {
  setUpAll(prepareSqliteForTests);

  Set<String> columnsOf(Database db, String table) => db
      .select('PRAGMA table_info($table)')
      .map((r) => r['name'] as String)
      .toSet();

  const nuevas = {
    'source',
    'mode',
    'engine',
    'format_path',
    'format_model',
    'format_notice',
    'translated',
    'app_name',
    'window_title',
    'spoken_ms',
    'transcribe_ms',
    'format_ms',
    'cost_usd',
  };

  test('una instalación NUEVA tiene las columnas del historial', () {
    final db = sqlite3.openInMemory();
    addTearDown(db.dispose);

    migrateSchema(db);

    expect(columnsOf(db, 'dictation_log'), containsAll(nuevas));
  });

  test('una base de la 0.7.1 (sin versionar) ACABA IGUAL que una nueva', () {
    final fixture = File('test/fixtures/pronto-0.7.1.db');
    expect(
      fixture.existsSync(),
      isTrue,
      reason: 'falta test/fixtures/pronto-0.7.1.db '
          '(dart run tools/generate_071_fixture.dart)',
    );
    final tmp = Directory.systemTemp.createTempSync('pronto-hist-');
    addTearDown(() => tmp.deleteSync(recursive: true));
    final copia = File('${tmp.path}/pronto.db')
      ..writeAsBytesSync(fixture.readAsBytesSync());

    final vieja = sqlite3.open(copia.path);
    addTearDown(vieja.dispose);
    final nueva = sqlite3.openInMemory();
    addTearDown(nueva.dispose);

    migrateSchema(vieja);
    migrateSchema(nueva);

    expect(dumpSchema(vieja), dumpSchema(nueva));
  });

  test('correrla dos veces no revienta y no duplica columnas', () {
    final db = sqlite3.openInMemory();
    addTearDown(db.dispose);

    migrateSchema(db);
    // Se rebobina la versión a mano para forzar el segundo pase sobre una base
    // que YA tiene las columnas: es lo que pasa si el proceso muere después de
    // aplicar el ALTER pero antes del COMMIT en otra versión de SQLite.
    db.execute('PRAGMA user_version = 1');
    expect(() => migrateSchema(db), returnsNormally);

    final cols = db.select('PRAGMA table_info(dictation_log)');
    final nombres = cols.map((r) => r['name'] as String).toList();
    expect(nombres.toSet().length, nombres.length, reason: 'columna duplicada');
  });

  test('las filas que ya había se quedan como estaban, con source por defecto',
      () {
    final db = sqlite3.openInMemory();
    addTearDown(db.dispose);

    migrateSchema(db, migrations: [kProntoMigrations.first]);
    db.execute(
      "INSERT INTO dictation_log (raw, final, avg_logprob, ts) "
      "VALUES ('crudo', 'final', -0.2, 1000)",
    );

    migrateSchema(db);

    final row = db.select('SELECT * FROM dictation_log').single;
    expect(row['raw'], 'crudo');
    expect(row['final'], 'final');
    expect(row['ts'], 1000);
    expect(row['source'], 'dictado');
  });

  test('hay índice por fecha: la lista se ordena por ts, no por rowid', () {
    final db = sqlite3.openInMemory();
    addTearDown(db.dispose);

    migrateSchema(db);

    final indices = db
        .select("SELECT name FROM sqlite_master WHERE type = 'index'")
        .map((r) => r['name'] as String)
        .toSet();
    expect(indices, contains('idx_dictation_log_ts'));
  });
}
