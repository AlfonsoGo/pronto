import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:window_manager/window_manager.dart';

import 'src/core/app.dart';
import 'src/features/dictation/dictation_controller.dart';
import 'src/features/overlay/window_mode_controller.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();

  if (Platform.isWindows || Platform.isMacOS || Platform.isLinux) {
    await windowManager.ensureInitialized();

    // Arrancamos directamente como barra flotante (pill): pequeña, sin barra de
    // título, siempre encima, sin entrada en la barra de tareas y con fondo
    // transparente para que solo se vea la píldora.
    const options = WindowOptions(
      size: kPillIdle,
      backgroundColor: Colors.transparent,
      skipTaskbar: true,
      alwaysOnTop: true,
      titleBarStyle: TitleBarStyle.hidden,
      windowButtonVisibility: false,
      title: 'Pronto',
    );

    windowManager.waitUntilReadyToShow(options, () async {
      await windowManager.setBackgroundColor(Colors.transparent);
      // Frameless: ventana sin marco para que solo se vea el punto.
      await windowManager.setAsFrameless();
      await windowManager.setResizable(false);
      await windowManager.setAlignment(Alignment.bottomCenter);
      // show() SIN focus(): no robamos el foco a la app en la que dictas.
      await windowManager.show();
    });
  }

  runApp(
    ProviderScope(
      child: Consumer(
        builder: (context, ref, _) {
          // Instancia el controlador del dictado al arrancar (carga modelo,
          // registra el atajo global, etc.).
          ref.watch(dictationControllerProvider);
          return const ProntoApp();
        },
      ),
    ),
  );
}
