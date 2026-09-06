# Qué vive en el Modo y qué sigue siendo un Ajuste de la app

Convertir la configuración global en Modos obliga a repartir, campo por campo, lo que había en
`AppSettings`. La regla de reparto es una sola pregunta: **¿esto tiene sentido distinto según
dónde estés escribiendo?** Si sí, es del Modo. Si no, es un Ajuste de la app.

**Dentro del Modo:** nombre · Preset · Prompt · Contexto (Aplicación · Texto seleccionado ·
Portapapeles) · idioma · Motor de voz · Camino y Modelo de formateo · Atajo propio · aplicaciones
de Auto-activación · avanzados del Modo (pausar la reproducción al grabar, autocapitalizar,
auto-pegar).

**Fuera del Modo, Ajustes de la app:** micrófono · arranque con el sistema · escala de la
Píldora · sonidos · captura de Correcciones · Vocabulario · Atajo de idioma · actualizaciones.

Los dos casos que cuestan y por qué caen donde caen:

- **El Vocabulario es de la app, no del Modo.** Que tu correo se escriba bien o que el nombre de
  un cliente se transcriba bien no depende de si estás en el terminal o en Gmail: depende de que
  esa palabra sea tuya. Partirlo por Modo obligaría a mantener la misma entrada en seis sitios y a
  decidir cuál gana. Si algún día hace falta un Término que solo aplique en un Modo, se añade como
  un filtro sobre una lista única, no duplicando la lista.
- **La captura de Correcciones es de la app.** Es cómo Pronto aprende de ti, no cómo escribe; y
  además vigila el portapapeles 45 s tras insertar, lo cual es una decisión de privacidad que no
  puede quedar escondida dentro de un Modo que se elige solo.

## Consequences

- `TriggerMode` (mantener pulsado / alternar) es del **Modo**: dictar con push-to-talk en el
  terminal y con alternar para un correo largo es exactamente el caso que motiva los Modos.
- `InjectionMode` (portapapeles o `SendInput`) es del **Modo**: los terminales necesitan uno
  distinto del resto de aplicaciones, y eso es una propiedad del sitio donde escribes.
- El Pulido determinista (`textPolish`) es del **Modo**: en Terminal estorba, en Correo hace falta.
- Un campo que se reparte mal cuesta una migración de base de datos arreglarlo. Ante la duda,
  entra en el Modo: es más fácil promover un campo a global que partir uno global en N.
