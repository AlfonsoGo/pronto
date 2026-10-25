//! Sondas del rompedor (2026-10-24): Whisper rápido (cd7c772) y arreglos del 24-oct (b58f74a, 9d7cc34).
//! Solo se compilan en pruebas. Usan la GPU y el modelo Whisper por defecto (turbo) de
//! `%APPDATA%\Pronto2\models\whisper` en **solo lectura** (en pruebas `verify` no escribe `.verificado`).
//! Audio: los WAV de `PRONTO_ROMPEDOR_JUEZ` (carpeta; por defecto `C:\Users\usuario\dev\pronto-juez`).
//! Sin modelo o sin audio, no miden nada.
//! `CARGO_TARGET_DIR=… cargo test --release rompedor_whisper_ -- --ignored --test-threads=1 --nocapture`
#![allow(dead_code, unused_imports)]

use crate::asr::{Engine, Timed};
use crate::whisper::{model, prompt_for, Whisper};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use whisper_rs::{DtwMode, DtwModelPreset, DtwParameters, FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState};

// ------------------------------------------------------------------ utilidades

fn juez() -> Vec<(String, Vec<f32>)> {
    let dir = std::env::var("PRONTO_ROMPEDOR_JUEZ").unwrap_or_else(|_| r"C:\Users\usuario\dev\pronto-juez".into());
    let mut v: Vec<_> = std::fs::read_dir(&dir)
        .map(|d| d.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "wav")).collect())
        .unwrap_or_else(|_| Vec::new());
    v.sort();
    v.into_iter()
        .filter_map(|p| {
            let a = crate::audio::decode_wav(&std::fs::read(&p).ok()?)?;
            Some((p.file_stem()?.to_string_lossy().into_owned(), a))
        })
        .collect()
}

fn listo() -> bool {
    let ok = model("").present() && crate::whisper::gpu().is_some() && !juez().is_empty();
    if !ok {
        eprintln!("SIN MODELO/GPU/AUDIO: sonda sin medir");
    }
    ok
}

fn pct(v: &[f64], p: f64) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.total_cmp(b));
    if s.is_empty() {
        return 0.0;
    }
    s[(((p / 100.0) * (s.len() as f64 - 1.0)).round() as usize).min(s.len() - 1)]
}

fn resumen(v: &[f64]) -> String {
    format!("n={} p50={:.0} p95={:.0} max={:.0} ms", v.len(), pct(v, 50.0), pct(v, 95.0), pct(v, 100.0))
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn vram_free_mb() -> f64 {
    whisper_rs::vulkan::list_devices().into_iter().max_by_key(|d| d.vram.total).map_or(0.0, |d| d.vram.free as f64 / 1_048_576.0)
}

fn threads() -> usize {
    let pid = std::process::id();
    let out = std::process::Command::new("powershell").args(["-NoProfile", "-Command", &format!("(Get-Process -Id {pid}).Threads.Count")]).output();
    out.ok().and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok()).unwrap_or(0)
}

/// Contexto y estado de whisper.cpp como `whisper::Ctx::open` (turbo).
fn abrir(flash: bool) -> (WhisperContext, WhisperState) {
    let m = model("");
    let mut cp = WhisperContextParameters::default();
    cp.use_gpu(true);
    cp.flash_attn(flash);
    if !flash {
        cp.dtw_parameters(DtwParameters { mode: DtwMode::ModelPreset { model_preset: DtwModelPreset::LargeV3Turbo }, ..Default::default() });
    }
    let ctx = WhisperContext::new_with_params(m.path().to_str().unwrap(), cp).expect("contexto");
    let st = ctx.create_state().expect("estado");
    (ctx, st)
}

/// (texto, tokens (id, t0, t1, t_dtw)) con los parámetros de `Whisper::run` y `audio_ctx`/`offset_ms`.
type Raw = (String, Vec<(i32, i64, i64, i64)>);
fn correr(st: &mut WhisperState, audio: &[f32], lang: &str, audio_ctx: i32, offset_ms: i32) -> Raw {
    let prompt = prompt_for(&[]);
    let mut p = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    p.set_language(Some(lang));
    p.set_n_threads(8);
    p.set_print_progress(false);
    p.set_print_realtime(false);
    p.set_print_special(false);
    p.set_print_timestamps(false);
    p.set_no_context(true);
    p.set_suppress_blank(true);
    p.set_suppress_nst(true);
    p.set_token_timestamps(true);
    p.set_temperature(0.0);
    p.set_no_speech_thold(0.6);
    p.set_initial_prompt(&prompt);
    p.set_audio_ctx(audio_ctx);
    p.set_offset_ms(offset_ms);
    if st.full(p, audio).is_err() {
        return ("<ERROR>".into(), Vec::new());
    }
    let mut text = String::new();
    let mut toks = Vec::new();
    for seg in st.as_iter() {
        text.push_str(&seg.to_str_lossy().map(|t| t.into_owned()).unwrap_or_default());
        text.push('|');
        for i in 0..seg.n_tokens() {
            if let Some(t) = seg.get_token(i) {
                let d = t.token_data();
                toks.push((t.token_id(), d.t0, d.t1, d.t_dtw));
            }
        }
    }
    (text, toks)
}

/// Audios de borde (sintéticos) más los del juez.
fn bordes(juez: &[(String, Vec<f32>)]) -> Vec<(String, Vec<f32>)> {
    let tono = |n: usize, f: f32| (0..n).map(|i| (i as f32 * f * std::f32::consts::TAU / 16_000.0).sin() * 0.1).collect::<Vec<f32>>();
    let mut v = vec![
        ("silencio-2s".to_string(), vec![0.0f32; 32_000]),
        ("tono-0.5s".to_string(), tono(8_000, 440.0)),
        ("ruido-0.95s".to_string(), (0..15_200).map(|i| ((i as f32 * 12.9898).sin() * 43758.545).fract() * 0.05).collect()),
    ];
    if let Some((_, a)) = juez.first() {
        v.push(("juez0-0.99s".into(), a[..a.len().min(15_840)].to_vec()));
        v.push(("juez0-1.05s".into(), a[..a.len().min(16_800)].to_vec()));
        // 30 s exactos y 45 s: varios juez concatenados (más de una ventana de Whisper).
        let todo: Vec<f32> = juez.iter().flat_map(|(_, a)| a.iter().copied()).collect();
        if todo.len() >= 720_000 {
            v.push(("juez-30s-exactos".into(), todo[..480_000].to_vec()));
            v.push(("juez-45s".into(), todo[..720_000].to_vec()));
        }
        // Voz al final de 28 s de silencio.
        let mut tarde = vec![0.0f32; 26 * 16_000];
        tarde.extend_from_slice(&a[..a.len().min(2 * 16_000)]);
        v.push(("silencio-y-voz-al-final".into(), tarde));
    }
    v
}

// ------------------------------------------------------------------ 1. parche [Pronto] de whisper.cpp

/// El parche reutiliza el encoder de la detección de idioma solo si `exp_n_audio_ctx` (de la llamada
/// ANTERIOR) es igual al `audio_ctx` de esta. Así se puede comparar en el mismo estado, con el mismo
/// `audio_ctx = 0`, una llamada con reuso (la anterior también fue 0) y otra sin él (la anterior fue
/// 1500 = el contexto completo de turbo, que no recorta nada). También se mezcla audio distinto entre
/// llamadas (estado sucio) y se compara contra un estado recién creado.
#[test]
#[ignore]
fn rompedor_whisper_parche_reuso_igual() {
    if !listo() {
        return;
    }
    let j = juez();
    let casos: Vec<_> = bordes(&j).into_iter().chain(j.iter().take(8).cloned()).collect();
    let mut difs = 0;
    for flash in [true, false] {
        let (ctx, mut st) = abrir(flash);
        let otro = &j.last().unwrap().1;
        for (id, a) in &casos {
            // Con reuso (anterior 0, y antes otro audio en el estado: la detección pisa el encoder).
            let _ = correr(&mut st, otro, "auto", 0, 0);
            let con = correr(&mut st, a, "auto", 0, 0);
            // Sin reuso: la anterior fue 1500.
            let _ = correr(&mut st, otro, "auto", 1500, 0);
            let sin = correr(&mut st, a, "auto", 0, 0);
            // Estado recién creado (exp_n_audio_ctx = 0: reusa) para comparar con un estado limpio.
            let mut nuevo = ctx.create_state().unwrap();
            let limpio = correr(&mut nuevo, a, "auto", 0, 0);
            let igual = con == sin && con == limpio;
            // El ruido puro sale distinto también con idioma fijo «es», sin el parche en juego
            // (`rompedor_whisper_parche_desempate`: muestreo con temperatura al fallar el voraz).
            if !igual && !id.starts_with("ruido") {
                difs += 1;
            }
            println!(
                "PARCHE flash={flash} {id} igual={igual} texto_igual={} tok_con={} tok_sin={} tok_limpio={}",
                con.0 == sin.0 && con.0 == limpio.0,
                con.1.len(),
                sin.1.len(),
                limpio.1.len()
            );
            if !igual {
                let dt = con.1.iter().zip(&sin.1).filter(|(x, y)| x != y).count();
                println!("PARCHE   tokens distintos con/sin: {dt}");
            }
        }
        // offset_ms > 0 nunca reutiliza: mismo resultado que en un estado limpio.
        let a = &j[0].1;
        let x = correr(&mut st, a, "auto", 0, 500);
        let mut nuevo = ctx.create_state().unwrap();
        let _ = correr(&mut nuevo, otro, "auto", 1500, 0);
        let y = correr(&mut nuevo, a, "auto", 0, 500);
        println!("PARCHE flash={flash} offset500 igual={}", x == y);
        if x != y {
            difs += 1;
        }
    }
    assert_eq!(difs, 0, "el reuso del encoder cambia la salida en {difs} casos");
}

// ------------------------------------------------------------------ 2. latido en paralelo con el dictado

/// `pulse()` corre en otro hilo sobre un segundo estado del MISMO contexto `fast` sin ningún cerrojo
/// común con el dictado. Se martillea `pulse` sin pausa mientras se dicta: ¿mismo texto que sin latido?
/// ¿cuelgue o caída de Vulkan? ¿cuánto se alarga el dictado?
#[test]
#[ignore]
fn rompedor_whisper_latido_en_paralelo() {
    if !listo() {
        return;
    }
    let j = juez();
    let w = Arc::new(Whisper::load(model(""), true).expect("cargar"));
    w.set_prompt(prompt_for(&[]));
    let base: Vec<String> = j.iter().map(|(_, a)| w.decode_timed(a, "auto").text).collect();
    let mut t_sin = Vec::new();
    for (_, a) in j.iter().take(10) {
        let t = Instant::now();
        let _ = w.decode_timed(a, "auto");
        t_sin.push(ms(t));
    }
    let stop = Arc::new(AtomicBool::new(false));
    let pulsos = Arc::new(AtomicUsize::new(0));
    let h = {
        let (w, stop, pulsos) = (w.clone(), stop.clone(), pulsos.clone());
        std::thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                w.pulse();
                pulsos.fetch_add(1, Ordering::SeqCst);
            }
        })
    };
    let mut dist = 0;
    let mut t_con = Vec::new();
    for vuelta in 0..3 {
        for (i, (id, a)) in j.iter().enumerate() {
            let t = Instant::now();
            let r = w.decode_timed(a, "auto").text;
            t_con.push(ms(t));
            if r != base[i] {
                dist += 1;
                println!("LATIDO vuelta={vuelta} {id}: texto distinto con latido en paralelo");
            }
        }
    }
    stop.store(true, Ordering::SeqCst);
    h.join().unwrap();
    println!("LATIDO pulsos={} distintos={dist}/{}", pulsos.load(Ordering::SeqCst), t_con.len());
    println!("LATIDO sin latido: {}", resumen(&t_sin));
    println!("LATIDO con latido sin pausa: {}", resumen(&t_con));
    assert_eq!(dist, 0);
}

/// Lo que paga el dictado si se suelta el atajo con un latido a medias: duración de un latido y
/// dictado lanzado justo tras empezar uno.
#[test]
#[ignore]
fn rompedor_whisper_latido_al_soltar() {
    if !listo() {
        return;
    }
    let j = juez();
    let w = Arc::new(Whisper::load(model(""), true).expect("cargar"));
    w.set_prompt(prompt_for(&[]));
    let mut pulso = Vec::new();
    for _ in 0..20 {
        let t = Instant::now();
        w.pulse();
        pulso.push(ms(t));
    }
    println!("SOLTAR latido solo: {}", resumen(&pulso));
    let a = &j[0].1;
    let mut caliente = Vec::new();
    let mut solapado = Vec::new();
    for _ in 0..12 {
        // Caliente sin solape: latido y luego dictado.
        w.pulse();
        let t = Instant::now();
        let _ = w.decode_timed(a, "auto");
        caliente.push(ms(t));
        // Latido en marcha al soltar.
        let w2 = w.clone();
        let h = std::thread::spawn(move || w2.pulse());
        std::thread::sleep(Duration::from_millis(2));
        let t = Instant::now();
        let _ = w.decode_timed(a, "auto");
        solapado.push(ms(t));
        h.join().unwrap();
    }
    println!("SOLTAR dictado tras latido acabado: {}", resumen(&caliente));
    println!("SOLTAR dictado con latido a medias: {}", resumen(&solapado));
}

// ------------------------------------------------------------------ 3. precarga del contexto con DTW

/// `preload_precise` corre en el hilo de carga DESPUÉS de dar el motor por listo y sin `busy`:
/// abre el contexto DTW (sube el modelo otra vez a la GPU) y hace una inferencia de calentamiento
/// mientras el usuario ya puede dictar. ¿Cuánto tarda el dictado que cae encima? ¿Mismo texto?
/// Y un dictado con huella (precise) pedido durante la precarga espera al cerrojo de `precise`.
#[test]
#[ignore]
fn rompedor_whisper_precarga_en_paralelo() {
    if !listo() {
        return;
    }
    let j = juez();
    let a = &j[0].1;
    // Referencia en un Whisper ya precargado.
    let w0 = Whisper::load(model(""), true).expect("cargar");
    w0.set_prompt(prompt_for(&[]));
    let base = w0.decode_timed(a, "auto").text;
    let mut ref_ms = Vec::new();
    for _ in 0..5 {
        let t = Instant::now();
        let _ = w0.decode_timed(a, "auto");
        ref_ms.push(ms(t));
    }
    drop(w0);
    for vuelta in 0..3 {
        let t_load = Instant::now();
        let w = Arc::new(Whisper::load(model(""), true).expect("cargar"));
        w.set_prompt(prompt_for(&[]));
        let load_ms = ms(t_load);
        let w2 = w.clone();
        let t_pre = Instant::now();
        let h = std::thread::spawn(move || {
            w2.preload_precise();
            ms(t_pre)
        });
        std::thread::sleep(Duration::from_millis(30));
        let mut durante = Vec::new();
        let mut dist = 0;
        for _ in 0..4 {
            let t = Instant::now();
            let r = w.decode_timed(a, "auto").text;
            durante.push(ms(t));
            dist += (r != base) as usize;
        }
        // Dictado con huella justo después de soltar el motor por listo.
        let t = Instant::now();
        let _ = w.decode(a, "auto", true);
        let huella = ms(t);
        let pre = h.join().unwrap();
        println!(
            "PRECARGA vuelta={vuelta} load={load_ms:.0} ms precarga={pre:.0} ms dictados durante: {} distintos={dist} huella primera={huella:.0} ms (ref dictado {})",
            resumen(&durante),
            resumen(&ref_ms)
        );
    }
}

// ------------------------------------------------------------------ 4. segunda pasada con DTW (huella)

/// La huella repite el dictado por `precise` y se queda con SU texto. ¿Cuánto cuesta un dictado
/// con huella (rápido + DTW) frente a antes de cd7c772 (solo DTW)? ¿Cambia el texto? ¿Alguna vez
/// la segunda pasada sale vacía con la primera llena (el dictado se pegaría vacío)?
#[test]
#[ignore]
fn rompedor_whisper_rapido_vs_preciso() {
    if !listo() {
        return;
    }
    let j = juez();
    let w = Whisper::load(model(""), true).expect("cargar");
    w.set_prompt(prompt_for(&[]));
    w.preload_precise();
    let _ = w.decode(&j[0].1, "auto", true);
    let mut solo_dtw = Vec::new();
    let mut dos = Vec::new();
    let mut solo_rapido = Vec::new();
    let (mut distintos, mut vacios) = (0, 0);
    let todos: Vec<_> = bordes(&j).into_iter().filter(|(_, a)| a.len() <= 28 * 16_000).chain(j.iter().cloned()).collect();
    for (id, a) in &todos {
        let t = Instant::now();
        let f = w.decode(a, "auto", false);
        let tf = ms(t);
        let t = Instant::now();
        let p = w.decode(a, "auto", true);
        let tp = ms(t);
        solo_rapido.push(tf);
        solo_dtw.push(tp);
        dos.push(tf + tp);
        if f.text != p.text {
            distintos += 1;
        }
        if p.text.trim().is_empty() && !f.text.trim().is_empty() {
            vacios += 1;
            println!("DOSPASADAS {id}: DTW vacío y rápido con texto ({} car.)", f.text.len());
        }
        println!("DOSPASADAS {id} ({:.1} s) rapido={tf:.0} dtw={tp:.0} igual={}", a.len() as f32 / 16_000.0, f.text == p.text);
    }
    println!("DOSPASADAS solo rápido: {}", resumen(&solo_rapido));
    println!("DOSPASADAS solo DTW (antes de cd7c772): {}", resumen(&solo_dtw));
    println!("DOSPASADAS rápido + DTW (dictado con huella hoy): {}", resumen(&dos));
    println!("DOSPASADAS textos distintos={distintos}/{} DTW vacío con rápido lleno={vacios}", todos.len());
}

// ------------------------------------------------------------------ 5. ¿cuántas veces dispara `hears`?

/// `voiceprint::hears` decide si se paga la segunda pasada: cualquier tramo bajo el tope más alto
/// (6,4) sin margen ni `decide`. Diccionario con muestras de voz reales (trozos de 0,4–0,8 s de un
/// WAV del juez, 3 por palabra, como al grabar una palabra) y dictados de OTROS WAV: ¿en cuántos
/// dispara? ¿y en cuántos la huella cambia algo de verdad? Cada disparo sin cambio = DTW pagado de más.
#[test]
#[ignore]
fn rompedor_whisper_hears_falsos() {
    if !listo() {
        return;
    }
    use crate::voiceprint::{correct_with_bank, detect, BankWord, Template};
    let j = juez();
    let w = Whisper::load(model(""), true).expect("cargar");
    w.set_prompt(prompt_for(&[]));
    w.preload_precise();
    let mut seed = 0x9E3779B97F4A7C15u64;
    let mut rnd = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for n_palabras in [1usize, 5, 20] {
        // Muestras de los 3 primeros WAV; dictados: el resto.
        let fuentes: Vec<&Vec<f32>> = j.iter().take(3).map(|x| &x.1).collect();
        let bank: Vec<BankWord> = (0..n_palabras)
            .map(|k| {
                let templates = (0..3)
                    .filter_map(|_| {
                        let src = fuentes[(rnd() % fuentes.len() as u64) as usize];
                        let len = 6_400 + (rnd() % 6_400) as usize;
                        let at = (rnd() % (src.len().saturating_sub(len).max(1)) as u64) as usize;
                        Template::from_audio(&src[at..(at + len).min(src.len())]).map(Arc::new)
                    })
                    .collect();
                BankWord { write: format!("Palabrazeta{k}"), replaced: false, forms: vec![format!("palabrazeta{k}")], templates }
            })
            .collect();
        let (mut oye, mut cambia, mut total) = (0, 0, 0);
        let mut coste = Vec::new();
        for (_, a) in j.iter().skip(3) {
            total += 1;
            let t = Instant::now();
            let h = !detect(&bank, a, 6.4).is_empty();
            coste.push(ms(t));
            if h {
                oye += 1;
                let p = w.decode(a, "auto", true);
                let (txt, fixes) = correct_with_bank(&bank, a, &p);
                if !fixes.is_empty() || txt != p.text {
                    cambia += 1;
                }
            }
        }
        println!("HEARS palabras={n_palabras} (x3 muestras): dispara en {oye}/{total} dictados; la huella cambia algo en {cambia}; coste de hears {}", resumen(&coste));
    }
}

// ------------------------------------------------------------------ 6. hilos y memoria del latido

/// 300 dictados cortos con `keep_warm` como en la app (latido mientras «se habla», soltar y
/// transcribir), algunos cancelados (sin transcribir) y algunos con huella: ¿se acumulan hilos
/// «pronto-latido», RAM o memoria de vídeo?
#[test]
#[ignore]
fn rompedor_whisper_latido_hilos_y_memoria() {
    if !listo() {
        return;
    }
    let j = juez();
    let engine = Engine::load_whisper_for_tests().expect("cargar");
    let a = &j[0].1;
    let _ = engine.transcribe_timed(a);
    let _ = engine.transcribe_timed_precise(a);
    std::thread::sleep(Duration::from_millis(800));
    let (v0, h0) = (vram_free_mb(), threads());
    let mut perdidos = 0;
    let mut lat = Vec::new();
    for i in 0..300 {
        let warm = engine.keep_warm();
        if warm.is_none() {
            perdidos += 1;
        }
        std::thread::sleep(Duration::from_millis(30 + (i % 7) * 37));
        drop(warm);
        match i % 5 {
            0 => {} // cancelado (Esc): sin transcribir
            1 => {
                let t = Instant::now();
                let _ = engine.transcribe_timed_precise(a);
                lat.push(ms(t));
            }
            _ => {
                let t = Instant::now();
                let _ = engine.transcribe_timed(a);
                lat.push(ms(t));
            }
        }
    }
    std::thread::sleep(Duration::from_millis(800));
    let (v1, h1) = (vram_free_mb(), threads());
    println!("LATIDO-HILOS hilos {h0} -> {h1}; VRAM libre {v0:.0} -> {v1:.0} MB; keep_warm None={perdidos}; transcripciones {}", resumen(&lat));
    // Warmth nunca soltado (dictado manos libres): tope de 120 s; aquí basta ver que el hilo vive.
    assert!(h1 <= h0 + 3, "hilos acumulados: {h0} -> {h1}");
}

/// `Warmth` soltado: ¿cuánto tarda en parar el hilo? Un latido empezado justo antes de soltar sigue.
/// Se mide cuántos latidos empiezan DESPUÉS de soltar (no debería ninguno).
#[test]
#[ignore]
fn rompedor_whisper_latido_para_al_soltar() {
    if !listo() {
        return;
    }
    let engine = Engine::load_whisper_for_tests().expect("cargar");
    let h0 = threads();
    for _ in 0..50 {
        let w = engine.keep_warm().expect("latido");
        std::thread::sleep(Duration::from_millis(5));
        drop(w);
    }
    std::thread::sleep(Duration::from_millis(500));
    let h1 = threads();
    println!("PARA hilos {h0} -> {h1} tras 50 latidos soltados a los 5 ms");
    assert!(h1 <= h0 + 2);
}

// ------------------------------------------------------------------ 7. motor: dictado + reintento con huella

/// Reintento largo con huella (`transcribe_timed_polite_precise`, 3 min = 7 trozos con DTW) y un
/// dictado a la vez: ¿cuánto espera el dictado? (el arreglo de hoy dice: como mucho lo que quede
/// del trozo en curso).
#[test]
#[ignore]
fn rompedor_whisper_reintento_huella_y_dictado() {
    if !listo() {
        return;
    }
    let j = juez();
    let engine = Arc::new(Engine::load_whisper_for_tests().expect("cargar"));
    let largo: Vec<f32> = j.iter().flat_map(|(_, a)| a.iter().copied()).cycle().take(180 * 16_000).collect();
    let a = j[0].1.clone();
    let _ = engine.transcribe_timed_precise(&a);
    let mut solo = Vec::new();
    for _ in 0..5 {
        let t = Instant::now();
        let _ = engine.transcribe_timed(&a);
        solo.push(ms(t));
    }
    // Pasa la gracia de 3 s tras el último dictado.
    std::thread::sleep(Duration::from_millis(3_200));
    let e2 = engine.clone();
    let t_re = Instant::now();
    let h = std::thread::spawn(move || {
        let r = e2.transcribe_timed_polite_precise(&largo);
        (r.map(|t| t.text.len()), ms(t_re))
    });
    let mut con = Vec::new();
    for _ in 0..6 {
        std::thread::sleep(Duration::from_millis(1_700));
        let t = Instant::now();
        let _ = engine.transcribe_timed(&a);
        con.push(ms(t));
    }
    let (r, total) = h.join().unwrap();
    println!("REINTENTO dictado solo: {}", resumen(&solo));
    println!("REINTENTO dictado con reintento con huella en marcha: {}", resumen(&con));
    println!("REINTENTO reintento: {r:?} en {total:.0} ms");
}

// ------------------------------------------------------------------ 8. entradas de audio raras

/// Audio con valores no finitos o saturados por los dos caminos (un WAV importado/dañado o un
/// micrófono que da basura): ¿cae el proceso (GGML_ASSERT en DTW) o sale texto inventado?
#[test]
#[ignore]
fn rompedor_whisper_audio_raro() {
    if !listo() {
        return;
    }
    let w = Whisper::load(model(""), true).expect("cargar");
    w.set_prompt(prompt_for(&[]));
    w.preload_precise();
    let j = juez();
    let mut casos: Vec<(&str, Vec<f32>)> = vec![
        ("vacio", Vec::new()),
        ("una-muestra", vec![0.3]),
        ("saturado-±1", (0..32_000).map(|i| if i % 2 == 0 { 1.0 } else { -1.0 }).collect()),
        ("fuera-de-rango-±40", (0..32_000).map(|i| ((i as f32) * 0.05).sin() * 40.0).collect()),
    ];
    let mut con_nan = j[0].1.clone();
    for k in (0..con_nan.len()).step_by(997) {
        con_nan[k] = f32::NAN;
    }
    casos.push(("juez0-con-NaN", con_nan));
    let mut con_inf = j[0].1.clone();
    con_inf[100] = f32::INFINITY;
    casos.push(("juez0-con-inf", con_inf));
    for (id, a) in &casos {
        let t = Instant::now();
        let f = w.decode(a, "auto", false);
        let tf = ms(t);
        let t = Instant::now();
        let p = w.decode(a, "auto", true);
        let tp = ms(t);
        println!("RARO {id}: rapido {tf:.0} ms {} car., tokens {}; dtw {tp:.0} ms {} car., tokens {}", f.text.chars().count(), f.tokens.len(), p.text.chars().count(), p.tokens.len());
    }
}

// ------------------------------------------------------------------ 9. idioma como lista (parche [Pronto] 2, sin commit)

fn idioma_de(st: &WhisperState) -> String {
    whisper_rs::get_lang_str(st.full_lang_id_from_state()).unwrap_or("?").to_string()
}

/// El idioma de los ajustes llega sin validar a whisper.cpp (`save_settings` acepta cualquier
/// `language`; `whisper_language` lo pasa tal cual si no es «auto»/«libre»). Con una coma es una lista.
/// Listas raras: ¿qué idioma sale, mismo texto que «es»?, ¿reuso del encoder igual?
#[test]
#[ignore]
fn rompedor_whisper_idioma_lista() {
    if !listo() {
        return;
    }
    let j = juez();
    let (_ctx, mut st) = abrir(true);
    let larga = std::iter::repeat("es,en").take(400).collect::<Vec<_>>().join(",");
    let casos: Vec<String> = ["es", "es,en", "en,es", "es,", ",es", ",", "xx,yy", "es,xx", " es, en", "ES,EN", "spanish,english", "pt,gl", "auto,es", "es,en,es"]
        .iter()
        .map(|s| s.to_string())
        .chain([larga])
        .collect();
    for (id, a) in j.iter().take(4).map(|(i, a)| (i.clone(), a.clone())).chain([("juez0-1.5s".to_string(), j[0].1[..24_000.min(j[0].1.len())].to_vec())]) {
        let es = correr(&mut st, &a, "es", 0, 0);
        let libre = correr(&mut st, &a, "auto", 0, 0);
        let libre_id = idioma_de(&st);
        for l in &casos {
            let t = Instant::now();
            let r = correr(&mut st, &a, l, 0, 0);
            let el = idioma_de(&st);
            // Sin reuso (anterior con audio_ctx distinto): mismo resultado.
            let _ = correr(&mut st, &a, "es", 1500, 0);
            let r2 = correr(&mut st, &a, l, 0, 0);
            let shown = if l.len() > 20 { format!("{}…({} car.)", &l[..12], l.len()) } else { format!("{l:?}") };
            println!(
                "LISTA {id} {shown}: idioma={el} (libre={libre_id}) texto==es:{} reuso_igual:{} {:.0} ms {}",
                r.0 == es.0,
                r == r2,
                ms(t),
                if r.0 == "<ERROR>" { "ERROR" } else { "" }
            );
        }
        let _ = libre;
    }
}

/// Un solo código inválido (sin coma, así que no es lista): llega a whisper.cpp como idioma fijo.
/// Antes y después del parche igual, pero el parche no lo cubre: ¿error, texto o caída?
#[test]
#[ignore]
fn rompedor_whisper_idioma_invalido() {
    if !listo() {
        return;
    }
    let j = juez();
    let (_ctx, mut st) = abrir(true);
    for l in ["xx", "es-ES", "", "auto"] {
        let r = correr(&mut st, &j[0].1, l, 0, 0);
        println!("INVALIDO {l:?}: idioma={} {} car. {}", idioma_de(&st), r.0.chars().count(), if r.0 == "<ERROR>" { "ERROR" } else { "" });
    }
}

/// Desempate de la sonda 1: el ruido de 0,95 s sale distinto según la historia del estado. ¿Es el
/// parche o no-determinismo general? Repite con estado limpio y sucio, con «auto» (reusa) y «es» (no
/// hay detección, el parche no actúa).
#[test]
#[ignore]
fn rompedor_whisper_parche_desempate() {
    if !listo() {
        return;
    }
    let j = juez();
    let ruido: Vec<f32> = (0..15_200).map(|i| ((i as f32 * 12.9898).sin() * 43758.545).fract() * 0.05).collect();
    let otro = &j.last().unwrap().1;
    for flash in [true, false] {
        let (ctx, mut st) = abrir(flash);
        for lang in ["auto", "es"] {
            let mut vistos = std::collections::BTreeMap::<String, usize>::new();
            for k in 0..12 {
                let r = match k % 4 {
                    0 => correr(&mut ctx.create_state().unwrap(), &ruido, lang, 0, 0), // limpio
                    1 => correr(&mut st, &ruido, lang, 0, 0),                          // repetido
                    2 => {
                        let _ = correr(&mut st, otro, lang, 0, 0);
                        correr(&mut st, &ruido, lang, 0, 0) // sucio con reuso
                    }
                    _ => {
                        let _ = correr(&mut st, otro, lang, 1500, 0);
                        correr(&mut st, &ruido, lang, 0, 0) // sucio sin reuso
                    }
                };
                *vistos.entry(format!("k%4={} toks={} {:?}", k % 4, r.1.len(), r.1.iter().map(|t| t.0).collect::<Vec<_>>())).or_default() += 1;
            }
            for (v, n) in vistos {
                println!("DESEMPATE flash={flash} {lang} x{n} {v}");
            }
        }
    }
}

/// Desempate de la VRAM que baja en `latido_hilos_y_memoria`: ¿fuga o reserva perezosa del primer
/// latido? Puntos de control cada 100 ciclos tras calentar los tres caminos (rápido, latido, DTW).
#[test]
#[ignore]
fn rompedor_whisper_vram_tendencia() {
    if !listo() {
        return;
    }
    let j = juez();
    let engine = Engine::load_whisper_for_tests().expect("cargar");
    let a = &j[0].1;
    let _ = engine.transcribe_timed(a);
    let _ = engine.transcribe_timed_precise(a);
    drop(engine.keep_warm());
    std::thread::sleep(Duration::from_millis(400));
    let mut puntos = Vec::new();
    for i in 0..=400 {
        if i % 100 == 0 {
            std::thread::sleep(Duration::from_millis(800));
            puntos.push(format!("{i}:{:.0}", vram_free_mb()));
        }
        let w = engine.keep_warm();
        std::thread::sleep(Duration::from_millis(60 + (i % 5) as u64 * 50));
        drop(w);
        let b = &j[i % j.len()].1;
        if i % 4 == 1 {
            let _ = engine.transcribe_timed_precise(b);
        } else {
            let _ = engine.transcribe_timed(b);
        }
    }
    println!("VRAM libre (MB) por ciclo: {}", puntos.join(" "));
}

/// Dónde salta la VRAM de `vram_tendencia`: cada paso por separado tras el calentamiento de un audio.
#[test]
#[ignore]
fn rompedor_whisper_vram_paso_a_paso() {
    if !listo() {
        return;
    }
    let j = juez();
    let engine = Engine::load_whisper_for_tests().expect("cargar");
    let a = &j[0].1;
    let _ = engine.transcribe_timed(a);
    let _ = engine.transcribe_timed_precise(a);
    drop(engine.keep_warm());
    let medir = |paso: &str| {
        std::thread::sleep(Duration::from_millis(800));
        println!("PASO {paso}: VRAM libre {:.0} MB", vram_free_mb());
    };
    medir("tras calentar con j[0]");
    let w = engine.keep_warm();
    std::thread::sleep(Duration::from_millis(600));
    drop(w);
    medir("tras latidos");
    for (i, (_, b)) in j.iter().enumerate() {
        let _ = engine.transcribe_timed(b);
        medir(&format!("rápido j[{i}] {:.1}s", b.len() as f32 / 16000.0));
    }
    for (i, (_, b)) in j.iter().enumerate().take(4) {
        let _ = engine.transcribe_timed_precise(b);
        medir(&format!("DTW j[{i}]"));
    }
}

/// Aísla la caída de `audio_raro`: `PRONTO_ROMPEDOR_N` muestras (de voz real del juez, o 0,3 si
/// `PRONTO_ROMPEDOR_CTE`), por el camino `PRONTO_ROMPEDOR_CAMINO` (rapido|dtw|raw-flash|raw-dtw).
#[test]
#[ignore]
fn rompedor_whisper_audio_corto() {
    let Ok(n) = std::env::var("PRONTO_ROMPEDOR_N").map(|s| s.parse::<usize>().unwrap()) else { return };
    if !listo() {
        return;
    }
    let camino = std::env::var("PRONTO_ROMPEDOR_CAMINO").unwrap_or_else(|_| "rapido".into());
    let j = juez();
    let a: Vec<f32> = if std::env::var("PRONTO_ROMPEDOR_CTE").is_ok() { vec![0.3; n] } else { j[0].1[..n].to_vec() };
    eprintln!("CORTO n={n} camino={camino} empieza");
    let r = match camino.as_str() {
        "rapido" | "dtw" => {
            let w = Whisper::load(model(""), true).expect("cargar");
            w.decode(&a, "auto", camino == "dtw").text
        }
        c => {
            let (_ctx, mut st) = abrir(c == "raw-flash");
            correr(&mut st, &a, "es", 0, 0).0
        }
    };
    println!("CORTO n={n} camino={camino}: sobrevive, {} car.", r.chars().count());
}

/// ¿Se respetan los códigos con espacios o mayúsculas? Con audio en español y una lista SIN «es», el
/// idioma elegido delata si cada código se entendió (si ninguno vale, vuelve a la detección libre: es).
#[test]
#[ignore]
fn rompedor_whisper_idioma_lista_formato() {
    if !listo() {
        return;
    }
    let j = juez();
    let (_ctx, mut st) = abrir(true);
    for (id, a) in [("juez0-1.5s", j[0].1[..24_000].to_vec()), ("juez3", j[3].1.clone())] {
        for l in ["pt,gl", "gl,pt", " pt, gl", "PT,GL", "pt ,gl", "portuguese,galician", "pt;gl", "pt,", "xx,", ","] {
            let _ = correr(&mut st, &a, l, 0, 0);
            println!("FORMATO {id} {l:?}: idioma={}", idioma_de(&st));
        }
    }
}
