import 'dart:convert';
import 'dart:ffi';

import 'package:ffi/ffi.dart';
import 'package:win32/win32.dart';

import '../credential_store.dart';

/// Almacén de credenciales de Windows (el que se ve en «Administrador de
/// credenciales → Credenciales genéricas»).
///
/// Windows cifra el blob con la sesión del usuario: otro usuario de la misma
/// máquina no lo lee, y no queda en ningún fichero de la app. El secreto se
/// guarda como UTF-8 y solo vive en memoria el tiempo de la llamada que lo usa.
class WindowsCredentialStore implements CredentialStore {
  const WindowsCredentialStore();

  @override
  String? read(String target) {
    final pTarget = target.toNativeUtf16();
    final ppCred = calloc<Pointer<CREDENTIAL>>();
    try {
      if (CredRead(pTarget, CRED_TYPE_GENERIC, 0, ppCred) == 0) return null;
      final cred = ppCred.value.ref;
      if (cred.CredentialBlobSize == 0) return null;
      // Copia inmediata a Dart: el buffer nativo se libera al salir.
      final bytes = cred.CredentialBlob.asTypedList(cred.CredentialBlobSize);
      return utf8.decode(List<int>.from(bytes), allowMalformed: true);
    } finally {
      if (ppCred.value != nullptr) CredFree(ppCred.value);
      calloc.free(ppCred);
      calloc.free(pTarget);
    }
  }

  /// Tamaño máximo del secreto, en BYTES UTF-8.
  ///
  /// Es el `CRED_MAX_CREDENTIAL_BLOB_SIZE` de Windows (5 × 512) y lo impone el
  /// PROPIO Windows: MEDIDO contra el almacén real, 2560 entra y 2561 se
  /// rechaza sin guardar nada, con `CredWrite` devolviendo 0. Por eso aquí no
  /// hay ninguna comprobación duplicada: [write] ya devuelve `false` solo, y
  /// la interfaz avisa con eso en vez de decir "guardada" sin guardarla. La
  /// constante existe para que el test pueda nombrar el límite. Una clave de
  /// OpenRouter ocupa ~73 bytes.
  static const int maxSecretBytes = 2560;

  /// Guarda el secreto. Un secreto vacío no lo manda nadie: [CloudKey.save]
  /// convierte "vaciar el campo" en un borrado antes de llegar aquí, y esa
  /// regla se comprueba allí.
  @override
  bool write(String target, String secret) {
    final bytes = utf8.encode(secret);
    final pTarget = target.toNativeUtf16();
    final pUser = 'Pronto'.toNativeUtf16();
    final pBlob = calloc<Uint8>(bytes.length);
    final pCred = calloc<CREDENTIAL>();
    try {
      pBlob.asTypedList(bytes.length).setAll(0, bytes);
      pCred.ref
        ..Type = CRED_TYPE_GENERIC
        ..TargetName = pTarget
        ..CredentialBlobSize = bytes.length
        ..CredentialBlob = pBlob
        ..Persist = CRED_PERSIST_LOCAL_MACHINE
        ..UserName = pUser;
      return CredWrite(pCred, 0) != 0;
    } finally {
      // El secreto se borra de la memoria nativa antes de devolverla: un
      // volcado del proceso no tiene por qué llevárselo de regalo.
      pBlob.asTypedList(bytes.length).fillRange(0, bytes.length, 0);
      bytes.fillRange(0, bytes.length, 0);
      calloc.free(pCred);
      calloc.free(pBlob);
      calloc.free(pUser);
      calloc.free(pTarget);
    }
  }

  @override
  bool delete(String target) {
    final pTarget = target.toNativeUtf16();
    try {
      if (CredDelete(pTarget, CRED_TYPE_GENERIC, 0) != 0) return true;
      // No existía: el estado final es el que se pedía.
      return GetLastError() == ERROR_NOT_FOUND;
    } finally {
      calloc.free(pTarget);
    }
  }
}
