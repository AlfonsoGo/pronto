# FR-10 · Datos a salvo y arranque

> Origen: observado-en-código · Objetivo al que sirve: G5 (también G3)

## Qué hace (lenguaje del usuario)
Mis dictados, mi diccionario, mis notas y mis ajustes se guardan solo en mi PC y sobreviven a las actualizaciones. Cada día Pronto deja una copia de la base de datos, solo hay un Pronto abierto a la vez y puede arrancar con Windows sin abrirse delante.

## Requisitos (cada uno comprobable; `/rumbo` da un veredicto por línea)
- R1. Historial, diccionario, snippets, notas, Transforms, reuniones y ajustes viven en un único fichero SQLite local, `%APPDATA%\Pronto2\pronto2.db`, con el diario WAL activado; ningún dato sale del PC.
- R2. Al arrancar, si aún no existe, se crea la copia del día `%APPDATA%\Pronto2\copias\pronto2-AAAAMMDD.db` con `VACUUM INTO` (consistente aunque haya escrituras), una sola por día, y se conservan solo las 7 más recientes.
- R3. Una base sin datos (sin filas en ajustes, historial, palabras y reuniones) nunca se copia, para que no desplace a las copias buenas.
- R4. El esquema se versiona con `PRAGMA user_version` (hoy 9) y cada migración corre en una transacción conservando los datos existentes (probado desde v1, v2, v5 y v7 con datos).
- R5. Los ajustes antiguos se migran sin perder lo elegido: `removeFillers` y `backtrack` pasan a `cleanupLevel`, `meetingDetect` a `meetingAuto`, el atajo antiguo del Transform (`Win + Alt + 1`) a `Alt + Shift + P` y `qwen2.5:7b` a `qwen3:1.7b` solo si lo guardado es exactamente el valor antiguo de fábrica (uno elegido a mano se respeta).
- R6. Solo puede haber un Pronto en marcha: un segundo arranque no abre otro proceso y enseña el hub del que ya está abierto, salvo que se lance con `--hidden`.
- R7. Al abrir Pronto estando ya abierto, el hub sale delante de las demás ventanas (fallo conocido 18-oct: sin comprobar en vivo; se cede el permiso de primer plano con `AllowSetForegroundWindow` al arrancar).
- R8. «Iniciar con Windows» (apagado de fábrica) escribe el valor `Pronto2` en `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` con `"<ruta de pronto.exe>" --hidden`, arranca sin abrir el hub y al apagarlo borra el valor.
- R9. Cerrar el hub no cierra Pronto: sigue en la bandeja y solo termina con «Salir»; el hub se destruye al cerrarlo (libera su WebView2) y la Flow bar nunca se cierra, solo se oculta.
- R10. En cada arranque se purga el audio de dictados con más de `keepAudioDays` días, se purga el historial con más de `historyRetentionDays` días (0 = nunca) y se cierran las reuniones que quedaron grabando; un fallo en cualquiera de estos pasos se registra y no impide arrancar.

## Fuera de alcance
- Restaurar una copia desde la interfaz (hoy es manual).
- Copia de los audios de dictado (`audio\`) y de las muestras de voz (`voz\`): la copia diaria solo cubre la base de datos.
- Sincronizar entre equipos o hacer copias en la nube.

## Dónde vive
- `src-tauri/src/db.rs:1-12` (resumen del módulo y migración de modelo), `:36-41` (carpeta de copias), `:241-305` (migraciones por `user_version`), `:419-423` (apertura con WAL), `:441-466` (migración de ajustes), `:580-609` (copia diaria y 7 copias), `:612-628` (purga de audio), `:564-578` (purga de historial)
- `src-tauri/src/main.rs:41-55` (mantenimiento al arrancar), `:60-72` (permiso de primer plano e instancia única), `:149-157` (cierre de ventanas), `:162` (purga de historial), `:207-209` (arranque oculto), `:215-221` (no salir al cerrar)
- `src-tauri/src/autostart.rs:10-29` (valor `Run` con `--hidden`), `src-tauri/src/commands.rs:57-61` (aplicarlo al guardar ajustes)
- `src-tauri/src/tray.rs:50-75` (enseñar o crear el hub)
- `src-tauri/src/meetingdb.rs:305-316` (reuniones huérfanas)
