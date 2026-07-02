import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:window_manager/window_manager.dart';

import '../overlay/window_mode_controller.dart';
import 'parakeet_model_downloader.dart';

/// Tamaño de la ventana durante el splash de primer arranque. Lo justo para la
/// marca, el mensaje y la barra de progreso, centrado en pantalla.
const Size _kSplashSize = Size(440, 320);

/// Se pone en `true` cuando el modelo ya está listo (presente o descargado) y
/// la app puede continuar al arranque normal (píldora/panel).
final modelReadyProvider = StateProvider<bool>((ref) => false);

/// Comprobación de arranque: ¿está el modelo Parakeet completo en disco? Se
/// resuelve una vez al abrir la app y gatea el splash. Mientras carga (o si
/// falla la comprobación) asumimos "presente" para NO tapar la app con el
/// splash por error.
final modelPresentProvider = FutureProvider<bool>((ref) async {
  return (await ParakeetModelDownloader.resolveModelDir()) != null;
});

/// Pantalla de carga de primer arranque, on-brand. Aparece SOLO cuando falta el
/// modelo de voz: agranda y opaca la ventana (que en marcha normal es una
/// píldora transparente sin marco), descarga el modelo mostrando una barra de
/// progreso MORADA y, al terminar, restaura la píldora y deja seguir a la app.
class SplashScreen extends ConsumerStatefulWidget {
  const SplashScreen({super.key});

  @override
  ConsumerState<SplashScreen> createState() => _SplashScreenState();
}

class _SplashScreenState extends ConsumerState<SplashScreen> {
  @override
  void initState() {
    super.initState();
    // Preparamos la ventana para el splash y arrancamos la descarga tras el
    // primer frame (necesitamos el árbol montado para navegar/leer providers).
    WidgetsBinding.instance.addPostFrameCallback((_) async {
      await _prepararVentana();
      await _descargar();
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

  Future<void> _descargar() async {
    await ref.read(parakeetDownloaderProvider.notifier).ensureModel();
    if (!mounted) return;
    final state = ref.read(parakeetDownloaderProvider);
    if (state.phase == DownloadPhase.listo) {
      await _continuar();
    }
  }

  /// Modelo listo: volvemos al modo píldora y dejamos que la app arranque.
  Future<void> _continuar() async {
    try {
      await ref.read(windowModeProvider.notifier).toPill();
    } catch (_) {
      // Aunque falle el ajuste de ventana, marcamos el modelo como listo para
      // no dejar al usuario atascado en el splash.
    }
    if (!mounted) return;
    ref.read(modelReadyProvider.notifier).state = true;
  }

  static String _mb(int bytes) => (bytes / (1024 * 1024)).toStringAsFixed(0);

  @override
  Widget build(BuildContext context) {
    final state = ref.watch(parakeetDownloaderProvider);
    final esError = state.phase == DownloadPhase.error;

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
                esError
                    ? 'No se pudo preparar Pronto'
                    : 'Preparando Pronto…',
                style: const TextStyle(
                  color: Color(0xFFF5F3FF),
                  fontSize: 15,
                  fontWeight: FontWeight.w600,
                ),
              ),
              const SizedBox(height: 4),
              Text(
                esError
                    ? (state.error ?? 'Error de descarga')
                    : 'Descargando el modelo de voz (solo la primera vez)…',
                textAlign: TextAlign.center,
                style: const TextStyle(color: Color(0xFF9C93AE), fontSize: 12),
              ),
              const SizedBox(height: 28),

              if (esError)
                FilledButton.icon(
                  onPressed: _descargar,
                  icon: const Icon(Icons.refresh_rounded),
                  label: const Text('Reintentar'),
                )
              else ...[
                // Barra de progreso MORADA (determinada mientras haya total).
                ClipRRect(
                  borderRadius: BorderRadius.circular(6),
                  child: LinearProgressIndicator(
                    value: state.totalBytes > 0 ? state.progress : null,
                    minHeight: 8,
                    backgroundColor: const Color(0xFF161021),
                    valueColor: const AlwaysStoppedAnimation<Color>(
                      Color(0xFF8B5CF6),
                    ),
                  ),
                ),
                const SizedBox(height: 12),
                Text(
                  state.totalBytes > 0
                      ? '${_mb(state.receivedBytes)} / ${_mb(state.totalBytes)} MB'
                          '${state.currentFile != null ? '  ·  ${state.currentFile}' : ''}'
                      : 'Calculando descarga…',
                  style: const TextStyle(
                    color: Color(0xFFA78BFA),
                    fontSize: 12,
                  ),
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }
}
