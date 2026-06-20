/// Cómo se inserta el texto en la app con foco.
enum InjectionMode {
  /// Guardar portapapeles -> poner texto -> Ctrl+V -> restaurar.
  /// Rápido y fiable para texto largo y Unicode (acentos, ñ, emoji).
  clipboardPaste,

  /// SendInput con KEYEVENTF_UNICODE, caracter a caracter.
  /// No toca el portapapeles; útil en apps que ignoran Ctrl+V (terminales).
  unicodeSendInput,
}

/// Inserta texto a nivel de sistema en la aplicación que tenga el foco.
///
/// Implementación por plataforma:
/// - Windows: [WindowsTextInjector] (win32 FFI).
/// - macOS/Linux/móvil: pendiente (ver ROADMAP.md).
abstract class TextInjector {
  /// Inserta [text] donde esté el cursor de la app enfocada.
  Future<void> insert(
    String text, {
    InjectionMode mode = InjectionMode.clipboardPaste,
  });
}
