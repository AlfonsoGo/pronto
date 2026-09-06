import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:pronto/src/features/history/dictation_entry.dart';
import 'package:pronto/src/features/history/history_controller.dart';
import 'package:pronto/src/features/history/history_repository.dart';
import 'package:pronto/src/features/learning/learning_repository.dart';
import 'package:pronto/src/platform/platform_services.dart';

/// Aprendizaje en memoria: solo interesa QUÉ se le pide aprender.
class _FakeLearning implements LearningRepository {
  final List<String> corrections = [];
  final List<String> vocab = [];

  @override
  Future<void> bumpCorrection(String raw, String corrected) async =>
      corrections.add('$raw -> $corrected');

  @override
  Future<List<CorrectionEntry>> activeCorrections({int minFreq = 3}) async =>
      const [];

  @override
  Future<void> bumpVocab(String term) async => vocab.add(term);

  @override
  Future<List<String>> topVocab({int limit = 45}) async => const [];
}

class _FakeHistory implements HistoryRepository {
  _FakeHistory(this.entries);

  final List<DictationEntry> entries;
  final Map<int, String> updated = {};

  @override
  Future<void> updateFinalText(int id, String text) async =>
      updated[id] = text;

  @override
  Future<List<DictationEntry>> page(
    HistoryQuery query, {
    int limit = 50,
    int offset = 0,
  }) async =>
      entries.skip(offset).take(limit).toList();

  @override
  Future<int> count(HistoryQuery query) async => entries.length;

  @override
  Future<List<DictationEntry>> all(HistoryQuery query) async => entries;

  @override
  Future<HistoryStats> stats(HistoryQuery query) async => HistoryStats.empty;

  @override
  Future<List<String>> distinctModes() async => const [];

  @override
  Future<void> deleteEntry(int id) async =>
      entries.removeWhere((e) => e.id == id);

  @override
  Future<int> deleteAll() async {
    final n = entries.length;
    entries.clear();
    return n;
  }

  @override
  Future<int> purgeOlderThan(Duration age, {DateTime? now}) async => 0;

  @override
  Future<int> log(DictationRecord record) async => 1;
}

DictationEntry _entry({
  required String raw,
  required String finalText,
  bool translated = false,
}) =>
    DictationEntry(
      id: 7,
      at: DateTime(2026, 9, 6),
      raw: raw,
      finalText: finalText,
      translated: translated,
    );

void main() {
  late _FakeHistory history;
  late _FakeLearning learning;
  late ProviderContainer container;

  void montar(DictationEntry entry) {
    history = _FakeHistory([entry]);
    learning = _FakeLearning();
    container = ProviderContainer(
      overrides: [
        historyRepositoryProvider.overrideWithValue(history),
        learningRepositoryProvider.overrideWithValue(learning),
      ],
    );
    addTearDown(container.dispose);
  }

  test('corregir arregla la fila Y le enseña la corrección a Pronto', () async {
    // Se cambia UNA palabra de siete: el aprendizaje descarta las reescrituras
    // grandes (no son un mapa de correcciones fiable), así que una corrección
    // de prueba tiene que parecerse a una corrección de verdad.
    final e = _entry(
      raw: 'la reunion de mañana con serviform va bien',
      finalText: 'La reunion de mañana con serviform va bien.',
    );
    montar(e);

    await container
        .read(historyControllerProvider.notifier)
        .correct(e, 'La reunion de mañana con Serviforn va bien.');

    expect(history.updated[7], 'La reunion de mañana con Serviforn va bien.');
    expect(learning.corrections, contains('serviform -> Serviforn'));
  });

  test('de una TRADUCCIÓN no se aprende: envenenaría el diccionario', () async {
    // Lo dictado fue español y lo pegado inglés. Su diferencia no son
    // correcciones: es la traducción entera. Meterla en el diccionario haría
    // que Pronto empezara a sustituir palabras españolas por inglesas.
    final e = _entry(
      raw: 'mándame el presupuesto',
      finalText: 'Send me the quote',
      translated: true,
    );
    montar(e);

    await container
        .read(historyControllerProvider.notifier)
        .correct(e, 'Send me the estimate');

    expect(
      history.updated[7],
      'Send me the estimate',
      reason: 'la fila sí se corrige',
    );
    expect(learning.corrections, isEmpty, reason: 'pero no se aprende de ella');
    expect(learning.vocab, isEmpty);
  });

  test('una corrección vacía no borra el texto que había', () async {
    final e = _entry(raw: 'algo', finalText: 'Algo.');
    montar(e);

    await container.read(historyControllerProvider.notifier).correct(e, '   ');

    expect(history.updated, isEmpty);
  });
}
