import 'dart:async';
import 'dart:math' as math;
import 'dart:typed_data';

import 'package:record/record.dart';

import '../../core/audio_utils.dart';
import '../../platform/audio_capture.dart';

/// Implementación de [AudioCapture] basada en el paquete `record` (^5.x).
///
/// Graba del micrófono en streaming con formato PCM 16-bit mono a 16 kHz
/// (lo que espera whisper.cpp), acumula los chunks en memoria y publica
/// un nivel de amplitud aproximado (0..1) para el medidor de la UI.
class AudioCaptureRecord implements AudioCapture {
  /// Grabador del paquete `record`.
  final AudioRecorder _recorder = AudioRecorder();

  /// Buffer de chunks PCM recibidos del stream durante la grabación.
  final List<Uint8List> _chunks = <Uint8List>[];

  /// Suscripción al stream de audio del grabador.
  StreamSubscription<Uint8List>? _subscription;

  /// Emisor del nivel de amplitud (0..1) para la UI. Broadcast: varios oyentes.
  final StreamController<double> _amplitudeController =
      StreamController<double>.broadcast();

  /// Indica si hay una grabación en curso.
  bool _recording = false;

  /// Evita usar la instancia después de [dispose].
  bool _disposed = false;

  /// Id del micrófono elegido en Ajustes, o null = por defecto del sistema.
  String? _deviceId;

  /// Configuración de grabación: PCM 16-bit, mono, 16 kHz.
  static const RecordConfig _config = RecordConfig(
    encoder: AudioEncoder.pcm16bits,
    sampleRate: 16000,
    numChannels: 1,
  );

  @override
  Future<bool> hasPermission() {
    _checkNotDisposed();
    return _recorder.hasPermission();
  }

  @override
  Future<List<MicDevice>> listInputDevices() async {
    _checkNotDisposed();
    final devices = await _recorder.listInputDevices();
    return devices.map((d) => MicDevice(d.id, d.label)).toList();
  }

  @override
  void setInputDeviceId(String? id) {
    _deviceId = id;
  }

  @override
  Future<void> start() async {
    _checkNotDisposed();

    if (_recording) {
      // Ya estamos grabando; no reiniciamos para no perder lo capturado.
      return;
    }

    // Comprobamos el permiso de micrófono antes de empezar.
    final bool permitido = await _recorder.hasPermission();
    if (!permitido) {
      throw StateError(
        'No hay permiso para usar el micrófono. Revisa Configuración > '
        'Privacidad y seguridad > Micrófono en Windows y permite el acceso '
        'a esta aplicación.',
      );
    }

    // Limpiamos cualquier resto de una grabación anterior.
    _chunks.clear();

    // Resolvemos el micrófono elegido (si hay uno) a su InputDevice; si no se
    // fijó ninguno o ya no existe, config.device queda null = por defecto.
    InputDevice? device;
    if (_deviceId != null) {
      final devices = await _recorder.listInputDevices();
      for (final d in devices) {
        if (d.id == _deviceId) {
          device = d;
          break;
        }
      }
    }
    // Reusamos los parámetros de [_config] y solo fijamos el dispositivo (el
    // copyWith de RecordConfig envuelve `device` en un record, así que es más
    // legible construirlo aquí con `device:` directo, que es InputDevice?).
    final RecordConfig config = RecordConfig(
      encoder: _config.encoder,
      sampleRate: _config.sampleRate,
      numChannels: _config.numChannels,
      device: device,
    );

    final Stream<Uint8List> stream = await _recorder.startStream(config);
    _recording = true;

    _subscription = stream.listen(
      (Uint8List chunk) {
        // Acumulamos el chunk para reconstruir el audio completo en stop().
        _chunks.add(chunk);
        // Publicamos el nivel de amplitud aproximado de este chunk.
        _publishAmplitude(chunk);
      },
      onError: (Object error, StackTrace stackTrace) {
        // Propagamos el error al medidor para que la UI pueda reaccionar.
        if (!_amplitudeController.isClosed) {
          _amplitudeController.addError(error, stackTrace);
        }
      },
      cancelOnError: false,
    );
  }

  @override
  Future<Float32List> stop() async {
    _checkNotDisposed();

    if (!_recording) {
      // No había grabación activa: devolvemos audio vacío.
      return Float32List(0);
    }

    _recording = false;

    // Detenemos el grabador y cancelamos la suscripción al stream.
    await _recorder.stop();
    await _subscription?.cancel();
    _subscription = null;

    // Reconstruimos el audio: concatenamos los chunks y convertimos a float32.
    final Uint8List pcm = concatChunks(_chunks);
    final Float32List muestras = pcm16ToFloat32(pcm);

    // Limpiamos el buffer para la próxima grabación.
    _chunks.clear();

    return muestras;
  }

  @override
  Stream<double> get amplitude => _amplitudeController.stream;

  @override
  Future<void> dispose() async {
    if (_disposed) {
      return;
    }
    _disposed = true;
    _recording = false;

    // Cancelamos la suscripción primero para dejar de recibir chunks.
    await _subscription?.cancel();
    _subscription = null;

    // Cerramos el grabador. Si ya estaba parado, dispose() es seguro.
    _recorder.dispose();

    // Cerramos el emisor de amplitud.
    if (!_amplitudeController.isClosed) {
      await _amplitudeController.close();
    }

    _chunks.clear();
  }

  /// Calcula el nivel de amplitud (0..1) de un chunk PCM 16-bit little-endian
  /// usando la raíz cuadrática media (RMS) normalizada a [0, 1] y lo publica.
  void _publishAmplitude(Uint8List chunk) {
    if (_amplitudeController.isClosed) {
      return;
    }

    final int sampleCount = chunk.lengthInBytes ~/ 2;
    if (sampleCount == 0) {
      return;
    }

    final ByteData view = ByteData.sublistView(chunk);
    var sumaCuadrados = 0.0;
    for (var i = 0; i < sampleCount; i++) {
      final int s = view.getInt16(i * 2, Endian.little);
      final double normalizado = s / 32768.0;
      sumaCuadrados += normalizado * normalizado;
    }

    final double rms = math.sqrt(sumaCuadrados / sampleCount);
    // RMS ya está en [0, 1]; lo acotamos por seguridad ante redondeos.
    final double nivel = rms.clamp(0.0, 1.0);

    _amplitudeController.add(nivel);
  }

  /// Lanza un error si la instancia ya fue liberada con [dispose].
  void _checkNotDisposed() {
    if (_disposed) {
      throw StateError(
        'AudioCaptureRecord ya fue liberado con dispose(); crea una instancia '
        'nueva para volver a capturar audio.',
      );
    }
  }
}
