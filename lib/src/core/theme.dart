import 'package:flutter/material.dart';

ThemeData buildProntoTheme() {
  // Morado de la marca Pronto (muestreado del logo).
  const seed = Color(0xFF7800E3);
  return ThemeData(
    useMaterial3: true,
    colorScheme: ColorScheme.fromSeed(
      seedColor: seed,
      brightness: Brightness.dark,
    ),
    visualDensity: VisualDensity.compact,
  );
}
