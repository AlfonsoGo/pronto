// ignore_for_file: require_trailing_commas
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:pronto/src/features/formatting/format_preset.dart';
import 'package:pronto/src/features/formatting/modelos_locales.dart';

void main() {
  group('los topes del guardia son del Preset (ADR 0004)', () {
    test('«Limpio» trae los suyos, medidos', () {
      expect(FormatPreset.limpio.maxAddedContent, 0.20);
      expect(FormatPreset.limpio.maxDeletedContent, 0.60);
      expect(FormatPreset.limpio.medidoSobre, contains('443'));
    });

    test('ningún Preset puede existir sin decir sobre qué se midió', () {
      // Esta es la puerta que abre el ADR 0004: añadir un Preset ya no es
      // añadir una fila a un enum, es traer dos números medidos con SU corpus.
      // «Nota» reestructura en viñetas a propósito y con los topes de «Limpio»
      // se le descartaría la salida buena.
      for (final p in const [FormatPreset.limpio]) {
        expect(p.medidoSobre.trim(), isNotEmpty, reason: p.id);
        expect(p.prompt.trim(), isNotEmpty, reason: p.id);
        expect(p.maxAddedContent, greaterThan(0), reason: p.id);
        expect(p.maxDeletedContent, greaterThan(0), reason: p.id);
      }
    });

    test('los topes YA NO viven en AppConfig', () async {
      // Guardia estática: mientras existiera la constante global, el siguiente
      // Preset la volvería a usar «porque estaba ahí» y heredaría los topes de
      // un trabajo que no es el suyo.
      final config = File('lib/src/core/config.dart').readAsStringSync();
      expect(
        config,
        isNot(contains('static const double formatMaxAddedContent')),
      );
      expect(
        config,
        isNot(contains('static const double formatMaxDeletedContent')),
      );
    });
  });

  group('la tabla de modelos medidos no se escribe a mano', () {
    test('la tabla NO está vacía', () {
      // Sin esto, los tres tests de abajo son bucles sobre una lista vacía:
      // verde perpetuo que no comprueba nada.
      expect(kModelosMedidos, isNotEmpty);
    });

    test('toda fila trae corpus, fecha, equipo y licencia verificada', () {
      // Es lo que separa nuestra tabla de las barras «Speed 100 % / Acc 96 %»
      // de FluidVoice, que salen de una lista escrita a mano en su código.
      for (final m in kModelosMedidos) {
        expect(m.dictados, greaterThan(0), reason: m.tag);
        expect(m.medidoEl, matches(RegExp(r'^\d{4}-\d{2}-\d{2}$')),
            reason: m.tag);
        expect(m.licenciaVerificadaEl, matches(RegExp(r'^\d{4}-\d{2}-\d{2}$')),
            reason: m.tag);
        expect(m.equipo.trim(), isNotEmpty, reason: m.tag);
        expect(m.fichaUrl, startsWith('https://'), reason: m.tag);
        expect(m.terLimpio, greaterThanOrEqualTo(0), reason: m.tag);
        expect(m.segundosMediana, greaterThan(0), reason: m.tag);
      }
    });

    test('solo entra un modelo con licencia libre y comprobada', () {
      // Pronto es MIT y público. Proponer —y en el PRNT-19, descargar— un
      // modelo de licencia restrictiva es un problema NUESTRO, no del usuario.
      const permitidas = {'apache-2.0', 'mit'};
      for (final m in kModelosMedidos) {
        expect(permitidas, contains(m.licencia.toLowerCase()), reason: m.tag);
      }
    });

    test('modeloMedido() tolera el «:latest» implícito', () {
      expect(modeloMedido('no-existe-de-verdad'), isNull);
      for (final m in kModelosMedidos) {
        expect(modeloMedido(m.tag), isNotNull);
      }
    });
  });
}
