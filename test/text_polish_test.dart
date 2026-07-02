// ignore_for_file: require_trailing_commas
import 'package:flutter_test/flutter_test.dart';
import 'package:pronto/src/core/text_polish.dart';

void main() {
  // Aísla cada bloque para tests deterministas.
  String nums(String s) =>
      TextPolish.apply(s, spokenPunctuation: false, spanishOrthography: false);
  String punct(String s) =>
      TextPolish.apply(s, numbers: false, spanishOrthography: false);
  String ortho(String s) =>
      TextPolish.apply(s, numbers: false, spokenPunctuation: false);

  group('numeros (ITN)', () {
    test('palabra simple', () => expect(nums('veinticinco'), '25'));
    test('compuesto con y', () => expect(nums('treinta y cuatro'), '34'));
    test(
        'miles', () => expect(nums('mil doscientos treinta y cuatro'), '1234'));
    test('anio (sin separador de miles)',
        () => expect(nums('dos mil veinticuatro'), '2024'));
    test('cien', () => expect(nums('cien'), '100'));
    test('preserva el texto alrededor',
        () => expect(nums('tengo tres euros'), 'tengo 3 euros'));
    test('articulo "un" suelto NO se convierte',
        () => expect(nums('dame un momento'), 'dame un momento'));
    test('por ciento', () => expect(nums('veinticinco por ciento'), '25 %'));
  });

  group('puntuacion dictada', () {
    test(
        'nueva linea',
        () =>
            expect(punct('primera nueva línea segunda'), 'primera \n segunda'));
    test('parentesis', () {
      expect(punct('abre paréntesis hola cierra paréntesis'), '( hola )');
    });
    test('dos puntos', () => expect(punct('a dos puntos b'), 'a : b'));
    test('"punto" suelto solo al final',
        () => expect(punct('la reunión punto'), 'la reunión.'));
    test('no toca "punto" en mitad de frase',
        () => expect(punct('punto de venta'), 'punto de venta'));
  });

  group('ortografia ES', () {
    test('mayuscula inicial', () => expect(ortho('hola mundo'), 'Hola mundo'));
    test('abre interrogacion',
        () => expect(ortho('cómo estás?'), '¿Cómo estás?'));
    test('abre exclamacion', () => expect(ortho('genial!'), '¡Genial!'));
    test('NO duplica apertura si el motor ya la puso (bug ¿¿)',
        () => expect(ortho('¿cómo estás?'), '¿Cómo estás?'));
    test('NO duplica apertura exclamacion',
        () => expect(ortho('¡genial!'), '¡Genial!'));
    test('mixto: una con apertura previa y otra sin ella',
        () => expect(ortho('¿vienes? seguro que sí!'),
            '¿Vienes? ¡Seguro que sí!'));
    test('NO rompe dominios',
        () => expect(ortho('mira github.com vale'), 'Mira github.com vale'));
    test(
        'espacio tras coma', () => expect(ortho('hola ,mundo'), 'Hola, mundo'));
    test('no mete espacio en hora 15:30',
        () => expect(ortho('a las 15:30 quedamos'), 'A las 15:30 quedamos'));
  });

  group('combinado (apply por defecto)', () {
    test('frase realista', () {
      expect(TextPolish.apply('tengo veinticinco años'), 'Tengo 25 años');
    });
    test('pregunta con numero', () {
      expect(TextPolish.apply('cuántos son? veinticinco'), '¿Cuántos son? 25');
    });
  });

  group('muletillas (opt-in)', () {
    test('borra eh aislado', () {
      expect(
          TextPolish.apply('eh hola mundo', removeFillers: true), 'Hola mundo');
    });
    test('por defecto NO borra muletillas', () {
      expect(TextPolish.apply('eh hola'), 'Eh hola');
    });
  });

  test('vacio se devuelve igual', () => expect(TextPolish.apply('  '), '  '));

  // ──────────────────────────────────────────────────────────────────────────
  // Casos adicionales — bordes no cubiertos antes
  // ──────────────────────────────────────────────────────────────────────────

  group('ortografia ES — bordes adicionales', () {
    test('exclamacion con apertura previa NO duplica ¡',
        () => expect(ortho('¡genial!'), '¡Genial!'));

    test('frase mixta: interrogacion previa + exclamacion sin apertura',
        () => expect(
              ortho('¿sabes lo que pasó? increíble!'),
              '¿Sabes lo que pasó? ¡Increíble!',
            ));

    test('dos preguntas seguidas ninguna tiene apertura previa',
        () => expect(
              ortho('qué tal? cómo estás?'),
              '¿Qué tal? ¿Cómo estás?',
            ));

    test('mayuscula tras salto de linea',
        () => expect(ortho('primera\nsegunda'), 'Primera\nSegunda'));

    test('no mete espacio en decimal 3,5',
        () => expect(ortho('son 3,5 litros'), 'Son 3,5 litros'));
  });

  group('puntuacion dictada — bordes adicionales', () {
    test('"coma" suelto al final de linea',
        () => expect(punct('en la lista coma'), 'en la lista,'));

    test('"coma" en mitad de frase no se toca',
        () => expect(punct('la coma flotante'), 'la coma flotante'));

    // El token queda rodeado de los espacios originales: diseño conservador
    // (no se colapsan espacios cuando spanishOrthography=false).
    test('abre comillas y cierra comillas (solo punt, sin ortografia)',
        () => expect(
              punct('dijo abre comillas hola cierra comillas'),
              'dijo " hola "',
            ));

    test('puntos suspensivos',
        () => expect(punct('bueno puntos suspensivos ya'), 'bueno … ya'));
  });

  group('numeros (ITN) — bordes adicionales', () {
    test('millones simples', () => expect(nums('dos millones'), '2000000'));

    test('novecientos noventa y nueve',
        () => expect(nums('novecientos noventa y nueve'), '999'));

    test('doscientos treinta y cuatro',
        () => expect(nums('doscientos treinta y cuatro'), '234'));

    test('"una" suelto NO se convierte',
        () => expect(nums('dame una oportunidad'), 'dame una oportunidad'));

    test('"un" suelto NO se convierte',
        () => expect(nums('es un problema'), 'es un problema'));
  });
}
