//! Comandos IPC del contrato (`#[tauri::command]`, siempre `Result<T, String>`).
//!
//! Los comandos síncronos corren en el hilo principal: aquí solo hay consultas rápidas a SQLite.
//! Lo que puede tardar (reintentar una transcripción, esperar una captura de atajo, hablar con
//! Ollama, leer audio) es `async` y trabaja en `spawn_blocking`.

use crate::aisetup::AiSetupStatus;
use crate::models::*;
use crate::updater::{self, UpdateStatus};
use crate::state::{self, AppState, Cmd};
use crate::{aisetup, audio, autostart, db, flowbar, hotkey, inject, meeting, modeldl, ollama, summary, tray};
use base64::Engine as _;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};

type St<'a> = State<'a, Arc<AppState>>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Aplica unos ajustes nuevos: guarda, recarga atajos/motor/arranque y avisa a las ventanas.
/// Lo usan `save_settings`, `reset_shortcuts` y los menús (cambio de micrófono).
pub fn apply_settings(app: &AppHandle, st: &Arc<AppState>, mut new: Settings) -> Result<(), String> {
    if new.model_dir.trim().is_empty() {
        new.model_dir = Settings::default().model_dir;
    }
    if new.ollama_url.trim().is_empty() {
        new.ollama_url = Settings::default().ollama_url;
    }
    // `meetingDetect` (booleano antiguo) y `meetingAuto` van a la par. Si la UI solo cambió el
    // booleano, manda ese; si no, manda `meetingAuto`.
    {
        let prev = st.settings.read();
        if new.meeting_auto == prev.meeting_auto && new.meeting_detect != prev.meeting_detect {
            new.meeting_auto = if new.meeting_detect { MeetingAuto::Auto } else { MeetingAuto::Off };
        }
    }
    new.meeting_detect = new.meeting_auto != MeetingAuto::Off;
    st.db.save_settings(&new).map_err(err)?;
    let old = std::mem::replace(&mut *st.settings.write(), new.clone());

    hotkey::set_config(&new);
    if old.model_dir != new.model_dir {
        st.engine.load(app, new.model_dir.clone());
    }
    if old.launch_at_login != new.launch_at_login {
        if let Err(e) = autostart::set(new.launch_at_login) {
            eprintln!("[settings] inicio con Windows: {e}");
        }
    }
    if old.show_flow_bar != new.show_flow_bar || old.flow_bar_hidden_until != new.flow_bar_hidden_until {
        state::refresh_flowbar(app, st);
    }
    if old.history_retention_days != new.history_retention_days {
        if let Err(e) = st.db.purge_history(new.history_retention_days) {
            eprintln!("[settings] purga del historial: {e}");
        }
    }
    if old.keep_audio_days != new.keep_audio_days {
        if let Err(e) = st.db.purge_audio(new.keep_audio_days) {
            eprintln!("[settings] purga de audio: {e}");
        }
    }
    let _ = app.emit("settings://changed", new.clone());
    tray::rebuild(app, &new);
    Ok(())
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(err)?
}

// ------------------------------------------------------------ ajustes

#[tauri::command]
pub fn get_settings(st: St) -> Result<Settings, String> {
    Ok(st.settings.read().clone())
}

#[tauri::command]
pub fn save_settings(app: AppHandle, st: St, settings: Settings) -> Result<(), String> {
    apply_settings(&app, st.inner(), settings)
}

/// Restablece los atajos a los de fábrica y devuelve los ajustes resultantes.
#[tauri::command]
pub fn reset_shortcuts(app: AppHandle, st: St) -> Result<Settings, String> {
    let defaults = Settings::default();
    let mut s = st.settings.read().clone();
    s.hotkey_hold = defaults.hotkey_hold;
    s.hotkey_hands_free = defaults.hotkey_hands_free;
    s.hotkey_paste_last = defaults.hotkey_paste_last;
    s.hotkey_copy_last = defaults.hotkey_copy_last;
    s.hotkey_transform = defaults.hotkey_transform;
    s.hotkey_meeting = defaults.hotkey_meeting;
    apply_settings(&app, st.inner(), s.clone())?;
    Ok(s)
}

/// Captura la siguiente combinación pulsada y soltada (Esc cancela con error).
#[tauri::command]
pub async fn capture_hotkey() -> Result<Hotkey, String> {
    blocking(|| hotkey::capture(Duration::from_secs(30))).await
}

#[tauri::command]
pub fn list_mics() -> Result<Vec<String>, String> {
    Ok(audio::list_input_devices())
}

// ------------------------------------------------------------ estadísticas e historial

#[tauri::command]
pub fn get_stats(st: St) -> Result<Stats, String> {
    st.db.stats().map_err(err)
}

#[tauri::command]
pub fn get_insights(st: St) -> Result<Insights, String> {
    st.db.insights().map_err(err)
}

#[tauri::command]
pub fn list_history(st: St, query: Option<String>, limit: i64, offset: i64) -> Result<Vec<HistoryItem>, String> {
    st.db.list_history(query.as_deref(), limit, offset).map_err(err)
}

#[tauri::command]
pub fn delete_history(st: St, id: i64) -> Result<(), String> {
    st.db.delete_history(id).map_err(err)
}

#[tauri::command]
pub fn clear_history(st: St) -> Result<(), String> {
    st.db.clear_history().map_err(err)
}

#[tauri::command]
pub fn copy_text(text: String) -> Result<(), String> {
    inject::copy_text(&text)
}

#[tauri::command]
pub fn paste_last(st: St) -> Result<(), String> {
    state::paste_last(st.inner(), Duration::ZERO)
}

#[tauri::command]
pub fn copy_last(app: AppHandle, st: St) -> Result<(), String> {
    state::copy_last(&app, st.inner())
}

/// Vuelve a transcribir el audio guardado de un dictado y actualiza su texto. No pega.
#[tauri::command]
pub async fn retry_history(st: St<'_>, id: i64) -> Result<HistoryItem, String> {
    let st = st.inner().clone();
    blocking(move || {
        let item = st.db.get_history(id).map_err(err)?.ok_or("Ese dictado ya no existe")?;
        let path = db::audio_path(id).ok_or("No se encontró la carpeta de audio")?;
        let bytes = std::fs::read(&path).map_err(|_| "Este dictado no tiene audio guardado".to_string())?;
        let samples = audio::decode_wav(&bytes).ok_or("El audio guardado está dañado")?;
        let raw = st.engine.transcribe(&samples)?;
        let processed = state::run_pipeline(&st, &raw, &item.app, "");
        if processed.text.trim().is_empty() {
            return Err("No se ha oído nada".into());
        }
        let words = processed.text.split_whitespace().count() as i64;
        st.db.update_history_text(id, &processed.text, &raw, words).map_err(err)?;
        st.db.get_history(id).map_err(err)?.ok_or_else(|| "Ese dictado ya no existe".to_string())
    })
    .await
}

/// WAV del dictado en base64 (para reproducir con un data URL), o `null` si no se guardó.
#[tauri::command]
pub async fn get_history_audio(id: i64) -> Result<Option<String>, String> {
    blocking(move || {
        let Some(path) = db::audio_path(id) else { return Ok(None) };
        match std::fs::read(&path) {
            Ok(bytes) => Ok(Some(base64::engine::general_purpose::STANDARD.encode(bytes))),
            Err(_) => Ok(None),
        }
    })
    .await
}

// ------------------------------------------------------------ diccionario, snippets, notas

#[tauri::command]
pub fn list_words(st: St) -> Result<Vec<Word>, String> {
    st.db.list_words().map_err(err)
}

#[tauri::command]
pub fn add_word(st: St, word: String, replacement: Option<String>) -> Result<Word, String> {
    if word.trim().is_empty() {
        return Err("La palabra no puede estar vacía".into());
    }
    st.db.add_word(&word, replacement.as_deref()).map_err(err)
}

#[tauri::command]
pub fn update_word(st: St, id: i64, word: String, replacement: Option<String>) -> Result<(), String> {
    if word.trim().is_empty() {
        return Err("La palabra no puede estar vacía".into());
    }
    st.db.update_word(id, &word, replacement.as_deref()).map_err(err)
}

#[tauri::command]
pub fn toggle_word_star(st: St, id: i64) -> Result<(), String> {
    st.db.toggle_word_star(id).map_err(err)
}

#[tauri::command]
pub fn delete_word(st: St, id: i64) -> Result<(), String> {
    st.db.delete_word(id).map_err(err)
}

#[tauri::command]
pub fn list_snippets(st: St) -> Result<Vec<Snippet>, String> {
    st.db.list_snippets().map_err(err)
}

#[tauri::command]
pub fn add_snippet(st: St, trigger: String, expansion: String) -> Result<Snippet, String> {
    if trigger.trim().is_empty() || expansion.is_empty() {
        return Err("El disparador y la expansión no pueden estar vacíos".into());
    }
    st.db.add_snippet(&trigger, &expansion).map_err(err)
}

#[tauri::command]
pub fn update_snippet(st: St, id: i64, trigger: String, expansion: String) -> Result<(), String> {
    if trigger.trim().is_empty() || expansion.is_empty() {
        return Err("El disparador y la expansión no pueden estar vacíos".into());
    }
    st.db.update_snippet(id, &trigger, &expansion).map_err(err)
}

#[tauri::command]
pub fn delete_snippet(st: St, id: i64) -> Result<(), String> {
    st.db.delete_snippet(id).map_err(err)
}

#[tauri::command]
pub fn list_notes(st: St) -> Result<Vec<Note>, String> {
    st.db.list_notes().map_err(err)
}

#[tauri::command]
pub fn save_note(st: St, id: Option<i64>, text: String) -> Result<Note, String> {
    st.db.save_note(id, &text).map_err(err)
}

#[tauri::command]
pub fn delete_note(st: St, id: i64) -> Result<(), String> {
    st.db.delete_note(id).map_err(err)
}

// ------------------------------------------------------------ transforms y Ollama

#[tauri::command]
pub fn list_transforms(st: St) -> Result<Vec<Transform>, String> {
    st.db.list_transforms().map_err(err)
}

/// Crea (sin `id`) o actualiza un transform y lo devuelve.
#[tauri::command]
pub fn save_transform(st: St, transform: Transform) -> Result<Transform, String> {
    if transform.name.trim().is_empty() || transform.prompt.trim().is_empty() {
        return Err("El nombre y el prompt no pueden estar vacíos".into());
    }
    st.db.save_transform(&transform).map_err(err)
}

/// Los transforms de fábrica no se pueden borrar.
#[tauri::command]
pub fn delete_transform(st: St, id: i64) -> Result<(), String> {
    st.db.delete_transform(id).map_err(err)
}

/// Aplica un transform a la selección actual de la app con foco. Corre en segundo plano y
/// informa con `dictation://state` (`polishing` -> `polished` o `error`).
#[tauri::command]
pub fn run_transform(st: St, id: i64) -> Result<(), String> {
    if st.db.get_transform(id).map_err(err)?.is_none() {
        return Err("Ese transform no existe".into());
    }
    st.send(Cmd::RunTransform { id });
    Ok(())
}

#[tauri::command]
pub async fn ollama_status(st: St<'_>) -> Result<OllamaStatus, String> {
    let url = st.settings.read().ollama_url.clone();
    blocking(move || Ok(ollama::status(&url))).await
}

// ------------------------------------------------------------ dictado y Flow bar

#[tauri::command]
pub fn start_dictation(st: St, hands_free: bool) -> Result<(), String> {
    st.send(Cmd::UiStart { hands_free });
    Ok(())
}

#[tauri::command]
pub fn stop_dictation(st: St) -> Result<(), String> {
    st.send(Cmd::UiStop);
    Ok(())
}

#[tauri::command]
pub fn cancel_dictation(st: St) -> Result<(), String> {
    st.send(Cmd::UiCancel);
    Ok(())
}

#[tauri::command]
pub fn engine_status(st: St) -> Result<EngineStatus, String> {
    Ok(st.engine.status())
}

/// Qué ficheros del modelo de voz hay en `modelDir` y cómo va la descarga, si hay una.
#[tauri::command]
pub fn model_status(st: St) -> Result<ModelStatus, String> {
    Ok(modeldl::status(&st.settings.read().model_dir))
}

/// Descarga el modelo de voz en segundo plano (`model://progress`) y carga el motor al acabar.
/// Si ya hay una descarga en marcha no hace nada.
#[tauri::command]
pub fn model_download_start(app: AppHandle, st: St) -> Result<(), String> {
    modeldl::start(&app, st.inner())
}

/// «Activar IA local»: instala Ollama si falta, lo arranca y baja `qwen3:1.7b` en segundo plano
/// (`ai://progress`). Si ya hay un proceso en marcha no hace nada.
#[tauri::command]
pub fn ai_setup_start(app: AppHandle, st: St) -> Result<(), String> {
    aisetup::start(&app, st.inner())
}

/// Pide parar la activación de la IA local (surte efecto en la descarga; un instalador en marcha termina).
#[tauri::command]
pub fn ai_setup_cancel() -> Result<(), String> {
    aisetup::cancel();
    Ok(())
}

/// Qué hay de IA local en este PC (Ollama, modelo) y cómo va la activación, si hay una.
#[tauri::command]
pub async fn ai_setup_status(st: St<'_>) -> Result<AiSetupStatus, String> {
    let settings = st.settings.read().clone();
    blocking(move || Ok(aisetup::status(&settings))).await
}

/// Estado del actualizador (sin red): versión actual, versión nueva si la hay, progreso y último error.
#[tauri::command]
pub fn update_status() -> Result<UpdateStatus, String> {
    Ok(updater::status())
}

/// «Buscar ahora»: consulta GitHub y devuelve el estado. Falla con un mensaje si no hay conexión.
#[tauri::command]
pub async fn update_check(app: AppHandle) -> Result<UpdateStatus, String> {
    blocking(move || updater::check(&app).map_err(|e| e.msg)).await
}

/// «Actualizar ahora»: descarga y verifica el instalador (`update://progress`), cierra Pronto y deja que un
/// ayudante lo instale en silencio y lo vuelva a abrir. Vuelve enseguida.
#[tauri::command]
pub fn update_install(app: AppHandle, st: St) -> Result<(), String> {
    updater::install(&app, st.inner())
}

/// Zona pulsable de la Flow bar (px CSS relativos a la ventana); `null` = nada pulsable.
#[tauri::command]
pub fn set_flowbar_hitbox(rect: Option<Rect>) -> Result<(), String> {
    flowbar::set_hitbox(rect);
    Ok(())
}

/// Menú contextual nativo de la píldora (clic derecho).
#[tauri::command]
pub fn flowbar_context_menu(app: AppHandle, st: St) -> Result<(), String> {
    let settings = st.settings.read().clone();
    tray::show_flowbar_menu(&app, &settings).map_err(err)
}

#[tauri::command]
pub fn hide_flowbar_for_hour(app: AppHandle, st: St) -> Result<(), String> {
    state::hide_flowbar_for_hour(&app, st.inner())
}

/// Muestra y enfoca el hub; con `page` emite `hub://navigate { page, id? }` (`id` abre el detalle,
/// p. ej. `{ page: "notetaker", id }` para una reunión).
#[tauri::command]
pub fn open_hub(app: AppHandle, page: Option<String>, id: Option<i64>) -> Result<(), String> {
    tray::open_hub(&app, page.as_deref(), id);
    Ok(())
}

/// Abre el hub y emite `hub://settings { tab }` para mostrar la Configuración.
#[tauri::command]
pub fn open_settings(app: AppHandle, tab: Option<String>) -> Result<(), String> {
    tray::open_settings(&app, tab.as_deref());
    Ok(())
}

// ------------------------------------------------------------ Notetaker (reuniones)

/// Empieza a tomar notas (mic + audio del sistema). Devuelve la reunión creada.
#[tauri::command]
pub async fn start_meeting(app: AppHandle, st: St<'_>, title: Option<String>) -> Result<Meeting, String> {
    let st = st.inner().clone();
    blocking(move || meeting::start(&app, &st, title)).await
}

/// Para la reunión; el resumen llega después por `meeting://summary`.
#[tauri::command]
pub async fn stop_meeting(app: AppHandle, st: St<'_>) -> Result<Meeting, String> {
    let st = st.inner().clone();
    blocking(move || meeting::stop(&app, &st)).await
}

#[tauri::command]
pub async fn meeting_status(st: St<'_>) -> Result<MeetingStatus, String> {
    let st = st.inner().clone();
    blocking(move || {
        Ok(match st.meeting.status() {
            Some((meeting, elapsed_ms)) => MeetingStatus { active: true, meeting: Some(meeting), elapsed_ms },
            None => MeetingStatus { active: false, meeting: None, elapsed_ms: 0 },
        })
    })
    .await
}

#[tauri::command]
pub fn list_meetings(st: St, query: Option<String>, limit: i64, offset: i64) -> Result<Vec<MeetingListItem>, String> {
    st.db.list_meetings(query.as_deref(), limit, offset).map_err(err)
}

#[tauri::command]
pub fn get_meeting(st: St, id: i64) -> Result<MeetingDetail, String> {
    st.db.get_meeting_detail(id).map_err(err)?.ok_or_else(|| "Esa reunión ya no existe".to_string())
}

#[tauri::command]
pub fn rename_meeting(st: St, id: i64, title: String) -> Result<(), String> {
    if title.trim().is_empty() {
        return Err("El título no puede estar vacío".into());
    }
    st.db.rename_meeting(id, &title).map_err(err)
}

#[tauri::command]
pub fn delete_meeting(st: St, id: i64) -> Result<(), String> {
    if st.meeting.status().is_some_and(|(m, _)| m.id == id) {
        return Err("La reunión sigue en curso: párala antes de eliminarla".into());
    }
    st.db.delete_meeting(id).map_err(err)?;
    meeting::remove_audio_files(id);
    Ok(())
}

/// Vuelve a pedir el resumen a Ollama (en segundo plano; avisa con `meeting://state`/`summary`).
#[tauri::command]
pub fn retry_summary(app: AppHandle, st: St, id: i64) -> Result<(), String> {
    meeting::retry_summary(&app, st.inner(), id)
}

/// Pregunta libre sobre las reuniones (o sobre una sola con `meetingId`).
#[tauri::command]
pub async fn ask_meetings(st: St<'_>, question: String, meeting_id: Option<i64>) -> Result<String, String> {
    let st = st.inner().clone();
    blocking(move || meeting::ask(&st, &question, meeting_id)).await
}

/// La reunión en Markdown: título, fecha, resumen, tareas, decisiones y transcripción.
#[tauri::command]
pub fn export_meeting(st: St, id: i64) -> Result<String, String> {
    let detail = st.db.get_meeting_detail(id).map_err(err)?.ok_or("Esa reunión ya no existe")?;
    Ok(summary::to_markdown(&detail))
}

/// Marca o desmarca una tarea del resumen.
#[tauri::command]
pub fn toggle_action_item(st: St, meeting_id: i64, index: i64) -> Result<(), String> {
    st.db.toggle_action_item(meeting_id, index).map_err(err)
}
