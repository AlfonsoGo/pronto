import 'dart:async';
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../../core/config.dart';
import '../settings/settings_controller.dart';
import 'cloud_budget.dart';

/// El libro de cuentas del gasto de nube, persistido.
///
/// Vive aparte de los ajustes porque no es una preferencia: es un contador que
/// escribe la app sola. Los TOPES sí son ajustes (los pone el usuario), y se
/// leen de [settingsProvider] en cada comprobación, para que bajar el tope
/// tenga efecto en el dictado siguiente y no al reiniciar.
///
/// ⚠ **[admit] pregunta y [record] apunta después: entre las dos hay una
/// ventana.** No se reserva el gasto por adelantado porque el pipeline del
/// dictado es SERIAL por construcción — `DictationController.startRecording`
/// se planta si `DictationState.isBusy`, y `sendingToCloud` cuenta como
/// ocupado (hay un test que lo fija). Sin dos dictados a la vez no hay dos
/// llamadas a la vez, y una reserva traería su propio lío: apuntar llamadas
/// fantasma cuando la petición no llega a salir. Si algún día el formateo se
/// pudiera lanzar desde dos sitios, esto pasa a ser una reserva de verdad.
final cloudSpendProvider = NotifierProvider<CloudSpendController, CloudSpend>(
  CloudSpendController.new,
);

class CloudSpendController extends Notifier<CloudSpend>
    implements CloudBudget {
  static const String _prefsKey = 'cloud_spend';

  /// Ya se ha intentado leer de disco lo gastado. Hasta entonces el libro está
  /// en blanco y CUALQUIER llamada parecería caber en el tope, así que se
  /// corta. Se marca en el `finally`: si la lectura falla no se puede saber lo
  /// gastado, pero dejar la nube inutilizable para siempre sería peor.
  bool _cargado = false;

  @override
  CloudSpend build() {
    scheduleMicrotask(_load);
    return CloudSpend.empty(DateTime.now());
  }

  @override
  BudgetVerdict admit(String text, {required String model}) {
    if (!_cargado) return BudgetVerdict.notReady;

    final ahora = DateTime.now();
    final s = ref.read(settingsProvider);
    final caps = CloudCaps(
      dayUsd: s.cloudDayCapUsd,
      monthUsd: s.cloudMonthCapUsd,
      maxChars: AppConfig.cloudMaxChars,
    );

    // El libro se pone al día ANTES de juzgar: si acaba de cambiar el día, el
    // contador que ve el usuario y el que mira el tope son el mismo.
    final alDia = state.rollTo(ahora);
    if (alDia != state) {
      state = alDia;
      unawaited(_save());
    }

    return judgeCloudCall(
      text: text,
      model: model,
      spend: state,
      caps: caps,
      now: ahora,
    );
  }

  @override
  Future<void> record(double costUsd) async {
    state = state.plus(costUsd, DateTime.now());
    await _save();
  }

  /// Pone los contadores a cero (botón «Poner a cero» de Ajustes). No es un
  /// tope: solo borra la cuenta que lleva Pronto.
  Future<void> reset() async {
    state = CloudSpend.empty(DateTime.now());
    await _save();
  }

  Future<void> _load() async {
    try {
      final prefs = await SharedPreferences.getInstance();
      final raw = prefs.getString(_prefsKey);
      if (raw == null || raw.isEmpty) return;
      final guardado = CloudSpend.fromJson(
        jsonDecode(raw) as Map<String, dynamic>,
        DateTime.now(),
      );
      // Si entre el arranque y esta carga ya se apuntó una llamada, no se
      // pierde: se suman las dos. Es dinero; no se redondea a la baja.
      state = (state.dayCalls == 0 && state.monthCalls == 0)
          ? guardado
          : _sumar(guardado, state);
    } catch (e) {
      debugPrint('No se pudo cargar el gasto de nube: $e');
    } finally {
      _cargado = true;
    }
  }

  Future<void> _save() async {
    try {
      final prefs = await SharedPreferences.getInstance();
      await prefs.setString(_prefsKey, jsonEncode(state.toJson()));
    } catch (e) {
      debugPrint('No se pudo guardar el gasto de nube: $e');
    }
  }

  static CloudSpend _sumar(CloudSpend a, CloudSpend b) => CloudSpend(
        day: b.day,
        dayUsd: a.dayUsd + b.dayUsd,
        dayCalls: a.dayCalls + b.dayCalls,
        month: b.month,
        monthUsd: a.monthUsd + b.monthUsd,
        monthCalls: a.monthCalls + b.monthCalls,
      );
}
