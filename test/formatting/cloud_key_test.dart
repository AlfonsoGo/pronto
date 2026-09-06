// ignore_for_file: require_trailing_commas
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:pronto/src/features/formatting/cloud_key.dart';
import 'package:pronto/src/platform/credential_store.dart';
import 'package:pronto/src/platform/windows/windows_credential_store.dart';

// La clave del proveedor NO puede acabar en un fichero de ajustes en claro
// (ya pasó una vez en otro proyecto: un PAT y un service_role guardados tal
// cual). Aquí se prueba contra el almacén REAL de Windows, no contra un doble:
// un almacén simulado que "dice que sí" no demuestra que el secreto se guarde.

void main() {
  group('CloudKey sobre un almacén en memoria', () {
    late InMemoryCredentialStore almacen;
    late CloudKey clave;

    setUp(() {
      almacen = InMemoryCredentialStore();
      clave = CloudKey(almacen);
    });

    test('sin guardar nada, no hay clave', () {
      expect(clave.read(), isNull);
      expect(clave.exists, isFalse);
    });

    test('guardar y leer', () {
      expect(clave.save('sk-or-v1-secreta'), isTrue);
      expect(clave.read(), 'sk-or-v1-secreta');
      expect(clave.exists, isTrue);
    });

    test('se recorta lo que se pega de más (espacios y saltos de línea)', () {
      // Pegar una clave de una web arrastra un salto de línea; con él, la
      // cabecera Authorization saldría mal formada y el proveedor daría 401.
      // Se mira lo GUARDADO, no lo leído: si solo se comprobara `read()`, un
      // `save()` que no recortara pasaría el test igual, porque `read()`
      // recorta también. El mutante lo cazó.
      clave.save('  sk-or-v1-secreta \n');
      expect(almacen.read(CloudKey.target), 'sk-or-v1-secreta');
      expect(clave.read(), 'sk-or-v1-secreta');
    });

    test('guardar una cadena vacía BORRA', () {
      clave.save('sk-or-v1-secreta');
      clave.save('   ');
      expect(clave.exists, isFalse);
    });

    test('borrar deja el almacén sin clave', () {
      clave.save('sk-or-v1-secreta');
      expect(clave.clear(), isTrue);
      expect(clave.read(), isNull);
    });

    test('borrar cuando no hay nada no es un fallo', () {
      expect(clave.clear(), isTrue);
    });
  });

  group('almacén REAL de credenciales de Windows', () {
    // Nombre propio del test para no pisar la credencial de verdad de Pronto.
    const target = 'Pronto/TEST-credential-store';
    const store = WindowsCredentialStore();

    setUp(() {
      expect(Platform.isWindows, isTrue,
          reason: 'Pronto es una app de Windows: este test no se salta, falla');
      store.delete(target);
    });

    tearDown(() => store.delete(target));

    test('lo que se guarda se lee IGUAL tras volver a pedirlo', () {
      expect(store.write(target, 'sk-or-v1-abc123'), isTrue);
      expect(store.read(target), 'sk-or-v1-abc123');
    });

    test('sobrescribir cambia el valor', () {
      store.write(target, 'primera');
      store.write(target, 'segunda');
      expect(store.read(target), 'segunda');
    });

    test('un secreto con tildes y emoji vuelve intacto (UTF-8)', () {
      const raro = 'clave-ñáéíóú-🍷-fin';
      store.write(target, raro);
      expect(store.read(target), raro);
    });

    test('el máximo de Windows entra; uno más se RECHAZA sin guardar nada', () {
      // MEDIDO contra el almacén real: 2560 bytes entran, 2561 no. Lo que
      // importa es que el fallo se note — `write` devuelve false y no queda un
      // secreto a medias — porque la interfaz avisa con eso.
      final tope = 'k' * WindowsCredentialStore.maxSecretBytes;
      expect(store.write(target, tope), isTrue);
      expect(store.read(target), tope);

      final pasado = 'k' * (WindowsCredentialStore.maxSecretBytes + 1);
      expect(store.write(target, pasado), isFalse);
      expect(store.read(target), tope, reason: 'lo guardado no se corrompe');
    });

    test('una clave de OpenRouter de verdad cabe de sobra', () {
      final real = 'sk-or-v1-${'a' * 64}';
      expect(store.write(target, real), isTrue);
      expect(store.read(target), real);
    });

    test('leer algo que no existe devuelve null, no revienta', () {
      expect(store.read('Pronto/TEST-no-existe-jamas'), isNull);
    });

    test('borrar de verdad borra, y borrar dos veces no falla', () {
      store.write(target, 'x');
      expect(store.delete(target), isTrue);
      expect(store.read(target), isNull);
      expect(store.delete(target), isTrue);
    });

    test('CloudKey de verdad: guardar, leer y borrar', () {
      // El camino de ACTIVACIÓN completo, con el almacén real: es lo que hace
      // el botón «Guardar» de Ajustes.
      const almacen = _AlmacenDePrueba(store, target);
      const clave = CloudKey(almacen);
      expect(clave.exists, isFalse);
      clave.save('sk-or-v1-de-verdad');
      expect(clave.read(), 'sk-or-v1-de-verdad');
      clave.clear();
      expect(clave.exists, isFalse);
    });
  });
}

/// Redirige [CloudKey] a un nombre de credencial de PRUEBA, para no tocar la
/// credencial real del usuario que corre los tests.
class _AlmacenDePrueba implements CredentialStore {
  const _AlmacenDePrueba(this._real, this._target);

  final CredentialStore _real;
  final String _target;

  @override
  String? read(String target) => _real.read(_target);

  @override
  bool write(String target, String secret) => _real.write(_target, secret);

  @override
  bool delete(String target) => _real.delete(_target);
}
