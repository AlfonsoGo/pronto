import 'dart:io';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:path/path.dart' as p;
import 'package:path_provider/path_provider.dart';

import '../core/config.dart';
import '../data/sqlite_learning_repository.dart';
import '../features/audio_capture/audio_capture_record.dart';
import '../features/learning/external_edit_capture.dart';
import '../features/learning/learning_repository.dart';
import '../features/learning/learning_service.dart';
import '../features/settings/settings_controller.dart';
import '../features/transcription/parakeet_engine.dart';
import 'audio_capture.dart';
import 'clipboard_watcher.dart';
import 'global_hotkey_service.dart';
import 'text_injector.dart';
import 'whisper_engine.dart';
import 'windows/windows_clipboard_watcher.dart';
import 'windows/windows_hotkey_service.dart';
import 'windows/windows_text_injector.dart';

/// Factory por plataforma. El MVP solo implementa Windows; al portar
/// (ver ROADMAP.md) se añaden las ramas macOS/Linux/Android/iOS aquí, detrás
/// de las mismas interfaces.

/// Motor de voz activo. Pronto usa SOLO Parakeet (NVIDIA, sherpa-onnx): más
/// rápido y preciso en español, puntúa solo y entiende mejor la jerga. Whisper
/// quedó retirado de la selección; conservamos la interfaz [WhisperEngine]
/// porque el pipeline es agnóstico del motor. El modelo queda residente desde
/// el arranque, así que se lee como SNAPSHOT tras `settings.loaded`.
final whisperEngineProvider = Provider<WhisperEngine>((ref) {
  final impl = ParakeetEngine();
  ref.onDispose(impl.dispose);
  return impl;
});

final textInjectorProvider = Provider<TextInjector>((ref) {
  if (Platform.isWindows) return const WindowsTextInjector();
  throw UnimplementedError('TextInjector solo implementado en Windows (MVP).');
});

final hotkeyServiceProvider = Provider<GlobalHotkeyService>((ref) {
  if (Platform.isWindows) {
    final svc = WindowsHotkeyService();
    ref.onDispose(svc.dispose);
    return svc;
  }
  throw UnimplementedError('Hotkey global solo en Windows (MVP).');
});

final audioCaptureProvider = Provider<AudioCapture>((ref) {
  final cap = AudioCaptureRecord();
  // Sincroniza el micrófono elegido SIN recrear la instancia: `read` para el
  // valor inicial + `listen` para los cambios. Con `ref.watch`, cambiar de
  // micrófono reconstruía el provider y hacía dispose() de la captura que el
  // dictado seguía usando → "AudioCaptureRecord ya fue liberado con dispose()".
  // Con `listen`, la MISMA instancia vive toda la sesión y solo actualiza el
  // dispositivo (se aplica en el próximo start()).
  cap.setInputDeviceId(ref.read(settingsProvider.select((s) => s.micDeviceId)));
  ref.listen<String?>(
    settingsProvider.select((s) => s.micDeviceId),
    (_, id) => cap.setInputDeviceId(id),
  );
  ref.onDispose(cap.dispose);
  return cap;
});

final learningRepositoryProvider = Provider<LearningRepository>((ref) {
  // Abre la BD de forma perezosa en el primer uso.
  return SqliteLearningRepository();
});

final learningServiceProvider = Provider<LearningService>((ref) {
  return LearningService(ref.watch(learningRepositoryProvider));
});

final clipboardWatcherProvider = Provider<ClipboardWatcher>((ref) {
  if (Platform.isWindows) {
    final watcher = WindowsClipboardWatcher();
    ref.onDispose(watcher.dispose);
    return watcher;
  }
  throw UnimplementedError('ClipboardWatcher solo en Windows (MVP).');
});

/// Captura de ediciones externas (estilo Wispr Flow). Se reconstruye si cambia
/// el ajuste [AppSettings.captureExternalEdits].
final externalEditCaptureProvider = Provider<ExternalEditCapture>((ref) {
  final capture = ExternalEditCapture(
    ref.watch(clipboardWatcherProvider),
    ref.read(learningServiceProvider),
  );
  capture.setEnabled(ref.watch(settingsProvider).captureExternalEdits);
  ref.onDispose(capture.dispose);
  return capture;
});

/// Carpeta de datos de la app (modelos, BD).
final appSupportDirProvider = FutureProvider<Directory>((ref) async {
  final dir = await getApplicationSupportDirectory();
  final models = Directory(p.join(dir.path, 'models'));
  if (!models.existsSync()) models.createSync(recursive: true);
  return dir;
});

/// Ruta del modelo del motor ACTIVO, o null si aún no existe. Para Parakeet es
/// la CARPETA del modelo; para Whisper, el fichero ggml. Busca, por orden:
///   1) `models/...` junto al ejecutable (distribución).
///   2) `models/...` en la carpeta de datos de la app (descarga bajo demanda).
final modelPathResolverProvider = FutureProvider<String?>((ref) async {
  final engine = ref.read(settingsProvider).engine;
  final exeDir = p.dirname(Platform.resolvedExecutable);
  final support = await ref.watch(appSupportDirProvider.future);

  if (engine == SpeechEngine.parakeet) {
    for (final base in [exeDir, support.path]) {
      final dir = p.join(base, 'models', AppConfig.parakeetModelDir);
      if (File(p.join(dir, ParakeetEngine.tokensFile)).existsSync()) return dir;
    }
    return null;
  }

  for (final base in [exeDir, support.path]) {
    final f = p.join(base, 'models', AppConfig.defaultModelFile);
    if (File(f).existsSync()) return f;
  }
  return null;
});
