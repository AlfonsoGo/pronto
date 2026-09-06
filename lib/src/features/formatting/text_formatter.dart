import 'dart:async';
import 'dart:convert';

import 'package:http/http.dart' as http;

import '../../core/config.dart';
import '../../core/llm_output.dart';
import 'cloud_budget.dart';
import 'cloud_cost.dart';
import 'content_drift.dart';
import 'format_preset.dart';

/// Por dónde formatea el dictado. Hoy es un ajuste global; en PRNT-3 pasa a ser
/// un campo del Modo y cada Modo elegirá el suyo.
enum FormatPath {
  /// Sin formateo: se pega lo que salió del motor. Instantáneo.
  none,

  /// Un modelo que corre en el propio equipo (Ollama). Privado y sin coste.
  local,

  /// Un modelo de nube compatible con OpenAI (OpenRouter).
  ///
  /// ⚠ Es el ÚNICO camino por el que algo sale del equipo, y solo sale el TEXTO
  /// ya transcrito: el audio no llega hasta aquí. Viene apagado, lo enciende el
  /// usuario y exige su propia clave. Sus guardias — clave en el almacén de
  /// credenciales, tope de gasto comprobado al entrar, aviso en la píldora —
  /// están en esta misma clase y en [CloudBudget].
  cloud,
}

/// Cómo acabó el formateo de un dictado.
enum FormatOutcome {
  /// El camino elegido es [FormatPath.none], o el texto venía vacío.
  disabled,

  /// El modelo formateó y su salida pasó el guardia.
  formatted,

  /// El modelo respondió, pero devolvió exactamente lo mismo.
  unchanged,

  /// No se pudo llegar al modelo (servidor caído, sin red, clave ausente o
  /// inválida, tiempo agotado). Se pega el texto sin formatear.
  unavailable,

  /// El modelo respondió, pero cambió el contenido más de lo tolerado y su
  /// salida se descartó. Se pega el texto sin formatear.
  rejected,

  /// La llamada no llegó a salir porque se habría pasado del tope de gasto, o
  /// porque el dictado era más largo de lo que se manda fuera. Se pega el texto
  /// sin formatear. NO es un fallo: es el freno haciendo su trabajo.
  overBudget,
}

/// Lo que el formateo devuelve. [text] SIEMPRE trae texto utilizable: pase lo
/// que pase, el dictado se pega. Nunca se pierde lo que el usuario dijo.
class FormatResult {
  /// El texto a pegar. Ante cualquier problema, es la entrada sin tocar.
  final String text;

  final FormatOutcome outcome;

  /// Qué contarle al usuario cuando algo no salió como esperaba. Es una cadena
  /// NUESTRA, nunca un trozo de la respuesta del modelo: el texto del modelo no
  /// habla nunca con la voz del sistema. `null` cuando no hay nada que decir.
  final String? notice;

  /// Cuánto tardó el camino completo. Lo usa el banco de pruebas para comparar
  /// «sin formateo», «local» y «nube» con números en vez de con impresiones.
  final Duration elapsed;

  /// Desviación medida de la salida del modelo. `null` si no llegó a haber
  /// salida que medir.
  final ContentDrift? drift;

  /// Lo que costó esta llamada, en dólares, si salió por la nube. `null` en el
  /// camino local y cuando no llegó a salir nada. Es el importe que se apuntó
  /// en el libro de cuentas, no una estimación aparte: el Historial y el tope
  /// de gasto cuentan lo mismo.
  final double? costUsd;

  const FormatResult({
    required this.text,
    required this.outcome,
    this.notice,
    this.elapsed = Duration.zero,
    this.drift,
    this.costUsd,
  });
}

/// Formatea el dictado con un modelo de lenguaje.
///
/// Es un paso NORMAL del pipeline, no una excepción: si el usuario ha elegido
/// un camino de formateo, se ejecuta siempre. No hay puerta de confianza — ver
/// la nota histórica en [AppConfig] sobre por qué se quitó.
///
/// Seguro por construcción:
///  - Viene apagado ([FormatPath.none] por defecto).
///  - Ante cualquier fallo devuelve la entrada intacta y dice por qué.
///  - Verifica la salida del modelo con un guardia ASIMÉTRICO: tolera que
///    borre muletillas, no que invente contenido.
class TextFormatter {
  TextFormatter({
    this.path = FormatPath.none,
    this.localBaseUrl = 'http://localhost:11434',
    this.localModel = AppConfig.defaultFormatLocalModel,
    this.cloudBaseUrl = AppConfig.openRouterBaseUrl,
    this.cloudModel = AppConfig.defaultFormatCloudModel,
    this.cloudApiKey,
    this.budget,
    this.preset = FormatPreset.limpio,
    this.timeout = AppConfig.formatTimeout,
    http.Client? httpClient,
  }) : _client = httpClient ?? http.Client();

  final FormatPath path;

  /// Qué trabajo se le pide al modelo y qué se le tolera a su salida. Los topes
  /// del guardia son SUYOS, no globales: ver [FormatPreset].
  final FormatPreset preset;
  final String localBaseUrl;
  final String localModel;
  final String cloudBaseUrl;
  final String cloudModel;

  /// De dónde se saca la clave de nube, JUSTO cuando hace falta.
  ///
  /// Es una función y no un `String` a propósito: así la clave no vive dentro
  /// de este objeto —que dura toda la sesión y aparece en cualquier volcado de
  /// providers— sino solo en la pila de la llamada que la usa. Devolver `null`
  /// significa "no hay clave" y entonces no sale ninguna petición.
  final String? Function()? cloudApiKey;

  /// El guardia del gasto. `null` = sin guardia, y entonces el camino de nube
  /// solo se puede usar desde una herramienta (el banco de pruebas), nunca
  /// desde la app: la app SIEMPRE le pone guardia.
  final CloudBudget? budget;

  final Duration timeout;

  final http.Client _client;

  /// Modelo que se usaría con el camino activo (para la interfaz y los logs).
  String get activeModel => path == FormatPath.cloud ? cloudModel : localModel;

  /// Formatea [text]. Nunca lanza: los fallos vuelven dentro de [FormatResult].
  ///
  /// [onLeavingDevice] se dispara en el instante EXACTO en que el texto va a
  /// salir del equipo, y solo entonces: no se avisa por intención, se avisa por
  /// petición. Si el tope la frena o falta la clave, no se dispara nada porque
  /// nada ha salido.
  Future<FormatResult> format(
    String text, {
    void Function()? onLeavingDevice,
  }) async {
    if (path == FormatPath.none || text.trim().isEmpty) {
      return FormatResult(text: text, outcome: FormatOutcome.disabled);
    }
    String? clave;
    if (path == FormatPath.cloud) {
      clave = cloudApiKey?.call()?.trim();
      if (clave == null || clave.isEmpty) {
        return FormatResult(
          text: text,
          outcome: FormatOutcome.unavailable,
          notice: 'Sin formatear: falta la clave del proveedor',
        );
      }

      // El tope se mira AL ENTRAR, con el coste de esta llamada ya sumado. Si
      // no cabe, no sale: un tope que se comprueba a mitad no es un tope.
      final veredicto = budget?.admit(text, model: cloudModel);
      if (veredicto != null && veredicto != BudgetVerdict.ok) {
        return FormatResult(
          text: text,
          outcome: FormatOutcome.overBudget,
          notice: veredicto.notice,
        );
      }
    }

    // Si la petición llega a salir, lo gastado se apunta pase lo que pase
    // después: el proveedor genera y cobra aunque nosotros cortemos a los 12 s
    // o no entendamos lo que contesta. Un bucle de tiempos agotados gastaría
    // dinero de verdad con el contador a cero.
    var salio = false;
    // Lo que se ACABA apuntando en el libro de cuentas. Se recuerda para que el
    // Historial enseñe el mismo importe que descontó el tope.
    double? gasto;
    void avisar() {
      salio = true;
      onLeavingDevice?.call();
    }

    final reloj = Stopwatch()..start();
    final String raw;
    try {
      final respuesta = await _call(text, clave, avisar);
      reloj.stop();

      // El dinero se apunta en cuanto el proveedor contesta, ANTES de juzgar su
      // salida: una respuesta que el guardia descarta se ha pagado igual.
      if (_useCloud && respuesta.costUsd != null) {
        gasto = respuesta.costUsd;
        await budget?.record(respuesta.costUsd!);
      }
      if (respuesta.error != null) {
        // Un 200 que no se entiende YA se ha cobrado; un 4xx/5xx no.
        if (_useCloud && respuesta.cobrada) {
          final estimado =
              estimateCallCostUsd(model: cloudModel, textChars: text.length);
          gasto = estimado;
          await budget?.record(estimado);
        }
        return FormatResult(
          text: text,
          outcome: FormatOutcome.unavailable,
          notice: respuesta.error,
          elapsed: reloj.elapsed,
      costUsd: gasto,
        );
      }
      raw = respuesta.content!;
    } catch (e) {
      // Timeout, red caída, DNS, JSON roto: degradación segura. El motivo se
      // clasifica aquí, con cadenas nuestras; nunca se enseña el error crudo.
      reloj.stop();
      if (_useCloud && salio) {
        final estimado =
            estimateCallCostUsd(model: cloudModel, textChars: text.length);
        gasto = estimado;
        await budget?.record(estimado);
      }
      return FormatResult(
        text: text,
        outcome: FormatOutcome.unavailable,
        notice: _motivoDe(e),
        elapsed: reloj.elapsed,
      costUsd: gasto,
      );
    }

    final limpio = sanitizeForInjection(cleanModelOutput(raw));
    if (limpio.isEmpty) {
      return FormatResult(
        text: text,
        outcome: FormatOutcome.unavailable,
        notice: 'Sin formatear: el modelo devolvió una respuesta vacía',
        elapsed: reloj.elapsed,
      costUsd: gasto,
      );
    }

    // Las cifras se comprueban EXACTAS y antes que la proporción: cambiar un
    // importe o una hora es el peor fallo posible del formateo y el guardia
    // proporcional no lo ve (una cifra en un dictado largo es una sustitución).
    if (!digitsPreserved(text, limpio)) {
      return FormatResult(
        text: text,
        outcome: FormatOutcome.rejected,
        notice: 'Sin formatear: el modelo cambió una cifra que dictaste',
        elapsed: reloj.elapsed,
      costUsd: gasto,
        drift: measureContentDrift(text, limpio),
      );
    }

    final drift = measureContentDrift(text, limpio);
    if (drift.added > preset.maxAddedContent) {
      return FormatResult(
        text: text,
        outcome: FormatOutcome.rejected,
        notice: 'Sin formatear: el modelo añadió texto que no dictaste',
        elapsed: reloj.elapsed,
      costUsd: gasto,
        drift: drift,
      );
    }
    if (drift.deleted > preset.maxDeletedContent) {
      return FormatResult(
        text: text,
        outcome: FormatOutcome.rejected,
        notice: 'Sin formatear: el modelo se dejó parte de lo que dictaste',
        elapsed: reloj.elapsed,
      costUsd: gasto,
        drift: drift,
      );
    }

    return FormatResult(
      text: limpio,
      outcome:
          limpio == text ? FormatOutcome.unchanged : FormatOutcome.formatted,
      elapsed: reloj.elapsed,
      costUsd: gasto,
      drift: drift,
    );
  }

  void dispose() => _client.close();

  // --- Internos ---------------------------------------------------------------

  bool get _useCloud => path == FormatPath.cloud;

  /// Llama al modelo. Devuelve el contenido, o un motivo NUESTRO si el servidor
  /// contestó algo que no sirve.
  ///
  /// [clave] llega por parámetro y no se guarda en ningún sitio. [avisar] se
  /// dispara justo antes del envío a la nube.
  Future<_Respuesta> _call(
    String text,
    String? clave,
    void Function()? avisar,
  ) async {
    final base = _normalizar(_useCloud ? cloudBaseUrl : localBaseUrl);
    final uri = Uri.parse(
      _useCloud ? '$base/v1/chat/completions' : '$base/api/chat',
    );
    final model = _useCloud ? cloudModel : localModel;

    final headers = <String, String>{'Content-Type': 'application/json'};
    if (_useCloud) headers['Authorization'] = 'Bearer $clave';

    final mensajes = [
      {'role': 'system', 'content': preset.prompt},
      for (final ejemplo in preset.fewShots) ...[
        {'role': 'user', 'content': ejemplo['user']!},
        {'role': 'assistant', 'content': ejemplo['assistant']!},
      ],
      {'role': 'user', 'content': text},
    ];

    final body = jsonEncode(
      _useCloud
          ? {
              'model': model,
              'messages': mensajes,
              'temperature': 0,
              'stream': false,
              // Solo proveedores que NO guardan ni entrenan con lo que se les
              // manda. OpenRouter enruta por defecto a los que sí pueden
              // hacerlo, así que esto no es un adorno: sin esta línea, el texto
              // dictado puede acabar en el entrenamiento de un tercero.
              // Verificado en openrouter.ai/docs/features/provider-routing
              // (2026-09-04).
              'provider': {'data_collection': 'deny'},
            }
          : {
              'model': model,
              'messages': mensajes,
              'stream': false,
              // En Ollama nativo la temperatura va dentro de `options`.
              'options': {'temperature': 0},
            },
    );

    // El último punto en el que el texto sigue dentro del equipo. También
    // avisa con el camino LOCAL si el servidor no está en esta máquina: un
    // `llmBaseUrl` apuntando a otro sitio saca el dictado igual, y decir que
    // solo la nube sale sería mentira.
    if (_useCloud || !_esDeEstaMaquina(uri)) avisar?.call();

    final res =
        await _client.post(uri, headers: headers, body: body).timeout(timeout);

    if (res.statusCode == 401 || res.statusCode == 403) {
      return const _Respuesta.error(
        'Sin formatear: el proveedor rechazó la clave',
      );
    }
    if (res.statusCode == 404) {
      // El 404 que dio nombre al PRNT-18. Desde la 0.9 la lista de modelos sale
      // de preguntarle a Ollama qué tiene, así que llegar aquí significa que el
      // modelo se borró DESPUÉS de elegirlo — y aun así se dice con todas las
      // letras, con dónde se arregla, en vez de dejar «el Formateo no sirve».
      return _Respuesta.error(
        _useCloud
            ? 'Sin formatear: el proveedor no conoce el modelo $model'
            : 'Sin formatear: $model ya no está en Ollama. Elige otro en '
                'Ajustes, de los que sí tienes',
      );
    }
    if (res.statusCode != 200) {
      return _Respuesta.error(
        'Sin formatear: el servidor respondió ${res.statusCode}',
      );
    }

    final decoded = jsonDecode(utf8.decode(res.bodyBytes));
    if (decoded is! Map) {
      return const _Respuesta.error(
        'Sin formatear: respuesta del servidor no reconocida',
        cobrada: true,
      );
    }

    if (_useCloud) {
      // Formato OpenAI: { choices: [ { message: { content } } ] }
      final choices = decoded['choices'];
      if (choices is List && choices.isNotEmpty) {
        final msg = choices.first;
        if (msg is Map && msg['message'] is Map) {
          final content = (msg['message'] as Map)['content'];
          if (content is String) {
            return _Respuesta.ok(content, costUsd: _costeDe(decoded, text));
          }
        }
      }
    } else {
      // Formato Ollama nativo: { message: { role, content } }
      final msg = decoded['message'];
      if (msg is Map && msg['content'] is String) {
        return _Respuesta.ok(msg['content'] as String);
      }
    }
    return const _Respuesta.error(
      'Sin formatear: respuesta del servidor no reconocida',
      cobrada: true,
    );
  }

  /// Lo que costó la llamada, en dólares.
  ///
  /// OpenRouter devuelve el coste REAL en `usage.cost` (créditos = dólares), y
  /// ese manda. Si por lo que sea no viene, se apunta la ESTIMACIÓN en vez de
  /// un cero: una llamada que se hizo y se apunta como gratis es un contador
  /// que miente y un tope que nunca salta.
  double _costeDe(Map decoded, String text) {
    final usage = decoded['usage'];
    if (usage is Map) {
      final cost = usage['cost'];
      // Estrictamente MAYOR que cero: un proveedor que informe siempre 0
      // congelaría el tope para siempre. Si dice que fue gratis, se apunta la
      // estimación igual.
      if (cost is num && cost.isFinite && cost > 0) return cost.toDouble();
    }
    return estimateCallCostUsd(model: cloudModel, textChars: text.length);
  }

  /// Traduce la excepción a un motivo que se pueda enseñar. Nunca se muestra el
  /// mensaje crudo de la excepción: puede traer rutas, cabeceras o la clave.
  String _motivoDe(Object e) {
    if (e is TimeoutException) {
      return 'Sin formatear: se agotó el tiempo de espera';
    }
    if (_useCloud) return 'Sin formatear: no hay conexión con el proveedor';
    return 'Sin formatear: el servidor local no responde';
  }

  /// ¿El servidor está en esta misma máquina? Solo entonces el texto no sale.
  static bool _esDeEstaMaquina(Uri uri) {
    final h = uri.host.toLowerCase();
    return h == 'localhost' ||
        h == '127.0.0.1' ||
        h == '::1' ||
        h == '[::1]' ||
        h.isEmpty;
  }

  String _normalizar(String url) {
    var b = url.trim();
    while (b.endsWith('/')) {
      b = b.substring(0, b.length - 1);
    }
    return b;
  }
}

/// Respuesta del servidor ya clasificada: o contenido, o un motivo nuestro.
class _Respuesta {
  final String? content;
  final String? error;

  /// Lo que costó, en dólares. `null` en el camino local y cuando el servidor
  /// no llegó a procesar nada (un 401 no se cobra).
  final double? costUsd;

  /// El proveedor llegó a generar (respondió 200) aunque su respuesta no nos
  /// sirviera: eso se paga, y el tope tiene que enterarse.
  final bool cobrada;

  const _Respuesta.ok(String this.content, {this.costUsd})
      : error = null,
        cobrada = true;
  const _Respuesta.error(String this.error, {this.cobrada = false})
      : content = null,
        costUsd = null;
}
