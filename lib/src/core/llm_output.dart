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

/// Deja la salida de un modelo en condiciones de PEGARSE en la app con foco.
///
/// Lo que se pega no se lee: se teclea. El inyector de Windows convierte un
/// salto de línea en un **Intro de verdad**, así que un modelo descarrilado —o
/// un proveedor comprometido— podría devolver un salto de línea seguido de una
/// orden, y ese Intro se pulsaría solo en el terminal o el chat que tuviera el
/// foco. Formatear un
/// dictado nunca necesita meter saltos de línea, así que se colapsan.
///
/// Se quitan además los caracteres que no se ven pero viajan: controles C0/C1,
/// anchura cero y marcas bidireccionales, que sirven para esconder texto dentro
/// de texto.
String sanitizeForInjection(String texto) {
  final buffer = StringBuffer();
  for (final r in texto.runes) {
    if (r == 0x0A || r == 0x0D || r == 0x09) {
      buffer.write(' '); // salto o tabulador -> espacio, nunca una tecla
      continue;
    }
    final invisible = (r >= 0x200B && r <= 0x200F) || // anchura cero y marcas
        (r >= 0x202A && r <= 0x202E) || // anulación bidireccional
        (r >= 0x2066 && r <= 0x2069) || // aislamiento bidireccional
        (r >= 0x2060 && r <= 0x2064) ||
        r == 0x061C ||
        r == 0xFEFF;
    if (invisible) continue;
    final control = r < 0x20 || (r >= 0x7F && r <= 0x9F);
    if (control) continue;
    buffer.writeCharCode(r);
  }
  // Los espacios que dejan los saltos colapsan: el texto se pega tal cual.
  return buffer.toString().replaceAll(RegExp(r' {2,}'), ' ').trim();
}
