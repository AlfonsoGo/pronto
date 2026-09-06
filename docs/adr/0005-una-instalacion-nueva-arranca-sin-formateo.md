# Una instalación nueva arranca sin Formateo

El Modo por Defecto que Pronto crea la primera vez nace con el Preset **Crudo**: transcribe y
pega, sin Modelo de formateo. No con «Limpio», que es el que arregla las tildes y el que Txonso
querrá.

El motivo es que **el Formateo necesita algo que una instalación nueva no tiene**: o un Ollama
corriendo en el equipo, o una clave de nube que el usuario aún no ha puesto. Arrancar en «Limpio»
significaría que el primer dictado de todo el mundo sale acompañado de un aviso de que no se pudo
formatear — un error feo de fábrica, en la pantalla, por una función que nadie ha pedido todavía.
Y degradar en silencio a Crudo sin decirlo es peor: el Modo diría una cosa y haría otra.

Esto además **no cambia nada de lo que ya hay**: `AppSettings.defaults()` ya trae
`formatPath: FormatPath.none`, así que la migración desde 0.7.1 conserva el comportamiento exacto
de cada instalación.

## Consequences

- La instalación nueva trae **un solo Modo**, «Por defecto» (Crudo). Los demás Presets se crean
  desde la pantalla de Modos; no se precrean seis Modos que no funcionarían sin modelo.
- Encender el Formateo es un acto explícito del usuario, y la pantalla tiene que decirle qué le
  hace falta (Ollama local, o clave de nube cuando exista PRNT-11).
- **Esta es la configuración de fábrica, no la de Txonso.** Su montaje es Parakeet local +
  «Limpio» contra un mini de nube; eso se configura, no se publica.
- El 55 % de los fallos en español que motivan PRNT-1 sigue sin arreglarse para quien instale
  Pronto y no toque nada. Es el precio de no mentir en el primer arranque, y se paga con una
  pantalla que lo explique bien, no cambiando el defecto.
