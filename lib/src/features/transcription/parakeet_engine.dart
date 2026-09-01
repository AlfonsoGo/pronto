// Motor de voz alternativo: NVIDIA Parakeet (transducer) vía sherpa-onnx.
//
// Implementa la MISMA interfaz [WhisperEngine] que el motor whisper.cpp, así el
// resto de la app (DictationController) no cambia: solo se elige uno u otro en
// Ajustes. Igual que el motor whisper, mantiene el modelo residente en un
// ISOLATE de larga vida (carga una vez, transcribe muchas) para no recargar ni
// bloquear la UI.
//
// A diferencia de whisper, Parakeet:
//   - NO usa initial_prompt (un transducer no sesga por prompt). El biasing se
//     hace con HOTWORDS: una lista de términos que se inyecta en la búsqueda.
//   - trae puntuación y mayúsculas "de fábrica" y no alucina en silencio.
//
// El "modelPath" que recibe load() es la CARPETA del modelo (no un fichero):
// debe contener encoder.int8.onnx, decoder.int8.onnx, joiner.int8.onnx y
// tokens.txt (formato sherpa-onnx para Parakeet TDT).
//
// BIASING (hotwords). Si en esa misma carpeta están ADEMÁS hotwords.txt y
// bpe.vocab, el motor arranca sesgado hacia tu vocabulario. Requiere las tres
// cosas a la vez, y por eso es fácil que parezca activo sin estarlo:
//   1. decodingMethod = 'modified_beam_search'  (con greedy_search sherpa-onnx
//      IGNORA los hotwords EN SILENCIO — no avisa, simplemente no sesga),
//   2. modelingUnit = 'bpe' + bpe.vocab para poder tokenizar los términos,
//   3. el propio hotwords.txt, que escribe LearningService desde lo aprendido.
// Soportado para transducers NeMo desde sherpa-onnx v1.13.0 (PR #3077).
// [hotwordsActive] dice si de verdad quedó puesto; no lo des por hecho.

import 'dart:async';
import 'dart:io';
import 'dart:isolate';
import 'dart:typed_data';

import 'package:sherpa_onnx/sherpa_onnx.dart' as sherpa;

import '../../core/config.dart';
import '../../platform/whisper_engine.dart';

class ParakeetEngine implements WhisperEngine {
  static const int _sampleRate = 16000;

  // Ficheros esperados dentro de la carpeta del modelo.
  static const String encoderFile = 'encoder.int8.onnx';
  static const String decoderFile = 'decoder.int8.onnx';
  static const String joinerFile = 'joiner.int8.onnx';
  static const String tokensFile = 'tokens.txt';

  Isolate? _isolate;
  SendPort? _toIsolate;
  ReceivePort? _fromIsolate;
  StreamSubscription<dynamic>? _sub;
  bool _loaded = false;

  int _nextId = 0;
  final Map<int, Completer<String>> _pending = {};
  bool _hotwordsActive = false;

  @override
  bool get isLoaded => _loaded;

  /// Si el motor cargado está REALMENTE sesgado con tus hotwords.
  ///
  /// Falso cuando falta `hotwords.txt` o `bpe.vocab`. Lo expone para que la UI
  /// y los diagnósticos puedan decirlo en vez de suponerlo: un biasing que se
  /// cae en silencio es indistinguible de uno que no sirve.
  bool get hotwordsActive => _hotwordsActive;

  /// [modelPath] aquí es la CARPETA del modelo Parakeet.
  @override
  Future<void> load(String modelPath) async {
    final sep = Platform.pathSeparator;
    final tokens = File('$modelPath$sep$tokensFile');
    if (!await tokens.exists()) {
      throw StateError(
        'No se encontró el modelo Parakeet en "$modelPath" '
        '(falta $tokensFile). Descárgalo o cambia de motor en Ajustes.',
      );
    }
    if (_loaded) return;

    // Los hotwords solo se activan si están LAS DOS piezas. Con una sola, se
    // arrancaría en modified_beam_search (más lento) sin sesgar nada.
    final hotwords = File('$modelPath$sep${AppConfig.hotwordsFile}');
    final bpeVocab = File('$modelPath$sep${AppConfig.bpeVocabFile}');
    final useHotwords = await hotwords.exists() &&
        await bpeVocab.exists() &&
        (await hotwords.length()) > 0;

    final ready = ReceivePort();
    final fromIsolate = ReceivePort();
    _fromIsolate = fromIsolate;

    _isolate = await Isolate.spawn(
      _isolateMain,
      _InitMsg(
        modelDir: modelPath,
        hotwordsPath: useHotwords ? hotwords.path : null,
        bpeVocabPath: useHotwords ? bpeVocab.path : null,
        readyPort: ready.sendPort,
        responsePort: fromIsolate.sendPort,
      ),
      debugName: 'parakeet',
      errorsAreFatal: true,
    );
    _hotwordsActive = useHotwords;

    final first = await ready.first;
    ready.close();
    if (first is! SendPort) {
      await _shutdown();
      throw StateError('No se pudo cargar el modelo Parakeet: $first');
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
    if (list[1] as bool) {
      completer.complete(list[2] as String);
    } else {
      completer.completeError(StateError(list[2] as String));
    }
  }

  @override
  Future<TranscriptResult> transcribe(
    Float32List pcm16kMonoF32, {
    String language = 'es',
    String? initialPrompt, // ignorado por Parakeet (ver cabecera)
    int? threads,
  }) async {
    final port = _toIsolate;
    if (!_loaded || port == null) {
      throw StateError('ParakeetEngine no está cargado. Llama a load() primero.');
    }
    final audioMs = ((pcm16kMonoF32.length / _sampleRate) * 1000).round();
    if (pcm16kMonoF32.isEmpty) {
      return TranscriptResult(
        text: '', avgLogProb: 0, language: language, audioMs: 0,
      );
    }

    final id = _nextId++;
    final completer = Completer<String>();
    _pending[id] = completer;
    port.send([id, pcm16kMonoF32]);

    final text = await completer.future;
    // Un transducer no da un logprob comparable; devolvemos 0 = "confianza alta"
    // (no dispara la post-corrección LLM, que es opcional de todos modos).
    return TranscriptResult(
      text: text, avgLogProb: 0, language: language, audioMs: audioMs,
    );
  }

  @override
  Future<void> dispose() async => _shutdown();

  Future<void> _shutdown() async {
    _loaded = false;
    _hotwordsActive = false;
    try {
      _toIsolate?.send(const ['shutdown']);
    } catch (_) {}
    _toIsolate = null;
    await _sub?.cancel();
    _sub = null;
    _fromIsolate?.close();
    _fromIsolate = null;
    for (final c in _pending.values) {
      if (!c.isCompleted) c.completeError(StateError('Motor Parakeet cerrado.'));
    }
    _pending.clear();
    _isolate?.kill(priority: Isolate.beforeNextEvent);
    _isolate = null;
  }
}

// ===========================================================================
// Isolate de larga vida: carga el recognizer una vez y atiende peticiones.
// ===========================================================================

class _InitMsg {
  final String modelDir;

  /// Rutas del biasing. Van SIEMPRE las dos o ninguna (lo garantiza load()).
  final String? hotwordsPath;
  final String? bpeVocabPath;

  final SendPort readyPort;
  final SendPort responsePort;
  const _InitMsg({
    required this.modelDir,
    required this.hotwordsPath,
    required this.bpeVocabPath,
    required this.readyPort,
    required this.responsePort,
  });
}

void _isolateMain(_InitMsg init) {
  final sherpa.OfflineRecognizer recognizer;
  try {
    // Carga la DLL nativa (sherpa-onnx-c-api.dll) e inicializa los bindings en
    // ESTE isolate. Sin esto, las llamadas FFI fallarían.
    // SEGURIDAD (DLL planting): sherpa_onnx abre su DLL por nombre simple y NO
    // expone forma de fijar una ruta absoluta, así que aquí no se puede
    // endurecer la carga. La mitigación vive en windows/runner/main.cpp
    // (SetDefaultDllDirectories + SetDllDirectory("")), que restringe la
    // búsqueda de DLLs del proceso a directorios seguros y saca el cwd/PATH del
    // orden de búsqueda, cubriendo también a las DLLs de sherpa_onnx.
    sherpa.initBindings();

    final sep = Platform.pathSeparator;
    final hotwords = init.hotwordsPath;
    final bpeVocab = init.bpeVocabPath;
    final biasing = hotwords != null && bpeVocab != null;

    final config = sherpa.OfflineRecognizerConfig(
      model: sherpa.OfflineModelConfig(
        transducer: sherpa.OfflineTransducerModelConfig(
          encoder: '${init.modelDir}$sep${ParakeetEngine.encoderFile}',
          decoder: '${init.modelDir}$sep${ParakeetEngine.decoderFile}',
          joiner: '${init.modelDir}$sep${ParakeetEngine.joinerFile}',
        ),
        tokens: '${init.modelDir}$sep${ParakeetEngine.tokensFile}',
        modelType: 'nemo_transducer',
        numThreads: 4,
        provider: 'cpu',
        // Sin bpe.vocab los hotwords no se pueden tokenizar y se descartan.
        modelingUnit: biasing ? 'bpe' : '',
        bpeVocab: biasing ? bpeVocab : '',
        debug: false,
      ),
      // greedy_search es más rápido, pero IGNORA los hotwords sin avisar. Solo
      // se paga el beam search cuando hay vocabulario que sesgar.
      decodingMethod: biasing ? 'modified_beam_search' : 'greedy_search',
      hotwordsFile: biasing ? hotwords : '',
      hotwordsScore: AppConfig.hotwordsScore,
    );
    recognizer = sherpa.OfflineRecognizer(config);
  } catch (e) {
    init.readyPort.send('$e');
    return;
  }

  final inbox = ReceivePort();
  init.readyPort.send(inbox.sendPort);

  inbox.listen((msg) {
    final list = msg as List;
    if (list.isNotEmpty && list[0] == 'shutdown') {
      recognizer.free();
      inbox.close();
      return;
    }
    final id = list[0] as int;
    final pcm = list[1] as Float32List;
    try {
      final stream = recognizer.createStream();
      stream.acceptWaveform(samples: pcm, sampleRate: _sampleRate16k);
      recognizer.decode(stream);
      final text = recognizer.getResult(stream).text.trim();
      stream.free();
      init.responsePort.send([id, true, text]);
    } catch (e) {
      init.responsePort.send([id, false, '$e']);
    }
  });
}

const int _sampleRate16k = 16000;
