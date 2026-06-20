import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/config.dart';
import '../../platform/audio_capture.dart';
import '../../platform/global_hotkey_service.dart';
import '../../platform/platform_services.dart';
import '../../platform/text_injector.dart';
import '../../platform/whisper_engine.dart';
import '../learning/learning_service.dart';
import '../overlay/pill_popup_controller.dart';
import '../post_correction/llm_provider.dart';
import '../settings/settings_controller.dart';
import 'dictation_state.dart';

final dictationControllerProvider =
    NotifierProvider<DictationController, DictationState>(
  DictationController.new,
);

/// Orquesta el pipeline completo:
/// hotkey down -> grabar -> hotkey up -> transcribir (en isolate) ->
/// diccionario aprendido -> [LLM opcional] -> insertar -> registrar.
class DictationController extends Notifier<DictationState> {
  late final WhisperEngine _engine;
  late final AudioCapture _audio;
  late final TextInjector _injector;
  late final GlobalHotkeyService _hotkeys;
  late final LearningService _learning;

  StreamSubscription<HotkeyEvent>? _hotkeySub;
  StreamSubscription<double>? _levelSub;

  @override
  DictationState build() {
    _engine = ref.read(whisperEngineProvider);
    _audio = ref.read(audioCaptureProvider);
    _injector = ref.read(textInjectorProvider);
    _hotkeys = ref.read(hotkeyServiceProvider);
    _learning = ref.read(learningServiceProvider);

    ref.onDispose(() {
      _hotkeySub?.cancel();
      _levelSub?.cancel();
    });

    // Re-registra el atajo global si el usuario lo cambia en ajustes.
    // El guard `_hotkeySub == null` evita que la carga inicial de ajustes
    // (defaults -> persistidos) dispare un re-registro antes de que el registro
    // inicial de initialize() haya terminado (causaba una carrera).
    ref.listen(settingsProvider, (prev, next) {
      if (prev == null || _hotkeySub == null) return;
      if (prev.hotkey != next.hotkey || prev.triggerMode != next.triggerMode) {
        unawaited(_reregisterHotkey(next.hotkey, next.triggerMode));
      }
    });

    // Arranque asíncrono (no bloquea la construcción del provider).
    scheduleMicrotask(initialize);
    return const DictationState();
  }

  /// Carga modelo, refresca aprendizaje y registra el atajo global.
  Future<void> initialize({String? modelPath}) async {
    try {
      await _learning.refresh();

      final path = modelPath ?? await _resolveModelPath();
      if (path != null) {
        await _engine.load(path);
      }

      _levelSub = _audio.amplitude.listen((lvl) {
        state = state.copyWith(level: lvl);
      });
    } catch (e) {
      debugPrint('[TF] init ERROR (modelo/aprendizaje): $e');
      state = state.copyWith(
        status: DictationStatus.error,
        error: 'No se pudo inicializar: $e',
      );
      return;
    }

    // El registro del atajo NO debe poner la luz en rojo si falla: el resto de
    // la app sigue funcionando (dictado manual desde el panel).
    try {
      final settings = ref.read(settingsProvider);
      await _hotkeys.register(
        settings.hotkey,
        mode: settings.triggerMode,
        copyCombo: HotkeyCombo.copyDefault,
      );
      _hotkeySub = _hotkeys.events.listen(_onHotkeyEvent);
      debugPrint('[TF] atajo OK: ${settings.hotkey.describe()} '
          '(${settings.triggerMode.name})');
    } catch (e) {
      debugPrint('[TF] atajo NO disponible (no fatal): $e');
    }

    state = state.copyWith(
      status:
          _engine.isLoaded ? DictationStatus.idle : DictationStatus.uninitialized,
    );
    debugPrint('[TF] init done -> status=${state.status.name}');
  }

  void _onHotkeyEvent(HotkeyEvent e) {
    debugPrint('[TF] hotkey ${e.type.name}');
    switch (e.type) {
      case HotkeyEventType.down:
        unawaited(startRecording());
      case HotkeyEventType.up:
        unawaited(stopAndProcess());
      case HotkeyEventType.copy:
        unawaited(copyLastTranscription());
    }
  }

  Future<void> startRecording() async {
    if (state.isBusy || !_engine.isLoaded) return;
    try {
      if (!await _audio.hasPermission()) {
        state = state.copyWith(
          status: DictationStatus.error,
          error: 'Sin permiso de micrófono (Configuración > Privacidad).',
        );
        return;
      }
      await _audio.start();
      state = state.copyWith(status: DictationStatus.recording);
    } catch (e) {
      state = state.copyWith(
        status: DictationStatus.error,
        error: 'Error al grabar: $e',
      );
    }
  }

  Future<void> stopAndProcess() async {
    if (state.status != DictationStatus.recording) return;
    try {
      final pcm = await _audio.stop();
      if (pcm.isEmpty) {
        state = state.copyWith(status: DictationStatus.idle, level: 0);
        return;
      }

      state = state.copyWith(status: DictationStatus.transcribing, level: 0);

      final initialPrompt = _learning.buildInitialPrompt();
      final result = await _engine.transcribe(
        pcm,
        language: AppConfig.defaultLanguage,
        initialPrompt: initialPrompt.isEmpty ? null : initialPrompt,
      );

      if (result.isEmpty) {
        state = state.copyWith(status: DictationStatus.idle);
        return;
      }

      // 1) Diccionario determinista (cero alucinación). SIEMPRE primero, para
      //    fijar nombres propios antes de que el LLM "normalice" el texto.
      var finalText = _learning.applyDictionary(result.text);

      // 2) Post-corrección LLM (opcional). Hace passthrough si está desactivada
      //    o si la confianza es alta; ante error devuelve el texto sin tocar.
      finalText = await ref
          .read(llmCorrectorProvider)
          .correct(finalText, result.avgLogProb);

      // 3) Insertar en la app con foco.
      state = state.copyWith(status: DictationStatus.injecting);
      await _injector.insert(finalText);

      state = state.copyWith(
        status: DictationStatus.idle,
        lastRaw: result.text,
        lastText: finalText,
      );

      // Muestra la cajita con la transcripción a la derecha del punto (píldora).
      ref.read(pillPopupProvider.notifier).showTranscription(finalText);

      // Arma la captura de ediciones externas: si el usuario corrige el texto
      // en su app y lo copia, lo aprenderemos (no-op si está desactivada).
      ref.read(externalEditCaptureProvider).arm(result.text, finalText);
    } catch (e) {
      state = state.copyWith(
        status: DictationStatus.error,
        error: 'Error procesando dictado: $e',
      );
    }
  }

  /// Entrada del sistema de automejora: el usuario corrige el último dictado
  /// (desde el panel/historial). Aprende del diff raw -> editado.
  Future<void> submitCorrection(String editedText) async {
    final raw = state.lastRaw;
    if (raw == null || raw == editedText) return;
    await _learning.recordEdit(raw, editedText);
    await _learning.refresh();
    state = state.copyWith(lastText: editedText);
  }

  /// Copia la última transcripción al portapapeles y muestra la confirmación en
  /// la píldora. Disparado por el atajo de copiar (Ctrl+Alt+V) o tocando la caja.
  Future<void> copyLastTranscription() async {
    final text = state.lastText;
    if (text == null || text.isEmpty) return;
    try {
      await Clipboard.setData(ClipboardData(text: text));
      ref.read(pillPopupProvider.notifier).showCopied(text);
    } catch (e) {
      debugPrint('[Pronto] no se pudo copiar al portapapeles: $e');
    }
  }

  Future<void> _reregisterHotkey(HotkeyCombo combo, TriggerMode mode) async {
    try {
      await _hotkeys.unregister();
      await _hotkeys.register(
        combo,
        mode: mode,
        copyCombo: HotkeyCombo.copyDefault,
      );
      debugPrint('[TF] atajo re-registrado: ${combo.describe()} (${mode.name})');
    } catch (e) {
      // No fatal: no ponemos la luz en rojo por un problema del atajo.
      debugPrint('[TF] re-registro de atajo falló: $e');
    }
  }

  Future<String?> _resolveModelPath() async {
    try {
      return await ref.read(modelPathResolverProvider.future);
    } catch (e) {
      debugPrint('No se pudo resolver la ruta del modelo: $e');
      return null;
    }
  }
}
