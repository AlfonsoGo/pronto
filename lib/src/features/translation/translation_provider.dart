import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../settings/settings_controller.dart';
import 'translator.dart';

/// Construye el [Translator] a partir de los ajustes del usuario.
///
/// Se reconstruye cuando cambian los ajustes de traducción (activar, URL o
/// modelo); el cliente HTTP anterior se libera con [Translator.dispose].
/// Comparte la URL del servidor con la post-corrección: es el mismo Ollama /
/// servidor compatible con OpenAI, solo cambia el modelo.
final translatorProvider = Provider<Translator>((ref) {
  final s = ref.watch(settingsProvider);
  final translator = Translator(
    enabled: s.translationEnabled,
    baseUrl: s.llmBaseUrl,
    model: s.translationModel,
  );
  ref.onDispose(translator.dispose);
  return translator;
});
