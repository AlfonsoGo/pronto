# FR-09 · Actualizador e instalador

> Origen: observado-en-código · Objetivo al que sirve: G5 (también G3)

## Qué hace (lenguaje del usuario)
Pronto busca solo las versiones nuevas y las instala con un botón: se descarga, se comprueba, se cierra, se actualiza y se vuelve a abrir sin que yo toque nada. Si algo sale mal sigo con la versión que tenía. El instalador no pide administrador y sirve también para actualizar desde Pronto 1.

## Requisitos (cada uno comprobable; `/rumbo` da un veredicto por línea)
- R1. Con «Buscar actualizaciones automáticamente» activado (de fábrica), Pronto consulta `releases/latest` de `AlfonsoGo/pronto` en GitHub 20 s después de arrancar y cada 4 h; sin red o con error reintenta a los 30 min y con límite de la API (403 o 429) a la hora, sin molestar al usuario; «Buscar ahora» sí enseña el error.
- R2. Una versión cuenta como nueva si su semver es mayor que la del ejecutable; se ignoran borradores, prereleases, tags con `-rc` y releases sin ningún `.exe`; se toma el primer asset `.exe`.
- R3. Si hay versión nueva aparece una tarjeta «Pronto X.Y.Z disponible» (con «Actualizar ahora», «Más tarde» y las notas recortadas a 600 caracteres y 12 líneas) y el elemento «Actualizar a X.Y.Z» arriba del menú de la bandeja.
- R4. No se instala mientras se dicta o se graba una reunión («Estás dictando ahora mismo…», «Hay una reunión grabándose…»), ni en una compilación de desarrollo, ni desde una dirección que no sea `https` de GitHub.
- R5. El instalador se baja a `%TEMP%\pronto-update\<asset>.part` y solo se acepta si su tamaño es el publicado, empieza por `MZ` y su SHA-256 coincide con el de la API (si la API lo da); con cualquier fallo se borra el `.part` y sale un error en español.
- R6. Un ayudante de PowerShell oculto espera a que Pronto muera (máx. 45 s, luego lo fuerza), ejecuta el instalador con `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /NOCANCEL /SP-` y vuelve a abrir `pronto.exe` una sola vez, también si el instalador falló; Pronto no se cierra si el ayudante no arranca en 10 s.
- R7. Si la instalación falló, el siguiente arranque avisa «La última actualización no se pudo instalar (código N). Sigues con tu versión de siempre…» con «Reintentar», y el registro queda en `%TEMP%\pronto-update\update.log` y `setup.log`.
- R8. El instalador usa el mismo AppId que Pronto 1 (actualiza en sitio y reutiliza su carpeta), no necesita administrador, no toca `models\` y solo limpia restos de Pronto 1 si encuentra `flutter_windows.dll`.
- R9. En modo silencioso el instalador espera hasta ≈ 20 s a que `pronto.exe` quede libre y si no aborta sin instalar nada; sin WebView2 Runtime aborta en silencio (se conserva la versión que funciona) y en modo normal ofrece abrir la descarga.
- R10. Los usuarios que tienen la 2.0.x reciben la versión nueva al publicarla (fallo conocido 18-oct: sin comprobar de punta a punta; la instalada es la 2.2.0 y lo publicado en GitHub era la 2.0.2).

## Fuera de alcance
- Detectar RAM, VRAM y GPU en un PC nuevo y recomendar qué instalar (FR-15).
- Firma de código del instalador y notificaciones nativas de Windows.
- Actualizar el modelo de voz o de IA (FR-04 y FR-05).

## Dónde vive
- `src-tauri/src/updater.rs:31-54` (URL, plazos y límites), `:96-151` (versiones), `:208-227` (release y primer `.exe`), `:237-256` (URL permitida), `:468-525` (consulta), `:544-561` (verificación), `:569-607` (descarga), `:618-660` (ayudante), `:692-752` (instalar), `:757-778` (error anterior y limpieza), `:781-806` (hilo de fondo)
- `src-tauri/src/update_helper.ps1:27-33` (espera 45 s), `:35-46` (instalar), `:48-52` (resultado), `:55-74` (reabrir una vez)
- `src-tauri/src/tray.rs:41-43` y `:190` (elemento «Actualizar a»)
- `ui/hub/update.js:41-47` (tarjeta), `ui/hub/pages/settings.js:153` (ajuste automático)
- `installer/pronto2.iss:1-13` y `:40-43` (mismo AppId), `:60` (sin administrador), `:109-126` (limpieza de Pronto 1), `:133` (`skipifsilent`), `:181-213` (WebView2), `:216-247` (espera de `pronto.exe`)
- `installer/build_installer.ps1` (reúne el stage y compila con Inno Setup; versión de `src-tauri/tauri.conf.json`)
