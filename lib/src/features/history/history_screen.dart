import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:path/path.dart' as p;
import 'package:path_provider/path_provider.dart';

import '../../platform/platform_services.dart';
import '../formatting/text_formatter.dart';
import '../learning/word_diff.dart';
import 'dictation_entry.dart';
import 'history_controller.dart';
import 'history_export.dart';
import 'history_repository.dart';
import 'time_saved.dart';

/// El Historial: lista con búsqueda a la izquierda, el dictado abierto a la
/// derecha (PRNT-16, variante A).
///
/// Lo que la hace útil no es el archivo: es ver **el crudo y el final uno
/// debajo del otro**, con lo que cambió el Formateo marcado. Eso convierte la
/// pantalla en una herramienta de diagnóstico.
class HistoryScreen extends ConsumerWidget {
  const HistoryScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final state = ref.watch(historyControllerProvider);
    final theme = Theme.of(context);

    return Scaffold(
      appBar: AppBar(
        title: const Text('Historial'),
        leading: BackButton(onPressed: () => Navigator.of(context).maybePop()),
        actions: [
          TextButton.icon(
            onPressed: state.total == 0
                ? null
                : () => _export(context, ref, json: false),
            icon: const Icon(Icons.table_view_outlined, size: 18),
            label: const Text('Exportar CSV'),
          ),
          TextButton.icon(
            onPressed: state.total == 0
                ? null
                : () => _export(context, ref, json: true),
            icon: const Icon(Icons.data_object, size: 18),
            label: const Text('JSON'),
          ),
          TextButton.icon(
            onPressed:
                state.total == 0 ? null : () => _confirmDeleteAll(context, ref),
            icon: Icon(Icons.delete_outline, size: 18, color: theme.colorScheme.error),
            label: Text(
              'Borrar todo',
              style: TextStyle(color: theme.colorScheme.error),
            ),
          ),
          const SizedBox(width: 8),
        ],
      ),
      body: state.unavailable
          ? const _Aviso(
              icon: Icons.storage_outlined,
              title: 'El historial no está disponible',
              body: 'No se pudo abrir la base de datos de Pronto. Los dictados '
                  'siguen funcionando; lo que no se puede es consultarlos.',
            )
          : Column(
              children: [
                _StatsBar(state: state),
                const Divider(height: 1),
                Expanded(
                  child: Row(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      SizedBox(width: 340, child: _Lista(state: state)),
                      const VerticalDivider(width: 1),
                      Expanded(child: _Detalle(entry: state.selected)),
                    ],
                  ),
                ),
              ],
            ),
    );
  }

  Future<void> _confirmDeleteAll(BuildContext context, WidgetRef ref) async {
    final total = ref.read(historyControllerProvider).total;
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('¿Borrar todo el historial?'),
        content: Text(
          'Se borran $total dictados y no se pueden recuperar. Lo que has '
          'dictado desaparece; lo que Pronto ha aprendido de ti se queda.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(ctx).pop(false),
            child: const Text('Cancelar'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(ctx).pop(true),
            child: const Text('Borrar todo'),
          ),
        ],
      ),
    );
    if (ok != true || !context.mounted) return;

    final borradas =
        await ref.read(historyControllerProvider.notifier).deleteAll();
    if (!context.mounted) return;
    ScaffoldMessenger.of(context)
      ..clearSnackBars()
      ..showSnackBar(
        SnackBar(
          content: Text('Historial borrado: $borradas dictados.'),
          behavior: SnackBarBehavior.floating,
        ),
      );
  }

  /// Exporta lo que cumple el filtro actual, avisando antes de lo que lleva
  /// dentro el fichero.
  Future<void> _export(
    BuildContext context,
    WidgetRef ref, {
    required bool json,
  }) async {
    final repo = ref.read(historyRepositoryProvider);
    if (repo == null) return;
    final state = ref.read(historyControllerProvider);

    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('Antes de guardar el fichero'),
        content: Text(exportWarning(state.total)),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(ctx).pop(false),
            child: const Text('Cancelar'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(ctx).pop(true),
            child: const Text('Guardar'),
          ),
        ],
      ),
    );
    if (ok != true) return;

    try {
      final entries = await repo.all(state.query);
      // A Documentos, NO a la carpeta de datos de la app: `data_backup` copia
      // esa carpeta entera antes de cada migración y esas copias no caducan
      // (ADR 0008). Un CSV en claro allí dentro se replicaría solo y quedaría
      // fuera de la Retención — justo lo que el ADR 0009 viene a evitar.
      final dir = await getApplicationDocumentsDirectory();
      final sello = DateTime.now()
          .toIso8601String()
          .substring(0, 19)
          .replaceAll(':', '-');
      final file = File(
        p.join(dir.path, 'historial-$sello.${json ? 'json' : 'csv'}'),
      );
      await file.writeAsString(
        json ? toJson(entries) : toCsv(entries),
        flush: true,
      );
      if (!context.mounted) return;
      ScaffoldMessenger.of(context)
        ..clearSnackBars()
        ..showSnackBar(
          SnackBar(
            content: Text('Guardado en ${file.path}'),
            behavior: SnackBarBehavior.floating,
            action: SnackBarAction(
              label: 'Copiar ruta',
              onPressed: () => Clipboard.setData(
                ClipboardData(text: file.path),
              ),
            ),
          ),
        );
    } catch (e) {
      if (!context.mounted) return;
      ScaffoldMessenger.of(context)
        ..clearSnackBars()
        ..showSnackBar(
          const SnackBar(
            content: Text('No se pudo guardar el fichero.'),
            behavior: SnackBarBehavior.floating,
          ),
        );
    }
  }
}

/// La franja de números de arriba. Responde al MISMO filtro que la lista.
class _StatsBar extends StatelessWidget {
  final HistoryState state;

  const _StatsBar({required this.state});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final s = state.stats;
    final ahorro = timeSaved(
      words: s.words,
      spoken: s.spoken,
      processing: s.processing,
    );

    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 10, 16, 10),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          // En horizontal, porque en una ventana estrecha las cifras no caben y
          // un Row que desborda no encoge: recorta y deja de enseñar números.
          SingleChildScrollView(
            scrollDirection: Axis.horizontal,
            child: Row(
              children: [
                _Cifra(label: 'Dictados', value: '${s.total}'),
                _Cifra(label: 'Palabras', value: '${s.words}'),
                _Cifra(label: 'Días con dictado', value: '${s.perDay.length}'),
                _Cifra(
                  label: 'Tiempo ahorrado (estimado)',
                  value: _duracion(ahorro),
                ),
                if (s.costUsd > 0)
                  _Cifra(label: 'Gasto de nube', value: _usd(s.costUsd)),
                if (s.perApp.isNotEmpty)
                  _Cifra(
                    label: 'Donde más escribes',
                    value: _top(s.perApp),
                  ),
                if (s.perMode.isNotEmpty)
                  _Cifra(label: 'Por Modo', value: _top(s.perMode)),
              ],
            ),
          ),
          if (s.perDay.isNotEmpty) _PorDia(perDay: s.perDay),
          const SizedBox(height: 4),
          // La cuenta del «tiempo ahorrado», a la vista y no en una ayuda
          // emergente: una cifra sin su fórmula se lee como una medida, y es
          // una estimación.
          Text(
            'Tiempo ahorrado = $kTimeSavedFormula',
            style: theme.textTheme.labelSmall?.copyWith(
              color: theme.colorScheme.outline,
            ),
          ),
        ],
      ),
    );
  }

  static String _top(Map<String, int> m) {
    final orden = m.entries.toList()
      ..sort((a, b) => b.value.compareTo(a.value));
    return orden.take(2).map((e) => '${e.key} (${e.value})').join(' · ');
  }
}

/// Dictados por día, en barras. Es la cifra que pide el ticket («dictados por
/// día»), no una media: lo que se ve de un vistazo es si dictas a diario o a
/// rachas, y eso una media lo esconde.
class _PorDia extends StatelessWidget {
  final Map<String, int> perDay;

  /// Cuántos días se pintan. Dos semanas caben sin apretar y son el tramo en
  /// el que uno reconoce su propia semana.
  static const int _dias = 14;

  const _PorDia({required this.perDay});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final claves = perDay.keys.toList()..sort();
    final ultimos = claves.length <= _dias
        ? claves
        : claves.sublist(claves.length - _dias);
    final maximo = ultimos.map((k) => perDay[k]!).reduce((a, b) => a > b ? a : b);

    return Padding(
      padding: const EdgeInsets.only(top: 8),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          for (final dia in ultimos)
            Tooltip(
              message: '$dia · ${perDay[dia]} dictados',
              child: Container(
                width: 14,
                height: 6 + 28 * (perDay[dia]! / maximo),
                margin: const EdgeInsets.only(right: 3),
                decoration: BoxDecoration(
                  color: theme.colorScheme.primary.withValues(alpha: 0.65),
                  borderRadius: const BorderRadius.vertical(
                    top: Radius.circular(2),
                  ),
                ),
              ),
            ),
          const SizedBox(width: 10),
          Padding(
            padding: const EdgeInsets.only(bottom: 2),
            child: Text(
              ultimos.length == 1
                  ? 'dictados el ${ultimos.first}'
                  : 'dictados por día, del ${ultimos.first} al ${ultimos.last}',
              style: theme.textTheme.labelSmall?.copyWith(
                color: theme.colorScheme.outline,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _Cifra extends StatelessWidget {
  final String label;
  final String value;

  const _Cifra({required this.label, required this.value});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.only(right: 28),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(value, style: theme.textTheme.titleMedium),
          Text(
            label,
            style: theme.textTheme.bodySmall?.copyWith(
              color: theme.colorScheme.onSurfaceVariant,
            ),
          ),
        ],
      ),
    );
  }
}

/// Panel izquierdo: búsqueda, filtros y lista.
class _Lista extends ConsumerStatefulWidget {
  final HistoryState state;

  const _Lista({required this.state});

  @override
  ConsumerState<_Lista> createState() => _ListaState();
}

class _ListaState extends ConsumerState<_Lista> {
  final ScrollController _scroll = ScrollController();

  @override
  void initState() {
    super.initState();
    _scroll.addListener(() {
      if (_scroll.position.pixels >=
          _scroll.position.maxScrollExtent - 200) {
        ref.read(historyControllerProvider.notifier).loadMore();
      }
    });
  }

  @override
  void dispose() {
    _scroll.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final state = widget.state;
    final ctrl = ref.read(historyControllerProvider.notifier);
    final theme = Theme.of(context);

    return Column(
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(12, 12, 12, 8),
          child: TextField(
            decoration: const InputDecoration(
              isDense: true,
              prefixIcon: Icon(Icons.search, size: 18),
              hintText: 'Buscar en lo dictado…',
              border: OutlineInputBorder(),
            ),
            onChanged: ctrl.search,
          ),
        ),
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 12),
          // En Wrap y no en Row: el panel mide 340 px y los filtros crecen
          // (llegan los Modos con el PRNT-3). Un Row que no cabe no encoge,
          // recorta — y un filtro a medio pintar es un filtro que no se usa.
          child: Wrap(
            spacing: 8,
            runSpacing: 4,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              FilterChip(
                label: const Text('Solo fallos'),
                selected: state.query.onlyFailed,
                onSelected: (v) => ctrl.setQuery(
                  state.query.copyWith(onlyFailed: v),
                ),
              ),
              _FiltroFecha(query: state.query, onChanged: ctrl.setQuery),
              // El desplegable de Modo solo aparece cuando hay Modos: hasta el
              // PRNT-3 no existen, y un filtro vacío es un control que miente.
              if (state.modes.isNotEmpty)
                DropdownButton<String?>(
                  value: state.query.mode,
                  hint: const Text('Todos los Modos'),
                  items: [
                    const DropdownMenuItem<String?>(
                      child: Text('Todos los Modos'),
                    ),
                    for (final m in state.modes)
                      DropdownMenuItem<String?>(value: m, child: Text(m)),
                  ],
                  onChanged: (v) => ctrl.setQuery(
                    v == null
                        ? state.query.copyWith(clearMode: true)
                        : state.query.copyWith(mode: v),
                  ),
                ),
            ],
          ),
        ),
        const SizedBox(height: 8),
        Expanded(
          child: state.loading
              ? const Center(child: CircularProgressIndicator())
              : state.entries.isEmpty
                  ? _Aviso(
                      icon: Icons.history,
                      title: state.query.isEmpty
                          ? 'Todavía no has dictado nada'
                          : 'Nada que cumpla eso',
                      body: state.query.isEmpty
                          ? 'Cada dictado que hagas aparecerá aquí, con lo que '
                              'oyó el motor y lo que se acabó pegando.'
                          : 'Prueba con menos filtros o con otra palabra.',
                    )
                  : ListView.builder(
                      controller: _scroll,
                      itemCount: state.entries.length + (state.hasMore ? 1 : 0),
                      itemBuilder: (context, i) {
                        if (i >= state.entries.length) {
                          return const Padding(
                            padding: EdgeInsets.all(16),
                            child: Center(
                              child: SizedBox(
                                width: 18,
                                height: 18,
                                child: CircularProgressIndicator(strokeWidth: 2),
                              ),
                            ),
                          );
                        }
                        final e = state.entries[i];
                        return _Fila(
                          entry: e,
                          selected: state.selected?.id == e.id,
                          onTap: () => ctrl.select(e.id),
                        );
                      },
                    ),
        ),
        const Divider(height: 1),
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 8),
          child: Align(
            alignment: Alignment.centerLeft,
            child: Text(
              '${state.entries.length} de ${state.total} dictados',
              style: theme.textTheme.bodySmall,
            ),
          ),
        ),
      ],
    );
  }
}

/// Filtro por rango de fechas.
///
/// El extremo final se estira al FIN del día elegido: quien pide «hasta el 5»
/// quiere lo del día 5, y `to` a medianoche se dejaría fuera el día entero.
class _FiltroFecha extends StatelessWidget {
  final HistoryQuery query;
  final void Function(HistoryQuery) onChanged;

  const _FiltroFecha({required this.query, required this.onChanged});

  @override
  Widget build(BuildContext context) {
    final activo = query.from != null || query.to != null;
    return ActionChip(
      avatar: const Icon(Icons.calendar_today, size: 14),
      label: Text(activo ? _rotulo() : 'Fechas'),
      onPressed: () => _elegir(context),
    );
  }

  String _rotulo() {
    String corto(DateTime? d) => d == null
        ? '…'
        : '${d.day.toString().padLeft(2, '0')}/'
            '${d.month.toString().padLeft(2, '0')}';
    return '${corto(query.from)} – ${corto(query.to)}';
  }

  Future<void> _elegir(BuildContext context) async {
    final hoy = DateTime.now();
    final rango = await showDateRangePicker(
      context: context,
      firstDate: DateTime(2025),
      lastDate: DateTime(hoy.year + 1),
      initialDateRange: query.from != null && query.to != null
          ? DateTimeRange(start: query.from!, end: query.to!)
          : null,
      helpText: 'Fechas del historial',
      cancelText: 'Quitar filtro',
      confirmText: 'Filtrar',
    );
    if (rango == null) {
      // «Cancelar» aquí significa quitar el filtro: es la única forma de
      // volver a verlo todo sin cerrar la pantalla.
      onChanged(query.copyWith(clearFrom: true, clearTo: true));
      return;
    }
    onChanged(
      query.copyWith(
        from: DateTime(
          rango.start.year,
          rango.start.month,
          rango.start.day,
        ),
        to: DateTime(
          rango.end.year,
          rango.end.month,
          rango.end.day,
          23,
          59,
          59,
          999,
        ),
      ),
    );
  }
}

class _Fila extends StatelessWidget {
  final DictationEntry entry;
  final bool selected;
  final VoidCallback onTap;

  const _Fila({
    required this.entry,
    required this.selected,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Material(
      color: selected
          ? theme.colorScheme.primaryContainer.withValues(alpha: 0.45)
          : Colors.transparent,
      child: InkWell(
        onTap: onTap,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Text(
                    _hora(entry.at),
                    style: theme.textTheme.labelSmall,
                  ),
                  if (entry.mode != null) ...[
                    const Text(' · '),
                    Text(entry.mode!, style: theme.textTheme.labelSmall),
                  ],
                  const Spacer(),
                  if (entry.failed)
                    Icon(
                      Icons.error_outline,
                      size: 14,
                      color: theme.colorScheme.error,
                    )
                  else if (entry.appName != null)
                    Text(entry.appName!, style: theme.textTheme.labelSmall),
                ],
              ),
              const SizedBox(height: 2),
              Text(
                entry.pastedText,
                maxLines: 2,
                overflow: TextOverflow.ellipsis,
                style: theme.textTheme.bodyMedium,
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// Panel derecho: el dictado abierto, con el crudo y el final enfrentados.
class _Detalle extends ConsumerWidget {
  final DictationEntry? entry;

  const _Detalle({required this.entry});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final e = entry;
    final theme = Theme.of(context);
    if (e == null) {
      return const _Aviso(
        icon: Icons.article_outlined,
        title: 'Elige un dictado',
        body: 'A la izquierda está todo lo que has dictado. Aquí se ve lo que '
            'oyó el motor y lo que se acabó pegando.',
      );
    }

    return ListView(
      padding: const EdgeInsets.all(20),
      children: [
        Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(_fechaLarga(e.at), style: theme.textTheme.titleMedium),
                  if (e.appName != null || e.windowTitle != null)
                    Text(
                      [e.appName, e.windowTitle]
                          .whereType<String>()
                          .join(' · '),
                      style: theme.textTheme.bodySmall,
                    ),
                ],
              ),
            ),
            TextButton.icon(
              onPressed: () => _copiar(context, e.pastedText),
              icon: const Icon(Icons.copy, size: 16),
              label: const Text('Copiar'),
            ),
            TextButton.icon(
              onPressed: () => _prepararParaPegar(context, e.pastedText),
              icon: const Icon(Icons.keyboard_return, size: 16),
              label: const Text('Preparar para pegar'),
            ),
            TextButton.icon(
              onPressed: () => _corregir(context, ref, e),
              icon: const Icon(Icons.edit_outlined, size: 16),
              label: const Text('Corregir'),
            ),
            IconButton(
              tooltip: 'Borrar este dictado',
              onPressed: () => _borrar(context, ref, e),
              icon: Icon(
                Icons.delete_outline,
                color: theme.colorScheme.error,
              ),
            ),
          ],
        ),
        const SizedBox(height: 16),
        if (e.failed)
          Container(
            width: double.infinity,
            margin: const EdgeInsets.only(bottom: 16),
            padding: const EdgeInsets.all(12),
            decoration: BoxDecoration(
              border: Border.all(color: theme.colorScheme.error),
              borderRadius: BorderRadius.circular(8),
            ),
            // El aviso es una cadena NUESTRA, escrita al formatear. Nunca la
            // respuesta cruda del proveedor.
            child: Text(
              e.formatNotice!,
              style: TextStyle(color: theme.colorScheme.error),
            ),
          ),
        const _Rotulo('Lo que oyó el motor'),
        _Caja(child: SelectableText(e.raw)),
        const SizedBox(height: 14),
        _Rotulo(
          e.finalText.isEmpty
              ? 'Lo que se pegó (el crudo, sin formatear)'
              : 'Lo que se pegó',
        ),
        _Caja(
          child: e.finalText.isEmpty
              ? SelectableText(e.raw)
              : _DiffFinal(raw: e.raw, finalText: e.finalText),
        ),
        const SizedBox(height: 20),
        _Campo('Modo', e.mode ?? 'sin Modo (dictado antes de que existieran)'),
        _Campo('Motor de voz', _conMs(e.engine ?? '—', e.transcribe)),
        _Campo('Formateo', _formateo(e)),
        _Campo('Duración de lo hablado', _duracion(e.spoken ?? Duration.zero)),
        _Campo('Coste', _coste(e)),
        _Campo('Palabras', '${e.wordCount}'),
      ],
    );
  }

  /// El coste, distinguiendo los tres casos que NO son lo mismo: no salió del
  /// equipo (0 de verdad), salió y costó, o salió y no se llegó a apuntar lo
  /// que costó. Enseñar «0 $» en el tercero sería decir que fue gratis.
  static String _coste(DictationEntry e) {
    final usd = e.costUsd;
    if (usd != null) return usd == 0 ? r'0 $' : _usd(usd);
    final fueANube = e.formatPath == FormatPath.cloud.name;
    return fueANube ? 'no se apuntó' : r'0 $ (en el equipo)';
  }

  static String _formateo(DictationEntry e) {
    if (e.formatPath == null || e.formatPath == FormatPath.none.name) {
      return 'sin formateo';
    }
    final donde =
        e.formatPath == FormatPath.cloud.name ? 'nube' : 'en el equipo';
    return _conMs('${e.formatModel ?? '—'} ($donde)', e.format);
  }

  static String _conMs(String base, Duration? d) =>
      d == null || d == Duration.zero ? base : '$base · ${d.inMilliseconds} ms';

  Future<void> _copiar(BuildContext context, String text) async {
    await Clipboard.setData(ClipboardData(text: text));
    if (!context.mounted) return;
    ScaffoldMessenger.of(context)
      ..clearSnackBars()
      ..showSnackBar(
        const SnackBar(
          content: Text('Copiado.'),
          behavior: SnackBarBehavior.floating,
        ),
      );
  }

  /// Deja el dictado listo para pegarlo donde el usuario quiera.
  ///
  /// El ticket pedía «volver a pegar en la app con foco», y mientras esta
  /// ventana está delante **el foco lo tiene Pronto**: inyectar aquí escribiría
  /// dentro de la propia app. Robar el foco a la ventana anterior para pegar en
  /// ella es una función de verdad, con su riesgo, y no cabe en este ticket. Así
  /// que se hace lo que la app ya hace cuando la inserción se bloquea —dejarlo
  /// en el portapapeles y decirlo— y el botón se llama por lo que hace.
  Future<void> _prepararParaPegar(BuildContext context, String text) async {
    await Clipboard.setData(ClipboardData(text: text));
    if (!context.mounted) return;
    ScaffoldMessenger.of(context)
      ..clearSnackBars()
      ..showSnackBar(
        const SnackBar(
          content: Text(
            'Listo en el portapapeles: cambia a tu app y pulsa Ctrl+V.',
          ),
          behavior: SnackBarBehavior.floating,
        ),
      );
  }

  Future<void> _borrar(
    BuildContext context,
    WidgetRef ref,
    DictationEntry e,
  ) async {
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('¿Borrar este dictado?'),
        content: Text(
          'Se borra y no se puede recuperar:\n\n«${e.pastedText}»',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(ctx).pop(false),
            child: const Text('Cancelar'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(ctx).pop(true),
            child: const Text('Borrar'),
          ),
        ],
      ),
    );
    if (ok != true) return;
    await ref.read(historyControllerProvider.notifier).deleteEntry(e.id);
  }

  Future<void> _corregir(
    BuildContext context,
    WidgetRef ref,
    DictationEntry e,
  ) async {
    final ctrl = TextEditingController(text: e.pastedText);
    final corregido = await showDialog<String>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('Corregir el dictado'),
        content: SizedBox(
          width: 520,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                'Lo que oyó el motor: ${e.raw}',
                style: Theme.of(ctx).textTheme.bodySmall,
              ),
              const SizedBox(height: 12),
              TextField(
                controller: ctrl,
                maxLines: 6,
                autofocus: true,
                decoration: const InputDecoration(border: OutlineInputBorder()),
              ),
              const SizedBox(height: 8),
              Text(
                'Pronto aprende de la diferencia: la próxima vez que oiga eso '
                'mismo, lo escribirá así.',
                style: Theme.of(ctx).textTheme.bodySmall,
              ),
            ],
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(ctx).pop(),
            child: const Text('Cancelar'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(ctx).pop(ctrl.text),
            child: const Text('Guardar'),
          ),
        ],
      ),
    );
    ctrl.dispose();
    if (corregido == null) return;
    await ref.read(historyControllerProvider.notifier).correct(e, corregido);
  }
}

/// El texto final con lo que puso el Formateo marcado.
class _DiffFinal extends StatelessWidget {
  final String raw;
  final String finalText;

  const _DiffFinal({required this.raw, required this.finalText});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final ops = alignWords(tokenizeWords(raw), tokenizeWords(finalText));
    final spans = <TextSpan>[];
    for (final op in ops) {
      switch (op.type) {
        case DiffOpType.equal:
          spans.add(TextSpan(text: '${op.target} '));
        case DiffOpType.substitute:
        case DiffOpType.insert:
          spans.add(
            TextSpan(
              text: '${op.target} ',
              style: TextStyle(
                backgroundColor:
                    theme.colorScheme.primaryContainer.withValues(alpha: 0.6),
                fontWeight: FontWeight.w600,
              ),
            ),
          );
        case DiffOpType.delete:
          // Lo que el Formateo se dejó por el camino: tachado, para que se vea
          // que ha desaparecido en vez de no verse nada.
          spans.add(
            TextSpan(
              text: '${op.source} ',
              style: TextStyle(
                decoration: TextDecoration.lineThrough,
                color: theme.colorScheme.outline,
              ),
            ),
          );
      }
    }
    return SelectableText.rich(
      TextSpan(style: theme.textTheme.bodyMedium, children: spans),
    );
  }
}

class _Rotulo extends StatelessWidget {
  final String text;

  const _Rotulo(this.text);

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.only(bottom: 6),
      child: Text(
        text.toUpperCase(),
        style: theme.textTheme.labelSmall?.copyWith(
          letterSpacing: 0.6,
          color: theme.colorScheme.onSurfaceVariant,
        ),
      ),
    );
  }
}

class _Caja extends StatelessWidget {
  final Widget child;

  const _Caja({required this.child});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        border: Border.all(color: theme.colorScheme.outlineVariant),
        borderRadius: BorderRadius.circular(8),
      ),
      child: child,
    );
  }
}

class _Campo extends StatelessWidget {
  final String label;
  final String value;

  const _Campo(this.label, this.value);

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 5),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            width: 200,
            child: Text(
              label,
              style: theme.textTheme.bodySmall?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
          ),
          Expanded(child: Text(value, style: theme.textTheme.bodyMedium)),
        ],
      ),
    );
  }
}

class _Aviso extends StatelessWidget {
  final IconData icon;
  final String title;
  final String body;

  const _Aviso({required this.icon, required this.title, required this.body});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(32),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(icon, size: 40, color: theme.colorScheme.outline),
            const SizedBox(height: 12),
            Text(title, style: theme.textTheme.titleMedium),
            const SizedBox(height: 6),
            Text(
              body,
              textAlign: TextAlign.center,
              style: theme.textTheme.bodySmall,
            ),
          ],
        ),
      ),
    );
  }
}

String _hora(DateTime d) =>
    '${d.hour.toString().padLeft(2, '0')}:${d.minute.toString().padLeft(2, '0')}';

String _fechaLarga(DateTime d) {
  String dos(int n) => n.toString().padLeft(2, '0');
  return '${dos(d.day)}/${dos(d.month)}/${d.year} ${_hora(d)}';
}

String _duracion(Duration d) {
  if (d.inHours > 0) return '${d.inHours} h ${d.inMinutes % 60} min';
  if (d.inMinutes > 0) return '${d.inMinutes} min ${d.inSeconds % 60} s';
  return '${d.inSeconds} s';
}

String _usd(double usd) =>
    '${usd.toStringAsFixed(4).replaceAll('.', ',')} \$';
