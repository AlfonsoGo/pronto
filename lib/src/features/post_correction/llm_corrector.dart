import 'dart:async';
import 'dart:convert';

import 'package:http/http.dart' as http;

import '../../core/config.dart';
import '../learning/word_diff.dart';

/// Post-corrección opcional mediante un LLM (off por defecto).
///
/// Su ÚNICA misión es arreglar **puntuación**, **mayúsculas** y la
/// **capitalización de nombres propios** de una transcripción de Whisper, sin
/// reescribir el contenido (no añade, elimina ni cambia palabras de contenido).
///
/// Está diseñada para ser segura por construcción:
///  - Está **desactivada por defecto** ([enabled] = false).
///  - Se **salta** cuando el texto ya es muy seguro (gate por confianza), para
///    evitar parafraseo innecesario.
///  - **Verifica a posteriori** la salida del LLM midiendo la desviación de
///    palabras-contenido frente a la entrada; si el modelo se desvía demasiado
///    (alucinación / reescritura), DESCARTA su salida y devuelve la entrada.
///  - Ante **timeouts o errores** devuelve siempre el texto de entrada
///    (degradación segura): nunca empeora el resultado del usuario.
///
/// Soporta tanto Ollama local (endpoint nativo `/api/chat`) como cualquier
/// servidor compatible con OpenAI (`/v1/chat/completions`).
class LlmCorrector {
  /// [enabled]: si es false, [correct] devuelve el texto sin tocar.
  /// [baseUrl]: raíz del servidor (Ollama por defecto, sin barra final).
  /// [model]: nombre del modelo a usar.
  /// [useCloud]: si es true usa el endpoint OpenAI-compatible
  ///   `{baseUrl}/v1/chat/completions`; si es false usa el nativo de Ollama
  ///   `{baseUrl}/api/chat`.
  /// [apiKey]: clave de API (necesaria para servidores en la nube; en Ollama
  ///   local es opcional/ignorada).
  /// [timeout]: tiempo máximo de espera de la llamada HTTP.
  /// [httpClient]: cliente HTTP inyectable (útil para pruebas).
  LlmCorrector({
    this.enabled = false,
    this.baseUrl = 'http://localhost:11434',
    this.model = 'qwen3:4b',
    this.useCloud = false,
    this.apiKey,
    this.timeout = const Duration(seconds: 20),
    http.Client? httpClient,
  }) : _client = httpClient ?? http.Client();

  final bool enabled;
  final String baseUrl;
  final String model;
  final bool useCloud;
  final String? apiKey;
  final Duration timeout;

  final http.Client _client;

  /// Instrucción de sistema ESTRICTA en es-ES: el modelo solo puede reparar
  /// puntuación y mayúsculas, nunca el contenido.
  static const String _systemPrompt = '''
Eres un corrector ortotipográfico en español (es-ES). Recibes una transcripción
de voz y debes devolverla corregida ÚNICAMENTE en estos aspectos:
- Puntuación (puntos, comas, signos de interrogación/exclamación de apertura y
  cierre, etc.).
- Mayúsculas al inicio de frase.
- Capitalización de nombres propios (personas, lugares, marcas).

REGLAS ABSOLUTAS:
- NO cambies, NO añadas y NO elimines ninguna palabra de contenido.
- NO reformules, NO parafrasees, NO traduzcas y NO resumas.
- NO corrijas la gramática ni el estilo; respeta las palabras tal cual están.
- Conserva el orden exacto de las palabras.
- Devuelve SOLO el texto corregido, sin comillas, sin explicaciones, sin notas
  y sin etiquetas de razonamiento.''';

  /// Ejemplos few-shot (entrada cruda -> salida solo con puntuación/mayúsculas).
  static const List<Map<String, String>> _fewShots = [
    {
      'user': 'hola me llamo juan y vivo en madrid',
      'assistant': 'Hola, me llamo Juan y vivo en Madrid.',
    },
    {
      'user': 'que hora es no llego a la reunion',
      'assistant': '¿Qué hora es? No llego a la reunión.',
    },
    {
      'user': 'compre pan leche y huevos en el mercado',
      'assistant': 'Compré pan, leche y huevos en el mercado.',
    },
  ];

  /// Corrige [text] respetando el contenido. [avgLogProb] es la confianza media
  /// de Whisper (más cercano a 0 = más seguro).
  ///
  /// Devuelve el texto de entrada sin cambios si:
  ///  - la post-corrección está desactivada;
  ///  - el texto está vacío;
  ///  - la confianza es alta ([avgLogProb] >= [AppConfig.llmConfidenceGate]);
  ///  - hay timeout o error de red/servidor;
  ///  - la salida del LLM se desvía demasiado del contenido original.
  Future<String> correct(String text, double avgLogProb) async {
    // 1) Desactivado o texto vacío: passthrough.
    if (!enabled) return text;
    if (text.trim().isEmpty) return text;

    // 2) Gate por confianza: si Whisper está muy seguro, no tocamos el texto
    //    (evita parafraseo en transcripciones ya limpias).
    if (avgLogProb >= AppConfig.llmConfidenceGate) return text;

    try {
      final corrected = await _callLlm(text);
      if (corrected == null) return text;

      final cleaned = _cleanModelOutput(corrected);
      if (cleaned.isEmpty) return text;

      // 3) Verificación post-hoc: si el LLM cambió contenido, descartamos.
      if (_contentDrift(text, cleaned) > AppConfig.llmMaxContentDrift) {
        return text;
      }
      return cleaned;
    } catch (_) {
      // Cualquier fallo (timeout, red, parseo, servidor): degradación segura.
      return text;
    }
  }

  /// Libera el cliente HTTP. Llamar cuando el corrector deja de usarse.
  void dispose() => _client.close();

  // --- Internos ---------------------------------------------------------------

  /// Realiza la llamada al LLM y devuelve el contenido de la respuesta, o null
  /// si no se pudo extraer.
  Future<String?> _callLlm(String text) async {
    final messages = _buildMessages(text);
    final uri = useCloud
        ? Uri.parse('$_normalizedBase/v1/chat/completions')
        : Uri.parse('$_normalizedBase/api/chat');

    final headers = <String, String>{'Content-Type': 'application/json'};
    if (apiKey != null && apiKey!.isNotEmpty) {
      headers['Authorization'] = 'Bearer ${apiKey!}';
    }

    final body = useCloud
        ? jsonEncode({
            'model': model,
            'messages': messages,
            'temperature': 0,
            'stream': false,
          })
        : jsonEncode({
            'model': model,
            'messages': messages,
            'stream': false,
            // En Ollama nativo la temperatura va dentro de `options`.
            'options': {'temperature': 0},
          });

    final response = await _client
        .post(uri, headers: headers, body: body)
        .timeout(timeout);

    if (response.statusCode != 200) return null;

    final decoded = jsonDecode(utf8.decode(response.bodyBytes));
    if (decoded is! Map) return null;

    if (useCloud) {
      // Formato OpenAI: { choices: [ { message: { content } } ] }
      final choices = decoded['choices'];
      if (choices is List && choices.isNotEmpty) {
        final msg = choices.first;
        if (msg is Map && msg['message'] is Map) {
          final content = (msg['message'] as Map)['content'];
          if (content is String) return content;
        }
      }
      return null;
    }

    // Formato Ollama nativo: { message: { role, content } }
    final msg = decoded['message'];
    if (msg is Map && msg['content'] is String) {
      return msg['content'] as String;
    }
    return null;
  }

  /// Construye la lista de mensajes (system + few-shot + usuario).
  List<Map<String, String>> _buildMessages(String text) {
    return [
      {'role': 'system', 'content': _systemPrompt},
      for (final shot in _fewShots) ...[
        {'role': 'user', 'content': shot['user']!},
        {'role': 'assistant', 'content': shot['assistant']!},
      ],
      {'role': 'user', 'content': text},
    ];
  }

  /// Limpia artefactos comunes de la salida del modelo: comillas envolventes,
  /// bloques de razonamiento `<think>...</think>` (modelos tipo qwen3) y
  /// espacios sobrantes.
  String _cleanModelOutput(String raw) {
    var out = raw.trim();

    // Elimina bloques de razonamiento si el modelo los emitiera.
    out = out.replaceAll(
      RegExp(r'<think>.*?</think>', dotAll: true),
      '',
    );
    out = out.trim();

    // Quita comillas envolventes si las hubiera.
    if (out.length >= 2) {
      final first = out[0];
      final last = out[out.length - 1];
      const quotes = {'"', "'", '«', '“', '”', '‘', '’'};
      if ((first == '"' && last == '"') ||
          (first == "'" && last == "'") ||
          (first == '«' && last == '»') ||
          (quotes.contains(first) && quotes.contains(last) && first != last)) {
        out = out.substring(1, out.length - 1).trim();
      }
    }
    return out;
  }

  /// URL base normalizada (sin barra final).
  String get _normalizedBase {
    var b = baseUrl.trim();
    while (b.endsWith('/')) {
      b = b.substring(0, b.length - 1);
    }
    return b;
  }

  /// Mide la desviación de palabras-contenido entre [original] y [candidate],
  /// ignorando puntuación y mayúsculas.
  ///
  /// Normaliza cada token (minúsculas + sin puntuación de borde), alinea con
  /// [alignWords] y usa [changeRatio]. Un valor alto significa que el LLM
  /// añadió/eliminó/cambió palabras de contenido (no solo puntuación).
  double _contentDrift(String original, String candidate) {
    final a = _normalizedTokens(original);
    final b = _normalizedTokens(candidate);

    // Si tras normalizar ambos quedan vacíos, no hay desviación.
    if (a.isEmpty && b.isEmpty) return 0.0;
    // Si uno queda vacío y el otro no, es una desviación total.
    if (a.isEmpty || b.isEmpty) return 1.0;

    return changeRatio(alignWords(a, b));
  }

  /// Tokeniza y normaliza para comparar SOLO contenido: pasa a minúsculas y
  /// elimina la puntuación de los bordes; descarta tokens que queden vacíos
  /// (p. ej. signos sueltos como «¿», «,» o «.»).
  List<String> _normalizedTokens(String text) {
    final result = <String>[];
    for (final token in tokenizeWords(text)) {
      final norm = _stripPunctuation(token.toLowerCase());
      if (norm.isNotEmpty) result.add(norm);
    }
    return result;
  }

  /// Elimina la puntuación de los extremos de un token y la puntuación interna
  /// que no forma parte de la palabra de contenido (mantiene letras, dígitos,
  /// acentos, ñ/ü y guiones/apóstrofos internos).
  static final RegExp _punctEdges =
      RegExp(r'''^[\s\p{P}]+|[\s\p{P}]+$''', unicode: true);

  String _stripPunctuation(String token) {
    return token.replaceAll(_punctEdges, '');
  }
}
