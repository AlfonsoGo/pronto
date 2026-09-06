import 'package:flutter_test/flutter_test.dart';
import 'package:pronto/src/data/schema_migrations.dart';
import 'package:pronto/src/data/sql_functions.dart';
import 'package:pronto/src/data/sqlite_history_repository.dart';
import 'package:pronto/src/features/history/dictation_entry.dart';
import 'package:pronto/src/features/history/history_repository.dart';
import 'package:sqlite3/sqlite3.dart';

import '../data/sqlite_for_tests.dart';

void main() {
  setUpAll(prepareSqliteForTests);

  late Database db;
  late SqliteHistoryRepository repo;

  DateTime at(int day, [int hour = 12]) => DateTime(2026, 9, day, hour);

  Future<void> log(
    String raw, {
    String? finalText,
    DateTime? when,
    String? app,
    String? mode,
    String? notice,
    DictationSource source = DictationSource.dictado,
    double? costUsd,
    Duration spoken = const Duration(seconds: 5),
  }) =>
      repo.log(
        DictationRecord(
          raw: raw,
          finalText: finalText ?? raw,
          at: when ?? at(6),
          source: source,
          appName: app,
          mode: mode,
          formatNotice: notice,
          costUsd: costUsd,
          spoken: spoken,
          transcribe: const Duration(milliseconds: 300),
          format: const Duration(milliseconds: 400),
          engine: 'parakeet',
          formatPath: 'local',
          formatModel: 'qwen2.5:7b',
          avgLogProb: -0.2,
        ),
      );

  setUp(() {
    db = sqlite3.openInMemory();
    registerProntoFunctions(db);
    migrateSchema(db);
    repo = SqliteHistoryRepository(() async => db);
  });

  tearDown(() => db.dispose());

  group('guardar y leer', () {
    test('un dictado vuelve con todos sus metadatos', () async {
      await log('oye celia', finalText: 'Oye, Celia.', app: 'Outlook');

      final e = (await repo.page(const HistoryQuery())).single;
      expect(e.raw, 'oye celia');
      expect(e.finalText, 'Oye, Celia.');
      expect(e.appName, 'Outlook');
      expect(e.engine, 'parakeet');
      expect(e.formatModel, 'qwen2.5:7b');
      expect(e.transcribe, const Duration(milliseconds: 300));
      expect(e.format, const Duration(milliseconds: 400));
      expect(e.source, DictationSource.dictado);
    });

    test('la lista sale de la más nueva a la más vieja', () async {
      await log('primera', when: at(1));
      await log('tercera', when: at(3));
      await log('segunda', when: at(2));

      final l = await repo.page(const HistoryQuery());
      expect(l.map((e) => e.raw), ['tercera', 'segunda', 'primera']);
    });
  });

  group('la prueba del asistente NO es historial', () {
    test('un dictado de prueba no aparece en la lista', () async {
      await log('dictado de verdad');
      await log('probando el micro', source: DictationSource.prueba);

      final l = await repo.page(const HistoryQuery());
      expect(l.map((e) => e.raw), ['dictado de verdad']);
      expect(await repo.count(const HistoryQuery()), 1);
    });

    test('tampoco cuenta en las estadísticas', () async {
      await log('una palabra');
      await log('esto no cuenta nada de nada', source: DictationSource.prueba);

      final s = await repo.stats(const HistoryQuery());
      expect(s.total, 1);
      expect(s.words, 2);
    });
  });

  group('búsqueda', () {
    test('encuentra sin tildes lo que se escribió con tildes', () async {
      await log('resumen de la reunión con Serviform');
      await log('nada que ver');

      final l = await repo.page(const HistoryQuery(text: 'reunion'));
      expect(l.single.raw, contains('reunión'));
    });

    test('no distingue mayúsculas ni en las acentuadas', () async {
      await log('ÓRDENES para el taller');

      expect((await repo.page(const HistoryQuery(text: 'ordenes'))).length, 1);
    });

    test('la ñ se pliega a n, como en cualquier buscador', () async {
      await log('el año pasado con Ñoño');

      expect((await repo.page(const HistoryQuery(text: 'ano'))).length, 1);
      expect((await repo.page(const HistoryQuery(text: 'nono'))).length, 1);
    });

    test('busca también en el texto final y en la app', () async {
      await log('crudo sin nada', finalText: 'Presupuesto enviado.');
      await log('otra cosa', app: 'Outlook');

      expect(
        (await repo.page(const HistoryQuery(text: 'presupuesto'))).single.raw,
        'crudo sin nada',
      );
      expect(
        (await repo.page(const HistoryQuery(text: 'outlook'))).single.raw,
        'otra cosa',
      );
    });
  });

  group('lo que el usuario escribe en el buscador es TEXTO, no SQL', () {
    test(r'el % no es un comodín: busca un porcentaje', () async {
      await log('subimos un 20% las ventas');
      await log('nada de nada');

      // Sin escapar, '%' encontraría las dos: el patrón sería '%%%'.
      final l = await repo.page(const HistoryQuery(text: '%'));
      expect(l, hasLength(1));
      expect(l.single.raw, contains('20%'));
    });

    test('el guion bajo no vale por cualquier letra', () async {
      await log('el fichero se llama informe_final');
      await log('abc');

      // Sin escapar, '_' casaría con cualquier carácter y traería las dos.
      final l = await repo.page(const HistoryQuery(text: '_'));
      expect(l, hasLength(1));
      expect(l.single.raw, contains('informe_final'));
    });

    test('la barra invertida se busca tal cual, no como escape', () async {
      await log(r'la ruta es C:\Pronto');
      await log('otra cosa sin rutas');

      // La barra es el carácter de ESCAPE del LIKE. Si no se duplica al
      // construir el patrón queda un escape colgando: SQLite se queja, o casa
      // cualquier cosa.
      final l = await repo.page(const HistoryQuery(text: r'\'));
      expect(l, hasLength(1));
      expect(l.single.raw, contains(r'\'));
    });

    test('una comilla no rompe la consulta', () async {
      await log("dijo 'vale' y colgó");

      final l = await repo.page(const HistoryQuery(text: "'vale'"));
      expect(l, hasLength(1));
    });
  });

  group('filtros', () {
    test('por rango de fechas, con el día final incluido', () async {
      await log('uno', when: at(1));
      await log('cinco', when: at(5, 23));
      await log('nueve', when: at(9));

      final l = await repo.page(
        HistoryQuery(from: at(1, 0), to: DateTime(2026, 9, 5, 23, 59, 59)),
      );
      expect(l.map((e) => e.raw), ['cinco', 'uno']);
    });

    test('por Modo', () async {
      await log('correo', mode: 'Correo');
      await log('nota', mode: 'Nota');

      final l = await repo.page(const HistoryQuery(mode: 'Correo'));
      expect(l.single.raw, 'correo');
    });

    test('solo los que fallaron al formatear', () async {
      await log('bien');
      await log('mal', notice: 'Sin formatear: tope de gasto alcanzado');

      final l = await repo.page(const HistoryQuery(onlyFailed: true));
      expect(l.single.raw, 'mal');
      expect(l.single.failed, isTrue);
    });

    test('los Modos que existen de verdad, para el desplegable', () async {
      await log('a', mode: 'Correo');
      await log('b', mode: 'Nota');
      await log('c', mode: 'Correo');
      await log('d');

      expect(await repo.distinctModes(), ['Correo', 'Nota']);
    });
  });

  group('estadísticas', () {
    test('cuenta dictados, palabras, coste y reparto por día y app', () async {
      await log('una dos tres', when: at(1), app: 'Outlook', costUsd: 0.001);
      await log('cuatro cinco', when: at(1), app: 'Outlook');
      await log('seis', when: at(2), app: 'Chrome', costUsd: 0.002);

      final s = await repo.stats(const HistoryQuery());
      expect(s.total, 3);
      expect(s.words, 6);
      expect(s.costUsd, closeTo(0.003, 1e-9));
      expect(s.perDay['2026-09-01'], 2);
      expect(s.perDay['2026-09-02'], 1);
      expect(s.perApp['Outlook'], 2);
      expect(s.perApp['Chrome'], 1);
    });

    test('las palabras se cuentan del texto que se pegó, no del crudo',
        () async {
      await log('una dos tres cuatro', finalText: 'Una, dos.');

      expect((await repo.stats(const HistoryQuery())).words, 2);
    });
  });

  group('borrar', () {
    test('una entrada desaparece de SQLite, no solo de la pantalla', () async {
      await log('se queda');
      await log('se va');
      final victima = (await repo.page(const HistoryQuery()))
          .firstWhere((e) => e.raw == 'se va');

      await repo.deleteEntry(victima.id);

      expect(db.select('SELECT count(*) n FROM dictation_log').first['n'], 1);
    });

    test('borrar todo vacía la tabla de verdad', () async {
      await log('uno');
      await log('dos');
      await log('prueba', source: DictationSource.prueba);

      final borradas = await repo.deleteAll();

      expect(borradas, 3);
      expect(db.select('SELECT count(*) n FROM dictation_log').first['n'], 0);
    });

    test('la retención borra lo viejo y respeta lo reciente', () async {
      await log('antiguo', when: at(1));
      await log('reciente', when: at(20));

      final borradas = await repo.purgeOlderThan(
        const Duration(days: 7),
        now: at(21),
      );

      expect(borradas, 1);
      expect((await repo.page(const HistoryQuery())).single.raw, 'reciente');
    });

    test('retención de 0 días = no se borra nada (nunca por accidente)',
        () async {
      await log('antiguo', when: at(1));

      expect(await repo.purgeOlderThan(Duration.zero, now: at(21)), 0);
      expect(await repo.count(const HistoryQuery()), 1);
    });
  });

  group('corregir a mano', () {
    test('cambia el texto final de esa entrada y nada más', () async {
      await log('reunion con serviform', finalText: 'Reunión con Serviform');
      final e = (await repo.page(const HistoryQuery())).single;

      await repo.updateFinalText(e.id, 'Reunión con Serviforn');

      final tras = (await repo.page(const HistoryQuery())).single;
      expect(tras.finalText, 'Reunión con Serviforn');
      expect(tras.raw, 'reunion con serviform');
    });
  });

  group('paginación', () {
    test('con 5.000 entradas la primera página no lee la tabla entera',
        () async {
      final stmt = db.prepare(
        'INSERT INTO dictation_log (raw, final, ts, source) '
        'VALUES (?, ?, ?, ?)',
      );
      for (var i = 0; i < 5000; i++) {
        stmt.execute(['crudo $i', 'Final $i', 1000000 + i, 'dictado']);
      }
      stmt.dispose();

      final reloj = Stopwatch()..start();
      final primera = await repo.page(const HistoryQuery(), limit: 50);
      reloj.stop();

      expect(primera.length, 50);
      expect(primera.first.finalText, 'Final 4999');
      expect(
        reloj.elapsedMilliseconds,
        lessThan(120),
        reason: 'la primera página tardó ${reloj.elapsedMilliseconds} ms',
      );
    });

    test('con 5.000 entradas, buscar y sumar las estadísticas no se atasca',
        () async {
      final stmt = db.prepare(
        'INSERT INTO dictation_log (raw, final, ts, source, spoken_ms) '
        'VALUES (?, ?, ?, ?, ?)',
      );
      for (var i = 0; i < 5000; i++) {
        stmt.execute([
          'la reunión número $i con el cliente',
          'La reunión número $i con el cliente.',
          1000000 + i,
          'dictado',
          4000,
        ]);
      }
      stmt.dispose();

      // La búsqueda pasa `pronto_fold` (una función Dart) por cada fila, y las
      // estadísticas traen todas las filas del filtro para contar palabras.
      // Las dos son las candidatas a bloquear la interfaz: se miden.
      final buscar = Stopwatch()..start();
      final encontradas = await repo.page(
        const HistoryQuery(text: 'reunion'),
        limit: 50,
      );
      buscar.stop();

      final sumar = Stopwatch()..start();
      final s = await repo.stats(const HistoryQuery());
      sumar.stop();

      expect(encontradas, hasLength(50));
      expect(s.total, 5000);
      expect(
        buscar.elapsedMilliseconds,
        lessThan(600),
        reason: 'buscar tardó ${buscar.elapsedMilliseconds} ms',
      );
      expect(
        sumar.elapsedMilliseconds,
        lessThan(600),
        reason: 'las estadísticas tardaron ${sumar.elapsedMilliseconds} ms',
      );
    });

    test('el desplazamiento continúa donde acabó la página anterior', () async {
      for (var i = 0; i < 5; i++) {
        await log('dictado $i', when: at(1, i));
      }

      final p2 = await repo.page(const HistoryQuery(), limit: 2, offset: 2);
      expect(p2.map((e) => e.raw), ['dictado 2', 'dictado 1']);
    });
  });
}
