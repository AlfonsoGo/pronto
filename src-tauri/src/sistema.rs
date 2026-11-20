//! Comandos para que el panel sepa en qué sistema corre (Windows o Mac) y, en Mac, para consultar y
//! pedir los permisos. Existen en los dos sistemas (el panel es el mismo); en Windows no hay permisos
//! que pedir.

use serde_json::{json, Value};

/// `"windows"` o `"macos"`.
pub const OS: &str = if cfg!(target_os = "macos") { "macos" } else { "windows" };

/// Sistema y, en Mac, el estado de los permisos (`mac::permisos::Status`).
#[tauri::command]
pub fn platform_info(state: tauri::State<'_, std::sync::Arc<crate::state::AppState>>) -> Value {
    #[cfg(target_os = "macos")]
    {
        let uses_fn = crate::mac::app::hotkeys_use_fn(&state.settings.read());
        json!({ "os": OS, "permissions": crate::mac::permisos::status(uses_fn) })
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = state;
        json!({ "os": OS, "permissions": Value::Null })
    }
}

/// Pide un permiso de macOS (`accessibility`, `inputMonitoring`, `microphone`, `keyboard`) o abre
/// su sitio en Ajustes. En Windows no hace nada.
#[tauri::command]
pub fn request_permission(kind: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        crate::mac::permisos::request(&kind)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = kind;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn el_sistema_es_el_de_la_compilacion() {
        assert_eq!(super::OS, if cfg!(windows) { "windows" } else { "macos" });
    }
}
