import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:pronto/src/data/pronto_database.dart';
import 'package:pronto/src/features/migration/migration_banner.dart';
import 'package:pronto/src/features/migration/migration_notice.dart';
import 'package:shared_preferences/shared_preferences.dart';

/// El camino completo del aviso: lo que pasó al abrir la base → el aviso
/// guardado → lo que ve el usuario → descartarlo.
///
/// Es el nivel de aceptación del PRNT-20: «que la app diga que ha migrado la
/// primera vez que abre, y dónde quedó la copia». Un aviso que no se puede
/// enseñar ni descartar no es un aviso — y uno que dice lo que no es, peor.
void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  ProviderContainer withReport(OpenReport? report) {
    final c = ProviderContainer(
      overrides: [
        openReportProvider.overrideWith((ref) async => report),
      ],
    );
    addTearDown(c.dispose);
    return c;
  }

  const backupPath =
      r'C:\Users\quien\AppData\Roaming\Pronto\copias\copia-2026-09-06-1432';

  const migrated = OpenReport(
    outcome: OpenOutcome.migrated,
    fromVersion: 0,
    toVersion: 1,
    backupPath: backupPath,
  );

  const nothing = OpenReport(
    outcome: OpenOutcome.nothingToSay,
    fromVersion: 1,
    toVersion: 1,
  );

  setUp(() => SharedPreferences.setMockInitialValues({}));

  group('cuándo hay aviso', () {
    test('una instalación nueva no avisa de nada', () async {
      final c = withReport(nothing);

      await c.read(migrationNoticeProvider.notifier).ready;

      expect(c.read(migrationNoticeProvider), isNull);
    });

    test('una migración de verdad avisa y dice dónde está la copia', () async {
      final c = withReport(migrated);

      await c.read(migrationNoticeProvider.notifier).ready;

      final notice = c.read(migrationNoticeProvider);
      expect(notice, isNotNull);
      expect(notice!.outcome, OpenOutcome.migrated);
      expect(notice.backupPath, backupPath);
      expect(notice.fromVersion, 0);
      expect(notice.toVersion, 1);
      expect(notice.technicalDetail, isNull);
    });

    test('si algo falló, el aviso lo dice y no lo disfraza de éxito', () async {
      final c = withReport(
        const OpenReport(
          outcome: OpenOutcome.untouchedNoBackup,
          technicalDetail: 'no se pudo escribir en la carpeta de copias',
        ),
      );

      await c.read(migrationNoticeProvider.notifier).ready;

      final notice = c.read(migrationNoticeProvider);
      expect(notice, isNotNull);
      expect(notice!.outcome, OpenOutcome.untouchedNoBackup);
      expect(notice.technicalDetail, contains('carpeta de copias'));
      expect(notice.backupPath, isNull);
    });

    test('volver a una versión anterior de Pronto también avisa', () async {
      final c = withReport(
        const OpenReport(
          outcome: OpenOutcome.databaseFromNewerVersion,
          fromVersion: 9,
          toVersion: 9,
        ),
      );

      await c.read(migrationNoticeProvider.notifier).ready;

      expect(
        c.read(migrationNoticeProvider)?.outcome,
        OpenOutcome.databaseFromNewerVersion,
      );
    });
  });

  group('sobrevive a cerrar la app', () {
    test('el aviso sigue ahí en el siguiente arranque', () async {
      final first = withReport(migrated);
      await first.read(migrationNoticeProvider.notifier).ready;

      // Segundo arranque: ya no hay nada que migrar, pero el aviso de la vez
      // anterior sigue pendiente de leer.
      final second = withReport(nothing);
      final notifier = second.read(migrationNoticeProvider.notifier);
      await notifier.ready;

      expect(second.read(migrationNoticeProvider)?.backupPath, backupPath);
      // Pero NO nació hoy: el panel no se abre solo otra vez.
      expect(notifier.bornThisLaunch, isFalse);
    });

    test('el aviso recién creado sí marca que nació en este arranque',
        () async {
      final c = withReport(migrated);
      final notifier = c.read(migrationNoticeProvider.notifier);

      await notifier.ready;

      expect(notifier.bornThisLaunch, isTrue);
    });

    test('descartarlo lo borra para siempre', () async {
      final first = withReport(migrated);
      await first.read(migrationNoticeProvider.notifier).ready;
      await first.read(migrationNoticeProvider.notifier).dismiss();

      expect(first.read(migrationNoticeProvider), isNull);
      final prefs = await SharedPreferences.getInstance();
      expect(prefs.getString(MigrationNoticeController.noticeKey), isNull);

      final second = withReport(nothing);
      await second.read(migrationNoticeProvider.notifier).ready;
      expect(second.read(migrationNoticeProvider), isNull);
    });

    test('un aviso guardado corrupto no tumba el arranque', () async {
      SharedPreferences.setMockInitialValues({
        MigrationNoticeController.noticeKey: 'esto no es json',
      });

      final c = withReport(nothing);
      await c.read(migrationNoticeProvider.notifier).ready;

      expect(c.read(migrationNoticeProvider), isNull);
    });

    test('un aviso guardado sin versiones se descarta en vez de mentir',
        () async {
      SharedPreferences.setMockInitialValues({
        MigrationNoticeController.noticeKey:
            jsonEncode({'outcome': 'migrated', 'backupPath': 'C:/x'}),
      });

      final c = withReport(nothing);
      await c.read(migrationNoticeProvider.notifier).ready;

      expect(c.read(migrationNoticeProvider), isNull);
    });

    test('un aviso guardado con un caso desconocido se descarta', () async {
      // Una versión futura podría añadir casos; una anterior no puede
      // inventarse qué significan.
      SharedPreferences.setMockInitialValues({
        MigrationNoticeController.noticeKey: jsonEncode({
          'outcome': 'algoQueNoExiste',
          'fromVersion': 0,
          'toVersion': 1,
        }),
      });

      final c = withReport(nothing);
      await c.read(migrationNoticeProvider.notifier).ready;

      expect(c.read(migrationNoticeProvider), isNull);
    });
  });

  group('lo que ve el usuario', () {
    Widget app(ProviderContainer c) => UncontrolledProviderScope(
          container: c,
          child: const MaterialApp(
            home: Scaffold(
              body: SingleChildScrollView(child: MigrationBanner()),
            ),
          ),
        );

    Future<void> pump(WidgetTester tester, OpenReport report) async {
      final c = withReport(report);
      await c.read(migrationNoticeProvider.notifier).ready;
      await tester.pumpWidget(app(c));
    }

    testWidgets('sin aviso, el banner no ocupa sitio', (tester) async {
      await pump(tester, nothing);

      expect(find.textContaining('Pronto'), findsNothing);
      expect(find.text('Entendido'), findsNothing);
    });

    testWidgets('con aviso, enseña la ruta de la copia y se puede descartar',
        (tester) async {
      await pump(tester, migrated);

      expect(find.textContaining('ha actualizado el formato'), findsOneWidget);
      expect(find.text(backupPath), findsOneWidget);

      await tester.tap(find.text('Entendido'));
      await tester.pumpAndSettle();

      expect(find.textContaining('ha actualizado el formato'), findsNothing);
      final prefs = await SharedPreferences.getInstance();
      expect(prefs.getString(MigrationNoticeController.noticeKey), isNull);
    });

    testWidgets('si no se pudo copiar, dice que no se ha tocado nada',
        (tester) async {
      await pump(
        tester,
        const OpenReport(
          outcome: OpenOutcome.untouchedNoBackup,
          technicalDetail: 'disco lleno',
        ),
      );

      expect(find.textContaining('no ha tocado nada'), findsOneWidget);
      expect(find.textContaining('ha actualizado el formato'), findsNothing);
      // El texto crudo de la excepción va etiquetado como lo que es, no como
      // si fuera Pronto hablando.
      expect(find.text('Detalle técnico del fallo:'), findsOneWidget);
      expect(find.text('disco lleno'), findsOneWidget);
    });

    testWidgets('una migración A MEDIAS no dice que no se tocó nada',
        (tester) async {
      await pump(
        tester,
        const OpenReport(
          outcome: OpenOutcome.migratedThenFailed,
          fromVersion: 0,
          toVersion: 1,
          backupPath: backupPath,
          technicalDetail: 'disco lleno',
        ),
      );

      expect(find.textContaining('no ha tocado nada'), findsNothing);
      expect(find.textContaining('a medias'), findsOneWidget);
      expect(find.text(backupPath), findsOneWidget);
    });

    testWidgets('volver atrás avisa de que la base viene del futuro',
        (tester) async {
      await pump(
        tester,
        const OpenReport(
          outcome: OpenOutcome.databaseFromNewerVersion,
          fromVersion: 9,
          toVersion: 9,
        ),
      );

      expect(
        find.textContaining('versión más nueva de Pronto'),
        findsOneWidget,
      );
      expect(find.textContaining('ha actualizado el formato'), findsNothing);
    });

    testWidgets('si no se pudo abrir la base, se dice que no va a aprender',
        (tester) async {
      await pump(
        tester,
        const OpenReport(
          outcome: OpenOutcome.couldNotOpen,
          technicalDetail: 'database is locked',
        ),
      );

      expect(find.textContaining('no ha podido abrir'), findsOneWidget);
      expect(find.text('database is locked'), findsOneWidget);
    });
  });
}
