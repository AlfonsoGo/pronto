import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;
import 'package:pronto/src/data/pronto_database.dart';
import 'package:pronto/src/data/schema_migrations.dart';
import 'package:sqlite3/sqlite3.dart';

import 'sqlite_for_tests.dart';

/// El ensayo de actualización, en test: se parte de una carpeta de datos como
/// la que tiene hoy quien instaló la 0.7.1 —base sin versionar, ajustes y el
/// modelo de 640 MB— y se abre con el código de hoy.
void main() {
  setUpAll(prepareSqliteForTests);

  late Directory root;
  late Directory dataDir;
  late Directory backups;

  setUp(() {
    root = Directory.systemTemp.createTempSync('pronto-bd-');
    dataDir = Directory(p.join(root.path, 'Pronto'))
      ..createSync(recursive: true);
    backups = Directory(p.join(root.path, kBackupsFolder));
  });

  tearDown(() {
    if (root.existsSync()) root.deleteSync(recursive: true);
  });

  /// Deja en [dataDir] la foto de una instalación de la 0.7.1.
  void installOf071() {
    final source = File('test/fixtures/pronto-0.7.1.db');
    expect(
      source.existsSync(),
      isTrue,
      reason: 'falta test/fixtures/pronto-0.7.1.db '
          '(dart run tools/generate_071_fixture.dart)',
    );
    File(p.join(dataDir.path, kDatabaseFile))
        .writeAsBytesSync(source.readAsBytesSync());
    File(p.join(dataDir.path, 'shared_preferences.json')).writeAsStringSync(
      '{"flutter.app_settings":"{\\"formatPath\\":\\"none\\"}"}',
    );
    final model = Directory(p.join(dataDir.path, 'models', 'parakeet'))
      ..createSync(recursive: true);
    File(p.join(model.path, 'encoder.int8.onnx')).writeAsStringSync('x' * 500);
  }

  Future<OpenedDatabase> open({
    List<Migration> migrations = kProntoMigrations,
    Directory? backupsTo,
  }) =>
      openDatabase(
        supportDir: dataDir,
        backupsRoot: backupsTo ?? backups,
        now: () => DateTime(2026, 9, 6, 14, 32),
        migrations: migrations,
      );

  List<String> backupNames() => backups.existsSync()
      ? backups.listSync().map((e) => p.basename(e.path)).toList()
      : <String>[];

  int countOf(Database db, String table) =>
      db.select('SELECT count(*) AS n FROM $table').first['n'] as int;

  group('dónde van las copias', () {
    test('a la carpeta HERMANA de los datos, nunca dentro', () {
      // Es la ruta que usa la app de verdad (`openProntoDatabase`), y la única
      // parte de ella que se puede fijar sin arrancar Flutter entero.
      final support =
          Directory(r'C:\Users\quien\AppData\Roaming\Pronto\Pronto');

      final where = backupsRootFor(support);

      expect(p.basename(where.path), 'copias');
      expect(p.dirname(where.path), p.dirname(support.path));
      expect(p.isWithin(support.path, where.path), isFalse);
    });
  });

  group('instalación nueva', () {
    test('no hace copia de nada ni cuenta nada al usuario', () async {
      final a = await open();
      addTearDown(a.db.dispose);

      expect(a.report.outcome, OpenOutcome.nothingToSay);
      expect(a.report.backupPath, isNull);
      expect(a.report.needsNotice, isFalse);
      expect(backupNames(), isEmpty);
      expect(schemaVersion(a.db), targetVersion(kProntoMigrations));
    });
  });

  group('actualización de una instalación de la 0.7.1', () {
    test('copia los datos ANTES de migrar y dice dónde quedaron', () async {
      installOf071();

      final a = await open();
      addTearDown(a.db.dispose);

      expect(a.report.outcome, OpenOutcome.migrated);
      expect(a.report.didUpgrade, isTrue);
      expect(a.report.needsNotice, isTrue);
      expect(a.report.backupPath, isNotNull);

      final backup = Directory(a.report.backupPath!);
      expect(backup.existsSync(), isTrue);
      expect(File(p.join(backup.path, kDatabaseFile)).existsSync(), isTrue);
      expect(
        File(p.join(backup.path, 'shared_preferences.json')).existsSync(),
        isTrue,
      );
      // Los 640 MB del modelo no entran en la copia...
      expect(Directory(p.join(backup.path, 'models')).existsSync(), isFalse);
      // ...y siguen intactos donde estaban: nadie se baja el modelo otra vez.
      expect(
        File(p.join(dataDir.path, 'models', 'parakeet', 'encoder.int8.onnx'))
            .existsSync(),
        isTrue,
      );
    });

    test('la copia guarda la base TAL COMO ESTABA, sin versionar', () async {
      installOf071();

      final a = await open();
      addTearDown(a.db.dispose);

      final copied = sqlite3.open(p.join(a.report.backupPath!, kDatabaseFile));
      addTearDown(copied.dispose);
      // Si la copia se hubiera hecho DESPUÉS de migrar no serviría de vuelta
      // atrás: tendría ya la versión nueva.
      expect(schemaVersion(copied), 0);
      expect(schemaVersion(a.db), targetVersion(kProntoMigrations));
    });

    test('no se pierde ni un dato', () async {
      installOf071();

      final a = await open();
      addTearDown(a.db.dispose);

      expect(countOf(a.db, 'corrections'), 3);
      expect(countOf(a.db, 'vocab_terms'), 4);
      expect(countOf(a.db, 'dictation_log'), 2);
      expect(
        a.db
            .select("SELECT freq FROM corrections WHERE raw = 'txonso'")
            .first['freq'],
        12,
      );
    });

    test('abrir otra vez no vuelve a copiar ni a avisar', () async {
      installOf071();

      final first = await open();
      first.db.dispose();
      final afterFirst = backupNames();

      final second = await open();
      addTearDown(second.db.dispose);

      expect(second.report.outcome, OpenOutcome.nothingToSay);
      expect(second.report.needsNotice, isFalse);
      expect(second.report.backupPath, isNull);
      expect(backupNames(), afterFirst);
    });

    test('acaba en el MISMO esquema que una instalación nueva', () async {
      installOf071();
      final updated = await open();
      addTearDown(updated.db.dispose);

      final otherRoot = Directory.systemTemp.createTempSync('pronto-bd-nueva-');
      addTearDown(() => otherRoot.deleteSync(recursive: true));
      final fresh = await openDatabase(
        supportDir: Directory(p.join(otherRoot.path, 'Pronto')),
        backupsRoot: Directory(p.join(otherRoot.path, kBackupsFolder)),
      );
      addTearDown(fresh.db.dispose);

      expect(dumpSchema(updated.db), dumpSchema(fresh.db));
      expect(schemaVersion(updated.db), schemaVersion(fresh.db));
    });
  });

  group('cuando algo sale mal', () {
    test('sin copia NO se migra, y se dice por qué', () async {
      installOf071();

      // Carpeta de copias dentro de la de datos: la copia se rechaza.
      final a = await open(
        backupsTo: Directory(p.join(dataDir.path, kBackupsFolder)),
      );
      addTearDown(a.db.dispose);

      expect(a.report.outcome, OpenOutcome.untouchedNoBackup);
      expect(a.report.technicalDetail, isNotNull);
      expect(a.report.needsNotice, isTrue);
      expect(a.report.didUpgrade, isFalse);
      expect(a.report.backupPath, isNull);
      // Lo importante: la base del usuario está EXACTAMENTE como estaba.
      expect(schemaVersion(a.db), 0);
      expect(countOf(a.db, 'corrections'), 3);
    });

    test('una migración que entra A MEDIAS no dice que no se tocó nada',
        () async {
      // Es la mentira que hay que evitar: la 1 SÍ entró, así que los datos NO
      // están como estaban, y el aviso no puede decir lo contrario.
      installOf071();

      final a = await open(
        migrations: [
          kProntoMigrations.first,
          Migration(
            version: 2,
            name: 'la que revienta',
            apply: (db) {
              db.execute('CREATE TABLE a_medias (id INTEGER)');
              throw StateError('disco lleno');
            },
          ),
        ],
      );
      addTearDown(a.db.dispose);

      expect(a.report.outcome, OpenOutcome.migratedThenFailed);
      expect(a.report.didUpgrade, isTrue);
      expect(a.report.needsNotice, isTrue);
      expect(a.report.technicalDetail, contains('disco lleno'));
      // La copia previa existe: hay camino de vuelta.
      expect(a.report.backupPath, isNotNull);
      expect(Directory(a.report.backupPath!).existsSync(), isTrue);
      // La 1 entró; la 2 no dejó ni su tabla ni su versión.
      expect(a.report.fromVersion, 0);
      expect(a.report.toVersion, 1);
      expect(schemaVersion(a.db), 1);
      expect(tableNames(a.db), isNot(contains('a_medias')));
      expect(countOf(a.db, 'corrections'), 3);
    });

    test('si falla la PRIMERA migración, ahí sí no se tocó nada', () async {
      installOf071();

      final a = await open(
        migrations: [
          Migration(
            version: 1,
            name: 'la que revienta',
            apply: (_) => throw StateError('disco lleno'),
          ),
        ],
      );
      addTearDown(a.db.dispose);

      expect(a.report.outcome, OpenOutcome.untouchedAfterError);
      expect(a.report.didUpgrade, isFalse);
      expect(a.report.fromVersion, a.report.toVersion);
      expect(schemaVersion(a.db), 0);
    });

    test('una base más nueva que la app se abre igual, y se avisa', () async {
      installOf071();
      final future = sqlite3.open(p.join(dataDir.path, kDatabaseFile));
      future.execute('PRAGMA user_version = 99');
      future.dispose();

      final a = await open();
      addTearDown(a.db.dispose);

      expect(a.report.outcome, OpenOutcome.databaseFromNewerVersion);
      // Volver a una versión anterior de Pronto es exactamente el caso que el
      // ticket llama «camino de vuelta»: el usuario tiene que enterarse.
      expect(a.report.needsNotice, isTrue);
      expect(a.report.didUpgrade, isFalse);
      expect(schemaVersion(a.db), 99);
      expect(a.report.backupPath, isNull);
      expect(countOf(a.db, 'corrections'), 3);
    });
  });
}
