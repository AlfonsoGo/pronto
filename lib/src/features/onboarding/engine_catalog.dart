import '../settings/settings_controller.dart';

/// Lo que Pronto sabe —y lo que NO sabe— de un Motor de voz.
///
/// Todo lo que hay aquí o es un hecho medible en este equipo (el tamaño de los
/// ficheros) o sale de la ficha publicada del modelo, **con su fuente y la
/// fecha en que se consultó**. Lo que no está publicado se queda en `null` y la
/// pantalla lo dice: un hueco vacío es información, un número inventado no
/// (regla de oro #5).
///
/// ⚠ Los dos huecos que hay hoy no son un olvido: no encontramos una cifra
/// comparable de Whisper small sobre el MISMO banco (FLEURS español) el día que
/// se escribió esto. Rellenarlos exige mirar la ficha, no estimarlos.
class SpeechEngineInfo {
  /// El motor tal y como lo nombra [AppSettings.engine].
  final SpeechEngine engine;

  /// Cómo se llama de cara al usuario.
  final String nombre;

  /// Qué idiomas dice cubrir su ficha.
  final String idiomas;

  /// Cuánto ocupa en disco, en bytes. Es la suma REAL de sus ficheros.
  final int bytes;

  /// Una línea de por qué querrías este motor y no otro.
  final String nota;

  /// WER publicado sobre FLEURS en español, en tanto por uno (0,0345 = 3,45 %).
  ///
  /// **No se enseña como número en ningún sitio.** Solo ordena la lista cuando
  /// el usuario pide precisión. Que exista aquí no lo convierte en una medida
  /// nuestra: es la de quien publicó el modelo, y por eso lleva [fuente].
  final double? werFleursEs;

  /// De dónde salen [werFleursEs] y [idiomas].
  final String? fuente;

  /// Cuándo se consultó [fuente] (AAAA-MM-DD). Sin fecha, un dato sacado de una
  /// ficha que su autor puede cambiar cuando quiera es una afirmación sin
  /// respaldo.
  final String? fuenteConsultada;

  /// ¿Puede Pronto usar este motor HOY, tal y como se distribuye?
  ///
  /// Un motor que se puede elegir pero que el pipeline nunca instancia es un
  /// control inerte. Si esto es `false`, [porQueNoSePuedeUsar] dice por qué y
  /// el motor no se ofrece como elegible.
  final bool seleccionable;

  /// El motivo, en cristiano, de que [seleccionable] sea `false`.
  final String? porQueNoSePuedeUsar;

  const SpeechEngineInfo({
    required this.engine,
    required this.nombre,
    required this.idiomas,
    required this.bytes,
    required this.nota,
    this.werFleursEs,
    this.fuente,
    this.fuenteConsultada,
    this.seleccionable = true,
    this.porQueNoSePuedeUsar,
  });

  /// El tamaño en la unidad que se lee de un vistazo («639 MB», «1,2 GB»).
  String get tamano {
    const mb = 1024 * 1024;
    if (bytes >= 1024 * mb) {
      final gb = bytes / (1024 * mb);
      return '${gb.toStringAsFixed(1).replaceAll('.', ',')} GB';
    }
    return '${(bytes / mb).round()} MB';
  }
}

/// Los Motores de voz que Pronto conoce.
///
/// 🔴 **Hoy solo uno es usable.** `whisperEngineProvider` instancia siempre
/// [ParakeetEngine], y desde la 0.7.1 el instalador no lleva `whisper.dll`, así
/// que elegir Whisper no cambiaría nada. Está en la lista porque la app sabe
/// descargarlo y porque callarlo haría pensar que nunca existió, pero se marca
/// `seleccionable: false` para que ninguna pantalla ofrezca activarlo.
class EngineCatalog {
  EngineCatalog._();

  /// Ficha del Parakeet que Pronto trae de fábrica.
  ///
  /// Tamaño: suma exacta de los cuatro ficheros int8 de `native/parakeet/`
  /// (encoder 652.184.281 + decoder 11.845.275 + joiner 6.355.277 +
  /// tokens 93.939), los mismos que descarga [ParakeetModelDownloader].
  static const parakeet = SpeechEngineInfo(
    engine: SpeechEngine.parakeet,
    nombre: 'Parakeet TDT 0.6b v3',
    idiomas: '25 idiomas europeos, español incluido',
    bytes: 670478772,
    nota: 'El que Pronto trae de fábrica. Reconoce en tu equipo, sin nube.',
    werFleursEs: 0.0345,
    fuente: 'https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3',
    fuenteConsultada: '2026-09-06',
  );

  /// Ficha de Whisper small. No es elegible: ver la nota de la clase.
  ///
  /// El tamaño es el de `ggml-small.bin` publicado en el mirror de whisper.cpp,
  /// no uno medido aquí — este fichero no viaja en el instalador.
  static const whisperSmall = SpeechEngineInfo(
    engine: SpeechEngine.whisper,
    nombre: 'Whisper small',
    idiomas: '99 idiomas',
    bytes: 487601967,
    nota: 'Cubre muchos más idiomas, pero es más lento en CPU.',
    werFleursEs: null,
    fuente: 'https://huggingface.co/ggerganov/whisper.cpp',
    fuenteConsultada: '2026-09-06',
    seleccionable: false,
    porQueNoSePuedeUsar:
        'Pronto no incluye la biblioteca de Whisper desde la versión 0.7.1: '
        'aunque descargues el modelo, el dictado seguiría usando Parakeet.',
  );

  /// Todos los motores conocidos, elegibles o no.
  static const List<SpeechEngineInfo> todos = [parakeet, whisperSmall];

  /// Los que Pronto puede usar de verdad hoy.
  static List<SpeechEngineInfo> get elegibles =>
      todos.where((e) => e.seleccionable).toList(growable: false);

  /// La ficha de [engine].
  static SpeechEngineInfo of(SpeechEngine engine) =>
      todos.firstWhere((e) => e.engine == engine);

  /// Ordena [motores] según lo que le importe al usuario.
  ///
  /// [preferencia] va de 0 (me da igual la precisión, quiero rapidez) a 1 (me
  /// da igual la espera, quiero que acierte). Hoy la ÚNICA cifra publicada y
  /// comparable que tenemos es el WER, así que:
  ///
  ///  - un motor **sin** WER publicado nunca adelanta a uno que sí lo tiene,
  ///    por mucho que se mueva el mando: no sabemos si es mejor o peor, y
  ///    colocarlo por delante sería afirmar algo que no hemos comprobado;
  ///  - entre dos sin dato, se mantiene el orden en que venían (estable), que
  ///    es el del catálogo y no una opinión disfrazada de ranking.
  ///
  /// El mando, por tanto, hoy casi no mueve nada. Es a propósito: en cuanto
  /// haya una segunda cifra medida sobre el mismo banco, esta función ya la
  /// usa sin tocarla.
  static List<SpeechEngineInfo> ordenar(
    List<SpeechEngineInfo> motores, {
    required double preferencia,
  }) {
    final p = preferencia.clamp(0.0, 1.0);
    final indice = {
      for (var i = 0; i < motores.length; i++) motores[i].engine: i,
    };

    final ordenados = [...motores];
    ordenados.sort((a, b) {
      final wa = a.werFleursEs;
      final wb = b.werFleursEs;

      // Sin dato comparable no se compite: el que no tiene cifra va detrás.
      if (wa == null && wb != null) return 1;
      if (wa != null && wb == null) return -1;
      if (wa != null && wb != null && wa != wb) {
        // Menos WER = más preciso. Solo pesa en la medida en que el usuario
        // haya pedido precisión; con el mando en rapidez pura no ordena nada.
        final ventaja = (wb - wa) * p;
        if (ventaja != 0) return ventaja > 0 ? -1 : 1;
      }
      // Empate (o mando en rapidez): orden del catálogo, estable.
      return indice[a.engine]!.compareTo(indice[b.engine]!);
    });
    return ordenados;
  }
}
