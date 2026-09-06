import 'text_formatter.dart';

/// Qué asistente arregló ESTE dictado y cuánto tardó, para la Píldora.
///
/// Es la línea que contesta la pregunta que hoy no se puede contestar mirando
/// la pantalla: «¿esto lo ha tocado el modelo, y cuánto me ha costado en
/// espera?». Sin ella, un Formateo local lento y uno de nube instantáneo se ven
/// exactamente igual — el texto aparece y ya está.
///
/// Devuelve cadena VACÍA cuando no hay nada que decir:
///  - sin Camino de formateo no hay asistente del que hablar;
///  - y cuando algo salió mal, quien habla es el aviso ([FormatResult.notice]),
///    que explica el motivo. Poner las dos cosas sería contar dos veces lo
///    mismo en una caja de cuatro líneas.
///
/// ⚠ Es una cadena NUESTRA, construida aquí con piezas nuestras y el nombre del
/// modelo (que lo eligió el usuario). Nada de esto lo escribe un modelo.
String etiquetaAsistente({
  required FormatPath path,
  required String modelo,
  required FormatOutcome outcome,
  required Duration elapsed,
}) {
  if (path == FormatPath.none) return '';
  if (outcome != FormatOutcome.formatted &&
      outcome != FormatOutcome.unchanged) {
    return '';
  }

  final donde = switch (path) {
    FormatPath.local => 'En tu equipo',
    FormatPath.cloud => 'Nube',
    FormatPath.none => '',
  };
  return '$donde · $modelo · ${formatearEspera(elapsed)}';
}

/// La espera, en la unidad en la que se lee sin pensar.
///
/// Por debajo del segundo se dan milisegundos: «0,1 s» y «0,8 s» parecen lo
/// mismo de un vistazo y no lo son. A partir del segundo manda el segundo, con
/// un decimal, porque es lo que se nota al dictar.
String formatearEspera(Duration d) {
  final ms = d.inMilliseconds;
  if (ms < 1000) return '$ms ms';
  return '${(ms / 1000).toStringAsFixed(1).replaceAll('.', ',')} s';
}
