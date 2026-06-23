import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../features/dictation/dictation_controller.dart';
import '../features/dictation/dictation_state.dart';
import 'thinking_particles.dart';

/// Píldora compacta que refleja el estado del dictado.
///
/// Cada estado tiene su propia vida:
/// - **Listo / reposo**: respira suavemente.
/// - **Grabando**: medidor de nivel con degradado rojo→morado.
/// - **Pensando** (transcribiendo/insertando): spinner doble + puntos que
///   rebotan y una nube de partículas orbitando alrededor.
class DictationPill extends ConsumerWidget {
  const DictationPill({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final state = ref.watch(dictationControllerProvider);
    final theme = Theme.of(context);
    final visuals = _PillVisuals.of(state.status, theme.colorScheme);
    final status = state.status;

    final pill = _Pill(visuals: visuals, status: status, level: state.level);

    // Pensando: partículas alrededor de la píldora. La caja es mayor que la
    // píldora y NO recorta (clipBehavior.none), así las partículas escapan.
    if (status == DictationStatus.transcribing ||
        status == DictationStatus.injecting) {
      return Stack(
        clipBehavior: Clip.none,
        alignment: Alignment.center,
        children: [
          const ThinkingParticles(
            size: Size(260, 120),
            baseRadius: 12,
            spread: 46,
          ),
          pill,
        ],
      );
    }
    return pill;
  }
}

/// La cápsula en sí. En reposo respira; el resto del aspecto sale de [visuals].
class _Pill extends StatelessWidget {
  const _Pill({
    required this.visuals,
    required this.status,
    required this.level,
  });

  final _PillVisuals visuals;
  final DictationStatus status;
  final double level;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final thinking = status == DictationStatus.transcribing ||
        status == DictationStatus.injecting;

    final capsule = AnimatedContainer(
      duration: const Duration(milliseconds: 250),
      curve: Curves.easeOutCubic,
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 10),
      decoration: BoxDecoration(
        color: visuals.background,
        borderRadius: BorderRadius.circular(999),
        border: Border.all(color: visuals.accent.withValues(alpha: 0.55)),
        boxShadow: [
          BoxShadow(
            color: visuals.accent.withValues(alpha: 0.28),
            blurRadius: 20,
            spreadRadius: 1,
          ),
        ],
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          // Icono / spinner según estado.
          if (thinking)
            _DualSpinner(a: const Color(0xFFFDE047), b: visuals.accent)
          else
            _StatusIcon(visuals: visuals, status: status),
          const SizedBox(width: 10),
          AnimatedDefaultTextStyle(
            duration: const Duration(milliseconds: 250),
            style: theme.textTheme.labelLarge!.copyWith(
              color: visuals.foreground,
              fontWeight: FontWeight.w600,
              letterSpacing: 0.3,
            ),
            child: Text(visuals.label),
          ),
          if (status == DictationStatus.recording) ...[
            const SizedBox(width: 12),
            _LevelMeter(level: level, color: visuals.accent),
          ],
          if (thinking) ...[
            const SizedBox(width: 10),
            const _BouncingDots(),
          ],
        ],
      ),
    );

    // En reposo (Listo / aún sin modelo) la píldora respira despacio.
    if (status == DictationStatus.idle ||
        status == DictationStatus.uninitialized) {
      return _Breathe(child: capsule);
    }
    return capsule;
  }
}

/// Icono de estado con pulso suave cuando hay actividad.
class _StatusIcon extends StatelessWidget {
  const _StatusIcon({required this.visuals, required this.status});

  final _PillVisuals visuals;
  final DictationStatus status;

  @override
  Widget build(BuildContext context) {
    Widget icon = AnimatedSwitcher(
      duration: const Duration(milliseconds: 250),
      transitionBuilder: (child, anim) =>
          ScaleTransition(scale: anim, child: child),
      child: Icon(
        visuals.icon,
        key: ValueKey(visuals.icon),
        size: 20,
        color: visuals.accent,
      ),
    );

    if (status == DictationStatus.recording) {
      icon = _Pulse(child: icon);
    }
    return icon;
  }
}

/// Respiración lenta (escala) para el estado en reposo.
class _Breathe extends StatefulWidget {
  const _Breathe({required this.child});

  final Widget child;

  @override
  State<_Breathe> createState() => _BreatheState();
}

class _BreatheState extends State<_Breathe>
    with SingleTickerProviderStateMixin {
  late final AnimationController _ctrl = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 3400),
  )..repeat(reverse: true);

  @override
  void dispose() {
    _ctrl.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return ScaleTransition(
      scale: Tween<double>(begin: 0.99, end: 1.025).animate(
        CurvedAnimation(parent: _ctrl, curve: Curves.easeInOut),
      ),
      child: widget.child,
    );
  }
}

/// Animación de pulso (escala + opacidad) en bucle, para el icono activo.
class _Pulse extends StatefulWidget {
  const _Pulse({required this.child});

  final Widget child;

  @override
  State<_Pulse> createState() => _PulseState();
}

class _PulseState extends State<_Pulse> with SingleTickerProviderStateMixin {
  late final AnimationController _ctrl = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 900),
  )..repeat(reverse: true);

  @override
  void dispose() {
    _ctrl.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return FadeTransition(
      opacity: Tween<double>(begin: 0.55, end: 1.0).animate(
        CurvedAnimation(parent: _ctrl, curve: Curves.easeInOut),
      ),
      child: ScaleTransition(
        scale: Tween<double>(begin: 0.85, end: 1.1).animate(
          CurvedAnimation(parent: _ctrl, curve: Curves.easeInOut),
        ),
        child: widget.child,
      ),
    );
  }
}

/// Spinner de dos aros concéntricos de distinto color (estilo del diseño).
class _DualSpinner extends StatelessWidget {
  const _DualSpinner({required this.a, required this.b});

  final Color a;
  final Color b;

  static const double size = 22;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: size,
      height: size,
      child: Stack(
        alignment: Alignment.center,
        children: [
          SizedBox(
            width: size,
            height: size,
            child: CircularProgressIndicator(
              strokeWidth: 2.5,
              valueColor: AlwaysStoppedAnimation(a),
            ),
          ),
          Padding(
            padding: const EdgeInsets.all(4),
            child: SizedBox(
              width: size - 8,
              height: size - 8,
              child: CircularProgressIndicator(
                strokeWidth: 2.5,
                valueColor: AlwaysStoppedAnimation(b),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// Tres puntos que rebotan, escalonados (el "…" de pensando).
class _BouncingDots extends StatefulWidget {
  const _BouncingDots();

  @override
  State<_BouncingDots> createState() => _BouncingDotsState();
}

class _BouncingDotsState extends State<_BouncingDots>
    with SingleTickerProviderStateMixin {
  late final AnimationController _ctrl = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 1300),
  )..repeat();

  static const _colors = [
    Color(0xFFFDE047),
    Color(0xFFC4B5FD),
    Color(0xFFFDE047),
  ];

  @override
  void dispose() {
    _ctrl.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: _ctrl,
      builder: (context, _) {
        return Row(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.end,
          children: List.generate(3, (i) {
            final p = (_ctrl.value + i * 0.16) % 1.0;
            final lift = math.max(0.0, math.sin(p * math.pi));
            return Padding(
              padding: EdgeInsets.only(left: i == 0 ? 0 : 4),
              child: Transform.translate(
                offset: Offset(0, -4 * lift),
                child: Container(
                  width: 5,
                  height: 5,
                  decoration: BoxDecoration(
                    shape: BoxShape.circle,
                    color: _colors[i].withValues(alpha: 0.3 + 0.7 * lift),
                  ),
                ),
              ),
            );
          }),
        );
      },
    );
  }
}

/// Medidor de nivel de audio con degradado rojo→morado (a juego con la onda).
class _LevelMeter extends StatelessWidget {
  const _LevelMeter({required this.level, required this.color});

  final double level; // 0.0 - 1.0
  final Color color;

  static const int _bars = 5;
  static const _violet = Color(0xFFA78BFA);

  @override
  Widget build(BuildContext context) {
    final clamped = level.clamp(0.0, 1.0);
    return SizedBox(
      height: 20,
      child: Row(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.center,
        children: List.generate(_bars, (i) {
          final threshold = (i + 1) / _bars;
          final on = clamped >= threshold * 0.65;
          final base = 5.0 + i * 2.0;
          final target = on ? (6.0 + clamped * 14.0 + i * 1.5) : base * 0.6;
          final barColor = Color.lerp(color, _violet, i / (_bars - 1))!;
          return Padding(
            padding: const EdgeInsets.symmetric(horizontal: 1.5),
            child: AnimatedContainer(
              duration: const Duration(milliseconds: 120),
              curve: Curves.easeOut,
              width: 3,
              height: target.clamp(3.0, 20.0),
              decoration: BoxDecoration(
                color: barColor.withValues(alpha: on ? 1.0 : 0.35),
                borderRadius: BorderRadius.circular(2),
              ),
            ),
          );
        }),
      ),
    );
  }
}

/// Mapeo estado -> color/icono/etiqueta. Encapsula toda la paleta de la píldora.
class _PillVisuals {
  const _PillVisuals({
    required this.accent,
    required this.background,
    required this.foreground,
    required this.icon,
    required this.label,
  });

  final Color accent;
  final Color background;
  final Color foreground;
  final IconData icon;
  final String label;

  factory _PillVisuals.of(DictationStatus status, ColorScheme scheme) {
    switch (status) {
      case DictationStatus.uninitialized:
        return _PillVisuals(
          accent: scheme.outline,
          background: scheme.surfaceContainerHighest,
          foreground: scheme.onSurfaceVariant,
          icon: Icons.cloud_download_outlined,
          label: 'Modelo no cargado',
        );
      case DictationStatus.idle:
        return _PillVisuals(
          accent: scheme.primary,
          background: scheme.primary.withValues(alpha: 0.07),
          foreground: scheme.onSurface,
          icon: Icons.mic_none_rounded,
          label: 'Listo',
        );
      case DictationStatus.recording:
        const rec = Color(0xFFE5484D);
        return _PillVisuals(
          accent: rec,
          background: rec.withValues(alpha: 0.16),
          foreground: scheme.onSurface,
          icon: Icons.mic_rounded,
          label: 'Grabando',
        );
      case DictationStatus.transcribing:
        return _PillVisuals(
          accent: scheme.secondary,
          background: scheme.primary.withValues(alpha: 0.12),
          foreground: scheme.onSurface,
          icon: Icons.graphic_eq_rounded,
          label: 'Transcribiendo',
        );
      case DictationStatus.injecting:
        return _PillVisuals(
          accent: scheme.secondary,
          background: scheme.primary.withValues(alpha: 0.12),
          foreground: scheme.onSurface,
          icon: Icons.keyboard_rounded,
          label: 'Insertando',
        );
      case DictationStatus.error:
        return _PillVisuals(
          accent: scheme.error,
          background: scheme.errorContainer.withValues(alpha: 0.35),
          foreground: scheme.onErrorContainer,
          icon: Icons.error_outline_rounded,
          label: 'Error',
        );
    }
  }
}
