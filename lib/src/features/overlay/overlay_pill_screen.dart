import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:window_manager/window_manager.dart';

import '../dictation/dictation_controller.dart';
import '../dictation/dictation_state.dart';
import '../settings/settings_controller.dart';
import '../update/update_controller.dart';
import 'pill_popup_controller.dart';
import 'window_mode_controller.dart';

/// Color de la luz de estado (semáforo): verde / rojo / amarillo / gris.
Color _colorFor(DictationStatus s) {
  switch (s) {
    case DictationStatus.idle:
      return const Color(0xFF34C759); // verde · listo
    case DictationStatus.recording:
      return const Color(0xFFFF3B30); // rojo · grabando
    case DictationStatus.transcribing:
    case DictationStatus.injecting:
      return const Color(0xFFFFCC00); // amarillo · pensando/escribiendo
    case DictationStatus.error:
      return const Color(0xFFFF3B30); // rojo · error
    case DictationStatus.uninitialized:
      return const Color(0xFF6E6E73); // gris · preparando
  }
}

/// El punto "respira"/brilla con actividad. En reposo es plano y discreto.
bool _animated(DictationStatus s) =>
    s == DictationStatus.recording ||
    s == DictationStatus.error;

/// ¿Está procesando? (pensando/escribiendo): mostramos un aro giratorio.
bool _thinking(DictationStatus s) =>
    s == DictationStatus.transcribing || s == DictationStatus.injecting;

/// Modo barra flotante: ventana transparente y sin marco, con un único punto.
/// Al terminar un dictado (o al copiar con Ctrl+Alt+V) se despliega una cajita
/// minimalista a la DERECHA del punto con la transcripción.
class OverlayPillScreen extends ConsumerStatefulWidget {
  const OverlayPillScreen({super.key});

  @override
  ConsumerState<OverlayPillScreen> createState() => _OverlayPillScreenState();
}

class _OverlayPillScreenState extends ConsumerState<OverlayPillScreen> {
  /// El ratón está sobre la caja (modo lectura: se amplía y no se auto-oculta).
  bool _hovered = false;

  @override
  void initState() {
    super.initState();
    // Si al entrar en píldora ya hay una caja visible, ajusta el tamaño.
    if (ref.read(pillPopupProvider).visible) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        _applyWindowSize(kPillExpanded);
      });
    }
  }

  /// El ratón entra/sale de la caja: amplía o colapsa, y pausa/reanuda el
  /// auto-ocultado para poder leer con calma.
  void _setHovered(bool value) {
    if (_hovered == value) return;
    setState(() => _hovered = value);
    final popup = ref.read(pillPopupProvider.notifier);
    if (value) {
      popup.pauseAutoHide();
    } else {
      popup.resumeAutoHide();
    }
    _syncWindowSize(ref.read(pillPopupProvider).visible);
  }

  /// Tamaño de ventana según el estado: sin caja → punto; con caja → expandida;
  /// con caja y ratón encima → lectura (más grande).
  void _syncWindowSize(bool visible) {
    final target =
        !visible ? kPillIdle : (_hovered ? kPillReading : kPillExpanded);
    _applyWindowSize(target);
  }

  /// Aplica el tamaño anclando la esquina INFERIOR-IZQUIERDA: al crecer en alto
  /// la ventana se expande hacia arriba (no hacia abajo, fuera de la pantalla)
  /// y el punto se queda clavado donde estaba.
  Future<void> _applyWindowSize(Size size) async {
    try {
      final b = await windowManager.getBounds();
      await windowManager.setBounds(
        Rect.fromLTWH(b.left, b.bottom - size.height, size.width, size.height),
      );
    } catch (_) {
      try {
        await windowManager.setSize(size);
      } catch (_) {}
    }
  }

  @override
  Widget build(BuildContext context) {
    final status =
        ref.watch(dictationControllerProvider.select((s) => s.status));
    final popup = ref.watch(pillPopupProvider);
    final updateAvailable =
        ref.watch(updateProvider.select((u) => u.isAvailable));
    final scale = ref.watch(settingsProvider.select((s) => s.pillScale));

    // Expande/colapsa la ventana según haya o no caja (ver _applyWindowSize).
    ref.listen<bool>(pillPopupProvider.select((p) => p.visible), (_, visible) {
      if (!visible && _hovered) _hovered = false;
      _syncWindowSize(visible);
    });

    return Scaffold(
      backgroundColor: Colors.transparent,
      body: Align(
        // Inferior-izquierda: el punto se ancla abajo y la caja crece hacia
        // arriba al ampliarse, igual que la ventana.
        alignment: Alignment.bottomLeft,
        child: Row(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.end,
          children: [
            SizedBox(
              width: 50,
              height: 50,
              child: Stack(
                children: [
                  _StatusLight(
                    status: status,
                    scale: scale,
                    onOpenPanel: () =>
                        ref.read(windowModeProvider.notifier).toPanel(),
                  ),
                  if (updateAvailable)
                    const Positioned(top: 13, right: 13, child: _UpdateDot()),
                ],
              ),
            ),
            if (popup.visible)
              MouseRegion(
                onEnter: (_) => _setHovered(true),
                onExit: (_) => _setHovered(false),
                child: _PopupBox(
                  key: ValueKey(popup.seq),
                  text: popup.text,
                  copied: popup.copied,
                  expanded: _hovered,
                  onCopy: () => ref
                      .read(dictationControllerProvider.notifier)
                      .copyLastTranscription(),
                ),
              ),
          ],
        ),
      ),
    );
  }
}

class _StatusLight extends StatefulWidget {
  final DictationStatus status;
  final double scale;
  final VoidCallback onOpenPanel;

  const _StatusLight({
    required this.status,
    required this.scale,
    required this.onOpenPanel,
  });

  @override
  State<_StatusLight> createState() => _StatusLightState();
}

class _StatusLightState extends State<_StatusLight>
    with SingleTickerProviderStateMixin {
  late final AnimationController _anim;

  @override
  void initState() {
    super.initState();
    _anim = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 1100),
    )..repeat();
  }

  @override
  void dispose() {
    _anim.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final status = widget.status;
    final color = _colorFor(status);

    return GestureDetector(
      behavior: HitTestBehavior.opaque,
      onTap: widget.onOpenPanel,
      onPanStart: (_) => windowManager.startDragging(),
      child: Center(
        child: _thinking(status)
            ? _ThinkingRing(color: color, scale: widget.scale)
            : _Dot(
                color: color,
                animation: _anim,
                glowing: _animated(status),
                scale: widget.scale,
              ),
      ),
    );
  }
}

/// Punto de color. Brilla y crece un poco cuando [glowing] (grabando/error).
class _Dot extends StatelessWidget {
  final Color color;
  final Animation<double> animation;
  final bool glowing;
  final double scale;

  const _Dot({
    required this.color,
    required this.animation,
    required this.glowing,
    required this.scale,
  });

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: animation,
      builder: (context, _) {
        final pulse =
            glowing ? (0.5 + 0.5 * math.sin(animation.value * 2 * math.pi)) : 0.0;
        final size = (glowing ? (15 + 3 * pulse) : 13.0) * scale;
        return Container(
          width: size,
          height: size,
          decoration: BoxDecoration(
            shape: BoxShape.circle,
            color: color,
            border: Border.all(
              color: Colors.white.withValues(alpha: 0.28),
              width: 1,
            ),
            boxShadow: glowing
                ? [
                    BoxShadow(
                      color: color.withValues(alpha: 0.35 + 0.4 * pulse),
                      blurRadius: 6 + 6 * pulse,
                      spreadRadius: 1 + 1.5 * pulse,
                    ),
                  ]
                : null,
          ),
        );
      },
    );
  }
}

/// Aro giratorio (estilo loading) alrededor de un punto, para el estado
/// "pensando/escribiendo".
class _ThinkingRing extends StatelessWidget {
  final Color color;
  final double scale;

  const _ThinkingRing({required this.color, required this.scale});

  @override
  Widget build(BuildContext context) {
    // Cap a 46 para que quepa en la ventana de la píldora (50px) a escalas altas.
    final ring = (30.0 * scale).clamp(20.0, 46.0);
    final inner = (11.0 * scale).clamp(7.0, 17.0);
    return SizedBox(
      width: ring,
      height: ring,
      child: Stack(
        alignment: Alignment.center,
        children: [
          // El aro indeterminado gira solo (loading).
          SizedBox(
            width: ring,
            height: ring,
            child: CircularProgressIndicator(
              strokeWidth: 2.6,
              valueColor: AlwaysStoppedAnimation(color),
              backgroundColor: color.withValues(alpha: 0.15),
            ),
          ),
          Container(
            width: inner,
            height: inner,
            decoration: BoxDecoration(shape: BoxShape.circle, color: color),
          ),
        ],
      ),
    );
  }
}

/// Punto morado sobre la píldora cuando hay una actualización disponible.
class _UpdateDot extends StatelessWidget {
  const _UpdateDot();

  @override
  Widget build(BuildContext context) {
    return Container(
      width: 9,
      height: 9,
      decoration: BoxDecoration(
        shape: BoxShape.circle,
        color: const Color(0xFF7800E3),
        border: Border.all(color: Colors.white, width: 1.5),
      ),
    );
  }
}

/// Cajita minimalista con la transcripción, a la derecha del punto. Entra con
/// una animación de aparición (desliza desde la izquierda + fundido). Si aún no
/// se ha copiado, al tocarla copia el texto al portapapeles.
///
/// Al pasar el ratón por encima ([expanded] == true) se amplía y muestra TODA
/// la transcripción en varias líneas (con scroll si es muy larga) para poder
/// leerla con calma.
class _PopupBox extends StatelessWidget {
  final String text;
  final bool copied;
  final bool expanded;
  final VoidCallback onCopy;

  const _PopupBox({
    super.key,
    required this.text,
    required this.copied,
    required this.expanded,
    required this.onCopy,
  });

  static const _accent = Color(0xFF7800E3);
  static const _hint = Color(0xFFB98CFF);
  static const _textColor = Color(0xFFEDEAF5);

  @override
  Widget build(BuildContext context) {
    return TweenAnimationBuilder<double>(
      tween: Tween(begin: 0, end: 1),
      duration: const Duration(milliseconds: 260),
      curve: Curves.easeOutCubic,
      builder: (context, t, child) => Opacity(
        opacity: t.clamp(0.0, 1.0),
        child: Transform.translate(
          offset: Offset(-14 * (1 - t), 0),
          child: child,
        ),
      ),
      child: Padding(
        padding: const EdgeInsets.only(left: 8, bottom: 3),
        child: GestureDetector(
          onTap: copied ? null : onCopy,
          child: AnimatedContainer(
            duration: const Duration(milliseconds: 180),
            curve: Curves.easeOut,
            width: expanded ? 312 : 264,
            constraints: BoxConstraints(maxHeight: expanded ? 200 : 44),
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
            decoration: BoxDecoration(
              color: const Color(0xF21B1A20),
              borderRadius: BorderRadius.circular(13),
              border: Border.all(
                color: _accent.withValues(alpha: expanded ? 0.7 : 0.45),
              ),
              boxShadow: [
                BoxShadow(
                  color: Colors.black.withValues(alpha: 0.38),
                  blurRadius: 12,
                  offset: const Offset(0, 3),
                ),
              ],
            ),
            child: expanded ? _expanded() : _compact(),
          ),
        ),
      ),
    );
  }

  /// Vista compacta: una sola línea con elipsis.
  Widget _compact() {
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Icon(
          copied ? Icons.check_circle_rounded : Icons.content_copy_rounded,
          size: 15,
          color: _hint,
        ),
        const SizedBox(width: 8),
        Expanded(
          child: Text(
            text,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: const TextStyle(
              color: _textColor,
              fontSize: 12.5,
              height: 1.1,
            ),
          ),
        ),
      ],
    );
  }

  /// Vista ampliada (ratón encima): etiqueta + texto completo desplazable.
  Widget _expanded() {
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              copied ? Icons.check_circle_rounded : Icons.content_copy_rounded,
              size: 14,
              color: _hint,
            ),
            const SizedBox(width: 6),
            Text(
              copied ? 'Copiado' : 'Toca para copiar',
              style: const TextStyle(
                color: _hint,
                fontSize: 11,
                fontWeight: FontWeight.w600,
              ),
            ),
          ],
        ),
        const SizedBox(height: 6),
        ConstrainedBox(
          constraints: const BoxConstraints(maxHeight: 162),
          child: SingleChildScrollView(
            child: Text(
              text,
              style: const TextStyle(
                color: _textColor,
                fontSize: 13,
                height: 1.3,
              ),
            ),
          ),
        ),
      ],
    );
  }
}
