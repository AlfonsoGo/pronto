import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:window_manager/window_manager.dart';

/// La app tiene dos formas: una barra flotante (pill) tipo Wispr Flow, siempre
/// encima y sin robar foco; y un panel normal con la UI completa (ajustes,
/// historial, prueba de dictado).
enum WindowMode { pill, panel }

/// Luz de estado flotante: mínima, solo un punto de color (sin texto, sin caja).
/// La ventana es algo mayor que el punto para dar sitio al halo y al aro
/// giratorio; al ser transparente y sin marco, ese margen es invisible.
const Size kPillIdle = Size(50, 50);

/// Píldora EXPANDIDA: cuando aparece la caja con la transcripción a la derecha
/// del punto. Crece hacia la derecha; la esquina INFERIOR-izquierda no se mueve,
/// así el punto se queda en su sitio y la caja se despliega a su lado.
const Size kPillExpanded = Size(330, 50);

/// Píldora GRABANDO: algo más ancha que en reposo para que quepa el waveform
/// (más barritas). Crece hacia la derecha, anclada abajo-izquierda.
const Size kPillRecording = Size(96, 50);

/// Píldora en modo LECTURA: al pasar el ratón por encima de la caja, se amplía
/// para leer toda la transcripción (multilínea, con scroll si es larga). Crece
/// hacia ARRIBA y a la derecha, anclada por la esquina inferior-izquierda, para
/// no salirse por debajo de la pantalla (la píldora vive abajo).
const Size kPillReading = Size(380, 240);

/// Tamaño del panel completo.
const Size kPanelSize = Size(440, 680);

final windowModeProvider =
    NotifierProvider<WindowModeController, WindowMode>(WindowModeController.new);

class WindowModeController extends Notifier<WindowMode> {
  @override
  WindowMode build() => WindowMode.pill;

  /// Convierte la ventana en la barra flotante (abajo-centro, siempre encima).
  Future<void> toPill() async {
    state = WindowMode.pill;
    await windowManager.setBackgroundColor(Colors.transparent);
    await windowManager.setAlwaysOnTop(true);
    await windowManager.setSkipTaskbar(true);
    // Frameless: quita el marco/borde redondeado de la ventana (la "caja").
    await windowManager.setAsFrameless();
    await windowManager.setResizable(false);
    await windowManager.setSize(kPillIdle);
    await windowManager.setAlignment(Alignment.bottomCenter);
    await windowManager.show();
    // OJO: NO llamamos a focus(): la app de destino debe conservar el foco
    // para que la inserción de texto funcione.
  }

  /// Expande a panel completo (centrado, con barra de título normal).
  Future<void> toPanel() async {
    state = WindowMode.panel;
    // El panel es una ventana normal: fondo sólido oscuro (no transparente).
    await windowManager.setBackgroundColor(const Color(0xFF121316));
    await windowManager.setAlwaysOnTop(false);
    await windowManager.setSkipTaskbar(false);
    await windowManager.setTitleBarStyle(TitleBarStyle.normal);
    await windowManager.setResizable(true);
    await windowManager.setSize(kPanelSize);
    await windowManager.center();
    await windowManager.show();
    await windowManager.focus();
  }
}
