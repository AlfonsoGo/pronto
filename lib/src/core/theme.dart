import 'package:flutter/material.dart';

/// Tema de Pronto: oscuro con morado de marca, inspirado en el diseño "Pronto
/// App" (fondo casi negro violáceo, tarjetas muy oscuras, acentos violeta y un
/// toque amarillo). Partimos de un ColorScheme generado por semilla y solo
/// fijamos los tonos clave, así los widgets existentes heredan el aspecto.
ThemeData buildProntoTheme() {
  // Paleta del diseño.
  const violet = Color(0xFF8B5CF6); // botones/acento principal
  const violetDeep = Color(0xFF6D28D9); // gradiente botón
  const violetSoft = Color(0xFFA78BFA); // iconos/encabezados de sección
  const yellow = Color(0xFFFDE047); // chispa de acento (puntos, ondas)
  const bg = Color(0xFF0B0810); // fondo de la app
  const surface = Color(0xFF0F0B16); // tarjetas
  const surfaceHigh = Color(0xFF161021); // tarjetas/inputs elevados
  const ink = Color(0xFFF5F3FF); // texto principal
  const muted = Color(0xFF9C93AE); // texto secundario

  final base = ColorScheme.fromSeed(
    seedColor: violet,
    brightness: Brightness.dark,
  );
  final scheme = base.copyWith(
    primary: violet,
    onPrimary: Colors.white,
    primaryContainer: violetDeep,
    onPrimaryContainer: const Color(0xFFEDE9FE),
    secondary: violetSoft,
    onSecondary: const Color(0xFF1A0B2E),
    secondaryContainer: const Color(0xFF2A1A47),
    onSecondaryContainer: const Color(0xFFEDE9FE),
    tertiary: yellow,
    onTertiary: const Color(0xFF1A1500),
    surface: bg,
    onSurface: ink,
    surfaceContainerLowest: const Color(0xFF080510),
    surfaceContainerLow: surface,
    surfaceContainer: surface,
    surfaceContainerHigh: surfaceHigh,
    surfaceContainerHighest: const Color(0xFF1C1530),
    onSurfaceVariant: muted,
    outline: violetSoft.withValues(alpha: 0.22),
    outlineVariant: violetSoft.withValues(alpha: 0.12),
  );

  return ThemeData(
    useMaterial3: true,
    colorScheme: scheme,
    scaffoldBackgroundColor: bg,
    visualDensity: VisualDensity.compact,
    appBarTheme: const AppBarTheme(
      backgroundColor: bg,
      surfaceTintColor: Colors.transparent,
      elevation: 0,
      scrolledUnderElevation: 0,
    ),
    cardTheme: CardThemeData(
      color: surface,
      elevation: 0,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(18)),
    ),
    filledButtonTheme: FilledButtonThemeData(
      style: FilledButton.styleFrom(
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(14)),
        padding: const EdgeInsets.symmetric(horizontal: 18, vertical: 14),
      ),
    ),
  );
}
