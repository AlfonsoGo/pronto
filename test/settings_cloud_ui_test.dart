// ignore_for_file: require_trailing_commas
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:pronto/src/features/formatting/cloud_key.dart';
import 'package:pronto/src/features/formatting/text_formatter.dart';
import 'package:pronto/src/features/settings/settings_controller.dart';
import 'package:pronto/src/features/settings/settings_screen.dart';
import 'package:pronto/src/features/update/update_controller.dart';
import 'package:pronto/src/platform/credential_store.dart';

// El camino de ACTIVACIÓN de la nube, tal y como lo recorre el usuario en
// Ajustes. Un interruptor que existe en el código pero que nadie ha accionado
// nunca no está probado: esto lo acciona.
//
// Tres sustituciones, y solo tres: el almacén de credenciales (para no escribir
// en el Administrador de credenciales de quien corra los tests), el buscador de
// actualizaciones (deja un temporizador de dos horas que revienta el test al
// desmontar) y la persistencia de los ajustes (el plugin de preferencias no
// existe en un test y su futuro no vuelve nunca). Todo lo demás es la pantalla
// de verdad.

void main() {
  late InMemoryCredentialStore almacen;
  late ProviderContainer container;

  Future<void> abrirAjustes(
    WidgetTester tester, {
    FormatPath camino = FormatPath.none,
  }) async {
    // Pantalla ALTA a propósito: así la lista entra entera y no hay que ir
    // haciendo scroll (con las animaciones de la píldora, esperar a que la
    // pantalla "se asiente" no termina nunca).
    tester.view.physicalSize = const Size(1400, 6000);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);

    almacen = InMemoryCredentialStore();
    container = ProviderContainer(
      overrides: [
        credentialStoreProvider.overrideWithValue(almacen),
        updateProvider.overrideWith(_SinActualizaciones.new),
        settingsProvider.overrideWith(
          () => _AjustesFijos(
            AppSettings.defaults().copyWith(formatPath: camino),
          ),
        ),
      ],
    );
    addTearDown(container.dispose);

    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: const MaterialApp(home: SettingsScreen()),
      ),
    );
    await tester.pump(const Duration(milliseconds: 400));
  }

  testWidgets('con «sin formateo» no se ve nada de la nube', (tester) async {
    await abrirAjustes(tester);

    expect(find.byKey(const ValueKey('formatPath')), findsOneWidget);
    expect(find.byKey(const ValueKey('cloudApiKey')), findsNothing);
    expect(find.byKey(const ValueKey('cloudDayCapUsd')), findsNothing);
    expect(find.textContaining('sale de tu equipo'), findsNothing);
  });

  testWidgets('con la nube aparecen el aviso, la clave y los topes',
      (tester) async {
    await abrirAjustes(tester, camino: FormatPath.cloud);

    // El aviso de que el texto sale del equipo, con todas las letras.
    expect(find.textContaining('sale de tu equipo'), findsOneWidget);
    expect(find.textContaining('Tu voz no sale nunca'), findsOneWidget);

    // Y los controles que hacen que eso sea seguro.
    expect(find.byKey(const ValueKey('cloudApiKey')), findsOneWidget);
    expect(find.byKey(const ValueKey('cloudModel')), findsOneWidget);
    expect(find.byKey(const ValueKey('cloudDayCapUsd')), findsOneWidget);
    expect(find.byKey(const ValueKey('cloudMonthCapUsd')), findsOneWidget);
    expect(find.textContaining('Este mes:'), findsOneWidget);
  });

  testWidgets('guardar la clave: va al almacén, recortada, y la app se entera',
      (tester) async {
    await abrirAjustes(tester, camino: FormatPath.cloud);

    expect(container.read(cloudKeyPresenceProvider), isFalse);
    expect(find.textContaining('Sin clave no sale ni una petición'),
        findsOneWidget);

    await tester.enterText(
      find.byKey(const ValueKey('cloudApiKey')),
      '  sk-or-v1-de-prueba  ',
    );
    await tester.tap(find.byKey(const ValueKey('cloudApiKeySave')));
    await tester.pump(const Duration(milliseconds: 400));

    expect(almacen.read(CloudKey.target), 'sk-or-v1-de-prueba');
    expect(container.read(cloudKeyPresenceProvider), isTrue);

    // El campo se ha vaciado y va oculto: la clave no se queda en pantalla.
    final campo = tester.widget<TextField>(
      find.byKey(const ValueKey('cloudApiKey')),
    );
    expect(campo.controller?.text, isEmpty);
    expect(campo.obscureText, isTrue);
  });

  testWidgets('borrar la clave la quita del almacén', (tester) async {
    await abrirAjustes(tester, camino: FormatPath.cloud);

    await tester.enterText(
      find.byKey(const ValueKey('cloudApiKey')),
      'sk-or-v1-de-prueba',
    );
    await tester.tap(find.byKey(const ValueKey('cloudApiKeySave')));
    await tester.pump(const Duration(milliseconds: 400));
    expect(almacen.read(CloudKey.target), isNotNull);

    await tester.tap(find.byKey(const ValueKey('cloudApiKeyDelete')));
    await tester.pump(const Duration(milliseconds: 400));

    expect(almacen.read(CloudKey.target), isNull);
    expect(container.read(cloudKeyPresenceProvider), isFalse);
  });

  testWidgets('el tope se escribe con coma; lo que no es número no lo toca',
      (tester) async {
    await abrirAjustes(tester, camino: FormatPath.cloud);

    await tester.enterText(
      find.byKey(const ValueKey('cloudDayCapUsd')),
      '0,25',
    );
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pump(const Duration(milliseconds: 400));
    expect(container.read(settingsProvider).cloudDayCapUsd, 0.25);

    await tester.enterText(
      find.byKey(const ValueKey('cloudDayCapUsd')),
      'barra libre',
    );
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pump(const Duration(milliseconds: 400));
    expect(container.read(settingsProvider).cloudDayCapUsd, 0.25,
        reason: 'lo que no es un número no puede tocar el tope');

    // Y un dedazo de dos ceros de más tampoco.
    await tester.enterText(
      find.byKey(const ValueKey('cloudDayCapUsd')),
      '100',
    );
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pump(const Duration(milliseconds: 400));
    expect(container.read(settingsProvider).cloudDayCapUsd, 0.25);
  });
}

/// Ajustes que arrancan con un valor fijo y no leen de disco.
class _AjustesFijos extends SettingsController {
  _AjustesFijos(this._inicial);

  final AppSettings _inicial;

  @override
  AppSettings build() => _inicial;
}

/// Buscador de actualizaciones que no busca nada: sin temporizadores.
class _SinActualizaciones extends UpdateController {
  @override
  UpdateState build() => const UpdateState();
}
