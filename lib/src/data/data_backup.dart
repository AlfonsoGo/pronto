/// Copia de seguridad de la carpeta de datos, ANTES de migrar el esquema.
///
/// Es lo único que hace reversible un error de migración (PRNT-20): Pronto se
/// actualiza solo, así que la primera noticia que tiene el usuario de que su
/// esquema ha cambiado es que ya ha cambiado.
///
/// QUÉ NO SE COPIA: `models/`. Ahí viven los ~640 MB del modelo Parakeet, que
/// no son datos del usuario: se vuelven a descargar y se verifican por SHA-256
/// (`AppConfig.parakeetModelSha256`). Copiarlos duplicaría el disco en cada
/// actualización para proteger algo que no se puede perder. Lo que sí se copia
/// —`pronto.db` y `shared_preferences.json`— son decenas de kilobytes.
library;

import 'dart:io';

import 'package:path/path.dart' as p;

/// Nombre de la carpeta del modelo dentro de la carpeta de datos. No se copia.
const String kCarpetaModelos = 'models';

/// Sufijo de una copia todavía a medias. Una copia se escribe primero con este
/// sufijo y solo se renombra al nombre bueno cuando ha terminado entera: así
/// una copia interrumpida NO puede confundirse con una completa, que es el
/// único fallo que dejaría al usuario creyendo que tiene vuelta atrás.
const String kSufijoParcial = '.a-medias';

/// Copia [origen] dentro de [destinoRaiz] y devuelve la ruta de la copia.
///
/// El nombre lleva la fecha (`copia-2026-09-06-1432`). Si ya existe una copia
/// con ese mismo nombre —dos migraciones en el mismo minuto— se añade un
/// sufijo numérico en vez de pisarla.
/// [copiarFichero] existe solo para poder probar que una copia interrumpida no
/// deja rastro: sin poder provocar el fallo, ese camino sería código que nadie
/// ha visto correr nunca.
Future<String> copiarCarpetaDeDatos({
  required Directory origen,
  required Directory destinoRaiz,
  required DateTime fecha,
  Set<String> excluir = const {kCarpetaModelos},
  Future<void> Function(File origen, String destino)? copiarFichero,
}) async {
  if (!origen.existsSync()) {
    throw StateError('No existe la carpeta de datos: ${origen.path}');
  }
  if (p.isWithin(origen.path, destinoRaiz.path)) {
    // Copiaría la copia dentro de sí misma hasta llenar el disco.
    throw StateError(
      'La carpeta de copias no puede estar dentro de la de datos: '
      '${destinoRaiz.path}',
    );
  }

  destinoRaiz.createSync(recursive: true);
  final destino = Directory(
    p.join(destinoRaiz.path, _nombreLibre(destinoRaiz, fecha)),
  );
  final aMedias = Directory('${destino.path}$kSufijoParcial');

  if (aMedias.existsSync()) aMedias.deleteSync(recursive: true);
  aMedias.createSync(recursive: true);
  try {
    await _copiarContenido(
      origen,
      aMedias,
      excluir,
      copiarFichero ?? (f, destino) => f.copy(destino),
    );
    aMedias.renameSync(destino.path);
  } catch (_) {
    // Una copia a medias no se deja por ahí: o hay copia entera o no hay copia.
    try {
      if (aMedias.existsSync()) aMedias.deleteSync(recursive: true);
    } catch (_) {
      // Si ni siquiera se puede limpiar, el sufijo `.a-medias` ya dice que no
      // sirve. El error de la copia es lo que importa y se propaga entero.
    }
    rethrow;
  }
  return destino.path;
}

/// Nombre con la fecha dentro, libre en [destinoRaiz].
String _nombreLibre(Directory destinoRaiz, DateTime fecha) {
  String dos(int n) => n.toString().padLeft(2, '0');
  final base = 'copia-${fecha.year}-${dos(fecha.month)}-${dos(fecha.day)}'
      '-${dos(fecha.hour)}${dos(fecha.minute)}';
  var nombre = base;
  var n = 2;
  while (Directory(p.join(destinoRaiz.path, nombre)).existsSync() ||
      Directory(p.join(destinoRaiz.path, '$nombre$kSufijoParcial'))
          .existsSync()) {
    nombre = '$base-$n';
    n++;
  }
  return nombre;
}

Future<void> _copiarContenido(
  Directory origen,
  Directory destino,
  Set<String> excluir,
  Future<void> Function(File origen, String destino) copiarFichero,
) async {
  for (final entrada in origen.listSync(followLinks: false)) {
    final nombre = p.basename(entrada.path);
    if (excluir.contains(nombre)) continue;
    final salida = p.join(destino.path, nombre);
    if (entrada is Directory) {
      Directory(salida).createSync(recursive: true);
      // Lo excluido solo aplica al primer nivel: `models` es una carpeta de la
      // raíz. Dentro se copia todo.
      await _copiarContenido(
        entrada,
        Directory(salida),
        const {},
        copiarFichero,
      );
    } else if (entrada is File) {
      await copiarFichero(entrada, salida);
    }
    // Los enlaces simbólicos se saltan a propósito: seguirlos podría sacar la
    // copia de la carpeta de datos, y Pronto no crea ninguno.
  }
}
