# El Modo se llama `DictationMode` en el código

El repo ya tenía tres cosas llamadas «modo» que **no** son Modos: `TriggerMode` (cómo se dispara
la grabación), `InjectionMode` (cómo se pega el texto) y `WindowModeController` (la ventana de la
píldora). Meter un `Mode` a secas al lado de esos tres haría el código inavegable justo en el
concepto más importante de la versión. El tipo se llama **`DictationMode`**.

Se mantiene la convención del repo —identificadores en inglés, comentarios y cadenas de usuario en
español—, igual que `Dictado` ya viaja como `DictationController` y `DictationState`. De ahí
cuelgan `activeMode`, `DictationModeRepository` y la tabla `dictation_modes`.

## Considered Options

- **`Modo`, en español.** Elimina la ambigüedad de raíz y habla el idioma de la interfaz, pero
  rompe la convención en el tipo que más se va a leer y deja el código a medio traducir.
- **Renombrar los viejos** (`TriggerMode` → `Trigger`, `InjectionMode` → `Injection`) y quedarse
  con `Mode`. Es lo más limpio a largo plazo, pero toca código que hoy funciona y que este ticket
  tiene prohibido tocar. Queda como refactor separado si la confusión aparece de verdad.

## Consequences

- `TriggerMode` e `InjectionMode` quedan declarados en `CONTEXT.md` como Ajustes **dentro** de un
  Modo, no como Modos. Si alguien los llama «modo» en una revisión, se corrige la palabra.
- Los textos de la interfaz siguen diciendo **Modo**, que es como lo llama el usuario.
- Los ficheros que ya existen (`hotkey_recorder.dart`, `features/overlay/`, `windows_hotkey_service.dart`)
  conservan su nombre. `CONTEXT.md` gobierna la prosa y lo que se escriba nuevo; renombrar ficheros que
  funcionan no aclara nada y ensucia el historial.
