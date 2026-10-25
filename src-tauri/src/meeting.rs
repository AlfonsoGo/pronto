//! Meets: notas de reunión en dos fases (Contrato v12).
//!
//! Fase 1 · captura (durante la reunión): un hilo por pista (micrófono = «Tú», audio del sistema
//! por loopback = «Ellos»; el `Stream` de cpal no es `Send`) que SOLO escribe el audio a un WAV
//! 16 kHz mono (`meeting_{id}_{me|them}.wav`), alineado al inicio de la reunión y con la cabecera
//! al día cada 5 s. No se transcribe nada mientras se habla; otro hilo emite el nivel. Si una pista
//! no se puede grabar, se avisa y se sigue con la otra.
//!
//! Fase 2 · comprensión (al parar, hilo `pronto-notes`): transcribe cada pista con tiempos por
//! palabra, quita el eco palabra a palabra, encadena los turnos por tiempo real, pule con Ollama
//! los turnos defectuosos, etiqueta cada turno y saca las notas en dos pasadas (ver `notes.rs`).
//! Avisa del avance con `meeting://progress`. «Reintentar»/«Regenerar» repite la fase 2:
//! transcribe el audio si aún no hay transcripción final y, si la hay, el resto.

use crate::audio::{self, OpenSource, SourceKind, WavStream, TARGET_RATE};
use crate::db;
use crate::models::*;
use crate::state::{self, AppState};
use crate::summary;
use crate::text::{self, Options};
use crate::{auto, detect, errlog, notes, ollama};
use crossbeam_channel::{unbounded, RecvTimeoutError, Sender};
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

/// Contexto de Ollama que pedimos para las notas (tokens).
const NUM_CTX: u32 = 8192;
/// Tope de tokens de la respuesta de etiquetas: 40 turnos × ~7 tokens en el formato compacto; el resto
/// es holgura para que el JSON de respaldo, si el modelo lo prefiere, también quepa.
const LABEL_MAX_TOKENS: u32 = 1024;
/// Presupuesto de contexto de «Ask» (~6k tokens).
const ASK_BUDGET_CHARS: usize = 20_000;
/// Una reunión automática más corta que esto y con menos de 2 turnos se descarta sin resumen
/// (alguien abrió Teams para probar el micro).
const AUTO_MIN_DURATION_MS: i64 = 60_000;
const AUTO_MIN_SEGMENTS: usize = 2;
/// Si una fuente va más de este desfase por detrás del reloj, se rellena con silencio.
const GAP_FILL_SAMPLES: i64 = 3_200; // 200 ms a 16 kHz

struct Active {
    meeting: Meeting,
    started: Instant,
    stop: Arc<AtomicBool>,
    sources: Vec<JoinHandle<()>>,
    level: Option<JoinHandle<()>>,
    /// Iniciada sola por el detector (no a mano).
    auto: bool,
}

#[derive(Default)]
pub struct MeetingManager {
    active: Mutex<Option<Active>>,
    flag: AtomicBool,
    /// La reunión en curso la empezó el detector (vale mientras `flag`). Atómico, como `flag`, para
    /// que el detector no espere al candado mientras `start_inner` abre los dispositivos.
    auto_flag: AtomicBool,
    /// Última app de reunión detectada (rellena `Meeting.app` al empezar).
    detected: Mutex<Option<String>>,
    /// Reuniones con la fase 2 en marcha.
    processing: Mutex<HashSet<i64>>,
    /// La pregunta de «la primera vez» (`meetingAutoConfirmed`): si el usuario empieza la reunión
    /// mientras está en pantalla, es su «sí».
    first_ask: Mutex<auto::FirstAsk>,
}

impl MeetingManager {
    pub fn is_active(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }

    /// Qué se está grabando, para el detector: así distingue una reunión manual nueva de la
    /// automática que el usuario acaba de parar (rompedor 22-oct-c, BAJA 4).
    pub fn recording(&self) -> auto::Recording {
        match (self.flag.load(Ordering::SeqCst), self.auto_flag.load(Ordering::SeqCst)) {
            (false, _) => auto::Recording::Idle,
            (true, true) => auto::Recording::Auto,
            (true, false) => auto::Recording::Manual,
        }
    }

    /// ¿Hay alguna reunión con la fase 2 (notas) en marcha? Cerrar Pronto ahora la dejaría `failed`.
    pub fn any_processing(&self) -> bool {
        !self.processing.lock().is_empty()
    }

    pub fn set_detected(&self, app: Option<String>) {
        *self.detected.lock() = app;
    }

    /// Reunión en curso y tiempo transcurrido.
    pub fn status(&self) -> Option<(Meeting, i64)> {
        self.active.lock().as_ref().map(|a| (a.meeting.clone(), a.started.elapsed().as_millis() as i64))
    }

    /// ¿Se están generando las notas (fase 2) de esta reunión?
    pub fn is_processing(&self, id: i64) -> bool {
        self.processing.lock().contains(&id)
    }
}

/// Marca una reunión «en fase 2» mientras vive.
struct Processing<'a> {
    st: &'a AppState,
    id: i64,
}

impl<'a> Processing<'a> {
    fn begin(st: &'a AppState, id: i64) -> Option<Self> {
        st.meeting.processing.lock().insert(id).then_some(Self { st, id })
    }
}

impl Drop for Processing<'_> {
    fn drop(&mut self) {
        self.st.meeting.processing.lock().remove(&self.id);
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct MeetingState<'a> {
    state: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    meeting_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

fn emit_state(app: &AppHandle, state: &str, meeting_id: Option<i64>, message: Option<String>) {
    let _ = app.emit("meeting://state", MeetingState { state, meeting_id, message });
}

/// Avance de la fase 2 por pasos: `transcribing` (1), `polishing` (2), `labeling` (3) y
/// `understanding` (4), cada uno con su 0–99 %, y `done` (100).
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Progress<'a> {
    meeting_id: i64,
    stage: &'a str,
    percent: u8,
    step: u8,
    steps: u8,
}

fn emit_progress(app: &AppHandle, meeting_id: i64, stage: &str, percent: u8) {
    let step = match stage {
        "transcribing" => 1,
        "polishing" => 2,
        "labeling" => 3,
        "understanding" => 4,
        _ => 5,
    };
    let _ = app.emit("meeting://progress", Progress { meeting_id, stage, percent, step, steps: 4 });
}

// ------------------------------------------------------------------ empezar / parar

/// Empieza a tomar notas a mano. Devuelve la reunión creada (`summaryStatus: "none"`).
pub fn start(app: &AppHandle, st: &Arc<AppState>, title: Option<String>) -> Result<Meeting, String> {
    start_inner(app, st, title, false)
}

/// Milisegundos desde la primera vez que se pregunta (reloj de `auto::FirstAsk`).
fn mono_ms() -> u64 {
    static T0: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    T0.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// El usuario empezó una reunión a mano: si acababa de verse la pregunta de la primera vez («¿Grabar
/// esta reunión en Meets?»), era un «sí». Se guarda `meetingAutoConfirmed` y las siguientes se graban
/// solas, como hasta ahora (decisión de Txonso, 23-oct-2026).
fn confirm_first_ask(app: &AppHandle, st: &Arc<AppState>) {
    if !st.meeting.first_ask.lock().accepted(mono_ms()) {
        return;
    }
    let snapshot = {
        let mut w = st.settings.write();
        if w.meeting_auto_confirmed {
            return;
        }
        w.meeting_auto_confirmed = true;
        w.clone()
    };
    if let Err(e) = st.db.save_settings(&snapshot) {
        errlog::error("meeting", format!("no se pudo guardar la confirmación de las reuniones automáticas: {e}"));
    }
    let _ = app.emit("settings://changed", snapshot);
}

/// Empieza sola al detectar `app`: título «Reunión de {App} · HH:MM» (provisional: el resumen lo
/// sustituye por uno del contenido) y aviso en la Flow bar con el botón de parar.
pub fn start_auto(app: &AppHandle, st: &Arc<AppState>, label: &str) -> Result<Meeting, String> {
    st.meeting.set_detected(Some(label.to_string()));
    let title = auto::auto_title(label, &chrono::Local::now().format("%H:%M").to_string());
    let meeting = start_inner(app, st, Some(title), true)?;
    let _ = app.emit(
        "dictation://notice",
        Notice { title: format!("Meets: grabando {label}"), body: Some("Se para solo al colgar.".into()), action: Some("stop_meeting".into()) },
    );
    Ok(meeting)
}

fn source_name(kind: SourceKind) -> &'static str {
    if kind == SourceKind::Mic { "tu micrófono" } else { "el audio del sistema" }
}

fn start_inner(app: &AppHandle, st: &Arc<AppState>, title: Option<String>, auto: bool) -> Result<Meeting, String> {
    let mut guard = st.meeting.active.lock();
    if guard.is_some() {
        return Err("Ya hay una reunión en curso".into());
    }
    let settings = st.settings.read().clone();
    let detected = st.meeting.detected.lock().clone();
    // El título de una reunión automática es provisional (lo sustituye el del resumen); el que
    // pone el usuario a mano, no.
    let meeting = st
        .db
        .create_meeting_titled(title.as_deref().unwrap_or(""), detected.as_deref(), title.is_none() || auto, auto)
        .map_err(|e| e.to_string())?;
    let started = Instant::now();
    let stop = Arc::new(AtomicBool::new(false));
    let level_mic = Arc::new(AtomicU32::new(0));
    let level_sys = Arc::new(AtomicU32::new(0));

    let mut sources = Vec::new();
    let mut failures: Vec<String> = Vec::new();
    for (kind, speaker, level) in [(SourceKind::Mic, "me", level_mic.clone()), (SourceKind::System, "them", level_sys.clone())] {
        let Some(wav) = audio_file(&meeting, speaker) else {
            failures.push(format!("{}: no hay carpeta de audio", source_name(kind)));
            continue;
        };
        let (ready_tx, ready_rx) = crossbeam_channel::bounded::<Result<(), String>>(1);
        let ctx = SourceCtx { app: app.clone(), kind, t0: started, stop: stop.clone(), level, mic: settings.mic_name.clone(), wav };
        let spawned = std::thread::Builder::new().name(format!("pronto-meeting-{speaker}")).spawn(move || run_source(ctx, ready_tx));
        match spawned {
            Ok(h) => match ready_rx.recv_timeout(Duration::from_secs(8)) {
                Ok(Ok(())) => sources.push(h),
                Ok(Err(e)) => {
                    let _ = h.join();
                    failures.push(format!("{}: {e}", source_name(kind)));
                }
                Err(_) => failures.push(format!("{} no respondió", source_name(kind))),
            },
            Err(e) => failures.push(e.to_string()),
        }
    }
    if sources.is_empty() {
        stop.store(true, Ordering::SeqCst);
        let _ = st.db.delete_meeting(meeting.id);
        remove_audio_files(&meeting);
        return Err(format!("No se pudo capturar audio ({})", failures.join("; ")));
    }
    // Una pista sola sigue valiendo: se avisa y se graba la otra.
    for f in &failures {
        state::emit_notice(app, "Meets con una sola pista", Some(f));
    }

    let level = spawn_level_emitter(app.clone(), stop.clone(), level_mic, level_sys);
    *guard = Some(Active { meeting: meeting.clone(), started, stop, sources, level: Some(level), auto });
    st.meeting.auto_flag.store(auto, Ordering::SeqCst);
    st.meeting.flag.store(true, Ordering::SeqCst);
    drop(guard);

    emit_state(app, "recording", Some(meeting.id), None);
    if !st.active.load(Ordering::SeqCst) {
        state::emit_state(app, "meeting", None);
    }
    if !auto {
        confirm_first_ask(app, st);
    }
    Ok(meeting)
}

/// Para la grabación, deja la reunión guardada y lanza la fase 2 en segundo plano.
pub fn stop(app: &AppHandle, st: &Arc<AppState>) -> Result<Meeting, String> {
    let mut active = st.meeting.active.lock().take().ok_or("No hay ninguna reunión en curso")?;
    st.meeting.flag.store(false, Ordering::SeqCst);
    active.stop.store(true, Ordering::SeqCst);
    for h in active.sources.drain(..) {
        let _ = h.join();
    }
    if let Some(h) = active.level.take() {
        let _ = h.join();
    }
    let duration_ms = active.started.elapsed().as_millis() as i64;
    let id = active.meeting.id;
    st.db.finish_meeting(id, duration_ms).map_err(|e| e.to_string())?;

    emit_state(app, "summarizing", Some(id), None);
    if !st.active.load(Ordering::SeqCst) {
        state::emit_state(app, "idle", None);
    }
    let auto_started = active.auto;
    let (app2, st2) = (app.clone(), st.clone());
    let spawned = std::thread::Builder::new().name("pronto-notes".into()).spawn(move || process_meeting(&app2, &st2, id, auto_started, auto_started));
    if let Err(e) = spawned {
        errlog::error("meeting", format!("no se pudo lanzar la fase 2: {e}"));
    }
    st.db.get_meeting(id).map_err(|e| e.to_string())?.ok_or_else(|| "La reunión ya no existe".to_string())
}

/// Una reunión automática de menos de 60 s con menos de 2 turnos no merece resumen.
fn too_short_to_keep(duration_ms: i64, segments: usize) -> bool {
    duration_ms < AUTO_MIN_DURATION_MS && segments < AUTO_MIN_SEGMENTS
}

/// Borra una reunión descartada y avisa de que ya no hay nada pendiente.
fn discard(app: &AppHandle, st: &Arc<AppState>, id: i64) {
    remove_audio_files_of(st, id);
    if let Err(e) = st.db.delete_meeting(id) {
        errlog::error("meeting", format!("no se pudo descartar la reunión corta: {e}"));
    }
    emit_state(app, "idle", Some(id), Some("Descartada: reunión demasiado corta".into()));
}

/// Atajo / aviso: empieza si no hay reunión y para si la hay. No bloquea al llamador.
pub fn toggle(app: &AppHandle, st: &Arc<AppState>) {
    let (app, st) = (app.clone(), st.clone());
    std::thread::spawn(move || {
        let result = if st.meeting.is_active() { stop(&app, &st).map(|_| ()) } else { start(&app, &st, None).map(|_| ()) };
        if let Err(e) = result {
            errlog::error("meeting", &e);
            state::emit_notice(&app, "Meets", Some(&e));
        }
    });
}

/// WAV de una pista: `meeting_{id}-{cifras de started_at}_{me|them}.wav`. Solo con el id (hasta la
/// 2.2.0), una base nueva reutilizaba los id y grababa encima del audio de las reuniones de la buena
/// (rompedor 22-oct, MEDIA 3).
pub(crate) fn audio_file(m: &Meeting, speaker: &str) -> Option<PathBuf> {
    Some(db::audio_dir()?.join(format!("meeting_{}-{}_{speaker}.wav", m.id, db::stamp_digits(&m.started_at))))
}

/// WAV ya grabado de una pista: el de su nombre o, para las de antes, `meeting_{id}_{who}.wav` solo
/// si se escribió durante esa reunión (si no, es de otra reunión con el mismo id).
fn existing_audio_file(m: &Meeting, speaker: &str) -> Option<PathBuf> {
    let p = audio_file(m, speaker)?;
    if p.is_file() {
        return Some(p);
    }
    let legacy = db::audio_dir()?.join(format!("meeting_{}_{speaker}.wav", m.id));
    (legacy.is_file() && db::written_between(&legacy, &m.started_at, m.ended_at.as_deref())).then_some(legacy)
}

/// WAV de la reunión que existen, con su hablante.
fn audio_files(m: &Meeting) -> Vec<(&'static str, PathBuf)> {
    ["them", "me"].into_iter().filter_map(|who| existing_audio_file(m, who).map(|p| (who, p))).collect()
}

/// Borra los WAV guardados de una reunión.
pub fn remove_audio_files(m: &Meeting) {
    for who in ["me", "them"] {
        if let Some(p) = existing_audio_file(m, who) {
            let _ = std::fs::remove_file(p);
        }
    }
}

/// Lo mismo, por id (la reunión aún existe en la base).
fn remove_audio_files_of(st: &AppState, id: i64) {
    if let Ok(Some(m)) = st.db.get_meeting(id) {
        remove_audio_files(&m);
    }
}

// ------------------------------------------------------------------ fase 1: captura de una pista

struct SourceCtx {
    app: AppHandle,
    kind: SourceKind,
    t0: Instant,
    stop: Arc<AtomicBool>,
    level: Arc<AtomicU32>,
    mic: Option<String>,
    wav: PathBuf,
}

fn store_level(a: &AtomicU32, v: f32) {
    a.store(v.to_bits(), Ordering::Relaxed);
}

fn load_level(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Ordering::Relaxed))
}

/// Graba una pista a su WAV, alineada con el reloj de la reunión. No transcribe nada: durante la
/// reunión solo se guarda el audio y se mide el nivel.
fn run_source(ctx: SourceCtx, ready: Sender<Result<(), String>>) {
    let mut wav = match WavStream::create_new(&ctx.wav) {
        Ok(w) => w,
        Err(e) => {
            let _ = ready.send(Err(format!("no se pudo crear el audio ({e})")));
            return;
        }
    };
    let (tx, rx) = unbounded::<Vec<f32>>();
    let OpenSource { stream, rate } = match audio::open_source(ctx.kind, ctx.mic.as_deref(), tx) {
        Ok(s) => s,
        Err(e) => {
            drop(wav);
            let _ = std::fs::remove_file(&ctx.wav);
            let _ = ready.send(Err(e));
            return;
        }
    };
    let _ = ready.send(Ok(()));

    let resampler = (rate != TARGET_RATE).then(|| sherpa_onnx::LinearResampler::create(rate as i32, TARGET_RATE as i32)).flatten();
    // La línea de tiempo (y el WAV) empiezan en el inicio de la reunión, no al abrir esta fuente:
    // el primer relleno de silencio cubre el arranque y las dos pistas quedan alineadas.
    let origin = ctx.t0;
    // Muestras (16 kHz) escritas desde `origin`.
    let mut fed: i64 = 0;
    let mut writing = true;
    let mut write = |samples: &[f32], fed: &mut i64| {
        *fed += samples.len() as i64;
        if writing {
            if let Err(e) = wav.write(samples) {
                // Disco lleno u otro fallo: esta pista se queda con lo grabado hasta aquí y la
                // reunión sigue con la otra.
                errlog::error("meeting", format!("error escribiendo el audio: {e}"));
                writing = false;
                let at = summary::format_clock(*fed * 1000 / TARGET_RATE as i64);
                state::emit_notice(&ctx.app, &format!("Meets: se dejó de grabar {} en el {at}", source_name(ctx.kind)), Some("Se sigue con la otra pista."));
            }
        }
    };

    while !ctx.stop.load(Ordering::SeqCst) {
        let expected = origin.elapsed().as_millis() as i64 * (TARGET_RATE as i64 / 1000);
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(chunk) => {
                let mono: Vec<f32> = match &resampler {
                    Some(r) => r.resample(&chunk, false),
                    None => chunk,
                };
                if !mono.is_empty() {
                    let rms = (mono.iter().map(|s| s * s).sum::<f32>() / mono.len() as f32).sqrt();
                    let prev = load_level(&ctx.level);
                    store_level(&ctx.level, audio::rms_to_level(rms).max(prev * 0.8));
                    // Si venimos de un silencio sin paquetes (loopback), alinea con el reloj.
                    let behind = expected - fed - mono.len() as i64;
                    if behind > GAP_FILL_SAMPLES {
                        fill_silence(behind, |s| write(s, &mut fed));
                    }
                    write(&mono, &mut fed);
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                // Sin paquetes = silencio. La línea de tiempo sigue al reloj de pared.
                store_level(&ctx.level, load_level(&ctx.level) * 0.5);
                let behind = expected - fed;
                if behind > GAP_FILL_SAMPLES {
                    fill_silence(behind, |s| write(s, &mut fed));
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }

    drop(stream);
    // Lo que quedara en cola de audio.
    while let Ok(chunk) = rx.try_recv() {
        let mono = match &resampler {
            Some(r) => r.resample(&chunk, false),
            None => chunk,
        };
        write(&mono, &mut fed);
    }
    if let Err(e) = wav.finish() {
        errlog::error("meeting", format!("no se pudo cerrar el audio: {e}"));
    }
    store_level(&ctx.level, 0.0);
}

/// Trozo más grande de silencio que se reserva de una vez al rellenar un hueco (1 s).
const SILENCE_CHUNK: usize = TARGET_RATE as usize;

/// Rellena `behind` muestras de silencio en trozos de como mucho 1 s. Tras una suspensión de horas
/// con la reunión abierta, `behind` puede valer cientos de millones de muestras: reservarlas de
/// una vez (`vec![0.0; behind]`) serían gigas por pista. Así la memoria no pasa de 64 KB y las dos
/// pistas siguen alineadas con el reloj.
fn fill_silence(behind: i64, mut write: impl FnMut(&[f32])) {
    let zeros = [0.0f32; SILENCE_CHUNK];
    let mut left = behind.max(0) as u64;
    while left > 0 {
        let n = left.min(SILENCE_CHUNK as u64) as usize;
        write(&zeros[..n]);
        left -= n as u64;
    }
}

fn spawn_level_emitter(app: AppHandle, stop: Arc<AtomicBool>, mic: Arc<AtomicU32>, sys: Arc<AtomicU32>) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name("pronto-meeting-level".into())
        .spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                let _ = app.emit("meeting://level", serde_json::json!({ "mic": load_level(&mic), "system": load_level(&sys) }));
                std::thread::sleep(Duration::from_millis(66));
            }
            let _ = app.emit("meeting://level", serde_json::json!({ "mic": 0.0, "system": 0.0 }));
        })
        .unwrap_or_else(|e| {
            errlog::error("meeting", format!("no se pudo lanzar el emisor de nivel: {e}"));
            std::thread::spawn(|| {})
        })
}

// ------------------------------------------------------------------ limpieza de texto

/// Nivel de limpieza y diccionario con que se limpian los turnos de reunión.
fn text_cleaner(st: &AppState) -> (CleanupLevel, Vec<Word>) {
    let cleanup = if st.settings.read().cleanup_level == CleanupLevel::None { CleanupLevel::None } else { CleanupLevel::Light };
    (cleanup, st.db.list_words().unwrap_or_default())
}

/// Limpieza ligera del texto de un turno (muletillas y diccionario, con mayúscula y punto).
/// `None` si es basura.
pub fn clean_text(raw: &str, cleanup: CleanupLevel, words: &[Word]) -> Option<String> {
    if is_junk_text(raw) {
        return None; // Parakeet a veces devuelve un punto o una letra suelta con ruido
    }
    let options = Options { cleanup, press_enter: false, tone: Tone::Formal, words, snippets: &[] };
    let text = text::process(raw, &options).text;
    Some(text.trim().to_string()).filter(|t| !is_junk_text(t))
}

/// Palabras de una o dos letras que sí son respuestas válidas («Sí.», «No.», «Ok.»).
const SHORT_WORDS: &[&str] = &[
    "si", "sí", "no", "ya", "ok", "ah", "oh", "eh", "ja", "ay", "hi", "go", "me", "we", "do", "it", "is", "so", "um", "uh", "ha", "mm",
];

/// Texto de un trozo que es ruido: menos de 2 caracteres alfanuméricos («.», «S.»), o una sola
/// palabra de ≤ 2 letras que no es una respuesta corta real (caso real: «S.» por ruido del RODE).
pub fn is_junk_text(text: &str) -> bool {
    if text.chars().filter(|c| c.is_alphanumeric()).count() < 2 {
        return true;
    }
    let mut words = text.split_whitespace();
    let (Some(only), None) = (words.next(), words.next()) else { return false };
    let letters: String = only.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
    letters.chars().count() <= 2 && !SHORT_WORDS.contains(&letters.as_str())
}

// ------------------------------------------------------------------ fase 2

/// Espera (hasta 2 min) a que el motor de voz termine de cargar.
fn wait_engine(st: &AppState) -> Result<(), String> {
    let t0 = Instant::now();
    while st.engine.status().loading && t0.elapsed() < Duration::from_secs(120) {
        std::thread::sleep(Duration::from_millis(500));
    }
    let status = st.engine.status();
    if status.ready { Ok(()) } else { Err(status.error.unwrap_or_else(|| "El motor de voz no está listo".into())) }
}

/// Motivo cuando Ollama dejó de responder durante la fase 2 (cortacircuitos).
const OLLAMA_HUNG: &str = "Ollama dejó de responder durante el proceso. La transcripción está guardada: reintenta el resumen cuando Ollama vuelva a funcionar.";

/// Lo que se explica al usuario cuando la transcripción final sale incompleta.
const PARTIAL_TRANSCRIPT: &str = "Parte del audio no se pudo transcribir. El audio se conserva: reintenta el resumen.";

/// ¿La transcripción final está entera? Solo entonces se puede borrar el audio: con una pista que
/// falló o con trozos saltados, la reunión queda para reintentar con el audio de todas las pistas.
fn transcript_complete(tracks: usize, failed: usize, skipped: usize) -> bool {
    tracks > 0 && failed == 0 && skipped == 0
}

/// Transcripción final desde los WAV (`None` si no hay audio): cada pista con tiempos por palabra,
/// eco fuera y turnos encadenados. Devuelve también los problemas de cada turno (para el pulido) y
/// si salió entera (`transcript_complete`).
/// El dictado sigue teniendo prioridad sobre el motor (`transcribe_background_timed`).
#[allow(clippy::type_complexity)]
fn final_transcript(app: &AppHandle, st: &Arc<AppState>, id: i64) -> Result<Option<(Vec<Segment>, Vec<Vec<&'static str>>, bool)>, String> {
    let Some(m) = st.db.get_meeting(id).map_err(|e| e.to_string())? else { return Ok(None) };
    let files = audio_files(&m);
    if files.is_empty() {
        return Ok(None);
    }
    wait_engine(st)?;
    emit_progress(app, id, "transcribing", 0);
    let total: u64 = files.iter().map(|(_, p)| std::fs::metadata(p).map(|m| m.len().saturating_sub(44) / 2).unwrap_or(0)).sum::<u64>().max(1);
    let (cleanup, words) = text_cleaner(st);
    st.engine.set_vocabulary(&crate::state::vocabulary(&words));
    // Huella de voz (Contrato v11) en cada trozo: ~1-2 ms por segundo de audio con 50 muestras.
    let voice = crate::voiceprint::bank(&words);
    let mut voice_fixes = 0usize;
    let mut asr = |s: &[f32]| -> Result<notes::AsrOut, String> {
        let t = st.engine.transcribe_background_timed(s)?;
        let fixes = if voice.is_empty() { Vec::new() } else { crate::voiceprint::corrections(&voice, s, &t) };
        voice_fixes += fixes.len();
        Ok((t, fixes))
    };
    let (mut me, mut them) = (Vec::new(), Vec::new());
    let (mut done, mut longest, mut last_pct, mut skipped) = (0u64, 0u64, 0u8, 0usize);
    let mut failed = Vec::new();
    for (who, path) in &files {
        let mut on_progress = |n: u64| {
            let pct = ((done + n) * 100 / total).min(99) as u8;
            if pct != last_pct {
                last_pct = pct;
                emit_progress(app, id, "transcribing", pct);
            }
        };
        let t0 = Instant::now();
        match notes::transcribe_words(path, who, &mut asr, &mut on_progress) {
            Ok((words, samples, skips)) => {
                eprintln!("[meeting] pista {who}: {:.0} s de audio en {:.1} s, {} palabras, {skips} trozos saltados", samples as f32 / TARGET_RATE as f32, t0.elapsed().as_secs_f32(), words.len());
                skipped += skips;
                done += samples;
                longest = longest.max(samples);
                if *who == "me" { me = words } else { them = words }
            }
            // Una pista que no se puede leer no tira la otra (pero la reunión queda para reintentar).
            Err(e) => {
                errlog::error("meeting", format!("pista {who} sin transcribir: {e}"));
                failed.push(e);
            }
        }
    }
    drop(asr);
    if failed.len() == files.len() {
        return Err(failed.remove(0));
    }
    let complete = transcript_complete(files.len(), failed.len(), skipped);
    if voice_fixes > 0 {
        eprintln!("[meeting] huella de voz: {voice_fixes} palabras corregidas");
    }
    let _ = st.db.extend_duration(id, (longest * 1000 / TARGET_RATE as u64) as i64);
    let clean = |raw: &str| clean_text(raw, cleanup, &words);
    let (segs, found, stats) = notes::conversation(me, them, id, &clean);
    eprintln!("[meeting] {} turnos; eco: {} palabras y {} turnos", segs.len(), stats.echo_words, stats.echo_turns);
    Ok(Some((segs, found, complete)))
}

/// Fase 2 de `id`: transcripción final (si aún no la hay), pulido de los turnos dudosos, etiquetas
/// y comprensión con Ollama. Avisa con `meeting://progress`, `meeting://summary` y
/// `meeting://state` (`done` | `error`). `discard_short`: una reunión automática que resulta
/// demasiado corta se borra sin resumen.
pub fn process_meeting(app: &AppHandle, st: &Arc<AppState>, id: i64, notify: bool, discard_short: bool) {
    let Some(_busy) = Processing::begin(st, id) else { return };
    let _ = st.db.set_summary_status(id, "pending");
    emit_state(app, "summarizing", Some(id), None);

    // 1) Transcripción final desde el audio. Si falla, o sale solo en parte, el audio se queda para
    //    reintentar.
    let mut found = None;
    if !st.db.transcript_final(id).unwrap_or(false) {
        match final_transcript(app, st, id) {
            Ok(Some((segments, f, complete))) => match st.db.replace_segments(id, &segments, complete) {
                // Falta una pista o algún trozo: se guarda lo que salió, pero sin darlo por final y
                // con el audio de todas las pistas, para reintentar (reglas 2 y 5).
                Ok(true) if !complete => return fail_summary(app, st, id, PARTIAL_TRANSCRIPT.into()),
                Ok(true) => {
                    found = Some(f);
                    // Con la transcripción final guardada el audio ya no hace falta para reintentar.
                    if !st.settings.read().meeting_keep_audio {
                        remove_audio_files_of(st, id);
                    }
                }
                Ok(false) => return, // la reunión se borró mientras tanto
                Err(e) => return fail_summary(app, st, id, format!("No se pudo guardar la transcripción: {e}")),
            },
            Ok(None) => {}
            Err(e) => return fail_summary(app, st, id, e),
        }
    }
    let Ok(Some(meeting)) = st.db.get_meeting(id) else { return };
    let mut segs = st.db.segments(id).unwrap_or_default();
    if discard_short && too_short_to_keep(meeting.duration_ms, segs.len()) {
        discard(app, st, id);
        return;
    }
    if segs.is_empty() {
        return fail_summary(app, st, id, notes::NO_TRANSCRIPT.into());
    }

    let (url, model, me) = {
        let s = st.settings.read();
        let me = if s.first_name.trim().is_empty() { "el usuario".to_string() } else { s.first_name.trim().to_string() };
        (s.ollama_url.clone(), s.ollama_model.clone(), me)
    };
    // Ollama instalado pero parado: se arranca solo (si no, las notas fallarían con «no responde»).
    crate::aisetup::ensure_running(&url, Duration::from_secs(30));
    let status = ollama::status(&url);
    let model = ollama::notes_model(&status.models, &model);
    // Topes por llamada: si el modelo se enrolla, el reintento toma el relevo.
    let opts = ollama::GenOpts { json: true, num_ctx: Some(NUM_CTX), timeout: Duration::from_secs(240), keep_alive: Some("10m"), temperature: None, num_predict: Some(2048) };
    // Cortacircuitos de la fase (rompedor 23-oct, reuniones MEDIA 4): cuando una petición a Ollama
    // vence su plazo o no conecta, las siguientes no se hacen. Cada una esperaría su plazo entero
    // (240 s) y con cientos de turnos la reunión se quedaba horas «procesando», sin poder borrarla,
    // reintentarla ni actualizar Pronto. También se corta pasado el plazo total de la fase. Lo que
    // ya está (transcripción, pulido parcial, etiquetas por reglas) se guarda y las notas fallan con
    // un mensaje claro, reintentable.
    let phase_budget = Duration::from_secs(30 * 60) + Duration::from_millis(2 * meeting.duration_ms.max(0) as u64);
    let phase_start = Instant::now();
    let hung = std::cell::Cell::new(false);
    let guarded = |temperature: f32, prompt: &str, o: &ollama::GenOpts| -> Result<String, String> {
        if hung.get() || phase_start.elapsed() > phase_budget {
            hung.set(true);
            return Err(OLLAMA_HUNG.into());
        }
        let r = ollama::generate_with(&url, &model, prompt, &ollama::GenOpts { temperature: Some(temperature), ..*o });
        if let Err(e) = &r {
            if notes::ollama_unresponsive(e) {
                hung.set(true);
            }
        }
        r
    };
    let mut llm = |prompt: &str, temperature: f32| guarded(temperature, prompt, &opts);
    // Las etiquetas van en líneas cortas (`5 r 4`), no en JSON: sin formato forzado y con tope pequeño.
    let label_opts = ollama::GenOpts { json: false, num_predict: Some(LABEL_MAX_TOKENS), ..opts };
    let mut llm_labels = |prompt: &str, temperature: f32| guarded(temperature, prompt, &label_opts);

    // El modelo grande puede tardar minutos en cargarse: una petición mínima con plazo amplio lo
    // deja listo para que las de verdad no venzan su plazo mientras carga.
    if status.reachable {
        let t0 = Instant::now();
        let warm = ollama::GenOpts { json: false, num_ctx: Some(NUM_CTX), timeout: Duration::from_secs(900), keep_alive: Some("10m"), temperature: Some(0.0), num_predict: Some(1) };
        if let Err(e) = ollama::generate_with(&url, &model, "ok", &warm) {
            errlog::error("meeting", format!("no se pudo cargar {model}: {e}"));
            if notes::ollama_unresponsive(&e) {
                hung.set(true);
            }
        } else if t0.elapsed() > Duration::from_secs(5) {
            eprintln!("[meeting] {model} cargado en {:.0} s", t0.elapsed().as_secs_f32());
        }
    }

    // 2) Pulido selectivo (una vez por reunión y solo con Ollama).
    if status.reachable && !hung.get() && !st.db.polish_done(id).unwrap_or(true) {
        emit_progress(app, id, "polishing", 0);
        let t0 = Instant::now();
        match notes::polish_turns(&mut llm, &mut segs, found.as_deref(), &mut |a, b| emit_progress(app, id, "polishing", (a * 100 / b.max(1)).min(99) as u8)) {
            Ok(n) => {
                eprintln!("[meeting] pulido: {n} turnos en {:.1} s", t0.elapsed().as_secs_f32());
                let _ = st.db.set_polish_done(id);
            }
            // Sin marcar como hecho: el siguiente «Regenerar» retoma el pulido (lo ya pulido queda).
            Err(e) => errlog::error("meeting", format!("pulido sin terminar: {e}")),
        }
    }

    // 3) Etiquetas: reglas siempre y, con Ollama, el modelo.
    emit_progress(app, id, "labeling", 0);
    let t0 = Instant::now();
    match notes::label_turns(&mut llm_labels, &mut segs, &me, &mut |a, b| emit_progress(app, id, "labeling", (a * 100 / b.max(1)).min(99) as u8)) {
        Ok(n) => eprintln!("[meeting] etiquetas: {n} del modelo en {:.1} s", t0.elapsed().as_secs_f32()),
        Err(e) => errlog::error("meeting", format!("etiquetas solo por reglas: {e}")),
    }
    match st.db.replace_segments(id, &segs, true) {
        Ok(true) => {}
        Ok(false) => return,
        Err(e) => errlog::error("meeting", format!("no se pudieron guardar pulido y etiquetas: {e}")),
    }

    // 4) Comprensión: notas por bloques y síntesis.
    emit_progress(app, id, "understanding", 0);
    let t0 = Instant::now();
    let result = notes::understand(&mut llm, &segs, &me, &mut |step, total| emit_progress(app, id, "understanding", (step * 100 / total.max(1)) as u8));
    eprintln!("[meeting] notas con {model} en {:.1} s", t0.elapsed().as_secs_f32());
    match result {
        Ok(parsed) => match st.db.save_summary(id, &parsed) {
            Ok(()) => {
                if let Ok(Some(detail)) = st.db.get_meeting_detail(id) {
                    if notify {
                        // Reunión automática: el usuario no la pidió, así que se le avisa de que está lista.
                        let _ = app.emit(
                            "dictation://notice",
                            Notice { title: "Notas listas".into(), body: Some(detail.meeting.title.clone()), action: Some(format!("open_meeting:{id}")) },
                        );
                    }
                    let _ = app.emit("meeting://summary", detail);
                }
                emit_progress(app, id, "done", 100);
                emit_state(app, "done", Some(id), None);
            }
            Err(e) => fail_summary(app, st, id, e.to_string()),
        },
        Err(e) => fail_summary(app, st, id, e),
    }
}

fn fail_summary(app: &AppHandle, st: &Arc<AppState>, id: i64, message: String) {
    errlog::error("meeting", format!("notas fallidas: {message}"));
    // Un «Regenerar» fallido conserva las notas buenas (resumen, tareas, marcas) y avisa del fallo:
    // con `failed` el hub las escondería (rompedor 23-oct, reuniones MEDIA 5).
    let kept = st.db.fail_summary_status(id).unwrap_or_else(|_| {
        let _ = st.db.set_summary_status(id, "failed");
        false
    });
    if let Ok(Some(detail)) = st.db.get_meeting_detail(id) {
        let _ = app.emit("meeting://summary", detail);
    }
    let message = if kept { format!("Se conservan las notas anteriores. {message}") } else { message };
    emit_state(app, "error", Some(id), Some(message));
}

/// Reintenta o regenera las notas en segundo plano (fase 2 con la transcripción final; si aún no
/// la hay y queda el audio, la saca primero).
pub fn retry_summary(app: &AppHandle, st: &Arc<AppState>, id: i64) -> Result<(), String> {
    if st.meeting.status().is_some_and(|(m, _)| m.id == id) {
        return Err("La reunión sigue en curso".into());
    }
    if st.meeting.is_processing(id) {
        return Err("Ya se están generando las notas de esta reunión".into());
    }
    if st.db.get_meeting(id).map_err(|e| e.to_string())?.is_none() {
        return Err("Esa reunión ya no existe".into());
    }
    st.db.set_summary_status(id, "pending").map_err(|e| e.to_string())?;
    let (app, st) = (app.clone(), st.clone());
    std::thread::Builder::new()
        .name("pronto-notes".into())
        .spawn(move || process_meeting(&app, &st, id, false, false))
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Pregunta libre sobre las reuniones (o sobre una concreta).
pub fn ask(st: &Arc<AppState>, question: &str, meeting_id: Option<i64>) -> Result<String, String> {
    if question.trim().is_empty() {
        return Err("Escribe una pregunta".into());
    }
    let (ids, words): (Vec<i64>, Vec<String>) = match meeting_id {
        Some(id) => (vec![id], Vec::new()),
        None => {
            let mut ids = st.db.rank_meetings(question, 4).map_err(|e| e.to_string())?;
            if ids.is_empty() {
                ids = st.db.recent_meeting_ids(3).map_err(|e| e.to_string())?;
            }
            (ids, crate::meetingdb::search_words(question))
        }
    };
    let mut meetings = Vec::new();
    for id in ids {
        let Some(d) = st.db.get_meeting_detail(id).map_err(|e| e.to_string())? else { continue };
        let all_text = d.segments.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ");
        let lang = summary::detect_language(&all_text);
        // El resumen corto y lo más importante que se dijo.
        let mut brief = d.summary.clone().unwrap_or_default();
        if !d.key_points.is_empty() {
            brief.push_str(&format!(" Puntos clave: {}", d.key_points.join("; ")));
        }
        meetings.push(summary::AskMeeting {
            title: d.meeting.title.clone(),
            date: summary::pretty_date(&d.meeting.started_at),
            summary: Some(brief.trim().to_string()).filter(|b| !b.is_empty()),
            lines: summary::transcript_lines(&d.segments, lang),
        });
    }
    if meetings.is_empty() {
        return Err("Aún no hay reuniones guardadas".into());
    }
    let context = summary::build_context(&meetings, &words, ASK_BUDGET_CHARS);
    let (url, model) = {
        let s = st.settings.read();
        (s.ollama_url.clone(), s.ollama_model.clone())
    };
    ollama::generate_raw(&url, &model, &summary::ask_prompt(question, &context), false, Some(NUM_CTX))
}

// ------------------------------------------------------------------ detección automática

/// Hilo que cada 3 s mira qué apps tienen el micrófono abierto y, según `meetingAuto`, empieza
/// a tomar notas solo (`auto`), avisa una vez por reunión (`ask`) o no hace nada (`off`). Una
/// reunión iniciada por el detector también la para él cuando la app cuelga (ver `auto.rs`).
pub fn spawn_detector(app: AppHandle, st: Arc<AppState>) {
    let spawned = std::thread::Builder::new().name("pronto-meeting-detect".into()).spawn(move || {
        let t0 = Instant::now();
        let mut machine = auto::AutoMachine::new();
        // Reuniones dudosas ya avisadas (modo «Automático»): una vez por llamada.
        let mut warned: Vec<String> = Vec::new();
        loop {
            std::thread::sleep(Duration::from_secs(3));
            let mode = st.settings.read().meeting_auto;
            let recording = st.meeting.recording();
            // Con la detección en «No» solo se sigue mirando si hay una automática en marcha: tiene
            // que poder pararse sola al colgar (rompedor 23-oct, reuniones BAJA 8).
            let off = mode == MeetingAuto::Off;
            let apps = if off && recording != auto::Recording::Auto { Vec::new() } else { detect::read_mic_consent() };
            let titles = if detect::any_browser_in_use(&apps) { detect::window_titles() } else { Vec::new() };
            let scan = detect::scan(&apps, &titles);
            st.meeting.set_detected(if off { None } else { scan.meetings.first().cloned() });
            // PRD §1: la detección automática se limita a Teams, Zoom y Meet. Slack, Webex, Jitsi y
            // Whereby solo se avisan en «ask»; y una reunión de navegador con otra ventana de Discord o
            // WhatsApp en el mismo navegador es dudosa (puede ser esa llamada): se avisa, no se graba
            // sola (rompedor 23-oct, reuniones MEDIA 3).
            let in_use = detect::candidates(&scan, mode == MeetingAuto::Auto);
            if mode == MeetingAuto::Auto && recording == auto::Recording::Idle {
                warned.retain(|w| scan.ambiguous.contains(w));
                let fresh: Vec<String> = scan.ambiguous.iter().filter(|l| !warned.contains(l)).cloned().collect();
                for label in fresh {
                    let _ = app.emit(
                        "dictation://notice",
                        Notice { title: "¿Grabar esta reunión en Meets?".into(), body: Some(label.clone()), action: Some("start_meeting".into()) },
                    );
                    warned.push(label);
                }
            } else {
                warned.clear();
            }
            let now_ms = t0.elapsed().as_millis() as u64;
            // La primera vez que «Automático» fuera a grabar, se pregunta (decisión de Txonso,
            // 23-oct-2026): `first_time_gate` cambia el `Start` por el mismo aviso del modo «ask».
            let confirmed = st.settings.read().meeting_auto_confirmed;
            match auto::first_time_gate(machine.step_browsers(now_ms, mode, recording, &in_use, &scan.browsers), confirmed) {
                Some(auto::AutoAction::Start(label)) => {
                    if let Err(e) = start_auto(&app, &st, &label) {
                        errlog::error("meeting", format!("no se pudo empezar sola: {e}"));
                    }
                }
                Some(auto::AutoAction::Stop) => {
                    if let Err(e) = stop(&app, &st) {
                        errlog::error("meeting", format!("no se pudo parar sola: {e}"));
                    }
                }
                Some(auto::AutoAction::Notify(label)) => {
                    let _ = app.emit(
                        "dictation://notice",
                        Notice { title: "¿Grabar esta reunión en Meets?".into(), body: Some(label), action: Some("start_meeting".into()) },
                    );
                    // En «Automático» y sin confirmar es la pregunta de la primera vez: si pulsa «Grabar», es el sí.
                    if mode == MeetingAuto::Auto && !confirmed {
                        st.meeting.first_ask.lock().asked(mono_ms());
                    }
                }
                None => {}
            }
        }
    });
    if let Err(e) = spawned {
        errlog::error("meeting", format!("no se pudo lanzar la detección: {e}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texto_basura_del_microfono() {
        // El caso real: ruido del RODE transcrito como «S.».
        assert!(is_junk_text("S."));
        assert!(is_junk_text("s"));
        assert!(is_junk_text("."));
        assert!(is_junk_text("¿?"));
        assert!(is_junk_text(""));
        assert!(is_junk_text("A."));
        assert!(is_junk_text("Xy."));
    }

    #[test]
    fn respuestas_cortas_reales_no_son_basura() {
        for ok in ["Sí.", "No.", "Ok.", "Vale.", "Hola", "Ya.", "Eh,", "Yes.", "Sí, claro", "Mm-hmm, vale"] {
            assert!(!is_junk_text(ok), "{ok}");
        }
    }

    /// Hueco de 8 h tras una suspensión: se rellena entero (las pistas siguen alineadas) sin
    /// reservar más de 1 s de silencio de una vez.
    #[test]
    fn el_relleno_tras_una_suspension_no_reserva_gigas() {
        let behind = 8 * 3_600 * TARGET_RATE as i64;
        let (mut total, mut biggest) = (0i64, 0usize);
        fill_silence(behind, |s| {
            total += s.len() as i64;
            biggest = biggest.max(s.len());
        });
        assert_eq!(total, behind);
        assert!(biggest <= TARGET_RATE as usize, "{biggest}");
        fill_silence(-5, |_| panic!("un desfase negativo no escribe nada"));
    }

    #[test]
    fn el_audio_solo_se_borra_con_todas_las_pistas_enteras() {
        assert!(transcript_complete(2, 0, 0));
        assert!(transcript_complete(1, 0, 0));
        assert!(!transcript_complete(2, 1, 0), "una pista falló: su audio y el de la otra se conservan");
        assert!(!transcript_complete(2, 0, 3), "trozos saltados: la transcripción no está entera");
        assert!(!transcript_complete(0, 0, 0));
    }

    #[test]
    fn reunion_automatica_demasiado_corta() {
        assert!(too_short_to_keep(30_000, 0));
        assert!(too_short_to_keep(59_999, 1));
        assert!(!too_short_to_keep(59_999, 2));
        assert!(!too_short_to_keep(60_000, 0));
        assert!(!too_short_to_keep(600_000, 1));
    }
}
