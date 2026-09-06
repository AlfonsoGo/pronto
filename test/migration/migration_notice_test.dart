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
/// enseñar ni descartar no es un aviso.
void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  ProviderContainer contenedor(InformeApertura? informe) {
    final c = ProviderContainer(
      overrides: [
        informeAperturaProvider.overrideWith((ref) async => informe),
      ],
    );
    addTearDown(c.dispose);
    return c;
  }

  const migrado = InformeApertura(
    versionInicial: 0,
    versionFinal: 1,
    aplicadas: ['esquema base'],
    rutaCopia:
        r'C:\Users\quien\AppData\Roaming\Pronto\copias\copia-2026-09-06-1432',
  );

  setUp(() => SharedPreferences.setMockInitialValues({}));

  group('cuándo hay aviso', () {
    test('una instalación nueva no avisa de nada', () async {
      final c = contenedor(
        const InformeApertura(
          versionInicial: 0,
          versionFinal: 1,
          instalacionNueva: true,
        ),
      );

      await c.read(avisoMigracionProvider.notifier).listo;

      expect(c.read(avisoMigracionProvider), isNull);
    });

    test('abrir sin cambios no avisa de nada', () async {
      final c = contenedor(
        const InformeApertura(versionInicial: 1, versionFinal: 1),
      );

      await c.read(avisoMigracionProvider.notifier).listo;

      expect(c.read(avisoMigracionProvider), isNull);
    });

    test('una migración de verdad avisa y dice dónde está la copia', () async {
      final c = contenedor(migrado);

      await c.read(avisoMigracionProvider.notifier).listo;

      final aviso = c.read(avisoMigracionProvider);
      expect(aviso, isNotNull);
      expect(aviso!.correcto, isTrue);
      expect(aviso.rutaCopia, migrado.rutaCopia);
      expect(aviso.versionInicial, 0);
      expect(aviso.versionFinal, 1);
    });

    test('si algo falló, el aviso lo dice y no lo disfraza de éxito', () async {
      final c = contenedor(
        const InformeApertura(
          versionInicial: 0,
          versionFinal: 0,
          errorCopia: 'no se pudo escribir en la carpeta de copias',
        ),
      );

      await c.read(avisoMigracionProvider.notifier).listo;

      final aviso = c.read(avisoMigracionProvider);
      expect(aviso, isNotNull);
      expect(aviso!.correcto, isFalse);
      expect(aviso.problema, contains('carpeta de copias'));
      expect(aviso.rutaCopia, isNull);
    });
  });

  group('sobrevive a cerrar la app', () {
    test('el aviso sigue ahí en el siguiente arranque', () async {
      final primero = contenedor(migrado);
      await primero.read(avisoMigracionProvider.notifier).listo;

      // Segundo arranque: ya no hay nada que migrar, pero el aviso de la vez
      // anterior sigue pendiente de leer.
      final segundo = contenedor(
        const InformeApertura(versionInicial: 1, versionFinal: 1),
      );
      await segundo.read(avisoMigracionProvider.notifier).listo;

      final aviso = segundo.read(avisoMigracionProvider);
      expect(aviso, isNotNull);
      expect(aviso!.rutaCopia, migrado.rutaCopia);
    });

    test('descartarlo lo borra para siempre', () async {
      final primero = contenedor(migrado);
      await primero.read(avisoMigracionProvider.notifier).listo;
      await primero.read(avisoMigracionProvider.notifier).descartar();

      expect(primero.read(avisoMigracionProvider), isNull);
      final prefs = await SharedPreferences.getInstance();
      expect(prefs.getString(AvisoMigracionController.claveAviso), isNull);

      final segundo = contenedor(
        const InformeApertura(versionInicial: 1, versionFinal: 1),
      );
      await segundo.read(avisoMigracionProvider.notifier).listo;
      expect(segundo.read(avisoMigracionProvider), isNull);
    });

    test('un aviso guardado corrupto no tumba el arranque', () async {
      SharedPreferences.setMockInitialValues({
        AvisoMigracionController.claveAviso: 'esto no es json',
      });

      final c = contenedor(
        const InformeApertura(versionInicial: 1, versionFinal: 1),
      );
      await c.read(avisoMigracionProvider.notifier).listo;

      expect(c.read(avisoMigracionProvider), isNull);
    });

    test('un aviso guardado sin versiones se descarta en vez de mentir',
        () async {
      SharedPreferences.setMockInitialValues({
        AvisoMigracionController.claveAviso:
            jsonEncode({'rutaCopia': 'C:/x', 'problema': null}),
      });

      final c = contenedor(
        const InformeApertura(versionInicial: 1, versionFinal: 1),
      );
      await c.read(avisoMigracionProvider.notifier).listo;

      expect(c.read(avisoMigracionProvider), isNull);
    });
  });

  group('lo que ve el usuario', () {
    Widget app(ProviderContainer c) => UncontrolledProviderScope(
          container: c,
          child: const MaterialApp(
            home:
                Scaffold(body: SingleChildScrollView(child: MigracionBanner())),
          ),
        );

    testWidgets('sin aviso, el banner no ocupa sitio', (tester) async {
      final c = contenedor(
        const InformeApertura(versionInicial: 1, versionFinal: 1),
      );
      await c.read(avisoMigracionProvider.notifier).listo;

      await tester.pumpWidget(app(c));

      expect(find.textContaining('Pronto ha actualizado'), findsNothing);
      expect(find.text('Entendido'), findsNothing);
    });

    testWidgets('con aviso, enseña la ruta de la copia y se puede descartar',
        (tester) async {
      final c = contenedor(migrado);
      await c.read(avisoMigracionProvider.notifier).listo;

      await tester.pumpWidget(app(c));

      expect(find.textContaining('ha actualizado el formato'), findsOneWidget);
      expect(find.text(migrado.rutaCopia!), findsOneWidget);

      await tester.tap(find.text('Entendido'));
      await tester.pumpAndSettle();

      expect(find.textContaining('ha actualizado el formato'), findsNothing);
      final prefs = await SharedPreferences.getInstance();
      expect(prefs.getString(AvisoMigracionController.claveAviso), isNull);
    });

    testWidgets('si falló, el aviso no dice que todo fue bien', (tester) async {
      final c = contenedor(
        const InformeApertura(
          versionInicial: 0,
          versionFinal: 0,
          errorCopia: 'disco lleno',
        ),
      );
      await c.read(avisoMigracionProvider.notifier).listo;

      await tester.pumpWidget(app(c));

      expect(find.textContaining('no ha tocado nada'), findsOneWidget);
      expect(find.textContaining('ha actualizado el formato'), findsNothing);
      expect(find.text('disco lleno'), findsOneWidget);
    });
  });
}
