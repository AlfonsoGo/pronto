import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:pronto/src/data/schema_migrations.dart';
import 'package:pronto/src/data/sql_functions.dart';
import 'package:pronto/src/data/sqlite_history_repository.dart';
import 'package:pronto/src/features/dictation/dictation_controller.dart';
import 'package:pronto/src/features/dictation/dictation_state.dart';
import 'package:pronto/src/features/history/dictation_entry.dart';
import 'package:pronto/src/features/history/history_repository.dart';
import 'package:pronto/src/features/learning/learning_repository.dart';
import 'package:pronto/src/features/settings/settings_controller.dart';
import 'package:pronto/src/platform/audio_capture.dart';
import 'package:pronto/src/platform/foreground_app.dart';
import 'package:pronto/src/platform/global_hotkey_service.dart';
import 'package:pronto/src/platform/platform_services.dart';
import 'package:pronto/src/platform/text_injector.dart';
import 'package:pronto/src/platform/whisper_engine.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:sqlite3/sqlite3.dart';

import '../data/sqlite_for_tests.dart';

// El criterio de aceptación del PRNT-13: **el dictado de prueba NO aparece en
// el historial** — la prueba negativa, recorrida entera por el pipeline de
// verdad y no por un atajo del test.
//
// Por qué merece este montaje: el filtro `source = 'dictado'` ya existía en el
// repositorio desde el PRNT-16, y ya tiene su test. Lo que NO estaba probado es
// que el asistente lo EJERZA. Un filtro que nadie usa es un control inerte
// (memoria `feedback-control-entregado-inerte`): comprobar que el filtro
// funciona y no comprobar que el camino nuevo lo atraviesa habría dado verde
// con la avería dentro.

class _MicFalso implements AudioCapture {
  @override
  Future<bool> hasPermission() async => true;
  @override
  Future<void> start() async {}
  @override
  Future<Float32List> stop() async => Float32List.fromList(
        List<double>.filled(16000, 0.1),
      );
  @override
  Stream<double> get amplitude => const Stream.empty();
  @override
  Future<void> dispose() async {}
  @override
  Future<List<MicDevice>> listInputDevices() async => const [];
  @override
  void setInputDeviceId(String? id) {}
}

class _MotorFalso implements WhisperEngine {
  @override
  bool get isLoaded => true;
  @override
  Future<void> load(String modelPath) async {}
  @override
  Future<void> dispose() async {}
  @override
  Future<TranscriptResult> transcribe(
    Float32List pcm, {
    String language = 'es',
    String? initialPrompt,
    int? threads,
  }) async =>
      const TranscriptResult(
        text: 'probando el micrófono',
        avgLogProb: 0,
        language: 'es',
        audioMs: 1000,
      );
}

/// Apunta si alguien intentó pegar. En la prueba del asistente nadie debe.
class _InyectorEspia implements TextInjector {
  int veces = 0;
  @override
  Future<InjectionResult> insert(
    String text, {
    InjectionMode mode = InjectionMode.clipboardPaste,
  }) async {
    veces++;
    return InjectionResult.ok;
  }
}

class _AtajosFalsos implements GlobalHotkeyService {
  @override
  Stream<HotkeyEvent> get events => const Stream.empty();
  @override
  Future<void> register(
    HotkeyCombo combo, {
    TriggerMode mode = TriggerMode.hold,
    HotkeyCombo? copyCombo,
    HotkeyCombo? langCombo,
  }) async {}
  @override
  Future<void> unregister() async {}
  @override
  Future<void> dispose() async {}
}

/// Aprendizaje en memoria. Apunta lo que se le pida guardar: la prueba no puede
/// enseñarle nada al Vocabulario.
class _AprendizajeFalso implements LearningRepository {
  final List<String> correcciones = [];
  final List<String> terminos = [];

  @override
  Future<void> bumpCorrection(String raw, String corrected) async {
    correcciones.add('$raw -> $corrected');
  }

  @override
  Future<List<CorrectionEntry>> activeCorrections({int minFreq = 3}) async =>
      const [];

  @override
  Future<void> bumpVocab(String term) async => terminos.add(term);

  @override
  Future<List<String>> topVocab({int limit = 45}) async => const [];
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  setUpAll(prepareSqliteForTests);

  late Database db;
  late SqliteHistoryRepository historial;
  late _InyectorEspia inyector;
  late _AprendizajeFalso aprendizaje;
  late ProviderContainer container;

  setUp(() async {
    db = sqlite3.openInMemory();
    registerProntoFunctions(db);
    migrateSchema(db);
    historial = SqliteHistoryRepository(() async => db);
    inyector = _InyectorEspia();
    aprendizaje = _AprendizajeFalso();

    // Ajustes de fábrica salvo los sonidos: un pitido de Win32 en mitad de la
    // suite no aporta nada.
    SharedPreferences.setMockInitialValues({
      'app_settings': jsonEncode(
        AppSettings.defaults().copyWith(sounds: false).toJson(),
      ),
    });

    container = ProviderContainer(
      overrides: [
        audioCaptureProvider.overrideWithValue(_MicFalso()),
        whisperEngineProvider.overrideWithValue(_MotorFalso()),
        textInjectorProvider.overrideWithValue(inyector),
        hotkeyServiceProvider.overrideWithValue(_AtajosFalsos()),
        foregroundAppProvider.overrideWithValue(const UnknownForegroundApp()),
        learningRepositoryProvider.overrideWithValue(aprendizaje),
        historyRepositoryProvider.overrideWithValue(historial),
        // El modelo ya está "cargado" en el motor falso: no hay ruta que
        // resolver, y resolverla tocaría path_provider (nativo).
        modelPathResolverProvider.overrideWith((ref) async => null),
      ],
    );
    addTearDown(container.dispose);

    final ctrl = container.read(dictationControllerProvider.notifier);
    await container.read(settingsProvider.notifier).loaded;
    // initialize() arranca en un microtask; se espera a que deje de estar sin
    // inicializar en vez de dormir un rato a ojo.
    final tope = DateTime.now().add(const Duration(seconds: 5));
    while (container.read(dictationControllerProvider).status ==
            DictationStatus.uninitialized &&
        DateTime.now().isBefore(tope)) {
      await Future<void>.delayed(const Duration(milliseconds: 10));
    }
    expect(
      container.read(dictationControllerProvider).status,
      DictationStatus.idle,
      reason: 'el controlador no llegó a estar listo: ${ctrl.state.error}',
    );
  });

  tearDown(() => db.dispose());

  Future<void> dictar() async {
    final ctrl = container.read(dictationControllerProvider.notifier);
    await ctrl.startRecording();
    await ctrl.stopAndProcess();
    // Apuntar en el Historial NO bloquea el pegado, así que la fila puede no
    // existir todavía. Sin esperarla, el test negativo daría verde por llegar
    // antes que la escritura, no porque la escritura no vaya a ocurrir.
    await ctrl.lastLogWrite;
  }

  group('la prueba del asistente', () {
    test('NO aparece en el Historial', () async {
      container.read(dictationControllerProvider.notifier).armarPrueba();
      await dictar();

      expect(await historial.page(const HistoryQuery()), isEmpty);
      expect(await historial.count(const HistoryQuery()), 0);
      expect(await historial.all(const HistoryQuery()), isEmpty);
    });

    test('pero SÍ queda apuntada, marcada como prueba', () async {
      // No es lo mismo «no guardarlo» que «no enseñarlo». Queda constancia de
      // que la instalación se probó, fuera del Corpus con el que se mide.
      container.read(dictationControllerProvider.notifier).armarPrueba();
      await dictar();

      final filas = db.select(
        "SELECT source FROM dictation_log WHERE source = 'prueba'",
      );
      expect(filas.length, 1);
    });

    test('NO se pega en ninguna aplicación', () async {
      container.read(dictationControllerProvider.notifier).armarPrueba();
      await dictar();
      expect(inyector.veces, 0);
    });

    test('el texto sale en la caja del asistente, no en lastText', () async {
      // Si compartieran hueco, «copiar la última transcripción» y las
      // Correcciones acabarían trabajando sobre una prueba.
      container.read(dictationControllerProvider.notifier).armarPrueba();
      await dictar();

      final st = container.read(dictationControllerProvider);
      // Con el Pulido puesto (de fábrica) la prueba enseña el texto tal y como
      // saldría en un dictado de verdad, mayúscula incluida.
      expect(st.lastTestText, 'Probando el micrófono');
      expect(st.lastText, isNull);
      expect(st.lastRaw, isNull);
    });

    test('no enseña nada al Vocabulario', () async {
      container.read(dictationControllerProvider.notifier).armarPrueba();
      await dictar();
      expect(aprendizaje.correcciones, isEmpty);
      expect(aprendizaje.terminos, isEmpty);
    });
  });

  group('desarmar', () {
    test('tras desarmar, el dictado vuelve a ser de verdad', () async {
      // Es la avería que se llevaría por delante el trabajo del usuario: salir
      // del asistente con la prueba armada y que el siguiente dictado no se
      // pegue en ningún sitio.
      final ctrl = container.read(dictationControllerProvider.notifier);
      ctrl.armarPrueba();
      ctrl.desarmarPrueba();
      await dictar();

      expect(inyector.veces, 1);
      final entradas = await historial.page(const HistoryQuery());
      expect(entradas.single.source, DictationSource.dictado);
      expect(container.read(dictationControllerProvider).lastText, isNotNull);
    });

    test('armar y dictar dos veces seguidas sigue sin ensuciar el Historial',
        () async {
      final ctrl = container.read(dictationControllerProvider.notifier);
      ctrl.armarPrueba();
      await dictar();
      await dictar();
      expect(await historial.count(const HistoryQuery()), 0);
      expect(
        db.select('SELECT id FROM dictation_log').length,
        2,
      );
    });

    test('armar con un dictado en marcha no cambia nada a mitad', () async {
      final ctrl = container.read(dictationControllerProvider.notifier);
      await ctrl.startRecording();
      ctrl.armarPrueba(); // llega tarde: ya se está grabando
      await ctrl.stopAndProcess();

      // Se pegó y se apuntó como dictado de verdad, que es lo que era cuando
      // empezó.
      expect(inyector.veces, 1);
      expect(await historial.count(const HistoryQuery()), 1);
    });
  });
}
