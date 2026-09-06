import 'dart:ffi';
import 'dart:io';

import 'package:sqlite3/open.dart';
import 'package:sqlite3/sqlite3.dart';

/// Carga SQLite en el entorno de `flutter test`.
///
/// En la app la librería nativa la aporta `sqlite3_flutter_libs`, que solo
/// existe dentro del bundle de Flutter. En los tests, que corren en la VM de
/// Dart, hay que apuntar a una: `winsqlite3.dll` viene con Windows 10/11.
///
/// ⚠ NO es el mismo binario que se publica. Es SQLite de verdad y todo lo que
/// estas pruebas usan —transacciones, `PRAGMA user_version`, `ALTER TABLE ADD
/// COLUMN`, `PRAGMA table_info`— lleva ahí desde hace más de una década y no
/// depende de las banderas de compilación. Pero el binario que corre en el
/// equipo del usuario no lo toca esta suite: eso solo lo cubre el ensayo de
/// `docs/ENSAYO-ACTUALIZACION.md`.
void prepareSqliteForTests() {
  if (!Platform.isWindows) return;
  open.overrideFor(
    OperatingSystem.windows,
    () => DynamicLibrary.open('winsqlite3.dll'),
  );
}

/// Vuelca el esquema completo, ordenado, para comparar dos bases campo a campo.
String dumpSchema(Database db) {
  final rows = db.select(
    'SELECT type, name, sql FROM sqlite_master '
    "WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name",
  );
  final parts = <String>[];
  for (final r in rows) {
    parts.add('${r['type']}|${r['name']}|${r['sql']}');
    if (r['type'] == 'table') {
      for (final c in db.select('PRAGMA table_info(${r['name']})')) {
        parts.add('  col|${c['name']}|${c['type']}|${c['notnull']}|'
            '${c['dflt_value']}|${c['pk']}');
      }
    }
  }
  return parts.join('\n');
}

/// Nombres de las tablas de la base (sin las internas de SQLite).
Set<String> tableNames(Database db) => db
    .select("SELECT name FROM sqlite_master WHERE type = 'table'")
    .map((r) => r['name'] as String)
    .toSet();
