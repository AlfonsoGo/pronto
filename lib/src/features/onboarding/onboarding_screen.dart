import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:path/path.dart' as p;
import 'package:window_manager/window_manager.dart';

import '../../core/config.dart';
import '../../platform/platform_services.dart';
import '../../platform/system_permissions.dart';
import '../dictation/dictation_controller.dart';
import '../dictation/dictation_state.dart';
import '../formatting/cloud_key.dart';
import '../formatting/text_formatter.dart';
import '../overlay/window_mode_controller.dart';
import '../settings/settings_controller.dart';
import '../splash/parakeet_model_downloader.dart';
import 'engine_catalog.dart';
import 'onboarding_progress.dart';

/// Tamaño de la ventana mientras el asistente está abierto. Los cinco bloques
/// caben a la vez: la idea de esta pantalla es que se vea todo de un vistazo y
/// se pueda hacer en el orden que a cada uno le apetezca.
const Size _kAsistenteSize = Size(740, 780);

const _fondo = Color(0xFF0B0810);
const _superficie = Color(0xFF161021);
const _linea = Color(0xFF2A2137);
const _tinta = Color(0xFFF5F3FF);
const _apagado = Color(0xFF9C93AE);
const _violeta = Color(0xFF8B5CF6);
const _violetaSuave = Color(0xFFA78BFA);
const _verde = Color(0xFF34D399);

/// El asistente de instalación de Pronto: una sola hoja con cinco bloques.
///
/// Sustituye al splash de 440×320 que descargaba el modelo y te soltaba dentro
/// sin haber elegido nada ni haber dictado una sola vez.
///
/// Solo hay UN bloqueo real: sin Motor de voz en disco no se puede empezar a
/// dictar. Todo lo demás se puede dejar para luego, y el asistente se reanuda
/// donde estaba si se cierra a mitad.
///
/// ⚠ Ni una sola línea de texto de esta pantalla sale de un modelo de lenguaje:
/// todos los avisos son cadenas escritas en este repo.
class OnboardingScreen extends ConsumerStatefulWidget {
  const OnboardingScreen({super.key});

  @override
  ConsumerState<OnboardingScreen> createState() => _OnboardingScreenState();
}

class _OnboardingScreenState extends ConsumerState<OnboardingScreen> {
  /// Carpeta del modelo si está completo en disco; null si falta.
  String? _modeloDir;
  bool _comprobandoModelo = true;

  PermissionCheck? _mic;
  PermissionCheck? _escritura;
  bool _comprobandoPermisos = true;

  /// El bloque de la Prueba está armado y esperando que el usuario dicte.
  bool _pruebaArmada = false;

  /// El motor se está cargando en memoria tras una descarga.
  bool _cargandoMotor = false;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) async {
      await _prepararVentana();
      await _refrescarModelo();
      await _refrescarPermisos();
    });
  }

  @override
  void dispose() {
    // Salir con la prueba armada dejaría el siguiente dictado de verdad sin
    // pegarse: se desarma siempre, pase lo que pase.
    ref.read(dictationControllerProvider.notifier).desarmarPrueba();
    super.dispose();
  }

  /// Saca la ventana del modo píldora para que quepa la hoja.
  Future<void> _prepararVentana() async {
    try {
      await windowManager.setBackgroundColor(_fondo);
      await windowManager.setAlwaysOnTop(false);
      await windowManager.setSkipTaskbar(false);
      await windowManager.setTitleBarStyle(TitleBarStyle.normal);
      await windowManager.setResizable(true);
      await windowManager.setSize(_kAsistenteSize);
      await windowManager.center();
      await windowManager.show();
      await windowManager.focus();
    } catch (_) {
      // Si window_manager falla, el asistente igualmente se dibuja.
    }
  }

  Future<void> _refrescarModelo() async {
    final dir = await ParakeetModelDownloader.resolveModelDir();
    if (!mounted) return;
    setState(() {
      _modeloDir = dir;
      _comprobandoModelo = false;
    });
    await _recalcularSiSePuedeBorrar();
  }

  Future<void> _refrescarPermisos() async {
    setState(() => _comprobandoPermisos = true);
    try {
      final permisos = ref.read(systemPermissionsProvider);
      final mic = await permisos.microphone();
      final escritura = await permisos.textInjection();
      if (!mounted) return;
      setState(() {
        _mic = mic;
        _escritura = escritura;
        _comprobandoPermisos = false;
      });
    } catch (_) {
      if (!mounted) return;
      setState(() {
        _mic = const PermissionCheck(
          PermissionState.desconocido,
          'No se pudieron comprobar los permisos en este equipo.',
        );
        _escritura = _mic;
        _comprobandoPermisos = false;
      });
    }
  }

  /// Descarga el modelo y, en cuanto está, carga el motor en memoria: sin eso
  /// el bloque de la Prueba diría «listo» y el atajo no haría nada.
  Future<void> _descargarMotor() async {
    await ref.read(parakeetDownloaderProvider.notifier).ensureModel();
    if (!mounted) return;
    if (ref.read(parakeetDownloaderProvider).phase != DownloadPhase.listo) {
      return; // cancelado o error: el propio bloque lo cuenta
    }
    await _refrescarModelo();
    await _cargarMotor();
  }

  Future<void> _cargarMotor() async {
    if (!mounted) return;
    setState(() => _cargandoMotor = true);
    try {
      await ref.read(dictationControllerProvider.notifier).initialize();
    } finally {
      if (mounted) setState(() => _cargandoMotor = false);
    }
  }

  Future<void> _borrarMotor() async {
    final confirmado = await showDialog<bool>(
          context: context,
          builder: (ctx) => AlertDialog(
            backgroundColor: _superficie,
            title: const Text('¿Borrar el Motor de voz?'),
            content: const Text(
              'Pronto no podrá dictar hasta que lo vuelvas a descargar. '
              'No se borra nada de lo que has dictado: solo el modelo.',
            ),
            actions: [
              TextButton(
                onPressed: () => Navigator.of(ctx).pop(false),
                child: const Text('Cancelar'),
              ),
              FilledButton(
                onPressed: () => Navigator.of(ctx).pop(true),
                child: const Text('Borrar'),
              ),
            ],
          ),
        ) ??
        false;
    if (!confirmado) return;
    await ref.read(parakeetDownloaderProvider.notifier).deleteDownloadedModel();
    await _refrescarModelo();
  }

  void _armarPrueba() {
    ref.read(dictationControllerProvider.notifier).armarPrueba();
    setState(() => _pruebaArmada = true);
  }

  void _desarmarPrueba() {
    ref.read(dictationControllerProvider.notifier).desarmarPrueba();
    setState(() => _pruebaArmada = false);
  }

  Future<void> _terminar() async {
    _desarmarPrueba();
    await ref.read(onboardingProvider.notifier).terminar();
    if (!mounted) return;
    await ref.read(windowModeProvider.notifier).toPill();
  }

  /// El modelo que se está usando es el que Pronto se descargó (y por tanto se
  /// puede borrar), y no el que trajo el instalador junto al `.exe`.
  ///
  /// Se compara con la carpeta de datos DE VERDAD y no con un «¿pone AppData en
  /// la ruta?»: los instaladores por usuario acaban dentro de AppData, así que
  /// esa pista habría enseñado un botón «Borrar» sobre un modelo que
  /// [ParakeetModelDownloader.deleteDownloadedModel] no toca — un botón que no
  /// hace nada.
  bool _sePuedeBorrar = false;

  Future<void> _recalcularSiSePuedeBorrar() async {
    final dir = _modeloDir;
    if (dir == null) {
      if (mounted) setState(() => _sePuedeBorrar = false);
      return;
    }
    try {
      final soporte = await ref.read(appSupportDirProvider.future);
      final descargado = p.equals(
        dir,
        p.join(soporte.path, 'models', AppConfig.parakeetModelDir),
      );
      if (!mounted) return;
      setState(() => _sePuedeBorrar = descargado);
    } catch (_) {
      // Sin saber dónde está la carpeta de datos no se ofrece borrar: es
      // preferible no enseñar el botón a enseñar uno que no haga nada.
      if (mounted) setState(() => _sePuedeBorrar = false);
    }
  }

  bool get _hayMotor => _modeloDir != null;

  @override
  Widget build(BuildContext context) {
    final progreso = ref.watch(onboardingProvider);
    final ajustes = ref.watch(settingsProvider);
    final dictado = ref.watch(dictationControllerProvider);

    final permisosOk = (_mic?.ok ?? false) && (_escritura?.ok ?? false);
    final hechos = [
      progreso.idiomaElegido,
      _hayMotor,
      permisosOk,
      progreso.pruebaHecha,
      progreso.formateoElegido,
    ].where((x) => x).length;

    return Scaffold(
      backgroundColor: _fondo,
      body: Column(
        children: [
          _cabecera(hechos),
          Expanded(
            child: ListView(
              padding: const EdgeInsets.fromLTRB(30, 8, 30, 24),
              children: [
                _bloqueIdioma(progreso, ajustes),
                _bloqueMotor(progreso),
                _bloquePermisos(permisosOk),
                _bloquePrueba(progreso, dictado),
                _bloqueFormateo(progreso, ajustes),
              ],
            ),
          ),
          _pie(),
        ],
      ),
    );
  }

  Widget _cabecera(int hechos) {
    return Container(
      padding: const EdgeInsets.fromLTRB(30, 22, 30, 14),
      decoration: const BoxDecoration(
        border: Border(bottom: BorderSide(color: _linea)),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          const Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  'Poner Pronto en marcha',
                  style: TextStyle(
                    color: _tinta,
                    fontSize: 20,
                    fontWeight: FontWeight.w700,
                  ),
                ),
                SizedBox(height: 3),
                Text(
                  'Todo en una pantalla. Puedes hacerlo en el orden que quieras.',
                  style: TextStyle(color: _apagado, fontSize: 12.5),
                ),
              ],
            ),
          ),
          Text(
            '$hechos de 5',
            style: const TextStyle(color: _apagado, fontSize: 12.5),
          ),
        ],
      ),
    );
  }

  Widget _pie() {
    return Container(
      padding: const EdgeInsets.fromLTRB(30, 14, 30, 14),
      decoration: const BoxDecoration(
        border: Border(top: BorderSide(color: _linea)),
      ),
      child: Row(
        children: [
          Expanded(
            child: Text(
              _hayMotor
                  ? 'Todo lo imprescindible está hecho.'
                  : 'Falta descargar un Motor de voz.',
              style: const TextStyle(color: _apagado, fontSize: 12),
            ),
          ),
          FilledButton(
            onPressed: _hayMotor ? _terminar : null,
            child: const Text('Empezar a dictar'),
          ),
        ],
      ),
    );
  }

  Widget _bloque({
    required bool ok,
    required String titulo,
    String? etiqueta,
    required Widget cuerpo,
  }) {
    return Container(
      padding: const EdgeInsets.symmetric(vertical: 18),
      decoration: const BoxDecoration(
        border: Border(bottom: BorderSide(color: _linea)),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Container(
            width: 24,
            height: 24,
            decoration: BoxDecoration(
              shape: BoxShape.circle,
              color: ok ? _verde.withValues(alpha: 0.16) : null,
              border: ok ? null : Border.all(color: _linea, width: 2),
            ),
            child: ok
                ? const Icon(Icons.check_rounded, size: 15, color: _verde)
                : null,
          ),
          const SizedBox(width: 14),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Text(
                      titulo,
                      style: const TextStyle(
                        color: _tinta,
                        fontSize: 14.5,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                    if (etiqueta != null) ...[
                      const SizedBox(width: 10),
                      _Chip(etiqueta),
                    ],
                  ],
                ),
                const SizedBox(height: 6),
                cuerpo,
              ],
            ),
          ),
        ],
      ),
    );
  }

  // --- 1) Idioma -----------------------------------------------------------

  Widget _bloqueIdioma(OnboardingProgress progreso, AppSettings ajustes) {
    const idiomas = [
      (codigo: 'es', nombre: 'Español'),
      (codigo: 'auto', nombre: 'Detección automática'),
      (codigo: 'en', nombre: 'English'),
    ];
    return _bloque(
      ok: progreso.idiomaElegido,
      titulo: 'Idioma',
      cuerpo: Wrap(
        spacing: 8,
        runSpacing: 8,
        children: [
          for (final i in idiomas)
            _Pildora(
              texto: i.nombre,
              seleccionado: ajustes.language == i.codigo,
              onTap: () async {
                await ref
                    .read(settingsProvider.notifier)
                    .setLanguage(i.codigo);
                await ref.read(onboardingProvider.notifier).marcarIdiomaElegido();
              },
            ),
        ],
      ),
    );
  }

  // --- 2) Motor de voz -----------------------------------------------------

  Widget _bloqueMotor(OnboardingProgress progreso) {
    final descarga = ref.watch(parakeetDownloaderProvider);
    final motores = EngineCatalog.ordenar(
      EngineCatalog.todos,
      preferencia: progreso.preferenciaMotor,
    );

    return _bloque(
      ok: _hayMotor,
      titulo: 'Motor de voz',
      etiqueta: _hayMotor ? null : 'pendiente',
      cuerpo: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Text(
            'Se descarga una vez y el reconocimiento ocurre en tu equipo.',
            style: TextStyle(color: _apagado, fontSize: 12.5),
          ),
          const SizedBox(height: 12),
          Row(
            children: [
              const Text(
                'Rapidez',
                style: TextStyle(color: _apagado, fontSize: 12),
              ),
              Expanded(
                child: Slider(
                  value: progreso.preferenciaMotor,
                  activeColor: _violeta,
                  onChanged: (v) => ref
                      .read(onboardingProvider.notifier)
                      .setPreferenciaMotor(v),
                ),
              ),
              const Text(
                'Precisión',
                style: TextStyle(color: _apagado, fontSize: 12),
              ),
            ],
          ),
          for (final motor in motores)
            _filaMotor(motor, descarga),
          const SizedBox(height: 6),
          const Text(
            'Las barras no llevan número porque todavía no hemos medido el WER '
            'en español con nuestro propio corpus. El orden sale del dato '
            'publicado en la ficha de cada modelo, y el que no tiene dato '
            'comparable no adelanta a los que sí lo tienen.',
            style: TextStyle(color: _apagado, fontSize: 11.5),
          ),
          if (_comprobandoModelo)
            const Padding(
              padding: EdgeInsets.only(top: 8),
              child: Text(
                'Comprobando si el modelo ya está en disco…',
                style: TextStyle(color: _apagado, fontSize: 11.5),
              ),
            ),
          if (_cargandoMotor)
            const Padding(
              padding: EdgeInsets.only(top: 8),
              child: Text(
                'Cargando el motor en memoria… ya casi está.',
                style: TextStyle(color: _violetaSuave, fontSize: 11.5),
              ),
            ),
          if (descarga.phase == DownloadPhase.error)
            Padding(
              padding: const EdgeInsets.only(top: 8),
              child: Text(
                descarga.error ?? 'No se pudo descargar el modelo de voz.',
                style: const TextStyle(color: Color(0xFFFF6B6B), fontSize: 12),
              ),
            ),
          if (descarga.phase == DownloadPhase.cancelado)
            const Padding(
              padding: EdgeInsets.only(top: 8),
              child: Text(
                'Descarga parada. Lo que se había bajado se ha borrado: al '
                'volver a intentarlo, el modelo baja entero.',
                style: TextStyle(color: _apagado, fontSize: 12),
              ),
            ),
        ],
      ),
    );
  }

  Widget _filaMotor(SpeechEngineInfo motor, DownloadState descarga) {
    final esElActual = motor.engine == SpeechEngine.parakeet;
    final instalado = esElActual && _hayMotor;
    final bajando = esElActual &&
        descarga.phase == DownloadPhase.descargando;

    return Container(
      margin: const EdgeInsets.only(bottom: 8),
      padding: const EdgeInsets.symmetric(horizontal: 13, vertical: 11),
      decoration: BoxDecoration(
        color: _superficie,
        borderRadius: BorderRadius.circular(11),
        border: Border.all(color: instalado ? _violeta : _linea),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      motor.nombre,
                      style: const TextStyle(
                        color: _tinta,
                        fontSize: 13.5,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                    const SizedBox(height: 2),
                    Text(
                      '${motor.idiomas} · ${motor.tamano}',
                      style: const TextStyle(color: _apagado, fontSize: 12),
                    ),
                  ],
                ),
              ),
              const SizedBox(width: 10),
              _BarraPrecision(motor: motor),
              const SizedBox(width: 12),
              if (!motor.seleccionable)
                const _Chip('no disponible')
              else if (bajando)
                _controlesDescarga(descarga)
              else if (instalado)
                Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    const _Chip('instalado', verde: true),
                    if (_sePuedeBorrar)
                      IconButton(
                        tooltip: 'Borrar el modelo descargado',
                        icon: const Icon(Icons.delete_outline_rounded, size: 18),
                        color: _apagado,
                        onPressed: _borrarMotor,
                      ),
                  ],
                )
              else
                FilledButton(
                  onPressed: _comprobandoModelo ? null : _descargarMotor,
                  child: const Text('Descargar'),
                ),
            ],
          ),
          if (!motor.seleccionable && motor.porQueNoSePuedeUsar != null)
            Padding(
              padding: const EdgeInsets.only(top: 8),
              child: Text(
                motor.porQueNoSePuedeUsar!,
                style: const TextStyle(color: _apagado, fontSize: 11.5),
              ),
            ),
        ],
      ),
    );
  }

  Widget _controlesDescarga(DownloadState descarga) {
    String mb(int bytes) => (bytes / (1024 * 1024)).toStringAsFixed(0);
    return SizedBox(
      width: 150,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          ClipRRect(
            borderRadius: BorderRadius.circular(6),
            child: LinearProgressIndicator(
              value: descarga.totalBytes > 0 ? descarga.progress : null,
              minHeight: 6,
              backgroundColor: _fondo,
              valueColor: const AlwaysStoppedAnimation<Color>(_violeta),
            ),
          ),
          const SizedBox(height: 4),
          Row(
            mainAxisAlignment: MainAxisAlignment.end,
            children: [
              Text(
                descarga.totalBytes > 0
                    ? '${mb(descarga.receivedBytes)} / '
                        '${mb(descarga.totalBytes)} MB'
                    : 'Calculando…',
                style: const TextStyle(color: _violetaSuave, fontSize: 11),
              ),
              TextButton(
                onPressed: () =>
                    ref.read(parakeetDownloaderProvider.notifier).cancel(),
                child: const Text('Parar'),
              ),
            ],
          ),
        ],
      ),
    );
  }

  // --- 3) Permisos ---------------------------------------------------------

  Widget _bloquePermisos(bool ok) {
    return _bloque(
      ok: ok,
      titulo: 'Permisos de Windows',
      cuerpo: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          _filaPermiso(
            'Micrófono',
            _mic,
            accion: _mic != null && !_mic!.ok
                ? TextButton(
                    onPressed: () async {
                      await ref
                          .read(systemPermissionsProvider)
                          .openMicrophoneSettings();
                    },
                    child: const Text('Abrir Configuración'),
                  )
                : null,
          ),
          _filaPermiso('Escribir en otras aplicaciones', _escritura),
          Align(
            alignment: Alignment.centerLeft,
            child: TextButton.icon(
              onPressed: _comprobandoPermisos ? null : _refrescarPermisos,
              icon: const Icon(Icons.refresh_rounded, size: 16),
              label: const Text('Volver a comprobar'),
            ),
          ),
        ],
      ),
    );
  }

  Widget _filaPermiso(
    String nombre,
    PermissionCheck? check, {
    Widget? accion,
  }) {
    final IconData icono;
    final Color color;
    switch (check?.estado) {
      case PermissionState.concedido:
        icono = Icons.check_circle_rounded;
        color = _verde;
      case PermissionState.denegado:
        icono = Icons.cancel_rounded;
        color = const Color(0xFFFF6B6B);
      case PermissionState.desconocido:
        icono = Icons.help_outline_rounded;
        color = _apagado;
      case null:
        icono = Icons.hourglass_empty_rounded;
        color = _apagado;
    }
    return Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Icon(icono, size: 17, color: color),
          const SizedBox(width: 8),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  nombre,
                  style: const TextStyle(color: _tinta, fontSize: 13),
                ),
                Text(
                  check?.detalle ?? 'Comprobando…',
                  style: const TextStyle(color: _apagado, fontSize: 12),
                ),
              ],
            ),
          ),
          if (accion != null) accion,
        ],
      ),
    );
  }

  // --- 4) Prueba -----------------------------------------------------------

  Widget _bloquePrueba(OnboardingProgress progreso, DictationState dictado) {
    final atajo = ref.watch(settingsProvider).hotkey.describe();
    final grabando = dictado.status == DictationStatus.recording;
    final procesando = dictado.status == DictationStatus.transcribing ||
        dictado.status == DictationStatus.sendingToCloud;

    // Al llegar texto de una prueba, se marca hecha una sola vez.
    final texto = dictado.lastTestText;
    if (texto != null && texto.isNotEmpty && !progreso.pruebaHecha) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        ref.read(onboardingProvider.notifier).marcarPruebaHecha();
      });
    }

    final String estado;
    if (!_hayMotor) {
      estado = 'Necesitas el Motor de voz descargado para poder probar.';
    } else if (!_pruebaArmada) {
      estado = 'Pulsa «Probar», mantén $atajo, habla y suelta.';
    } else if (grabando) {
      estado = 'Grabando… suelta $atajo cuando termines.';
    } else if (procesando) {
      estado = 'Transcribiendo…';
    } else {
      estado = 'Mantén $atajo, habla y suelta. Esto no se pega en ningún '
          'sitio ni entra en tu historial.';
    }

    return _bloque(
      ok: progreso.pruebaHecha,
      titulo: 'Prueba un dictado',
      etiqueta: 'opcional',
      cuerpo: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(estado, style: const TextStyle(color: _apagado, fontSize: 12.5)),
          const SizedBox(height: 8),
          Container(
            width: double.infinity,
            constraints: const BoxConstraints(minHeight: 54),
            padding: const EdgeInsets.all(14),
            decoration: BoxDecoration(
              color: _superficie,
              borderRadius: BorderRadius.circular(11),
              border: Border.all(color: _pruebaArmada ? _violeta : _linea),
            ),
            child: Text(
              (texto == null || texto.isEmpty) ? '…' : texto,
              style: TextStyle(
                color: (texto == null || texto.isEmpty) ? _apagado : _tinta,
                fontSize: 13.5,
              ),
            ),
          ),
          const SizedBox(height: 8),
          Row(
            children: [
              OutlinedButton(
                onPressed: !_hayMotor
                    ? null
                    : (_pruebaArmada ? _desarmarPrueba : _armarPrueba),
                child: Text(_pruebaArmada ? 'Terminar la prueba' : 'Probar'),
              ),
            ],
          ),
        ],
      ),
    );
  }

  // --- 5) Formateo ---------------------------------------------------------

  Widget _bloqueFormateo(OnboardingProgress progreso, AppSettings ajustes) {
    final hayClave = ref.watch(cloudKeyPresenceProvider);
    const opciones = [
      (
        camino: FormatPath.none,
        nombre: 'Sin Formateo',
        detalle: 'El texto sale tal y como lo oye el motor. '
            'Cero espera y cero coste. Es lo de fábrica.',
      ),
      (
        camino: FormatPath.local,
        nombre: 'Local, en tu equipo',
        detalle: 'Un modelo pequeño arregla tildes, comas y muletillas. '
            'Necesita Ollama corriendo en el equipo; no sale nada del PC.',
      ),
      (
        camino: FormatPath.cloud,
        nombre: 'Nube, con tu clave',
        detalle: 'Sale el TEXTO ya transcrito, nunca el audio, y con tope de '
            'gasto. Necesita tu propia clave: se pone en Ajustes.',
      ),
    ];

    return _bloque(
      ok: progreso.formateoElegido,
      titulo: 'Formateo con IA',
      etiqueta: 'apagado de fábrica',
      cuerpo: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              for (final o in opciones)
                _Pildora(
                  texto: o.nombre,
                  seleccionado: ajustes.formatPath == o.camino,
                  // Sin clave, la nube no se puede elegir: encenderla dejaría
                  // un camino que falla en el primer dictado.
                  onTap: (o.camino == FormatPath.cloud && !hayClave)
                      ? null
                      : () async {
                          await ref
                              .read(settingsProvider.notifier)
                              .setFormatPath(o.camino);
                          await ref
                              .read(onboardingProvider.notifier)
                              .marcarFormateoElegido();
                        },
                ),
            ],
          ),
          const SizedBox(height: 8),
          Text(
            opciones
                .firstWhere((o) => o.camino == ajustes.formatPath)
                .detalle,
            style: const TextStyle(color: _apagado, fontSize: 12),
          ),
          if (!hayClave)
            const Padding(
              padding: EdgeInsets.only(top: 6),
              child: Text(
                'La nube está en gris porque no hay ninguna clave guardada. '
                'Puedes dejarlo para luego: se configura en Ajustes.',
                style: TextStyle(color: _apagado, fontSize: 11.5),
              ),
            ),
        ],
      ),
    );
  }
}

/// Barra de precisión SIN número.
///
/// Se dibuja solo cuando hay una cifra publicada; cuando no la hay se dice, en
/// vez de pintar una barra a ojo que parecería una medida.
class _BarraPrecision extends StatelessWidget {
  final SpeechEngineInfo motor;
  const _BarraPrecision({required this.motor});

  @override
  Widget build(BuildContext context) {
    final wer = motor.werFleursEs;
    if (wer == null) {
      return const SizedBox(
        width: 110,
        child: Text(
          'sin dato publicado',
          textAlign: TextAlign.right,
          style: TextStyle(color: _apagado, fontSize: 11),
        ),
      );
    }
    // Menos WER, barra más llena. La escala (0 a 10 % de WER) es solo para
    // dibujar: no se enseña, no se compara con nada de fuera y no se guarda.
    final lleno = (1 - (wer / 0.10)).clamp(0.05, 1.0);
    return SizedBox(
      width: 110,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          const Text(
            'precisión',
            style: TextStyle(color: _apagado, fontSize: 10.5),
          ),
          const SizedBox(height: 3),
          ClipRRect(
            borderRadius: BorderRadius.circular(4),
            child: LinearProgressIndicator(
              value: lleno,
              minHeight: 5,
              backgroundColor: _fondo,
              valueColor: const AlwaysStoppedAnimation<Color>(_violetaSuave),
            ),
          ),
        ],
      ),
    );
  }
}

class _Chip extends StatelessWidget {
  final String texto;
  final bool verde;
  const _Chip(this.texto, {this.verde = false});

  @override
  Widget build(BuildContext context) {
    final color = verde ? _verde : _apagado;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.14),
        borderRadius: BorderRadius.circular(99),
      ),
      child: Text(
        texto,
        style: TextStyle(color: color, fontSize: 11),
      ),
    );
  }
}

class _Pildora extends StatelessWidget {
  final String texto;
  final bool seleccionado;
  final VoidCallback? onTap;
  const _Pildora({
    required this.texto,
    required this.seleccionado,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    final habilitado = onTap != null;
    return OutlinedButton(
      onPressed: onTap,
      style: OutlinedButton.styleFrom(
        backgroundColor: seleccionado ? _violeta.withValues(alpha: 0.16) : null,
        side: BorderSide(color: seleccionado ? _violeta : _linea),
        foregroundColor: habilitado ? _tinta : _apagado,
      ),
      child: Text(texto),
    );
  }
}
