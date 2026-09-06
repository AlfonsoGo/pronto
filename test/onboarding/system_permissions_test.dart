import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:pronto/src/platform/audio_capture.dart';
import 'package:pronto/src/platform/system_permissions.dart';
import 'package:pronto/src/platform/windows/windows_system_permissions.dart';

// El bloque de Permisos del asistente (PRNT-13).
//
// El ticket pide «cada uno con su estado real, no un texto fijo». Lo que estas
// pruebas fijan es justo eso: que el estado SALE de una comprobación y que
// cuando la comprobación no se puede hacer, se dice — en vez de enseñar un ✓
// que nadie ha ganado.

/// Micrófono de mentira que contesta lo que se le diga (o revienta).
class _MicFalso implements AudioCapture {
  final bool? respuesta;
  _MicFalso(this.respuesta);

  @override
  Future<bool> hasPermission() async {
    final r = respuesta;
    if (r == null) throw StateError('el driver no contesta');
    return r;
  }

  @override
  Stream<double> get amplitude => const Stream.empty();
  @override
  Future<void> dispose() async {}
  @override
  Future<List<MicDevice>> listInputDevices() async => const [];
  @override
  void setInputDeviceId(String? id) {}
  @override
  Future<void> start() async {}
  @override
  Future<Float32List> stop() async => Float32List(0);
}

void main() {
  group('micrófono', () {
    test('con permiso: concedido', () async {
      final p = WindowsSystemPermissions(_MicFalso(true), elevado: () => false);
      final r = await p.microphone();
      expect(r.estado, PermissionState.concedido);
      expect(r.ok, isTrue);
    });

    test('sin permiso: denegado, y dice dónde se arregla', () async {
      final p = WindowsSystemPermissions(_MicFalso(false), elevado: () => false);
      final r = await p.microphone();
      expect(r.estado, PermissionState.denegado);
      expect(r.ok, isFalse);
      expect(r.detalle, contains('Configuración'));
    });

    test('si la comprobación revienta NO se dice «concedido»', () async {
      // Es la diferencia entre no saber y decir que sí. Un ✓ aquí mandaría al
      // usuario a dictar contra un micrófono que Windows tiene cerrado.
      final p = WindowsSystemPermissions(_MicFalso(null), elevado: () => false);
      final r = await p.microphone();
      expect(r.estado, PermissionState.desconocido);
      expect(r.ok, isFalse);
    });
  });

  group('escribir en otras aplicaciones', () {
    test('sin elevar: se puede escribir, y se dice dónde NO', () async {
      final p = WindowsSystemPermissions(_MicFalso(true), elevado: () => false);
      final r = await p.textInjection();
      expect(r.estado, PermissionState.concedido);
      expect(r.detalle, contains('administrador'));
      expect(r.detalle, contains('portapapeles'));
    });

    test('elevado: se puede escribir en todo, y el texto CAMBIA', () async {
      final normal =
          await WindowsSystemPermissions(_MicFalso(true), elevado: () => false)
              .textInjection();
      final admin =
          await WindowsSystemPermissions(_MicFalso(true), elevado: () => true)
              .textInjection();
      expect(admin.estado, PermissionState.concedido);
      // Si los dos textos fueran iguales, esto sería el «texto fijo» que el
      // ticket prohíbe: parecería una comprobación sin serlo.
      expect(admin.detalle, isNot(normal.detalle));
    });

    test('si no se sabe con qué privilegios corre, se dice', () async {
      final p = WindowsSystemPermissions(_MicFalso(true), elevado: () => null);
      final r = await p.textInjection();
      expect(r.estado, PermissionState.desconocido);
    });
  });
}
