import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;
import 'package:pronto/src/data/data_backup.dart';

void main() {
  late Directory raiz;
  late Directory datos;
  late Directory copias;

  setUp(() {
    raiz = Directory.systemTemp.createTempSync('pronto-copia-');
    datos = Directory(p.join(raiz.path, 'Pronto'))..createSync(recursive: true);
    copias = Directory(p.join(raiz.path, 'copias'));
  });

  tearDown(() {
    if (raiz.existsSync()) raiz.deleteSync(recursive: true);
  });

  /// Deja la carpeta de datos como la de un usuario de verdad: la base, los
  /// ajustes y los 640 MB de modelo (aquí, cuatro ficheros de mentira).
  void carpetaDeUsuario() {
    File(p.join(datos.path, 'pronto.db')).writeAsStringSync('base con datos');
    File(p.join(datos.path, 'shared_preferences.json'))
        .writeAsStringSync('{"app_settings":"{}"}');
    final modelo = Directory(p.join(datos.path, 'models', 'parakeet'))
      ..createSync(recursive: true);
    for (final f in [
      'encoder.int8.onnx',
      'decoder.int8.onnx',
      'joiner.int8.onnx',
      'tokens.txt',
    ]) {
      File(p.join(modelo.path, f)).writeAsStringSync('x' * 1000);
    }
  }

  final fecha = DateTime(2026, 9, 6, 14, 32);

  test('copia los datos del usuario con su contenido intacto', () async {
    carpetaDeUsuario();

    final ruta = await copiarCarpetaDeDatos(
      origen: datos,
      destinoRaiz: copias,
      fecha: fecha,
    );

    expect(
      File(p.join(ruta, 'pronto.db')).readAsStringSync(),
      'base con datos',
    );
    expect(
      File(p.join(ruta, 'shared_preferences.json')).readAsStringSync(),
      '{"app_settings":"{}"}',
    );
  });

  test('NO copia models/: son 640 MB que se vuelven a descargar', () async {
    carpetaDeUsuario();

    final ruta = await copiarCarpetaDeDatos(
      origen: datos,
      destinoRaiz: copias,
      fecha: fecha,
    );

    expect(Directory(p.join(ruta, 'models')).existsSync(), isFalse);
    // Y sobre todo: el modelo sigue donde estaba. Actualizar no puede acabar
    // en una redescarga de 640 MB.
    expect(
      File(p.join(datos.path, 'models', 'parakeet', 'encoder.int8.onnx'))
          .existsSync(),
      isTrue,
    );
  });

  test('el nombre de la copia lleva la fecha', () async {
    carpetaDeUsuario();

    final ruta = await copiarCarpetaDeDatos(
      origen: datos,
      destinoRaiz: copias,
      fecha: fecha,
    );

    expect(p.basename(ruta), 'copia-2026-09-06-1432');
  });

  test('dos copias en el mismo minuto no se pisan', () async {
    carpetaDeUsuario();

    final a = await copiarCarpetaDeDatos(
      origen: datos,
      destinoRaiz: copias,
      fecha: fecha,
    );
    File(p.join(datos.path, 'pronto.db')).writeAsStringSync('segunda versión');
    final b = await copiarCarpetaDeDatos(
      origen: datos,
      destinoRaiz: copias,
      fecha: fecha,
    );

    expect(a, isNot(b));
    expect(File(p.join(a, 'pronto.db')).readAsStringSync(), 'base con datos');
    expect(File(p.join(b, 'pronto.db')).readAsStringSync(), 'segunda versión');
  });

  test('copia las subcarpetas que no están excluidas', () async {
    carpetaDeUsuario();
    Directory(p.join(datos.path, 'logs')).createSync();
    File(p.join(datos.path, 'logs', 'ayer.txt')).writeAsStringSync('hola');

    final ruta = await copiarCarpetaDeDatos(
      origen: datos,
      destinoRaiz: copias,
      fecha: fecha,
    );

    expect(File(p.join(ruta, 'logs', 'ayer.txt')).readAsStringSync(), 'hola');
  });

  group('rotura', () {
    test('nombres con tildes, ñ y emoji sobreviven a la copia', () async {
      File(p.join(datos.path, 'añoveros — copia (1) 🎤.txt'))
          .writeAsStringSync('ñandú');

      final ruta = await copiarCarpetaDeDatos(
        origen: datos,
        destinoRaiz: copias,
        fecha: fecha,
      );

      expect(
        File(p.join(ruta, 'añoveros — copia (1) 🎤.txt')).readAsStringSync(),
        'ñandú',
      );
    });

    test('una carpeta de datos vacía se copia sin quejarse', () async {
      final ruta = await copiarCarpetaDeDatos(
        origen: datos,
        destinoRaiz: copias,
        fecha: fecha,
      );

      expect(Directory(ruta).existsSync(), isTrue);
      expect(Directory(ruta).listSync(), isEmpty);
    });

    test('una copia interrumpida no queda con nombre de copia buena', () async {
      carpetaDeUsuario();
      var copiados = 0;

      await expectLater(
        copiarCarpetaDeDatos(
          origen: datos,
          destinoRaiz: copias,
          fecha: fecha,
          copiarFichero: (f, destino) async {
            if (copiados++ >= 1) throw const FileSystemException('disco lleno');
            await f.copy(destino);
          },
        ),
        throwsA(isA<FileSystemException>()),
      );

      // Ni copia buena ni restos a medias: quien mire la carpeta ve que NO
      // tiene copia, que es la verdad.
      expect(
        copias.existsSync()
            ? copias.listSync().map((e) => p.basename(e.path)).toList()
            : <String>[],
        isEmpty,
      );
    });

    test('mientras se copia, la carpeta con nombre bueno NO existe todavía',
        () async {
      // Si se copiara directamente sobre el nombre final, matar el proceso a
      // mitad (que no lanza excepción y no limpia nada) dejaría media copia
      // con pinta de copia entera. Por eso se escribe en `.a-medias` y solo se
      // renombra al terminar.
      carpetaDeUsuario();
      final buena = p.join(copias.path, 'copia-2026-09-06-1432');
      var visto = 0;

      await copiarCarpetaDeDatos(
        origen: datos,
        destinoRaiz: copias,
        fecha: fecha,
        copiarFichero: (f, destino) async {
          visto++;
          expect(
            Directory(buena).existsSync(),
            isFalse,
            reason: 'a media copia ya había una carpeta con nombre de copia '
                'buena',
          );
          await f.copy(destino);
        },
      );

      expect(visto, greaterThan(0));
      expect(Directory(buena).existsSync(), isTrue);
    });

    test('unos restos a medias de un intento anterior no se toman por copia',
        () async {
      carpetaDeUsuario();
      Directory(p.join(copias.path, 'copia-2026-09-06-1432$kSufijoParcial'))
          .createSync(recursive: true);

      final ruta = await copiarCarpetaDeDatos(
        origen: datos,
        destinoRaiz: copias,
        fecha: fecha,
      );

      expect(p.basename(ruta), isNot('copia-2026-09-06-1432'));
      expect(File(p.join(ruta, 'pronto.db')).existsSync(), isTrue);
    });

    test('una carpeta de datos que no existe se dice, no se inventa', () async {
      await expectLater(
        copiarCarpetaDeDatos(
          origen: Directory(p.join(raiz.path, 'no-existe')),
          destinoRaiz: copias,
          fecha: fecha,
        ),
        throwsA(isA<StateError>()),
      );
    });

    test('las copias no pueden vivir dentro de la carpeta de datos', () async {
      carpetaDeUsuario();

      await expectLater(
        copiarCarpetaDeDatos(
          origen: datos,
          destinoRaiz: Directory(p.join(datos.path, 'copias')),
          fecha: fecha,
        ),
        throwsA(isA<StateError>()),
      );
    });
  });
}
