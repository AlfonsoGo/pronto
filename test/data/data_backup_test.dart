import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;
import 'package:pronto/src/data/data_backup.dart';

void main() {
  late Directory root;
  late Directory dataDir;
  late Directory backups;

  setUp(() {
    root = Directory.systemTemp.createTempSync('pronto-copia-');
    dataDir = Directory(p.join(root.path, 'Pronto'))
      ..createSync(recursive: true);
    backups = Directory(p.join(root.path, 'copias'));
  });

  tearDown(() {
    if (root.existsSync()) root.deleteSync(recursive: true);
  });

  /// Deja la carpeta de datos como la de un usuario de verdad: la base, los
  /// ajustes y los 640 MB de modelo (aquí, cuatro ficheros de mentira).
  void userDataFolder() {
    File(p.join(dataDir.path, 'pronto.db')).writeAsStringSync('base con datos');
    File(p.join(dataDir.path, 'shared_preferences.json'))
        .writeAsStringSync('{"app_settings":"{}"}');
    final model = Directory(p.join(dataDir.path, 'models', 'parakeet'))
      ..createSync(recursive: true);
    for (final f in [
      'encoder.int8.onnx',
      'decoder.int8.onnx',
      'joiner.int8.onnx',
      'tokens.txt',
    ]) {
      File(p.join(model.path, f)).writeAsStringSync('x' * 1000);
    }
  }

  final date = DateTime(2026, 9, 6, 14, 32);

  List<String> backupNames() => backups.existsSync()
      ? backups.listSync().map((e) => p.basename(e.path)).toList()
      : <String>[];

  test('copia los datos del usuario con su contenido intacto', () async {
    userDataFolder();

    final path = await backUpDataFolder(
      source: dataDir,
      backupsRoot: backups,
      date: date,
    );

    expect(
      File(p.join(path, 'pronto.db')).readAsStringSync(),
      'base con datos',
    );
    expect(
      File(p.join(path, 'shared_preferences.json')).readAsStringSync(),
      '{"app_settings":"{}"}',
    );
  });

  test('NO copia models/: son 640 MB que se vuelven a descargar', () async {
    userDataFolder();

    final path = await backUpDataFolder(
      source: dataDir,
      backupsRoot: backups,
      date: date,
    );

    expect(Directory(p.join(path, 'models')).existsSync(), isFalse);
    // Y sobre todo: el modelo sigue donde estaba. Actualizar no puede acabar
    // en una redescarga de 640 MB.
    for (final f in [
      'encoder.int8.onnx',
      'decoder.int8.onnx',
      'joiner.int8.onnx',
      'tokens.txt',
    ]) {
      expect(
        File(p.join(dataDir.path, 'models', 'parakeet', f)).existsSync(),
        isTrue,
        reason: 'la copia se ha llevado por delante $f',
      );
    }
  });

  test('el nombre de la copia lleva la fecha', () async {
    userDataFolder();

    final path = await backUpDataFolder(
      source: dataDir,
      backupsRoot: backups,
      date: date,
    );

    expect(p.basename(path), 'copia-2026-09-06-1432');
  });

  test('dos copias en el mismo minuto no se pisan', () async {
    userDataFolder();

    final a = await backUpDataFolder(
      source: dataDir,
      backupsRoot: backups,
      date: date,
    );
    File(p.join(dataDir.path, 'pronto.db'))
        .writeAsStringSync('segunda versión');
    final b = await backUpDataFolder(
      source: dataDir,
      backupsRoot: backups,
      date: date,
    );

    expect(a, isNot(b));
    expect(File(p.join(a, 'pronto.db')).readAsStringSync(), 'base con datos');
    expect(File(p.join(b, 'pronto.db')).readAsStringSync(), 'segunda versión');
  });

  test('copia las subcarpetas que no están excluidas', () async {
    userDataFolder();
    Directory(p.join(dataDir.path, 'logs')).createSync();
    File(p.join(dataDir.path, 'logs', 'ayer.txt')).writeAsStringSync('hola');

    final path = await backUpDataFolder(
      source: dataDir,
      backupsRoot: backups,
      date: date,
    );

    expect(File(p.join(path, 'logs', 'ayer.txt')).readAsStringSync(), 'hola');
  });

  group('rotura', () {
    test('nombres con tildes, ñ y emoji sobreviven a la copia', () async {
      File(p.join(dataDir.path, 'añoveros — copia (1) 🎤.txt'))
          .writeAsStringSync('ñandú');

      final path = await backUpDataFolder(
        source: dataDir,
        backupsRoot: backups,
        date: date,
      );

      expect(
        File(p.join(path, 'añoveros — copia (1) 🎤.txt')).readAsStringSync(),
        'ñandú',
      );
    });

    test('una carpeta de datos vacía se copia sin quejarse', () async {
      final path = await backUpDataFolder(
        source: dataDir,
        backupsRoot: backups,
        date: date,
      );

      expect(Directory(path).existsSync(), isTrue);
      expect(Directory(path).listSync(), isEmpty);
    });

    test('una copia interrumpida no queda con nombre de copia buena', () async {
      userDataFolder();
      var copied = 0;

      await expectLater(
        backUpDataFolder(
          source: dataDir,
          backupsRoot: backups,
          date: date,
          copyFile: (f, target) async {
            if (copied++ >= 1) throw const FileSystemException('disco lleno');
            await f.copy(target);
          },
        ),
        throwsA(isA<FileSystemException>()),
      );

      // Ni copia buena ni restos a medias: quien mire la carpeta ve que NO
      // tiene copia, que es la verdad.
      expect(backupNames(), isEmpty);
    });

    test('mientras se copia, la carpeta con nombre bueno NO existe todavía',
        () async {
      // Si se copiara directamente sobre el nombre final, matar el proceso a
      // mitad (que no lanza excepción y no limpia nada) dejaría media copia
      // con pinta de copia entera. Por eso se escribe en `.a-medias` y solo se
      // renombra al terminar.
      userDataFolder();
      final good = p.join(backups.path, 'copia-2026-09-06-1432');
      var seen = 0;

      await backUpDataFolder(
        source: dataDir,
        backupsRoot: backups,
        date: date,
        copyFile: (f, target) async {
          seen++;
          expect(
            Directory(good).existsSync(),
            isFalse,
            reason: 'a media copia ya había una carpeta con nombre de copia '
                'buena',
          );
          await f.copy(target);
        },
      );

      expect(seen, greaterThan(0));
      expect(Directory(good).existsSync(), isTrue);
    });

    test('los restos de una copia que murió a medias se barren', () async {
      // Matar el proceso NO lanza ninguna excepción, así que nadie limpia. Sin
      // este barrido esos restos se quedan para siempre y encima empujan el
      // nombre de las copias buenas a `-2`, `-3`, `-4`…
      userDataFolder();
      final leftovers = Directory(
        p.join(backups.path, 'copia-2026-09-05-1000$kPartialSuffix'),
      )..createSync(recursive: true);
      File(p.join(leftovers.path, 'medio.db')).writeAsStringSync('a medias');

      final path = await backUpDataFolder(
        source: dataDir,
        backupsRoot: backups,
        date: date,
      );

      expect(leftovers.existsSync(), isFalse);
      // Y la copia de hoy se queda con su nombre limpio, sin sufijo numérico.
      expect(p.basename(path), 'copia-2026-09-06-1432');
      expect(backupNames(), ['copia-2026-09-06-1432']);
    });

    test('una copia buena de otro día NO se toca al hacer la de hoy', () async {
      userDataFolder();
      final old = Directory(p.join(backups.path, 'copia-2026-09-05-1000'))
        ..createSync(recursive: true);
      File(p.join(old.path, 'pronto.db')).writeAsStringSync('la de ayer');

      await backUpDataFolder(
        source: dataDir,
        backupsRoot: backups,
        date: date,
      );

      expect(
        File(p.join(old.path, 'pronto.db')).readAsStringSync(),
        'la de ayer',
      );
      expect(backupNames().length, 2);
    });

    test('una carpeta de datos que no existe se dice, no se inventa', () async {
      await expectLater(
        backUpDataFolder(
          source: Directory(p.join(root.path, 'no-existe')),
          backupsRoot: backups,
          date: date,
        ),
        throwsA(isA<StateError>()),
      );
    });

    test('las copias no pueden vivir dentro de la carpeta de datos', () async {
      userDataFolder();

      await expectLater(
        backUpDataFolder(
          source: dataDir,
          backupsRoot: Directory(p.join(dataDir.path, 'copias')),
          date: date,
        ),
        throwsA(isA<StateError>()),
      );
    });
  });
}
