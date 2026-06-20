// Implementación de [WhisperEngine] sobre whisper.cpp vía dart:ffi.
//
// whisper_full es CPU-intensiva (segundos en CPU). Para no bloquear la UI la
// ejecutamos dentro de un isolate con `Isolate.run`.
//
// PROBLEMA: los punteros nativos (el `whisper_context*` y los bindings) NO
// cruzan isolates: un Pointer obtenido en el isolate principal no es válido en
// otro isolate, y DynamicLibrary debe abrirse en el isolate que la usa. Hay dos
// estrategias:
//   (a) [MVP, elegida] Cargar el modelo DENTRO del isolate en cada transcripción.
//       Sencillo y robusto; coste: relee el .bin del disco y reconstruye el ctx
//       cada vez (cientos de ms a ~1-2 s según el modelo; ggml-small es asumible).
//   (b) [Optimización futura] Un isolate de larga vida que mantenga el ctx vivo
//       y reciba PCM por un puerto; evita reLeer el modelo. Más complejo
//       (gestión del ciclo de vida del isolate, backpressure, errores).
// Ver 'todos': migrar a (b) para modelos grandes / dictado intensivo.
//
// Este fichero usa imports RELATIVOS dentro de lib/ (regla del proyecto).

import 'dart:ffi';
import 'dart:io';
import 'dart:isolate';
import 'dart:math' as math;
import 'dart:typed_data';

import 'package:ffi/ffi.dart';

import '../../core/config.dart';
import '../../platform/whisper_engine.dart';
import 'whisper_bindings.dart';

/// Motor Whisper on-device basado en whisper.cpp (dart:ffi).
class WhisperEngineFfi implements WhisperEngine {
  String? _modelPath;
  bool _loaded = false;

  /// Audio esperado por whisper.cpp: PCM mono a 16 kHz.
  static const int _sampleRate = 16000;

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

    // Validación temprana de la DLL: abrir los bindings aquí (en el isolate
    // principal) para fallar rápido y con un mensaje claro si falta whisper.dll,
    // en lugar de descubrirlo dentro del primer isolate de transcripción.
    // (La carga real para transcribir se rehace dentro del isolate.)
    WhisperBindings.open();

    _modelPath = modelPath;
    _loaded = true;
  }

  @override
  Future<TranscriptResult> transcribe(
    Float32List pcm16kMonoF32, {
    String language = 'es',
    String? initialPrompt,
    int? threads,
  }) async {
    if (!_loaded || _modelPath == null) {
      throw StateError(
        'WhisperEngineFfi no está cargado. Llama a load(modelPath) primero.',
      );
    }

    final audioMs =
        ((pcm16kMonoF32.length / _sampleRate) * 1000).round();

    if (pcm16kMonoF32.isEmpty) {
      return TranscriptResult(
        text: '',
        avgLogProb: 0,
        language: language,
        audioMs: 0,
      );
    }

    final req = _TranscribeRequest(
      modelPath: _modelPath!,
      pcm: pcm16kMonoF32,
      language: language,
      initialPrompt: initialPrompt,
      // 0 = auto (whisper usará un valor por defecto sensato).
      threads: threads ?? AppConfig.defaultThreads,
      sampleRate: _sampleRate,
      audioMs: audioMs,
    );

    // Toda la parte nativa (cargar modelo + whisper_full + leer segmentos +
    // liberar) ocurre dentro del isolate. Cruzamos solo datos planos.
    final out = await Isolate.run(() => _runInIsolate(req));

    return TranscriptResult(
      text: out.text,
      avgLogProb: out.avgLogProb,
      language: out.language,
      audioMs: out.audioMs,
    );
  }

  @override
  Future<void> dispose() async {
    // Con la estrategia (a) no mantenemos un ctx vivo entre llamadas: cada
    // transcripción crea y libera su propio contexto dentro del isolate. Aquí
    // solo invalidamos el estado lógico.
    _loaded = false;
    _modelPath = null;
  }
}

// ===========================================================================
// Código que se ejecuta DENTRO del isolate (sin acceso al estado de la clase).
// Solo recibe/devuelve datos serializables por SendPort.
// ===========================================================================

/// Datos de entrada para el isolate (todos serializables).
class _TranscribeRequest {
  final String modelPath;
  final Float32List pcm;
  final String language;
  final String? initialPrompt;
  final int threads;
  final int sampleRate;
  final int audioMs;

  const _TranscribeRequest({
    required this.modelPath,
    required this.pcm,
    required this.language,
    required this.initialPrompt,
    required this.threads,
    required this.sampleRate,
    required this.audioMs,
  });
}

/// Resultado plano devuelto por el isolate.
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

/// Punto de entrada de la transcripción dentro del isolate.
///
/// Abre la DLL, inicializa el contexto desde el modelo, ejecuta whisper_full,
/// agrega el texto de los segmentos y estima el avgLogProb, y libera todo.
_TranscribeResult _runInIsolate(_TranscribeRequest req) {
  final bindings = WhisperBindings.open();

  // --- Inicializar el contexto desde el modelo ---
  final pathPtr = req.modelPath.toNativeUtf8();
  final Pointer<Void> ctx;
  try {
    ctx = bindings.initFromFile(pathPtr);
  } finally {
    calloc.free(pathPtr);
  }

  if (ctx == nullptr) {
    throw StateError(
      'whisper_init_from_file devolvió NULL para "${req.modelPath}". '
      '¿El fichero es un modelo GGML válido y compatible con esta whisper.dll?',
    );
  }

  // Punteros que hay que liberar pase lo que pase.
  Pointer<Float> samplesPtr = nullptr;
  Pointer<Utf8> langPtr = nullptr;
  Pointer<Utf8> promptPtr = nullptr;

  try {
    // --- Marshaling Float32List -> Pointer<Float> ---
    final n = req.pcm.length;
    samplesPtr = calloc<Float>(n);
    // Copia eficiente a través de la vista tipada del bloque nativo.
    samplesPtr.asTypedList(n).setAll(0, req.pcm);

    // --- Parámetros: partir de los defaults y tocar solo lo necesario ---
    // whisper_full_default_params devuelve la struct POR VALOR con todos los
    // campos ya inicializados (incluidos callbacks=nullptr, gramáticas, VAD…).
    final params =
        bindings.fullDefaultParams(WhisperSamplingStrategy.greedy);

    // Idioma (ISO 639-1). Puntero que debe sobrevivir hasta tras whisper_full.
    langPtr = req.language.toNativeUtf8();
    params.language = langPtr;
    params.detectLanguage = false;

    // Hilos (0 = dejar el valor por defecto que ya trae la struct = auto).
    if (req.threads > 0) {
      params.nThreads = req.threads;
    }

    // Silenciar toda la salida por stdout/stderr de la librería.
    params.printProgress = false;
    params.printRealtime = false;
    params.printTimestamps = false;
    params.printSpecial = false;

    // No necesitamos timestamps por token para el caso de uso de dictado.
    params.noTimestamps = true;
    params.translate = false;

    // initial_prompt para sesgar vocabulario (nombres propios, jerga).
    final prompt = req.initialPrompt;
    if (prompt != null && prompt.trim().isNotEmpty) {
      promptPtr = prompt.toNativeUtf8();
      params.initialPrompt = promptPtr;
    }

    // --- Ejecutar la transcripción ---
    final rc = bindings.full(ctx, params, samplesPtr, n);
    if (rc != 0) {
      throw StateError('whisper_full falló con código $rc.');
    }

    // --- Leer segmentos + estimar confianza ---
    final nSeg = bindings.fullNSegments(ctx);
    final buffer = StringBuffer();
    double sumLogProb = 0;
    int tokenCount = 0;

    for (var i = 0; i < nSeg; i++) {
      final segTextPtr = bindings.fullGetSegmentText(ctx, i);
      if (segTextPtr != nullptr) {
        buffer.write(segTextPtr.toDartString());
      }

      // avgLogProb: whisper expone p (prob) por token; aproximamos el log-prob
      // medio como mean(ln(p_token)). Si por alguna razón no hay tokens,
      // dejamos 0.0 (= "muy seguro", no dispara post-corrección).
      final nTok = bindings.fullNTokens(ctx, i);
      for (var t = 0; t < nTok; t++) {
        final pTok = bindings.fullGetTokenP(ctx, i, t);
        if (pTok > 0) {
          sumLogProb += math.log(pTok);
          tokenCount++;
        }
      }
    }

    final avgLogProb = tokenCount > 0 ? sumLogProb / tokenCount : 0.0;
    final text = buffer.toString().trim();

    return _TranscribeResult(
      text: text,
      avgLogProb: avgLogProb,
      language: req.language,
      audioMs: req.audioMs,
    );
  } finally {
    // Liberar SIEMPRE el contexto y la memoria nativa, también ante excepción.
    bindings.free(ctx);
    if (samplesPtr != nullptr) calloc.free(samplesPtr);
    if (langPtr != nullptr) calloc.free(langPtr);
    if (promptPtr != nullptr) calloc.free(promptPtr);
  }
}
