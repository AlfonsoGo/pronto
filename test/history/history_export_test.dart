import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:pronto/src/features/history/dictation_entry.dart';
import 'package:pronto/src/features/history/history_export.dart';

/// Filas reales de un CSV: saltos de línea que caen fuera de comillas.
int _filas(String csv) {
  var dentro = false;
  var filas = 0;
  for (var i = 0; i < csv.length; i++) {
    final c = csv[i];
    if (c == '"') dentro = !dentro;
    if (c == '\n' && !dentro) filas++;
  }
  return filas;
}

void main() {
  DictationEntry entry({
    int id = 1,
    String raw = 'crudo',
    String finalText = 'Final',
    String? app,
    String? notice,
    double? costUsd,
  }) =>
      DictationEntry(
        id: id,
        at: DateTime(2026, 9, 6, 11, 42, 7),
        raw: raw,
        finalText: finalText,
        appName: app,
        formatNotice: notice,
        costUsd: costUsd,
        engine: 'parakeet',
        formatModel: 'qwen2.5:7b',
        spoken: const Duration(seconds: 4),
        transcribe: const Duration(milliseconds: 300),
        format: const Duration(milliseconds: 800),
      );

  group('CSV para Excel', () {
    test('empieza por el BOM: sin él Excel se come los acentos', () {
      final csv = toCsv([entry(finalText: 'Reunión')]);

      expect(csv.codeUnitAt(0), 0xFEFF);
    });

    test('separa por punto y coma, que es lo que espera el Excel español', () {
      final csv = toCsv([entry()]);
      final cabecera = csv.substring(1).split('\r\n').first;

      expect(cabecera.split(';').first, 'fecha');
      expect(cabecera, contains(';crudo;'));
    });

    test('los importes van con coma decimal', () {
      final csv = toCsv([entry(costUsd: 0.0021)]);

      expect(csv, contains('0,0021'));
    });

    test('un texto con punto y coma, comillas o salto de línea no rompe la '
        'columna', () {
      final csv = toCsv([
        entry(finalText: 'Uno; dos "tres"\ncuatro'),
      ]);
      // Lo que cuenta no es cuántos saltos hay, sino cuántos caen FUERA de
      // comillas: los de dentro son parte del texto y un lector de CSV los
      // respeta. Dos filas = cabecera + dictado.
      expect(_filas(csv), 2);
      expect(csv, contains('"Uno; dos ""tres""'));
    });

    test('el fichero escrito en disco empieza por los bytes EF BB BF', () {
      // El BOM tiene que llegar al DISCO, no solo a la cadena: es lo que mira
      // Excel al abrir. `writeAsString` codifica en UTF-8, así que el carácter
      // se convierte en esos tres bytes; si mañana alguien le pasa otra
      // codificación, este test lo caza.
      final tmp = Directory.systemTemp.createTempSync('pronto-csv-');
      addTearDown(() => tmp.deleteSync(recursive: true));
      final file = File('${tmp.path}/historial.csv')
        ..writeAsStringSync(toCsv([entry(finalText: 'Reunión')]));

      final bytes = file.readAsBytesSync();
      expect(bytes.take(3), [0xEF, 0xBB, 0xBF]);
      expect(utf8.decode(bytes.skip(3).toList()), contains('Reunión'));
    });

    test('los acentos sobreviven de ida y vuelta en UTF-8', () {
      final csv = toCsv([entry(finalText: 'La reunión con Ñoño y Óscar')]);
      final bytes = utf8.encode(csv);

      expect(utf8.decode(bytes), contains('reunión con Ñoño y Óscar'));
    });

    test('el aviso que sale es el NUESTRO, no el del proveedor', () {
      final csv = toCsv([
        entry(notice: 'Sin formatear: tope de gasto diario alcanzado'),
      ]);

      expect(csv, contains('Sin formatear: tope de gasto diario alcanzado'));
    });
  });

  group('JSON', () {
    test('vuelve a leerse como lista de objetos con sus campos', () {
      final json = toJson([entry(app: 'Outlook', costUsd: 0.5)]);
      final leido = jsonDecode(json) as List<dynamic>;

      final fila = leido.single as Map<String, dynamic>;
      expect(fila['crudo'], 'crudo');
      expect(fila['final'], 'Final');
      expect(fila['app'], 'Outlook');
      expect(fila['coste_usd'], 0.5);
      expect(fila['fecha'], '2026-09-06T11:42:07');
    });

    test('un campo que no se sabe sale como null, no como cadena vacía', () {
      final fila = (jsonDecode(toJson([entry()])) as List).single
          as Map<String, dynamic>;

      expect(fila['app'], isNull);
      expect(fila['modo'], isNull);
    });
  });

  group('el aviso de lo que lleva el fichero', () {
    test('dice cuántas entradas y que va sin cifrar', () {
      final aviso = exportWarning(42);

      expect(aviso, contains('42'));
      expect(aviso.toLowerCase(), contains('sin cifrar'));
    });
  });
}
