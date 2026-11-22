//! Banco de entendimiento (BELTR-PRT-11): mide lo bien que Pronto entiende al dictar, con las cuatro
//! cifras del contrato de `specs/features/entender-y-diccionario.md`:
//! - WER: % de palabras mal (cambiadas, perdidas o añadidas) frente al texto de referencia.
//! - Diccionario: de las palabras guardadas que se dijeron, cuántas salen escritas tal cual.
//! - Inventadas: palabras que salen sin haberse dicho (añadidas, o del diccionario donde no se dijo).
//! - ms de soltar a texto: el camino del dictado (`state::transcribe_heard` + `text::process`).
//! Mide el texto final y la vista previa del texto en vivo (lo que se ve en la Flow bar al soltar).
//!
//! Lo lanza `tests/banco-entender.test.mjs` con `PRONTO_BANCO_SETS` (`nombre=carpeta_wav[|carpeta_txt]`
//! separados por `;`; cada `X.wav` con su `X.txt` de referencia), `PRONTO_BANCO_DICCIONARIO` y
//! `PRONTO_NSN_MODELO` (Parakeet). Sin ellos, no mide (`BANCO_OMITIDO`). Las cifras salen en líneas
//! `BANCO_…=` para que el test de node las compare con `tests/banco-entender/umbrales.json`.

/// Pulido y formatos (BELTR-PRT-10): solo pruebas, va aquí para no tocar `main.rs`.
#[cfg(test)]
#[path = "banco_pulido.rs"]
pub mod banco_pulido;

use crate::asr::Engine;
use crate::models::{Word, WordVariant};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

/// Palabras de un texto para comparar: minúsculas, sin signos (se quedan letras, números y tildes).
pub fn palabras(texto: &str) -> Vec<String> {
    texto
        .chars()
        .map(|c| if c.is_alphanumeric() { c.to_lowercase().next().unwrap_or(c) } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

/// Igual, conservando mayúsculas (para ver si una palabra del diccionario sale tal cual).
fn palabras_tal_cual(texto: &str) -> Vec<String> {
    texto.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_string).collect()
}

/// Errores de una alineación palabra a palabra (distancia de Levenshtein).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Errores {
    pub cambiadas: usize,
    pub perdidas: usize,
    pub anadidas: usize,
}

impl Errores {
    pub fn total(&self) -> usize {
        self.cambiadas + self.perdidas + self.anadidas
    }
}

pub fn alinear(referencia: &[String], oido: &[String]) -> Errores {
    let (n, m) = (referencia.len(), oido.len());
    // d[i][j] = (coste, cambiadas, perdidas, añadidas) para referencia[..i] frente a oido[..j].
    let mut d = vec![vec![(0usize, Errores::default()); m + 1]; n + 1];
    for i in 1..=n {
        d[i][0] = (i, Errores { perdidas: i, ..Default::default() });
    }
    for j in 1..=m {
        d[0][j] = (j, Errores { anadidas: j, ..Default::default() });
    }
    for i in 1..=n {
        for j in 1..=m {
            let igual = referencia[i - 1] == oido[j - 1];
            let (c_diag, mut e_diag) = d[i - 1][j - 1];
            let diag = c_diag + usize::from(!igual);
            if !igual {
                e_diag.cambiadas += 1;
            }
            let (c_up, mut e_up) = d[i - 1][j];
            e_up.perdidas += 1;
            let (c_left, mut e_left) = d[i][j - 1];
            e_left.anadidas += 1;
            // A igual coste, mejor cambiada que perdida+añadida, y mejor perdida que añadida (no
            // se cuenta como inventado lo que puede ser un cambio).
            d[i][j] = if diag <= c_up + 1 && diag <= c_left + 1 {
                (diag, e_diag)
            } else if c_up + 1 <= c_left + 1 {
                (c_up + 1, e_up)
            } else {
                (c_left + 1, e_left)
            };
        }
    }
    d[n][m].1
}

/// Formas buenas del diccionario: el reemplazo si lo hay, si no la palabra.
pub fn objetivos(words: &[Word]) -> Vec<String> {
    let mut v: Vec<String> = words.iter().map(|w| w.replacement.clone().filter(|r| !r.trim().is_empty()).unwrap_or_else(|| w.word.clone())).collect();
    v.sort();
    v.dedup();
    v
}

/// Cuenta del diccionario en un texto: (dichas, bien escritas, salidas sin decirse).
pub fn cuenta_diccionario(referencia: &str, texto: &str, objetivos: &[String]) -> (usize, usize, usize) {
    let r = palabras_tal_cual(referencia);
    let t = palabras_tal_cual(texto);
    let (mut dichas, mut bien, mut falsas) = (0, 0, 0);
    for o in objetivos {
        // Palabras de varias partes («Claude Code»): se cuentan por su primera palabra con el resto detrás.
        let partes: Vec<&str> = o.split_whitespace().collect();
        let cuenta = |v: &[String]| (0..v.len()).filter(|&i| partes.iter().enumerate().all(|(k, p)| v.get(i + k).is_some_and(|w| w == p))).count();
        let cuenta_sin_mayus = |v: &[String]| (0..v.len()).filter(|&i| partes.iter().enumerate().all(|(k, p)| v.get(i + k).is_some_and(|w| w.to_lowercase() == p.to_lowercase()))).count();
        let en_ref = cuenta_sin_mayus(&r);
        let en_texto = cuenta(&t);
        dichas += en_ref;
        bien += en_ref.min(en_texto);
        falsas += en_texto.saturating_sub(en_ref);
    }
    (dichas, bien, falsas)
}

/// Lee el diccionario del banco: «palabra» o «oído => palabra» por línea; `#` comenta.
pub fn leer_diccionario(texto: &str) -> Vec<Word> {
    texto
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .enumerate()
        .map(|(i, l)| {
            let (word, replacement) = match l.split_once("=>") {
                Some((a, b)) => (a.trim().to_string(), Some(b.trim().to_string())),
                None => (l.to_string(), None),
            };
            Word { id: i as i64 + 1, word, replacement, created_at: String::new(), starred: false, source: "manual".into(), variants: Vec::<WordVariant>::new(), samples: Vec::new() }
        })
        .collect()
}

pub struct Muestra {
    pub nombre: String,
    pub audio: Vec<f32>,
    pub referencia: String,
}

/// WAV de `wavs` con su `.txt` de referencia en `refs` (la misma carpeta si no se da). Los que no
/// tienen referencia no cuentan.
pub fn cargar(wavs: &Path, refs: &Path) -> Vec<Muestra> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(wavs).map(|d| d.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "wav")).collect()).unwrap_or_default();
    v.sort();
    v.into_iter()
        .filter_map(|w| {
            let stem = w.file_stem()?.to_string_lossy().into_owned();
            let referencia = std::fs::read_to_string(refs.join(format!("{stem}.txt"))).ok()?;
            // Las líneas que empiezan por `#` son notas (p. ej. «provisional»), no texto dicho.
            let referencia = referencia.lines().filter(|l| !l.trim_start().starts_with('#')).collect::<Vec<_>>().join(" ");
            let audio = crate::audio::decode_wav(&std::fs::read(&w).ok()?)?;
            Some(Muestra { nombre: stem, audio, referencia })
        })
        .collect()
}

/// Cifras de un conjunto con un motor.
#[derive(Debug, Default, Clone)]
pub struct Cifras {
    pub muestras: usize,
    pub palabras: usize,
    pub errores: Errores,
    pub dic_dichas: usize,
    pub dic_bien: usize,
    pub dic_falsas: usize,
    pub ms: Vec<f64>,
    pub vista_palabras: usize,
    pub vista_errores: Errores,
    pub vista_dic_bien: usize,
    pub vista_dic_falsas: usize,
}

impl Cifras {
    pub fn wer(&self) -> f64 {
        100.0 * self.errores.total() as f64 / self.palabras.max(1) as f64
    }
    pub fn vista_wer(&self) -> f64 {
        100.0 * self.vista_errores.total() as f64 / self.vista_palabras.max(1) as f64
    }
    pub fn inventadas(&self) -> usize {
        self.errores.anadidas + self.dic_falsas
    }
    pub fn vista_inventadas(&self) -> usize {
        self.vista_errores.anadidas + self.vista_dic_falsas
    }
    pub fn dic_pct(&self) -> f64 {
        100.0 * self.dic_bien as f64 / self.dic_dichas.max(1) as f64
    }
    pub fn ms_p50(&self) -> f64 {
        let mut v = self.ms.clone();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        if v.is_empty() {
            0.0
        } else {
            v[v.len() / 2]
        }
    }
}

/// Texto final de un dictado tal como lo pega Pronto sin pulido: `transcribe_heard` + `text::process`
/// con el diccionario (ajustes de fábrica, estilo «Otros» = casual).
pub fn dictar(engine: &Engine, words: &[Word], audio: &[f32]) -> (String, f64) {
    let t0 = Instant::now();
    engine.set_vocabulary(&crate::state::vocabulary(words));
    let (_, oido, _) = crate::state::transcribe_heard(engine, words, audio, false).expect("transcribir");
    let s = crate::models::Settings::default();
    let options = crate::text::Options { cleanup: s.cleanup_level, press_enter: s.press_enter_command, tone: crate::models::Tone::Casual, words, snippets: &[] };
    let texto = crate::text::process(&oido, &options).text;
    (texto, t0.elapsed().as_secs_f64() * 1000.0)
}

/// Mide un conjunto. `vista`: además, la vista previa del texto en vivo al soltar.
pub fn medir(engine: &Arc<Engine>, words: &[Word], muestras: &[Muestra], vista: bool, volcar: bool) -> Cifras {
    let obj = objetivos(words);
    let mut c = Cifras::default();
    // Calienta el motor (la primera inferencia es más lenta).
    if let Some(m) = muestras.first() {
        let _ = dictar(engine, words, &m.audio);
    }
    for m in muestras {
        let referencia = palabras(&m.referencia);
        let (texto, ms) = dictar(engine, words, &m.audio);
        let e = alinear(&referencia, &palabras(&texto));
        let (dichas, bien, falsas) = cuenta_diccionario(&m.referencia, &texto, &obj);
        c.muestras += 1;
        c.palabras += referencia.len();
        c.errores.cambiadas += e.cambiadas;
        c.errores.perdidas += e.perdidas;
        c.errores.anadidas += e.anadidas;
        c.dic_dichas += dichas;
        c.dic_bien += bien;
        c.dic_falsas += falsas;
        c.ms.push(ms);
        if volcar {
            println!("BANCO_MUESTRA {} ({:.1} s, {ms:.0} ms) errores {} · dic {bien}/{dichas} · falsas {falsas}", m.nombre, m.audio.len() as f64 / 16000.0, e.total());
            println!("   ref:   {}", m.referencia.trim());
            println!("   final: {texto}");
        }
        if vista {
            let v = crate::vivo::tests_vista_al_soltar(engine, words, &m.audio);
            let ev = alinear(&referencia, &palabras(&v));
            let (_, vb, vf) = cuenta_diccionario(&m.referencia, &v, &obj);
            c.vista_palabras += referencia.len();
            c.vista_errores.cambiadas += ev.cambiadas;
            c.vista_errores.perdidas += ev.perdidas;
            c.vista_errores.anadidas += ev.anadidas;
            c.vista_dic_bien += vb;
            c.vista_dic_falsas += vf;
            if volcar {
                println!("   vista: {v}");
            }
        }
    }
    c
}

/// Con el pulido de IA local (Ollama, como lo tiene Txonso): cuántas palabras del diccionario quedan
/// bien tras pulir sin y con la vuelta del diccionario que hace `state::finish_dictation`. Solo
/// informativo (el modelo no es determinista): `PRONTO_BANCO_PULIDO=modelo` (p. ej. `qwen3:1.7b`).
pub fn medir_pulido(engine: &Engine, words: &[Word], muestras: &[Muestra], modelo: &str, conjunto: &str) {
    let obj = objetivos(words);
    let mut s = crate::models::Settings::default();
    s.ai_polish = true;
    s.ai_polish_model = modelo.to_string();
    s.ai_polish_timeout_ms = 20_000;
    let (mut dichas, mut sin, mut con, mut pulidos, mut err_con, mut inv_con, mut palabras_ref) = (0, 0, 0, 0, 0, 0, 0);
    for m in muestras {
        let (texto, _) = dictar(engine, words, &m.audio);
        let p = crate::polish::apply(&s, &texto, crate::models::Tone::Casual, 0);
        let arreglado = if p.polished && !words.is_empty() { crate::text::apply_dictionary(&p.text, words) } else { p.text.clone() };
        pulidos += usize::from(p.polished);
        let (d, b_sin, _) = cuenta_diccionario(&m.referencia, &p.text, &obj);
        let (_, b_con, f_con) = cuenta_diccionario(&m.referencia, &arreglado, &obj);
        let e = alinear(&palabras(&m.referencia), &palabras(&arreglado));
        dichas += d;
        sin += b_sin;
        con += b_con;
        err_con += e.total();
        inv_con += e.anadidas + f_con;
        palabras_ref += palabras(&m.referencia).len();
        if p.text != texto {
            println!("   PULIDO {}: {texto}\n          → {}{}", m.nombre, p.text, if arreglado != p.text { format!("\n          → {arreglado} (diccionario)") } else { String::new() });
        }
    }
    println!(
        "BANCO_PULIDO {conjunto} ({modelo}): {pulidos}/{} pulidos · diccionario tras pulir {sin}/{dichas} sin la vuelta del diccionario, {con}/{dichas} con ella · WER {:.1} % · inventadas {inv_con}",
        muestras.len(),
        100.0 * err_con as f64 / palabras_ref.max(1) as f64
    );
}

pub fn imprimir(conjunto: &str, motor: &str, c: &Cifras) {
    let k = format!("{conjunto}_{motor}").to_uppercase().replace('-', "_");
    println!("BANCO_{k}_MUESTRAS={}", c.muestras);
    println!("BANCO_{k}_PALABRAS={}", c.palabras);
    println!("BANCO_{k}_WER={:.2}", c.wer());
    println!("BANCO_{k}_DIC={}/{}", c.dic_bien, c.dic_dichas);
    println!("BANCO_{k}_INVENTADAS={}", c.inventadas());
    println!("BANCO_{k}_MS_P50={:.0}", c.ms_p50());
    println!("BANCO_{k}_MS_MAX={:.0}", c.ms.iter().cloned().fold(0.0, f64::max));
    if c.vista_palabras > 0 {
        println!("BANCO_{k}_VISTA_WER={:.2}", c.vista_wer());
        println!("BANCO_{k}_VISTA_DIC={}/{}", c.vista_dic_bien, c.dic_dichas);
        println!("BANCO_{k}_VISTA_INVENTADAS={}", c.vista_inventadas());
    }
    println!(
        "BANCO_RESUMEN {conjunto} {motor}: {} muestras, {} palabras · WER {:.1} % ({} cambiadas, {} perdidas, {} añadidas) · diccionario {}/{} ({:.0} %) · inventadas {} · soltar→texto p50 {:.0} ms{}",
        c.muestras,
        c.palabras,
        c.wer(),
        c.errores.cambiadas,
        c.errores.perdidas,
        c.errores.anadidas,
        c.dic_bien,
        c.dic_dichas,
        c.dic_pct(),
        c.inventadas(),
        c.ms_p50(),
        if c.vista_palabras > 0 { format!(" · vista: WER {:.1} %, diccionario {}/{}, inventadas {}", c.vista_wer(), c.vista_dic_bien, c.dic_dichas, c.vista_inventadas()) } else { String::new() }
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_alineacion_cuenta_cambiadas_perdidas_y_anadidas() {
        let r = palabras("Sube el código a GitHub, por favor.");
        assert_eq!(alinear(&r, &palabras("sube el código a GitHub por favor")).total(), 0);
        let e = alinear(&r, &palabras("Sube el código a Guitjub por favor gracias"));
        assert_eq!(e, Errores { cambiadas: 1, perdidas: 0, anadidas: 1 });
        let e = alinear(&r, &palabras("Sube código a GitHub por favor"));
        assert_eq!(e, Errores { cambiadas: 0, perdidas: 1, anadidas: 0 });
    }

    #[test]
    fn el_diccionario_cuenta_lo_dicho_lo_bien_escrito_y_lo_falso() {
        let obj = vec!["GitHub".to_string(), "BeltranLabs".to_string(), "Claude Code".to_string()];
        assert_eq!(cuenta_diccionario("Sube a GitHub y a GitHub.", "Sube a Github y a GitHub.", &obj), (2, 1, 0));
        assert_eq!(cuenta_diccionario("Hola equipo.", "Hola BeltranLabs.", &obj), (0, 0, 1));
        assert_eq!(cuenta_diccionario("Abre Claude Code ya.", "Abre Claude Code ya.", &obj), (1, 1, 0));
        assert_eq!(cuenta_diccionario("Abre Claude Code ya.", "Abre Cloud Code ya.", &obj), (1, 0, 0));
    }

    #[test]
    fn el_diccionario_del_banco_admite_reemplazos() {
        let w = leer_diccionario("# nota\nBeltranLabs\nGihad => GitHub\n");
        assert_eq!(w.len(), 2);
        assert_eq!(objetivos(&w), vec!["BeltranLabs".to_string(), "GitHub".to_string()]);
    }

    /// Prueba en contra (BELTR-PRT-11): con el diccionario del banco cargado, un texto corriente en
    /// español con trampas que suenan parecido («olla», «sonata», «su pase», «pero aquí te») no cambia
    /// ni una palabra. También la vista previa en vivo, que pasa por lo mismo.
    #[test]
    fn el_diccionario_no_toca_texto_corriente() {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/banco-entender");
        let words = leer_diccionario(&std::fs::read_to_string(base.join("diccionario.txt")).expect("diccionario"));
        let texto = std::fs::read_to_string(base.join("texto-corriente.txt")).expect("texto corriente");
        let mut cambios = Vec::new();
        let mut frases = 0;
        for l in texto.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
            frases += 1;
            let fuera = crate::text::apply_dictionary(l, &words);
            if fuera != l {
                cambios.push(format!("  {l}\n→ {fuera}"));
            }
        }
        assert!(frases >= 50, "el texto corriente se quedó corto ({frases} frases)");
        assert!(cambios.is_empty(), "el diccionario cambió texto corriente:\n{}", cambios.join("\n"));
    }

    /// Lo que la pasada aproximada sí tiene que arreglar (lo que Parakeet escribe de verdad).
    #[test]
    fn el_diccionario_arregla_palabras_partidas_y_casi_iguales() {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/banco-entender");
        let words = leer_diccionario(&std::fs::read_to_string(base.join("diccionario.txt")).expect("diccionario"));
        for (oido, bien) in [
            ("la factura de Beltran Labs", "la factura de BeltranLabs"),
            ("en Beltranabs probamos", "en BeltranLabs probamos"),
            ("los usuarios en su pavase y", "los usuarios en Supabase y"),
            ("El modelo Perakit transcribe", "El modelo Parakeet transcribe"),
            ("más rápido que Wispar en", "más rápido que Whisper en"),
            ("Pregúntale a Sanit si", "Pregúntale a Sonnet si"),
            ("Estimado Chonso: te escribo", "Estimado Txonso: te escribo"),
        ] {
            assert_eq!(crate::text::apply_dictionary(oido, &words), bien);
        }
    }

    /// La vista previa del texto en vivo ya sale con el diccionario (BELTR-PRT-11).
    #[test]
    fn la_vista_previa_lleva_el_diccionario() {
        let words = leer_diccionario("Txonso\nOllama\nBeltranLabs\n");
        let v = crate::vivo::Vista { fixed: "Hola Chonso, abre Oyama.".into(), partial: "Lo de Beltran Labs".into() };
        let d = v.con_diccionario(&words);
        assert_eq!((d.fixed.as_str(), d.partial.as_str()), ("Hola Txonso, abre Ollama.", "Lo de BeltranLabs"));
        assert_eq!(v.con_diccionario(&[]), v);
    }

    /// El banco de verdad (ver la cabecera del módulo).
    #[test]
    fn banco_entender() {
        let (Ok(sets), Ok(modelo)) = (std::env::var("PRONTO_BANCO_SETS"), std::env::var("PRONTO_NSN_MODELO")) else {
            println!("BANCO_OMITIDO: faltan PRONTO_BANCO_SETS y PRONTO_NSN_MODELO");
            return;
        };
        let words = std::env::var("PRONTO_BANCO_DICCIONARIO").ok().map(|p| leer_diccionario(&std::fs::read_to_string(p).expect("diccionario"))).unwrap_or_default();
        let volcar = std::env::var("PRONTO_BANCO_VOLCAR").is_ok();
        let vista = std::env::var("PRONTO_BANCO_SIN_VISTA").is_err();
        let mut motores: Vec<(&str, Arc<Engine>)> = vec![("parakeet", Arc::new(Engine::load_for_tests(&modelo).expect("Parakeet")))];
        // `PRONTO_BANCO_WHISPER=1` (el de fábrica) o una lista de modelos: `medium,large-v3-turbo`.
        let whisper = std::env::var("PRONTO_BANCO_WHISPER").unwrap_or_default();
        let nombres: Vec<String> = match whisper.as_str() {
            "" => Vec::new(),
            "1" => vec![String::new()],
            l => l.split(',').map(|m| m.trim().to_string()).collect(),
        };
        for m in nombres {
            let etiqueta: &'static str = Box::leak(if m.is_empty() { "whisper".to_string() } else { format!("whisper-{m}") }.into_boxed_str());
            motores.push((etiqueta, Arc::new(Engine::load_whisper_model_for_tests(&m, "auto").expect("Whisper"))));
        }
        for set in sets.split(';').filter(|s| !s.is_empty()) {
            let (nombre, rutas) = set.split_once('=').expect("nombre=carpeta");
            let (wavs, refs) = rutas.split_once('|').unwrap_or((rutas, rutas));
            let muestras = cargar(Path::new(wavs), Path::new(refs));
            assert!(!muestras.is_empty(), "el conjunto {nombre} no tiene WAV con referencia en {wavs}");
            for (motor, engine) in &motores {
                let c = medir(engine, &words, &muestras, vista, volcar);
                imprimir(nombre, motor, &c);
            }
            if let Ok(modelo) = std::env::var("PRONTO_BANCO_PULIDO") {
                medir_pulido(&motores[0].1, &words, &muestras, &modelo, nombre);
            }
        }
    }
}


