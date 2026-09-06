/// Lo que hemos MEDIDO nosotros de un modelo local, y bajo qué licencia está.
///
/// Esta tabla existe para no repetir el error de FluidVoice, que pinta barras de
/// «Speed 100 % / Acc 96 %» sacadas de una lista escrita a mano en su código:
/// sin corpus, sin métrica y sin fecha. Aquí una fila **solo se escribe después
/// de correr el banco** (`tools/formateo_bench.dart`) contra el corpus real en
/// español, y trae el número de dictados y el día en que se midió.
///
/// La licencia se comprueba en la ficha del modelo **el día que se elige**, no
/// de memoria ni de un blog: Pronto es MIT y público, y proponerle a alguien un
/// modelo de licencia restrictiva es un problema nuestro, no suyo.
library;

class ModeloMedido {
  /// El tag de Ollama tal cual: `qwen3:1.7b`.
  final String tag;

  /// Licencia del modelo, tal y como la declara su ficha.
  final String licencia;

  /// La ficha donde se leyó, y el día en que se leyó (`AAAA-MM-DD`).
  final String fichaUrl;
  final String licenciaVerificadaEl;

  /// TER de la salida formateada contra la referencia, sobre el corpus real en
  /// español. **Menor es mejor.** El TER del texto sin formatear es la línea
  /// base: un modelo que no baja de ahí no está haciendo nada.
  final double terLimpio;

  /// TER de ese mismo corpus SIN formatear, para que el número de arriba se
  /// pueda leer sin abrir el informe.
  final double terSinFormatear;

  /// Mediana de lo que tarda una llamada de formateo, en segundos, en el
  /// equipo donde se midió. La mediana y no la media: una sola llamada lenta
  /// (la primera, que carga el modelo) desplaza la media y no representa nada.
  final double segundosMediana;

  /// Cuántos dictados y qué día. Sin las dos cosas el número no vale.
  final int dictados;
  final String medidoEl;

  /// Dónde se midió. Un tiempo sin la máquina que lo produjo no dice nada.
  final String equipo;

  const ModeloMedido({
    required this.tag,
    required this.licencia,
    required this.fichaUrl,
    required this.licenciaVerificadaEl,
    required this.terLimpio,
    required this.terSinFormatear,
    required this.segundosMediana,
    required this.dictados,
    required this.medidoEl,
    required this.equipo,
  });

  /// ¿Mejoró de verdad el texto respecto a no formatear?
  ///
  /// Si no, no se propone: un asistente que no baja el TER es espera y calor a
  /// cambio de nada, y decirlo es más barato que dejar que lo descubra el
  /// usuario dictado a dictado.
  bool get mejora => terLimpio < terSinFormatear;

  /// Cuánto baja el error, en porcentaje relativo.
  double get mejoraRelativa =>
      (terSinFormatear - terLimpio) / terSinFormatear * 100;
}

/// Los modelos locales que hemos medido, con su licencia comprobada.
///
/// Se llena corriendo el banco. **No se añade una fila a mano.**
///
/// ⚠ **Lo que este número NO dice.** El corpus con el que se mide sale de la
/// base de Wispr Flow, y su reconocedor **ya puntúa y acentúa bastante**: por
/// eso la línea base es tan buena (TER 0,1682) y la mejora sale pequeña. El
/// Motor de voz de Pronto es Parakeet, cuya salida viene mucho más cruda, así
/// que **este −2 % es un suelo, no una predicción**. El corpus de Parakeet lo
/// dará el Historial (PRNT-16) en cuanto esa versión lleve un tiempo corriendo:
/// a día de hoy `dictation_log` está vacío porque no está desplegada.
const List<ModeloMedido> kModelosMedidos = [
  ModeloMedido(
    tag: 'qwen2.5:7b',
    licencia: 'apache-2.0',
    fichaUrl: 'https://huggingface.co/Qwen/Qwen2.5-7B-Instruct',
    licenciaVerificadaEl: '2026-09-06',
    terLimpio: 0.1649,
    terSinFormatear: 0.1682,
    segundosMediana: 0.499,
    dictados: 100,
    medidoEl: '2026-09-06',
    equipo: 'un PC con RTX 4070 Ti (12 GB) y Windows 11',
  ),
];

/// Lo que sabemos de [tag], o `null` si no lo hemos medido nunca.
ModeloMedido? modeloMedido(String tag) {
  final buscado = tag.contains(':') ? tag.trim() : '${tag.trim()}:latest';
  for (final m in kModelosMedidos) {
    final suyo = m.tag.contains(':') ? m.tag : '${m.tag}:latest';
    if (suyo == buscado) return m;
  }
  return null;
}
