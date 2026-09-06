import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'migration_notice.dart';

/// Aviso de que Pronto migró el formato de los datos al actualizarse, con la
/// ruta de la copia previa. Se queda puesto hasta que el usuario lo descarta:
/// es la única forma de enterarse de que hay camino de vuelta.
class MigracionBanner extends ConsumerWidget {
  const MigracionBanner({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final aviso = ref.watch(avisoMigracionProvider);
    if (aviso == null) return const SizedBox.shrink();

    final theme = Theme.of(context);
    final scheme = theme.colorScheme;
    final bien = aviso.correcto;
    final color = bien ? scheme.primary : scheme.error;

    return Padding(
      padding: const EdgeInsets.only(bottom: 20),
      child: Container(
        padding: const EdgeInsets.all(16),
        decoration: BoxDecoration(
          color: (bien ? scheme.primaryContainer : scheme.errorContainer)
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
                  bien
                      ? Icons.check_circle_outline_rounded
                      : Icons.warning_amber_rounded,
                  color: color,
                ),
                const SizedBox(width: 12),
                Expanded(
                  child: Text(
                    bien
                        ? 'Pronto ha actualizado el formato de tus datos '
                            '(versión ${aviso.versionInicial} → '
                            '${aviso.versionFinal}). Tus correcciones, tu '
                            'vocabulario y tus ajustes siguen donde estaban.'
                        : 'No se pudo actualizar el formato de tus datos, así '
                            'que Pronto no ha tocado nada. Tus datos están '
                            'como estaban.',
                    style: theme.textTheme.bodyMedium?.copyWith(
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                ),
              ],
            ),
            if (aviso.rutaCopia != null) ...[
              const SizedBox(height: 10),
              Text(
                'Antes de tocar nada se guardó una copia en:',
                style: theme.textTheme.bodySmall,
              ),
              const SizedBox(height: 2),
              SelectableText(
                aviso.rutaCopia!,
                style: theme.textTheme.bodySmall?.copyWith(
                  fontFamily: 'monospace',
                ),
              ),
            ],
            if (aviso.problema != null) ...[
              const SizedBox(height: 10),
              SelectableText(
                aviso.problema!,
                style: theme.textTheme.bodySmall,
              ),
            ],
            Align(
              alignment: Alignment.centerRight,
              child: TextButton(
                onPressed: () =>
                    ref.read(avisoMigracionProvider.notifier).descartar(),
                child: const Text('Entendido'),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
