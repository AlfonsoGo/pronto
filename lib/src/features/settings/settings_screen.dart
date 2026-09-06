import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:http/http.dart' as http;
import 'package:path/path.dart' as p;
import 'package:path_provider/path_provider.dart';

import '../../core/config.dart';
import '../../platform/audio_capture.dart';
import '../../platform/global_hotkey_service.dart';
import '../../platform/platform_services.dart';
import '../../platform/text_injector.dart';
import '../formatting/cloud_budget_provider.dart';
import '../formatting/cloud_cost.dart';
import '../formatting/cloud_key.dart';
import '../formatting/text_formatter.dart';
import '../onboarding/onboarding_progress.dart';
import '../translation/language_detect.dart';
import '../translation/translator.dart';
import '../update/update_controller.dart';
import 'hotkey_recorder.dart';
import 'settings_controller.dart';

/// Pantalla de ajustes de Pronto (Material 3).
///
/// Lee y escribe el estado a través de [settingsProvider]. Cada control
/// invoca el actualizador correspondiente del [SettingsController], que ya
/// persiste el cambio.
class SettingsScreen extends ConsumerWidget {
  const SettingsScreen({super.key});

  /// Modelos Whisper disponibles para elegir.
  static const List<({String file, String label})> _models = [
    (file: 'ggml-small.bin', label: 'Small (rápido, equilibrado)'),
    (
      file: 'ggml-large-v3-turbo.bin',
      label: 'Large v3 Turbo (máxima calidad)'
    ),
  ];

  /// Retenciones del Historial que se ofrecen. El 0 es «para siempre» y va el
  /// último a propósito: es la que hay que elegir a conciencia.
  static const List<int> _retentionOptions = [7, 30, 90, 365, 0];

  /// Idiomas disponibles (código -> etiqueta).
  static const List<({String code, String label})> _languages = [
    (code: 'es', label: 'Español'),
    (code: 'en', label: 'Inglés'),
    (code: 'auto', label: 'Automático'),
  ];

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final settings = ref.watch(settingsProvider);
    final controller = ref.read(settingsProvider.notifier);
    final theme = Theme.of(context);

    return Scaffold(
      appBar: AppBar(
        title: const Text('Ajustes'),
        leading: BackButton(onPressed: () => Navigator.of(context).maybePop()),
      ),
      body: ListView(
        padding: const EdgeInsets.symmetric(vertical: 8),
        children: [
          // --- Motor de voz ---
          _SectionHeader('Motor de voz', theme: theme),
          const _EngineTile(),
          const _WhisperModelTile(),
          const _MicDeviceTile(),

          const Divider(height: 24),

          // --- Transcripción ---
          _SectionHeader('Transcripción', theme: theme),

          ListTile(
            title: const Text('Modelo Whisper'),
            subtitle: const Text('Modelo on-device usado para transcribir'),
            trailing: DropdownButton<String>(
              value: _models.any((m) => m.file == settings.modelFile)
                  ? settings.modelFile
                  : _models.first.file,
              onChanged: (value) {
                if (value != null) controller.setModelFile(value);
              },
              items: [
                for (final m in _models)
                  DropdownMenuItem(value: m.file, child: Text(m.label)),
              ],
            ),
          ),

          ListTile(
            title: const Text('Idioma'),
            subtitle: const Text('Idioma del dictado'),
            trailing: DropdownButton<String>(
              value: _languages.any((l) => l.code == settings.language)
                  ? settings.language
                  : _languages.first.code,
              onChanged: (value) {
                if (value != null) controller.setLanguage(value);
              },
              items: [
                for (final l in _languages)
                  DropdownMenuItem(value: l.code, child: Text(l.label)),
              ],
            ),
          ),

          SwitchListTile(
            title: const Text('Pulir el texto automáticamente'),
            subtitle: const Text(
              'Puntuación dictada ("coma", "nueva línea"), signos ¿ ¡, '
              'mayúsculas y números a dígitos (25 %).',
            ),
            value: settings.textPolish,
            onChanged: controller.setTextPolish,
          ),

          const Divider(height: 24),

          // --- Inserción y disparo ---
          _SectionHeader('Inserción y disparo', theme: theme),

          ListTile(
            title: const Text('Modo de inserción'),
            subtitle: const Text('Cómo se escribe el texto en la app activa'),
            trailing: DropdownButton<InjectionMode>(
              value: settings.injectionMode,
              onChanged: (value) {
                if (value != null) controller.setInjectionMode(value);
              },
              items: const [
                DropdownMenuItem(
                  value: InjectionMode.clipboardPaste,
                  child: Text('Pegar (portapapeles)'),
                ),
                DropdownMenuItem(
                  value: InjectionMode.unicodeSendInput,
                  child: Text('Tecleo Unicode'),
                ),
              ],
            ),
          ),

          ListTile(
            title: const Text('Modo de disparo'),
            subtitle: const Text('Cómo se inicia y detiene el dictado'),
            trailing: DropdownButton<TriggerMode>(
              value: settings.triggerMode,
              onChanged: (value) {
                if (value != null) controller.setTriggerMode(value);
              },
              items: const [
                DropdownMenuItem(
                  value: TriggerMode.hold,
                  child: Text('Mantener pulsado'),
                ),
                DropdownMenuItem(
                  value: TriggerMode.toggle,
                  child: Text('Alternar'),
                ),
              ],
            ),
          ),

          ListTile(
            title: const Text('Atajo para dictar'),
            subtitle: const Text('Pulsa el botón y teclea tu combinación'),
            trailing: HotkeyRecorderField(
              value: settings.hotkey,
              onChanged: controller.setHotkey,
            ),
          ),

          ListTile(
            title: const Text('Atajo para cambiar de idioma'),
            subtitle: Text(
              'Alterna entre pegar en español y en inglés '
              '(ahora: ${settings.outputLang.label.toLowerCase()})',
            ),
            trailing: HotkeyRecorderField(
              value: settings.translateHotkey,
              onChanged: controller.setTranslateHotkey,
            ),
          ),

          const Divider(height: 24),

          // --- Traducción ---
          _SectionHeader('Traducción (español ⇄ inglés)', theme: theme),
          const _TranslationTiles(),

          const Divider(height: 24),

          // --- Apariencia ---
          _SectionHeader('Apariencia', theme: theme),
          const _PillSizeTile(),

          SwitchListTile(
            title: const Text('Sonidos al grabar'),
            subtitle: const Text(
              'Un tono al empezar y otro al parar de dictar',
            ),
            value: settings.sounds,
            onChanged: controller.setSounds,
          ),

          const Divider(height: 24),

          // --- Sistema ---
          _SectionHeader('Sistema', theme: theme),

          SwitchListTile(
            title: const Text('Iniciar con el sistema'),
            subtitle: const Text('Abrir Pronto al arrancar Windows'),
            value: settings.autostart,
            onChanged: controller.setAutostart,
          ),

          const _UpdateTile(),
          const _RepetirInstalacionTile(),

          const Divider(height: 24),

          // --- Formateo del dictado ---
          _SectionHeader('Formateo del dictado (opcional)', theme: theme),

          // Los tres caminos del ticket PRNT-1. La nube es el único que saca
          // algo del equipo, y solo el TEXTO ya transcrito: nunca el audio.
          ListTile(
            title: const Text('Camino de formateo'),
            subtitle: const Text(
              'Sin formateo pega tal cual y es instantáneo. Local arregla '
              'puntuación, muletillas, mayúsculas y tildes sin salir del '
              'equipo, a cambio de algo de espera. Nube manda el texto '
              'dictado a un proveedor con tu clave: es el que mejor escribe '
              'en español, y el único que sale de tu ordenador',
            ),
            trailing: DropdownButton<FormatPath>(
              key: const ValueKey('formatPath'),
              value: settings.formatPath,
              onChanged: (value) {
                if (value != null) controller.setFormatPath(value);
              },
              items: const [
                DropdownMenuItem(
                  value: FormatPath.none,
                  child: Text('Sin formateo'),
                ),
                DropdownMenuItem(
                  value: FormatPath.local,
                  child: Text('Local (Ollama)'),
                ),
                DropdownMenuItem(
                  value: FormatPath.cloud,
                  child: Text('Nube (OpenRouter)'),
                ),
              ],
            ),
          ),

          Padding(
            padding: const EdgeInsets.fromLTRB(16, 8, 16, 0),
            child: _LlmTextField(
              key: const ValueKey('llmBaseUrl'),
              label: 'URL base (servidor local, también para traducir)',
              hint: 'http://localhost:11434',
              initialValue: settings.llmBaseUrl,
              // La traducción usa el MISMO servidor: si está activada, hay que
              // poder editar la URL aunque el formateo esté apagado.
              enabled: settings.formatPath != FormatPath.none ||
                  settings.translationEnabled,
              keyboardType: TextInputType.url,
              onSubmitted: controller.setLlmBaseUrl,
            ),
          ),
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 12, 16, 8),
            child: _LlmTextField(
              key: const ValueKey('formatModel'),
              label: 'Modelo de formateo',
              hint: AppConfig.defaultFormatLocalModel,
              initialValue: settings.formatModel,
              enabled: settings.formatPath == FormatPath.local,
              onSubmitted: controller.setFormatModel,
            ),
          ),

          if (settings.formatPath == FormatPath.cloud) const _CloudTiles(),

          const Divider(height: 24),

          // --- Aprendizaje ---
          _SectionHeader('Aprendizaje', theme: theme),

          SwitchListTile(
            title: const Text('Aprender de ediciones externas'),
            subtitle: const Text(
              'Si corriges el texto en otra app y lo copias, Pronto lo aprende',
            ),
            value: settings.captureExternalEdits,
            onChanged: controller.setCaptureExternalEdits,
          ),

          _SectionHeader('Historial', theme: theme),

          ListTile(
            title: const Text('Cuánto se guarda'),
            subtitle: Text(
              settings.historyRetentionDays == 0
                  ? 'Para siempre. En pronto.db, sin cifrar, queda todo lo que '
                      'dictes: correos, nombres, lo que digas en voz alta.'
                  : 'Los dictados de más de ${settings.historyRetentionDays} '
                      'días se borran solos al arrancar Pronto.',
            ),
            trailing: DropdownButton<int>(
              value: _retentionOptions.contains(settings.historyRetentionDays)
                  ? settings.historyRetentionDays
                  : null,
              hint: Text('${settings.historyRetentionDays} días'),
              items: const [
                DropdownMenuItem(value: 7, child: Text('7 días')),
                DropdownMenuItem(value: 30, child: Text('30 días')),
                DropdownMenuItem(value: 90, child: Text('90 días')),
                DropdownMenuItem(value: 365, child: Text('1 año')),
                DropdownMenuItem(value: 0, child: Text('Para siempre')),
              ],
              onChanged: (v) {
                if (v != null) controller.setHistoryRetentionDays(v);
              },
            ),
          ),

          const SizedBox(height: 24),
        ],
      ),
    );
  }
}

/// Bloque de ajustes de la traducción español ⇄ inglés.
///
/// Traducir necesita un servidor LLM (Ollama local por defecto, el mismo que la
/// post-corrección), por eso viene desactivada y hay un botón para comprobar
/// que responde antes de fiarse de ella.
class _TranslationTiles extends ConsumerStatefulWidget {
  const _TranslationTiles();

  @override
  ConsumerState<_TranslationTiles> createState() => _TranslationTilesState();
}

class _TranslationTilesState extends ConsumerState<_TranslationTiles> {
  bool _checking = false;

  /// Resultado de la última comprobación: null = aún no se ha probado.
  String? _result;
  bool _ok = false;

  Future<void> _check() async {
    setState(() {
      _checking = true;
      _result = null;
    });
    // Se construye a mano (y no con el provider) para poder probar aunque la
    // traducción esté desactivada: es justo lo que quieres hacer ANTES de
    // encenderla.
    final s = ref.read(settingsProvider);
    final probe = Translator(
      enabled: true,
      baseUrl: s.llmBaseUrl,
      model: s.translationModel,
    );
    try {
      final error = await probe.checkConnection();
      if (!mounted) return;
      setState(() {
        _checking = false;
        _ok = error == null;
        _result = error ?? 'Conectado. El modelo «${s.translationModel}» '
            'traduce correctamente.';
      });
    } finally {
      probe.dispose();
    }
  }

  @override
  Widget build(BuildContext context) {
    final settings = ref.watch(settingsProvider);
    final controller = ref.read(settingsProvider.notifier);
    final scheme = Theme.of(context).colorScheme;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SwitchListTile(
          title: const Text('Traducir al dictar'),
          subtitle: const Text(
            'Pronto pega el dictado en tu idioma por defecto y deja la otra '
            'versión a mano en la píldora. Necesita un servidor LLM.',
          ),
          isThreeLine: true,
          value: settings.translationEnabled,
          onChanged: controller.setTranslationEnabled,
        ),

        ListTile(
          enabled: settings.translationEnabled,
          leading: const Icon(Icons.translate_rounded, color: Color(0xFF8B5CF6)),
          title: const Text('Idioma al pegar'),
          subtitle: Text(
            'Si dictas en el otro idioma, Pronto traduce antes de pegar. '
            'Atajo: ${settings.translateHotkey.describe()}',
          ),
          isThreeLine: true,
          trailing: DropdownButton<TextLanguage>(
            value: settings.outputLang,
            onChanged: settings.translationEnabled
                ? (v) {
                    if (v != null) controller.setOutputLanguage(v);
                  }
                : null,
            items: [
              for (final l in TextLanguage.values)
                DropdownMenuItem(value: l, child: Text(l.label)),
            ],
          ),
        ),

        Padding(
          padding: const EdgeInsets.fromLTRB(16, 4, 16, 8),
          child: _LlmTextField(
            key: const ValueKey('translationModel'),
            label: 'Modelo de traducción',
            hint: AppConfig.defaultTranslationModel,
            initialValue: settings.translationModel,
            enabled: settings.translationEnabled,
            onSubmitted: controller.setTranslationModel,
          ),
        ),

        ListTile(
          title: const Text('Probar la traducción'),
          subtitle: Text(
            _result ??
                'Comprueba que ${settings.llmBaseUrl} responde y que el '
                    'modelo existe.',
            style: _result == null
                ? null
                : TextStyle(
                    color: _ok ? const Color(0xFF34C759) : scheme.error,
                  ),
          ),
          isThreeLine: _result != null,
          trailing: _checking
              ? const SizedBox(
                  width: 18,
                  height: 18,
                  child: CircularProgressIndicator(strokeWidth: 2),
                )
              : const Icon(Icons.play_circle_outline_rounded),
          onTap: _checking ? null : _check,
        ),
      ],
    );
  }
}

/// Cabecera de sección reutilizable.
class _SectionHeader extends StatelessWidget {
  final String title;
  final ThemeData theme;

  const _SectionHeader(this.title, {required this.theme});

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 12, 16, 4),
      child: Text(
        title,
        style: theme.textTheme.titleSmall?.copyWith(
          color: theme.colorScheme.primary,
          fontWeight: FontWeight.bold,
        ),
      ),
    );
  }
}

/// Campo de texto del LLM: guarda al perder el foco o al pulsar Enter.
///
/// Mantiene su propio [TextEditingController] para no reconstruir el texto
/// en cada pulsación (lo que movería el cursor); se sincroniza con el valor
/// persistido solo cuando cambia desde fuera.
class _LlmTextField extends StatefulWidget {
  final String label;
  final String hint;
  final String initialValue;
  final bool enabled;
  final TextInputType? keyboardType;
  final ValueChanged<String> onSubmitted;

  const _LlmTextField({
    super.key,
    required this.label,
    required this.hint,
    required this.initialValue,
    required this.enabled,
    required this.onSubmitted,
    this.keyboardType,
  });

  @override
  State<_LlmTextField> createState() => _LlmTextFieldState();
}

class _LlmTextFieldState extends State<_LlmTextField> {
  late final TextEditingController _ctrl;
  late final FocusNode _focus;

  @override
  void initState() {
    super.initState();
    _ctrl = TextEditingController(text: widget.initialValue);
    _focus = FocusNode();
    // Al perder el foco, persistimos si hubo cambios.
    _focus.addListener(() {
      if (!_focus.hasFocus) _commit();
    });
  }

  @override
  void didUpdateWidget(covariant _LlmTextField oldWidget) {
    super.didUpdateWidget(oldWidget);
    // Sincroniza solo si el valor cambió externamente y el campo no se edita.
    if (widget.initialValue != _ctrl.text && !_focus.hasFocus) {
      _ctrl.text = widget.initialValue;
    }
  }

  void _commit() {
    final value = _ctrl.text.trim();
    if (value.isNotEmpty && value != widget.initialValue) {
      widget.onSubmitted(value);
    }
  }

  @override
  void dispose() {
    _ctrl.dispose();
    _focus.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return TextField(
      controller: _ctrl,
      focusNode: _focus,
      enabled: widget.enabled,
      keyboardType: widget.keyboardType,
      textInputAction: TextInputAction.done,
      onSubmitted: (_) => _commit(),
      decoration: InputDecoration(
        labelText: widget.label,
        hintText: widget.hint,
        border: const OutlineInputBorder(),
      ),
    );
  }
}

/// Fila de "Buscar actualizaciones" en Ajustes: comprueba manualmente y muestra
/// el estado. La descarga/instalación se ofrece desde el aviso del panel.
class _UpdateTile extends ConsumerWidget {
  const _UpdateTile();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final upd = ref.watch(updateProvider);
    final checking = upd.status == UpdateStatus.checking;

    final String subtitle;
    switch (upd.status) {
      case UpdateStatus.available:
        subtitle = 'Disponible: v${upd.latestVersion} (toca el aviso del panel para instalar)';
      case UpdateStatus.downloading:
        subtitle = 'Descargando…';
      case UpdateStatus.installing:
        subtitle = 'Instalando…';
      case UpdateStatus.checking:
        subtitle = 'Comprobando…';
      case UpdateStatus.error:
        subtitle = upd.error ?? 'No se pudo comprobar';
      case UpdateStatus.upToDate:
        subtitle = upd.currentVersion != null
            ? 'Estás en la última versión (v${upd.currentVersion})'
            : 'Estás en la última versión';
      case UpdateStatus.idle:
        subtitle = 'Pulsa para comprobar si hay versión nueva';
    }

    return ListTile(
      title: const Text('Buscar actualizaciones'),
      subtitle: Text(subtitle),
      trailing: checking
          ? const SizedBox(
              width: 18,
              height: 18,
              child: CircularProgressIndicator(strokeWidth: 2),
            )
          : const Icon(Icons.refresh_rounded),
      onTap: upd.isWorking
          ? null
          : () =>
              ref.read(updateProvider.notifier).checkForUpdate(manual: true),
    );
  }
}

/// Slider para hacer el punto flotante más grande o más pequeño (en vivo).
class _PillSizeTile extends ConsumerWidget {
  const _PillSizeTile();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final scale = ref.watch(settingsProvider.select((s) => s.pillScale));
    final ctrl = ref.read(settingsProvider.notifier);
    final scheme = Theme.of(context).colorScheme;

    return ListTile(
      title: const Text('Tamaño del punto flotante'),
      subtitle: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Text('Haz el punto más grande o más pequeño'),
          Row(
            children: [
              Icon(Icons.circle, size: 8, color: scheme.onSurfaceVariant),
              Expanded(
                child: Slider(
                  value: scale.clamp(0.7, 2.0),
                  min: 0.7,
                  max: 2.0,
                  divisions: 13,
                  label: '${(scale * 100).round()}%',
                  onChanged: (v) => ctrl.previewPillScale(v),
                  onChangeEnd: (v) => ctrl.setPillScale(v),
                ),
              ),
              Icon(Icons.circle, size: 18, color: scheme.onSurfaceVariant),
            ],
          ),
        ],
      ),
    );
  }
}

/// «Repetir la instalación»: vuelve a abrir el asistente de primera vez.
///
/// Para quien actualizó desde una versión anterior y nunca lo vio, y para quien
/// quiera rehacer una parte (cambiar de idioma, volver a bajar el modelo,
/// comprobar los permisos) sin ir bloque a bloque por los Ajustes.
class _RepetirInstalacionTile extends ConsumerWidget {
  const _RepetirInstalacionTile();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return ListTile(
      leading: const Icon(Icons.restart_alt_rounded, color: Color(0xFFA78BFA)),
      title: const Text('Repetir la instalación'),
      subtitle: const Text(
        'Vuelve a abrir el asistente de primera vez. No borra nada de lo que '
        'has dictado ni lo que Pronto ha aprendido.',
      ),
      onTap: () async {
        final navigator = Navigator.of(context);
        await ref.read(onboardingProvider.notifier).repetir();
        // Cierra Ajustes: el asistente se dibuja en la raíz, y si esta pantalla
        // se queda encima el usuario vería su propia barra "← Ajustes" flotando
        // sobre el asistente.
        if (navigator.canPop()) navigator.popUntil((r) => r.isFirst);
      },
    );
  }
}

/// Motor de voz. Pronto usa EXCLUSIVAMENTE Parakeet (NVIDIA, sherpa-onnx), así
/// que este tile es informativo (Whisper quedó retirado de la selección).
class _EngineTile extends StatelessWidget {
  const _EngineTile();

  @override
  Widget build(BuildContext context) {
    return const ListTile(
      leading: Icon(Icons.graphic_eq_rounded, color: Color(0xFF8B5CF6)),
      title: Text('Motor de reconocimiento'),
      subtitle: Text(
        'Parakeet (NVIDIA): rápido, puntúa solo y entiende mejor la jerga.\n'
        'Reconocimiento 100 % en tu equipo, sin enviar tu voz a la nube.',
      ),
      isThreeLine: true,
      trailing: Text(
        'Parakeet',
        style: TextStyle(
          fontWeight: FontWeight.w600,
          color: Color(0xFFA78BFA),
        ),
      ),
    );
  }
}

/// Selector de micrófono de entrada. Enumera los dispositivos del sistema y
/// deja elegir uno concreto o el "por defecto". La lista se carga en initState
/// y se puede refrescar (p. ej. si conectas un micro USB con la app abierta).
class _MicDeviceTile extends ConsumerStatefulWidget {
  const _MicDeviceTile();

  @override
  ConsumerState<_MicDeviceTile> createState() => _MicDeviceTileState();
}

class _MicDeviceTileState extends ConsumerState<_MicDeviceTile> {
  List<MicDevice> _devices = const [];
  bool _loading = true;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    setState(() => _loading = true);
    try {
      final devices =
          await ref.read(audioCaptureProvider).listInputDevices();
      if (!mounted) return;
      setState(() {
        _devices = devices;
        _loading = false;
      });
    } catch (_) {
      // Si la enumeración falla (sin permiso, driver), dejamos lista vacía:
      // el usuario sigue pudiendo usar el micrófono por defecto del sistema.
      if (!mounted) return;
      setState(() {
        _devices = const [];
        _loading = false;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final selectedId = ref.watch(settingsProvider.select((s) => s.micDeviceId));
    final controller = ref.read(settingsProvider.notifier);
    // El valor guardado solo es válido si sigue en la lista; si el micro se
    // desconectó, caemos a "por defecto" para no romper el Dropdown.
    final bool selectedExists =
        selectedId != null && _devices.any((d) => d.id == selectedId);
    final String? value = selectedExists ? selectedId : null;

    // Cabecera (título + estado + refrescar) y DEBAJO el desplegable a ancho
    // completo. Antes el Dropdown iba en `trailing` y, con nombres de micro
    // largos, se comía todo el ancho → el título "Micrófono" salía en vertical.
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        ListTile(
          leading: const Icon(Icons.mic_rounded, color: Color(0xFFA78BFA)),
          title: const Text('Micrófono'),
          subtitle: Text(
            _loading
                ? 'Buscando micrófonos…'
                : (_devices.isEmpty
                    ? 'No se detectaron micrófonos (se usa el del sistema)'
                    : 'Elige qué micrófono usa Pronto para dictar'),
          ),
          trailing: IconButton(
            tooltip: 'Volver a buscar micrófonos',
            icon: const Icon(Icons.refresh_rounded),
            onPressed: _loading ? null : _load,
          ),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 0, 16, 8),
          child: DropdownButton<String?>(
            isExpanded: true, // ocupa el ancho y recorta nombres largos
            value: value,
            onChanged: _loading ? null : (v) => controller.setMicDeviceId(v),
            items: [
              const DropdownMenuItem<String?>(
                value: null,
                child: Text('Micrófono por defecto del sistema'),
              ),
              for (final d in _devices)
                DropdownMenuItem<String?>(
                  value: d.id,
                  child: Text(d.label, overflow: TextOverflow.ellipsis),
                ),
            ],
          ),
        ),
      ],
    );
  }
}

/// Estado/descarga del modelo Whisper. Solo aparece si el motor elegido es
/// Whisper; como Whisper NO va en el instalador, se descarga aquí bajo demanda
/// a la carpeta de datos de la app.
class _WhisperModelTile extends ConsumerStatefulWidget {
  const _WhisperModelTile();

  @override
  ConsumerState<_WhisperModelTile> createState() => _WhisperModelTileState();
}

class _WhisperModelTileState extends ConsumerState<_WhisperModelTile> {
  bool _present = false;
  bool _checking = true;
  bool _downloading = false;
  double _progress = 0;
  String? _error;

  @override
  void initState() {
    super.initState();
    _check();
  }

  Future<String?> _existingPath() async {
    const file = AppConfig.defaultModelFile;
    final beside =
        p.join(p.dirname(Platform.resolvedExecutable), 'models', file);
    if (File(beside).existsSync()) return beside;
    final dir = await getApplicationSupportDirectory();
    final inSupport = p.join(dir.path, 'models', file);
    if (File(inSupport).existsSync()) return inSupport;
    return null;
  }

  Future<void> _check() async {
    final path = await _existingPath();
    if (!mounted) return;
    setState(() {
      _present = path != null;
      _checking = false;
    });
  }

  Future<void> _download() async {
    setState(() {
      _downloading = true;
      _progress = 0;
      _error = null;
    });
    try {
      final dir = await getApplicationSupportDirectory();
      final modelsDir = Directory(p.join(dir.path, 'models'));
      if (!modelsDir.existsSync()) modelsDir.createSync(recursive: true);
      final out = File(p.join(modelsDir.path, AppConfig.defaultModelFile));
      final tmp = File('${out.path}.part');

      final req = http.Request('GET', Uri.parse(AppConfig.whisperModelUrl));
      final resp = await http.Client().send(req);
      if (resp.statusCode != 200) throw 'HTTP ${resp.statusCode}';
      final total = resp.contentLength ?? 0;
      var received = 0;
      final sink = tmp.openWrite();
      await for (final chunk in resp.stream) {
        sink.add(chunk);
        received += chunk.length;
        if (total > 0 && mounted) {
          setState(() => _progress = received / total);
        }
      }
      await sink.flush();
      await sink.close();
      if (out.existsSync()) out.deleteSync();
      tmp.renameSync(out.path);
      if (!mounted) return;
      setState(() {
        _downloading = false;
        _present = true;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _downloading = false;
        _error = 'No se pudo descargar: $e';
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final engine = ref.watch(settingsProvider.select((s) => s.engine));
    if (engine != SpeechEngine.whisper) return const SizedBox.shrink();

    if (_checking) {
      return const ListTile(
        leading: SizedBox(
          width: 18,
          height: 18,
          child: CircularProgressIndicator(strokeWidth: 2),
        ),
        title: Text('Comprobando modelo Whisper…'),
      );
    }
    if (_downloading) {
      return ListTile(
        title: const Text('Descargando modelo Whisper…'),
        subtitle: LinearProgressIndicator(
          value: _progress == 0 ? null : _progress,
        ),
        trailing: Text('${(_progress * 100).round()}%'),
      );
    }
    if (_present) {
      return const ListTile(
        leading: Icon(Icons.check_circle_rounded, color: Color(0xFF34C759)),
        title: Text('Modelo Whisper listo'),
        subtitle: Text('Reinicia Pronto para usar Whisper.'),
      );
    }
    return ListTile(
      leading: const Icon(Icons.download_rounded),
      title: const Text('Descargar modelo Whisper (~465 MB)'),
      subtitle: Text(
        _error ?? 'Whisper no viene incluido; descárgalo para poder usarlo.',
      ),
      onTap: _download,
    );
  }
}

/// Todo lo de la nube: el aviso de que el texto sale del equipo, la clave, el
/// modelo, los topes de gasto y lo que llevas gastado.
///
/// Solo se dibuja cuando el camino elegido es la nube: un panel de topes de
/// gasto visible con la nube apagada haría creer que hay algo saliendo.
class _CloudTiles extends ConsumerWidget {
  const _CloudTiles();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final theme = Theme.of(context);
    final settings = ref.watch(settingsProvider);
    final controller = ref.read(settingsProvider.notifier);
    final gasto = ref.watch(cloudSpendProvider);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 12, 16, 4),
          child: Container(
            padding: const EdgeInsets.all(12),
            decoration: BoxDecoration(
              color: theme.colorScheme.surfaceContainerHighest,
              borderRadius: BorderRadius.circular(8),
              border: Border.all(color: const Color(0xFFFF9500), width: 1.5),
            ),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const Icon(
                  Icons.cloud_upload_rounded,
                  color: Color(0xFFFF9500),
                ),
                const SizedBox(width: 12),
                Expanded(
                  child: Text(
                    'Con este camino, el TEXTO de cada dictado sale de tu '
                    'equipo hacia OpenRouter para formatearlo. Tu voz no sale '
                    'nunca: el audio se transcribe aquí y no se manda a ningún '
                    'sitio. La clave es tuya, la pagas tú y se guarda en el '
                    'Administrador de credenciales de Windows. Mientras dura '
                    'el envío, la Píldora se pone naranja y dice «Saliendo del '
                    'equipo».',
                    style: theme.textTheme.bodySmall,
                  ),
                ),
              ],
            ),
          ),
        ),
        const _CloudKeyField(),
        ListTile(
          title: const Text('Modelo de nube'),
          subtitle: const Text(
            'Los tres cuestan céntimos al mes con un uso normal',
          ),
          trailing: DropdownButton<String>(
            key: const ValueKey('cloudModel'),
            value: settings.cloudModel,
            onChanged: (v) {
              if (v != null) controller.setCloudModel(v);
            },
            items: [
              for (final modelo in kCloudPrices.keys)
                DropdownMenuItem(value: modelo, child: Text(modelo)),
            ],
          ),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 8, 16, 0),
          child: Row(
            children: [
              Expanded(
                child: _UsdCapField(
                  key: const ValueKey('cloudDayCapUsd'),
                  label: 'Tope por día (dólares)',
                  value: settings.cloudDayCapUsd,
                  onSubmitted: controller.setCloudDayCapUsd,
                ),
              ),
              const SizedBox(width: 12),
              Expanded(
                child: _UsdCapField(
                  key: const ValueKey('cloudMonthCapUsd'),
                  label: 'Tope por mes (dólares)',
                  value: settings.cloudMonthCapUsd,
                  onSubmitted: controller.setCloudMonthCapUsd,
                ),
              ),
            ],
          ),
        ),
        ListTile(
          title: const Text('Gasto'),
          subtitle: Text(
            'Este mes: ${formatUsd(gasto.monthUsd)} en ${gasto.monthCalls} '
            'llamadas · Hoy: ${formatUsd(gasto.dayUsd)} en ${gasto.dayCalls}. '
            'Al llegar al tope, el dictado se pega sin formatear',
          ),
          trailing: TextButton(
            key: const ValueKey('cloudSpendReset'),
            onPressed: ref.read(cloudSpendProvider.notifier).reset,
            child: const Text('Poner a cero'),
          ),
        ),
      ],
    );
  }
}

/// La clave del proveedor. Nunca se muestra: la interfaz solo sabe si hay una
/// guardada, porque el valor vive en el almacén de credenciales de Windows y no
/// se cachea en ningún estado de la app.
class _CloudKeyField extends ConsumerStatefulWidget {
  const _CloudKeyField();

  @override
  ConsumerState<_CloudKeyField> createState() => _CloudKeyFieldState();
}

class _CloudKeyFieldState extends ConsumerState<_CloudKeyField> {
  final TextEditingController _ctrl = TextEditingController();

  /// Qué salió mal al guardar. Windows puede rechazar el secreto (por tamaño,
  /// o por política) y devolver `false`: decir «guardada» sin haberla guardado
  /// dejaría al usuario dictando contra una clave que no existe.
  String? _fallo;

  @override
  void dispose() {
    _ctrl.dispose();
    super.dispose();
  }

  void _guardar() {
    if (_ctrl.text.trim().isEmpty) return;
    final ok = ref.read(cloudKeyProvider).save(_ctrl.text);
    // El campo se vacía SIEMPRE, salga bien o mal: la clave no se queda escrita
    // en pantalla ni viva dentro de un controlador de texto. Si falló, se pide
    // que la pegue otra vez, que es más barato que dejarla ahí.
    _ctrl.clear();
    setState(() {
      _fallo = ok
          ? null
          : 'No se pudo guardar la clave en el Administrador de credenciales '
              'de Windows. No hay clave guardada: vuelve a pegarla.';
    });
    ref.read(cloudKeyPresenceProvider.notifier).refresh();
  }

  void _borrar() {
    final ok = ref.read(cloudKeyProvider).clear();
    _ctrl.clear();
    setState(() {
      _fallo = ok ? null : 'No se pudo borrar la clave guardada.';
    });
    ref.read(cloudKeyPresenceProvider.notifier).refresh();
  }

  @override
  Widget build(BuildContext context) {
    final hayClave = ref.watch(cloudKeyPresenceProvider);
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 12, 16, 0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Expanded(
                child: TextField(
                  key: const ValueKey('cloudApiKey'),
                  controller: _ctrl,
                  obscureText: true,
                  autocorrect: false,
                  enableSuggestions: false,
                  textInputAction: TextInputAction.done,
                  onSubmitted: (_) => _guardar(),
                  decoration: InputDecoration(
                    labelText: 'Clave de OpenRouter',
                    hintText: hayClave
                        ? 'Hay una clave guardada · escribe otra para cambiarla'
                        : 'Pega aquí tu clave',
                    border: const OutlineInputBorder(),
                  ),
                ),
              ),
              const SizedBox(width: 8),
              FilledButton(
                key: const ValueKey('cloudApiKeySave'),
                onPressed: _guardar,
                child: const Text('Guardar'),
              ),
              if (hayClave) ...[
                const SizedBox(width: 8),
                TextButton(
                  key: const ValueKey('cloudApiKeyDelete'),
                  onPressed: _borrar,
                  child: const Text('Borrar'),
                ),
              ],
            ],
          ),
          if (_fallo != null)
            Padding(
              padding: const EdgeInsets.only(top: 6),
              child: Text(
                _fallo!,
                style: Theme.of(context).textTheme.bodySmall?.copyWith(
                      color: Theme.of(context).colorScheme.error,
                    ),
              ),
            ),
          if (!hayClave)
            Padding(
              padding: const EdgeInsets.only(top: 6),
              child: Text(
                'Sin clave no sale ni una petición: el dictado se pega sin '
                'formatear.',
                style: Theme.of(context).textTheme.bodySmall,
              ),
            ),
        ],
      ),
    );
  }
}

/// Campo de un tope de gasto en dólares. Acepta coma o punto decimal, como se
/// escribe en España, y descarta lo que no sea un número.
class _UsdCapField extends StatefulWidget {
  final String label;
  final double value;
  final ValueChanged<double> onSubmitted;

  const _UsdCapField({
    super.key,
    required this.label,
    required this.value,
    required this.onSubmitted,
  });

  @override
  State<_UsdCapField> createState() => _UsdCapFieldState();
}

class _UsdCapFieldState extends State<_UsdCapField> {
  late final TextEditingController _ctrl;
  late final FocusNode _focus;

  String get _texto => widget.value.toStringAsFixed(2).replaceAll('.', ',');

  @override
  void initState() {
    super.initState();
    _ctrl = TextEditingController(text: _texto);
    _focus = FocusNode();
    _focus.addListener(() {
      if (!_focus.hasFocus) _commit();
    });
  }

  @override
  void didUpdateWidget(covariant _UsdCapField oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!_focus.hasFocus && _ctrl.text != _texto) _ctrl.text = _texto;
  }

  void _commit() {
    final v = parseUsd(_ctrl.text);
    if (v == null) {
      // Lo que no es un número no cambia el tope: se devuelve el valor bueno.
      _ctrl.text = _texto;
      return;
    }
    if (v != widget.value) widget.onSubmitted(v);
  }

  @override
  void dispose() {
    _ctrl.dispose();
    _focus.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return TextField(
      controller: _ctrl,
      focusNode: _focus,
      keyboardType: const TextInputType.numberWithOptions(decimal: true),
      textInputAction: TextInputAction.done,
      onSubmitted: (_) => _commit(),
      decoration: InputDecoration(
        labelText: widget.label,
        border: const OutlineInputBorder(),
      ),
    );
  }
}
