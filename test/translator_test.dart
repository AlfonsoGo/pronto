import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:pronto/src/features/translation/language_detect.dart';
import 'package:pronto/src/features/translation/translator.dart';

// Tests del traductor con un cliente HTTP falso: sin red real ni LLM.
// Lo que se verifica es el CONTRATO de seguridad: ante cualquier problema
// devuelve null y quien llama se queda con el texto dictado; nunca pega basura.

/// Cliente que responde como Ollama nativo con [content].
MockClient _ollamaRespondiendo(String content, {int status = 200}) {
  return MockClient((req) async {
    return http.Response(
      jsonEncode({
        'message': {'role': 'assistant', 'content': content},
      }),
      status,
      headers: {'content-type': 'application/json; charset=utf-8'},
    );
  });
}

Translator _traductor(http.Client client, {bool enabled = true}) => Translator(
      enabled: enabled,
      baseUrl: 'http://localhost:11434',
      model: 'modelo-de-prueba',
      httpClient: client,
    );

void main() {
  group('camino feliz', () {
    test('devuelve la traducción del modelo', () async {
      final t = _traductor(_ollamaRespondiendo('Hi, how are you?'));
      final out = await t.translate(
        'Hola, ¿cómo estás?',
        from: TextLanguage.es,
        to: TextLanguage.en,
      );
      expect(out, 'Hi, how are you?');
    });

    test('limpia las comillas envolventes del modelo', () async {
      final t = _traductor(_ollamaRespondiendo('"Hi, how are you?"'));
      final out = await t.translate(
        'Hola',
        from: TextLanguage.es,
        to: TextLanguage.en,
      );
      expect(out, 'Hi, how are you?');
    });

    test('descarta el bloque de razonamiento <think>', () async {
      final t = _traductor(
        _ollamaRespondiendo('<think>Vamos a ver…</think>\nHi there!'),
      );
      final out = await t.translate(
        'Hola',
        from: TextLanguage.es,
        to: TextLanguage.en,
      );
      expect(out, 'Hi there!');
    });

    test('conserva los saltos de línea del texto traducido', () async {
      final t = _traductor(_ollamaRespondiendo('First line\nSecond line'));
      final out = await t.translate(
        'Primera línea\nSegunda línea',
        from: TextLanguage.es,
        to: TextLanguage.en,
      );
      expect(out, 'First line\nSecond line');
    });
  });

  group('degradación segura: devuelve null y nunca lanza', () {
    test('traductor desactivado no llama al servidor', () async {
      var llamadas = 0;
      final client = MockClient((req) async {
        llamadas++;
        return http.Response('{}', 200);
      });
      final t = _traductor(client, enabled: false);
      final out = await t.translate(
        'Hola',
        from: TextLanguage.es,
        to: TextLanguage.en,
      );
      expect(out, isNull);
      expect(llamadas, 0);
    });

    test('texto vacío o en blanco', () async {
      final t = _traductor(_ollamaRespondiendo('algo'));
      expect(
        await t.translate('', from: TextLanguage.es, to: TextLanguage.en),
        isNull,
      );
      expect(
        await t.translate('   ', from: TextLanguage.es, to: TextLanguage.en),
        isNull,
      );
    });

    test('el servidor devuelve un error HTTP', () async {
      final t = _traductor(_ollamaRespondiendo('x', status: 500));
      final out = await t.translate(
        'Hola',
        from: TextLanguage.es,
        to: TextLanguage.en,
      );
      expect(out, isNull);
    });

    test('modelo inexistente (404 de Ollama)', () async {
      final client = MockClient(
        (req) async => http.Response('{"error":"model not found"}', 404),
      );
      final t = _traductor(client);
      final out = await t.translate(
        'Hola',
        from: TextLanguage.es,
        to: TextLanguage.en,
      );
      expect(out, isNull);
    });

    test('el servidor devuelve JSON con otra forma', () async {
      final client = MockClient(
        (req) async => http.Response('{"respuesta":"nope"}', 200),
      );
      final t = _traductor(client);
      final out = await t.translate(
        'Hola',
        from: TextLanguage.es,
        to: TextLanguage.en,
      );
      expect(out, isNull);
    });

    test('el servidor devuelve algo que no es JSON', () async {
      final client = MockClient((req) async => http.Response('<html>', 200));
      final t = _traductor(client);
      final out = await t.translate(
        'Hola',
        from: TextLanguage.es,
        to: TextLanguage.en,
      );
      expect(out, isNull);
    });

    test('respuesta vacía tras limpiar (solo un <think>)', () async {
      final t = _traductor(_ollamaRespondiendo('<think>mmm</think>'));
      final out = await t.translate(
        'Hola',
        from: TextLanguage.es,
        to: TextLanguage.en,
      );
      expect(out, isNull);
    });

    test('la conexión falla', () async {
      final client = MockClient(
        (req) async => throw const SocketExceptionFake(),
      );
      final t = _traductor(client);
      final out = await t.translate(
        'Hola',
        from: TextLanguage.es,
        to: TextLanguage.en,
      );
      expect(out, isNull);
    });

    test('el servidor tarda más que el timeout', () async {
      final client = MockClient((req) async {
        await Future<void>.delayed(const Duration(milliseconds: 200));
        return http.Response('{}', 200);
      });
      final t = Translator(
        enabled: true,
        model: 'm',
        timeout: const Duration(milliseconds: 20),
        httpClient: client,
      );
      final out = await t.translate(
        'Hola',
        from: TextLanguage.es,
        to: TextLanguage.en,
      );
      expect(out, isNull);
    });

    test('texto por encima del tope: no traduce ni llama al servidor', () async {
      var llamadas = 0;
      final client = MockClient((req) async {
        llamadas++;
        return http.Response('{}', 200);
      });
      final t = _traductor(client);
      final out = await t.translate(
        'a' * 5000,
        from: TextLanguage.es,
        to: TextLanguage.en,
      );
      expect(out, isNull);
      expect(llamadas, 0);
    });
  });

  group('mismo idioma origen y destino', () {
    test('devuelve el texto tal cual sin llamar al servidor', () async {
      var llamadas = 0;
      final client = MockClient((req) async {
        llamadas++;
        return http.Response('{}', 200);
      });
      final t = _traductor(client);
      final out = await t.translate(
        '  Hola de nuevo  ',
        from: TextLanguage.es,
        to: TextLanguage.es,
      );
      expect(out, 'Hola de nuevo');
      expect(llamadas, 0);
    });
  });

  group('petición al servidor', () {
    test('usa el endpoint nativo de Ollama y normaliza la barra final',
        () async {
      late Uri visitada;
      final client = MockClient((req) async {
        visitada = req.url;
        return http.Response(
          jsonEncode({
            'message': {'content': 'Hi'},
          }),
          200,
        );
      });
      final t = Translator(
        enabled: true,
        baseUrl: 'http://localhost:11434///',
        model: 'm',
        httpClient: client,
      );
      await t.translate('Hola', from: TextLanguage.es, to: TextLanguage.en);
      expect(visitada.toString(), 'http://localhost:11434/api/chat');
    });

    test('usa el endpoint OpenAI y la clave cuando useCloud', () async {
      late Uri visitada;
      String? auth;
      final client = MockClient((req) async {
        visitada = req.url;
        auth = req.headers['Authorization'];
        return http.Response(
          jsonEncode({
            'choices': [
              {
                'message': {'content': 'Hi'},
              },
            ],
          }),
          200,
        );
      });
      final t = Translator(
        enabled: true,
        baseUrl: 'https://api.ejemplo.com',
        model: 'm',
        useCloud: true,
        apiKey: 'sk-test',
        httpClient: client,
      );
      final out =
          await t.translate('Hola', from: TextLanguage.es, to: TextLanguage.en);
      expect(visitada.toString(), 'https://api.ejemplo.com/v1/chat/completions');
      expect(auth, 'Bearer sk-test');
      expect(out, 'Hi');
    });

    test('manda el texto del usuario como último mensaje, sin recortar',
        () async {
      late Map<String, dynamic> cuerpo;
      final client = MockClient((req) async {
        cuerpo = jsonDecode(req.body) as Map<String, dynamic>;
        return http.Response(
          jsonEncode({
            'message': {'content': 'Hi'},
          }),
          200,
        );
      });
      final t = _traductor(client);
      const texto = 'Hola, ¿qué tal? Esto es una ñapa con acentos.';
      await t.translate(texto, from: TextLanguage.es, to: TextLanguage.en);

      final mensajes = (cuerpo['messages'] as List).cast<Map>();
      expect(mensajes.last['role'], 'user');
      expect(mensajes.last['content'], texto);
      expect(mensajes.first['role'], 'system');
      // Temperatura 0: la traducción no debe variar entre dictados idénticos.
      expect((cuerpo['options'] as Map)['temperature'], 0);
      expect(cuerpo['stream'], false);
    });
  });

  group('warmUp (precalentar el modelo local)', () {
    test('pide la carga del modelo sin mensajes y con keep_alive', () async {
      late Map<String, dynamic> cuerpo;
      late Uri visitada;
      final client = MockClient((req) async {
        visitada = req.url;
        cuerpo = jsonDecode(req.body) as Map<String, dynamic>;
        return http.Response('{}', 200);
      });
      await _traductor(client).warmUp();
      expect(visitada.toString(), 'http://localhost:11434/api/chat');
      expect(cuerpo['messages'], isEmpty);
      expect(cuerpo['keep_alive'], isNotNull);
    });

    test('la traducción normal también renueva el keep_alive', () async {
      late Map<String, dynamic> cuerpo;
      final client = MockClient((req) async {
        cuerpo = jsonDecode(req.body) as Map<String, dynamic>;
        return http.Response(
          jsonEncode({
            'message': {'content': 'Hi'},
          }),
          200,
        );
      });
      await _traductor(client)
          .translate('Hola', from: TextLanguage.es, to: TextLanguage.en);
      expect(cuerpo['keep_alive'], isNotNull);
    });

    test('no hace nada si la traducción está desactivada', () async {
      var llamadas = 0;
      final client = MockClient((req) async {
        llamadas++;
        return http.Response('{}', 200);
      });
      await _traductor(client, enabled: false).warmUp();
      expect(llamadas, 0);
    });

    test('no hace nada contra un servidor en la nube', () async {
      var llamadas = 0;
      final client = MockClient((req) async {
        llamadas++;
        return http.Response('{}', 200);
      });
      await Translator(
        enabled: true,
        model: 'm',
        useCloud: true,
        httpClient: client,
      ).warmUp();
      expect(llamadas, 0);
    });

    test('un servidor caído no propaga la excepción', () async {
      final client = MockClient(
        (req) async => throw const SocketExceptionFake(),
      );
      await expectLater(_traductor(client).warmUp(), completes);
    });
  });

  group('checkConnection', () {
    test('null cuando el servidor y el modelo responden', () async {
      final t = _traductor(_ollamaRespondiendo('Good morning.'));
      expect(await t.checkConnection(), isNull);
    });

    test('mensaje de error cuando el servidor falla', () async {
      final client = MockClient(
        (req) async => throw const SocketExceptionFake(),
      );
      final t = _traductor(client);
      final error = await t.checkConnection();
      expect(error, isNotNull);
      expect(error, contains('localhost:11434'));
    });

    test('mensaje de error cuando el modelo no devuelve texto', () async {
      final t = _traductor(_ollamaRespondiendo('   '));
      final error = await t.checkConnection();
      expect(error, isNotNull);
      expect(error, contains('modelo-de-prueba'));
    });
  });
}

/// Excepción de red de mentira: evita depender de `dart:io` en el test.
class SocketExceptionFake implements Exception {
  const SocketExceptionFake();
  @override
  String toString() => 'SocketException: conexión rechazada';
}
