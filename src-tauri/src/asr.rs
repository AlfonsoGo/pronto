//! Motor de voz: Parakeet TDT 0.6B v3 int8 vía sherpa-onnx (offline, transductor NeMo) o, si el
//! usuario lo elige y el PC tiene GPU, Whisper large-v3-turbo (`whisper.rs`, Contrato v13).

use crate::models::EngineStatus;
use parking_lot::Mutex;
use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

const SAMPLE_RATE: i32 = 16_000;
/// Trozo máximo por inferencia: la atención del encoder crece con la duración.
const MAX_CHUNK_SECS: usize = 60;

/// Un token de Parakeet (trozo BPE; empieza por espacio si abre palabra) con su tramo en el audio,
/// en segundos desde el principio de lo transcrito. Lo usa la huella de voz (`voiceprint.rs`).
#[derive(Debug, Clone, PartialEq)]
pub struct TimedToken {
    pub text: String,
    pub start: f32,
    pub end: f32,
}

/// Transcripción con las marcas de tiempo de cada token.
#[derive(Debug, Clone, Default)]
pub struct Timed {
    pub text: String,
    pub tokens: Vec<TimedToken>,
}

/// Qué motor cargar: `parakeet` (de fábrica) o `whisper`, y en qué idioma dicta el usuario.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub kind: String,
    /// Id del modelo de Whisper (`whisper::MODELS`).
    pub model: String,
    pub language: String,
}

impl Choice {
    pub fn from_settings(s: &crate::models::Settings) -> Self {
        // Parakeet no usa el idioma: así cambiarlo no recarga el motor.
        let whisper = s.asr_engine == "whisper";
        Self {
            kind: s.asr_engine.clone(),
            model: if whisper { s.whisper_model.clone() } else { String::new() },
            language: if whisper { s.language.clone() } else { String::new() },
        }
    }

    fn whisper(&self) -> bool {
        self.kind == "whisper"
    }
}

impl Default for Choice {
    fn default() -> Self {
        Self { kind: "parakeet".into(), model: String::new(), language: "auto".into() }
    }
}

/// El reconocedor cargado.
enum Recognizer {
    Parakeet(OfflineRecognizer),
    Whisper(crate::whisper::Whisper, String),
}

impl Recognizer {
    fn max_chunk(&self) -> usize {
        match self {
            Recognizer::Parakeet(_) => MAX_CHUNK_SECS,
            Recognizer::Whisper(..) => crate::whisper::MAX_CHUNK_SECS,
        }
    }

    /// `precise`: tiempos por token finos (Whisper con DTW; Parakeet siempre los da).
    fn decode_timed(&self, samples: &[f32], precise: bool) -> Timed {
        match self {
            Recognizer::Parakeet(rec) => decode_timed(rec, samples),
            Recognizer::Whisper(w, lang) => w.decode(samples, lang, precise),
        }
    }
}

/// Una carga del modelo pedida y aún por hacer.
struct LoadJob {
    app: AppHandle,
    choice: Choice,
    model_dir: String,
    epoch: u64,
    on_ok: Box<dyn FnOnce() + Send>,
}

/// Cola de cargas de capacidad uno: una en marcha y, como mucho, una esperando (la última pedida).
struct LoadQueue<J> {
    running: bool,
    pending: Option<J>,
}

impl<J> Default for LoadQueue<J> {
    fn default() -> Self {
        Self { running: false, pending: None }
    }
}

impl<J> LoadQueue<J> {
    /// `Some(job)` = no hay otra carga: arranca ya. `None` = hay una en marcha; `job` queda en
    /// espera y sustituye a la que esperaba.
    fn submit(&mut self, job: J) -> Option<J> {
        if self.running {
            self.pending = Some(job);
            None
        } else {
            self.running = true;
            Some(job)
        }
    }

    /// La carga en marcha acabó: la que esperaba (si la hay) pasa a ser la que corre.
    fn finish(&mut self) -> Option<J> {
        self.pending.take().or_else(|| {
            self.running = false;
            None
        })
    }

    /// No se pudo lanzar el hilo de la carga que acababa de aceptarse.
    fn abandon(&mut self) {
        self.running = false;
        self.pending = None;
    }
}

pub struct Engine {
    recognizer: Mutex<Option<Arc<Recognizer>>>,
    choice: Mutex<Choice>,
    /// Texto previo de Whisper con las palabras del diccionario (`whisper::prompt_for`).
    whisper_prompt: Mutex<String>,
    status: Mutex<EngineStatus>,
    /// Número de carga actual; descarta cargas obsoletas si se cambia `modelDir` a mitad.
    generation: Mutex<u64>,
    /// Cargas del modelo: una sola a la vez, y de las que se piden mientras tanto solo queda la última
    /// (rompedor 22-oct-c, carga BAJA 1: N cambios de motor seguidos cargaban N modelos a la vez, +670 MB cada uno).
    loads: Mutex<LoadQueue<LoadJob>>,
    /// Una sola inferencia a la vez: dictado y reunión comparten el motor.
    busy: Mutex<()>,
    /// Cuándo acabó el último dictado (o pronunciación) con prioridad: el reintento largo del hub no
    /// retoma el motor justo detrás de uno, para no pillar al siguiente dictado a mitad de trozo.
    last_dictation_end: Mutex<Option<std::time::Instant>>,
    /// Dictados esperando turno; mientras haya alguno, la reunión cede el motor.
    dictation_waiting: AtomicUsize,
    /// Hay un dictado en marcha desde que se pulsa el atajo (`AppState.active`): la fase 2 de una
    /// reunión no empieza otro trozo mientras tanto, así el motor está libre al soltar.
    dictation_active: std::sync::OnceLock<Arc<std::sync::atomic::AtomicBool>>,
    /// Tope de esa espera, por si la marca se quedara puesta: la reunión nunca se queda sin motor.
    hold_cap: Duration,
}

/// Un dictado dura como mucho 20 min (`state::MAX_RECORDING`); un minuto más de margen.
const HOLD_CAP: Duration = Duration::from_secs(21 * 60);

impl Engine {
    pub fn new(model_dir: &str) -> Self {
        Self {
            recognizer: Mutex::new(None),
            choice: Mutex::new(Choice::default()),
            whisper_prompt: Mutex::new(crate::whisper::prompt_for(&[])),
            status: Mutex::new(EngineStatus { ready: false, loading: false, model_dir: model_dir.into(), error: None, active: String::new() }),
            generation: Mutex::new(0),
            loads: Mutex::new(LoadQueue::default()),
            busy: Mutex::new(()),
            last_dictation_end: Mutex::new(None),
            dictation_waiting: AtomicUsize::new(0),
            dictation_active: std::sync::OnceLock::new(),
            hold_cap: HOLD_CAP,
        }
    }

    /// Comparte la marca de «dictado en marcha» (`AppState.active`, puesta al pulsar el atajo).
    pub fn share_dictation_flag(&self, flag: Arc<std::sync::atomic::AtomicBool>) {
        let _ = self.dictation_active.set(flag);
    }

    /// Espera (con tope) a que no haya ningún dictado grabando.
    fn wait_dictation_released(&self) {
        let Some(flag) = self.dictation_active.get() else { return };
        let t0 = std::time::Instant::now();
        while flag.load(Ordering::SeqCst) && t0.elapsed() < self.hold_cap {
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    pub fn status(&self) -> EngineStatus {
        self.status.lock().clone()
    }

    /// Motor ya cargado, sin `AppHandle` (pruebas con el modelo real).
    #[cfg(test)]
    pub fn load_for_tests(model_dir: &str) -> Result<Self, String> {
        Self::load_for_tests_threads(model_dir, dictation_threads())
    }

    /// Como `load_for_tests`, con `threads` hilos (medidas de la fase 2 de Meets).
    #[cfg(test)]
    pub fn load_for_tests_threads(model_dir: &str, threads: usize) -> Result<Self, String> {
        let engine = Self::new(model_dir);
        *engine.recognizer.lock() = Some(Arc::new(Recognizer::Parakeet(create_recognizer(model_dir, threads)?)));
        engine.status.lock().ready = true;
        Ok(engine)
    }

    /// Whisper ya cargado, sin `AppHandle` (pruebas con el modelo real).
    #[cfg(test)]
    pub fn load_whisper_for_tests() -> Result<Self, String> {
        let engine = Self::new("");
        let w = crate::whisper::Whisper::load(crate::whisper::model(""), true)?;
        *engine.recognizer.lock() = Some(Arc::new(Recognizer::Whisper(w, "es".into())));
        engine.status.lock().ready = true;
        Ok(engine)
    }

    fn set_status(&self, app: &AppHandle, s: EngineStatus) {
        *self.status.lock() = s.clone();
        let _ = app.emit("engine://status", s);
    }

    /// Motor elegido ahora.
    pub fn choice(&self) -> Choice {
        self.choice.lock().clone()
    }

    /// Palabras del diccionario para Whisper (las más importantes primero); Parakeet no las usa.
    pub fn set_vocabulary(&self, words: &[String]) {
        let prompt = crate::whisper::prompt_for(words);
        let mut cur = self.whisper_prompt.lock();
        if *cur == prompt {
            return;
        }
        *cur = prompt.clone();
        drop(cur);
        if let Some(rec) = self.recognizer.lock().clone() {
            if let Recognizer::Whisper(w, _) = &*rec {
                w.set_prompt(prompt);
            }
        }
    }

    /// Cambia el motor elegido; hay que llamar a `load` después para que se note.
    pub fn set_choice(&self, choice: Choice) {
        *self.choice.lock() = choice;
    }

    /// Carga (o recarga) el modelo en un hilo aparte y emite `engine://status`. Con Whisper elegido
    /// pero sin modelo o sin GPU válida, carga Parakeet: Pronto nunca se queda sin dictado por eso.
    pub fn load(self: &Arc<Self>, app: &AppHandle, model_dir: String) {
        self.load_then(app, model_dir, || {});
    }

    /// Como `load`; `on_ok` se llama solo si esta carga termina bien (y no la ha sustituido otra).
    /// Así una carpeta del modelo nueva se guarda en los ajustes solo después de cargar.
    ///
    /// Solo corre una carga a la vez. Si se pide otra mientras hay una en marcha, queda en espera y
    /// arranca al acabar la actual; si se piden varias, solo cuenta la última.
    pub fn load_then(self: &Arc<Self>, app: &AppHandle, model_dir: String, on_ok: impl FnOnce() + Send + 'static) {
        let choice = self.choice();
        let epoch = {
            let mut g = self.generation.lock();
            *g += 1;
            *g
        };
        // El motor que hubiera sigue dictando mientras carga el nuevo (Whisper tarda de 2 a 40 s
        // en cargar y calentar); se cambia de golpe cuando el nuevo está listo.
        self.set_status(app, EngineStatus { ready: false, loading: true, model_dir: model_dir.clone(), error: None, active: String::new() });

        let job = LoadJob { app: app.clone(), choice, model_dir, epoch, on_ok: Box::new(on_ok) };
        let Some(job) = self.loads.lock().submit(job) else { return }; // en espera tras la que corre
        let engine = self.clone();
        let spawned = std::thread::Builder::new().name("pronto-asr-load".into()).spawn(move || {
            let mut next = Some(job);
            while let Some(job) = next {
                let engine2 = engine.clone();
                if std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || engine2.run_load(job))).is_err() {
                    eprintln!("[asr] la carga del motor entró en pánico");
                }
                next = engine.loads.lock().finish();
            }
        });
        if let Err(e) = spawned {
            eprintln!("[asr] no se pudo lanzar el hilo de carga: {e}");
            self.loads.lock().abandon();
        }
    }

    /// Una carga del modelo, en el hilo `pronto-asr-load`.
    fn run_load(self: &Arc<Self>, job: LoadJob) {
        let LoadJob { app, choice, model_dir, epoch, on_ok } = job;
        let engine = self;
        let started = std::time::Instant::now();
        let result = std::panic::catch_unwind(|| create_any(&choice, &model_dir)).unwrap_or_else(|_| Err("el motor de voz falló al cargar".to_string()));
        if *engine.generation.lock() != epoch {
            return; // otra carga más reciente manda
        }
        match result {
            Ok(r) => {
                eprintln!("[asr] modelo cargado en {:.1}s", started.elapsed().as_secs_f32());
                let active = match &r {
                    Recognizer::Parakeet(_) => "parakeet",
                    Recognizer::Whisper(w, _) => {
                        w.set_prompt(engine.whisper_prompt.lock().clone());
                        "whisper"
                    }
                };
                let r = Arc::new(r);
                *engine.recognizer.lock() = Some(r.clone());
                engine.set_status(&app, EngineStatus { ready: true, loading: false, model_dir, error: None, active: active.into() });
                on_ok();
                // Ya se puede dictar; el contexto con DTW de Whisper (reuniones, huella de voz) se
                // carga detrás, en este mismo hilo.
                if let Recognizer::Whisper(w, _) = &*r {
                    w.preload_precise();
                }
            }
            Err(e) => {
                eprintln!("[asr] {e}");
                // El motor que ya hubiera sigue dictando: se dice el error sin darlo por parado.
                let active = match engine.recognizer.lock().as_deref() {
                    Some(Recognizer::Parakeet(_)) => "parakeet",
                    Some(Recognizer::Whisper(..)) => "whisper",
                    None => "",
                };
                engine.set_status(&app, EngineStatus { ready: !active.is_empty(), loading: false, model_dir, error: Some(e), active: active.into() });
            }
        }
    }

    /// Transcribe audio mono a 16 kHz con **prioridad** (dictado). Bloquea; llamar siempre fuera
    /// del hilo del hook.
    pub fn transcribe(&self, samples: &[f32]) -> Result<String, String> {
        self.transcribe_timed(samples).map(|t| t.text)
    }

    /// Como `transcribe`, con las marcas de tiempo de los tokens.
    pub fn transcribe_timed(&self, samples: &[f32]) -> Result<Timed, String> {
        self.dictation_waiting.fetch_add(1, Ordering::SeqCst);
        let turn = self.busy.lock();
        self.dictation_waiting.fetch_sub(1, Ordering::SeqCst);
        self.transcribe_locked(samples, turn, Kind::Dictation)
    }

    /// Como `transcribe_timed`, con tiempos por token finos aunque cueste más (Whisper con DTW): para la
    /// huella de voz cuando oye una palabra del diccionario (`state::transcribe_processed_as`).
    pub fn transcribe_timed_precise(&self, samples: &[f32]) -> Result<Timed, String> {
        self.dictation_waiting.fetch_add(1, Ordering::SeqCst);
        let turn = self.busy.lock();
        self.dictation_waiting.fetch_sub(1, Ordering::SeqCst);
        self.transcribe_locked(samples, turn, Kind::Precise)
    }

    /// Latido de la GPU mientras se graba un dictado con Whisper (`Whisper::pulse`), para que al soltar
    /// el atajo no esté a medio reloj: un latido cada `PULSE_EVERY` hasta que se suelta el `Warmth`
    /// (al acabar la grabación) o pasan `PULSE_CAP`. `None` con Parakeet o sin camino rápido.
    pub fn keep_warm(&self) -> Option<Warmth> {
        let rec = self.recognizer.lock().clone()?;
        if !matches!(&*rec, Recognizer::Whisper(w, _) if w.rough_timings()) {
            return None;
        }
        let on = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let flag = on.clone();
        std::thread::Builder::new()
            .name("pronto-latido".into())
            .spawn(move || {
                let Recognizer::Whisper(w, _) = &*rec else { return };
                let t0 = std::time::Instant::now();
                while flag.load(Ordering::SeqCst) && t0.elapsed() < PULSE_CAP {
                    w.pulse();
                    // Espera en pasos cortos: al soltar el atajo no debe empezar otro latido.
                    let wait = std::time::Instant::now();
                    while flag.load(Ordering::SeqCst) && wait.elapsed() < PULSE_EVERY {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
            })
            .map_err(|e| eprintln!("[asr] sin latido: {e}"))
            .ok()?;
        Some(Warmth(on))
    }

    /// ¿Los tiempos por token de `transcribe_timed` son aproximados? Solo con Whisper por su camino
    /// rápido (atención flash, sin DTW).
    pub fn rough_timings(&self) -> bool {
        matches!(self.recognizer.lock().as_deref(), Some(Recognizer::Whisper(w, _)) if w.rough_timings())
    }

    /// Transcripción de fondo (fase 2 de Meets), con las marcas de tiempo de los tokens: espera a
    /// que no haya ningún dictado grabando, esperando ni en marcha. Como no empieza un trozo mientras
    /// se graba un dictado, el trozo que estuviera a medias suele acabar antes de soltar el atajo y
    /// el dictado no espera (rompedor 22-oct, MEDIA 5: con trozos de 30 s esperaba hasta 1,8 s).
    pub fn transcribe_background_timed(&self, samples: &[f32]) -> Result<Timed, String> {
        let turn = self.background_turn(Duration::ZERO);
        self.transcribe_locked(samples, turn, Kind::Background)
    }

    /// Reintento de un dictado largo desde el hub («Reintentar transcripción»): empieza como un
    /// dictado (con prioridad) pero suelta el motor entre trozos de 60 s y, para coger el siguiente,
    /// cede ante cualquier dictado esperando o grabando, como la fase 2, y espera además unos
    /// segundos tras el último dictado. Antes cogía `busy` para todo el audio: con 20 min un dictado
    /// pulsado durante el reintento esperaba 90 s (rompedor 22-oct-c, carga MEDIA 1). Lo peor que
    /// espera ahora un dictado es lo que quede del trozo en curso.
    pub fn transcribe_timed_polite(&self, samples: &[f32]) -> Result<Timed, String> {
        self.dictation_waiting.fetch_add(1, Ordering::SeqCst);
        let turn = self.busy.lock();
        self.dictation_waiting.fetch_sub(1, Ordering::SeqCst);
        self.transcribe_locked(samples, turn, Kind::Retry)
    }

    /// Como `transcribe_timed_polite`, con tiempos por token finos: la segunda pasada de un reintento
    /// en el que la huella oye una palabra. Por `transcribe_timed_precise` cogía `busy` para todo el
    /// audio y deshacía el arreglo de arriba (seguridad 24-oct, MEDIA 1). Empieza cediendo, como la fase 2.
    pub fn transcribe_timed_polite_precise(&self, samples: &[f32]) -> Result<Timed, String> {
        let turn = self.background_turn(RETRY_GRACE);
        self.transcribe_locked(samples, turn, Kind::RetryPrecise)
    }

    /// Turno del motor para quien no tiene prisa: espera a que no haya un dictado grabando ni
    /// esperando (ni, con `grace`, acabado hace menos de ese tiempo), y cede si llega uno mientras
    /// se esperaba el cerrojo.
    fn background_turn(&self, grace: Duration) -> parking_lot::MutexGuard<'_, ()> {
        let calm = || self.dictation_waiting.load(Ordering::SeqCst) == 0 && self.last_dictation_end.lock().is_none_or(|t| t.elapsed() >= grace);
        loop {
            self.wait_dictation_released();
            while !calm() {
                std::thread::sleep(Duration::from_millis(20));
            }
            let turn = self.busy.lock();
            // Si mientras esperábamos el cerrojo llegó un dictado (o acaba de acabar uno), se lo cedemos.
            if !calm() {
                drop(turn);
                continue;
            }
            return turn;
        }
    }

    fn transcribe_locked<'a>(&'a self, samples: &[f32], mut turn: parking_lot::MutexGuard<'a, ()>, kind: Kind) -> Result<Timed, String> {
        let rec = self.recognizer.lock().clone().ok_or_else(|| {
            let st = self.status();
            if st.loading {
                "El motor de voz aún se está cargando".to_string()
            } else {
                st.error.unwrap_or_else(|| "El motor de voz no está listo".to_string())
            }
        })?;
        let mut parts: Vec<String> = Vec::new();
        let mut tokens = Vec::new();
        for (i, chunk) in split_long(samples, rec.max_chunk() * SAMPLE_RATE as usize).into_iter().enumerate() {
            if matches!(kind, Kind::Retry | Kind::RetryPrecise) && i > 0 {
                // Entre trozos: soltar el motor y volver a pedirlo cediendo a los dictados.
                drop(turn);
                turn = self.background_turn(RETRY_GRACE);
            }
            // Los trozos son subrebanadas de `samples`: su desplazamiento sale de los punteros.
            let offset = (chunk.as_ptr() as usize - samples.as_ptr() as usize) / std::mem::size_of::<f32>();
            // Meets y la huella de voz necesitan los tiempos por token finos; el dictado, rapidez.
            let precise = matches!(kind, Kind::Background | Kind::Precise | Kind::RetryPrecise);
            let t = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| rec.decode_timed(chunk, precise)))
                .map_err(|_| "el motor de voz falló al transcribir".to_string())?;
            if !t.text.is_empty() {
                parts.push(t.text);
            }
            let shift = offset as f32 / SAMPLE_RATE as f32;
            tokens.extend(t.tokens.into_iter().map(|k| TimedToken { start: k.start + shift, end: k.end + shift, ..k }));
        }
        if matches!(kind, Kind::Dictation | Kind::Precise) {
            // Antes de soltar el motor, para que el reintento que espera vea la marca.
            *self.last_dictation_end.lock() = Some(std::time::Instant::now());
        }
        Ok(Timed { text: parts.join(" "), tokens })
    }
}

/// Quién transcribe: un dictado (o pronunciación) con prioridad, el mismo dictado repetido con
/// tiempos finos para la huella de voz, un reintento del hub que suelta el motor entre trozos, o la
/// fase 2 de una reunión.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Dictation,
    Precise,
    Retry,
    RetryPrecise,
    Background,
}

/// Pausa entre latidos de la GPU mientras se graba (`Engine::keep_warm`; medido: 150 ms deja el
/// dictado como con la GPU caliente, 300 ms a 40 ms de eso).
const PULSE_EVERY: Duration = Duration::from_millis(200);
/// Tope de latidos por dictado: en uno de más de 2 minutos, 0,1 s más al final no se nota.
const PULSE_CAP: Duration = Duration::from_secs(120);

/// Mientras vive, la GPU recibe latidos (`Engine::keep_warm`); al soltarlo, paran.
pub struct Warmth(Arc<std::sync::atomic::AtomicBool>);

impl Drop for Warmth {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// Tras un dictado, el reintento del hub espera esto antes de retomar el motor: un dictado seguido de
/// otro (dos frases) no se topa con un trozo de 60 s empezado entre medias.
const RETRY_GRACE: Duration = Duration::from_secs(3);

#[cfg(test)]
fn decode(rec: &OfflineRecognizer, samples: &[f32]) -> String {
    decode_timed(rec, samples).text
}

fn decode_timed(rec: &OfflineRecognizer, samples: &[f32]) -> Timed {
    // Un cuarto de segundo de silencio al final ayuda al último token con audios cortos.
    let mut padded = Vec::with_capacity(samples.len() + 4000);
    padded.extend_from_slice(samples);
    padded.extend(std::iter::repeat(0.0).take(4000));
    let stream = rec.create_stream();
    stream.accept_waveform(SAMPLE_RATE, &padded);
    rec.decode(&stream);
    let Some(r) = stream.get_result() else { return Timed::default() };
    Timed { text: r.text.trim().to_string(), tokens: timed_tokens(r.tokens, r.timestamps, r.durations) }
}

/// Junta tokens, inicios y duraciones de sherpa-onnx. Sin duraciones (otros modelos), cada token
/// dura hasta el inicio del siguiente (el último, 80 ms: una trama del encoder de Parakeet).
fn timed_tokens(tokens: Vec<String>, starts: Option<Vec<f32>>, durations: Option<Vec<f32>>) -> Vec<TimedToken> {
    let Some(starts) = starts.filter(|s| s.len() == tokens.len()) else { return Vec::new() };
    let durations = durations.filter(|d| d.len() == tokens.len());
    tokens
        .into_iter()
        .enumerate()
        .map(|(i, text)| {
            let start = starts[i];
            let dur = match &durations {
                Some(d) if d[i] > 0.0 => d[i],
                _ => starts.get(i + 1).map(|n| n - start).filter(|d| *d > 0.0).unwrap_or(0.08),
            };
            TimedToken { text, start, end: start + dur }
        })
        .collect()
}

/// El motor elegido. Whisper solo si su modelo está y el PC vale (`whisper::availability`); si no,
/// Parakeet, que funciona en cualquier PC.
fn create_any(choice: &Choice, model_dir: &str) -> Result<Recognizer, String> {
    if choice.whisper() {
        let m = crate::whisper::model(&choice.model);
        let ready = if !m.present() { Err(format!("falta el modelo {}", m.name)) } else { m.availability() };
        match ready.and_then(|_| crate::whisper::Whisper::load(m, true)) {
            Ok(w) => {
                let lang = if choice.language.is_empty() { "auto".to_string() } else { choice.language.clone() };
                return Ok(Recognizer::Whisper(w, lang));
            }
            Err(e) => eprintln!("[asr] Whisper no disponible ({e}); se usa Parakeet"),
        }
    }
    create_recognizer(model_dir, dictation_threads()).map(Recognizer::Parakeet)
}

/// Hilos del reconocedor del dictado: con más de 4 un dictado corto no va más rápido.
fn dictation_threads() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2).min(4)
}

fn create_recognizer(model_dir: &str, threads: usize) -> Result<OfflineRecognizer, String> {
    let dir = Path::new(model_dir);
    // sherpa-onnx no devuelve error ante un modelo dañado: hace `exit(-1)` o lanza una excepción C++
    // que aborta el proceso (`catch_unwind` no lo para), y Pronto moriría en cada arranque. Solo se
    // le da un modelo entero y verificado (rompedor 22-oct, ALTA 2).
    crate::modeldl::verify(dir, &crate::modeldl::FILES)
        .map_err(|e| format!("El modelo de voz no se puede usar: {e}. Descárgalo de nuevo desde Configuración → Motor."))?;
    let file = |name: &str| -> Result<String, String> {
        let p = dir.join(name);
        if p.is_file() {
            Ok(native_path(&p))
        } else {
            Err(format!("Falta {} en {}", name, dir.display()))
        }
    };
    let mut config = OfflineRecognizerConfig::default();
    config.model_config.transducer.encoder = Some(file("encoder.int8.onnx")?);
    config.model_config.transducer.decoder = Some(file("decoder.int8.onnx")?);
    config.model_config.transducer.joiner = Some(file("joiner.int8.onnx")?);
    config.model_config.tokens = Some(file("tokens.txt")?);
    config.model_config.model_type = Some("nemo_transducer".into());
    config.model_config.num_threads = threads.max(1) as i32;
    config.decoding_method = Some("greedy_search".into());
    OfflineRecognizer::create(&config).ok_or_else(|| "No se pudo crear el reconocedor (¿modelo dañado?)".to_string())
}

/// Ruta que sherpa-onnx puede abrir. Rust admite rutas de más de 259 caracteres (MAX_PATH) y por eso
/// `verify` las aprueba, pero sherpa-onnx no: con una carpeta de modelo tan honda el hub decía «listo»
/// y no se podía dictar (rompedor 22-oct-c, descargas BAJA 6). Con el prefijo `\\?\` (ruta absoluta y
/// normalizada, que con el prefijo Windows ya no normaliza) sí la abre.
pub(crate) fn native_path(p: &Path) -> String {
    let s = p.to_string_lossy().into_owned();
    if s.encode_utf16().count() < LONG_PATH || s.starts_with(r"\\?\") {
        return s;
    }
    let abs = std::path::absolute(p).map(|a| a.to_string_lossy().into_owned()).unwrap_or(s);
    if abs.starts_with(r"\\?\") {
        abs
    } else if let Some(unc) = abs.strip_prefix(r"\\") {
        format!(r"\\?\UNC\{unc}")
    } else {
        format!(r"\\?\{abs}")
    }
}

/// A partir de aquí (en unidades UTF-16) una ruta lleva `\\?\` (MAX_PATH = 260 con el nulo final; algo
/// de margen por si la librería añade algo).
const LONG_PATH: usize = 240;

/// Parte audio largo en trozos de como mucho `max` muestras, cortando en el punto más silencioso
/// de los últimos 6 s de cada trozo para no partir palabras.
fn split_long(samples: &[f32], max: usize) -> Vec<&[f32]> {
    if samples.len() <= max {
        return vec![samples];
    }
    let window = 1600; // 100 ms
    let search = 6 * SAMPLE_RATE as usize;
    let mut out = Vec::new();
    let mut start = 0;
    while samples.len() - start > max {
        let end = start + max;
        let from = end - search;
        let mut best = end;
        let mut best_e = f32::MAX;
        let mut i = from;
        while i + window <= end {
            let e: f32 = samples[i..i + window].iter().map(|s| s * s).sum();
            if e < best_e {
                best_e = e;
                best = i + window / 2;
            }
            i += window / 2;
        }
        out.push(&samples[start..best]);
        start = best;
    }
    out.push(&samples[start..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rutas_largas_llevan_el_prefijo_para_sherpa() {
        let corta = Path::new(r"C:\Users\a\AppData\Roaming\Pronto2\models\parakeet\tokens.txt");
        assert_eq!(native_path(corta), corta.to_string_lossy());
        let honda = format!(r"C:\x\{}\tokens.txt", "carpeta-muy-larga\\".repeat(16));
        let p = native_path(Path::new(&honda));
        assert!(p.starts_with(r"\\?\C:\x\carpeta-muy-larga\") && p.ends_with(r"\tokens.txt"), "{p}");
        assert!(!p.contains(r"\\carpeta") && !p[4..].contains(r"\\"), "normalizada: {p}");
        let unc = format!(r"\\servidor\recurso\{}\tokens.txt", "x".repeat(250));
        assert!(native_path(Path::new(&unc)).starts_with(r"\\?\UNC\servidor\recurso\"));
        let ya = format!(r"\\?\C:\{}\tokens.txt", "y".repeat(260));
        assert_eq!(native_path(Path::new(&ya)), ya);
    }

    /// Rompedor 22-oct-c, carga BAJA 1: una sola carga a la vez; de las pedidas mientras tanto, la última.
    #[test]
    fn las_cargas_del_motor_van_de_una_en_una_y_gana_la_ultima() {
        let mut q = LoadQueue::<u32>::default();
        assert_eq!(q.submit(1), Some(1), "la primera arranca");
        assert_eq!(q.submit(2), None, "con una en marcha, las demás esperan");
        assert_eq!(q.submit(3), None);
        assert_eq!(q.submit(4), None);
        assert_eq!(q.finish(), Some(4), "al acabar la 1, arranca solo la última pedida");
        assert_eq!(q.submit(5), None, "la 4 corre: la 5 espera");
        assert_eq!(q.finish(), Some(5));
        assert_eq!(q.finish(), None, "sin nadie esperando, queda libre");
        assert_eq!(q.submit(6), Some(6), "y la siguiente vuelve a arrancar ya");
        q.abandon();
        assert_eq!(q.submit(7), Some(7), "si el hilo no pudo lanzarse, no se queda bloqueada");
    }

    #[test]
    fn trozos_cubren_todo_el_audio() {
        let n = 16_000 * 150;
        let audio: Vec<f32> = (0..n).map(|i| if (i / 8000) % 7 == 0 { 0.0 } else { 0.3 }).collect();
        let chunks = split_long(&audio, 16_000 * 60);
        assert!(chunks.len() >= 3);
        assert_eq!(chunks.iter().map(|c| c.len()).sum::<usize>(), n);
        assert!(chunks.iter().all(|c| c.len() <= 16_000 * 60));
    }

    #[test]
    fn audio_corto_va_entero() {
        let audio = vec![0.1f32; 1000];
        assert_eq!(split_long(&audio, 16_000 * 60).len(), 1);
    }

    /// Rompedor 22-oct, MEDIA 5: la fase 2 no empieza un trozo mientras se graba un dictado (así el
    /// motor está libre al soltar), y una marca atascada no la deja sin motor para siempre.
    #[test]
    fn fase_2_no_empieza_un_trozo_mientras_se_graba_un_dictado() {
        use std::sync::atomic::AtomicBool;
        let flag = Arc::new(AtomicBool::new(true));
        let mut e = Engine::new("");
        e.hold_cap = Duration::from_secs(60);
        e.share_dictation_flag(flag.clone());
        let e = Arc::new(e);
        let (tx, rx) = std::sync::mpsc::channel();
        let e2 = e.clone();
        std::thread::spawn(move || {
            let _ = e2.transcribe_background_timed(&[0.0; 10]);
            let _ = tx.send(());
        });
        assert!(rx.recv_timeout(Duration::from_millis(400)).is_err(), "empezó un trozo de la reunión con un dictado grabando");
        flag.store(false, Ordering::SeqCst);
        rx.recv_timeout(Duration::from_secs(2)).expect("al soltar el atajo, la fase 2 sigue");

        let mut atascada = Engine::new("");
        atascada.hold_cap = Duration::from_millis(200);
        atascada.share_dictation_flag(Arc::new(AtomicBool::new(true)));
        let t0 = std::time::Instant::now();
        let _ = atascada.transcribe_background_timed(&[0.0; 10]);
        assert!(t0.elapsed() < Duration::from_secs(2), "una marca atascada dejó la reunión sin motor");
    }
}

#[cfg(test)]
mod model_smoke {
    use super::*;

    /// Carga el modelo real y decodifica silencio. `cargo test -- --ignored` (necesita el modelo).
    #[test]
    #[ignore]
    fn carga_y_decodifica_silencio() {
        let dir = default_model_dir();
        let rec = create_recognizer(&dir, dictation_threads()).expect("cargar modelo");
        let text = decode(&rec, &vec![0.0f32; 16_000]);
        eprintln!("texto de silencio: {text:?}");
    }

    /// Transcribe los WAV de `PRONTO_WAV` (rutas separadas por `;`) y los pasa por la tubería de
    /// texto. `PRONTO_WAV=a.wav;b.wav cargo test transcribe_wav -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn transcribe_wav() {
        let Ok(paths) = std::env::var("PRONTO_WAV") else { return };
        let rec = create_recognizer(&default_model_dir(), dictation_threads()).expect("cargar modelo");
        transcribe_paths(&rec, &paths);
    }

    /// Descarga el modelo de verdad (~655 MB) a `PRONTO_DL_DIR`, verifica los SHA-256, lo carga y
    /// transcribe los WAV de `PRONTO_WAV`. Nunca toca la carpeta del modelo de Pronto 1.
    /// `PRONTO_DL_DIR=... PRONTO_WAV=a.wav;b.wav cargo test descarga_real -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn descarga_real() {
        let Ok(dir) = std::env::var("PRONTO_DL_DIR") else { return };
        let dir = std::path::PathBuf::from(dir);
        assert!(!dir.starts_with(dirs::data_dir().unwrap().join("Pronto")), "no se descarga sobre Pronto 1");
        let t0 = std::time::Instant::now();
        let mut last = 0;
        crate::modeldl::download_all(&dir, crate::modeldl::BASE_URL, &crate::modeldl::FILES, &mut |p| {
            if p.overall_percent / 10 != last / 10 || p.state != "downloading" {
                eprintln!("[dl] {} {} {}% ({}/{} B)", p.state, p.file, p.overall_percent, p.done_bytes, p.total_bytes);
            }
            last = p.overall_percent;
        })
        .expect("descargar");
        eprintln!("[dl] descarga y verificación: {:.1}s", t0.elapsed().as_secs_f32());
        for f in &crate::modeldl::FILES {
            let hash = crate::modeldl::hash_file(&dir.join(f.name)).expect("leer");
            eprintln!("[dl] {} sha256 {hash} {}", f.name, if hash == f.sha256 { "OK" } else { "DISTINTO" });
            assert_eq!(hash, f.sha256);
        }
        assert!(crate::modeldl::is_complete(&dir, &crate::modeldl::FILES));
        assert!(!dir.read_dir().unwrap().flatten().any(|e| e.file_name().to_string_lossy().ends_with(".part")));
        let rec = create_recognizer(&dir.to_string_lossy(), dictation_threads()).expect("cargar el modelo descargado");
        transcribe_paths(&rec, &std::env::var("PRONTO_WAV").unwrap_or_default());
    }

    /// Medida de hilos para la fase 2 de Meets: trocea `PRONTO_MEETS_WAV` como la fase 2 y lo
    /// transcribe con cada número de hilos de `PRONTO_THREADS` (por defecto 4,8,12), uno a uno y por
    /// lotes; mide también un dictado de 5 s.
    /// `PRONTO_MEETS_WAV=me.wav cargo test --release hilos_fase_2 -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn hilos_fase_2() {
        let Ok(path) = std::env::var("PRONTO_MEETS_WAV") else { return };
        let mut reader = crate::audio::WavReader::open(std::path::Path::new(&path)).expect("leer wav");
        let mut seg = crate::segment::Segmenter::with_config(0, crate::segment::FINAL);
        let mut chunks = Vec::new();
        let mut total = 0usize;
        loop {
            let b = reader.read_block(160_000).unwrap();
            if b.is_empty() {
                break;
            }
            total += b.len();
            chunks.extend(seg.push(&b));
        }
        chunks.extend(seg.flush());
        let audio_s = total as f32 / 16_000.0;
        let dict: Vec<f32> = chunks.iter().flat_map(|c| c.samples.iter().copied()).take(16_000 * 5).collect();
        let threads = std::env::var("PRONTO_THREADS").unwrap_or_else(|_| "4,8,12".into());
        for n in threads.split(',').filter_map(|t| t.trim().parse::<usize>().ok()) {
            let t0 = std::time::Instant::now();
            let rec = create_recognizer(&default_model_dir(), n).expect("cargar modelo");
            let load = t0.elapsed().as_secs_f32();
            let t1 = std::time::Instant::now();
            for c in &chunks {
                let _ = decode_timed(&rec, &c.samples);
            }
            let one = t1.elapsed().as_secs_f32();
            let t2 = std::time::Instant::now();
            for batch in chunks.chunks(4) {
                let streams: Vec<_> = batch.iter().map(|c| {
                    let s = rec.create_stream();
                    let mut padded = c.samples.clone();
                    padded.extend(std::iter::repeat(0.0).take(4000));
                    s.accept_waveform(SAMPLE_RATE, &padded);
                    s
                }).collect();
                rec.decode_multiple_streams(&streams.iter().collect::<Vec<_>>());
            }
            let batched = t2.elapsed().as_secs_f32();
            let mut d = Vec::new();
            for _ in 0..3 {
                let t = std::time::Instant::now();
                let _ = decode_timed(&rec, &dict);
                d.push(t.elapsed().as_millis());
            }
            d.sort();
            // Varios reconocedores en paralelo (uno por hilo de trabajo), repartiéndose los trozos.
            let workers: usize = std::env::var("PRONTO_WORKERS").ok().and_then(|w| w.parse().ok()).unwrap_or(1);
            if workers > 1 {
                let recs: Vec<OfflineRecognizer> = (0..workers).map(|_| create_recognizer(&default_model_dir(), n).expect("cargar modelo")).collect();
                let next = std::sync::atomic::AtomicUsize::new(0);
                let t3 = std::time::Instant::now();
                std::thread::scope(|sc| {
                    for r in &recs {
                        let (next, chunks) = (&next, &chunks);
                        sc.spawn(move || loop {
                            let i = next.fetch_add(1, Ordering::SeqCst);
                            let Some(c) = chunks.get(i) else { break };
                            let _ = decode_timed(r, &c.samples);
                        });
                    }
                });
                let par = t3.elapsed().as_secs_f32();
                eprintln!("{n:>2} hilos x {workers} reconocedores: {audio_s:.0} s de audio en {par:.1} s ({:.1}x)", audio_s / par);
            }
            eprintln!(
                "{n:>2} hilos: carga {load:.1} s · {audio_s:.0} s de audio en {one:.1} s ({:.1}x) · por lotes de 4 {batched:.1} s ({:.1}x) · dictado de 5 s {} ms",
                audio_s / one,
                audio_s / batched,
                d[1]
            );
        }
    }

    fn transcribe_paths(rec: &OfflineRecognizer, paths: &str) {
        for p in paths.split(';').filter(|p| !p.is_empty()) {
            let wave = sherpa_onnx::Wave::read(p).expect("leer wav");
            assert_eq!(wave.sample_rate(), 16_000, "{p}: el wav debe ir a 16 kHz");
            let t0 = std::time::Instant::now();
            let raw = decode(rec, wave.samples());
            let ms = t0.elapsed().as_millis();
            let opts = crate::text::Options {
                cleanup: crate::models::CleanupLevel::Medium,
                press_enter: false,
                tone: crate::models::Tone::Formal,
                words: &[],
                snippets: &[],
            };
            let done = crate::text::process(&raw, &opts);
            eprintln!("\n{p}\n  crudo ({ms} ms): {raw}\n  final: {}", done.text);
        }
    }

    fn default_model_dir() -> String {
        crate::models::Settings::default().model_dir
    }

    /// Banco del dictado con Whisper como en la app: por cada WAV de `PRONTO_WAVS`, «se habla» lo que
    /// dura el audio (con o sin latido, `Engine::keep_warm`, turnándose), se suelta y se mide hasta el
    /// texto (`transcribe_timed`). Tras cada dictado, `PRONTO_REPOSO_MS` de reposo (3 s por defecto).
    /// Idioma `PRONTO_LANG` («auto», el de Txonso). Imprime `COMP <con|sin>	<id>	<ms,…>	<texto>`.
    #[test]
    #[ignore]
    fn banco_latido() {
        let Ok(wavs) = std::env::var("PRONTO_WAVS") else { return };
        let runs: usize = std::env::var("PRONTO_RUNS").ok().and_then(|r| r.parse().ok()).unwrap_or(3);
        let rest = Duration::from_millis(std::env::var("PRONTO_REPOSO_MS").ok().and_then(|r| r.parse().ok()).unwrap_or(3000));
        let lang = std::env::var("PRONTO_LANG").unwrap_or_else(|_| "auto".into());
        let engine = Engine::new("");
        let w = crate::whisper::Whisper::load(crate::whisper::model(""), true).expect("cargar Whisper");
        w.preload_precise();
        *engine.recognizer.lock() = Some(Arc::new(Recognizer::Whisper(w, lang)));
        engine.set_vocabulary(&[]);
        for path in wavs.split(';').filter(|p| !p.is_empty()) {
            let audio = crate::audio::decode_wav(&std::fs::read(path).unwrap()).expect("wav");
            let id = std::path::Path::new(path).file_stem().unwrap().to_string_lossy().into_owned();
            let speak = Duration::from_secs_f32(audio.len() as f32 / 16_000.0);
            let mut ms = [Vec::new(), Vec::new()];
            let mut text = [String::new(), String::new()];
            for r in 0..runs * 2 {
                let warm = r % 2 == 0;
                std::thread::sleep(rest);
                let warmth = if warm { engine.keep_warm() } else { None };
                assert_eq!(warm, warmth.is_some(), "el latido no arrancó");
                std::thread::sleep(speak);
                drop(warmth);
                let t = std::time::Instant::now();
                text[warm as usize] = engine.transcribe_timed(&audio).unwrap().text;
                ms[warm as usize].push(format!("{:.0}", t.elapsed().as_secs_f64() * 1000.0));
            }
            for (i, name) in ["sin", "con"].iter().enumerate() {
                println!("COMP {name}	{id}	{}	{}", ms[i].join(","), text[i]);
            }
        }
    }

    /// Banco de velocidad de Parakeet sobre el juez (como `whisper::tests::banco_dictado` con Whisper).
    #[test]
    #[ignore]
    fn banco_juez_parakeet() {
        let Ok(wavs) = std::env::var("PRONTO_WAVS") else { return };
        let runs: usize = std::env::var("PRONTO_RUNS").ok().and_then(|r| r.parse().ok()).unwrap_or(5);
        let engine = Engine::load_for_tests(&default_model_dir()).expect("cargar Parakeet");
        for path in wavs.split(';') {
            let audio = crate::audio::decode_wav(&std::fs::read(path).unwrap()).expect("wav");
            let id = std::path::Path::new(path).file_stem().unwrap().to_string_lossy().into_owned();
            let _ = engine.transcribe_timed(&audio);
            let mut ms = Vec::new();
            let mut text = String::new();
            for _ in 0..runs {
                let t = std::time::Instant::now();
                text = engine.transcribe_timed(&audio).unwrap().text;
                ms.push(format!("{:.0}", t.elapsed().as_secs_f64() * 1000.0));
            }
            println!("JUEZ {id}\t{}\t{text}", ms.join(","));
        }
    }
}

/// Regla 3 de CLAUDE.md (velocidad). La lanza `tests/no-se-negocia/03-velocidad.test.mjs` con
/// `PRONTO_NSN_WAV` (WAV de ~15 s, 16 kHz mono) y `PRONTO_NSN_MODELO` (carpeta de Parakeet, solo
/// lectura). Sin ellas no mide nada (así `cargo test` a secas no depende del modelo). Imprime los
/// tiempos para que el test de Node compare la mediana con la línea base.
#[cfg(test)]
mod nsn_velocidad {
    use super::*;
    use std::time::Instant;

    #[test]
    fn nsn_velocidad_parakeet_dictado_15s() {
        let (Ok(wav), Ok(dir)) = (std::env::var("PRONTO_NSN_WAV"), std::env::var("PRONTO_NSN_MODELO")) else {
            println!("NSN_OMITIDO: faltan PRONTO_NSN_WAV y PRONTO_NSN_MODELO");
            return;
        };
        let runs: usize = std::env::var("PRONTO_NSN_RUNS").ok().and_then(|v| v.parse().ok()).unwrap_or(5);
        let samples = crate::audio::decode_wav(&std::fs::read(&wav).expect("leer el WAV")).expect("WAV PCM16 mono 16 kHz");
        println!("NSN_AUDIO_S={:.2}", samples.len() as f64 / SAMPLE_RATE as f64);
        let engine = Engine::load_for_tests(&dir).expect("cargar Parakeet");
        engine.set_vocabulary(&[]);
        // Calentamiento: la primera pasada carga pesos en caché (en la app ocurre al arrancar).
        let _ = engine.transcribe_timed(&samples).expect("transcribir");
        let mut asr = Vec::new();
        let mut post = Vec::new();
        let mut last = String::new();
        let mut crudo = String::new();
        let settings = crate::models::Settings::default();
        for _ in 0..runs {
            let t0 = Instant::now();
            let timed = engine.transcribe_timed(&samples).expect("transcribir");
            asr.push(t0.elapsed().as_secs_f64() * 1000.0);
            // Resto de la tubería hasta el texto que se pega: huella de voz + limpieza/estilo.
            let t1 = Instant::now();
            let (heard, _) = crate::voiceprint::correct(&[], &samples, &timed);
            let opts = crate::text::Options { cleanup: settings.cleanup_level, press_enter: settings.press_enter_command, tone: settings.styles.work, words: &[], snippets: &[] };
            let done = crate::text::process(&heard, &opts);
            post.push(t1.elapsed().as_secs_f64() * 1000.0);
            last = done.text;
            crudo = timed.text;
        }
        let fmt = |v: &[f64]| v.iter().map(|x| format!("{x:.1}")).collect::<Vec<_>>().join(",");
        println!("NSN_TRANSCRIPCION_MS={}", fmt(&asr));
        println!("NSN_TUBERIA_MS={}", fmt(&post));
        println!("NSN_CRUDO={crudo}");
        println!("NSN_TEXTO={last}");
        assert!(last.split_whitespace().count() >= 10, "la transcripción salió casi vacía: {last:?}");
    }
}
