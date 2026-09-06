import 'dart:async';
import 'dart:io';

import 'package:crypto/crypto.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:http/http.dart' as http;
import 'package:path/path.dart' as p;
import 'package:path_provider/path_provider.dart';

import '../../core/config.dart';
import '../../platform/platform_services.dart';

/// Fases del arranque cara al usuario.
enum DownloadPhase {
  /// Aún no se ha comprobado si el modelo está.
  idle,

  /// Descargando los ficheros que falten.
  descargando,

  /// Todo listo: el modelo ya está en disco.
  listo,

  /// El usuario paró la descarga. No es un error: no se enseña como avería ni
  /// se ofrece «reintentar» como si algo hubiera fallado.
  cancelado,

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

  /// [clearCurrentFile] borra el nombre del fichero en curso.
  ///
  /// Hace falta porque con `??` no se puede pasar de un valor a null: al
  /// cancelar, pasar `currentFile: null` dejaba el nombre del fichero anterior
  /// puesto y la pantalla seguía diciendo que se estaba bajando algo que ya no
  /// se estaba bajando.
  DownloadState copyWith({
    DownloadPhase? phase,
    double? progress,
    int? receivedBytes,
    int? totalBytes,
    String? currentFile,
    bool clearCurrentFile = false,
    String? error,
  }) {
    return DownloadState(
      phase: phase ?? this.phase,
      progress: progress ?? this.progress,
      receivedBytes: receivedBytes ?? this.receivedBytes,
      totalBytes: totalBytes ?? this.totalBytes,
      currentFile: clearCurrentFile ? null : (currentFile ?? this.currentFile),
      error: error ?? this.error,
    );
  }
}

/// Cómo se abre una conexión para descargar.
///
/// Existe para poder sustituirlo en una prueba: el criterio de aceptación del
/// PRNT-13 dice que **cancelar a mitad no puede dejar el modelo a medias**, y
/// eso no se demuestra bajando 639 MB de Hugging Face.
final downloadClientProvider = Provider<http.Client Function()>(
  (ref) => http.Client.new,
);

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
  /// El usuario ha pedido parar. Se mira entre trozo y trozo.
  bool _cancelado = false;

  /// Cliente de la descarga en curso. Se guarda para poder CERRARLO al
  /// cancelar: sin eso, una descarga atascada esperando bytes que no llegan no
  /// se enteraría nunca de que la han parado.
  http.Client? _cliente;

  @override
  DownloadState build() {
    ref.onDispose(() {
      _cancelado = true;
      _cliente?.close();
    });
    return const DownloadState();
  }

  /// Para la descarga en curso y **borra el trozo a medio bajar**.
  ///
  /// Un `.part` superviviente es la avería que este método existe para evitar:
  /// al reintentar hay que bajar el fichero ENTERO, no continuar por donde iba
  /// un flujo que nadie verificó.
  void cancel() {
    if (state.phase != DownloadPhase.descargando) return;
    _cancelado = true;
    _cliente?.close();
  }

  /// Borra el modelo descargado de la carpeta de datos.
  ///
  /// Solo toca la copia bajada por Pronto (`<appSupport>/models/parakeet/`).
  /// La que viaja junto al `.exe` es del instalador y no se toca: borrarla
  /// dejaría a Pronto sin motor y sin manera de recuperarlo salvo reinstalando.
  ///
  /// Devuelve `true` si había algo que borrar y se borró.
  Future<bool> deleteDownloadedModel() async {
    try {
      // Ojo: [_targetDir] CREA la carpeta. Aquí solo se quiere mirar, o
      // «borrar» acabaría creando la carpeta para borrarla acto seguido y
      // diciendo que había algo.
      final dir = await _modelDir();
      if (!dir.existsSync()) return false;
      dir.deleteSync(recursive: true);
      state = const DownloadState();
      return true;
    } catch (e) {
      debugPrint('No se pudo borrar el modelo descargado: $e');
      state = state.copyWith(
        phase: DownloadPhase.error,
        error: 'No se pudo borrar el modelo: $e',
      );
      return false;
    }
  }

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
    // Se pide al provider de la carpeta de datos, no a `path_provider` a pelo:
    // es la misma respuesta y, además, deja probar la descarga contra una
    // carpeta temporal en vez de escribir en el AppData de verdad de quien
    // ejecute la suite.
    final dir = await _modelDir();
    if (!dir.existsSync()) dir.createSync(recursive: true);
    return dir;
  }

  /// La MISMA carpeta que [_targetDir] pero sin crearla. Para mirar si hay algo.
  Future<Directory> _modelDir() async {
    final support = await ref.read(appSupportDirProvider.future);
    return Directory(
      p.join(support.path, 'models', AppConfig.parakeetModelDir),
    );
  }

  /// Descarga los ficheros del modelo que falten. Idempotente: los que ya
  /// existen se saltan. Actualiza [state] con el progreso global.
  Future<void> ensureModel() async {
    _cancelado = false;
    state = state.copyWith(
      phase: DownloadPhase.descargando,
      progress: 0,
      receivedBytes: 0,
      clearCurrentFile: true,
      error: '',
    );

    try {
      final dir = await _targetDir();
      final client = ref.read(downloadClientProvider)();
      _cliente = client;
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
          var parado = false;
          try {
            await for (final chunk in resp.stream) {
              if (_cancelado) {
                parado = true;
                break;
              }
              sink.add(chunk);
              acumulado += chunk.length;
              final prog = total > 0 ? (acumulado / total).clamp(0.0, 1.0) : 0.0;
              state = state.copyWith(
                receivedBytes: acumulado,
                progress: prog,
              );
            }
            if (!parado) await sink.flush();
          } finally {
            await sink.close();
          }

          // Parado a mitad: el trozo bajado se BORRA. Dejarlo sería guardar
          // medio modelo con nombre de fichero temporal y confiar en que la
          // próxima vez alguien lo pise entero.
          if (parado || _cancelado) {
            if (tmp.existsSync()) tmp.deleteSync();
            state = state.copyWith(
              phase: DownloadPhase.cancelado,
              progress: 0,
              receivedBytes: 0,
              clearCurrentFile: true,
            );
            return;
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
        _cliente = null;
      }
    } catch (e) {
      // Cancelar cierra el cliente, y cerrar el cliente rompe el flujo: la
      // excepción que llega aquí es la CONSECUENCIA de haber parado, no una
      // avería. Enseñarla como error haría creer que la descarga falló sola.
      if (_cancelado) {
        await _borrarParciales();
        state = state.copyWith(
          phase: DownloadPhase.cancelado,
          progress: 0,
          receivedBytes: 0,
          clearCurrentFile: true,
        );
        return;
      }
      debugPrint('Fallo al descargar el modelo Parakeet: $e');
      state = state.copyWith(
        phase: DownloadPhase.error,
        error: 'No se pudo descargar el modelo de voz: $e',
      );
    }
  }

  /// Barre los `.part` que hayan quedado en la carpeta del modelo.
  ///
  /// Se llama al cancelar. Un `.part` es siempre basura: o la descarga terminó
  /// —y entonces se renombró— o no terminó, y lo que hay dentro no vale.
  Future<void> _borrarParciales() async {
    try {
      final dir = await _modelDir();
      if (!dir.existsSync()) return;
      for (final entrada in dir.listSync()) {
        if (entrada is File && entrada.path.endsWith('.part')) {
          entrada.deleteSync();
        }
      }
    } catch (e) {
      debugPrint('No se pudieron borrar los trozos a medio descargar: $e');
    }
  }
}
