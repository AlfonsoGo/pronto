import 'package:flutter_test/flutter_test.dart';
import 'package:pronto/src/features/learning/word_diff.dart';

void main() {
  group('alignWords', () {
    test('detecta una sustitución de palabra (nombre propio mal oído)', () {
      final a = tokenizeWords('hola me llamo wiroski');
      final b = tokenizeWords('hola me llamo Wirowski');
      final ops = alignWords(a, b);

      expect(ops.length, 4);
      expect(ops.take(3).every((o) => o.type == DiffOpType.equal), isTrue);
      expect(ops.last.type, DiffOpType.substitute);
      expect(ops.last.source, 'wiroski');
      expect(ops.last.target, 'Wirowski');
    });

    test('detecta inserción', () {
      final ops = alignWords(
        tokenizeWords('quiero café'),
        tokenizeWords('quiero un café'),
      );
      expect(ops.any((o) => o.type == DiffOpType.insert && o.target == 'un'),
          isTrue,);
    });

    test('detecta borrado', () {
      final ops = alignWords(
        tokenizeWords('quiero un café'),
        tokenizeWords('quiero café'),
      );
      expect(ops.any((o) => o.type == DiffOpType.delete && o.source == 'un'),
          isTrue,);
    });

    test('texto idéntico => todo equal', () {
      final ops = alignWords(
        tokenizeWords('esto es una prueba'),
        tokenizeWords('esto es una prueba'),
      );
      expect(ops.every((o) => o.type == DiffOpType.equal), isTrue);
      expect(changeRatio(ops), 0.0);
    });
  });
}
