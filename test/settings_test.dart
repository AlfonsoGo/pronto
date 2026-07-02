// ignore_for_file: require_trailing_commas
import 'package:flutter_test/flutter_test.dart';
import 'package:pronto/src/features/settings/settings_controller.dart';
import 'package:pronto/src/platform/global_hotkey_service.dart';
import 'package:pronto/src/platform/text_injector.dart';

// Tests PUROS de AppSettings: solo lógica Dart, sin plugins/FS/red.
// Nota: SettingsController._load toca SharedPreferences y launchAtStartup —
// ambos son nativos y no se pueden instanciar en flutter test. La lógica de
// migración whisper→parakeet que vive dentro de _load se verifica aquí
// de forma equivalente (el mismo bloque if) construyendo el estado a mano.

void main() {
  // Atajo para construir un AppSettings completo con un único campo distinto.
  AppSettings base() => AppSettings.defaults();

  group('AppSettings.toJson / fromJson (round-trip)', () {
    test('round-trip completo con valores por defecto', () {
      final original = base();
      final restored = AppSettings.fromJson(original.toJson());
      expect(restored, equals(original));
    });

    test('round-trip con todos los campos no-defecto', () {
      final original = AppSettings(
        modelFile: 'ggml-large-v3-turbo.bin',
        language: 'en',
        injectionMode: InjectionMode.unicodeSendInput,
        triggerMode: TriggerMode.hold,
        autostart: true,
        llmEnabled: true,
        llmBaseUrl: 'http://localhost:1234',
        llmModel: 'phi3:mini',
        captureExternalEdits: false,
        hotkey: const HotkeyCombo(
          virtualKey: 0x73, // F4
          ctrl: true,
          shift: true,
        ),
        pillScale: 1.5,
        engine: SpeechEngine.parakeet,
        textPolish: false,
        sounds: false,
        micDeviceId: 'mic-001',
      );
      final restored = AppSettings.fromJson(original.toJson());
      expect(restored, equals(original));
    });

    group('micDeviceId', () {
      test('null se serializa como null y se restaura como null', () {
        final s = base(); // micDeviceId es null por defecto
        expect(s.micDeviceId, isNull);
        final restored = AppSettings.fromJson(s.toJson());
        expect(restored.micDeviceId, isNull);
      });

      test('valor no-nulo sobrevive el round-trip', () {
        final s = base().copyWith(micDeviceId: 'USB-Audio-Device-42');
        expect(s.micDeviceId, 'USB-Audio-Device-42');
        final restored = AppSettings.fromJson(s.toJson());
        expect(restored.micDeviceId, 'USB-Audio-Device-42');
      });

      test('clave ausente en JSON devuelve null (compatibilidad hacia atrás)', () {
        final json = base().toJson()..remove('micDeviceId');
        final restored = AppSettings.fromJson(json);
        expect(restored.micDeviceId, isNull);
      });
    });

    group('migración engine whisper→parakeet', () {
      // Replica el bloque de migración que vive en SettingsController._load.
      // Lo probamos sobre el modelo puro, sin necesitar SharedPreferences.
      AppSettings applyMigration(AppSettings s) {
        if (s.engine == SpeechEngine.whisper) {
          return s.copyWith(engine: SpeechEngine.parakeet);
        }
        return s;
      }

      test('engine=whisper se migra a parakeet', () {
        final fromDisk = AppSettings.fromJson(
          base().toJson()..['engine'] = 'whisper',
        );
        expect(fromDisk.engine, SpeechEngine.whisper); // leído tal cual
        final migrated = applyMigration(fromDisk);
        expect(migrated.engine, SpeechEngine.parakeet);
      });

      test('engine=parakeet no cambia tras la migración', () {
        final s = base(); // parakeet por defecto
        final migrated = applyMigration(s);
        expect(migrated.engine, SpeechEngine.parakeet);
      });

      test('engine desconocido en JSON cae al valor por defecto (parakeet)', () {
        final fromDisk = AppSettings.fromJson(
          base().toJson()..['engine'] = 'valor_inventado',
        );
        // _engineFromName devuelve null → defaults() usa parakeet.
        expect(fromDisk.engine, SpeechEngine.parakeet);
      });
    });

    group('tolerancia a claves ausentes o inválidas', () {
      test('JSON vacío devuelve los valores por defecto', () {
        final restored = AppSettings.fromJson({});
        expect(restored, equals(base()));
      });

      test('injectionMode inválido cae al valor por defecto', () {
        final json = base().toJson()..['injectionMode'] = 'desconocido';
        final restored = AppSettings.fromJson(json);
        expect(restored.injectionMode, equals(base().injectionMode));
      });

      test('triggerMode inválido cae al valor por defecto', () {
        final json = base().toJson()..['triggerMode'] = 'desconocido';
        final restored = AppSettings.fromJson(json);
        expect(restored.triggerMode, equals(base().triggerMode));
      });

      test('pillScale como entero se convierte a double', () {
        final json = base().toJson()..['pillScale'] = 2; // int, no double
        final restored = AppSettings.fromJson(json);
        expect(restored.pillScale, 2.0);
      });
    });

    group('copyWith', () {
      test('resetMicDevice=true pone micDeviceId a null', () {
        final s = base().copyWith(micDeviceId: 'mic-X');
        final reset = s.copyWith(resetMicDevice: true);
        expect(reset.micDeviceId, isNull);
      });

      test('copyWith sin resetMicDevice conserva el valor existente', () {
        final s = base().copyWith(micDeviceId: 'mic-X');
        final copy = s.copyWith(language: 'en');
        expect(copy.micDeviceId, 'mic-X');
      });

      test('copyWith con micDeviceId nuevo lo actualiza', () {
        final s = base().copyWith(micDeviceId: 'mic-A');
        final updated = s.copyWith(micDeviceId: 'mic-B');
        expect(updated.micDeviceId, 'mic-B');
      });
    });
  });

  group('HotkeyCombo.toJson / fromJson', () {
    test('round-trip combo por defecto', () {
      const combo = HotkeyCombo.defaultCombo;
      final restored = HotkeyCombo.fromJson(combo.toJson());
      expect(restored, equals(combo));
    });

    test('round-trip combo con todos los modificadores', () {
      const combo = HotkeyCombo(
        virtualKey: 0x73,
        ctrl: true,
        alt: true,
        shift: true,
        win: true,
      );
      final restored = HotkeyCombo.fromJson(combo.toJson());
      expect(restored, equals(combo));
    });

    test('describe combo por defecto', () {
      expect(HotkeyCombo.defaultCombo.describe(), 'Ctrl + Alt + Espacio');
    });

    test('describe tecla F4', () {
      const combo = HotkeyCombo(virtualKey: 0x73, ctrl: true);
      expect(combo.describe(), 'Ctrl + F4');
    });

    test('describe tecla alfanumérica', () {
      const combo = HotkeyCombo(virtualKey: 0x41); // 'A'
      expect(combo.describe(), 'A');
    });
  });
}
