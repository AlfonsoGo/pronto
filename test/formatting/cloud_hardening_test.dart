// ignore_for_file: require_trailing_commas
import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:pronto/src/core/config.dart';
import 'package:pronto/src/core/llm_output.dart';
import 'package:pronto/src/features/dictation/dictation_state.dart';
import 'package:pronto/src/features/formatting/cloud_budget.dart';
import 'package:pronto/src/features/formatting/cloud_budget_provider.dart';
import 'package:pronto/src/features/formatting/text_formatter.dart';
import 'package:pronto/src/features/settings/settings_controller.dart';

// Los agujeros que encontró la auditoría de seguridad de PRNT-11, cada uno con
// el test que impide que vuelvan. Nombrados por el SÍNTOMA, no por el arreglo.

http.Response _openrouter(String content, {double? cost}) => http.Response.bytes(
      utf8.encode(jsonEncode({
        'choices': [
          {
            'message': {'content': content}
          }
        ],
        'usage': {'prompt_tokens': 1180, 'completion_tokens': 20, 'cost': cost},
      })),
      200,
      headers: {'content-type': 'application/json; charset=utf-8'},
    );

class _Guardia implements CloudBudget {
  final List<double> apuntado = [];
  @override
  BudgetVerdict admit(String text, {required String model}) =>
      BudgetVerdict.ok;
  @override
  Future<void> record(double costUsd) async => apuntado.add(costUsd);
}

void main() {
  const crudo = 'a ver mandame el informe de agosto antes del viernes';
  const clave = 'sk-or-v1-secreta';

  group('el aviso naranja no se puede apagar con el texto aún fuera', () {
    test('mientras sale a la nube, el dictado está OCUPADO', () {
      // Si `sendingToCloud` no contara como ocupado, pulsar el atajo con el
      // texto en vuelo arrancaría otro dictado: la píldora naranja
      // desaparecería mientras el texto sigue fuera, y habría dos formateos
      // volando contra el mismo tope.
      const s = DictationState(status: DictationStatus.sendingToCloud);
      expect(s.isBusy, isTrue);
    });

    test('todos los estados de trabajo cuentan como ocupado', () {
      for (final st in [
        DictationStatus.recording,
        DictationStatus.transcribing,
        DictationStatus.sendingToCloud,
        DictationStatus.translating,
        DictationStatus.injecting,
      ]) {
        expect(DictationState(status: st).isBusy, isTrue, reason: st.name);
      }
      for (final st in [
        DictationStatus.idle,
        DictationStatus.error,
        DictationStatus.uninitialized,
      ]) {
        expect(DictationState(status: st).isBusy, isFalse, reason: st.name);
      }
    });
  });

  group('la salida del modelo se pega, no se teclea', () {
    test('un salto de línea NO llega al inyector (sería un Intro de verdad)',
        () {
      final limpio = sanitizeForInjection('Hola.\nrm -rf algo');
      expect(limpio, isNot(contains('\n')));
      expect(limpio, 'Hola. rm -rf algo');
    });

    test('los caracteres invisibles se caen por el camino', () {
      // Anchura cero y marcas bidireccionales: esconden texto dentro de texto.
      const veneno = 'Hola\u200Bmundo\u202Ereverso\uFEFF\u2066aislado\u2069';
      final limpio = sanitizeForInjection(veneno);
      for (final r in limpio.runes) {
        expect(r, isNot(0x200B));
        expect(r, isNot(0x202E));
        expect(r, isNot(0xFEFF));
        expect(r, isNot(0x2066));
      }
      expect(limpio, 'Holamundoreversoaislado');
    });

    test('los controles se van, el español entero se queda', () {
      const texto = '¿Qué tal? Mañana a las 9, ¡vale! Ñandú, año, 1250 €. 🍷';
      expect(sanitizeForInjection('$texto'), texto);
    });

    test('el formateo real no deja pasar un Intro del modelo', () async {
      // El salto va DENTRO de un texto que el guardia anti-parafraseo acepta
      // (son las mismas palabras): si no, la salida se descartaría por otro
      // motivo y este test no estaría mirando lo que dice mirar.
      final f = TextFormatter(
        path: FormatPath.cloud,
        cloudApiKey: () => clave,
        budget: _Guardia(),
        httpClient: MockClient(
          (_) async => _openrouter(
            'Mándame el informe de agosto\nantes del viernes.',
          ),
        ),
      );
      final r = await f.format(
        'mandame el informe de agosto antes del viernes',
      );
      expect(r.outcome, FormatOutcome.formatted);
      expect(r.text, 'Mándame el informe de agosto antes del viernes.');
      expect(r.text, isNot(contains('\n')));
    });
  });

  group('el dinero que se gasta SIEMPRE se apunta', () {
    test('un tiempo agotado tras salir la petición se apunta igual', () async {
      // El proveedor genera y cobra aunque nosotros cortemos a los 12 s. Un
      // bucle de tiempos agotados gastaría de verdad con el contador a cero.
      final guardia = _Guardia();
      final f = TextFormatter(
        path: FormatPath.cloud,
        cloudApiKey: () => clave,
        budget: guardia,
        timeout: const Duration(milliseconds: 30),
        httpClient: MockClient((_) async {
          await Future<void>.delayed(const Duration(seconds: 2));
          return _openrouter('tarde');
        }),
      );
      final r = await f.format(crudo);
      expect(r.text, crudo);
      expect(r.notice, contains('tiempo'));
      expect(guardia.apuntado, hasLength(1));
      expect(guardia.apuntado.first, greaterThan(0));
    });

    test('una respuesta 200 ininteligible también se ha pagado', () async {
      final guardia = _Guardia();
      final f = TextFormatter(
        path: FormatPath.cloud,
        cloudApiKey: () => clave,
        budget: guardia,
        httpClient: MockClient((_) async => http.Response.bytes(
              utf8.encode('{"otra":"cosa"}'),
              200,
              headers: {'content-type': 'application/json'},
            )),
      );
      await f.format(crudo);
      expect(guardia.apuntado, hasLength(1));
    });

    test('un 401 NO se apunta: ahí no se generó nada', () async {
      final guardia = _Guardia();
      final f = TextFormatter(
        path: FormatPath.cloud,
        cloudApiKey: () => clave,
        budget: guardia,
        httpClient: MockClient((_) async => http.Response('no', 401)),
      );
      await f.format(crudo);
      expect(guardia.apuntado, isEmpty);
    });

    test('se apunta si y solo si la petición llegó a salir', () async {
      // El aviso de salida no llegó a dispararse: no hubo petición.
      final guardia = _Guardia();
      final f = TextFormatter(
        path: FormatPath.cloud,
        cloudApiKey: () => clave,
        budget: guardia,
        httpClient: MockClient((_) async => throw const _RedCaida()),
      );
      var avisos = 0;
      await f.format(crudo, onLeavingDevice: () => avisos++);
      // El doble revienta DESPUÉS del aviso (el aviso va justo antes del POST),
      // así que aquí sí hubo salida y sí se apunta. Lo que se fija es la regla:
      // se apunta si y solo si el aviso se disparó.
      expect(guardia.apuntado.length, avisos);
    });

    test('un coste de CERO del proveedor no congela el tope', () async {
      // Un proveedor que informe siempre 0 dejaría el contador clavado.
      final guardia = _Guardia();
      final f = TextFormatter(
        path: FormatPath.cloud,
        cloudApiKey: () => clave,
        budget: guardia,
        httpClient: MockClient((_) async => _openrouter('Bien.', cost: 0)),
      );
      await f.format(crudo);
      expect(guardia.apuntado.single, greaterThan(0));
    });
  });

  group('un servidor "local" que no está en esta máquina TAMBIÉN saca texto',
      () {
    test('con un llmBaseUrl remoto se avisa igual', () async {
      var avisos = 0;
      final f = TextFormatter(
        path: FormatPath.local,
        localBaseUrl: 'http://192.168.1.50:11434',
        httpClient: MockClient((_) async => http.Response.bytes(
              utf8.encode(jsonEncode({
                'message': {'content': 'Bien.'}
              })),
              200,
            )),
      );
      await f.format(crudo, onLeavingDevice: () => avisos++);
      expect(avisos, 1, reason: 'decir que solo la nube sale sería mentira');
    });

    test('con Ollama en esta máquina NO se avisa', () async {
      for (final url in [
        'http://localhost:11434',
        'http://127.0.0.1:11434',
        'http://[::1]:11434',
      ]) {
        var avisos = 0;
        final f = TextFormatter(
          path: FormatPath.local,
          localBaseUrl: url,
          httpClient: MockClient((_) async => http.Response.bytes(
                utf8.encode(jsonEncode({
                  'message': {'content': 'Bien.'}
                })),
                200,
              )),
        );
        await f.format(crudo, onLeavingDevice: () => avisos++);
        expect(avisos, 0, reason: url);
      }
    });
  });

  group('el guardia no juzga con el libro en blanco', () {
    test('antes de leer lo gastado, la nube no sale', () {
      final container = ProviderContainer();
      addTearDown(container.dispose);
      final guardia = container.read(cloudSpendProvider.notifier);
      // Recién construido: la carga de disco es una microtarea que aún no ha
      // corrido. Con el libro en blanco cualquier llamada parecería caber.
      expect(
        guardia.admit('hola', model: 'openai/gpt-5-nano'),
        BudgetVerdict.notReady,
      );
    });

    test('cuando la carga termina, vuelve a juzgar de verdad', () async {
      final container = ProviderContainer();
      addTearDown(container.dispose);
      final guardia = container.read(cloudSpendProvider.notifier);
      // En test no hay plugin de preferencias: la carga falla, pero el
      // `finally` marca el libro como leído (dejar la nube muerta para siempre
      // sería peor que arrancar con el contador a cero).
      await Future<void>.delayed(Duration.zero);
      expect(
        guardia.admit('hola', model: 'openai/gpt-5-nano'),
        BudgetVerdict.ok,
      );
    });

    test('el aviso de «aún no está listo» es una cadena nuestra', () {
      expect(BudgetVerdict.notReady.notice, startsWith('Sin formatear:'));
    });
  });

  group('un tope de gasto tiene techo', () {
    test('teclear 100 donde se quería 1,00 no cambia el tope', () {
      final container = ProviderContainer();
      addTearDown(container.dispose);
      final ajustes = container.read(settingsProvider.notifier);
      final antes = container.read(settingsProvider).cloudDayCapUsd;
      ajustes.setCloudDayCapUsd(100);
      expect(container.read(settingsProvider).cloudDayCapUsd, antes);
    });

    test('un tope razonable sí se acepta', () {
      final container = ProviderContainer();
      addTearDown(container.dispose);
      container.read(settingsProvider.notifier).setCloudDayCapUsd(0.5);
      expect(container.read(settingsProvider).cloudDayCapUsd, 0.5);
    });

    test('un tope disparatado en el fichero se recorta al techo', () {
      final s = AppSettings.fromJson(const <String, dynamic>{
        'cloudDayCapUsd': 999,
        'cloudMonthCapUsd': 999,
      });
      expect(s.cloudDayCapUsd, AppConfig.cloudMaxCapUsd);
      expect(s.cloudMonthCapUsd, AppConfig.cloudMaxCapUsd);
    });
  });

}

class _RedCaida implements Exception {
  const _RedCaida();
  @override
  String toString() => 'SocketException';
}
