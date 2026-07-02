// Seguimiento del "monitor activo" para la píldora (solo Windows).
//
// Objetivo: cuando el usuario se va a OTRA pantalla y pincha en una ventana
// donde escribir, la píldora debe SALTAR al borde inferior de ese monitor (sin
// perseguir el cursor dentro de la pantalla: el eje vertical sigue bloqueado).
//
// La señal correcta de "dónde estoy trabajando" es la VENTANA EN PRIMER PLANO
// (la que acabas de pinchar para escribir), no el ratón (que se mueve y vuelve).
// Por eso miramos GetForegroundWindow y su monitor. Todo es de solo lectura y no
// roba el foco: la píldora sigue sin activarse.
//
// Coordenadas: devolvemos el área de trabajo del monitor en PÍXELES FÍSICOS. El
// llamante la pasa a lógicos dividiendo por el DPR actual de la píldora, lo que
// da un round-trip EXACTO con window_manager.setBounds (que vuelve a multiplicar
// por ese mismo DPR). Así aterriza justo en el monitor destino.

import 'dart:ffi';

import 'package:ffi/ffi.dart';
import 'package:win32/win32.dart';

/// HMONITOR de la ventana en primer plano, o 0 si no hay ninguna o si esa
/// ventana es de NUESTRO propio proceso (panel abierto, clic en la píldora…):
/// en esos casos no queremos seguir nada. Barato: pensado para sondearlo cada
/// pocos cientos de milisegundos.
int foregroundMonitorIfExternal() {
  final fg = GetForegroundWindow();
  if (fg == 0) return 0;
  if (_isOurProcess(fg)) return 0;
  return MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST);
}

/// Datos para recolocar la píldora en [foregroundMonitor]: en qué monitor está
/// AHORA la píldora (para no recolocarla si ya está en el bueno, y así respetar
/// su posición horizontal manual) y el área de trabajo FÍSICA del monitor
/// destino (sin barra de tareas). Devuelve null si no se pudo leer el monitor.
FollowTarget? monitorFollowTarget(int foregroundMonitor, String pillWindowTitle) {
  if (foregroundMonitor == 0) return null;

  final mi = calloc<MONITORINFO>();
  try {
    mi.ref.cbSize = sizeOf<MONITORINFO>();
    if (GetMonitorInfo(foregroundMonitor, mi) == 0) return null;
    final w = mi.ref.rcWork;
    return FollowTarget(
      pillMonitor: _ourWindowMonitor(pillWindowTitle),
      workLeft: w.left,
      workTop: w.top,
      workRight: w.right,
      workBottom: w.bottom,
    );
  } finally {
    calloc.free(mi);
  }
}

/// Resultado de [monitorFollowTarget].
class FollowTarget {
  /// Monitor donde vive ahora la píldora (0 si no se pudo determinar).
  final int pillMonitor;

  /// Área de trabajo del monitor destino en píxeles físicos.
  final int workLeft, workTop, workRight, workBottom;

  const FollowTarget({
    required this.pillMonitor,
    required this.workLeft,
    required this.workTop,
    required this.workRight,
    required this.workBottom,
  });
}

/// Monitor de NUESTRA ventana (la píldora), localizada por título (main.cpp la
/// crea como "Pronto"). Verifica que sea de nuestro proceso por si otra app se
/// llama igual. 0 si no se encuentra.
int _ourWindowMonitor(String title) {
  final titlePtr = title.toNativeUtf16();
  try {
    final hwnd = FindWindow(nullptr, titlePtr);
    if (hwnd == 0 || !_isOurProcess(hwnd)) return 0;
    return MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
  } finally {
    calloc.free(titlePtr);
  }
}

/// ¿La ventana [hwnd] pertenece a nuestro proceso?
bool _isOurProcess(int hwnd) {
  final pidPtr = calloc<Uint32>();
  try {
    GetWindowThreadProcessId(hwnd, pidPtr);
    return pidPtr.value == GetCurrentProcessId();
  } finally {
    calloc.free(pidPtr);
  }
}
