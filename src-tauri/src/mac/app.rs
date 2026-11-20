//! macOS: cómo vive Pronto en el sistema.
//!
//! - Sin icono en el Dock mientras no hay panel abierto (`ActivationPolicy::Accessory`): vive en la
//!   barra de menús, como cualquier utilidad del Mac. Con el panel abierto pasa a `Regular` (Dock y
//!   Cmd+Tab) y vuelve a `Accessory` al cerrarlo (decisión D10).
//! - Abrir Pronto otra vez (Finder, Launchpad, Spotlight) con la app en marcha enseña el panel
//!   (`RunEvent::Reopen`), igual que el segundo arranque en Windows.
//! - Si falta algún permiso, al arrancar se abre el panel en la página de permisos, que explica
//!   cada uno y lleva a su sitio en Ajustes.

use tauri::{ActivationPolicy, AppHandle, Manager, RunEvent};

/// Al arrancar (desde el `setup` de `main.rs`).
pub fn setup(app: &AppHandle) {
    if let Err(e) = app.set_activation_policy(ActivationPolicy::Accessory) {
        crate::errlog::error("mac", format!("no se pudo quitar el icono del Dock: {e}"));
    }
    // Los permisos se miran un poco después: el gancho de teclado tarda un instante en arrancar.
    let h = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(1500));
        let uses_fn = h
            .try_state::<std::sync::Arc<crate::state::AppState>>()
            .map(|st| hotkeys_use_fn(&st.settings.read()))
            .unwrap_or(true);
        let s = crate::mac::permisos::status(uses_fn);
        if !s.all_ok {
            crate::tray::open_hub(&h, Some("permisos"), None);
        }
    });
}

/// ¿Algún atajo usa la tecla Fn? (si no, el ajuste de la tecla Globo no importa).
pub fn hotkeys_use_fn(s: &crate::models::Settings) -> bool {
    [&s.hotkey_hold, &s.hotkey_hands_free, &s.hotkey_paste_last, &s.hotkey_copy_last, &s.hotkey_transform, &s.hotkey_meeting].iter().any(|h| h.func)
}

/// El panel se enseña: Pronto aparece en el Dock y en Cmd+Tab.
pub fn hub_shown(app: &AppHandle) {
    let h = app.clone();
    let _ = app.run_on_main_thread(move || {
        let _ = h.set_activation_policy(ActivationPolicy::Regular);
    });
}

/// El panel se cierra: Pronto vuelve a vivir solo en la barra de menús.
pub fn hub_hidden(app: &AppHandle) {
    let h = app.clone();
    let _ = app.run_on_main_thread(move || {
        let _ = h.set_activation_policy(ActivationPolicy::Accessory);
    });
}

/// Eventos de la app que solo existen en Mac.
pub fn on_run_event(app: &AppHandle, event: &RunEvent) {
    if let RunEvent::Reopen { .. } = event {
        crate::tray::show_hub(app);
    }
}
