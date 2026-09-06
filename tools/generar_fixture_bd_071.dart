// Genera `test/fixtures/pronto-0.7.1.db`: una base como la que tiene HOY en su
// disco cualquiera que instaló Pronto 0.7.1 o anterior.
//
//   dart run tools/generar_fixture_bd_071.dart
//
// Es el fichero que impide el verde falso del PRNT-20: los tests que crean la
// base de cero nunca ven el fallo de una instalación ACTUALIZADA, porque en una
// base nueva `CREATE TABLE IF NOT EXISTS` sí crea las columnas nuevas.
//
// El esquema de aquí abajo es COPIA LITERAL del `_createSchema` que Pronto
// llevó desde la 0.3.1 hasta la 0.7.1 (`lib/src/data/sqlite_learning_repository
// .dart`, commit a86072a). Sin `PRAGMA user_version` y sin una sola
// `ALTER TABLE`: eso es exactamente lo que hay que reproducir.
//
// Los datos son inventados. No sale de aquí nada de ninguna instalación real.
import 'dart:ffi';
import 'dart:io';

import 'package:sqlite3/open.dart';
import 'package:sqlite3/sqlite3.dart';

void main() {
  if (Platform.isWindows) {
    open.overrideFor(
      OperatingSystem.windows,
      () => DynamicLibrary.open('winsqlite3.dll'),
    );
  }

  final salida = File('test/fixtures/pronto-0.7.1.db');
  salida.parent.createSync(recursive: true);
  if (salida.existsSync()) salida.deleteSync();

  final db = sqlite3.open(salida.path);

  db.execute('''
    CREATE TABLE IF NOT EXISTS corrections (
      raw TEXT,
      corrected TEXT,
      freq INTEGER NOT NULL DEFAULT 0,
      last_seen INTEGER,
      PRIMARY KEY (raw, corrected)
    );
  ''');
  db.execute('''
    CREATE TABLE IF NOT EXISTS vocab_terms (
      term TEXT PRIMARY KEY,
      freq INTEGER NOT NULL DEFAULT 0
    );
  ''');
  db.execute('''
    CREATE TABLE IF NOT EXISTS dictation_log (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      raw TEXT,
      final TEXT,
      avg_logprob REAL,
      ts INTEGER
    );
  ''');

  // Uso real de alguien que lleva meses dictando: correcciones repetidas,
  // vocabulario propio con tildes y ñ, y dictados guardados.
  const ts = 1751000000000; // 27-jun-2026, dentro de la vida de la 0.7.1.
  db.execute(
    'INSERT INTO corrections (raw, corrected, freq, last_seen) VALUES '
    "('beltran labs', 'BeltranLabs', 7, $ts), "
    "('parakit', 'Parakeet', 4, $ts), "
    "('txonso', 'Txonso', 12, $ts)",
  );
  db.execute(
    'INSERT INTO vocab_terms (term, freq) VALUES '
    "('BeltranLabs', 12), ('Parakeet', 5), ('Añoveros', 3), ('ORDAX', 9)",
  );
  db.execute(
    'INSERT INTO dictation_log (raw, final, avg_logprob, ts) VALUES '
    "('hola que tal', 'Hola, ¿qué tal?', -0.21, $ts), "
    "('esto es una prueba', 'Esto es una prueba.', -0.14, ${ts + 60000})",
  );

  final v = db.select('PRAGMA user_version').first.values.first;
  db.dispose();

  stdout.writeln('Escrito ${salida.path} (${salida.lengthSync()} bytes), '
      'user_version = $v');
}
