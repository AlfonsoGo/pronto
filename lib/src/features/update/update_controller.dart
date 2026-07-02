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

/// Versión actual de la app (p. ej. "0.5.0"). Para mostrarla en la cabecera.
final appVersionProvider = FutureProvider<String>((ref) async {
  final info = await PackageInfo.fromPlatform();
  return info.version;
});

/// Busca releases nuevas en GitHub, avisa y, a petición, descarga el instalador
/// y lo lanza en silencio para actualizar la app (y reiniciarla).
class UpdateController extends Notifier<UpdateState> {
  Timer? _periodic;

  @override
  UpdateState build() {
    ref.onDispose(() {
      _periodic?.cancel();
      _periodic = null;
    });

    // Primer chequeo poco después de arrancar, con un margen para no competir
    // con el arranque del modelo/atajo y para dar tiempo a que la red esté lista.
    Future.delayed(const Duration(seconds: 5), () {
      if (state.status == UpdateStatus.idle) {
        unawaited(checkForUpdate());
      }
    });

    // Re-chequeo periódico: detecta una release nueva sin tener que reiniciar la
    // app y, sobre todo, REINTENTA si el primer chequeo falló (red no lista,
    // límite de la API de GitHub, etc.) en vez de quedarse callado para siempre.
    _periodic = Timer.periodic(const Duration(hours: 2), (_) {
      if (!state.isWorking) unawaited(checkForUpdate());
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
        // 404 (sin releases / repo privado), 403 (límite de la API) u otro: no
        // es un "estás al día" fiable. En manual lo mostramos como al día; en
        // automático dejamos 'idle' para que el chequeo periódico reintente.
        state = state.copyWith(
          status: manual ? UpdateStatus.upToDate : UpdateStatus.idle,
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
      if (manual) {
        state = state.copyWith(
          status: UpdateStatus.error,
          error: 'No se pudo comprobar: $e',
        );
      } else if (state.status != UpdateStatus.available) {
        // Fallo silencioso de un chequeo automático: NO afirmamos "al día"
        // (sería engañoso). Volvemos a 'idle' y el chequeo periódico reintenta.
        // Si ya habíamos detectado una versión nueva, la conservamos.
        state = state.copyWith(status: UpdateStatus.idle, error: null);
      }
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

      // SEGURIDAD (integridad del instalador). No verificamos aquí un hash:
      // sería seguridad falsa, porque el hash esperado tendría que venir del
      // mismo GitHub Release que el .exe (misma fuente), así que un release
      // comprometido serviría ambos. La verificación ROBUSTA es la firma
      // Authenticode (code-signing) del instalador, que el propio Windows
      // valida al ejecutarlo. Eso queda FUERA DE SCOPE aquí (requiere un
      // certificado de firma). Mitigaciones ya presentes: la descarga va por
      // HTTPS y el instalador lo lanza el usuario (no es silencioso a espaldas
      // suyas). TODO: firmar el instalador con Authenticode y exigir la firma.
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

  /// Lanza un ayudante (PowerShell, en VENTANA OCULTA) que espera a que Pronto
  /// se cierre, instala la nueva versión en silencio (actualiza en sitio) y
  /// reabre la app; luego cerramos la app para liberar el ejecutable.
  ///
  /// Se usa PowerShell con `-WindowStyle Hidden` en vez de un `.cmd` para evitar
  /// las ventanas de consola que asomaban, y `Start-Process` para un reinicio
  /// fiable.
  Future<void> _launchSilentInstallerAndQuit(String installerPath) async {
    final exe = Platform.resolvedExecutable;
    final tmpDir = await getTemporaryDirectory();
    final ps1Path = p.join(tmpDir.path, 'pronto_update.ps1');
    final logPath = p.join(tmpDir.path, 'pronto_update.log');

    // Ayudante robusto:
    //  1) espera a que Pronto cierre y, si no lo hace, lo FUERZA (así el
    //     instalador no choca con el AppMutex y aborta en silencio);
    //  2) instala en silencio y espera a que TERMINE de verdad;
    //  3) reabre Pronto CON REINTENTOS hasta confirmar que el proceso está en
    //     marcha — antes hacía un único intento y, si fallaba (p. ej. el .exe
    //     recién escrito aún bloqueado), la app se quedaba cerrada.
    // Deja un log en %TEMP%\pronto_update.log por si hay que diagnosticar.
    final script = "\$ErrorActionPreference = 'SilentlyContinue'\r\n"
        "\$log = '$logPath'\r\n"
        "function Log(\$m) { ((Get-Date -Format o) + '  ' + \$m) | Out-File -FilePath \$log -Append -Encoding utf8 }\r\n"
        "Log 'helper iniciado'\r\n"
        "# 1) Esperar (max ~45s) a que Pronto se cierre; si no, forzarlo.\r\n"
        "for (\$i = 0; \$i -lt 64; \$i++) {\r\n"
        "  if (-not (Get-Process -Name pronto -ErrorAction SilentlyContinue)) { break }\r\n"
        "  Start-Sleep -Milliseconds 700\r\n"
        "}\r\n"
        "if (Get-Process -Name pronto -ErrorAction SilentlyContinue) {\r\n"
        "  Log 'Pronto seguia abierto; forzando cierre'\r\n"
        "  Stop-Process -Name pronto -Force -ErrorAction SilentlyContinue\r\n"
        "  Start-Sleep -Milliseconds 1200\r\n"
        "}\r\n"
        "Log 'instalando'\r\n"
        "# 2) Instalar en silencio y esperar a que termine del todo.\r\n"
        "Unblock-File -Path '$installerPath' -ErrorAction SilentlyContinue\r\n"
        "\$inst = Start-Process -FilePath '$installerPath' -ArgumentList '/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/NOCANCEL' -PassThru -Wait\r\n"
        "Log ('instalador exit: ' + \$inst.ExitCode)\r\n"
        "Start-Sleep -Milliseconds 900\r\n"
        "# 3) Reabrir Pronto con reintentos hasta confirmar que arranca.\r\n"
        "for (\$i = 0; \$i -lt 10; \$i++) {\r\n"
        "  if (Get-Process -Name pronto -ErrorAction SilentlyContinue) { break }\r\n"
        "  if (Test-Path '$exe') { Log ('lanzando intento ' + \$i); Start-Process -FilePath '$exe' }\r\n"
        "  Start-Sleep -Milliseconds 1200\r\n"
        "}\r\n"
        "if (Get-Process -Name pronto -ErrorAction SilentlyContinue) { Log 'OK Pronto en marcha' } else { Log 'FALLO Pronto no arranco' }\r\n";

    await File(ps1Path).writeAsString(script);

    // Lanzamos el PowerShell vía un VBScript con wscript: así NO aparece NINGUNA
    // ventana de consola (ni un parpadeo). wscript no muestra ventana propia y
    // el Run con estilo 0 mantiene el PowerShell oculto.
    final vbsPath = p.join(tmpDir.path, 'pronto_update.vbs');
    final vbs = 'CreateObject("WScript.Shell").Run '
        '"powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass '
        '-WindowStyle Hidden -File ""$ps1Path""", 0, False\r\n';
    await File(vbsPath).writeAsString(vbs);

    await Process.start(
      'wscript.exe',
      [vbsPath],
      mode: ProcessStartMode.detached,
    );

    // Un instante para que arranque el ayudante y salimos del todo (libera el
    // .exe y el mutex para que el instalador pueda reemplazarlo).
    await Future<void>.delayed(const Duration(milliseconds: 500));
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
