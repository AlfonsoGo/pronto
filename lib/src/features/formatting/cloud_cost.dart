import 'format_prompt.dart';

/// Cuánto cuesta formatear en la nube, y cómo se escribe en español.
///
/// Sirve para DOS cosas distintas que no hay que confundir:
///  - **Estimar** lo que va a costar la llamada que está a punto de salir. Es lo
///    que mira el tope de gasto ANTES de mandar nada.
///  - **Escribir** un importe en la interfaz.
///
/// Lo que se APUNTA en el libro de cuentas no sale de aquí: OpenRouter devuelve
/// el coste real de cada llamada en `usage.cost` y ese es el que manda. La
/// estimación solo existe porque el tope se comprueba antes de tener el real.

/// Precios de OpenRouter en dólares por millón de tokens: (entrada, salida).
///
/// Verificados el 2026-09-04 en `openrouter.ai/models`. ⚠ Envejecen: son para
/// el TOPE, no para facturar. La lista es además la LISTA BLANCA de modelos de
/// nube que la interfaz ofrece: un modelo que no está aquí no se puede elegir,
/// así que el tope nunca calcula con un precio que no conoce.
const Map<String, (double, double)> kCloudPrices = {
  'openai/gpt-5-nano': (0.05, 0.40),
  'google/gemini-3.1-flash-lite': (0.25, 1.50),
  'openai/gpt-5-mini': (0.25, 2.00),
};

/// Precio que se le supone a un modelo que NO está en la tabla: caro a
/// propósito. Un desconocido tiene que estorbar al tope, no colarse por debajo.
const (double, double) kUnknownCloudPrice = (1.0, 3.0);

/// Caracteres por token en español. Medida gruesa (el bench usa la misma) y
/// suficiente: el tope necesita un orden de magnitud, no una factura.
const double kCharsPerToken = 3.5;

/// Tokens que gasta el prompt fijo (sistema + ejemplos) en CADA llamada. Se
/// calcula de las cadenas reales en vez de ponerse a ojo: si mañana el prompt
/// crece, el tope se entera solo.
final int kFormatPromptTokens = () {
  var chars = kFormatSystemPrompt.length;
  for (final ejemplo in kFormatFewShots) {
    chars += (ejemplo['user']?.length ?? 0) +
        (ejemplo['assistant']?.length ?? 0);
  }
  return (chars / kCharsPerToken).ceil();
}();

/// Coste estimado, en dólares, de formatear [textChars] caracteres con [model].
///
/// Deliberadamente PESIMISTA: cuenta la salida como 1,5 veces la entrada más un
/// colchón. Un tope que se queda corto en la estimación deja pasar la llamada
/// que lo rompe, así que el error se comete siempre hacia arriba.
double estimateCallCostUsd({required String model, required int textChars}) {
  final precio = kCloudPrices[model] ?? kUnknownCloudPrice;
  final tokensTexto = textChars / kCharsPerToken;
  final entrada = kFormatPromptTokens + tokensTexto;
  final salida = tokensTexto * 1.5 + 50;
  return entrada / 1e6 * precio.$1 + salida / 1e6 * precio.$2;
}

/// Un importe en dólares como se escribe en España: coma decimal y el símbolo
/// detrás. [decimals] sube a 4 para los importes minúsculos del uso normal
/// (un mes entero de dictado son céntimos: con dos decimales saldría «0,00 $»).
String formatUsd(double usd, {int decimals = 4}) =>
    '${usd.toStringAsFixed(decimals).replaceAll('.', ',')} \$';

/// Lee un importe escrito por el usuario aceptando coma o punto decimal.
/// Devuelve `null` si no es un número o si es negativo (un tope negativo
/// apagaría la nube por la puerta de atrás en vez de decirlo).
double? parseUsd(String raw) {
  final v = double.tryParse(raw.trim().replaceAll(',', '.'));
  if (v == null || v.isNaN || v.isInfinite || v < 0) return null;
  return v;
}
