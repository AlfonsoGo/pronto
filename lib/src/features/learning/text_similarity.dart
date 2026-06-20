import 'dart:math' as math;

/// Distancia de edición de Levenshtein entre dos cadenas.
int levenshtein(String a, String b) {
  if (a == b) return 0;
  if (a.isEmpty) return b.length;
  if (b.isEmpty) return a.length;

  final prev = List<int>.generate(b.length + 1, (i) => i);
  final curr = List<int>.filled(b.length + 1, 0);

  for (var i = 0; i < a.length; i++) {
    curr[0] = i + 1;
    for (var j = 0; j < b.length; j++) {
      final cost = a.codeUnitAt(i) == b.codeUnitAt(j) ? 0 : 1;
      curr[j + 1] = math.min(
        math.min(curr[j] + 1, prev[j + 1] + 1),
        prev[j] + cost,
      );
    }
    for (var j = 0; j <= b.length; j++) {
      prev[j] = curr[j];
    }
  }
  return prev[b.length];
}

/// Levenshtein normalizado a [0.0, 1.0]: 0 = idénticas, 1 = totalmente distintas.
double normalizedLevenshtein(String a, String b) {
  if (a.isEmpty && b.isEmpty) return 0.0;
  final maxLen = math.max(a.length, b.length);
  if (maxLen == 0) return 0.0;
  return levenshtein(a, b) / maxLen;
}

/// Similitud de Jaro entre dos cadenas, en [0.0, 1.0].
double jaro(String s1, String s2) {
  if (s1 == s2) return 1.0;
  if (s1.isEmpty || s2.isEmpty) return 0.0;

  final matchDistance = (math.max(s1.length, s2.length) ~/ 2) - 1;
  final s1Matches = List<bool>.filled(s1.length, false);
  final s2Matches = List<bool>.filled(s2.length, false);

  var matches = 0;
  for (var i = 0; i < s1.length; i++) {
    final start = math.max(0, i - matchDistance);
    final end = math.min(i + matchDistance + 1, s2.length);
    for (var j = start; j < end; j++) {
      if (s2Matches[j]) continue;
      if (s1.codeUnitAt(i) != s2.codeUnitAt(j)) continue;
      s1Matches[i] = true;
      s2Matches[j] = true;
      matches++;
      break;
    }
  }
  if (matches == 0) return 0.0;

  var transpositions = 0;
  var k = 0;
  for (var i = 0; i < s1.length; i++) {
    if (!s1Matches[i]) continue;
    while (!s2Matches[k]) {
      k++;
    }
    if (s1.codeUnitAt(i) != s2.codeUnitAt(k)) transpositions++;
    k++;
  }
  final m = matches.toDouble();
  return ((m / s1.length) + (m / s2.length) + ((m - transpositions / 2) / m)) /
      3.0;
}

/// Similitud de Jaro-Winkler en [0.0, 1.0] (favorece prefijos comunes).
double jaroWinkler(String s1, String s2, {double prefixScale = 0.1}) {
  final j = jaro(s1, s2);
  var prefix = 0;
  final maxPrefix = math.min(4, math.min(s1.length, s2.length));
  for (var i = 0; i < maxPrefix; i++) {
    if (s1.codeUnitAt(i) == s2.codeUnitAt(i)) {
      prefix++;
    } else {
      break;
    }
  }
  return j + (prefix * prefixScale * (1 - j));
}
