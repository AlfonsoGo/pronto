import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../features/dictation/dictation_controller.dart';
import '../features/dictation/dictation_state.dart';

/// Píldora compacta que refleja el estado del dictado.
///
/// Muestra color, icono y etiqueta segun [DictationStatus] y, mientras se
/// graba, un pequeño medidor de nivel alimentado por `state.level` (0.0 - 1.0).
/// Todo el aspecto está animado para que las transiciones sean suaves.
class DictationPill extends ConsumerWidget {
  const DictationPill({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final state = ref.watch(dictationControllerProvider);
    final theme = Theme.of(context);
    final visuals = _PillVisuals.of(state.status, theme.colorScheme);

    return AnimatedContainer(
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
            blurRadius: 18,
            spreadRadius: 1,
          ),
        ],
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          _StatusIcon(visuals: visuals, status: state.status),
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
          // El medidor solo tiene sentido mientras se graba.
          if (state.status == DictationStatus.recording) ...[
            const SizedBox(width: 12),
            _LevelMeter(level: state.level, color: visuals.accent),
          ],
        ],
      ),
    );
  }
}

/// Icono de estado con pulso suave cuando hay actividad.
class _StatusIcon extends StatelessWidget {
  const _StatusIcon({required this.visuals, required this.status});

  final _PillVisuals visuals;
  final DictationStatus status;

  @override
  Widget build(BuildContext context) {
    final isActive = status == DictationStatus.recording ||
        status == DictationStatus.transcribing ||
        status == DictationStatus.injecting;

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

    if (isActive) {
      // Pulso continuo para señalar que algo está en marcha.
      icon = _Pulse(child: icon);
    }
    return icon;
  }
}

/// Animación de pulso (escala + opacidad) en bucle.
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

/// Medidor de nivel de audio: varias barras cuya altura sigue `level`.
class _LevelMeter extends StatelessWidget {
  const _LevelMeter({required this.level, required this.color});

  final double level; // 0.0 - 1.0
  final Color color;

  static const int _bars = 5;

  @override
  Widget build(BuildContext context) {
    final clamped = level.clamp(0.0, 1.0);
    return SizedBox(
      height: 20,
      child: Row(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.center,
        children: List.generate(_bars, (i) {
          // Cada barra se "enciende" a partir de un umbral creciente, de modo
          // que un nivel alto llena más barras.
          final threshold = (i + 1) / _bars;
          final on = clamped >= threshold * 0.65;
          final base = 5.0 + i * 2.0; // perfil escalonado de altura mínima
          final target = on ? (6.0 + clamped * 14.0 + i * 1.5) : base * 0.6;
          return Padding(
            padding: const EdgeInsets.symmetric(horizontal: 1.5),
            child: AnimatedContainer(
              duration: const Duration(milliseconds: 120),
              curve: Curves.easeOut,
              width: 3,
              height: target.clamp(3.0, 20.0),
              decoration: BoxDecoration(
                color: color.withValues(alpha: on ? 1.0 : 0.35),
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
          background: scheme.surfaceContainerHigh,
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
          accent: scheme.tertiary,
          background: scheme.tertiaryContainer.withValues(alpha: 0.35),
          foreground: scheme.onSurface,
          icon: Icons.graphic_eq_rounded,
          label: 'Transcribiendo',
        );
      case DictationStatus.injecting:
        return _PillVisuals(
          accent: scheme.secondary,
          background: scheme.secondaryContainer.withValues(alpha: 0.35),
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
