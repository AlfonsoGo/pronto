//! Diccionario por voz: graba unos segundos del micrófono mientras el usuario dice una palabra,
//! la transcribe con el motor y devuelve qué oye Pronto («chonso» para «Txonso»). Lo oído se
//! guarda como variante (`word_variants`) y la tubería de texto lo corrige a la palabra.
//!
//! Una sola grabación a la vez, y nunca con un dictado o una reunión en marcha. Se para sola tras
//! un silencio después de hablar, o a los 5 s; `stop` la para antes y `stop(cancel)` la descarta.
//!
//! Huella de voz (Contrato v11): la grabación, recortada a la voz, se deja además como muestra
//! pendiente en `%APPDATA%\Pronto2oz\pendiente-*.wav`; la UI la engancha a la palabra al guardar.

use crate::audio::{LevelFn, Recorder, TARGET_RATE};
use crate::models::Pronunciation;
use crate::state::AppState;
use crate::{db, modeldl, text, voiceprint};
use crossbeam_channel::{bounded, RecvTimeoutError, Sender};
use parking_lot::{const_mutex, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

/// Tope de la grabación: una palabra o un nombre caben de sobra.
const MAX_MS: u64 = 5000;
/// Silencio tras haber hablado que da la palabra por terminada.
const TAIL_SILENCE_MS: u64 = 900;
/// Voz SEGUIDA mínima para fiarse de que ya ha hablado. Antes se sumaban ruidos sueltos (el clic de
/// «Grabar», una respiración, una tecla) y la grabación se paraba antes de que Txonso hablara:
/// el motor solo recibía ruido y salía «No se ha entendido nada».
const MIN_VOICE_MS: u64 = 200;
/// Lo que se mide al principio para estimar el ruido de fondo.
const FLOOR_MS: u64 = 300;
/// Por debajo de este pico el micrófono está mudo (el mismo umbral que el dictado).
const SILENCE_PEAK: f32 = 0.0005;

static BUSY: AtomicBool = AtomicBool::new(false);
/// Canal para parar la grabación en curso: `true` = descartar.
static STOP: Mutex<Option<Sender<bool>>> = const_mutex(None);

/// Libera `BUSY` pase lo que pase (error, pánico, cancelación).
struct BusyGuard;

impl Drop for BusyGuard {
    fn drop(&mut self) {
        *STOP.lock() = None;
        BUSY.store(false, Ordering::SeqCst);
    }
}

/// Para la grabación en curso (si la hay). `cancel` la descarta sin transcribir.
pub fn stop(cancel: bool) {
    if let Some(tx) = STOP.lock().as_ref() {
        let _ = tx.try_send(cancel);
    }
}

fn emit_state(app: &AppHandle, state: &str) {
    let _ = app.emit("pronounce://state", serde_json::json!({ "state": state }));
}

/// Decide cuándo parar a partir del nivel 0..1 de `audio::rms_to_level` (~30 medidas por segundo).
/// El umbral de voz se mide sobre el ruido de fondo de los primeros 250 ms, para que valga igual
/// con un micro de mesa ruidoso que con unos auriculares.
#[derive(Debug, Default)]
struct AutoStop {
    floor: Option<f32>,
    floor_min: f32,
    run_ms: u64,
    armed: bool,
    last_voice_ms: Option<u64>,
    prev_ms: u64,
}

impl AutoStop {
    fn new() -> Self {
        Self { floor_min: 1.0, ..Self::default() }
    }

    /// `t_ms` desde el inicio; `level` la última medida. Devuelve `true` cuando hay que parar.
    fn feed(&mut self, t_ms: u64, level: f32) -> bool {
        let dt = t_ms.saturating_sub(self.prev_ms);
        self.prev_ms = t_ms;
        if t_ms >= MAX_MS {
            return true;
        }
        if self.floor.is_none() {
            self.floor_min = self.floor_min.min(level);
            if t_ms < FLOOR_MS {
                return false;
            }
            self.floor = Some(self.floor_min);
        }
        let floor = self.floor.unwrap_or(0.0);
        // ~7 dB por encima del fondo, y nunca por debajo de un susurro cercano (0,22 ≈ −45 dB: vale
        // para micros flojos, que en el PC de Txonso dan picos de 0,02-0,13).
        if level >= (floor + 0.15).max(0.22) {
            self.run_ms += dt;
            if self.run_ms >= MIN_VOICE_MS {
                self.armed = true;
            }
            self.last_voice_ms = Some(t_ms);
        } else {
            self.run_ms = 0;
        }
        match self.last_voice_ms {
            Some(last) if self.armed => t_ms - last >= TAIL_SILENCE_MS,
            _ => false,
        }
    }
}

/// Graba, transcribe y da el veredicto para `word` (y su `replacement`). Bloquea hasta ~4 s más
/// la transcripción: llamar fuera del hilo principal.
pub fn record(app: &AppHandle, st: &Arc<AppState>, word: &str, replacement: Option<&str>) -> Result<Pronunciation, String> {
    if word.trim().is_empty() {
        return Err("Escribe primero la palabra".into());
    }
    let settings = st.settings.read().clone();
    let engine = st.engine.status();
    if !engine.ready {
        return Err(if !modeldl::model_present(&settings.model_dir) {
            "Falta el modelo de voz: descárgalo en Configuración para poder grabar la pronunciación".to_string()
        } else if engine.loading {
            "El motor de voz aún se está cargando. Prueba otra vez en unos segundos".to_string()
        } else {
            format!("El motor de voz no está listo: {}", engine.error.unwrap_or_else(|| "error desconocido".into()))
        });
    }
    if st.active.load(Ordering::SeqCst) {
        return Err("Hay un dictado en curso. Termínalo antes de grabar la pronunciación".into());
    }
    if st.meeting.is_active() {
        return Err("Se está grabando una reunión. Párala antes de grabar la pronunciación".into());
    }
    if BUSY.swap(true, Ordering::SeqCst) {
        return Err("Ya se está grabando una pronunciación".into());
    }
    let _guard = BusyGuard;
    let (stop_tx, stop_rx) = bounded::<bool>(1);
    *STOP.lock() = Some(stop_tx);

    // La última medida de nivel, en bits de f32 (NaN = aún no ha llegado ninguna).
    let level = Arc::new(AtomicU32::new(f32::NAN.to_bits()));
    let on_level: LevelFn = {
        let (level, app) = (level.clone(), app.clone());
        Arc::new(move |l| {
            level.store(l.to_bits(), Ordering::Relaxed);
            let _ = app.emit("pronounce://level", serde_json::json!({ "level": l }));
        })
    };
    let rec = Recorder::start(settings.mic_name.clone(), on_level)?;
    emit_state(app, "recording");
    let started = Instant::now();
    let mut auto = AutoStop::new();
    let mut cancelled = false;
    loop {
        match stop_rx.recv_timeout(Duration::from_millis(30)) {
            Ok(cancel) => {
                cancelled = cancel;
                break;
            }
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }
        let l = f32::from_bits(level.load(Ordering::Relaxed));
        let t = started.elapsed().as_millis() as u64;
        if (!l.is_nan() && auto.feed(t, l)) || t >= MAX_MS + 1000 {
            break;
        }
    }
    let samples = rec.stop();
    if cancelled {
        emit_state(app, "idle");
        return Err("Grabación cancelada".into());
    }
    let duration_ms = samples.len() as i64 * 1000 / TARGET_RATE as i64;
    let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    eprintln!("[pronounce] {duration_ms} ms, pico {peak:.4}");
    if peak < SILENCE_PEAK {
        emit_state(app, "idle");
        return Err("No llega sonido del micrófono. ¿Lo tienes silenciado?".into());
    }
    emit_state(app, "transcribing");
    let heard = st.engine.transcribe(&boost(&samples));
    emit_state(app, "idle");
    let mut p = verdict(word, replacement, &heard?, duration_ms)?;
    if let Some((file, ms)) = save_pending_sample(&samples) {
        (p.sample, p.sample_ms) = (Some(file), ms);
    }
    Ok(p)
}

/// Pendientes de más de un día: grabaciones que nunca se guardaron con su palabra (se canceló el
/// diálogo).
const PENDING_MAX_AGE: Duration = Duration::from_secs(24 * 3600);

/// Guarda la grabación recortada a la voz como muestra pendiente. Devuelve (fichero, duración en
/// ms), o `None` si no hay un tramo de voz con duración de palabra.
fn save_pending_sample(samples: &[f32]) -> Option<(String, i64)> {
    let dir = db::voice_dir()?;
    purge_pending(&dir);
    let (from, to) = voiceprint::voice_range(samples)?;
    let voice = &samples[from..to];
    voiceprint::Template::from_audio(voice)?;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let file = format!("{}{stamp}.wav", db::PENDING_PREFIX);
    if let Err(e) = std::fs::write(dir.join(&file), crate::audio::encode_wav(voice)) {
        eprintln!("[pronounce] no se pudo guardar la muestra de voz: {e}");
        return None;
    }
    Some((file, voice.len() as i64 * 1000 / TARGET_RATE as i64))
}

fn purge_pending(dir: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age > PENDING_MAX_AGE);
        if name.starts_with(db::PENDING_PREFIX) && old {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// Sube el volumen de una grabación floja (pico < 0,5) hasta 0,5, como mucho ×20: con una palabra
/// suelta dicha bajito Parakeet devuelve a menudo texto vacío.
fn boost(samples: &[f32]) -> Vec<f32> {
    let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if peak <= 0.0 || peak >= 0.5 {
        return samples.to_vec();
    }
    let gain = (0.5 / peak).min(20.0);
    samples.iter().map(|s| s * gain).collect()
}

/// Lo oído -> resultado para la UI. Error si el motor no entendió nada.
fn verdict(word: &str, replacement: Option<&str>, heard: &str, duration_ms: i64) -> Result<Pronunciation, String> {
    let status = text::pronounce_status(word, replacement, heard);
    if status == "empty" {
        return Err("No se ha entendido nada. Prueba otra vez, más cerca del micrófono".into());
    }
    let variant = text::normalize_heard(heard);
    Ok(Pronunciation {
        heard: heard.trim().to_string(),
        common: status == "new" && text::is_common_phrase(&variant),
        variant,
        status,
        duration_ms,
        sample: None,
        sample_ms: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav_samples(path: &std::path::Path) -> Vec<f32> {
        let mut r = crate::audio::WavReader::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(r.rate(), TARGET_RATE, "{}", path.display());
        let mut out = Vec::new();
        loop {
            let b = r.read_block(16_000).unwrap();
            if b.is_empty() {
                return out;
            }
            out.extend(b);
        }
    }

    /// Diccionario de punta a punta con el motor real, sin micrófono: para cada `w_<palabra>.wav`
    /// (la palabra suelta) de `PRONTO_DICT_DIR`, «Grabar pronunciación» → veredicto → variante
    /// guardada; y la frase `s_<palabra>.wav`, dictada después, tiene que salir con la palabra bien.
    /// `PRONTO_DICT_DIR=<carpeta> cargo test diccionario_real -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn diccionario_real_de_punta_a_punta() {
        let Ok(dir) = std::env::var("PRONTO_DICT_DIR") else { return };
        let dir = std::path::PathBuf::from(dir);
        let model = dirs::data_dir().unwrap().join("Pronto2").join("models").join("parakeet");
        let engine = crate::asr::Engine::load_for_tests(&model.to_string_lossy()).expect("modelo de Pronto 2");
        let db = crate::db::Db::open_memory().unwrap();
        let cases = [("Txonso", "txonso"), ("Ollama", "ollama")];
        for (word, file) in cases {
            let heard = engine.transcribe(&wav_samples(&dir.join(format!("w_{file}.wav")))).unwrap();
            let p = verdict(word, None, &heard, 0).unwrap();
            eprintln!("{word}: Pronto oye «{heard}» → {} ({})", p.variant, p.status);
            let variants = if p.status == "new" { vec![p.variant.clone()] } else { Vec::new() };
            db.add_word(word, None, &variants).unwrap();
        }
        let words = db.list_words().unwrap();
        let o = text::Options { cleanup: crate::models::CleanupLevel::Medium, press_enter: false, tone: crate::models::Tone::Formal, words: &words, snippets: &[] };
        for (word, file) in cases {
            let raw = engine.transcribe(&wav_samples(&dir.join(format!("s_{file}.wav")))).unwrap();
            let done = text::process(&raw, &o);
            eprintln!("frase: «{raw}» → «{}»", done.text);
            // Ollama suelta se oye «Oyama» y en frase «Yama»: con una sola grabación no basta (hay que
            // grabarla otra vez). Txonso sí tiene que salir bien.
            if word == "Txonso" {
                assert!(done.text.contains(word), "«{word}» no aparece en «{}»", done.text);
            }
        }
    }

    /// Simula medidas cada 33 ms: `voice(t)` dice el nivel en cada instante.
    fn stops_at(voice: impl Fn(u64) -> f32) -> u64 {
        let mut a = AutoStop::new();
        let mut t = 0;
        while !a.feed(t, voice(t)) {
            t += 33;
        }
        t
    }

    #[test]
    fn para_tras_el_silencio_que_sigue_a_la_palabra() {
        // Fondo 0,1; habla de 0,5 s a 1,2 s.
        let t = stops_at(|t| if (500..1200).contains(&t) { 0.6 } else { 0.1 });
        assert!((1200 + TAIL_SILENCE_MS..1200 + TAIL_SILENCE_MS + 100).contains(&t), "paró a los {t} ms");
    }

    #[test]
    fn sin_voz_agota_el_tope() {
        assert_eq!(stops_at(|_| 0.1), MAX_MS + 33 - MAX_MS % 33);
        // Un chasquido suelto (una medida) no cuenta como haber hablado.
        let t = stops_at(|t| if t == 990 { 0.9 } else { 0.1 });
        assert!(t >= MAX_MS);
    }

    #[test]
    fn umbral_relativo_al_ruido_de_fondo() {
        // Micro ruidoso (fondo 0,4): 0,45 es ruido, no voz; 0,7 sí es voz.
        assert!(stops_at(|_| 0.45) >= MAX_MS);
        let t = stops_at(|t| if (300..900).contains(&t) { 0.7 } else { 0.4 });
        assert!(t < MAX_MS, "paró a los {t} ms");
    }

    #[test]
    fn ruidos_sueltos_antes_de_hablar_no_paran_la_grabacion() {
        // Fondo 0,1; clic de «Grabar», una respiración y una tecla; habla de 1,6 s a 2,3 s.
        let t = stops_at(|t| match t {
            40..=90 => 0.9,
            600..=680 => 0.6,
            1000..=1090 => 0.6,
            1600..=2300 => 0.5,
            _ => 0.1,
        });
        assert!(t >= 2300 + TAIL_SILENCE_MS && t < 2300 + TAIL_SILENCE_MS + 100, "paró a los {t} ms");
    }

    #[test]
    fn micro_flojo_tambien_cuenta_como_voz() {
        // Fondo 0,05 y voz a 0,28 (≈ −42 dB): antes el mínimo de 0,3 no la veía.
        let t = stops_at(|t| if (700..1400).contains(&t) { 0.28 } else { 0.05 });
        assert!(t < MAX_MS, "paró a los {t} ms");
    }

    #[test]
    fn sube_el_volumen_de_una_grabacion_floja() {
        let quiet = vec![0.0, 0.02, -0.05, 0.01];
        let b = boost(&quiet);
        assert!((b[2] + 0.5).abs() < 1e-6, "{b:?}");
        assert_eq!(boost(&[0.0, 0.8, -0.3]), vec![0.0, 0.8, -0.3], "una grabación fuerte no se toca");
        assert!((boost(&[0.0, 0.001])[1] - 0.02).abs() < 1e-6, "como mucho ×20");
    }

    #[test]
    fn deja_la_muestra_de_voz_recortada() {
        // 0,6 s de silencio, 0,5 s de «voz» y 0,8 s de silencio: queda la voz con su margen.
        let mut a = vec![0.0005f32; 9600];
        a.extend((0..8000).map(|i| 0.3 * (i as f32 * 0.07).sin() * (1.0 + 0.5 * (i as f32 * 0.003).sin())));
        a.extend(vec![0.0005f32; 12800]);
        let (file, ms) = save_pending_sample(&a).expect("muestra");
        assert!(file.starts_with(db::PENDING_PREFIX), "{file}");
        assert!((500..=650).contains(&ms), "{ms} ms");
        let path = db::voice_dir().unwrap().join(&file);
        assert!(path.starts_with(std::env::temp_dir()));
        let back = crate::audio::decode_wav(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(back.len() as i64 * 1000 / TARGET_RATE as i64, ms);
        let _ = std::fs::remove_file(path);
        // Sin voz (o un chasquido de 50 ms) no hay muestra.
        assert!(save_pending_sample(&vec![0.0005f32; 16000]).is_none());
        let mut click = vec![0.0005f32; 16000];
        click[8000..8800].iter_mut().for_each(|v| *v = 0.5);
        assert!(save_pending_sample(&click).is_none());
    }

    #[test]
    fn veredicto() {
        let p = verdict("Txonso", None, "Chonso.", 1200).unwrap();
        assert_eq!((p.variant.as_str(), p.status, p.common), ("Chonso", "new", false));
        assert_eq!(verdict("Txonso", None, "Txonso.", 900).unwrap().status, "ok");
        assert_eq!(verdict("Wispr", None, "casa", 900).unwrap().common, true);
        assert!(verdict("Txonso", None, " . ", 900).is_err());
    }
}
