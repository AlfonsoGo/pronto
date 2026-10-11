//! Notetaker local: graba la reunión en dos fuentes a la vez (micrófono = «Tú», audio del sistema
//! por loopback = «Ellos»), las corta por silencio, transcribe cada trozo con Parakeet en un hilo
//! de fondo (el dictado tiene prioridad) y al parar pide un resumen estructurado a Ollama.
//!
//! Hilos de una reunión: un hilo por fuente (el `Stream` de cpal no es `Send`), uno que transcribe
//! la cola de trozos, y uno que emite el nivel. Al parar se cierran en ese orden y un hilo final
//! espera a que se vacíe la cola, genera el resumen y avisa a la interfaz.

use crate::audio::{self, OpenSource, SourceKind, WavStream, TARGET_RATE};
use crate::db;
use crate::models::*;
use crate::segment::{AudioSegment, Segmenter};
use crate::state::{self, AppState};
use crate::summary::{self, ParsedSummary};
use crate::text::{self, Options};
use crate::{auto, detect, ollama};
use crossbeam_channel::{unbounded, Receiver, RecvTimeoutError, Sender};
use parking_lot::Mutex;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

/// Contexto de Ollama que pedimos para los resúmenes (tokens).
const NUM_CTX: u32 = 8192;
/// Transcripciones más largas se resumen por trozos y luego en conjunto (caracteres).
const SUMMARY_DIRECT_CHARS: usize = 14_000;
const SUMMARY_CHUNK_CHARS: usize = 10_000;
/// Presupuesto de contexto de «Ask» (~6k tokens).
const ASK_BUDGET_CHARS: usize = 20_000;
/// Una reunión automática más corta que esto y con menos de 2 trozos se descarta sin resumen
/// (alguien abrió Teams para probar el micro).
const AUTO_MIN_DURATION_MS: i64 = 60_000;
const AUTO_MIN_SEGMENTS: usize = 2;
/// Si una fuente va más de este desfase por detrás del reloj, se rellena con silencio.
const GAP_FILL_SAMPLES: i64 = 3_200; // 200 ms a 16 kHz

/// Trozo de audio listo para transcribir.
struct Job {
    speaker: &'static str,
    seg: AudioSegment,
}

struct Active {
    meeting: Meeting,
    started: Instant,
    stop: Arc<AtomicBool>,
    sources: Vec<JoinHandle<()>>,
    transcriber: Option<JoinHandle<()>>,
    level: Option<JoinHandle<()>>,
    /// Emisor de la cola; se suelta al parar para que el transcriptor termine al vaciarla.
    jobs: Option<Sender<Job>>,
    /// Iniciada sola por el detector (no a mano).
    auto: bool,
}

#[derive(Default)]
pub struct MeetingManager {
    active: Mutex<Option<Active>>,
    flag: AtomicBool,
    /// Última app de reunión detectada (rellena `Meeting.app` al empezar).
    detected: Mutex<Option<String>>,
}

impl MeetingManager {
    pub fn is_active(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }

    pub fn set_detected(&self, app: Option<String>) {
        *self.detected.lock() = app;
    }

    /// Reunión en curso y tiempo transcurrido.
    pub fn status(&self) -> Option<(Meeting, i64)> {
        self.active.lock().as_ref().map(|a| (a.meeting.clone(), a.started.elapsed().as_millis() as i64))
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

// ------------------------------------------------------------------ empezar / parar

/// Empieza a tomar notas a mano. Devuelve la reunión creada (`summaryStatus: "none"`).
pub fn start(app: &AppHandle, st: &Arc<AppState>, title: Option<String>) -> Result<Meeting, String> {
    start_inner(app, st, title, false)
}

/// Empieza sola al detectar `app`: título «Reunión de {App} · HH:MM» (provisional: el resumen lo
/// sustituye por uno del contenido) y aviso en la Flow bar con el botón de parar.
pub fn start_auto(app: &AppHandle, st: &Arc<AppState>, label: &str) -> Result<Meeting, String> {
    st.meeting.set_detected(Some(label.to_string()));
    let title = auto::auto_title(label, &chrono::Local::now().format("%H:%M").to_string());
    let meeting = start_inner(app, st, Some(title), true)?;
    let _ = app.emit(
        "dictation://notice",
        Notice {
            title: format!("Tomando notas de {label}"),
            body: Some("Se pararán solas al colgar.".into()),
            action: Some("stop_meeting".into()),
        },
    );
    Ok(meeting)
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
    let (job_tx, job_rx) = unbounded::<Job>();
    let level_mic = Arc::new(AtomicU32::new(0));
    let level_sys = Arc::new(AtomicU32::new(0));

    let mut sources = Vec::new();
    let mut failures: Vec<String> = Vec::new();
    for (kind, speaker, level) in [(SourceKind::Mic, "me", level_mic.clone()), (SourceKind::System, "them", level_sys.clone())] {
        let (ready_tx, ready_rx) = crossbeam_channel::bounded::<Result<(), String>>(1);
        let ctx = SourceCtx {
            kind,
            speaker,
            t0: started,
            stop: stop.clone(),
            jobs: job_tx.clone(),
            level,
            mic: settings.mic_name.clone(),
            wav: settings.meeting_keep_audio.then(|| audio_file(meeting.id, speaker)).flatten(),
        };
        let spawned = std::thread::Builder::new().name(format!("pronto-meeting-{speaker}")).spawn(move || run_source(ctx, ready_tx));
        match spawned {
            Ok(h) => match ready_rx.recv_timeout(Duration::from_secs(8)) {
                Ok(Ok(())) => sources.push(h),
                Ok(Err(e)) => {
                    let _ = h.join();
                    failures.push(format!("{}: {e}", if kind == SourceKind::Mic { "micrófono" } else { "audio del sistema" }));
                }
                Err(_) => failures.push("una fuente de audio no respondió".into()),
            },
            Err(e) => failures.push(e.to_string()),
        }
    }
    if sources.is_empty() {
        stop.store(true, Ordering::SeqCst);
        let _ = st.db.delete_meeting(meeting.id);
        return Err(format!("No se pudo capturar audio ({})", failures.join("; ")));
    }
    for f in &failures {
        state::emit_notice(app, "Notas de reunión con una sola fuente", Some(f));
    }

    let transcriber = spawn_transcriber(app.clone(), st.clone(), job_rx, meeting.id);
    let level = spawn_level_emitter(app.clone(), stop.clone(), level_mic, level_sys);
    *guard = Some(Active { meeting: meeting.clone(), started, stop, sources, transcriber: Some(transcriber), level: Some(level), jobs: Some(job_tx), auto });
    st.meeting.flag.store(true, Ordering::SeqCst);
    drop(guard);

    emit_state(app, "recording", Some(meeting.id), None);
    if !st.active.load(Ordering::SeqCst) {
        state::emit_state(app, "meeting", None);
    }
    Ok(meeting)
}

/// Para la grabación, deja la reunión guardada y lanza el resumen en segundo plano.
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
    // Soltamos nuestro emisor: cuando los hilos de fuente (ya terminados) ya no tienen ninguno,
    // el transcriptor acaba tras vaciar la cola.
    drop(active.jobs.take());
    let duration_ms = active.started.elapsed().as_millis() as i64;
    let id = active.meeting.id;
    st.db.finish_meeting(id, duration_ms).map_err(|e| e.to_string())?;

    emit_state(app, "summarizing", Some(id), None);
    if !st.active.load(Ordering::SeqCst) {
        state::emit_state(app, "idle", None);
    }
    let transcriber = active.transcriber.take();
    let auto_started = active.auto;
    let (app2, st2) = (app.clone(), st.clone());
    let spawned = std::thread::Builder::new().name("pronto-summary".into()).spawn(move || {
        // La cola de trozos pendientes puede tardar unos segundos en vaciarse.
        if let Some(h) = transcriber {
            let _ = h.join();
        }
        if auto_started && too_short_to_keep(duration_ms, st2.db.segments(id).map(|s| s.len()).unwrap_or(0)) {
            discard(&app2, &st2, id);
            return;
        }
        summarize_and_save(&app2, &st2, id, auto_started);
    });
    if let Err(e) = spawned {
        eprintln!("[meeting] no se pudo lanzar el resumen: {e}");
    }
    st.db.get_meeting(id).map_err(|e| e.to_string())?.ok_or_else(|| "La reunión ya no existe".to_string())
}

/// Una reunión automática de menos de 60 s con menos de 2 trozos no merece resumen.
fn too_short_to_keep(duration_ms: i64, segments: usize) -> bool {
    duration_ms < AUTO_MIN_DURATION_MS && segments < AUTO_MIN_SEGMENTS
}

/// Borra una reunión descartada y avisa de que ya no hay nada pendiente.
fn discard(app: &AppHandle, st: &Arc<AppState>, id: i64) {
    if let Err(e) = st.db.delete_meeting(id) {
        eprintln!("[meeting] no se pudo descartar la reunión corta: {e}");
    }
    remove_audio_files(id);
    emit_state(app, "idle", Some(id), Some("Descartada: reunión demasiado corta".into()));
}

/// Atajo / aviso: empieza si no hay reunión y para si la hay. No bloquea al llamador.
pub fn toggle(app: &AppHandle, st: &Arc<AppState>) {
    let (app, st) = (app.clone(), st.clone());
    std::thread::spawn(move || {
        let result = if st.meeting.is_active() { stop(&app, &st).map(|_| ()) } else { start(&app, &st, None).map(|_| ()) };
        if let Err(e) = result {
            eprintln!("[meeting] {e}");
            state::emit_notice(&app, "Notas de reunión", Some(&e));
        }
    });
}

fn audio_file(meeting_id: i64, speaker: &str) -> Option<std::path::PathBuf> {
    Some(db::audio_dir()?.join(format!("meeting_{meeting_id}_{speaker}.wav")))
}

/// Borra los WAV guardados de una reunión.
pub fn remove_audio_files(meeting_id: i64) {
    for who in ["me", "them"] {
        if let Some(p) = audio_file(meeting_id, who) {
            let _ = std::fs::remove_file(p);
        }
    }
}

// ------------------------------------------------------------------ captura de una fuente

struct SourceCtx {
    kind: SourceKind,
    speaker: &'static str,
    t0: Instant,
    stop: Arc<AtomicBool>,
    jobs: Sender<Job>,
    level: Arc<AtomicU32>,
    mic: Option<String>,
    wav: Option<std::path::PathBuf>,
}

fn store_level(a: &AtomicU32, v: f32) {
    a.store(v.to_bits(), Ordering::Relaxed);
}

fn load_level(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Ordering::Relaxed))
}

fn run_source(ctx: SourceCtx, ready: Sender<Result<(), String>>) {
    let (tx, rx) = unbounded::<Vec<f32>>();
    let OpenSource { stream, rate } = match audio::open_source(ctx.kind, ctx.mic.as_deref(), tx) {
        Ok(s) => s,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    let _ = ready.send(Ok(()));

    let resampler = (rate != TARGET_RATE).then(|| sherpa_onnx::LinearResampler::create(rate as i32, TARGET_RATE as i32)).flatten();
    let origin = Instant::now();
    let offset_ms = ctx.t0.elapsed().as_millis() as i64;
    let mut seg = Segmenter::new(offset_ms);
    let mut wav = ctx.wav.as_deref().and_then(|p| match WavStream::create(p) {
        Ok(w) => Some(w),
        Err(e) => {
            eprintln!("[meeting] no se pudo crear {}: {e}", p.display());
            None
        }
    });
    // Muestras (16 kHz) entregadas al segmentador desde `origin`.
    let mut fed: i64 = 0;

    let mut feed = |samples: &[f32], seg: &mut Segmenter, fed: &mut i64| {
        *fed += samples.len() as i64;
        if let Some(w) = wav.as_mut() {
            if let Err(e) = w.write(samples) {
                eprintln!("[meeting] error escribiendo el audio: {e}");
                wav = None;
            }
        }
        for s in seg.push(samples) {
            let _ = ctx.jobs.send(Job { speaker: ctx.speaker, seg: s });
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
                        feed(&vec![0.0; behind as usize], &mut seg, &mut fed);
                    }
                    feed(&mono, &mut seg, &mut fed);
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                // Sin paquetes = silencio. La línea de tiempo sigue al reloj de pared.
                store_level(&ctx.level, load_level(&ctx.level) * 0.5);
                let behind = expected - fed;
                if behind > GAP_FILL_SAMPLES {
                    feed(&vec![0.0; behind as usize], &mut seg, &mut fed);
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
        feed(&mono, &mut seg, &mut fed);
    }
    if let Some(s) = seg.flush() {
        let _ = ctx.jobs.send(Job { speaker: ctx.speaker, seg: s });
    }
    if let Some(w) = wav.take() {
        if let Err(e) = w.finish() {
            eprintln!("[meeting] no se pudo cerrar el audio: {e}");
        }
    }
    store_level(&ctx.level, 0.0);
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
            eprintln!("[meeting] no se pudo lanzar el emisor de nivel: {e}");
            std::thread::spawn(|| {})
        })
}

// ------------------------------------------------------------------ transcripción de fondo

fn spawn_transcriber(app: AppHandle, st: Arc<AppState>, rx: Receiver<Job>, meeting_id: i64) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name("pronto-meeting-asr".into())
        .spawn(move || {
            // Termina cuando se sueltan todos los emisores y la cola queda vacía.
            for job in rx.iter() {
                transcribe_job(&app, &st, meeting_id, job);
            }
        })
        .unwrap_or_else(|e| {
            eprintln!("[meeting] no se pudo lanzar el transcriptor: {e}");
            std::thread::spawn(|| {})
        })
}

fn transcribe_job(app: &AppHandle, st: &Arc<AppState>, meeting_id: i64, job: Job) {
    // Micrófono: lo que apenas sobresale del ruido de fondo no se transcribe (chasquidos, soplidos).
    if job.speaker == "me" && job.seg.is_weak() {
        return;
    }
    // Si el modelo aún está cargando, esperamos en vez de perder el trozo.
    let t0 = Instant::now();
    while st.engine.status().loading && t0.elapsed() < Duration::from_secs(120) {
        std::thread::sleep(Duration::from_millis(500));
    }
    let raw = match st.engine.transcribe_background(&job.seg.samples) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("[meeting] no se pudo transcribir un trozo: {e}");
            return;
        }
    };
    let Some(text) = clean_segment_text(st, &raw) else { return };
    let segment = Segment { meeting_id, speaker: job.speaker.into(), start_ms: job.seg.start_ms, end_ms: job.seg.end_ms, text };
    if let Err(e) = st.db.add_segment(&segment) {
        eprintln!("[meeting] no se pudo guardar un trozo: {e}");
        return;
    }
    let _ = app.emit("meeting://segment", segment);
}

/// Limpieza ligera del texto de un trozo (muletillas y diccionario, con mayúscula y punto).
fn clean_segment_text(st: &Arc<AppState>, raw: &str) -> Option<String> {
    if is_junk_text(raw) {
        return None; // Parakeet a veces devuelve un punto o una letra suelta con ruido
    }
    let settings = st.settings.read().clone();
    let words = st.db.list_words().unwrap_or_default();
    let cleanup = if settings.cleanup_level == CleanupLevel::None { CleanupLevel::None } else { CleanupLevel::Light };
    let options = Options { cleanup, press_enter: false, tone: Tone::Formal, words: &words, snippets: &[] };
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

// ------------------------------------------------------------------ resumen y Ask

/// Pide el resumen a Ollama. Las reuniones largas se resumen por trozos y luego en conjunto.
pub fn generate_summary(url: &str, model: &str, segments: &[Segment], me: &str) -> Result<ParsedSummary, String> {
    if segments.is_empty() {
        return Err("La reunión no tiene transcripción (no se detectó conversación)".into());
    }
    let all_text = segments.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ");
    let lang = summary::detect_language(&all_text);
    let lines = summary::prompt_lines(segments, lang, me);
    let full = lines.join("\n");
    let (body, partial) = if full.len() <= SUMMARY_DIRECT_CHARS {
        (full, false)
    } else {
        let mut notes = Vec::new();
        for chunk in summary::chunk_lines(&lines, SUMMARY_CHUNK_CHARS) {
            notes.push(ollama::generate_raw(url, model, &summary::chunk_prompt(lang, &chunk), false, Some(NUM_CTX))?);
        }
        (notes.join("\n\n"), true)
    };
    let prompt = summary::summary_prompt(lang, &body, partial, me);
    // Los modelos pequeños a veces fallan el JSON a la primera: un segundo intento.
    let mut last = String::new();
    for _ in 0..2 {
        last = ollama::generate_raw(url, model, &prompt, true, Some(NUM_CTX))?;
        if let Some(p) = summary::parse_summary(&last) {
            return Ok(p);
        }
    }
    eprintln!("[meeting] respuesta de Ollama no interpretable: {last:?}");
    Err("Ollama devolvió un resumen que no se pudo leer".into())
}

/// Genera y guarda el resumen de `id`, y avisa a la interfaz del resultado.
pub fn summarize_and_save(app: &AppHandle, st: &Arc<AppState>, id: i64, notify: bool) {
    let _ = st.db.set_summary_status(id, "pending");
    emit_state(app, "summarizing", Some(id), None);
    let (url, model, me) = {
        let s = st.settings.read();
        let me = if s.first_name.trim().is_empty() { "el usuario".to_string() } else { s.first_name.trim().to_string() };
        (s.ollama_url.clone(), s.ollama_model.clone(), me)
    };
    let result = st
        .db
        .segments(id)
        .map_err(|e| e.to_string())
        .and_then(|segments| generate_summary(&url, &model, &segments, &me));
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
                emit_state(app, "done", Some(id), None);
            }
            Err(e) => fail_summary(app, st, id, e.to_string()),
        },
        Err(e) => fail_summary(app, st, id, e),
    }
}

fn fail_summary(app: &AppHandle, st: &Arc<AppState>, id: i64, message: String) {
    eprintln!("[meeting] resumen fallido: {message}");
    let _ = st.db.set_summary_status(id, "failed");
    if let Ok(Some(detail)) = st.db.get_meeting_detail(id) {
        let _ = app.emit("meeting://summary", detail);
    }
    emit_state(app, "error", Some(id), Some(message));
}

/// Reintenta el resumen en segundo plano.
pub fn retry_summary(app: &AppHandle, st: &Arc<AppState>, id: i64) -> Result<(), String> {
    if st.meeting.status().is_some_and(|(m, _)| m.id == id) {
        return Err("La reunión sigue en curso".into());
    }
    if st.db.get_meeting(id).map_err(|e| e.to_string())?.is_none() {
        return Err("Esa reunión ya no existe".into());
    }
    st.db.set_summary_status(id, "pending").map_err(|e| e.to_string())?;
    let (app, st) = (app.clone(), st.clone());
    std::thread::Builder::new()
        .name("pronto-summary".into())
        .spawn(move || summarize_and_save(&app, &st, id, false))
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
        meetings.push(summary::AskMeeting {
            title: d.meeting.title.clone(),
            date: summary::pretty_date(&d.meeting.started_at),
            summary: d.summary.clone(),
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
        loop {
            std::thread::sleep(Duration::from_secs(3));
            let mode = st.settings.read().meeting_auto;
            let apps = if mode == MeetingAuto::Off { Vec::new() } else { detect::read_mic_consent() };
            let titles = if detect::any_browser_in_use(&apps) { detect::window_titles() } else { Vec::new() };
            let in_use = detect::detect_all(&apps, &titles);
            st.meeting.set_detected(in_use.first().cloned());
            let now_ms = t0.elapsed().as_millis() as u64;
            match machine.step(now_ms, mode, st.meeting.is_active(), &in_use) {
                Some(auto::AutoAction::Start(label)) => {
                    if let Err(e) = start_auto(&app, &st, &label) {
                        eprintln!("[meeting] no se pudo empezar sola: {e}");
                    }
                }
                Some(auto::AutoAction::Stop) => {
                    if let Err(e) = stop(&app, &st) {
                        eprintln!("[meeting] no se pudo parar sola: {e}");
                    }
                }
                Some(auto::AutoAction::Notify(label)) => {
                    let _ = app.emit(
                        "dictation://notice",
                        Notice { title: "¿Tomar notas de esta reunión?".into(), body: Some(label), action: Some("start_meeting".into()) },
                    );
                }
                None => {}
            }
        }
    });
    if let Err(e) = spawned {
        eprintln!("[meeting] no se pudo lanzar la detección: {e}");
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

    #[test]
    fn reunion_automatica_demasiado_corta() {
        assert!(too_short_to_keep(30_000, 0));
        assert!(too_short_to_keep(59_999, 1));
        assert!(!too_short_to_keep(59_999, 2));
        assert!(!too_short_to_keep(60_000, 0));
        assert!(!too_short_to_keep(600_000, 1));
    }
}
