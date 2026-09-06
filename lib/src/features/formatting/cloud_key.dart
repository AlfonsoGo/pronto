import 'dart:io';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../platform/credential_store.dart';
import '../../platform/windows/windows_credential_store.dart';

/// La clave del proveedor de nube: la pone el usuario y es SUYA.
///
/// Reglas que este objeto existe para que no se rompan:
///  - La clave no se guarda en los ajustes ni en la base de datos: va al
///    almacén de credenciales del sistema.
///  - No se cachea en ningún estado de la app. Se lee del almacén justo cuando
///    hay que mandar una petición y se olvida. Así no aparece en un volcado de
///    providers, ni en un log de estado, ni en un informe de fallo.
///  - Lo que la interfaz conoce es si HAY clave, nunca cuál es
///    ([cloudKeyPresenceProvider]).
class CloudKey {
  const CloudKey(this._store);

  final CredentialStore _store;

  /// Nombre bajo el que aparece en el Administrador de credenciales.
  static const String target = 'Pronto/OpenRouter';

  /// La clave, o `null` si no hay. Nunca se guarda el resultado.
  String? read() {
    final v = _store.read(target)?.trim();
    return (v == null || v.isEmpty) ? null : v;
  }

  /// Guarda la clave. Una cadena vacía BORRA, que es lo que espera quien vacía
  /// el campo y le da a guardar.
  bool save(String key) {
    final v = key.trim();
    if (v.isEmpty) return clear();
    return _store.write(target, v);
  }

  bool clear() => _store.delete(target);

  bool get exists => read() != null;
}

final credentialStoreProvider = Provider<CredentialStore>((ref) {
  if (Platform.isWindows) return const WindowsCredentialStore();
  // Otras plataformas todavía no tienen almacén: se usa uno en memoria, que no
  // persiste. Es preferible a escribir un secreto en un fichero en claro.
  return InMemoryCredentialStore();
});

final cloudKeyProvider = Provider<CloudKey>(
  (ref) => CloudKey(ref.watch(credentialStoreProvider)),
);

/// Si hay clave guardada. Es lo ÚNICO que la interfaz sabe de la clave.
final cloudKeyPresenceProvider =
    NotifierProvider<CloudKeyPresence, bool>(CloudKeyPresence.new);

class CloudKeyPresence extends Notifier<bool> {
  @override
  bool build() => ref.watch(cloudKeyProvider).exists;

  /// Vuelve a mirar el almacén tras guardar o borrar.
  void refresh() => state = ref.read(cloudKeyProvider).exists;
}
