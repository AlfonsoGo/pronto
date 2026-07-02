import 'dart:typed_data';

/// Dispositivo de entrada (micrófono) enumerable del sistema.
///
/// Modelo mínimo y estable para la UI: [id] es el identificador con el que la
/// plataforma selecciona el dispositivo; [label] es el texto legible.
class MicDevice {
  final String id;
  final String label;
  const MicDevice(this.id, this.label);
}

/// Captura de audio del micrófono.
///
/// Entrega audio en el formato que espera whisper.cpp: PCM mono 16 kHz,
/// float32 normalizado a [-1, 1].
///
/// Implementación: [AudioCaptureRecord] (paquete `record`).
abstract class AudioCapture {
  /// ¿Hay permiso de micrófono? (En Windows, Configuración > Privacidad.)
  Future<bool> hasPermission();

  /// Enumera los micrófonos de entrada disponibles en el sistema.
  Future<List<MicDevice>> listInputDevices();

  /// Fija el micrófono a usar por su [id] (de [listInputDevices]). Si es null
  /// o no existe, la próxima grabación usa el dispositivo por defecto.
  void setInputDeviceId(String? id);

  /// Empieza a grabar y a acumular muestras internamente.
  Future<void> start();

  /// Detiene la grabación y devuelve todas las muestras como float32 mono 16k.
  Future<Float32List> stop();

  /// Nivel de entrada (0.0 a 1.0) para el medidor de la UI.
  Stream<double> get amplitude;

  Future<void> dispose();
}
