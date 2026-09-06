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
final openReportProvider = FutureProvider<OpenReport?>((ref) async {
  final repo = ref.watch(learningRepositoryProvider);
  if (repo is! SqliteLearningRepository) return null;
  return repo.ensureOpen();
});

/// Lo que se le enseña al usuario.
///
/// Guarda el CASO ([outcome]) y los datos, no la frase: el texto lo escribe la
/// pantalla a partir del caso, con cadenas nuestras. [technicalDetail] es la
/// excepción cruda y se muestra etiquetada como tal, nunca como si fuera Pronto
/// hablando.
@immutable
class MigrationNotice {
  final OpenOutcome outcome;

  /// Dónde quedó la copia previa de la carpeta de datos.
  final String? backupPath;

  /// El texto crudo del fallo, para diagnosticar.
  final String? technicalDetail;

  final int fromVersion;
  final int toVersion;

  const MigrationNotice({
    required this.outcome,
    this.backupPath,
    this.technicalDetail,
    required this.fromVersion,
    required this.toVersion,
  });

  Map<String, dynamic> toJson() => {
        'outcome': outcome.name,
        'backupPath': backupPath,
        'technicalDetail': technicalDetail,
        'fromVersion': fromVersion,
        'toVersion': toVersion,
      };

  /// Reconstruye un aviso guardado. Devuelve `null` si el guardado no se
  /// entiende: mejor no enseñar nada que enseñar un aviso inventado.
  static MigrationNotice? fromJson(Map<String, dynamic> j) {
    final from = j['fromVersion'];
    final to = j['toVersion'];
    if (from is! int || to is! int) return null;
    final name = j['outcome'];
    OpenOutcome? outcome;
    for (final v in OpenOutcome.values) {
      if (v.name == name) outcome = v;
    }
    if (outcome == null) return null;
    return MigrationNotice(
      outcome: outcome,
      backupPath: j['backupPath'] as String?,
      technicalDetail: j['technicalDetail'] as String?,
      fromVersion: from,
      toVersion: to,
    );
  }
}

final migrationNoticeProvider =
    NotifierProvider<MigrationNoticeController, MigrationNotice?>(
  MigrationNoticeController.new,
);

/// Guarda el aviso pendiente y lo borra cuando el usuario lo lee.
class MigrationNoticeController extends Notifier<MigrationNotice?> {
  /// Clave del aviso pendiente en [SharedPreferences].
  static const String noticeKey = 'migration_notice';

  final Completer<void> _ready = Completer<void>();

  /// Se completa cuando ya se sabe si hay aviso o no.
  Future<void> get ready => _ready.future;

  /// El aviso lo ha creado ESTE arranque (no viene de uno anterior sin leer).
  ///
  /// Es lo que decide si se abre el panel: el ticket pide avisar «la primera
  /// vez que abre», no plantarle el panel en la cara en cada arranque hasta que
  /// se acuerde de pulsar «Entendido».
  bool bornThisLaunch = false;

  @override
  MigrationNotice? build() {
    scheduleMicrotask(_load);
    return null;
  }

  Future<void> _load() async {
    try {
      final prefs = await SharedPreferences.getInstance();

      // 1) Un aviso de un arranque anterior que nadie llegó a descartar.
      final stored = prefs.getString(noticeKey);
      if (stored != null && stored.isNotEmpty) {
        final j = jsonDecode(stored);
        if (j is Map<String, dynamic>) {
          state = MigrationNotice.fromJson(j);
        }
      }

      // 2) Lo que haya pasado en ESTE arranque.
      final report = await ref.read(openReportProvider.future);
      if (report == null || !report.needsNotice) return;

      final notice = MigrationNotice(
        outcome: report.outcome,
        backupPath: report.backupPath,
        technicalDetail: report.technicalDetail,
        fromVersion: report.fromVersion,
        toVersion: report.toVersion,
      );
      state = notice;
      bornThisLaunch = true;
      await prefs.setString(noticeKey, jsonEncode(notice.toJson()));
    } catch (e) {
      debugPrint('No se pudo preparar el aviso de migración: $e');
    } finally {
      if (!_ready.isCompleted) _ready.complete();
    }
  }

  /// El usuario ya lo ha leído.
  Future<void> dismiss() async {
    state = null;
    bornThisLaunch = false;
    try {
      final prefs = await SharedPreferences.getInstance();
      await prefs.remove(noticeKey);
    } catch (e) {
      debugPrint('No se pudo borrar el aviso de migración: $e');
    }
  }
}
