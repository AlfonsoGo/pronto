/// Versionado del esquema de `pronto.db` y migraciones numeradas.
///
/// POR QUÉ EXISTE ESTE FICHERO (PRNT-20). Hasta la 0.7.1 el esquema se creaba
/// con tres `CREATE TABLE IF NOT EXISTS` y nada más: sin `PRAGMA user_version`
/// y sin una sola `ALTER TABLE`. Eso funciona mientras el esquema no cambie,
/// pero Pronto **se actualiza solo** desde GitHub Releases: en cuanto una
/// versión añada una columna, `IF NOT EXISTS` no la añade a una tabla que ya
/// existe. La instalación NUEVA nacería bien y la ACTUALIZADA se rompería —
/// y los tests, que crean la base de cero, dirían verde igual.
///
/// El contrato de este módulo:
///  - cada migración tiene un número consecutivo desde 1 y no se renumera nunca;
///  - cada migración es **idempotente** (correrla dos veces = correrla una);
///  - cada migración corre en su propia transacción JUNTO con el bump de
///    `user_version`, así que matar el proceso a mitad deja la base o entera en
///    la versión vieja o entera en la nueva, nunca a medias;
///  - una base MÁS NUEVA que el código (el usuario volvió a una versión
///    anterior) no se toca ni se degrada: se informa y se sigue.
library;

import 'package:sqlite3/sqlite3.dart';

/// Una migración numerada del esquema.
class Migration {
  /// Número consecutivo desde 1. No se renumera jamás: es lo que queda escrito
  /// en `PRAGMA user_version` de las bases de los usuarios.
  final int version;

  /// Nombre corto para el informe y los logs.
  final String name;

  /// Los cambios. DEBE ser idempotente: puede correr sobre una base donde el
  /// cambio ya esté hecho (por ejemplo una instalación nueva).
  ///
  /// Para ampliar una tabla, SQLite no tiene `ADD COLUMN IF NOT EXISTS`: hay
  /// que mirar antes `PRAGMA table_info(<tabla>)` y saltarse la columna que ya
  /// esté. Sin eso, la migración revienta al segundo intento.
  final void Function(Database db) apply;

  const Migration({
    required this.version,
    required this.name,
    required this.apply,
  });
}

/// Las migraciones de Pronto, en orden.
///
/// La 1 es el esquema que ya tenían las instalaciones de la 0.7.1: sobre una
/// base existente no cambia nada (`IF NOT EXISTS`), sobre una nueva lo crea.
/// Eso es lo que hace que una instalación nueva y una actualizada acaben en el
/// mismo estado.
const List<Migration> kProntoMigrations = [
  Migration(
    version: 1,
    name: 'esquema base (correcciones, vocabulario, log de dictados)',
    apply: _v1BaseSchema,
  ),
  Migration(
    version: 2,
    name: 'historial: metadatos del dictado e índice por fecha',
    apply: _v2HistoryColumns,
  ),
];

void _v1BaseSchema(Database db) {
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
}

/// PRNT-16. `dictation_log` guardaba cuatro campos: crudo, final, confianza y
/// fecha. El Historial necesita saber además POR DÓNDE pasó cada dictado —qué
/// motor, qué formateo, cuánto tardó cada etapa, qué costó y en qué app
/// aterrizó— porque eso es lo que convierte la pantalla en una herramienta de
/// diagnóstico y no en un archivo.
///
/// Todas las columnas nuevas son opcionales salvo [source]: las filas que ya
/// existen se quedan como estaban y solo ganan huecos vacíos.
void _v2HistoryColumns(Database db) {
  // `source` separa lo que el usuario dictó de verdad de las pruebas que hace
  // el asistente de instalación (PRNT-13). El Historial enseña solo lo primero.
  _addColumn(db, 'dictation_log', 'source', "TEXT NOT NULL DEFAULT 'dictado'");
  // El Modo con el que se dictó (PRNT-3). Hoy nadie lo escribe: queda a null
  // hasta que los Modos existan, y entonces las filas viejas dirán la verdad
  // —no se sabe— en vez de mentir con «Por defecto».
  _addColumn(db, 'dictation_log', 'mode', 'TEXT');
  _addColumn(db, 'dictation_log', 'engine', 'TEXT');
  _addColumn(db, 'dictation_log', 'format_path', 'TEXT');
  _addColumn(db, 'dictation_log', 'format_model', 'TEXT');
  // El aviso es SIEMPRE una cadena nuestra, nunca la respuesta del proveedor:
  // el texto del modelo no habla con la voz del sistema.
  _addColumn(db, 'dictation_log', 'format_notice', 'TEXT');
  // ¿Lo que se pegó era una TRADUCCIÓN de lo dictado? Si lo es, el crudo y el
  // final están en idiomas distintos y su diferencia no es una corrección:
  // aprender de ella envenena el diccionario. Sin esta columna, el Historial no
  // tiene forma de saberlo y la corrección a mano lo haría a ciegas.
  _addColumn(db, 'dictation_log', 'translated', 'INTEGER NOT NULL DEFAULT 0');
  _addColumn(db, 'dictation_log', 'app_name', 'TEXT');
  _addColumn(db, 'dictation_log', 'window_title', 'TEXT');
  // Cuánto se habló. Es el dato honesto del «tiempo ahorrado»: sin él habría
  // que estimarlo, y una estimación en una pantalla de estadísticas se lee
  // como una medida.
  _addColumn(db, 'dictation_log', 'spoken_ms', 'INTEGER');
  _addColumn(db, 'dictation_log', 'transcribe_ms', 'INTEGER');
  _addColumn(db, 'dictation_log', 'format_ms', 'INTEGER');
  // En dólares, que es la moneda en la que factura OpenRouter. Convertirlo a
  // euros aquí sería inventarse un tipo de cambio.
  _addColumn(db, 'dictation_log', 'cost_usd', 'REAL');

  // La lista se pagina por fecha descendente. Sin índice, 5.000 entradas son
  // un scan de tabla en cada tecla de la búsqueda.
  db.execute(
    'CREATE INDEX IF NOT EXISTS idx_dictation_log_ts '
    'ON dictation_log (ts DESC)',
  );
}

/// `ALTER TABLE ... ADD COLUMN` sin `IF NOT EXISTS` (SQLite no lo tiene): se
/// mira antes `PRAGMA table_info`. Sin esto, la migración revienta al segundo
/// intento y el contrato de idempotencia se queda en el comentario.
void _addColumn(Database db, String table, String column, String decl) {
  final existe = db
      .select('PRAGMA table_info($table)')
      .any((r) => r['name'] == column);
  if (existe) return;
  db.execute('ALTER TABLE $table ADD COLUMN $column $decl');
}

/// Lo que pasó al migrar el esquema.
class MigrationResult {
  /// Versión de esquema que tenía la base al abrirla.
  final int fromVersion;

  /// Versión de esquema con la que queda.
  final int toVersion;

  /// Nombres de las migraciones que se aplicaron, en orden.
  final List<String> applied;

  /// La base estaba vacía: es una instalación NUEVA, no una actualización.
  /// Aquí no hay nada que salvar ni nada que contarle al usuario.
  final bool freshInstall;

  /// La base venía de una versión de Pronto MÁS NUEVA que esta. No se ha
  /// tocado: degradar el esquema perdería datos.
  final bool databaseNewerThanApp;

  const MigrationResult({
    required this.fromVersion,
    required this.toVersion,
    required this.applied,
    required this.freshInstall,
    this.databaseNewerThanApp = false,
  });
}

/// Versión de esquema escrita en la base (`PRAGMA user_version`). 0 = sin
/// versionar, que es como salieron todas las instalaciones hasta la 0.7.1.
int schemaVersion(Database db) =>
    db.select('PRAGMA user_version').first.values.first! as int;

/// ¿Tiene la base tablas nuestras? Distingue una instalación NUEVA (fichero
/// recién creado, sin tablas) de una de la 0.7.1 (tablas sin `user_version`).
/// Sin esta distinción las dos se verían iguales —`user_version = 0`— y le
/// contaríamos una migración a quien acaba de instalar Pronto por primera vez.
bool databaseIsEmpty(Database db) {
  final rows = db.select(
    'SELECT count(*) AS n FROM sqlite_master '
    "WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
  );
  return (rows.first['n'] as int) == 0;
}

/// La versión a la que lleva la lista de migraciones (0 si está vacía).
int targetVersion(List<Migration> migrations) =>
    migrations.isEmpty ? 0 : migrations.last.version;

/// ¿Hay algo que migrar en esta base? Se responde ANTES de tocar nada para
/// decidir si hace falta la copia de seguridad previa.
bool hasPendingMigrations(
  Database db, {
  List<Migration> migrations = kProntoMigrations,
}) {
  _validateList(migrations);
  return schemaVersion(db) < targetVersion(migrations);
}

/// Aplica las migraciones pendientes, en orden, cada una en su transacción.
///
/// No hace copias ni sabe de ficheros: eso es de `openDatabase`. Aquí solo se
/// mueve el esquema.
MigrationResult migrateSchema(
  Database db, {
  List<Migration> migrations = kProntoMigrations,
}) {
  _validateList(migrations);

  final empty = databaseIsEmpty(db);
  final from = schemaVersion(db);
  final target = targetVersion(migrations);

  if (from > target) {
    // El usuario ha vuelto a una versión anterior de Pronto. Su base ya sabe
    // más que este código. Degradarla borraría columnas con datos dentro, así
    // que no se toca: SQLite ignora las columnas que no consulta.
    return MigrationResult(
      fromVersion: from,
      toVersion: from,
      applied: const [],
      freshInstall: empty,
      databaseNewerThanApp: true,
    );
  }

  final applied = <String>[];
  for (final m in migrations) {
    if (m.version <= from) continue;
    // Transacción por migración: el `PRAGMA user_version` viaja DENTRO, así que
    // o se ven los cambios y la versión nueva, o no se ve ninguna de las dos.
    // Matar el proceso aquí en medio no deja una base a medias.
    db.execute('BEGIN IMMEDIATE');
    try {
      m.apply(db);
      db.execute('PRAGMA user_version = ${m.version}');
      db.execute('COMMIT');
    } catch (_) {
      db.execute('ROLLBACK');
      rethrow;
    }
    applied.add(m.name);
  }

  return MigrationResult(
    fromVersion: from,
    toVersion: schemaVersion(db),
    applied: applied,
    freshInstall: empty,
  );
}

/// Una lista mal formada es un error de programación, no del usuario: dos
/// migraciones con el mismo número, o desordenadas, dejarían bases distintas en
/// máquinas distintas según el orden en que se aplicaran.
void _validateList(List<Migration> migrations) {
  for (var i = 0; i < migrations.length; i++) {
    final expected = i + 1;
    if (migrations[i].version != expected) {
      throw StateError(
        'Migraciones mal numeradas: en la posición $i se esperaba la versión '
        '$expected y hay la ${migrations[i].version}. Deben ser consecutivas '
        'desde 1 y no se renumeran nunca.',
      );
    }
  }
}
