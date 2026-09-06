import 'dart:async';
import 'dart:convert';

import 'package:http/http.dart' as http;

import 'modelos_locales.dart';

/// Hasta dónde se llegó al preguntarle a Ollama qué modelos tiene.
///
/// Existe porque hasta la 0.9 **Pronto nunca preguntaba**: el nombre del modelo
/// se escribía a mano en los Ajustes y, si ese modelo no estaba instalado, la
/// primera llamada de formateo devolvía un 404 que en pantalla parecía «el
/// Formateo no sirve». La lista sale de la máquina, no de una constante.
enum OllamaReach {
  /// Contestó y tiene al menos un modelo con el que se puede formatear.
  disponible,

  /// Contestó, pero no tiene ningún modelo que sirva para formatear.
  sinModelos,

  /// No contestó: no está instalado, no está arrancado, o la URL es otra.
  noResponde,
}

/// Un modelo instalado en el Ollama de esta máquina.
class OllamaModel {
  /// El nombre con el que Ollama lo conoce: `qwen2.5:7b`, `qwen3:1.7b`.
  final String tag;

  /// Lo que ocupa en disco. Es también la referencia de lo que va a ocupar en
  /// memoria al cargarlo: un GGUF se mapea, no se descomprime.
  final int sizeBytes;

  /// Familia (`qwen2`, `llama`), tamaño en parámetros (`7.6B`) y cuantización
  /// (`Q4_K_M`), tal y como los informa Ollama. Son suyos, no nuestros: se
  /// enseñan como vienen y no se traducen.
  final String? familia;
  final String? parametros;
  final String? cuantizacion;

  /// Lo que Ollama dice que sabe hacer: `completion`, `vision`, `tools`,
  /// `thinking`, `embedding`. **Vacío** significa «esta versión de Ollama no
  /// informa capacidades», no «no sabe hacer nada».
  final Set<String> capacidades;

  const OllamaModel({
    required this.tag,
    required this.sizeBytes,
    this.familia,
    this.parametros,
    this.cuantizacion,
    this.capacidades = const {},
  });

  /// ¿Se puede formatear con él?
  ///
  /// Un modelo de *embeddings* no genera texto: elegirlo dejaría el Formateo
  /// muerto con un error del servidor. Lo demás entra, incluido lo que no
  /// conocemos: decidir por el usuario qué modelo suyo «no vale» sería
  /// inventarnos un juicio que no hemos medido.
  bool get puedeFormatear {
    if (capacidades.isEmpty) return true;
    if (capacidades.contains('completion')) return true;
    return false;
  }

  /// Memoria que hace falta para cargarlo: lo que ocupa más un margen de 1 GB
  /// para el contexto (la caché KV), el proceso de Ollama y el sistema.
  ///
  /// No es una medida fina —depende del contexto y de si va a GPU o a CPU—,
  /// pero es la frontera que importa: por debajo de esto el modelo o no carga
  /// o se pone a paginar, y un Formateo que tarda diez segundos es peor que no
  /// tener Formateo.
  int get memoriaNecesariaBytes => sizeBytes + 1024 * 1024 * 1024;

  /// Lo que sabemos NOSOTROS de este modelo (licencia y medida), o `null` si
  /// no lo hemos medido nunca.
  ModeloMedido? get medido => modeloMedido(tag);

  /// ¿Es este el modelo que se llama [otro]?
  ///
  /// Ollama trata `qwen3` y `qwen3:latest` como el mismo modelo, y en los
  /// Ajustes se puede haber guardado cualquiera de las dos formas. Sin esto,
  /// un usuario con el modelo instalado vería «no está instalado».
  bool coincideCon(String otro) => _canonico(tag) == _canonico(otro);

  static String _canonico(String tag) {
    final t = tag.trim();
    return t.contains(':') ? t : '$t:latest';
  }

  /// Construye desde una entrada de `/api/tags`. Devuelve `null` si la entrada
  /// no trae ni nombre: una respuesta rara no puede tumbar la pantalla.
  static OllamaModel? fromJson(Object? raw) {
    if (raw is! Map) return null;
    final nombre = raw['name'] ?? raw['model'];
    if (nombre is! String || nombre.trim().isEmpty) return null;

    final detalles = raw['details'];
    String? campo(String clave) {
      if (detalles is! Map) return null;
      final v = detalles[clave];
      return v is String && v.trim().isNotEmpty ? v : null;
    }

    final caps = raw['capabilities'];
    return OllamaModel(
      tag: nombre.trim(),
      sizeBytes: raw['size'] is num ? (raw['size'] as num).toInt() : 0,
      familia: campo('family'),
      parametros: campo('parameter_size'),
      cuantizacion: campo('quantization_level'),
      capacidades: caps is List
          ? {
              for (final c in caps)
                if (c is String) c,
            }
          : const {},
    );
  }

  @override
  String toString() => 'OllamaModel($tag, ${sizeBytes}B)';
}

/// Lo que esta máquina puede usar para el Formateo local, ya filtrado y
/// ordenado, con lo que haya que contarle al usuario si algo falta.
///
/// [notice] es SIEMPRE una cadena nuestra: aquí no entra ni un trozo de la
/// respuesta del servidor.
class OllamaCatalog {
  final OllamaReach reach;

  /// Los que sirven para formatear y caben en la memoria del equipo, con el
  /// que proponemos el primero.
  final List<OllamaModel> modelos;

  /// Los que Ollama tiene pero NO caben en este equipo. Se guardan aparte para
  /// poder decir «tienes X pero no cabe» en vez de callárselo: un modelo que
  /// desaparece de la lista sin explicación parece una avería.
  final List<OllamaModel> noCaben;

  final String notice;

  const OllamaCatalog({
    required this.reach,
    this.modelos = const [],
    this.noCaben = const [],
    this.notice = '',
  });

  /// El modelo que Pronto propone, o `null` si no hay ninguno que valga.
  OllamaModel? get propuesto => modelos.isEmpty ? null : modelos.first;

  /// ¿Está instalado el modelo que dicen los Ajustes?
  bool tiene(String tag) =>
      [...modelos, ...noCaben].any((m) => m.coincideCon(tag));

  /// Ordena y filtra lo que Ollama devolvió.
  ///
  /// El orden no es una opinión sobre qué modelo es mejor: **primero los que
  /// hemos medido nosotros** (por su TER, el mejor delante) y detrás el resto
  /// por tamaño, del más ligero al más pesado. Un modelo sin medir nunca
  /// adelanta a uno medido, igual que en la lista de Motores de voz del
  /// asistente de instalación.
  ///
  /// [memoriaBytes] es la memoria física del equipo. Si es `null` no se filtra
  /// por hardware: preferimos no decir nada a decir un número que no hemos
  /// podido leer.
  factory OllamaCatalog.de(
    List<OllamaModel> crudos, {
    int? memoriaBytes,
  }) {
    final utiles = crudos.where((m) => m.puedeFormatear).toList();
    if (utiles.isEmpty) {
      return const OllamaCatalog(
        reach: OllamaReach.sinModelos,
        notice: 'Ollama responde, pero no tiene ningún modelo con el que se '
            'pueda formatear. Instala uno con «ollama pull» y vuelve a mirar.',
      );
    }

    final caben = <OllamaModel>[];
    final fuera = <OllamaModel>[];
    for (final m in utiles) {
      if (memoriaBytes != null && m.memoriaNecesariaBytes > memoriaBytes) {
        fuera.add(m);
      } else {
        caben.add(m);
      }
    }
    caben.sort(_porRecomendacion);
    fuera.sort(_porRecomendacion);

    if (caben.isEmpty) {
      return OllamaCatalog(
        reach: OllamaReach.sinModelos,
        noCaben: fuera,
        notice: 'Los modelos que tienes instalados no caben en la memoria de '
            'este equipo (${_gb(memoriaBytes!)}). Formatear con ellos dejaría '
            'el dictado esperando; usa la nube o dicta sin Formateo.',
      );
    }

    return OllamaCatalog(
      reach: OllamaReach.disponible,
      modelos: caben,
      noCaben: fuera,
    );
  }

  /// Medidos delante, y dentro de cada grupo el más ligero primero.
  static int _porRecomendacion(OllamaModel a, OllamaModel b) {
    final ma = a.medido;
    final mb = b.medido;
    if (ma != null && mb == null) return -1;
    if (ma == null && mb != null) return 1;
    if (ma != null && mb != null) {
      final c = ma.terLimpio.compareTo(mb.terLimpio);
      if (c != 0) return c;
    }
    return a.sizeBytes.compareTo(b.sizeBytes);
  }

  static String _gb(int bytes) {
    final gb = bytes / (1024 * 1024 * 1024);
    return '${gb.toStringAsFixed(1).replaceAll('.', ',')} GB';
  }
}

/// Le pregunta a Ollama qué tiene. Nunca lanza.
class OllamaClient {
  OllamaClient({http.Client? httpClient, this.timeout = _porDefecto})
      : _client = httpClient ?? http.Client();

  /// Corto a propósito: esto se consulta al abrir una pantalla. Si el servidor
  /// no está, lo que hace falta es decirlo ya, no tener la pantalla en blanco.
  static const Duration _porDefecto = Duration(seconds: 4);

  final Duration timeout;
  final http.Client _client;

  Future<OllamaCatalog> catalogo(String baseUrl, {int? memoriaBytes}) async {
    final base = _normalizar(baseUrl);
    final Uri uri;
    try {
      uri = Uri.parse('$base/api/tags');
    } catch (_) {
      return OllamaCatalog(reach: OllamaReach.noResponde, notice: _noResponde(base));
    }

    try {
      final res = await _client.get(uri).timeout(timeout);
      if (res.statusCode != 200) {
        return OllamaCatalog(
          reach: OllamaReach.noResponde,
          notice: _noResponde(base),
        );
      }
      final decoded = jsonDecode(utf8.decode(res.bodyBytes));
      if (decoded is! Map || decoded['models'] is! List) {
        return OllamaCatalog(
          reach: OllamaReach.noResponde,
          notice: 'En $base responde algo que no es Ollama. Comprueba la URL '
              'del servidor local en Ajustes.',
        );
      }
      final modelos = <OllamaModel>[];
      for (final m in decoded['models'] as List) {
        final parsed = OllamaModel.fromJson(m);
        if (parsed != null) modelos.add(parsed);
      }
      return OllamaCatalog.de(modelos, memoriaBytes: memoriaBytes);
    } catch (_) {
      // Servidor caído, DNS, tiempo agotado, JSON roto: todo es lo mismo para
      // el usuario —no hay Ollama ahí— y el error crudo no se enseña nunca.
      return OllamaCatalog(
        reach: OllamaReach.noResponde,
        notice: _noResponde(base),
      );
    }
  }

  void dispose() => _client.close();

  static String _noResponde(String base) =>
      'No hay ningún Ollama respondiendo en $base. Sin él no se puede '
      'formatear en el equipo: puedes dictar sin Formateo (se pega lo que oye '
      'el motor) o usar la nube con tu clave.';

  static String _normalizar(String url) {
    var b = url.trim();
    while (b.endsWith('/')) {
      b = b.substring(0, b.length - 1);
    }
    return b.isEmpty ? 'http://localhost:11434' : b;
  }
}
