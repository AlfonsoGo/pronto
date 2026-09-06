import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:pronto/src/core/config.dart';
import 'package:pronto/src/features/formatting/text_formatter.dart';
import 'package:pronto/src/features/settings/settings_controller.dart';
import 'package:pronto/src/platform/global_hotkey_service.dart';
import 'package:pronto/src/platform/text_injector.dart';

/// Los Ajustes de la app al ACTUALIZAR (PRNT-20).
///
/// Se parte del `shared_preferences.json` REAL que deja Pronto en Windows
/// (`test/fixtures/shared_preferences-0.7.1.json`), no de un mapa escrito a
/// mano: las claves llevan el prefijo `flutter.` y el valor de `app_settings`
/// es una CADENA con JSON dentro. Ese doble encoding es justo donde se
/// esconderían los fallos que un mapa a mano nunca vería. Los valores son
/// inventados; no sale nada de ninguna instalación real.
///
/// Lo que se prueba es lo que pide el ticket: que no se pierda ni un campo y
/// que **lo que estaba apagado siga apagado** (ADR 0005 vale también al
/// actualizar; nadie se encuentra el Formateo, el asistente local ni la nube
/// encendidos por sorpresa).
void main() {
  /// Lee los ajustes como los lee la app: `getString('app_settings')` →
  /// `jsonDecode` → [AppSettings.fromJson].
  AppSettings settingsFrom(String fixture) {
    final file = File('test/fixtures/$fixture');
    expect(file.existsSync(), isTrue, reason: 'falta test/fixtures/$fixture');
    final prefs = jsonDecode(file.readAsStringSync()) as Map<String, dynamic>;
    final raw = prefs['flutter.app_settings'] as String;
    return AppSettings.fromJson(jsonDecode(raw) as Map<String, dynamic>);
  }

  AppSettings of071() => settingsFrom('shared_preferences-0.7.1.json');

  group('no se pierde ni un campo', () {
    test('todo lo que el usuario había configurado sigue configurado', () {
      final s = of071();

      expect(s.modelFile, 'ggml-small.bin');
      expect(s.language, 'es');
      expect(s.injectionMode, InjectionMode.unicodeSendInput);
      expect(s.triggerMode, TriggerMode.hold);
      expect(s.autostart, isTrue);
      expect(s.llmBaseUrl, 'http://192.168.1.50:11434');
      expect(s.captureExternalEdits, isFalse);
      expect(s.pillScale, 1.4);
      expect(s.textPolish, isFalse);
      expect(s.sounds, isFalse);
      expect(s.micDeviceId, 'micro-de-mesa');
    });

    test('el Atajo del usuario sigue siendo el suyo, no el de fábrica', () {
      final s = of071();

      expect(s.hotkey.virtualKey, 0x78); // F9
      expect(s.hotkey.ctrl, isTrue);
      expect(s.hotkey.shift, isTrue);
      expect(s.hotkey.alt, isFalse);
      expect(s.hotkey.win, isFalse);
      expect(s.hotkey, isNot(HotkeyCombo.defaultCombo));
      // Y con el mismo comportamiento de mantener-pulsado que tenía.
      expect(s.triggerMode, TriggerMode.hold);
    });

    test('el atajo sobrevive a guardar y volver a leer tras actualizar', () {
      final s = of071();
      final after = AppSettings.fromJson(s.toJson());

      expect(after.hotkey, s.hotkey);
      expect(after, equals(s));
    });
  });

  group('lo que estaba apagado sigue apagado', () {
    test('quien no tenía formateo no lo tiene después de actualizar', () {
      expect(of071().formatPath, FormatPath.none);
    });

    test('quien lo tenía en local se queda en LOCAL, nunca en la nube', () {
      // El mismo fichero con `llmEnabled` a true: en la 0.7.1 el LLM solo podía
      // ser Ollama local. Actualizar no puede meter a nadie en la nube: eso
      // cuesta dinero y saca su texto del equipo.
      final file = File('test/fixtures/shared_preferences-0.7.1.json');
      final prefs = jsonDecode(file.readAsStringSync()) as Map<String, dynamic>;
      final raw = jsonDecode(prefs['flutter.app_settings'] as String)
          as Map<String, dynamic>;
      raw['llmEnabled'] = true;

      final s = AppSettings.fromJson(raw);

      expect(s.formatPath, FormatPath.local);
      expect(s.formatPath, isNot(FormatPath.cloud));
    });

    test('la traducción no se enciende sola', () {
      expect(of071().translationEnabled, isFalse);
    });

    test('los topes de gasto de nube caen a los de fábrica, no a "sin tope"',
        () {
      final s = of071();
      expect(s.cloudDayCapUsd, AppConfig.defaultCloudDayCapUsd);
      expect(s.cloudMonthCapUsd, AppConfig.defaultCloudMonthCapUsd);
      expect(s.cloudModel, AppConfig.defaultFormatCloudModel);
    });
  });

  group('un usuario de la 0.6.0, con menos claves todavía', () {
    // La otra versión con la que puede haber gente ahí fuera.
    Map<String, dynamic> of060() => {
          'modelFile': 'ggml-small.bin',
          'language': 'es',
          'injectionMode': 'clipboardPaste',
          'triggerMode': 'toggle',
          'autostart': false,
          'llmEnabled': false,
          'llmBaseUrl': 'http://localhost:11434',
          'llmModel': 'qwen3:4b',
          'captureExternalEdits': true,
          'hotkey': {'vk': 0x79, 'ctrl': false, 'alt': true},
        };

    test('conserva su atajo aunque le falten claves del JSON', () {
      final s = AppSettings.fromJson(of060());

      expect(s.hotkey.virtualKey, 0x79);
      expect(s.hotkey.alt, isTrue);
      expect(s.hotkey.ctrl, isFalse);
      // Las que no existían caen a su defecto sin reventar.
      expect(s.pillScale, 1.0);
      expect(s.sounds, isTrue);
      expect(s.micDeviceId, isNull);
    });

    test('no se le enciende nada y el modelo roto se cambia por uno que existe',
        () {
      final s = AppSettings.fromJson(of060());

      expect(s.formatPath, FormatPath.none);
      expect(s.translationEnabled, isFalse);
      // `qwen3:4b` no existe en el registro de Ollama: arrastrarlo daría 404.
      expect(s.formatModel, AppConfig.defaultFormatLocalModel);
    });
  });
}
