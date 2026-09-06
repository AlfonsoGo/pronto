// ignore_for_file: require_trailing_commas
import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:pronto/src/features/formatting/cloud_budget.dart';
import 'package:pronto/src/features/formatting/cloud_cost.dart';
import 'package:pronto/src/features/formatting/cloud_key.dart';
import 'package:pronto/src/features/formatting/text_formatter.dart';
import 'package:pronto/src/features/settings/settings_controller.dart';
import 'package:pronto/src/platform/credential_store.dart';

// El camino de nube ENTERO: la clave, el tope de gasto, el aviso de que el
// texto sale del equipo y lo que se apunta después.
//
// El criterio de aceptación del ticket PRNT-11, en el lenguaje del usuario:
//   1. Con la nube apagada no sale NI UNA petición.
//   2. Con la nube encendida sale el texto y JAMÁS el audio.
//   3. La clave no aparece en los ajustes, ni en un aviso, ni en un volcado.
//   4. Sin conexión o con la clave mal, el dictado se pega igual.

/// Respuesta de OpenRouter tal y como llega de verdad, con su `usage`.
http.Response _openrouter(
  String content, {
  double? cost,
  bool conUsage = true,
}) {
  final cuerpo = <String, dynamic>{
    'choices': [
      {
        'message': {'role': 'assistant', 'content': content}
      }
    ],
    if (conUsage)
      'usage': {
        'prompt_tokens': 1180,
        'completion_tokens': 24,
        'total_tokens': 1204,
        if (cost != null) 'cost': cost,
      },
  };
  return http.Response.bytes(
    utf8.encode(jsonEncode(cuerpo)),
    200,
    headers: {'content-type': 'application/json; charset=utf-8'},
  );
}

/// Guardia de mentira con memoria: dice lo que se le manda decir y apunta lo
/// que se le apunta, para poder mirarlo desde el test.
class _GuardiaFalso implements CloudBudget {
  _GuardiaFalso({this.veredicto = BudgetVerdict.ok});

  BudgetVerdict veredicto;
  final List<double> apuntado = [];
  int preguntas = 0;

  @override
  BudgetVerdict admit(String text, {required String model}) {
    preguntas++;
    return veredicto;
  }

  @override
  Future<void> record(double costUsd) async => apuntado.add(costUsd);
}

void main() {
  const crudo = 'a ver eh mandame el informe de agosto antes del viernes';
  const bien = 'Mándame el informe de agosto antes del viernes.';
  const claveSecreta = 'sk-or-v1-ESTA-CLAVE-NO-PUEDE-APARECER';

  group('1. con la nube apagada no sale nada del equipo', () {
    test('camino «sin formateo»: cero peticiones, aunque haya clave y tope',
        () async {
      var peticiones = 0;
      final f = TextFormatter(
        cloudApiKey: () => claveSecreta,
        budget: _GuardiaFalso(),
        httpClient: MockClient((_) async {
          peticiones++;
          return _openrouter(bien);
        }),
      );
      for (final t in [crudo, 'otro dictado', 'y otro más largo todavía']) {
        expect((await f.format(t)).text, t);
      }
      expect(peticiones, 0);
    });

    test('camino local: todo lo que sale va a la MÁQUINA, a ningún sitio más',
        () async {
      final destinos = <Uri>[];
      final f = TextFormatter(
        path: FormatPath.local,
        cloudApiKey: () => claveSecreta,
        httpClient: MockClient((req) async {
          destinos.add(req.url);
          return http.Response.bytes(
            utf8.encode(jsonEncode({
              'message': {'role': 'assistant', 'content': bien}
            })),
            200,
            headers: {'content-type': 'application/json; charset=utf-8'},
          );
        }),
      );
      await f.format(crudo);
      expect(destinos, isNotEmpty);
      for (final u in destinos) {
        expect(['localhost', '127.0.0.1'], contains(u.host));
      }
    });

    test('con la nube apagada NO se le pregunta siquiera al guardia', () async {
      final guardia = _GuardiaFalso();
      final f = TextFormatter(
        budget: guardia,
        httpClient: MockClient((_) async => _openrouter(bien)),
      );
      await f.format(crudo);
      expect(guardia.preguntas, 0);
    });
  });

  group('2. con la nube encendida sale el TEXTO, jamás el audio', () {
    test('lo que viaja es una lista blanca de campos, todos de texto',
        () async {
      late http.Request visto;
      final f = TextFormatter(
        path: FormatPath.cloud,
        cloudApiKey: () => claveSecreta,
        budget: _GuardiaFalso(),
        httpClient: MockClient((req) async {
          visto = req;
          return _openrouter(bien, cost: 0.00008);
        }),
      );
      await f.format(crudo);

      final cuerpo = jsonDecode(visto.body) as Map<String, dynamic>;
      // Lista BLANCA, no negra: se enumera lo que PUEDE salir. Buscar "audio"
      // por subcadena no probaría nada; esto sí prueba el negativo.
      expect(cuerpo.keys.toSet(),
          {'model', 'messages', 'temperature', 'stream', 'provider'});
      for (final m in (cuerpo['messages'] as List).cast<Map>()) {
        expect(m.keys.toSet(), {'role', 'content'});
        expect(m['content'], isA<String>());
      }
      expect(cuerpo['messages'].last['content'], crudo);
    });

    test('se le prohíbe al proveedor guardar y entrenar con el texto',
        () async {
      // Sin esta línea OpenRouter enruta por defecto a proveedores que pueden
      // entrenar con lo que se les manda.
      late Map<String, dynamic> cuerpo;
      final f = TextFormatter(
        path: FormatPath.cloud,
        cloudApiKey: () => claveSecreta,
        budget: _GuardiaFalso(),
        httpClient: MockClient((req) async {
          cuerpo = jsonDecode(req.body) as Map<String, dynamic>;
          return _openrouter(bien);
        }),
      );
      await f.format(crudo);
      expect(cuerpo['provider'], {'data_collection': 'deny'});
    });

    test('el aviso de «sale del equipo» se dispara UNA vez y solo al salir',
        () async {
      var avisos = 0;
      final f = TextFormatter(
        path: FormatPath.cloud,
        cloudApiKey: () => claveSecreta,
        budget: _GuardiaFalso(),
        httpClient: MockClient((_) async => _openrouter(bien)),
      );
      await f.format(crudo, onLeavingDevice: () => avisos++);
      expect(avisos, 1);
    });

    test('el camino local NO enciende el aviso: no sale nada del equipo',
        () async {
      var avisos = 0;
      final f = TextFormatter(
        path: FormatPath.local,
        httpClient: MockClient((_) async => http.Response.bytes(
              utf8.encode(jsonEncode({
                'message': {'content': bien}
              })),
              200,
            )),
      );
      await f.format(crudo, onLeavingDevice: () => avisos++);
      expect(avisos, 0);
    });
  });

  group('3. la clave no se enseña ni se filtra', () {
    test('viaja en la cabecera y NO en el cuerpo', () async {
      late http.Request visto;
      final f = TextFormatter(
        path: FormatPath.cloud,
        cloudApiKey: () => claveSecreta,
        budget: _GuardiaFalso(),
        httpClient: MockClient((req) async {
          visto = req;
          return _openrouter(bien);
        }),
      );
      await f.format(crudo);
      expect(visto.headers['Authorization'], 'Bearer $claveSecreta');
      expect(visto.body, isNot(contains(claveSecreta)));
    });

    test('el proveedor que ESCUPE la clave en su error no la reenseña',
        () async {
      // Un 401 cuyo cuerpo trae la clave enterita. El aviso es NUESTRO.
      final f = TextFormatter(
        path: FormatPath.cloud,
        cloudApiKey: () => claveSecreta,
        budget: _GuardiaFalso(),
        httpClient: MockClient(
          (_) async => http.Response('clave inválida: $claveSecreta', 401),
        ),
      );
      final r = await f.format(crudo);
      expect(r.notice, isNot(contains(claveSecreta)));
      expect(r.notice, 'Sin formatear: el proveedor rechazó la clave');
      expect(r.text, crudo);
    });

    test('una excepción que arrastra la clave tampoco la enseña', () async {
      final f = TextFormatter(
        path: FormatPath.cloud,
        cloudApiKey: () => claveSecreta,
        budget: _GuardiaFalso(),
        httpClient: MockClient(
          (_) async => throw Exception('fallo con $claveSecreta dentro'),
        ),
      );
      final r = await f.format(crudo);
      expect(r.notice, isNot(contains(claveSecreta)));
    });

    test('la clave NO cabe en el volcado de ajustes: ahí no hay sitio para ella',
        () {
      // El fichero de ajustes es un JSON en claro. Se comprueba el volcado
      // ENTERO, no un campo concreto.
      final ajustes = AppSettings.defaults().copyWith(
        formatPath: FormatPath.cloud,
        cloudModel: 'openai/gpt-5-mini',
      );
      final volcado = jsonEncode(ajustes.toJson());
      expect(volcado, isNot(contains(claveSecreta)));
      expect(volcado.toLowerCase(), isNot(contains('sk-or')));
      expect(ajustes.toJson().keys, isNot(contains('cloudApiKey')));
    });

    test('el formateador no guarda la clave: la pide en CADA llamada', () async {
      // Así, borrarla en Ajustes surte efecto en el dictado siguiente sin
      // reconstruir nada, y no queda un secreto vivo dentro de un objeto que
      // dura toda la sesión.
      final almacen = InMemoryCredentialStore();
      final clave = CloudKey(almacen);
      var peticiones = 0;
      final f = TextFormatter(
        path: FormatPath.cloud,
        cloudApiKey: clave.read,
        budget: _GuardiaFalso(),
        httpClient: MockClient((_) async {
          peticiones++;
          return _openrouter(bien);
        }),
      );

      // Sin clave todavía: no sale nada (el camino de ACTIVACIÓN aún no está).
      expect((await f.format(crudo)).notice, contains('falta la clave'));
      expect(peticiones, 0);

      // El usuario la guarda en Ajustes: la MISMA instancia ya formatea.
      clave.save(claveSecreta);
      expect((await f.format(crudo)).text, bien);
      expect(peticiones, 1);

      // Y si la borra, se apaga sin reiniciar nada.
      clave.clear();
      expect((await f.format(crudo)).outcome, FormatOutcome.unavailable);
      expect(peticiones, 1);
    });
  });

  group('4. pase lo que pase, el dictado se pega', () {
    test('sin conexión con el proveedor', () async {
      final f = TextFormatter(
        path: FormatPath.cloud,
        cloudApiKey: () => claveSecreta,
        budget: _GuardiaFalso(),
        httpClient: MockClient((_) async => throw const _RedCaida()),
      );
      final r = await f.format(crudo);
      expect(r.text, crudo);
      expect(r.outcome, FormatOutcome.unavailable);
    });

    test('clave inválida', () async {
      final f = TextFormatter(
        path: FormatPath.cloud,
        cloudApiKey: () => 'clave-que-no-vale',
        budget: _GuardiaFalso(),
        httpClient: MockClient((_) async => http.Response('nope', 401)),
      );
      expect((await f.format(crudo)).text, crudo);
    });

    test('el proveedor devuelve algo que no se entiende', () async {
      final f = TextFormatter(
        path: FormatPath.cloud,
        cloudApiKey: () => claveSecreta,
        budget: _GuardiaFalso(),
        httpClient: MockClient((_) async => http.Response.bytes(
              utf8.encode('{"vaya":"cosa"}'),
              200,
              headers: {'content-type': 'application/json'},
            )),
      );
      final r = await f.format(crudo);
      expect(r.text, crudo);
      expect(r.notice, startsWith('Sin formatear:'));
    });
  });

  group('el tope de gasto frena ANTES de mandar nada', () {
    test('al llegar al tope del día no sale la petición', () async {
      var peticiones = 0;
      final f = TextFormatter(
        path: FormatPath.cloud,
        cloudApiKey: () => claveSecreta,
        budget: _GuardiaFalso(veredicto: BudgetVerdict.dayCapReached),
        httpClient: MockClient((_) async {
          peticiones++;
          return _openrouter(bien);
        }),
      );
      var avisos = 0;
      final r = await f.format(crudo, onLeavingDevice: () => avisos++);

      expect(peticiones, 0);
      expect(avisos, 0, reason: 'no se avisa de una salida que no ocurrió');
      expect(r.text, crudo);
      expect(r.outcome, FormatOutcome.overBudget);
      expect(r.notice, 'Sin formatear: alcanzado el tope de gasto de hoy');
    });

    test('un dictado demasiado largo tampoco sale', () async {
      var peticiones = 0;
      final f = TextFormatter(
        path: FormatPath.cloud,
        cloudApiKey: () => claveSecreta,
        budget: _GuardiaFalso(veredicto: BudgetVerdict.textTooLong),
        httpClient: MockClient((_) async {
          peticiones++;
          return _openrouter(bien);
        }),
      );
      expect((await f.format('a' * 9000)).outcome, FormatOutcome.overBudget);
      expect(peticiones, 0);
    });

    test('sin clave NI SIQUIERA se le pregunta al guardia', () async {
      // El orden importa: sin clave no hay llamada posible, así que no se
      // ensucia el contador de nada.
      final guardia = _GuardiaFalso();
      final f = TextFormatter(
        path: FormatPath.cloud,
        cloudApiKey: () => null,
        budget: guardia,
        httpClient: MockClient((_) async => _openrouter(bien)),
      );
      await f.format(crudo);
      expect(guardia.preguntas, 0);
    });
  });

  group('lo gastado se apunta de verdad', () {
    test('se apunta el coste REAL que devuelve el proveedor', () async {
      final guardia = _GuardiaFalso();
      final f = TextFormatter(
        path: FormatPath.cloud,
        cloudApiKey: () => claveSecreta,
        budget: guardia,
        httpClient: MockClient((_) async => _openrouter(bien, cost: 0.000123)),
      );
      await f.format(crudo);
      expect(guardia.apuntado, [0.000123]);
    });

    test('si el proveedor no dice el coste, se apunta la ESTIMACIÓN, no cero',
        () async {
      // Una llamada que se hizo y se apunta como gratis es un contador que
      // miente y un tope que nunca salta.
      final guardia = _GuardiaFalso();
      final f = TextFormatter(
        path: FormatPath.cloud,
        cloudApiKey: () => claveSecreta,
        budget: guardia,
        httpClient: MockClient((_) async => _openrouter(bien, conUsage: false)),
      );
      await f.format(crudo);
      expect(guardia.apuntado, hasLength(1));
      expect(guardia.apuntado.first, greaterThan(0));
      expect(
        guardia.apuntado.first,
        closeTo(
          estimateCallCostUsd(
            model: 'openai/gpt-5-nano',
            textChars: crudo.length,
          ),
          1e-12,
        ),
      );
    });

    test('el dinero se apunta AUNQUE el guardia descarte la salida', () async {
      // El modelo se fue por las ramas y su texto se tira; la llamada se pagó
      // igual. Si esto no se apuntara, el tope contaría de menos.
      final guardia = _GuardiaFalso();
      final f = TextFormatter(
        path: FormatPath.cloud,
        cloudApiKey: () => claveSecreta,
        budget: guardia,
        httpClient: MockClient(
          (_) async => _openrouter(
            'Claro, aquí tienes un resumen ejecutivo del informe de agosto con '
            'sus conclusiones y varias recomendaciones que no dictó nadie.',
            cost: 0.0002,
          ),
        ),
      );
      final r = await f.format(crudo);
      expect(r.outcome, FormatOutcome.rejected);
      expect(r.text, crudo);
      expect(guardia.apuntado, [0.0002]);
    });

    test('un 401 NO se apunta como gasto: no se cobró nada', () async {
      final guardia = _GuardiaFalso();
      final f = TextFormatter(
        path: FormatPath.cloud,
        cloudApiKey: () => claveSecreta,
        budget: guardia,
        httpClient: MockClient((_) async => http.Response('nope', 401)),
      );
      await f.format(crudo);
      expect(guardia.apuntado, isEmpty);
    });

    test('un coste absurdo del proveedor no se apunta como negativo', () async {
      final guardia = _GuardiaFalso();
      final f = TextFormatter(
        path: FormatPath.cloud,
        cloudApiKey: () => claveSecreta,
        budget: guardia,
        httpClient: MockClient((_) async => _openrouter(bien, cost: -3)),
      );
      await f.format(crudo);
      // El formateador cae a la estimación; y el libro, por si acaso, tampoco
      // deja bajar el gasto (ver cloud_budget_test).
      expect(guardia.apuntado.single, greaterThan(0));
    });

    test('el camino local no apunta gasto ninguno', () async {
      final guardia = _GuardiaFalso();
      final f = TextFormatter(
        path: FormatPath.local,
        budget: guardia,
        httpClient: MockClient((_) async => http.Response.bytes(
              utf8.encode(jsonEncode({
                'message': {'content': bien}
              })),
              200,
            )),
      );
      await f.format(crudo);
      expect(guardia.apuntado, isEmpty);
    });
  });
}

/// Fallo de red sin depender de `dart:io`.
class _RedCaida implements Exception {
  const _RedCaida();
  @override
  String toString() => 'SocketException: conexión rechazada';
}
