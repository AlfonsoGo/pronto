// Pulido determinista del texto dictado (puro String -> String, sin red, sin
// estado, sin LLM). Mejora el texto de CUALQUIER motor (Parakeet o Whisper):
//   - Puntuación dictada: "nueva línea", "dos puntos", "abre paréntesis"...
//   - Números/ITN en español: "veinticinco por ciento" -> "25 %".
//   - Ortografía ES: signos de apertura ¿ ¡, espaciado y mayúsculas de frase.
//   - (opcional) borrado de muletillas aisladas: "eh", "em", "mmm".
//
// Diseño CONSERVADOR: ante la duda, NO tocar (un cambio erróneo molesta más que
// una mejora omitida). Por eso solo se transforman patrones de alta confianza.
//
// Es una función pura -> se prueba con tests (test/text_polish_test.dart).

class TextPolish {
  const TextPolish._();

  /// Aplica el pulido. Cada bloque es conmutable para poder afinar/desactivar.
  static String apply(
    String input, {
    bool spokenPunctuation = true,
    bool numbers = true,
    bool spanishOrthography = true,
    bool removeFillers = false,
  }) {
    if (input.trim().isEmpty) return input;
    var t = input;
    if (numbers) t = _percent(t);
    if (spokenPunctuation) t = _spokenPunctuation(t);
    if (numbers) t = _numbersToDigits(t);
    if (removeFillers) t = _removeFillers(t);
    if (spanishOrthography) t = _orthography(t);
    return t;
  }

  // ─── Puntuación dictada ────────────────────────────────────────────────────
  // Solo los comandos de baja colisión van SIEMPRE. "punto"/"coma" como palabra
  // suelta solo se interpretan al FINAL del texto/línea (evita "punto de venta").

  static String _spokenPunctuation(String text) {
    var t = text;
    // Multi-palabra y delimitadores (poco colisionables): reemplazo por token.
    const map = <String, String>{
      'nuevo párrafo': '\n\n',
      'nuevo parrafo': '\n\n',
      'nueva línea': '\n',
      'nueva linea': '\n',
      'salto de línea': '\n',
      'salto de linea': '\n',
      'punto y coma': ';',
      'dos puntos': ':',
      'puntos suspensivos': '…',
      'abre paréntesis': '(',
      'abre parentesis': '(',
      'abrir paréntesis': '(',
      'abrir parentesis': '(',
      'cierra paréntesis': ')',
      'cierra parentesis': ')',
      'cerrar paréntesis': ')',
      'cerrar parentesis': ')',
      'abre comillas': '"',
      'abrir comillas': '"',
      'cierra comillas': '"',
      'cerrar comillas': '"',
      'signo de interrogación': '?',
      'signo de interrogacion': '?',
      'signo de exclamación': '!',
      'signo de exclamacion': '!',
    };
    map.forEach((phrase, symbol) {
      t = t.replaceAll(
        RegExp(
          r'(?<![\wáéíóúñü])' + RegExp.escape(phrase) + r'(?![\wáéíóúñü])',
          caseSensitive: false,
        ),
        symbol,
      );
    });
    // "punto"/"coma" sueltos SOLO si cierran el texto o la línea (caso seguro:
    // "...la reunión punto" -> "...la reunión.").
    t = t.replaceAll(
      RegExp(r'\s+punto\s*(?=$|\n)', caseSensitive: false),
      '.',
    );
    t = t.replaceAll(
      RegExp(r'\s+coma\s*(?=$|\n)', caseSensitive: false),
      ',',
    );
    return t;
  }

  // ─── Porcentaje ────────────────────────────────────────────────────────────
  // Antes de convertir números, para que "por ciento" no se parta en "por 100".

  static String _percent(String text) => text.replaceAll(
        RegExp(r'\bpor\s+ciento\b|\bporciento\b', caseSensitive: false),
        '%',
      );

  // ─── Números en palabras -> dígitos (ITN español, conservador) ──────────────

  static const Map<String, int> _numWords = {
    'cero': 0,
    'uno': 1,
    'una': 1,
    'dos': 2,
    'tres': 3,
    'cuatro': 4,
    'cinco': 5,
    'seis': 6,
    'siete': 7,
    'ocho': 8,
    'nueve': 9,
    'diez': 10,
    'once': 11,
    'doce': 12,
    'trece': 13,
    'catorce': 14,
    'quince': 15,
    'dieciséis': 16,
    'dieciseis': 16,
    'diecisiete': 17,
    'dieciocho': 18,
    'diecinueve': 19,
    'veinte': 20,
    'veintiuno': 21,
    'veintiún': 21,
    'veintiun': 21,
    'veintiuna': 21,
    'veintidós': 22,
    'veintidos': 22,
    'veintitrés': 23,
    'veintitres': 23,
    'veinticuatro': 24,
    'veinticinco': 25,
    'veintiséis': 26,
    'veintiseis': 26,
    'veintisiete': 27,
    'veintiocho': 28,
    'veintinueve': 29,
    'treinta': 30,
    'cuarenta': 40,
    'cincuenta': 50,
    'sesenta': 60,
    'setenta': 70,
    'ochenta': 80,
    'noventa': 90,
    'cien': 100,
    'ciento': 100,
    'doscientos': 200,
    'doscientas': 200,
    'trescientos': 300,
    'trescientas': 300,
    'cuatrocientos': 400,
    'cuatrocientas': 400,
    'quinientos': 500,
    'quinientas': 500,
    'seiscientos': 600,
    'seiscientas': 600,
    'setecientos': 700,
    'setecientas': 700,
    'ochocientos': 800,
    'ochocientas': 800,
    'novecientos': 900,
    'novecientas': 900,
    'mil': 1000,
    'millón': 1000000,
    'millon': 1000000,
    'millones': 1000000,
  };

  /// Palabras ambiguas: solo cuentan como número dentro de una secuencia más
  /// larga (evita convertir el artículo "un/una/uno" suelto).
  static const Set<String> _ambiguousSingles = {'uno', 'una', 'un'};

  static final RegExp _numberPhrase = () {
    final alt = (_numWords.keys.toList()..sort((a, b) => b.length - a.length))
        .map(RegExp.escape)
        .join('|');
    // Una frase = palabra-número, seguida de más palabras-número unidas por
    // espacios y un "y" opcional. Las fronteras (?<!..)/(?!..) no consumen los
    // espacios EXTERIORES, así que el texto de alrededor se conserva.
    return RegExp(
      r'(?<![\wáéíóúñü])((?:' +
          alt +
          r')(?:\s+(?:y\s+)?(?:' +
          alt +
          r'))*)(?![\wáéíóúñü])',
      caseSensitive: false,
    );
  }();

  static String _numbersToDigits(String text) {
    return text.replaceAllMapped(_numberPhrase, (m) {
      final phrase = m.group(1)!;
      final words = phrase
          .toLowerCase()
          .split(RegExp(r'\s+'))
          .where((w) => w.isNotEmpty && w != 'y')
          .toList();
      if (words.isEmpty || !words.every(_numWords.containsKey)) return phrase;
      // No conviertas el artículo suelto "un/una/uno".
      if (words.length == 1 && _ambiguousSingles.contains(words.first)) {
        return phrase;
      }
      return _runToNumber(words).toString();
    });
  }

  /// Convierte una secuencia de palabras-número (sin "y") a su valor entero.
  static int _runToNumber(List<String> words) {
    var total = 0;
    var current = 0;
    for (final w in words) {
      final v = _numWords[w]!;
      if (v == 1000000) {
        current = (current == 0 ? 1 : current) * 1000000;
        total += current;
        current = 0;
      } else if (v == 1000) {
        current = (current == 0 ? 1 : current) * 1000;
        total += current;
        current = 0;
      } else if (v == 100) {
        current = (current == 0 ? 1 : current) * 100;
      } else {
        current += v;
      }
    }
    return total + current;
  }

  // ─── Muletillas (opt-in) ────────────────────────────────────────────────────

  static String _removeFillers(String text) {
    var t = text;
    // Solo interjecciones aisladas que casi nunca tienen sentido escrito.
    for (final f in ['eh', 'ehh', 'em', 'emm', 'mmm', 'mm']) {
      t = t.replaceAll(
        RegExp(
          r'(?<![\wáéíóúñü])' + f + r'(?![\wáéíóúñü])\s*,?\s*',
          caseSensitive: false,
        ),
        '',
      );
    }
    return t;
  }

  // ─── Ortografía española ────────────────────────────────────────────────────

  static String _orthography(String text) {
    var t = text;
    // 1a) Sin espacio antes de , . ; : ? ! ) …
    t = t.replaceAllMapped(RegExp(r'[ \t]+([,.;:?!)…])'), (m) => m.group(1)!);
    // 1b) Un espacio tras , ; :  — NO tras "." (rompería dominios tipo
    //     github.com) ni si sigue dígito (15:30, 3,5).
    t = t.replaceAllMapped(
      RegExp(r'([,;:])(?=[^\s\d\n)])'),
      (m) => '${m.group(1)} ',
    );
    // 1c) Sin espacio tras "(".
    t = t.replaceAll(RegExp(r'\(\s+'), '(');
    // 1d) Limpia espacios alrededor de saltos y colapsa espacios repetidos.
    t = t.replaceAll(RegExp(r'[ \t]*\n[ \t]*'), '\n');
    t = t.replaceAll(RegExp(r'[ \t]{2,}'), ' ');

    // 2) Signos de apertura ¿ ¡ por enunciado terminado en ? o !.
    t = _openingMarks(t);

    // 3) Mayúscula inicial de cada enunciado (inicio, y tras . ? ! … o salto).
    t = _capitalizeSentences(t);

    return t.trim();
  }

  /// Inserta ¿ / ¡ al comienzo de cada enunciado que termina en ? / !.
  static String _openingMarks(String text) {
    return text.replaceAllMapped(
      // Una "frase" = run sin terminadores que acaba en ? o !.
      RegExp(r'([^.?!¿¡\n]*[?!])'),
      (m) {
        final phrase = m.group(1)!;
        final lead = RegExp(r'^\s*').firstMatch(phrase)!.group(0)!;
        final body = phrase.substring(lead.length);
        if (body.isEmpty) return phrase;
        if (body.startsWith('¿') || body.startsWith('¡')) return phrase;
        final open = body.endsWith('?') ? '¿' : '¡';
        return '$lead$open$body';
      },
    );
  }

  static String _capitalizeSentences(String text) {
    final chars = text.split('');
    var capNext = true;
    for (var i = 0; i < chars.length; i++) {
      final c = chars[i];
      if (capNext && RegExp(r'[A-Za-zÁÉÍÓÚáéíóúÑñÜü]').hasMatch(c)) {
        chars[i] = c.toUpperCase();
        capNext = false;
      } else if (c == '?' || c == '!' || c == '\n') {
        capNext = true;
      } else if (c == '.') {
        // Fin de frase SOLO si tras el punto hay espacio/salto o es el final;
        // así no capitaliza dominios/decimales (github.com, 3.5, v1.2).
        final next = i + 1 < chars.length ? chars[i + 1] : ' ';
        capNext = next == ' ' || next == '\t' || next == '\n';
      } else if (c == '¿' || c == '¡' || c == ' ' || c == '"' || c == '(') {
        // Mantén la intención de capitalizar tras estos.
      } else {
        capNext = false;
      }
    }
    return chars.join();
  }
}
