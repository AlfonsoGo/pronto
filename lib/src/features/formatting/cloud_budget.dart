import 'package:flutter/foundation.dart';

import 'cloud_cost.dart';

/// El guardia del gasto de nube, y el libro de cuentas que mira.
///
/// Por qué vive DENTRO de Pronto y no basta con el vigilante del VPS: un mes
/// entero de dictado cuesta céntimos, así que un tope de 2 $/día no puede
/// detectar nada — un bucle se lo gasta entero antes de que salte, y el aviso
/// llega cuando el dinero ya no está. El freno tiene que estar donde se pulsa
/// el botón.
///
/// Se comprueba **al ENTRAR**, nunca a mitad: al gasto ya apuntado se le suma
/// lo que va a costar ESTA llamada, y si con eso se pasa del tope, la llamada
/// no sale. Un tope que se comprueba después es un tope que se cruza.

/// Qué dijo el guardia. Solo [ok] deja salir la petición.
enum BudgetVerdict {
  ok,

  /// El dictado es más largo de lo que se manda fuera de una sentada.
  textTooLong,

  /// Con esta llamada se pasaría del tope de hoy.
  dayCapReached,

  /// Con esta llamada se pasaría del tope del mes.
  monthCapReached,

  /// Todavía no se ha leído de disco lo que se lleva gastado. Se corta: con el
  /// libro en blanco, cualquier llamada parecería caber.
  notReady,
}

extension BudgetVerdictNotice on BudgetVerdict {
  /// Qué contarle al usuario. Cadena NUESTRA, como el resto de avisos.
  String? get notice => switch (this) {
        BudgetVerdict.ok => null,
        BudgetVerdict.textTooLong =>
          'Sin formatear: el dictado es demasiado largo para mandarlo a la nube',
        BudgetVerdict.dayCapReached =>
          'Sin formatear: alcanzado el tope de gasto de hoy',
        BudgetVerdict.monthCapReached =>
          'Sin formatear: alcanzado el tope de gasto de este mes',
        BudgetVerdict.notReady =>
          'Sin formatear: el contador de gasto aún no está listo',
      };
}

/// Los topes. Se editan en Ajustes y son de la app, no del Modo: protegen la
/// cartera, y la cartera es la misma escribas donde escribas.
@immutable
class CloudCaps {
  final double dayUsd;
  final double monthUsd;

  /// Tope por dictado, en caracteres. Es el tope "por llamada" del ticket: sin
  /// él, un solo dictado disparatado se come el tope del día de una vez.
  final int maxChars;

  const CloudCaps({
    required this.dayUsd,
    required this.monthUsd,
    required this.maxChars,
  });
}

/// Lo gastado hasta ahora. El día y el mes se guardan como texto
/// (`2026-09-04`, `2026-09`) para que el corte sea el del calendario local, que
/// es el que entiende quien mira el contador.
@immutable
class CloudSpend {
  final String day;
  final double dayUsd;
  final int dayCalls;
  final String month;
  final double monthUsd;
  final int monthCalls;

  const CloudSpend({
    required this.day,
    required this.dayUsd,
    required this.dayCalls,
    required this.month,
    required this.monthUsd,
    required this.monthCalls,
  });

  factory CloudSpend.empty(DateTime now) => CloudSpend(
        day: dayKey(now),
        dayUsd: 0,
        dayCalls: 0,
        month: monthKey(now),
        monthUsd: 0,
        monthCalls: 0,
      );

  static String dayKey(DateTime t) => '${monthKey(t)}-'
      '${t.day.toString().padLeft(2, '0')}';

  static String monthKey(DateTime t) =>
      '${t.year.toString().padLeft(4, '0')}-'
      '${t.month.toString().padLeft(2, '0')}';

  /// Pone el libro al día de [now]: si cambió el día, el contador de hoy vuelve
  /// a cero; si cambió el mes, el del mes también. Es idempotente.
  CloudSpend rollTo(DateTime now) {
    final d = dayKey(now);
    final m = monthKey(now);
    if (d == day && m == month) return this;
    final mismoMes = m == month;
    return CloudSpend(
      day: d,
      dayUsd: d == day ? dayUsd : 0,
      dayCalls: d == day ? dayCalls : 0,
      month: m,
      monthUsd: mismoMes ? monthUsd : 0,
      monthCalls: mismoMes ? monthCalls : 0,
    );
  }

  /// Apunta una llamada de [usd]. Importes negativos se ignoran: el gasto no
  /// baja nunca (si el proveedor devolviera algo raro, no puede regalar tope).
  CloudSpend plus(double usd, DateTime now) {
    final base = rollTo(now);
    final coste = (usd.isFinite && usd > 0) ? usd : 0.0;
    return CloudSpend(
      day: base.day,
      dayUsd: base.dayUsd + coste,
      dayCalls: base.dayCalls + 1,
      month: base.month,
      monthUsd: base.monthUsd + coste,
      monthCalls: base.monthCalls + 1,
    );
  }

  Map<String, dynamic> toJson() => {
        'day': day,
        'dayUsd': dayUsd,
        'dayCalls': dayCalls,
        'month': month,
        'monthUsd': monthUsd,
        'monthCalls': monthCalls,
      };

  /// Reconstruye desde JSON. Cualquier campo ausente o absurdo cae a cero: un
  /// libro ilegible se lee como "no he gastado nada todavía", nunca como
  /// "puedo gastar sin límite".
  factory CloudSpend.fromJson(Map<String, dynamic> j, DateTime now) {
    double num_(Object? v) {
      final d = v is num ? v.toDouble() : null;
      return (d == null || !d.isFinite || d < 0) ? 0 : d;
    }

    int int_(Object? v) => (v is num && v.isFinite && v >= 0) ? v.toInt() : 0;

    final vacio = CloudSpend.empty(now);
    return CloudSpend(
      day: j['day'] as String? ?? vacio.day,
      dayUsd: num_(j['dayUsd']),
      dayCalls: int_(j['dayCalls']),
      month: j['month'] as String? ?? vacio.month,
      monthUsd: num_(j['monthUsd']),
      monthCalls: int_(j['monthCalls']),
    ).rollTo(now);
  }

  @override
  bool operator ==(Object other) =>
      other is CloudSpend &&
      other.day == day &&
      other.dayUsd == dayUsd &&
      other.dayCalls == dayCalls &&
      other.month == month &&
      other.monthUsd == monthUsd &&
      other.monthCalls == monthCalls;

  @override
  int get hashCode =>
      Object.hash(day, dayUsd, dayCalls, month, monthUsd, monthCalls);
}

/// La decisión, aislada de dónde se guarda el libro para poder probarla sola.
BudgetVerdict judgeCloudCall({
  required String text,
  required String model,
  required CloudSpend spend,
  required CloudCaps caps,
  required DateTime now,
}) {
  if (text.length > caps.maxChars) return BudgetVerdict.textTooLong;

  final alDia = spend.rollTo(now);
  final estimado = estimateCallCostUsd(model: model, textChars: text.length);

  // `>` y no `>=`: con el libro a cero y un tope de cero, la nube queda
  // apagada, que es lo que significa un tope de cero.
  if (alDia.dayUsd + estimado > caps.dayUsd) return BudgetVerdict.dayCapReached;
  if (alDia.monthUsd + estimado > caps.monthUsd) {
    return BudgetVerdict.monthCapReached;
  }
  return BudgetVerdict.ok;
}

/// Lo que [TextFormatter] necesita del guardia. Se le pregunta antes de mandar
/// nada y se le apunta lo gastado después.
abstract class CloudBudget {
  /// ¿Puede salir esta llamada? Se responde con el libro ya al día.
  BudgetVerdict admit(String text, {required String model});

  /// Apunta lo que costó de verdad la llamada que acaba de volver.
  Future<void> record(double costUsd);
}
