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
///
/// Por eso la copia **no sirve para reemplazar** la carpeta de datos: se
/// restaura ENCIMA. Reemplazarla dejaría al usuario sin el modelo de voz.
library;

import 'dart:io';

import 'package:path/path.dart' as p;

/// Nombre de la carpeta del modelo dentro de la carpeta de datos. No se copia.
const String kModelsFolder = 'models';

/// Sufijo de una copia todavía a medias. Una copia se escribe primero con este
/// sufijo y solo se renombra al nombre bueno cuando ha terminado entera: así
/// una copia interrumpida NO puede confundirse con una completa, que es el
/// único fallo que dejaría al usuario creyendo que tiene vuelta atrás.
const String kPartialSuffix = '.a-medias';

/// Copia [source] dentro de [backupsRoot] y devuelve la ruta de la copia.
///
/// El nombre lleva la fecha (`copia-2026-09-06-1432`). Si ya existe una copia
/// con ese mismo nombre —dos migraciones en el mismo minuto— se añade un
/// sufijo numérico en vez de pisarla.
///
/// [copyFile] existe solo para poder probar que una copia interrumpida no deja
/// rastro: sin poder provocar el fallo, ese camino sería código que nadie ha
/// visto correr nunca.
Future<String> backUpDataFolder({
  required Directory source,
  required Directory backupsRoot,
  required DateTime date,
  Future<void> Function(File source, String target)? copyFile,
}) async {
  if (!source.existsSync()) {
    throw StateError('No existe la carpeta de datos: ${source.path}');
  }
  if (p.isWithin(source.path, backupsRoot.path)) {
    // Copiaría la copia dentro de sí misma hasta llenar el disco.
    throw StateError(
      'La carpeta de copias no puede estar dentro de la de datos: '
      '${backupsRoot.path}',
    );
  }

  backupsRoot.createSync(recursive: true);
  // Restos de un intento que se murió por el camino (matar el proceso no lanza
  // ninguna excepción, así que nadie los limpió). Se barren ahora: si no, se
  // quedan para siempre ocupando disco y empujan el nombre de las copias buenas
  // a `-2`, `-3`, `-4`…
  _sweepPartials(backupsRoot);

  final target = Directory(
    p.join(backupsRoot.path, _freeName(backupsRoot, date)),
  );
  final partial = Directory('${target.path}$kPartialSuffix');

  partial.createSync(recursive: true);
  try {
    await _copyContents(
      source,
      partial,
      const {kModelsFolder},
      copyFile ?? (f, target) => f.copy(target),
    );
    partial.renameSync(target.path);
  } catch (_) {
    // Una copia a medias no se deja por ahí: o hay copia entera o no hay copia.
    try {
      if (partial.existsSync()) partial.deleteSync(recursive: true);
    } catch (_) {
      // Si ni siquiera se puede limpiar, el sufijo `.a-medias` ya dice que no
      // sirve, y el barrido del próximo intento se la lleva. El error de la
      // copia es lo que importa y se propaga entero.
    }
    rethrow;
  }
  return target.path;
}

/// Borra las copias a medias que quedaron de intentos anteriores.
void _sweepPartials(Directory backupsRoot) {
  for (final entry in backupsRoot.listSync(followLinks: false)) {
    if (entry is! Directory) continue;
    if (!p.basename(entry.path).endsWith(kPartialSuffix)) continue;
    try {
      entry.deleteSync(recursive: true);
    } catch (_) {
      // Si está bloqueada, se intentará la próxima vez. No es motivo para no
      // hacer la copia de hoy.
    }
  }
}

/// Nombre con la fecha dentro, libre en [backupsRoot].
String _freeName(Directory backupsRoot, DateTime date) {
  String two(int n) => n.toString().padLeft(2, '0');
  final base = 'copia-${date.year}-${two(date.month)}-${two(date.day)}'
      '-${two(date.hour)}${two(date.minute)}';
  var name = base;
  var n = 2;
  while (Directory(p.join(backupsRoot.path, name)).existsSync() ||
      Directory(p.join(backupsRoot.path, '$name$kPartialSuffix'))
          .existsSync()) {
    name = '$base-$n';
    n++;
  }
  return name;
}

Future<void> _copyContents(
  Directory source,
  Directory target,
  Set<String> exclude,
  Future<void> Function(File source, String target) copyFile,
) async {
  for (final entry in source.listSync(followLinks: false)) {
    final name = p.basename(entry.path);
    if (exclude.contains(name)) continue;
    final out = p.join(target.path, name);
    if (entry is Directory) {
      Directory(out).createSync(recursive: true);
      // Lo excluido solo aplica al primer nivel: `models` es una carpeta de la
      // raíz. Dentro se copia todo.
      await _copyContents(entry, Directory(out), const {}, copyFile);
    } else if (entry is File) {
      await copyFile(entry, out);
    }
    // Los enlaces simbólicos se saltan a propósito: seguirlos podría sacar la
    // copia de la carpeta de datos, y Pronto no crea ninguno.
  }
}
