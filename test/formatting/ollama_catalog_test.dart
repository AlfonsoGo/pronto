// ignore_for_file: require_trailing_commas
import 'dart:convert';
import 'dart:math';

import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:pronto/src/features/formatting/modelos_locales.dart';
import 'package:pronto/src/features/formatting/ollama_catalog.dart';

/// Una entrada de `/api/tags` con la forma REAL que devuelve Ollama.
///
/// Copiada de una respuesta de verdad (Ollama 0.12, 6-sep-2026): `capabilities`
/// es un campo relativamente nuevo, y la mitad de las trampas de este fichero
/// están en que puede no venir.
Map<String, Object?> _entrada(
  String nombre, {
  int size = 1000 * 1000 * 1000,
  String? parametros,
  List<String>? capacidades,
  String familia = 'qwen2',
}) {
  return {
    'name': nombre,
    'model': nombre,
    'size': size,
    'details': {
      'family': familia,
      'parameter_size': parametros,
      'quantization_level': 'Q4_K_M',
    },
    if (capacidades != null) 'capabilities': capacidades,
  };
}

String _tags(List<Map<String, Object?>> modelos) =>
    jsonEncode({'models': modelos});

const int _gb = 1024 * 1024 * 1024;

void main() {
  group('OllamaModel — leer lo que devuelve la máquina', () {
    test('lee nombre, tamaño y detalles', () {
      final m = OllamaModel.fromJson(
        _entrada('qwen2.5:7b', size: 4683087332, parametros: '7.6B'),
      )!;
      expect(m.tag, 'qwen2.5:7b');
      expect(m.sizeBytes, 4683087332);
      expect(m.parametros, '7.6B');
      expect(m.cuantizacion, 'Q4_K_M');
    });

    test('una entrada sin nombre se descarta, no tumba la pantalla', () {
      expect(OllamaModel.fromJson({'size': 12}), isNull);
      expect(OllamaModel.fromJson('vaya'), isNull);
      expect(OllamaModel.fromJson(null), isNull);
    });

    test('un modelo de embeddings NO sirve para formatear', () {
      final m = OllamaModel.fromJson(
        _entrada('nomic-embed-text', capacidades: ['embedding']),
      )!;
      expect(m.puedeFormatear, isFalse);
    });

    test('sin capacidades declaradas se asume que SÍ vale', () {
      // Ollama viejo no informa `capabilities`. Descartarlo todo dejaría al
      // usuario mirando una lista vacía teniendo modelos instalados.
      final m = OllamaModel.fromJson(_entrada('qwen2.5:7b'))!;
      expect(m.capacidades, isEmpty);
      expect(m.puedeFormatear, isTrue);
    });

    test('un modelo raro nuestro NO se juzga: entra en la lista', () {
      // No sabemos si un modelo de visión formatea bien o mal. Sacarlo de la
      // lista sería inventarse un juicio que no hemos medido; el usuario lo
      // eligió y puede volver a elegirlo.
      final m = OllamaModel.fromJson(
        _entrada('llava:7b', capacidades: ['completion', 'vision']),
      )!;
      expect(m.puedeFormatear, isTrue);
    });

    test('«qwen3» y «qwen3:latest» son el MISMO modelo', () {
      final m = OllamaModel.fromJson(_entrada('qwen3:latest'))!;
      expect(m.coincideCon('qwen3'), isTrue);
      expect(m.coincideCon('qwen3:latest'), isTrue);
      expect(m.coincideCon('qwen3:4b'), isFalse);
    });
  });

  group('OllamaCatalog.de — filtrar y ordenar', () {
    test('sin modelos utilizables: lo dice con texto NUESTRO', () {
      final c = OllamaCatalog.de([
        OllamaModel.fromJson(
          _entrada('nomic-embed-text', capacidades: ['embedding']),
        )!,
      ]);
      expect(c.reach, OllamaReach.sinModelos);
      expect(c.notice, contains('ollama pull'));
      expect(c.modelos, isEmpty);
    });

    test('un modelo MEDIDO va delante aunque pese mucho más', () {
      // El orden no es una opinión sobre qué modelo es mejor: delante lo que
      // hemos medido nosotros, detrás lo demás por tamaño. Sin este test, dar
      // la vuelta a la preferencia no rompía nada (mutante superviviente).
      final medido = kModelosMedidos.first.tag;
      final c = OllamaCatalog.de([
        OllamaModel.fromJson(_entrada('plumita:1b', size: 1 * _gb))!,
        OllamaModel.fromJson(_entrada(medido, size: 5 * _gb))!,
      ], memoriaBytes: 64 * _gb);
      expect(c.modelos.map((m) => m.tag), [medido, 'plumita:1b']);
      expect(c.propuesto!.medido, isNotNull);
    });

    test('el más ligero primero cuando ninguno está medido', () {
      final c = OllamaCatalog.de([
        OllamaModel.fromJson(_entrada('grande', size: 20 * _gb))!,
        OllamaModel.fromJson(_entrada('pequeño', size: 1 * _gb))!,
        OllamaModel.fromJson(_entrada('mediano', size: 5 * _gb))!,
      ], memoriaBytes: 64 * _gb);
      expect(c.modelos.map((m) => m.tag), ['pequeño', 'mediano', 'grande']);
      expect(c.propuesto!.tag, 'pequeño');
    });

    test('lo que no cabe en memoria NO se propone, pero se dice que está', () {
      // Un modelo que desaparece de la lista sin explicación parece una
      // avería: el usuario sabe que lo tiene instalado.
      final c = OllamaCatalog.de([
        OllamaModel.fromJson(_entrada('enorme', size: 23 * _gb))!,
        OllamaModel.fromJson(_entrada('cabe', size: 1 * _gb))!,
      ], memoriaBytes: 8 * _gb);
      expect(c.modelos.map((m) => m.tag), ['cabe']);
      expect(c.noCaben.map((m) => m.tag), ['enorme']);
      expect(c.reach, OllamaReach.disponible);
    });

    test('si NADA cabe, se dice y se remite a la nube o a no formatear', () {
      final c = OllamaCatalog.de([
        OllamaModel.fromJson(_entrada('enorme', size: 23 * _gb))!,
      ], memoriaBytes: 8 * _gb);
      expect(c.reach, OllamaReach.sinModelos);
      expect(c.notice, contains('memoria'));
      expect(c.propuesto, isNull);
    });

    test('sin saber la memoria NO se filtra nada', () {
      // El `null` de [SystemMemory] es «no se pudo leer». Inventarse una
      // memoria para poder decir «no te cabe» sería peor que no decirlo.
      final c = OllamaCatalog.de([
        OllamaModel.fromJson(_entrada('enorme', size: 400 * _gb))!,
      ]);
      expect(c.reach, OllamaReach.disponible);
      expect(c.noCaben, isEmpty);
    });

    test('en la frontera EXACTA de la memoria, el modelo SÍ cabe', () {
      // El filtro es «>», no «>=». Con «>=», un equipo con exactamente la
      // memoria que pide su modelo se quedaría sin ninguno que proponer.
      final justo = OllamaModel.fromJson(_entrada('justo', size: 3 * _gb))!;
      expect(justo.memoriaNecesariaBytes, 4 * _gb);
      final c = OllamaCatalog.de([justo], memoriaBytes: 4 * _gb);
      expect(c.modelos.map((m) => m.tag), ['justo']);
      expect(c.noCaben, isEmpty);

      // Y un byte por debajo, ya no cabe.
      final c2 = OllamaCatalog.de([justo], memoriaBytes: 4 * _gb - 1);
      expect(c2.noCaben.map((m) => m.tag), ['justo']);
    });

    test('el margen de memoria es de 1 GB por encima del propio modelo', () {
      final m = OllamaModel.fromJson(_entrada('x', size: 4 * _gb))!;
      expect(m.memoriaNecesariaBytes, 5 * _gb);
    });

    test('tiene() encuentra tanto lo que cabe como lo que no', () {
      final c = OllamaCatalog.de([
        OllamaModel.fromJson(_entrada('enorme', size: 23 * _gb))!,
        OllamaModel.fromJson(_entrada('cabe:latest', size: 1 * _gb))!,
      ], memoriaBytes: 8 * _gb);
      expect(c.tiene('enorme'), isTrue);
      expect(c.tiene('cabe'), isTrue, reason: 'el :latest implícito');
      expect(c.tiene('inexistente'), isFalse);
    });
  });

  group('OllamaClient — preguntar a la máquina sin romperse nunca', () {
    Future<OllamaCatalog> conRespuesta(
      Future<http.Response> Function(http.Request) responder, {
      int? memoria,
    }) {
      final c = OllamaClient(
        httpClient: MockClient(responder),
        timeout: const Duration(milliseconds: 80),
      );
      return c.catalogo('http://localhost:11434', memoriaBytes: memoria);
    }

    test('pregunta a /api/tags, no a otro sitio', () async {
      late Uri visitada;
      await conRespuesta((req) async {
        visitada = req.url;
        return http.Response(_tags([]), 200);
      });
      expect(visitada.path, '/api/tags');
    });

    test('la barra final de la URL no rompe la ruta', () async {
      late Uri visitada;
      final c = OllamaClient(
        httpClient: MockClient((req) async {
          visitada = req.url;
          return http.Response(_tags([]), 200);
        }),
      );
      await c.catalogo('http://localhost:11434///');
      expect(visitada.toString(), 'http://localhost:11434/api/tags');
    });

    test('devuelve los modelos que la máquina dice tener', () async {
      final cat = await conRespuesta(
        (_) async => http.Response(
          _tags([
            _entrada('qwen2.5:7b', size: 4 * _gb, parametros: '7.6B'),
            _entrada('qwen3:1.7b', size: 1 * _gb, parametros: '1.7B'),
          ]),
          200,
        ),
        memoria: 32 * _gb,
      );
      expect(cat.reach, OllamaReach.disponible);
      expect(
          cat.modelos.map((m) => m.tag),
          containsAll(<String>[
            'qwen2.5:7b',
            'qwen3:1.7b',
          ]));
    });

    test('servidor caído: lo dice con texto nuestro y ofrece salidas',
        () async {
      final cat = await conRespuesta(
        (_) async => throw const SocketExceptionFalsa(),
      );
      expect(cat.reach, OllamaReach.noResponde);
      expect(cat.modelos, isEmpty);
      // Nunca se enseña la excepción cruda, y se ofrecen los otros caminos.
      expect(cat.notice, contains('http://localhost:11434'));
      expect(cat.notice, contains('sin Formateo'));
      expect(cat.notice, contains('nube'));
      expect(cat.notice, isNot(contains('Exception')));
    });

    test('tiempo agotado: tampoco lanza', () async {
      final cat = await conRespuesta((_) async {
        await Future<void>.delayed(const Duration(milliseconds: 400));
        return http.Response(_tags([]), 200);
      });
      expect(cat.reach, OllamaReach.noResponde);
    });

    test('un 500 no es «no tienes modelos»: es «no responde»', () async {
      final cat = await conRespuesta((_) async => http.Response('boom', 500));
      expect(cat.reach, OllamaReach.noResponde);
    });

    test('JSON roto: no lanza', () async {
      final cat = await conRespuesta((_) async => http.Response('{{{', 200));
      expect(cat.reach, OllamaReach.noResponde);
    });

    test('algo que responde 200 pero NO es Ollama: se dice que es la URL',
        () async {
      final cat = await conRespuesta(
        (_) async => http.Response('{"hola":"mundo"}', 200),
      );
      expect(cat.reach, OllamaReach.noResponde);
      expect(cat.notice, contains('no es Ollama'));
    });

    test(
        'una entrada rota entre varias buenas no se lleva la lista por delante',
        () async {
      final cat = await conRespuesta(
        (_) async => http.Response(
          jsonEncode({
            'models': [
              {'size': 10},
              _entrada('bueno:1b', size: 1 * _gb),
              'basura',
            ],
          }),
          200,
        ),
      );
      expect(cat.modelos.map((m) => m.tag), ['bueno:1b']);
    });
  });

  // ─── Nivel 4: invariantes, con listas que nadie escribió a mano ────────────
  group('invariantes de OllamaCatalog.de', () {
    // Sin generador de propiedades en Dart, la semilla se fija a mano: así un
    // fallo se reproduce exactamente en vez de aparecer un martes.
    final rnd = Random(20260906);

    /// Tamaños en MEGAS y luego a bytes: `nextInt` no llega a 2^32, y un modelo
    /// de 40 GB en bytes se sale de su rango.
    int bytesAlAzar(int maxGb) => (1 + rnd.nextInt(maxGb * 1024)) * 1024 * 1024;

    List<OllamaModel> listaAlAzar(int n) => [
          for (var i = 0; i < n; i++)
            OllamaModel.fromJson(
              _entrada(
                'm$i:${rnd.nextInt(99)}b',
                size: bytesAlAzar(40),
                capacidades: rnd.nextBool()
                    ? ['completion']
                    : (rnd.nextBool() ? ['embedding'] : null),
              ),
            )!,
        ];

    test('lo que se propone SIEMPRE cabe en la memoria del equipo', () {
      for (var caso = 0; caso < 300; caso++) {
        final memoria = bytesAlAzar(64);
        final c = OllamaCatalog.de(
          listaAlAzar(rnd.nextInt(8)),
          memoriaBytes: memoria,
        );
        for (final m in c.modelos) {
          expect(m.memoriaNecesariaBytes, lessThanOrEqualTo(memoria),
              reason: 'propuesto ${m.tag} con $memoria de memoria');
        }
      }
    });

    test('no se pierde ningún modelo utilizable por el camino', () {
      // O se propone, o se dice que no cabe. Desaparecer no es una opción.
      for (var caso = 0; caso < 300; caso++) {
        final crudos = listaAlAzar(rnd.nextInt(8));
        final c = OllamaCatalog.de(crudos, memoriaBytes: bytesAlAzar(50));
        final utiles = crudos.where((m) => m.puedeFormatear).map((m) => m.tag);
        final vistos = [...c.modelos, ...c.noCaben].map((m) => m.tag);
        expect(vistos.toSet(), utiles.toSet());
      }
    });

    test('un modelo sin medir NUNCA adelanta a uno medido', () {
      for (var caso = 0; caso < 200; caso++) {
        final c = OllamaCatalog.de(listaAlAzar(rnd.nextInt(8)),
            memoriaBytes: 64 * _gb);
        var visoSinMedir = false;
        for (final m in c.modelos) {
          if (m.medido == null) {
            visoSinMedir = true;
          } else {
            expect(visoSinMedir, isFalse,
                reason: '${m.tag} está medido y va detrás de uno que no');
          }
        }
      }
    });

    test('el orden de llegada NO cambia el resultado', () {
      // Ollama no promete ningún orden en `/api/tags`. Si el nuestro dependiera
      // de él, la lista bailaría entre dos aperturas de la pantalla.
      for (var caso = 0; caso < 200; caso++) {
        final crudos = listaAlAzar(2 + rnd.nextInt(6));
        final alDerecho = OllamaCatalog.de(crudos, memoriaBytes: 64 * _gb);
        final delReves = OllamaCatalog.de(
          crudos.reversed.toList(),
          memoriaBytes: 64 * _gb,
        );
        expect(
          delReves.modelos.map((m) => m.tag).toList(),
          alDerecho.modelos.map((m) => m.tag).toList(),
        );
      }
    });
  });

  // ─── Nivel 5: rotura ──────────────────────────────────────────────────────
  group('rotura', () {
    test('N=0: ninguna entrada', () {
      final c = OllamaCatalog.de([], memoriaBytes: 8 * _gb);
      expect(c.reach, OllamaReach.sinModelos);
      expect(c.propuesto, isNull);
    });

    test('N=1: el único modelo se propone', () {
      final c = OllamaCatalog.de([
        OllamaModel.fromJson(_entrada('solo:1b', size: _gb))!,
      ], memoriaBytes: 8 * _gb);
      expect(c.propuesto!.tag, 'solo:1b');
    });

    test('N enorme: 500 modelos no rompen ni reordenan mal', () {
      final muchos = [
        for (var i = 0; i < 500; i++)
          OllamaModel.fromJson(_entrada('m$i', size: (500 - i) * 1000000))!,
      ];
      final c = OllamaCatalog.de(muchos, memoriaBytes: 64 * _gb);
      expect(c.modelos.length, 500);
      expect(c.modelos.first.tag, 'm499', reason: 'el más ligero primero');
    });

    test('nombres con unicode, emoji y acentos se respetan tal cual', () {
      const raro = 'ñandú-modelo_ÉÑ:última🐍';
      final c = OllamaCatalog.de([
        OllamaModel.fromJson(_entrada(raro, size: _gb))!,
      ], memoriaBytes: 8 * _gb);
      expect(c.propuesto!.tag, raro);
      expect(c.tiene(raro), isTrue);
    });

    test('un nombre larguísimo no tumba nada', () {
      final largo = 'x' * 10000;
      final c = OllamaCatalog.de([
        OllamaModel.fromJson(_entrada(largo, size: _gb))!,
      ], memoriaBytes: 8 * _gb);
      expect(c.propuesto!.tag.length, 10000);
    });

    test('tamaño ausente o absurdo se trata como cero, no como null', () {
      final m = OllamaModel.fromJson({'name': 'x', 'size': 'mucho'})!;
      expect(m.sizeBytes, 0);
      // Con tamaño desconocido, lo único honesto es no descartarlo: es el
      // margen de 1 GB el que decide.
      expect(m.memoriaNecesariaBytes, 1 * _gb);
    });

    test('el nombre se recorta: « qwen3 » es qwen3', () {
      final m = OllamaModel.fromJson({'name': '  qwen3:4b  '})!;
      expect(m.tag, 'qwen3:4b');
    });

    test('un modelo con capabilities vacía se trata como desconocido', () {
      final m = OllamaModel.fromJson(_entrada('x', capacidades: []))!;
      expect(m.puedeFormatear, isTrue);
    });
  });
}


/// Un fallo de red cualquiera. No se usa `SocketException` de `dart:io` para
/// que este fichero siga siendo un test puro.
class SocketExceptionFalsa implements Exception {
  const SocketExceptionFalsa();
}