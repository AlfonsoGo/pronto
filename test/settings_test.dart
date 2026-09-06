// ignore_for_file: require_trailing_commas
import 'package:flutter_test/flutter_test.dart';
import 'package:pronto/src/core/config.dart';
import 'package:pronto/src/features/formatting/cloud_cost.dart';
import 'package:pronto/src/features/formatting/text_formatter.dart';
import 'package:pronto/src/features/settings/settings_controller.dart';
import 'package:pronto/src/features/translation/language_detect.dart';
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
      const original = AppSettings(
        modelFile: 'ggml-large-v3-turbo.bin',
        language: 'en',
        injectionMode: InjectionMode.unicodeSendInput,
        triggerMode: TriggerMode.hold,
        autostart: true,
        formatPath: FormatPath.local,
        llmBaseUrl: 'http://localhost:1234',
        formatModel: 'phi3:mini',
        captureExternalEdits: false,
        hotkey: HotkeyCombo(
          virtualKey: 0x73, // F4
          ctrl: true,
          shift: true,
        ),
        pillScale: 1.5,
        engine: SpeechEngine.parakeet,
        textPolish: false,
        sounds: false,
        micDeviceId: 'mic-001',
        translationEnabled: true,
        outputLanguage: 'en',
        translationModel: 'gemma3:4b',
        translateHotkey: HotkeyCombo(virtualKey: 0x4C, ctrl: true, win: true),
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

      test('clave ausente en JSON devuelve null (compatibilidad hacia atrás)',
          () {
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

      test('engine desconocido en JSON cae al valor por defecto (parakeet)',
          () {
        final fromDisk = AppSettings.fromJson(
          base().toJson()..['engine'] = 'valor_inventado',
        );
        // _engineFromName devuelve null → defaults() usa parakeet.
        expect(fromDisk.engine, SpeechEngine.parakeet);
      });
    });

    group('tolerancia a claves ausentes o inválidas', () {
      test('JSON vacío devuelve los valores por defecto', () {
        final restored = AppSettings.fromJson(const {});
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

    // Los ajustes de traducción son NUEVOS: un Pronto ya instalado tiene en
    // disco un JSON sin estas claves y debe seguir arrancando con la traducción
    // apagada, no romperse ni activarla sola.
    group('traducción: compatibilidad con ajustes antiguos', () {
      Map<String, dynamic> sinTraduccion() => base().toJson()
        ..remove('translationEnabled')
        ..remove('outputLanguage')
        ..remove('translationModel')
        ..remove('translateHotkey');

      test('ajustes viejos (sin claves de traducción) caen a los defectos', () {
        final restored = AppSettings.fromJson(sinTraduccion());
        expect(restored.translationEnabled, isFalse);
        expect(restored.outputLang, TextLanguage.es);
        expect(restored.translateHotkey, HotkeyCombo.translateDefault);
        expect(restored.translationModel, base().translationModel);
      });

      test('el resto de ajustes viejos sobrevive intacto', () {
        final json = sinTraduccion()..['pillScale'] = 1.4;
        final restored = AppSettings.fromJson(json);
        expect(restored.pillScale, 1.4);
        expect(restored.hotkey, base().hotkey);
      });

      test('outputLanguage inválido cae a español', () {
        final json = base().toJson()..['outputLanguage'] = 'fr';
        expect(AppSettings.fromJson(json).outputLang, TextLanguage.es);
      });

      test('outputLanguage válido se respeta', () {
        final json = base().toJson()..['outputLanguage'] = 'en';
        expect(AppSettings.fromJson(json).outputLang, TextLanguage.en);
      });

      test('translateHotkey corrupto (no-Map) cae al defecto', () {
        final json = base().toJson()..['translateHotkey'] = 'ctrl+alt+t';
        expect(
          AppSettings.fromJson(json).translateHotkey,
          HotkeyCombo.translateDefault,
        );
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

  // ─── PRNT-1: el formateo deja de estar gateado por confianza ──────────────
  group('camino de formateo (migración desde llmEnabled)', () {
    test('por defecto no se formatea', () {
      expect(AppSettings.defaults().formatPath, FormatPath.none);
    });

    test('el modelo por defecto SÍ existe en el registro de Ollama', () {
      // `qwen3:4b`, el defecto viejo, no existe: encender el formateo daba 404
      // y parecía que la función no servía.
      expect(AppSettings.defaults().formatModel, isNot('qwen3:4b'));
      expect(AppSettings.defaults().formatModel,
          AppConfig.defaultFormatLocalModel);
    });

    test('ajustes viejos con el LLM ENCENDIDO -> formateo local', () {
      const viejo = <String, dynamic>{
        'llmEnabled': true,
        'llmBaseUrl': 'http://localhost:11434',
        'llmModel': 'phi3:mini',
      };
      final s = AppSettings.fromJson(viejo);
      expect(s.formatPath, FormatPath.local);
      expect(s.formatModel, 'phi3:mini');
    });

    test('ajustes viejos con el LLM APAGADO -> sin formateo', () {
      final s =
          AppSettings.fromJson(const <String, dynamic>{'llmEnabled': false});
      expect(s.formatPath, FormatPath.none);
    });

    test('ajustes viejos con el modelo roto qwen3:4b -> cae al defecto', () {
      final s = AppSettings.fromJson(const <String, dynamic>{
        'llmEnabled': true,
        'llmModel': 'qwen3:4b',
      });
      expect(s.formatModel, AppConfig.defaultFormatLocalModel);
    });

    test('ajustes sin ninguna clave de LLM -> defecto, sin reventar', () {
      final s = AppSettings.fromJson(const <String, dynamic>{});
      expect(s.formatPath, FormatPath.none);
      expect(s.formatModel, AppConfig.defaultFormatLocalModel);
    });

    test('un formatPath desconocido cae al defecto', () {
      final s = AppSettings.fromJson(
          const <String, dynamic>{'formatPath': 'satelite'});
      expect(s.formatPath, FormatPath.none);
    });

    test('la nube YA es un camino elegible (PRNT-11), y viene apagada', () {
      // Hasta PRNT-11 un `cloud` persistido caía a "sin formateo" porque sus
      // guardias no existían. Ahora existen —clave en el almacén de
      // credenciales, tope de gasto y aviso en la Píldora— así que el camino se
      // respeta. Encenderlo editando el fichero a mano no salta ninguno: sin
      // clave no sale ni una petición (ver cloud_formatting_test.dart).
      final s =
          AppSettings.fromJson(const <String, dynamic>{'formatPath': 'cloud'});
      expect(s.formatPath, FormatPath.cloud);
      expect(AppSettings.defaults().formatPath, FormatPath.none);
    });

    test('el modelo de nube por defecto está en la lista blanca', () {
      expect(kCloudPrices, contains(AppSettings.defaults().cloudModel));
    });

    test('un modelo de nube DESCONOCIDO no entra por el fichero de ajustes',
        () {
      // Si entrara, el tope de gasto no sabría lo que cuesta cada llamada.
      final s = AppSettings.fromJson(const <String, dynamic>{
        'cloudModel': 'carisimo/modelo-inventado',
      });
      expect(s.cloudModel, AppSettings.defaults().cloudModel);
    });

    test('un modelo de nube conocido SÍ se respeta', () {
      final s = AppSettings.fromJson(const <String, dynamic>{
        'cloudModel': 'openai/gpt-5-mini',
      });
      expect(s.cloudModel, 'openai/gpt-5-mini');
    });

    test('los topes de gasto por defecto son mucho más bajos que 2 \$/día', () {
      // El tope del vigilante del VPS (2 \$/día) son cincuenta meses de uso
      // normal: no puede detectar nada. El freno de verdad es este.
      expect(AppSettings.defaults().cloudDayCapUsd, lessThan(0.5));
      expect(AppSettings.defaults().cloudMonthCapUsd, lessThan(2.0));
    });

    test('un tope roto o negativo cae al defecto, nunca a "sin tope"', () {
      for (final basura in ['mucho', -1, null, double.nan, double.infinity]) {
        final s = AppSettings.fromJson(<String, dynamic>{
          'cloudDayCapUsd': basura,
          'cloudMonthCapUsd': basura,
        });
        expect(s.cloudDayCapUsd, AppConfig.defaultCloudDayCapUsd,
            reason: '\$basura');
        expect(s.cloudMonthCapUsd, AppConfig.defaultCloudMonthCapUsd);
      }
    });

    test('un tope de CERO sí se respeta: es apagar la nube', () {
      final s = AppSettings.fromJson(const <String, dynamic>{
        'cloudDayCapUsd': 0,
      });
      expect(s.cloudDayCapUsd, 0);
    });

    test('round-trip del camino de nube con sus topes', () {
      final s = AppSettings.defaults().copyWith(
        formatPath: FormatPath.cloud,
        cloudModel: 'google/gemini-3.1-flash-lite',
        cloudDayCapUsd: 0.25,
        cloudMonthCapUsd: 3,
      );
      expect(AppSettings.fromJson(s.toJson()), s);
    });

    test('🔒 el volcado de ajustes es una lista BLANCA, y la clave no está', () {
      // El fichero de ajustes es JSON en claro. Se enumera lo que PUEDE estar
      // dentro, no lo que no: buscar subcadenas prohibidas falla solo
      // («hotkey» contiene «key»), y lo que hay que demostrar es el negativo.
      final json = AppSettings.defaults()
          .copyWith(formatPath: FormatPath.cloud)
          .toJson();
      expect(json.keys.toSet(), {
        'modelFile',
        'language',
        'injectionMode',
        'triggerMode',
        'autostart',
        'formatPath',
        'llmBaseUrl',
        'formatModel',
        'cloudModel',
        'cloudDayCapUsd',
        'cloudMonthCapUsd',
        'captureExternalEdits',
        'hotkey',
        'pillScale',
        'engine',
        'textPolish',
        'sounds',
        'micDeviceId',
        'translationEnabled',
        'outputLanguage',
        'translationModel',
        'translateHotkey',
        'historyRetentionDays',
      });
    });

    test('formatPath nuevo gana sobre llmEnabled viejo', () {
      final s = AppSettings.fromJson(const <String, dynamic>{
        'formatPath': 'none',
        'llmEnabled': true,
      });
      expect(s.formatPath, FormatPath.none);
    });

    test('round-trip del camino local', () {
      final s = AppSettings.defaults().copyWith(formatPath: FormatPath.local);
      expect(AppSettings.fromJson(s.toJson()).formatPath, FormatPath.local);
    });
  });
}
