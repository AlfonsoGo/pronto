/// Si Windows nos deja hacer algo, y qué contarle al usuario si no.
enum PermissionState {
  /// Comprobado y disponible.
  concedido,

  /// Comprobado y NO disponible: hay algo que el usuario tiene que tocar.
  denegado,

  /// No se pudo comprobar (plataforma sin implementar, API que falló). Es un
  /// estado propio a conciencia: decir «concedido» sin haberlo mirado sería
  /// inventarse el resultado de una comprobación.
  desconocido,
}

/// El resultado de mirar un permiso, con la frase que se enseña al usuario.
///
/// ⚠ [detalle] es SIEMPRE una cadena nuestra, escrita en este repo. Ningún
/// texto de esta pantalla puede salir de un modelo de lenguaje.
class PermissionCheck {
  final PermissionState estado;
  final String detalle;

  const PermissionCheck(this.estado, this.detalle);

  bool get ok => estado == PermissionState.concedido;
}

/// Lo que Pronto necesita del sistema para poder dictar.
///
/// Son dos cosas distintas y se comprueban distinto: el micrófono es un permiso
/// de verdad, que Windows concede o niega; escribir en otras aplicaciones no lo
/// es —no existe tal permiso— y lo único real que se puede decir ahí es hasta
/// dónde llega Pronto con los privilegios que tiene.
abstract class SystemPermissions {
  /// ¿Puede Pronto grabar del micrófono?
  Future<PermissionCheck> microphone();

  /// ¿Hasta dónde puede escribir Pronto en otras aplicaciones?
  Future<PermissionCheck> textInjection();

  /// Abre la pantalla de Windows donde se arregla el micrófono. Devuelve si se
  /// pudo abrir: si no, la pantalla dice la ruta a mano en vez de callarse.
  Future<bool> openMicrophoneSettings();
}
