import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../data/pronto_database.dart';
import '../../data/schema_migrations.dart';
import 'migration_notice.dart';

/// Aviso de que la actualización tocó la base de datos, con la ruta de la copia
/// previa. Se queda puesto hasta que el usuario lo descarta: es la única forma
/// de enterarse de que hay camino de vuelta.
///
/// Cada caso tiene su frase y todas tienen que ser verdad. En particular, una
/// migración que entró a medias **no** puede decir «no se ha tocado nada».
class MigrationBanner extends ConsumerWidget {
  const MigrationBanner({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final notice = ref.watch(migrationNoticeProvider);
    if (notice == null) return const SizedBox.shrink();

    final theme = Theme.of(context);
    final scheme = theme.colorScheme;
    final good = notice.outcome == OpenOutcome.migrated;
    final color = good ? scheme.primary : scheme.error;

    return Padding(
      padding: const EdgeInsets.only(bottom: 20),
      child: Container(
        padding: const EdgeInsets.all(16),
        decoration: BoxDecoration(
          color: (good ? scheme.primaryContainer : scheme.errorContainer)
              .withValues(alpha: 0.5),
          borderRadius: BorderRadius.circular(14),
          border: Border.all(color: color.withValues(alpha: 0.5)),
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Icon(
                  good
                      ? Icons.check_circle_outline_rounded
                      : Icons.warning_amber_rounded,
                  color: color,
                ),
                const SizedBox(width: 12),
                Expanded(
                  child: Text(
                    _mensaje(notice),
                    style: theme.textTheme.bodyMedium?.copyWith(
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                ),
              ],
            ),
            if (notice.backupPath != null) ...[
              const SizedBox(height: 10),
              Text(
                'Antes de tocar nada se guardó una copia de tus datos aquí '
                '(el modelo de voz no entra en la copia: se vuelve a '
                'descargar solo):',
                style: theme.textTheme.bodySmall,
              ),
              const SizedBox(height: 2),
              SelectableText(
                notice.backupPath!,
                style: theme.textTheme.bodySmall?.copyWith(
                  fontFamily: 'monospace',
                ),
              ),
            ],
            if (notice.technicalDetail != null) ...[
              const SizedBox(height: 10),
              Text(
                'Detalle técnico del fallo:',
                style: theme.textTheme.bodySmall,
              ),
              const SizedBox(height: 2),
              SelectableText(
                notice.technicalDetail!,
                style: theme.textTheme.bodySmall?.copyWith(
                  fontFamily: 'monospace',
                ),
              ),
            ],
            Align(
              alignment: Alignment.centerRight,
              child: TextButton(
                onPressed: () =>
                    ref.read(migrationNoticeProvider.notifier).dismiss(),
                child: const Text('Entendido'),
              ),
            ),
          ],
        ),
      ),
    );
  }

  /// La frase de cada caso. Cadenas nuestras, ninguna viene de la excepción.
  String _mensaje(MigrationNotice n) {
    switch (n.outcome) {
      case OpenOutcome.migrated:
        return 'Pronto ha actualizado el formato de tus datos '
            '(versión ${n.fromVersion} → ${n.toVersion}). Tus correcciones, tu '
            'vocabulario y tus ajustes siguen donde estaban.';
      case OpenOutcome.migratedThenFailed:
        return 'Pronto actualizó el formato de tus datos a medias '
            '(versión ${n.fromVersion} → ${n.toVersion}) y se quedó a mitad. '
            'No sigas dictando: restaura la copia de abajo encima de tu '
            'carpeta de datos y cuéntaselo a quien te dio Pronto.';
      case OpenOutcome.untouchedAfterError:
      case OpenOutcome.untouchedNoBackup:
        return 'No se pudo actualizar el formato de tus datos, así que Pronto '
            'no ha tocado nada. Tus datos están como estaban.';
      case OpenOutcome.databaseFromNewerVersion:
        return 'Tus datos vienen de una versión más nueva de Pronto '
            '(formato ${n.fromVersion}, y esta entiende hasta el '
            '${targetVersion(kProntoMigrations)}). No se han tocado, pero '
            'puede que esta versión no vea todo lo que guardaste con la nueva.';
      case OpenOutcome.couldNotOpen:
        return 'Pronto no ha podido abrir su base de datos. Funciona el '
            'dictado, pero esta sesión no va a aprender de tus correcciones ni '
            'a guardar nada.';
      case OpenOutcome.nothingToSay:
        return '';
    }
  }
}
