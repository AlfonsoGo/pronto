# Los Presets son nuestros, no los de Superwhisper

Superwhisper reparte sus modos en Super · Voz a texto · Mensaje · Correo · Nota · Reunión ·
Custom. Copiamos la **estructura** (un Modo nace de una plantilla que fija su Prompt) pero no la
lista, porque la nuestra tiene que estar escrita para el español y para dónde se dicta aquí. Los
Presets son siete:

| Preset | Qué hace |
|---|---|
| **Crudo** | Transcribe y pega. Sin Modelo de formateo y sin espera. Es el camino rápido. |
| **Limpio** | Puntuación, tildes, concordancia, mayúsculas y muletillas fuera. El corrector de español. |
| **Mensaje** | Corto e informal, para chat. |
| **Correo** | De usted y en párrafos. No inventa saludo ni despedida que no se dictaron. |
| **Terminal** | Respeta los nombres técnicos en inglés, no capitaliza y no «arregla» rutas ni comandos. Para dictar en Claude Code. |
| **Nota** | Reestructura lo dictado en viñetas. |
| **Personalizado** | El Prompt lo escribe el usuario. Es el único Preset en el que ese campo se puede editar. |

**Personalizado existe por honestidad**: sin él, un Modo al que le has reescrito el Prompt seguiría
diciendo que es «Limpio» cuando ya no lo es. El Preset es lo que define el trabajo, así que en
cuanto el trabajo lo escribes tú, el Preset es otro.

## Consequences

- Cada Prompt de fábrica es una **cadena nuestra versionada en el repo** (ver
  `lib/src/features/formatting/format_prompt.dart`): ningún texto que gobierne el comportamiento
  de Pronto lo escribe un modelo.
- Siete Presets son siete prompts que hay que escribir, **medir en español** y mantener. Añadir
  uno no es añadir una fila a un `enum`.
- **Los topes del guardia de deriva son propiedad del Preset, no globales.** Los que hay hoy
  (`formatMaxAddedContent = 0.20`, `formatMaxDeletedContent = 0.60`) se midieron sobre 443
  dictados formateados al estilo **Limpio**, y ahí valen. **Nota** reestructura a propósito y
  «Correo» reordena en párrafos: aplicarles el tope de Limpio descartaría su salida legítima. Cada
  Preset trae los suyos y se miden con su propio corpus antes de encenderlo. Esto cierra la Q3 de
  la spec de v0.8: el guardia no era global, era de un trabajo concreto.
- **Crudo no es «el formateo apagado»**: es un Preset con su sitio en la lista, y en la pantalla
  tiene que quedar claro que es la opción veloz, no la degradada.
