//! Motor de voz: Parakeet TDT 0.6B v3 int8 vía sherpa-onnx (offline, transductor NeMo).

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

pub struct Engine {
    recognizer: Mutex<Option<Arc<OfflineRecognizer>>>,
    status: Mutex<EngineStatus>,
    /// Número de carga actual; descarta cargas obsoletas si se cambia `modelDir` a mitad.
    generation: Mutex<u64>,
    /// Una sola inferencia a la vez: dictado y reunión comparten el motor.
    busy: Mutex<()>,
    /// Dictados esperando turno; mientras haya alguno, la reunión cede el motor.
    dictation_waiting: AtomicUsize,
}

impl Engine {
    pub fn new(model_dir: &str) -> Self {
        Self {
            recognizer: Mutex::new(None),
            status: Mutex::new(EngineStatus { ready: false, loading: false, model_dir: model_dir.into(), error: None }),
            generation: Mutex::new(0),
            busy: Mutex::new(()),
            dictation_waiting: AtomicUsize::new(0),
        }
    }

    pub fn status(&self) -> EngineStatus {
        self.status.lock().clone()
    }

    fn set_status(&self, app: &AppHandle, s: EngineStatus) {
        *self.status.lock() = s.clone();
        let _ = app.emit("engine://status", s);
    }

    /// Carga (o recarga) el modelo en un hilo aparte y emite `engine://status`.
    pub fn load(self: &Arc<Self>, app: &AppHandle, model_dir: String) {
        let epoch = {
            let mut g = self.generation.lock();
            *g += 1;
            *g
        };
        *self.recognizer.lock() = None;
        self.set_status(app, EngineStatus { ready: false, loading: true, model_dir: model_dir.clone(), error: None });

        let engine = self.clone();
        let app = app.clone();
        let spawned = std::thread::Builder::new().name("pronto-asr-load".into()).spawn(move || {
            let started = std::time::Instant::now();
            let result = std::panic::catch_unwind(|| create_recognizer(&model_dir))
                .unwrap_or_else(|_| Err("el motor de voz falló al cargar".to_string()));
            if *engine.generation.lock() != epoch {
                return; // otra carga más reciente manda
            }
            match result {
                Ok(r) => {
                    eprintln!("[asr] modelo cargado en {:.1}s", started.elapsed().as_secs_f32());
                    *engine.recognizer.lock() = Some(Arc::new(r));
                    engine.set_status(&app, EngineStatus { ready: true, loading: false, model_dir, error: None });
                }
                Err(e) => {
                    eprintln!("[asr] {e}");
                    engine.set_status(&app, EngineStatus { ready: false, loading: false, model_dir, error: Some(e) });
                }
            }
        });
        if let Err(e) = spawned {
            eprintln!("[asr] no se pudo lanzar el hilo de carga: {e}");
        }
    }

    /// Transcribe audio mono a 16 kHz con **prioridad** (dictado). Bloquea; llamar siempre fuera
    /// del hilo del hook.
    pub fn transcribe(&self, samples: &[f32]) -> Result<String, String> {
        self.dictation_waiting.fetch_add(1, Ordering::SeqCst);
        let _turn = self.busy.lock();
        self.dictation_waiting.fetch_sub(1, Ordering::SeqCst);
        self.transcribe_locked(samples)
    }

    /// Transcripción de fondo (reuniones): espera a que no haya ningún dictado esperando ni en
    /// marcha, así que un dictado nunca se retrasa más que el trozo de reunión que esté a medias.
    pub fn transcribe_background(&self, samples: &[f32]) -> Result<String, String> {
        loop {
            while self.dictation_waiting.load(Ordering::SeqCst) > 0 {
                std::thread::sleep(Duration::from_millis(20));
            }
            let turn = self.busy.lock();
            // Si mientras esperábamos el cerrojo llegó un dictado, se lo cedemos.
            if self.dictation_waiting.load(Ordering::SeqCst) > 0 {
                drop(turn);
                continue;
            }
            return self.transcribe_locked(samples);
        }
    }

    fn transcribe_locked(&self, samples: &[f32]) -> Result<String, String> {
        let rec = self.recognizer.lock().clone().ok_or_else(|| {
            let st = self.status();
            if st.loading {
                "El motor de voz aún se está cargando".to_string()
            } else {
                st.error.unwrap_or_else(|| "El motor de voz no está listo".to_string())
            }
        })?;
        let mut parts: Vec<String> = Vec::new();
        for chunk in split_long(samples, MAX_CHUNK_SECS * SAMPLE_RATE as usize) {
            let text = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| decode(&rec, chunk)))
                .map_err(|_| "el motor de voz falló al transcribir".to_string())?;
            if !text.is_empty() {
                parts.push(text);
            }
        }
        Ok(parts.join(" "))
    }
}

fn decode(rec: &OfflineRecognizer, samples: &[f32]) -> String {
    // Un cuarto de segundo de silencio al final ayuda al último token con audios cortos.
    let mut padded = Vec::with_capacity(samples.len() + 4000);
    padded.extend_from_slice(samples);
    padded.extend(std::iter::repeat(0.0).take(4000));
    let stream = rec.create_stream();
    stream.accept_waveform(SAMPLE_RATE, &padded);
    rec.decode(&stream);
    stream.get_result().map(|r| r.text.trim().to_string()).unwrap_or_default()
}

fn create_recognizer(model_dir: &str) -> Result<OfflineRecognizer, String> {
    let dir = Path::new(model_dir);
    let file = |name: &str| -> Result<String, String> {
        let p = dir.join(name);
        if p.is_file() {
            Ok(p.to_string_lossy().into_owned())
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
    config.model_config.num_threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2).min(4) as i32;
    config.decoding_method = Some("greedy_search".into());
    OfflineRecognizer::create(&config).ok_or_else(|| "No se pudo crear el reconocedor (¿modelo dañado?)".to_string())
}

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
}

#[cfg(test)]
mod model_smoke {
    use super::*;

    /// Carga el modelo real y decodifica silencio. `cargo test -- --ignored` (necesita el modelo).
    #[test]
    #[ignore]
    fn carga_y_decodifica_silencio() {
        let dir = default_model_dir();
        let rec = create_recognizer(&dir).expect("cargar modelo");
        let text = decode(&rec, &vec![0.0f32; 16_000]);
        eprintln!("texto de silencio: {text:?}");
    }

    /// Transcribe los WAV de `PRONTO_WAV` (rutas separadas por `;`) y los pasa por la tubería de
    /// texto. `PRONTO_WAV=a.wav;b.wav cargo test transcribe_wav -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn transcribe_wav() {
        let Ok(paths) = std::env::var("PRONTO_WAV") else { return };
        let rec = create_recognizer(&default_model_dir()).expect("cargar modelo");
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
        let rec = create_recognizer(&dir.to_string_lossy()).expect("cargar el modelo descargado");
        transcribe_paths(&rec, &std::env::var("PRONTO_WAV").unwrap_or_default());
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
}
