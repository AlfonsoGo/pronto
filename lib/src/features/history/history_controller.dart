import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../platform/platform_services.dart';
import 'dictation_entry.dart';
import 'history_repository.dart';

/// Cuántas entradas se traen de golpe. La lista pide más al llegar al final:
/// con 5.000 dictados, cargarlos todos para enseñar veinte es lo que hace que
/// la pantalla tarde en abrir.
const int kHistoryPageSize = 50;

/// Lo que la pantalla del Historial está enseñando ahora mismo.
@immutable
class HistoryState {
  final HistoryQuery query;

  /// Las entradas cargadas hasta ahora (las páginas pedidas, en orden).
  final List<DictationEntry> entries;

  /// Cuántas cumplen el filtro en total (para «N de M» y para saber si queda
  /// algo por cargar).
  final int total;

  /// La entrada abierta en el panel de la derecha.
  final int? selectedId;

  /// Cuántas filas se han pedido al repositorio. NO es `entries.length`:
  /// borrar una entrada acorta la lista pero no deshace la lectura, y usar la
  /// longitud como desplazamiento se saltaría una fila en la página siguiente.
  final int fetched;

  /// Los Modos que existen en el historial. Vacío hasta que existan los Modos.
  final List<String> modes;

  final HistoryStats stats;

  /// Primera carga en curso: la pantalla enseña el hueco, no una lista vacía
  /// que parecería «no has dictado nunca».
  final bool loading;

  /// Cargando la siguiente página al final de la lista.
  final bool loadingMore;

  /// El historial no está disponible (base sin abrir). Es distinto de estar
  /// vacío y se dice distinto.
  final bool unavailable;

  const HistoryState({
    this.query = const HistoryQuery(),
    this.entries = const [],
    this.total = 0,
    this.fetched = 0,
    this.selectedId,
    this.modes = const [],
    this.stats = HistoryStats.empty,
    this.loading = true,
    this.loadingMore = false,
    this.unavailable = false,
  });

  /// La entrada abierta, o la primera si la seleccionada ya no está.
  DictationEntry? get selected {
    if (entries.isEmpty) return null;
    for (final e in entries) {
      if (e.id == selectedId) return e;
    }
    return entries.first;
  }

  bool get hasMore => fetched < total;

  HistoryState copyWith({
    HistoryQuery? query,
    List<DictationEntry>? entries,
    int? total,
    int? fetched,
    int? selectedId,
    bool clearSelection = false,
    List<String>? modes,
    HistoryStats? stats,
    bool? loading,
    bool? loadingMore,
    bool? unavailable,
  }) =>
      HistoryState(
        query: query ?? this.query,
        entries: entries ?? this.entries,
        total: total ?? this.total,
        fetched: fetched ?? this.fetched,
        selectedId: clearSelection ? null : (selectedId ?? this.selectedId),
        modes: modes ?? this.modes,
        stats: stats ?? this.stats,
        loading: loading ?? this.loading,
        loadingMore: loadingMore ?? this.loadingMore,
        unavailable: unavailable ?? this.unavailable,
      );
}

final historyControllerProvider =
    NotifierProvider<HistoryController, HistoryState>(HistoryController.new);

class HistoryController extends Notifier<HistoryState> {
  /// Cada búsqueda que entra invalida la anterior: si se teclea rápido, la
  /// respuesta lenta de una consulta vieja no puede pisar a la nueva.
  int _generation = 0;

  Timer? _debounce;

  @override
  HistoryState build() {
    ref.onDispose(() => _debounce?.cancel());
    scheduleMicrotask(refresh);
    return const HistoryState();
  }

  HistoryRepository? get _repo => ref.read(historyRepositoryProvider);

  /// Vuelve a cargar la primera página, el total, las estadísticas y los Modos.
  Future<void> refresh() async {
    final repo = _repo;
    if (repo == null) {
      state = state.copyWith(loading: false, unavailable: true);
      return;
    }
    final gen = ++_generation;
    state = state.copyWith(loading: true, unavailable: false);
    try {
      final query = state.query;
      final entries = await repo.page(query, limit: kHistoryPageSize);
      final total = await repo.count(query);
      final stats = await repo.stats(query);
      final modes = await repo.distinctModes();
      if (gen != _generation) return;
      state = state.copyWith(
        entries: entries,
        total: total,
        fetched: entries.length,
        stats: stats,
        modes: modes,
        loading: false,
      );
    } catch (e) {
      if (gen != _generation) return;
      debugPrint('[TF] historial: no se pudo leer: $e');
      state = state.copyWith(loading: false, unavailable: true);
    }
  }

  /// Trae la siguiente página al llegar al final de la lista.
  Future<void> loadMore() async {
    if (state.loadingMore || !state.hasMore) return;
    final repo = _repo;
    if (repo == null) return;
    final gen = _generation;
    state = state.copyWith(loadingMore: true);
    try {
      final more = await repo.page(
        state.query,
        limit: kHistoryPageSize,
        offset: state.fetched,
      );
      if (gen != _generation) return;
      state = state.copyWith(
        entries: [...state.entries, ...more],
        fetched: state.fetched + more.length,
        loadingMore: false,
      );
    } catch (e) {
      if (gen != _generation) return;
      debugPrint('[TF] historial: no se pudo cargar más: $e');
      state = state.copyWith(loadingMore: false);
    }
  }

  /// Cambia el texto de búsqueda. Espera a que se deje de teclear: una consulta
  /// por pulsación sobre 5.000 filas se nota en la mano.
  void search(String text) {
    _debounce?.cancel();
    _debounce = Timer(const Duration(milliseconds: 220), () {
      setQuery(state.query.copyWith(text: text));
    });
  }

  void setQuery(HistoryQuery query) {
    state = state.copyWith(
      query: query,
      entries: const [],
      total: 0,
      fetched: 0,
    );
    unawaited(refresh());
  }

  void select(int id) => state = state.copyWith(selectedId: id);

  /// Borra una entrada. Se quita de la lista sin recargar entera para no
  /// mover lo que el usuario está mirando.
  Future<void> deleteEntry(int id) async {
    final repo = _repo;
    if (repo == null) return;
    await repo.deleteEntry(id);
    final quedan = [...state.entries]..removeWhere((e) => e.id == id);
    state = state.copyWith(
      entries: quedan,
      total: state.total > 0 ? state.total - 1 : 0,
      // La fila leída ya no está: la ventana leída encoge con ella, o la
      // siguiente página empezaría una fila más allá y se perdería un dictado.
      fetched: state.fetched > 0 ? state.fetched - 1 : 0,
      clearSelection: state.selectedId == id,
    );
    unawaited(_refreshStats());
  }

  /// Borra el historial entero.
  ///
  /// Devuelve lo que el usuario TENÍA DELANTE, no las filas que caen: la tabla
  /// guarda además las pruebas del asistente, que él nunca vio. Decirle «se han
  /// borrado 40» cuando en pantalla ponía 37 le hace pensar que hay algo suyo
  /// que no se le estaba enseñando.
  Future<int> deleteAll() async {
    final repo = _repo;
    if (repo == null) return 0;
    final visibles = state.total;
    await repo.deleteAll();
    state = state.copyWith(
      entries: const [],
      total: 0,
      fetched: 0,
      stats: HistoryStats.empty,
      clearSelection: true,
    );
    return visibles;
  }

  /// Corrige a mano el texto final de una entrada.
  ///
  /// Hace las dos cosas que pide el ticket: arregla la fila —que si no seguiría
  /// enseñando el texto malo como si fuera lo bueno— y le pasa la corrección al
  /// aprendizaje, que es lo que hace que la próxima vez salga bien.
  Future<void> correct(DictationEntry entry, String corrected) async {
    final repo = _repo;
    if (repo == null) return;
    final limpio = corrected.trim();
    if (limpio.isEmpty || limpio == entry.finalText) return;

    await repo.updateFinalText(entry.id, limpio);

    // De una TRADUCCIÓN no se aprende: el crudo está en un idioma y lo pegado
    // en otro, así que su diferencia no son correcciones sino la traducción
    // entera. Meterla en el diccionario lo envenena. Misma guarda que
    // `DictationController.submitCorrection`.
    final learning = ref.read(learningServiceProvider);
    if (!entry.translated && entry.raw.isNotEmpty && entry.raw != limpio) {
      await learning.recordEdit(entry.raw, limpio);
      await learning.refresh();
    }

    state = state.copyWith(
      entries: [
        for (final e in state.entries)
          if (e.id == entry.id) e.copyWith(finalText: limpio) else e,
      ],
    );
    unawaited(_refreshStats());
  }

  Future<void> _refreshStats() async {
    final repo = _repo;
    if (repo == null) return;
    try {
      final stats = await repo.stats(state.query);
      state = state.copyWith(stats: stats);
    } catch (_) {
      // Las estadísticas son un extra de la pantalla: que no se puedan
      // recalcular no justifica romperle la lista al usuario.
    }
  }
}

