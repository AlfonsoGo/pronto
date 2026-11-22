# Cola — Wispr Flow (+ Glaido) → Pronto 2   (preparada 9-nov-2026, PRNT-46 a PRNT-49)

Artefacto Wispr: `%LOCALAPPDATA%\WisprFlow\app-1.6.1102\resources\app.asar` (Electron; el hash se rellena en la Fase A).
Glaido: NO instalado el 9-nov-2026 (pide cuenta para descargar: la crea Txonso). Gladia y Spokenly: sin artefacto.
Parent evidence: (vacío: se rellena en la Fase A).

## Cómo se usa (guion para la sesión que construya)

1. Encender REA: `python -I ~/.claude/mcp-apagados/rea.py on` y abrir una sesión NUEVA en `BELTRANLABS2\ProntoV2`.
2. Lanzar `/clonar-app` con esta cola. **Fase A reducida:** analizar el `.asar` una vez (`detail: "summary"`) y
   rellenar las columnas Seed y Obs. de las filas de abajo. La lista de funciones NO se cambia: la eligió Txonso.
   Si REA delata algo nuevo, se añade al final como `pendiente`.
3. **Fase B**, una fila cada vez, en orden: ledger `n-<función>/EVIDENCIA.md` (visto / inferido / desconocido) →
   ajustar la ficha `specs/features/fr-NN-rd.md` SOLO con lo visto → construir en Pronto 2 con código propio →
   test de la ficha + `npm test` (incluye `tests/no-se-negocia/`) → checkpoint y preguntar a Txonso la siguiente.
4. Al cerrar cada tanda (cada ticket): `/fases` en Pronto 2 (ruta «cambio»). Un rojo de `/fases` tumba el cierre.
5. Al acabar: `python -I ~/.claude/mcp-apagados/rea.py off`. Nada se sube sin que Txonso diga «sube».

## Frenos para no cometer fallos
- **Nunca copiar código, textos, iconos ni marca de Wispr o Glaido.** Se copia el comportamiento, escrito por nosotros.
- **Nada inferido se construye como hecho.** Si una fila depende de lo que pasa en el servidor de Wispr, se marca
  «desconocido» y la ficha decide el diseño por su cuenta, dicho en voz alta.
- **No iniciar sesión en Wispr ni en Glaido desde Pronto, ni llamar a sus servidores** (regla 1).
- **No ejecutar Wispr ni Glaido sin el sí de Txonso** (`capture_*_scenario` corre con sus permisos y su red).
- No tocar `%APPDATA%\Pronto2` (datos reales); para probar, una copia de `copias\` en carpeta temporal.
- Cada fila: test que la active de verdad (un control que no se puede activar no existe).

| # | Función | Pregunta verificable | Seed | Obs. | Depende de | Ficha | Ticket | Estado | Resultado |
|---|---|---|---|---|---|---|---|---|---|
| 1 | Aviso de micro mudo | ¿Cuándo y con qué umbral avisa Wispr de que el micro no recoge sonido? | por Fase A | alta | — | fr-19 | PRNT-46 | pendiente | |
| 2 | No actualizar mientras trabajas | ¿Qué señales usa Wispr para saber que estás trabajando antes de instalar? | por Fase A | alta | — | fr-20 | PRNT-46 | pendiente | |
| 3 | Etiquetas en fragmentos | ¿Qué etiquetas acepta y cuándo se sustituyen? | Glaido (no instalado) | baja | — | fr-21 | PRNT-46 | pendiente | |
| 4 | Importar diccionario de Wispr | ¿Dónde y en qué formato guarda Wispr el diccionario en este PC? | por Fase A | alta | — | fr-22 | PRNT-46 | pendiente | |
| 5 | Aprender de mis correcciones | ¿Cómo detecta Wispr que corregiste una palabra pegada? | por Fase A | media | — | fr-23 | PRNT-47 | pendiente | |
| 6 | Reglas por app o web | ¿Cómo identifica la app y la web activas para aplicar un estilo? | por Fase A | alta | — | fr-24 | PRNT-47 | pendiente | |
| 7 | Modo comando sin selección | ¿Qué hace Command Mode cuando no hay texto seleccionado? | por Fase A | media | — | fr-25 | PRNT-47 | pendiente | |
| 8 | Plantillas de preguntas | ¿Cómo guarda y lanza Wispr sus «skills» de reuniones? | por Fase A | media | — | fr-26 | PRNT-48 | pendiente | |
| 9 | Tapar datos al exportar | (sin artefacto: diseño propio, idea de Gladia) | — | — | — | fr-27 | PRNT-48 | pendiente | |
| 10 | MCP local de reuniones | ¿Qué herramientas expone el MCP de Notetaker y con qué permisos? | por Fase A | baja (remoto) | decisión de Txonso | fr-28 | PRNT-48 | bloqueada | espera decisión regla 1 |
| 11 | «¿Qué me he perdido?» | ¿Transcribe en directo en el cliente o en su nube? | por Fase A | baja | — | fr-29 | PRNT-48 | pendiente | |
| 12 | Transcribir archivo suelto | (sin artefacto: diseño propio) | — | — | — | fr-30 | PRNT-49 | pendiente | |
| 13 | Exportar SRT | (sin artefacto: diseño propio) | — | — | 12 | fr-31 | PRNT-49 | pendiente | |
