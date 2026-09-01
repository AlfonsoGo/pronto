import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:pronto/src/core/config.dart';
import 'package:pronto/src/features/learning/learning_service.dart';

import 'learning_service_test.dart' show FakeLearningRepository;

/// Biasing del motor: el vocabulario aprendido tiene que LLEGAR a sherpa-onnx.
///
/// Contexto de estas pruebas: hasta la 0.7.1 lo aprendido se entregaba a
/// `buildInitialPrompt()`, un parámetro de Whisper que Parakeet ignora. El
/// aprendizaje funcionaba y su salida se perdía. Lo que se prueba aquí es el
/// camino nuevo, y sobre todo que no vuelva a estrecharse sin que salte nada.
void main() {
  group('buildHotwords', () {
    late FakeLearningRepository repo;
    late LearningService service;

    setUp(() {
      repo = FakeLearningRepository();
      service = LearningService(repo);
    });

    Future<void> seed(Map<String, int> terms) async {
      for (final e in terms.entries) {
        for (var i = 0; i < e.value; i++) {
          await repo.bumpVocab(e.key);
        }
      }
      await service.refresh();
    }

    test('devuelve el vocabulario, del más frecuente al menos', () async {
      await seed({'Supabase': 9, 'Serviforn': 5, 'Perplexity': 2});

      expect(service.buildHotwords(), ['Supabase', 'Serviforn', 'Perplexity']);
    });

    test('descarta términos por debajo del mínimo de longitud', () async {
      await seed({'IA': 9, 'CRM': 8});

      // 'IA' tiene 2 caracteres: por debajo del mínimo, dispara falsos
      // positivos en mitad de cualquier palabra.
      expect(service.buildHotwords(), ['CRM']);
    });

    test('descarta términos con caracteres que no se tokenizan', () async {
      await seed({'Supabase': 5, r'$$$': 4, 'rag test': 3});

      expect(service.buildHotwords(), ['Supabase', 'rag test']);
    });

    test('deduplica sin distinguir mayúsculas y conserva la primera', () async {
      await seed({'Claude': 9, 'claude': 4});

      expect(service.buildHotwords(), ['Claude']);
    });

    // ESTA es la prueba que importa. El tope viejo (initialPromptMaxTerms = 45)
    // venía de la ventana de contexto de Whisper, que los hotwords no tienen.
    // Si alguien lo reintroduce, el biasing se queda ciego EN SILENCIO: la BD
    // sigue llena de vocabulario y el motor solo ve los 45 primeros.
    test('no se corta en el tope viejo del initial_prompt', () async {
      final muchos = <String, int>{};
      for (var i = 0; i < 120; i++) {
        // Frecuencia descendente para que el orden sea determinista.
        muchos['termino${i.toString().padLeft(3, '0')}'] = 200 - i;
      }
      await seed(muchos);

      final hotwords = service.buildHotwords();

      expect(hotwords.length, 120);
      expect(hotwords.length, greaterThan(AppConfig.initialPromptMaxTerms));
      expect(hotwords.last, 'termino119');
    });

    test('respeta el tope propio de hotwords', () async {
      final muchos = <String, int>{};
      for (var i = 0; i < AppConfig.hotwordsMaxTerms + 50; i++) {
        muchos['termino${i.toString().padLeft(4, '0')}'] = 10000 - i;
      }
      await seed(muchos);

      expect(service.buildHotwords().length, AppConfig.hotwordsMaxTerms);
    });
  });

  group('writeHotwordsFile', () {
    late FakeLearningRepository repo;
    late LearningService service;
    late Directory dir;

    setUp(() async {
      repo = FakeLearningRepository();
      service = LearningService(repo);
      dir = await Directory.systemTemp.createTemp('pronto_hotwords_');
    });

    tearDown(() async {
      if (dir.existsSync()) await dir.delete(recursive: true);
    });

    File hotwordsFile() =>
        File('${dir.path}${Platform.pathSeparator}${AppConfig.hotwordsFile}');

    test('escribe un término por línea', () async {
      await repo.bumpVocab('Supabase');
      await repo.bumpVocab('Serviforn');
      await service.refresh();

      final written = await service.writeHotwordsFile(dir.path);

      expect(written, isTrue);
      final lines = hotwordsFile()
          .readAsLinesSync()
          .where((l) => l.trim().isNotEmpty)
          .toList();
      expect(lines, containsAll(['Supabase', 'Serviforn']));
    });

    // Un fichero vacío haría que el motor arrancase en modified_beam_search
    // (más lento) sin nada que sesgar: se paga el peaje y no se compra nada.
    test('borra el fichero cuando no hay vocabulario', () async {
      hotwordsFile().writeAsStringSync('sobra\n');
      await service.refresh();

      final written = await service.writeHotwordsFile(dir.path);

      expect(written, isFalse);
      expect(hotwordsFile().existsSync(), isFalse);
    });

    test('una carpeta inexistente no tumba el dictado', () async {
      await repo.bumpVocab('Supabase');
      await service.refresh();

      final written = await service.writeHotwordsFile(
        '${dir.path}${Platform.pathSeparator}no_existe',
      );

      expect(written, isFalse);
    });
  });
}
