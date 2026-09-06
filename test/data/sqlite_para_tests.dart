import 'dart:ffi';
import 'dart:io';

import 'package:sqlite3/open.dart';

/// Carga SQLite en el entorno de `flutter test`.
///
/// En la app la librería nativa la aporta `sqlite3_flutter_libs`, que solo
/// existe dentro del bundle de Flutter. En los tests, que corren en la VM de
/// Dart, hay que apuntar a una: `winsqlite3.dll` viene con Windows 10/11.
/// Es SQLite de verdad —transacciones, `PRAGMA user_version`, `ALTER TABLE
/// ADD COLUMN`, `PRAGMA table_info`—, que es todo lo que estas pruebas usan.
void prepararSqliteParaTests() {
  if (!Platform.isWindows) return;
  open.overrideFor(
    OperatingSystem.windows,
    () => DynamicLibrary.open('winsqlite3.dll'),
  );
}
