//! Sondas del rompedor (2026-10-22, tanda paralela): carga. Solo se compilan en pruebas.
//! Las que demuestran una rotura llevan #[ignore = "rompedor: <título>"].
//!
//! Las de motor real necesitan `PRONTO_ROMPEDOR_MODELO` (carpeta de Parakeet, solo lectura: en
//! pruebas `modeldl::verify` no escribe la marca) y `PRONTO_ROMPEDOR_WAV` (`dictado-15s.wav`); sin
//! ellas no miden nada. La del micrófono, `PRONTO_ROMPEDOR_MIC=1`.
//! `cargo test rompedor_carga_ -- --test-threads=1 --nocapture` (y `--ignored` para las rotas).
#![allow(dead_code, unused_imports)]

use crate::asr::{Engine, Timed, TimedToken};
use crate::models::{CleanupLevel, Snippet, Tone, Word, WordVariant};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

// ------------------------------------------------------------------ utilidades

/// Memoria del proceso: (working set, pico del working set, privada) en MB y handles abiertos.
fn mem() -> (f64, f64, f64, u32) {
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX, PROCESS_MEMORY_COUNTERS};
    use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessHandleCount};
    unsafe {
        let mut c = PROCESS_MEMORY_COUNTERS_EX::default();
        let cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
        let _ = GetProcessMemoryInfo(GetCurrentProcess(), &mut c as *mut _ as *mut PROCESS_MEMORY_COUNTERS, cb);
        let mut h = 0u32;
        let _ = GetProcessHandleCount(GetCurrentProcess(), &mut h);
        let mb = |b: usize| b as f64 / 1_048_576.0;
        (mb(c.WorkingSetSize), mb(c.PeakWorkingSetSize), mb(c.PrivateUsage), h)
    }
}

/// Hilos del proceso (por PowerShell: el crate `windows` no trae ToolHelp en este proyecto).
fn threads() -> usize {
    let pid = std::process::id();
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &format!("(Get-Process -Id {pid}).Threads.Count")])
        .output();
    out.ok().and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok()).unwrap_or(0)
}

/// Muestrea el working set cada 50 ms hasta que se pare; devuelve el máximo visto (MB).
struct Sampler {
    stop: Arc<AtomicBool>,
    h: Option<std::thread::JoinHandle<f64>>,
}

impl Sampler {
    fn start() -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let s = stop.clone();
        let h = std::thread::spawn(move || {
            let mut max = 0.0f64;
            while !s.load(Ordering::SeqCst) {
                max = max.max(mem().0);
                std::thread::sleep(Duration::from_millis(50));
            }
            max
        });
        Self { stop, h: Some(h) }
    }
    fn finish(mut self) -> f64 {
        self.stop.store(true, Ordering::SeqCst);
        self.h.take().unwrap().join().unwrap()
    }
}

fn pct(v: &[f64], p: f64) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.total_cmp(b));
    if s.is_empty() {
        return 0.0;
    }
    let i = ((p / 100.0) * (s.len() as f64 - 1.0)).round() as usize;
    s[i.min(s.len() - 1)]
}

fn resumen(v: &[f64]) -> String {
    format!("n={} p50={:.0} p95={:.0} max={:.0} ms", v.len(), pct(v, 50.0), pct(v, 95.0), pct(v, 100.0))
}

/// Motor real y WAV de 15 s, o `None` (sin las variables no se mide nada).
fn motor() -> Option<(Arc<Engine>, Vec<f32>)> {
    let (Ok(dir), Ok(wav)) = (std::env::var("PRONTO_ROMPEDOR_MODELO"), std::env::var("PRONTO_ROMPEDOR_WAV")) else {
        println!("OMITIDO: faltan PRONTO_ROMPEDOR_MODELO y PRONTO_ROMPEDOR_WAV");
        return None;
    };
    let samples = crate::audio::decode_wav(&std::fs::read(&wav).expect("leer WAV")).expect("WAV PCM16 mono 16 kHz");
    let t0 = Instant::now();
    let e = Engine::load_for_tests(&dir).expect("cargar Parakeet");
    println!("motor cargado en {:.1} s", t0.elapsed().as_secs_f64());
    let _ = e.transcribe_timed(&samples).expect("calentar");
    Some((Arc::new(e), samples))
}

/// `n` segundos de audio repitiendo el WAV (con 0,3 s de silencio entre copias).
fn largo(base: &[f32], secs: usize) -> Vec<f32> {
    let n = secs * 16_000;
    let mut out = Vec::with_capacity(n);
    while out.len() < n {
        out.extend_from_slice(base);
        out.extend(std::iter::repeat(0.0).take(4800));
    }
    out.truncate(n);
    out
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

// ------------------------------------------------------------------ 1. pánicos en la tubería

/// Generador fijo (las pruebas no dependen de nada externo).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn pick<'a, T>(&mut self, v: &'a [T]) -> &'a T {
        &v[self.next() as usize % v.len()]
    }
    fn chance(&mut self, n: u64) -> bool {
        self.next() % n == 0
    }
}

const PIEZAS: &[&str] = &[
    "eh", "este", "o sea", "no", "no, mejor", "quiero decir", "perdón", "digo", "pulsa enter", "enter", "nueva línea", "punto",
    "coma", "Beltrán", "Labs", "İstanbul", "İ", "straße", "ẞ", "ﬁ", "ﬀ", "e\u{301}", "ñ", "Ñ", "😀", "👍🏽", "\u{E000}", "\u{E001}",
    "\u{E000}0\u{E001}", "\u{200B}", "\u{FEFF}", "...", " ,", ".", "?", "¿", "!", "¡", ":", ";", "—", "\"", "«", "»", "(", ")", "123",
    "4,5", "€", "%", "@", "http://x.y", "C:\\a", "\n", "\t", "  ", "a", "A", "ÀÉÎ", "ǅ", "ΐ", "scratch that", "I mean",
    "press enter", "new line", "mejor dicho", "Ⅻ", "ŉ", "ﬃ", "\u{0}", "\r\n", "ß", "Σ", "ς", "ı", "K", "Å", "Ω", "$1", "$0", "\\",
    "^", "*", "+", "?", "[", "]", "{", "}", "|",
];

fn frase(r: &mut Rng, max: u64) -> String {
    let n = 1 + r.next() % max;
    let mut s = String::new();
    for _ in 0..n {
        if r.chance(20) {
            // Un carácter cualquiera (válido) de vez en cuando.
            if let Some(c) = char::from_u32((r.next() % 0x2FFFF) as u32) {
                s.push(c);
            }
        } else {
            s.push_str(r.pick(PIEZAS));
        }
        if !r.chance(4) {
            s.push(' ');
        }
    }
    s
}

fn palabra(r: &mut Rng, id: i64) -> Word {
    let w = frase(r, 3);
    Word {
        id,
        word: w,
        replacement: if r.chance(2) { Some(frase(r, 3)) } else { None },
        created_at: "2026-10-22 10:00:00".into(),
        starred: r.chance(3),
        source: "manual".into(),
        variants: (0..r.next() % 3).map(|i| WordVariant { id: i as i64, heard: frase(r, 3) }).collect(),
        samples: Vec::new(),
    }
}

/// Tokens «tipo Parakeet» a partir de un texto (trozos de 1-4 caracteres; los que abren palabra
/// llevan espacio delante), con tiempos crecientes que pueden pasarse del audio.
fn tokens(r: &mut Rng, text: &str) -> Timed {
    let mut toks = Vec::new();
    let mut t = 0.0f32;
    let mut cur = String::new();
    for c in text.chars() {
        cur.push(c);
        if c == ' ' || r.chance(3) {
            let step = (r.next() % 40) as f32 * 0.01;
            toks.push(TimedToken { text: std::mem::take(&mut cur), start: t, end: t + step });
            t += step;
        }
    }
    if !cur.is_empty() {
        toks.push(TimedToken { text: cur, start: t, end: t + 0.08 });
    }
    Timed { text: text.to_string(), tokens: toks }
}

/// Lo que el hilo `pronto-transcribe` hace con lo que devuelve el motor (`state.rs:534-560`,
/// `:570-611` sin pegar) más lo que hace `pronto-transform` con lo que devuelve Ollama
/// (`state.rs:714-720`), en funciones públicas.
fn caso(r: &mut Rng, bank: &[crate::voiceprint::BankWord], audio: &[f32]) -> String {
    let raw = frase(r, 25);
    let words: Vec<Word> = (0..r.next() % 6).map(|i| palabra(r, i as i64)).collect();
    let snippets: Vec<Snippet> = (0..r.next() % 4)
        .map(|i| Snippet { id: i as i64, trigger: frase(r, 3), expansion: frase(r, 6), created_at: String::new() })
        .collect();
    let cleanup = *r.pick(&[CleanupLevel::None, CleanupLevel::Light, CleanupLevel::Medium, CleanupLevel::High]);
    let tone = *r.pick(&[Tone::Formal, Tone::Casual, Tone::VeryCasual, Tone::Excited]);
    let press_enter = r.chance(2);
    let timed = tokens(r, &raw);
    let (heard, _) = crate::voiceprint::correct_with_bank(bank, audio, &timed);
    let o = crate::text::Options { cleanup, press_enter, tone, words: &words, snippets: &snippets };
    let p = crate::text::process(&heard, &o);
    let _ = crate::polish::accept(&p.text, &frase(r, 25));
    let _ = crate::text::pronounce_status(&words.first().map(|w| w.word.clone()).unwrap_or_default(), None, &raw);
    let _ = crate::whisper::prompt_for(&crate::state::vocabulary(&words));
    let sel = frase(r, 30);
    let out = if r.chance(3) { format!("<think>{}</think>{}", frase(r, 5), frase(r, 30)) } else { frase(r, 30) };
    for k in [crate::ollama::TransformKind::Rewrite, crate::ollama::TransformKind::Shrink, crate::ollama::TransformKind::Free] {
        let _ = crate::ollama::accept_transform(k, &sel, &out);
    }
    format!("raw={raw:?} words={:?} snippets={:?} cleanup={cleanup:?} tone={tone:?} enter={press_enter} sel={sel:?} out={out:?}", words.iter().map(|w| (&w.word, &w.replacement, w.variants.iter().map(|v| &v.heard).collect::<Vec<_>>())).collect::<Vec<_>>(), snippets.iter().map(|s| (&s.trigger, &s.expansion)).collect::<Vec<_>>())
}

/// Hueco del informe -b: ¿hay un pánico alcanzable en lo que corren `pronto-transcribe` y
/// `pronto-transform`? (sin `catch_unwind`: `Cmd::Finished` no llegaría y `active` quedaría puesta).
#[test]
fn rompedor_carga_fuzz_tuberia_sin_panico() {
    let iters: usize = std::env::var("PRONTO_ROMPEDOR_ITERS").ok().and_then(|v| v.parse().ok()).unwrap_or(3000);
    let seed: u64 = std::env::var("PRONTO_ROMPEDOR_SEED").ok().and_then(|v| v.parse().ok()).unwrap_or(22_10_2026);
    // Banco de la huella con una muestra sintética, para que `correct_with_bank` llegue al DTW.
    let tone = |f: f32, n: usize| (0..n).map(|i| (i as f32 * f * 6.283 / 16_000.0).sin() * 0.3).collect::<Vec<f32>>();
    let tpl = Arc::new(crate::voiceprint::Template::from_audio(&tone(440.0, 8000)).expect("muestra"));
    let mut audio = tone(300.0, 16_000);
    audio.extend(tone(440.0, 8000));
    audio.extend(tone(300.0, 8000));
    let bank = vec![crate::voiceprint::BankWord { write: "BeltranLabs".into(), replaced: false, forms: vec!["beltranlabs".into()], templates: vec![tpl] }];

    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let mut r = Rng(seed);
    let mut fallos: Vec<(String, String)> = Vec::new();
    let t0 = Instant::now();
    for i in 0..iters {
        let mut desc = String::new();
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut rr = Rng(r.next() ^ i as u64);
            desc = caso(&mut rr, if i % 10 == 0 { &bank } else { &[] }, &audio);
        }));
        if let Err(e) = res {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            fallos.push((msg, desc));
            if fallos.len() >= 5 {
                break;
            }
        }
    }
    std::panic::set_hook(prev);
    println!("FUZZ: {iters} casos en {:.1} s, {} pánicos", t0.elapsed().as_secs_f64(), fallos.len());
    for (m, d) in &fallos {
        println!("PANICO: {m}\n  caso: {d}");
    }
    assert!(fallos.is_empty(), "pánico en la tubería del dictado/transform: {:?}", fallos.first());
}

// ------------------------------------------------------------------ 2. motor: línea base

/// Regla 3: 15 s de audio, motor solo. p50/p95 y memoria.
#[test]
fn rompedor_carga_motor_linea_base() {
    let m0 = mem();
    let Some((e, s)) = motor() else { return };
    let m1 = mem();
    let runs: usize = std::env::var("PRONTO_ROMPEDOR_RUNS").ok().and_then(|v| v.parse().ok()).unwrap_or(15);
    let mut v = Vec::new();
    for _ in 0..runs {
        let t = Instant::now();
        e.transcribe_timed(&s).unwrap();
        v.push(ms(t));
    }
    let m2 = mem();
    println!("BASE 15 s ({:.1} s de audio): {}", s.len() as f64 / 16_000.0, resumen(&v));
    println!("RSS antes de cargar {:.0} MB · motor cargado {:.0} MB · tras {runs} dictados {:.0} MB (pico {:.0}, privada {:.0}) · handles {}→{}", m0.0, m1.0, m2.0, m2.1, m2.2, m0.3, m2.3);
    assert!(pct(&v, 50.0) <= 1068.0, "regla 3 sin carga: {}", resumen(&v));
}

// ------------------------------------------------------------------ 3. reintento largo + dictado

/// «Reintentar transcripción» de un dictado largo (`commands.rs` `retry_history` →
/// `state::retry_processed` → `Engine::transcribe_timed_polite`). Antes cogía `busy` para todo el
/// audio (trozos de 60 s seguidos sin soltar el cerrojo) y un dictado pulsado durante el reintento
/// esperaba a que acabara entero (90 s con 20 min de audio). Arreglado (rompedor 22-oct-c, carga
/// MEDIA 1): el reintento suelta el motor entre trozos y cede a los dictados, así que un dictado
/// espera como mucho lo que quede del trozo de 60 s en curso.
///
/// `PRONTO_ROMPEDOR_LARGO_S` (por defecto 300 s = 5 trozos; 1200 para el caso de 20 min).
#[test]
fn rompedor_carga_dictado_durante_reintento_largo() {
    let Some((e, s)) = motor() else { return };
    let secs: usize = std::env::var("PRONTO_ROMPEDOR_LARGO_S").ok().and_then(|v| v.parse().ok()).unwrap_or(300);
    let audio = largo(&s, secs);
    // Lo que tarda un trozo de 60 s con la CPU de este momento: es lo máximo que debe esperar un dictado.
    let sesenta = largo(&s, 60);
    let t = Instant::now();
    e.transcribe_timed(&sesenta).unwrap();
    let trozo_ms = ms(t);
    // Ronda «solo» de este momento (la CPU la comparten otros): referencia de un dictado de 15 s.
    let mut solo = Vec::new();
    for _ in 0..3 {
        let t = Instant::now();
        e.transcribe_timed(&s).unwrap();
        solo.push(ms(t));
    }
    let (m0, _, _, _) = mem();
    let sampler = Sampler::start();
    let e2 = e.clone();
    let reintento = std::thread::Builder::new()
        .name("reintento".into())
        .spawn(move || {
            let t = Instant::now();
            let r = e2.transcribe_timed_polite(&audio).unwrap();
            (ms(t), r.text.split_whitespace().count())
        })
        .unwrap();
    std::thread::sleep(Duration::from_millis(1500));
    let mut v = Vec::new();
    let mut fin_dictados = 0.0;
    let t_dictados = Instant::now();
    for _ in 0..5 {
        let t = Instant::now();
        e.transcribe_timed(&s).unwrap();
        v.push(ms(t));
        fin_dictados = ms(t_dictados);
    }
    let (rt, palabras) = reintento.join().unwrap();
    let pico = sampler.finish();
    println!("REINTENTO de {secs} s de audio: {rt:.0} ms, {palabras} palabras (un trozo de 60 s solo: {trozo_ms:.0} ms)");
    println!("SOLO esta ronda: {}", resumen(&solo));
    println!("DICTADO 15 s pulsado 1,5 s después del reintento: {} (primero {:.0} ms; los 5 acabaron a los {fin_dictados:.0} ms)", resumen(&v), v[0]);
    println!("  uno a uno: {:?} ms", v.iter().map(|x| x.round()).collect::<Vec<_>>());
    println!("RSS antes {m0:.0} MB · pico durante el reintento {pico:.0} MB (+{:.0})", pico - m0);
    // Regla 3 + un trozo: el primer dictado puede caer justo al empezar un trozo de 60 s.
    let base = pct(&solo, 100.0).max(1068.0);
    assert!(v[0] <= base + trozo_ms * 1.2, "regla 3: el dictado esperó al reintento: {:.0} ms (trozo {trozo_ms:.0} ms)", v[0]);
    // Los siguientes no esperan: el reintento no retoma el motor justo detrás de un dictado.
    assert!(pct(&v[1..], 100.0) <= base * 1.5, "los dictados siguientes siguen esperando: {}", resumen(&v));
    // Los 5 dictados acabaron antes que el reintento: no tuvieron que esperar a que acabara.
    assert!(fin_dictados < rt, "los dictados acabaron después del reintento: {fin_dictados:.0} frente a {rt:.0} ms");
}

/// La segunda pasada del reintento cuando la huella oye una palabra (`state::transcribe_processed_as`
/// con Whisper) iba por `transcribe_timed_precise`, que coge `busy` para todo el audio: un dictado
/// pulsado durante un reintento de 20 min esperaba ~1 min (seguridad 24-oct, MEDIA 1). Ahora va por
/// `transcribe_timed_polite_precise`, que cede entre trozos como el reintento normal.
#[test]
fn rompedor_carga_dictado_durante_reintento_con_huella() {
    let Some((e, s)) = motor() else { return };
    let audio = largo(&s, 240);
    let sesenta = largo(&s, 60);
    let t = Instant::now();
    e.transcribe_timed(&sesenta).unwrap();
    let trozo_ms = ms(t);
    let mut solo = Vec::new();
    for _ in 0..3 {
        let t = Instant::now();
        e.transcribe_timed(&s).unwrap();
        solo.push(ms(t));
    }
    let e2 = e.clone();
    let reintento = std::thread::spawn(move || {
        let t = Instant::now();
        e2.transcribe_timed_polite_precise(&audio).unwrap();
        ms(t)
    });
    // Más que `RETRY_GRACE` (3 s): el reintento ya va por su primer trozo cuando se pulsa.
    std::thread::sleep(Duration::from_millis(5000));
    let t = Instant::now();
    e.transcribe_timed(&s).unwrap();
    let dictado = ms(t);
    let rt = reintento.join().unwrap();
    println!("REINTENTO CON HUELLA de 240 s: {rt:.0} ms · dictado pulsado a los 5 s: {dictado:.0} ms (trozo de 60 s: {trozo_ms:.0} ms)");
    let base = pct(&solo, 100.0).max(1068.0);
    assert!(dictado <= base + trozo_ms * 1.2, "regla 3: el dictado esperó al reintento con huella: {dictado:.0} ms (trozo {trozo_ms:.0} ms)");
    assert!(dictado < rt, "el dictado acabó después del reintento: {dictado:.0} frente a {rt:.0} ms");
}

// ------------------------------------------------------------------ 4. todo a la vez

/// Dictado (con la marca `active` puesta al «pulsar», como `state.rs:357`) + fase 2 de una reunión
/// (`transcribe_background_timed`, trozos de 30 s sin parar) + pronunciaciones del hub en bucle
/// (`pronounce.rs:183`: `transcribe`, cola con prioridad, 1,5 s de audio) + la tubería de texto
/// en otro hilo (lo que haría un Transform/hub en paralelo). p50/p95 del dictado.
#[test]
#[ignore = "rompedor: con la fase 2 en marcha el dictado pasa de 1068 ms (BAJA 8 del informe -b, sin cambios)"]
fn rompedor_carga_todo_a_la_vez() {
    let Some((e, s)) = motor() else { return };
    let active = Arc::new(AtomicBool::new(false));
    // `Engine` está detrás de `Arc` sin `&mut`: compartir la marca es posible una sola vez (OnceLock).
    e.share_dictation_flag(active.clone());

    // Solo: referencia de esta ronda (la CPU la comparten otros agentes).
    let mut solo = Vec::new();
    for _ in 0..8 {
        let t = Instant::now();
        e.transcribe_timed(&s).unwrap();
        solo.push(ms(t));
    }

    let parar = Arc::new(AtomicBool::new(false));
    let trozos = Arc::new(AtomicUsize::new(0));
    let pron = Arc::new(AtomicUsize::new(0));
    let trozo = largo(&s, 30);
    let corto: Vec<f32> = s[..24_000].to_vec();
    let mut hilos = Vec::new();
    // `PRONTO_ROMPEDOR_SIN=fase2,pron,texto` quita cargas para ver cuál pesa.
    let sin = std::env::var("PRONTO_ROMPEDOR_SIN").unwrap_or_default();
    if !sin.contains("fase2") {
        let (e, parar, trozos) = (e.clone(), parar.clone(), trozos.clone());
        hilos.push(std::thread::spawn(move || {
            while !parar.load(Ordering::SeqCst) {
                e.transcribe_background_timed(&trozo).unwrap();
                trozos.fetch_add(1, Ordering::SeqCst);
            }
        }));
    }
    if !sin.contains("pron") {
        let (e, parar, pron) = (e.clone(), parar.clone(), pron.clone());
        hilos.push(std::thread::spawn(move || {
            while !parar.load(Ordering::SeqCst) {
                e.transcribe(&corto).unwrap();
                pron.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(300));
            }
        }));
    }
    if !sin.contains("texto") {
        let parar = parar.clone();
        hilos.push(std::thread::spawn(move || {
            let mut r = Rng(7);
            while !parar.load(Ordering::SeqCst) {
                let _ = caso(&mut r, &[], &[]);
            }
        }));
    }
    std::thread::sleep(Duration::from_millis(2500));
    let m0 = mem();
    let mut v = Vec::new();
    for i in 0..12 {
        // Se pulsa el atajo, se «habla» 15 s (aquí 1-2 s para no eternizar) y se suelta.
        active.store(true, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(1000 + (i % 3) * 500));
        let t = Instant::now();
        e.transcribe_timed(&s).unwrap();
        v.push(ms(t));
        active.store(false, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(400));
    }
    parar.store(true, Ordering::SeqCst);
    for h in hilos {
        h.join().unwrap();
    }
    let m1 = mem();
    println!("SOLO esta ronda: {}", resumen(&solo));
    println!("SIN [{sin}]");
    println!("TODO A LA VEZ: {} · fase 2: {} trozos de 30 s · pronunciaciones: {}", resumen(&v), trozos.load(Ordering::SeqCst), pron.load(Ordering::SeqCst));
    println!("RSS {:.0}→{:.0} MB, handles {}→{}", m0.0, m1.0, m0.3, m1.3);
    assert!(pct(&v, 95.0) <= 1068.0, "regla 3 con todo a la vez: {}", resumen(&v));
}

// ------------------------------------------------------------------ 5. reunión larga: memoria

/// Fase 2 de una reunión larga: trozos de 30 s por `transcribe_background_timed`. Memoria (RSS) y
/// tiempo por trozo cada 20 trozos: ¿crece sin parar?
#[test]
fn rompedor_carga_reunion_larga_memoria() {
    let Some((e, s)) = motor() else { return };
    let n: usize = std::env::var("PRONTO_ROMPEDOR_TROZOS").ok().and_then(|v| v.parse().ok()).unwrap_or(120);
    let trozo = largo(&s, 30);
    let m0 = mem();
    let mut lineas = Vec::new();
    let mut v = Vec::new();
    let mut m20 = m0;
    for i in 1..=n {
        let t = Instant::now();
        e.transcribe_background_timed(&trozo).unwrap();
        v.push(ms(t));
        if i % 20 == 0 {
            let m = mem();
            if i == 20 {
                m20 = m;
            }
            lineas.push(format!("{i:>4} trozos ({:>3} min de audio): RSS {:.0} MB, privada {:.0} MB, handles {}, trozo p50 {:.0} ms", i / 2, m.0, m.2, m.3, pct(&v[v.len() - 20..], 50.0)));
        }
    }
    let m1 = mem();
    println!("REUNION {} min en trozos de 30 s · {} · RSS {:.0}→{:.0} MB (privada {:.0}→{:.0})", n / 2, resumen(&v), m0.0, m1.0, m0.2, m1.2);
    for l in &lineas {
        println!("  {l}");
    }
    // Los primeros trozos calientan el asignador de onnxruntime (+~400 MB una vez); después no debe crecer.
    assert!(m1.2 - m20.2 < 50.0, "la memoria privada siguió creciendo: {:.0} MB desde el trozo 20", m1.2 - m20.2);
}

// ------------------------------------------------------------------ 6. cargas del motor a la vez

/// Cada cambio de motor desde el hub lanza un hilo `pronto-asr-load` (`asr.rs:205-249`) que carga
/// el modelo entero; el número de generación solo descarta la carga obsoleta **al terminar**
/// (`asr.rs:221`). Cambiar de motor N veces seguidas = N modelos en memoria a la vez. Aquí, lo
/// que hace cada hilo (`create_any` → Parakeet), en paralelo, con el pico de memoria.
#[test]
fn rompedor_carga_cargas_del_motor_a_la_vez() {
    let Ok(dir) = std::env::var("PRONTO_ROMPEDOR_MODELO") else {
        println!("OMITIDO: falta PRONTO_ROMPEDOR_MODELO");
        return;
    };
    let n: usize = std::env::var("PRONTO_ROMPEDOR_CARGAS").ok().and_then(|v| v.parse().ok()).unwrap_or(4);
    let base = mem().0;
    // Una sola carga (referencia).
    let sampler = Sampler::start();
    let t = Instant::now();
    let uno = Engine::load_for_tests(&dir).expect("cargar");
    let t1 = ms(t);
    drop(uno);
    let pico1 = sampler.finish();
    std::thread::sleep(Duration::from_millis(500));
    let base2 = mem().0;
    // N a la vez.
    let sampler = Sampler::start();
    let t = Instant::now();
    let hs: Vec<_> = (0..n)
        .map(|_| {
            let d = dir.clone();
            std::thread::spawn(move || Engine::load_for_tests(&d).map(|_| ()))
        })
        .collect();
    let ok = hs.into_iter().map(|h| h.join().unwrap().is_ok()).filter(|b| *b).count();
    let tn = ms(t);
    let picon = sampler.finish();
    println!("CARGA x1: {t1:.0} ms, pico {pico1:.0} MB (+{:.0} sobre {base:.0})", pico1 - base);
    println!("CARGA x{n} a la vez: {tn:.0} ms, {ok} bien, pico {picon:.0} MB (+{:.0} sobre {base2:.0})", picon - base2);
}

// ------------------------------------------------------------------ 7. 1000 cancelaciones

/// Lo que hace Esc a mitad de dictado (`state.rs:432-437` → `finish_recording` → `Recorder::stop`):
/// abrir y cerrar el micrófono 1000 veces. Hilos, handles y memoria antes/después. El audio no se
/// guarda en ningún sitio.
#[test]
fn rompedor_carga_1000_cancelaciones_del_microfono() {
    if std::env::var("PRONTO_ROMPEDOR_MIC").as_deref() != Ok("1") {
        println!("OMITIDO: PRONTO_ROMPEDOR_MIC=1 para abrir el micrófono");
        return;
    }
    let n: usize = std::env::var("PRONTO_ROMPEDOR_CICLOS").ok().and_then(|v| v.parse().ok()).unwrap_or(1000);
    let level: crate::audio::LevelFn = Arc::new(|_| {});
    // Calentar (COM, cpal) antes de medir.
    for _ in 0..5 {
        drop(crate::audio::Recorder::start(None, level.clone()).expect("micro").stop());
    }
    let (m0, h0) = (mem(), threads());
    let mut v = Vec::new();
    let mut fallos = 0;
    let mut puntos = Vec::new();
    for i in 1..=n {
        let t = Instant::now();
        match crate::audio::Recorder::start(None, level.clone()) {
            Ok(r) => drop(r.stop()),
            Err(_) => fallos += 1,
        }
        v.push(ms(t));
        if i % 250 == 0 {
            let m = mem();
            puntos.push(format!("{i}: RSS {:.0} MB, privada {:.0} MB, handles {}, hilos {}", m.0, m.2, m.3, threads()));
        }
    }
    // Abandonar sin `stop` (el `Drop` de `Recorder`): ¿termina el hilo?
    for _ in 0..50 {
        drop(crate::audio::Recorder::start(None, level.clone()).expect("micro"));
    }
    std::thread::sleep(Duration::from_millis(1500));
    let (m1, h1) = (mem(), threads());
    println!("CANCELACIONES {n}: abrir+cerrar {} · fallos {fallos}", resumen(&v));
    for p in &puntos {
        println!("  {p}");
    }
    println!("ANTES RSS {:.0} MB privada {:.0} MB handles {} hilos {h0} · DESPUÉS (+50 abandonados) RSS {:.0} MB privada {:.0} MB handles {} hilos {h1}", m0.0, m0.2, m0.3, m1.0, m1.2, m1.3);
    assert!(h1 <= h0 + 5, "hilos {h0} → {h1}");
    assert!(m1.3 <= m0.3 + 50, "handles {} → {}", m0.3, m1.3);
}

// ------------------------------------------------------------------ 8. guardar con 100 000 dictados

/// Lo que hace cada dictado ANTES de pegar (`state.rs:655` → `keep_dictation` → `add_history` +
/// `save_audio`: WAV + `set_has_audio`; antes también `purge_audio`, que recorría `history` entera
/// con `datetime(created_at)`: 26 ms con 100 000 filas y hasta 195 ms con el hub pidiendo). Arreglado
/// (rompedor 22-oct-c, carga BAJA 2): la purga se hace al arrancar, al cambiar `keepAudioDays` y
/// cada hora (`state::spawn_audio_purge`); `purge_audio sola` abajo es lo que ya no se paga en cada
/// dictado. Con 100 000 dictados en la base y el hub pidiendo estadísticas, insights y la primera
/// página sin parar en otro hilo.
#[test]
fn rompedor_carga_guardar_dictado_con_100k_y_hub() {
    let n: i64 = std::env::var("PRONTO_ROMPEDOR_FILAS").ok().and_then(|v| v.parse().ok()).unwrap_or(100_000);
    let dir = std::env::temp_dir().join(format!("pronto-rompedor-carga-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("pronto2.db");
    let db = Arc::new(crate::db::Db::open_at(&path).expect("abrir base temporal"));
    {
        let mut conn = db.conn.lock();
        let tx = conn.transaction().unwrap();
        {
            let mut st = tx
                .prepare("INSERT INTO history (created_at, text, raw_text, app, duration_ms, words, has_audio) VALUES (?1, ?2, ?2, 'Code', 15000, 40, ?3)")
                .unwrap();
            let now = chrono::Local::now();
            for i in 0..n {
                // Una fila por cada ~5 min hacia atrás; las de la última semana con audio.
                let t = now - chrono::Duration::minutes(5 * (n - i));
                let reciente = (now - t).num_days() < 7;
                st.execute(rusqlite::params![t.to_rfc3339(), format!("dictado {i} con unas cuantas palabras de relleno para que pese"), reciente as i64]).unwrap();
            }
        }
        tx.commit().unwrap();
    }
    let samples: Vec<f32> = (0..16_000 * 15).map(|i| (i as f32 * 0.05).sin() * 0.2).collect();
    let guardar = |db: &crate::db::Db| -> (f64, f64, f64) {
        let t = Instant::now();
        let item = db
            .add_history(&crate::db::NewHistory { polished: false, text: "hola", raw: "hola", app: "Code", duration_ms: 15000, words: 1, fixes: Default::default(), transform: None })
            .unwrap();
        let a = ms(t);
        let t2 = Instant::now();
        assert!(crate::state::save_audio(db, 7, &item, &samples));
        let b = ms(t2);
        (a, b, ms(t))
    };
    // Sin hub.
    let mut solo = Vec::new();
    for _ in 0..30 {
        solo.push(guardar(&db).2);
    }
    let t = Instant::now();
    let _ = db.purge_audio(7).unwrap();
    let purga = ms(t);
    // Con el hub pidiendo sin parar.
    let parar = Arc::new(AtomicBool::new(false));
    let pedidas = Arc::new(AtomicUsize::new(0));
    let hub = {
        let (db, parar, pedidas) = (db.clone(), parar.clone(), pedidas.clone());
        std::thread::spawn(move || {
            let mut v = Vec::new();
            while !parar.load(Ordering::SeqCst) {
                let t = Instant::now();
                let _ = db.stats().unwrap();
                let _ = db.insights().unwrap();
                let _ = db.list_history(None, 50, 0).unwrap();
                v.push(ms(t));
                pedidas.fetch_add(1, Ordering::SeqCst);
            }
            v
        })
    };
    let mut con = Vec::new();
    for _ in 0..30 {
        con.push(guardar(&db).2);
        std::thread::sleep(Duration::from_millis(50));
    }
    parar.store(true, Ordering::SeqCst);
    let hubv = hub.join().unwrap();
    println!("FILAS {n} · purge_audio sola {purga:.1} ms");
    println!("GUARDAR (antes de pegar) sin hub: {}", resumen(&solo));
    println!("GUARDAR con hub: {} · hub (stats+insights+página) {}", resumen(&con), resumen(&hubv));
    drop(db);
    let _ = std::fs::remove_dir_all(&dir);
    if let Some(a) = crate::db::audio_dir() {
        let _ = std::fs::remove_dir_all(a);
    }
}

// ------------------------------------------------------------------ 9. huella de voz con diccionario

/// La huella de voz (`voiceprint::correct`, `state.rs:538`) corre tras el motor y antes de pegar:
/// DTW de cada muestra del diccionario contra todo el dictado (O(muestra × dictado)). El test de la
/// regla 3 la mide con el diccionario vacío (`asr.rs:670`). Aquí con K muestras de 1,2 s.
#[test]
fn rompedor_carga_huella_de_voz_con_diccionario() {
    let ks: Vec<usize> = std::env::var("PRONTO_ROMPEDOR_MUESTRAS").ok().map(|v| v.split(',').filter_map(|x| x.parse().ok()).collect()).unwrap_or(vec![2, 30]);
    let secs: Vec<usize> = std::env::var("PRONTO_ROMPEDOR_HUELLA_S").ok().map(|v| v.split(',').filter_map(|x| x.parse().ok()).collect()).unwrap_or(vec![15]);
    let mut r = Rng(5);
    let voz = |r: &mut Rng, n: usize| -> Vec<f32> {
        let f1 = 200.0 + (r.next() % 600) as f32;
        let f2 = 900.0 + (r.next() % 1500) as f32;
        (0..n).map(|i| {
            let t = i as f32 / 16_000.0;
            let env = (std::f32::consts::PI * i as f32 / n as f32).sin();
            env * (0.3 * (6.283 * f1 * t).sin() + 0.2 * (6.283 * f2 * t).sin()) + 0.01 * ((r.next() % 1000) as f32 / 500.0 - 1.0)
        }).collect()
    };
    for &secs in &secs {
        let audio: Vec<f32> = (0..secs).flat_map(|_| voz(&mut r, 16_000)).collect();
        // Texto con tokens que cubren el audio (una palabra cada 0,4 s).
        let nw = secs * 5 / 2;
        let toks: Vec<TimedToken> = (0..nw).map(|i| TimedToken { text: format!(" palabra{i}"), start: i as f32 * 0.4, end: i as f32 * 0.4 + 0.35 }).collect();
        let text = toks.iter().map(|t| t.text.trim()).collect::<Vec<_>>().join(" ");
        let timed = Timed { text, tokens: toks };
        for &k in &ks {
            let bank: Vec<crate::voiceprint::BankWord> = (0..k.div_ceil(3))
                .map(|w| crate::voiceprint::BankWord {
                    write: format!("Marca{w}"),
                    replaced: false,
                    forms: vec![format!("marca{w}")],
                    templates: (0..3.min(k - 3 * w)).map(|_| Arc::new(crate::voiceprint::Template::from_audio(&voz(&mut r, 19_200)).unwrap())).collect(),
                })
                .collect();
            let mut v = Vec::new();
            for _ in 0..3 {
                let t = Instant::now();
                let _ = crate::voiceprint::correct_with_bank(&bank, &audio, &timed);
                v.push(ms(t));
            }
            println!("HUELLA {secs} s de dictado, {k} muestras: {}", resumen(&v));
        }
    }
}

// ------------------------------------------------------------------ 10. Transform con Ollama colgado

/// Un Transform marca `active` y pasa a `Polishing` (`state.rs` `start_transform`); solo vuelve a
/// reposo con `Cmd::Finished`, que manda el hilo `pronto-transform` al acabar la petición a Ollama.
/// Antes el plazo era de 600 s: el dictado y la fase 2 de una reunión (`asr.rs`
/// `wait_dictation_released`) quedaban parados 10 min. Arreglado (rompedor 22-oct-c, carga ALTA 1):
/// plazo propio (`ollama::TRANSFORM_TIMEOUT`, 45 s) y el atajo de dictar corta el Transform
/// (`state::interrupts_transform`, probado en `state::carga_transform`). Aquí: Ollama falso en
/// 127.0.0.1 que acepta y no contesta, `transform_text_within` (lo que corre el hilo) con el plazo
/// acortado a `PRONTO_ROMPEDOR_PLAZO_S` (3 s por defecto) y la fase 2 esperando la marca.
#[test]
fn rompedor_carga_transform_con_ollama_colgado() {
    let plazo: u64 = std::env::var("PRONTO_ROMPEDOR_PLAZO_S").ok().and_then(|v| v.parse().ok()).unwrap_or(3);
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    std::thread::spawn(move || {
        let mut vivos = Vec::new();
        for s in l.incoming().flatten() {
            vivos.push(s); // acepta, lee nada, no contesta nunca
        }
    });
    let active = Arc::new(AtomicBool::new(false));
    let engine = Arc::new(Engine::new(""));
    engine.share_dictation_flag(active.clone());
    // start_transform
    active.store(true, Ordering::SeqCst);
    let t0 = Instant::now();
    let (tx, rx) = std::sync::mpsc::channel();
    {
        let (active, url) = (active.clone(), url.clone());
        std::thread::Builder::new()
            .name("pronto-transform".into())
            .spawn(move || {
                let tr = crate::models::Transform { id: 1, name: "Formal".into(), prompt: "Reescribe el texto en un tono formal.".into(), builtin: false, hotkey_index: None };
                let r = crate::state::transform_text_within(&url, "falso", &tr, "hola que tal", Duration::from_secs(plazo));
                // Cmd::Finished → active = false
                active.store(false, Ordering::SeqCst);
                let _ = tx.send((ms(t0), r.err().map(|f| f.message)));
            })
            .unwrap();
    }
    // Fase 2 de una reunión: intenta empezar un trozo.
    let (ftx, frx) = std::sync::mpsc::channel();
    {
        let e = engine.clone();
        std::thread::spawn(move || {
            let t = Instant::now();
            let _ = e.transcribe_background_timed(&[0.0; 160]);
            let _ = ftx.send(ms(t));
        });
    }
    let margen = Duration::from_secs(plazo + 5);
    let fin = rx.recv_timeout(margen);
    let fase2 = frx.recv_timeout(Duration::from_secs(2));
    match &fin {
        Ok((t, e)) => println!("TRANSFORM terminó en {t:.0} ms: {e:?} · fase 2 esperó {fase2:?} ms"),
        Err(_) => println!("TRANSFORM sigue sin terminar a los {} s (active={}) · ¿la fase 2 empezó su trozo? {}", margen.as_secs(), active.load(Ordering::SeqCst), fase2.is_ok()),
    }
    let (t, msg) = fin.expect("con Ollama colgado, el Transform (y con él el dictado y la fase 2) sigue parado");
    assert!(t <= (plazo * 1000 + 2000) as f64, "tardó {t:.0} ms con un plazo de {plazo} s");
    assert_eq!(msg.as_deref(), Some("Ollama no respondió a tiempo"), "el mensaje no debe decir «no se pudo contactar»");
    assert!(!active.load(Ordering::SeqCst), "`active` sigue puesta");
    assert!(fase2.is_ok(), "la fase 2 no pudo empezar su trozo al volver `active` a false");
    assert!(crate::ollama::TRANSFORM_TIMEOUT <= Duration::from_secs(45));
}

// ================================================================== segunda pasada (2026-10-23)

/// Lee una petición HTTP entera (cabeceras + `Content-Length`). Devuelve el texto.
fn leer_peticion(sock: &mut std::net::TcpStream) -> String {
    use std::io::Read;
    let mut got = Vec::new();
    let mut buf = vec![0u8; 16384];
    loop {
        let n = sock.read(&mut buf).unwrap_or(0);
        got.extend_from_slice(&buf[..n]);
        let text = String::from_utf8_lossy(&got).to_string();
        if let Some(i) = text.find("\r\n\r\n") {
            let len = text
                .lines()
                .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").and_then(|v| v.trim().parse::<usize>().ok()))
                .unwrap_or(0);
            if got.len() >= i + 4 + len {
                break;
            }
        }
        if n == 0 {
            break;
        }
    }
    String::from_utf8_lossy(&got).to_string()
}

/// ¿El cliente cerró la conexión? (Ollama abandona la generación cuando el cliente se va.)
fn cliente_se_fue(sock: &std::net::TcpStream) -> bool {
    let _ = sock.set_nonblocking(true);
    let mut b = [0u8; 1];
    let r = sock.peek(&mut b);
    let _ = sock.set_nonblocking(false);
    match r {
        Ok(0) => true,
        Ok(_) => false,
        Err(e) => e.kind() != std::io::ErrorKind::WouldBlock,
    }
}

const DICTADO_PULIR: &str = "hola qué tal estás hoy te escribo para confirmar la reunión del jueves por la tarde";
const DICTADO_PULIDO: &str = "Hola, qué tal estás hoy. Te escribo para confirmar la reunión del jueves por la tarde.";
const SELECCION: &str = "oye mira que el informe de ventas lo tengo casi listo y te lo paso mañana a primera hora";

/// Ollama falso de UN hueco (como Ollama con `OLLAMA_NUM_PARALLEL=1`): atiende las peticiones de una
/// en una, por orden de llegada. Un Transform (sin `keep_alive`) tarda `lento`; un pulido
/// (`keep_alive`, `polish.rs`) tarda `rapido`. Como Ollama, si el cliente cierra la conexión
/// mientras espera o se genera, abandona esa petición. Devuelve la URL y el diario de lo atendido.
fn ollama_un_hueco(lento: Duration, rapido: Duration, t0: Instant) -> (String, Arc<parking_lot::Mutex<Vec<String>>>) {
    use std::io::Write;
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    let diario = Arc::new(parking_lot::Mutex::new(Vec::new()));
    let (tx, rx) = std::sync::mpsc::channel::<(std::net::TcpStream, bool, f64)>();
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let mut s = s;
                let req = leer_peticion(&mut s);
                let pulido = req.contains("\"keep_alive\"");
                let _ = tx.send((s, pulido, t0.elapsed().as_secs_f64()));
            });
        }
    });
    let d = diario.clone();
    std::thread::spawn(move || {
        for (mut s, pulido, llegada) in rx {
            let que = if pulido { "pulido" } else { "transform" };
            let ini = t0.elapsed().as_secs_f64();
            if cliente_se_fue(&s) {
                d.lock().push(format!("{que} (llegó {llegada:.1} s): el cliente ya se había ido a los {ini:.1} s; abandonada sin generar"));
                continue;
            }
            let dur = if pulido { rapido } else { lento };
            let fin = Instant::now() + dur;
            let mut abandonada = false;
            while Instant::now() < fin {
                if cliente_se_fue(&s) {
                    abandonada = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            let acabo = t0.elapsed().as_secs_f64();
            if abandonada {
                d.lock().push(format!("{que} (llegó {llegada:.1} s): generando de {ini:.1} a {acabo:.1} s; el cliente se fue, abandonada"));
                continue;
            }
            let resp = if pulido { DICTADO_PULIDO } else { SELECCION };
            let body = serde_json::json!({ "response": resp }).to_string();
            let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            d.lock().push(format!("{que} (llegó {llegada:.1} s): generando de {ini:.1} a {acabo:.1} s; respondida"));
        }
    });
    (url, diario)
}

fn ajustes_pulido(url: &str) -> crate::models::Settings {
    crate::models::Settings { ai_polish: true, ollama_url: url.into(), ..Default::default() }
}

fn transform_formal() -> crate::models::Transform {
    crate::models::Transform { id: 1, name: "Formal".into(), prompt: "Reescribe el texto en un tono formal.".into(), builtin: false, hotkey_index: None }
}

/// Encargo 2.ª pasada: «el atajo de dictar cancela un Transform». `cancel_transform`
/// (`state.rs:546-554`) solo pone la marca `cancelled`; el hilo `pronto-transform` sigue dentro de
/// `ollama::generate_timeout` con la conexión abierta hasta que Ollama responde o vencen los 45 s
/// (`run_transform_job` mira la marca DESPUÉS, `state.rs:899-900`). Ollama sigue generando para un
/// Transform que ya nadie quiere. Con Ollama de un hueco, el pulido del dictado que se empezó
/// justo al cancelar, y el Transform que se lance a continuación, hacen cola detrás.
///
/// Aquí, con las funciones reales: A = `state::transform_text_within` con `TRANSFORM_TIMEOUT`
/// (lo que corre el hilo cancelado), 2,5 s después el pulido del dictado nuevo (`polish::apply`,
/// plazo de fábrica 6 s) y después otro Transform B. El Ollama falso tarda 30 s en un Transform y
/// 0,3 s en un pulido.
#[test]
#[ignore = "rompedor: cancelar un Transform no suelta Ollama; el pulido del dictado nuevo y el siguiente Transform hacen cola"]
fn rompedor_carga_cancelar_transform_no_suelta_ollama() {
    let lento = Duration::from_secs(std::env::var("PRONTO_ROMPEDOR_LENTO_S").ok().and_then(|v| v.parse().ok()).unwrap_or(30));
    let t0 = Instant::now();
    let (url, diario) = ollama_un_hueco(lento, Duration::from_millis(300), t0);
    let tr = transform_formal();

    // Referencia: el pulido con Ollama libre.
    let t = Instant::now();
    let solo = crate::polish::apply(&ajustes_pulido(&url), DICTADO_PULIR, Tone::Formal, 0);
    let pulido_solo = ms(t);
    println!("PULIDO con Ollama libre: {pulido_solo:.0} ms, pulido={} fallo={}", solo.polished, solo.failed);

    // A: el Transform que el usuario cancela pulsando el atajo de dictar.
    let a = {
        let (url, tr) = (url.clone(), tr.clone());
        std::thread::spawn(move || {
            let t = Instant::now();
            let r = crate::state::transform_text_within(&url, "falso", &tr, SELECCION, crate::ollama::TRANSFORM_TIMEOUT);
            (ms(t), t0.elapsed().as_secs_f64(), r.map(|_| ()).map_err(|f| f.message))
        })
    };
    std::thread::sleep(Duration::from_secs(1));
    // cancel_transform: marca puesta, active=false, empieza el dictado. Nada corta la petición de A.
    let t_cancel = t0.elapsed().as_secs_f64();
    // El dictado nuevo (1-2 s de grabación + motor) llega al pulido:
    std::thread::sleep(Duration::from_millis(1500));
    let t = Instant::now();
    let d = crate::polish::apply(&ajustes_pulido(&url), DICTADO_PULIR, Tone::Formal, 0);
    let pulido_d = ms(t);
    println!("CANCELADO A a los {t_cancel:.1} s · PULIDO del dictado nuevo: {pulido_d:.0} ms (solo: {pulido_solo:.0}), pulido={} fallo={}", d.polished, d.failed);

    // B: el usuario vuelve a lanzar un Transform.
    let t = Instant::now();
    let t_b = t0.elapsed().as_secs_f64();
    let b = crate::state::transform_text_within(&url, "falso", &tr, SELECCION, crate::ollama::TRANSFORM_TIMEOUT);
    let tb = ms(t);
    println!("TRANSFORM B lanzado a los {t_b:.1} s: {tb:.0} ms → {:?} (solo tardaría {} s)", b.as_ref().map(|_| "ok").map_err(|f| f.message.clone()), lento.as_secs());
    let (ta, fin_a, ra) = a.join().unwrap();
    println!("TRANSFORM A (cancelado): siguió {ta:.0} ms, hasta los {fin_a:.1} s → {ra:?}");
    for l in diario.lock().iter() {
        println!("  ollama: {l}");
    }
    assert!(!d.failed && pulido_d < 2000.0, "el pulido del dictado nuevo hizo cola tras el Transform cancelado: {pulido_d:.0} ms, sin pulir={}", d.failed);
    assert!(b.is_ok(), "el Transform siguiente hizo cola tras el cancelado y venció su plazo");
}

/// Pulido activado y Ollama colgado (acepta y no contesta) o parado (puerto cerrado): cada dictado
/// espera el plazo entero antes de pegar (`state.rs:771` `polish::apply` dentro de
/// `finish_dictation`, antes de `paste`). ¿Aprende tras el primer fallo o paga el plazo siempre?
#[test]
fn rompedor_carga_pulido_con_ollama_colgado_o_parado() {
    let n: usize = std::env::var("PRONTO_ROMPEDOR_DICTADOS").ok().and_then(|v| v.parse().ok()).unwrap_or(4);
    // Colgado.
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let colgado = format!("http://{}", l.local_addr().unwrap());
    std::thread::spawn(move || {
        let mut vivos = Vec::new();
        for s in l.incoming().flatten() {
            vivos.push(s);
        }
    });
    // Parado: un puerto que se acaba de cerrar.
    let parado = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://{}", l.local_addr().unwrap())
    };
    let mut v_colgado = Vec::new();
    let mut v_parado = Vec::new();
    for _ in 0..n {
        let t = Instant::now();
        let o = crate::polish::apply(&ajustes_pulido(&colgado), DICTADO_PULIR, Tone::Formal, 0);
        assert!(o.failed && o.text == DICTADO_PULIR);
        v_colgado.push(ms(t));
        let t = Instant::now();
        let o = crate::polish::apply(&ajustes_pulido(&parado), DICTADO_PULIR, Tone::Formal, 0);
        assert!(o.failed && o.text == DICTADO_PULIR);
        v_parado.push(ms(t));
    }
    println!("PULIDO con Ollama COLGADO, {n} dictados seguidos (lo que espera cada uno antes de pegar): {:?}", v_colgado.iter().map(|x| x.round()).collect::<Vec<_>>());
    println!("PULIDO con Ollama PARADO (puerto cerrado), {n} dictados: {:?}", v_parado.iter().map(|x| x.round()).collect::<Vec<_>>());
    // Regla 3 + regla 4: tras el primer fallo, los siguientes no deberían volver a esperar el plazo.
    assert!(pct(&v_colgado[1..], 100.0) < 1068.0, "cada dictado espera el plazo del pulido: {}", resumen(&v_colgado));
    assert!(pct(&v_parado[1..], 100.0) < 1068.0, "cada dictado espera la conexión rechazada: {}", resumen(&v_parado));
}

/// «Reintentar transcripción» (`transcribe_timed_polite`) mientras el usuario dicta frase tras frase
/// con pausas de 1,5 s (menos que `RETRY_GRACE`, 3 s): ¿pasa hambre? ¿se cuelga? El dictado se
/// simula como en la app: `active` puesta al pulsar, 1,5 s de grabación, motor, `active` fuera.
#[test]
fn rompedor_carga_reintento_con_dictados_seguidos() {
    let Some((e, s)) = motor() else { return };
    let active = Arc::new(AtomicBool::new(false));
    e.share_dictation_flag(active.clone());
    let secs: usize = std::env::var("PRONTO_ROMPEDOR_LARGO_S").ok().and_then(|v| v.parse().ok()).unwrap_or(180);
    let durante: u64 = std::env::var("PRONTO_ROMPEDOR_DURANTE_S").ok().and_then(|v| v.parse().ok()).unwrap_or(40);
    let audio = Arc::new(largo(&s, secs));
    let mut solo = Vec::new();
    for _ in 0..3 {
        let t = Instant::now();
        e.transcribe_timed(&s).unwrap();
        solo.push(ms(t));
    }
    // Reintento solo (referencia); se espera la gracia de 3 s tras el último dictado.
    std::thread::sleep(Duration::from_millis(3100));
    let t = Instant::now();
    e.transcribe_timed_polite(&audio).unwrap();
    let reintento_solo = ms(t);
    // Reintento con dictados seguidos durante `durante` s.
    let t0 = Instant::now();
    let r = {
        let (e, audio) = (e.clone(), audio.clone());
        std::thread::spawn(move || {
            e.transcribe_timed_polite(&audio).unwrap();
            ms(t0)
        })
    };
    std::thread::sleep(Duration::from_millis(300));
    let mut v = Vec::new();
    while t0.elapsed() < Duration::from_secs(durante) {
        active.store(true, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(1500));
        let t = Instant::now();
        e.transcribe_timed(&s).unwrap();
        v.push(ms(t));
        active.store(false, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(1500));
    }
    let fin_dictados = ms(t0);
    let rt = r.join().unwrap();
    println!("SOLO esta ronda (dictado 15 s): {}", resumen(&solo));
    println!("REINTENTO {secs} s solo: {reintento_solo:.0} ms · con dictados seguidos: acabó a los {rt:.0} ms; los dictados pararon a los {fin_dictados:.0} ms");
    println!("DICTADOS durante el reintento: {} (primero {:.0} ms)", resumen(&v), v[0]);
    // No se cuelga: acaba en cuanto los dictados dejan hueco.
    assert!(rt < fin_dictados + reintento_solo * 1.5 + 5000.0, "el reintento no acabó tras parar los dictados");
}

/// El plazo de 45 s del Transform (`ollama::TRANSFORM_TIMEOUT`, `agent(...).timeout`) ¿es duro de
/// verdad? Un «Ollama» que contesta 200 y gotea un trozo del cuerpo cada 0,5 s (`Transfer-Encoding:
/// chunked`), y otro que gotea las cabeceras. Con el plazo acortado a 3 s.
#[test]
fn rompedor_carga_plazo_transform_con_goteo() {
    use std::io::Write;
    let tr = transform_formal();
    for modo in ["cuerpo", "cabeceras"] {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", l.local_addr().unwrap());
        std::thread::spawn(move || {
            for s in l.incoming().flatten() {
                std::thread::spawn(move || {
                    let mut s = s;
                    let _ = leer_peticion(&mut s);
                    let cab = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\n";
                    if modo == "cabeceras" {
                        for b in cab.iter() {
                            if s.write_all(&[*b]).is_err() {
                                return;
                            }
                            std::thread::sleep(Duration::from_millis(500));
                        }
                    } else if s.write_all(cab).is_err() {
                        return;
                    }
                    for _ in 0..240 {
                        if s.write_all(b"1\r\n \r\n").is_err() {
                            return;
                        }
                        std::thread::sleep(Duration::from_millis(500));
                    }
                });
            }
        });
        let t = Instant::now();
        let r = crate::state::transform_text_within(&url, "falso", &tr, SELECCION, Duration::from_secs(3));
        let took = ms(t);
        println!("GOTEO de {modo}: {took:.0} ms → {:?}", r.as_ref().map(|_| "ok").map_err(|f| f.message.clone()));
        assert!(took < 5000.0, "goteo de {modo}: el plazo de 3 s no cortó ({took:.0} ms)");
        assert!(r.is_err());
    }
}

/// La purga de audio ya no va en cada dictado, pero sigue ahí: al arrancar (`main.rs:79`), al
/// cambiar `keepAudioDays` (`commands.rs:85`, en el hilo del comando) y cada hora. `purge_audio`
/// (`db.rs:1492-1500`) hace un `UPDATE` por fila con el cerrojo de la base cogido. Aquí: 100 000
/// dictados, N con audio caducado (lo que pasa al bajar `keepAudioDays`, o al abrir Pronto tras
/// unas vacaciones), y a la vez un dictado guardándose cada 20 ms (`add_history`, lo que va antes
/// de pegar).
#[test]
#[ignore = "rompedor: purge_audio (db.rs) sigue tardando y reteniendo la base con una purga masiva; ya no congela el hilo principal (va en un hilo desde Ajustes)"]
fn rompedor_carga_purga_masiva_y_dictado() {
    let n: i64 = std::env::var("PRONTO_ROMPEDOR_FILAS").ok().and_then(|v| v.parse().ok()).unwrap_or(100_000);
    let caducadas: i64 = std::env::var("PRONTO_ROMPEDOR_CADUCADAS").ok().and_then(|v| v.parse().ok()).unwrap_or(6_000);
    let dir = std::env::temp_dir().join(format!("pronto-rompedor-carga-purga-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let db = Arc::new(crate::db::Db::open_at(&dir.join("pronto2.db")).expect("abrir base temporal"));
    {
        let mut conn = db.conn.lock();
        let tx = conn.transaction().unwrap();
        {
            let mut st = tx
                .prepare("INSERT INTO history (created_at, text, raw_text, app, duration_ms, words, has_audio) VALUES (?1, ?2, ?2, 'Code', 15000, 40, ?3)")
                .unwrap();
            let now = chrono::Local::now();
            for i in 0..n {
                let t = now - chrono::Duration::minutes(5 * (n - i));
                let edad = (now - t).num_minutes();
                // Con audio: la última semana y las `caducadas` filas justo anteriores.
                let con_audio = edad < 7 * 24 * 60 + caducadas * 5;
                let ts = t.to_rfc3339();
                st.execute(rusqlite::params![ts, format!("dictado {i} con unas cuantas palabras de relleno"), con_audio as i64]).unwrap();
                // Las caducadas, con su WAV de verdad (1 KB) en la carpeta de audio de pruebas (temporal).
                if con_audio && edad >= 7 * 24 * 60 {
                    let id = tx.last_insert_rowid();
                    std::fs::write(crate::db::audio_path(id, &ts).unwrap(), [0u8; 1024]).unwrap();
                }
            }
        }
        tx.commit().unwrap();
    }
    let parar = Arc::new(AtomicBool::new(false));
    let guardados = {
        let (db, parar) = (db.clone(), parar.clone());
        std::thread::spawn(move || {
            let mut v = Vec::new();
            while !parar.load(Ordering::SeqCst) {
                let t = Instant::now();
                db.add_history(&crate::db::NewHistory { polished: false, text: "hola", raw: "hola", app: "Code", duration_ms: 1000, words: 1, fixes: Default::default(), transform: None })
                    .unwrap();
                v.push(ms(t));
                std::thread::sleep(Duration::from_millis(20));
            }
            v
        })
    };
    std::thread::sleep(Duration::from_millis(300));
    let t = Instant::now();
    let borradas = db.purge_audio(7).unwrap();
    let purga = ms(t);
    std::thread::sleep(Duration::from_millis(300));
    parar.store(true, Ordering::SeqCst);
    let v = guardados.join().unwrap();
    // Purga horaria normal: nada (o casi nada) que borrar.
    let t = Instant::now();
    let _ = db.purge_audio(7).unwrap();
    let vacia = ms(t);
    println!("FILAS {n} · PURGA de {borradas} audios caducados: {purga:.0} ms · purga sin nada que borrar: {vacia:.1} ms");
    println!("GUARDAR un dictado mientras tanto: {}", resumen(&v));
    drop(db);
    let _ = std::fs::remove_dir_all(&dir);
    if let Some(a) = crate::db::audio_dir() {
        let _ = std::fs::remove_dir_all(a);
    }
    // `save_settings` es un comando síncrono (hilo principal): la purga de `apply_settings` lo congela.
    assert!(purga < 500.0, "la purga de {borradas} audios tarda {purga:.0} ms en el hilo que la llama");
    assert!(pct(&v, 100.0) < 100.0, "un dictado esperó {:.0} ms a la purga para guardarse", pct(&v, 100.0));
}

/// El goteo del cuerpo lo corta el plazo (bien), pero el aviso dice «Respuesta de Ollama ilegible:
/// timed out reading response» (`ollama.rs:307`): `is_timeout` solo mira los fallos de `send_json`.
#[test]
fn rompedor_carga_plazo_vencido_en_el_cuerpo_dice_ilegible() {
    use std::io::Write;
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            std::thread::spawn(move || {
                let mut s = s;
                let _ = leer_peticion(&mut s);
                let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 40\r\n\r\n{\"response\":");
                std::thread::sleep(Duration::from_secs(10));
            });
        }
    });
    let r = crate::state::transform_text_within(&url, "falso", &transform_formal(), SELECCION, Duration::from_secs(2));
    let msg = r.err().map(|f| f.message).unwrap_or_default();
    println!("CUERPO a medias y Ollama callado: {msg:?}");
    assert_eq!(msg, "Ollama no respondió a tiempo");
}

/// «Reintentar transcripción» de un dictado de 2 min y, 1 s después, un dictado de 15 s (el usuario
/// pulsa Reintentar en el hub y se va a dictar). El primer trozo del reintento coge el motor con
/// prioridad (`asr.rs:360-364`) y es de 60 s (`MAX_CHUNK_SECS`): el dictado espera lo que quede.
#[test]
#[ignore = "rompedor: un dictado justo después de pulsar «Reintentar» espera un trozo entero de 60 s"]
fn rompedor_carga_dictado_justo_tras_pulsar_reintentar() {
    let Some((e, s)) = motor() else { return };
    let mut solo = Vec::new();
    for _ in 0..3 {
        let t = Instant::now();
        e.transcribe_timed(&s).unwrap();
        solo.push(ms(t));
    }
    std::thread::sleep(Duration::from_millis(3100));
    let audio = largo(&s, 120);
    let e2 = e.clone();
    let r = std::thread::spawn(move || e2.transcribe_timed_polite(&audio).map(|_| ()));
    std::thread::sleep(Duration::from_millis(1000));
    let t = Instant::now();
    e.transcribe_timed(&s).unwrap();
    let d = ms(t);
    r.join().unwrap().unwrap();
    println!("SOLO esta ronda: {} · DICTADO 1 s después de «Reintentar»: {d:.0} ms", resumen(&solo));
    assert!(d <= pct(&solo, 100.0) * 1.2, "regla 3: el dictado esperó al primer trozo del reintento: {d:.0} ms");
}
