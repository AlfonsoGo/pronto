import 'package:flutter_test/flutter_test.dart';
import 'package:pronto/src/features/history/time_saved.dart';

void main() {
  group('tiempo ahorrado', () {
    test('es lo que se habría tardado tecleando menos lo que se tardó', () {
      // 200 palabras a 40 ppm = 5 min de teclear. Hablarlas costó 1 min.
      final ahorro = timeSaved(
        words: 200,
        spoken: const Duration(minutes: 1),
        processing: Duration.zero,
      );

      expect(ahorro, const Duration(minutes: 4));
    });

    test('el tiempo de la máquina también cuenta: es espera del usuario', () {
      final ahorro = timeSaved(
        words: 40,
        spoken: const Duration(seconds: 30),
        processing: const Duration(seconds: 10),
      );

      expect(ahorro, const Duration(seconds: 20));
    });

    test('nunca es negativo: si tardaste más, el ahorro es cero, no una deuda',
        () {
      final ahorro = timeSaved(
        words: 5,
        spoken: const Duration(minutes: 3),
        processing: Duration.zero,
      );

      expect(ahorro, Duration.zero);
    });

    test('sin palabras no hay ahorro', () {
      expect(
        timeSaved(words: 0, spoken: Duration.zero, processing: Duration.zero),
        Duration.zero,
      );
    });
  });

  group('la fórmula se enseña, no se esconde', () {
    test('el texto dice la velocidad de mecanografía que se supone', () {
      expect(kTimeSavedFormula, contains('$kTypingWordsPerMinute'));
      expect(kTimeSavedFormula.toLowerCase(), contains('palabras'));
    });
  });
}
