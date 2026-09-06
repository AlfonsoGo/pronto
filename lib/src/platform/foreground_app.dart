/// Quién tiene el foco cuando se pega el dictado.
///
/// El Historial lo apunta para poder contestar «¿dónde escribo más?» y para que
/// una entrada suelta se reconozca de un vistazo. Es solo LECTURA: no toca el
/// foco ni activa nada.
class ForegroundApp {
  /// Ejecutable sin ruta ni extensión: `OUTLOOK`, `chrome`, `WindowsTerminal`.
  final String? appName;

  /// Título de la ventana. ⚠ Puede llevar dentro el asunto de un correo o el
  /// nombre de un cliente: vive en la misma base local que el resto del
  /// historial y se borra con él.
  final String? windowTitle;

  const ForegroundApp({this.appName, this.windowTitle});

  bool get isEmpty => (appName ?? '').isEmpty && (windowTitle ?? '').isEmpty;
}

/// Lee la ventana en primer plano. Devuelve `null` cuando no hay ninguna, o
/// cuando la que hay es NUESTRA (el panel de Pronto abierto): apuntar «Pronto»
/// como app de destino sería mentir sobre dónde acabó el texto.
abstract class ForegroundAppReader {
  ForegroundApp? current();
}

/// Lector que no sabe nada. Es lo que se usa fuera de Windows y en los tests:
/// el historial se queda sin el campo, que es la verdad.
class UnknownForegroundApp implements ForegroundAppReader {
  const UnknownForegroundApp();

  @override
  ForegroundApp? current() => null;
}
