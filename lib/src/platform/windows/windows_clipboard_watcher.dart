import 'dart:async';
import 'dart:ffi';

import 'package:ffi/ffi.dart';
import 'package:win32/win32.dart';

import '../clipboard_watcher.dart';

/// Monitor de portapapeles para Windows basado en SONDEO.
///
/// En lugar de registrar un listener nativo (AddClipboardFormatListener), que
/// exigiría una ventana y un bucle de mensajes, sondea
/// `GetClipboardSequenceNumber()` cada [_pollInterval]. Ese contador del sistema
/// cambia con cada modificación del portapapeles, así que comparar su valor es
/// barato y fiable. Cuando cambia, leemos el texto Unicode y lo emitimos.
class WindowsClipboardWatcher implements ClipboardWatcher {
  WindowsClipboardWatcher({Duration? pollInterval})
      : _pollInterval = pollInterval ?? const Duration(milliseconds: 500);

  final Duration _pollInterval;
  final StreamController<String> _controller = StreamController<String>.broadcast();

  Timer? _timer;
  int _lastSeq = -1;

  @override
  Stream<String> get changes => _controller.stream;

  @override
  void start() {
    if (_timer != null) return;
    // Tomamos la secuencia actual como base para NO emitir el contenido que ya
    // hubiera al arrancar.
    _lastSeq = GetClipboardSequenceNumber();
    _timer = Timer.periodic(_pollInterval, (_) => _poll());
  }

  @override
  void stop() {
    _timer?.cancel();
    _timer = null;
  }

  void _poll() {
    final seq = GetClipboardSequenceNumber();
    if (seq == _lastSeq) return;
    _lastSeq = seq;
    final text = _readClipboardText();
    if (text != null && text.isNotEmpty && !_controller.isClosed) {
      _controller.add(text);
    }
  }

  /// Lee el texto (CF_UNICODETEXT) del portapapeles, o null si no hay.
  String? _readClipboardText() {
    if (OpenClipboard(NULL) == 0) return null;
    try {
      final int hData = GetClipboardData(CF_UNICODETEXT);
      if (hData == NULL) return null;
      final Pointer<Utf16> pData =
          GlobalLock(Pointer.fromAddress(hData)).cast<Utf16>();
      if (pData == nullptr) return null;
      try {
        return pData.toDartString();
      } finally {
        GlobalUnlock(Pointer.fromAddress(hData));
      }
    } finally {
      CloseClipboard();
    }
  }

  void dispose() {
    stop();
    _controller.close();
  }
}
