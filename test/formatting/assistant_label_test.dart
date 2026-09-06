// ignore_for_file: require_trailing_commas
import 'package:flutter_test/flutter_test.dart';
import 'package:pronto/src/features/formatting/assistant_label.dart';
import 'package:pronto/src/features/formatting/text_formatter.dart';

void main() {
  String etiqueta({
    FormatPath path = FormatPath.local,
    String modelo = 'qwen2.5:7b',
    FormatOutcome outcome = FormatOutcome.formatted,
    Duration elapsed = const Duration(milliseconds: 1234),
  }) =>
      etiquetaAsistente(
        path: path,
        modelo: modelo,
        outcome: outcome,
        elapsed: elapsed,
      );

  group('etiquetaAsistente — qué arregló ESTE dictado', () {
    test('local: dónde corrió, con qué modelo y cuánto tardó', () {
      expect(etiqueta(), 'En tu equipo · qwen2.5:7b · 1,2 s');
    });

    test('nube: se dice que fue la nube, con su modelo', () {
      expect(
        etiqueta(path: FormatPath.cloud, modelo: 'openai/gpt-5-nano'),
        'Nube · openai/gpt-5-nano · 1,2 s',
      );
    });

    test('sin formateo no hay asistente del que hablar', () {
      expect(
        etiqueta(path: FormatPath.none, outcome: FormatOutcome.disabled),
        '',
      );
    });

    test('el modelo respondió lo mismo: SIGUE contando, corrió igual', () {
      // «No cambió nada» es información: el asistente miró el texto y lo dio
      // por bueno. Callarlo haría creer que no se ejecutó.
      expect(etiqueta(outcome: FormatOutcome.unchanged), isNotEmpty);
    });

    test('cuando algo salió mal habla el aviso, no la etiqueta', () {
      // El aviso ya explica el motivo. Poner las dos cosas sería contar dos
      // veces lo mismo en una caja de cuatro líneas.
      for (final malo in [
        FormatOutcome.unavailable,
        FormatOutcome.rejected,
        FormatOutcome.overBudget,
      ]) {
        expect(etiqueta(outcome: malo), '', reason: '$malo');
      }
    });
  });

  group('formatearEspera — que se lea sin pensar', () {
    test('por debajo del segundo, milisegundos', () {
      // «0,1 s» y «0,8 s» parecen lo mismo de un vistazo y no lo son.
      expect(formatearEspera(const Duration(milliseconds: 120)), '120 ms');
      expect(formatearEspera(const Duration(milliseconds: 999)), '999 ms');
      expect(formatearEspera(Duration.zero), '0 ms');
    });

    test('a partir del segundo, segundos con coma decimal', () {
      expect(formatearEspera(const Duration(milliseconds: 1000)), '1,0 s');
      expect(formatearEspera(const Duration(milliseconds: 4560)), '4,6 s');
      expect(formatearEspera(const Duration(seconds: 12)), '12,0 s');
    });

    test('nunca sale un punto decimal: se escribe en español', () {
      expect(formatearEspera(const Duration(milliseconds: 2500)),
          isNot(contains('.')));
    });
  });
}
