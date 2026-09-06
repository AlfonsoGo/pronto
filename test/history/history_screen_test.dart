import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:pronto/src/features/history/dictation_entry.dart';
import 'package:pronto/src/features/history/history_controller.dart';
import 'package:pronto/src/features/history/history_repository.dart';
import 'package:pronto/src/features/history/history_screen.dart';
import 'package:pronto/src/platform/platform_services.dart';

/// Repositorio en memoria: la pantalla se prueba sin SQLite ni disco.
class _FakeHistory implements HistoryRepository {
  _FakeHistory(this.entries);

  final List<DictationEntry> entries;
  int paginasPedidas = 0;
  int borradas = 0;

  List<DictationEntry> _filtered(HistoryQuery q) {
    final texto = q.text.trim().toLowerCase();
    return entries.where((e) {
      if (q.onlyFailed && !e.failed) return false;
      if (texto.isEmpty) return true;
      return (e.raw + e.finalText).toLowerCase().contains(texto);
    }).toList();
  }

  @override
  Future<List<DictationEntry>> page(
    HistoryQuery query, {
    int limit = 50,
    int offset = 0,
  }) async {
    paginasPedidas++;
    return _filtered(query).skip(offset).take(limit).toList();
  }

  @override
  Future<int> count(HistoryQuery query) async => _filtered(query).length;

  @override
  Future<List<DictationEntry>> all(HistoryQuery query) async =>
      _filtered(query);

  @override
  Future<HistoryStats> stats(HistoryQuery query) async {
    final l = _filtered(query);
    return HistoryStats(
      total: l.length,
      words: l.fold(0, (a, e) => a + e.wordCount),
      spoken: Duration.zero,
      processing: Duration.zero,
      costUsd: 0,
      perDay: const {},
      perMode: const {},
      perApp: const {},
    );
  }

  @override
  Future<List<String>> distinctModes() async => const [];

  @override
  Future<void> deleteEntry(int id) async =>
      entries.removeWhere((e) => e.id == id);

  @override
  Future<int> deleteAll() async {
    borradas = entries.length;
    entries.clear();
    return borradas;
  }

  @override
  Future<int> purgeOlderThan(Duration age, {DateTime? now}) async => 0;

  @override
  Future<void> updateFinalText(int id, String text) async {}

  @override
  Future<int> log(DictationRecord record) async => 1;
}

DictationEntry _entry({
  required int id,
  String raw = 'crudo',
  String finalText = 'Final',
  String? notice,
}) =>
    DictationEntry(
      id: id,
      at: DateTime(2026, 9, 6, 11, id % 60),
      raw: raw,
      finalText: finalText,
      formatNotice: notice,
      engine: 'parakeet',
    );

Widget _app(_FakeHistory repo) => ProviderScope(
      overrides: [historyRepositoryProvider.overrideWithValue(repo)],
      child: const MaterialApp(home: HistoryScreen()),
    );

void main() {
  // La pantalla es de escritorio y tiene dos paneles: en la ventana de test por
  // defecto (800x600) el panel derecho se queda sin sitio.
  Future<void> abrir(WidgetTester tester, _FakeHistory repo) async {
    tester.view.physicalSize = const Size(1600, 1000);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);
    await tester.pumpWidget(_app(repo));
    await tester.pumpAndSettle();
  }

  testWidgets('con 5.000 dictados abre pidiendo UNA página, no la tabla entera',
      (tester) async {
    final repo = _FakeHistory([
      for (var i = 0; i < 5000; i++)
        _entry(id: i, raw: 'crudo $i', finalText: 'Final $i'),
    ]);

    final reloj = Stopwatch()..start();
    await abrir(tester, repo);
    reloj.stop();

    // Solo se construyen las filas de la primera página.
    expect(find.text('Final 0'), findsOneWidget);
    expect(find.text('Final 4999'), findsNothing);
    expect(find.textContaining('de 5000 dictados'), findsOneWidget);
    expect(
      reloj.elapsedMilliseconds,
      lessThan(2000),
      reason: 'abrir tardó ${reloj.elapsedMilliseconds} ms',
    );
  });

  testWidgets('sin dictados lo dice, en vez de enseñar una lista vacía',
      (tester) async {
    await abrir(tester, _FakeHistory([]));

    expect(find.text('Todavía no has dictado nada'), findsOneWidget);
  });

  testWidgets('el detalle enfrenta el crudo y lo que se pegó', (tester) async {
    await abrir(
      tester,
      _FakeHistory([
        _entry(id: 1, raw: 'oye celia', finalText: 'Oye, Celia.'),
      ]),
    );

    expect(find.text('LO QUE OYÓ EL MOTOR'), findsOneWidget);
    expect(find.text('LO QUE SE PEGÓ'), findsOneWidget);
    expect(find.text('oye celia'), findsOneWidget);
  });

  testWidgets('un dictado que falló al formatear enseña NUESTRO aviso',
      (tester) async {
    const nuestro = 'Sin formatear: el modelo cambió una cifra que dictaste';
    await abrir(
      tester,
      _FakeHistory([
        _entry(id: 1, raw: 'son 300 euros', finalText: '', notice: nuestro),
      ]),
    );

    expect(find.text(nuestro), findsOneWidget);
  });

  testWidgets('«borrar todo» pide confirmación y dice cuántos se van',
      (tester) async {
    final repo = _FakeHistory([
      _entry(id: 1),
      _entry(id: 2),
      _entry(id: 3),
    ]);
    await abrir(tester, repo);

    await tester.tap(find.text('Borrar todo'));
    await tester.pumpAndSettle();

    expect(find.textContaining('Se borran 3 dictados'), findsOneWidget);
    expect(repo.entries, hasLength(3), reason: 'aún no se ha confirmado');

    await tester.tap(find.text('Cancelar'));
    await tester.pumpAndSettle();
    expect(repo.entries, hasLength(3));
  });

  testWidgets('exportar avisa de lo que lleva el fichero ANTES de escribirlo',
      (tester) async {
    await abrir(tester, _FakeHistory([_entry(id: 1)]));

    await tester.tap(find.text('Exportar CSV'));
    await tester.pumpAndSettle();

    expect(find.textContaining('sin cifrar'), findsOneWidget);
    expect(find.text('Antes de guardar el fichero'), findsOneWidget);
  });

  testWidgets('la fórmula del tiempo ahorrado se ve en pantalla',
      (tester) async {
    await abrir(tester, _FakeHistory([_entry(id: 1)]));

    expect(find.textContaining('÷ 40 por minuto'), findsWidgets);
  });

  testWidgets('buscar filtra la lista', (tester) async {
    await abrir(
      tester,
      _FakeHistory([
        _entry(id: 1, raw: 'presupuesto', finalText: 'Presupuesto enviado'),
        _entry(id: 2, raw: 'otra cosa', finalText: 'Otra cosa'),
      ]),
    );

    await tester.enterText(find.byType(TextField).first, 'presupuesto');
    await tester.pump(const Duration(milliseconds: 400));
    await tester.pumpAndSettle();

    expect(find.text('Presupuesto enviado'), findsOneWidget);
    expect(find.text('Otra cosa'), findsNothing);
  });

  testWidgets('sin repositorio lo dice, y no finge estar vacío',
      (tester) async {
    tester.view.physicalSize = const Size(1600, 1000);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);
    await tester.pumpWidget(
      ProviderScope(
        overrides: [historyRepositoryProvider.overrideWithValue(null)],
        child: const MaterialApp(home: HistoryScreen()),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('El historial no está disponible'), findsOneWidget);
  });

  test('la página por defecto es pequeña: la lista no carga de golpe', () {
    expect(kHistoryPageSize, lessThanOrEqualTo(100));
  });
}
