Rompedor · 2026-10-23 · descargas · 0 altas, 2 medias, 2 bajas · Arreglado: MEDIA 1 (plazo de cabeceras de 60 s y plazo total en instalador, modelos y descarga de Ollama; ventana de avance en modelos: `updater::get_following_timed`), MEDIA 2 (URL de Ollama normalizada en `ollama::normalize_url` y validada al guardar en `commands.rs`), BAJA 3 (máximo absoluto de 512 MB en el instalador). Sin arreglar: BAJA 4 (`asr.rs`, de otro). 23-oct
Datos: ninguno real usado. Los modelos de `%APPDATA%\Pronto2\models` solo se leyeron y se copiaron a `%TEMP%`. `%APPDATA%\Pronto2\pronto2.db`: 270336 B y mtime 2026-10-15 21:29:15.377, igual antes y después. Ni en `models\parakeet` ni en `models\whisper` hay `.verificado`, ni antes ni después. `%LOCALAPPDATA%\Programs\Pronto\pronto.exe` sigue igual: 96348672 B, 17-oct 22:54. `%TEMP%\pronto-update` ya existía (21-oct) y no se tocó. El `pronto.exe` de Txonso (PID 14560) siguió en marcha y no se tocó. Ollama no estaba en marcha y no se arrancó: no se llamó a `ensure_running` ni a `setup`. Resultado: restaurados ✅. | Servidor usado: servidores TCP propios en `127.0.0.1:<efímero>`. Los «otros hosts» son `127.0.0.2` y `[::1]`, que también son este PC.
Huecos: ver abajo.

Zona: descargas (`updater.rs`, `update_helper.ps1`, `modeldl.rs`, `whisper.rs`, `aisetup.rs`, `ollama.rs`, `asr.rs` en lo que toca a las rutas del modelo). Rama `fases/2026-10-22`, a partir de 2f78f7f; cuando se probó, HEAD estaba en aeda142, que no cambia nada de la zona. Arreglos atacados: 022e85c y ca4f00b.
Sondas: todas en `src-tauri/src/rompedor_descargas.rs`. Tras la segunda pasada hay 26: 17 de la primera y 9 nuevas.
Compilación: `CARGO_TARGET_DIR=C:/Users/usuario/dev/pt-d`.
Lanzar: `cargo test rompedor_descargas_ -- --include-ignored --nocapture --test-threads=1`, con `PRONTO_ROMPEDOR_MODELO=%APPDATA%\Pronto2\models\parakeet` y `PRONTO_ROMPEDOR_WHISPER=%APPDATA%\Pronto2\models\whisper\ggml-medium-q5_0.bin`.
El ayudante solo se probó con `.exe` de juguete en `%TEMP%`.
No cuenta lo ya conocido: M1 (el digest es opcional).

## ALTA

Ninguna.

## MEDIA

1. **El plazo de PRNT-41 solo cubre el cuerpo de la descarga. Si el servidor manda las cabeceras gota a gota, la descarga del instalador no se corta nunca. Los modelos de voz no tienen ni plazo ni ventana, ni en las cabeceras ni en el cuerpo.**
   Código:
   - `updater.rs:785`: `get_following(&agent(), &asset.url, &|r| r, …)` no pone `.timeout(…)` a la petición.
   - `updater.rs:617`: el agente solo tiene `timeout_read(30 s)`, que cuenta por cada lectura.
   - La ventana (64 KB cada 30 s) y el plazo de 30 min están en `copy_limited` (`:374`), y esa función solo arranca cuando ureq ya ha leído todas las cabeceras.
   - Modelos: `modeldl.rs:450` tiene `Limits { max_bytes: f.size, deadline: None, window: None }`, y el agente de `:381` tampoco lleva `.timeout`.

   Reproducción: `rompedor_descargas_goteo_en_cabeceras_sin_plazo` (`rompedor_descargas.rs:929`). Dos servidores locales:
   - uno manda `HTTP/1.1 200 OK\r\nContent-Length: 4\r\nX-Goteo: ` y después un byte por segundo;
   - el otro manda las cabeceras completas y después un byte por segundo del cuerpo.

   Se llama a la vez a `updater::fetch_installer` y a `modeldl::download_all` contra los dos.

   Evidencia:
   - `instalador · cuerpo a goteo: terminó a los 30.2 s -> "La descarga va demasiado lenta (menos de 64 KB en 30 s)…"`. Esta parte del arreglo funciona.
   - `instalador · cabeceras a goteo: SIGUE en marcha a los 75 s (1 byte/s)`.
   - `modelo · cabeceras a goteo: SIGUE en marcha a los 75 s`.
   - `modelo · cuerpo a goteo: SIGUE en marcha a los 75 s`.
   - Solo terminan cuando el servidor cierra (`Header field didn't end with \n`, `response body closed before all bytes were read`).

   Tope en ureq 2.12.1 (`response.rs:36-37`): 100 KB por cabecera y 100 cabeceras. A 1 byte por segundo son unos 116 días.

   Consecuencias, leyendo el código:
   - El hub se queda en «Descargando…» indefinidamente. `INSTALLING` y el estado `downloading` siguen activos, así que «Buscar ahora» y la comprobación de fondo devuelven el estado viejo (`check`, `:656`) hasta reiniciar Pronto.
   - En los modelos, `RUNNING` (o `WHISPER_DL`) se queda en `true`, no hay botón de cancelar, y «Descargar» no hace nada (`modeldl.rs:498`).
   - Para provocarlo hace falta un servidor (o un intermediario) que conteste así en la ruta de GitHub o de Hugging Face. Es el mismo escenario que el PRNT-41 que se dio por arreglado.

   Arreglo propuesto:
   - Ponerle a cada petición del instalador y de los modelos un plazo para la respuesta (`prepare = |r| r.timeout(Duration::from_secs(60))` en `get_following`; ureq aplica `timeout` a cabeceras y cuerpo, así que la descarga larga necesita otra petición o un plazo distinto), o vigilar las cabeceras desde otro hilo.
   - En los modelos, una ventana de avance mínimo (por ejemplo 64 KB cada 60 s) y un «Cancelar» que se mire en cada lectura.

2. **Una URL de Ollama escrita sin esquema (`localhost:11434`, como en la variable `OLLAMA_HOST` de Ollama) cuenta como «este PC» para la interfaz y para «Activar IA local», pero `ollama.rs` no consigue usarla. El pulido, los Transforms y las notas fallan sin ningún aviso, y «Activar IA local» intenta arrancar Ollama (o instalarlo) para una dirección que nunca va a funcionar.**
   Código:
   - `aisetup.rs:131-135`: `url_host` le añade `http://` delante.
   - `ui/hub/pages/settings.js:42-45`: hace lo mismo y no muestra el aviso de «fuera del PC».
   - `ollama.rs:10`, `:290`, `:382`: `base(url)` la usa tal cual. `Url::parse("localhost:11434/api/tags")` lee `localhost` como esquema y se queda sin host, así que ureq falla (`EmptyHost`).
   - `commands.rs:29`: no se valida el esquema al guardar.

   Reproducción: `rompedor_descargas_url_de_ollama_sin_esquema` (`rompedor_descargas.rs:1114`). Un Ollama falso en `127.0.0.1:P` responde a `/api/tags` y a `/api/generate`.

   Evidencia:
   - `http://127.0.0.1:63777: is_loopback_url=true · status.reachable=true · pulido failed=false · generate=Ok · peticiones +4`.
   - `127.0.0.1:63777`, `localhost:63777` y `LOCALHOST:63777/`: `is_loopback_url=true · status.reachable=false · pulido failed=true · generate=Err("No se pudo contactar con Ollama en localhost:63777") · aisetup.ollama_running=false · peticiones al puerto: +0`.

   Lo que sigue sale de leer el código, porque ejecutarlo arrancaría el Ollama real. `is_local_url` (`:138`) usa el mismo `url_host`. Por eso `setup` (`:503-515`) ve «no responde» y «es local» y entonces:
   - si Ollama está instalado (como en este PC), lo lanza y espera 90 s hasta fallar con «Ollama no responde en localhost:11434. Ábrelo desde el menú Inicio», con Ollama ya abierto;
   - si no lo está, baja el instalador de más de 1 GB, lo instala y falla igual.

   Arreglo propuesto: normalizar la URL al guardarla (`commands.rs:29`: si no lleva `://`, anteponer `http://`), o que `ollama::base` haga lo mismo que `url_host`. Y que `is_local_url` y `is_loopback_url` usen exactamente la URL que va a usar ureq.

## BAJA

3. **El tope del instalador se calcula con el `size` del JSON de la release y no tiene máximo propio: con un `size` enorme se puede escribir en `%TEMP%` sin límite.** El tope de 512 MB (`INSTALLER_MAX_BYTES`) solo se aplica cuando `size` es 0.
   Código: `updater.rs:792-796`, `(size, _) => size.saturating_add(INSTALLER_SIZE_MARGIN)`, y con `size = u64::MAX` el resultado es `u64::MAX`.

   Reproducción: `rompedor_descargas_tope_del_instalador_sale_del_json` (`rompedor_descargas.rs:1012`). El servidor anuncia y manda 520 MB (8 MB más que el tope).

   Evidencia:
   - `size=0` → «El servidor anuncia un instalador de 545259520 bytes… no se descarga», con 0 B escritos. Bien.
   - `size=53687091200` (50 GB) → `escritos en .part 545259520 B (520 MB) en 1.5 s`.
   - `size=18446744073709551615` → `545259520 B en 1.1 s`.
   - En los dos últimos casos solo se corta al final, con «incompleto (545259520 de … bytes)», y el `.part` se borra.

   La ventana y el plazo dejan escribir durante 30 min a la velocidad de la red. El JSON viene de api.github.com por TLS, así que hace falta controlar la release (el mismo escenario que M1). Es una laguna en el «tope» de PRNT-41.
   Arreglo propuesto: `max_bytes = size.saturating_add(MARGIN).min(INSTALLER_MAX_BYTES)` y rechazar un asset con `size > INSTALLER_MAX_BYTES` antes de descargar.

4. **`native_path` estropea las rutas largas escritas en forma de dispositivo (`\\.\C:\…` o `//./C:/…`): las convierte en `\\?\UNC\.\C:\…`, que no existe.** `verify` y `model_present` las aprueban, así que el hub dice «listo» y no se puede dictar. Es el mismo síntoma de BAJA 6, que en el resto de formas sí queda arreglada.
   Código: `asr.rs:531`, `abs.strip_prefix(r"\\")` → `\\?\UNC\{…}`, que también recoge `\\.\`.

   Reproducción:
   - `rompedor_descargas_native_path_formas` (`rompedor_descargas.rs:1165`) prueba 16 formas.
   - `rompedor_descargas_modelo_en_ruta_de_dispositivo` (`:1213`) usa el Parakeet real en un proceso hijo.

   Evidencia:
   - `\\.\ dispositivo (266 u16): Rust lee=true · nativa existe=false · «\\?\UNC\.\C:\Users\…»`, y lo mismo con `//./`.
   - Las otras 13 formas (barras `/`, `.\` y `x\..\`, relativa, `\\?\`, `//?/`, `GLOBALROOT`, UNC `\\localhost\C$`, `\\127.0.0.1\C$`, punto final) sí existen.
   - Modelo real: `larga normal (control)` → `salida=0, transcribe Ok`. `\\.\ corta` → bien. `\\.\ larga (261 car.)` → `salida=101 · model_present=true · verify=None | c-api.cc:SherpaOnnxCreateOfflineRecognizer:698 Errors in config | load -> "No se pudo crear el reconocedor (¿modelo dañado?)"`.

   Solo pasa si el usuario escribe a mano «Ruta del modelo» de esa forma, así que es poco realista.
   Arreglo propuesto: en `native_path`, tratar `\\.\` como `\\?\` (cambiar el prefijo) antes de mirar `\\`, o usar `\\?\UNC\` solo cuando el resto no empiece por `.\` ni por `?\`.

## Arreglos de la primera pasada: confirmados

Todas las sondas de la primera pasada se volvieron a ejecutar: 16 en verde y 1 roja, la de M1, que es conocida.

- ✅ **MEDIA 1, pánico con una redirección sin host.** `rompedor_descargas_redireccion_sin_host_es_panico`: 16 de 16 `sin pánico`.
  - `fetch_latest` → «GitHub respondió con una redirección que no se sigue: … «file:», que no es http ni https».
  - `fetch_installer`, `download_all` → error legible.
  - `ollama::status` → `reachable=false`.
  - Con `file:`, `data:`, `javascript:` y `mailto:`.
- ✅ **MEDIA 2, corchetes en el ayudante.** `rompedor_descargas_ayudante_corchetes_en_la_carpeta`: las 5 variantes dan `instalador_ejecutado=true result="ok" reaperturas=1 log=true`: control, `Programas [Alfonso]`, `Apps]`, `tmp [1]` y `Pronto-Setup[x64].exe`. `rompedor_descargas_ayudante_con_rutas_raras`, con el `ProcessStartInfo` nuevo: 8 de 8, 1 sola reapertura y `fallos []`.
- ✅ **MEDIA 3, url_allowed en cada salto.** `rompedor_descargas_redireccion_salta_url_allowed`: `peticiones A=1 B=0 · fetch_installer=Err("No se descarga el instalador: redirección a un servidor no permitido (127.0.0.1).")`, con el disco vacío.
- ✅ **BAJA 4, `\@` en la URL de Ollama.** `rompedor_descargas_loopback_con_barra_invertida`: `http://evil.example\@127.0.0.1:11434` → `is_loopback_url=false`, y lo mismo con `:80\@localhost` y con `\@[::1]`.
- ✅ **BAJA 5, nombre del asset de 251 caracteres.** `rompedor_descargas_nombre_largo_bloquea_la_actualizacion`: se descarga bien, con el nombre local propio `Pronto-Setup.exe`.
- ✅ **BAJA 6, Parakeet en una ruta de más de 260 caracteres.** `rompedor_descargas_modelo_en_rutas_raras`, variante `larga (282 car.)`: `salida=0 · load -> None · transcribe Ok("")`. Siguen bien la ruta no ASCII, `\\?\` y la junta. Queda la variante `\\.\`, que es la BAJA 4 de arriba.
- ✅ **PRNT-41 en el cuerpo**: tope por `size` y ventana de 64 KB/30 s. La ventana corta un goteo en 30,2 s. En las cabeceras y en los modelos sigue abierto (MEDIA 1), y el tope tiene la laguna de BAJA 3.
- ❌ ninguna.

## Probado en la segunda pasada y aguanta

- **`get_following` y `next_hop`.** Sonda `rompedor_descargas_saltos_de_redireccion` (`:829`): 41 `Location` × 3 reglas de salto (actualizador, modelos, instalador de Ollama), 0 pánicos.
  - Se resuelven bien: relativa, de ruta, `?query`, `//host`, `127.1`, `0x7f.1`, `HTTP://LOCALHOST`, `http:\\…`, `%2e%2e`, espacios y tab en el host.
  - En el actualizador se rechazan `127.0.0.2`, `[::1]`, `[::ffff:127.0.0.1]`, `u@otro`, `u:p@`, `#@` y `https` a un host que no es de GitHub.
  - `file:`, `data:`, `javascript:`, `about:`, `blob:`, `ws:` y `ftp:` → «no es http ni https».
  - `Location` duplicada: manda la primera, y si la primera no está permitida se rechaza.
  - 307 con cuerpo, 303 y 308 → se siguen. 300, 305 y 399 → `Status`.
  - `Location` vacía o no UTF-8 → «sin destino». Las `Location` con IDN (`ⓛocalhost`) también salen como «sin destino», porque ureq no acepta cabeceras que no sean ASCII.
  - `Location` de 60 KB → se sigue.
- **`url_allowed` con el parser WHATWG.** Sonda `rompedor_descargas_url_allowed_coincide_con_el_parser` (`:1335`): 33792 comprobaciones generadas con `@`, `\`, tab, CR/LF, `%2e`, `%40`, `。` y `．`, caracteres invisibles, puertos raros, `#`, `?`, esquemas mal escritos y userinfo. Se aceptan 206, y ninguna es una URL que el parser lea con otro esquema, otro host o con usuario.
- **`.redirects(0)` en Ollama.** Sonda `rompedor_descargas_ollama_3xx_no_rompe_el_pulido` (`:1055`): con 301, 302, 303, 307, 308, 302 sin `Location` y 304, todos con un cuerpo JSON que imita una respuesta:
  - `pulido failed=true igual=true`, así que se pega el texto sin pulir;
  - `status.reachable=false`;
  - `generate=Err("Ollama respondió con una redirección (30x); no se sigue")`;
  - `aisetup running=false`;
  - 0 peticiones al servidor de la redirección, y unos 6 ms por caso.

  Un 3xx no rompe ni el pulido ni el estado.
- **`copy_limited`.** Sonda `rompedor_descargas_copy_limited_bordes` (`:1278`):
  - justo el tope → Ok; un byte de más → `TooBig`, sin escribir nunca más que el tope;
  - tope 0 y tope `u64::MAX` → bien;
  - goteo → `Stalled` a los 300 ms;
  - ráfagas justo antes del borde de cada ventana → pasan, como está diseñado; el plazo total es el que pone el límite;
  - ráfaga seguida de goteo → `Stalled`;
  - plazo → `Deadline`;
  - escritura fallida → `Write`.

  En el instalador: `Content-Length` mayor o menor que el cuerpo, o basura, y gzip sin descomprimir. Sonda `rompedor_descargas_respuestas_http_raras_aguantan`, en verde.
- **`is_loopback_host`.** `127.1` → 127.0.0.1, y `[::ffff:127.0.0.1]` → loopback. `localhost.`, `127.0.0.1.evil` y `localhost@evil` → no son loopback; son casos dudosos y se quedan en el lado seguro. Un salto a `[::ffff:7f00:1]` ni siquiera conecta en Windows («La dirección solicitada no es válida»).

## Huecos

- **`Lowers` y `catch_unwind` en `check` e `install`.** Se ejecutan a través de `AppHandle`, que no se puede crear sin `tauri/test`, así que no hubo forma de llegar a ellos. La sonda de pánico demuestra que la causa original ya no existe. El que `Lowers` libere `CHECKING`/`INSTALLING` si hay un pánico sale de leer el código (`updater.rs:557-563`, `:661`, `:972`, `:1058`).
- **Bajar de https a http en un salto.** No hay servidor TLS local. La comprobación está en `updater.rs:343`, leída en el código.
- **`aisetup::download_installer`** (`aisetup.rs:194-225`, privada). Leyendo el código:
  - no tiene tope de bytes (el ~1 GB de Ollama va sin `copy_limited`);
  - no tiene plazo ni ventana;
  - «Cancelar» (`check(cancel)`) solo se mira entre lecturas del cuerpo, así que con las cabeceras gota a gota (MEDIA 1) no responde.

  `verify_ollama_setup` (`:297`) da por bueno cualquier firmante cuyo nombre empiece por «ollama» (por ejemplo «Ollamax Ltd»). Habría que probarlo con un binario firmado, que no tengo.
- **`aisetup::pull_model`** (privada): lo mismo que en la primera pasada, `lines()` sin tope de línea.
- **`copy_limited` ante un `ErrorKind::Interrupted`**: aborta la descarga, mientras que `std::io::copy` la reintentaría. Está reproducido en la sonda, pero no conozco nada que lo provoque en sockets de Windows, así que no cuenta.
- **Carrera en `install`/`check`** (leída en el código): si un `check` en marcha termina sin release nueva en los microsegundos que hay entre el preflight de `install` y el primer `publish("downloading")`, el estado queda `downloading` con `release = None`. Entonces, si la descarga falla, `set_error` no lo cambia (`:924`) y se queda «Descargando…» hasta reiniciar. Además, la release tendría que desaparecer de GitHub justo en ese momento. No se ejecutó.
- **`ollama::status` y `generate_with`**: `into_json` no tiene tope; solo los limita el plazo (3 s en `status` y hasta 600 s en las notas). Lo único que puede abusar es un proceso local en 11434, el escenario B3. No se midió.
- **Whisper en un `%APPDATA%` redirigido a una ruta UNC** (redirección de carpetas en una empresa): `whisper.rs:217` le pasa la ruta sin `native_path`. Haría falta SMB con un recurso propio.
- **`whisper_start` y `modeldl::start` reales**: escriben en `%APPDATA%\Pronto2\models`, así que no se lanzaron. `download_all` sí se probó, contra carpetas temporales.
