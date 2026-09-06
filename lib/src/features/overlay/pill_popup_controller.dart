import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../translation/language_detect.dart';

/// Qué está enseñando la cajita de la píldora.
enum PillPopupKind {
  /// La transcripción recién terminada.
  transcription,

  /// Confirmación de "Copiado".
  copied,

  /// Un aviso corto del sistema (p. ej. el cambio de idioma de salida).
  notice,
}

/// Estado de la cajita que aparece a la derecha del punto (píldora) con la
/// última transcripción, y que también confirma el "Copiado".
@immutable
class PillPopup {
  /// Si la caja debe mostrarse.
  final bool visible;

  /// Qué está mostrando.
  final PillPopupKind kind;

  /// Texto principal: lo que se ha pegado (en el idioma de salida).
  final String text;

  /// Idioma de [text]; `null` si la traducción está desactivada o no se supo.
  final TextLanguage? lang;

  /// La OTRA versión del mismo dictado (traducción u original). Vacío si no la
  /// hay todavía.
  final String altText;

  /// Idioma de [altText].
  final TextLanguage? altLang;

  /// La otra versión se está traduciendo en segundo plano (spinner en la
  /// insignia).
  final bool altPending;

  /// Aviso corto del sistema que acompaña al texto (p. ej. «Sin formatear: el
  /// servidor local no responde»). Vacío cuando no hay nada que decir.
  ///
  /// Es SIEMPRE una cadena nuestra: aquí no entra jamás un trozo de la
  /// respuesta de un modelo.
  final String notice;

  /// Contador que sube en cada aparición: sirve de `key` para re-disparar la
  /// animación de entrada aunque la caja ya estuviera visible.
  final int seq;

  const PillPopup({
    this.visible = false,
    this.kind = PillPopupKind.transcription,
    this.text = '',
    this.lang,
    this.altText = '',
    this.altLang,
    this.altPending = false,
    this.notice = '',
    this.seq = 0,
  });

  /// `true` si hay que dibujar la insignia ES/EN de la otra versión.
  bool get hasAlt => altText.isNotEmpty || altPending;

  PillPopup copyWith({
    bool? visible,
    PillPopupKind? kind,
    String? text,
    TextLanguage? lang,
    String? altText,
    TextLanguage? altLang,
    bool? altPending,
    String? notice,
    int? seq,
  }) {
    return PillPopup(
      visible: visible ?? this.visible,
      kind: kind ?? this.kind,
      text: text ?? this.text,
      lang: lang ?? this.lang,
      altText: altText ?? this.altText,
      altLang: altLang ?? this.altLang,
      altPending: altPending ?? this.altPending,
      notice: notice ?? this.notice,
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
  ///
  /// [altText]/[altLang] son la otra versión del par español-inglés; si aún se
  /// está traduciendo en segundo plano se pasa [altPending] y luego se completa
  /// con [completeAlt].
  void showTranscription(
    String text, {
    TextLanguage? lang,
    String altText = '',
    TextLanguage? altLang,
    bool altPending = false,
    String? notice,
  }) {
    _show(
      text,
      kind: PillPopupKind.transcription,
      lang: lang,
      altText: altText,
      altLang: altLang,
      altPending: altPending,
      notice: notice,
    );
  }

  /// Confirma que se ha copiado un texto. [lang] es el idioma de lo copiado,
  /// para que la caja lo etiquete.
  void showCopied(
    String text, {
    TextLanguage? lang,
    String altText = '',
    TextLanguage? altLang,
    bool altPending = false,
    String? notice,
  }) {
    _show(
      text,
      kind: PillPopupKind.copied,
      lang: lang,
      altText: altText,
      altLang: altLang,
      altPending: altPending,
      notice: notice,
    );
  }

  /// Aviso corto del sistema (cambio de idioma de salida, traducción no
  /// disponible…). No tiene "otra versión" ni acción de copiar.
  void showNotice(String message) => _show(message, kind: PillPopupKind.notice);

  /// Completa la otra versión cuando termina la traducción en segundo plano.
  ///
  /// No reabre la caja si ya se ocultó (sería un pop fantasma varios segundos
  /// después del dictado); si sigue visible, reinicia la cuenta atrás para dar
  /// tiempo a leer la traducción recién llegada.
  void completeAlt(String altText, TextLanguage altLang) {
    if (!state.visible || !state.altPending) {
      state = state.copyWith(altPending: false);
      return;
    }
    state = state.copyWith(
      altText: altText,
      altLang: altLang,
      altPending: false,
    );
    _timer?.cancel();
    _timer = Timer(_visibleFor, hide);
  }

  /// La traducción en segundo plano falló: quitamos el spinner y dejamos la
  /// caja con lo que ya tenía (nunca empeoramos lo que ve el usuario).
  void failAlt() {
    if (!state.altPending) return;
    state = state.copyWith(altPending: false);
  }

  void _show(
    String text, {
    required PillPopupKind kind,
    TextLanguage? lang,
    String altText = '',
    TextLanguage? altLang,
    bool altPending = false,
    String? notice,
  }) {
    final t = text.trim();
    if (t.isEmpty) return;
    _timer?.cancel();
    // Estado nuevo COMPLETO (no copyWith): así la otra versión del dictado
    // anterior —ni su aviso— se cuela en este.
    state = PillPopup(
      visible: true,
      kind: kind,
      text: t,
      lang: lang,
      altText: altText.trim(),
      altLang: altLang,
      altPending: altPending,
      notice: notice?.trim() ?? '',
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
