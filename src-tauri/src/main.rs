// Sin consola en release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod aisetup;
mod anclaje;
mod asr;
mod auto;
mod audio;
mod autostart;
mod commands;
mod db;
mod detect;
mod errlog;
mod flowbar;
mod hotkey;
mod inject;
mod media;
mod meeting;
mod meetingdb;
mod modeldl;
mod models;
mod notes;
mod ollama;
mod polish;
mod pronounce;
mod segment;
mod state;
mod summary;
mod text;
mod tray;
mod updater;
mod voiceprint;
mod whisper;
#[cfg(test)]
mod rompedor_sondas;
#[cfg(test)]
mod rompedor_reuniones;
#[cfg(test)]
mod rompedor_dictado;
#[cfg(test)]
mod rompedor_ia;
#[cfg(test)]
mod rompedor_descargas;
#[cfg(test)]
mod rompedor_carga;
#[cfg(test)]
mod rompedor_datos;
#[cfg(test)]
mod rompedor_whisper;

use crate::asr::Engine;
use crate::db::Db;
use crate::state::AppState;
use crossbeam_channel::unbounded;
use std::sync::Arc;
use tauri::{Manager, RunEvent, WindowEvent};

/// Copia diaria de la base y purga de audio caducado al arrancar.
fn db_maintenance(app: &tauri::AppHandle, st: &Arc<AppState>) -> anyhow::Result<()> {
    if let Some(dir) = db::backup_dir() {
        match st.db.daily_backup(&dir, 7) {
            // La base tiene mucho menos que la última copia buena: no se ha borrado ninguna copia.
            Ok(Some(p)) if p.to_string_lossy().contains("sospechosa") => {
                let h = app.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(10));
                    state::emit_notice(&h, "Tu base tiene muchos menos datos que la última copia", Some("Se conservan las copias buenas en la carpeta copias de Pronto. Si echas algo en falta, restáuralo desde ahí: cierra Pronto (Salir), aparta pronto2.db-wal y pronto2.db-shm y copia la copia encima de pronto2.db."));
                });
            }
            Ok(_) => {}
            Err(e) => errlog::error("main", format!("copia diaria de la base: {e}")),
        }
    }
    let days = st.settings.read().keep_audio_days;
    startup_cleanup(&st.db, days);
    Ok(())
}

/// Purga del audio caducado y cierre de las reuniones que quedaron a medias. Un paso que falla se
/// registra y no se salta el otro.
fn startup_cleanup(db: &Db, days: u32) {
    if let Err(e) = db.purge_audio(days) {
        errlog::error("main", format!("purga del audio caducado: {e}"));
    }
    // Una reunión que quedó grabando o a medio procesar (la app se cerró) queda para reintentar.
    match db.close_orphan_meetings() {
        Ok(0) => {}
        Ok(n) => eprintln!("[main] {n} reunión(es) sin cerrar recuperadas"),
        Err(e) => errlog::error("main", format!("reuniones sin cerrar: {e}")),
    }
}

/// Ajustes guardados que hay que corregir al arrancar, y se guardan ya corregidos:
/// - Ollama en otro equipo (de una versión que lo dejaba): vuelve a la de fábrica y se devuelve el aviso
///   que enseñar una vez (la siguiente vez ya es la de fábrica). Decisión de Txonso, 23-oct-2026, regla 1.
/// - `meetingAutoConfirmed`: quien ya tiene reuniones grabadas viene de una versión que grababa sin
///   preguntar y no se ve preguntado de repente (decisión de Txonso, 23-oct-2026).
fn fix_stored_settings(db: &Db, settings: &mut models::Settings) -> Option<String> {
    let mut changed = false;
    let mut aviso = None;
    if let Some(host) = aisetup::force_local(settings) {
        changed = true;
        errlog::error("main", "los ajustes traían Ollama fuera de este PC: se vuelve a la dirección de fábrica");
        aviso = Some(format!("Tus ajustes apuntaban a «{host}», que no es de este equipo. Pronto la ha cambiado por {}.", settings.ollama_url));
    }
    if !settings.meeting_auto_confirmed {
        let meetings = db.list_meetings(None, 1, 0).map(|m| m.len()).unwrap_or(0);
        if auto::confirmed_with_history(false, meetings) {
            settings.meeting_auto_confirmed = true;
            changed = true;
        }
    }
    if changed {
        if let Err(e) = db.save_settings(settings) {
            errlog::error("main", format!("no se pudieron guardar los ajustes corregidos al arrancar: {e}"));
        }
    }
    aviso
}

fn main() {
    // Antes que nada: los pánicos y los errores se apuntan en `%APPDATA%\Pronto2\logs\errores.log`
    // (sin consola, el `eprintln!` no lo ve nadie). Todo local, sin red (regla 1).
    errlog::init();
    // El arranque que lanza el usuario tiene permiso para pasar al frente; se lo cede al Pronto que ya
    // está en marcha, que es quien enseña el hub (si no, Windows lo deja detrás de la ventana activa).
    // SAFETY: llamada Win32 sin punteros.
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow(windows::Win32::UI::WindowsAndMessaging::ASFW_ANY);
    }
    let app = tauri::Builder::default()
        // Un solo Pronto: dos a la vez se pelean por el atajo, el micro y la base de datos. Abrir
        // otra vez (menú Inicio, acceso directo) enseña el hub del que ya está en marcha; un segundo
        // arranque oculto (con Windows) no hace nada.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if !argv.iter().any(|a| a == autostart::HIDDEN_FLAG) {
                tray::show_hub(app);
            }
        }))
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
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
            commands::asr_options,
            commands::whisper_download_start,
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
            commands::add_word_variant,
            commands::delete_word_variant,
            commands::add_word_sample,
            commands::delete_word_sample,
            commands::get_word_sample_audio,
            commands::pronounce_record,
            commands::pronounce_stop,
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
            commands::pin_flowbar,
            commands::list_monitors,
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
            commands::open_error_log,
        ])
        .on_menu_event(|app, event| tray::on_ctx_menu(app, event.id().as_ref()))
        .on_window_event(|window, event| {
            // Cerrar el hub lo destruye (libera su WebView2); Pronto sigue en la bandeja. La Flow bar
            // nunca se cierra.
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() != "hub" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            let handle = app.handle().clone();

            let db = match Db::open() {
                Ok(db) => Arc::new(db),
                Err(e) => {
                    // Sin consola no queda otro rastro de por qué Pronto no arranca.
                    errlog::error("main", format!("no se pudo abrir la base de datos: {e:#}"));
                    return Err(e.into());
                }
            };
            // Si al abrir hubo que apartar algo (un -wal ajeno, `Db::open_at`), se avisa en la Flow
            // bar cuando ya esté escuchando; el detalle queda en `apartado-*\LEEME.txt`.
            if let Some(aviso) = db.aviso().map(String::from) {
                let h = handle.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(8));
                    state::emit_notice(&h, "Pronto apartó un fichero raro de la base", Some(&aviso));
                });
            }
            let mut settings = db.load_settings();
            if let Some(aviso) = fix_stored_settings(&db, &mut settings) {
                let h = handle.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(10));
                    state::emit_notice(&h, "Pronto solo usa Ollama en este PC", Some(&aviso));
                });
            }
            if let Err(e) = db.purge_history(settings.history_retention_days) {
                errlog::error("main", format!("purga del historial: {e}"));
            }

            let (cmd_tx, cmd_rx) = unbounded();
            let (hk_tx, hk_rx) = unbounded();
            let engine = Arc::new(Engine::new(&settings.model_dir));
            let st = Arc::new(AppState::new(settings.clone(), db, engine.clone(), cmd_tx));
            app.manage(st.clone());

            // Las ventanas se crean AHORA (`"create": false` en tauri.conf.json): si nacieran desde
            // la configuración, su página podría llamar a un comando antes del `manage()` de arriba
            // y fallar con «state not managed».
            // El hub no: se crea al abrirlo (`tray::show_hub`) y se destruye al cerrarlo, para que
            // Pronto en la bandeja no tenga cogida su WebView2 (~300 MB).
            for cfg in app.config().app.windows.clone().into_iter().filter(|w| w.label != "hub") {
                tauri::WebviewWindowBuilder::from_config(&handle, &cfg)?.build()?;
            }

            engine.set_choice(asr::Choice::from_settings(&settings));
            engine.load(&handle, settings.model_dir.clone());
            // El modelo del pulido, cargado desde el arranque: el primer dictado no espera a Ollama.
            polish::warm_up(&settings);
            hotkey::set_config(&settings);
            hotkey::start(hk_tx);
            let st_for_init = st.clone();
            state::spawn_controller(handle.clone(), st, hk_rx, cmd_rx);

            if let Err(e) = db_maintenance(&handle, &st_for_init) {
                errlog::error("main", format!("mantenimiento: {e}"));
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
                errlog::error("main", format!("no se pudo crear el icono de bandeja: {e}"));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_fallo_de_la_purga_no_se_salta_las_reuniones_sin_cerrar() {
        let db = Db::open_memory().unwrap();
        let m = db.create_meeting("Sin cerrar", None).unwrap();
        db.conn.lock().execute_batch("DROP TABLE history").unwrap(); // la purga falla
        assert!(db.purge_audio(14).is_err());
        startup_cleanup(&db, 14);
        assert_eq!(db.get_meeting(m.id).unwrap().unwrap().summary_status, "failed");
    }

    #[test]
    fn al_arrancar_una_ollama_remota_guardada_vuelve_a_la_de_fabrica() {
        let db = Db::open_memory().unwrap();
        let mut s = models::Settings { ollama_url: "http://192.168.1.50:11434".into(), ..models::Settings::default() };
        db.save_settings(&s).unwrap();
        let aviso = fix_stored_settings(&db, &mut s).expect("avisa una vez");
        assert!(aviso.contains("192.168.1.50"), "{aviso}");
        assert_eq!(s.ollama_url, models::Settings::default().ollama_url);
        // Lo corregido queda guardado: la siguiente vez ya no avisa.
        let mut again = db.load_settings();
        assert_eq!(again.ollama_url, s.ollama_url);
        assert_eq!(fix_stored_settings(&db, &mut again), None);
    }

    #[test]
    fn quien_ya_tiene_reuniones_arranca_como_confirmado_y_quien_no_la_primera_vez_pregunta() {
        let db = Db::open_memory().unwrap();
        let mut s = db.load_settings();
        assert!(!s.meeting_auto_confirmed);
        assert_eq!(fix_stored_settings(&db, &mut s), None);
        assert!(!s.meeting_auto_confirmed, "sin reuniones: se le preguntará la primera vez");
        assert!(!db.load_settings().meeting_auto_confirmed);
        db.create_meeting("Una de antes", None).unwrap();
        assert_eq!(fix_stored_settings(&db, &mut s), None);
        assert!(s.meeting_auto_confirmed, "con reuniones grabadas cuenta como confirmado");
        assert!(db.load_settings().meeting_auto_confirmed, "y se guarda");
    }
}
