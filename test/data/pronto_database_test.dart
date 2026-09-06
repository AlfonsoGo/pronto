import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;
import 'package:pronto/src/data/pronto_database.dart';
import 'package:pronto/src/data/schema_migrations.dart';
import 'package:sqlite3/sqlite3.dart';

import 'sqlite_para_tests.dart';

/// El ensayo de actualización, en test: se parte de una carpeta de datos como
/// la que tiene hoy quien instaló la 0.7.1 —base sin versionar, ajustes y el
/// modelo de 640 MB— y se abre con el código de hoy.
void main() {
  setUpAll(prepararSqliteParaTests);

  late Directory raiz;
  late Directory datos;
  late Directory copias;

  setUp(() {
    raiz = Directory.systemTemp.createTempSync('pronto-bd-');
    datos = Directory(p.join(raiz.path, 'Pronto'))..createSync(recursive: true);
    copias = Directory(p.join(raiz.path, kCarpetaCopias));
  });

  tearDown(() {
    if (raiz.existsSync()) raiz.deleteSync(recursive: true);
  });

  /// Deja en [datos] la foto de una instalación de la 0.7.1.
  void instalacionDe071() {
    final origen = File('test/fixtures/pronto-0.7.1.db');
    expect(
      origen.existsSync(),
      isTrue,
      reason: 'falta test/fixtures/pronto-0.7.1.db '
          '(dart run tools/generar_fixture_bd_071.dart)',
    );
    File(p.join(datos.path, kFicheroBd))
        .writeAsBytesSync(origen.readAsBytesSync());
    File(p.join(datos.path, 'shared_preferences.json')).writeAsStringSync(
      '{"flutter.app_settings":"{\\"formatPath\\":\\"none\\"}"}',
    );
    final modelo = Directory(p.join(datos.path, 'models', 'parakeet'))
      ..createSync(recursive: true);
    File(p.join(modelo.path, 'encoder.int8.onnx')).writeAsStringSync('x' * 500);
  }

  Future<AperturaBaseDeDatos> abrir({
    List<Migracion> migraciones = kMigracionesPronto,
    Directory? destinoCopias,
  }) =>
      abrirBaseDeDatos(
        soporte: datos,
        copiasRaiz: destinoCopias ?? copias,
        ahora: () => DateTime(2026, 9, 6, 14, 32),
        migraciones: migraciones,
      );

  List<String> nombresDeCopias() => copias.existsSync()
      ? copias.listSync().map((e) => p.basename(e.path)).toList()
      : <String>[];

  group('instalación nueva', () {
    test('no hace copia de nada ni cuenta nada al usuario', () async {
      final a = await abrir();
      addTearDown(a.db.dispose);

      expect(a.informe.instalacionNueva, isTrue);
      expect(a.informe.rutaCopia, isNull);
      expect(a.informe.mereceAviso, isFalse);
      expect(nombresDeCopias(), isEmpty);
      expect(versionEsquema(a.db), versionObjetivo(kMigracionesPronto));
    });
  });

  group('actualización de una instalación de la 0.7.1', () {
    test('copia los datos ANTES de migrar y dice dónde quedaron', () async {
      instalacionDe071();

      final a = await abrir();
      addTearDown(a.db.dispose);

      expect(a.informe.instalacionNueva, isFalse);
      expect(a.informe.huboActualizacion, isTrue);
      expect(a.informe.mereceAviso, isTrue);
      expect(a.informe.rutaCopia, isNotNull);

      final copia = Directory(a.informe.rutaCopia!);
      expect(copia.existsSync(), isTrue);
      expect(File(p.join(copia.path, kFicheroBd)).existsSync(), isTrue);
      expect(
        File(p.join(copia.path, 'shared_preferences.json')).existsSync(),
        isTrue,
      );
      // Los 640 MB del modelo no entran en la copia...
      expect(Directory(p.join(copia.path, 'models')).existsSync(), isFalse);
      // ...y siguen intactos donde estaban: nadie se baja el modelo otra vez.
      expect(
        File(p.join(datos.path, 'models', 'parakeet', 'encoder.int8.onnx'))
            .existsSync(),
        isTrue,
      );
    });

    test('la copia guarda la base TAL COMO ESTABA, sin versionar', () async {
      instalacionDe071();

      final a = await abrir();
      addTearDown(a.db.dispose);

      final copiada = sqlite3.open(
        p.join(a.informe.rutaCopia!, kFicheroBd),
      );
      addTearDown(copiada.dispose);
      // Si la copia se hubiera hecho DESPUÉS de migrar no serviría de vuelta
      // atrás: tendría ya la versión nueva.
      expect(versionEsquema(copiada), 0);
      expect(versionEsquema(a.db), versionObjetivo(kMigracionesPronto));
    });

    test('no se pierde ni un dato', () async {
      instalacionDe071();

      final a = await abrir();
      addTearDown(a.db.dispose);

      expect(
        a.db.select('SELECT count(*) AS n FROM corrections').first['n'],
        3,
      );
      expect(
        a.db.select('SELECT count(*) AS n FROM vocab_terms').first['n'],
        4,
      );
      expect(
        a.db.select('SELECT count(*) AS n FROM dictation_log').first['n'],
        2,
      );
      expect(
        a.db
            .select("SELECT freq FROM corrections WHERE raw = 'txonso'")
            .first['freq'],
        12,
      );
    });

    test('abrir otra vez no vuelve a copiar ni a avisar', () async {
      instalacionDe071();

      final primera = await abrir();
      primera.db.dispose();
      final copiasTrasLaPrimera = nombresDeCopias();

      final segunda = await abrir();
      addTearDown(segunda.db.dispose);

      expect(segunda.informe.mereceAviso, isFalse);
      expect(segunda.informe.rutaCopia, isNull);
      expect(nombresDeCopias(), copiasTrasLaPrimera);
    });

    test('acaba en el MISMO esquema que una instalación nueva', () async {
      instalacionDe071();
      final actualizada = await abrir();
      addTearDown(actualizada.db.dispose);

      final otraRaiz = Directory.systemTemp.createTempSync('pronto-bd-nueva-');
      addTearDown(() => otraRaiz.deleteSync(recursive: true));
      final nueva = await abrirBaseDeDatos(
        soporte: Directory(p.join(otraRaiz.path, 'Pronto')),
        copiasRaiz: Directory(p.join(otraRaiz.path, kCarpetaCopias)),
      );
      addTearDown(nueva.db.dispose);

      String esquema(Database db) => db
          .select(
            'SELECT type, name, sql FROM sqlite_master '
            "WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name",
          )
          .map((f) => '${f['type']}|${f['name']}|${f['sql']}')
          .join('\n');

      expect(esquema(actualizada.db), esquema(nueva.db));
      expect(versionEsquema(actualizada.db), versionEsquema(nueva.db));
    });
  });

  group('cuando algo sale mal', () {
    test('sin copia NO se migra, y se dice por qué', () async {
      instalacionDe071();

      // Carpeta de copias dentro de la de datos: la copia se rechaza.
      final a = await abrir(
        destinoCopias: Directory(p.join(datos.path, kCarpetaCopias)),
      );
      addTearDown(a.db.dispose);

      expect(a.informe.errorCopia, isNotNull);
      expect(a.informe.mereceAviso, isTrue);
      expect(a.informe.rutaCopia, isNull);
      // Lo importante: la base del usuario está EXACTAMENTE como estaba.
      expect(versionEsquema(a.db), 0);
      expect(
        a.db.select('SELECT count(*) AS n FROM corrections').first['n'],
        3,
      );
    });

    test('si la migración falla, la copia ya está hecha y la base intacta',
        () async {
      instalacionDe071();

      final a = await abrir(
        migraciones: [
          kMigracionesPronto.first,
          Migracion(
            version: 2,
            nombre: 'la que revienta',
            aplicar: (db) {
              db.execute('CREATE TABLE a_medias (id INTEGER)');
              throw StateError('disco lleno');
            },
          ),
        ],
      );
      addTearDown(a.db.dispose);

      expect(a.informe.errorMigracion, isNotNull);
      expect(a.informe.mereceAviso, isTrue);
      // La copia previa existe: hay camino de vuelta.
      expect(a.informe.rutaCopia, isNotNull);
      expect(Directory(a.informe.rutaCopia!).existsSync(), isTrue);
      // La 1 sí entró; la 2 no dejó ni su tabla ni su versión.
      expect(versionEsquema(a.db), 1);
      final tablas = a.db
          .select("SELECT name FROM sqlite_master WHERE type = 'table'")
          .map((f) => f['name'] as String)
          .toSet();
      expect(tablas, isNot(contains('a_medias')));
      expect(
        a.db.select('SELECT count(*) AS n FROM corrections').first['n'],
        3,
      );
    });

    test('una base más nueva que la app se abre igual, sin degradarla',
        () async {
      instalacionDe071();
      final futura = sqlite3.open(p.join(datos.path, kFicheroBd));
      futura.execute('PRAGMA user_version = 99');
      futura.dispose();

      final a = await abrir();
      addTearDown(a.db.dispose);

      expect(a.informe.baseMasNuevaQueLaApp, isTrue);
      expect(versionEsquema(a.db), 99);
      expect(a.informe.rutaCopia, isNull);
      expect(
        a.db.select('SELECT count(*) AS n FROM corrections').first['n'],
        3,
      );
    });
  });
}
