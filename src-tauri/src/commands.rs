//! Comandos IPC del contrato (`#[tauri::command]`, siempre `Result<T, String>`).
//!
//! Los comandos síncronos corren en el hilo principal: aquí solo hay consultas rápidas a SQLite.
//! Lo que puede tardar (reintentar una transcripción, esperar una captura de atajo, hablar con
//! Ollama, leer audio) es `async` y trabaja en `spawn_blocking`.

use crate::aisetup::AiSetupStatus;
use crate::models::*;
use crate::updater::{self, UpdateStatus};
use crate::state::{self, AppState, Cmd};
use crate::{aisetup, audio, autostart, db, errlog, flowbar, hotkey, inject, meeting, modeldl, ollama, pronounce, summary, tray};
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
    new.ollama_url = resolve_ollama_url(&new.ollama_url, &st.settings.read().ollama_url)?;
    // `meetingDetect` (booleano antiguo) y `meetingAuto` van a la par. Si la UI solo cambió el
    // booleano, manda ese; si no, manda `meetingAuto`.
    {
        let prev = st.settings.read();
        if new.meeting_auto == prev.meeting_auto && new.meeting_detect != prev.meeting_detect {
            new.meeting_auto = if new.meeting_detect { MeetingAuto::Auto } else { MeetingAuto::Off };
        }
        // Monitor de la barra: el rectángulo lo pone el backend al cambiar de monitor (la UI solo
        // manda el nombre de dispositivo); si no cambia, se conserva el guardado.
        new.flow_bar_monitor_rect = if new.flow_bar_monitor == prev.flow_bar_monitor {
            prev.flow_bar_monitor_rect
        } else {
            new.flow_bar_monitor.as_deref().and_then(flowbar::monitor_rect)
        };
        new.meeting_auto_confirmed = meeting_confirmed(prev.meeting_auto_confirmed, prev.meeting_auto, new.meeting_auto, new.meeting_auto_confirmed);
    }
    new.meeting_detect = new.meeting_auto != MeetingAuto::Off;
    // Una carpeta del modelo nueva no se guarda hasta que el motor la carga bien: si no vale (le
    // falta algo, está dañada), el motor dice por qué y los ajustes conservan la que funcionaba. Si
    // se guardara antes, un modelo malo dejaría a Pronto sin motor en cada arranque (rompedor, ALTA 2).
    let wanted_dir = std::mem::replace(&mut new.model_dir, st.settings.read().model_dir.clone());
    st.db.save_settings(&new).map_err(err)?;
    let old = std::mem::replace(&mut *st.settings.write(), new.clone());

    hotkey::set_config(&new);
    let choice = crate::asr::Choice::from_settings(&new);
    if old.model_dir != wanted_dir {
        st.engine.set_choice(choice);
        let (app2, st2, dir) = (app.clone(), st.clone(), wanted_dir.clone());
        st.engine.load_then(app, wanted_dir, move || adopt_model_dir(&app2, &st2, dir));
    } else if choice != st.engine.choice() {
        st.engine.set_choice(choice);
        st.engine.load(app, new.model_dir.clone());
    }
    if old.launch_at_login != new.launch_at_login {
        if let Err(e) = autostart::set(new.launch_at_login) {
            errlog::error("settings", format!("inicio con Windows: {e}"));
        }
    }
    if old.show_flow_bar != new.show_flow_bar
        || old.flow_bar_hidden_until != new.flow_bar_hidden_until
        || old.flow_bar_monitor != new.flow_bar_monitor
    {
        state::refresh_flowbar(app, st);
    }
    if old.history_retention_days != new.history_retention_days {
        if let Err(e) = st.db.purge_history(new.history_retention_days) {
            errlog::error("settings", format!("purga del historial: {e}"));
        }
    }
    if old.keep_audio_days != new.keep_audio_days {
        // En un hilo: `save_settings` va en el hilo principal y una purga masiva lo congelaba hasta 3 s
        // (rompedor 23-oct, carga BAJA 2).
        let (st2, days) = (st.clone(), new.keep_audio_days);
        let launched = std::thread::Builder::new().name("pronto-purga-audio".into()).spawn(move || {
            if let Err(e) = st2.db.purge_audio(days) {
                errlog::error("settings", format!("purga de audio: {e}"));
            }
        });
        if let Err(e) = launched {
            errlog::error("settings", format!("purga de audio: no se pudo lanzar el hilo ({e})"));
        }
    }
    let _ = app.emit("settings://changed", new.clone());
    tray::rebuild(app, &new);
    Ok(())
}

/// La dirección de Ollama que se guardará. Vacía → la de fábrica. Una distinta de la guardada se
/// normaliza (sin esquema → `http://`) y se valida AQUÍ, en un solo sitio: si no, `ollama.rs` la leía mal
/// y el pulido fallaba sin aviso (rompedor 23-oct, descargas MEDIA 2). Decisión de Txonso, 23-oct-2026
/// (regla 1): además tiene que ser de este PC (loopback); una dirección de otro equipo se rechaza con un
/// error claro y no se guarda (antes solo se avisaba). Una ya guardada no se toca, para que un ajuste
/// ajeno (el micrófono del menú) no falle por ella (al arrancar, `main` ya cambia las remotas viejas).
fn resolve_ollama_url(wanted: &str, stored: &str) -> Result<String, String> {
    if wanted.trim().is_empty() {
        Ok(Settings::default().ollama_url)
    } else if wanted != stored {
        aisetup::checked_local_url(wanted)
    } else {
        Ok(wanted.to_string())
    }
}

/// `meetingAutoConfirmed` (decisión de Txonso, 23-oct-2026: la primera vez que «Automático» fuera a
/// grabar, se pregunta). Un «sí» no se deshace guardando ajustes (una UI antigua puede mandarlo en
/// `false`), y elegir «Automático» a mano en Ajustes también es haberlo dicho.
fn meeting_confirmed(prev_confirmed: bool, prev: MeetingAuto, new: MeetingAuto, sent: bool) -> bool {
    prev_confirmed || sent || (new == MeetingAuto::Auto && prev != MeetingAuto::Auto)
}

/// La carpeta del modelo ya cargó bien: ahora sí se guarda en los ajustes.
fn adopt_model_dir(app: &AppHandle, st: &Arc<AppState>, dir: String) {
    let s = {
        let mut w = st.settings.write();
        w.model_dir = dir;
        w.clone()
    };
    if let Err(e) = st.db.save_settings(&s) {
        errlog::error("settings", format!("guardar la carpeta del modelo: {e}"));
    }
    let _ = app.emit("settings://changed", s);
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

/// Reintentos en marcha, por id de dictado. Un segundo clic en «Reintentar» sobre el mismo dictado
/// no encola otra transcripción entera: espera a la primera y devuelve su resultado (rompedor
/// 22-oct-c, carga MEDIA 1: dos reintentos de 20 min seguidos eran 3 min de motor ocupado).
static RETRYING: parking_lot::Mutex<Vec<i64>> = parking_lot::Mutex::new(Vec::new());
static RETRY_DONE: parking_lot::Condvar = parking_lot::Condvar::new();

/// Cómo acabó el último reintento de cada dictado: `None` = bien, `Some(error)`. Lo lee quien hizo el
/// segundo clic mientras el primero seguía en marcha: debe ver el MISMO resultado, no un falso «actualizada»
/// (rompedor 23-oct, hub BAJA: el primer reintento fallaba y el segundo devolvía el dictado sin cambios).
static RETRY_OUTCOME: parking_lot::Mutex<Option<std::collections::HashMap<i64, Option<String>>>> = parking_lot::Mutex::new(None);

/// Marca `id` como «reintentando». `Err` = ya lo estaba (hay que esperar al otro). Si no, la marca
/// se quita sola al soltar la guarda, también por un pánico.
fn retry_claim(id: i64) -> Result<RetryGuard, ()> {
    let mut running = RETRYING.lock();
    if running.contains(&id) {
        return Err(());
    }
    running.push(id);
    if let Some(m) = RETRY_OUTCOME.lock().as_mut() {
        m.remove(&id);
    }
    Ok(RetryGuard { id, outcome: Some("El reintento se interrumpió; vuelve a intentarlo".into()) })
}

/// Espera a que acabe el reintento de `id` que ya estaba en marcha y devuelve su error, si lo tuvo.
fn retry_wait(id: i64) -> Option<String> {
    let mut running = RETRYING.lock();
    while running.contains(&id) {
        RETRY_DONE.wait(&mut running);
    }
    RETRY_OUTCOME.lock().as_ref().and_then(|m| m.get(&id).cloned()).flatten()
}

struct RetryGuard {
    id: i64,
    /// Resultado a dejar al soltarla; por defecto un error (pánico o salida sin anotar).
    outcome: Option<String>,
}

impl RetryGuard {
    fn finish(&mut self, error: Option<String>) {
        self.outcome = error;
    }
}

impl Drop for RetryGuard {
    fn drop(&mut self) {
        // El resultado se anota ANTES de quitar la marca: el que espera lo lee al despertar.
        RETRY_OUTCOME.lock().get_or_insert_with(Default::default).insert(self.id, self.outcome.take());
        RETRYING.lock().retain(|i| *i != self.id);
        RETRY_DONE.notify_all();
    }
}

/// Vuelve a transcribir el audio guardado de un dictado y actualiza su texto. No pega.
#[tauri::command]
pub async fn retry_history(st: St<'_>, id: i64) -> Result<HistoryItem, String> {
    let st = st.inner().clone();
    blocking(move || {
        let mut running = match retry_claim(id) {
            Ok(g) => g,
            Err(()) => {
                // Doble clic: ya hay un reintento de este dictado; se devuelve su resultado, también si falló.
                if let Some(e) = retry_wait(id) {
                    return Err(e);
                }
                return st.db.get_history(id).map_err(err)?.ok_or_else(|| "Ese dictado ya no existe".to_string());
            }
        };
        let result = (|| {
            let item = st.db.get_history(id).map_err(err)?.ok_or("Ese dictado ya no existe")?;
            let path = db::existing_audio_path(id, &item.created_at).ok_or("Este dictado no tiene audio guardado")?;
            let bytes = std::fs::read(&path).map_err(|_| "Este dictado no tiene audio guardado".to_string())?;
            let samples = audio::decode_wav(&bytes).ok_or("El audio guardado está dañado")?;
            let (raw, processed) = state::retry_processed(&st, &samples, &item.app, "")?;
            if processed.text.trim().is_empty() {
                return Err("No se ha oído nada".to_string());
            }
            let words = processed.text.split_whitespace().count() as i64;
            st.db.update_history_text(id, &processed.text, &raw, words).map_err(err)?;
            st.db.get_history(id).map_err(err)?.ok_or_else(|| "Ese dictado ya no existe".to_string())
        })();
        running.finish(result.as_ref().err().cloned());
        result
    })
    .await
}

/// WAV del dictado en base64 (para reproducir con un data URL), o `null` si no se guardó.
#[tauri::command]
pub async fn get_history_audio(st: St<'_>, id: i64) -> Result<Option<String>, String> {
    let st = st.inner().clone();
    blocking(move || {
        let Some(item) = st.db.get_history(id).map_err(err)? else { return Ok(None) };
        let Some(path) = db::existing_audio_path(id, &item.created_at) else { return Ok(None) };
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

/// `variants`: lo oído en las pronunciaciones grabadas antes de guardar (opcional).
/// `newSamples`: las muestras de voz pendientes de esas grabaciones (`Pronunciation.sample`).
#[tauri::command]
pub fn add_word(st: St, word: String, replacement: Option<String>, variants: Option<Vec<String>>, new_samples: Option<Vec<String>>) -> Result<Word, String> {
    if word.trim().is_empty() {
        return Err("La palabra no puede estar vacía".into());
    }
    st.db.add_word_with_samples(&word, replacement.as_deref(), &variants.unwrap_or_default(), &new_samples.unwrap_or_default()).map_err(err)
}

/// `variants: Some(lista)` deja exactamente esas variantes; sin `variants` no se tocan.
/// `keepSamples: Some(ids)` deja solo esas muestras de voz de las que ya tenía; `newSamples`
/// engancha las pendientes recién grabadas.
#[tauri::command]
pub fn update_word(
    st: St,
    id: i64,
    word: String,
    replacement: Option<String>,
    variants: Option<Vec<String>>,
    keep_samples: Option<Vec<i64>>,
    new_samples: Option<Vec<String>>,
) -> Result<(), String> {
    if word.trim().is_empty() {
        return Err("La palabra no puede estar vacía".into());
    }
    st.db
        .update_word_with_samples(id, &word, replacement.as_deref(), variants.as_deref(), keep_samples.as_deref(), &new_samples.unwrap_or_default())
        .map_err(err)
}

/// Engancha una muestra de voz pendiente (`Pronunciation.sample`) a una palabra ya guardada.
#[tauri::command]
pub fn add_word_sample(st: St, word_id: i64, sample: String) -> Result<VoiceSample, String> {
    st.db.add_word_sample(word_id, &sample).map_err(err)
}

/// Borra una muestra de voz (fila y WAV).
#[tauri::command]
pub fn delete_word_sample(st: St, id: i64) -> Result<(), String> {
    st.db.delete_word_sample(id).map_err(err)
}

/// WAV de una muestra de voz en base64 (para oírla con un data URL), o `null` si ya no está. Con
/// `pending` en vez de `id`, el de una grabación aún sin guardar.
#[tauri::command]
pub async fn get_word_sample_audio(st: St<'_>, id: Option<i64>, pending: Option<String>) -> Result<Option<String>, String> {
    let st = st.inner().clone();
    blocking(move || {
        let file = match (id, pending) {
            (Some(id), _) => st.db.word_sample_file(id).map_err(err)?,
            (None, Some(p)) if p.starts_with(db::PENDING_PREFIX) => Some(p),
            _ => None,
        };
        let Some(file) = file.filter(|f| db::valid_sample_file(f)) else { return Ok(None) };
        let Some(dir) = db::voice_dir() else { return Ok(None) };
        Ok(std::fs::read(dir.join(file)).ok().map(|b| base64::engine::general_purpose::STANDARD.encode(b)))
    })
    .await
}

/// Añade una variante (lo que oye Pronto) a una palabra ya guardada. Si ya la tenía, devuelve esa.
#[tauri::command]
pub fn add_word_variant(st: St, word_id: i64, heard: String) -> Result<WordVariant, String> {
    st.db.add_word_variant(word_id, &heard).map_err(err)
}

#[tauri::command]
pub fn delete_word_variant(st: St, id: i64) -> Result<(), String> {
    st.db.delete_word_variant(id).map_err(err)
}

/// Graba cómo se pronuncia `word` (se para sola tras la palabra o a los 4 s) y devuelve qué oye
/// Pronto. No guarda nada: la UI decide si añade la variante. Nivel y fases por
/// `pronounce://level` y `pronounce://state`.
#[tauri::command]
pub async fn pronounce_record(app: AppHandle, st: St<'_>, word: String, replacement: Option<String>) -> Result<Pronunciation, String> {
    let st = st.inner().clone();
    blocking(move || pronounce::record(&app, &st, &word, replacement.as_deref())).await
}

/// Para la grabación de pronunciación en curso; con `cancel: true` la descarta sin transcribir.
#[tauri::command]
pub fn pronounce_stop(cancel: Option<bool>) -> Result<(), String> {
    pronounce::stop(cancel.unwrap_or(false));
    Ok(())
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
/// Un modelo de voz para la pestaña «Motor» (Contrato v13).
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AsrModel {
    pub id: String,
    pub name: String,
    pub desc: String,
    pub file: String,
    pub mb: u64,
    pub present: bool,
    /// Se puede usar en este PC; si no, `reason` dice por qué.
    pub available: bool,
    pub reason: Option<String>,
    pub min_vram_gb: f32,
    /// Errores por palabra medidos (%), si se midió.
    pub wer: Option<f32>,
    /// 1 (lento) a 5 (rápido).
    pub speed: u8,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AsrOptions {
    pub gpu: Option<String>,
    pub vram_gb: f32,
    pub whisper: Vec<AsrModel>,
    /// Id del modelo de Whisper que se está descargando.
    pub downloading: Option<String>,
}

#[tauri::command]
pub fn asr_options() -> AsrOptions {
    let gpu = crate::whisper::gpu();
    let whisper = crate::whisper::MODELS
        .iter()
        .map(|m| {
            let avail = m.availability();
            AsrModel {
                id: m.id.into(),
                name: m.name.into(),
                desc: m.desc.into(),
                file: m.file.into(),
                mb: m.bytes / 1_000_000,
                present: m.present(),
                available: avail.is_ok(),
                reason: avail.err(),
                min_vram_gb: m.min_vram_gb,
                wer: m.wer,
                speed: m.speed,
            }
        })
        .collect();
    AsrOptions { gpu: gpu.as_ref().map(|g| g.0.clone()), vram_gb: gpu.map_or(0.0, |g| (g.1 as f64 / 1e9) as f32), whisper, downloading: modeldl::whisper_downloading() }
}

#[tauri::command]
pub fn whisper_download_start(app: AppHandle, st: St, id: String) -> Result<(), String> {
    modeldl::whisper_start(&app, &st, &id)
}

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

/// Candado de la Flow bar: `true` la fija en el monitor donde está ahora; `false` la suelta y vuelve
/// a seguir al cursor. Devuelve los ajustes guardados (también llegan por `settings://changed`).
#[tauri::command]
pub fn pin_flowbar(app: AppHandle, st: St, pinned: bool) -> Result<Settings, String> {
    let mut new = st.settings.read().clone();
    new.flow_bar_monitor = if pinned {
        Some(flowbar::current_monitor(&app).ok_or("No se encontró el monitor de la barra")?.0)
    } else {
        None
    };
    apply_settings(&app, st.inner(), new)?;
    Ok(st.settings.read().clone())
}

/// Monitores conectados, para elegir en Ajustes dónde se fija la Flow bar.
#[tauri::command]
pub fn list_monitors(app: AppHandle) -> Result<Vec<MonitorInfo>, String> {
    Ok(flowbar::list_monitors(&app))
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
    if st.meeting.is_processing(id) {
        return Err("Se están generando las notas de esta reunión: espera a que terminen para eliminarla".into());
    }
    let m = st.db.get_meeting(id).map_err(err)?;
    st.db.delete_meeting(id).map_err(err)?;
    if let Some(m) = m {
        meeting::remove_audio_files(&m);
    }
    Ok(())
}

/// Reintenta o regenera las notas (fase 2 en segundo plano; avisa con `meeting://progress`,
/// `meeting://state` y `meeting://summary`).
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

// ------------------------------------------------------------ registro de errores

/// Abre en el Explorador la carpeta del registro local de errores (`%APPDATA%\Pronto2\logs`). El
/// registro no sale del PC: es el usuario quien decide si le manda el fichero a alguien (decisión de
/// Txonso, 23-oct-2026).
/// Versión de Pronto y si es la versión de pruebas (`PRONTO_DATOS`): el hub las enseña en la barra
/// lateral para distinguir la de pruebas de la instalada (petición de Txonso, 23-oct).
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    version: &'static str,
    test: bool,
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo { version: env!("CARGO_PKG_VERSION"), test: crate::db::test_build() }
}

#[tauri::command]
pub fn open_error_log() -> Result<(), String> {
    let dir = errlog::ensure_dir()?;
    open_folder(&dir)
}

/// Abre una carpeta en el Explorador de Windows. `explorer.exe` devuelve 1 aunque abra bien, así que
/// solo se mira que haya podido lanzarse.
fn open_folder(dir: &std::path::Path) -> Result<(), String> {
    std::process::Command::new("explorer.exe").arg(dir).spawn().map(|_| ()).map_err(|e| {
        errlog::error("settings", format!("no se pudo abrir la carpeta del registro: {e}"));
        format!("No se pudo abrir la carpeta: {e}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rompedor 22-oct-c, carga MEDIA 1: el doble clic en «Reintentar» no encola dos transcripciones.
    #[test]
    fn un_segundo_reintento_del_mismo_dictado_espera_al_primero() {
        let id = 987_654_321; // fuera del rango de cualquier otra prueba
        let first = retry_claim(id).expect("el primero lo coge");
        assert!(retry_claim(id).is_err(), "el segundo del mismo dictado no puede empezar");
        let other = retry_claim(id + 1).expect("otro dictado sí puede reintentarse a la vez");
        drop(other);

        let (tx, rx) = std::sync::mpsc::channel();
        let waiter = std::thread::spawn(move || {
            retry_wait(id);
            tx.send(()).unwrap();
        });
        assert!(rx.recv_timeout(Duration::from_millis(200)).is_err(), "el segundo esperó");
        drop(first);
        rx.recv_timeout(Duration::from_secs(5)).expect("el segundo no despertó al acabar el primero");
        waiter.join().unwrap();
        assert!(retry_claim(id).is_ok(), "al acabar, el dictado se puede reintentar otra vez");
    }

    #[test]
    fn un_reintento_que_entra_en_panico_no_deja_la_marca_puesta() {
        let id = 987_654_330;
        let r = std::panic::catch_unwind(|| {
            let _g = retry_claim(id).unwrap();
            panic!("fallo simulado");
        });
        assert!(r.is_err());
        assert!(retry_claim(id).is_ok());
    }

    /// Rompedor 23-oct, hub BAJA: si el primer reintento falla, el segundo clic ve el MISMO error.
    #[test]
    fn el_segundo_reintento_ve_el_error_del_primero() {
        let id = 987_654_340;
        let mut first = retry_claim(id).unwrap();
        let waiter = std::thread::spawn(move || retry_wait(id));
        std::thread::sleep(Duration::from_millis(100));
        first.finish(Some("No se ha oído nada".into()));
        drop(first);
        assert_eq!(waiter.join().unwrap().as_deref(), Some("No se ha oído nada"));
        // Y si el primero salió bien, el segundo no ve error.
        let mut ok = retry_claim(id).unwrap();
        ok.finish(None);
        drop(ok);
        assert_eq!(retry_wait(id), None);
        // Un pánico del primero no se confunde con un éxito.
        let id2 = id + 1;
        let _ = std::panic::catch_unwind(|| {
            let _g = retry_claim(id2).unwrap();
            panic!("fallo simulado");
        });
        assert!(retry_wait(id2).is_some());
    }

    /// Decisión de Txonso, 23-oct-2026: Ollama solo en este PC. Esto es lo que decide `apply_settings`
    /// (que necesita un `AppHandle`) sobre la dirección: lo que no es de este PC se rechaza y no se guarda.
    #[test]
    fn guardar_ajustes_rechaza_una_ollama_que_no_es_de_este_pc() {
        let guardada = "http://localhost:11434";
        for u in ["http://192.168.1.50:11434", "ollama.example.com", "http://0.0.0.0:11434", "http://evil.example\\@localhost:11434", "https://10.0.0.5"] {
            let e = resolve_ollama_url(u, guardada).expect_err(u);
            assert!(e.starts_with("Pronto solo usa Ollama en este PC (localhost o 127.0.0.1)"), "{u}: {e}");
        }
        assert!(resolve_ollama_url("no es una url", guardada).is_err());
        // Las de este PC pasan (y se normalizan); la vacía vuelve a la de fábrica.
        assert_eq!(resolve_ollama_url("127.0.0.1:11434", guardada).as_deref(), Ok("http://127.0.0.1:11434"));
        assert_eq!(resolve_ollama_url("http://[::1]:11434", guardada).as_deref(), Ok("http://[::1]:11434"));
        assert_eq!(resolve_ollama_url("  ", "http://127.0.0.1:1"), Ok(Settings::default().ollama_url));
        // La guardada no se vuelve a comprobar: un ajuste ajeno (el micrófono del menú) no falla por ella.
        assert_eq!(resolve_ollama_url("http://192.168.1.50:11434", "http://192.168.1.50:11434").as_deref(), Ok("http://192.168.1.50:11434"));
    }

    #[test]
    fn el_si_a_las_reuniones_automaticas_no_se_deshace_al_guardar_ajustes() {
        use MeetingAuto::*;
        // Ya confirmado: una UI que manda `false` no lo borra.
        assert!(meeting_confirmed(true, Auto, Auto, false));
        assert!(meeting_confirmed(true, Ask, Off, false));
        // Sin confirmar y sin tocar el modo: sigue sin confirmar (la pregunta de la primera vez).
        assert!(!meeting_confirmed(false, Auto, Auto, false));
        assert!(!meeting_confirmed(false, Ask, Ask, false));
        assert!(!meeting_confirmed(false, Auto, Off, false));
        // Elegir «Automático» a mano desde otro modo ya es haberlo dicho.
        assert!(meeting_confirmed(false, Ask, Auto, false));
        assert!(meeting_confirmed(false, Off, Auto, false));
        // El backend que guarda el sí lo manda a true.
        assert!(meeting_confirmed(false, Auto, Auto, true));
    }

    #[test]
    fn la_carpeta_del_registro_se_crea_y_no_es_la_del_usuario_en_las_pruebas() {
        let d = errlog::ensure_dir().unwrap();
        assert!(d.is_dir());
        assert!(d.to_string_lossy().contains("pronto2-pruebas-logs"), "{}", d.display());
    }
}
