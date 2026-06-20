import '../../core/config.dart';
import 'learning_repository.dart';
import 'text_similarity.dart';
import 'word_diff.dart';

/// Stopwords en español: cambios sobre estas palabras no son "correcciones de
/// vocabulario" (suelen ser reescrituras o ajustes gramaticales del LLM).
const Set<String> _spanishStopwords = {
  'el', 'la', 'los', 'las', 'un', 'una', 'unos', 'unas', 'y', 'o', 'u', 'de',
  'del', 'a', 'al', 'en', 'que', 'se', 'su', 'sus', 'lo', 'le', 'les', 'me',
  'te', 'nos', 'es', 'son', 'mi', 'tu', 'con', 'por', 'para', 'como', 'mas',
  'más', 'pero', 'si', 'sí', 'no', 'ya', 'muy', 'esta', 'este', 'esto', 'eso',
  'esa', 'ese', 'he', 'ha', 'han', 'hay', 'fue', 'era',
};

/// Sistema de automejora de Pronto.
///
/// 1. [recordEdit]: aprende de las diferencias entre la transcripción cruda y
///    el texto que el usuario dejó (extrae pares de corrección con filtros
///    anti-falsos-positivos y registra vocabulario propio).
/// 2. [applyDictionary]: aplica los reemplazos aprendidos (whole-word,
///    respetando mayúsculas) — determinista, instantáneo, cero alucinación.
/// 3. [buildInitialPrompt]: arma el prompt de biasing para Whisper con tu
///    vocabulario más frecuente.
class LearningService {
  LearningService(this._repo);

  final LearningRepository _repo;

  // Caché en memoria (refrescada desde el repositorio).
  Map<String, String> _activeMap = {};
  List<String> _vocab = const [];

  /// Recarga correcciones activas y vocabulario desde el repositorio.
  Future<void> refresh() async {
    final corr = await _repo.activeCorrections(
      minFreq: AppConfig.minCorrectionFreq,
    );
    _activeMap = {for (final c in corr) c.raw.toLowerCase(): c.corrected};
    _vocab = await _repo.topVocab(limit: AppConfig.initialPromptMaxTerms);
  }

  /// Aprende de una edición del usuario.
  ///
  /// [raw] = lo que dijo Whisper; [edited] = lo que el usuario dejó tras editar.
  Future<void> recordEdit(
    String raw,
    String edited, {
    double avgLogProb = 0.0,
  }) async {
    await _repo.logDictation(
      raw: raw,
      finalText: edited,
      avgLogProb: avgLogProb,
    );

    final a = tokenizeWords(raw);
    final b = tokenizeWords(edited);
    if (a.isEmpty || b.isEmpty) return;

    final ops = alignWords(a, b);

    // Si el usuario reescribió casi todo, no es un mapa de correcciones fiable.
    if (changeRatio(ops) > AppConfig.maxRewriteRatio) return;

    for (final op in ops) {
      if (op.type != DiffOpType.substitute) continue;
      final pair = _extractCorrection(op.source!, op.target!);
      if (pair == null) continue;
      await _repo.bumpCorrection(pair.$1, pair.$2);
      // El destino correcto alimenta el vocabulario para el biasing.
      await _repo.bumpVocab(pair.$2);
    }
  }

  /// Valida un par (origen crudo, destino editado) y devuelve
  /// `(rawNormalizado, corregido)` o null si no es una corrección legítima.
  (String, String)? _extractCorrection(String source, String target) {
    final src = _normalizeToken(source);
    final tgt = _stripEdgePunct(target);
    if (src.isEmpty || tgt.isEmpty) return null;

    final tgtLower = tgt.toLowerCase();

    // Solo cambia mayúsculas/acentos/puntuación => trabajo del LLM, no del dict.
    if (src == tgtLower) return null;

    // Palabras vacías o números: no son vocabulario propio.
    if (_spanishStopwords.contains(src)) return null;
    if (RegExp(r'^\d+$').hasMatch(src)) return null;

    // ¿Es "la misma palabra mal oída" o una reescritura semántica?
    final dist = normalizedLevenshtein(src, tgtLower);
    final jw = jaroWinkler(src, tgtLower);
    final samishWord = dist <= AppConfig.maxCorrectionDistance || jw >= 0.7;
    if (!samishWord) return null;

    return (src, tgt);
  }

  /// Aplica las correcciones aprendidas al [text] (whole-word, case-aware).
  String applyDictionary(String text) {
    if (_activeMap.isEmpty || text.isEmpty) return text;

    // \b no respeta acentos en Dart; usamos lookaround por carácter de palabra.
    return text.replaceAllMapped(
      RegExp(r'([A-Za-zÀ-ÿ0-9]+)'),
      (match) {
        final word = match.group(0)!;
        final replacement = _activeMap[word.toLowerCase()];
        if (replacement == null) return word;
        return _matchCase(word, replacement);
      },
    );
  }

  /// Genera el initial_prompt de biasing: vocabulario propio separado por
  /// comas, con los más importantes (frecuentes) AL FINAL (Whisper pesa más
  /// los últimos tokens). Truncado por longitud.
  String buildInitialPrompt({String? base}) {
    if (_vocab.isEmpty) return base ?? '';
    // _vocab viene en orden descendente por freq; invertimos para poner los
    // más frecuentes al final.
    final ordered = _vocab.reversed.toList();
    final buf = StringBuffer();
    if (base != null && base.isNotEmpty) {
      buf.write(base.trim());
      buf.write(' ');
    }
    final added = <String>[];
    for (final term in ordered) {
      final candidate = added.isEmpty ? term : '${buf.toString()}, $term';
      if (candidate.length > AppConfig.initialPromptMaxChars) break;
      if (added.isNotEmpty) buf.write(', ');
      buf.write(term);
      added.add(term);
    }
    return buf.toString();
  }

  // --- helpers ---

  static String _normalizeToken(String t) =>
      _stripEdgePunct(t).toLowerCase();

  static String _stripEdgePunct(String t) =>
      t.replaceAll(RegExp(r'''^[^\wÀ-ÿ]+|[^\wÀ-ÿ]+$'''), '');

  /// Ajusta las mayúsculas de [replacement] al patrón de [original].
  static String _matchCase(String original, String replacement) {
    if (original.isEmpty || replacement.isEmpty) return replacement;
    // TODO EN MAYÚSCULAS
    if (original == original.toUpperCase() && original.length > 1) {
      return replacement.toUpperCase();
    }
    // Capitalizada (Primera mayúscula)
    final firstIsUpper = original[0] == original[0].toUpperCase() &&
        original[0] != original[0].toLowerCase();
    if (firstIsUpper) {
      return replacement[0].toUpperCase() + replacement.substring(1);
    }
    return replacement;
  }
}
