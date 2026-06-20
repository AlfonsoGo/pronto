// Implementación de [WhisperEngine] sobre whisper.cpp vía dart:ffi.
//
// whisper_full es CPU-intensiva (segundos en CPU). Para no bloquear la UI se
// ejecuta dentro de un isolate.
//
// Los punteros nativos (el `whisper_context*` y los bindings) NO cruzan
// isolates. Estrategia usada aquí (ISOLATE DE LARGA VIDA):
//   - Al cargar (load), se lanza UN isolate que abre la DLL e inicializa el
//     contexto del modelo UNA sola vez, y se queda esperando peticiones.
//   - Cada transcripción envía solo el audio (PCM, datos planos) por un puerto;
//     el isolate reutiliza el MISMO contexto y devuelve el texto.
//   - Así NO se relee el modelo (cientos de MB) del disco en cada dictado:
//     el primer dictado tras arrancar es instantáneo en cuanto a carga, y los
//     siguientes no pagan ningún coste de recarga.
// Coste: el modelo queda residente en memoria mientras la app vive (lo normal
// en dictado por voz). Antes (estrategia MVP) se cargaba y liberaba en cada
// transcripción: simple pero lento.
//
// Este fichero usa imports RELATIVOS dentro de lib/ (regla del proyecto).

import 'dart:async';
import 'dart:ffi';
import 'dart:io';
import 'dart:isolate';
import 'dart:math' as math;
import 'dart:typed_data';

import 'package:ffi/ffi.dart';

import '../../core/config.dart';
import '../../platform/whisper_engine.dart';
import 'whisper_bindings.dart';

/// Motor Whisper on-device basado en whisper.cpp (dart:ffi), con el contexto
/// del modelo residente en un isolate de larga vida.
class WhisperEngineFfi implements WhisperEngine {
  /// Audio esperado por whisper.cpp: PCM mono a 16 kHz.
  static const int _sampleRate = 16000;

  Isolate? _isolate;
  SendPort? _toIsolate; // puerto para enviar peticiones al isolate
  ReceivePort? _fromIsolate; // puerto por el que llegan las respuestas
  StreamSubscription<dynamic>? _sub;
  bool _loaded = false;

  int _nextId = 0;
  final Map<int, Completer<_TranscribeResult>> _pending = {};

  @override
  bool get isLoaded => _loaded;

  @override
  Future<void> load(String modelPath) async {
    final file = File(modelPath);
    if (!await file.exists()) {
      throw WhisperLibraryNotFound(
        'No se encontró el modelo GGML en "$modelPath". '
        'Descarga un modelo (p. ej. ${AppConfig.defaultModelFile}) y colócalo '
        'en esa ruta. Ver SETUP.md.',
      );
    }

    // Validación temprana de la DLL en el isolate principal: falla rápido y con
    // un mensaje claro si falta whisper.dll, antes de lanzar el isolate.
    WhisperBindings.open();

    // Idempotente: si ya hay un isolate cargado, no respawneamos.
    if (_loaded) return;

    final ready = ReceivePort();
    final fromIsolate = ReceivePort();
    _fromIsolate = fromIsolate;

    _isolate = await Isolate.spawn(
      _isolateMain,
      _InitMsg(
        modelPath: modelPath,
        readyPort: ready.sendPort,
        responsePort: fromIsolate.sendPort,
      ),
      debugName: 'whisper',
      errorsAreFatal: true,
    );

    // La primera respuesta por [ready] es el SendPort de entrada (OK) o un
    // String con el error de inicialización del modelo.
    final first = await ready.first;
    ready.close();
    if (first is! SendPort) {
      await _shutdown();
      throw StateError('No se pudo cargar el modelo Whisper: $first');
    }
    _toIsolate = first;
    _sub = fromIsolate.listen(_onResponse);
    _loaded = true;
  }

  void _onResponse(dynamic msg) {
    final list = msg as List;
    final id = list[0] as int;
    final completer = _pending.remove(id);
    if (completer == null) return;
    final ok = list[1] as bool;
    if (ok) {
      completer.complete(
        _TranscribeResult(
          text: list[2] as String,
          avgLogProb: list[3] as double,
          language: list[4] as String,
          audioMs: list[5] as int,
        ),
      );
    } else {
      completer.completeError(StateError(list[2] as String));
    }
  }

  @override
  Future<TranscriptResult> transcribe(
    Float32List pcm16kMonoF32, {
    String language = 'es',
    String? initialPrompt,
    int? threads,
  }) async {
    final port = _toIsolate;
    if (!_loaded || port == null) {
      throw StateError(
        'WhisperEngineFfi no está cargado. Llama a load(modelPath) primero.',
      );
    }

    final audioMs = ((pcm16kMonoF32.length / _sampleRate) * 1000).round();
    if (pcm16kMonoF32.isEmpty) {
      return TranscriptResult(
        text: '',
        avgLogProb: 0,
        language: language,
        audioMs: 0,
      );
    }

    final id = _nextId++;
    final completer = Completer<_TranscribeResult>();
    _pending[id] = completer;

    // Solo viajan datos planos; el contexto nativo se queda en el isolate.
    port.send([
      id,
      pcm16kMonoF32,
      language,
      initialPrompt,
      threads ?? AppConfig.defaultThreads,
      audioMs,
    ]);

    final out = await completer.future;
    return TranscriptResult(
      text: out.text,
      avgLogProb: out.avgLogProb,
      language: out.language,
      audioMs: out.audioMs,
    );
  }

  @override
  Future<void> dispose() async => _shutdown();

  Future<void> _shutdown() async {
    _loaded = false;
    try {
      _toIsolate?.send(const ['shutdown']);
    } catch (_) {}
    _toIsolate = null;
    await _sub?.cancel();
    _sub = null;
    _fromIsolate?.close();
    _fromIsolate = null;
    for (final c in _pending.values) {
      if (!c.isCompleted) c.completeError(StateError('Motor Whisper cerrado.'));
    }
    _pending.clear();
    // Da un instante al isolate para liberar el ctx; luego lo matamos por si
    // se quedó bloqueado en un whisper_full.
    _isolate?.kill(priority: Isolate.beforeNextEvent);
    _isolate = null;
  }
}

// ===========================================================================
// Código que se ejecuta DENTRO del isolate de larga vida.
// Solo recibe/devuelve datos serializables por SendPort.
// ===========================================================================

/// Mensaje de arranque del isolate (sendable: String + SendPorts).
class _InitMsg {
  final String modelPath;
  final SendPort readyPort;
  final SendPort responsePort;
  const _InitMsg({
    required this.modelPath,
    required this.readyPort,
    required this.responsePort,
  });
}

/// Resultado plano de una transcripción.
class _TranscribeResult {
  final String text;
  final double avgLogProb;
  final String language;
  final int audioMs;
  const _TranscribeResult({
    required this.text,
    required this.avgLogProb,
    required this.language,
    required this.audioMs,
  });
}

/// Punto de entrada del isolate: abre la DLL, inicializa el contexto del modelo
/// UNA vez y atiende peticiones de transcripción reutilizándolo.
void _isolateMain(_InitMsg init) {
  final WhisperBindings bindings;
  final Pointer<Void> ctx;
  try {
    bindings = WhisperBindings.open();
    final pathPtr = init.modelPath.toNativeUtf8();
    try {
      ctx = bindings.initFromFile(pathPtr);
    } finally {
      calloc.free(pathPtr);
    }
    if (ctx == nullptr) {
      init.readyPort.send(
        'whisper_init_from_file devolvió NULL para "${init.modelPath}". '
        '¿El fichero es un modelo GGML válido y compatible con esta whisper.dll?',
      );
      return;
    }
  } catch (e) {
    init.readyPort.send('$e');
    return;
  }

  final inbox = ReceivePort();
  // Listo: enviamos el puerto por el que recibiremos peticiones.
  init.readyPort.send(inbox.sendPort);

  inbox.listen((msg) {
    final list = msg as List;
    // Mensaje de cierre: liberar el contexto y terminar.
    if (list.isNotEmpty && list[0] == 'shutdown') {
      bindings.free(ctx);
      inbox.close();
      return;
    }

    final id = list[0] as int;
    final pcm = list[1] as Float32List;
    final language = list[2] as String;
    final initialPrompt = list[3] as String?;
    final threads = list[4] as int;
    final audioMs = list[5] as int;

    try {
      final r = _transcribeOnCtx(
        bindings,
        ctx,
        pcm,
        language,
        initialPrompt,
        threads,
        audioMs,
      );
      init.responsePort
          .send([id, true, r.text, r.avgLogProb, r.language, r.audioMs]);
    } catch (e) {
      init.responsePort.send([id, false, '$e']);
    }
  });
}

/// Ejecuta whisper_full sobre un contexto YA inicializado (no lo crea ni lo
/// libera: el contexto vive entre llamadas). Libera solo la memoria nativa
/// temporal de esta llamada.
_TranscribeResult _transcribeOnCtx(
  WhisperBindings b,
  Pointer<Void> ctx,
  Float32List pcm,
  String language,
  String? initialPrompt,
  int threads,
  int audioMs,
) {
  Pointer<Float> samplesPtr = nullptr;
  Pointer<Utf8> langPtr = nullptr;
  Pointer<Utf8> promptPtr = nullptr;

  try {
    final n = pcm.length;
    samplesPtr = calloc<Float>(n);
    samplesPtr.asTypedList(n).setAll(0, pcm);

    final params = b.fullDefaultParams(WhisperSamplingStrategy.greedy);

    langPtr = language.toNativeUtf8();
    params.language = langPtr;
    params.detectLanguage = false;

    if (threads > 0) {
      params.nThreads = threads;
    }

    params.printProgress = false;
    params.printRealtime = false;
    params.printTimestamps = false;
    params.printSpecial = false;
    params.noTimestamps = true;
    params.translate = false;

    if (initialPrompt != null && initialPrompt.trim().isNotEmpty) {
      promptPtr = initialPrompt.toNativeUtf8();
      params.initialPrompt = promptPtr;
    }

    final rc = b.full(ctx, params, samplesPtr, n);
    if (rc != 0) {
      throw StateError('whisper_full falló con código $rc.');
    }

    final nSeg = b.fullNSegments(ctx);
    final buffer = StringBuffer();
    double sumLogProb = 0;
    int tokenCount = 0;

    for (var i = 0; i < nSeg; i++) {
      final segTextPtr = b.fullGetSegmentText(ctx, i);
      if (segTextPtr != nullptr) {
        buffer.write(segTextPtr.toDartString());
      }
      final nTok = b.fullNTokens(ctx, i);
      for (var t = 0; t < nTok; t++) {
        final pTok = b.fullGetTokenP(ctx, i, t);
        if (pTok > 0) {
          sumLogProb += math.log(pTok);
          tokenCount++;
        }
      }
    }

    final avgLogProb = tokenCount > 0 ? sumLogProb / tokenCount : 0.0;
    return _TranscribeResult(
      text: buffer.toString().trim(),
      avgLogProb: avgLogProb,
      language: language,
      audioMs: audioMs,
    );
  } finally {
    if (samplesPtr != nullptr) calloc.free(samplesPtr);
    if (langPtr != nullptr) calloc.free(langPtr);
    if (promptPtr != nullptr) calloc.free(promptPtr);
  }
}
