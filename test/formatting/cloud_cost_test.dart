// ignore_for_file: require_trailing_commas
import 'dart:math';

import 'package:flutter_test/flutter_test.dart';
import 'package:pronto/src/features/formatting/cloud_cost.dart';

// Lo que cuesta mandar un dictado a la nube, y cómo se escribe un importe.
// Es la cuenta que mira el tope de gasto ANTES de dejar salir una petición:
// si se queda corta, el tope deja pasar la llamada que lo rompe.

void main() {
  group('estimación del coste', () {
    test('un dictado normal cuesta céntimas de céntimo', () {
      // ~40 palabras, el tamaño de un dictado de verdad.
      final coste = estimateCallCostUsd(
        model: 'openai/gpt-5-nano',
        textChars: 240,
      );
      expect(coste, greaterThan(0));
      expect(coste, lessThan(0.001));
    });

    test('el prompt fijo se cuenta aunque el dictado sea vacío', () {
      // El sistema y los ejemplos viajan en CADA llamada: un coste de cero para
      // el texto vacío haría creer que llamar es gratis.
      expect(
        estimateCallCostUsd(model: 'openai/gpt-5-nano', textChars: 0),
        greaterThan(0),
      );
    });

    test('cuanto más largo el dictado, más cuesta (monotonía)', () {
      var previo = 0.0;
      for (final chars in [0, 10, 100, 1000, 4000]) {
        final coste =
            estimateCallCostUsd(model: 'openai/gpt-5-nano', textChars: chars);
        expect(coste, greaterThan(previo));
        previo = coste;
      }
    });

    test('un modelo DESCONOCIDO se cuenta como caro, no como gratis', () {
      final conocido =
          estimateCallCostUsd(model: 'openai/gpt-5-mini', textChars: 500);
      final desconocido =
          estimateCallCostUsd(model: 'modelo/que-no-existe', textChars: 500);
      expect(desconocido, greaterThan(conocido));
    });

    test('el prompt fijo se mide de las cadenas reales, no a ojo', () {
      // Si alguien alarga el prompt, el tope tiene que enterarse solo.
      expect(kFormatPromptTokens, greaterThan(500));
      expect(kFormatPromptTokens, lessThan(3000));
    });

    test('la estimación es PESIMISTA: pasa del coste de solo la entrada', () {
      // El error se comete siempre hacia arriba: un tope que subestima deja
      // pasar la llamada que lo rompe.
      const chars = 1000;
      final precio = kCloudPrices['openai/gpt-5-nano']!;
      final soloEntrada =
          (kFormatPromptTokens + chars / kCharsPerToken) / 1e6 * precio.$1;
      expect(
        estimateCallCostUsd(model: 'openai/gpt-5-nano', textChars: chars),
        greaterThan(soloEntrada),
      );
    });
  });

  group('importes en español', () {
    test('se escriben con coma decimal y el símbolo detrás', () {
      expect(formatUsd(0.1234), '0,1234 \$');
      expect(formatUsd(0), '0,0000 \$');
      expect(formatUsd(1.5, decimals: 2), '1,50 \$');
    });

    test('cuatro decimales porque el uso normal son céntimas', () {
      // Con dos decimales, un mes entero de dictado saldría como «0,00 $» y
      // el contador parecería roto.
      expect(formatUsd(0.0081), isNot('0,00 \$'));
    });

    test('se leen con coma o con punto', () {
      expect(parseUsd('0,25'), 0.25);
      expect(parseUsd('0.25'), 0.25);
      expect(parseUsd('  1,5  '), 1.5);
      expect(parseUsd('0'), 0);
    });

    test('lo que no es un número, o es negativo, no se acepta', () {
      // Un tope negativo apagaría la nube por la puerta de atrás en vez de
      // decirlo, y uno ilegible no puede convertirse en "sin tope".
      for (final basura in ['', '  ', 'gratis', '-1', '1,2,3', 'NaN', '∞']) {
        expect(parseUsd(basura), isNull, reason: basura);
      }
    });

    test('ida y vuelta: lo que se escribe se vuelve a leer igual', () {
      final rnd = Random(7);
      for (var i = 0; i < 1000; i++) {
        final v = rnd.nextDouble() * 10;
        final texto = formatUsd(v).replaceAll(' \$', '');
        expect(parseUsd(texto), closeTo(v, 0.0001));
      }
    });
  });
}
