/// Tipo de operación en el alineamiento palabra a palabra.
enum DiffOpType { equal, substitute, insert, delete }

/// Una operación del alineamiento entre la transcripción cruda y el texto
/// final editado por el usuario.
class DiffOp {
  final DiffOpType type;

  /// Token de la transcripción cruda (null en `insert`).
  final String? source;

  /// Token del texto editado (null en `delete`).
  final String? target;

  const DiffOp(this.type, {this.source, this.target});

  @override
  String toString() => '${type.name}(${source ?? '∅'} -> ${target ?? '∅'})';
}

/// Tokeniza por espacios en blanco (mantiene la puntuación adherida; los
/// filtros de [LearningService] la normalizan después).
List<String> tokenizeWords(String text) {
  final trimmed = text.trim();
  if (trimmed.isEmpty) return const [];
  return trimmed.split(RegExp(r'\s+'));
}

/// Alinea dos secuencias de palabras con Needleman-Wunsch y devuelve la lista
/// de operaciones (equal/substitute/insert/delete).
///
/// Costes: igualdad 0, sustitución 1, hueco (insert/delete) 1.
List<DiffOp> alignWords(List<String> a, List<String> b) {
  final n = a.length;
  final m = b.length;

  // Matriz de costes (n+1) x (m+1).
  final dp = List.generate(n + 1, (_) => List<int>.filled(m + 1, 0));
  for (var i = 0; i <= n; i++) {
    dp[i][0] = i;
  }
  for (var j = 0; j <= m; j++) {
    dp[0][j] = j;
  }

  for (var i = 1; i <= n; i++) {
    for (var j = 1; j <= m; j++) {
      final cost = a[i - 1] == b[j - 1] ? 0 : 1;
      final diag = dp[i - 1][j - 1] + cost;
      final up = dp[i - 1][j] + 1; // delete a[i-1]
      final left = dp[i][j - 1] + 1; // insert b[j-1]
      dp[i][j] = diag < up
          ? (diag < left ? diag : left)
          : (up < left ? up : left);
    }
  }

  // Backtrack desde (n, m) hasta (0, 0).
  final ops = <DiffOp>[];
  var i = n;
  var j = m;
  while (i > 0 || j > 0) {
    if (i > 0 && j > 0) {
      final cost = a[i - 1] == b[j - 1] ? 0 : 1;
      if (dp[i][j] == dp[i - 1][j - 1] + cost) {
        ops.add(cost == 0
            ? DiffOp(DiffOpType.equal, source: a[i - 1], target: b[j - 1])
            : DiffOp(DiffOpType.substitute,
                source: a[i - 1], target: b[j - 1],),);
        i--;
        j--;
        continue;
      }
    }
    if (i > 0 && dp[i][j] == dp[i - 1][j] + 1) {
      ops.add(DiffOp(DiffOpType.delete, source: a[i - 1]));
      i--;
      continue;
    }
    // insert
    ops.add(DiffOp(DiffOpType.insert, target: b[j - 1]));
    j--;
  }

  return ops.reversed.toList();
}

/// Fracción de operaciones que NO son `equal` (proxy de cuánto cambió el texto).
double changeRatio(List<DiffOp> ops) {
  if (ops.isEmpty) return 0.0;
  final changed = ops.where((o) => o.type != DiffOpType.equal).length;
  return changed / ops.length;
}
