// ignore_for_file: library_private_types_in_public_api
//
// Bindings dart:ffi MÍNIMOS a la API C de whisper.cpp (whisper.h, v1.9.x).
//
// Filosofía de este fichero:
//   - Mapear SOLO lo necesario para: inicializar el contexto desde un .bin,
//     ejecutar whisper_full, leer los segmentos resultantes y liberar memoria.
//   - La estructura `whisper_full_params` es GRANDE (~60 campos, con structs
//     anidados, callbacks y punteros a gramáticas/VAD). Mapearla "a mano"
//     campo a campo es frágil ante cambios de la cabecera, PERO es inevitable:
//     `whisper_full` recibe la struct POR VALOR y `whisper_full_default_params`
//     la devuelve POR VALOR. dart:ffi soporta paso/retorno de structs por valor
//     desde Dart 2.12, así que el enfoque correcto es:
//       1) Llamar a `whisper_full_default_params(GREEDY)` para obtener una copia
//          con TODOS los valores por defecto ya rellenados por la librería.
//       2) Sobre esa copia, tocar SOLO los campos que nos interesan
//          (language, n_threads, no_timestamps, print_* = false, initial_prompt).
//          Así no dependemos de adivinar los defaults del resto de campos.
//     El layout de la struct refleja EXACTAMENTE el orden y los tipos de
//     whisper.h. Si actualizas whisper.cpp y cambia la cabecera, hay que
//     revisar esta struct (ver comentarios "ABI").
//
// Referencia de firmas:
//   https://github.com/ggml-org/whisper.cpp/blob/master/include/whisper.h
//
// Tipos C relevantes (whisper.h):
//   typedef int32_t whisper_token;
//   typedef int32_t whisper_pos;
//   struct whisper_context;  // opaca

import 'dart:ffi';
import 'dart:io';

import 'package:ffi/ffi.dart';
import 'package:path/path.dart' as p;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// enum whisper_sampling_strategy { GREEDY, BEAM_SEARCH }
abstract class WhisperSamplingStrategy {
  static const int greedy = 0;
  static const int beamSearch = 1;
}

// ---------------------------------------------------------------------------
// Structs anidadas (ABI: el orden y los tipos deben coincidir con whisper.h)
// ---------------------------------------------------------------------------

/// struct { int best_of; } greedy;
final class WhisperGreedy extends Struct {
  @Int32()
  external int bestOf;
}

/// struct { int beam_size; float patience; } beam_search;
final class WhisperBeamSearch extends Struct {
  @Int32()
  external int beamSize;

  @Float()
  external double patience;
}

/// struct whisper_vad_params { float; int; int; float; int; float; }
final class WhisperVadParams extends Struct {
  @Float()
  external double threshold;

  @Int32()
  external int minSpeechDurationMs;

  @Int32()
  external int minSilenceDurationMs;

  @Float()
  external double maxSpeechDurationS;

  @Int32()
  external int speechPadMs;

  @Float()
  external double samplesOverlap;
}

// ---------------------------------------------------------------------------
// struct whisper_context_params (se pasa por valor a init_from_file_with_params)
//
// ABI whisper.h:
//   bool   use_gpu;
//   bool   flash_attn;
//   int    gpu_device;
//   bool   dtw_token_timestamps;
//   enum   dtw_aheads_preset;          // int
//   int    dtw_n_top;
//   struct whisper_aheads dtw_aheads;  // { size_t n_heads; const void* heads; }
//   size_t dtw_mem_size;
//
// Para evitar ambigüedades de tamaño con `whisper_aheads` (tiene size_t +
// puntero), preferimos NO pasar context_params a mano: usamos
// whisper_init_from_file(path) que no requiere struct alguna. Mantengo la
// struct documentada por si en el futuro hace falta activar la GPU.
// ---------------------------------------------------------------------------

/// struct whisper_aheads { size_t n_heads; const whisper_ahead* heads; }
final class WhisperAheads extends Struct {
  @Size()
  external int nHeads;

  external Pointer<Void> heads;
}

/// struct whisper_context_params (ver nota ABI arriba).
final class WhisperContextParams extends Struct {
  @Bool()
  external bool useGpu;

  @Bool()
  external bool flashAttn;

  @Int32()
  external int gpuDevice;

  @Bool()
  external bool dtwTokenTimestamps;

  @Int32()
  external int dtwAheadsPreset;

  @Int32()
  external int dtwNTop;

  external WhisperAheads dtwAheads;

  @Size()
  external int dtwMemSize;
}

// ---------------------------------------------------------------------------
// struct whisper_full_params  (ABI CRÍTICO — orden exacto de whisper.h)
//
// Los callbacks y los punteros a gramáticas se modelan como Pointer<Void>
// (no los usamos; sus defaults son nullptr y los dejamos intactos). Lo único
// que toca el resto del código es:
//   strategy, n_threads, language, no_timestamps, print_*, initial_prompt.
// ---------------------------------------------------------------------------

final class WhisperFullParams extends Struct {
  @Int32() // enum whisper_sampling_strategy
  external int strategy;

  @Int32()
  external int nThreads;

  @Int32()
  external int nMaxTextCtx;

  @Int32()
  external int offsetMs;

  @Int32()
  external int durationMs;

  @Bool()
  external bool translate;

  @Bool()
  external bool noContext;

  @Bool()
  external bool noTimestamps;

  @Bool()
  external bool singleSegment;

  @Bool()
  external bool printSpecial;

  @Bool()
  external bool printProgress;

  @Bool()
  external bool printRealtime;

  @Bool()
  external bool printTimestamps;

  @Bool()
  external bool tokenTimestamps;

  @Float()
  external double tholdPt;

  @Float()
  external double tholdPtsum;

  @Int32()
  external int maxLen;

  @Bool()
  external bool splitOnWord;

  @Int32()
  external int maxTokens;

  @Bool()
  external bool debugMode;

  @Int32()
  external int audioCtx;

  @Bool()
  external bool tdrzEnable;

  external Pointer<Utf8> suppressRegex; // const char*

  external Pointer<Utf8> initialPrompt; // const char*

  @Bool()
  external bool carryInitialPrompt;

  external Pointer<Int32> promptTokens; // const whisper_token*

  @Int32()
  external int promptNTokens;

  external Pointer<Utf8> language; // const char*

  @Bool()
  external bool detectLanguage;

  @Bool()
  external bool suppressBlank;

  @Bool()
  external bool suppressNst;

  @Float()
  external double temperature;

  @Float()
  external double maxInitialTs;

  @Float()
  external double lengthPenalty;

  @Float()
  external double temperatureInc;

  @Float()
  external double entropyThold;

  @Float()
  external double logprobThold;

  @Float()
  external double noSpeechThold;

  // struct { int best_of; } greedy;
  external WhisperGreedy greedy;

  // struct { int beam_size; float patience; } beam_search;
  external WhisperBeamSearch beamSearch;

  // Callbacks: punteros a función. No los usamos -> Pointer<Void> (nullptr).
  external Pointer<Void> newSegmentCallback;
  external Pointer<Void> newSegmentCallbackUserData;

  external Pointer<Void> progressCallback;
  external Pointer<Void> progressCallbackUserData;

  external Pointer<Void> encoderBeginCallback;
  external Pointer<Void> encoderBeginCallbackUserData;

  external Pointer<Void> abortCallback;
  external Pointer<Void> abortCallbackUserData;

  external Pointer<Void> logitsFilterCallback;
  external Pointer<Void> logitsFilterCallbackUserData;

  // const whisper_grammar_element ** grammar_rules;
  external Pointer<Void> grammarRules;

  @Size()
  external int nGrammarRules;

  @Size()
  external int iStartRule;

  @Float()
  external double grammarPenalty;

  @Bool()
  external bool vad;

  external Pointer<Utf8> vadModelPath; // const char*

  external WhisperVadParams vadParams;
}

// ---------------------------------------------------------------------------
// Firmas C <-> Dart (typedefs)
// ---------------------------------------------------------------------------

// struct whisper_context* whisper_init_from_file(const char* path_model);
typedef _InitFromFileC = Pointer<Void> Function(Pointer<Utf8> pathModel);
typedef _InitFromFileD = Pointer<Void> Function(Pointer<Utf8> pathModel);

// struct whisper_full_params whisper_full_default_params(
//     enum whisper_sampling_strategy strategy);
typedef _FullDefaultParamsC = WhisperFullParams Function(Int32 strategy);
typedef _FullDefaultParamsD = WhisperFullParams Function(int strategy);

// int whisper_full(struct whisper_context* ctx, struct whisper_full_params,
//                  const float* samples, int n_samples);
typedef _FullC = Int32 Function(Pointer<Void> ctx, WhisperFullParams params,
    Pointer<Float> samples, Int32 nSamples,);
typedef _FullD = int Function(Pointer<Void> ctx, WhisperFullParams params,
    Pointer<Float> samples, int nSamples,);

// int whisper_full_n_segments(struct whisper_context* ctx);
typedef _NSegmentsC = Int32 Function(Pointer<Void> ctx);
typedef _NSegmentsD = int Function(Pointer<Void> ctx);

// const char* whisper_full_get_segment_text(ctx, int i_segment);
typedef _GetSegmentTextC = Pointer<Utf8> Function(
    Pointer<Void> ctx, Int32 iSegment,);
typedef _GetSegmentTextD = Pointer<Utf8> Function(
    Pointer<Void> ctx, int iSegment,);

// int64_t whisper_full_get_segment_t0/t1(ctx, int i_segment);
typedef _GetSegmentTC = Int64 Function(Pointer<Void> ctx, Int32 iSegment);
typedef _GetSegmentTD = int Function(Pointer<Void> ctx, int iSegment);

// int whisper_full_n_tokens(ctx, int i_segment);
typedef _NTokensC = Int32 Function(Pointer<Void> ctx, Int32 iSegment);
typedef _NTokensD = int Function(Pointer<Void> ctx, int iSegment);

// float whisper_full_get_token_p(ctx, int i_segment, int i_token);
typedef _GetTokenPC = Float Function(
    Pointer<Void> ctx, Int32 iSegment, Int32 iToken,);
typedef _GetTokenPD = double Function(
    Pointer<Void> ctx, int iSegment, int iToken,);

// void whisper_free(struct whisper_context* ctx);
typedef _FreeC = Void Function(Pointer<Void> ctx);
typedef _FreeD = void Function(Pointer<Void> ctx);

// ---------------------------------------------------------------------------
// Carga de la librería + lookups
// ---------------------------------------------------------------------------

/// Excepción específica cuando no se localiza/carga la DLL de whisper.
class WhisperLibraryNotFound implements Exception {
  final String message;
  WhisperLibraryNotFound(this.message);
  @override
  String toString() => 'WhisperLibraryNotFound: $message';
}

/// Envoltorio de la librería nativa whisper.cpp ya enlazada.
///
/// Se construye una vez (es barato) y expone las funciones C como métodos
/// Dart. La carga del modelo y la transcripción se hacen aparte.
class WhisperBindings {
  final DynamicLibrary _lib;

  late final _InitFromFileD initFromFile =
      _lib.lookupFunction<_InitFromFileC, _InitFromFileD>(
          'whisper_init_from_file',);

  late final _FullDefaultParamsD fullDefaultParams =
      _lib.lookupFunction<_FullDefaultParamsC, _FullDefaultParamsD>(
          'whisper_full_default_params',);

  late final _FullD full =
      _lib.lookupFunction<_FullC, _FullD>('whisper_full');

  late final _NSegmentsD fullNSegments =
      _lib.lookupFunction<_NSegmentsC, _NSegmentsD>('whisper_full_n_segments');

  late final _GetSegmentTextD fullGetSegmentText =
      _lib.lookupFunction<_GetSegmentTextC, _GetSegmentTextD>(
          'whisper_full_get_segment_text',);

  late final _GetSegmentTD fullGetSegmentT0 =
      _lib.lookupFunction<_GetSegmentTC, _GetSegmentTD>(
          'whisper_full_get_segment_t0',);

  late final _GetSegmentTD fullGetSegmentT1 =
      _lib.lookupFunction<_GetSegmentTC, _GetSegmentTD>(
          'whisper_full_get_segment_t1',);

  late final _NTokensD fullNTokens =
      _lib.lookupFunction<_NTokensC, _NTokensD>('whisper_full_n_tokens');

  late final _GetTokenPD fullGetTokenP =
      _lib.lookupFunction<_GetTokenPC, _GetTokenPD>(
          'whisper_full_get_token_p',);

  late final _FreeD free =
      _lib.lookupFunction<_FreeC, _FreeD>('whisper_free');

  WhisperBindings._(this._lib);

  /// Abre la librería nativa.
  ///
  /// SEGURIDAD (DLL planting): en Windows se carga SIEMPRE por ruta ABSOLUTA
  /// junto al ejecutable (`whisper.dll` en la carpeta de
  /// Platform.resolvedExecutable). NO se cae de vuelta a `'whisper.dll'` a
  /// secas, que dejaría al cargador de Windows resolverla vía el directorio de
  /// trabajo o el PATH (vector de secuestro de DLL).
  /// En Linux/macOS intenta libwhisper.so / libwhisper.dylib (best-effort; el
  /// objetivo principal del MVP es Windows escritorio).
  ///
  /// Lanza [WhisperLibraryNotFound] con un mensaje claro si no la encuentra.
  factory WhisperBindings.open() {
    return WhisperBindings._(_openLibrary());
  }

  static DynamicLibrary _openLibrary() {
    final candidates = _libraryCandidates();
    Object? lastError;

    for (final path in candidates) {
      try {
        return DynamicLibrary.open(path);
      } catch (e) {
        lastError = e;
      }
    }

    throw WhisperLibraryNotFound(
      'No se pudo cargar la librería nativa de whisper.cpp.\n'
      'Rutas probadas: ${candidates.join(", ")}.\n'
      'En Windows necesitas "whisper.dll" junto al ejecutable de Pronto '
      '(o en el PATH del sistema).\n'
      'Compila whisper.dll: ver BUILD_WHISPER.md.\n'
      'Último error del cargador: $lastError',
    );
  }

  /// Lista de rutas candidatas a probar, dependientes de la plataforma.
  static List<String> _libraryCandidates() {
    final exeDir = p.dirname(Platform.resolvedExecutable);

    if (Platform.isWindows) {
      // Solo ruta absoluta junto al .exe: NO caemos a 'whisper.dll' a secas
      // para no dejar que el cargador la resuelva vía cwd/PATH (DLL planting).
      // Las dependencias (ggml.dll…) las resuelve el cargador desde esta misma
      // carpeta, que es un directorio seguro por defecto.
      return <String>[
        p.join(exeDir, 'whisper.dll'),
      ];
    }

    if (Platform.isMacOS) {
      return <String>[
        p.join(exeDir, 'libwhisper.dylib'),
      ];
    }

    // Linux y otros POSIX.
    return <String>[
      p.join(exeDir, 'libwhisper.so'),
    ];
  }
}
