import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:launch_at_startup/launch_at_startup.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../../core/config.dart';
import '../../platform/global_hotkey_service.dart';
import '../../platform/text_injector.dart';
import '../formatting/cloud_cost.dart';
import '../formatting/text_formatter.dart';
import '../translation/language_detect.dart';

/// Provider global de ajustes de la app.
final settingsProvider = NotifierProvider<SettingsController, AppSettings>(
  SettingsController.new,
);

/// Motor de reconocimiento de voz seleccionable en Ajustes.
/// - [parakeet]: NVIDIA Parakeet (transducer) vía sherpa-onnx. Rápido, puntúa
///   solo, mejor con jerga/nombres propios. POR DEFECTO.
/// - [whisper]: whisper.cpp (modelo small). Acepta biasing por initial_prompt.
enum SpeechEngine { parakeet, whisper }

/// Modelo inmutable con todos los ajustes persistentes de Pronto.
///
/// Se serializa a una única clave JSON en [SharedPreferences] para mantener
/// la persistencia simple y atómica.
@immutable
class AppSettings {
  /// Nombre de fichero del modelo Whisper (p.ej. 'ggml-small.bin').
  final String modelFile;

  /// Idioma de transcripción: 'es', 'en' o 'auto'.
  final String language;

  /// Cómo se inserta el texto en la app con foco.
  final InjectionMode injectionMode;

  /// Cómo se dispara el dictado (mantener pulsado / alternar).
  final TriggerMode triggerMode;

  /// Si la app debe arrancar con el sistema.
  final bool autostart;

  /// Por dónde se formatea el dictado: sin formateo, local o nube.
  ///
  /// Sustituye al viejo `llmEnabled`. Son dos cosas distintas: el usuario elige
  /// SI quiere formateo (esto), y cuando lo ha elegido se ejecuta SIEMPRE — ya
  /// no hay puerta de confianza que se lo salte por detrás.
  final FormatPath formatPath;

  /// URL base del servidor LLM local (Ollama). La comparten el formateo y la
  /// traducción.
  final String llmBaseUrl;

  /// Modelo local con el que se formatea.
  final String formatModel;

  /// Modelo de nube con el que se formatea. Siempre uno de [kCloudPrices]: la
  /// lista blanca es lo que permite que el tope de gasto sepa lo que cuesta.
  final String cloudModel;

  /// Tope de gasto de nube por día, en dólares.
  final double cloudDayCapUsd;

  /// Tope de gasto de nube por mes, en dólares.
  final double cloudMonthCapUsd;

  /// Si se capturan ediciones externas (lo que copias en otra app tras dictar)
  /// para alimentar el aprendizaje. Conservador y filtrado por similitud.
  final bool captureExternalEdits;

  /// Atajo global para dictar (push-to-talk).
  final HotkeyCombo hotkey;

  /// Escala del punto flotante (1.0 = tamaño por defecto). Ajustable con un
  /// slider en Ajustes.
  final double pillScale;

  /// Motor de voz a usar. Parakeet por defecto.
  final SpeechEngine engine;

  /// Pulir el texto dictado: puntuación dictada, ortografía ES (¿¡, espacios,
  /// mayúsculas) y números a dígitos. Determinista, para ambos motores.
  final bool textPolish;

  /// Reproducir un sonido corto al empezar y parar de grabar.
  final bool sounds;

  /// Id del micrófono elegido, o null = micrófono por defecto del sistema.
  final String? micDeviceId;

  /// Si la traducción español ⇄ inglés al dictar está activada. Necesita un
  /// servidor LLM (Ollama local por defecto), por eso viene apagada.
  final bool translationEnabled;

  /// Idioma en el que se PEGA el dictado ('es' o 'en'). Es el "idioma por
  /// defecto": si dictas en el otro, Pronto traduce antes de pegar. Lo alterna
  /// [translateHotkey] sin abrir Ajustes.
  final String outputLanguage;

  /// Modelo LLM usado para traducir (contra [llmBaseUrl]).
  final String translationModel;

  /// Atajo global que alterna el idioma de salida (español ⇄ inglés).
  final HotkeyCombo translateHotkey;

  /// Días que se guarda el Historial. **0 = para siempre**, y eso hay que
  /// elegirlo a mano: el historial lleva dentro todo lo que has dictado nunca,
  /// sin cifrar, así que una retención infinita por defecto sería un olvido
  /// disfrazado de decisión (ADR 0009).
  final int historyRetentionDays;

  const AppSettings({
    required this.modelFile,
    required this.language,
    required this.injectionMode,
    required this.triggerMode,
    required this.autostart,
    required this.formatPath,
    required this.llmBaseUrl,
    required this.formatModel,
    required this.captureExternalEdits,
    required this.hotkey,
    this.cloudModel = AppConfig.defaultFormatCloudModel,
    this.cloudDayCapUsd = AppConfig.defaultCloudDayCapUsd,
    this.cloudMonthCapUsd = AppConfig.defaultCloudMonthCapUsd,
    this.pillScale = 1.0,
    this.engine = SpeechEngine.parakeet,
    this.textPolish = true,
    this.sounds = true,
    this.micDeviceId,
    this.translationEnabled = false,
    this.outputLanguage = AppConfig.defaultOutputLanguage,
    this.translationModel = AppConfig.defaultTranslationModel,
    this.translateHotkey = HotkeyCombo.translateDefault,
    this.historyRetentionDays = AppConfig.defaultHistoryRetentionDays,
  });

  /// Idioma en el que se pega el dictado, ya validado ('es' o 'en'; cae a
  /// español si el valor persistido no se reconoce).
  TextLanguage get outputLang =>
      TextLanguage.fromCode(outputLanguage) ?? TextLanguage.es;

  /// Valores por defecto, tomados de [AppConfig] cuando existen.
  factory AppSettings.defaults() => const AppSettings(
        modelFile: AppConfig.defaultModelFile,
        language: AppConfig.defaultLanguage,
        injectionMode: InjectionMode.clipboardPaste,
        triggerMode: TriggerMode.toggle,
        autostart: false,
        formatPath: FormatPath.none,
        // Ollama local por defecto. Solo la RAÍZ del servidor: TextFormatter
        // añade la ruta (`/api/chat` en Ollama, `/v1/chat/completions` en nube).
        llmBaseUrl: 'http://localhost:11434',
        formatModel: AppConfig.defaultFormatLocalModel,
        captureExternalEdits: true,
        hotkey: HotkeyCombo.defaultCombo,
      );

  AppSettings copyWith({
    String? modelFile,
    String? language,
    InjectionMode? injectionMode,
    TriggerMode? triggerMode,
    bool? autostart,
    FormatPath? formatPath,
    String? llmBaseUrl,
    String? formatModel,
    String? cloudModel,
    double? cloudDayCapUsd,
    double? cloudMonthCapUsd,
    bool? captureExternalEdits,
    HotkeyCombo? hotkey,
    double? pillScale,
    SpeechEngine? engine,
    bool? textPolish,
    bool? sounds,
    // Nullable con "reset": por defecto conserva el valor actual; pasa
    // `resetMicDevice: true` para volver al micrófono por defecto (null).
    String? micDeviceId,
    bool resetMicDevice = false,
    bool? translationEnabled,
    String? outputLanguage,
    String? translationModel,
    HotkeyCombo? translateHotkey,
    int? historyRetentionDays,
  }) {
    return AppSettings(
      modelFile: modelFile ?? this.modelFile,
      language: language ?? this.language,
      injectionMode: injectionMode ?? this.injectionMode,
      triggerMode: triggerMode ?? this.triggerMode,
      autostart: autostart ?? this.autostart,
      formatPath: formatPath ?? this.formatPath,
      llmBaseUrl: llmBaseUrl ?? this.llmBaseUrl,
      formatModel: formatModel ?? this.formatModel,
      cloudModel: cloudModel ?? this.cloudModel,
      cloudDayCapUsd: cloudDayCapUsd ?? this.cloudDayCapUsd,
      cloudMonthCapUsd: cloudMonthCapUsd ?? this.cloudMonthCapUsd,
      captureExternalEdits: captureExternalEdits ?? this.captureExternalEdits,
      hotkey: hotkey ?? this.hotkey,
      pillScale: pillScale ?? this.pillScale,
      engine: engine ?? this.engine,
      textPolish: textPolish ?? this.textPolish,
      sounds: sounds ?? this.sounds,
      micDeviceId: resetMicDevice ? null : (micDeviceId ?? this.micDeviceId),
      translationEnabled: translationEnabled ?? this.translationEnabled,
      outputLanguage: outputLanguage ?? this.outputLanguage,
      translationModel: translationModel ?? this.translationModel,
      translateHotkey: translateHotkey ?? this.translateHotkey,
      historyRetentionDays: historyRetentionDays ?? this.historyRetentionDays,
    );
  }

  Map<String, dynamic> toJson() => {
        'modelFile': modelFile,
        'language': language,
        'injectionMode': injectionMode.name,
        'triggerMode': triggerMode.name,
        'autostart': autostart,
        'formatPath': formatPath.name,
        'llmBaseUrl': llmBaseUrl,
        'formatModel': formatModel,
        'cloudModel': cloudModel,
        'cloudDayCapUsd': cloudDayCapUsd,
        'cloudMonthCapUsd': cloudMonthCapUsd,
        'captureExternalEdits': captureExternalEdits,
        'hotkey': hotkey.toJson(),
        'pillScale': pillScale,
        'engine': engine.name,
        'textPolish': textPolish,
        'sounds': sounds,
        'micDeviceId': micDeviceId,
        'translationEnabled': translationEnabled,
        'outputLanguage': outputLanguage,
        'translationModel': translationModel,
        'translateHotkey': translateHotkey.toJson(),
        'historyRetentionDays': historyRetentionDays,
      };

  /// Reconstruye desde JSON tolerando claves ausentes o valores inválidos:
  /// cualquier campo que falte o no se reconozca cae al valor por defecto.
  factory AppSettings.fromJson(Map<String, dynamic> j) {
    final d = AppSettings.defaults();
    return AppSettings(
      modelFile: j['modelFile'] as String? ?? d.modelFile,
      language: j['language'] as String? ?? d.language,
      injectionMode: _injectionFromName(j['injectionMode'] as String?) ??
          d.injectionMode,
      triggerMode: _triggerFromName(j['triggerMode'] as String?) ??
          d.triggerMode,
      autostart: j['autostart'] as bool? ?? d.autostart,
      formatPath: _formatPathFromJson(j) ?? d.formatPath,
      llmBaseUrl: j['llmBaseUrl'] as String? ?? d.llmBaseUrl,
      formatModel: _formatModelFromJson(j) ?? d.formatModel,
      // Lista blanca: un modelo de nube que no conocemos NO se acepta, porque
      // el tope de gasto no sabría lo que cuesta y dejaría pasar cualquier cosa.
      cloudModel: kCloudPrices.containsKey(j['cloudModel'])
          ? j['cloudModel'] as String
          : d.cloudModel,
      cloudDayCapUsd: _capFromJson(j['cloudDayCapUsd'], d.cloudDayCapUsd),
      cloudMonthCapUsd: _capFromJson(j['cloudMonthCapUsd'], d.cloudMonthCapUsd),
      captureExternalEdits:
          j['captureExternalEdits'] as bool? ?? d.captureExternalEdits,
      hotkey: j['hotkey'] is Map
          ? HotkeyCombo.fromJson(Map<String, dynamic>.from(j['hotkey'] as Map))
          : d.hotkey,
      pillScale: (j['pillScale'] as num?)?.toDouble() ?? d.pillScale,
      engine: _engineFromName(j['engine'] as String?) ?? d.engine,
      textPolish: j['textPolish'] as bool? ?? d.textPolish,
      sounds: j['sounds'] as bool? ?? d.sounds,
      micDeviceId: j['micDeviceId'] as String?,
      translationEnabled:
          j['translationEnabled'] as bool? ?? d.translationEnabled,
      // Solo aceptamos 'es'/'en'; cualquier otra cosa cae al defecto.
      outputLanguage: TextLanguage.fromCode(j['outputLanguage'] as String?)
              ?.code ??
          d.outputLanguage,
      translationModel: j['translationModel'] as String? ?? d.translationModel,
      translateHotkey: j['translateHotkey'] is Map
          ? HotkeyCombo.fromJson(
              Map<String, dynamic>.from(j['translateHotkey'] as Map),
            )
          : d.translateHotkey,
      historyRetentionDays: _retentionFromJson(
        j['historyRetentionDays'],
        d.historyRetentionDays,
      ),
    );
  }

  /// Días de retención del Historial. Un valor negativo, disparatado o que no
  /// es un número cae al defecto: nunca a cero, que aquí significa «guardar
  /// para siempre». El tope se aplica también al LEER, no solo al escribir: un
  /// fichero de ajustes editado a mano no pasa por el setter.
  static int _retentionFromJson(Object? value, int fallback) {
    if (value is! num || !value.isFinite) return fallback;
    final days = value.toInt();
    if (days < 0 || days > AppConfig.maxHistoryRetentionDays) return fallback;
    return days;
  }

  /// Lee el camino de formateo, MIGRANDO los ajustes viejos.
  ///
  /// Hasta la 0.7.1 esto era `llmEnabled` (un booleano) y el destino siempre era
  /// Ollama local. Quien lo tuviera encendido se queda en local; quien no, sin
  /// formateo.
  ///
  /// Desde PRNT-11 la nube SÍ es un camino elegible, con sus guardias puestos
  /// (clave en el almacén de credenciales, tope de gasto comprobado al entrar y
  /// aviso en la Píldora). Encenderla editando el fichero a mano no salta
  /// ninguno de los tres: sin clave no sale ni una petición.
  static FormatPath? _formatPathFromJson(Map<String, dynamic> j) {
    final name = j['formatPath'] as String?;
    if (name != null) {
      for (final v in FormatPath.values) {
        if (v.name == name) return v;
      }
      return null;
    }
    final viejo = j['llmEnabled'] as bool?;
    if (viejo == null) return null;
    return viejo ? FormatPath.local : FormatPath.none;
  }

  /// Lee el modelo de formateo, migrando desde `llmModel`.
  ///
  /// `qwen3:4b` era el defecto viejo y NO existe en el registro público de
  /// Ollama: encender el formateo daba 404. Si alguien lo tiene persistido, se
  /// le cambia por el defecto actual en vez de arrastrarle un modelo que nunca
  /// pudo funcionar.
  static String? _formatModelFromJson(Map<String, dynamic> j) {
    final actual = j['formatModel'] as String?;
    if (actual != null && actual.isNotEmpty) return actual;
    final viejo = j['llmModel'] as String?;
    if (viejo == null || viejo.isEmpty || viejo == 'qwen3:4b') return null;
    return viejo;
  }

  /// Lee un tope de gasto persistido. Un valor que no es un número, es
  /// negativo o no es finito cae al defecto: un tope roto no puede convertirse
  /// en "sin tope".
  static double _capFromJson(Object? v, double porDefecto) {
    if (v is num && v.isFinite && v >= 0) {
      // Un tope disparatado en el fichero se recorta al techo en vez de
      // respetarse: se ve en Ajustes y no hay forma de gastar de más a ciegas.
      return v.toDouble().clamp(0.0, AppConfig.cloudMaxCapUsd);
    }
    return porDefecto;
  }

  static SpeechEngine? _engineFromName(String? name) {
    if (name == null) return null;
    for (final v in SpeechEngine.values) {
      if (v.name == name) return v;
    }
    return null;
  }

  static InjectionMode? _injectionFromName(String? name) {
    if (name == null) return null;
    for (final v in InjectionMode.values) {
      if (v.name == name) return v;
    }
    return null;
  }

  static TriggerMode? _triggerFromName(String? name) {
    if (name == null) return null;
    for (final v in TriggerMode.values) {
      if (v.name == name) return v;
    }
    return null;
  }

  @override
  bool operator ==(Object other) =>
      other is AppSettings &&
      other.modelFile == modelFile &&
      other.language == language &&
      other.injectionMode == injectionMode &&
      other.triggerMode == triggerMode &&
      other.autostart == autostart &&
      other.formatPath == formatPath &&
      other.llmBaseUrl == llmBaseUrl &&
      other.formatModel == formatModel &&
      other.cloudModel == cloudModel &&
      other.cloudDayCapUsd == cloudDayCapUsd &&
      other.cloudMonthCapUsd == cloudMonthCapUsd &&
      other.captureExternalEdits == captureExternalEdits &&
      other.hotkey == hotkey &&
      other.pillScale == pillScale &&
      other.engine == engine &&
      other.textPolish == textPolish &&
      other.sounds == sounds &&
      other.micDeviceId == micDeviceId &&
      other.translationEnabled == translationEnabled &&
      other.outputLanguage == outputLanguage &&
      other.translationModel == translationModel &&
      other.translateHotkey == translateHotkey &&
      other.historyRetentionDays == historyRetentionDays;

  @override
  int get hashCode => Object.hashAll([
        modelFile,
        language,
        injectionMode,
        triggerMode,
        autostart,
        formatPath,
        llmBaseUrl,
        formatModel,
        cloudModel,
        cloudDayCapUsd,
        cloudMonthCapUsd,
        captureExternalEdits,
        hotkey,
        pillScale,
        engine,
        textPolish,
        sounds,
        micDeviceId,
        translationEnabled,
        outputLanguage,
        translationModel,
        translateHotkey,
        historyRetentionDays,
      ]);
}

/// Controla y persiste los ajustes de la app con [SharedPreferences].
///
/// La carga es perezosa: [build] devuelve los valores por defecto al instante
/// y dispara una carga asíncrona que actualiza el estado cuando termina, para
/// no bloquear la construcción del provider.
class SettingsController extends Notifier<AppSettings> {
  /// Clave única bajo la que se serializa todo el JSON de ajustes.
  static const String _prefsKey = 'app_settings';

  final Completer<void> _loadedCompleter = Completer<void>();

  /// Se completa cuando los ajustes PERSISTIDOS ya se han cargado (o han
  /// fallado). Útil para que el arranque (p. ej. elegir motor de voz) espere al
  /// valor real en vez de quedarse con el defecto.
  Future<void> get loaded => _loadedCompleter.future;

  @override
  AppSettings build() {
    // Carga perezosa: arrancamos con defaults y rellenamos al leer prefs.
    scheduleMicrotask(_load);
    return AppSettings.defaults();
  }

  Future<void> _load() async {
    try {
      final prefs = await SharedPreferences.getInstance();
      final raw = prefs.getString(_prefsKey);
      if (raw == null || raw.isEmpty) return;
      final map = jsonDecode(raw) as Map<String, dynamic>;
      state = AppSettings.fromJson(map);
      // Pronto usa solo Parakeet: migra en memoria cualquier ajuste antiguo de
      // Whisper para que resolutor de modelo y UI queden consistentes.
      if (state.engine == SpeechEngine.whisper) {
        state = state.copyWith(engine: SpeechEngine.parakeet);
      }
      // Re-sincroniza el autostart del sistema con lo persistido.
      await _applyAutostart(state.autostart);
    } catch (e) {
      debugPrint('No se pudieron cargar los ajustes: $e');
      // Mantenemos los valores por defecto ya presentes en [state].
    } finally {
      if (!_loadedCompleter.isCompleted) _loadedCompleter.complete();
    }
  }

  /// Persiste el estado actual en disco.
  Future<void> _save() async {
    try {
      final prefs = await SharedPreferences.getInstance();
      await prefs.setString(_prefsKey, jsonEncode(state.toJson()));
    } catch (e) {
      debugPrint('No se pudieron guardar los ajustes: $e');
    }
  }

  // --- Actualizadores por campo (actualizan estado + persisten) ---

  Future<void> setModelFile(String value) async {
    state = state.copyWith(modelFile: value);
    await _save();
  }

  Future<void> setLanguage(String value) async {
    state = state.copyWith(language: value);
    await _save();
  }

  Future<void> setInjectionMode(InjectionMode value) async {
    state = state.copyWith(injectionMode: value);
    await _save();
  }

  Future<void> setTriggerMode(TriggerMode value) async {
    state = state.copyWith(triggerMode: value);
    await _save();
  }

  Future<void> setAutostart(bool value) async {
    state = state.copyWith(autostart: value);
    await _save();
    await _applyAutostart(value);
  }

  /// Cambia por dónde se formatea el dictado.
  Future<void> setFormatPath(FormatPath value) async {
    state = state.copyWith(formatPath: value);
    await _save();
  }

  Future<void> setLlmBaseUrl(String value) async {
    state = state.copyWith(llmBaseUrl: value);
    await _save();
  }

  Future<void> setFormatModel(String value) async {
    state = state.copyWith(formatModel: value);
    await _save();
  }

  /// Cambia el modelo de nube. Solo acepta uno de la lista blanca: fuera de
  /// ella el tope de gasto no sabría con qué precio contar.
  Future<void> setCloudModel(String value) async {
    if (!kCloudPrices.containsKey(value)) return;
    state = state.copyWith(cloudModel: value);
    await _save();
  }

  /// Fija el tope de gasto diario, en dólares. Un valor negativo, o por encima
  /// del techo de [AppConfig.cloudMaxCapUsd], se ignora: el campo vuelve al
  /// valor bueno en vez de aceptar un 100 que se quería 1,00.
  Future<void> setCloudDayCapUsd(double value) async {
    if (!value.isFinite || value < 0 || value > AppConfig.cloudMaxCapUsd) {
      return;
    }
    state = state.copyWith(cloudDayCapUsd: value);
    await _save();
  }

  /// Fija el tope de gasto mensual, en dólares. Mismo techo que el diario.
  Future<void> setCloudMonthCapUsd(double value) async {
    if (!value.isFinite || value < 0 || value > AppConfig.cloudMaxCapUsd) {
      return;
    }
    state = state.copyWith(cloudMonthCapUsd: value);
    await _save();
  }

  /// Cambia los días que se guarda el Historial. 0 = para siempre. Un valor
  /// absurdo (negativo o de años) se ignora en vez de aceptarse a medias.
  Future<void> setHistoryRetentionDays(int value) async {
    if (value < 0 || value > AppConfig.maxHistoryRetentionDays) return;
    state = state.copyWith(historyRetentionDays: value);
    await _save();
  }

  Future<void> setCaptureExternalEdits(bool value) async {
    state = state.copyWith(captureExternalEdits: value);
    await _save();
  }

  Future<void> setHotkey(HotkeyCombo value) async {
    state = state.copyWith(hotkey: value);
    await _save();
  }

  /// Cambia el motor de voz (Parakeet / Whisper). Surte efecto al reiniciar la
  /// app (el motor se carga al arrancar y queda residente).
  Future<void> setEngine(SpeechEngine value) async {
    state = state.copyWith(engine: value);
    await _save();
  }

  Future<void> setTextPolish(bool value) async {
    state = state.copyWith(textPolish: value);
    await _save();
  }

  Future<void> setSounds(bool value) async {
    state = state.copyWith(sounds: value);
    await _save();
  }

  Future<void> setTranslationEnabled(bool value) async {
    state = state.copyWith(translationEnabled: value);
    await _save();
  }

  /// Fija el idioma en el que se pega el dictado ('es' o 'en').
  Future<void> setOutputLanguage(TextLanguage value) async {
    state = state.copyWith(outputLanguage: value.code);
    await _save();
  }

  /// Alterna el idioma de salida (español ⇄ inglés) y devuelve el nuevo. Lo usa
  /// el atajo global de idioma.
  Future<TextLanguage> toggleOutputLanguage() async {
    final next = state.outputLang.other;
    await setOutputLanguage(next);
    return next;
  }

  Future<void> setTranslationModel(String value) async {
    state = state.copyWith(translationModel: value);
    await _save();
  }

  Future<void> setTranslateHotkey(HotkeyCombo value) async {
    state = state.copyWith(translateHotkey: value);
    await _save();
  }

  /// Fija el micrófono a usar. `null` = micrófono por defecto del sistema.
  /// Surte efecto en la siguiente grabación (el dispositivo se resuelve en
  /// [AudioCapture.start]).
  Future<void> setMicDeviceId(String? value) async {
    state = state.copyWith(micDeviceId: value, resetMicDevice: value == null);
    await _save();
  }

  /// Vista previa en vivo del tamaño del punto (sin persistir; se usa mientras
  /// se arrastra el slider).
  void previewPillScale(double value) {
    state = state.copyWith(pillScale: value);
  }

  /// Fija y persiste el tamaño del punto flotante.
  Future<void> setPillScale(double value) async {
    state = state.copyWith(pillScale: value);
    await _save();
  }

  /// Aplica el ajuste de autostart al sistema operativo.
  ///
  /// Se envuelve en try/catch porque depende del registro/escritorio y puede
  /// fallar en plataformas no soportadas o sin permisos: en ese caso el ajuste
  /// queda persistido pero no rompe la UI.
  Future<void> _applyAutostart(bool enabled) async {
    try {
      launchAtStartup.setup(
        appName: AppConfig.appName,
        appPath: Platform.resolvedExecutable,
      );
      if (enabled) {
        await launchAtStartup.enable();
      } else {
        await launchAtStartup.disable();
      }
    } catch (e) {
      debugPrint('No se pudo configurar el arranque automático: $e');
    }
  }
}
