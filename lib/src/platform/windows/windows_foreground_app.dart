import 'dart:ffi';

import 'package:ffi/ffi.dart';
import 'package:win32/win32.dart';

import '../foreground_app.dart';

/// Ventana en primer plano en Windows, por win32.
///
/// Todo es de solo lectura y barato (tres llamadas), pensado para ejecutarse
/// justo ANTES de pegar: en ese instante el foco está en la app de destino, que
/// es lo que el Historial quiere apuntar. Un segundo después el usuario ya pudo
/// cambiar de ventana.
class WindowsForegroundApp implements ForegroundAppReader {
  const WindowsForegroundApp();

  @override
  ForegroundApp? current() {
    final hwnd = GetForegroundWindow();
    if (hwnd == 0) return null;

    final pid = _pidOf(hwnd);
    if (pid == 0 || pid == GetCurrentProcessId()) return null;

    final app = ForegroundApp(
      appName: _executableName(pid),
      windowTitle: _windowTitle(hwnd),
    );
    return app.isEmpty ? null : app;
  }

  int _pidOf(int hwnd) {
    final out = calloc<Uint32>();
    try {
      GetWindowThreadProcessId(hwnd, out);
      return out.value;
    } finally {
      calloc.free(out);
    }
  }

  String? _windowTitle(int hwnd) {
    final length = GetWindowTextLength(hwnd);
    if (length <= 0) return null;
    final buffer = wsalloc(length + 1);
    try {
      final written = GetWindowText(hwnd, buffer, length + 1);
      if (written <= 0) return null;
      return buffer.toDartString();
    } finally {
      free(buffer);
    }
  }

  /// Nombre del ejecutable, sin ruta ni `.exe`.
  ///
  /// Con `PROCESS_QUERY_LIMITED_INFORMATION`: es el permiso que funciona contra
  /// procesos de otro nivel de integridad sin pedir privilegios. Si aun así no
  /// se puede abrir (proceso protegido), se devuelve `null` en vez de inventar.
  String? _executableName(int pid) {
    final handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid);
    if (handle == 0) return null;
    const max = 260;
    final buffer = wsalloc(max);
    final size = calloc<Uint32>()..value = max;
    try {
      if (QueryFullProcessImageName(handle, 0, buffer, size) == 0) return null;
      final path = buffer.toDartString();
      final slash = path.lastIndexOf(r'\');
      final name = slash >= 0 ? path.substring(slash + 1) : path;
      return name.toLowerCase().endsWith('.exe')
          ? name.substring(0, name.length - 4)
          : name;
    } finally {
      free(buffer);
      calloc.free(size);
      CloseHandle(handle);
    }
  }
}
