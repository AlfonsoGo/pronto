# Ideas de Glaido, Wispr Flow y Gladia para Pronto 2 (PRNT-45, 9-nov-2026)

Fuentes (leídas el 9-nov-2026): wisprflow.ai/features · wisprflow.ai/whats-new (hasta 30-oct-2026) ·
glaido.com · spokenly.app/blog/glaido-review (26-sep-2026, la escribe un competidor) · gladia.io/pricing ·
gladia.io/blog/best-speech-to-text-apis. Lo que Pronto 2 ya tiene se comprobó con búsquedas en `src-tauri/src` y `ui/`,
no ejecutando la app.

Nota: «gladio» se interpretó como **Glaido** (app de dictado rival de Wispr, Mac/Windows 11/Linux). **Gladia** es otra
cosa: una API de transcripción en la nube para programadores. Se miran las dos.

## Ya lo tiene Pronto 2 (no hace falta copiar)
Dictado en cualquier app · manos libres con doble toque · autocorrección al hablar («backtrack», dentro del nivel de
limpieza) · estilos por tipo de app · diccionario · fragmentos · reescribir texto seleccionado con Enter para pegar ·
notas de reunión sin bot con quién dijo qué · preguntar sobre reuniones · audio de cada dictado 14 días · elegir micro ·
estadísticas · modo oscuro · novedades dentro de la app.

## Lo que no tiene y merece la pena (orden: utilidad / coste)

| # | Función | De | Coste | Choca con «todo local» |
|---|---|---|---|---|
| 1 | Aviso al empezar si el micro no recoge sonido | Wispr (9-oct) | bajo | no |
| 2 | No actualizar mientras dictas o hay reunión; esperar a que el PC esté quieto | Wispr (4-sep) | bajo | no |
| 3 | Etiquetas en fragmentos: `{fecha}`, `{portapapeles}` | Glaido | bajo | no |
| 4 | Importar el diccionario de Wispr Flow al instalar | Glaido | bajo | no (fichero local) |
| 5 | Aprender palabras solas cuando corriges un dictado | Wispr / Glaido | medio | no |
| 6 | Reglas por app o web concreta (Gmail web, Slack…) con instrucción propia de hasta 500 caracteres | Glaido | medio | no (Ollama) |
| 7 | Modo comando sin selección: «escríbeme un correo para…» y lo redacta en el cursor | Wispr / Glaido | medio | no (Ollama) |
| 8 | Plantillas de preguntas para reuniones («habilidades» en Markdown) que se lanzan sobre una o todas | Wispr (30-oct) | medio | no |
| 9 | Transcribir un archivo de audio o vídeo, con exportar a SRT | Spokenly (Glaido no lo tiene) | medio | no |
| 10 | Servidor MCP local: Claude Code o Claude Desktop leen tus reuniones | Wispr Notetaker / Glaido | medio | ⚠ el dato sale si el usuario lo conecta a una IA de nube: decidir |
| 11 | «¿Qué me he perdido?» durante la reunión | Wispr | alto (hoy solo graba durante la llamada) | no |
| 12 | Tapar datos personales (nombres, correos, teléfonos) al exportar o compartir notas | Gladia (PII redaction) | medio | no |
| 13 | Cambiar de idioma dentro de la misma frase (español + inglés) | Gladia (code-switching) | alto: depende del motor | no |

## Lo que NO se copia
- Búsqueda web, leer páginas, «deep research» (Glaido): manda el texto a la nube. Rompe la regla 1.
- Extras tipo selector de color, historial del portapapeles, mantener despierto (Glaido): fuera del producto.
- Equipos, ranking, SSO, diccionario compartido (Wispr): Pronto no tiene cuentas.
- Nombres de hablantes desde el calendario de Google/Outlook (Wispr): exige conectar cuentas en la nube.
- Modo susurro y su modelo propio «Canto» (Wispr): es un modelo cerrado; depende de que Parakeet/Whisper lo aguanten.
- La API de Gladia en sí: es nube y de pago (0,02-0,07 €/h por cada extra). Solo se copian ideas.

## Datos de contexto
- Glaido: Whisper en la nube + un segundo modelo que formatea. Pro 20 $/mes. Sin modo sin conexión. Grabaciones de
  10 min como máximo. Historial: últimos 100 dictados, cifrados en el equipo.
- Wispr Flow: Pro 15 $/mes. Modelo propio «Canto» desde el 17-sep-2026. Notetaker en Windows desde el 15-sep-2026,
  transcribe reuniones en 21 idiomas.
- Ventaja de Pronto contra los dos: gratis, sin cuenta y sin conexión. Ninguno de los dos funciona sin Internet.
