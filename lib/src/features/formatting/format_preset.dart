import 'format_prompt.dart';

/// El trabajo que se le pide al Modelo de formateo, con los topes de su
/// guardia.
///
/// Es la mitad del **Preset** de `CONTEXT.md` que ya se puede escribir hoy: el
/// Prompt y lo que se le tolera a su salida. Cuando el PRNT-3 traiga el almacén
/// de Modos, cada Modo apuntará al suyo; hasta entonces solo se usa
/// [FormatPreset.limpio], que es el único trabajo que Pronto sabe hacer.
///
/// ─── Por qué los topes viven AQUÍ y no en `AppConfig` ────────────────────────
/// Estaban en `AppConfig`, globales, y el [ADR 0004] ya decía que no: son
/// propiedad del Preset. Los números que hay (0,20 / 0,60) **se midieron sobre
/// 443 dictados formateados al estilo «Limpio»**, y ahí valen. «Nota»
/// reestructura en viñetas a propósito y «Correo» reordena en párrafos:
/// aplicarles el tope de Limpio descartaría su salida legítima, y el usuario
/// vería «sin formatear» sin entender por qué.
///
/// Mientras el tope fuera global, ese fallo estaba garantizado en cuanto
/// existiera el segundo Preset. Ahora, añadir un Preset obliga a traer sus dos
/// números — y eso significa medirlos con su propio corpus antes de encenderlo.
///
/// [ADR 0004]: docs/adr/0004-los-presets-son-nuestros-no-los-de-superwhisper.md
class FormatPreset {
  /// Cómo se llama en el código y en la base de datos. Estable: se guarda.
  final String id;

  /// Cómo se llama en pantalla.
  final String nombre;

  /// Las instrucciones que recibe el Modelo de formateo. Cadena NUESTRA,
  /// versionada en el repo.
  final String prompt;

  /// Ejemplos que acompañan al prompt.
  final List<Map<String, String>> fewShots;

  /// Fracción máxima de contenido AÑADIDO (palabras insertadas o sustituidas)
  /// que se le tolera a la salida antes de descartarla. Es la señal de
  /// alucinación: un modelo que se inventa algo añade palabras.
  final double maxAddedContent;

  /// Fracción máxima de contenido BORRADO. Quitar muletillas es formateo
  /// correcto; comerse media frase es resumir, y eso sí pierde texto.
  final double maxDeletedContent;

  /// De dónde salen esos dos números. Se enseña en ningún sitio: existe para
  /// que quien los cambie tenga delante sobre qué se midieron.
  final String medidoSobre;

  const FormatPreset({
    required this.id,
    required this.nombre,
    required this.prompt,
    required this.fewShots,
    required this.maxAddedContent,
    required this.maxDeletedContent,
    required this.medidoSobre,
  });

  /// El corrector de español: puntuación, tildes, concordancia, mayúsculas y
  /// muletillas fuera. Es el único Preset con Prompt escrito hoy, y el que
  /// usan los dos Caminos de formateo.
  ///
  /// Sus topes son los medidos: un guardia SIMÉTRICO al 0,15 descartaba el
  /// 10,2 % del formateo legítimo porque contaba las muletillas borradas como
  /// invención; midiendo solo lo añadido, el 0,20 descarta el 1,6 %. Y el 0,60
  /// de borrado no es un percentil: el borrado legítimo del corpus llega a
  /// 0,52 y un modelo que RESUME se va por encima de 0,80.
  static const FormatPreset limpio = FormatPreset(
    id: 'limpio',
    nombre: 'Limpio',
    prompt: kFormatSystemPrompt,
    fewShots: kFormatFewShots,
    maxAddedContent: 0.20,
    maxDeletedContent: 0.60,
    medidoSobre: '443 dictados reales en español (Wispr Flow, abr–jun 2026)',
  );

  // «Crudo» NO está aquí. Crudo es no formatear —[FormatPath.none]—, no un
  // Prompt distinto: darle una entrada en esta lista sería un Preset con un
  // prompt que nadie manda nunca. Los otros cinco («Mensaje», «Correo»,
  // «Terminal», «Nota», «Personalizado») llegan con el PRNT-3, que es lo que
  // les da una pantalla donde elegirlos; escribirlos antes dejaría Presets
  // medidos que nada puede seleccionar.
}
