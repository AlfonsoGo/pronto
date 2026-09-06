/// El aviso de que Pronto tocó tus datos al actualizarse.
///
/// Pronto se actualiza solo: la v0.9 aterriza en máquinas de gente que no ha
/// pedido nada y no va a leer ninguna nota de versión. Si el esquema de
/// `pronto.db` ha cambiado, lo mínimo es decirlo y decir dónde quedó la copia
/// previa por si hay que volver (PRNT-20).
///
/// El aviso se PERSISTE hasta que el usuario lo descarta: si se enseñara solo
/// en memoria, cerrar Pronto antes de mirarlo lo borraría para siempre y el
/// usuario nunca sabría que tiene una copia esperándole.
library;

import 'dart:async';
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../../data/pronto_database.dart';
import '../../data/sqlite_learning_repository.dart';
import '../../platform/platform_services.dart';

/// Lo que pasó al abrir `pronto.db` en este arranque. `null` cuando la
/// implementación no es la de SQLite (tests con repositorio en memoria).
final informeAperturaProvider = FutureProvider<InformeApertura?>((ref) async {
  final repo = ref.watch(learningRepositoryProvider);
  if (repo is! SqliteLearningRepository) return null;
  return repo.asegurarAbierta();
});

/// Lo que se le enseña al usuario. Guarda datos, no frases: el texto lo pone la
/// pantalla, para que ningún mensaje del sistema salga de una cadena ajena.
@immutable
class AvisoMigracion {
  /// Dónde quedó la copia previa de la carpeta de datos.
  final String? rutaCopia;

  /// Qué falló, si falló algo. `null` = todo salió bien.
  final String? problema;

  final int versionInicial;
  final int versionFinal;

  const AvisoMigracion({
    this.rutaCopia,
    this.problema,
    required this.versionInicial,
    required this.versionFinal,
  });

  bool get correcto => problema == null;

  Map<String, dynamic> toJson() => {
        'rutaCopia': rutaCopia,
        'problema': problema,
        'versionInicial': versionInicial,
        'versionFinal': versionFinal,
      };

  static AvisoMigracion? fromJson(Map<String, dynamic> j) {
    final ini = j['versionInicial'];
    final fin = j['versionFinal'];
    if (ini is! int || fin is! int) return null;
    return AvisoMigracion(
      rutaCopia: j['rutaCopia'] as String?,
      problema: j['problema'] as String?,
      versionInicial: ini,
      versionFinal: fin,
    );
  }
}

final avisoMigracionProvider =
    NotifierProvider<AvisoMigracionController, AvisoMigracion?>(
  AvisoMigracionController.new,
);

/// Guarda el aviso pendiente y lo borra cuando el usuario lo lee.
class AvisoMigracionController extends Notifier<AvisoMigracion?> {
  /// Clave del aviso pendiente en [SharedPreferences].
  static const String claveAviso = 'aviso_migracion';

  final Completer<void> _listo = Completer<void>();

  /// Se completa cuando ya se sabe si hay aviso o no. Para los tests y para
  /// que el arranque pueda decidir si abrir el panel.
  Future<void> get listo => _listo.future;

  @override
  AvisoMigracion? build() {
    scheduleMicrotask(_cargar);
    return null;
  }

  Future<void> _cargar() async {
    try {
      final prefs = await SharedPreferences.getInstance();

      // 1) Un aviso de un arranque anterior que nadie llegó a descartar.
      final guardado = prefs.getString(claveAviso);
      if (guardado != null && guardado.isNotEmpty) {
        final j = jsonDecode(guardado);
        if (j is Map<String, dynamic>) {
          state = AvisoMigracion.fromJson(j);
        }
      }

      // 2) Lo que haya pasado en ESTE arranque.
      final informe = await ref.read(informeAperturaProvider.future);
      if (informe == null || !informe.mereceAviso) return;

      final aviso = AvisoMigracion(
        rutaCopia: informe.rutaCopia,
        problema: informe.errorApertura ??
            informe.errorCopia ??
            informe.errorMigracion,
        versionInicial: informe.versionInicial,
        versionFinal: informe.versionFinal,
      );
      state = aviso;
      await prefs.setString(claveAviso, jsonEncode(aviso.toJson()));
    } catch (e) {
      debugPrint('No se pudo preparar el aviso de migración: $e');
    } finally {
      if (!_listo.isCompleted) _listo.complete();
    }
  }

  /// El usuario ya lo ha leído.
  Future<void> descartar() async {
    state = null;
    try {
      final prefs = await SharedPreferences.getInstance();
      await prefs.remove(claveAviso);
    } catch (e) {
      debugPrint('No se pudo borrar el aviso de migración: $e');
    }
  }
}
