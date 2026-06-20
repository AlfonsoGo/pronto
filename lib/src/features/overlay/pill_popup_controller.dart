import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

/// Estado de la cajita que aparece a la derecha del punto (píldora) con la
/// última transcripción, y que también confirma el "Copiado".
@immutable
class PillPopup {
  /// Si la caja debe mostrarse.
  final bool visible;

  /// Texto a mostrar (la transcripción).
  final String text;

  /// Si el motivo es "se acaba de copiar" (muestra el check + "Copiado").
  final bool copied;

  /// Contador que sube en cada aparición: sirve de `key` para re-disparar la
  /// animación de entrada aunque la caja ya estuviera visible.
  final int seq;

  const PillPopup({
    this.visible = false,
    this.text = '',
    this.copied = false,
    this.seq = 0,
  });

  PillPopup copyWith({bool? visible, String? text, bool? copied, int? seq}) {
    return PillPopup(
      visible: visible ?? this.visible,
      text: text ?? this.text,
      copied: copied ?? this.copied,
      seq: seq ?? this.seq,
    );
  }
}

final pillPopupProvider =
    NotifierProvider<PillPopupController, PillPopup>(PillPopupController.new);

class PillPopupController extends Notifier<PillPopup> {
  Timer? _timer;

  /// Cuánto tiempo permanece visible la caja antes de auto-ocultarse.
  static const Duration _visibleFor = Duration(seconds: 4);

  @override
  PillPopup build() {
    ref.onDispose(() => _timer?.cancel());
    return const PillPopup();
  }

  /// Muestra la transcripción recién terminada.
  void showTranscription(String text) => _show(text, copied: false);

  /// Confirma que se ha copiado la transcripción.
  void showCopied(String text) => _show(text, copied: true);

  void _show(String text, {required bool copied}) {
    final t = text.trim();
    if (t.isEmpty) return;
    _timer?.cancel();
    state = state.copyWith(
      visible: true,
      text: t,
      copied: copied,
      seq: state.seq + 1,
    );
    _timer = Timer(_visibleFor, hide);
  }

  /// Pausa el auto-ocultado: mientras el ratón está encima de la caja leyendo,
  /// no debe desaparecer.
  void pauseAutoHide() => _timer?.cancel();

  /// Reanuda el auto-ocultado: reinicia la cuenta atrás si la caja sigue
  /// visible (p. ej. al apartar el ratón de la caja).
  void resumeAutoHide() {
    _timer?.cancel();
    if (state.visible) _timer = Timer(_visibleFor, hide);
  }

  void hide() {
    _timer?.cancel();
    if (state.visible) state = state.copyWith(visible: false);
  }
}
