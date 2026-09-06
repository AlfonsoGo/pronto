/// Funciones propias que se registran en cada conexión a `pronto.db`.
///
/// Solo hay una y existe por una razón concreta: **buscar en español**. El
/// `LOWER` de SQLite es ASCII, así que «ÓRDENES» no baja a «órdenes» y una
/// búsqueda por «ordenes» no encuentra «reunión», «Serviform» ni nada con
/// tilde. En una app donde el 55% de los fallos son tildes, una búsqueda que
/// se atasca justo ahí no sirve.
library;

import 'package:sqlite3/sqlite3.dart';

/// Pasa un texto a su forma de búsqueda: minúsculas y sin diacríticos.
///
/// La ñ también se pliega a n. Es lo normal en un buscador —quien escribe
/// «ano» buscando «año» encuentra— y el precio es que las dos se cruzan.
String foldForSearch(String text) {
  final buffer = StringBuffer();
  for (final rune in text.toLowerCase().runes) {
    buffer.write(_folded[rune] ?? String.fromCharCode(rune));
  }
  return buffer.toString();
}

const Map<int, String> _folded = {
  0xE1: 'a', // á
  0xE0: 'a', // à
  0xE4: 'a', // ä
  0xE2: 'a', // â
  0xE9: 'e', // é
  0xE8: 'e', // è
  0xEB: 'e', // ë
  0xEA: 'e', // ê
  0xED: 'i', // í
  0xEC: 'i', // ì
  0xEF: 'i', // ï
  0xEE: 'i', // î
  0xF3: 'o', // ó
  0xF2: 'o', // ò
  0xF6: 'o', // ö
  0xF4: 'o', // ô
  0xFA: 'u', // ú
  0xF9: 'u', // ù
  0xFC: 'u', // ü
  0xFB: 'u', // û
  0xF1: 'n', // ñ
  0xE7: 'c', // ç
};

/// Registra `pronto_fold(texto)` en [db].
///
/// Se llama en CADA apertura, incluidas las de los tests: una consulta que la
/// use contra una conexión donde no esté registrada falla en tiempo de
/// ejecución, no de compilación.
void registerProntoFunctions(Database db) {
  db.createFunction(
    functionName: 'pronto_fold',
    argumentCount: const AllowedArgumentCount(1),
    deterministic: true,
    directOnly: false,
    function: (args) {
      final value = args.first;
      if (value is! String) return null;
      return foldForSearch(value);
    },
  );
}
