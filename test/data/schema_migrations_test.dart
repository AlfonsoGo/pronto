import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:pronto/src/data/schema_migrations.dart';
import 'package:sqlite3/sqlite3.dart';

import 'sqlite_for_tests.dart';

/// Migraciones de juguete para probar la MAQUINARIA (orden, transacción,
/// idempotencia) sin inventarse migraciones de verdad que Pronto no necesita.
Migration _createTable(int version, String table) => Migration(
      version: version,
      name: 'crea $table',
      apply: (db) => db.execute(
        'CREATE TABLE IF NOT EXISTS $table (id INTEGER PRIMARY KEY)',
      ),
    );

void main() {
  setUpAll(prepareSqliteForTests);

  late Database db;

  setUp(() => db = sqlite3.openInMemory());
  tearDown(() => db.dispose());

  group('instalación nueva', () {
    test('crea el esquema y deja la versión en la última migración', () {
      final r = migrateSchema(db);

      expect(r.freshInstall, isTrue);
      expect(r.fromVersion, 0);
      expect(r.toVersion, targetVersion(kProntoMigrations));
      expect(schemaVersion(db), targetVersion(kProntoMigrations));
    });

    test('las tres tablas de Pronto existen tras migrar', () {
      migrateSchema(db);
      expect(
        tableNames(db),
        containsAll(['corrections', 'vocab_terms', 'dictation_log']),
      );
    });
  });

  group('actualización desde una base de la 0.7.1 (el verde falso)', () {
    // Este grupo es la razón de ser del PRNT-20: arranca de un fichero REAL
    // de la 0.7.1, no de una base creada de cero. Sin esto, un
    // `CREATE TABLE IF NOT EXISTS` que no añade columnas a una tabla ya
    // existente pasa la suite en verde y revienta en el disco del usuario.
    late Directory tmp;
    late Database old;

    setUp(() {
      tmp = Directory.systemTemp.createTempSync('pronto-mig-');
      final source = File('test/fixtures/pronto-0.7.1.db');
      expect(
        source.existsSync(),
        isTrue,
        reason: 'falta test/fixtures/pronto-0.7.1.db '
            '(dart run tools/generate_071_fixture.dart)',
      );
      final copy = File('${tmp.path}/pronto.db');
      copy.writeAsBytesSync(source.readAsBytesSync());
      old = sqlite3.open(copy.path);
    });

    tearDown(() {
      old.dispose();
      tmp.deleteSync(recursive: true);
    });

    test('la base de la 0.7.1 llega sin versión de esquema y con tablas', () {
      expect(schemaVersion(old), 0);
      expect(databaseIsEmpty(old), isFalse);
      expect(hasPendingMigrations(old), isTrue);
    });

    test('migrar no la reconoce como instalación nueva', () {
      final r = migrateSchema(old);
      expect(r.freshInstall, isFalse);
      expect(r.toVersion, greaterThan(r.fromVersion));
      expect(r.applied, isNotEmpty);
    });

    test('no se pierde ni un dato al migrar', () {
      String rows(String sql) =>
          old.select(sql).map((f) => f.values.join('|')).join('\n');

      final beforeCorr = rows('SELECT * FROM corrections ORDER BY raw');
      final beforeVocab = rows('SELECT * FROM vocab_terms ORDER BY term');
      final beforeLog = rows('SELECT * FROM dictation_log ORDER BY id');

      migrateSchema(old);

      expect(rows('SELECT * FROM corrections ORDER BY raw'), beforeCorr);
      expect(rows('SELECT * FROM vocab_terms ORDER BY term'), beforeVocab);
      expect(rows('SELECT * FROM dictation_log ORDER BY id'), beforeLog);
      // Tildes y ñ del vocabulario propio sobreviven al viaje.
      expect(
        old
            .select("SELECT freq FROM vocab_terms WHERE term = 'Añoveros'")
            .first['freq'],
        3,
      );
    });

    test('una instalación nueva y una actualizada acaban en el mismo esquema',
        () {
      // Es el criterio de aceptación literal del ticket: el usuario que
      // actualiza tiene que quedar con lo mismo que el que instala hoy.
      migrateSchema(old);
      final fresh = sqlite3.openInMemory();
      addTearDown(fresh.dispose);
      migrateSchema(fresh);

      expect(dumpSchema(old), dumpSchema(fresh));
      expect(schemaVersion(old), schemaVersion(fresh));
    });
  });

  group('idempotencia', () {
    test('migrar dos veces deja el mismo esquema y no aplica nada la segunda',
        () {
      final first = migrateSchema(db);
      final schema = dumpSchema(db);

      final second = migrateSchema(db);

      expect(first.applied, isNotEmpty);
      expect(second.applied, isEmpty);
      expect(second.toVersion, first.toVersion);
      expect(dumpSchema(db), schema);
      // Estar al día NO es venir del futuro: confundirlas haría que Pronto
      // dejara de migrar en cuanto llegase la siguiente versión.
      expect(second.databaseNewerThanApp, isFalse);
    });

    test('para cualquier punto intermedio, terminar da el mismo resultado', () {
      // Invariante: da igual en qué versión te pille la actualización (0.6.0,
      // 0.7.1, a medio camino de una tanda de migraciones futuras); al acabar
      // todas, el esquema es el mismo. Se prueba con una lista sintética
      // porque hoy solo hay una migración de verdad.
      final list = [
        _createTable(1, 'uno'),
        _createTable(2, 'dos'),
        _createTable(3, 'tres'),
      ];

      final full = sqlite3.openInMemory();
      addTearDown(full.dispose);
      migrateSchema(full, migrations: list);
      final reference = dumpSchema(full);

      for (var cut = 0; cut <= list.length; cut++) {
        final partial = sqlite3.openInMemory();
        addTearDown(partial.dispose);
        migrateSchema(partial, migrations: list.sublist(0, cut));
        final r = migrateSchema(partial, migrations: list);

        expect(dumpSchema(partial), reference, reason: 'corte $cut');
        expect(schemaVersion(partial), 3, reason: 'corte $cut');
        expect(r.applied.length, list.length - cut, reason: 'corte $cut');
      }
    });
  });

  group('orden y numeración', () {
    test('las migraciones se aplican en orden', () {
      final order = <int>[];
      final list = [
        for (var i = 1; i <= 3; i++)
          Migration(
            version: i,
            name: 'm$i',
            apply: (db) {
              order.add(i);
              db.execute('CREATE TABLE IF NOT EXISTS t$i (id INTEGER)');
            },
          ),
      ];

      final r = migrateSchema(db, migrations: list);

      expect(order, [1, 2, 3]);
      expect(r.applied, ['m1', 'm2', 'm3']);
      expect(schemaVersion(db), 3);
    });

    test('una lista desordenada se rechaza en vez de aplicarse a lo loco', () {
      final list = [_createTable(2, 'dos'), _createTable(1, 'uno')];
      expect(
        () => migrateSchema(db, migrations: list),
        throwsA(isA<StateError>()),
      );
      expect(schemaVersion(db), 0);
    });

    test('un número saltado se rechaza (renumerar rompe bases ya migradas)',
        () {
      final list = [_createTable(1, 'uno'), _createTable(3, 'tres')];
      expect(
        () => migrateSchema(db, migrations: list),
        throwsA(isA<StateError>()),
      );
    });

    test('dos migraciones con el mismo número se rechazan', () {
      final list = [_createTable(1, 'uno'), _createTable(1, 'otra')];
      expect(
        () => migrateSchema(db, migrations: list),
        throwsA(isA<StateError>()),
      );
    });

    test('la lista real de Pronto está bien numerada', () {
      expect(() => migrateSchema(db), returnsNormally);
      expect(targetVersion(kProntoMigrations), kProntoMigrations.length);
    });
  });

  group('rotura', () {
    test('SQLite deshace el PRAGMA user_version con la transacción', () {
      // Es el supuesto sobre el que se apoya todo lo demás: si el bump de
      // versión NO viajara dentro de la transacción, una migración a medias
      // dejaría la base marcada como migrada sin estarlo, y el arranque
      // siguiente ya no la volvería a intentar.
      db.execute('PRAGMA user_version = 1');
      db.execute('BEGIN IMMEDIATE');
      db.execute('CREATE TABLE t (a INTEGER)');
      db.execute('PRAGMA user_version = 2');
      expect(schemaVersion(db), 2);

      db.execute('ROLLBACK');

      expect(schemaVersion(db), 1);
      expect(tableNames(db), isNot(contains('t')));
    });

    test('una migración que falla no deja la base a medias', () {
      // Simula que falla el disco DENTRO de la segunda migración: lo suyo se
      // deshace entero y la versión se queda en la 1.
      final list = [
        _createTable(1, 'uno'),
        Migration(
          version: 2,
          name: 'la que revienta',
          apply: (db) {
            db.execute('CREATE TABLE dos (id INTEGER)');
            throw StateError('disco lleno');
          },
        ),
        _createTable(3, 'tres'),
      ];

      expect(
        () => migrateSchema(db, migrations: list),
        throwsA(isA<StateError>()),
      );

      // La 1 quedó aplicada y confirmada; la 2 no dejó ni su tabla ni su
      // versión; la 3 ni se intentó.
      expect(schemaVersion(db), 1);
      expect(tableNames(db), contains('uno'));
      expect(tableNames(db), isNot(contains('dos')));
      expect(tableNames(db), isNot(contains('tres')));
    });

    test('en un FICHERO, la migración rota tampoco sobrevive al cierre', () {
      // El mismo caso pero con una base en disco que se cierra y se vuelve a
      // abrir: es lo que le pasa al usuario cuando el actualizador MATA la app
      // a mitad. Una base en memoria no demuestra esto.
      final tmp = Directory.systemTemp.createTempSync('pronto-mig-file-');
      addTearDown(() => tmp.deleteSync(recursive: true));
      final path = '${tmp.path}/pronto.db';

      final list = [
        _createTable(1, 'uno'),
        Migration(
          version: 2,
          name: 'la que revienta',
          apply: (db) {
            db.execute('CREATE TABLE dos (id INTEGER)');
            throw StateError('el proceso se fue');
          },
        ),
      ];

      final first = sqlite3.open(path);
      expect(
        () => migrateSchema(first, migrations: list),
        throwsA(isA<StateError>()),
      );
      first.dispose();

      final reopened = sqlite3.open(path);
      addTearDown(reopened.dispose);
      expect(schemaVersion(reopened), 1);
      expect(tableNames(reopened), contains('uno'));
      expect(tableNames(reopened), isNot(contains('dos')));
    });

    test('reintentar tras el fallo termina el trabajo', () {
      var fails = true;
      final list = [
        _createTable(1, 'uno'),
        Migration(
          version: 2,
          name: 'falla la primera vez',
          apply: (db) {
            db.execute('CREATE TABLE IF NOT EXISTS dos (id INTEGER)');
            if (fails) throw StateError('disco lleno');
          },
        ),
      ];

      expect(() => migrateSchema(db, migrations: list), throwsStateError);
      fails = false;
      final r = migrateSchema(db, migrations: list);

      expect(schemaVersion(db), 2);
      expect(r.fromVersion, 1);
      expect(r.applied, ['falla la primera vez']);
    });

    test('una lista vacía no toca nada', () {
      final r = migrateSchema(db, migrations: const []);
      expect(r.toVersion, 0);
      expect(r.applied, isEmpty);
      expect(databaseIsEmpty(db), isTrue);
    });

    test('una base más nueva que la app no se degrada', () {
      // El usuario instaló la 0.9, migró, y luego volvió a la 0.7.1.
      db.execute('CREATE TABLE corrections (raw TEXT)');
      db.execute('PRAGMA user_version = 7');

      final r = migrateSchema(db, migrations: [_createTable(1, 'uno')]);

      expect(r.databaseNewerThanApp, isTrue);
      expect(r.applied, isEmpty);
      expect(schemaVersion(db), 7);
      // No se ha creado la tabla de la migración vieja encima de la base nueva.
      expect(tableNames(db), isNot(contains('uno')));
    });

    test('hasPendingMigrations dice que no si la base va por delante', () {
      db.execute('PRAGMA user_version = 99');
      expect(hasPendingMigrations(db), isFalse);
    });
  });
}
