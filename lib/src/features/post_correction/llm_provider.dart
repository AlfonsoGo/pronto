import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../settings/settings_controller.dart';
import 'llm_corrector.dart';

/// Construye el [LlmCorrector] a partir de los ajustes del usuario.
///
/// Se reconstruye cuando cambian los ajustes (activar/desactivar LLM, URL o
/// modelo). El cliente HTTP previo se libera con [LlmCorrector.dispose].
///
/// MVP: usa el endpoint nativo de Ollama (useCloud=false). La opción de nube
/// (OpenAI-compatible con API key) queda como extensión — ver ROADMAP.md.
final llmCorrectorProvider = Provider<LlmCorrector>((ref) {
  final s = ref.watch(settingsProvider);
  final corrector = LlmCorrector(
    enabled: s.llmEnabled,
    baseUrl: s.llmBaseUrl,
    model: s.llmModel,
    useCloud: false,
  );
  ref.onDispose(corrector.dispose);
  return corrector;
});
