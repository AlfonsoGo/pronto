//! macOS: «Activar IA local» con Ollama (decisión D3), hermana de lo `#[cfg(windows)]` de `aisetup.rs`.
//!
//! En Mac Pronto no descarga ni instala nada en esta versión: si falta Ollama abre su página oficial
//! (`https://ollama.com/download/mac`, host de la lista de la regla 1) con `/usr/bin/open` y espera a
//! que el usuario lo instale; la UI sigue en «Descargar Ollama» con el texto de qué hacer. Para
//! arrancarlo usa LaunchServices (`open -g -a Ollama.app`, sin traerlo al frente) o, si solo hay el
//! binario (Homebrew, script oficial), `ollama serve` en segundo plano. Siempre con `OLLAMA_HOST` en
//! loopback (`aisetup::ollama_host_env`), igual que en Windows: la guarda de la regla 1 es común.
//!
//! `/usr/bin/open` es lo más simple y fiable para abrir apps y páginas sin enlazar AppKit en este hilo
//! (que no es el principal). Toda espera tiene plazo y todo fallo se devuelve como texto, sin pánicos.

/// Nombre del paquete de Ollama (`Ollama` + `.app`; escrito así para que no parezca un dominio).
const OLLAMA_APP: &str = concat!("Ollama", ".", "app");

use crate::aisetup::{check, ollama_host_env, path_dirs, prog, Emit, Stop};
use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

/// Página oficial de descarga de Ollama para Mac.
pub const DOWNLOAD_PAGE: &str = "https://ollama.com/download/mac";
/// `open` de macOS (LaunchServices).
const OPEN: &str = "/usr/bin/open";
/// `open` vuelve en cuanto LaunchServices acepta la orden; más de esto es que algo va mal.
const OPEN_TIMEOUT: Duration = Duration::from_secs(10);
/// Cuánto se espera a que el usuario instale Ollama desde la página que se abrió.
const INSTALL_WAIT: Duration = Duration::from_secs(30 * 60);
/// Margen tras ver Ollama.app en Aplicaciones: Finder puede estar aún copiándolo.
const COPY_SETTLE: Duration = Duration::from_secs(3);

const MSG_OPENING: &str = "Abriendo la página de descarga de Ollama…";
const MSG_WAITING: &str = "Instala Ollama desde la página que se ha abierto y vuelve aquí";

/// Dónde puede estar Ollama, por orden: la app en Aplicaciones (del sistema y del usuario), el binario
/// de Homebrew (Apple Silicon e Intel) y cada carpeta absoluta del PATH (`aisetup::path_dirs`: nunca
/// relativas). Una app de Mac abierta desde el Finder tiene un PATH mínimo; por eso las rutas fijas.
pub(crate) fn candidates(home: Option<&Path>, path: Option<&OsStr>) -> Vec<PathBuf> {
    let mut c = vec![PathBuf::from("/Applications/Ollama.app")];
    if let Some(h) = home {
        c.push(h.join("Applications").join(OLLAMA_APP));
    }
    c.push(PathBuf::from("/opt/homebrew/bin/ollama"));
    c.push(PathBuf::from("/usr/local/bin/ollama"));
    if let Some(p) = path {
        c.extend(path_dirs(p).into_iter().map(|d| d.join("ollama")));
    }
    c
}

/// ¿Es un paquete `.app`?
pub(crate) fn is_app(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("app"))
}

/// Una app completa (con `Info.plist` y su binario) o un binario ejecutable.
fn usable(p: &Path) -> bool {
    if is_app(p) {
        p.join("Contents").join("Info.plist").is_file() && p.join("Contents").join("MacOS").is_dir()
    } else {
        std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
}

/// Ollama de este Mac: la ruta de `Ollama.app` o la del binario `ollama`. Misma firma que la de Windows.
pub fn find_ollama() -> Option<PathBuf> {
    let home = dirs::home_dir();
    let path = std::env::var_os("PATH");
    candidates(home.as_deref(), path.as_deref()).into_iter().find(|p| usable(p))
}

/// Ejecuta `cmd` (algo que vuelve enseguida, como `open`) y espera su resultado con plazo.
fn run_quick(mut cmd: Command, what: &str) -> Result<(), String> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("no se pudo ejecutar {what}: {e}"))?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(st)) if st.success() => return Ok(()),
            Ok(Some(st)) => return Err(format!("{what} terminó con error ({st})")),
            Ok(None) => {}
            Err(e) => return Err(format!("no se pudo vigilar {what}: {e}")),
        }
        if started.elapsed() > OPEN_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("{what} no respondió en {} s", OPEN_TIMEOUT.as_secs()));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Abre una página web, una carpeta o un fichero con la app que el usuario tenga por defecto
/// (`/usr/bin/open <destino>`, LaunchServices). Lo usan también el actualizador (página del release)
/// y el hub (carpeta del registro de errores).
pub(crate) fn open_with_system(target: &OsStr) -> Result<(), String> {
    let mut cmd = Command::new(OPEN);
    cmd.arg(target);
    run_quick(cmd, "open")
}

/// `open -g --env OLLAMA_HOST=… -a Ollama.app`: la abre sin traerla al frente y con el host en loopback.
fn open_app(app: &Path, host: &str) -> Result<(), String> {
    let mut cmd = Command::new(OPEN);
    cmd.arg("-g").arg("--env").arg(format!("OLLAMA_HOST={host}")).arg("-a").arg(app);
    run_quick(cmd, "open -a Ollama")
}

/// `ollama serve` en segundo plano, en su propio grupo de procesos (no cae con Pronto) y con
/// `OLLAMA_HOST` en loopback. Un hilo recoge su final para no dejar un proceso zombi.
fn serve(bin: &Path, host: &str) -> Result<(), String> {
    let dir = bin.parent().unwrap_or(Path::new("/"));
    let mut child = Command::new(bin)
        .arg("serve")
        .current_dir(dir)
        .env("OLLAMA_HOST", host)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|e| format!("No se pudo arrancar Ollama ({}): {e}", bin.display()))?;
    let _ = std::thread::Builder::new().name("pronto-ollama-serve".into()).spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// Lanza Ollama sin esperarlo (misma firma que la de Windows). Con `Ollama.app`: LaunchServices; si
/// `open` falla (p. ej. un macOS sin `--env`), el binario que lleva dentro con `serve`. Con solo el
/// binario: `ollama serve`.
pub fn start_ollama(exe: &Path, url: &str) -> Result<(), String> {
    let host = ollama_host_env(url);
    if !is_app(exe) {
        return serve(exe, &host);
    }
    match open_app(exe, &host) {
        Ok(()) => Ok(()),
        Err(e) => {
            crate::errlog::error("aisetup", format!("no se pudo abrir Ollama.app ({e}); se prueba con ollama serve"));
            let bin = exe.join("Contents").join("Resources").join("ollama");
            if usable(&bin) {
                serve(&bin, &host)
            } else {
                Err(format!("No se pudo abrir Ollama: {e}"))
            }
        }
    }
}

/// En Mac no se instala nada: abre la página oficial de descarga y espera (cancelable, con plazo) a
/// que Ollama aparezca en Aplicaciones o en el PATH. Emite `download-ollama` (no `install-ollama`, que
/// en la UI bloquea «Cancelar» y dice «no hace falta que hagas nada»). Misma firma que la de Windows.
pub(crate) fn install_ollama(cancel: &AtomicBool, emit: Emit) -> Result<PathBuf, Stop> {
    emit(prog("download-ollama", 0, 0, MSG_OPENING));
    if let Err(e) = open_with_system(OsStr::new(DOWNLOAD_PAGE)) {
        crate::errlog::error("aisetup", format!("no se pudo abrir la página de Ollama: {e}"));
        return Err(Stop::Failed(
            "No se pudo abrir la página de descarga de Ollama. Descárgalo desde ollama.com/download/mac, instálalo y vuelve a intentarlo.".into(),
        ));
    }
    let started = Instant::now();
    let mut last_emit: Option<Instant> = None;
    loop {
        check(cancel)?;
        if let Some(found) = find_ollama() {
            // Recién copiado: un momento para que el Finder termine antes de abrirlo.
            let settle = Instant::now();
            while settle.elapsed() < COPY_SETTLE {
                check(cancel)?;
                std::thread::sleep(Duration::from_millis(250));
            }
            return Ok(found);
        }
        if started.elapsed() > INSTALL_WAIT {
            return Err(Stop::Failed(
                "No encuentro Ollama en la carpeta Aplicaciones. Instálalo desde ollama.com/download/mac y vuelve a intentarlo.".into(),
            ));
        }
        if last_emit.map_or(true, |t| t.elapsed() >= Duration::from_secs(1)) {
            last_emit = Some(Instant::now());
            emit(prog("download-ollama", 0, 0, MSG_WAITING));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn busca_primero_la_app_y_nunca_en_carpetas_relativas() {
        let path = std::env::join_paths(["/opt/x/bin", ".", "bin"]).unwrap();
        let c = candidates(Some(Path::new("/Users/ana")), Some(&path));
        assert_eq!(c[0], PathBuf::from("/Applications/Ollama.app"));
        assert_eq!(c[1], PathBuf::from("/Users/ana/Applications/Ollama.app"));
        assert!(c.contains(&PathBuf::from("/opt/homebrew/bin/ollama")) && c.contains(&PathBuf::from("/opt/x/bin/ollama")));
        assert!(c.iter().all(|p| p.is_absolute()), "{c:?}");
    }

    #[test]
    fn una_app_a_medias_no_cuenta() {
        let d = std::env::temp_dir().join(format!("pronto-mac-ollama-{}", std::process::id()));
        let app = d.join(OLLAMA_APP);
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(app.join("Contents")).unwrap();
        assert!(is_app(&app) && !usable(&app));
        std::fs::create_dir_all(app.join("Contents").join("MacOS")).unwrap();
        std::fs::write(app.join("Contents").join("Info.plist"), b"<plist/>").unwrap();
        assert!(usable(&app));
        let _ = std::fs::remove_dir_all(&d);
    }
}
