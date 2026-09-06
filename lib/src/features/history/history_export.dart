/// Sacar el historial a un fichero.
///
/// Es el corpus con el que se mide el formateo (PRNT-1) y el vocabulario
/// (PRNT-12/17), así que **el formato se decide aquí una vez**: si cambia
/// después, hay que rehacer todas las medidas hechas con el anterior.
///
/// Columnas (CSV) y claves (JSON), en este orden y con estos nombres:
/// `fecha, modo, app, ventana, motor, formateo, modelo, crudo, final,
/// palabras, s_hablados, ms_motor, ms_formateo, coste_usd, aviso`.
library;

import 'dart:convert';

import 'dictation_entry.dart';

/// Separador de columnas: **punto y coma**.
///
/// El Excel en español usa la coma como separador DECIMAL, así que un CSV
/// separado por comas se abre en una sola columna. Esto no es una preferencia:
/// es la diferencia entre que el fichero se abra bien o no.
const String kCsvSeparator = ';';

/// Marca de orden de bytes. Sin ella Excel abre el UTF-8 como si fuera la
/// página de códigos de Windows y «reunión» sale «reuniÃ³n».
const String kUtf8Bom = '﻿';

const List<String> _headers = [
  'fecha',
  'modo',
  'app',
  'ventana',
  'motor',
  'formateo',
  'modelo',
  'crudo',
  'final',
  'palabras',
  's_hablados',
  'ms_motor',
  'ms_formateo',
  'coste_usd',
  'aviso',
];

/// El historial como CSV listo para abrir en Excel.
String toCsv(List<DictationEntry> entries) {
  final buffer = StringBuffer(kUtf8Bom)
    ..write(_headers.join(kCsvSeparator))
    ..write('\r\n');
  for (final e in entries) {
    buffer
      ..write([
        _isoDate(e.at),
        e.mode ?? '',
        e.appName ?? '',
        e.windowTitle ?? '',
        e.engine ?? '',
        e.formatPath ?? '',
        e.formatModel ?? '',
        e.raw,
        e.finalText,
        '${e.wordCount}',
        _decimal(e.spoken == null ? null : e.spoken!.inMilliseconds / 1000),
        '${e.transcribe?.inMilliseconds ?? ''}',
        '${e.format?.inMilliseconds ?? ''}',
        _decimal(e.costUsd),
        e.formatNotice ?? '',
      ].map(_csvField).join(kCsvSeparator),)
      ..write('\r\n');
  }
  return buffer.toString();
}

/// El historial como JSON. Lo que no se sabe va como `null`, no como cadena
/// vacía: un hueco y un dato vacío no son lo mismo cuando luego se mide.
String toJson(List<DictationEntry> entries) => jsonEncode([
      for (final e in entries)
        {
          'fecha': _isoDate(e.at),
          'modo': e.mode,
          'app': e.appName,
          'ventana': e.windowTitle,
          'motor': e.engine,
          'formateo': e.formatPath,
          'modelo': e.formatModel,
          'crudo': e.raw,
          'final': e.finalText,
          'palabras': e.wordCount,
          's_hablados':
              e.spoken == null ? null : e.spoken!.inMilliseconds / 1000,
          'ms_motor': e.transcribe?.inMilliseconds,
          'ms_formateo': e.format?.inMilliseconds,
          'coste_usd': e.costUsd,
          'aviso': e.formatNotice,
        },
    ]);

/// Lo que se le dice al usuario ANTES de escribir el fichero.
///
/// El historial lleva dentro todo lo que ha dictado —correos, clientes, lo que
/// dijera en voz alta sin pensar— y el fichero que sale de aquí no está
/// cifrado. Sacarlo de la app es una decisión, así que se toma con el dato
/// delante.
String exportWarning(int count) =>
    'Vas a guardar $count dictados en un fichero sin cifrar: lleva el texto '
    'completo de todo lo que has dictado, con los nombres y las ventanas donde '
    'lo pegaste. Guárdalo donde guardarías un documento privado.';

/// Un campo de CSV: se entrecomilla si lleva separador, comillas o saltos de
/// línea, y las comillas de dentro se duplican.
String _csvField(String value) {
  final necesita = value.contains(kCsvSeparator) ||
      value.contains('"') ||
      value.contains('\n') ||
      value.contains('\r');
  if (!necesita) return value;
  return '"${value.replaceAll('"', '""')}"';
}

/// Un número como lo escribe España: coma decimal. Vacío si no hay dato.
String _decimal(double? value) {
  if (value == null) return '';
  return value
      .toStringAsFixed(4)
      .replaceAll(RegExp(r'0+$'), '')
      .replaceAll(RegExp(r'\.$'), '')
      .replaceAll('.', ',');
}

/// `2026-09-06T11:42:07`, sin zona: la base guarda hora local y añadir una Z
/// diría que es UTC.
String _isoDate(DateTime d) {
  String dos(int n) => n.toString().padLeft(2, '0');
  return '${d.year}-${dos(d.month)}-${dos(d.day)}T'
      '${dos(d.hour)}:${dos(d.minute)}:${dos(d.second)}';
}
