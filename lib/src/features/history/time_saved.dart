/// El «tiempo ahorrado» de las estadísticas del Historial.
///
/// Es una **estimación**, y por eso la pantalla enseña la fórmula entera al
/// lado del número: una cifra sin su cuenta se lee como una medida, y esto no
/// lo es. Lo único medido de verdad son las palabras, lo que se habló y lo que
/// tardó la máquina; lo que se supone es a qué velocidad habrías tecleado.
library;

/// Velocidad de mecanografía que se supone, en palabras por minuto.
///
/// 40 ppm es la referencia habitual de un adulto que escribe a diario en
/// teclado. No está medida en Txonso ni en nadie: es el supuesto que hace falta
/// para poder restar, y va escrito en la pantalla para que se pueda discutir.
const int kTypingWordsPerMinute = 40;

/// La cuenta, en una línea, tal cual se enseña bajo el número.
const String kTimeSavedFormula =
    'palabras ÷ $kTypingWordsPerMinute por minuto (lo que se tarda tecleando) '
    '− lo que tardaste hablando − lo que tardó Pronto';

/// Cuánto se ahorró dictando en vez de teclear.
///
/// [processing] cuenta porque es espera del usuario: los segundos que Pronto
/// pasa transcribiendo y formateando son suyos, no gratis.
///
/// Nunca devuelve negativo. Un «has perdido tiempo» acumulado no le sirve a
/// nadie y convierte la pantalla en un reproche.
Duration timeSaved({
  required int words,
  required Duration spoken,
  required Duration processing,
}) {
  if (words <= 0) return Duration.zero;
  final tecleando = Duration(
    microseconds:
        (words / kTypingWordsPerMinute * Duration.microsecondsPerMinute)
            .round(),
  );
  final gastado = spoken + processing;
  return tecleando > gastado ? tecleando - gastado : Duration.zero;
}
