/// Idiomas entre los que Pronto sabe traducir.
enum TextLanguage {
  es,
  en;

  /// Código ISO en minúsculas ('es' / 'en').
  String get code => name;

  /// Etiqueta corta para la insignia de la píldora ('ES' / 'EN').
  String get badge => name.toUpperCase();

  /// Nombre en español, para textos de interfaz.
  String get label => this == TextLanguage.es ? 'Español' : 'Inglés';

  /// Nombre en inglés, para el prompt del traductor.
  String get englishName => this == TextLanguage.es ? 'Spanish' : 'English';

  /// El otro idioma del par (es ⇄ en).
  TextLanguage get other =>
      this == TextLanguage.es ? TextLanguage.en : TextLanguage.es;

  /// Reconstruye desde un código persistido; `null` si no se reconoce.
  static TextLanguage? fromCode(String? code) {
    for (final v in TextLanguage.values) {
      if (v.code == code) return v;
    }
    return null;
  }
}

/// Detecta si un texto está en español o en inglés.
///
/// Es una heurística DETERMINISTA y local (nada de LLM ni de red): cuenta
/// palabras funcionales exclusivas de cada idioma y suma un empujón por los
/// caracteres que solo existen en español (á é í ó ú ñ ü ¿ ¡). Basta para el
/// único uso que le damos: decidir hacia qué lado traducir un dictado.
///
/// Devuelve `null` cuando no hay señal suficiente (texto muy corto, una sola
/// palabra, cifras, nombres propios…). Quien llama decide el desempate; en
/// Pronto se cae al idioma de dictado configurado.
TextLanguage? detectLanguage(String text) {
  final words = _tokenize(text);
  if (words.isEmpty) return null;

  var es = 0;
  var en = 0;
  for (final w in words) {
    if (_esStopwords.contains(w)) es++;
    if (_enStopwords.contains(w)) en++;
  }

  // Los caracteres propios del español son una señal fortísima: valen por dos
  // palabras funcionales cada uno (hasta un tope, para que un texto inglés con
  // un "café" suelto no bascule entero).
  final marks = _spanishMarks.allMatches(text).length;
  es += (marks * 2).clamp(0, 6);

  if (es == en) return null;
  return es > en ? TextLanguage.es : TextLanguage.en;
}

/// Caracteres que solo aparecen en español dentro del par es/en.
final RegExp _spanishMarks = RegExp('[áéíóúñüÁÉÍÓÚÑÜ¿¡]');

/// Parte el texto en palabras en minúsculas, sin puntuación ni cifras. Se
/// conserva el apóstrofo INTERIOR (para "don't", "it's") pero no el de los
/// bordes, que viene de comillas simples.
List<String> _tokenize(String text) {
  return text
      .toLowerCase()
      .split(RegExp(r"[^a-záéíóúñü']+", unicode: true))
      .map((w) => w.replaceAll(RegExp(r"^'+|'+$"), ''))
      .where((w) => w.isNotEmpty)
      .toList(growable: false);
}

/// Palabras funcionales frecuentes en español que NO son palabras inglesas.
/// (Se excluyen a propósito las ambiguas: "no", "a", "e", "son", "van"…)
const Set<String> _esStopwords = {
  'el', 'la', 'los', 'las', 'un', 'una', 'unos', 'unas', 'del', 'al',
  'que', 'qué', 'de', 'en', 'y', 'o', 'pero', 'porque', 'como', 'cómo',
  'con', 'sin', 'por', 'para', 'sobre', 'entre', 'hasta', 'desde',
  'es', 'está', 'están', 'era', 'ser', 'estar', 'hay', 'tiene', 'tengo',
  'esto', 'esta', 'este', 'estos', 'estas', 'eso', 'esa', 'ese',
  'se', 'lo', 'le', 'les', 'nos', 'te', 'mi', 'tu', 'su', 'sus', 'mis',
  'yo', 'tú', 'él', 'ella', 'nosotros', 'vosotros', 'ellos', 'ellas',
  'muy', 'más', 'menos', 'también', 'todo', 'toda', 'todos', 'todas',
  'cuando', 'cuándo', 'donde', 'dónde', 'quien', 'quién', 'cual', 'cuál',
  'hola', 'gracias', 'buenos', 'buenas', 'señor', 'señora', 'vale',
  'ahora', 'luego', 'siempre', 'nunca', 'nada', 'algo', 'alguien',
  'puedo', 'puede', 'quiero', 'quiere', 'hacer', 'hace', 'dice', 'decir',
};

/// Palabras funcionales frecuentes en inglés que NO son palabras españolas.
/// (Se excluye "a", que es también preposición española.)
const Set<String> _enStopwords = {
  'the', 'an', 'and', 'or', 'but', 'because', 'so', 'if',
  'of', 'to', 'in', 'on', 'at', 'for', 'with', 'without', 'from', 'by',
  'about', 'into', 'over', 'under', 'between', 'until', 'while',
  'is', 'are', 'was', 'were', 'be', 'been', 'being', 'am',
  'have', 'has', 'had', 'do', 'does', 'did', 'will', 'would', 'can',
  'could', 'should', 'must', 'may', 'might',
  'this', 'that', 'these', 'those', 'it', 'its',
  'i', 'you', 'he', 'she', 'we', 'they', 'him', 'her', 'them', 'their',
  'my', 'your', 'our', 'his',
  'not', "don't", "doesn't", "isn't", "can't", "it's", "i'm",
  'what', 'when', 'where', 'who', 'which', 'how', 'why',
  'very', 'more', 'less', 'also', 'all', 'some', 'any', 'every',
  'hello', 'thanks', 'thank', 'please', 'sorry', 'yes',
  'now', 'then', 'always', 'never', 'nothing', 'something', 'someone',
  'want', 'need', 'make', 'makes', 'says', 'say', 'get', 'got',
};
