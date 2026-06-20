import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:http/http.dart' as http;
import 'package:package_info_plus/package_info_plus.dart';
import 'package:path/path.dart' as p;
import 'package:path_provider/path_provider.dart';

import '../../core/config.dart';

/// Estado del auto-actualizador.
enum UpdateStatus {
  idle,
  checking,
  upToDate,
  available, // hay versión nueva descargable
  downloading,
  installing, // lanzado el instalador; la app se va a cerrar
  error,
}

@immutable
class UpdateState {
  final UpdateStatus status;
  final String? currentVersion;
  final String? latestVersion;
  final String? downloadUrl;
  final String? assetName;

  /// Progreso de descarga (0.0 - 1.0).
  final double progress;
  final String? error;

  const UpdateState({
    this.status = UpdateStatus.idle,
    this.currentVersion,
    this.latestVersion,
    this.downloadUrl,
    this.assetName,
    this.progress = 0,
    this.error,
  });

  bool get isAvailable => status == UpdateStatus.available;
  bool get isWorking =>
      status == UpdateStatus.downloading || status == UpdateStatus.installing;

  static const Object _keep = Object();

  UpdateState copyWith({
    UpdateStatus? status,
    String? currentVersion,
    String? latestVersion,
    String? downloadUrl,
    String? assetName,
    double? progress,
    Object? error = _keep,
  }) {
    return UpdateState(
      status: status ?? this.status,
      currentVersion: currentVersion ?? this.currentVersion,
      latestVersion: latestVersion ?? this.latestVersion,
      downloadUrl: downloadUrl ?? this.downloadUrl,
      assetName: assetName ?? this.assetName,
      progress: progress ?? this.progress,
      error: identical(error, _keep) ? this.error : error as String?,
    );
  }
}

final updateProvider =
    NotifierProvider<UpdateController, UpdateState>(UpdateController.new);

/// Busca releases nuevas en GitHub, avisa y, a petición, descarga el instalador
/// y lo lanza en silencio para actualizar la app (y reiniciarla).
class UpdateController extends Notifier<UpdateState> {
  @override
  UpdateState build() {
    // Comprobación automática al arrancar, con un margen para no competir con
    // el arranque del modelo/atajo.
    Future.delayed(const Duration(seconds: 6), () {
      if (state.status == UpdateStatus.idle) {
        unawaited(checkForUpdate());
      }
    });
    return const UpdateState();
  }

  /// Consulta la última release publicada y compara con la versión actual.
  Future<void> checkForUpdate({bool manual = false}) async {
    if (state.isWorking) return;
    state = state.copyWith(status: UpdateStatus.checking, error: null);
    try {
      final info = await PackageInfo.fromPlatform();
      final current = info.version; // p. ej. "0.2.0"

      final uri = Uri.parse(
        'https://api.github.com/repos/${AppConfig.githubOwner}/'
        '${AppConfig.githubRepo}/releases/latest',
      );
      final res = await http.get(
        uri,
        headers: const {
          'Accept': 'application/vnd.github+json',
          'User-Agent': 'Pronto-Updater',
        },
      ).timeout(const Duration(seconds: 15));

      if (res.statusCode != 200) {
        // 404 = repo privado o sin releases: lo tratamos como "sin novedades".
        state = state.copyWith(
          status: UpdateStatus.upToDate,
          currentVersion: current,
        );
        return;
      }

      final json = jsonDecode(res.body) as Map<String, dynamic>;
      final tag = (json['tag_name'] as String? ?? '').trim();
      final latest = tag.startsWith('v') ? tag.substring(1) : tag;
      final assets = (json['assets'] as List?) ?? const [];

      Map<String, dynamic>? exeAsset;
      for (final a in assets) {
        final m = a as Map<String, dynamic>;
        final name = (m['name'] as String? ?? '').toLowerCase();
        if (name.endsWith('.exe')) {
          exeAsset = m;
          break;
        }
      }

      if (latest.isEmpty || exeAsset == null || _cmp(latest, current) <= 0) {
        state = state.copyWith(
          status: UpdateStatus.upToDate,
          currentVersion: current,
          latestVersion: latest,
        );
        return;
      }

      state = state.copyWith(
        status: UpdateStatus.available,
        currentVersion: current,
        latestVersion: latest,
        downloadUrl: exeAsset['browser_download_url'] as String?,
        assetName: exeAsset['name'] as String?,
      );
    } catch (e) {
      debugPrint('[Pronto] comprobar actualización falló: $e');
      state = state.copyWith(
        status: manual ? UpdateStatus.error : UpdateStatus.upToDate,
        error: manual ? 'No se pudo comprobar: $e' : null,
      );
    }
  }

  /// Descarga el instalador (con progreso) y lo lanza en silencio; la app se
  /// cierra para que el instalador pueda reemplazar los ficheros, y al terminar
  /// se vuelve a abrir sola.
  Future<void> downloadAndInstall() async {
    final url = state.downloadUrl;
    final name = state.assetName;
    if (url == null || name == null) return;

    state = state.copyWith(
      status: UpdateStatus.downloading,
      progress: 0,
      error: null,
    );
    try {
      final tmpDir = await getTemporaryDirectory();
      final outPath = p.join(tmpDir.path, name);
      final file = File(outPath);

      final req = http.Request('GET', Uri.parse(url));
      req.headers['User-Agent'] = 'Pronto-Updater';
      final resp = await http.Client().send(req);
      if (resp.statusCode != 200) {
        throw 'Descarga HTTP ${resp.statusCode}';
      }
      final total = resp.contentLength ?? 0;
      var received = 0;
      final sink = file.openWrite();
      await for (final chunk in resp.stream) {
        sink.add(chunk);
        received += chunk.length;
        if (total > 0) {
          state = state.copyWith(progress: received / total);
        }
      }
      await sink.flush();
      await sink.close();

      state = state.copyWith(status: UpdateStatus.installing, progress: 1);
      await _launchSilentInstallerAndQuit(outPath);
    } catch (e) {
      debugPrint('[Pronto] descarga/instalación falló: $e');
      state = state.copyWith(
        status: UpdateStatus.error,
        error: 'No se pudo actualizar: $e',
      );
    }
  }

  /// Escribe un pequeño script que espera a que Pronto se cierre, ejecuta el
  /// instalador en silencio (actualiza en sitio) y vuelve a abrir la app; luego
  /// cerramos la app para liberar el ejecutable.
  Future<void> _launchSilentInstallerAndQuit(String installerPath) async {
    final exe = Platform.resolvedExecutable;
    final tmpDir = await getTemporaryDirectory();
    final cmdPath = p.join(tmpDir.path, 'pronto_update.cmd');

    // ping como "sleep" porque el proceso va detached (sin consola) y `timeout`
    // necesita consola. tasklist/find detectan cuándo Pronto se ha cerrado.
    final script = '@echo off\r\n'
        'ping -n 3 127.0.0.1 >nul\r\n'
        ':waitloop\r\n'
        'tasklist /FI "IMAGENAME eq pronto.exe" 2>nul | find /I "pronto.exe" >nul\r\n'
        'if not errorlevel 1 (\r\n'
        '  ping -n 2 127.0.0.1 >nul\r\n'
        '  goto waitloop\r\n'
        ')\r\n'
        '"$installerPath" /VERYSILENT /SUPPRESSMSGBOXES /NORESTART\r\n'
        'start "" "$exe"\r\n';

    await File(cmdPath).writeAsString(script);

    await Process.start(
      'cmd.exe',
      ['/c', cmdPath],
      mode: ProcessStartMode.detached,
    );

    // Damos un instante a que el proceso detached arranque y salimos del todo
    // (libera el .exe y el mutex para que el instalador pueda reemplazarlo).
    await Future<void>.delayed(const Duration(milliseconds: 400));
    exit(0);
  }

  /// Compara dos versiones "a.b.c". Devuelve 1 si a>b, -1 si a<b, 0 si iguales.
  int _cmp(String a, String b) {
    List<int> parse(String s) => s
        .split('+')
        .first
        .split('.')
        .map((x) => int.tryParse(x.trim()) ?? 0)
        .toList();
    final pa = parse(a);
    final pb = parse(b);
    for (var i = 0; i < 3; i++) {
      final x = i < pa.length ? pa[i] : 0;
      final y = i < pb.length ? pb[i] : 0;
      if (x != y) return x > y ? 1 : -1;
    }
    return 0;
  }
}
