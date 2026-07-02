import 'dart:async';
import 'dart:io';

import 'package:crypto/crypto.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:http/http.dart' as http;
import 'package:path/path.dart' as p;
import 'package:path_provider/path_provider.dart';

import '../../core/config.dart';

/// Fases del arranque cara al usuario.
enum DownloadPhase {
  /// Aún no se ha comprobado si el modelo está.
  idle,

  /// Descargando los ficheros que falten.
  descargando,

  /// Todo listo: el modelo ya está en disco.
  listo,

  /// Falló la descarga; se puede reintentar.
  error,
}

/// Estado inmutable de la preparación del modelo de voz para la UI del splash.
@immutable
class DownloadState {
  final DownloadPhase phase;

  /// Progreso global 0..1 sobre el total de bytes de los ficheros a descargar.
  final double progress;

  /// Bytes ya descargados en esta sesión (para el texto "X / Y MB").
  final int receivedBytes;

  /// Total de bytes a descargar (suma de los ficheros que faltan).
  final int totalBytes;

  /// Nombre del fichero que se está descargando ahora (para el texto de la UI).
  final String? currentFile;

  /// Mensaje de error legible, si [phase] es [DownloadPhase.error].
  final String? error;

  const DownloadState({
    this.phase = DownloadPhase.idle,
    this.progress = 0,
    this.receivedBytes = 0,
    this.totalBytes = 0,
    this.currentFile,
    this.error,
  });

  DownloadState copyWith({
    DownloadPhase? phase,
    double? progress,
    int? receivedBytes,
    int? totalBytes,
    String? currentFile,
    String? error,
  }) {
    return DownloadState(
      phase: phase ?? this.phase,
      progress: progress ?? this.progress,
      receivedBytes: receivedBytes ?? this.receivedBytes,
      totalBytes: totalBytes ?? this.totalBytes,
      currentFile: currentFile ?? this.currentFile,
      error: error ?? this.error,
    );
  }
}

/// Provider del descargador del modelo Parakeet. Lo consume el splash.
final parakeetDownloaderProvider =
    NotifierProvider<ParakeetModelDownloader, DownloadState>(
  ParakeetModelDownloader.new,
);

/// Descarga (bajo demanda, la primera vez) el modelo Parakeet a la carpeta de
/// datos de la app. Mismo patrón que [_WhisperModelTile]: streaming HTTP con
/// progreso, escribe a `.part` y renombra al terminar cada fichero.
///
/// El progreso que expone es GLOBAL: bytes descargados acumulados sobre el
/// total de bytes de los ficheros que faltan (medido con HEAD antes de bajar).
class ParakeetModelDownloader extends Notifier<DownloadState> {
  @override
  DownloadState build() => const DownloadState();

  /// ¿Está el modelo completo (los 4 ficheros) junto al .exe o en appSupport?
  ///
  /// Devuelve la carpeta donde vive si está; null si falta algún fichero.
  static Future<String?> resolveModelDir() async {
    final exeDir = p.dirname(Platform.resolvedExecutable);
    final support = await getApplicationSupportDirectory();
    for (final base in [exeDir, support.path]) {
      final dir = p.join(base, 'models', AppConfig.parakeetModelDir);
      final completo = AppConfig.parakeetModelFiles
          .every((f) => File(p.join(dir, f)).existsSync());
      if (completo) return dir;
    }
    return null;
  }

  /// SHA-256 de un fichero, calculado por streaming (los ficheros del modelo
  /// llegan a cientos de MB: no se lee entero a memoria).
  Future<String> _sha256(File file) async {
    final digest = await file.openRead().transform(sha256).first;
    return digest.toString();
  }

  /// Carpeta destino de la descarga: `<appSupport>/models/parakeet/`.
  Future<Directory> _targetDir() async {
    final support = await getApplicationSupportDirectory();
    final dir = Directory(
      p.join(support.path, 'models', AppConfig.parakeetModelDir),
    );
    if (!dir.existsSync()) dir.createSync(recursive: true);
    return dir;
  }

  /// Descarga los ficheros del modelo que falten. Idempotente: los que ya
  /// existen se saltan. Actualiza [state] con el progreso global.
  Future<void> ensureModel() async {
    state = state.copyWith(
      phase: DownloadPhase.descargando,
      progress: 0,
      receivedBytes: 0,
      currentFile: null,
      error: '',
    );

    try {
      final dir = await _targetDir();
      final client = http.Client();
      try {
        // 1) Averigua qué falta y cuánto pesa (HEAD) para el progreso global.
        final pendientes = <String>[];
        final tamanos = <String, int>{};
        var total = 0;
        for (final file in AppConfig.parakeetModelFiles) {
          final out = File(p.join(dir.path, file));
          if (out.existsSync()) continue;
          pendientes.add(file);
          final url = '${AppConfig.parakeetModelBaseUrl}/$file';
          final head = await client.head(Uri.parse(url));
          // Hugging Face redirige a un CDN; sigue el redirect y da el tamaño.
          final len = head.contentLength ?? 0;
          tamanos[file] = len;
          total += len;
        }

        if (pendientes.isEmpty) {
          state = state.copyWith(phase: DownloadPhase.listo, progress: 1);
          return;
        }

        state = state.copyWith(totalBytes: total);

        // 2) Descarga cada fichero por streaming, acumulando el progreso.
        var acumulado = 0;
        for (final file in pendientes) {
          state = state.copyWith(currentFile: file);
          final url = '${AppConfig.parakeetModelBaseUrl}/$file';
          final out = File(p.join(dir.path, file));
          final tmp = File('${out.path}.part');

          final req = http.Request('GET', Uri.parse(url));
          final resp = await client.send(req);
          if (resp.statusCode != 200) {
            throw 'HTTP ${resp.statusCode} al descargar $file';
          }

          final sink = tmp.openWrite();
          try {
            await for (final chunk in resp.stream) {
              sink.add(chunk);
              acumulado += chunk.length;
              final prog = total > 0 ? (acumulado / total).clamp(0.0, 1.0) : 0.0;
              state = state.copyWith(
                receivedBytes: acumulado,
                progress: prog,
              );
            }
            await sink.flush();
          } finally {
            await sink.close();
          }

          // Integridad: verifica el SHA-256 del `.part` recién bajado ANTES de
          // promoverlo al nombre final. Si no coincide (fichero corrupto o
          // manipulado), borra el `.part` y falla: no se carga un modelo sin
          // verificar.
          final esperado = AppConfig.parakeetModelSha256[file];
          if (esperado != null) {
            final real = await _sha256(tmp);
            if (real != esperado) {
              if (tmp.existsSync()) tmp.deleteSync();
              throw 'integridad del modelo no verificada: $file '
                  '(SHA-256 esperado $esperado, calculado $real)';
            }
          }

          if (out.existsSync()) out.deleteSync();
          tmp.renameSync(out.path);
        }

        state = state.copyWith(phase: DownloadPhase.listo, progress: 1);
      } finally {
        client.close();
      }
    } catch (e) {
      debugPrint('Fallo al descargar el modelo Parakeet: $e');
      state = state.copyWith(
        phase: DownloadPhase.error,
        error: 'No se pudo descargar el modelo de voz: $e',
      );
    }
  }
}
