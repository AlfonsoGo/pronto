// Sin consola en release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod aisetup;
mod asr;
mod auto;
mod audio;
mod autostart;
mod commands;
mod db;
mod detect;
mod flowbar;
mod hotkey;
mod inject;
mod media;
mod meeting;
mod meetingdb;
mod modeldl;
mod models;
mod ollama;
mod polish;
mod segment;
mod state;
mod summary;
mod text;
mod tray;
mod updater;

use crate::asr::Engine;
use crate::db::Db;
use crate::state::AppState;
use crossbeam_channel::unbounded;
use std::sync::Arc;
use tauri::{Manager, RunEvent, WindowEvent};

/// Purga de audio caducado al arrancar.
fn db_maintenance(st: &Arc<AppState>) -> anyhow::Result<()> {
    let days = st.settings.read().keep_audio_days;
    st.db.purge_audio(days)?;
    // Una reunión que quedó grabando (la app se cerró) se cierra con lo que se transcribió.
    let n = st.db.close_orphan_meetings()?;
    if n > 0 {
        eprintln!("[main] {n} reunión(es) sin cerrar recuperadas");
    }
    Ok(())
}

fn main() {
    let app = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::list_mics,
            commands::get_stats,
            commands::list_history,
            commands::delete_history,
            commands::clear_history,
            commands::copy_text,
            commands::paste_last,
            commands::list_words,
            commands::add_word,
            commands::update_word,
            commands::delete_word,
            commands::list_snippets,
            commands::add_snippet,
            commands::update_snippet,
            commands::delete_snippet,
            commands::list_notes,
            commands::save_note,
            commands::delete_note,
            commands::start_dictation,
            commands::stop_dictation,
            commands::cancel_dictation,
            commands::engine_status,
            commands::model_status,
            commands::model_download_start,
            commands::update_status,
            commands::update_check,
            commands::update_install,
            commands::ai_setup_start,
            commands::ai_setup_cancel,
            commands::ai_setup_status,
            commands::get_insights,
            commands::copy_last,
            commands::retry_history,
            commands::get_history_audio,
            commands::toggle_word_star,
            commands::list_transforms,
            commands::save_transform,
            commands::delete_transform,
            commands::run_transform,
            commands::ollama_status,
            commands::reset_shortcuts,
            commands::capture_hotkey,
            commands::set_flowbar_hitbox,
            commands::flowbar_context_menu,
            commands::hide_flowbar_for_hour,
            commands::open_hub,
            commands::open_settings,
            commands::start_meeting,
            commands::stop_meeting,
            commands::meeting_status,
            commands::list_meetings,
            commands::get_meeting,
            commands::rename_meeting,
            commands::delete_meeting,
            commands::retry_summary,
            commands::ask_meetings,
            commands::export_meeting,
            commands::toggle_action_item,
        ])
        .on_menu_event(|app, event| tray::on_ctx_menu(app, event.id().as_ref()))
        .on_window_event(|window, event| {
            // Cerrar = ocultar: Pronto sigue en la bandeja. La Flow bar nunca se cierra.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .setup(|app| {
            let handle = app.handle().clone();

            let db = Arc::new(Db::open()?);
            let settings = db.load_settings();
            if let Err(e) = db.purge_history(settings.history_retention_days) {
                eprintln!("[main] purga del historial: {e}");
            }

            let (cmd_tx, cmd_rx) = unbounded();
            let (hk_tx, hk_rx) = unbounded();
            let engine = Arc::new(Engine::new(&settings.model_dir));
            let st = Arc::new(AppState::new(settings.clone(), db, engine.clone(), cmd_tx));
            app.manage(st.clone());

            // Las ventanas se crean AHORA (`"create": false` en tauri.conf.json): si nacieran desde
            // la configuración, su página podría llamar a un comando antes del `manage()` de arriba
            // y fallar con «state not managed».
            for cfg in app.config().app.windows.clone() {
                tauri::WebviewWindowBuilder::from_config(&handle, &cfg)?.build()?;
            }

            engine.load(&handle, settings.model_dir.clone());
            hotkey::set_config(&settings);
            hotkey::start(hk_tx);
            let st_for_init = st.clone();
            state::spawn_controller(handle.clone(), st, hk_rx, cmd_rx);

            if let Err(e) = db_maintenance(&st_for_init) {
                eprintln!("[main] mantenimiento: {e}");
            }
            meeting::spawn_detector(handle.clone(), st_for_init.clone());
            flowbar::init(&handle, flowbar::rest_visible(&settings));
            // Si arrancamos con la barra oculta temporalmente, vuelve sola cuando toque.
            if !flowbar::rest_visible(&settings) && settings.show_flow_bar {
                let (h, s) = (handle.clone(), st_for_init.clone());
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(61 * 60));
                    state::refresh_flowbar(&h, &s);
                });
            }
            if let Err(e) = tray::build(&handle, &settings) {
                eprintln!("[main] no se pudo crear el icono de bandeja: {e}");
            }
            updater::spawn_background(handle.clone(), st_for_init.clone());

            // El hub se muestra al arrancar, salvo si Windows nos lanzó al iniciar sesión.
            if !std::env::args().any(|a| a == autostart::HIDDEN_FLAG) {
                tray::show_hub(&handle);
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error al construir Pronto");

    app.run(|_app, event| {
        // Con el hub oculto no queda ninguna ventana visible; eso no debe cerrar la app.
        if let RunEvent::ExitRequested { api, code, .. } = event {
            if code.is_none() {
                api.prevent_exit();
            }
        }
    });
}
