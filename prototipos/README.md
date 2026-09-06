# Prototipos — desechables

**Esto no es código de Pronto.** Son ficheros HTML sueltos que contestan una pregunta de diseño
antes de escribir Dart. Sin tests, sin manejo de errores, sin persistencia. Cuando una variante
gane, se reescribe **en condiciones** dentro de `lib/` y estos ficheros se tiran.

Se abren haciendo doble clic. Cada uno trae tres variantes: se cambia con **← →** o con la barra
blanca de abajo, y la variante va en la URL (`?variant=A|B|C`) para que se pueda compartir.

| Fichero | Ticket | La pregunta que contesta | Ganó |
|---|---|---|---|
| `prototipo-01-instalacion.html` | PRNT-13 | ¿Cómo se elige el Motor de voz la primera vez? ¿Con % o sin %? | **C** — hoja única, sin % |
| `prototipo-02-pildora.html` | PRNT-14 | ¿Qué enseña la Píldora mientras dictas, y cuánto sitio ocupa? | **B** — el punto crece hacia el lado |
| `prototipo-03-atajos.html` | PRNT-15 | ¿Cómo se configura un Atajo con N Modos y tres formas de activarlo? | **A** — tabla con edición en línea |
| `prototipo-04-historial.html` | PRNT-16 | ¿Cómo se ve de un vistazo qué cambió el Formateo? | **A** — dos paneles |
| `prototipo-05-vocabulario.html` | PRNT-17 | ¿Cómo le enseñas a Pronto una palabra que nunca acierta? | **A** — todo en una pantalla |
| `prototipo-06-asistente-local.html` | PRNT-18 | ¿Cómo se ofrece un modelo local pequeño y gratis? | ⏳ sin decidir |

Elegidas el **2026-09-06**. Las perdedoras siguen aquí a propósito: son el registro de por qué se
eligió lo que se eligió. Se tiran cuando el ticket esté construido, no antes.

## Estudios de estilo — `estilos/`

Las variantes de arriba contestan **qué** enseñar. Estos contestan **cómo se ve**: tres
**versiones** de estilo (las tres de `taste-skill`) por tres **pantallas** del sistema.
Mismo contenido en las nueve, sacado de `home_screen.dart`, `dictation_pill.dart` y el PRNT-13.
La paleta de `lib/src/core/theme.dart` está congelada en todas.
Se entra por **`estilos/index.html`**.

| Versión | Píldora | Interfaz | Instalación |
|---|---|---|---|
| 1 · editorial oscuro (`minimalist-ui`) | `sistema-pildora.html?variant=A` | `sistema-app.html?variant=A` | `taste-01-editorial.html` |
| 2 · telemetría táctica (`industrial-brutalist-ui`) | `…?variant=B` | `…?variant=B` | `taste-02-telemetria.html` |
| 3 · cristal etéreo (`high-end-visual-design`) | `…?variant=C` | `…?variant=C` | `taste-03-cristal.html` |

Fuera de la matriz, en `comparador-estilos.html`: el original sin tocar (`?estilo=actual`),
`/impeccable` en modo Operate (`?estilo=imp`) y `ui-ux-pro-max` (`?estilo=pmx`).

⚠ **Ninguna skill respetó la paleta por su cuenta** (pidieron blanco hueso, rojo aviación y
slate+verde). El override es manual y hay que sostenerlo en cada ejecución.

## Cómo se usan

- **02 (píldora):** teclas `1`-`5` para los estados, `M` para el menú, `↑↓ Enter` para recorrerlo.
  Se juzga sobre un Bloc de notas falso: una píldora sola siempre parece bonita.
- **03 (atajos):** el grabador es **real**. Pulsa `Ctrl` izquierdo y luego el derecho: se
  distinguen. Los botones 4 y 5 del ratón también entran.
- **05 (vocabulario):** escribe `Serviforn`, `BeltranLabs`, `Supabase` o `Txonso` y dale a grabar:
  suelta las variantes que el motor oye de verdad para esas palabras.
- **06 (asistente local):** el botón «Simular *ya tengo Ollama*» del pie cambia todo el
  comportamiento — es el caso que no se puede olvidar al construirlo.

### Estados por URL
`01` acepta `?paso=0..4`, `&listo=1` (motor ya instalado) y `&formateo=local`.
`02` acepta `?estado=inactivo|escuchando|procesando|pegado|error`, `&tam=min|med|gra` y `&menu=1`.
Sirve para volver a una pantalla concreta sin ir clicando — y para capturarlas.

## El contexto

Salen de la investigación de FluidVoice (GPLv3 — **no se copia código, solo comportamiento**), en
`.scratch/pronto-v09-fluidvoice/INVESTIGACION.md` del AI OS. La pila de trabajo está en
`PILA.md`, en esa misma carpeta.
