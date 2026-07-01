import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:window_manager/window_manager.dart';

import '../dictation/dictation_controller.dart';
import '../dictation/dictation_state.dart';
import '../overlay/window_mode_controller.dart';
import 'parakeet_model_downloader.dart';

/// Tamaño de la ventana durante el splash de primer arranque. Lo justo para la
/// marca, el mensaje y la barra de progreso, centrado en pantalla.
const Size _kSplashSize = Size(440, 320);

/// Pref que marca que el motor ya se preparó con éxito al menos una vez. A
/// partir de ahí, los arranques normales NO muestran el splash (el motor carga
/// en segundo plano en unos segundos). Solo se re-muestra si falta el modelo
/// (p. ej. reinstalación o datos borrados).
const String _kReadyOnceKey = 'pronto_engine_ready_once';

/// Se pone en `true` cuando Pronto está LISTO para dictar: modelo descargado Y
/// motor cargado en memoria. Gatea el cierre del splash.
final modelReadyProvider = StateProvider<bool>((ref) => false);

/// ¿Debe mostrarse el splash al arrancar? Sí cuando falta el modelo (hay que
/// descargarlo) o en el primer arranque logrado (para cubrir la precarga del
/// motor con una pantalla de espera en vez de una píldora gris). Mientras se
/// resuelve devuelve null → no se decide aún (evita parpadeo).
final shouldSplashProvider = FutureProvider<bool>((ref) async {
  final present = (await ParakeetModelDownloader.resolveModelDir()) != null;
  if (!present) return true; // falta el modelo → descarga obligatoria
  final prefs = await SharedPreferences.getInstance();
  return !(prefs.getBool(_kReadyOnceKey) ?? false);
});

/// Pantalla de carga on-brand de instalación/primer arranque. La app NO se abre
/// hasta que TODO está listo: descarga el modelo si falta y LUEGO precarga el
/// motor (lo carga en memoria). Agranda y opaca la ventana (que en marcha normal
/// es una píldora transparente sin marco), muestra una barra de progreso MORADA
/// y, al terminar, restaura la píldora y deja seguir a la app.
class SplashScreen extends ConsumerStatefulWidget {
  const SplashScreen({super.key});

  @override
  ConsumerState<SplashScreen> createState() => _SplashScreenState();
}

class _SplashScreenState extends ConsumerState<SplashScreen> {
  /// Fase visible: false = descargando el modelo; true = precargando el motor.
  bool _precargando = false;

  @override
  void initState() {
    super.initState();
    // Preparamos la ventana y arrancamos el flujo tras el primer frame
    // (necesitamos el árbol montado para leer providers / navegar).
    WidgetsBinding.instance.addPostFrameCallback((_) async {
      await _prepararVentana();
      await _flujo();
    });
  }

  /// Saca la ventana del modo píldora: opaca, con marco, tamaño del splash y
  /// centrada, para que el usuario vea que la app SÍ está abriendo.
  Future<void> _prepararVentana() async {
    try {
      await windowManager.setBackgroundColor(const Color(0xFF0B0810));
      await windowManager.setAlwaysOnTop(false);
      await windowManager.setSkipTaskbar(false);
      await windowManager.setTitleBarStyle(TitleBarStyle.normal);
      await windowManager.setResizable(false);
      await windowManager.setSize(_kSplashSize);
      await windowManager.center();
      await windowManager.show();
      await windowManager.focus();
    } catch (_) {
      // Si window_manager falla, seguimos: el splash igualmente se dibuja.
    }
  }

  /// Prepara TODO antes de abrir Pronto: descarga (si falta) + precarga del motor.
  Future<void> _flujo() async {
    // 1) Descarga solo si falta el modelo.
    final presente = (await ParakeetModelDownloader.resolveModelDir()) != null;
    if (!presente) {
      await ref.read(parakeetDownloaderProvider.notifier).ensureModel();
      if (!mounted) return;
      if (ref.read(parakeetDownloaderProvider).phase != DownloadPhase.listo) {
        return; // error de descarga → se muestra el estado con "Reintentar"
      }
    }
    // 2) Precarga del motor: NO abrimos hasta que esté listo para dictar.
    await _precargarMotor();
  }

  /// Instancia el motor (dispara la carga del modelo en un isolate) y espera a
  /// que quede LISTO (el estado deja de ser "uninitialized"). Con tope de tiempo
  /// para no quedarnos atascados si el motor fallara.
  Future<void> _precargarMotor() async {
    if (!mounted) return;
    setState(() => _precargando = true);
    // Leer el controlador dispara su initialize() → carga del modelo en memoria.
    ref.read(dictationControllerProvider);
    final tope = DateTime.now().add(const Duration(seconds: 60));
    while (mounted && DateTime.now().isBefore(tope)) {
      final st = ref.read(dictationControllerProvider).status;
      if (st != DictationStatus.uninitialized) break; // idle (listo) o error
      await Future<void>.delayed(const Duration(milliseconds: 150));
    }
    if (!mounted) return;
    await _continuar();
  }

  /// Todo listo: marca el "ready once", vuelve al modo píldora y deja arrancar.
  Future<void> _continuar() async {
    try {
      final prefs = await SharedPreferences.getInstance();
      await prefs.setBool(_kReadyOnceKey, true);
    } catch (_) {}
    try {
      await ref.read(windowModeProvider.notifier).toPill();
    } catch (_) {
      // Aunque falle el ajuste de ventana, marcamos listo para no atascar.
    }
    if (!mounted) return;
    ref.read(modelReadyProvider.notifier).state = true;
  }

  static String _mb(int bytes) => (bytes / (1024 * 1024)).toStringAsFixed(0);

  @override
  Widget build(BuildContext context) {
    final state = ref.watch(parakeetDownloaderProvider);
    final esError = state.phase == DownloadPhase.error;

    final String subtitulo;
    if (esError) {
      subtitulo = state.error ?? 'Error de descarga';
    } else if (_precargando) {
      subtitulo = 'Cargando el motor de voz… ya casi está.';
    } else {
      subtitulo = 'Descargando el modelo de voz (solo la primera vez)…';
    }

    return Scaffold(
      backgroundColor: const Color(0xFF0B0810),
      body: Center(
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 32),
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              // Marca.
              ShaderMask(
                shaderCallback: (r) => const LinearGradient(
                  colors: [Color(0xFF8B5CF6), Color(0xFF6D28D9)],
                ).createShader(r),
                child: const Text(
                  'Pronto',
                  style: TextStyle(
                    fontSize: 40,
                    fontWeight: FontWeight.w800,
                    color: Colors.white,
                    letterSpacing: 0.5,
                  ),
                ),
              ),
              const SizedBox(height: 8),
              Text(
                esError ? 'No se pudo preparar Pronto' : 'Preparando Pronto…',
                style: const TextStyle(
                  color: Color(0xFFF5F3FF),
                  fontSize: 15,
                  fontWeight: FontWeight.w600,
                ),
              ),
              const SizedBox(height: 4),
              Text(
                subtitulo,
                textAlign: TextAlign.center,
                style: const TextStyle(color: Color(0xFF9C93AE), fontSize: 12),
              ),
              const SizedBox(height: 28),
              if (esError)
                FilledButton.icon(
                  onPressed: _flujo,
                  icon: const Icon(Icons.refresh_rounded),
                  label: const Text('Reintentar'),
                )
              else ...[
                // Barra de progreso MORADA: determinada en descarga (con total),
                // indeterminada en la precarga del motor.
                ClipRRect(
                  borderRadius: BorderRadius.circular(6),
                  child: LinearProgressIndicator(
                    value: _precargando
                        ? null
                        : (state.totalBytes > 0 ? state.progress : null),
                    minHeight: 8,
                    backgroundColor: const Color(0xFF161021),
                    valueColor: const AlwaysStoppedAnimation<Color>(
                      Color(0xFF8B5CF6),
                    ),
                  ),
                ),
                const SizedBox(height: 12),
                Text(
                  _precargando
                      ? 'Cargando en memoria…'
                      : (state.totalBytes > 0
                          ? '${_mb(state.receivedBytes)} / ${_mb(state.totalBytes)} MB'
                              '${state.currentFile != null ? '  ·  ${state.currentFile}' : ''}'
                          : 'Calculando descarga…'),
                  style: const TextStyle(color: Color(0xFFA78BFA), fontSize: 12),
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }
}
