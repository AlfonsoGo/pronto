import 'dart:async';
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:path/path.dart' as p;
import 'package:pronto/src/core/config.dart';
import 'package:pronto/src/features/splash/parakeet_model_downloader.dart';
import 'package:pronto/src/platform/platform_services.dart';

// El criterio de aceptación del PRNT-13 sobre la descarga:
// «Cancelar una descarga a mitad **no deja el modelo a medias**: al reintentar,
// baja entero.»
//
// Sin este test, «se puede cancelar» sería una afirmación: el botón existiría y
// nadie habría comprobado nunca qué queda en disco después de pulsarlo.

/// Servidor de mentira. Sirve cuerpos pequeños y avisa en cada trozo, para que
/// el test pueda cancelar EN MITAD y no antes ni después.
class _ServidorFalso extends http.BaseClient {
  /// Se llama tras entregar cada trozo, con cuántos van.
  final void Function(int trozo)? alEntregarTrozo;

  /// Cuántas veces se ha pedido cada fichero.
  final Map<String, int> peticiones = {};

  /// Cabeceras de la última petición, para comprobar que nadie intenta
  /// continuar por donde iba (`Range`).
  final List<Map<String, String>> cabeceras = [];

  bool cerrado = false;

  _ServidorFalso({this.alEntregarTrozo});

  static const int trozos = 4;
  static const int bytesPorTrozo = 8;
  static const int tamano = trozos * bytesPorTrozo;

  @override
  Future<http.StreamedResponse> send(http.BaseRequest request) async {
    final fichero = p.basename(request.url.path);
    peticiones[fichero] = (peticiones[fichero] ?? 0) + 1;
    cabeceras.add(Map.of(request.headers));

    if (request.method == 'HEAD') {
      return http.StreamedResponse(
        const Stream<List<int>>.empty(),
        200,
        contentLength: tamano,
        request: request,
      );
    }

    var entregados = 0;
    final stream = Stream<List<int>>.fromIterable(
      List.generate(
        trozos,
        (_) => Uint8List(bytesPorTrozo),
      ),
    ).asyncMap((trozo) async {
      // Un respiro entre trozos: es lo que deja que un `cancel()` del test se
      // vea entre uno y otro, igual que lo vería el clic de un usuario.
      await Future<void>.delayed(Duration.zero);
      entregados++;
      alEntregarTrozo?.call(entregados);
      return trozo;
    });

    return http.StreamedResponse(
      stream,
      200,
      contentLength: tamano,
      request: request,
    );
  }

  @override
  void close() => cerrado = true;
}

void main() {
  late Directory tmp;

  setUp(() {
    tmp = Directory.systemTemp.createTempSync('pronto_descarga_');
  });

  tearDown(() {
    if (tmp.existsSync()) tmp.deleteSync(recursive: true);
  });

  Directory carpetaModelo() => Directory(
        p.join(tmp.path, 'models', AppConfig.parakeetModelDir),
      );

  ProviderContainer montar(_ServidorFalso servidor) {
    final container = ProviderContainer(
      overrides: [
        appSupportDirProvider.overrideWith((ref) async => tmp),
        downloadClientProvider.overrideWithValue(() => servidor),
      ],
    );
    addTearDown(container.dispose);
    return container;
  }

  List<String> ficherosEn(Directory dir) {
    if (!dir.existsSync()) return const [];
    return dir
        .listSync()
        .whereType<File>()
        .map((f) => p.basename(f.path))
        .toList()
      ..sort();
  }

  test('cancelar a mitad: ni modelo a medias ni ficheros .part', () async {
    late ProviderContainer container;
    final servidor = _ServidorFalso(
      alEntregarTrozo: (n) {
        // A mitad del primer fichero, no al principio ni al final.
        if (n == 2) container.read(parakeetDownloaderProvider.notifier).cancel();
      },
    );
    container = montar(servidor);

    await container.read(parakeetDownloaderProvider.notifier).ensureModel();

    final estado = container.read(parakeetDownloaderProvider);
    expect(estado.phase, DownloadPhase.cancelado);
    // Cancelar NO es un error: enseñarlo como avería haría creer que la
    // descarga se rompió sola.
    expect(estado.error, isNot(contains('No se pudo descargar')));
    expect(ficherosEn(carpetaModelo()), isEmpty);
  });

  test('el progreso vuelve a cero: no se enseña lo que ya no está', () async {
    late ProviderContainer container;
    final servidor = _ServidorFalso(
      alEntregarTrozo: (n) {
        if (n == 2) container.read(parakeetDownloaderProvider.notifier).cancel();
      },
    );
    container = montar(servidor);
    await container.read(parakeetDownloaderProvider.notifier).ensureModel();

    final estado = container.read(parakeetDownloaderProvider);
    expect(estado.receivedBytes, 0);
    expect(estado.progress, 0);
    expect(estado.currentFile, isNull);
  });

  test('al reintentar, el fichero se pide ENTERO otra vez', () async {
    late ProviderContainer container;
    var cancelado = false;
    final servidor = _ServidorFalso(
      alEntregarTrozo: (n) {
        if (n == 2 && !cancelado) {
          cancelado = true;
          container.read(parakeetDownloaderProvider.notifier).cancel();
        }
      },
    );
    container = montar(servidor);

    final notifier = container.read(parakeetDownloaderProvider.notifier);
    await notifier.ensureModel();
    servidor.cabeceras.clear();
    await notifier.ensureModel();

    // Nadie pide un rango: no se continúa un flujo que nadie verificó.
    for (final h in servidor.cabeceras) {
      expect(h.keys.map((k) => k.toLowerCase()), isNot(contains('range')));
    }
    // Y el primer fichero se volvió a pedir de cero (HEAD + GET las dos veces).
    expect(servidor.peticiones[AppConfig.parakeetModelFiles.first], 4);
  });

  test('cancelar sin descarga en curso no hace nada', () async {
    final container = montar(_ServidorFalso());
    container.read(parakeetDownloaderProvider.notifier).cancel();
    expect(
      container.read(parakeetDownloaderProvider).phase,
      DownloadPhase.idle,
    );
  });

  test('integridad: un fichero con el SHA-256 que no toca NO se promueve',
      () async {
    // El servidor falso devuelve ceros, que jamás casarán con el hash real del
    // modelo. Lo que se comprueba es que el `.part` malo se borra en vez de
    // quedarse esperando a que alguien lo renombre.
    final container = montar(_ServidorFalso());
    await container.read(parakeetDownloaderProvider.notifier).ensureModel();

    expect(
      container.read(parakeetDownloaderProvider).phase,
      DownloadPhase.error,
    );
    expect(ficherosEn(carpetaModelo()), isEmpty);
  });

  test('borrar el modelo descargado deja la carpeta limpia', () async {
    carpetaModelo().createSync(recursive: true);
    File(p.join(carpetaModelo().path, 'tokens.txt')).writeAsStringSync('x');

    final container = montar(_ServidorFalso());
    final borrado = await container
        .read(parakeetDownloaderProvider.notifier)
        .deleteDownloadedModel();

    expect(borrado, isTrue);
    expect(carpetaModelo().existsSync(), isFalse);
  });

  test('borrar cuando no hay nada descargado no es un error', () async {
    final container = montar(_ServidorFalso());
    final borrado = await container
        .read(parakeetDownloaderProvider.notifier)
        .deleteDownloadedModel();
    expect(borrado, isFalse);
    expect(
      container.read(parakeetDownloaderProvider).phase,
      DownloadPhase.idle,
    );
  });
}
