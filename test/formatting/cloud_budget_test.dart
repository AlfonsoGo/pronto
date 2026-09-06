// ignore_for_file: require_trailing_commas
import 'dart:math';

import 'package:flutter_test/flutter_test.dart';
import 'package:pronto/src/features/formatting/cloud_budget.dart';
import 'package:pronto/src/features/formatting/cloud_cost.dart';

// El guardia del gasto: lo único que impide que un bucle se coma el dinero
// antes de que nadie se entere. Se prueba la DECISIÓN (judgeCloudCall) y el
// LIBRO (CloudSpend) por separado, sin tocar disco.

const String _modelo = 'openai/gpt-5-nano';

CloudCaps _caps({
  double dia = 0.10,
  double mes = 1.00,
  int chars = 4000,
}) =>
    CloudCaps(dayUsd: dia, monthUsd: mes, maxChars: chars);

final DateTime _hoy = DateTime(2026, 9, 4, 12);

CloudSpend _libro({
  double dia = 0,
  int llamadasDia = 0,
  double mes = 0,
  int llamadasMes = 0,
  DateTime? cuando,
}) {
  final t = cuando ?? _hoy;
  return CloudSpend(
    day: CloudSpend.dayKey(t),
    dayUsd: dia,
    dayCalls: llamadasDia,
    month: CloudSpend.monthKey(t),
    monthUsd: mes,
    monthCalls: llamadasMes,
  );
}

void main() {
  const dictado = 'a ver mandame el informe de agosto antes del viernes';

  group('el guardia deja pasar lo normal', () {
    test('libro a cero y topes por defecto: pasa', () {
      expect(
        judgeCloudCall(
          text: dictado,
          model: _modelo,
          spend: _libro(),
          caps: _caps(),
          now: _hoy,
        ),
        BudgetVerdict.ok,
      );
    });

    test('un mes entero de dictado normal cabe de sobra en el tope', () {
      // 161 dictados/mes es el volumen real medido. Si el tope por defecto no
      // los dejara pasar, el freno estorbaría en vez de proteger.
      var libro = _libro();
      for (var i = 0; i < 161; i++) {
        final v = judgeCloudCall(
          text: dictado,
          model: _modelo,
          spend: libro,
          caps: _caps(),
          now: _hoy,
        );
        expect(v, BudgetVerdict.ok, reason: 'dictado $i');
        libro = libro.plus(
          estimateCallCostUsd(model: _modelo, textChars: dictado.length),
          _hoy,
        );
      }
    });
  });

  group('el guardia corta ANTES, no a mitad', () {
    test('si con esta llamada se pasa del día, la llamada NO sale', () {
      // Justo por debajo del tope: lo que queda no da para otra llamada.
      final estimado =
          estimateCallCostUsd(model: _modelo, textChars: dictado.length);
      final libro = _libro(dia: 0.10 - estimado / 2, mes: 0.10);
      expect(
        judgeCloudCall(
          text: dictado,
          model: _modelo,
          spend: libro,
          caps: _caps(),
          now: _hoy,
        ),
        BudgetVerdict.dayCapReached,
      );
    });

    test('el tope del mes también frena aunque el del día tenga sitio', () {
      final libro = _libro(dia: 0, mes: 1.0);
      expect(
        judgeCloudCall(
          text: dictado,
          model: _modelo,
          spend: libro,
          caps: _caps(),
          now: _hoy,
        ),
        BudgetVerdict.monthCapReached,
      );
    });

    test('tope a cero = nube apagada', () {
      expect(
        judgeCloudCall(
          text: dictado,
          model: _modelo,
          spend: _libro(),
          caps: _caps(dia: 0),
          now: _hoy,
        ),
        BudgetVerdict.dayCapReached,
      );
    });

    test('un dictado demasiado largo no sale del equipo', () {
      final larguisimo = 'a' * 4001;
      expect(
        judgeCloudCall(
          text: larguisimo,
          model: _modelo,
          spend: _libro(),
          caps: _caps(),
          now: _hoy,
        ),
        BudgetVerdict.textTooLong,
      );
    });

    test('justo en el máximo de caracteres SÍ pasa (N y N+1)', () {
      expect(
        judgeCloudCall(
          text: 'a' * 4000,
          model: _modelo,
          spend: _libro(),
          caps: _caps(),
          now: _hoy,
        ),
        BudgetVerdict.ok,
      );
    });

    test('el modelo caro llena el tope antes que el barato', () {
      final libro = _libro(dia: 0.0004);
      final caps = _caps(dia: 0.0005);
      expect(
        judgeCloudCall(
          text: dictado,
          model: 'openai/gpt-5-nano',
          spend: libro,
          caps: caps,
          now: _hoy,
        ),
        BudgetVerdict.ok,
      );
      expect(
        judgeCloudCall(
          text: dictado,
          model: 'modelo/desconocido',
          spend: libro,
          caps: caps,
          now: _hoy,
        ),
        BudgetVerdict.dayCapReached,
      );
    });
  });

  group('avisos: la voz del sistema es NUESTRA', () {
    test('cada veredicto que frena trae su motivo en español', () {
      for (final v in BudgetVerdict.values) {
        if (v == BudgetVerdict.ok) {
          expect(v.notice, isNull);
        } else {
          expect(v.notice, startsWith('Sin formatear:'));
        }
      }
    });
  });

  group('el libro de cuentas', () {
    test('apuntar una llamada sube gasto y contador', () {
      final libro = _libro().plus(0.002, _hoy);
      expect(libro.dayUsd, 0.002);
      expect(libro.dayCalls, 1);
      expect(libro.monthUsd, 0.002);
      expect(libro.monthCalls, 1);
    });

    test('al cambiar de DÍA el contador de hoy vuelve a cero, el mes no', () {
      final ayer = _libro(
        dia: 0.05,
        llamadasDia: 3,
        mes: 0.30,
        llamadasMes: 40,
        cuando: DateTime(2026, 9, 3),
      );
      final hoy = ayer.rollTo(_hoy);
      expect(hoy.dayUsd, 0);
      expect(hoy.dayCalls, 0);
      expect(hoy.monthUsd, 0.30);
      expect(hoy.monthCalls, 40);
    });

    test('al cambiar de MES se ponen a cero los dos', () {
      final agosto = _libro(
        dia: 0.05,
        mes: 0.30,
        llamadasMes: 40,
        cuando: DateTime(2026, 8, 31),
      );
      final septiembre = agosto.rollTo(_hoy);
      expect(septiembre.dayUsd, 0);
      expect(septiembre.monthUsd, 0);
      expect(septiembre.monthCalls, 0);
    });

    test('rollTo es idempotente', () {
      final a = _libro(dia: 0.01, mes: 0.02).rollTo(_hoy);
      expect(a.rollTo(_hoy), a);
    });

    test('un coste absurdo NO regala tope', () {
      // Si el proveedor devolviera un número raro, el gasto no puede bajar.
      for (final raro in [-5.0, double.nan, double.infinity]) {
        final libro = _libro(dia: 0.01, mes: 0.01).plus(raro, _hoy);
        expect(libro.dayUsd, 0.01);
        // La llamada se hizo: se cuenta aunque su coste no se sepa.
        expect(libro.dayCalls, 1);
      }
    });

    test('ida y vuelta por JSON', () {
      final libro = _libro(dia: 0.01, llamadasDia: 2, mes: 0.3, llamadasMes: 9);
      expect(CloudSpend.fromJson(libro.toJson(), _hoy), libro);
    });

    test('un libro ILEGIBLE se lee como cero, nunca como "sin límite"', () {
      final roto = CloudSpend.fromJson(const <String, dynamic>{
        'day': '2026-09-04',
        'dayUsd': 'un montón',
        'dayCalls': -7,
        'month': '2026-09',
        'monthUsd': null,
      }, _hoy);
      expect(roto.dayUsd, 0);
      expect(roto.dayCalls, 0);
      expect(roto.monthUsd, 0);
    });

    test('un libro de otro día se pone al día al leerlo', () {
      final viejo = _libro(dia: 0.09, mes: 0.5, cuando: DateTime(2026, 9, 1));
      final leido = CloudSpend.fromJson(viejo.toJson(), _hoy);
      expect(leido.dayUsd, 0);
      expect(leido.monthUsd, 0.5);
    });

    test('las claves de día y mes se escriben con cero delante', () {
      expect(CloudSpend.dayKey(DateTime(2026, 1, 5)), '2026-01-05');
      expect(CloudSpend.monthKey(DateTime(2026, 1, 5)), '2026-01');
    });
  });

  group('invariantes con entradas que nadie escribió a mano', () {
    test('NUNCA se deja pasar una llamada que cruzaría el tope', () {
      final rnd = Random(42);
      for (var i = 0; i < 2000; i++) {
        final capsDia = rnd.nextDouble() * 0.5;
        final gastado = rnd.nextDouble() * 0.5;
        final chars = rnd.nextInt(5000);
        final texto = 'x' * chars;
        final caps = _caps(dia: capsDia, mes: 100);
        final libro = _libro(dia: gastado, mes: gastado);
        final veredicto = judgeCloudCall(
          text: texto,
          model: _modelo,
          spend: libro,
          caps: caps,
          now: _hoy,
        );
        if (veredicto == BudgetVerdict.ok) {
          final despues = libro.plus(
            estimateCallCostUsd(model: _modelo, textChars: chars),
            _hoy,
          );
          expect(despues.dayUsd, lessThanOrEqualTo(capsDia),
              reason: 'caso $i: caps=$capsDia gastado=$gastado chars=$chars');
        }
      }
    });

    test('el gasto solo sube: N llamadas suman lo que costaron', () {
      final rnd = Random(9);
      var libro = _libro();
      var suma = 0.0;
      var anterior = 0.0;
      for (var i = 0; i < 1000; i++) {
        final coste = rnd.nextDouble() * 0.01;
        suma += coste;
        libro = libro.plus(coste, _hoy);
        expect(libro.monthUsd, greaterThanOrEqualTo(anterior));
        anterior = libro.monthUsd;
      }
      expect(libro.monthUsd, closeTo(suma, 1e-9));
      expect(libro.monthCalls, 1000);
    });
  });

  group('lo hostil', () {
    test('texto vacío: el guardia no revienta', () {
      expect(
        judgeCloudCall(
          text: '',
          model: _modelo,
          spend: _libro(),
          caps: _caps(),
          now: _hoy,
        ),
        BudgetVerdict.ok,
      );
    });

    test('emojis y tildes cuentan como caracteres, no revientan', () {
      expect(
        judgeCloudCall(
          text: '🍷ñáéíóú' * 10,
          model: _modelo,
          spend: _libro(),
          caps: _caps(),
          now: _hoy,
        ),
        BudgetVerdict.ok,
      );
    });

    test('un dictado de 10k se frena por longitud, no por gasto', () {
      expect(
        judgeCloudCall(
          text: 'a' * 10000,
          model: _modelo,
          spend: _libro(),
          caps: _caps(dia: 100, mes: 100),
          now: _hoy,
        ),
        BudgetVerdict.textTooLong,
      );
    });

    test('el 29 de febrero y el cambio de año no rompen las claves', () {
      final bisiesto = DateTime(2028, 2, 29, 23, 59);
      final nochevieja = DateTime(2026, 12, 31, 23, 59);
      expect(CloudSpend.dayKey(bisiesto), '2028-02-29');
      expect(
        CloudSpend.empty(nochevieja).rollTo(DateTime(2027, 1, 1)).month,
        '2027-01',
      );
    });
  });
}
