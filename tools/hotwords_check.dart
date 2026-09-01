// Comprobación de que el biasing por hotwords ESTÁ HACIENDO ALGO.
//
// Transcribe el mismo audio dos veces contra el mismo modelo: una en
// greedy_search sin hotwords (lo que hacía Pronto hasta la 0.7.1) y otra en
// modified_beam_search con el hotwords.txt aprendido. Si las dos salidas son
// idénticas palabra por palabra, el biasing NO está entrando y no hay que
// creerse que funciona porque los tests unitarios estén en verde: esos prueban
// que el fichero se escribe bien, no que sherpa-onnx lo lea.
//
// Uso (desde la raíz del repo):
//   dart run tools/hotwords_check.dart <audio.wav> [carpetaModelo] [carpetaDlls]
//
// El audio debe ser WAV PCM 16 bits, mono, 16 kHz.
//
// OJO CON LAS DLLs. Windows trae su propio C:\Windows\System32\onnxruntime.dll
// (ORT 1.17.1) y el directorio de sistema va ANTES que el PATH en el orden de
// búsqueda, así que gana él y sherpa-onnx revienta con "The requested API
// version [24] is not available". Pronto.exe no lo sufre porque sus DLLs viven
// junto al ejecutable (que se busca primero) y porque windows/runner/main.cpp
// restringe la búsqueda; este script sí, porque quien manda es dart.exe.
// Se esquiva precargando el onnxruntime.dll correcto por RUTA ABSOLUTA: una vez
// cargado, el loader lo reutiliza por nombre base cuando sherpa lo importa.

import 'dart:ffi';
import 'dart:io';
import 'dart:typed_data';

import 'package:sherpa_onnx/sherpa_onnx.dart' as sherpa;

const String encoderFile = 'encoder.int8.onnx';
const String decoderFile = 'decoder.int8.onnx';
const String joinerFile = 'joiner.int8.onnx';
const String tokensFile = 'tokens.txt';
const String hotwordsFile = 'hotwords.txt';
const String bpeVocabFile = 'bpe.vocab';

void main(List<String> args) {
  if (args.isEmpty) {
    stderr.writeln('Uso: dart run tools/hotwords_check.dart <audio.wav> '
        '[carpetaModelo]');
    exitCode = 64;
    return;
  }

  final wavPath = args[0];
  final modelDir = args.length > 1
      ? args[1]
      : '${Platform.environment['APPDATA']}\\Pronto\\Pronto\\models\\parakeet';
  final sep = Platform.pathSeparator;

  final hotwords = File('$modelDir$sep$hotwordsFile');
  final bpeVocab = File('$modelDir$sep$bpeVocabFile');
  if (!hotwords.existsSync() || !bpeVocab.existsSync()) {
    stderr.writeln('Faltan $hotwordsFile o $bpeVocabFile en "$modelDir". '
        'Sin las dos piezas no hay biasing que comprobar.');
    exitCode = 66;
    return;
  }

  final terms = hotwords
      .readAsLinesSync()
      .where((l) => l.trim().isNotEmpty)
      .toList();
  stdout.writeln('modelo   : $modelDir');
  stdout.writeln('audio    : $wavPath');
  stdout.writeln('hotwords : ${terms.length} términos '
      '(${terms.take(8).join(', ')}${terms.length > 8 ? ', …' : ''})');
  stdout.writeln('');

  _preloadNativeLibs(args.length > 2 ? args[2] : _defaultDllDir());
  sherpa.initBindings();
  final samples = _readWav16kMono(File(wavPath));
  stdout.writeln('audio leído: ${(samples.length / 16000).toStringAsFixed(1)} s'
      '\n');

  final sinSesgo = _transcribe(modelDir, samples, biasing: false);
  final conSesgo = _transcribe(modelDir, samples, biasing: true);

  stdout.writeln('--- SIN hotwords (greedy_search) ---');
  stdout.writeln(sinSesgo);
  stdout.writeln('');
  stdout.writeln('--- CON hotwords (modified_beam_search) ---');
  stdout.writeln(conSesgo);
  stdout.writeln('');

  if (sinSesgo.trim() == conSesgo.trim()) {
    stdout.writeln('RESULTADO: IDÉNTICAS. El biasing no cambió nada en este '
        'audio — o no está entrando, o ninguno de los términos sale aquí.');
    exitCode = 1;
    return;
  }

  stdout.writeln('RESULTADO: DISTINTAS. El biasing está entrando.');
  for (final t in terms) {
    final antes = sinSesgo.toLowerCase().contains(t.toLowerCase());
    final ahora = conSesgo.toLowerCase().contains(t.toLowerCase());
    if (!antes && ahora) stdout.writeln('  ganado: "$t"');
    if (antes && !ahora) stdout.writeln('  perdido: "$t"');
  }
}

/// Carpeta con las DLLs buenas: la del build Release del propio repo.
String _defaultDllDir() =>
    'build${Platform.pathSeparator}windows${Platform.pathSeparator}x64'
    '${Platform.pathSeparator}runner${Platform.pathSeparator}Release';

/// Carga onnxruntime y sherpa por ruta absoluta ANTES de initBindings, para
/// que el loader no se quede con el onnxruntime.dll de System32.
void _preloadNativeLibs(String dllDir) {
  final sep = Platform.pathSeparator;
  for (final name in ['onnxruntime.dll', 'sherpa-onnx-c-api.dll']) {
    final f = File('$dllDir$sep$name');
    if (!f.existsSync()) {
      stderr.writeln('AVISO: no está $name en "$dllDir". Compila el proyecto '
          '(flutter build windows) o pasa la carpeta como tercer argumento.');
      continue;
    }
    DynamicLibrary.open(f.absolute.path);
  }
}

String _transcribe(
  String modelDir,
  Float32List samples, {
  required bool biasing,
}) {
  final sep = Platform.pathSeparator;
  final config = sherpa.OfflineRecognizerConfig(
    model: sherpa.OfflineModelConfig(
      transducer: sherpa.OfflineTransducerModelConfig(
        encoder: '$modelDir$sep$encoderFile',
        decoder: '$modelDir$sep$decoderFile',
        joiner: '$modelDir$sep$joinerFile',
      ),
      tokens: '$modelDir$sep$tokensFile',
      modelType: 'nemo_transducer',
      numThreads: 4,
      provider: 'cpu',
      modelingUnit: biasing ? 'bpe' : '',
      bpeVocab: biasing ? '$modelDir$sep$bpeVocabFile' : '',
    ),
    decodingMethod: biasing ? 'modified_beam_search' : 'greedy_search',
    hotwordsFile: biasing ? '$modelDir$sep$hotwordsFile' : '',
    hotwordsScore: 2.0,
  );

  final recognizer = sherpa.OfflineRecognizer(config);
  final started = DateTime.now();
  final stream = recognizer.createStream();
  stream.acceptWaveform(samples: samples, sampleRate: 16000);
  recognizer.decode(stream);
  final text = recognizer.getResult(stream).text;
  final ms = DateTime.now().difference(started).inMilliseconds;
  stream.free();
  recognizer.free();
  stdout.writeln('  (${biasing ? 'con' : 'sin'} sesgo: ${ms}ms)');
  return text;
}

/// Lee un WAV PCM 16 bits mono a 16 kHz y lo normaliza a [-1, 1].
Float32List _readWav16kMono(File f) {
  final bytes = f.readAsBytesSync();
  final view = ByteData.sublistView(bytes);

  // Recorre las cabeceras RIFF hasta el trozo "data" en vez de dar por hecho
  // que empieza en el byte 44: muchos WAV traen chunks extra antes.
  var offset = 12;
  var dataStart = -1;
  var dataLen = 0;
  while (offset + 8 <= bytes.length) {
    final id = String.fromCharCodes(bytes.sublist(offset, offset + 4));
    final size = view.getUint32(offset + 4, Endian.little);
    if (id == 'data') {
      dataStart = offset + 8;
      dataLen = size;
      break;
    }
    offset += 8 + size + (size.isOdd ? 1 : 0);
  }
  if (dataStart < 0) throw StateError('WAV sin trozo "data": ${f.path}');
  if (dataStart + dataLen > bytes.length) dataLen = bytes.length - dataStart;

  final n = dataLen ~/ 2;
  final out = Float32List(n);
  for (var i = 0; i < n; i++) {
    out[i] = view.getInt16(dataStart + i * 2, Endian.little) / 32768.0;
  }
  return out;
}
