/// Limpieza de la salida cruda de un LLM.
///
/// Los modelos pequeños suelen envolver la respuesta en comillas o emitir
/// bloques de razonamiento (`<think>…</think>`, típico de qwen3). Esta función
/// deja solo el texto útil. La comparten la post-corrección y la traducción.
String cleanModelOutput(String raw) {
  var out = raw.trim();

  // Elimina bloques de razonamiento si el modelo los emitiera.
  out = out.replaceAll(RegExp(r'<think>.*?</think>', dotAll: true), '');
  out = out.trim();

  // Quita comillas envolventes si las hubiera.
  if (out.length >= 2) {
    final first = out[0];
    final last = out[out.length - 1];
    const quotes = {'"', "'", '«', '“', '”', '‘', '’'};
    if ((first == '"' && last == '"') ||
        (first == "'" && last == "'") ||
        (first == '«' && last == '»') ||
        (quotes.contains(first) && quotes.contains(last) && first != last)) {
      out = out.substring(1, out.length - 1).trim();
    }
  }
  return out;
}
