import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';

/// Partículas que orbitan y se desvanecen alrededor de un centro, para el
/// estado "pensando". Reutilizable en la píldora del panel y en la flotante.
///
/// Usa un [Ticker] con tiempo monótono (no un controlador en bucle) para que la
/// deriva de la órbita nunca dé un salto al reiniciarse. Cada partícula nace en
/// el centro, sale hacia fuera y se apaga (vida 0→1) en bucle continuo.
class ThinkingParticles extends StatefulWidget {
  const ThinkingParticles({
    super.key,
    required this.size,
    this.count = 14,
    this.baseRadius = 8,
    this.spread = 40,
  });

  /// Caja que ocupa; las partículas se pintan respecto a su centro.
  final Size size;
  final int count;

  /// Radio inicial (al nacer) y cuánto se alejan como máximo al morir.
  final double baseRadius;
  final double spread;

  @override
  State<ThinkingParticles> createState() => _ThinkingParticlesState();
}

class _ThinkingParticlesState extends State<ThinkingParticles>
    with SingleTickerProviderStateMixin {
  late final Ticker _ticker;
  double _t = 0;

  @override
  void initState() {
    super.initState();
    _ticker = createTicker((elapsed) {
      setState(() => _t = elapsed.inMicroseconds / 1e6);
    })..start();
  }

  @override
  void dispose() {
    _ticker.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return SizedBox.fromSize(
      size: widget.size,
      child: CustomPaint(
        painter: _ParticlePainter(
          t: _t,
          count: widget.count,
          baseRadius: widget.baseRadius,
          spread: widget.spread,
        ),
      ),
    );
  }
}

class _ParticlePainter extends CustomPainter {
  _ParticlePainter({
    required this.t,
    required this.count,
    required this.baseRadius,
    required this.spread,
  });

  final double t;
  final int count;
  final double baseRadius;
  final double spread;

  static const _yellow = Color(0xFFFDE047);
  static const _purple = Color(0xFFA78BFA);

  @override
  void paint(Canvas canvas, Size size) {
    final cx = size.width / 2;
    final cy = size.height / 2;
    for (var i = 0; i < count; i++) {
      final speed = 0.6 + (i % 5) * 0.22;
      final life = (t * speed + i / count) % 1.0; // 0..1, renace en bucle
      final angle = (i / count) * 2 * math.pi + i * 1.7;
      final radius = baseRadius + life * (spread + (i % 4) * (spread * 0.35));
      final x = math.cos(angle + t * 0.4) * radius;
      final y = math.sin(angle + t * 0.4) * radius * 0.65;
      // Opacidad en campana: 0 al nacer y al morir → el bucle es invisible.
      final opacity = (math.sin(life * math.pi) * 0.9).clamp(0.0, 1.0);
      final r = (2 + (i % 3) * 1.4 + (1 - life) * 2) * 0.5; // radio del círculo
      final color = (i.isEven ? _yellow : _purple).withValues(alpha: opacity);
      final pos = Offset(cx + x, cy + y);
      // Halo difuminado + núcleo nítido = aspecto neón.
      canvas.drawCircle(
        pos,
        r,
        Paint()
          ..color = color
          ..maskFilter = MaskFilter.blur(BlurStyle.normal, r * 0.9),
      );
      canvas.drawCircle(pos, r * 0.6, Paint()..color = color);
    }
  }

  @override
  bool shouldRepaint(_ParticlePainter old) => old.t != t;
}
