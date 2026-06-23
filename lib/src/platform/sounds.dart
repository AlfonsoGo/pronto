import 'dart:async';
import 'dart:ffi';
import 'dart:io';
import 'dart:isolate';

/// Sonidos cortos de feedback al grabar/parar (solo Windows; no-op en otras
/// plataformas). Tono ASCENDENTE al empezar, DESCENDENTE al parar, para
/// confirmar con el oído sin mirar la píldora.
class Sounds {
  const Sounds._();

  /// "Empieza a hablar".
  static void recordStart() {
    _beep(880, 70);
  }

  /// "Puedes parar".
  static void recordStop() {
    _beep(560, 90);
  }

  /// Beep() de Win32 es SÍNCRONO (bloquea su hilo durante [durMs]); lo lanzamos
  /// en un isolate de usar y tirar para no congelar la UI.
  static void _beep(int freq, int durMs) {
    if (!Platform.isWindows) return;
    unawaited(Isolate.run(() => _beepNative(freq, durMs)));
  }

  static void _beepNative(int freq, int durMs) {
    try {
      final beep = DynamicLibrary.open('kernel32.dll').lookupFunction<
          Int32 Function(Uint32, Uint32), int Function(int, int)>('Beep');
      beep(freq, durMs);
    } catch (_) {
      // Sin sonido si el dispositivo no lo soporta; no es crítico.
    }
  }
}
