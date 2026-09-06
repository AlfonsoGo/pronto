import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:pronto/src/data/schema_migrations.dart';
import 'package:sqlite3/sqlite3.dart';

import 'sqlite_para_tests.dart';

/// Migraciones de juguete para probar la MAQUINARIA (orden, transacción,
/// idempotencia) sin inventarse migraciones de verdad que Pronto no necesita.
Migracion _crearTabla(int version, String tabla) => Migracion(
      version: version,
      nombre: 'crea $tabla',
      aplicar: (db) => db.execute(
        'CREATE TABLE IF NOT EXISTS $tabla (id INTEGER PRIMARY KEY)',
      ),
    );

/// Vuelca el esquema completo, ordenado, para comparar dos bases campo a campo.
String _volcarEsquema(Database db) {
  final filas = db.select(
    'SELECT type, name, sql FROM sqlite_master '
    "WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name",
  );
  final partes = <String>[];
  for (final f in filas) {
    partes.add('${f['type']}|${f['name']}|${f['sql']}');
    if (f['type'] == 'table') {
      for (final c in db.select('PRAGMA table_info(${f['name']})')) {
        partes.add('  col|${c['name']}|${c['type']}|${c['notnull']}|'
            '${c['dflt_value']}|${c['pk']}');
      }
    }
  }
  return partes.join('\n');
}

void main() {
  setUpAll(prepararSqliteParaTests);

  late Database db;

  setUp(() => db = sqlite3.openInMemory());
  tearDown(() => db.dispose());

  group('instalación nueva', () {
    test('crea el esquema y deja la versión en la última migración', () {
      final r = migrarEsquema(db);

      expect(r.instalacionNueva, isTrue);
      expect(r.versionInicial, 0);
      expect(r.versionFinal, versionObjetivo(kMigracionesPronto));
      expect(versionEsquema(db), versionObjetivo(kMigracionesPronto));
      // Una instalación nueva NO ha migrado nada de nadie: no hay aviso.
      expect(r.huboActualizacion, isFalse);
    });

    test('las tres tablas de Pronto existen tras migrar', () {
      migrarEsquema(db);
      final tablas = db
          .select("SELECT name FROM sqlite_master WHERE type = 'table'")
          .map((f) => f['name'] as String)
          .toSet();
      expect(
        tablas,
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
    late Database vieja;

    setUp(() {
      tmp = Directory.systemTemp.createTempSync('pronto-mig-');
      final origen = File('test/fixtures/pronto-0.7.1.db');
      expect(
        origen.existsSync(),
        isTrue,
        reason: 'falta test/fixtures/pronto-0.7.1.db '
            '(dart run tools/generar_fixture_bd_071.dart)',
      );
      final copia = File('${tmp.path}/pronto.db');
      copia.writeAsBytesSync(origen.readAsBytesSync());
      vieja = sqlite3.open(copia.path);
    });

    tearDown(() {
      vieja.dispose();
      tmp.deleteSync(recursive: true);
    });

    test('la base de la 0.7.1 llega sin versión de esquema y con tablas', () {
      expect(versionEsquema(vieja), 0);
      expect(baseVacia(vieja), isFalse);
      expect(hayMigracionesPendientes(vieja), isTrue);
    });

    test('migrar no la reconoce como instalación nueva', () {
      final r = migrarEsquema(vieja);
      expect(r.instalacionNueva, isFalse);
      expect(r.huboActualizacion, isTrue);
      expect(r.aplicadas, isNotEmpty);
    });

    test('no se pierde ni un dato al migrar', () {
      final antesCorr = vieja.select('SELECT * FROM corrections ORDER BY raw');
      final antesVocab =
          vieja.select('SELECT * FROM vocab_terms ORDER BY term');
      final antesLog = vieja.select('SELECT * FROM dictation_log ORDER BY id');

      migrarEsquema(vieja);

      expect(
        vieja
            .select('SELECT * FROM corrections ORDER BY raw')
            .map((f) => f.values.join('|')),
        antesCorr.map((f) => f.values.join('|')),
      );
      expect(
        vieja
            .select('SELECT * FROM vocab_terms ORDER BY term')
            .map((f) => f.values.join('|')),
        antesVocab.map((f) => f.values.join('|')),
      );
      expect(
        vieja
            .select('SELECT * FROM dictation_log ORDER BY id')
            .map((f) => f.values.join('|')),
        antesLog.map((f) => f.values.join('|')),
      );
      // Tildes y ñ del vocabulario propio sobreviven al viaje.
      expect(
        vieja
            .select("SELECT freq FROM vocab_terms WHERE term = 'Añoveros'")
            .first['freq'],
        3,
      );
    });

    test('una instalación nueva y una actualizada acaban en el mismo esquema',
        () {
      // Es el criterio de aceptación literal del ticket: el usuario que
      // actualiza tiene que quedar con lo mismo que el que instala hoy.
      migrarEsquema(vieja);
      final nueva = sqlite3.openInMemory();
      addTearDown(nueva.dispose);
      migrarEsquema(nueva);

      expect(_volcarEsquema(vieja), _volcarEsquema(nueva));
      expect(versionEsquema(vieja), versionEsquema(nueva));
    });
  });

  group('idempotencia', () {
    test('migrar dos veces deja el mismo esquema y no aplica nada la segunda',
        () {
      final primera = migrarEsquema(db);
      final esquema = _volcarEsquema(db);

      final segunda = migrarEsquema(db);

      expect(primera.aplicadas, isNotEmpty);
      expect(segunda.aplicadas, isEmpty);
      expect(segunda.versionFinal, primera.versionFinal);
      // Estar al día NO es venir del futuro: confundirlas haría que Pronto
      // dejara de migrar en cuanto llegase la siguiente versión.
      expect(segunda.baseMasNuevaQueLaApp, isFalse);
      expect(_volcarEsquema(db), esquema);
    });

    test('para cualquier punto intermedio, terminar da el mismo resultado', () {
      // Invariante: da igual en qué versión te pille la actualización (0.6.0,
      // 0.7.1, a medio camino de una tanda de migraciones futuras); al acabar
      // todas, el esquema es el mismo. Se prueba con una lista sintética
      // porque hoy solo hay una migración de verdad.
      final lista = [
        _crearTabla(1, 'uno'),
        _crearTabla(2, 'dos'),
        _crearTabla(3, 'tres'),
      ];

      final completa = sqlite3.openInMemory();
      addTearDown(completa.dispose);
      migrarEsquema(completa, migraciones: lista);
      final referencia = _volcarEsquema(completa);

      for (var corte = 0; corte <= lista.length; corte++) {
        final parcial = sqlite3.openInMemory();
        addTearDown(parcial.dispose);
        migrarEsquema(parcial, migraciones: lista.sublist(0, corte));
        final r = migrarEsquema(parcial, migraciones: lista);

        expect(_volcarEsquema(parcial), referencia, reason: 'corte $corte');
        expect(versionEsquema(parcial), 3, reason: 'corte $corte');
        expect(
          r.aplicadas.length,
          lista.length - corte,
          reason: 'corte $corte',
        );
      }
    });

    test('anadirColumnaSiFalta añade una vez y calla la segunda', () {
      db.execute('CREATE TABLE t (a INTEGER)');

      anadirColumnaSiFalta(db, 't', 'b', 'TEXT');
      anadirColumnaSiFalta(db, 't', 'b', 'TEXT');

      final cols = db
          .select('PRAGMA table_info(t)')
          .map((f) => f['name'] as String)
          .toList();
      expect(cols, ['a', 'b']);
    });

    test('anadirColumnaSiFalta no pisa los datos de la columna que ya está',
        () {
      db.execute('CREATE TABLE t (a INTEGER, b TEXT)');
      db.execute("INSERT INTO t VALUES (1, 'valor del usuario')");

      anadirColumnaSiFalta(db, 't', 'b', 'TEXT');

      expect(db.select('SELECT b FROM t').first['b'], 'valor del usuario');
    });
  });

  group('orden y numeración', () {
    test('las migraciones se aplican en orden', () {
      final orden = <int>[];
      final lista = [
        for (var i = 1; i <= 3; i++)
          Migracion(
            version: i,
            nombre: 'm$i',
            aplicar: (db) {
              orden.add(i);
              db.execute('CREATE TABLE IF NOT EXISTS t$i (id INTEGER)');
            },
          ),
      ];

      final r = migrarEsquema(db, migraciones: lista);

      expect(orden, [1, 2, 3]);
      expect(r.aplicadas, ['m1', 'm2', 'm3']);
      expect(versionEsquema(db), 3);
    });

    test('una lista desordenada se rechaza en vez de aplicarse a lo loco', () {
      final lista = [_crearTabla(2, 'dos'), _crearTabla(1, 'uno')];
      expect(
        () => migrarEsquema(db, migraciones: lista),
        throwsA(isA<StateError>()),
      );
      expect(versionEsquema(db), 0);
    });

    test('un número saltado se rechaza (renumerar rompe bases ya migradas)',
        () {
      final lista = [_crearTabla(1, 'uno'), _crearTabla(3, 'tres')];
      expect(
        () => migrarEsquema(db, migraciones: lista),
        throwsA(isA<StateError>()),
      );
    });

    test('dos migraciones con el mismo número se rechazan', () {
      final lista = [_crearTabla(1, 'uno'), _crearTabla(1, 'otra')];
      expect(
        () => migrarEsquema(db, migraciones: lista),
        throwsA(isA<StateError>()),
      );
    });

    test('la lista real de Pronto está bien numerada', () {
      expect(() => migrarEsquema(db), returnsNormally);
      expect(versionObjetivo(kMigracionesPronto), kMigracionesPronto.length);
    });
  });

  group('rotura', () {
    test('una migración que falla no deja la base a medias', () {
      // Simula que matan el proceso (o falla el disco) DENTRO de la segunda
      // migración: lo suyo se deshace entero y la versión se queda en la 1.
      final lista = [
        _crearTabla(1, 'uno'),
        Migracion(
          version: 2,
          nombre: 'la que revienta',
          aplicar: (db) {
            db.execute('CREATE TABLE dos (id INTEGER)');
            throw StateError('disco lleno');
          },
        ),
        _crearTabla(3, 'tres'),
      ];

      expect(
        () => migrarEsquema(db, migraciones: lista),
        throwsA(isA<StateError>()),
      );

      // La 1 quedó aplicada y confirmada; la 2 no dejó ni su tabla ni su
      // versión; la 3 ni se intentó.
      expect(versionEsquema(db), 1);
      final tablas = db
          .select("SELECT name FROM sqlite_master WHERE type = 'table'")
          .map((f) => f['name'] as String)
          .toSet();
      expect(tablas, contains('uno'));
      expect(tablas, isNot(contains('dos')));
      expect(tablas, isNot(contains('tres')));
    });

    test('reintentar tras el fallo termina el trabajo', () {
      var falla = true;
      final lista = [
        _crearTabla(1, 'uno'),
        Migracion(
          version: 2,
          nombre: 'falla la primera vez',
          aplicar: (db) {
            db.execute('CREATE TABLE IF NOT EXISTS dos (id INTEGER)');
            if (falla) throw StateError('disco lleno');
          },
        ),
      ];

      expect(() => migrarEsquema(db, migraciones: lista), throwsStateError);
      falla = false;
      final r = migrarEsquema(db, migraciones: lista);

      expect(versionEsquema(db), 2);
      expect(r.versionInicial, 1);
      expect(r.aplicadas, ['falla la primera vez']);
    });

    test('una lista vacía no toca nada', () {
      final r = migrarEsquema(db, migraciones: const []);
      expect(r.versionFinal, 0);
      expect(r.aplicadas, isEmpty);
      expect(baseVacia(db), isTrue);
    });

    test('una base más nueva que la app no se degrada', () {
      // El usuario instaló la 0.9, migró, y luego volvió a la 0.7.1.
      db.execute('CREATE TABLE corrections (raw TEXT)');
      db.execute('PRAGMA user_version = 7');

      final r = migrarEsquema(db, migraciones: [_crearTabla(1, 'uno')]);

      expect(r.baseMasNuevaQueLaApp, isTrue);
      expect(r.aplicadas, isEmpty);
      expect(versionEsquema(db), 7);
      expect(r.huboActualizacion, isFalse);
      // No se ha creado la tabla de la migración vieja encima de la base nueva.
      final tablas = db
          .select("SELECT name FROM sqlite_master WHERE type = 'table'")
          .map((f) => f['name'] as String)
          .toSet();
      expect(tablas, isNot(contains('uno')));
    });

    test('hayMigracionesPendientes dice que no si la base va por delante', () {
      db.execute('PRAGMA user_version = 99');
      expect(hayMigracionesPendientes(db), isFalse);
    });
  });
}
