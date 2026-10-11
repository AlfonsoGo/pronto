//! Estado de la app y máquina de estados del dictado.
//!
//! A prueba de carreras por construcción: **un único hilo (el controlador)** toma todas las
//! decisiones. Hotkeys, botones de la Flow bar y fin de transcripción llegan como mensajes por
//! canal y se procesan de uno en uno, así que nunca hay dos dictados a la vez ni dos transiciones
//! simultáneas. La transcripción y los transforms corren en otro hilo que avisa con
//! `Cmd::Finished`. Los plazos (debounce de 500 ms, aviso a los 19 min, tope de 20) los gestiona
//! el propio bucle del controlador con un temporizador, sin hilos de espera.

use crate::asr::Engine;
use crate::audio::{self, LevelFn, Recorder, TARGET_RATE};
use crate::db::{self, Db, NewHistory};
use crate::hotkey::{self, HotkeyEvent, TimedEvent};
use crate::inject::{self, AppInfo};
use crate::models::{DictationState, Fixes, HistoryItem, Notice, Settings, Tone};
use crate::text::{self, Category};
use crate::meeting::MeetingManager;
use crate::{flowbar, media, modeldl, ollama, polish};
use crossbeam_channel::{after, never, select, Receiver, Sender};
use parking_lot::RwLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

/// Grabaciones más cortas se ignoran (toque accidental).
const MIN_RECORDING_MS: i64 = 300;
/// `DICTATION_DEBOUNCE` de Wispr: una pulsación más corta es un toque y espera un segundo toque.
const DEBOUNCE: Duration = Duration::from_millis(500);
/// Duración máxima de un dictado y aviso previo.
const MAX_RECORDING: Duration = Duration::from_secs(20 * 60);
const WARN_AT: Duration = Duration::from_secs(19 * 60);
/// Pausa antes de silenciar para que el pitido de inicio se oiga.
const MUTE_DELAY: Duration = Duration::from_millis(180);
/// Cuánto se ve «Hecho»/«Cancelado» antes de volver a reposo (y ocultar la barra si procede).
const SETTLE_DONE: Duration = Duration::from_millis(1500);
const SETTLE_ERROR: Duration = Duration::from_millis(3000);
/// Un transform admite hasta 1000 palabras, como Wispr.
const MAX_TRANSFORM_WORDS: usize = 1000;
/// Por debajo de este pico el micrófono está mudo (silenciado o desconectado).
const SILENCE_PEAK: f32 = 0.0005;

pub struct AppState {
    pub settings: RwLock<Settings>,
    pub db: Arc<Db>,
    pub engine: Arc<Engine>,
    pub tx: Sender<Cmd>,
    /// Se incrementa al empezar cada dictado; invalida los temporizadores de la UI anteriores.
    ui_epoch: AtomicU64,
    /// Hay un dictado o un transform en curso.
    pub active: AtomicBool,
    /// Notetaker: reunión en curso (si la hay).
    pub meeting: MeetingManager,
}

impl AppState {
    pub fn new(settings: Settings, db: Arc<Db>, engine: Arc<Engine>, tx: Sender<Cmd>) -> Self {
        Self { settings: RwLock::new(settings), db, engine, tx, ui_epoch: AtomicU64::new(0), active: AtomicBool::new(false), meeting: MeetingManager::default() }
    }

    pub fn send(&self, cmd: Cmd) {
        if self.tx.send(cmd).is_err() {
            eprintln!("[state] el controlador ya no está activo");
        }
    }
}

/// Órdenes al controlador.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmd {
    UiStart { hands_free: bool },
    UiStop,
    UiCancel,
    RunTransform { id: i64 },
    /// Aviso del hilo de trabajo: el dictado o transform terminó (bien o mal).
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    Listening { hands_free: bool },
    Processing,
    Polishing,
}

pub fn emit_state(app: &AppHandle, state: &'static str, message: Option<String>) {
    emit_state_kind(app, state, message, None);
}

pub fn emit_state_kind(app: &AppHandle, state: &'static str, message: Option<String>, kind: Option<&'static str>) {
    let _ = app.emit("dictation://state", DictationState { state, message, kind });
}

/// Estado de reposo de la Flow bar: `meeting` mientras se toman notas, `idle` si no.
pub fn emit_idle(app: &AppHandle, st: &AppState) {
    emit_state(app, if st.meeting.is_active() { "meeting" } else { "idle" }, None);
}

pub fn emit_notice(app: &AppHandle, title: &str, body: Option<&str>) {
    let _ = app.emit("dictation://notice", Notice { title: title.into(), body: body.map(String::from), action: None });
}

/// Pega la última transcripción tras `delay` (tiempo para que el foco vuelva a la app destino).
pub fn paste_last(st: &Arc<AppState>, delay: Duration) -> Result<(), String> {
    let text = st.db.last_history_text().ok_or_else(|| "Aún no hay ninguna transcripción".to_string())?;
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        if let Err(e) = inject::paste_text(&text) {
            eprintln!("[state] pegar última: {e}");
        }
    });
    Ok(())
}

/// Copia la última transcripción al portapapeles y lo avisa en la Flow bar.
pub fn copy_last(app: &AppHandle, st: &Arc<AppState>) -> Result<(), String> {
    let text = st.db.last_history_text().ok_or_else(|| "Aún no hay ninguna transcripción".to_string())?;
    inject::copy_text(&text)?;
    emit_notice(app, "Copiado", None);
    Ok(())
}

/// Coloca y muestra u oculta la Flow bar según los ajustes, salvo que haya un dictado en curso.
pub fn refresh_flowbar(app: &AppHandle, st: &Arc<AppState>) {
    if st.active.load(Ordering::SeqCst) {
        return;
    }
    if flowbar::rest_visible(&st.settings.read()) {
        flowbar::place(app, flowbar::Anchor::Cursor);
        flowbar::show(app);
    } else {
        flowbar::hide(app);
    }
}

/// «Ocultar 1 hora»: la barra desaparece en reposo (sigue saliendo al dictar) y vuelve sola.
pub fn hide_flowbar_for_hour(app: &AppHandle, st: &Arc<AppState>) -> Result<(), String> {
    let until = chrono::Local::now() + chrono::Duration::minutes(60);
    let new = {
        let mut s = st.settings.write();
        s.flow_bar_hidden_until = Some(until.to_rfc3339());
        s.clone()
    };
    st.db.save_settings(&new).map_err(|e| e.to_string())?;
    let _ = app.emit("settings://changed", new);
    refresh_flowbar(app, st);
    let (app, st) = (app.clone(), st.clone());
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(60 * 60 + 1));
        refresh_flowbar(&app, &st);
    });
    Ok(())
}

pub fn spawn_controller(app: AppHandle, st: Arc<AppState>, hk_rx: Receiver<TimedEvent>, cmd_rx: Receiver<Cmd>) {
    let spawned = std::thread::Builder::new().name("pronto-ctl".into()).spawn(move || {
        let mut ctl = Controller { app, st, phase: Phase::Idle, rec: None, started: Instant::now(), hold_down_at: None, pending_tap: None, warned: false };
        loop {
            // Un único temporizador para el plazo más próximo (segundo toque, aviso, tope).
            let timer = match ctl.next_deadline() {
                Some(t) => after(t.saturating_duration_since(Instant::now())),
                None => never(),
            };
            select! {
                recv(hk_rx) -> m => match m {
                    Ok(ev) => ctl.on_hotkey(ev),
                    Err(_) => return,
                },
                recv(cmd_rx) -> m => match m {
                    Ok(cmd) => ctl.on_cmd(cmd),
                    Err(_) => return,
                },
                recv(timer) -> _ => ctl.on_timer(),
            }
        }
    });
    if let Err(e) = spawned {
        eprintln!("[state] no se pudo lanzar el controlador: {e}");
    }
}

struct Controller {
    app: AppHandle,
    st: Arc<AppState>,
    phase: Phase,
    rec: Option<Recorder>,
    started: Instant,
    /// Momento (del hook) en que se pulsó el atajo «mantener» que sigue pulsado.
    hold_down_at: Option<Instant>,
    /// Plazo para un segundo toque, tras un toque suelto.
    pending_tap: Option<Instant>,
    warned: bool,
}

impl Controller {
    fn next_deadline(&self) -> Option<Instant> {
        if !matches!(self.phase, Phase::Listening { .. }) {
            return None;
        }
        let limit = if self.warned { self.started + MAX_RECORDING } else { self.started + WARN_AT };
        Some(self.pending_tap.map_or(limit, |t| t.min(limit)))
    }

    fn on_timer(&mut self) {
        let now = Instant::now();
        if !matches!(self.phase, Phase::Listening { .. }) {
            return;
        }
        if self.pending_tap.is_some_and(|t| now >= t) {
            self.discard_tap();
        } else if now >= self.started + MAX_RECORDING {
            self.stop();
        } else if !self.warned && now >= self.started + WARN_AT {
            self.warned = true;
            let hands_free = self.phase == Phase::Listening { hands_free: true };
            let msg = "Queda 1 minuto: la sesión termina a los 20 minutos".to_string();
            emit_state(&self.app, if hands_free { "handsfree" } else { "listening" }, Some(msg.clone()));
            emit_notice(&self.app, "Sesión de dictado casi al límite", Some(&msg));
        }
    }

    fn on_hotkey(&mut self, t: TimedEvent) {
        match (t.ev, self.phase) {
            (HotkeyEvent::HoldDown, Phase::Idle) => self.start(false, Some(t.at)),
            // Segundo toque dentro del plazo: se bloquea el modo manos libres sin perder lo grabado.
            (HotkeyEvent::HoldDown, Phase::Listening { hands_free: false }) if self.pending_tap.is_some() => self.lock_hands_free(),
            // En manos libres, el atajo «mantener» termina y pega.
            (HotkeyEvent::HoldDown, Phase::Listening { hands_free: true }) => self.stop(),
            (HotkeyEvent::HoldUp, Phase::Listening { hands_free: false }) => self.on_hold_up(t.at),
            // Manos libres: arranca, o fija una grabación «mantener» ya empezada, o la para.
            (HotkeyEvent::HandsFree, Phase::Idle) => self.start(true, None),
            (HotkeyEvent::HandsFree, Phase::Listening { hands_free: false }) => self.lock_hands_free(),
            (HotkeyEvent::HandsFree, Phase::Listening { hands_free: true }) => self.stop(),
            (HotkeyEvent::Cancel, Phase::Listening { .. }) => self.cancel(),
            (HotkeyEvent::PasteLast, Phase::Idle) => {
                if let Err(e) = paste_last(&self.st, Duration::ZERO) {
                    eprintln!("[state] {e}");
                }
            }
            // Win+Alt+M: si el atajo «mantener» (Win+Alt) ya había empezado a grabar, ese dictado
            // se descarta en silencio y se alterna la reunión.
            (HotkeyEvent::Meeting, phase) => {
                if matches!(phase, Phase::Listening { .. }) {
                    drop(self.finish_recording());
                    self.phase = Phase::Idle;
                    self.st.active.store(false, Ordering::SeqCst);
                    emit_idle(&self.app, &self.st);
                }
                if matches!(phase, Phase::Idle | Phase::Listening { .. }) {
                    crate::meeting::toggle(&self.app, &self.st);
                }
            }
            (HotkeyEvent::CopyLast, Phase::Idle) => {
                if let Err(e) = copy_last(&self.app, &self.st) {
                    eprintln!("[state] {e}");
                }
            }
            // Win+Alt+1: igual que la reunión, `Win+Alt` ya arrancó un dictado «mantener» al
            // pulsarse; ese dictado se descarta en silencio y se lanza el Transform. Sin esto el
            // Transform llegaba en `Listening` y se ignoraba (medido: el texto no cambiaba).
            (HotkeyEvent::Transform(n), phase @ (Phase::Idle | Phase::Listening { hands_free: false })) => {
                if matches!(phase, Phase::Listening { .. }) {
                    drop(self.finish_recording());
                    self.phase = Phase::Idle;
                    self.st.active.store(false, Ordering::SeqCst);
                    emit_idle(&self.app, &self.st);
                }
                match self.st.db.transform_by_hotkey(n as i64) {
                    Ok(Some(tr)) => self.start_transform(tr.id),
                    Ok(None) => eprintln!("[state] no hay ningún transform con el atajo {n}"),
                    Err(e) => eprintln!("[state] transforms: {e}"),
                }
            }
            // Cualquier otra combinación (p. ej. HoldDown mientras se procesa) se ignora.
            _ => {}
        }
    }

    fn on_cmd(&mut self, cmd: Cmd) {
        match (cmd, self.phase) {
            (Cmd::UiStart { hands_free }, Phase::Idle) => self.start(hands_free, None),
            (Cmd::UiStop, Phase::Listening { .. }) => self.stop(),
            (Cmd::UiCancel, Phase::Listening { .. }) => self.cancel(),
            (Cmd::RunTransform { id }, Phase::Idle) => self.start_transform(id),
            (Cmd::Finished, _) => {
                self.phase = Phase::Idle;
                self.st.active.store(false, Ordering::SeqCst);
            }
            _ => {}
        }
    }

    /// Soltado del atajo «mantener»: ≥ 500 ms termina y pega; menos es un toque y se espera otro.
    fn on_hold_up(&mut self, at: Instant) {
        let held = self.hold_down_at.take().map(|d| at.saturating_duration_since(d));
        if held.is_none_or(|h| h >= DEBOUNCE) {
            self.stop();
        } else {
            self.pending_tap = Some(at + DEBOUNCE);
        }
    }

    fn discard_tap(&mut self) {
        drop(self.finish_recording());
        self.phase = Phase::Idle;
        self.st.active.store(false, Ordering::SeqCst);
        let hold = self.st.settings.read().hotkey_hold.describe();
        self.end_ui("error", Some(format!("No lo toques. Mantén pulsado {hold}.")), Some("tap"), SETTLE_ERROR);
    }

    fn lock_hands_free(&mut self) {
        self.pending_tap = None;
        self.hold_down_at = None;
        self.phase = Phase::Listening { hands_free: true };
        emit_state(&self.app, "handsfree", None);
        if self.st.settings.read().sounds {
            media::play_lock();
        }
    }

    fn start(&mut self, hands_free: bool, down_at: Option<Instant>) {
        let settings = self.st.settings.read().clone();
        let engine = self.st.engine.status();
        if !engine.ready && !engine.loading && !modeldl::model_present(&settings.model_dir) {
            flowbar::place(&self.app, flowbar::Anchor::Cursor);
            flowbar::show(&self.app);
            let _ = self.app.emit("dictation://notice", Notice { title: "Falta el modelo de voz".into(), body: Some("Ábrelo en Pronto".into()), action: Some("open_hub".into()) });
            return;
        }
        self.st.ui_epoch.fetch_add(1, Ordering::SeqCst);

        let app = self.app.clone();
        let level: LevelFn = Arc::new(move |l| {
            let _ = app.emit("dictation://level", serde_json::json!({ "level": l }));
        });
        let rec = match Recorder::start(settings.mic_name.clone(), level) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[state] {e}");
                flowbar::place(&self.app, flowbar::Anchor::Cursor);
                flowbar::show(&self.app);
                self.end_ui("error", Some(e), Some("nomic"), SETTLE_ERROR);
                return;
            }
        };
        self.rec = Some(rec);
        self.started = down_at.unwrap_or_else(Instant::now);
        self.hold_down_at = if hands_free { None } else { down_at };
        self.pending_tap = None;
        self.warned = false;
        self.phase = Phase::Listening { hands_free };
        self.st.active.store(true, Ordering::SeqCst);
        hotkey::set_recording(true);

        flowbar::place(&self.app, flowbar::Anchor::Cursor);
        flowbar::show(&self.app);
        emit_state(&self.app, if hands_free { "handsfree" } else { "listening" }, None);
        if settings.sounds {
            media::play_start();
        }
        // Durante una reunión no se silencia el audio: el loopback ("Ellos") dejaría de oírse.
        if settings.mute_music && !self.st.meeting.is_active() {
            media::mute_after(MUTE_DELAY);
        }
    }

    /// Cierra la grabación y devuelve el audio (o nada si no había).
    fn finish_recording(&mut self) -> Vec<f32> {
        hotkey::set_recording(false);
        self.pending_tap = None;
        self.hold_down_at = None;
        let samples = self.rec.take().map(|r| r.stop()).unwrap_or_default();
        media::restore();
        fake_wav().unwrap_or(samples)
    }

    fn stop(&mut self) {
        let samples = self.finish_recording();
        let settings = self.st.settings.read().clone();
        if settings.sounds {
            media::play_stop();
        }
        let duration_ms = samples.len() as i64 * 1000 / TARGET_RATE as i64;
        if duration_ms < MIN_RECORDING_MS {
            // Toque accidental: vuelta a reposo sin ruido.
            self.phase = Phase::Idle;
            self.st.active.store(false, Ordering::SeqCst);
            emit_idle(&self.app, &self.st);
            if !flowbar::rest_visible(&settings) {
                flowbar::hide(&self.app);
            }
            return;
        }
        let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        eprintln!("[state] grabación: {duration_ms} ms, pico {peak:.4}");
        if peak < SILENCE_PEAK {
            self.phase = Phase::Idle;
            self.st.active.store(false, Ordering::SeqCst);
            self.end_ui("error", Some("¿Tienes el micrófono silenciado?".into()), Some("noaudio"), SETTLE_ERROR);
            return;
        }
        // La app destino se mide ahora, antes de pegar: la Flow bar no roba el foco, así que es la
        // misma que tenía el usuario al soltar el atajo.
        let target = inject::foreground_app();
        self.phase = Phase::Processing;
        emit_state(&self.app, "processing", None);

        let app = self.app.clone();
        let st = self.st.clone();
        let spawned = std::thread::Builder::new().name("pronto-transcribe".into()).spawn(move || {
            let outcome = transcribe_and_paste(&app, &st, &samples, duration_ms, &target);
            let epoch = st.ui_epoch.load(Ordering::SeqCst);
            match outcome {
                Ok(()) => end_ui(&app, &st, epoch, "done", None, None, SETTLE_DONE),
                Err(f) => end_ui(&app, &st, epoch, "error", Some(f.message), f.kind, SETTLE_ERROR),
            }
            st.send(Cmd::Finished);
        });
        if let Err(e) = spawned {
            eprintln!("[state] no se pudo lanzar la transcripción: {e}");
            self.phase = Phase::Idle;
            self.st.active.store(false, Ordering::SeqCst);
            self.end_ui("error", Some("No se pudo transcribir".into()), None, SETTLE_ERROR);
        }
    }

    fn cancel(&mut self) {
        drop(self.finish_recording());
        self.phase = Phase::Idle;
        self.st.active.store(false, Ordering::SeqCst);
        self.end_ui("cancelled", None, None, SETTLE_DONE);
    }

    fn start_transform(&mut self, id: i64) {
        self.st.ui_epoch.fetch_add(1, Ordering::SeqCst);
        self.phase = Phase::Polishing;
        self.st.active.store(true, Ordering::SeqCst);
        flowbar::place(&self.app, flowbar::Anchor::Cursor);
        flowbar::show(&self.app);
        emit_state(&self.app, "polishing", None);

        let app = self.app.clone();
        let st = self.st.clone();
        let spawned = std::thread::Builder::new().name("pronto-transform".into()).spawn(move || {
            let outcome = run_transform_job(&app, &st, id);
            let epoch = st.ui_epoch.load(Ordering::SeqCst);
            match outcome {
                Ok(()) => end_ui(&app, &st, epoch, "polished", None, None, SETTLE_DONE),
                Err(f) => end_ui(&app, &st, epoch, "error", Some(f.message), f.kind, SETTLE_ERROR),
            }
            st.send(Cmd::Finished);
        });
        if let Err(e) = spawned {
            eprintln!("[state] no se pudo lanzar el transform: {e}");
            self.phase = Phase::Idle;
            self.st.active.store(false, Ordering::SeqCst);
            self.end_ui("error", Some("No se pudo ejecutar el transform".into()), None, SETTLE_ERROR);
        }
    }

    fn end_ui(&self, state: &'static str, message: Option<String>, kind: Option<&'static str>, settle: Duration) {
        let epoch = self.st.ui_epoch.load(Ordering::SeqCst);
        end_ui(&self.app, &self.st, epoch, state, message, kind, settle);
    }
}

/// Muestra un estado final y, pasado `settle`, vuelve a `idle` (ocultando la Flow bar si no es
/// visible en reposo) siempre que no haya empezado otro dictado entretanto.
fn end_ui(app: &AppHandle, st: &Arc<AppState>, epoch: u64, state: &'static str, message: Option<String>, kind: Option<&'static str>, settle: Duration) {
    emit_state_kind(app, state, message, kind);
    let app = app.clone();
    let st = st.clone();
    std::thread::spawn(move || {
        std::thread::sleep(settle);
        if st.ui_epoch.load(Ordering::SeqCst) != epoch || st.active.load(Ordering::SeqCst) {
            return;
        }
        emit_idle(&app, &st);
        if !flowbar::rest_visible(&st.settings.read()) {
            flowbar::hide(&app);
        }
    });
}

fn category_tone(settings: &Settings, cat: Category) -> Tone {
    match cat {
        Category::Personal => settings.styles.personal,
        Category::Work => settings.styles.work,
        Category::Email => settings.styles.email,
        Category::Other => settings.styles.other,
    }
}

/// Error mostrable en la Flow bar, con su `kind` opcional.
pub struct Failure {
    pub message: String,
    pub kind: Option<&'static str>,
}

impl Failure {
    fn new(message: impl Into<String>, kind: Option<&'static str>) -> Self {
        Self { message: message.into(), kind }
    }
}

impl From<String> for Failure {
    fn from(message: String) -> Self {
        Self { message, kind: None }
    }
}

/// Texto crudo -> tubería -> texto final, usando ajustes y diccionario actuales.
pub fn run_pipeline(st: &Arc<AppState>, raw: &str, app_name: &str, title: &str) -> text::Processed {
    let settings = st.settings.read().clone();
    let words = st.db.list_words().unwrap_or_else(|e| {
        eprintln!("[state] diccionario: {e}");
        Vec::new()
    });
    let snippets = st.db.list_snippets().unwrap_or_else(|e| {
        eprintln!("[state] snippets: {e}");
        Vec::new()
    });
    let options = text::Options {
        cleanup: settings.cleanup_level,
        press_enter: settings.press_enter_command,
        tone: category_tone(&settings, text::category_for_app(app_name, title)),
        words: &words,
        snippets: &snippets,
    };
    text::process(raw, &options)
}

/// Audio -> texto -> historial (+ WAV) -> pegado. Devuelve el error para la Flow bar.
fn transcribe_and_paste(app: &AppHandle, st: &Arc<AppState>, samples: &[f32], duration_ms: i64, target: &AppInfo) -> Result<(), Failure> {
    let raw = st.engine.transcribe(samples)?;
    if raw.trim().is_empty() {
        return Err(Failure::new("No se ha oído nada", Some("noaudio")));
    }
    let processed = run_pipeline(st, &raw, &target.exe, &target.title);
    if processed.text.trim().is_empty() && !processed.press_enter {
        return Err(Failure::new("No se ha oído nada", Some("noaudio")));
    }
    // «Pulir con IA siempre»: tras la tubería local y antes de guardar/pegar, con plazo duro.
    let settings = st.settings.read().clone();
    let tone = category_tone(&settings, text::category_for_app(&target.exe, &target.title));
    let polish = polish::apply(&settings, &processed.text, tone, processed.fixes_snippets);
    let final_text = polish.text.clone();
    let word_count = final_text.split_whitespace().count() as i64;
    // Se guarda antes de pegar: aunque el pegado falle, la transcripción no se pierde. Si hubo
    // pulido, `raw_text` es el texto local previo (lo que permite deshacerlo).
    let saved = st.db.add_history(&NewHistory {
        polished: polish.polished,
        text: &final_text,
        raw: if polish.polished { &processed.text } else { &raw },
        app: &target.display_name(),
        duration_ms,
        words: word_count,
        fixes: Fixes {
            dictionary: processed.fixes_dictionary as i64,
            snippets: processed.fixes_snippets as i64,
            fillers: processed.fixes_fillers as i64,
        },
        transform: None,
    });
    match saved {
        Ok(mut item) => {
            item.has_audio = save_audio(st, item.id, samples);
            let _ = app.emit("dictation://result", item);
        }
        Err(e) => eprintln!("[state] no se pudo guardar el historial: {e}"),
    }
    if !final_text.trim().is_empty() {
        inject::paste_text(&final_text).map_err(|e| Failure::new(format!("Copiado, pero no se pudo pegar: {e}"), Some("paste")))?;
    }
    if polish.failed {
        // Aviso suave, ya con el texto pegado.
        emit_notice(app, "Pegado sin pulir: Ollama no respondió", None);
    }
    if processed.press_enter {
        // Margen para que la app procese el pegado antes del Enter.
        std::thread::sleep(Duration::from_millis(150));
        inject::press_enter();
    }
    Ok(())
}

/// Guarda el WAV del dictado si `keepAudioDays > 0`. Devuelve si quedó guardado.
fn save_audio(st: &Arc<AppState>, id: i64, samples: &[f32]) -> bool {
    let days = st.settings.read().keep_audio_days;
    if days == 0 {
        return false;
    }
    let Some(path) = db::audio_path(id) else { return false };
    if let Err(e) = std::fs::write(&path, audio::encode_wav(samples)) {
        eprintln!("[state] no se pudo guardar el audio: {e}");
        return false;
    }
    if let Err(e) = st.db.set_has_audio(id, true) {
        eprintln!("[state] audio: {e}");
    }
    if let Err(e) = st.db.purge_audio(days) {
        eprintln!("[state] purga de audio: {e}");
    }
    true
}

/// Transform sobre la selección actual: Ctrl+C -> Ollama -> pegar el resultado.
fn run_transform_job(app: &AppHandle, st: &Arc<AppState>, id: i64) -> Result<(), Failure> {
    let transform = st
        .db
        .get_transform(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| Failure::new("Ese transform ya no existe", None))?;
    let (url, model) = {
        let s = st.settings.read();
        (s.ollama_url.clone(), s.ollama_model.clone())
    };
    let target = inject::foreground_app();
    let selected = inject::capture_selection().map_err(|e| Failure::new(e, Some("select")))?;
    if selected.split_whitespace().count() > MAX_TRANSFORM_WORDS {
        inject::restore_pending();
        return Err(Failure::new("Texto demasiado largo para transformar (máx. 1000 palabras)", Some("select")));
    }
    let result = match ollama::generate(&url, &model, &transform.prompt, &selected) {
        Ok(r) => r,
        Err(e) => {
            inject::restore_pending();
            return Err(Failure::new(e, Some("ollama")));
        }
    };
    // Ya hay resultado: un fallo al guardar el historial no debe impedir pegarlo.
    let words = result.split_whitespace().count() as i64;
    let saved = st.db.add_history(&NewHistory {
        polished: false,
        text: &result,
        raw: &selected,
        app: &target.display_name(),
        duration_ms: 0,
        words,
        fixes: Fixes::default(),
        transform: Some(&transform.name),
    });
    match saved {
        Ok(item) => {
            let item: HistoryItem = item;
            let _ = app.emit("dictation://result", item);
        }
        Err(e) => eprintln!("[state] no se pudo guardar el transform: {e}"),
    }
    inject::paste_text(&result).map_err(|e| Failure::new(format!("Copiado, pero no se pudo pegar: {e}"), Some("paste")))
}

/// Solo en depuración: si `PRONTO_FAKE_WAV` apunta a un WAV PCM16 mono a 16 kHz, el dictado usa
/// ese audio en vez del micrófono. Sirve para probar de punta a punta (atajo → texto → pegado)
/// sin hablar. En release no existe.
fn fake_wav() -> Option<Vec<f32>> {
    if !cfg!(debug_assertions) {
        return None;
    }
    let path = std::env::var_os("PRONTO_FAKE_WAV")?;
    let bytes = std::fs::read(&path).ok()?;
    let samples = crate::audio::decode_wav(&bytes)?;
    eprintln!("[state] PRONTO_FAKE_WAV: {} muestras de {}", samples.len(), path.to_string_lossy());
    Some(samples)
}
