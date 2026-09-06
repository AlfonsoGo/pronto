// Banco de pruebas del formateo: mide los tres caminos con NÚMEROS.
//
// Contesta las dos preguntas que el ticket PRNT-1 no permite suponer:
//   1. ¿Cuánto mejora el texto cada camino? (calidad, contra una referencia)
//   2. ¿Cuánto retrasa el pegado?          (latencia, que es lo que molesta)
//
// La calidad se mide como TER (tasa de error por tokens) contra un texto de
// referencia que el usuario dio por bueno, contando mayúsculas y puntuación:
// para formatear, un punto que falta ES un error. El TER del texto SIN formatear
// es la línea base — si un camino no baja de ahí, no está haciendo nada.
//
// Uso (desde la raíz del repo):
//   dart run tools/formateo_bench.dart --corpus=<ruta.jsonl> [opciones]
//
// Opciones:
//   --corpus=<ruta>   JSONL con un objeto por línea: {"asr": ..., "gold": ...}
//   --n=<entero>      Cuántos dictados usar (por defecto: todos)
//   --local=<modelo>  Modelo de Ollama a medir (repetible)
//   --nube=<modelo>   Modelo de OpenRouter a medir (repetible)
//   --url=<url>       Raíz de Ollama (por defecto http://localhost:11434)
//   --tope=<usd>      Tope de gasto de la corrida. Aborta ANTES de pasarse
//   --ejemplos=<n>    Enseña n dictados con su antes/después y la referencia.
//                     El número dice CUÁNTO mejora; esto dice EN QUÉ. Un TER
//                     que baja no demuestra que el texto salga bien: hay que
//                     mirarlo. OJO: saca dictados reales por pantalla.
//
// La clave de OpenRouter se lee de la variable de entorno OPENROUTER_API_KEY y
// NO se escribe en ningún sitio: ni en la salida, ni en un fichero, ni en los
// ajustes de la app.
//
// El corpus NO va en este repo: son dictados reales. Vive fuera y se pasa por
// --corpus.

import 'dart:convert';
import 'dart:io';

import 'package:pronto/src/core/text_polish.dart';
import 'package:pronto/src/features/formatting/cloud_cost.dart';
import 'package:pronto/src/features/formatting/text_formatter.dart';

void main(List<String> args) async {
  final opciones = _Opciones.parse(args);
  if (opciones == null) {
    stderr.writeln(
      'Uso: dart run tools/formateo_bench.dart --corpus=<ruta.jsonl> '
      '[--n=50] [--local=qwen2.5:7b] [--nube=openai/gpt-5-nano] [--tope=0.10]',
    );
    exitCode = 64;
    return;
  }

  final corpus = _leerCorpus(opciones.corpus, opciones.n);
  if (corpus.isEmpty) {
    stderr.writeln('Corpus vacío o ilegible: ${opciones.corpus}');
    exitCode = 66;
    return;
  }
  stdout.writeln('Corpus: ${corpus.length} dictados · ${opciones.corpus}\n');

  final claveNube = Platform.environment['OPENROUTER_API_KEY'];
  if (opciones.nube.isNotEmpty && (claveNube == null || claveNube.isEmpty)) {
    stderr.writeln(
      'Pediste medir la nube pero OPENROUTER_API_KEY no está en el entorno. '
      'Se miden solo los caminos locales.',
    );
  }

  // Cada fila se imprime EN CUANTO se mide, no solo en la tabla del final.
  // Una corrida de tres modelos son horas de GPU, y si el proceso se cae en el
  // segundo —pasó: un tiempo agotado en un dictado largo— la tabla no llega a
  // imprimirse y se pierde también lo que YA estaba medido.
  final filas = <_Fila>[];
  void anotar(_Fila f) {
    filas.add(f);
    stdout.writeln(
      'FILA · ${f.etiqueta} · TER ${f.ter.toStringAsFixed(4)} · '
      'p50 ${f.p50ms} ms · p95 ${f.p95ms} ms · '
      'ok/rech/caido ${f.formateados}/${f.rechazados}/${f.caidos} · '
      'n=${f.total}',
    );
  }

  // Escalón 1: el crudo del reconocedor, sin tocar nada. Es el suelo.
  anotar(_medirCrudo(corpus));

  // Escalón 2: lo que Pronto pega HOY con el formateo apagado — el pulido
  // determinista ya corre siempre. Es la línea base honesta: el LLM tiene que
  // mejorar ESTO, no el crudo, o no está aportando nada que no fuera gratis.
  anotar(_medirPulido(corpus));

  for (final modelo in opciones.local) {
    anotar(
      await _medir(
        etiqueta: 'local · $modelo',
        corpus: corpus,
        muestras: opciones.ejemplos,
        formatter: TextFormatter(
          path: FormatPath.local,
          localBaseUrl: opciones.url,
          localModel: modelo,
          timeout: const Duration(seconds: 120),
        ),
      ),
    );
  }

  if (claveNube != null && claveNube.isNotEmpty) {
    for (final modelo in opciones.nube) {
      // El tope se comprueba ANTES de empezar, no a mitad: una corrida que se
      // corta por la mitad ya se ha gastado el dinero.
      final estimado = _estimarCoste(corpus, modelo);
      if (estimado > opciones.tope) {
        stderr.writeln(
          'ABORTADO: $modelo costaría ~\$${estimado.toStringAsFixed(4)} y el '
          'tope de la corrida es \$${opciones.tope.toStringAsFixed(4)}. '
          'Sube --tope o baja --n.',
        );
        continue;
      }
      stdout.writeln(
        '· $modelo: coste estimado \$${estimado.toStringAsFixed(4)}',
      );
      anotar(
        await _medir(
          etiqueta: 'nube · $modelo',
          corpus: corpus,
          muestras: opciones.ejemplos,
          formatter: TextFormatter(
            path: FormatPath.cloud,
            cloudModel: modelo,
            // La clave llega por función: el formateador la pide justo al
            // mandar la petición y no la guarda en ningún sitio.
            cloudApiKey: () => claveNube,
            timeout: const Duration(seconds: 60),
          ),
        ),
      );
    }
  }

  _imprimirTabla(filas);
}

// ─── Medición ────────────────────────────────────────────────────────────────

class _Caso {
  final String asr;
  final String gold;
  const _Caso(this.asr, this.gold);
}

class _Fila {
  final String etiqueta;
  final double ter;
  final int p50ms;
  final int p95ms;
  final int formateados;
  final int rechazados;
  final int caidos;
  final int total;

  const _Fila({
    required this.etiqueta,
    required this.ter,
    required this.p50ms,
    required this.p95ms,
    required this.formateados,
    required this.rechazados,
    required this.caidos,
    required this.total,
  });
}

_Fila _medirCrudo(List<_Caso> corpus) => _Fila(
      etiqueta: 'crudo del reconocedor',
      ter: _media([for (final c in corpus) _ter(c.gold, c.asr)]),
      p50ms: 0,
      p95ms: 0,
      formateados: 0,
      rechazados: 0,
      caidos: 0,
      total: corpus.length,
    );

_Fila _medirPulido(List<_Caso> corpus) => _Fila(
      etiqueta: 'sin formateo (LINEA BASE)',
      ter: _media([
        for (final c in corpus) _ter(c.gold, TextPolish.apply(c.asr)),
      ]),
      p50ms: 0,
      p95ms: 0,
      formateados: 0,
      rechazados: 0,
      caidos: 0,
      total: corpus.length,
    );

Future<_Fila> _medir({
  required String etiqueta,
  required List<_Caso> corpus,
  required TextFormatter formatter,
  int muestras = 0,
}) async {
  final errores = <double>[];
  final latencias = <int>[];
  var formateados = 0, rechazados = 0, caidos = 0;

  stdout.write('$etiqueta ');
  final ejemplos = <(String, String, String)>[];
  for (var i = 0; i < corpus.length; i++) {
    final c = corpus[i];
    // Mismo orden que el pipeline real: pulido determinista y luego el modelo.
    final entrada = TextPolish.apply(c.asr);
    final r = await formatter.format(entrada);
    errores.add(_ter(c.gold, r.text));
    if (ejemplos.length < muestras) ejemplos.add((entrada, r.text, c.gold));
    latencias.add(r.elapsed.inMilliseconds);
    switch (r.outcome) {
      case FormatOutcome.formatted:
      case FormatOutcome.unchanged:
        formateados++;
      case FormatOutcome.rejected:
        rechazados++;
      case FormatOutcome.unavailable:
      // El banco corre SIN guardia de gasto (su tope es --tope, comprobado
      // antes de empezar), así que aquí no debería caer nunca; pero el switch
      // es exhaustivo y hay que nombrarlo.
      case FormatOutcome.overBudget:
        caidos++;
        if (caidos == 1) stdout.write('\n  ⚠ ${r.notice}\n  ');
      case FormatOutcome.disabled:
        break;
    }
    if (i % 10 == 0) stdout.write('.');
  }
  stdout.writeln(' hecho');
  formatter.dispose();

  for (var i = 0; i < ejemplos.length; i++) {
    final (entrada, salida, gold) = ejemplos[i];
    stdout.writeln('');
    stdout.writeln('  -- ejemplo ${i + 1} --');
    stdout.writeln('  ANTES     : $entrada');
    stdout.writeln('  DESPUES   : $salida');
    stdout.writeln('  REFERENCIA: $gold');
  }

  latencias.sort();
  int pct(double p) => latencias.isEmpty
      ? 0
      : latencias[
          (latencias.length * p).floor().clamp(0, latencias.length - 1)];

  return _Fila(
    etiqueta: etiqueta,
    ter: _media(errores),
    p50ms: pct(0.50),
    p95ms: pct(0.95),
    formateados: formateados,
    rechazados: rechazados,
    caidos: caidos,
    total: corpus.length,
  );
}

// ─── Métrica ─────────────────────────────────────────────────────────────────

/// Tokeniza contando la puntuación como token propio y RESPETANDO mayúsculas:
/// para el formateo, «que» y «qué» son tokens distintos, y un punto que falta
/// es un error igual que una palabra que falta.
final RegExp _token = RegExp(r'[\wáéíóúüñÁÉÍÓÚÜÑ]+|[^\s\w]', unicode: true);

List<String> _tokenizar(String s) =>
    _token.allMatches(s).map((m) => m.group(0)!).toList();

/// Tasa de error por tokens: distancia de edición entre la referencia y el
/// candidato, dividida entre la longitud de la referencia.
double _ter(String referencia, String candidato) {
  final a = _tokenizar(referencia);
  final b = _tokenizar(candidato);
  if (a.isEmpty) return b.isEmpty ? 0 : 1;

  var previa = List<int>.generate(b.length + 1, (j) => j);
  for (var i = 1; i <= a.length; i++) {
    final actual = List<int>.filled(b.length + 1, 0);
    actual[0] = i;
    for (var j = 1; j <= b.length; j++) {
      final coste = a[i - 1] == b[j - 1] ? 0 : 1;
      final diag = previa[j - 1] + coste;
      final arriba = previa[j] + 1;
      final izq = actual[j - 1] + 1;
      actual[j] = diag < arriba
          ? (diag < izq ? diag : izq)
          : (arriba < izq ? arriba : izq);
    }
    previa = actual;
  }
  return previa[b.length] / a.length;
}

double _media(List<double> xs) =>
    xs.isEmpty ? 0 : xs.reduce((a, b) => a + b) / xs.length;

// ─── Coste ───────────────────────────────────────────────────────────────────

/// Estimación de lo que costaría la corrida entera. Usa la MISMA tabla de
/// precios y la misma cuenta que el tope de gasto de la app
/// (`cloud_cost.dart`): dos tablas de precios acaban siendo dos precios
/// distintos, y el que se queda viejo es siempre el que frena el dinero.
double _estimarCoste(List<_Caso> corpus, String modelo) {
  var total = 0.0;
  for (final c in corpus) {
    total += estimateCallCostUsd(model: modelo, textChars: c.asr.length);
  }
  return total;
}

// ─── Entrada / salida ────────────────────────────────────────────────────────

List<_Caso> _leerCorpus(String ruta, int? n) {
  final f = File(ruta);
  if (!f.existsSync()) return const [];
  final casos = <_Caso>[];
  for (final linea in f.readAsLinesSync()) {
    if (linea.trim().isEmpty) continue;
    final j = jsonDecode(linea);
    if (j is! Map) continue;
    final asr = j['asr'] as String?;
    final gold = j['gold'] as String?;
    if (asr == null || gold == null) continue;
    casos.add(_Caso(asr, gold));
    if (n != null && casos.length >= n) break;
  }
  return casos;
}

void _imprimirTabla(List<_Fila> filas) {
  final base = filas
      .firstWhere(
        (f) => f.etiqueta.contains('LINEA BASE'),
        orElse: () => filas.first,
      )
      .ter;
  stdout.writeln('\n${'─' * 88}');
  stdout.writeln(
    '${'camino'.padRight(32)}${'TER'.padLeft(8)}${'vs base'.padLeft(10)}'
    '${'p50'.padLeft(9)}${'p95'.padLeft(9)}${'ok/rech/caído'.padLeft(18)}',
  );
  stdout.writeln('─' * 88);
  for (final f in filas) {
    final delta = base == 0 ? 0.0 : (f.ter - base) / base * 100;
    stdout.writeln(
      '${f.etiqueta.padRight(32)}'
      '${f.ter.toStringAsFixed(4).padLeft(8)}'
      '${(f.etiqueta.contains('LINEA BASE') ? '—' : '${delta >= 0 ? '+' : ''}${delta.toStringAsFixed(1)} %').padLeft(10)}'
      '${'${f.p50ms} ms'.padLeft(9)}'
      '${'${f.p95ms} ms'.padLeft(9)}'
      '${'${f.formateados}/${f.rechazados}/${f.caidos}'.padLeft(18)}',
    );
  }
  stdout.writeln('─' * 88);
  stdout.writeln(
    'TER = tasa de error por tokens contra la referencia (menos es mejor).\n'
    'La LINEA BASE es lo que Pronto pega hoy con el formateo apagado,'
    ' pulido determinista incluido.\n'
    'Un camino que no baja de ahi no aporta nada.',
  );
}

class _Opciones {
  final String corpus;
  final int? n;
  final List<String> local;
  final List<String> nube;
  final String url;
  final double tope;
  final int ejemplos;

  const _Opciones({
    required this.corpus,
    required this.n,
    required this.local,
    required this.nube,
    required this.url,
    required this.tope,
    required this.ejemplos,
  });

  static _Opciones? parse(List<String> args) {
    String? corpus;
    int? n;
    final local = <String>[];
    final nube = <String>[];
    var url = 'http://localhost:11434';
    var tope = 0.10;
    var ejemplos = 0;

    for (final a in args) {
      final i = a.indexOf('=');
      if (!a.startsWith('--') || i < 0) continue;
      final clave = a.substring(2, i);
      final valor = a.substring(i + 1);
      switch (clave) {
        case 'corpus':
          corpus = valor;
        case 'n':
          n = int.tryParse(valor);
        case 'local':
          local.add(valor);
        case 'nube':
          nube.add(valor);
        case 'url':
          url = valor;
        case 'tope':
          tope = double.tryParse(valor) ?? tope;
        case 'ejemplos':
          ejemplos = int.tryParse(valor) ?? 0;
      }
    }
    if (corpus == null) return null;
    return _Opciones(
      corpus: corpus,
      n: n,
      local: local,
      nube: nube,
      url: url,
      tope: tope,
      ejemplos: ejemplos,
    );
  }
}
