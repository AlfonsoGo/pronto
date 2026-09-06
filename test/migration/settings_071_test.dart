import 'package:flutter_test/flutter_test.dart';
import 'package:pronto/src/core/config.dart';
import 'package:pronto/src/features/formatting/text_formatter.dart';
import 'package:pronto/src/features/settings/settings_controller.dart';
import 'package:pronto/src/platform/global_hotkey_service.dart';
import 'package:pronto/src/platform/text_injector.dart';

/// Los Ajustes de la app al ACTUALIZAR (PRNT-20).
///
/// Los ajustes de abajo son el `toJson` literal de la 0.7.1 (commit f660b22):
/// lo que hay hoy en el `shared_preferences.json` de cualquiera que instalara
/// Pronto antes de esta versión. Lo que se prueba es lo que el ticket pide:
/// que no se pierda ni un campo y que **lo que estaba apagado siga apagado**
/// (ADR 0005 vale también al actualizar; nadie se encuentra el Formateo, el
/// asistente local ni la nube encendidos por sorpresa).
void main() {
  /// El fichero de ajustes de un usuario de la 0.7.1 que se lo tenía todo
  /// configurado a su manera. Ningún valor sale de una instalación real.
  Map<String, dynamic> ajustes071({bool llmEnabled = false}) => {
        'modelFile': 'ggml-small.bin',
        'language': 'es',
        'injectionMode': 'unicodeSendInput',
        'triggerMode': 'hold',
        'autostart': true,
        'llmEnabled': llmEnabled,
        'llmBaseUrl': 'http://192.168.1.50:11434',
        'llmModel': 'qwen2.5:7b',
        'captureExternalEdits': false,
        // Ctrl + Shift + F9, no el atajo de fábrica.
        'hotkey': {
          'vk': 0x78,
          'ctrl': true,
          'alt': false,
          'shift': true,
          'win': false,
        },
        'pillScale': 1.4,
        'engine': 'parakeet',
        'textPolish': false,
        'sounds': false,
        'micDeviceId': 'micro-de-mesa',
      };

  group('no se pierde ni un campo', () {
    test('todo lo que el usuario había configurado sigue configurado', () {
      final s = AppSettings.fromJson(ajustes071());

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
      final s = AppSettings.fromJson(ajustes071());

      expect(s.hotkey.virtualKey, 0x78);
      expect(s.hotkey.ctrl, isTrue);
      expect(s.hotkey.shift, isTrue);
      expect(s.hotkey.alt, isFalse);
      expect(s.hotkey.win, isFalse);
      expect(s.hotkey, isNot(HotkeyCombo.defaultCombo));
      // Y con el mismo comportamiento de mantener-pulsado que tenía.
      expect(s.triggerMode, TriggerMode.hold);
    });

    test('el atajo sobrevive a guardar y volver a leer tras actualizar', () {
      final s = AppSettings.fromJson(ajustes071());
      final tras = AppSettings.fromJson(s.toJson());

      expect(tras.hotkey, s.hotkey);
      expect(tras, equals(s));
    });
  });

  group('lo que estaba apagado sigue apagado', () {
    test('quien no tenía formateo no lo tiene después de actualizar', () {
      final s = AppSettings.fromJson(ajustes071());
      expect(s.formatPath, FormatPath.none);
    });

    test('quien lo tenía en local se queda en LOCAL, nunca en la nube', () {
      final s = AppSettings.fromJson(ajustes071(llmEnabled: true));
      // En la 0.7.1 el LLM solo podía ser Ollama local. Actualizar no puede
      // meter a nadie en la nube: eso cuesta dinero y saca su texto del equipo.
      expect(s.formatPath, FormatPath.local);
      expect(s.formatPath, isNot(FormatPath.cloud));
    });

    test('la traducción no se enciende sola', () {
      final s = AppSettings.fromJson(ajustes071());
      expect(s.translationEnabled, isFalse);
    });

    test('los topes de gasto de nube caen a los de fábrica, no a "sin tope"',
        () {
      final s = AppSettings.fromJson(ajustes071());
      expect(s.cloudDayCapUsd, AppConfig.defaultCloudDayCapUsd);
      expect(s.cloudMonthCapUsd, AppConfig.defaultCloudMonthCapUsd);
      expect(s.cloudModel, AppConfig.defaultFormatCloudModel);
    });
  });

  group('un usuario de la 0.6.0, con menos claves todavía', () {
    // La otra versión con la que puede haber gente ahí fuera.
    Map<String, dynamic> ajustes060() => {
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
      final s = AppSettings.fromJson(ajustes060());

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
      final s = AppSettings.fromJson(ajustes060());

      expect(s.formatPath, FormatPath.none);
      expect(s.translationEnabled, isFalse);
      // `qwen3:4b` no existe en el registro de Ollama: arrastrarlo daría 404.
      expect(s.formatModel, AppConfig.defaultFormatLocalModel);
    });
  });
}
