import 'package:flutter_test/flutter_test.dart';
import 'package:pronto/src/features/translation/language_detect.dart';

// Tests PUROS del detector de idioma: sin red, sin plugins, sin LLM.
// El detector decide hacia qué lado traduce Pronto, así que lo que importa es
// (a) que acierte en frases dictadas normales y (b) que devuelva null —y no una
// respuesta inventada— cuando no hay señal.

void main() {
  group('TextLanguage', () {
    test('other alterna el par es ⇄ en', () {
      expect(TextLanguage.es.other, TextLanguage.en);
      expect(TextLanguage.en.other, TextLanguage.es);
    });

    test('fromCode reconoce los códigos válidos', () {
      expect(TextLanguage.fromCode('es'), TextLanguage.es);
      expect(TextLanguage.fromCode('en'), TextLanguage.en);
    });

    test('fromCode devuelve null con códigos desconocidos o nulos', () {
      expect(TextLanguage.fromCode('fr'), isNull);
      expect(TextLanguage.fromCode('ES'), isNull); // sensible a mayúsculas
      expect(TextLanguage.fromCode(null), isNull);
      expect(TextLanguage.fromCode(''), isNull);
    });

    test('badge y label son los que muestra la interfaz', () {
      expect(TextLanguage.es.badge, 'ES');
      expect(TextLanguage.en.badge, 'EN');
      expect(TextLanguage.es.label, 'Español');
      expect(TextLanguage.en.label, 'Inglés');
    });
  });

  group('detectLanguage · español', () {
    const frases = [
      'Hola, ¿podemos mover la reunión a las cuatro de la tarde?',
      'Te paso el presupuesto revisado esta tarde con los cambios.',
      'No he podido terminar el informe porque falta el dato de ventas.',
      'Buenos días, quiero confirmar la cita del jueves.',
      'El cliente dice que la factura no le ha llegado todavía.',
    ];

    for (final f in frases) {
      test('detecta español: "${f.substring(0, 28)}…"', () {
        expect(detectLanguage(f), TextLanguage.es);
      });
    }

    test('acierta aunque no haya tildes (dictado sin acentuar)', () {
      expect(
        detectLanguage('el cliente quiere que le mandemos la propuesta manana'),
        TextLanguage.es,
      );
    });

    test('los signos ¿ ¡ bastan en una frase corta', () {
      expect(detectLanguage('¿Vamos?'), TextLanguage.es);
    });
  });

  group('detectLanguage · inglés', () {
    const frases = [
      'Good morning, can we move the meeting to four in the afternoon?',
      "I'll send you the revised quote this afternoon with the changes.",
      'The client says the invoice has not arrived yet.',
      'I need to check the numbers before we send the proposal.',
      'Thanks for your time, I will get back to you tomorrow.',
    ];

    for (final f in frases) {
      test('detecta inglés: "${f.substring(0, 28)}…"', () {
        expect(detectLanguage(f), TextLanguage.en);
      });
    }
  });

  group('detectLanguage · sin señal devuelve null', () {
    test('texto vacío', () {
      expect(detectLanguage(''), isNull);
    });

    test('solo espacios', () {
      expect(detectLanguage('   \n  '), isNull);
    });

    test('solo cifras y puntuación', () {
      expect(detectLanguage('42, 17. 2026-07-29 -- 3,50 €'), isNull);
    });

    test('nombre propio suelto', () {
      expect(detectLanguage('Alfonso Beltran'), isNull);
    });

    test('palabra ambigua a los dos idiomas', () {
      // "no" y "total" existen en ambos: no debe inventarse un idioma.
      expect(detectLanguage('no total'), isNull);
    });
  });

  group('detectLanguage · robustez', () {
    test('no se cuelga ni falla con un texto muy largo', () {
      final largo = 'Hola, esto es una prueba de dictado largo. ' * 500;
      expect(detectLanguage(largo), TextLanguage.es);
    });

    test('un "café" suelto no vuelca un texto inglés entero a español', () {
      expect(
        detectLanguage(
          'I would like to have a coffee at the café near the office, '
          'because the meeting is not until three in the afternoon.',
        ),
        TextLanguage.en,
      );
    });

    test('mayúsculas y minúsculas dan el mismo resultado', () {
      const frase = 'El cliente quiere la propuesta para el lunes.';
      expect(detectLanguage(frase), detectLanguage(frase.toUpperCase()));
    });

    test('las comillas alrededor de una palabra no la ocultan', () {
      expect(
        detectLanguage("'the' 'meeting' 'is' 'not' 'ready'"),
        TextLanguage.en,
      );
    });
  });
}
