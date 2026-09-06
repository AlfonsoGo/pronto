import '../learning/word_diff.dart';

/// Cuánto contenido añadió y cuánto borró el formateador.
///
/// Se mide sobre palabras-contenido normalizadas (minúsculas, sin la
/// puntuación de los bordes): la puntuación que el formateador AÑADE es
/// justamente su trabajo, así que no puede contar como desviación.
class ContentDrift {
  /// Fracción de operaciones que meten contenido nuevo (inserciones y
  /// sustituciones). Es la señal de alucinación: un modelo que se inventa algo
  /// añade palabras.
  final double added;

  /// Fracción de operaciones que quitan contenido. Borrar muletillas es
  /// formateo correcto; borrar media frase es resumir.
  final double deleted;

  const ContentDrift({required this.added, required this.deleted});

  static const ContentDrift none = ContentDrift(added: 0, deleted: 0);

  /// Desviación total. Es la métrica SIMÉTRICA que usaba la versión anterior;
  /// se conserva solo para poder compararlas en el banco de pruebas.
  double get total => added + deleted;

  @override
  String toString() => 'ContentDrift(añadido: ${added.toStringAsFixed(3)}, '
      'borrado: ${deleted.toStringAsFixed(3)})';
}

/// Mide la desviación de contenido entre [original] y [candidate].
///
/// Por qué asimétrica, con números: sobre 443 dictados reales en español, un
/// guardia simétrico al 0,15 descartaba el 10,2 % del formateo legítimo, porque
/// contaba las muletillas borradas como si fueran invención. Separando las dos
/// direcciones, el mismo 0,20 de contenido añadido descarta el 1,6 %.
ContentDrift measureContentDrift(String original, String candidate) {
  final a = normalizedContentTokens(original);
  final b = normalizedContentTokens(candidate);

  // Sin contenido por ninguno de los dos lados no hay nada que comparar.
  if (a.isEmpty && b.isEmpty) return ContentDrift.none;
  // Uno vacío y el otro no: desviación total en la dirección que corresponda.
  if (a.isEmpty) return const ContentDrift(added: 1, deleted: 0);
  if (b.isEmpty) return const ContentDrift(added: 0, deleted: 1);

  final ops = alignWords(a, b);
  if (ops.isEmpty) return ContentDrift.none;

  var added = 0;
  var deleted = 0;
  for (final op in ops) {
    switch (op.type) {
      case DiffOpType.insert:
      case DiffOpType.substitute:
        added++;
      case DiffOpType.delete:
        deleted++;
      case DiffOpType.equal:
        break;
    }
  }
  return ContentDrift(
    added: added / ops.length,
    deleted: deleted / ops.length,
  );
}

/// Tokeniza para comparar SOLO contenido: minúsculas, sin la puntuación de los
/// extremos y SIN TILDES. Los tokens que quedan vacíos (signos sueltos como
/// «¿», «,» o «.») se descartan.
///
/// Las tildes se pliegan a propósito: poner la tilde que falta («reunion» →
/// «reunión», «que» → «qué») es justamente uno de los cuatro trabajos del
/// formateo. Si contaran como contenido cambiado, el guardia estaría castigando
/// al modelo por hacer lo que se le pide.
List<String> normalizedContentTokens(String text) {
  final result = <String>[];
  for (final token in tokenizeWords(text)) {
    final norm = _foldAccents(_stripPunctuation(token.toLowerCase()));
    if (norm.isNotEmpty) result.add(norm);
  }
  return result;
}

/// Quita la puntuación (y los espacios) de los extremos de un token, dejando
/// intactos los guiones y apóstrofos internos de la palabra.
final RegExp _punctEdges = RegExp(r'^[\s\p{P}]+|[\s\p{P}]+$', unicode: true);

String _stripPunctuation(String token) => token.replaceAll(_punctEdges, '');

/// Vocales acentuadas del español (y la diéresis) a su forma sin tilde. La ñ NO
/// se pliega: «ano» y «año» son palabras distintas, no una tilde que falta.
const Map<String, String> _sinTilde = {
  'á': 'a',
  'é': 'e',
  'í': 'i',
  'ó': 'o',
  'ú': 'u',
  'ü': 'u',
  'à': 'a',
  'è': 'e',
  'ì': 'i',
  'ò': 'o',
  'ù': 'u',
  'â': 'a',
  'ê': 'e',
  'î': 'i',
  'ô': 'o',
  'û': 'u',
};

String _foldAccents(String token) {
  final buf = StringBuffer();
  for (final ch in token.split('')) {
    buf.write(_sinTilde[ch] ?? ch);
  }
  return buf.toString();
}

/// Devuelve true si TODAS las cifras que se dictaron siguen en [candidate].
///
/// Hace falta aparte de [measureContentDrift] porque el guardia de proporción
/// no puede protegerlas: cambiar «1250» por «1520» en un dictado de cuarenta
/// palabras es UNA sustitución, y sale en 0,18 — por debajo de cualquier umbral
/// razonable. Una cifra mal no es un porcentaje: o está o no está.
///
/// Tolera que el formateador meta el separador de millares («1250» → «1.250»)
/// y que AÑADA cifras que no estaban («9 y media» → «9:30»): lo que no se le
/// permite es perder ni cambiar una de las que se dictaron.
bool digitsPreserved(String original, String candidate) {
  final quedan = _digitRuns(candidate);
  for (final cifra in _digitRuns(original)) {
    if (!quedan.remove(cifra)) return false;
  }
  return true;
}

/// Extrae las tiradas de dígitos, quitando los separadores de millares y los
/// espacios que las parten, para que «1.250» y «1250» sean la misma cifra.
final RegExp _digitRun = RegExp(r'[0-9][0-9.,  ]*[0-9]|[0-9]');
final RegExp _noDigits = RegExp(r'[^0-9]');

List<String> _digitRuns(String text) {
  final out = <String>[];
  for (final m in _digitRun.allMatches(text)) {
    final limpio = m.group(0)!.replaceAll(_noDigits, '');
    if (limpio.isNotEmpty) out.add(limpio);
  }
  return out;
}
