import 'package:flutter_test/flutter_test.dart';
import 'package:pronto/src/features/learning/learning_repository.dart';
import 'package:pronto/src/features/learning/learning_service.dart';

/// Repositorio en memoria para tests.
class FakeLearningRepository implements LearningRepository {
  final Map<String, ({String corrected, int freq})> corrections = {};
  final Map<String, int> vocab = {};
  final List<String> log = [];

  @override
  Future<void> bumpCorrection(String raw, String corrected) async {
    final key = '$raw=>$corrected';
    final cur = corrections[key];
    corrections[key] = (corrected: corrected, freq: (cur?.freq ?? 0) + 1);
  }

  @override
  Future<List<CorrectionEntry>> activeCorrections({int minFreq = 3}) async {
    return corrections.entries
        .where((e) => e.value.freq >= minFreq)
        .map((e) => CorrectionEntry(
              raw: e.key.split('=>').first,
              corrected: e.value.corrected,
              freq: e.value.freq,
            ),)
        .toList();
  }

  @override
  Future<void> bumpVocab(String term) async {
    vocab[term] = (vocab[term] ?? 0) + 1;
  }

  @override
  Future<List<String>> topVocab({int limit = 45}) async {
    final entries = vocab.entries.toList()
      ..sort((a, b) => b.value.compareTo(a.value));
    return entries.take(limit).map((e) => e.key).toList();
  }

  @override
  Future<void> logDictation({
    required String raw,
    required String finalText,
    required double avgLogProb,
  }) async {
    log.add('$raw | $finalText');
  }
}

void main() {
  group('LearningService', () {
    late FakeLearningRepository repo;
    late LearningService service;

    setUp(() {
      repo = FakeLearningRepository();
      service = LearningService(repo);
    });

    test('aprende un nombre propio tras repetir la corrección (freq>=3)',
        () async {
      for (var i = 0; i < 3; i++) {
        await service.recordEdit(
          'mi amigo wiroski vino hoy',
          'mi amigo Wirowski vino hoy',
        );
      }
      await service.refresh();

      expect(service.applyDictionary('saluda a wiroski'),
          'saluda a Wirowski',);
    });

    test('NO aplica corrección por debajo de la frecuencia mínima', () async {
      await service.recordEdit('hola wiroski', 'hola Wirowski');
      await service.refresh();
      // Solo 1 vez: no debe activarse.
      expect(service.applyDictionary('hola wiroski'), 'hola wiroski');
    });

    test('respeta el patrón de mayúsculas del texto destino', () async {
      for (var i = 0; i < 3; i++) {
        await service.recordEdit('uso supabasse', 'uso Supabase');
      }
      await service.refresh();
      expect(service.applyDictionary('SUPABASSE'), 'SUPABASE');
      expect(service.applyDictionary('supabasse'), 'Supabase');
    });

    test('ignora reescrituras semánticas (palabras muy distintas)', () async {
      for (var i = 0; i < 3; i++) {
        // "perro" -> "gato" no es la misma palabra mal oída.
        await service.recordEdit('tengo un perro', 'tengo un gato');
      }
      await service.refresh();
      expect(service.applyDictionary('tengo un perro'), 'tengo un perro');
    });

    test('ignora cambios solo de puntuación/mayúsculas (eso es del LLM)',
        () async {
      for (var i = 0; i < 3; i++) {
        await service.recordEdit('hola mundo', 'Hola, mundo.');
      }
      await service.refresh();
      // No debe haber creado correcciones de vocabulario.
      expect(service.applyDictionary('hola'), 'hola');
    });

    test('buildInitialPrompt incluye el vocabulario aprendido', () async {
      for (var i = 0; i < 3; i++) {
        await service.recordEdit('uso supabasse', 'uso Supabase');
      }
      await service.refresh();
      final prompt = service.buildInitialPrompt();
      expect(prompt.contains('Supabase'), isTrue);
    });
  });
}
