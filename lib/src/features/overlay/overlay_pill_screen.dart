import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:screen_retriever/screen_retriever.dart';
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

  /// Debounce del colapso: al ampliar, la ventana se redimensiona bajo el
  /// cursor y eso dispara un onExit/onEnter espurio. Si el ratón vuelve antes
  /// de que salte este timer, cancelamos el colapso → no parpadea.
  Timer? _collapseTimer;

  /// Ancla (esquina inferior-izquierda, en coords de pantalla) que se fija al
  /// aparecer la caja. Todas las redimensiones de hover usan ESTE punto, en vez
  /// de releer getBounds() a mitad de un resize (lo que causaba jitter y, con
  /// el MouseRegion, parpadeo). Se recalcula cada vez que aparece la caja (por
  /// si moviste la píldora) y se borra al ocultarse.
  Offset? _pillAnchor;

  /// ¿Grabando ahora? Mientras graba, la ventana se ensancha para el waveform.
  bool _isRecording = false;

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

  @override
  void dispose() {
    _collapseTimer?.cancel();
    super.dispose();
  }

  /// El ratón entra en la caja: amplía YA (cancela cualquier colapso pendiente).
  void _onBoxEnter() {
    _collapseTimer?.cancel();
    _collapseTimer = null;
    if (_hovered) return;
    setState(() => _hovered = true);
    ref.read(pillPopupProvider.notifier).pauseAutoHide();
    _syncWindowSize(ref.read(pillPopupProvider).visible);
  }

  /// El ratón sale de la caja: colapsa, pero con un pequeño retardo para
  /// absorber los onExit espurios que provoca el propio redimensionado.
  void _onBoxExit() {
    _collapseTimer?.cancel();
    _collapseTimer = Timer(const Duration(milliseconds: 240), () {
      if (!mounted || !_hovered) return;
      setState(() => _hovered = false);
      ref.read(pillPopupProvider.notifier).resumeAutoHide();
      _syncWindowSize(ref.read(pillPopupProvider).visible);
    });
  }

  /// Tamaño de ventana según el estado: sin caja → punto; con caja → expandida;
  /// con caja y ratón encima → lectura (más grande). Al ocultarse la caja se
  /// borra el ancla para recalcularla la próxima vez.
  void _syncWindowSize(bool visible) {
    if (visible) {
      _applyWindowSize(_hovered ? kPillReading : kPillExpanded);
      return;
    }
    // Sin caja: grabando = un poco más ancha (waveform); en reposo = el punto.
    _applyWindowSize(_isRecording ? kPillRecording : kPillIdle);
    if (!_isRecording) _pillAnchor = null;
  }

  /// Aplica el tamaño anclando la esquina INFERIOR-IZQUIERDA: al crecer en alto
  /// la ventana se expande hacia arriba (no hacia abajo, fuera de la pantalla)
  /// y el punto se queda clavado donde estaba. Usa un ancla CACHEADA para no
  /// releer getBounds() a mitad de un resize (lo que provocaba jitter/parpadeo);
  /// [refreshAnchor] la recalcula (al aparecer la caja, por si moviste la píldora).
  Future<void> _applyWindowSize(Size size, {bool refreshAnchor = false}) async {
    try {
      if (refreshAnchor || _pillAnchor == null) {
        final b = await windowManager.getBounds();
        _pillAnchor = Offset(b.left, b.bottom);
      }
      final a = _pillAnchor!;
      await windowManager.setBounds(
        Rect.fromLTWH(a.dx, a.dy - size.height, size.width, size.height),
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
    // Nivel de audio real (0-1): alimenta el waveform mientras grabas.
    final level = ref.watch(dictationControllerProvider.select((s) => s.level));
    final recording = status == DictationStatus.recording;

    // Expande/colapsa la ventana según haya o no caja (ver _applyWindowSize).
    ref.listen<bool>(pillPopupProvider.select((p) => p.visible), (_, visible) {
      if (visible) {
        // Nueva caja: fija el ancla aquí (por si moviste la píldora) y expande.
        _collapseTimer?.cancel();
        _applyWindowSize(kPillExpanded, refreshAnchor: true);
      } else {
        _collapseTimer?.cancel();
        if (_hovered) _hovered = false;
        _syncWindowSize(false);
      }
    });

    // Al empezar/parar de grabar, ensancha/encoge la ventana para el waveform.
    ref.listen<bool>(
      dictationControllerProvider
          .select((s) => s.status == DictationStatus.recording),
      (_, rec) {
        _isRecording = rec;
        if (!ref.read(pillPopupProvider).visible) _syncWindowSize(false);
      },
    );

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
              // Al grabar, ensancha la zona para que quepa el waveform largo.
              width: recording ? kPillRecording.width : 50,
              height: 50,
              child: Stack(
                children: [
                  _StatusLight(
                    status: status,
                    scale: scale,
                    level: level,
                    onOpenPanel: () =>
                        ref.read(windowModeProvider.notifier).toPanel(),
                    // Al arrastrar, actualiza el ancla para no volver atrás en
                    // el próximo redimensionado (grabar/parar).
                    onMoved: (bl) => _pillAnchor = bl,
                  ),
                  // Solo en reposo (punto verde): sobre las ondas de grabación
                  // el punto morado quedaba feo. Idle = buen momento para avisar.
                  if (updateAvailable && status == DictationStatus.idle)
                    const Positioned(top: 13, right: 13, child: _UpdateDot()),
                ],
              ),
            ),
            if (popup.visible)
              MouseRegion(
                onEnter: (_) => _onBoxEnter(),
                onExit: (_) => _onBoxExit(),
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
  final double level;
  final VoidCallback onOpenPanel;

  /// Notifica la nueva esquina inferior-izquierda de la ventana tras arrastrar,
  /// para que la píldora reancle sus reajustes de tamaño a esa posición.
  final ValueChanged<Offset>? onMoved;

  const _StatusLight({
    required this.status,
    required this.scale,
    required this.level,
    required this.onOpenPanel,
    this.onMoved,
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

  // ─── Arrastre SOLO horizontal, con soporte MULTI-MONITOR ────────────────────
  // El eje vertical queda bloqueado, pero la píldora cruza libremente entre
  // monitores. Al pasar a otro monitor de distinta resolución NO "salta arriba":
  // se ancla al borde inferior del monitor bajo el que está, conservando la
  // misma distancia al fondo que tenía al empezar. Solo movemos en onPanUpdate
  // (rebasado el touch-slop), así un clic simple no la desplaza. Funciona igual
  // esté grabando o no (este gestor está presente en todos los estados).
  bool _dragReady = false;
  double _logicalLeft = 0; // X lógica de la ventana durante el arrastre
  double _winWidth = 50;
  double _winHeight = 50;
  double _startTop = 0; // Y inicial (fallback si no hay datos de monitores)
  double _bottomOffset = 0; // distancia del borde inferior del monitor al top
  List<Display> _displays = const [];

  Future<void> _onDragStart(DragStartDetails _) async {
    try {
      final b = await windowManager.getBounds();
      _logicalLeft = b.left;
      _winWidth = b.width;
      _winHeight = b.height;
      _startTop = b.top;
      _displays = await screenRetriever.getAllDisplays();
      final d = _displayForX(b.left + b.width / 2);
      _bottomOffset = d != null ? _workBottom(d) - b.top : 0;
      _dragReady = true;
    } catch (_) {
      _dragReady = false;
    }
  }

  void _onDragUpdate(DragUpdateDetails d) {
    if (!_dragReady) return;
    // Solo eje X (vertical bloqueado); movimiento libre por todo el escritorio
    // virtual → cruza entre monitores.
    _logicalLeft += d.delta.dx;
    final disp = _displayForX(_logicalLeft + _winWidth / 2);
    final top = disp != null ? _workBottom(disp) - _bottomOffset : _startTop;
    windowManager.setPosition(Offset(_logicalLeft, top));
    // Informa a la píldora de su nueva esquina inferior-izquierda para que los
    // reajustes de tamaño (grabar/parar) NO la devuelvan a la posición previa.
    widget.onMoved?.call(Offset(_logicalLeft, top + _winHeight));
  }

  /// Monitor cuya área de trabajo contiene la X dada (o el primero si ninguno).
  Display? _displayForX(double x) {
    for (final d in _displays) {
      final pos = d.visiblePosition ?? Offset.zero;
      final size = d.visibleSize ?? d.size;
      if (x >= pos.dx && x < pos.dx + size.width) return d;
    }
    return _displays.isNotEmpty ? _displays.first : null;
  }

  /// Borde inferior del área de trabajo (sin barra de tareas) del monitor.
  double _workBottom(Display d) {
    final pos = d.visiblePosition ?? Offset.zero;
    final size = d.visibleSize ?? d.size;
    return pos.dy + size.height;
  }

  @override
  Widget build(BuildContext context) {
    final status = widget.status;
    final color = _colorFor(status);

    return GestureDetector(
      behavior: HitTestBehavior.opaque,
      onTap: widget.onOpenPanel,
      // Arrastre SOLO horizontal (ver _onDragUpdate): la píldora no se mueve en
      // vertical y un clic no la arrastra.
      onPanStart: _onDragStart,
      onPanUpdate: _onDragUpdate,
      onPanEnd: (_) => _dragReady = false,
      child: Center(
        child: status == DictationStatus.recording
            ? _RecordingBars(
                level: widget.level,
                animation: _anim,
                scale: widget.scale,
              )
            : _thinking(status)
                ? _ThinkingRing(scale: widget.scale)
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
        // El punto siempre respira; fuerte cuando hay actividad, suave en reposo.
        final pulse = 0.5 + 0.5 * math.sin(animation.value * 2 * math.pi);
        final size = (glowing ? (15 + 3 * pulse) : (12.5 + 1.0 * pulse)) * scale;
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
            boxShadow: [
              BoxShadow(
                color: color.withValues(
                  alpha: glowing ? (0.35 + 0.4 * pulse) : (0.18 + 0.14 * pulse),
                ),
                blurRadius: glowing ? (6 + 6 * pulse) : (4 + 3 * pulse),
                spreadRadius: glowing ? (1 + 1.5 * pulse) : 0.5,
              ),
            ],
          ),
        );
      },
    );
  }
}

/// Mini-waveform (5 barras) mientras se graba: las barras crecen con el NIVEL
/// de audio real ([level], 0-1) y se mueven con una onda para dar vida. Sustituye
/// al pulso decorativo: ahora ves de un vistazo que el micro te capta.
class _RecordingBars extends StatelessWidget {
  final double level;
  final Animation<double> animation;
  final double scale;

  const _RecordingBars({
    required this.level,
    required this.animation,
    required this.scale,
  });

  static const int _count = 17;

  @override
  Widget build(BuildContext context) {
    // Área del waveform: largo fijo y cómodo; el alto sí sigue al slider del punto.
    const w = 84.0;
    final h = (34.0 * scale).clamp(24.0, 46.0);
    // La voz (RMS) viene baja (~0.05-0.25): la amplificamos bastante para que las
    // ondas SE MUEVAN mucho al hablar. ponytail: ganancia 7.5; ajústala a gusto.
    final voice = (level.clamp(0.0, 1.0) * 7.5).clamp(0.0, 1.0);
    return SizedBox(
      width: w,
      height: h,
      child: AnimatedBuilder(
        animation: animation,
        builder: (context, _) {
          // Tiempo cíclico (0..2π): el bucle de la animación no da saltos.
          final t = animation.value * 2 * math.pi;
          return Row(
            mainAxisAlignment: MainAxisAlignment.center,
            children: List.generate(_count, (i) {
              final frac = i / (_count - 1);
              // Campana: las del centro más altas que las de los bordes.
              final bell = 0.55 + 0.45 * math.sin(frac * math.pi);
              // Movimiento multi-frecuencia (varias ondas superpuestas) = vida
              // orgánica, como el Voice Wave. Fases enteras → bucle sin costuras.
              final phase = i * 0.45;
              final a = math.sin(2 * t + phase);
              final b = math.sin(3 * t + phase * 1.7 + 1.3);
              final c = math.sin(1 * t + phase * 0.6);
              final motion = (a * 0.5 + b * 0.3 + c * 0.4 + 1.2) / 2.4; // 0..1
              // Silencio: leve parpadeo bajito. Al hablar: la voz amplía altura
              // Y energía del movimiento, así las ondas se mueven mucho más.
              final idle = 0.06 + 0.05 * motion;
              final f = (idle + voice * bell * (0.25 + 0.75 * motion))
                  .clamp(0.05, 1.0);
              // Degradado rojo→morado a lo largo de la fila (estilo Voice Wave).
              final hue = 350.0 - frac * 60.0; // 350 (rojo) → 290 (morado)
              final light = (0.52 + f * 0.14).clamp(0.0, 1.0);
              final color = HSLColor.fromAHSL(1, hue, 0.88, light).toColor();
              return Container(
                width: 2.6,
                height: (h * f).clamp(2.5, h),
                margin: const EdgeInsets.symmetric(horizontal: 1.1),
                decoration: BoxDecoration(
                  color: color,
                  borderRadius: BorderRadius.circular(999),
                  // Resplandor que crece con la voz (estilo neón del Voice Wave).
                  boxShadow: [
                    BoxShadow(
                      color: color.withValues(alpha: 0.35 + f * 0.35),
                      blurRadius: 2 + f * 4,
                    ),
                  ],
                ),
              );
            }),
          );
        },
      ),
    );
  }
}

/// Doble aro giratorio (amarillo + morado) para el estado "pensando/escribiendo".
class _ThinkingRing extends StatelessWidget {
  final double scale;

  const _ThinkingRing({required this.scale});

  static const _yellow = Color(0xFFFDE047);
  static const _violet = Color(0xFFA78BFA);

  @override
  Widget build(BuildContext context) {
    // Cap a 46 para que quepa en la ventana de la píldora (50px) a escalas altas.
    final ring = (30.0 * scale).clamp(20.0, 46.0);
    final inner = (10.0 * scale).clamp(6.0, 15.0);
    return SizedBox(
      width: ring,
      height: ring,
      child: Stack(
        alignment: Alignment.center,
        children: [
          // Aro exterior amarillo.
          SizedBox(
            width: ring,
            height: ring,
            child: CircularProgressIndicator(
              strokeWidth: 2.6,
              valueColor: const AlwaysStoppedAnimation(_yellow),
              backgroundColor: _yellow.withValues(alpha: 0.12),
            ),
          ),
          // Aro interior morado.
          SizedBox(
            width: ring - 8,
            height: ring - 8,
            child: const CircularProgressIndicator(
              strokeWidth: 2.4,
              valueColor: AlwaysStoppedAnimation(_violet),
            ),
          ),
          Container(
            width: inner,
            height: inner,
            decoration: const BoxDecoration(
              shape: BoxShape.circle,
              color: _violet,
            ),
          ),
        ],
      ),
    );
  }
}

/// Indicador de "hay actualización disponible": un punto MORADO grande y
/// pulsante en la parte superior de la píldora, con un halo amplio que tiñe la
/// píldora de morado para que salte a la vista de un vistazo. Mantiene el color
/// de estado del punto principal intacto (el morado solo aparece si hay update).
class _UpdateDot extends StatefulWidget {
  const _UpdateDot();

  @override
  State<_UpdateDot> createState() => _UpdateDotState();
}

class _UpdateDotState extends State<_UpdateDot>
    with SingleTickerProviderStateMixin {
  /// Morado de marca (theme.dart → violet).
  static const _violet = Color(0xFF8B5CF6);

  late final AnimationController _anim;

  @override
  void initState() {
    super.initState();
    _anim = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 1400),
    )..repeat();
  }

  @override
  void dispose() {
    _anim.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: _anim,
      builder: (context, _) {
        final pulse = 0.5 + 0.5 * math.sin(_anim.value * 2 * math.pi);
        return Container(
          width: 13,
          height: 13,
          decoration: BoxDecoration(
            shape: BoxShape.circle,
            color: _violet,
            border: Border.all(color: Colors.white, width: 1.6),
            boxShadow: [
              // Halo amplio que "tiñe" la píldora de morado; más intenso al pulsar.
              BoxShadow(
                color: _violet.withValues(alpha: 0.45 + 0.35 * pulse),
                blurRadius: 8 + 6 * pulse,
                spreadRadius: 1 + 2 * pulse,
              ),
            ],
          ),
        );
      },
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
