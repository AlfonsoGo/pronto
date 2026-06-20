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

/// Provider global de ajustes de la app.
final settingsProvider = NotifierProvider<SettingsController, AppSettings>(
  SettingsController.new,
);

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

  /// Si la post-corrección por LLM está activada.
  final bool llmEnabled;

  /// URL base del servidor LLM (Ollama local o compatible OpenAI).
  final String llmBaseUrl;

  /// Modelo LLM a usar para la post-corrección.
  final String llmModel;

  /// Si se capturan ediciones externas (lo que copias en otra app tras dictar)
  /// para alimentar el aprendizaje. Conservador y filtrado por similitud.
  final bool captureExternalEdits;

  /// Atajo global para dictar (push-to-talk).
  final HotkeyCombo hotkey;

  /// Escala del punto flotante (1.0 = tamaño por defecto). Ajustable con un
  /// slider en Ajustes.
  final double pillScale;

  const AppSettings({
    required this.modelFile,
    required this.language,
    required this.injectionMode,
    required this.triggerMode,
    required this.autostart,
    required this.llmEnabled,
    required this.llmBaseUrl,
    required this.llmModel,
    required this.captureExternalEdits,
    required this.hotkey,
    this.pillScale = 1.0,
  });

  /// Valores por defecto, tomados de [AppConfig] cuando existen.
  factory AppSettings.defaults() => const AppSettings(
        modelFile: AppConfig.defaultModelFile,
        language: AppConfig.defaultLanguage,
        injectionMode: InjectionMode.clipboardPaste,
        triggerMode: TriggerMode.toggle,
        autostart: false,
        llmEnabled: false,
        // Ollama local por defecto. Solo la RAÍZ del servidor: LlmCorrector
        // añade la ruta (`/api/chat` en Ollama, `/v1/chat/completions` en nube).
        llmBaseUrl: 'http://localhost:11434',
        llmModel: 'qwen3:4b',
        captureExternalEdits: true,
        hotkey: HotkeyCombo.defaultCombo,
      );

  AppSettings copyWith({
    String? modelFile,
    String? language,
    InjectionMode? injectionMode,
    TriggerMode? triggerMode,
    bool? autostart,
    bool? llmEnabled,
    String? llmBaseUrl,
    String? llmModel,
    bool? captureExternalEdits,
    HotkeyCombo? hotkey,
    double? pillScale,
  }) {
    return AppSettings(
      modelFile: modelFile ?? this.modelFile,
      language: language ?? this.language,
      injectionMode: injectionMode ?? this.injectionMode,
      triggerMode: triggerMode ?? this.triggerMode,
      autostart: autostart ?? this.autostart,
      llmEnabled: llmEnabled ?? this.llmEnabled,
      llmBaseUrl: llmBaseUrl ?? this.llmBaseUrl,
      llmModel: llmModel ?? this.llmModel,
      captureExternalEdits: captureExternalEdits ?? this.captureExternalEdits,
      hotkey: hotkey ?? this.hotkey,
      pillScale: pillScale ?? this.pillScale,
    );
  }

  Map<String, dynamic> toJson() => {
        'modelFile': modelFile,
        'language': language,
        'injectionMode': injectionMode.name,
        'triggerMode': triggerMode.name,
        'autostart': autostart,
        'llmEnabled': llmEnabled,
        'llmBaseUrl': llmBaseUrl,
        'llmModel': llmModel,
        'captureExternalEdits': captureExternalEdits,
        'hotkey': hotkey.toJson(),
        'pillScale': pillScale,
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
      llmEnabled: j['llmEnabled'] as bool? ?? d.llmEnabled,
      llmBaseUrl: j['llmBaseUrl'] as String? ?? d.llmBaseUrl,
      llmModel: j['llmModel'] as String? ?? d.llmModel,
      captureExternalEdits:
          j['captureExternalEdits'] as bool? ?? d.captureExternalEdits,
      hotkey: j['hotkey'] is Map
          ? HotkeyCombo.fromJson(Map<String, dynamic>.from(j['hotkey'] as Map))
          : d.hotkey,
      pillScale: (j['pillScale'] as num?)?.toDouble() ?? d.pillScale,
    );
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
      other.llmEnabled == llmEnabled &&
      other.llmBaseUrl == llmBaseUrl &&
      other.llmModel == llmModel &&
      other.captureExternalEdits == captureExternalEdits &&
      other.hotkey == hotkey &&
      other.pillScale == pillScale;

  @override
  int get hashCode => Object.hash(
        modelFile,
        language,
        injectionMode,
        triggerMode,
        autostart,
        llmEnabled,
        llmBaseUrl,
        llmModel,
        captureExternalEdits,
        hotkey,
        pillScale,
      );
}

/// Controla y persiste los ajustes de la app con [SharedPreferences].
///
/// La carga es perezosa: [build] devuelve los valores por defecto al instante
/// y dispara una carga asíncrona que actualiza el estado cuando termina, para
/// no bloquear la construcción del provider.
class SettingsController extends Notifier<AppSettings> {
  /// Clave única bajo la que se serializa todo el JSON de ajustes.
  static const String _prefsKey = 'app_settings';

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
      // Re-sincroniza el autostart del sistema con lo persistido.
      await _applyAutostart(state.autostart);
    } catch (e) {
      debugPrint('No se pudieron cargar los ajustes: $e');
      // Mantenemos los valores por defecto ya presentes en [state].
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

  Future<void> setLlmEnabled(bool value) async {
    state = state.copyWith(llmEnabled: value);
    await _save();
  }

  Future<void> setLlmBaseUrl(String value) async {
    state = state.copyWith(llmBaseUrl: value);
    await _save();
  }

  Future<void> setLlmModel(String value) async {
    state = state.copyWith(llmModel: value);
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
