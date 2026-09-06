import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/config.dart';
import '../../core/text_polish.dart';
import '../../platform/audio_capture.dart';
import '../../platform/global_hotkey_service.dart';
import '../../platform/platform_services.dart';
import '../../platform/sounds.dart';
import '../../platform/foreground_app.dart';
import '../../platform/text_injector.dart';
import '../../platform/whisper_engine.dart';
import '../history/history_repository.dart';
import '../learning/learning_service.dart';
import '../overlay/pill_popup_controller.dart';
import '../formatting/cloud_budget_provider.dart';
import '../formatting/formatter_provider.dart';
import '../formatting/text_formatter.dart';
import '../settings/settings_controller.dart';
import '../transcription/parakeet_engine.dart';
import '../translation/language_detect.dart';
import '../translation/translation_provider.dart';
import 'dictation_state.dart';

final dictationControllerProvider =
    NotifierProvider<DictationController, DictationState>(
  DictationController.new,
);

/// Orquesta el pipeline completo:
/// hotkey down -> grabar -> hotkey up -> transcribir (en isolate) ->
/// diccionario aprendido -> pulido -> formateo -> insertar -> registrar.
class DictationController extends Notifier<DictationState> {
  // Motor de voz activo (Parakeet o Whisper). Se elige en initialize(), tras
  // cargar los ajustes persistidos; nulo hasta entonces.
  WhisperEngine? _engine;
  late final AudioCapture _audio;
  late final TextInjector _injector;
  late final GlobalHotkeyService _hotkeys;
  late final LearningService _learning;

  /// La escritura en el Historial del último dictado, en curso o ya terminada.
  ///
  /// Se guarda el FUTURO, no el `id`: apuntar el dictado no bloquea el pegado,
  /// así que cuando el usuario corrige puede que la fila todavía no exista. Con
  /// el id a secas, esa corrección leería el de un dictado ANTERIOR y le
  /// cambiaría el texto al dictado equivocado.
  Future<int?>? _lastLogWrite;

  StreamSubscription<HotkeyEvent>? _hotkeySub;
  StreamSubscription<double>? _levelSub;

  /// El provider ya se ha liberado: la traducción de cortesía corre en segundo
  /// plano y puede volver mucho después, cuando ya no hay estado que tocar.
  bool _disposed = false;

  @override
  DictationState build() {
    _audio = ref.read(audioCaptureProvider);
    _injector = ref.read(textInjectorProvider);
    _hotkeys = ref.read(hotkeyServiceProvider);
    _learning = ref.read(learningServiceProvider);

    ref.onDispose(() {
      _disposed = true;
      _hotkeySub?.cancel();
      _levelSub?.cancel();
    });

    // Re-registra el atajo global si el usuario lo cambia en ajustes.
    // El guard `_hotkeySub == null` evita que la carga inicial de ajustes
    // (defaults -> persistidos) dispare un re-registro antes de que el registro
    // inicial de initialize() haya terminado (causaba una carrera).
    ref.listen(settingsProvider, (prev, next) {
      if (prev == null || _hotkeySub == null) return;
      if (prev.hotkey != next.hotkey ||
          prev.triggerMode != next.triggerMode ||
          prev.translateHotkey != next.translateHotkey) {
        unawaited(
          _reregisterHotkey(
            next.hotkey,
            next.triggerMode,
            next.translateHotkey,
          ),
        );
      }
    });

    // Arranque asíncrono (no bloquea la construcción del provider).
    scheduleMicrotask(initialize);
    return const DictationState();
  }

  /// Carga modelo, refresca aprendizaje y registra el atajo global.
  Future<void> initialize({String? modelPath}) async {
    try {
      // Despierta el libro de cuentas del gasto de nube al arrancar: hasta que
      // se ha leído de disco, el guardia corta por seguridad, y no queremos que
      // el primer dictado de la sesión se quede sin formatear por eso.
      ref.read(cloudSpendProvider);

      await _learning.refresh();

      // Espera a los ajustes persistidos para elegir el motor correcto
      // (Parakeet por defecto). whisperEngineProvider devuelve ese motor.
      await ref.read(settingsProvider.notifier).loaded;
      final engine = ref.read(whisperEngineProvider);
      _engine = engine;

      // La retención se aplica AL ARRANCAR, una vez, y no bloquea nada: si
      // solo se aplicara al abrir la pantalla del Historial, quien no la abre
      // nunca guardaría para siempre sin haberlo elegido.
      unawaited(_purgeOldHistory());

      final path = modelPath ?? await _resolveModelPath();

      // Vuelca el vocabulario aprendido ANTES de cargar: sherpa-onnx lee los
      // hotwords al construir el recognizer, no en caliente. Si esto se hiciera
      // después, el biasing entraría un arranque tarde y parecería que no
      // funciona.
      if (path != null && engine is ParakeetEngine) {
        await _learning.writeHotwordsFile(path);
      }

      if (path != null) {
        await engine.load(path);
      }
      final biasing =
          engine is ParakeetEngine ? '${engine.hotwordsActive}' : 'n/a';
      _logEngine('engine=${engine.runtimeType} loaded=${engine.isLoaded} '
          'path=$path hotwords=$biasing');

      _levelSub = _audio.amplitude.listen((lvl) {
        state = state.copyWith(level: lvl);
      });
    } catch (e) {
      debugPrint('[TF] init ERROR (modelo/aprendizaje): $e');
      _logEngine('INIT ERROR: $e');
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
        langCombo: settings.translateHotkey,
      );
      _hotkeySub = _hotkeys.events.listen(_onHotkeyEvent);
      debugPrint('[TF] atajo OK: ${settings.hotkey.describe()} '
          '(${settings.triggerMode.name})');
    } catch (e) {
      debugPrint('[TF] atajo NO disponible (no fatal): $e');
    }

    state = state.copyWith(
      status: (_engine?.isLoaded ?? false)
          ? DictationStatus.idle
          : DictationStatus.uninitialized,
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
      case HotkeyEventType.toggleLanguage:
        unawaited(toggleOutputLanguage());
    }
  }

  /// Alterna el idioma en el que se pega el dictado (español ⇄ inglés) y lo
  /// confirma en la píldora. Es el atajo Ctrl+Alt+T.
  Future<void> toggleOutputLanguage() async {
    final next =
        await ref.read(settingsProvider.notifier).toggleOutputLanguage();
    final popup = ref.read(pillPopupProvider.notifier);
    if (!ref.read(settingsProvider).translationEnabled) {
      // Sin traductor configurado el cambio no tendría efecto: lo decimos en
      // vez de dejar al usuario esperando un inglés que no va a llegar.
      popup.showNotice(
        'Traducción desactivada · actívala en Ajustes para pegar '
        'en ${next.label.toLowerCase()}',
      );
      return;
    }
    popup.showNotice('Pegar en ${next.label.toLowerCase()}');
  }

  Future<void> startRecording() async {
    if (state.isBusy || !(_engine?.isLoaded ?? false)) return;
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
      if (ref.read(settingsProvider).sounds) Sounds.recordStart();

      // Mientras hablas, el modelo de traducción se va cargando en segundo
      // plano: así al soltar el atajo ya está listo (~1 s en vez de ~40 s).
      if (ref.read(settingsProvider).translationEnabled) {
        unawaited(ref.read(translatorProvider).warmUp());
      }
    } catch (e) {
      state = state.copyWith(
        status: DictationStatus.error,
        error: 'Error al grabar: $e',
      );
    }
  }

  Future<void> stopAndProcess() async {
    if (state.status != DictationStatus.recording) return;
    if (ref.read(settingsProvider).sounds) Sounds.recordStop();
    try {
      final pcm = await _audio.stop();
      if (pcm.isEmpty) {
        state = state.copyWith(status: DictationStatus.idle, level: 0);
        return;
      }

      state = state.copyWith(status: DictationStatus.transcribing, level: 0);

      final engine = _engine;
      if (engine == null) {
        state = state.copyWith(status: DictationStatus.idle);
        return;
      }
      final initialPrompt = _learning.buildInitialPrompt();
      final relojMotor = Stopwatch()..start();
      final result = await engine.transcribe(
        pcm,
        language: AppConfig.defaultLanguage,
        initialPrompt: initialPrompt.isEmpty ? null : initialPrompt,
      );
      relojMotor.stop();

      if (result.isEmpty) {
        state = state.copyWith(status: DictationStatus.idle);
        return;
      }

      // 1) Diccionario determinista (cero alucinación). SIEMPRE primero, para
      //    fijar nombres propios antes de que el LLM "normalice" el texto.
      var finalText = _learning.applyDictionary(result.text);

      // 1b) Pulido determinista: puntuación dictada, ortografía ES (¿¡, espacios,
      //     mayúsculas) y números a dígitos. Para AMBOS motores; conmutable.
      if (ref.read(settingsProvider).textPolish) {
        finalText = TextPolish.apply(finalText);
      }

      // 2) Formateo con modelo de lenguaje (opcional). Es un paso NORMAL del
      //    pipeline: si el usuario eligió un camino de formateo, se ejecuta
      //    SIEMPRE. Ya no hay puerta de confianza — con Parakeet valía siempre
      //    y dejaba el formateo muerto sin decírselo a nadie.
      //    El aviso de que el texto sale del equipo lo dispara la PETICIÓN, no
      //    la intención: si el tope de gasto la frena o falta la clave, no se
      //    enciende nada porque no ha salido nada.
      final formato = await ref.read(textFormatterProvider).format(
            finalText,
            onLeavingDevice: () => state = state.copyWith(
              status: DictationStatus.sendingToCloud,
            ),
          );
      finalText = formato.text;

      // 3) Traducción español ⇄ inglés (opcional). Decide QUÉ texto se pega y
      //    cuál queda como "la otra versión" de la caja de la píldora.
      final plan = await _planOutput(finalText);

      // 4) Insertar en la app con foco. Quién lo tiene se lee AQUÍ, justo
      //    antes de pegar: un instante después el usuario ya pudo cambiar de
      //    ventana y el Historial apuntaría la app equivocada.
      state = state.copyWith(status: DictationStatus.injecting);
      final destino = ref.read(foregroundAppProvider).current();
      final injected = await _injector.insert(plan.text);

      state = state.copyWith(
        status: DictationStatus.idle,
        lastRaw: result.text,
        lastText: plan.text,
        lastLang: plan.lang,
        lastAlt: plan.altText,
        lastAltLang: plan.altLang,
        lastWasTranslated: plan.translated,
        // Si la otra versión aún no existe (está por traducir o no la habrá),
        // hay que BORRAR la del dictado anterior, no heredarla.
        clearAlt: plan.altText == null,
      );

      // Píldora: si la inserción quedó bloqueada (app elevada/foco perdido),
      // dejamos el texto en el portapapeles y avisamos para pegarlo a mano; si
      // fue bien, mostramos la transcripción normal.
      final popup = ref.read(pillPopupProvider.notifier);
      if (injected == InjectionResult.blocked) {
        await Clipboard.setData(ClipboardData(text: plan.text));
        popup.showCopied(
          plan.text,
          lang: plan.lang,
          altText: plan.altText ?? '',
          altLang: plan.altLang,
          altPending: plan.altPending,
          notice: formato.notice,
        );
      } else {
        popup.showTranscription(
          plan.text,
          lang: plan.lang,
          altText: plan.altText ?? '',
          altLang: plan.altLang,
          altPending: plan.altPending,
          notice: formato.notice,
        );
      }

      // La traducción de cortesía (dictaste ya en el idioma de salida) va en
      // segundo plano: el pegado no espera por ella.
      if (plan.altPending) {
        unawaited(_translateInBackground(plan.text, plan.lang!));
      }

      // Registrar el dictado. TODOS, no solo los que se corrigen: el
      // historial es el corpus con el que se mide el formateo, el vocabulario y
      // la velocidad. Si falla, el dictado ya está pegado y no se pierde nada:
      // no se le estropea el trabajo al usuario por no poder apuntarlo.
      _lastLogWrite = _logDictation(
        raw: result.text,
        pasted: plan.text,
        spoken: Duration(milliseconds: result.audioMs),
        transcribe: relojMotor.elapsed,
        format: formato,
        target: destino,
        translated: plan.translated,
      );

      // Arma la captura de ediciones externas: si el usuario corrige el texto
      // en su app y lo copia, lo aprenderemos (no-op si está desactivada).
      // Se salta cuando lo pegado es una TRADUCCIÓN: comparar el inglés que el
      // usuario corrija con el español que dictó ensuciaría el diccionario.
      if (!plan.translated) {
        ref.read(externalEditCaptureProvider).arm(result.text, plan.text);
      }
    } catch (e) {
      state = state.copyWith(
        status: DictationStatus.error,
        error: 'Error procesando dictado: $e',
      );
    }
  }

  /// Borra del Historial lo más viejo que la retención elegida.
  ///
  /// Con la retención en 0 (guardar para siempre) no borra nada: eso lo
  /// garantiza [HistoryRepository.purgeOlderThan], que trata el cero como
  /// «no toques nada» en vez de como «todo es viejo».
  Future<void> _purgeOldHistory() async {
    final days = ref.read(settingsProvider).historyRetentionDays;
    if (days <= 0) return;
    final history = ref.read(historyRepositoryProvider);
    if (history == null) return;
    try {
      final borradas = await history.purgeOlderThan(Duration(days: days));
      if (borradas > 0) {
        debugPrint('[TF] historial: $borradas entradas por retención ($days d)');
      }
    } catch (e) {
      debugPrint('[TF] no se pudo aplicar la retención del historial: $e');
    }
  }

  /// Apunta el dictado en el Historial.
  ///
  /// El `id` se guarda para que una corrección posterior arregle ESA fila en
  /// vez de dejar dos versiones del mismo dictado.
  Future<int?> _logDictation({
    required String raw,
    required String pasted,
    required Duration spoken,
    required Duration transcribe,
    required FormatResult format,
    required ForegroundApp? target,
    required bool translated,
  }) async {
    final history = ref.read(historyRepositoryProvider);
    if (history == null) return null;
    final settings = ref.read(settingsProvider);
    try {
      return await history.log(
        DictationRecord(
          raw: raw,
          finalText: pasted,
          at: DateTime.now(),
          engine: settings.engine.name,
          formatPath: settings.formatPath.name,
          // El modelo que de verdad iba a hacer el trabajo. Con el formateo
          // apagado no hay ninguno, y decir el que está configurado haría
          // pensar que corrió.
          formatModel: switch (settings.formatPath) {
            FormatPath.none => null,
            FormatPath.local => settings.formatModel,
            FormatPath.cloud => settings.cloudModel,
          },
          formatNotice: format.notice,
          appName: target?.appName,
          windowTitle: target?.windowTitle,
          spoken: spoken,
          transcribe: transcribe,
          format: format.elapsed,
          costUsd: format.costUsd,
          translated: translated,
        ),
      );
    } catch (e) {
      // El historial es un cuaderno, no el trabajo. Que no se pueda escribir se
      // anota y se sigue.
      debugPrint('[TF] no se pudo registrar el dictado: $e');
      return null;
    }
  }

  /// Decide qué texto se pega y cuál queda como "la otra versión".
  ///
  /// Solo hay UNA llamada al traductor por dictado como mucho:
  ///  - Dictaste en el idioma de salida (caso normal): se pega tal cual sin
  ///    esperar a nada, y la otra versión se traduce después en segundo plano.
  ///  - Dictaste en el otro idioma: hay que traducir ANTES de pegar, y la otra
  ///    versión es el original, que ya tenemos (traducción gratis).
  ///
  /// Con la traducción desactivada, o si el traductor falla, devuelve el texto
  /// dictado sin más: nunca se pega algo peor de lo que ya había.
  Future<_OutputPlan> _planOutput(String spokenText) async {
    final settings = ref.read(settingsProvider);
    if (!settings.translationEnabled) return _OutputPlan(text: spokenText);

    // Si el detector no se moja (texto muy corto, solo cifras…), asumimos que
    // dictaste en el idioma de salida: así el pegado nunca se bloquea por una
    // traducción que probablemente no hacía falta.
    final outLang = settings.outputLang;
    final spokenLang = detectLanguage(spokenText) ?? outLang;

    if (spokenLang == outLang) {
      return _OutputPlan(text: spokenText, lang: outLang, altPending: true);
    }

    state = state.copyWith(status: DictationStatus.translating);
    final translated = await ref
        .read(translatorProvider)
        .translate(spokenText, from: spokenLang, to: outLang);

    if (translated == null) {
      // Traductor caído o fuera de tiempo: pegamos lo dictado y avisamos.
      ref.read(pillPopupProvider.notifier).failAlt();
      return _OutputPlan(text: spokenText, lang: spokenLang);
    }

    return _OutputPlan(
      text: translated,
      lang: outLang,
      altText: spokenText,
      altLang: spokenLang,
      translated: true,
    );
  }

  /// Traduce en segundo plano la otra versión del dictado ya pegado. No toca el
  /// texto insertado: solo rellena la insignia ES/EN de la caja.
  Future<void> _translateInBackground(String text, TextLanguage from) async {
    final to = from.other;
    final alt = await ref.read(translatorProvider).translate(
          text,
          from: from,
          to: to,
        );
    if (_disposed) return;

    if (alt == null) {
      ref.read(pillPopupProvider.notifier).failAlt();
      return;
    }
    // Si mientras traducíamos ha entrado OTRO dictado, esta traducción ya no
    // corresponde al texto actual: se descarta en vez de mezclar dictados.
    if (state.lastText != text) return;

    state = state.copyWith(lastAlt: alt, lastAltLang: to);
    ref.read(pillPopupProvider.notifier).completeAlt(alt, to);
  }

  /// Entrada del sistema de automejora: el usuario corrige el último dictado
  /// (desde el panel/historial). Aprende del diff raw -> editado.
  ///
  /// Si lo último que se pegó fue una TRADUCCIÓN, no se aprende nada: el diff
  /// sería entre idiomas distintos y envenenaría el diccionario.
  Future<void> submitCorrection(String editedText) async {
    final raw = state.lastRaw;
    if (raw == null || raw == editedText) return;
    if (state.lastWasTranslated) {
      state = state.copyWith(lastText: editedText);
      return;
    }
    await _learning.recordEdit(raw, editedText);
    await _learning.refresh();
    // La corrección es lo que el dictado DEBERÍA haber dejado: se arregla su
    // fila del Historial, que si no se quedaría enseñando el texto malo como si
    // fuera lo bueno.
    final id = await _lastLogWrite;
    final history = ref.read(historyRepositoryProvider);
    if (id != null && history != null) {
      try {
        await history.updateFinalText(id, editedText);
      } catch (e) {
        debugPrint('[TF] no se pudo corregir la fila del historial: $e');
      }
    }
    state = state.copyWith(lastText: editedText);
  }

  /// Copia la última transcripción al portapapeles y muestra la confirmación en
  /// la píldora. Disparado por el atajo de copiar (Ctrl+Alt+V) o tocando la caja.
  Future<void> copyLastTranscription() async {
    final text = state.lastText;
    if (text == null || text.isEmpty) return;
    await _copy(text, state.lastLang, state.lastAlt, state.lastAltLang);
  }

  /// Copia la OTRA versión que la caja está enseñando ahora mismo. Es el botón
  /// ES/EN de la píldora.
  ///
  /// Lee del POPUP, no del dictado: así, tras copiar el inglés, la caja pasa a
  /// mostrarlo como principal y el botón (ya etiquetado ES) copia el español.
  /// Si leyera del dictado, pulsarlo dos veces copiaría dos veces lo mismo.
  Future<void> copyAlternate() async {
    final popup = ref.read(pillPopupProvider);
    if (popup.altText.isEmpty) return;
    await _copy(popup.altText, popup.altLang, popup.text, popup.lang);
  }

  /// Copia [text] y lo confirma en la píldora, dejando [alt] como la otra
  /// versión para poder seguir alternando entre las dos.
  Future<void> _copy(
    String text,
    TextLanguage? lang,
    String? alt,
    TextLanguage? altLang,
  ) async {
    try {
      await Clipboard.setData(ClipboardData(text: text));
      ref.read(pillPopupProvider.notifier).showCopied(
            text,
            lang: lang,
            altText: alt ?? '',
            altLang: altLang,
          );
    } catch (e) {
      debugPrint('[Pronto] no se pudo copiar al portapapeles: $e');
    }
  }

  Future<void> _reregisterHotkey(
    HotkeyCombo combo,
    TriggerMode mode,
    HotkeyCombo langCombo,
  ) async {
    try {
      await _hotkeys.unregister();
      await _hotkeys.register(
        combo,
        mode: mode,
        copyCombo: HotkeyCombo.copyDefault,
        langCombo: langCombo,
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

  /// Diagnóstico: registra el resultado de cargar el motor en
  /// `%TEMP%\pronto_engine.log` (como los logs de crash/update). Útil para
  /// verificar qué motor cargó sin ver la UI.
  void _logEngine(String msg) {
    try {
      final f = File('${Directory.systemTemp.path}/pronto_engine.log');
      f.writeAsStringSync(
        '${DateTime.now().toIso8601String()}  $msg\n',
        mode: FileMode.append,
        flush: true,
      );
    } catch (_) {}
  }
}

/// Qué se pega tras un dictado y qué queda como "la otra versión".
class _OutputPlan {
  /// El texto que se inserta en la app con foco.
  final String text;

  /// Idioma de [text]; `null` si la traducción está desactivada.
  final TextLanguage? lang;

  /// La otra versión, si ya la tenemos. `null` = no hay (o está por llegar).
  final String? altText;
  final TextLanguage? altLang;

  /// La otra versión se está traduciendo en segundo plano.
  final bool altPending;

  /// [text] es una traducción, no lo que se dictó.
  final bool translated;

  const _OutputPlan({
    required this.text,
    this.lang,
    this.altText,
    this.altLang,
    this.altPending = false,
    this.translated = false,
  });
}
