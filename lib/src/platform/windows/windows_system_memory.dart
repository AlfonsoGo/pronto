import 'dart:ffi';

import 'package:ffi/ffi.dart';
import 'package:flutter/foundation.dart';
import 'package:win32/win32.dart';

import '../system_memory.dart';

/// [SystemMemory] de Windows, vía `GlobalMemoryStatusEx` de kernel32.
class WindowsSystemMemory implements SystemMemory {
  const WindowsSystemMemory();

  @override
  int? totalBytes() {
    final buffer = calloc<MEMORYSTATUSEX>();
    try {
      buffer.ref.dwLength = sizeOf<MEMORYSTATUSEX>();
      if (GlobalMemoryStatusEx(buffer) == 0) return null;
      final total = buffer.ref.ullTotalPhys;
      // Un cero (o un negativo, que en teoría no puede pasar) es «no se sabe»,
      // no «este equipo no tiene memoria»: con un 0 se descartarían TODOS los
      // modelos y la pantalla diría que no cabe ninguno.
      return total > 0 ? total : null;
    } catch (e) {
      debugPrint('[TF] no se pudo leer la memoria del equipo: $e');
      return null;
    } finally {
      calloc.free(buffer);
    }
  }
}
