import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../platform/platform_services.dart';
import '../settings/settings_controller.dart';
import 'cloud_budget_provider.dart';
import 'cloud_key.dart';
import 'ollama_catalog.dart';
import 'text_formatter.dart';

/// Construye el [TextFormatter] a partir de los ajustes del usuario.
///
/// Se reconstruye cuando cambian el camino de formateo, la URL o el modelo. El
/// cliente HTTP previo se libera con [TextFormatter.dispose].
///
/// 🔒 La clave de nube NO se le pasa como valor: se le pasa la FORMA de leerla,
/// y el formateador la pide al almacén de credenciales justo cuando va a mandar
/// una petición. Así la clave no vive dentro de un provider —que cualquier
/// volcado de estado imprimiría— y cambiarla surte efecto en el dictado
/// siguiente sin reconstruir nada.
///
/// El guardia del gasto va SIEMPRE puesto aquí: el camino de nube sin tope solo
/// existe para el banco de pruebas de `tools/`, nunca para la app.
final textFormatterProvider = Provider<TextFormatter>((ref) {
  final s = ref.watch(settingsProvider);
  final formatter = TextFormatter(
    path: s.formatPath,
    localBaseUrl: s.llmBaseUrl,
    localModel: s.formatModel,
    cloudModel: s.cloudModel,
    cloudApiKey: ref.watch(cloudKeyProvider).read,
    budget: ref.watch(cloudSpendProvider.notifier),
  );
  ref.onDispose(formatter.dispose);
  return formatter;
});

/// Quien le pregunta a Ollama qué tiene. Vive toda la sesión.
final ollamaClientProvider = Provider<OllamaClient>((ref) {
  final client = OllamaClient();
  ref.onDispose(client.dispose);
  return client;
});

/// Los modelos con los que ESTA máquina puede formatear en local.
///
/// Se le pregunta al servidor cada vez que hace falta, y no se guarda ninguna
/// marca: igual que el asistente de instalación mira el disco en vez de fiarse
/// de un «ya lo descargaste», aquí se mira Ollama en vez de fiarse de un
/// nombre escrito hace meses en los Ajustes. El modelo pudo borrarse ayer.
///
/// `autoDispose` a propósito: al cerrar la pantalla se olvida, y al volver a
/// abrirla se vuelve a preguntar. Para releer sin cerrar nada, invalídalo.
final ollamaCatalogProvider = FutureProvider.autoDispose<OllamaCatalog>(
  (ref) async {
    final base = ref.watch(settingsProvider.select((s) => s.llmBaseUrl));
    final memoria = ref.watch(systemMemoryProvider).totalBytes();
    return ref.watch(ollamaClientProvider).catalogo(base, memoriaBytes: memoria);
  },
);
