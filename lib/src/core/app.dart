import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:tray_manager/tray_manager.dart';
import 'package:window_manager/window_manager.dart';

import '../features/home/home_screen.dart';
import '../features/overlay/overlay_pill_screen.dart';
import '../features/overlay/window_mode_controller.dart';
import '../features/update/update_controller.dart';
import 'theme.dart';

class ProntoApp extends ConsumerWidget {
  const ProntoApp({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return MaterialApp(
      title: 'Pronto',
      debugShowCheckedModeBanner: false,
      theme: buildProntoTheme(),
      home: const _Root(),
    );
  }
}

/// Raíz: alterna entre barra flotante y panel, y gestiona el icono de bandeja
/// (segundo plano) y el cierre-a-bandeja.
class _Root extends ConsumerStatefulWidget {
  const _Root();

  @override
  ConsumerState<_Root> createState() => _RootState();
}

class _RootState extends ConsumerState<_Root> with TrayListener, WindowListener {
  @override
  void initState() {
    super.initState();
    trayManager.addListener(this);
    windowManager.addListener(this);
    _setupTray();
    _maybeOpenPanelOnFirstRun();
    // Instancia el auto-actualizador al arrancar, SIEMPRE (aunque empieces en el
    // panel y no en la píldora): su build() programa la comprobación inicial y
    // el re-chequeo periódico. Antes solo se creaba al observarlo la píldora, así
    // que si arrancabas en el panel el aviso (punto morado) no aparecía solo.
    ref.read(updateProvider.notifier);
  }

  /// La primera vez que se abre la app mostramos el PANEL (no la píldora) para
  /// que el usuario elija el modo de dictado de un vistazo. A partir de ahí
  /// arranca en la píldora como siempre.
  Future<void> _maybeOpenPanelOnFirstRun() async {
    try {
      final prefs = await SharedPreferences.getInstance();
      const key = 'pronto_first_run_done';
      if (prefs.getBool(key) ?? false) return;
      await prefs.setBool(key, true);
      if (!mounted) return;
      await ref.read(windowModeProvider.notifier).toPanel();
    } catch (_) {
      // Si falla la lectura de prefs, arrancamos en píldora como siempre.
    }
  }

  Future<void> _setupTray() async {
    try {
      // Interceptamos el cierre para esconder a bandeja en vez de salir.
      await windowManager.setPreventClose(true);
      await trayManager.setIcon('assets/app_icon.ico');
      await trayManager.setToolTip('Pronto — dictado por voz');
      await trayManager.setContextMenu(
        Menu(
          items: [
            MenuItem(key: 'show', label: 'Abrir panel'),
            MenuItem(key: 'pill', label: 'Barra flotante'),
            MenuItem.separator(),
            MenuItem(key: 'quit', label: 'Salir'),
          ],
        ),
      );
    } catch (e) {
      debugPrint('No se pudo iniciar la bandeja del sistema: $e');
    }
  }

  @override
  void dispose() {
    trayManager.removeListener(this);
    windowManager.removeListener(this);
    super.dispose();
  }

  // --- Bandeja ---
  @override
  void onTrayIconMouseDown() {
    ref.read(windowModeProvider.notifier).toPanel();
  }

  @override
  void onTrayIconRightMouseDown() {
    trayManager.popUpContextMenu();
  }

  @override
  void onTrayMenuItemClick(MenuItem menuItem) {
    switch (menuItem.key) {
      case 'show':
        ref.read(windowModeProvider.notifier).toPanel();
      case 'pill':
        _collapseToPill();
      case 'quit':
        _quit();
    }
  }

  Future<void> _quit() async {
    await windowManager.setPreventClose(false);
    await windowManager.destroy();
  }

  /// Vuelve a la píldora cerrando antes cualquier pantalla abierta encima
  /// (p. ej. Ajustes). Si no se cierra, su barra "← Ajustes" se quedaría
  /// incrustada y tapando la píldora cuando la ventana se encoge.
  Future<void> _collapseToPill() async {
    final nav = Navigator.of(context);
    if (nav.canPop()) nav.popUntil((route) => route.isFirst);
    await ref.read(windowModeProvider.notifier).toPill();
  }

  // --- Ventana ---
  // Al cerrar el panel volvemos a la barra flotante: la app sigue viva en
  // segundo plano (bandeja). Para salir de verdad, usa "Salir" en la bandeja.
  @override
  void onWindowClose() {
    _collapseToPill();
  }

  @override
  Widget build(BuildContext context) {
    final mode = ref.watch(windowModeProvider);

    if (mode == WindowMode.pill) {
      return const OverlayPillScreen();
    }

    return Stack(
      children: [
        const HomeScreen(),
        Positioned(
          right: 14,
          bottom: 14,
          child: FloatingActionButton.small(
            heroTag: 'toPill',
            tooltip: 'Minimizar a barra flotante',
            onPressed: () => _collapseToPill(),
            child: const Icon(Icons.compress),
          ),
        ),
      ],
    );
  }
}
