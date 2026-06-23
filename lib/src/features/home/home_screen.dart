import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../common/dictation_pill.dart';
import '../../platform/global_hotkey_service.dart';
import '../dictation/dictation_controller.dart';
import '../dictation/dictation_state.dart';
import '../settings/settings_controller.dart';
import '../settings/settings_screen.dart';
import '../update/update_controller.dart';

/// Pantalla principal de Pronto.
///
/// Reúne el estado del dictado (píldora), las instrucciones de uso, un botón
/// manual de "mantener para dictar", el campo editable de la última
/// transcripción (núcleo del ciclo de automejora) y los avisos de estado.
///
/// Es un [ConsumerStatefulWidget] porque necesita gestionar el ciclo de vida
/// de un [TextEditingController] (crearlo y liberarlo con dispose) y sincronizar
/// su contenido con la última transcripción que produce el controlador.
class HomeScreen extends ConsumerStatefulWidget {
  const HomeScreen({super.key});

  @override
  ConsumerState<HomeScreen> createState() => _HomeScreenState();
}

class _HomeScreenState extends ConsumerState<HomeScreen> {
  final TextEditingController _correctionCtrl = TextEditingController();

  /// Último texto que volcamos al campo, para no pisar ediciones del usuario
  /// en cada rebuild (solo actualizamos cuando llega una transcripción nueva).
  String? _syncedText;

  @override
  void dispose() {
    _correctionCtrl.dispose();
    super.dispose();
  }

  void _syncFieldWithLastText(DictationState state) {
    final last = state.lastText;
    if (last != null && last != _syncedText) {
      _syncedText = last;
      _correctionCtrl.text = last;
      // Coloca el cursor al final del texto recién insertado.
      _correctionCtrl.selection = TextSelection.fromPosition(
        TextPosition(offset: _correctionCtrl.text.length),
      );
    }
  }

  Future<void> _guardarCorreccion() async {
    final texto = _correctionCtrl.text.trim();
    if (texto.isEmpty) return;
    await ref
        .read(dictationControllerProvider.notifier)
        .submitCorrection(texto);
    if (!mounted) return;
    ScaffoldMessenger.of(context)
      ..clearSnackBars()
      ..showSnackBar(
        const SnackBar(
          content: Text('Corrección guardada. Pronto aprenderá de ella.'),
          behavior: SnackBarBehavior.floating,
        ),
      );
  }

  @override
  Widget build(BuildContext context) {
    final state = ref.watch(dictationControllerProvider);
    final version = ref.watch(appVersionProvider).valueOrNull;

    // Sincroniza el campo editable cuando hay transcripción nueva.
    _syncFieldWithLastText(state);

    final scheme = Theme.of(context).colorScheme;

    return Scaffold(
      appBar: AppBar(
        title: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Image.asset(
              'assets/branding/pronto_icon.png',
              width: 26,
              height: 26,
              filterQuality: FilterQuality.high,
            ),
            const SizedBox(width: 10),
            const Text(
              'Pronto',
              style: TextStyle(fontWeight: FontWeight.w700, letterSpacing: 0.2),
            ),
            if (version != null) ...[
              const SizedBox(width: 10),
              Container(
                padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
                decoration: BoxDecoration(
                  color: scheme.primary.withValues(alpha: 0.15),
                  borderRadius: BorderRadius.circular(8),
                ),
                child: Text(
                  'v$version',
                  style: TextStyle(
                    fontSize: 12.5,
                    fontWeight: FontWeight.w600,
                    color: scheme.primary,
                  ),
                ),
              ),
            ],
          ],
        ),
        actions: [
          IconButton(
            tooltip: 'Configuración',
            icon: const Icon(Icons.settings_outlined),
            onPressed: () => Navigator.of(context).push(
              MaterialPageRoute<void>(
                builder: (_) => const SettingsScreen(),
              ),
            ),
          ),
        ],
      ),
      body: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 560),
          child: SingleChildScrollView(
            padding: const EdgeInsets.all(24),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                // Aviso de actualización disponible (auto-actualizador).
                const _UpdateBanner(),

                // Aviso si el modelo aún no está cargado.
                if (state.status == DictationStatus.uninitialized) ...[
                  const _ModeloNoCargadoBanner(),
                  const SizedBox(height: 20),
                ],

                // Aviso de error (si lo hay).
                if (state.error != null) ...[
                  _ErrorBanner(mensaje: state.error!),
                  const SizedBox(height: 20),
                ],

                // Estado del dictado.
                const Center(child: DictationPill()),
                const SizedBox(height: 28),

                // Selector de modo de disparo + instrucciones, sin entrar en
                // Ajustes: el usuario elige Alternar o Pulsar y hablar aquí.
                const _ModoDictadoCard(),
                const SizedBox(height: 20),

                // Botón manual "mantener para dictar".
                _BotonProbarDictado(status: state.status),
                const SizedBox(height: 28),

                // Campo editable + automejora.
                _CorreccionSection(
                  controller: _correctionCtrl,
                  hayTranscripcion: state.lastText != null,
                  onGuardar: _guardarCorreccion,
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// Selector del modo de disparo (Alternar / Pulsar y hablar) con instrucciones
/// que se adaptan al modo elegido. Permite cambiarlo sin entrar en Ajustes.
class _ModoDictadoCard extends ConsumerWidget {
  const _ModoDictadoCard();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final theme = Theme.of(context);
    final scheme = theme.colorScheme;
    final settings = ref.watch(settingsProvider);
    final controller = ref.read(settingsProvider.notifier);
    final combo = settings.hotkey.describe();
    final esToggle = settings.triggerMode == TriggerMode.toggle;

    return Card(
      elevation: 0,
      color: scheme.surfaceContainerHigh,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(16)),
      child: Padding(
        padding: const EdgeInsets.all(18),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Icon(Icons.keyboard_command_key_rounded,
                    color: scheme.primary, size: 24,),
                const SizedBox(width: 10),
                Text(
                  'Cómo quieres dictar',
                  style: theme.textTheme.titleMedium
                      ?.copyWith(fontWeight: FontWeight.w700),
                ),
              ],
            ),
            const SizedBox(height: 14),
            SizedBox(
              width: double.infinity,
              child: SegmentedButton<TriggerMode>(
                segments: const [
                  ButtonSegment(
                    value: TriggerMode.toggle,
                    label: Text('Alternar'),
                    icon: Icon(Icons.repeat_rounded),
                  ),
                  ButtonSegment(
                    value: TriggerMode.hold,
                    label: Text('Pulsar y hablar'),
                    icon: Icon(Icons.mic_none_rounded),
                  ),
                ],
                selected: {settings.triggerMode},
                showSelectedIcon: false,
                onSelectionChanged: (sel) =>
                    controller.setTriggerMode(sel.first),
              ),
            ),
            const SizedBox(height: 14),
            Text.rich(
              TextSpan(
                style: theme.textTheme.bodyMedium
                    ?.copyWith(color: scheme.onSurfaceVariant),
                children: [
                  TextSpan(text: esToggle ? 'Pulsa ' : 'Mantén pulsado '),
                  TextSpan(
                    text: combo,
                    style: const TextStyle(fontWeight: FontWeight.w700),
                  ),
                  TextSpan(
                    text: esToggle
                        ? ' una vez para empezar y otra vez para parar, en '
                            'CUALQUIER app. El texto se escribe donde tengas '
                            'el cursor.'
                        : ' mientras hablas y suéltalo, en CUALQUIER app. El '
                            'texto se escribe donde tengas el cursor.',
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// Botón de "mantener para dictar" (push-to-talk manual, sin atajo global).
class _BotonProbarDictado extends ConsumerWidget {
  const _BotonProbarDictado({required this.status});

  final DictationStatus status;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final theme = Theme.of(context);
    final scheme = theme.colorScheme;
    final grabando = status == DictationStatus.recording;
    final habilitado = status == DictationStatus.idle || grabando;

    final notifier = ref.read(dictationControllerProvider.notifier);

    return GestureDetector(
      // Mantener pulsado -> grabar; soltar -> transcribir e insertar.
      onTapDown: habilitado ? (_) => notifier.startRecording() : null,
      onTapUp: habilitado ? (_) => notifier.stopAndProcess() : null,
      // Si el dedo/cursor sale del botón también finalizamos para no quedar
      // colgados grabando.
      onTapCancel: grabando ? () => notifier.stopAndProcess() : null,
      child: AnimatedContainer(
        duration: const Duration(milliseconds: 200),
        height: 56,
        decoration: BoxDecoration(
          color: grabando
              ? const Color(0xFFE5484D)
              : (habilitado
                  ? scheme.primaryContainer
                  : scheme.surfaceContainerHighest),
          borderRadius: BorderRadius.circular(14),
        ),
        alignment: Alignment.center,
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              grabando ? Icons.mic_rounded : Icons.touch_app_rounded,
              color: grabando ? Colors.white : scheme.onPrimaryContainer,
            ),
            const SizedBox(width: 10),
            Text(
              grabando ? 'Suelta para transcribir' : 'Probar dictado',
              style: theme.textTheme.titleMedium?.copyWith(
                color: grabando ? Colors.white : scheme.onPrimaryContainer,
                fontWeight: FontWeight.w700,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// Sección con el campo editable de la última transcripción y el botón de
/// guardar corrección (el ciclo de automejora).
class _CorreccionSection extends StatelessWidget {
  const _CorreccionSection({
    required this.controller,
    required this.hayTranscripcion,
    required this.onGuardar,
  });

  final TextEditingController controller;
  final bool hayTranscripcion;
  final Future<void> Function() onGuardar;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final scheme = theme.colorScheme;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            Icon(Icons.auto_fix_high_rounded,
                size: 20, color: scheme.primary,),
            const SizedBox(width: 8),
            Text(
              'Última transcripción',
              style: theme.textTheme.titleMedium
                  ?.copyWith(fontWeight: FontWeight.w700),
            ),
          ],
        ),
        const SizedBox(height: 8),
        Text(
          'Edita el texto si Whisper se equivocó y pulsa "Guardar corrección". '
          'Pronto aprenderá del cambio y mejorará la próxima vez.',
          style: theme.textTheme.bodySmall
              ?.copyWith(color: scheme.onSurfaceVariant),
        ),
        const SizedBox(height: 12),
        TextField(
          controller: controller,
          maxLines: 5,
          minLines: 3,
          decoration: InputDecoration(
            hintText: hayTranscripcion
                ? null
                : 'Aún no hay transcripción. Dicta algo para empezar.',
            filled: true,
            fillColor: scheme.surfaceContainerHigh,
            border: OutlineInputBorder(
              borderRadius: BorderRadius.circular(14),
              borderSide: BorderSide.none,
            ),
          ),
        ),
        const SizedBox(height: 12),
        Align(
          alignment: Alignment.centerRight,
          child: FilledButton.icon(
            onPressed: hayTranscripcion ? () => onGuardar() : null,
            icon: const Icon(Icons.save_outlined),
            label: const Text('Guardar corrección'),
          ),
        ),
      ],
    );
  }
}

/// Banner que avisa de que el modelo Whisper no está cargado y enlaza la guía.
class _ModeloNoCargadoBanner extends StatelessWidget {
  const _ModeloNoCargadoBanner();

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final scheme = theme.colorScheme;
    return Container(
      padding: const EdgeInsets.all(16),
      decoration: BoxDecoration(
        color: scheme.tertiaryContainer.withValues(alpha: 0.45),
        borderRadius: BorderRadius.circular(14),
        border: Border.all(color: scheme.tertiary.withValues(alpha: 0.5)),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Icon(Icons.cloud_download_outlined, color: scheme.tertiary),
          const SizedBox(width: 12),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  'El modelo de voz no está cargado',
                  style: theme.textTheme.titleSmall
                      ?.copyWith(fontWeight: FontWeight.w700),
                ),
                const SizedBox(height: 4),
                Text(
                  'Descarga/compila el modelo: ver BUILD_WHISPER.md y SETUP.md',
                  style: theme.textTheme.bodySmall
                      ?.copyWith(color: scheme.onSurfaceVariant),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

/// Banner de error legible.
class _ErrorBanner extends StatelessWidget {
  const _ErrorBanner({required this.mensaje});

  final String mensaje;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final scheme = theme.colorScheme;
    return Container(
      padding: const EdgeInsets.all(16),
      decoration: BoxDecoration(
        color: scheme.errorContainer.withValues(alpha: 0.5),
        borderRadius: BorderRadius.circular(14),
        border: Border.all(color: scheme.error.withValues(alpha: 0.5)),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Icon(Icons.error_outline_rounded, color: scheme.error),
          const SizedBox(width: 12),
          Expanded(
            child: Text(
              mensaje,
              style: theme.textTheme.bodyMedium
                  ?.copyWith(color: scheme.onErrorContainer),
            ),
          ),
        ],
      ),
    );
  }
}

/// Aviso de actualización disponible: muestra la versión nueva y un botón que
/// descarga e instala solo. Se oculta si no hay actualización.
class _UpdateBanner extends ConsumerWidget {
  const _UpdateBanner();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final upd = ref.watch(updateProvider);
    final theme = Theme.of(context);
    final scheme = theme.colorScheme;

    final show = upd.status == UpdateStatus.available ||
        upd.status == UpdateStatus.downloading ||
        upd.status == UpdateStatus.installing;
    if (!show) return const SizedBox.shrink();

    final downloading = upd.status == UpdateStatus.downloading;
    final installing = upd.status == UpdateStatus.installing;

    return Padding(
      padding: const EdgeInsets.only(bottom: 20),
      child: Container(
        padding: const EdgeInsets.all(16),
        decoration: BoxDecoration(
          color: scheme.primaryContainer.withValues(alpha: 0.45),
          borderRadius: BorderRadius.circular(14),
          border: Border.all(color: scheme.primary.withValues(alpha: 0.55)),
        ),
        child: Row(
          children: [
            Icon(Icons.system_update_alt_rounded, color: scheme.primary),
            const SizedBox(width: 12),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    installing
                        ? 'Instalando la actualización…'
                        : downloading
                            ? 'Descargando la actualización…'
                            : 'Hay una actualización disponible',
                    style: theme.textTheme.titleSmall
                        ?.copyWith(fontWeight: FontWeight.w700),
                  ),
                  const SizedBox(height: 2),
                  Text(
                    installing
                        ? 'Pronto se cerrará y se reiniciará solo.'
                        : downloading
                            ? '${(upd.progress * 100).clamp(0, 100).toStringAsFixed(0)} %'
                            : 'Versión ${upd.latestVersion} (tienes la ${upd.currentVersion}).',
                    style: theme.textTheme.bodySmall
                        ?.copyWith(color: scheme.onSurfaceVariant),
                  ),
                  if (downloading) ...[
                    const SizedBox(height: 8),
                    ClipRRect(
                      borderRadius: BorderRadius.circular(6),
                      child: LinearProgressIndicator(
                        value: upd.progress > 0 ? upd.progress : null,
                        minHeight: 6,
                      ),
                    ),
                  ],
                ],
              ),
            ),
            if (upd.status == UpdateStatus.available) ...[
              const SizedBox(width: 12),
              FilledButton.icon(
                onPressed: () =>
                    ref.read(updateProvider.notifier).downloadAndInstall(),
                icon: const Icon(Icons.download_rounded, size: 18),
                label: const Text('Actualizar'),
              ),
            ],
          ],
        ),
      ),
    );
  }
}
