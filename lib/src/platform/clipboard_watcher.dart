/// Observa cambios en el portapapeles del sistema y emite el texto nuevo.
///
/// Se usa para la captura de ediciones externas: si tras dictar el usuario
/// copia (Ctrl+C) una versión corregida del texto, lo detectamos y aprendemos.
///
/// Implementación Windows: [WindowsClipboardWatcher] (sondeo de
/// GetClipboardSequenceNumber, sin necesidad de bucle de mensajes).
abstract class ClipboardWatcher {
  /// Emite el texto del portapapeles cada vez que cambia (solo CF_UNICODETEXT).
  Stream<String> get changes;

  /// Empieza a observar (idempotente).
  void start();

  /// Deja de observar.
  void stop();
}
