import 'dart:async';

import '../../platform/clipboard_watcher.dart';
import 'learning_service.dart';
import 'text_similarity.dart';

/// Captura de ediciones EXTERNAS (estilo Wispr Flow).
///
/// Tras insertar un dictado, se "arma" con (transcripción cruda, texto
/// insertado). Si dentro de una ventana de tiempo el usuario copia al
/// portapapeles una versión **parecida pero distinta** de lo que insertamos
/// (es decir, lo corrigió en su app y lo copió), lo interpretamos como una
/// corrección y se la pasamos al [LearningService].
///
/// Salvaguardas contra ruido (además de los filtros del propio
/// LearningService, que exige freq>=3 y alta similitud para activar una
/// corrección):
///  - Se ignora texto idéntico al insertado (incluye nuestro propio pegado).
///  - Solo se acepta si la distancia normalizada está en (0, [_maxDistance]]:
///    ni idéntico (0) ni demasiado distinto (copia no relacionada).
///  - Solo la PRIMERA captura válida tras cada dictado; luego se desarma.
///  - Caduca tras [_window].
class ExternalEditCapture {
  ExternalEditCapture(this._watcher, this._learning);

  final ClipboardWatcher _watcher;
  final LearningService _learning;

  static const Duration _window = Duration(seconds: 45);
  static const double _maxDistance = 0.4;

  bool _enabled = false;
  StreamSubscription<String>? _sub;
  Timer? _timeout;
  String? _raw;
  String? _inserted;

  void setEnabled(bool value) {
    _enabled = value;
    if (!value) disarm();
  }

  /// Arma la captura tras una inserción. No hace nada si está desactivada.
  void arm(String raw, String inserted) {
    if (!_enabled) return;
    _raw = raw;
    _inserted = inserted;
    _watcher.start();
    _sub ??= _watcher.changes.listen(_onClipboard);
    _timeout?.cancel();
    _timeout = Timer(_window, disarm);
  }

  Future<void> _onClipboard(String text) async {
    final raw = _raw;
    final inserted = _inserted;
    if (raw == null || inserted == null) return;

    final captured = text.trim();
    final base = inserted.trim();
    if (captured.isEmpty || captured == base) return;

    final dist = normalizedLevenshtein(base.toLowerCase(), captured.toLowerCase());
    if (dist <= 0 || dist > _maxDistance) return; // idéntico o no relacionado

    await _learning.recordEdit(raw, captured);
    await _learning.refresh();
    disarm();
  }

  void disarm() {
    _raw = null;
    _inserted = null;
    _timeout?.cancel();
    _timeout = null;
  }

  void dispose() {
    _sub?.cancel();
    _sub = null;
    _timeout?.cancel();
    _watcher.stop();
  }
}
