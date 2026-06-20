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
import '../features/transcription/whisper_engine_ffi.dart';
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

final whisperEngineProvider = Provider<WhisperEngine>((ref) {
  final engine = WhisperEngineFfi();
  ref.onDispose(engine.dispose);
  return engine;
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

/// Ruta absoluta del modelo Whisper, o null si aún no existe. Busca, por orden:
///   1) `models/<modelo>` junto al ejecutable (desarrollo y distribución).
///   2) `models/<modelo>` en la carpeta de datos de la app.
final modelPathResolverProvider = FutureProvider<String?>((ref) async {
  const file = AppConfig.defaultModelFile;

  final besideExe =
      p.join(p.dirname(Platform.resolvedExecutable), 'models', file);
  if (File(besideExe).existsSync()) return besideExe;

  final dir = await ref.watch(appSupportDirProvider.future);
  final inSupport = p.join(dir.path, 'models', file);
  if (File(inSupport).existsSync()) return inSupport;

  return null;
});
