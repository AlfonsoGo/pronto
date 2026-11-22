//! Banco del pulido y los formatos (BELTR-PRT-10, contrato en `specs/features/pulido-en-directo.md`).
//! Solo pruebas. Lo lanzan `tests/banco-formatos.test.mjs` y `tests/banco-velocidad-pulido.test.mjs`.
//!
//! - `banco_formatos`: dicta los 40 casos de `tests/banco-formatos/` (audio de voces locales) y saca, por
//!   caso, el texto sin pulir y el pulido (`FORMATO\tid\tsin\tpulido`). El test de node puntúa.
//! - `banco_velocidad_pulido`: ms de soltar el atajo a texto listo para pegar con el pulido encendido
//!   (Ollama real), en dictados de ~15 s y de ~2 min (`VELOCIDAD_…=` y `VELOCIDAD_MUESTRA`).

use crate::asr::Engine;
use super::{cargar, leer_diccionario, Muestra};
use crate::models::{Settings, Tone, Word};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

/// Ajustes de fábrica con el pulido encendido y el modelo dado.
pub fn ajustes_con_pulido(modelo: &str) -> Settings {
    let mut s = Settings::default();
    s.ai_polish = true;
    s.ai_polish_model = modelo.to_string();
    s
}

/// Lo que pasa al soltar el atajo, como `state::transcribe_and_paste` sin texto en vivo: audio entero →
/// `transcribe_heard` → `text::process` → pulido → diccionario. Devuelve (sin pulir, final, ms, pulido).
pub fn soltar_sin_vivo(engine: &Engine, words: &[Word], settings: &Settings, audio: &[f32]) -> (String, String, f64, bool) {
    let t0 = Instant::now();
    engine.set_vocabulary(&crate::state::vocabulary(words));
    let (_, oido, _) = crate::state::transcribe_heard(engine, words, audio, false).expect("transcribir");
    let options = crate::text::Options { cleanup: settings.cleanup_level, press_enter: settings.press_enter_command, tone: Tone::Casual, words, snippets: &[] };
    let processed = crate::text::process(&oido, &options);
    let p = crate::polish::apply(settings, &processed.text, Tone::Casual, processed.fixes_snippets);
    let final_ = if p.polished && !words.is_empty() { crate::text::apply_dictionary(&p.text, words) } else { p.text.clone() };
    (processed.text, final_, t0.elapsed().as_secs_f64() * 1000.0, p.polished)
}

/// Deja el modelo del pulido cargado y respondiendo antes de medir (como la app, que lo precarga al
/// arrancar y al empezar a dictar): un pulido con plazo largo y el cortacircuitos cerrado.
pub fn calentar_pulido(ajustes: &Settings) {
    let mut largo = ajustes.clone();
    largo.ai_polish_timeout_ms = 60_000;
    crate::ollama::circuit::reset(&ajustes.ollama_url);
    for _ in 0..2 {
        let _ = crate::polish::apply(&largo, "bueno pues mañana a las diez tenemos reunion con los de la imprenta", Tone::Casual, 0);
    }
    crate::ollama::circuit::reset(&ajustes.ollama_url);
}

/// Un dictado «en directo» (BELTR-PRT-16), con el código de la app: la sesión de vivo (vista previa +
/// segmentos confirmados) escucha un micrófono falso que recibe el audio a ritmo real (`velocidad` = 1.0;
/// 2.0 = el doble de rápido), el pulidor de fondo pule cada segmento y al soltar se mide hasta el texto
/// listo, como `state::transcribe_and_paste`. Devuelve (final, ms desde soltar, segmentos, trozos usados).
pub fn soltar_en_directo(engine: &Arc<Engine>, words: &[Word], settings: &Settings, audio: &[f32], velocidad: f64) -> (String, f64, usize, bool) {
    use crate::vivo::{Opciones, Sesion, SEG_MINIMO, SEG_TOPE};
    let opts = |w: &[Word]| -> crate::text::Options<'static> {
        let w: &'static [Word] = Box::leak(w.to_vec().into_boxed_slice());
        crate::text::Options { cleanup: settings.cleanup_level, press_enter: settings.press_enter_command, tone: Tone::Casual, words: w, snippets: &[] }
    };
    engine.set_vocabulary(&crate::state::vocabulary(words));
    let (tap, buf) = crate::audio::Tap::creciente(16_000);
    let pulidor = crate::polish::Pulidor::nuevo(settings).map(Arc::new);
    let (e1, e2, e3, w1, w3, p1, p3) = (engine.clone(), engine.clone(), engine.clone(), words.to_vec(), words.to_vec(), pulidor.clone(), pulidor.clone());
    let (o1, o3) = (opts(words), opts(words));
    // Lo mismo que `state::start_live`: anticipar con la decodificación sin turno + huella de voz.
    let anticipar: crate::vivo::Confirmar = Box::new(move |s: &[f32]| {
        let timed = e3.transcribe_live(s)?.ok()?;
        let (oido, _) = crate::voiceprint::correct(&w3, s, &timed);
        if let Some(p) = &p3 {
            p.encolar(&crate::text::process(&oido, &o3).text, Tone::Casual);
        }
        Some(oido)
    });
    let confirmar: crate::vivo::Confirmar = Box::new(move |s: &[f32]| {
        let (_, oido, _) = crate::state::transcribe_heard(&e1, &w1, s, false).ok()?;
        if let Some(p) = &p1 {
            p.encolar(&crate::text::process(&oido, &o1).text, Tone::Casual);
        }
        Some(oido)
    });
    let sesion = Sesion::empezar_con(
        tap,
        Box::new(|| {}),
        Box::new(move |s: &[f32]| e2.transcribe_live(s)),
        Box::new(|_| {}),
        Opciones {
            vista: true,
            confirmar: Some(confirmar),
            // `PRONTO_SEG_MINIMO` (s): para medir otros mínimos de segmento.
            minimo: std::env::var("PRONTO_SEG_MINIMO").ok().and_then(|v| v.parse::<f64>().ok()).map_or(SEG_MINIMO, |v| (v * 16_000.0) as usize),
            tope: SEG_TOPE,
            anticipar: std::env::var("PRONTO_SIN_ANTICIPAR").is_err().then_some(anticipar),
        },
    )
    .expect("hilo de vivo");
    // El micrófono: 100 ms de audio cada 100 ms / velocidad.
    let t0 = Instant::now();
    for (i, trozo) in audio.chunks(1600).enumerate() {
        buf.lock().extend_from_slice(trozo);
        let toca = t0 + std::time::Duration::from_secs_f64((i + 1) as f64 * 0.1 / velocidad);
        std::thread::sleep(toca.saturating_duration_since(Instant::now()));
    }
    // Soltar.
    let soltar = Instant::now();
    let confirmados = sesion.confirmados();
    drop(sesion);
    let segs = confirmados.al_soltar_con(std::time::Duration::from_secs(5), audio).filter(|(s, c)| (!s.is_empty() || c.is_some()) && crate::vivo::segmentos_validos(s, audio.len()));
    let t_espera = soltar.elapsed().as_millis();
    let o = opts(words);
    let (processed, piezas, nsegs) = match &segs {
        Some((segs, cola)) => {
            let (raw, crudos) = crate::state::juntar_segmentos(engine, words, audio, segs, cola.as_deref()).expect("cola");
            let processed = crate::text::process(&raw, &o);
            let piezas: Vec<String> = crudos.iter().map(|c| crate::text::process(c, &o).text).collect();
            let piezas = crate::state::partir_como(&processed.text, &piezas);
            (processed, piezas, segs.len() + usize::from(cola.is_some()) * 100)
        }
        None => {
            let (_, oido, _) = crate::state::transcribe_heard(engine, words, audio, false).expect("transcribir");
            (crate::text::process(&oido, &o), None, 0)
        }
    };
    let t_transcribir = soltar.elapsed().as_millis();
    let p = match &piezas {
        Some(pz) => crate::polish::apply_pieces(settings, &processed.text, pz, pulidor.as_deref(), Tone::Casual),
        None => crate::polish::apply(settings, &processed.text, Tone::Casual, processed.fixes_snippets),
    };
    let final_ = if p.polished && !words.is_empty() { crate::text::apply_dictionary(&p.text, words) } else { p.text.clone() };
    if std::env::var("PRONTO_BANCO_VOLCAR").is_ok() {
        println!("   desglose: espera {t_espera} ms · transcribir {} ms · pulir {} ms", t_transcribir - t_espera, soltar.elapsed().as_millis() - t_transcribir);
    }
    (final_, soltar.elapsed().as_secs_f64() * 1000.0, nsegs, piezas.is_some())
}

pub fn percentil(v: &[f64], p: f64) -> f64 {
    let mut v = v.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    if v.is_empty() {
        return 0.0;
    }
    let i = ((v.len() - 1) as f64 * p).round() as usize;
    v[i]
}

fn modelo_parakeet() -> Option<Arc<Engine>> {
    let m = std::env::var("PRONTO_NSN_MODELO").ok()?;
    Some(Arc::new(Engine::load_for_tests(&m).expect("Parakeet")))
}

fn diccionario() -> Vec<Word> {
    std::env::var("PRONTO_BANCO_DICCIONARIO").ok().map(|p| leer_diccionario(&std::fs::read_to_string(p).expect("diccionario"))).unwrap_or_default()
}

fn una_linea(s: &str) -> String {
    s.replace(['\t', '\n', '\r'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Los 40 casos de formatos: texto sin pulir y, con `PRONTO_BANCO_PULIDO=modelo`, pulido.
    #[test]
    fn banco_formatos() {
        let (Ok(dir), Some(engine)) = (std::env::var("PRONTO_FORMATOS_DIR"), modelo_parakeet()) else {
            println!("FORMATOS_OMITIDO: faltan PRONTO_FORMATOS_DIR y PRONTO_NSN_MODELO");
            return;
        };
        // `PRONTO_FORMATOS_MOTOR=whisper-medium`: el mismo banco con Whisper (la voz real de Txonso, BELTR-PRT-30).
        let engine = match std::env::var("PRONTO_FORMATOS_MOTOR").ok().as_deref().and_then(|m| m.strip_prefix("whisper-")) {
            Some(m) => Arc::new(Engine::load_whisper_model_for_tests(m, "auto").expect("Whisper")),
            None => engine,
        };
        let words = diccionario();
        let pulido = std::env::var("PRONTO_BANCO_PULIDO").ok();
        let ajustes = ajustes_con_pulido(pulido.as_deref().unwrap_or(""));
        if pulido.is_some() {
            calentar_pulido(&ajustes);
        }
        let sin_pulido = Settings::default();
        let muestras: Vec<Muestra> = cargar(Path::new(&dir), Path::new(&dir));
        assert!(!muestras.is_empty(), "no hay audio en {dir}");
        for m in &muestras {
            let (sin, _, _, _) = soltar_sin_vivo(&engine, &words, &sin_pulido, &m.audio);
            let con = match &pulido {
                Some(_) => soltar_sin_vivo(&engine, &words, &ajustes, &m.audio).1,
                None => "-".to_string(),
            };
            println!("FORMATO\t{}\t{}\t{}", m.nombre, una_linea(&sin), una_linea(&con));
        }
    }

    /// Ajuste del corte de segmentos (BELTR-PRT-16), sin tiempo real ni pulido: cada WAV se pasa al
    /// cortador en trozos de 700 ms, como en directo; cada segmento y la cola se transcriben por el camino
    /// del dictado y se comparan con la referencia y con el audio entero. `PRONTO_SEGMENTOS_SETS`
    /// (`nombre=carpeta;…`) y `PRONTO_SEGMENTOS_REJILLA` (`minimo_s/pausa_ms/tope_s,…`).
    #[test]
    fn banco_segmentos() {
        let (Ok(sets), Some(engine)) = (std::env::var("PRONTO_SEGMENTOS_SETS"), modelo_parakeet()) else {
            println!("SEGMENTOS_OMITIDO");
            return;
        };
        let words = diccionario();
        let rejilla = std::env::var("PRONTO_SEGMENTOS_REJILLA").unwrap_or_else(|_| "20/600/40".into());
        let o = crate::text::Options { cleanup: Settings::default().cleanup_level, press_enter: false, tone: Tone::Casual, words: &words, snippets: &[] };
        for set in sets.split(';').filter(|s| !s.is_empty()) {
            let (nombre, dir) = set.split_once('=').expect("nombre=carpeta");
            let muestras = cargar(Path::new(dir), Path::new(dir));
            let entero: Vec<String> = muestras.iter().map(|m| crate::text::process(&crate::state::transcribe_heard(&engine, &words, &m.audio, false).expect("t").1, &o).text).collect();
            let wer = |textos: &[String]| {
                let (mut e, mut n) = (0, 0);
                for (m, t) in muestras.iter().zip(textos) {
                    let r = crate::banco_entender::palabras(&m.referencia);
                    e += crate::banco_entender::alinear(&r, &crate::banco_entender::palabras(t)).total();
                    n += r.len();
                }
                100.0 * e as f64 / n.max(1) as f64
            };
            println!("SEGMENTOS {nombre} entero: WER {:.2} %", wer(&entero));
            for celda in rejilla.split(',') {
                let v: Vec<f64> = celda.split('/').map(|x| x.parse().expect("número")).collect();
                let (minimo, pausa, tope) = ((v[0] * 16_000.0) as usize, (v[1] / 20.0) as usize, (v[2] * 16_000.0) as usize);
                let (mut textos, mut cortes, mut colas, mut iguales) = (Vec::new(), 0, Vec::new(), 0);
                for (m, ent) in muestras.iter().zip(&entero) {
                    let mut f = crate::vivo::Fijador::con_pausa(pausa);
                    let mut segs = Vec::new();
                    let mut hasta = 0;
                    while hasta < m.audio.len() {
                        hasta = (hasta + 11_200).min(m.audio.len());
                        let pend = &m.audio[f.fijado()..hasta];
                        if let Some(n) = f.corte_segmento(pend, minimo, tope) {
                            let inicio = f.fijado();
                            let crudo = crate::state::transcribe_heard(&engine, &words, &m.audio[inicio..inicio + n], false).expect("t").1;
                            segs.push(crate::vivo::Segmento { inicio, fin: inicio + n, crudo });
                            f.fijar(n);
                        }
                    }
                    cortes += segs.len();
                    colas.push((m.audio.len() - f.fijado()) as f64 / 16_000.0);
                    let (raw, _) = crate::state::juntar_segmentos(&engine, &words, &m.audio, &segs, None).expect("cola");
                    let t = crate::text::process(&raw, &o).text;
                    iguales += usize::from(&t == ent);
                    textos.push(t);
                }
                colas.sort_by(|a, b| a.partial_cmp(b).unwrap());
                println!(
                    "SEGMENTOS {nombre} {celda}: WER {:.2} % · {cortes} cortes · cola p50 {:.1} s, máx {:.1} s · {iguales}/{} iguales al entero",
                    wer(&textos),
                    colas[colas.len() / 2],
                    colas[colas.len() - 1],
                    muestras.len()
                );
            }
        }
    }

    /// ms de soltar a texto listo con el pulido encendido. `PRONTO_VELOCIDAD_SETS` = `nombre=carpeta;…`,
    /// `PRONTO_BANCO_PULIDO` = modelo de Ollama. `PRONTO_VELOCIDAD_MODO` = `hoy` (audio entero al soltar,
    /// como antes de BELTR-PRT-16), `directo` (dictado a ritmo real con segmentos confirmados y pulido de
    /// fondo, el de la app) o `ambos` (por defecto). `PRONTO_VELOCIDAD_RITMO` = 1.0 (tiempo real).
    /// Saca también el % de palabras mal de cada modo frente a la referencia (`VELOCIDAD_…_WER`).
    #[test]
    fn banco_velocidad_pulido() {
        let (Ok(sets), Ok(modelo), Some(engine)) = (std::env::var("PRONTO_VELOCIDAD_SETS"), std::env::var("PRONTO_BANCO_PULIDO"), modelo_parakeet()) else {
            println!("VELOCIDAD_OMITIDO: faltan PRONTO_VELOCIDAD_SETS, PRONTO_BANCO_PULIDO y PRONTO_NSN_MODELO");
            return;
        };
        let words = diccionario();
        let ajustes = ajustes_con_pulido(&modelo);
        calentar_pulido(&ajustes);
        for set in sets.split(';').filter(|s| !s.is_empty()) {
            let (nombre, dir) = set.split_once('=').expect("nombre=carpeta");
            let mut muestras = cargar(Path::new(dir), Path::new(dir));
            assert!(!muestras.is_empty(), "no hay audio en {dir}");
            // `PRONTO_VELOCIDAD_MAX`: solo las primeras N (la versión rápida de `npm test`).
            if let Some(n) = std::env::var("PRONTO_VELOCIDAD_MAX").ok().and_then(|v| v.parse::<usize>().ok()) {
                muestras.truncate(n);
            }
            // Calienta motor y modelo con la primera (no cuenta).
            let _ = soltar_sin_vivo(&engine, &words, &ajustes, &muestras[0].audio);
            let modo = std::env::var("PRONTO_VELOCIDAD_MODO").unwrap_or_else(|_| "ambos".into());
            let ritmo: f64 = std::env::var("PRONTO_VELOCIDAD_RITMO").ok().and_then(|v| v.parse().ok()).unwrap_or(1.0);
            for m_ in ["hoy", "directo"].into_iter().filter(|x| modo == "ambos" || modo == *x) {
                let (mut ms, mut errores, mut refs, mut segs, mut trozos, mut anticipados) = (Vec::new(), 0usize, 0usize, 0usize, 0usize, 0usize);
                for m in &muestras {
                    let (final_, t, n, ok) = if m_ == "hoy" {
                        let (_, f, t, _) = soltar_sin_vivo(&engine, &words, &ajustes, &m.audio);
                        (f, t, 0, false)
                    } else {
                        soltar_en_directo(&engine, &words, &ajustes, &m.audio, ritmo)
                    };
                    let r = crate::banco_entender::palabras(&m.referencia);
                    let e = crate::banco_entender::alinear(&r, &crate::banco_entender::palabras(&final_)).total();
                    errores += e;
                    refs += r.len();
                    // `n` = segmentos + 100 si la cola salió del texto anticipado.
                    let (n, ant) = (n % 100, n >= 100);
                    segs += n;
                    anticipados += usize::from(ant);
                    trozos += usize::from(ok);
                    ms.push(t);
                    println!("VELOCIDAD_MUESTRA {nombre} {m_} {} ({:.1} s): {t:.0} ms · {n} segmentos{}{} · {e} errores", m.nombre, m.audio.len() as f64 / 16000.0, if ant { " · cola anticipada" } else { "" }, if ok { " · pulido por trozos" } else { "" });
                    if std::env::var("PRONTO_BANCO_VOLCAR").is_ok() {
                        println!("   final: {final_}");
                    }
                }
                let k = format!("{}_{}", nombre.to_uppercase(), m_.to_uppercase());
                let wer = 100.0 * errores as f64 / refs.max(1) as f64;
                println!("VELOCIDAD_{k}_MUESTRAS={}", ms.len());
                println!("VELOCIDAD_{k}_P50={:.0}", percentil(&ms, 0.5));
                println!("VELOCIDAD_{k}_P90={:.0}", percentil(&ms, 0.9));
                println!("VELOCIDAD_{k}_WER={wer:.2}");
                println!("VELOCIDAD_RESUMEN {nombre} {m_}: {} dictados · soltar→texto p50 {:.0} ms, p90 {:.0} ms · WER {wer:.1} % · {segs} segmentos · {anticipados} con la cola anticipada · {trozos} pulidos por trozos", ms.len(), percentil(&ms, 0.5), percentil(&ms, 0.9));
            }
        }
    }
}
