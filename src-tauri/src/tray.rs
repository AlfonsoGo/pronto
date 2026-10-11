//! Icono de bandeja: «Abrir Pronto», «Pegar última transcripción», «Micrófono ▸» y «Salir».

use crate::audio;
use crate::commands::apply_settings;
use crate::models::Settings;
use crate::state::{self, AppState};
use std::sync::Arc;
use std::time::Duration;
use tauri::menu::{CheckMenuItem, ContextMenu, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Wry};

const TRAY_ID: &str = "main";
const ID_OPEN: &str = "open";
const ID_PASTE: &str = "paste";
const ID_QUIT: &str = "quit";
const ID_MIC_DEFAULT: &str = "mic-default";
const MIC_PREFIX: &str = "mic:";

struct Labels {
    open: &'static str,
    paste: &'static str,
    mic: &'static str,
    default_mic: &'static str,
    quit: &'static str,
    hide_hour: &'static str,
    settings: &'static str,
    history: &'static str,
}

fn labels(lang: &str) -> Labels {
    if lang == "en" {
        Labels { open: "Open Pronto", paste: "Paste last transcription", mic: "Microphone", default_mic: "System default", quit: "Quit", hide_hour: "Hide for 1 hour", settings: "Settings", history: "Transcript history" }
    } else {
        Labels { open: "Abrir Pronto", paste: "Pegar última transcripción", mic: "Micrófono", default_mic: "Predeterminado del sistema", quit: "Salir", hide_hour: "Ocultar 1 hora", settings: "Configuración", history: "Historial de transcripciones" }
    }
}

pub fn show_hub(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("hub") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Muestra el hub y, con `page`, le pide navegar (`hub://navigate { page, id? }`).
pub fn open_hub(app: &AppHandle, page: Option<&str>, id: Option<i64>) {
    show_hub(app);
    if let Some(page) = page {
        let payload = match id {
            Some(id) => serde_json::json!({ "page": page, "id": id }),
            None => serde_json::json!({ "page": page }),
        };
        let _ = app.emit("hub://navigate", payload);
    }
}

/// Muestra el hub y le pide abrir Configuración (`hub://settings { tab }`).
pub fn open_settings(app: &AppHandle, tab: Option<&str>) {
    show_hub(app);
    let _ = app.emit("hub://settings", serde_json::json!({ "tab": tab }));
}

// Los ids del menú contextual de la Flow bar llevan este prefijo para no mezclarse con la bandeja:
// Tauri entrega los eventos de ambos menús al manejador global de la app.
const CTX_HIDE: &str = "ctx-hide";
const CTX_SETTINGS: &str = "ctx-settings";
const CTX_HISTORY: &str = "ctx-history";
const CTX_PASTE: &str = "ctx-paste";
const CTX_MIC_DEFAULT: &str = "ctx-mic-default";
const CTX_MIC_PREFIX: &str = "ctx-mic:";

/// Abre el menú nativo de la Flow bar en la posición del cursor.
pub fn show_flowbar_menu(app: &AppHandle, s: &Settings) -> tauri::Result<()> {
    let l = labels(&s.app_language);
    let hide = MenuItem::with_id(app, CTX_HIDE, l.hide_hour, true, None::<&str>)?;
    let settings = MenuItem::with_id(app, CTX_SETTINGS, l.settings, true, None::<&str>)?;

    let mic_menu = Submenu::with_id(app, "ctx-mics", l.mic, true)?;
    mic_menu.append(&CheckMenuItem::with_id(app, CTX_MIC_DEFAULT, l.default_mic, true, s.mic_name.is_none(), None::<&str>)?)?;
    for name in audio::list_input_devices() {
        let checked = s.mic_name.as_deref() == Some(name.as_str());
        mic_menu.append(&CheckMenuItem::with_id(app, format!("{CTX_MIC_PREFIX}{name}"), &name, true, checked, None::<&str>)?)?;
    }

    let history = MenuItem::with_id(app, CTX_HISTORY, l.history, true, None::<&str>)?;
    let paste = MenuItem::with_id(app, CTX_PASTE, l.paste, true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &hide,
            &settings,
            &PredefinedMenuItem::separator(app)?,
            &mic_menu,
            &PredefinedMenuItem::separator(app)?,
            &history,
            &paste,
        ],
    )?;
    match app.get_webview_window("flowbar") {
        Some(w) => menu.popup(w.as_ref().window()),
        None => Ok(()),
    }
}

/// Eventos del menú contextual de la Flow bar (manejador global de la app).
pub fn on_ctx_menu(app: &AppHandle, id: &str) {
    if !id.starts_with("ctx-") {
        return;
    }
    let Some(st) = app.try_state::<Arc<AppState>>() else { return };
    let st = st.inner().clone();
    match id {
        CTX_HIDE => {
            if let Err(e) = state::hide_flowbar_for_hour(app, &st) {
                eprintln!("[tray] {e}");
            }
        }
        CTX_SETTINGS => open_settings(app, None),
        CTX_HISTORY => open_hub(app, Some("home"), None),
        CTX_PASTE => {
            if let Err(e) = state::paste_last(&st, Duration::from_millis(250)) {
                eprintln!("[tray] {e}");
            }
        }
        CTX_MIC_DEFAULT => set_mic(app, &st, None),
        other => {
            if let Some(name) = other.strip_prefix(CTX_MIC_PREFIX) {
                set_mic(app, &st, Some(name.to_string()));
            }
        }
    }
}

fn build_menu(app: &AppHandle, s: &Settings) -> tauri::Result<Menu<Wry>> {
    let l = labels(&s.app_language);
    let open = MenuItem::with_id(app, ID_OPEN, l.open, true, None::<&str>)?;
    let paste = MenuItem::with_id(app, ID_PASTE, l.paste, true, None::<&str>)?;

    let default_item = CheckMenuItem::with_id(app, ID_MIC_DEFAULT, l.default_mic, true, s.mic_name.is_none(), None::<&str>)?;
    let mic_menu = Submenu::with_id(app, "mics", l.mic, true)?;
    mic_menu.append(&default_item)?;
    for name in audio::list_input_devices() {
        let checked = s.mic_name.as_deref() == Some(name.as_str());
        let item = CheckMenuItem::with_id(app, format!("{MIC_PREFIX}{name}"), &name, true, checked, None::<&str>)?;
        mic_menu.append(&item)?;
    }

    let quit = MenuItem::with_id(app, ID_QUIT, l.quit, true, None::<&str>)?;
    Menu::with_items(app, &[&open, &paste, &mic_menu, &PredefinedMenuItem::separator(app)?, &quit])
}

pub fn build(app: &AppHandle, s: &Settings) -> tauri::Result<()> {
    let menu = build_menu(app, s)?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Pronto")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_hub(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

/// Reconstruye el menú (idioma o micrófono elegido cambian).
pub fn rebuild(app: &AppHandle, s: &Settings) {
    match (app.tray_by_id(TRAY_ID), build_menu(app, s)) {
        (Some(tray), Ok(menu)) => {
            if let Err(e) = tray.set_menu(Some(menu)) {
                eprintln!("[tray] no se pudo actualizar el menú: {e}");
            }
        }
        (_, Err(e)) => eprintln!("[tray] no se pudo construir el menú: {e}"),
        _ => {}
    }
}

fn on_menu(app: &AppHandle, id: &str) {
    let Some(st) = app.try_state::<Arc<AppState>>() else { return };
    let st = st.inner().clone();
    match id {
        ID_OPEN => show_hub(app),
        // Margen para que el foco vuelva a la app anterior tras cerrarse el menú.
        ID_PASTE => {
            if let Err(e) = state::paste_last(&st, Duration::from_millis(250)) {
                eprintln!("[tray] {e}");
            }
        }
        ID_QUIT => app.exit(0),
        ID_MIC_DEFAULT => set_mic(app, &st, None),
        other => {
            if let Some(name) = other.strip_prefix(MIC_PREFIX) {
                set_mic(app, &st, Some(name.to_string()));
            }
        }
    }
}

fn set_mic(app: &AppHandle, st: &Arc<AppState>, name: Option<String>) {
    let mut s = st.settings.read().clone();
    s.mic_name = name;
    if let Err(e) = apply_settings(app, st, s) {
        eprintln!("[tray] no se pudo cambiar el micrófono: {e}");
    }
}
