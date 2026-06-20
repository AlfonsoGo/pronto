import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:window_manager/window_manager.dart';

import '../dictation/dictation_controller.dart';
import '../dictation/dictation_state.dart';
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
  @override
  void initState() {
    super.initState();
    // Si al entrar en píldora ya hay una caja visible, ajusta el tamaño.
    if (ref.read(pillPopupProvider).visible) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        windowManager.setSize(kPillExpanded);
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final status =
        ref.watch(dictationControllerProvider.select((s) => s.status));
    final popup = ref.watch(pillPopupProvider);
    final updateAvailable =
        ref.watch(updateProvider.select((u) => u.isAvailable));

    // Expande/colapsa la ventana según haya o no caja. Crece SOLO hacia la
    // derecha (la esquina superior-izquierda no se mueve): el punto se queda en
    // su sitio y la caja se despliega a su lado.
    ref.listen<bool>(pillPopupProvider.select((p) => p.visible), (_, visible) {
      windowManager.setSize(visible ? kPillExpanded : kPillIdle);
    });

    return Scaffold(
      backgroundColor: Colors.transparent,
      body: Align(
        alignment: Alignment.centerLeft,
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            SizedBox(
              width: 50,
              height: 50,
              child: Stack(
                children: [
                  _StatusLight(
                    status: status,
                    onOpenPanel: () =>
                        ref.read(windowModeProvider.notifier).toPanel(),
                  ),
                  if (updateAvailable)
                    const Positioned(top: 13, right: 13, child: _UpdateDot()),
                ],
              ),
            ),
            if (popup.visible)
              _PopupBox(
                key: ValueKey(popup.seq),
                text: popup.text,
                copied: popup.copied,
                onCopy: () => ref
                    .read(dictationControllerProvider.notifier)
                    .copyLastTranscription(),
              ),
          ],
        ),
      ),
    );
  }
}

class _StatusLight extends StatefulWidget {
  final DictationStatus status;
  final VoidCallback onOpenPanel;

  const _StatusLight({required this.status, required this.onOpenPanel});

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
            ? _ThinkingRing(color: color)
            : _Dot(color: color, animation: _anim, glowing: _animated(status)),
      ),
    );
  }
}

/// Punto de color. Brilla y crece un poco cuando [glowing] (grabando/error).
class _Dot extends StatelessWidget {
  final Color color;
  final Animation<double> animation;
  final bool glowing;

  const _Dot({
    required this.color,
    required this.animation,
    required this.glowing,
  });

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: animation,
      builder: (context, _) {
        final pulse =
            glowing ? (0.5 + 0.5 * math.sin(animation.value * 2 * math.pi)) : 0.0;
        final size = glowing ? (15 + 3 * pulse) : 13.0;
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

  const _ThinkingRing({required this.color});

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: 30,
      height: 30,
      child: Stack(
        alignment: Alignment.center,
        children: [
          // El aro indeterminado gira solo (loading).
          SizedBox(
            width: 30,
            height: 30,
            child: CircularProgressIndicator(
              strokeWidth: 2.6,
              valueColor: AlwaysStoppedAnimation(color),
              backgroundColor: color.withValues(alpha: 0.15),
            ),
          ),
          Container(
            width: 11,
            height: 11,
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
class _PopupBox extends StatelessWidget {
  final String text;
  final bool copied;
  final VoidCallback onCopy;

  const _PopupBox({
    super.key,
    required this.text,
    required this.copied,
    required this.onCopy,
  });

  @override
  Widget build(BuildContext context) {
    const accent = Color(0xFF7800E3);
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
        padding: const EdgeInsets.only(left: 8),
        child: GestureDetector(
          onTap: copied ? null : onCopy,
          child: Container(
            width: 264,
            constraints: const BoxConstraints(maxHeight: 44),
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
            decoration: BoxDecoration(
              color: const Color(0xF21B1A20),
              borderRadius: BorderRadius.circular(13),
              border: Border.all(color: accent.withValues(alpha: 0.45)),
              boxShadow: [
                BoxShadow(
                  color: Colors.black.withValues(alpha: 0.38),
                  blurRadius: 12,
                  offset: const Offset(0, 3),
                ),
              ],
            ),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(
                  copied
                      ? Icons.check_circle_rounded
                      : Icons.content_copy_rounded,
                  size: 15,
                  color: const Color(0xFFB98CFF),
                ),
                const SizedBox(width: 8),
                Expanded(
                  child: Text(
                    text,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: const TextStyle(
                      color: Color(0xFFEDEAF5),
                      fontSize: 12.5,
                      height: 1.1,
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
