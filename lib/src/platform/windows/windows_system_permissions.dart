import 'dart:ffi';
import 'dart:io';

import 'package:ffi/ffi.dart';
import 'package:win32/win32.dart';

import '../audio_capture.dart';
import '../system_permissions.dart';

/// `TokenElevation` de `TOKEN_INFORMATION_CLASS`. Se escribe el número a mano
/// porque la constante del paquete `win32` está marcada como obsoleta y cambia
/// de sitio entre versiones; el valor, en cambio, es parte del contrato de
/// Windows y no se mueve.
const int _tokenElevation = 20;

/// Comprobación real de lo que Windows deja hacer a Pronto.
///
/// El micrófono se pregunta al mismo objeto que graba —no a una copia del
/// estado— para que la respuesta sea la de ahora mismo y no la del arranque.
class WindowsSystemPermissions implements SystemPermissions {
  final AudioCapture _audio;

  /// Inyectable para las pruebas: en un test no hay proceso de Windows al que
  /// preguntarle si va elevado.
  final bool? Function() _elevado;

  WindowsSystemPermissions(
    this._audio, {
    bool? Function()? elevado,
  }) : _elevado = elevado ?? procesoElevado;

  @override
  Future<PermissionCheck> microphone() async {
    try {
      final ok = await _audio.hasPermission();
      if (ok) {
        return const PermissionCheck(
          PermissionState.concedido,
          'Windows deja a Pronto grabar del micrófono.',
        );
      }
      return const PermissionCheck(
        PermissionState.denegado,
        'Windows no deja a Pronto usar el micrófono. Actívalo en '
        'Configuración > Privacidad y seguridad > Micrófono.',
      );
    } catch (_) {
      return const PermissionCheck(
        PermissionState.desconocido,
        'No se pudo comprobar el micrófono. Revísalo en Configuración > '
        'Privacidad y seguridad > Micrófono.',
      );
    }
  }

  /// Escribir en otras aplicaciones NO es un permiso de Windows: es una
  /// consecuencia de los privilegios del proceso.
  ///
  /// Windows impide que un proceso normal mande teclas a una ventana que corre
  /// como administrador (UIPI). Pronto no se ejecuta elevado a propósito, así
  /// que lo honesto no es un ✓ ni una ✗, sino decir exactamente dónde funciona
  /// y dónde no. Solo se marca [PermissionState.denegado] cuando de verdad no
  /// se puede comprobar nada.
  @override
  Future<PermissionCheck> textInjection() async {
    final elevado = _elevado();
    if (elevado == null) {
      return const PermissionCheck(
        PermissionState.desconocido,
        'No se pudo comprobar con qué privilegios corre Pronto. El dictado de '
        'prueba te dirá si el texto llega a tu aplicación.',
      );
    }
    if (elevado) {
      return const PermissionCheck(
        PermissionState.concedido,
        'Pronto corre como administrador: puede escribir en cualquier '
        'aplicación, incluidas las abiertas como administrador.',
      );
    }
    return const PermissionCheck(
      PermissionState.concedido,
      'Pronto escribe en tus aplicaciones normales. En las que hayas abierto '
      'como administrador, Windows bloquea la escritura y el texto se queda '
      'en el portapapeles para que lo pegues tú.',
    );
  }

  @override
  Future<bool> openMicrophoneSettings() async {
    if (!Platform.isWindows) return false;
    try {
      // URI fijo del propio Windows, sin nada del usuario dentro. Se lanza con
      // `start` porque `ms-settings:` es un protocolo, no un ejecutable.
      final r = await Process.run(
        'cmd',
        const ['/c', 'start', '', 'ms-settings:privacy-microphone'],
      );
      return r.exitCode == 0;
    } catch (_) {
      return false;
    }
  }

  /// ¿Corre este proceso con el token elevado? `null` si no se pudo saber.
  static bool? procesoElevado() {
    if (!Platform.isWindows) return null;
    final token = calloc<IntPtr>();
    final info = calloc<Uint32>();
    final devuelto = calloc<Uint32>();
    try {
      if (OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, token) == 0) {
        return null;
      }
      final ok = GetTokenInformation(
        token.value,
        _tokenElevation,
        info,
        sizeOf<Uint32>(),
        devuelto,
      );
      if (ok == 0) return null;
      return info.value != 0;
    } catch (_) {
      return null;
    } finally {
      if (token.value != 0) CloseHandle(token.value);
      calloc.free(token);
      calloc.free(info);
      calloc.free(devuelto);
    }
  }
}
