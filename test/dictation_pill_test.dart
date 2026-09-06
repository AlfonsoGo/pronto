// ignore_for_file: require_trailing_commas
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:pronto/src/common/dictation_pill.dart';
import 'package:pronto/src/features/dictation/dictation_controller.dart';
import 'package:pronto/src/features/dictation/dictation_state.dart';

// Lo que la Píldora DICE en cada estado. Importa sobre todo uno: el único
// momento en el que algo de un dictado sale del ordenador tiene que verse sin
// ambigüedad, y eso solo se comprueba dibujándola.

/// Controlador de pega que se planta en un estado fijo. No arranca motor, ni
/// micrófono, ni atajos: solo hace falta el estado para pintar.
class _ControladorFijo extends DictationController {
  _ControladorFijo(this._estado);

  final DictationState _estado;

  @override
  DictationState build() => _estado;
}

Future<void> _pintar(WidgetTester tester, DictationStatus status) async {
  await tester.pumpWidget(
    ProviderScope(
      overrides: [
        dictationControllerProvider.overrideWith(
          () => _ControladorFijo(DictationState(status: status)),
        ),
      ],
      child: const MaterialApp(
        home: Scaffold(body: Center(child: DictationPill())),
      ),
    ),
  );
  await tester.pump();
}

void main() {
  testWidgets('«Saliendo del equipo» cuando el texto va camino de la nube',
      (tester) async {
    await _pintar(tester, DictationStatus.sendingToCloud);

    expect(find.text('Saliendo del equipo'), findsOneWidget);
    expect(find.byIcon(Icons.cloud_upload_rounded), findsOneWidget);

    // Y con su color propio: naranja, distinto del amarillo de "pensando".
    final icono = tester.widget<Icon>(
      find.byIcon(Icons.cloud_upload_rounded),
    );
    expect(icono.color, const Color(0xFFFF9500));
  });

  testWidgets('los demás estados NO dicen que algo sale del equipo',
      (tester) async {
    for (final status in [
      DictationStatus.idle,
      DictationStatus.recording,
      DictationStatus.transcribing,
      DictationStatus.translating,
      DictationStatus.injecting,
      DictationStatus.error,
      DictationStatus.uninitialized,
    ]) {
      await _pintar(tester, status);
      expect(find.text('Saliendo del equipo'), findsNothing,
          reason: 'estado ${status.name}');
      expect(find.byIcon(Icons.cloud_upload_rounded), findsNothing,
          reason: 'estado ${status.name}');
    }
  });

  testWidgets('cada estado tiene su etiqueta y ninguna se queda vacía',
      (tester) async {
    for (final status in DictationStatus.values) {
      await _pintar(tester, status);
      final textos = tester
          .widgetList<Text>(find.byType(Text))
          .map((t) => t.data ?? '')
          .where((t) => t.isNotEmpty);
      expect(textos, isNotEmpty, reason: 'estado ${status.name} sin etiqueta');
    }
  });
}
