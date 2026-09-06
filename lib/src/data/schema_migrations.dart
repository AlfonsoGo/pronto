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
class Migracion {
  /// Número consecutivo desde 1. No se renumera jamás: es lo que queda escrito
  /// en `PRAGMA user_version` de las bases de los usuarios.
  final int version;

  /// Nombre corto para el informe y los logs.
  final String nombre;

  /// Los cambios. DEBE ser idempotente: puede correr sobre una base donde el
  /// cambio ya esté hecho (por ejemplo una instalación nueva).
  final void Function(Database db) aplicar;

  const Migracion({
    required this.version,
    required this.nombre,
    required this.aplicar,
  });
}

/// Las migraciones de Pronto, en orden.
///
/// La 1 es el esquema que ya tenían las instalaciones de la 0.7.1: sobre una
/// base existente no cambia nada (`IF NOT EXISTS`), sobre una nueva lo crea.
/// Eso es lo que hace que una instalación nueva y una actualizada acaben en el
/// mismo estado.
const List<Migracion> kMigracionesPronto = [
  Migracion(
    version: 1,
    nombre: 'esquema base (correcciones, vocabulario, log de dictados)',
    aplicar: _v1EsquemaBase,
  ),
];

void _v1EsquemaBase(Database db) {
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

/// Lo que pasó al migrar. Es lo que se le enseña al usuario.
class ResultadoMigracion {
  /// Versión de esquema que tenía la base al abrirla.
  final int versionInicial;

  /// Versión de esquema con la que queda.
  final int versionFinal;

  /// Nombres de las migraciones que se aplicaron, en orden.
  final List<String> aplicadas;

  /// La base estaba vacía: es una instalación NUEVA, no una actualización.
  /// Aquí no hay nada que salvar ni nada que contarle al usuario.
  final bool instalacionNueva;

  /// La base venía de una versión de Pronto MÁS NUEVA que esta. No se ha
  /// tocado: degradar el esquema perdería datos.
  final bool baseMasNuevaQueLaApp;

  const ResultadoMigracion({
    required this.versionInicial,
    required this.versionFinal,
    required this.aplicadas,
    required this.instalacionNueva,
    this.baseMasNuevaQueLaApp = false,
  });

  /// Se ha tocado el esquema de una base que ya tenía datos: esto es lo que el
  /// usuario merece saber (y lo que justifica haber hecho una copia antes).
  bool get huboActualizacion =>
      !instalacionNueva && versionFinal > versionInicial;
}

/// Versión de esquema escrita en la base (`PRAGMA user_version`). 0 = sin
/// versionar, que es como salieron todas las instalaciones hasta la 0.7.1.
int versionEsquema(Database db) =>
    db.select('PRAGMA user_version').first.values.first! as int;

/// ¿Tiene la base tablas nuestras? Distingue una instalación NUEVA (fichero
/// recién creado, sin tablas) de una de la 0.7.1 (tablas sin `user_version`).
/// Sin esta distinción las dos se verían iguales —`user_version = 0`— y le
/// contaríamos una migración a quien acaba de instalar Pronto por primera vez.
bool baseVacia(Database db) {
  final filas = db.select(
    'SELECT count(*) AS n FROM sqlite_master '
    "WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
  );
  return (filas.first['n'] as int) == 0;
}

/// La versión a la que lleva la lista de migraciones (0 si está vacía).
int versionObjetivo(List<Migracion> migraciones) =>
    migraciones.isEmpty ? 0 : migraciones.last.version;

/// ¿Hay algo que migrar en esta base? Se responde ANTES de tocar nada para
/// decidir si hace falta la copia de seguridad previa.
bool hayMigracionesPendientes(
  Database db, {
  List<Migracion> migraciones = kMigracionesPronto,
}) {
  _validarLista(migraciones);
  return versionEsquema(db) < versionObjetivo(migraciones);
}

/// Aplica las migraciones pendientes, en orden, cada una en su transacción.
///
/// No hace copias ni sabe de ficheros: eso es de `abrirBaseDeDatos`. Aquí solo
/// se mueve el esquema.
ResultadoMigracion migrarEsquema(
  Database db, {
  List<Migracion> migraciones = kMigracionesPronto,
}) {
  _validarLista(migraciones);

  final vacia = baseVacia(db);
  final inicial = versionEsquema(db);
  final objetivo = versionObjetivo(migraciones);

  if (inicial > objetivo) {
    // El usuario ha vuelto a una versión anterior de Pronto. Su base ya sabe
    // más que este código. Degradarla borraría columnas con datos dentro, así
    // que no se toca: SQLite ignora las columnas que no consulta.
    return ResultadoMigracion(
      versionInicial: inicial,
      versionFinal: inicial,
      aplicadas: const [],
      instalacionNueva: vacia,
      baseMasNuevaQueLaApp: true,
    );
  }

  final aplicadas = <String>[];
  for (final m in migraciones) {
    if (m.version <= inicial) continue;
    // Transacción por migración: el `PRAGMA user_version` viaja DENTRO, así que
    // o se ven los cambios y la versión nueva, o no se ve ninguna de las dos.
    // Matar el proceso aquí en medio no deja una base a medias.
    db.execute('BEGIN IMMEDIATE');
    try {
      m.aplicar(db);
      db.execute('PRAGMA user_version = ${m.version}');
      db.execute('COMMIT');
    } catch (_) {
      db.execute('ROLLBACK');
      rethrow;
    }
    aplicadas.add(m.nombre);
  }

  return ResultadoMigracion(
    versionInicial: inicial,
    versionFinal: versionEsquema(db),
    aplicadas: aplicadas,
    instalacionNueva: vacia,
  );
}

/// Añade una columna solo si no está ya. Es la herramienta con la que se
/// escriben las migraciones que amplían una tabla: SQLite no tiene
/// `ADD COLUMN IF NOT EXISTS`, y sin esta comprobación una migración correcta
/// deja de ser idempotente y revienta al segundo intento.
void anadirColumnaSiFalta(
  Database db,
  String tabla,
  String columna,
  String declaracion,
) {
  final cols = db.select('PRAGMA table_info($tabla)');
  final existe = cols.any((f) => f['name'] == columna);
  if (existe) return;
  db.execute('ALTER TABLE $tabla ADD COLUMN $columna $declaracion');
}

/// Una lista mal formada es un error de programación, no del usuario: dos
/// migraciones con el mismo número, o desordenadas, dejarían bases distintas en
/// máquinas distintas según el orden en que se aplicaran.
void _validarLista(List<Migracion> migraciones) {
  for (var i = 0; i < migraciones.length; i++) {
    final esperada = i + 1;
    if (migraciones[i].version != esperada) {
      throw StateError(
        'Migraciones mal numeradas: en la posición $i se esperaba la versión '
        '$esperada y hay la ${migraciones[i].version}. Deben ser consecutivas '
        'desde 1 y no se renumeran nunca.',
      );
    }
  }
}
