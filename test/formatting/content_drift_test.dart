// ignore_for_file: require_trailing_commas
import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:pronto/src/features/formatting/content_drift.dart';
import 'package:pronto/src/features/formatting/format_preset.dart';

// Tests PUROS del guardia de contenido: sin red, sin plugins, sin ficheros de
// la app. Lo único que se lee de disco es el corpus de regresión saneado.
void main() {
  group('measureContentDrift — casos base', () {
    test('texto idéntico: cero desviación por los dos lados', () {
      final d = measureContentDrift('hola qué tal', 'hola qué tal');
      expect(d.added, 0);
      expect(d.deleted, 0);
    });

    test('solo puntuación y mayúsculas: NO es desviación', () {
      // Es literalmente el trabajo del formateo. Si esto contara, el guardia
      // descartaría todas las salidas buenas.
      final d = measureContentDrift(
        'que hora es no llego a la reunion',
        '¿Qué hora es? No llego a la reunión.',
      );
      expect(d.added, 0);
      expect(d.deleted, 0);
    });

    test('palabra añadida: cuenta como añadido, no como borrado', () {
      final d =
          measureContentDrift('mándame el informe', 'mándame el informe hoy');
      expect(d.added, greaterThan(0));
      expect(d.deleted, 0);
    });

    test('palabra quitada: cuenta como borrado, no como añadido', () {
      final d =
          measureContentDrift('eh mándame el informe', 'mándame el informe');
      expect(d.deleted, greaterThan(0));
      expect(d.added, 0);
    });

    test('palabra sustituida: cuenta como añadido (contenido nuevo)', () {
      final d =
          measureContentDrift('mándame el informe', 'mándame el contrato');
      expect(d.added, greaterThan(0));
    });

    test('la asimetría es real: quitar y añadir no dan lo mismo', () {
      final quita = measureContentDrift('a b c d e f', 'a b c d e');
      final anade = measureContentDrift('a b c d e', 'a b c d e f');
      expect(quita.deleted, greaterThan(0));
      expect(quita.added, 0);
      expect(anade.added, greaterThan(0));
      expect(anade.deleted, 0);
    });
  });

  group('measureContentDrift — bordes', () {
    test('los dos vacíos: sin desviación', () {
      expect(measureContentDrift('', '').added, 0);
      expect(measureContentDrift('   ', '\n\t ').deleted, 0);
    });

    test('el original vacío y el candidato no: todo añadido', () {
      final d = measureContentDrift('', 'se lo inventó entero');
      expect(d.added, 1);
      expect(d.deleted, 0);
    });

    test('el candidato vacío: todo borrado', () {
      final d = measureContentDrift('esto sí se dictó', '');
      expect(d.deleted, 1);
      expect(d.added, 0);
    });

    test('solo signos de puntuación: se normaliza a vacío', () {
      final d = measureContentDrift('¿?¡!', '...');
      expect(d.added, 0);
      expect(d.deleted, 0);
    });

    test('emoji y unicode no revientan el guardia', () {
      final d = measureContentDrift('vale 👍 lo miro', 'Vale, 👍 lo miro.');
      expect(d.added, 0);
      expect(d.deleted, 0);
    });

    test('un solo token', () {
      expect(measureContentDrift('hola', 'Hola.').added, 0);
      expect(measureContentDrift('hola', 'adiós').added, 1);
    });

    test('texto muy largo: no se cuelga ni desborda', () {
      final largo = List.filled(400, 'palabra').join(' ');
      final d = measureContentDrift(largo, '$largo extra');
      expect(d.added, greaterThan(0));
      expect(d.added, lessThan(0.02));
    });
  });

  group('invariantes', () {
    test('las dos fracciones están siempre en [0, 1]', () {
      const muestras = [
        ['', ''],
        ['a', ''],
        ['', 'a'],
        ['a b c', 'c b a'],
        ['uno dos tres', 'uno dos tres cuatro cinco seis'],
        ['¿qué tal?', 'que tal'],
        ['a a a a a', 'a'],
      ];
      for (final m in muestras) {
        final d = measureContentDrift(m[0], m[1]);
        expect(d.added, inInclusiveRange(0, 1), reason: '${m[0]} -> ${m[1]}');
        expect(d.deleted, inInclusiveRange(0, 1), reason: '${m[0]} -> ${m[1]}');
      }
    });

    test('medirse contra sí mismo da siempre cero', () {
      const muestras = [
        'hola',
        '¿Qué tal? Bien, gracias.',
        'a b c d e f g',
        '1250 euros el 3 de octubre',
        '',
      ];
      for (final s in muestras) {
        final d = measureContentDrift(s, s);
        expect(d.total, 0, reason: s);
      }
    });
  });

  // ─── Regresión: la calibración medida no se puede aflojar sin que salte ────
  group('corpus de regresión (saneado, ver test/fixtures/formateo_es.json)',
      () {
    late Map<String, dynamic> fixture;

    setUpAll(() {
      final f = File('test/fixtures/formateo_es.json');
      fixture = jsonDecode(f.readAsStringSync()) as Map<String, dynamic>;
    });

    test('TODO el formateo legítimo pasa el guardia', () {
      final casos =
          (fixture['aceptables'] as List).cast<Map<String, dynamic>>();
      expect(casos, isNotEmpty);
      for (final c in casos) {
        final d = measureContentDrift(
          c['crudo'] as String,
          c['formateado'] as String,
        );
        expect(
          d.added,
          lessThanOrEqualTo(FormatPreset.limpio.maxAddedContent),
          reason: 'DESCARTADO por añadido: ${c['caso']} · $d',
        );
        expect(
          d.deleted,
          lessThanOrEqualTo(FormatPreset.limpio.maxDeletedContent),
          reason: 'DESCARTADO por borrado: ${c['caso']} · $d',
        );
      }
    });

    test('TODA alucinación se descarta', () {
      final casos =
          (fixture['alucinaciones'] as List).cast<Map<String, dynamic>>();
      expect(casos, isNotEmpty);
      for (final c in casos) {
        final d = measureContentDrift(
          c['crudo'] as String,
          c['formateado'] as String,
        );
        final descartada = d.added > FormatPreset.limpio.maxAddedContent ||
            d.deleted > FormatPreset.limpio.maxDeletedContent;
        expect(
          descartada,
          isTrue,
          reason: 'SE COLÓ una alucinación: ${c['caso']} · $d',
        );
      }
    });

    test(
      'el guardia SIMÉTRICO viejo (0,15 total) descartaba formateo bueno',
      () {
        // Este test es la prueba escrita de por qué se cambió el guardia. Si
        // alguien vuelve al umbral simétrico, aquí se ve qué se pierde.
        final casos =
            (fixture['aceptables'] as List).cast<Map<String, dynamic>>();
        final descartados = casos.where((c) {
          final d = measureContentDrift(
            c['crudo'] as String,
            c['formateado'] as String,
          );
          return d.total > 0.15;
        }).toList();
        expect(
          descartados,
          isNotEmpty,
          reason: 'Si esto queda vacío, el corpus ya no cubre el caso de las '
              'muletillas y el test dejó de proteger nada',
        );
      },
    );
  });

  group('las cifras se comprueban EXACTAS, no por proporción', () {
    // El guardia proporcional NO puede proteger una cifra: cambiar «1250» por
    // «1520» en un dictado largo es una sola sustitución y sale en 0,18, por
    // debajo de cualquier umbral razonable. Por eso hay una comprobación aparte.
    test('el original sin cifras siempre pasa', () {
      expect(digitsPreserved('hola qué tal', 'Hola, ¿qué tal?'), isTrue);
    });

    test('la cifra intacta pasa', () {
      expect(
        digitsPreserved('son 1250 euros', 'Son 1250 euros.'),
        isTrue,
      );
    });

    test('la cifra CAMBIADA no pasa', () {
      expect(
        digitsPreserved('son 1250 euros', 'Son 1520 euros.'),
        isFalse,
      );
    });

    test('la cifra PERDIDA no pasa', () {
      expect(digitsPreserved('son 1250 euros', 'Son unos euros.'), isFalse);
    });

    test('el separador de millares SÍ se tolera', () {
      // «1.250 euros» es ortografía española correcta, no una cifra distinta.
      expect(digitsPreserved('son 1250 euros', 'Son 1.250 euros.'), isTrue);
    });

    test('añadir una cifra que no estaba SÍ se tolera', () {
      // «a las 9 y media» -> «a las 9:30» no pierde nada de lo dictado.
      expect(
        digitsPreserved('quedamos a las 9 y media', 'Quedamos a las 9:30.'),
        isTrue,
      );
    });

    test('varias cifras: se comprueban todas, y por cuenta', () {
      expect(
        digitsPreserved('3 informes y 3 facturas', '3 informes y 3 facturas.'),
        isTrue,
      );
      expect(
        digitsPreserved('3 informes y 3 facturas', '3 informes y facturas.'),
        isFalse,
        reason: 'se perdió una de las dos, aunque el 3 siga apareciendo',
      );
    });

    test('una hora cambiada no pasa', () {
      expect(
        digitsPreserved(
          'la reunión es el 3 de octubre a las 17 horas',
          'La reunión es el 3 de octubre a las 18 horas.',
        ),
        isFalse,
      );
    });
  });

  group('calibración: los umbrales medidos están FIJADOS', () {
    // Sin estos tests, aflojar maxAddedContent de 0,20 a 0,60 no rompía nada:
    // la calibración medida no estaba protegida por ninguna prueba.
    test('un parafraseo MODERADO se descarta (añadido ~0,47)', () {
      const crudo = 'el lunes montamos la reunion con el equipo y revisamos '
          'los tickets abiertos antes de la entrega';
      const parafraseado = 'El lunes celebraremos una sesión con el '
          'departamento y analizaremos las incidencias pendientes antes de la '
          'entrega.';
      final d = measureContentDrift(crudo, parafraseado);
      expect(d.added, greaterThan(FormatPreset.limpio.maxAddedContent));
      expect(d.added, lessThan(0.60),
          reason: 'si sube de 0,60 este caso deja de fijar el umbral');
    });

    test('el modelo se inventa una condición (añadido ~0,55)', () {
      final d = measureContentDrift(
        'manda el informe el viernes',
        'Manda el informe el viernes, siempre que el cliente haya confirmado.',
      );
      expect(d.added, greaterThan(FormatPreset.limpio.maxAddedContent));
      expect(d.added, lessThan(0.60));
    });

    test('el tope de añadido es exactamente el medido', () {
      // Los números salen de 443 dictados reales: 0,20 descarta el 1,6 % del
      // formateo legítimo; 0,15 descartaría el 3,6 %; 0,10 el 9,3 %.
      expect(FormatPreset.limpio.maxAddedContent, 0.20);
      expect(FormatPreset.limpio.maxDeletedContent, 0.60);
    });
  });
}
