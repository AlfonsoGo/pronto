/// Dónde se guardan los secretos del usuario.
///
/// Un secreto NUNCA va a los ajustes: `shared_preferences` es un JSON en claro
/// en el perfil del usuario, y una clave de API ahí es una clave regalada a
/// cualquier cosa que lea ficheros. Va al almacén de credenciales del sistema,
/// que la cifra con la sesión de Windows.
///
/// El API es SÍNCRONO a propósito: en Windows leer una credencial es una
/// llamada FFI que tarda microsegundos, y hacerlo async obligaría a arrastrar
/// futuros por todo el camino del dictado para no ganar nada.
abstract class CredentialStore {
  /// El secreto guardado bajo [target], o `null` si no hay ninguno.
  String? read(String target);

  /// Guarda (o reemplaza) el secreto. Devuelve `false` si el sistema lo
  /// rechazó: guardar una clave y creer que se guardó es peor que fallar.
  bool write(String target, String secret);

  /// Borra el secreto. Devuelve `false` si no se pudo borrar (borrar algo que
  /// no existe cuenta como éxito: el resultado es el que se pedía).
  bool delete(String target);
}

/// Almacén en memoria para los tests y para plataformas sin implementación.
/// No persiste nada: lo que guarda muere con el proceso.
class InMemoryCredentialStore implements CredentialStore {
  final Map<String, String> _secretos = {};

  @override
  String? read(String target) => _secretos[target];

  @override
  bool write(String target, String secret) {
    _secretos[target] = secret;
    return true;
  }

  @override
  bool delete(String target) {
    _secretos.remove(target);
    return true;
  }
}
