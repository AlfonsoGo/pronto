# Probar Pronto en tu Mac

Gracias por probar Pronto. Son dos partes: un comando que lo comprueba casi todo solo (unos 30 minutos)
y 15 pasos a mano (unos 15 minutos). Al final, mándale a Txonso el informe y tus notas.

Necesitas un Mac con chip M (M1, M2, M3, M4…) y macOS 14 o más nuevo.

## Parte 1 · El comando

1. Descomprime `pronto-mac-….zip` en Descargas.
2. Abre la app **Terminal** (búscala con ⌘ + Espacio).
3. Copia esto, pégalo en la Terminal y pulsa Intro:

   ```
   cd ~/Downloads/pronto-mac && bash scripts/mac/probar.sh
   ```

   Si te pide instalar las herramientas de Xcode, acepta y, cuando acaben, vuelve a pegar el comando.
   Si te pide la contraseña del Mac, es para instalar Homebrew (las herramientas que faltan).
4. Al acabar sale `RESULTADO` y la ruta del informe (`informe-pronto-mac-….txt`, en el Escritorio).

## Parte 2 · A mano (15 pasos)

Apunta en cada paso **sí / no** y, si algo falla, qué viste. Usa la app construida, no la Terminal.

| # | Haz esto | Debe pasar |
|---|---|---|
| 1 | En Finder, ve a `pronto-mac/src-tauri/target/release/bundle/macos/`, arrastra **Pronto** a Aplicaciones y ábrela desde Aplicaciones. | Sale el panel de Pronto. Si macOS dice que no puede comprobar la app: Ajustes del Sistema → Privacidad y seguridad → abajo, **Abrir igualmente**. |
| 2 | Mira la barra de menús, arriba a la derecha. | Está el icono de Pronto (unas barritas). Al pulsarlo sale su menú. |
| 3 | En el panel, abre **Permisos del Mac** y pulsa **Abrir Ajustes** en Accesibilidad. Activa Pronto. | En 2-3 segundos la tarjeta pasa a **Listo**. |
| 4 | Haz lo mismo con Monitorización de entrada (si sale como «Falta») y Micrófono (**Permitir**). | Las tarjetas pasan a **Listo**. Si pide cerrar y abrir Pronto, hazlo. |
| 5 | Tarjeta **Tecla fn (🌐)**: si sale «Falta», sigue lo que dice (Teclado → «Pulsar la tecla 🌐 para» → **No hacer nada**). | Pasa a **Listo**. |
| 6 | Abre **Notas**, crea una nota, mantén pulsada **fn**, di «Hola, esto es una prueba de Pronto» y suelta. | En la barrita de abajo se ve que escucha; al soltar, el texto aparece en la nota. |
| 7 | Copia una palabra cualquiera (⌘C) antes de dictar otra vez como en el paso 6. Después pulsa ⌘V. | El dictado entra en Notas y luego ⌘V pega **tu palabra**, no el dictado (Pronto te devuelve el portapapeles). |
| 8 | En **Safari**, abre google.com, pulsa en el buscador y dicta con fn. | El texto aparece en el buscador. |
| 9 | Pulsa **fn + Espacio** (manos libres), habla 10 segundos sin tocar nada y vuelve a pulsar fn + Espacio. | Escucha sin mantener la tecla y pega al parar. |
| 10 | Mantén fn, empieza a hablar y pulsa **Esc**. | Se cancela: no se pega nada. |
| 11 | Pulsa **⌥ ⇧ Z** (Opción + Mayúsculas + Z) en Notas. | Se pega otra vez el último dictado. |
| 12 | Abre FaceTime (o Zoom, Meet o Teams) y haz una llamada de prueba de 1 minuto. | Pronto pregunta o empieza a tomar notas de la reunión. Al colgar, en **Reuniones** sale la nota con «Tú» y «Ellos». |
| 13 | Pulsa la barrita de abajo (la Flow bar) y luego dicta en Notas. | El texto entra en Notas, no en Pronto. |
| 14 | En Ajustes de Pronto → Sistema, activa **Abrir Pronto al encender el ordenador**. Cierra sesión y vuelve a entrar. | Pronto arranca solo, sin abrir el panel (solo el icono arriba). |
| 15 | Cierra el panel con el botón rojo y mira el Dock. | El panel se cierra, Pronto sigue arriba en la barra de menús y su icono sale del Dock. |

**Opcional (IA local):** en Ajustes → IA en tu PC, pulsa el botón de activar. Se abre la web de Ollama:
instálalo, vuelve a Pronto y enciende «Pulir cada dictado». Dicta algo con muletillas («eh, bueno, pues…»).

## Qué mandar a Txonso

- El archivo `informe-pronto-mac-….txt` del Escritorio.
- Tus 15 respuestas (sí/no + lo que viste). Una captura de pantalla de lo que falle ayuda mucho.
- Si Pronto se cierra solo: el archivo `~/Library/Application Support/Pronto2/logs/errores.log`
  (no lleva tu voz ni tus textos).
