/// Cuánta memoria tiene este equipo.
///
/// Se pregunta para no proponerle a nadie un modelo local que no le cabe: un
/// modelo que no entra en RAM no da un error claro, se pone a paginar, y el
/// usuario ve un Formateo que tarda diez segundos sin saber por qué.
abstract class SystemMemory {
  /// Memoria física total, en bytes. `null` cuando no se ha podido leer.
  ///
  /// El `null` es un valor propio a conciencia: sin cifra **no se filtra nada**
  /// y no se enseña ningún número. Inventarse una memoria para poder decir «no
  /// te cabe» sería peor que no decirlo.
  int? totalBytes();
}

/// La de una plataforma donde todavía no sabemos preguntarlo.
class UnknownSystemMemory implements SystemMemory {
  const UnknownSystemMemory();

  @override
  int? totalBytes() => null;
}
