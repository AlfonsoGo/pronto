/// Un par de corrección aprendido: `raw` (mal transcrito, en minúsculas) ->
/// `corrected` (forma correcta, respeta mayúsculas del usuario).
class CorrectionEntry {
  final String raw;
  final String corrected;
  final int freq;

  const CorrectionEntry({
    required this.raw,
    required this.corrected,
    required this.freq,
  });
}

/// Persistencia del perfil de aprendizaje (correcciones + vocabulario + log).
///
/// Implementación con SQLite: [SqliteLearningRepository] (ver data/).
/// En tests se usa una implementación en memoria.
abstract class LearningRepository {
  /// Incrementa (o crea) la frecuencia del par [raw] -> [corrected].
  Future<void> bumpCorrection(String raw, String corrected);

  /// Devuelve las correcciones activas (freq >= [minFreq]).
  Future<List<CorrectionEntry>> activeCorrections({int minFreq = 3});

  /// Incrementa (o crea) la frecuencia de un término de vocabulario propio
  /// (nombres propios, jerga) para alimentar el initial_prompt.
  Future<void> bumpVocab(String term);

  /// Términos de vocabulario más frecuentes (orden descendente por freq).
  Future<List<String>> topVocab({int limit = 45});

  /// Registra un dictado para auditoría/aprendizaje.
  Future<void> logDictation({
    required String raw,
    required String finalText,
    required double avgLogProb,
  });
}
