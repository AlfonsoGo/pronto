//! Segundo motor de voz (Contrato v13): OpenAI Whisper large-v3-turbo vía whisper.cpp (`whisper-rs`),
//! con la GPU por Vulkan (NVIDIA, AMD e Intel) o, si no hay, en CPU.
//!
//! Medido el 17-oct-2026 (`docs/asr-comparativa.md`): turbo comete un 40 % menos de errores que
//! Parakeet (WER 4,4 % frente a 7,0 %) y acierta nombres («Beltran Labs», «subagentes»), pero solo
//! compensa con GPU: en CPU tarda minutos. Por eso Pronto ofrece cada modelo (`MODELS`) solo con una
//! GPU Vulkan con la memoria que pide y CPU con AVX2 (whisper.cpp se compila para AVX2).
//!
//! Whisper procesa ventanas de 30 s e inventa frases en los silencios («Subtítulos realizados por…»,
//! repeticiones): el audio llega ya recortado a la voz, y `drop_hallucinations` limpia lo que sale.

use crate::asr::{Timed, TimedToken};
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use whisper_rs::{DtwMode, DtwModelPreset, DtwParameters, FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState};

/// Un modelo de Whisper que Pronto sabe bajar (de `ggerganov/whisper.cpp` en Hugging Face, con su
/// SHA-256) y usar. `wer`: errores por palabra medidos con las reuniones de prueba troceadas por
/// turnos (`docs/asr-comparativa.md`; Parakeet, 5,7 %); `None` si no se midió.
pub struct WhisperModel {
    pub id: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    pub file: &'static str,
    pub sha256: &'static str,
    pub bytes: u64,
    /// Memoria de vídeo mínima para ofrecerlo, en GB.
    pub min_vram_gb: f32,
    /// Memoria de vídeo total (GB) con la que se cargan a la vez el contexto rápido, el del latido y el
    /// de DTW (`room_for_two`): lo medido con `tests::banco_memoria` (24-oct-2026, RTX 4070 Ti, tras
    /// una inferencia por cada camino y un latido), 0,5 GB que crece después con audios distintos
    /// (rompedor 24-oct, BAJA 2) y 1,5 GB para Windows y otras apps.
    pub pair_vram_gb: f32,
    pub wer: Option<f32>,
    /// 1 (lento) a 5 (rápido), con GPU.
    pub speed: u8,
}

pub const DEFAULT_MODEL: &str = "large-v3-turbo";

pub const MODELS: [WhisperModel; 5] = [
    WhisperModel {
        id: "large-v3-turbo",
        name: "Whisper large-v3-turbo",
        desc: "El más rápido de los grandes: casi la precisión de large-v3 a varias veces su velocidad. El recomendado con tarjeta gráfica.",
        file: "ggml-large-v3-turbo-q5_0.bin",
        sha256: "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
        bytes: 574_041_195,
        min_vram_gb: 3.0,
        pair_vram_gb: 4.3,
        wer: Some(4.2),
        speed: 5,
    },
    WhisperModel {
        id: "large-v3",
        name: "Whisper large-v3",
        desc: "El más grande de Whisper: el que mejor aguanta ruido y acentos. Dos o tres veces más lento que turbo y más pesado.",
        file: "ggml-large-v3-q5_0.bin",
        sha256: "d75795ecff3f83b5faa89d1900604ad8c780abd5739fae406de19f23ecd98ad1",
        bytes: 1_081_140_203,
        min_vram_gb: 4.0,
        pair_vram_gb: 6.7,
        wer: Some(3.8),
        speed: 2,
    },
    WhisperModel {
        id: "medium",
        name: "Whisper medium",
        desc: "El que menos se equivocó en las pruebas de reuniones, pero lento. Para tarjetas de 2 GB.",
        file: "ggml-medium-q5_0.bin",
        sha256: "19fea4b380c3a618ec4723c3eef2eb785ffba0d0538cf43f8f235e7b3b34220f",
        bytes: 539_212_467,
        min_vram_gb: 2.0,
        pair_vram_gb: 4.7,
        wer: Some(3.4),
        speed: 2,
    },
    WhisperModel {
        id: "small",
        name: "Whisper small",
        desc: "Ligero y rápido, para tarjetas pequeñas (1 GB). Algo menos preciso que los grandes.",
        file: "ggml-small-q5_1.bin",
        sha256: "ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb",
        bytes: 190_085_487,
        min_vram_gb: 1.0,
        pair_vram_gb: 2.5,  // sin medir: no descargado en el PC de pruebas
        wer: Some(4.0),
        speed: 4,
    },
    WhisperModel {
        id: "base",
        name: "Whisper base",
        desc: "El mínimo: 60 MB y muy rápido, pero el que más se equivoca. Para equipos muy justos.",
        file: "ggml-base-q5_1.bin",
        sha256: "422f1ae452ade6f30a004d7e5c6a43195e4433bc370bf23fac9cc591f01a8898",
        bytes: 59_707_625,
        min_vram_gb: 1.0,
        pair_vram_gb: 2.5,  // sin medir: no descargado en el PC de pruebas
        wer: None,
        speed: 5,
    },
];

/// El modelo con ese id (o el de fábrica si no existe).
pub fn model(id: &str) -> &'static WhisperModel {
    MODELS.iter().find(|m| m.id == id).unwrap_or(&MODELS[0])
}

impl WhisperModel {
    pub fn path(&self) -> PathBuf {
        model_dir().join(self.file)
    }

    /// Del tamaño justo y no visto ya dañado (no hashea: lo pregunta el hub; ver `verify`).
    pub fn present(&self) -> bool {
        let path = self.path();
        std::fs::metadata(&path).is_ok_and(|m| m.len() == self.bytes) && !crate::modeldl::file_known_bad(&path, self.sha256)
    }

    /// Tamaño y SHA-256 del `.bin` (con la caché `.verificado` de `modeldl`) antes de dárselo a
    /// whisper.cpp, que con algunos daños (un bit en la cabecera) aborta el proceso con `GGML_ASSERT`
    /// en vez de devolver error, y Pronto moriría en cada arranque (rompedor 22-oct-b, MEDIA 5).
    pub fn verify(&self) -> Result<(), String> {
        crate::modeldl::verify_file(&self.path(), self.sha256, self.bytes)
            .map_err(|e| format!("El modelo de Whisper no se puede usar: {e}. Descárgalo de nuevo desde Configuración → Motor."))
    }

    /// Cabezas de alineamiento de este modelo para los tiempos por token (DTW).
    fn dtw(&self) -> DtwModelPreset {
        match self.id {
            "large-v3" => DtwModelPreset::LargeV3,
            "medium" => DtwModelPreset::Medium,
            "small" => DtwModelPreset::Small,
            "base" => DtwModelPreset::Base,
            _ => DtwModelPreset::LargeV3Turbo,
        }
    }

    /// ¿Se puede usar en este PC? `Err` dice por qué no.
    pub fn availability(&self) -> Result<(), String> {
        if !std::arch::is_x86_feature_detected!("avx2") {
            return Err("El procesador no tiene AVX2".into());
        }
        match gpu() {
            Some((_, vram)) if vram as f64 >= self.min_vram_gb as f64 * 1e9 * 0.95 => Ok(()),
            Some((name, vram)) => Err(format!("{name} tiene {:.1} GB de memoria de vídeo; este modelo necesita {:.0} GB", vram as f64 / 1e9, self.min_vram_gb)),
            None => Err("No hay una tarjeta gráfica compatible (Vulkan)".into()),
        }
    }
}

/// Trozo máximo por inferencia: la ventana de Whisper es de 30 s.
pub const MAX_CHUNK_SECS: usize = 28;
/// Un segmento con más probabilidad de «no hay voz» que esto se descarta.
const NO_SPEECH_MAX: f32 = 0.6;

/// `%APPDATA%\Pronto2\models\whisper`.
pub fn model_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_default().join("Pronto2").join("models").join("whisper")
}

/// La GPU con la que correría Whisper: nombre y memoria total, o `None` si no hay ninguna Vulkan.
/// `vulkan-1.dll` va con carga diferida (`build.rs`): se comprueba que exista antes de tocar Vulkan,
/// para que Pronto arranque igual en un PC sin controlador Vulkan.
/// Se mira una vez por arranque (enumerar Vulkan tarda).
pub fn gpu() -> Option<(String, u64)> {
    static GPU: std::sync::OnceLock<Option<(String, u64)>> = std::sync::OnceLock::new();
    GPU.get_or_init(|| {
        if !vulkan_present() {
            return None;
        }
        whisper_rs::vulkan::list_devices().into_iter().max_by_key(|d| d.vram.total).map(|d| (d.name.trim().to_string(), d.vram.total as u64))
    })
    .clone()
}

fn vulkan_present() -> bool {
    use windows::core::w;
    use windows::Win32::Foundation::FreeLibrary;
    use windows::Win32::System::LibraryLoader::LoadLibraryW;
    // SAFETY: cargar y soltar una DLL del sistema por su nombre.
    unsafe {
        match LoadLibraryW(w!("vulkan-1.dll")) {
            Ok(h) => {
                let _ = FreeLibrary(h);
                true
            }
            Err(_) => false,
        }
    }
}

/// Un contexto de whisper.cpp con su único estado (el motor ya serializa las inferencias).
struct Ctx {
    ctx: WhisperContext,
    state: Mutex<WhisperState>,
}

impl Ctx {
    /// `flash`: atención flash, sin tiempos DTW. Sin ella, tiempos por token con DTW (cabezas de
    /// alineamiento del modelo): los normales de Whisper se desvían ~0,35 s de media (hasta 1 s, juez de
    /// G1, 23-oct-2026), y Meets los necesita finos para quitar el eco del altavoz (medido: 29 de 263
    /// palabras al otro hablante sin DTW). whisper.cpp no admite DTW con atención flash.
    fn open(model: &WhisperModel, use_gpu: bool, flash: bool) -> Result<Self, String> {
        let path = model.path();
        let mut cp = WhisperContextParameters::default();
        cp.use_gpu(use_gpu);
        cp.flash_attn(flash);
        if !flash {
            cp.dtw_parameters(DtwParameters { mode: DtwMode::ModelPreset { model_preset: model.dtw() }, ..Default::default() });
        }
        let path_str = path.to_str().ok_or("Ruta del modelo Whisper no válida")?;
        let ctx = WhisperContext::new_with_params(path_str, cp).map_err(|e| format!("No se pudo cargar Whisper ({}): {e}", path.display()))?;
        let state = ctx.create_state().map_err(|e| format!("No se pudo preparar Whisper: {e}"))?;
        Ok(Self { ctx, state: Mutex::new(state) })
    }
}

/// Whisper cargado: dos contextos del mismo modelo.
///
/// - `fast` (atención flash, sin DTW) para el dictado: medido el 23-oct-2026 con el juez de G1 (RTX 4070
///   Ti, turbo, idioma automático), 208 ms por frase frente a 1.051 ms con DTW, con el mismo acierto
///   (WER 9,9 % frente a 10,6 %).
/// - `precise` (DTW) para lo que necesita tiempos finos: la fase 2 de Meets y los dictados en los que
///   la huella de voz oye una palabra del diccionario. Se carga tras `fast` si caben los dos en la
///   memoria de vídeo (`preload_precise`) o la primera vez que se pide.
///
/// Si `fast` no se puede crear (o no hay GPU), todo va por `precise`, como antes del contrato v14.
pub struct Whisper {
    model: &'static WhisperModel,
    use_gpu: bool,
    fast: Option<Ctx>,
    /// Segundo estado de `fast` para el latido (`pulse`): no compite por el cerrojo del dictado.
    warm: Option<Mutex<WhisperState>>,
    precise: Mutex<Option<Arc<Ctx>>>,
    /// Cargar `precise` ya falló una vez: no se reintenta en cada trozo (cada intento son segundos).
    precise_failed: AtomicBool,
    eot: i32,
    threads: i32,
    /// Texto previo que guía a Whisper: una frase bien puntuada (sin ella, a veces escribe sin puntos
    /// ni comas) y las palabras del diccionario del usuario, que así reconoce mejor.
    prompt: Mutex<String>,
}

/// ¿Caben en la memoria de vídeo los dos contextos de `model` y el estado del latido
/// (`pair_vram_gb`)? Antes se pedía el doble de `min_vram_gb` (turbo: 5,7 GB): medido, turbo con todo
/// ocupa 2,2-2,7 GB y las tarjetas de 4 GB se quedaban sin latido sin necesidad. Si no caben, el contexto
/// con DTW se carga igual al pedirlo (Meets, huella), así que esto solo decide la precarga y el latido.
fn room_for_two(model: &WhisperModel) -> bool {
    gpu().is_some_and(|(_, vram)| fits_two(model, vram))
}

fn fits_two(model: &WhisperModel, vram: u64) -> bool {
    vram as f64 >= model.pair_vram_gb as f64 * 1e9 * 0.95
}

/// Idiomas entre los que elige el «automático» de Pronto, el primero cuando duda (parche `[Pronto]` de
/// whisper.cpp: el idioma como lista). Con la detección libre de Whisper, un español con acento salía a
/// veces en portugués (Txonso con medium, 24-oct-2026).
const AUTO_LANGS: &str = "es,en";

/// Lo que se le pasa a whisper.cpp como idioma: «auto» (o nada) → `AUTO_LANGS`; «libre» (solo bancos)
/// → la detección de Whisper entre sus 99 idiomas; un código ISO, tal cual.
fn whisper_language(language: &str) -> &str {
    match language {
        "" | "auto" => AUTO_LANGS,
        "libre" => "auto",
        // Un código ISO de dos letras tal cual; cualquier otra cosa (ajuste a mano, «PT,GL») → español o inglés.
        l if l.len() == 2 && l.bytes().all(|b| b.is_ascii_lowercase()) => l,
        _ => AUTO_LANGS,
    }
}

/// Frase de ejemplo puntuada: Whisper imita el estilo del texto previo.
const STYLE_PROMPT: &str = "Hola, ¿qué tal? Vale, te cuento: mañana lo revisamos y, si hace falta, lo cambiamos.";
/// Palabras del diccionario que caben en el texto previo (el de Whisper no pasa de ~220 tokens).
const PROMPT_WORDS: usize = 40;

/// Texto previo para `words` (las del diccionario, las más importantes primero).
pub fn prompt_for(words: &[String]) -> String {
    let words: Vec<&str> = words.iter().map(|w| w.trim()).filter(|w| !w.is_empty()).take(PROMPT_WORDS).collect();
    if words.is_empty() {
        STYLE_PROMPT.to_string()
    } else {
        format!("{STYLE_PROMPT} {}.", words.join(", "))
    }
}

impl Whisper {
    /// Carga el modelo y lo calienta: la primera inferencia con Vulkan compila los shaders (~40 s
    /// la primera vez en un PC); mejor aquí, en el hilo de carga, que en el primer dictado.
    pub fn load(model: &'static WhisperModel, use_gpu: bool) -> Result<Self, String> {
        model.verify()?;
        whisper_rs::install_logging_hooks();
        // La atención flash de whisper.cpp solo acelera en GPU.
        let fast = if use_gpu { Ctx::open(model, true, true).map_err(|e| eprintln!("[whisper] sin atención flash ({e}); todo con DTW")).ok() } else { None };
        let precise = match fast {
            Some(_) => None,
            None => Some(Arc::new(Ctx::open(model, use_gpu, false)?)),
        };
        let eot = fast.as_ref().or(precise.as_deref()).map(|c| c.ctx.token_eot()).unwrap_or_default();
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2).min(8) as i32;
        // El estado del latido ocupa memoria de vídeo: solo si sobra (la misma cuenta que `preload_precise`).
        let warm = fast.as_ref().filter(|_| room_for_two(model)).and_then(|c| c.ctx.create_state().map_err(|e| eprintln!("[whisper] sin latido: {e}")).ok()).map(Mutex::new);
        let w = Self { model, use_gpu, fast, warm, precise: Mutex::new(precise), precise_failed: AtomicBool::new(false), eot, threads, prompt: Mutex::new(STYLE_PROMPT.to_string()) };
        let _ = w.decode_timed(&vec![0.0; 16_000], "es");
        Ok(w)
    }

    /// Carga ya el contexto con DTW si caben los dos en la memoria de vídeo (el doble de lo que pide
    /// el modelo), para que la primera reunión o huella de voz no espere a cargarlo. Si no caben, se
    /// carga la primera vez que se pide.
    pub fn preload_precise(&self) {
        if room_for_two(self.model) && self.fast.is_some() {
            let t0 = std::time::Instant::now();
            if self.precise().is_some() {
                eprintln!("[whisper] contexto con DTW listo en {:.1} s", t0.elapsed().as_secs_f32());
            }
        }
    }

    /// Latido mientras se graba un dictado (`asr::Engine::keep_warm`): una inferencia corta (medio
    /// segundo de silencio, un token) en un estado aparte, para que la GPU no baje de reloj antes de
    /// soltar el atajo. Medido el 24-oct-2026 (juez de G1, turbo, RTX 4070 Ti, 3 s de reposo antes de
    /// cada frase): sin latido 330 ms por frase, con latido cada 300 ms 228, cada 150 ms 200 (en
    /// caliente, 190). Con el encoder recortado (`audio_ctx`) el latido no basta (286 ms). No espera:
    /// si hay otro latido en marcha, no hace nada.
    pub fn pulse(&self) {
        let Some(mut state) = self.warm.as_ref().and_then(|w| w.try_lock()) else { return };
        let mut p = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        p.set_language(Some("es"));
        p.set_n_threads(2);
        p.set_print_progress(false);
        p.set_print_realtime(false);
        p.set_print_special(false);
        p.set_print_timestamps(false);
        p.set_no_context(true);
        p.set_single_segment(true);
        p.set_max_tokens(1);
        let _ = state.full(p, &[0.0; 8_000]);
    }

    /// Los tiempos por token de `decode_timed` son los aproximados de Whisper, sin DTW.
    pub fn rough_timings(&self) -> bool {
        self.fast.is_some()
    }

    /// El contexto con DTW, cargándolo si hace falta. `None` si no se pudo (sin memoria de vídeo):
    /// entonces se usa `fast` con sus tiempos aproximados y las reuniones siguen (regla 4).
    fn precise(&self) -> Option<Arc<Ctx>> {
        let mut slot = self.precise.lock();
        if slot.is_none() && !self.precise_failed.load(Ordering::SeqCst) {
            // Puede abrirse horas después de `load`: se vuelve a comprobar el `.bin`, que dañado tumba
            // whisper.cpp con `GGML_ASSERT` (seguridad 24-oct, BAJA 2).
            match self.model.verify().and_then(|_| Ctx::open(self.model, self.use_gpu, false)) {
                Ok(c) => {
                    let c = Arc::new(c);
                    let _ = self.run(&c, &vec![0.0; 16_000], "es", |_| {});
                    *slot = Some(c);
                }
                Err(e) => {
                    eprintln!("[whisper] no se pudo cargar el contexto con DTW ({e}); tiempos aproximados");
                    self.precise_failed.store(true, Ordering::SeqCst);
                }
            }
        }
        slot.clone()
    }

    /// Cambia el texto previo (ver `prompt_for`; vacío = ninguno).
    pub fn set_prompt(&self, prompt: String) {
        *self.prompt.lock() = prompt;
    }

    /// Transcribe audio mono a 16 kHz (como mucho `MAX_CHUNK_SECS`) por el camino rápido del dictado,
    /// con tiempos por token aproximados (ver `rough_timings`). `language`: código ISO («es») o «auto»
    /// (español o inglés, ver `whisper_language`).
    pub fn decode_timed(&self, samples: &[f32], language: &str) -> Timed {
        self.decode(samples, language, false)
    }

    /// Como `decode_timed`; con `precise`, tiempos por token con DTW (contexto `precise`).
    pub fn decode(&self, samples: &[f32], language: &str, precise: bool) -> Timed {
        if let (Some(fast), false) = (&self.fast, precise) {
            return self.run(fast, samples, language, |_| {});
        }
        match self.precise() {
            Some(ctx) => self.run(&ctx, samples, language, |_| {}),
            None => self.fast.as_ref().map(|fast| self.run(fast, samples, language, |_| {})).unwrap_or_default(),
        }
    }

    /// Una inferencia en `ctx`, con `tweak` aplicado a los parámetros al final (bancos de medida).
    fn run(&self, ctx: &Ctx, samples: &[f32], language: &str, tweak: impl FnOnce(&mut FullParams)) -> Timed {
        let mut p = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        p.set_language(Some(whisper_language(language)));
        p.set_n_threads(self.threads);
        p.set_print_progress(false);
        p.set_print_realtime(false);
        p.set_print_special(false);
        p.set_print_timestamps(false);
        // Sin el texto anterior como contexto: es la causa típica de repeticiones en bucle.
        p.set_no_context(true);
        p.set_suppress_blank(true);
        p.set_suppress_nst(true);
        p.set_token_timestamps(true);
        p.set_temperature(0.0);
        p.set_no_speech_thold(NO_SPEECH_MAX);
        let prompt = self.prompt.lock().clone();
        if !prompt.is_empty() {
            p.set_initial_prompt(&prompt);
        }
        tweak(&mut p);
        let mut state = ctx.state.lock();
        if let Err(e) = state.full(p, samples) {
            eprintln!("[whisper] {e}");
            return Timed::default();
        }
        let mut segments: Vec<(String, Vec<TimedToken>)> = Vec::new();
        for seg in state.as_iter() {
            if seg.no_speech_probability() > NO_SPEECH_MAX {
                continue;
            }
            // (texto, inicio, fin) en centésimas: el inicio, por DTW si lo hay.
            let mut raw: Vec<(String, i64, i64)> = Vec::new();
            for i in 0..seg.n_tokens() {
                let Some(tok) = seg.get_token(i) else { continue };
                // Fuera los especiales ([_BEG_], marcas de tiempo, fin de texto…).
                if tok.token_id() >= self.eot {
                    continue;
                }
                let Ok(text) = tok.to_str_lossy() else { continue };
                if text.is_empty() {
                    continue;
                }
                let d = tok.token_data();
                let start = if d.t_dtw >= 0 { d.t_dtw } else { d.t0 };
                raw.push((text.into_owned(), start, d.t1.max(d.t0)));
            }
            // Cada token dura hasta que empieza el siguiente (el último, hasta su fin de Whisper).
            let mut tokens = Vec::with_capacity(raw.len());
            for i in 0..raw.len() {
                let (start, end) = (raw[i].1, raw.get(i + 1).map_or(raw[i].2, |n| n.1));
                tokens.push(TimedToken { text: std::mem::take(&mut raw[i].0), start: start as f32 / 100.0, end: end.max(start + 2) as f32 / 100.0 });
            }
            let text = seg.to_str_lossy().map(|t| t.trim().to_string()).unwrap_or_default();
            segments.push((text, tokens));
        }
        drop(state);
        let segments = drop_hallucinations(segments);
        let text = segments.iter().map(|s| s.0.as_str()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" ");
        Timed { text, tokens: segments.into_iter().flat_map(|s| s.1).collect() }
    }
}

/// Frases que Whisper inventa en silencios o con ruido (las aprendió de subtítulos de vídeos).
const KNOWN: &[&str] = &[
    "subtitulos realizados por",
    "subtitulos por la comunidad de amara",
    "amara.org",
    "gracias por ver el video",
    "gracias por ver",
    "suscribete",
    "no olvides suscribirte",
    "thanks for watching",
    "thank you for watching",
    "please subscribe",
    "musica",
];

fn fold(s: &str) -> String {
    crate::text::fold(&s.to_lowercase())
}

/// Quita segmentos inventados: frases de subtítulos conocidas y repeticiones en bucle («qué pasa
/// qué pasa qué pasa…»: una misma secuencia de 1 a 4 palabras más de tres veces seguidas se deja en una).
pub fn drop_hallucinations(segments: Vec<(String, Vec<TimedToken>)>) -> Vec<(String, Vec<TimedToken>)> {
    let mut out: Vec<(String, Vec<TimedToken>)> = Vec::new();
    for (text, tokens) in segments {
        let f = fold(&text);
        let bare: String = f.chars().filter(|c| c.is_alphanumeric() || c.is_whitespace() || *c == '.').collect();
        let bare = bare.split_whitespace().collect::<Vec<_>>().join(" ");
        if bare.is_empty() || KNOWN.iter().any(|k| bare.contains(k) && bare.len() <= k.len() + 40) {
            continue;
        }
        // El mismo segmento dos veces seguidas: el segundo sobra.
        if out.last().is_some_and(|(prev, _)| fold(prev) == f) {
            continue;
        }
        match collapse_loops(&text) {
            Some(short) => out.push((short, Vec::new())),
            None => out.push((text, tokens)),
        }
    }
    out
}

/// Si el texto repite en bucle una secuencia de 1 a 4 palabras (más de 3 veces seguidas), lo
/// devuelve con una sola; `None` si no hay bucle.
fn collapse_loops(text: &str) -> Option<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let key = |w: &str| fold(w.trim_matches(|c: char| !c.is_alphanumeric()));
    for n in 1..=4 {
        let mut i = 0;
        while i + n * 4 <= words.len() {
            let unit: Vec<String> = words[i..i + n].iter().map(|w| key(w)).collect();
            let mut reps = 1;
            while i + (reps + 1) * n <= words.len() && words[i + reps * n..i + (reps + 1) * n].iter().map(|w| key(w)).collect::<Vec<_>>() == unit {
                reps += 1;
            }
            if reps > 3 {
                let mut kept: Vec<&str> = words[..i + n].to_vec();
                kept.extend_from_slice(&words[i + reps * n..]);
                return Some(kept.join(" "));
            }
            i += 1;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(t: &str) -> (String, Vec<TimedToken>) {
        (t.to_string(), vec![TimedToken { text: format!(" {t}"), start: 0.0, end: 1.0 }])
    }

    #[test]
    fn quita_frases_inventadas_y_bucles() {
        let out = drop_hallucinations(vec![
            seg("Vale, mañana lo miramos."),
            seg("Subtítulos realizados por la comunidad de Amara.org"),
            seg("¡Gracias por ver el vídeo!"),
            seg("Vale, mañana lo miramos."),
            seg("Y luego qué pasa qué pasa qué pasa qué pasa qué pasa con el servidor."),
        ]);
        let texts: Vec<&str> = out.iter().map(|s| s.0.as_str()).collect();
        assert_eq!(texts, ["Vale, mañana lo miramos.", "Y luego qué pasa con el servidor."]);
        // Una frase normal que contiene «gracias» no se toca; tres repeticiones tampoco (pueden ser reales).
        let keep = drop_hallucinations(vec![seg("Muchas gracias por ver el presupuesto tan rápido, de verdad, me has salvado la semana."), seg("Sí, sí, sí.")]);
        assert_eq!(keep.len(), 2);
    }

    /// Banco del dictado: cada WAV de `PRONTO_WAVS` (`;`) por el camino rápido del dictado (`rapido`,
    /// atención flash) y por el de antes (`preciso`, DTW), turnándose en cada vuelta para que la GPU
    /// esté igual para los dos. `PRONTO_LANG` («auto» por defecto, el de Txonso), `PRONTO_RUNS` vueltas
    /// y, con `PRONTO_REPOSO_MS`, un reposo antes de cada medida (la GPU baja de reloj, como entre dos
    /// dictados). Audios de más de `MAX_CHUNK_SECS` se saltan. Imprime
    /// `COMP <camino>	<id>	<ms,…>	<texto>` y, si el texto coincide, cuánto se desvían los inicios de
    /// palabra sin DTW (`TIEMPOS`).
    /// `cargo test --release banco_dictado -- --ignored --nocapture --test-threads=1`
    #[test]
    #[ignore]
    fn banco_dictado() {
        let Ok(wavs) = std::env::var("PRONTO_WAVS") else { return };
        let m = model(&std::env::var("PRONTO_WHISPER").unwrap_or_default());
        let lang = std::env::var("PRONTO_LANG").unwrap_or_else(|_| "auto".into());
        let runs: usize = std::env::var("PRONTO_RUNS").ok().and_then(|r| r.parse().ok()).unwrap_or(5);
        let rest = std::time::Duration::from_millis(std::env::var("PRONTO_REPOSO_MS").ok().and_then(|r| r.parse().ok()).unwrap_or(0));
        // `PRONTO_PULSO_MS`: durante el reposo, un latido (`pulse`) cada tantos ms, como mientras se graba.
        let pulse_every: u64 = std::env::var("PRONTO_PULSO_MS").ok().and_then(|r| r.parse().ok()).unwrap_or(0);
        let rest_now = |w: &Whisper| {
            if pulse_every == 0 {
                return std::thread::sleep(rest);
            }
            let t0 = std::time::Instant::now();
            while t0.elapsed() < rest {
                w.pulse();
                std::thread::sleep(std::time::Duration::from_millis(pulse_every).min(rest.saturating_sub(t0.elapsed())));
            }
        };
        let w = Whisper::load(m, true).expect("cargar Whisper");
        w.set_prompt(prompt_for(&[]));
        assert!(w.rough_timings(), "sin el camino rápido (¿GPU sin atención flash?)");
        eprintln!("BANCO modelo={} idioma={lang} reposo={rest:?} gpu={:?}", m.name, gpu());
        for path in wavs.split(';').filter(|p| !p.is_empty()) {
            let audio = crate::audio::decode_wav(&std::fs::read(path).unwrap()).expect("wav");
            if audio.len() > MAX_CHUNK_SECS * 16_000 {
                continue;
            }
            let id = std::path::Path::new(path).file_stem().unwrap().to_string_lossy().into_owned();
            let mut ms = [Vec::new(), Vec::new()];
            let mut out = [Timed::default(), Timed::default()];
            for r in 0..=runs {
                for k in 0..2 {
                    let precise = (k + r) % 2 == 1;
                    rest_now(&w);
                    let t = std::time::Instant::now();
                    out[precise as usize] = w.decode(&audio, &lang, precise);
                    if r > 0 {
                        ms[precise as usize].push(format!("{:.0}", t.elapsed().as_secs_f64() * 1000.0));
                    }
                }
            }
            for (i, name) in ["rapido", "preciso"].iter().enumerate() {
                println!("COMP {name}	{id}	{}	{}", ms[i].join(","), out[i].text);
            }
            let starts = |t: &Timed| t.tokens.iter().filter(|k| k.text.starts_with(' ')).map(|k| k.start).collect::<Vec<_>>();
            let (a, b) = (starts(&out[1]), starts(&out[0]));
            if a.len() == b.len() && !a.is_empty() {
                let d: Vec<f32> = a.iter().zip(&b).map(|(x, y)| (x - y).abs()).collect();
                println!("TIEMPOS {id}	palabras={}	media={:.3}s	max={:.3}s", d.len(), d.iter().sum::<f32>() / d.len() as f32, d.iter().cloned().fold(0.0, f32::max));
            }
        }
    }

    /// Whisper real sobre los WAV de `PRONTO_WAVS` (separados por `;`). Con `PRONTO_NO_GPU`, en CPU.
    /// `PRONTO_WAVS=… cargo test --release whisper_real -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn whisper_real() {
        let Ok(wavs) = std::env::var("PRONTO_WAVS") else { return };
        eprintln!("GPU: {:?}", gpu());
        let t0 = std::time::Instant::now();
        // `PRONTO_WHISPER=small` (u otro id de `MODELS`) para probar otro modelo.
        let m = model(&std::env::var("PRONTO_WHISPER").unwrap_or_default());
        eprintln!("modelo: {}", m.name);
        let w = Whisper::load(m, std::env::var("PRONTO_NO_GPU").is_err()).expect("cargar Whisper");
        // `PRONTO_PROMPT=no` sin texto previo; si no, el de fábrica con unas palabras de diccionario.
        match std::env::var("PRONTO_PROMPT").as_deref() {
            Ok("no") => w.set_prompt(String::new()),
            _ => w.set_prompt(prompt_for(&["BeltranLabs".into(), "Ollama".into(), "Pronto".into(), "Txonso".into()])),
        }
        eprintln!("carga + calentamiento: {:.1} s", t0.elapsed().as_secs_f32());
        for path in wavs.split(';') {
            let audio = crate::audio::decode_wav(&std::fs::read(path).unwrap()).expect("wav");
            let clip = &audio[..audio.len().min(MAX_CHUNK_SECS * 16_000)];
            let _ = w.decode_timed(clip, "es");
            let t = std::time::Instant::now();
            let r = w.decode_timed(clip, "es");
            eprintln!("{path} ({:.1} s): {:.2} s → {} [{} tokens]", audio.len() as f32 / 16_000.0, t.elapsed().as_secs_f32(), r.text, r.tokens.len());
        }
    }

    #[test]
    fn automatico_es_espanol_o_ingles() {
        assert_eq!(whisper_language("auto"), "es,en");
        assert_eq!(whisper_language(""), "es,en");
        assert_eq!(whisper_language("en"), "en");
        assert_eq!(whisper_language("libre"), "auto");
        // Ajustes raros (rompedor 24-oct, BAJA 3): nunca la detección libre ni un idioma -1.
        for raro in ["PT,GL", " pt, gl", "pt;gl", "español", "ES"] {
            assert_eq!(whisper_language(raro), "es,en", "{raro}");
        }
    }

    /// Idioma que elige Whisper con la detección libre («libre») y con la de Pronto («auto» = español o
    /// inglés) para cada WAV de `PRONTO_WAVS` (`;`), entero y recortado a 1,5 s (lo corto es lo que más
    /// se equivoca), con cada modelo de `PRONTO_WHISPER` (comas; por defecto los descargados). Imprime
    /// `IDIOMA <modelo> <wav> <entero|corto> <libre> <auto> <ms libre> <ms auto> <p_es> <p_en> <texto auto>` y falla si
    /// «auto» sale de español e inglés. `PRONTO_IDIOMA_ESPERADO=es|en` exige además ese idioma.
    /// `cargo test --release banco_idioma -- --ignored --nocapture --test-threads=1`
    #[test]
    #[ignore]
    fn banco_idioma() {
        let Ok(wavs) = std::env::var("PRONTO_WAVS") else { return };
        let expected = std::env::var("PRONTO_IDIOMA_ESPERADO").ok();
        let ids = std::env::var("PRONTO_WHISPER").unwrap_or_default();
        let models: Vec<&'static WhisperModel> =
            if ids.is_empty() { MODELS.iter().filter(|m| m.present()).collect() } else { ids.split(',').map(model).collect() };
        let mut wrong = Vec::new();
        for m in models {
            let w = Whisper::load(m, true).expect("cargar Whisper");
            w.set_prompt(prompt_for(&[]));
            let fast = w.fast.as_ref().expect("camino rápido");
            for path in wavs.split(';').filter(|p| !p.is_empty()) {
                let audio = crate::audio::decode_wav(&std::fs::read(path).unwrap()).expect("wav");
                let audio = &audio[..audio.len().min(MAX_CHUNK_SECS * 16_000)];
                let id = std::path::Path::new(path).file_stem().unwrap().to_string_lossy().into_owned();
                for (cut, clip) in [("entero", audio), ("corto", &audio[..audio.len().min(24_000)])] {
                    let mut out = Vec::new();
                    for lang in ["libre", "auto"] {
                        let _ = w.run(fast, clip, lang, |_| {});
                        let t = std::time::Instant::now();
                        let r = w.run(fast, clip, lang, |_| {});
                        let ms = t.elapsed().as_secs_f64() * 1000.0;
                        let code = whisper_rs::get_lang_str(fast.state.lock().full_lang_id_from_state()).unwrap_or("?");
                        out.push((code, ms, r.text));
                    }
                    let (free, auto) = (&out[0], &out[1]);
                    // Probabilidades de la detección (el estado ya tiene el espectrograma de `clip`).
                    let p = |l: &str, probs: &[f32]| whisper_rs::get_lang_id(l).and_then(|i| probs.get(i as usize).copied()).unwrap_or(0.0);
                    let (pes, pen) = fast.state.lock().lang_detect(0, 2).map_or((0.0, 0.0), |(_, pr)| (p("es", &pr), p("en", &pr)));
                    println!("IDIOMA {}\t{id}\t{cut}\t{}\t{}\t{:.0}\t{:.0}\tp_es={pes:.3}\tp_en={pen:.3}\t{}", m.id, free.0, auto.0, free.1, auto.1, auto.2);
                    if !matches!(auto.0, "es" | "en") || expected.as_deref().is_some_and(|e| e != auto.0) {
                        wrong.push(format!("{} {id} {cut}: {}", m.id, auto.0));
                    }
                }
            }
        }
        assert!(wrong.is_empty(), "idioma equivocado con «auto»: {wrong:?}");
    }

    /// Tarjetas de 4, 6 y 8 GB (lo que dice Vulkan: GiB): con turbo caben los dos contextos y el
    /// latido desde 4 GB; con large-v3, desde 6 GB.
    #[test]
    fn caben_los_dos_segun_la_tarjeta() {
        let gib = |g: u64| g * 1_073_741_824;
        let turbo = model("large-v3-turbo");
        let large = model("large-v3");
        assert!(!fits_two(turbo, gib(3)));
        assert!(fits_two(turbo, gib(4)));
        assert!(!fits_two(large, gib(4)));
        assert!(fits_two(large, gib(6)));
        for m in &MODELS {
            assert!(m.pair_vram_gb > m.min_vram_gb.min(2.0), "{}: el par no puede ocupar menos que uno", m.id);
            assert!(fits_two(m, gib(8)), "{} no cabe ni en 8 GB", m.id);
        }
    }

    /// Huella del árbol `vendor/whisper-rs-sys` (whisper-rs-sys 0.15.0 de crates.io + el parche
    /// `[Pronto]`, ver su `PRONTO.md`). Con `[patch]` por ruta, Cargo.lock no guarda checksum: sin este
    /// test, cualquier cambio en los ~13 MB de código de terceros pasaría sin que se note (seguridad
    /// 24-oct, BAJA 1). Saltos de línea normalizados (git con autocrlf). Si se actualiza el vendor a
    /// propósito, se cambia la constante a la que imprime el fallo.
    const VENDOR_SHA256: &str = "fa8707f301709235aa73121279f46d6a71e06c53956c86fc5a18c81d15fe9323";

    #[test]
    fn vendor_whisper_sin_cambios_ajenos() {
        use sha2::{Digest, Sha256};
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("vendor").join("whisper-rs-sys");
        let mut files = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    let rel = p.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
                    if rel != "PRONTO.md" && rel != ".cargo-ok" {
                        files.push((rel, p));
                    }
                }
            }
        }
        files.sort();
        let mut h = Sha256::new();
        for (rel, p) in &files {
            let bytes = std::fs::read(p).unwrap();
            let mut clean = Vec::with_capacity(bytes.len());
            for (i, b) in bytes.iter().enumerate() {
                if !(*b == b'\r' && bytes.get(i + 1) == Some(&b'\n')) {
                    clean.push(*b);
                }
            }
            h.update(rel.as_bytes());
            h.update([0]);
            h.update(Sha256::digest(&clean));
        }
        let got = format!("{:x}", h.finalize());
        assert_eq!(got, VENDOR_SHA256, "vendor/whisper-rs-sys cambió ({} ficheros)", files.len());
        // Y el parche sigue en uso: Cargo.lock lo toma de la ruta, no de crates.io.
        let lock = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.lock")).unwrap();
        let block = lock.split("[[package]]").find(|b| b.contains("name = \"whisper-rs-sys\"")).expect("whisper-rs-sys en Cargo.lock");
        assert!(!block.contains("source ="), "whisper-rs-sys vuelve a venir de crates.io: el [patch] de Cargo.toml no se aplica");
    }

    /// Memoria de vídeo libre de la GPU de `gpu()` en MB (presupuesto de Vulkan: cuenta todos los
    /// procesos, así que Ollama u otra app que cargue algo a la vez ensucia la medida).
    fn vram_free_mb() -> f64 {
        whisper_rs::vulkan::list_devices().into_iter().max_by_key(|d| d.vram.total).map_or(0.0, |d| d.vram.free as f64 / 1_048_576.0)
    }

    /// Memoria del proceso en MB: (privada, working set).
    fn ram_mb() -> (f64, f64) {
        use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX};
        use windows::Win32::System::Threading::GetCurrentProcess;
        let mut c = PROCESS_MEMORY_COUNTERS_EX::default();
        let cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
        // SAFETY: `c` es un PROCESS_MEMORY_COUNTERS_EX válido del tamaño que se pasa.
        let _ = unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut c as *mut _ as *mut PROCESS_MEMORY_COUNTERS, cb) };
        (c.PrivateUsage as f64 / 1_048_576.0, c.WorkingSetSize as f64 / 1_048_576.0)
    }

    /// Cuánta memoria de vídeo y RAM gasta cada pieza de `Whisper` por modelo: contexto rápido (flash),
    /// estado del latido, contexto con DTW y lo que añaden las inferencias (28 s de audio por cada
    /// camino y un latido). Para decidir la cuenta de `room_for_two`. Modelos: `PRONTO_WHISPER`
    /// (ids separados por comas; por defecto los que estén descargados). Audio: el primer WAV de
    /// `PRONTO_WAVS` o, si no hay, 28 s de ruido suave.
    /// `cargo test --release banco_memoria -- --ignored --nocapture --test-threads=1`
    #[test]
    #[ignore]
    fn banco_memoria() {
        assert!(gpu().is_some(), "sin GPU Vulkan");
        let ids = std::env::var("PRONTO_WHISPER").unwrap_or_default();
        let models: Vec<&'static WhisperModel> =
            if ids.is_empty() { MODELS.iter().filter(|m| m.present()).collect() } else { ids.split(',').map(model).collect() };
        let audio: Vec<f32> = match std::env::var("PRONTO_WAVS").ok().and_then(|w| w.split(';').next().map(str::to_string)) {
            Some(p) => crate::audio::decode_wav(&std::fs::read(p).unwrap()).expect("wav"),
            None => (0..MAX_CHUNK_SECS * 16_000).map(|i| (i as f32 * 0.37).sin() * 0.01).collect(),
        };
        let clip = &audio[..audio.len().min(MAX_CHUNK_SECS * 16_000)];
        let settle = || std::thread::sleep(std::time::Duration::from_millis(800));
        println!("MEM gpu={:?}", gpu());
        println!("MEM modelo\tpaso\tvram_usada_mb\tram_privada_mb\tworking_set_mb");
        for m in models {
            settle();
            let (v0, (p0, s0)) = (vram_free_mb(), ram_mb());
            let row = |step: &str| {
                settle();
                let (v, (p, s)) = (vram_free_mb(), ram_mb());
                println!("MEM {}\t{step}\t{:.0}\t{:.0}\t{:.0}", m.id, v0 - v, p - p0, s - s0);
            };
            {
                let fast = Ctx::open(m, true, true).expect("contexto rápido");
                row("1 rapido (contexto + estado)");
                let warm = fast.ctx.create_state().expect("estado del latido");
                row("2 + estado del latido");
                let precise = Ctx::open(m, true, false).expect("contexto con DTW");
                row("3 + contexto con DTW");
                let run = |c: &Ctx| {
                    let mut p = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
                    p.set_language(Some("auto"));
                    p.set_print_progress(false);
                    p.set_print_realtime(false);
                    p.set_token_timestamps(true);
                    let _ = c.state.lock().full(p, clip);
                };
                run(&fast);
                row("4 + inferencia rapida");
                run(&precise);
                row("5 + inferencia con DTW");
                drop(warm);
                let w = Whisper { model: m, use_gpu: true, fast: None, warm: Some(Mutex::new(fast.ctx.create_state().unwrap())), precise: Mutex::new(None), precise_failed: AtomicBool::new(false), eot: 0, threads: 2, prompt: Mutex::new(String::new()) };
                w.pulse();
                row("6 latido con su estado (sustituye al 2)");
                drop(w);
                drop(precise);
                row("7 sin contexto con DTW");
            }
            row("8 todo liberado");
        }
    }
}
