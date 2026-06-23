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
}
