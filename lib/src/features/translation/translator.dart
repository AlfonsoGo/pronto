import 'dart:async';
import 'dart:convert';

import 'package:http/http.dart' as http;

import '../../core/config.dart';
import '../../core/llm_output.dart';
import 'language_detect.dart';

/// Traduce el texto dictado entre español e inglés con un LLM.
///
/// Mismo contrato de seguridad que la post-corrección: **nunca empeora el
/// resultado del usuario**. Ante cualquier problema (desactivado, servidor
/// caído, timeout, respuesta vacía o inesperada) devuelve `null` y quien llama
/// se queda con el texto original.
///
/// Sirve tanto un Ollama local (endpoint nativo `/api/chat`) como cualquier
/// servidor compatible con OpenAI (`/v1/chat/completions`). Por defecto apunta
/// a Ollama en local: la traducción se queda en tu equipo, igual que el
/// reconocimiento de voz.
class Translator {
  Translator({
    this.enabled = false,
    this.baseUrl = 'http://localhost:11434',
    this.model = AppConfig.defaultTranslationModel,
    this.useCloud = false,
    this.apiKey,
    this.timeout = AppConfig.translationTimeout,
    http.Client? httpClient,
  }) : _client = httpClient ?? http.Client();

  final bool enabled;
  final String baseUrl;
  final String model;
  final bool useCloud;
  final String? apiKey;
  final Duration timeout;

  final http.Client _client;

  /// Traduce [text] de [from] a [to].
  ///
  /// Devuelve `null` si no se pudo traducir por cualquier motivo. Si [from] y
  /// [to] son el mismo idioma devuelve el texto tal cual (no gasta LLM).
  Future<String?> translate(
    String text, {
    required TextLanguage from,
    required TextLanguage to,
  }) async {
    if (!enabled) return null;
    final source = text.trim();
    if (source.isEmpty) return null;
    if (from == to) return source;

    // Tope de tamaño: un dictado gigante dispararía el tiempo de respuesta del
    // modelo local sin aportar nada. Preferimos no traducir a colgar la píldora.
    if (source.length > AppConfig.translationMaxChars) return null;

    try {
      final raw = await _callLlm(source, from: from, to: to);
      if (raw == null) return null;
      final cleaned = cleanModelOutput(raw);
      return cleaned.isEmpty ? null : cleaned;
    } catch (_) {
      // Timeout, red, parseo, servidor: degradación segura.
      return null;
    }
  }

  /// Pide al servidor local que cargue el modelo en memoria SIN traducir nada.
  ///
  /// Por qué existe: con Ollama, la primera traducción tras un rato de reposo
  /// tarda ~40 s (carga los GB del modelo) mientras que en caliente tarda ~1 s.
  /// Pronto lo dispara al EMPEZAR a grabar: mientras hablas, el modelo se está
  /// cargando, así la traducción ya lo encuentra listo. Es "dispara y olvida":
  /// cualquier fallo se ignora, esto nunca debe estorbar al dictado.
  ///
  /// Solo aplica al endpoint nativo de Ollama; un servidor en la nube no
  /// necesita precalentarse.
  Future<void> warmUp() async {
    if (!enabled || useCloud) return;
    try {
      await _client
          .post(
            Uri.parse('$_normalizedBase/api/chat'),
            headers: {'Content-Type': 'application/json'},
            body: jsonEncode({
              'model': model,
              'messages': <Map<String, String>>[],
              'keep_alive': _keepAlive,
            }),
          )
          .timeout(AppConfig.translationWarmUpTimeout);
    } catch (_) {
      // Precalentar es un lujo, no un requisito.
    }
  }

  /// Cuánto mantiene Ollama el modelo cargado tras usarlo. Con el defecto
  /// (5 min) se descargaba entre dictados espaciados y la siguiente traducción
  /// volvía a costar 40 s.
  static const String _keepAlive = '30m';

  /// Comprueba que el servidor y el modelo responden. Devuelve `null` si todo
  /// va bien, o un mensaje de error legible para mostrar en Ajustes.
  Future<String?> checkConnection() async {
    try {
      final out = await _callLlm(
        'Buenos días.',
        from: TextLanguage.es,
        to: TextLanguage.en,
      );
      if (out == null || cleanModelOutput(out).isEmpty) {
        return 'El servidor respondió, pero el modelo «$model» no devolvió '
            'texto. Revisa el nombre del modelo.';
      }
      return null;
    } on TimeoutException {
      return 'El servidor no respondió en ${timeout.inSeconds} s.';
    } catch (e) {
      return 'No se pudo conectar con $baseUrl ($e).';
    }
  }

  /// Libera el cliente HTTP.
  void dispose() => _client.close();

  // --- Internos -------------------------------------------------------------

  Future<String?> _callLlm(
    String text, {
    required TextLanguage from,
    required TextLanguage to,
  }) async {
    final uri = useCloud
        ? Uri.parse('$_normalizedBase/v1/chat/completions')
        : Uri.parse('$_normalizedBase/api/chat');

    final headers = <String, String>{'Content-Type': 'application/json'};
    if (apiKey != null && apiKey!.isNotEmpty) {
      headers['Authorization'] = 'Bearer ${apiKey!}';
    }

    final messages = _buildMessages(text, from: from, to: to);
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
            'keep_alive': _keepAlive,
          });

    final response =
        await _client.post(uri, headers: headers, body: body).timeout(timeout);

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
    if (msg is Map && msg['content'] is String) return msg['content'] as String;
    return null;
  }

  /// Instrucción de sistema + dos ejemplos en la dirección pedida. Los ejemplos
  /// son la forma más barata de que un modelo pequeño entienda que solo debe
  /// devolver la traducción, sin preámbulos ni comentarios.
  List<Map<String, String>> _buildMessages(
    String text, {
    required TextLanguage from,
    required TextLanguage to,
  }) {
    final system = '''
Eres un traductor profesional. Traduces del ${from.englishName} al ${to.englishName}.

REGLAS ABSOLUTAS:
- Devuelve SOLO la traducción: sin comillas, sin preámbulos, sin explicaciones,
  sin notas y sin etiquetas de razonamiento.
- No añadas ni elimines información. Traduce todo el texto, entero.
- Conserva el tono y el registro del original (formal/informal).
- Respeta el formato: saltos de línea, listas y viñetas tal cual están.
- Deja intactos los nombres propios, marcas, URLs, correos, rutas de fichero,
  fragmentos de código y cifras.
- Si el texto ya está en ${to.englishName}, devuélvelo sin cambios.''';

    final shots = from == TextLanguage.es
        ? const [
            ('Buenos días, ¿podemos mover la reunión a las cuatro?',
                'Good morning, can we move the meeting to four o\'clock?'),
            ('Te paso el presupuesto revisado esta tarde.',
                'I\'ll send you the revised quote this afternoon.'),
          ]
        : const [
            ('Good morning, can we move the meeting to four o\'clock?',
                'Buenos días, ¿podemos mover la reunión a las cuatro?'),
            ('I\'ll send you the revised quote this afternoon.',
                'Te paso el presupuesto revisado esta tarde.'),
          ];

    return [
      {'role': 'system', 'content': system},
      for (final (input, output) in shots) ...[
        {'role': 'user', 'content': input},
        {'role': 'assistant', 'content': output},
      ],
      {'role': 'user', 'content': text},
    ];
  }

  /// URL base normalizada (sin barra final).
  String get _normalizedBase {
    var b = baseUrl.trim();
    while (b.endsWith('/')) {
      b = b.substring(0, b.length - 1);
    }
    return b;
  }
}
