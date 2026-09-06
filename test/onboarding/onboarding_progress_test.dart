import 'dart:convert';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:pronto/src/features/onboarding/onboarding_progress.dart';
import 'package:shared_preferences/shared_preferences.dart';

// El avance del asistente de instalación (PRNT-13).
//
// 🔴 El test que importa de verdad es el de la ACTUALIZACIÓN (PRNT-20): Pronto
// se actualiza solo, así que la v0.9 aterriza en equipos donde el dictado ya
// funcionaba. Si el asistente se abriera ahí, a esa gente le secuestraríamos la
// app para configurar lo que ya estaba configurado.

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  Future<OnboardingController> arrancar(
    Map<String, Object> prefsIniciales,
  ) async {
    SharedPreferences.setMockInitialValues(prefsIniciales);
    final container = ProviderContainer();
    addTearDown(container.dispose);
    final controller = container.read(onboardingProvider.notifier);
    await controller.loaded;
    return controller;
  }

  Future<Map<String, dynamic>?> guardado() async {
    final prefs = await SharedPreferences.getInstance();
    final crudo = prefs.getString(OnboardingController.prefsKey);
    if (crudo == null) return null;
    return jsonDecode(crudo) as Map<String, dynamic>;
  }

  group('instalación nueva', () {
    test('sin nada en disco, el asistente está sin terminar', () async {
      final c = await arrancar({});
      expect(c.state.terminado, isFalse);
      expect(c.state.idiomaElegido, isFalse);
      expect(c.state.pruebaHecha, isFalse);
      expect(c.state.formateoElegido, isFalse);
    });
  });

  group('actualización sobre una instalación que YA funcionaba (PRNT-20)', () {
    test('nace terminado: no se le abre el asistente a quien ya dictaba',
        () async {
      final c = await arrancar({
        OnboardingController.legacyReadyOnceKey: true,
      });
      expect(c.state.terminado, isTrue);
    });

    test('la decisión se GUARDA, no se vuelve a deducir cada arranque',
        () async {
      await arrancar({OnboardingController.legacyReadyOnceKey: true});
      expect((await guardado())?['terminado'], isTrue);
    });

    test('la marca vieja en false NO cuenta como instalación buena', () async {
      final c = await arrancar({
        OnboardingController.legacyReadyOnceKey: false,
      });
      expect(c.state.terminado, isFalse);
    });
  });

  group('reanudar', () {
    test('se recupera el avance a medias tal cual se dejó', () async {
      const aMedias = OnboardingProgress(
        idiomaElegido: true,
        pruebaHecha: true,
        preferenciaMotor: 0.8,
      );
      final c = await arrancar({
        OnboardingController.prefsKey: jsonEncode(aMedias.toJson()),
      });
      expect(c.state, aMedias);
      expect(c.state.terminado, isFalse);
    });

    test('un avance guardado MANDA sobre la huella de instalación vieja',
        () async {
      // Alguien que pulsó «Repetir la instalación» tiene las dos cosas: el
      // avance nuevo a cero y la marca vieja a true. Gana el avance.
      final c = await arrancar({
        OnboardingController.legacyReadyOnceKey: true,
        OnboardingController.prefsKey:
            jsonEncode(const OnboardingProgress().toJson()),
      });
      expect(c.state.terminado, isFalse);
    });

    test('cada marca se persiste en el acto, no al salir', () async {
      final c = await arrancar({});
      await c.marcarIdiomaElegido();
      expect((await guardado())?['idiomaElegido'], isTrue);
      await c.marcarPruebaHecha();
      expect((await guardado())?['pruebaHecha'], isTrue);
      await c.marcarFormateoElegido();
      expect((await guardado())?['formateoElegido'], isTrue);
    });
  });

  group('JSON corrupto o de otra versión', () {
    test('un JSON que no es un objeto deja el estado inicial, sin reventar',
        () async {
      final c = await arrancar({OnboardingController.prefsKey: '[1,2,3]'});
      expect(c.state.terminado, isFalse);
    });

    test('texto que no es JSON deja el estado inicial, sin reventar', () async {
      final c = await arrancar({OnboardingController.prefsKey: 'no soy json{'});
      expect(c.state, const OnboardingProgress());
    });

    test('campos ausentes caen al valor por defecto', () {
      final p = OnboardingProgress.fromJson(const {});
      expect(p, const OnboardingProgress());
    });

    test('campos con el tipo equivocado no se cuelan como true', () {
      final p = OnboardingProgress.fromJson(const {
        'terminado': 'sí',
        'idiomaElegido': 1,
        'preferenciaMotor': 'mucha',
      });
      expect(p.terminado, isFalse);
      expect(p.idiomaElegido, isFalse);
      expect(p.preferenciaMotor, 0.5);
    });

    test('una preferencia fuera de rango se recorta al leerla', () {
      expect(
        OnboardingProgress.fromJson(const {'preferenciaMotor': 42})
            .preferenciaMotor,
        1.0,
      );
      expect(
        OnboardingProgress.fromJson(const {'preferenciaMotor': -7})
            .preferenciaMotor,
        0.0,
      );
    });

    test('ida y vuelta completa', () {
      const original = OnboardingProgress(
        idiomaElegido: true,
        pruebaHecha: true,
        formateoElegido: true,
        terminado: true,
        preferenciaMotor: 0.25,
      );
      expect(OnboardingProgress.fromJson(original.toJson()), original);
    });
  });

  group('terminar y repetir', () {
    test('terminar deja también la marca vieja del splash', () async {
      // Sin esto, al cerrar el asistente saldría la pantalla de carga a
      // preparar justo lo que el asistente acaba de preparar.
      final c = await arrancar({});
      await c.terminar();
      final prefs = await SharedPreferences.getInstance();
      expect(c.state.terminado, isTrue);
      expect(prefs.getBool(OnboardingController.legacyReadyOnceKey), isTrue);
    });

    test('repetir vuelve al principio pero conserva el mando', () async {
      final c = await arrancar({});
      await c.setPreferenciaMotor(0.9);
      await c.marcarIdiomaElegido();
      await c.marcarPruebaHecha();
      await c.terminar();

      await c.repetir();
      expect(c.state.terminado, isFalse);
      expect(c.state.idiomaElegido, isFalse);
      expect(c.state.pruebaHecha, isFalse);
      expect(c.state.preferenciaMotor, 0.9);
      expect((await guardado())?['terminado'], isFalse);
    });

    test('el mando se recorta al guardarse', () async {
      final c = await arrancar({});
      await c.setPreferenciaMotor(5);
      expect(c.state.preferenciaMotor, 1.0);
      await c.setPreferenciaMotor(-5);
      expect(c.state.preferenciaMotor, 0.0);
    });
  });

  group('debeAbrirAsistenteProvider', () {
    test('vale null mientras el avance no se ha leído de disco', () {
      SharedPreferences.setMockInitialValues({});
      final container = ProviderContainer();
      addTearDown(container.dispose);
      // Sin esperar a `loaded`: es el instante exacto del arranque.
      expect(container.read(debeAbrirAsistenteProvider), isNull);
    });

    test('tras leer, dice true en instalación nueva', () async {
      SharedPreferences.setMockInitialValues({});
      final container = ProviderContainer();
      addTearDown(container.dispose);
      await container.read(avanceLeidoProvider.future);
      expect(container.read(debeAbrirAsistenteProvider), isTrue);
    });

    test('tras leer, dice false si ya había una instalación buena', () async {
      SharedPreferences.setMockInitialValues({
        OnboardingController.legacyReadyOnceKey: true,
      });
      final container = ProviderContainer();
      addTearDown(container.dispose);
      await container.read(avanceLeidoProvider.future);
      expect(container.read(debeAbrirAsistenteProvider), isFalse);
    });
  });
}
