/// Apertura de `pronto.db`: copia de seguridad primero, migración después.
///
/// Este es el único sitio donde `pronto.db` se abre. Junta las dos mitades del
/// PRNT-20 —la copia previa y las migraciones numeradas— porque el orden entre
/// ellas ES el requisito: primero se copia, luego se toca.
library;

import 'dart:io';

import 'package:path/path.dart' as p;
import 'package:path_provider/path_provider.dart';
import 'package:sqlite3/sqlite3.dart';

import 'data_backup.dart';
import 'schema_migrations.dart';

/// Nombre del fichero de la base de datos dentro de la carpeta de datos.
const String kFicheroBd = 'pronto.db';

/// Carpeta donde se dejan las copias previas: HERMANA de la carpeta de datos
/// (`%APPDATA%\Pronto\copias`), no dentro. Dentro, cada copia entraría en la
/// siguiente y crecerían unas sobre otras.
const String kCarpetaCopias = 'copias';

/// Lo que pasó al abrir la base. Es lo que el usuario ve tras actualizar.
class InformeApertura {
  final int versionInicial;
  final int versionFinal;

  final List<String> aplicadas;
  final bool instalacionNueva;
  final bool baseMasNuevaQueLaApp;

  /// Dónde quedó la copia previa, si se hizo.
  final String? rutaCopia;

  /// Por qué no se pudo copiar. Si esto no es null, NO se ha migrado nada: sin
  /// copia no hay vuelta atrás, y migrar a ciegas sería el único camino sin
  /// salida.
  final String? errorCopia;

  /// Por qué falló la migración. La base sigue en [versionInicial]: cada
  /// migración va en su transacción, así que no queda nada a medias.
  final String? errorMigracion;

  /// Por qué no se pudo ni abrir la base. Aquí no hay versiones que contar:
  /// Pronto arranca sin aprendizaje y el usuario tiene que saberlo.
  final String? errorApertura;

  const InformeApertura({
    this.versionInicial = 0,
    this.versionFinal = 0,
    this.aplicadas = const [],
    this.instalacionNueva = false,
    this.baseMasNuevaQueLaApp = false,
    this.rutaCopia,
    this.errorCopia,
    this.errorMigracion,
    this.errorApertura,
  });

  /// Se migró una instalación que ya tenía datos: hay algo que contarle al
  /// usuario. Una instalación nueva no cuenta nada.
  bool get huboActualizacion =>
      !instalacionNueva && versionFinal > versionInicial;

  /// Algo salió mal y el usuario tiene que enterarse aunque la app arranque.
  bool get huboProblema =>
      errorCopia != null || errorMigracion != null || errorApertura != null;

  /// ¿Hay que enseñar el aviso?
  bool get mereceAviso => huboActualizacion || huboProblema;
}

/// La base abierta y lista, con el informe de lo que pasó al abrirla.
class AperturaBaseDeDatos {
  final Database db;
  final InformeApertura informe;

  const AperturaBaseDeDatos(this.db, this.informe);
}

/// Abre `pronto.db` de [soporte], copiando y migrando si hace falta.
///
/// [copiasRaiz] es dónde van las copias previas; [ahora] permite fijar la fecha
/// del nombre de la copia en los tests.
Future<AperturaBaseDeDatos> abrirBaseDeDatos({
  required Directory soporte,
  required Directory copiasRaiz,
  DateTime Function()? ahora,
  List<Migracion> migraciones = kMigracionesPronto,
}) async {
  soporte.createSync(recursive: true);
  final ruta = p.join(soporte.path, kFicheroBd);

  var db = sqlite3.open(ruta);
  try {
    final inicial = versionEsquema(db);
    final nueva = baseVacia(db);
    final pendientes = hayMigracionesPendientes(db, migraciones: migraciones);

    // Nada que hacer: ni copia ni migración ni aviso.
    if (!pendientes) {
      final r = migrarEsquema(db, migraciones: migraciones);
      return AperturaBaseDeDatos(
        db,
        InformeApertura(
          versionInicial: r.versionInicial,
          versionFinal: r.versionFinal,
          instalacionNueva: r.instalacionNueva,
          baseMasNuevaQueLaApp: r.baseMasNuevaQueLaApp,
        ),
      );
    }

    // Hay que migrar. Si ya había datos, primero la copia — y con la base
    // CERRADA, para que lo copiado sea un fichero consistente y no una foto a
    // media escritura.
    String? rutaCopia;
    if (!nueva) {
      db.dispose();
      try {
        rutaCopia = await copiarCarpetaDeDatos(
          origen: soporte,
          destinoRaiz: copiasRaiz,
          fecha: (ahora ?? DateTime.now)(),
        );
      } catch (e) {
        // Sin copia no se migra. La app arranca con el esquema viejo y el
        // usuario se entera por el aviso, en vez de descubrirlo cuando le
        // falten datos.
        db = sqlite3.open(ruta);
        return AperturaBaseDeDatos(
          db,
          InformeApertura(
            versionInicial: inicial,
            versionFinal: inicial,
            instalacionNueva: false,
            errorCopia: '$e',
          ),
        );
      }
      db = sqlite3.open(ruta);
    }

    try {
      final r = migrarEsquema(db, migraciones: migraciones);
      return AperturaBaseDeDatos(
        db,
        InformeApertura(
          versionInicial: r.versionInicial,
          versionFinal: r.versionFinal,
          aplicadas: r.aplicadas,
          instalacionNueva: r.instalacionNueva,
          baseMasNuevaQueLaApp: r.baseMasNuevaQueLaApp,
          rutaCopia: rutaCopia,
        ),
      );
    } catch (e) {
      return AperturaBaseDeDatos(
        db,
        InformeApertura(
          versionInicial: inicial,
          versionFinal: versionEsquema(db),
          instalacionNueva: nueva,
          rutaCopia: rutaCopia,
          errorMigracion: '$e',
        ),
      );
    }
  } catch (_) {
    // No dejamos la base abierta si la apertura no llega a buen puerto. El
    // dispose va protegido porque en el camino de la copia la instancia ya
    // pudo cerrarse antes.
    try {
      db.dispose();
    } catch (_) {}
    rethrow;
  }
}

/// Igual que [abrirBaseDeDatos] pero resolviendo las carpetas reales de la app:
/// los datos en `getApplicationSupportDirectory()` y las copias en su hermana
/// `copias/`.
Future<AperturaBaseDeDatos> abrirBaseDeDatosDePronto() async {
  final soporte = await getApplicationSupportDirectory();
  final copias = Directory(p.join(p.dirname(soporte.path), kCarpetaCopias));
  return abrirBaseDeDatos(soporte: soporte, copiasRaiz: copias);
}
