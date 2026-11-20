//! macOS: «Iniciar al abrir sesión» (decisión D11), hermana de `autostart::set` de Windows.
//!
//! Un LaunchAgent del usuario: `~/Library/LaunchAgents/com.beltranlabs.pronto2.plist` con el binario de
//! Pronto y `--hidden` (`autostart::launch_agent_plist`). launchd lo carga en el próximo inicio de
//! sesión, así que no hace falta `launchctl load`. No pide cuenta de Apple de pago ni firma (a
//! diferencia de `SMAppService`), y desactivarlo es borrar el fichero.

use crate::autostart::{launch_agent_plist, LAUNCH_AGENT_LABEL};
use std::path::{Path, PathBuf};

/// `~/Library/LaunchAgents/com.beltranlabs.pronto2.plist`.
fn plist_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("No se encuentra la carpeta de inicio del usuario.")?;
    Ok(home.join("Library").join("LaunchAgents").join(format!("{LAUNCH_AGENT_LABEL}.plist")))
}

/// El binario de esta copia de Pronto. Si macOS la ejecuta «trasladada» (App Translocation: una app
/// sin mover fuera de Descargas o de la imagen .dmg) o desde la propia imagen, esa ruta no existirá en
/// el próximo inicio de sesión: se pide moverla a Aplicaciones.
fn this_exe() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("No se pudo localizar Pronto: {e}"))?;
    let s = exe.to_string_lossy();
    if s.contains("/AppTranslocation/") || s.starts_with("/Volumes/") {
        return Err("Mueve Pronto a la carpeta Aplicaciones para que pueda abrirse al iniciar sesión.".into());
    }
    Ok(exe)
}

/// ¿Ya está activado y apunta a este Pronto? (el plist existe y es justo el que se escribiría).
pub fn is_enabled() -> bool {
    match (plist_path(), this_exe()) {
        (Ok(p), Ok(exe)) => std::fs::read_to_string(p).is_ok_and(|t| t == launch_agent_plist(&exe.to_string_lossy())),
        _ => false,
    }
}

/// Escribe `text` en `path` sin dejarlo a medias: a un temporal al lado y `rename`.
fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    let dir = path.parent().ok_or("Ruta del LaunchAgent sin carpeta.")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("No se pudo crear {}: {e}", dir.display()))?;
    let tmp = dir.join(format!(".{LAUNCH_AGENT_LABEL}.plist.tmp-{}", std::process::id()));
    std::fs::write(&tmp, text).map_err(|e| format!("No se pudo escribir el arranque al iniciar sesión: {e}"))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("No se pudo guardar el arranque al iniciar sesión: {e}")
    })
}

/// Activa o desactiva el arranque al iniciar sesión. Misma firma que la de Windows.
pub fn set(enabled: bool) -> Result<(), String> {
    // La versión de pruebas (`PRONTO_DATOS`) no toca el arranque: es del Pronto instalado.
    if crate::db::test_build() {
        return Ok(());
    }
    let path = plist_path()?;
    if enabled {
        if is_enabled() {
            return Ok(());
        }
        let exe = this_exe()?;
        write_atomic(&path, &launch_agent_plist(&exe.to_string_lossy()))
    } else {
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            // Si no existe, ya está como queremos.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("No se pudo quitar el arranque al iniciar sesión: {e}")),
        }
    }
}
