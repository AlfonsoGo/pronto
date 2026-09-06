// ignore_for_file: require_trailing_commas
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:pronto/src/core/theme.dart';
import 'package:pronto/src/features/formatting/assistant_comparator.dart';
import 'package:pronto/src/features/formatting/formatter_provider.dart';
import 'package:pronto/src/features/formatting/ollama_catalog.dart';
import 'package:pronto/src/features/formatting/text_formatter.dart';
import 'package:pronto/src/features/settings/settings_controller.dart';
import 'package:pronto/src/platform/platform_services.dart';
import 'package:pronto/src/platform/system_memory.dart';
import 'package:shared_preferences/shared_preferences.dart';

// El comparador de los tres Caminos, accionado como lo acciona el usuario.
//
// Aquí vive la prueba negativa que da nombre al PRNT-18: **Ollama corriendo
// SIN el modelo configurado**. Hasta la 0.9 eso era un 404 que en pantalla se
// leía como «el Formateo no sirve», y quien lo veía abandonaba la función.

const int _gb = 1024 * 1024 * 1024;

String _tags(List<String> nombres) => jsonEncode({
      'models': [
        for (final n in nombres)
          {
            'name': n,
            'size': _gb,
            'details': {'family': 'qwen2', 'parameter_size': '1.7B'},
            'capabilities': ['completion'],
          },
      ],
    });

void main() {
  late ProviderContainer container;

  setUp(() {
    // Los ajustes se guardan de verdad al elegir un camino, y sin esto el
    // futuro de SharedPreferences no vuelve nunca: la elección se quedaría a
    // medias y quien la escuche (el asistente de instalación) no se enteraría.
    SharedPreferences.setMockInitialValues({});
  });

  /// Abre el comparador solo, sin el resto de Ajustes.
  ///
  /// [respuesta] es lo que contesta el Ollama de mentira: por defecto, dos
  /// modelos instalados.
  Future<void> abrir(
    WidgetTester tester, {
    Future<http.Response> Function(http.Request)? respuesta,
    String modeloConfigurado = 'qwen3:1.7b',
    FormatPath camino = FormatPath.local,
    int? memoria = 32 * _gb,
    double ancho = 900,
  }) async {
    tester.view.physicalSize = Size(ancho, 1400);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);

    container = ProviderContainer(
      overrides: [
        systemMemoryProvider.overrideWithValue(_MemoriaFija(memoria)),
        ollamaClientProvider.overrideWithValue(
          OllamaClient(
            httpClient: MockClient(
              respuesta ??
                  (_) async =>
                      http.Response(_tags(['qwen3:1.7b', 'qwen2.5:7b']), 200),
            ),
            timeout: const Duration(milliseconds: 200),
          ),
        ),
        settingsProvider.overrideWith(
          () => _AjustesFijos(
            AppSettings.defaults().copyWith(
              formatPath: camino,
              formatModel: modeloConfigurado,
            ),
          ),
        ),
      ],
    );
    addTearDown(container.dispose);

    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: MaterialApp(
          theme: buildProntoTheme(),
          home: const Scaffold(
            body: SingleChildScrollView(child: AssistantComparator()),
          ),
        ),
      ),
    );
    await tester.pump(const Duration(milliseconds: 300));
  }

  testWidgets('los tres caminos están a la vista con lo que cuesta cada uno',
      (tester) async {
    await abrir(tester);

    expect(find.text('Sin Formateo'), findsOneWidget);
    expect(find.text('En tu equipo'), findsOneWidget);
    expect(find.text('Nube con tu clave'), findsOneWidget);

    // Las cinco filas que hacen que se pueda comparar de verdad.
    expect(find.text('Espera por dictado'), findsNWidgets(3));
    expect(find.text('Coste'), findsNWidgets(3));
    expect(find.text('Descarga'), findsNWidgets(3));
    expect(find.text('Privacidad'), findsNWidgets(3));
    expect(find.text('Licencia'), findsNWidgets(3));
  });

  testWidgets('sin medida NO se inventa un número: dice «Sin medir»',
      (tester) async {
    await abrir(tester);
    // Es la diferencia con las barras «Speed 100 % / Acc 96 %» de FluidVoice.
    expect(find.text('Sin medir'), findsWidgets);
  });

  testWidgets('la lista de modelos es la que devuelve la MÁQUINA',
      (tester) async {
    await abrir(tester);

    await tester.tap(find.byKey(const ValueKey('modeloLocal')));
    await tester.pumpAndSettle();

    expect(find.textContaining('qwen3:1.7b'), findsWidgets);
    expect(find.textContaining('qwen2.5:7b'), findsWidgets);
  });

  testWidgets('elegir un modelo del desplegable lo guarda en los Ajustes',
      (tester) async {
    await abrir(tester);

    await tester.tap(find.byKey(const ValueKey('modeloLocal')));
    await tester.pumpAndSettle();
    await tester.tap(find.textContaining('qwen2.5:7b').last);
    await tester.pumpAndSettle();

    expect(container.read(settingsProvider).formatModel, 'qwen2.5:7b');
  });

  testWidgets(
      'NEGATIVA (el 404 del PRNT-18): Ollama sin el modelo configurado '
      'se dice con texto nuestro y se ofrecen los que sí hay', (tester) async {
    await abrir(
      tester,
      // El defecto de fábrica, que es justo el que puede no estar instalado.
      modeloConfigurado: 'qwen2.5:7b',
      respuesta: (_) async => http.Response(_tags(['llama3.2:1b']), 200),
    );

    // Se nombra el que falta...
    expect(find.textContaining('«qwen2.5:7b» no está en tu Ollama'),
        findsOneWidget);
    // ...y se ofrece lo que la máquina sí tiene.
    await tester.tap(find.byKey(const ValueKey('modeloLocal')));
    await tester.pumpAndSettle();
    expect(find.textContaining('llama3.2:1b'), findsWidgets);
  });

  testWidgets('sin Ollama: se dice, se ofrecen salidas y no hay error feo',
      (tester) async {
    await abrir(
      tester,
      respuesta: (_) async => throw const _RedCaida(),
    );

    expect(find.textContaining('No hay ningún Ollama respondiendo'),
        findsOneWidget);
    expect(find.textContaining('sin Formateo'), findsOneWidget);
    // Y los otros dos caminos siguen ahí para elegirlos.
    expect(find.text('Sin Formateo'), findsOneWidget);
    expect(find.text('Nube con tu clave'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('«Volver a mirar» vuelve a preguntarle a la máquina',
      (tester) async {
    var visitas = 0;
    await abrir(tester, respuesta: (_) async {
      visitas++;
      return http.Response(_tags(['qwen3:1.7b']), 200);
    });
    expect(visitas, 1);

    await tester.tap(find.byKey(const ValueKey('volverAMirarOllama')));
    await tester.pump(const Duration(milliseconds: 300));
    expect(visitas, 2, reason: 'si no vuelve a preguntar, el botón es inerte');
  });

  testWidgets('un modelo que no cabe en la memoria del equipo se dice',
      (tester) async {
    await abrir(
      tester,
      memoria: 2 * _gb,
      respuesta: (_) async => http.Response(
        jsonEncode({
          'models': [
            {
              'name': 'gigante:70b',
              'size': 40 * _gb,
              'capabilities': ['completion'],
            },
          ],
        }),
        200,
      ),
    );

    expect(find.textContaining('no caben en la memoria'), findsOneWidget);
  });

  testWidgets('elegir «Sin Formateo» lo guarda y avisa a quien lo pidió',
      (tester) async {
    var avisado = false;
    tester.view.physicalSize = const Size(900, 1400);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);

    final c = ProviderContainer(
      overrides: [
        systemMemoryProvider.overrideWithValue(const _MemoriaFija(32 * _gb)),
        ollamaClientProvider.overrideWithValue(
          OllamaClient(
            httpClient: MockClient(
              (_) async => http.Response(_tags(['qwen3:1.7b']), 200),
            ),
          ),
        ),
        settingsProvider.overrideWith(
          () => _AjustesFijos(
            AppSettings.defaults().copyWith(formatPath: FormatPath.local),
          ),
        ),
      ],
    );
    addTearDown(c.dispose);

    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: c,
        child: MaterialApp(
          theme: buildProntoTheme(),
          home: Scaffold(
            body: SingleChildScrollView(
              child: AssistantComparator(onElegido: () => avisado = true),
            ),
          ),
        ),
      ),
    );
    await tester.pump(const Duration(milliseconds: 300));

    await tester.tap(find.text('Sin Formateo'));
    await tester.pump(const Duration(milliseconds: 300));

    expect(c.read(settingsProvider).formatPath, FormatPath.none);
    expect(avisado, isTrue,
        reason: 'el asistente de instalación marca su bloque con esto');
  });

  testWidgets('sin clave, la nube NO se puede encender desde aquí',
      (tester) async {
    await abrir(tester, camino: FormatPath.none);

    await tester.tap(find.text('Nube con tu clave'));
    await tester.pump(const Duration(milliseconds: 300));

    expect(container.read(settingsProvider).formatPath, FormatPath.none,
        reason: 'encenderla sin clave dejaría un camino que falla al dictar');
    expect(find.textContaining('no hay ninguna clave guardada'), findsOneWidget);
  });

  testWidgets('en una ventana estrecha las tarjetas se apilan y siguen enteras',
      (tester) async {
    await abrir(tester, ancho: 420);

    expect(find.text('Sin Formateo'), findsOneWidget);
    expect(find.text('En tu equipo'), findsOneWidget);
    expect(find.text('Nube con tu clave'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets(
      'REGRESION: el modelo guardado escrito de otra forma («qwen3» contra '
      '«qwen3:latest») no revienta el desplegable', (tester) async {
    // El desplegable exige que su `value` sea EXACTAMENTE uno de sus items.
    // Con «qwen3» guardado y «qwen3:latest» instalado, pasarle la cadena del
    // ajuste tumbaba la pantalla de Ajustes entera con un assert de Flutter.
    await abrir(
      tester,
      modeloConfigurado: 'qwen3',
      respuesta: (_) async => http.Response(_tags(['qwen3:latest']), 200),
    );

    expect(tester.takeException(), isNull);
    expect(find.textContaining('no está en tu Ollama'), findsNothing,
        reason: 'es el mismo modelo, no uno que falte');
  });

  testWidgets(
      'REGRESION: un modelo instalado que NO cabe tampoco revienta, y se '
      'distingue de uno que falta', (tester) async {
    await abrir(
      tester,
      memoria: 4 * _gb,
      modeloConfigurado: 'gigante:70b',
      respuesta: (_) async => http.Response(
        jsonEncode({
          'models': [
            {
              'name': 'gigante:70b',
              'size': 40 * _gb,
              'capabilities': ['completion'],
            },
            {
              'name': 'chico:1b',
              'size': 1 * _gb,
              'capabilities': ['completion'],
            },
          ],
        }),
        200,
      ),
    );

    expect(tester.takeException(), isNull);
    expect(find.textContaining('no cabe en la memoria de este equipo'),
        findsOneWidget);
  });
}

class _AjustesFijos extends SettingsController {
  _AjustesFijos(this._inicial);

  final AppSettings _inicial;

  @override
  AppSettings build() => _inicial;
}

class _MemoriaFija implements SystemMemory {
  const _MemoriaFija(this._bytes);

  final int? _bytes;

  @override
  int? totalBytes() => _bytes;
}

class _RedCaida implements Exception {
  const _RedCaida();
}
