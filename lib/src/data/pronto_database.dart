/// Apertura de `pronto.db`: copia de seguridad primero, migración después.
///
/// Este es el único sitio donde `pronto.db` se abre. Junta las dos mitades del
/// PRNT-20 —la copia previa y las migraciones numeradas— porque el orden entre
/// ellas ES el requisito: primero se copia, luego se toca.
library;

import 'dart:io';

import 'package:path/path.dart' as p;
import 'package:path_provider/path_provider.dart';
import 'package:sqlite3/sqlite3.dart';

import 'data_backup.dart';
import 'schema_migrations.dart';
import 'sql_functions.dart';

/// Nombre del fichero de la base de datos dentro de la carpeta de datos.
const String kDatabaseFile = 'pronto.db';

/// Carpeta donde se dejan las copias previas: HERMANA de la carpeta de datos
/// (`%APPDATA%\Pronto\copias`), no dentro. Dentro, cada copia entraría en la
/// siguiente y crecerían unas sobre otras.
const String kBackupsFolder = 'copias';

/// Dónde van las copias de una carpeta de datos dada.
Directory backupsRootFor(Directory supportDir) =>
    Directory(p.join(p.dirname(supportDir.path), kBackupsFolder));

/// En qué acabó abrir la base. Es lo que decide QUÉ se le dice al usuario, para
/// que la pantalla no tenga que adivinarlo cruzando banderas: cada caso tiene
/// una frase distinta y todas tienen que ser verdad.
enum OpenOutcome {
  /// Nada que contar: instalación nueva, o la base ya estaba al día.
  nothingToSay,

  /// Se migró una instalación que ya tenía datos, y salió bien.
  migrated,

  /// Se migró EN PARTE y luego falló. Los datos **no** están como estaban:
  /// decirle al usuario que no se ha tocado nada sería mentirle.
  migratedThenFailed,

  /// Falló antes de aplicar nada: la base sigue igual que estaba.
  untouchedAfterError,

  /// No se pudo hacer la copia previa, así que no se migró nada a propósito.
  untouchedNoBackup,

  /// La base viene de una versión de Pronto más nueva (el usuario volvió a una
  /// anterior). No se ha degradado.
  databaseFromNewerVersion,

  /// No se pudo ni abrir la base. Pronto arranca, pero sin aprendizaje: eso el
  /// usuario tiene que saberlo, no descubrirlo cuando deje de acertar con sus
  /// palabras.
  couldNotOpen,
}

/// Lo que pasó al abrir la base. Es lo que el usuario ve tras actualizar.
class OpenReport {
  final OpenOutcome outcome;

  /// Versión de esquema al abrir y con la que queda.
  final int fromVersion;
  final int toVersion;

  /// Dónde quedó la copia previa, si se hizo.
  final String? backupPath;

  /// El texto crudo del fallo (de SQLite o del sistema de ficheros). Va aparte
  /// de la frase que se le enseña al usuario: sirve para diagnosticar, no para
  /// hablarle.
  final String? technicalDetail;

  const OpenReport({
    required this.outcome,
    this.fromVersion = 0,
    this.toVersion = 0,
    this.backupPath,
    this.technicalDetail,
  });

  /// Construye el informe de una migración que terminó sin errores.
  factory OpenReport.fromMigration(MigrationResult r, {String? backupPath}) {
    final OpenOutcome outcome;
    if (r.databaseNewerThanApp) {
      outcome = OpenOutcome.databaseFromNewerVersion;
    } else if (!r.freshInstall && r.toVersion > r.fromVersion) {
      outcome = OpenOutcome.migrated;
    } else {
      outcome = OpenOutcome.nothingToSay;
    }
    return OpenReport(
      outcome: outcome,
      fromVersion: r.fromVersion,
      toVersion: r.toVersion,
      backupPath: backupPath,
    );
  }

  /// ¿Hay que enseñarle algo al usuario?
  bool get needsNotice => outcome != OpenOutcome.nothingToSay;

  /// ¿Se ha tocado el esquema de una base que ya tenía datos?
  bool get didUpgrade =>
      outcome == OpenOutcome.migrated ||
      outcome == OpenOutcome.migratedThenFailed;
}

/// La base abierta y lista, con el informe de lo que pasó al abrirla.
class OpenedDatabase {
  final Database db;
  final OpenReport report;

  const OpenedDatabase(this.db, this.report);
}

/// Abre `pronto.db` de [supportDir], copiando y migrando si hace falta.
///
/// [backupsRoot] es dónde van las copias previas; [now] permite fijar la fecha
/// del nombre de la copia en los tests.
Future<OpenedDatabase> openDatabase({
  required Directory supportDir,
  required Directory backupsRoot,
  DateTime Function()? now,
  List<Migration> migrations = kProntoMigrations,
}) async {
  supportDir.createSync(recursive: true);
  final path = p.join(supportDir.path, kDatabaseFile);

  var db = _openWithFunctions(path);
  try {
    final from = schemaVersion(db);
    final fresh = databaseIsEmpty(db);

    if (!hasPendingMigrations(db, migrations: migrations)) {
      // Ni copia ni migración. Solo queda decidir si la base viene del futuro.
      return OpenedDatabase(
        db,
        OpenReport(
          outcome: from > targetVersion(migrations)
              ? OpenOutcome.databaseFromNewerVersion
              : OpenOutcome.nothingToSay,
          fromVersion: from,
          toVersion: from,
        ),
      );
    }

    // Hay que migrar. Si ya había datos, primero la copia — y con la base
    // CERRADA, para que lo copiado sea un fichero consistente y no una foto a
    // media escritura.
    String? backupPath;
    if (!fresh) {
      db.dispose();
      try {
        backupPath = await backUpDataFolder(
          source: supportDir,
          backupsRoot: backupsRoot,
          date: (now ?? DateTime.now)(),
        );
      } catch (e) {
        // Sin copia no se migra. La app arranca con el esquema viejo y el
        // usuario se entera por el aviso, en vez de descubrirlo cuando le
        // falten datos.
        db = _openWithFunctions(path);
        return OpenedDatabase(
          db,
          OpenReport(
            outcome: OpenOutcome.untouchedNoBackup,
            fromVersion: from,
            toVersion: from,
            technicalDetail: '$e',
          ),
        );
      }
      db = _openWithFunctions(path);
    }

    try {
      return OpenedDatabase(
        db,
        OpenReport.fromMigration(
          migrateSchema(db, migrations: migrations),
          backupPath: backupPath,
        ),
      );
    } catch (e) {
      // Cada migración va en su transacción, así que la base está entera en
      // alguna versión — pero puede NO ser la que tenía al empezar.
      final to = schemaVersion(db);
      return OpenedDatabase(
        db,
        OpenReport(
          outcome: to > from
              ? OpenOutcome.migratedThenFailed
              : OpenOutcome.untouchedAfterError,
          fromVersion: from,
          toVersion: to,
          backupPath: backupPath,
          technicalDetail: '$e',
        ),
      );
    }
  } catch (_) {
    // No dejamos la base abierta si la apertura no llega a buen puerto. El
    // dispose va protegido porque en el camino de la copia la instancia ya
    // pudo cerrarse antes.
    try {
      db.dispose();
    } catch (_) {}
    rethrow;
  }
}

/// Abre el fichero y le deja puestas las funciones propias.
///
/// `pronto_fold` la usa la búsqueda del Historial, y una función SQL vive en la
/// CONEXIÓN, no en el fichero: si se registra en un solo sitio y luego se
/// reabre la base (el camino de la copia previa lo hace), la consulta falla en
/// tiempo de ejecución. Por eso pasa por aquí toda apertura.
Database _openWithFunctions(String path) {
  final db = sqlite3.open(path);
  registerProntoFunctions(db);
  return db;
}

/// Igual que [openDatabase] pero resolviendo las carpetas reales de la app: los
/// datos en `getApplicationSupportDirectory()` y las copias en su hermana
/// `copias/`.
Future<OpenedDatabase> openProntoDatabase() async {
  final supportDir = await getApplicationSupportDirectory();
  return openDatabase(
    supportDir: supportDir,
    backupsRoot: backupsRootFor(supportDir),
  );
}
