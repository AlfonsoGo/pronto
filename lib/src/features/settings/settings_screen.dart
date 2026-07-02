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

          const Divider(height: 24),

          // --- Post-corrección LLM ---
          _SectionHeader('Post-corrección LLM (opcional)', theme: theme),

          SwitchListTile(
            title: const Text('Activar post-corrección LLM'),
            subtitle: const Text(
              'Mejora el texto con un modelo de lenguaje local o en la nube',
            ),
            value: settings.llmEnabled,
            onChanged: controller.setLlmEnabled,
          ),

          // Campos de configuración del LLM (solo activos si está habilitado).
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 8, 16, 0),
            child: _LlmTextField(
              key: const ValueKey('llmBaseUrl'),
              label: 'URL base',
              hint: 'http://localhost:11434',
              initialValue: settings.llmBaseUrl,
              enabled: settings.llmEnabled,
              keyboardType: TextInputType.url,
              onSubmitted: controller.setLlmBaseUrl,
            ),
          ),
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 12, 16, 8),
            child: _LlmTextField(
              key: const ValueKey('llmModel'),
              label: 'Modelo',
              hint: 'qwen3:4b',
              initialValue: settings.llmModel,
              enabled: settings.llmEnabled,
              onSubmitted: controller.setLlmModel,
            ),
          ),

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

          const SizedBox(height: 24),
        ],
      ),
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
