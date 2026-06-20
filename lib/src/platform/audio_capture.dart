import 'dart:typed_data';

/// Captura de audio del micrófono.
///
/// Entrega audio en el formato que espera whisper.cpp: PCM mono 16 kHz,
/// float32 normalizado a [-1, 1].
///
/// Implementación: [AudioCaptureRecord] (paquete `record`).
abstract class AudioCapture {
  /// ¿Hay permiso de micrófono? (En Windows, Configuración > Privacidad.)
  Future<bool> hasPermission();

  /// Empieza a grabar y a acumular muestras internamente.
  Future<void> start();

  /// Detiene la grabación y devuelve todas las muestras como float32 mono 16k.
  Future<Float32List> stop();

  /// Nivel de entrada (0.0 a 1.0) para el medidor de la UI.
  Stream<double> get amplitude;

  Future<void> dispose();
}
