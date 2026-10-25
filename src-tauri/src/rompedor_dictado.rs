//! Sondas del rompedor (2026-10-22, tanda paralela): dictado. Solo se compilan en pruebas.
//! Las que demuestran una rotura llevan #[ignore = "rompedor: <título>"].
//!
//! Zona: tubería de texto (`text.rs`), huella de voz (`voiceprint.rs`), historial (`db.rs`) y las
//! funciones puras del portapapeles (`inject.rs`, compiladas aparte desde su fuente, ver abajo).
//! Nunca el portapapeles real, el teclado, el micrófono ni `%APPDATA%\Pronto2`: bases en memoria o
//! en carpetas temporales propias.
//!
//! `cargo test rompedor_dictado_ -- --include-ignored --nocapture --test-threads=1`
//! La de 1 M de filas solo corre con `PRONTO_ROMPEDOR_DICTADO_1M=1`.
#![allow(dead_code, unused_imports)]

use crate::db::{Db, NewHistory};
use crate::models::{CleanupLevel, Fixes, Snippet, Tone, Word, WordVariant};
use crate::text::{self, Options, Processed};
use std::path::PathBuf;
use std::time::Instant;

// ------------------------------------------------------------ utilidades

struct Tmp(PathBuf);

impl Tmp {
    fn new(name: &str) -> Self {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("pronto2-rompedor-dictado-{name}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Tmp(dir)
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn word(w: &str, r: Option<&str>, heard: &[&str]) -> Word {
    let variants = heard.iter().enumerate().map(|(i, h)| WordVariant { id: i as i64, heard: (*h).into() }).collect();
    Word { id: 0, word: w.into(), replacement: r.map(String::from), created_at: String::new(), starred: false, source: "manual".into(), variants, samples: Vec::new() }
}

fn snip(t: &str, e: &str) -> Snippet {
    Snippet { id: 0, trigger: t.into(), expansion: e.into(), created_at: String::new() }
}

fn run(raw: &str, cleanup: CleanupLevel, tone: Tone, words: &[Word], snippets: &[Snippet], press_enter: bool) -> Processed {
    text::process(raw, &Options { cleanup, press_enter, tone, words, snippets })
}

/// Como lo deja Pronto de fábrica: limpieza media, Casual.
fn default_run(raw: &str, words: &[Word], snippets: &[Snippet]) -> Processed {
    run(raw, CleanupLevel::Medium, Tone::Casual, words, snippets, false)
}

fn hist<'a>(text: &'a str, transform: Option<&'a str>) -> NewHistory<'a> {
    NewHistory { polished: false, text, raw: text, app: "Notepad", duration_ms: 1000, words: text.split_whitespace().count() as i64, fixes: Fixes::default(), transform }
}

const LEVELS: [CleanupLevel; 4] = [CleanupLevel::None, CleanupLevel::Light, CleanupLevel::Medium, CleanupLevel::High];
const TONES: [Tone; 4] = [Tone::Formal, Tone::Casual, Tone::VeryCasual, Tone::Excited];

// ------------------------------------------------------------ tubería: roturas

/// FR-02 R4 + regla 5: «quiero decir» al final de una frase normal vacía el dictado entero. Con el
/// texto vacío `transcribe_and_paste` sale por `nothing_heard` (state.rs:584) ANTES de
/// `finish_dictation`: ni pegado, ni historial, ni audio. El usuario pierde lo dictado.
#[test]
fn rompedor_dictado_quiero_decir_vacia_el_dictado() {
    let frases = [
        "Eso es justo lo que quiero decir.",
        "No sé muy bien lo que quiero decir",
        "Me has entendido mal, no es lo que quiero decir.",
        "Lo que te quiero decir es que mañana no puedo.",
        "Creo que eso está mejor dicho así.",
    ];
    let mut vacios = Vec::new();
    for f in frases {
        let r = default_run(f, &[], &[]);
        println!("«{f}» -> «{}»", r.text);
        if r.text.trim().is_empty() && !r.press_enter {
            vacios.push(f);
        }
    }
    assert!(vacios.is_empty(), "dictados que la tubería deja vacíos (= «No se ha oído nada», sin historial): {vacios:?}");
}

/// FR-02 R6: la holgura de la vocal inicial (`near_key`) convierte «llama» (verbo corriente) en
/// «Ollama» si «Ollama» está en el diccionario, como en el caso real del propio test de text.rs.
#[test]
fn rompedor_dictado_suena_igual_cambia_palabras_correctas() {
    let ollama = [word("Ollama", None, &["Oyama"])];
    let casos = [
        ("Mi madre me llama todos los días.", &ollama[..], "llama"),
        ("Te llama Marta.", &ollama[..], "llama"),
    ];
    let zara = [word("Zara", None, &[])];
    let baca = [word("Baca", None, &[])];
    let mut rotos = Vec::new();
    for (raw, w, debe) in casos.iter().map(|(a, b, c)| (*a, *b, *c)).chain([
        ("Ayer cené con Sara y su hermano.", &zara[..], "Sara"),
        ("La vaca está en el prado.", &baca[..], "vaca"),
    ]) {
        let r = default_run(raw, w, &[]);
        println!("dicc {:?}: «{raw}» -> «{}» (arreglos {})", w.iter().map(|x| &x.word).collect::<Vec<_>>(), r.text, r.fixes_dictionary);
        // Palabra completa: «Ollama» contiene «llama» y con `contains` la rotura pasaba por buena.
        if !r.text.split(|c: char| !c.is_alphanumeric()).any(|w| w == debe) {
            rotos.push((raw, r.text));
        }
    }
    assert!(rotos.is_empty(), "palabras correctas cambiadas por «suena igual»: {rotos:?}");
}

/// FR-02 R7: un snippet cuyo disparador contiene una palabra del diccionario no se expande nunca:
/// el diccionario (paso anterior) cambia esa palabra por un marcador y el disparador ya no casa.
#[test]
fn rompedor_dictado_snippet_con_palabra_del_diccionario() {
    let words = [word("Txonso", None, &[])];
    let snippets = [snip("firma txonso", "Un saludo,\nTxonso Beltrán\nBeltranLabs")];
    let sin_dicc = default_run("Firma Txonso.", &[], &snippets);
    let con_dicc = default_run("Firma Txonso.", &words, &snippets);
    let dentro = default_run("Pon al final la firma txonso y envíalo", &words, &snippets);
    println!("sin diccionario: {:?}", sin_dicc.text);
    println!("con «Txonso» en el diccionario: {:?} (snippets {})", con_dicc.text, con_dicc.fixes_snippets);
    println!("dentro de la frase: {:?}", dentro.text);
    assert_eq!(sin_dicc.text, snippets[0].expansion, "control: sin diccionario sí se expande");
    assert_eq!(con_dicc.text, snippets[0].expansion, "con la palabra en el diccionario el snippet no se expande");
    assert!(dentro.text.contains("BeltranLabs"), "dentro de la frase tampoco");
}

/// FR-02 R10: «No pulse enter.» (orden negativa) termina en «pulse enter» y Pronto pulsa Enter.
#[test]
fn rompedor_dictado_no_pulse_enter_pulsa_enter() {
    let r = run("Revisa el formulario y no pulse enter.", CleanupLevel::Medium, Tone::Casual, &[], &[], true);
    println!("-> {:?}, press_enter={}", r.text, r.press_enter);
    assert!(!r.press_enter, "una orden negativa no debería pulsar Enter (y el texto pierde «no pulse enter»: {:?})", r.text);
}

/// R1 (FR-02): un carácter de uso privado U+E000 en el texto crudo se toma por un marcador abierto y
/// `restore_placeholders` se come todo hasta el siguiente cierre: se pierde texto del dictado.
#[test]
fn rompedor_dictado_caracter_privado_se_come_texto() {
    let words = [word("Txonso", None, &[])];
    let raw = "Hola \u{E000} esto se pierde entero hasta que diga txonso y sigo.";
    let r = default_run(raw, &words, &[]);
    println!("-> {:?}", r.text);
    assert!(r.text.contains("esto se pierde"), "texto perdido: {:?}", r.text);
}

/// Búsqueda del historial: «óscar» no encuentra «Óscar» (LIKE de SQLite solo pliega ASCII).
#[test]
fn rompedor_dictado_buscar_no_pliega_mayusculas_acentuadas() {
    let db = Db::open_memory().unwrap();
    for t in ["Óscar llega a las tres", "Ángel no viene", "ÚLTIMO aviso", "oscar sin tilde"] {
        db.add_history(&hist(t, None)).unwrap();
    }
    let mut fallos = Vec::new();
    for (q, debe) in [("óscar", "Óscar llega a las tres"), ("ángel", "Ángel no viene"), ("último", "ÚLTIMO aviso"), ("Oscar", "oscar sin tilde")] {
        let got: Vec<String> = db.list_history(Some(q), 50, 0).unwrap().into_iter().map(|h| h.text).collect();
        println!("buscar {q:?} -> {got:?}");
        if !got.iter().any(|t| t == debe) {
            fallos.push(q);
        }
    }
    assert!(fallos.is_empty(), "búsquedas que no encuentran su dictado: {fallos:?}");
}

/// FR-01 R9: «pegar/copiar último» toma la última fila sea cual sea. Tras una transcripción fallida
/// (regla 5: fila con texto vacío y su audio) devuelve «»: `copy_last` pone «» en el portapapeles
/// (sin restaurar: el portapapeles del usuario se pierde) y avisa «Copiado». Tras un transform pega el
/// transform aunque el doc diga «Último dictado (no transform)».
#[test]
fn rompedor_dictado_ultimo_tras_transcripcion_fallida() {
    let db = Db::open_memory().unwrap();
    db.add_history(&hist("Dictado bueno de antes", None)).unwrap();
    // Lo que guarda keep_failed_transcription (state.rs:677).
    db.add_history(&NewHistory { polished: false, text: "", raw: "", app: "Notepad", duration_ms: 4000, words: 0, fixes: Fixes::default(), transform: None }).unwrap();
    let ultimo = db.last_history_text();
    println!("tras fallo: last_history_text = {ultimo:?}");
    db.add_history(&hist("RESULTADO DEL TRANSFORM", Some("Polish"))).unwrap();
    let tras_transform = db.last_history_text();
    println!("tras transform: last_history_text = {tras_transform:?}");
    assert_ne!(ultimo.as_deref(), Some(""), "«pegar/copiar último» devuelve un texto vacío");
    assert_eq!(tras_transform.as_deref(), Some("Dictado bueno de antes"), "devuelve el transform aunque el doc dice «no transform»");
}

// ------------------------------------------------------------ tubería: lo que aguanta

/// Entradas §3 en todos los niveles y estilos: ni pánico, ni marcadores internos en la salida, y los
/// emojis con ZWJ, RTL, combinantes, NUL y mayúsculas turcas siguen ahí.
#[test]
fn rompedor_dictado_entradas_hostiles_en_toda_la_tuberia() {
    let words = [word("Txonso", None, &["Cholso"]), word("İzmir", None, &[]), word("a.b", Some("X$1"), &[]), word("(", Some("paren"), &[]), word(".*", Some("todo"), &[]), word("  ", Some("vacío"), &[]), word("vacía", Some(""), &[])];
    let snippets = [snip("uno", "dos"), snip("dos", "uno"), snip("tres", "tres tres"), snip("...", "puntos"), snip("¿?", "nada"), snip("\\d+", "regex")];
    let fam = "👨\u{200D}👩\u{200D}👧\u{200D}👦";
    let flag = "🏳\u{FE0F}\u{200D}🌈";
    let entradas: Vec<String> = vec![
        String::new(),
        " ".into(),
        "\0".into(),
        "hola\0mundo".into(),
        format!("Mi familia {fam} y la bandera {flag}."),
        "مرحبا بالعالم \u{202E}txet desrever\u{202C} שלום".into(),
        "José y Jose\u{301} y n\u{303}u".into(),
        "İSTANBUL ıi IZMIR izmir ǅ ß ŉ".into(),
        "?!...,,;:¿¡…".into(),
        ", , , ;".into(),
        "eh eh eh mmm um".into(),
        "uno dos tres".into(),
        "a.b axb \\d+ 123 ( ) .* todo".into(),
        "borra eso ".repeat(40),
        "\u{E001}\u{E100}\u{E001}".into(),
        "\u{FEFF}hola\u{200B}mundo\u{00A0}fin".into(),
    ];
    let mut perdidos = Vec::new();
    for raw in &entradas {
        for lvl in LEVELS {
            for tone in TONES {
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(raw, lvl, tone, &words, &snippets, true)));
                let Ok(r) = r else { panic!("pánico con {raw:?} {lvl:?} {tone:?}") };
                assert!(!r.text.contains(['\u{E000}', '\u{E001}']) || raw.contains(['\u{E000}', '\u{E001}']), "marcador en la salida: {raw:?} -> {:?}", r.text);
                for keep in [fam, flag] {
                    if raw.contains(keep) && !r.text.contains(keep) {
                        perdidos.push((raw.clone(), r.text.clone()));
                    }
                }
            }
        }
        println!("{raw:?} -> {:?}", default_run(raw, &words, &snippets).text);
    }
    assert!(perdidos.is_empty(), "emojis perdidos: {perdidos:?}");
    // Snippets que se expanden a otros disparadores: una sola pasada, sin bucle.
    assert_eq!(text::apply_snippets("uno dos tres", &snippets), "dos uno tres tres");
    // Diccionario con «regex»: literal, sin expandir `$1`.
    assert_eq!(text::apply_dictionary("a.b axb", &words), "X$1 axb");
}

/// 1 MB de texto por la tubería entera (debug): ni pánico ni cuelgue.
#[test]
fn rompedor_dictado_un_mega_por_la_tuberia() {
    let words: Vec<Word> = (0..50).map(|i| word(&format!("Palabra{i}"), None, &[&format!("palabra {i}")])).chain([word("Txonso", None, &["Cholso"]), word("Ollama", None, &["Oyama"])]).collect();
    let snippets: Vec<Snippet> = (0..20).map(|i| snip(&format!("atajo {i}"), &format!("expansión {i}"))).collect();
    let frase = "Bueno, pues eh hoy hablé con Cholso de que que el proyecto, no, mejor dicho, del atajo 3. ";
    let raw = frase.repeat(1_048_576 / frase.len() + 1);
    for lvl in [CleanupLevel::None, CleanupLevel::Medium, CleanupLevel::High] {
        let t0 = Instant::now();
        let r = run(&raw, lvl, Tone::Excited, &words, &snippets, true);
        println!("1 MB {lvl:?}: {} ms, salida {} bytes, arreglos dicc {} snip {} mulet {}", t0.elapsed().as_millis(), r.text.len(), r.fixes_dictionary, r.fixes_snippets, r.fixes_fillers);
        assert!(!r.text.is_empty());
    }
}

/// Diccionario grande sobre un dictado normal: lo que cuesta la tubería (regla 3: «resto de la
/// tubería ~0,3 ms»). Perfil test (debug): solo orientativo.
#[test]
fn rompedor_dictado_diccionario_grande_coste() {
    let raw = "Hola Chonzo, mañana hablamos con el equipo de Beltranab sobre Perakit y lo instalé con Oyama.";
    for n in [0usize, 50, 200, 1000] {
        let words: Vec<Word> = (0..n).map(|i| word(&format!("Marca{i}x"), if i % 2 == 0 { Some("MARCA") } else { None }, &[&format!("marca {i} equis")])).collect();
        let mut ts = Vec::new();
        for _ in 0..5 {
            let t0 = Instant::now();
            let _ = default_run(raw, &words, &[]);
            ts.push(t0.elapsed().as_secs_f64() * 1000.0);
        }
        ts.sort_by(f64::total_cmp);
        println!("diccionario de {n} palabras: p50 {:.1} ms, max {:.1} ms", ts[2], ts[4]);
    }
}

/// Mayúsculas turcas y cambios de longitud al pasar a mayúsculas/minúsculas: sin pánico en
/// `capitalize_first`/`lowercase_unprotected` y el retroceso con índices de bytes.
#[test]
fn rompedor_dictado_casos_de_longitud_variable() {
    for raw in ["ßtraße. borra eso. ﬁn", "ŉ no, mejor ǆ", "İ quiero decir ı", "ﬀ, ﬀ, ﬀ", "ΐ ΐ ΐ.", "a\u{301}\u{301}\u{301} quiero decir"] {
        for lvl in LEVELS {
            for tone in TONES {
                let _ = run(raw, lvl, tone, &[word("İ", None, &[])], &[snip("ı", "i sin punto")], true);
            }
        }
    }
}

// ------------------------------------------------------------ huella de voz

/// `text_words` + `decide` con tokens de varios bytes (emoji, CJK, combinantes) y aciertos fabricados
/// en todos los tramos: los rangos caen en límites de carácter y no hay pánico.
#[test]
fn rompedor_dictado_huella_rangos_unicode() {
    use crate::asr::TimedToken;
    use crate::voiceprint::{decide, text_words, BankWord, Hit, Template};
    let tok = |t: &str, s: f32, e: f32| TimedToken { text: t.into(), start: s, end: e };
    let tokens = vec![
        tok("Ho", 0.0, 0.2),
        tok("la", 0.2, 0.4),
        tok(" 👨\u{200D}👩\u{200D}👧", 0.4, 0.6),
        tok(" Jose\u{301}", 0.6, 0.9),
        tok(",", 0.9, 0.95),
        tok(" 東京", 0.95, 1.3),
        tok(" Txon", 1.3, 1.5),
        tok("so.", 1.5, 1.7),
        tok(" ", 1.7, 1.8),
        tok("", 1.8, 1.8),
    ];
    let (text, words) = text_words(&tokens);
    println!("{text:?} -> {:?}", words.iter().map(|w| &text[w.range.clone()]).collect::<Vec<_>>());
    for w in &words {
        assert!(text.is_char_boundary(w.range.start) && text.is_char_boundary(w.range.end));
    }
    // Plantilla real a partir de un tono con envolvente (0,5 s).
    let audio: Vec<f32> = (0..8000).map(|i| (i as f32 * 0.07).sin() * 0.3 * (std::f32::consts::PI * i as f32 / 8000.0).sin()).collect();
    let Some(t) = Template::from_audio(&audio) else {
        println!("Template::from_audio no da plantilla con el tono sintético: decide no se prueba");
        return;
    };
    let t = std::sync::Arc::new(t);
    let bank = vec![
        BankWord { write: "Txonso".into(), replaced: false, forms: vec!["txonso".into()], templates: vec![t.clone()] },
        BankWord { write: "東京".into(), replaced: false, forms: vec!["東京".into()], templates: vec![t.clone(), t.clone()] },
    ];
    let mut hits = Vec::new();
    for s in 0..180 {
        for e in (s + 1..=185).step_by(7) {
            hits.push(Hit { word: s % 2, cost: (s as f32 * 0.37) % 7.0, start: s, end: e });
        }
    }
    let chosen = decide(&bank, &hits, &text, &words, &[6.3, 6.4, 6.4]);
    println!("{} aciertos elegidos de {}", chosen.len(), hits.len());
}

// ------------------------------------------------------------ historial

/// Búsqueda con `%`, `_` y `\`: literales (lo que aguanta).
#[test]
fn rompedor_dictado_buscar_comodines_literales() {
    let db = Db::open_memory().unwrap();
    for t in ["100% seguro", "a_b", r"C:\ruta\fichero", "abc", "100 por cien", "axb"] {
        db.add_history(&hist(t, None)).unwrap();
    }
    let q = |s: &str| -> Vec<String> { db.list_history(Some(s), 50, 0).unwrap().into_iter().map(|h| h.text).collect() };
    assert_eq!(q("%"), vec!["100% seguro"]);
    assert_eq!(q("_"), vec!["a_b"]);
    assert_eq!(q("\\"), vec![r"C:\ruta\fichero"]);
    assert_eq!(q("\\r"), vec![r"C:\ruta\fichero"]);
    assert_eq!(q("%_\\"), Vec::<String>::new());
    assert_eq!(q("a_b"), vec!["a_b"]);
    // Borrar ids raros no falla ni toca otras filas.
    for id in [0, -1, i64::MIN, i64::MAX] {
        db.delete_history(id).unwrap();
    }
    assert_eq!(db.list_history(None, 50, 0).unwrap().len(), 6);
}

/// Historial con 1 M de filas (solo con `PRONTO_ROMPEDOR_DICTADO_1M=1`). Base en carpeta temporal.
#[test]
fn rompedor_dictado_historial_un_millon() {
    if std::env::var("PRONTO_ROMPEDOR_DICTADO_1M").is_err() {
        return;
    }
    let tmp = Tmp::new("1m");
    let path = tmp.0.join("p.db");
    let db = Db::open_at(&path).unwrap();
    {
        let mut c = rusqlite::Connection::open(&path).unwrap();
        let tx = c.transaction().unwrap();
        {
            let mut st = tx.prepare("INSERT INTO history (created_at, text, raw_text, app, duration_ms, words) VALUES (?1, ?2, ?2, 'Notepad', 3000, 8)").unwrap();
            for i in 0..1_000_000i64 {
                let t = format!("dictado número {i} con 50% de texto_normal y algo más");
                st.execute(rusqlite::params![format!("2026-10-{:02}T10:00:00+02:00", 1 + i % 20), t]).unwrap();
            }
        }
        tx.commit().unwrap();
    }
    let medir = |nombre: &str, f: &dyn Fn() -> usize| {
        let mut ts = Vec::new();
        let mut n = 0;
        for _ in 0..5 {
            let t0 = Instant::now();
            n = f();
            ts.push(t0.elapsed().as_secs_f64() * 1000.0);
        }
        ts.sort_by(f64::total_cmp);
        println!("{nombre}: p50 {:.1} ms, max {:.1} ms ({n} filas)", ts[2], ts[4]);
    };
    medir("primera página", &|| db.list_history(None, 50, 0).unwrap().len());
    medir("página 20 000 (offset 999 950)", &|| db.list_history(None, 50, 999_950).unwrap().len());
    medir("buscar «%»", &|| db.list_history(Some("%"), 50, 0).unwrap().len());
    medir("buscar «_» sin resultado al final («zz_»)", &|| db.list_history(Some("zz_"), 50, 0).unwrap().len());
    medir("buscar «\\»", &|| db.list_history(Some("\\"), 50, 0).unwrap().len());
    medir("último (pegar último)", &|| db.last_history_text().map_or(0, |_| 1));
    medir("añadir 1", &|| db.add_history(&hist("uno más", None)).map(|_| 1).unwrap());
    medir("borrar 1", &|| db.delete_history(500_000).map(|_| 1).unwrap());
    drop(db);
}

// ------------------------------------------------------------ portapapeles (funciones puras)

/// `save_formats`/`restore_formats`/`copyable_format`/`text_formats` son privadas de `inject.rs`. La
/// sonda copia su fuente TAL CUAL (de «// Formatos estándar» a «/// Formatos registrados») a un
/// programa aparte en una carpeta temporal y lo compila con `rustc`: se ejecuta el código real, sin
/// tocar el portapapeles.
#[test]
fn rompedor_dictado_portapapeles_formatos() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/inject.rs")).unwrap();
    let a = src.find("// Formatos estándar").expect("inicio");
    let b = src.find("/// Formatos registrados").expect("fin");
    let harness = r#"
fn main() {
    // Duplicados, privados, de handle y de 0 bytes.
    let avail = [13u32, 13, 1, 7, 2, 0x200, 0x2FF, 0x300, 0x3FF, 0x400, 15, 0xC0F1, 0xC0F1, 0xFFFF, 0];
    let mut lecturas = 0;
    let clip = save_formats(avail, |f| { lecturas += 1; Some(if f == 0xC0F1 { Vec::new() } else { vec![f as u8; 3] }) });
    println!("guardados={:?} lecturas={}", clip.iter().map(|(f, b)| (*f, b.len())).collect::<Vec<_>>(), lecturas);
    let fmts: Vec<u32> = clip.iter().map(|(f, _)| *f).collect();
    assert_eq!(fmts, vec![13, 1, 7, 0x400, 15, 0xC0F1, 0xFFFF, 0]);
    // Un formato enorme (200 MB) entra y sale igual.
    let big = vec![(8u32, vec![7u8; 200 << 20])];
    let mut escritos = 0usize;
    let fallidos = restore_formats(&big, |_, b| { escritos += b.len(); true });
    assert!(fallidos.is_empty() && escritos == 200 << 20);
    // Restaurar con fallos: devuelve exactamente los que fallan, también repetidos.
    let saved = vec![(13u32, vec![1]), (1, vec![]), (13, vec![2])];
    let f = restore_formats(&saved, |f, _| f != 13);
    assert_eq!(f, vec![13, 13]);
    // Texto: UTF-16 con su NUL final. ¿Un NUL dentro corta el texto?
    let t = "hola\0mundo";
    let c = text_formats(t, &[0xC1D3]);
    let u16s: Vec<u16> = c[0].1.chunks(2).map(|p| u16::from_le_bytes([p[0], p[1]])).collect();
    let hasta_nul = String::from_utf16_lossy(&u16s[..u16s.iter().position(|&x| x == 0).unwrap()]);
    println!("texto {:?} -> lo que lee una app (hasta el primer NUL) {:?}", t, hasta_nul);
    // Emojis con ZWJ y 1 MB: ida y vuelta exacta.
    let e = "👨\u{200D}👩\u{200D}👧 ".repeat(100_000);
    let c = text_formats(&e, &[]);
    let u: Vec<u16> = c[0].1.chunks(2).map(|p| u16::from_le_bytes([p[0], p[1]])).collect();
    assert_eq!(String::from_utf16(&u[..u.len() - 1]).unwrap(), e);
    assert_eq!(*u.last().unwrap(), 0);
    // El NUL no se puede pegar (CF_UNICODETEXT acaba en él): lo correcto es que se quite y llegue
    // todo lo demás, no que llegue «hola\0mundo» tal cual.
    if hasta_nul != t.replace('\0', "") { std::process::exit(3); }
}
"#;
    let tmp = Tmp::new("clip");
    let file = tmp.0.join("clip.rs");
    std::fs::write(&file, format!("#![allow(dead_code)]\n{}\n{harness}", &src[a..b])).unwrap();
    let exe = tmp.0.join("clip.exe");
    let out = std::process::Command::new("rustc").args(["--edition", "2021", "-O", "-o"]).arg(&exe).arg(&file).output().unwrap();
    assert!(out.status.success(), "no compila el extracto: {}", String::from_utf8_lossy(&out.stderr));
    let run = std::process::Command::new(&exe).output().unwrap();
    println!("{}{}", String::from_utf8_lossy(&run.stdout), String::from_utf8_lossy(&run.stderr));
    assert_eq!(run.status.code(), Some(0), "el texto con NUL se pega cortado (salida 3) o una aserción falló");
}

// ============================================================ segunda pasada (2026-10-23)
// Ataca los arreglos de 68707a4 y busca roturas nuevas. Mismas reglas: bases en memoria o en
// carpetas temporales propias; el motor real solo con `PRONTO_ROMPEDOR_MODELO` y WAV propios.

/// Palabras de `raw` que faltan en `out` (comparación sin tildes ni mayúsculas, por palabra).
fn perdidas(raw: &str, out: &str) -> Vec<String> {
    let toks = |s: &str| s.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(text::fold).collect::<Vec<_>>();
    let mut quedan = toks(out);
    let mut faltan = Vec::new();
    for w in toks(raw) {
        match quedan.iter().position(|x| *x == w) {
            Some(i) => {
                quedan.remove(i);
            }
            None => faltan.push(w),
        }
    }
    faltan
}

/// ALTA. FR-02 R4 tras 68707a4: la lista de contexto (`BACKTRACK_CONTEXT_BEFORE`, text.rs:301) solo
/// cubre unas pocas palabras; «quiero decir» usado como verbo detrás de cualquier otra («hoy»,
/// «primero», «también», «simplemente», «empezar»), al empezar frase con otra antes, «no quiero
/// decir» + cualquier palabra fuera de `BACKTRACK_NEGATION_AFTER`, «mejor dicho» tras un verbo y
/// «borra eso»/«scratch that»/«no wait» en frases normales siguen borrando lo dictado.
#[test]
// Arreglado (ALTA 1): solo cuenta una corrección aislada por pausas o del mismo tipo que lo que reemplaza.
fn rompedor_dictado_b_retroceso_se_come_frases_normales() {
    let frases = [
        "Hoy quiero decir algo importante.",
        "Antes de empezar quiero decir que estoy muy contento.",
        "Primero quiero decir gracias a todos.",
        "Yo también quiero decir una cosa.",
        "Simplemente quiero decir gracias.",
        "Os cuento una cosa. Quiero decir que mañana no vengo.",
        "Gracias por venir. Quiero decir unas palabras.",
        "Mira, no quiero decir adiós todavía.",
        "Yo no quiero decir su nombre.",
        "Esto suena mejor dicho en voz alta.",
        "Mi hijo siempre borra eso de la pizarra.",
        "There is no wait at the restaurant.",
        "My cat likes to scratch that old sofa.",
    ];
    let mut rotas = Vec::new();
    for f in frases {
        for lvl in [CleanupLevel::Medium, CleanupLevel::High] {
            let r = run(f, lvl, Tone::Casual, &[], &[], false);
            let p = perdidas(f, &r.text);
            if lvl == CleanupLevel::Medium {
                println!("«{f}» -> «{}»  (perdidas: {p:?})", r.text);
            }
            if !p.is_empty() {
                rotas.push(f);
            }
        }
    }
    rotas.dedup();
    assert!(rotas.is_empty(), "{} de {} frases normales pierden palabras: {rotas:?}", rotas.len(), frases.len());
}

/// MEDIA. FR-02 R4: «no, mejor dicho» (dos disparadores seguidos, muy común) deja «Dicho,» pegado:
/// la regex casa primero «no, mejor» (text.rs:291) y lo que queda empieza por «dicho».
#[test]
// Arreglado (MEDIA 2): «no, mejor dicho» es una alternativa propia de la regex.
#[ignore = "retroceso quitado por Txonso el 23-oct-2026 (apply_backtrack fuera de la tubería)"]
fn rompedor_dictado_b_no_mejor_dicho_deja_dicho() {
    let casos = [
        ("Quedamos el lunes, no, mejor dicho, el martes.", "El martes"),
        ("Es para mañana no, mejor dicho, para el viernes.", "Para el viernes"),
        ("Llámame a las cinco. No, mejor dicho, a las seis.", "A las seis"),
    ];
    let mut mal = Vec::new();
    for (f, debe) in casos {
        let r = default_run(f, &[], &[]);
        println!("«{f}» -> «{}»", r.text);
        if r.text != debe {
            mal.push((f, r.text));
        }
    }
    assert!(mal.is_empty(), "{mal:?}");
}

/// BAJA (BAJA 10 de la primera pasada, sin arreglar): el tope de 16 retrocesos aplicados sigue
/// (text.rs:355). Un dictado largo en manos libres (hasta 20 min) con más de 16 correcciones deja
/// las restantes tal cual, con «quiero decir» y la frase equivocada.
#[test]
// Arreglado (BAJA 8): el tope ya no es 16 fijo, depende de los disparadores del texto.
#[ignore = "retroceso quitado por Txonso el 23-oct-2026 (apply_backtrack fuera de la tubería)"]
fn rompedor_dictado_b_tope_16_retrocesos() {
    let largo = "Quedamos el lunes, quiero decir, el martes. ".repeat(20);
    let r = default_run(&largo, &[], &[]);
    let quedan = r.text.matches("quiero decir").count();
    println!("20 correcciones -> quedan {quedan} sin aplicar; final: {:?}", &r.text[r.text.len().saturating_sub(120)..]);
    assert_eq!(quedan, 0);
}

/// BAJA (regresión de 68707a4): una corrección sin pausa justo detrás de un pronombre de la lista
/// de contexto («tú», «yo», «te», «eso»…) ya no se aplica: «quiero decir» queda en el texto.
#[test]
// Arreglado (BAJA 9): sin «yo»/«tú» en la lista de contexto; «mismo tipo» (pronombre por pronombre).
#[ignore = "retroceso quitado por Txonso el 23-oct-2026 (apply_backtrack fuera de la tubería)"]
fn rompedor_dictado_b_correccion_tras_pronombre() {
    let casos = [("Cómpralo tú quiero decir yo.", "Yo"), ("Lo hago yo quiero decir tú.", "Tú"), ("Dáselo a ella y a ti quiero decir a él.", "A él")];
    let mut mal = Vec::new();
    for (f, debe) in casos {
        let r = default_run(f, &[], &[]);
        println!("«{f}» -> «{}»", r.text);
        if r.text.contains("quiero decir") || r.text != debe {
            mal.push((f, r.text));
        }
    }
    assert!(mal.is_empty(), "{mal:?}");
}

/// BAJA. «¿Juan borra eso?» (una pregunta) se toma por la orden y se pega «?»: no queda vacío, así
/// que tampoco va por el camino de «Se borró todo el dictado» (state.rs:704).
#[test]
// Arreglado (BAJA 10): una orden de borrar nunca cuenta dentro de una pregunta ni con sujeto delante.
fn rompedor_dictado_b_pregunta_borra_eso() {
    for f in ["¿Juan borra eso?", "¿Borra eso?"] {
        let r = default_run(f, &[], &[]);
        println!("«{f}» -> «{}»", r.text);
        assert!(r.text.chars().any(char::is_alphanumeric) || r.text.is_empty(), "se pega solo puntuación: {:?}", r.text);
    }
}

/// BAJA (el arreglo de BAJA 5 se pasa): un «No.» como respuesta en la frase anterior cuenta como
/// negación de la orden (text.rs:263-266 mira la palabra anterior sin mirar si hay fin de frase
/// entre medias): no pulsa Enter y pega «Pulsa enter.» literal.
#[test]
// Arreglado (BAJA 11): la negación tiene que ir en la misma frase que la orden.
fn rompedor_dictado_b_no_punto_pulsa_enter() {
    for f in ["¿Lo mando así? No. Pulsa enter.", "Is it ready? No. Press enter."] {
        let r = run(f, CleanupLevel::Medium, Tone::Casual, &[], &[], true);
        println!("{f:?} -> {:?} press_enter={}", r.text, r.press_enter);
        assert!(r.press_enter && !text::fold(&r.text).contains("enter"), "{f:?} -> {:?} press_enter={}", r.text, r.press_enter);
    }
}

/// MEDIA. FR-02 R5: una palabra del diccionario sin reemplazo «ajusta mayúsculas» en cualquier
/// aparición si no está en `COMMON_WORDS` (text.rs:604-611 `case_rule_ok` usa `is_common_phrase`, no
/// el léxico de `is_real_word` que 68707a4 añadió solo a «suena igual»). Nombres de producto que
/// son palabras corrientes ponen mayúscula a la palabra corriente: «Pronto» (el nombre de la app),
/// «Llama» (el modelo de Meta), «Cursor», «Claqueta», «Luna», «Mistral».
#[test]
// Arreglado (MEDIA 3): `case_rule_ok` no crea la regla para palabras del léxico (sí para nombres de pila).
fn rompedor_dictado_b_mayusculas_pisan_palabras_corrientes() {
    let casos = [
        ("Pronto", "Llegaré pronto a casa, hasta pronto."),
        ("Llama", "Mi madre me llama todos los días."),
        ("Cursor", "Mueve el cursor a la derecha."),
        ("Claqueta", "Coge la claqueta y empieza."),
        ("Luna", "Esta noche hay luna llena."),
        ("Mistral", "Hoy sopla el mistral."),
    ];
    let mut mal = Vec::new();
    for (w, f) in casos {
        let r = default_run(f, &[word(w, None, &[])], &[]);
        println!("dicc [{w}]: «{f}» -> «{}» (arreglos {})", r.text, r.fixes_dictionary);
        if r.text.contains(w) {
            mal.push((w, r.text));
        }
    }
    assert!(mal.is_empty(), "{} de {}: {mal:?}", mal.len(), casos.len());
}

/// MEDIA. FR-02 R6 tras 68707a4: el léxico embebido (`lexico_es.txt`, ~1000 formas) no cubre el
/// español corriente: «sana», «lisa», «riel», «cana» (y el inglés, «Sarah») siguen cambiándose
/// por una palabra del diccionario (`near_key` text.rs:914 o clave exacta).
#[test]
// Arreglado (MEDIA 4): `near_key` solo con «y»/«ll» tras la vocal y léxico ampliado (nombres en inglés).
fn rompedor_dictado_b_suena_igual_fuera_del_lexico() {
    let casos = [
        ("Asana", "Mi abuela ya está sana.", "sana"),
        ("Elisa", "La mesa es lisa.", "lisa"),
        ("Ariel", "El tren salió del riel.", "riel"),
        ("Ocaña", "Le salió una cana.", "cana"),
        ("Zara", "I met Sarah yesterday.", "Sarah"),
    ];
    let mut mal = Vec::new();
    for (w, f, debe) in casos {
        let r = default_run(f, &[word(w, None, &[])], &[]);
        println!("dicc [{w}]: «{f}» -> «{}» (arreglos {})", r.text, r.fixes_dictionary);
        if !r.text.split(|c: char| !c.is_alphanumeric()).any(|x| x == debe) {
            mal.push((w, r.text));
        }
    }
    assert!(mal.is_empty(), "{} de {}: {mal:?}", mal.len(), casos.len());
}

/// BAJA (BAJA 9 de la primera pasada, arreglada a medias): la caché de 68707a4 solo sirve en
/// caliente. El primer dictado tras arrancar y el primero tras CADA cambio del diccionario (grabar
/// una pronunciación añade una variante) recompilan todas las regex en el camino del pegado.
#[test]
// Arreglado (BAJA 12): caché de regex por patrón; tras un cambio solo se compila lo nuevo. El frío tras arrancar sigue.
fn rompedor_dictado_b_diccionario_en_frio() {
    let raw = "Hola Chonzo, mañana hablamos con el equipo de Beltranab sobre Perakit y lo instalé con Oyama.";
    let mut words: Vec<Word> = (0..1000).map(|i| word(&format!("Marca{i}x"), if i % 2 == 0 { Some("MARCA") } else { None }, &[&format!("marca {i} equis")])).collect();
    let medir = |w: &[Word]| {
        let t0 = Instant::now();
        let _ = default_run(raw, w, &[]);
        t0.elapsed().as_secs_f64() * 1000.0
    };
    let frio = medir(&words);
    let caliente = medir(&words);
    words[500].variants.push(WordVariant { id: 99, heard: "marca nueva".into() });
    let tras_cambio = medir(&words);
    println!("1000 palabras (debug): frío {frio:.1} ms, caliente {caliente:.1} ms, tras grabar una variante {tras_cambio:.1} ms");
    assert!(tras_cambio < caliente * 5.0, "tras un cambio del diccionario el dictado paga la compilación entera: {tras_cambio:.1} ms frente a {caliente:.1} ms");
}

/// BAJA. FR-02 R7: el disparador de un snippet no se compara sin tildes: «mi direccion» (escrito
/// sin tilde en el hub) no se expande nunca, porque Parakeet escribe «dirección».
#[test]
#[ignore = "rompedor: un disparador sin tilde no casa con el dictado"]
fn rompedor_dictado_b_snippet_sin_tilde() {
    let sn = [snip("mi direccion", "Calle Mayor 1, Madrid")];
    let solo = default_run("Mi dirección.", &[], &sn);
    let dentro = default_run("Te paso mi dirección ahora.", &[], &sn);
    println!("solo: {:?} · dentro: {:?}", solo.text, dentro.text);
    assert_eq!(solo.text, "Calle Mayor 1, Madrid");
}

/// BAJA. Búsqueda del historial (68707a4): `text::fold` no normaliza Unicode; un texto con tildes
/// combinantes (NFD) no se encuentra buscando con la letra compuesta, ni al revés.
#[test]
#[ignore = "rompedor: la búsqueda no encuentra textos con tildes combinantes"]
fn rompedor_dictado_b_buscar_combinantes() {
    let db = Db::open_memory().unwrap();
    for t in ["O\u{301}scar llega tarde", "Begon\u{303}a NFD", "Óscar en NFC"] {
        db.add_history(&hist(t, None)).unwrap();
    }
    let mut mal = Vec::new();
    for (q, debe) in [("óscar", "O\u{301}scar llega tarde"), ("oscar", "O\u{301}scar llega tarde"), ("begoña", "Begon\u{303}a NFD"), ("O\u{301}scar", "Óscar en NFC")] {
        let got: Vec<String> = db.list_history(Some(q), 50, 0).unwrap().into_iter().map(|h| h.text).collect();
        println!("buscar {q:?} -> {got:?}");
        if !got.iter().any(|t| t == debe) {
            mal.push(q);
        }
    }
    assert!(mal.is_empty(), "{mal:?}");
}

/// Huella de voz con el motor real (solo con `PRONTO_ROMPEDOR_MODELO` y `PRONTO_ROMPEDOR_VOZ`, una
/// carpeta con WAV 16 kHz mono generados con la voz TTS de Windows directo a fichero):
/// `w_<palabra>_N.wav` = la palabra suelta («Grabar pronunciación», 3 muestras) y `s_<palabra>.wav`
/// = una frase con una palabra corriente que suena parecido. `decide` (voiceprint.rs:595) solo
/// descarta `is_common_phrase`, no las palabras reales del léxico que sí usa «suena igual».
#[test]
// Arreglado (rompedor 23-oct, dictado MEDIA 5): `decide` descarta tramos de palabras reales y recorta las de borde.
fn rompedor_dictado_b_huella_pisa_palabras_reales() {
    use crate::voiceprint::{correct_with_bank, BankWord, Template};
    let (Ok(model), Ok(dir)) = (std::env::var("PRONTO_ROMPEDOR_MODELO"), std::env::var("PRONTO_ROMPEDOR_VOZ")) else {
        println!("OMITIDA: faltan PRONTO_ROMPEDOR_MODELO y PRONTO_ROMPEDOR_VOZ");
        return;
    };
    let dir = PathBuf::from(dir);
    let wav = |name: &str| crate::audio::decode_wav(&std::fs::read(dir.join(format!("{name}.wav"))).unwrap()).expect("WAV 16 kHz mono");
    let engine = crate::asr::Engine::load_for_tests(&model).expect("motor");
    engine.set_vocabulary(&[]);
    let mut pisadas = Vec::new();
    for (write, file, real) in [("Ollama", "ollama", "llama"), ("Asana", "asana", "sana"), ("Pronto", "pronto", "pronto")] {
        let mut templates = Vec::new();
        let mut forms = vec![text::fold(write)];
        for i in 0..3 {
            let s = wav(&format!("w_{file}_{i}"));
            let heard = engine.transcribe(&s).unwrap();
            forms.push(text::fold(&text::normalize_heard(&heard)));
            println!("{write}: muestra {i} se oye {heard:?}");
            if let Some(t) = Template::from_audio(&s) {
                templates.push(std::sync::Arc::new(t));
            }
        }
        forms.retain(|f| !f.is_empty());
        let bank = vec![BankWord { write: write.into(), replaced: false, forms, templates }];
        let audio = wav(&format!("s_{file}"));
        let timed = engine.transcribe_timed(&audio).unwrap();
        let (heard, fixes) = correct_with_bank(&bank, &audio, &timed);
        let fin = default_run(&heard, &[word(write, None, &[])], &[]).text;
        println!("{write}: crudo {:?}\n   huella {:?} cambios {:?}\n   final {:?}", timed.text, heard, fixes.iter().map(|f| (&f.from, &f.to, f.cost)).collect::<Vec<_>>(), fin);
        let antes = timed.text.split(|c: char| !c.is_alphanumeric()).filter(|w| w.eq_ignore_ascii_case(real)).count();
        let despues = fin.split(|c: char| !c.is_alphanumeric()).filter(|w| *w == real).count();
        if despues < antes {
            pisadas.push((write, timed.text.clone(), fin));
        }
    }
    assert!(pisadas.is_empty(), "palabras reales cambiadas por la huella o el ajuste de mayúsculas: {pisadas:?}");
}

/// Búsqueda del historial con 100 000 filas: `fold()` (función de Rust por fila, 68707a4) frente
/// al `LIKE` de antes, medidos a la vez sobre la misma base (así la carga de la máquina afecta a los
/// dos por igual). Base en carpeta temporal propia.
#[test]
fn rompedor_dictado_b_buscar_coste_fold() {
    let tmp = Tmp::new("fold");
    let path = tmp.0.join("p.db");
    let db = Db::open_at(&path).unwrap();
    let raw = rusqlite::Connection::open(&path).unwrap();
    {
        let mut c = rusqlite::Connection::open(&path).unwrap();
        let tx = c.transaction().unwrap();
        {
            let mut st = tx.prepare("INSERT INTO history (created_at, text, raw_text, app, duration_ms, words) VALUES (?1, ?2, ?2, 'Notepad', 3000, 8)").unwrap();
            for i in 0..100_000i64 {
                let t = format!("Dictado número {i}: mañana hablamos con Begoña del presupuesto y de la reunión del jueves, ¿vale?");
                st.execute(rusqlite::params![format!("2026-10-{:02}T10:00:00+02:00", 1 + i % 20), t]).unwrap();
            }
        }
        tx.commit().unwrap();
    }
    let old = |q: &str| -> usize {
        let like = format!("%{q}%");
        let mut st = raw.prepare("SELECT id FROM history WHERE (text LIKE ?1 ESCAPE '\\' OR app LIKE ?1 ESCAPE '\\') ORDER BY id DESC LIMIT 50").unwrap();
        st.query_map([like], |r| r.get::<_, i64>(0)).unwrap().collect::<Result<Vec<_>, _>>().unwrap().len()
    };
    let medir = |nombre: &str, f: &dyn Fn() -> usize| -> f64 {
        let mut ts = Vec::new();
        let mut n = 0;
        for _ in 0..5 {
            let t0 = Instant::now();
            n = f();
            ts.push(t0.elapsed().as_secs_f64() * 1000.0);
        }
        ts.sort_by(f64::total_cmp);
        println!("{nombre}: p50 {:.1} ms, max {:.1} ms ({n} filas)", ts[2], ts[4]);
        ts[2]
    };
    for _ in 0..2 {
        let a = medir("antes (LIKE) «zzz» sin resultado", &|| old("zzz"));
        let b = medir("ahora (fold) «zzz» sin resultado", &|| db.list_history(Some("zzz"), 50, 0).unwrap().len());
        println!("  ×{:.1}", b / a);
        let a = medir("antes (LIKE) «presupuesto»", &|| old("presupuesto"));
        let b = medir("ahora (fold) «presupuesto»", &|| db.list_history(Some("presupuesto"), 50, 0).unwrap().len());
        println!("  ×{:.1}", b / a);
    }
    drop(db);
}

/// Coste de la huella en el camino de cada dictado (regla 3), con 9 muestras y el WAV de 15 s de
/// `tests/no-se-negocia/datos`. Mismas variables que la sonda de la huella.
#[test]
fn rompedor_dictado_b_huella_coste_15s() {
    use crate::voiceprint::{correct_with_bank, BankWord, Template};
    let (Ok(model), Ok(dir)) = (std::env::var("PRONTO_ROMPEDOR_MODELO"), std::env::var("PRONTO_ROMPEDOR_VOZ")) else {
        println!("OMITIDA: faltan PRONTO_ROMPEDOR_MODELO y PRONTO_ROMPEDOR_VOZ");
        return;
    };
    let dir = PathBuf::from(dir);
    let wav = |p: PathBuf| crate::audio::decode_wav(&std::fs::read(p).unwrap()).expect("WAV");
    let engine = crate::asr::Engine::load_for_tests(&model).expect("motor");
    engine.set_vocabulary(&[]);
    let audio = wav(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/no-se-negocia/datos/dictado-15s.wav")));
    let _ = engine.transcribe_timed(&audio).unwrap();
    let timed = engine.transcribe_timed(&audio).unwrap();
    let bank: Vec<BankWord> = ["ollama", "asana", "pronto"]
        .iter()
        .map(|f| BankWord {
            write: f.to_string(),
            replaced: false,
            forms: vec![f.to_string()],
            templates: (0..3).filter_map(|i| Template::from_audio(&wav(dir.join(format!("w_{f}_{i}.wav"))))).map(std::sync::Arc::new).collect(),
        })
        .collect();
    for _ in 0..2 {
        let mut ts = Vec::new();
        for _ in 0..5 {
            let t0 = Instant::now();
            let _ = correct_with_bank(&bank, &audio, &timed);
            ts.push(t0.elapsed().as_secs_f64() * 1000.0);
        }
        ts.sort_by(f64::total_cmp);
        println!("huella 9 muestras, 15 s (debug): p50 {:.1} ms, max {:.1} ms", ts[2], ts[4]);
    }
}

/// Regla 3 + regla 5: el dictado guarda en el historial ANTES de pegar (`finish_dictation`) y lee el
/// diccionario y los snippets al empezar, todo por el mismo `Mutex<Connection>` que la búsqueda del
/// hub. Con 100 000 filas, una búsqueda sin resultados con `fold()` tiene la conexión ocupada todo
/// su recorrido: el dictado espera para pegar. Base en carpeta temporal propia.
#[test]
fn rompedor_dictado_b_busqueda_frena_el_dictado() {
    let tmp = Tmp::new("fold-dictado");
    let path = tmp.0.join("p.db");
    let db = std::sync::Arc::new(Db::open_at(&path).unwrap());
    {
        let mut c = rusqlite::Connection::open(&path).unwrap();
        let tx = c.transaction().unwrap();
        {
            let mut st = tx.prepare("INSERT INTO history (created_at, text, raw_text, app, duration_ms, words) VALUES (?1, ?2, ?2, 'Notepad', 3000, 8)").unwrap();
            for i in 0..100_000i64 {
                let t = format!("Dictado número {i}: mañana hablamos con Begoña del presupuesto y de la reunión del jueves, ¿vale?");
                st.execute(rusqlite::params![format!("2026-10-{:02}T10:00:00+02:00", 1 + i % 20), t]).unwrap();
            }
        }
        tx.commit().unwrap();
    }
    // Lo que hace un dictado contra la base: diccionario + snippets al empezar, historial antes de pegar.
    let dictado = |db: &Db| {
        let t0 = Instant::now();
        let _ = db.list_words().unwrap();
        let _ = db.list_snippets().unwrap();
        db.add_history(&hist("Texto dictado", None)).unwrap();
        t0.elapsed().as_secs_f64() * 1000.0
    };
    let mut solo: Vec<f64> = (0..10).map(|_| dictado(&db)).collect();
    solo.sort_by(f64::total_cmp);
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (db2, stop2) = (db.clone(), stop.clone());
    // El usuario busca en el hub: una consulta tras otra (debounce de 250 ms, home.js:147).
    let buscador = std::thread::spawn(move || {
        let mut n = 0;
        for q in ["presupuesto de marzo", "factura", "zzz", "óscar", "contrato", "llamada"].iter().cycle() {
            if stop2.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            let _ = db2.list_history(Some(q), 40, 0).unwrap();
            n += 1;
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        n
    });
    std::thread::sleep(std::time::Duration::from_millis(300));
    let mut con: Vec<f64> = Vec::new();
    for i in 0..10 {
        std::thread::sleep(std::time::Duration::from_millis(137 * (i % 3 + 1)));
        con.push(dictado(&db));
    }
    stop.store(true, std::sync::atomic::Ordering::SeqCst);
    let busquedas = buscador.join().unwrap();
    con.sort_by(f64::total_cmp);
    println!("dictado sin buscar: p50 {:.1} ms, max {:.1} ms", solo[5], solo[9]);
    println!("dictado mientras se busca ({busquedas} búsquedas): p50 {:.1} ms, p90 {:.1} ms, max {:.1} ms", con[5], con[8], con[9]);
    assert!(con[8] < 50.0, "el dictado espera a la búsqueda para guardar y pegar: p90 {:.1} ms", con[8]);
}

/// Regla 5: `transcribe_and_paste` (state.rs:715-717) descarta sin fila ni audio un dictado cuya
/// transcripción sale vacía («No se ha oído nada»), aunque el audio pase el umbral de micro mudo
/// (`SILENCE_PEAK` = 0,0005, state.rs:41). Con el motor real: el WAV de 15 s a menos volumen (voz
/// floja o micro lejos), ¿desde qué pico Parakeet ya no devuelve nada?
#[test]
fn rompedor_dictado_b_voz_floja_sale_vacia() {
    let Ok(model) = std::env::var("PRONTO_ROMPEDOR_MODELO") else {
        println!("OMITIDA: falta PRONTO_ROMPEDOR_MODELO");
        return;
    };
    let audio = crate::audio::decode_wav(&std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/no-se-negocia/datos/dictado-15s.wav")).unwrap()).unwrap();
    let engine = crate::asr::Engine::load_for_tests(&model).expect("motor");
    engine.set_vocabulary(&[]);
    let pico0 = audio.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    for k in [1.0f32, 0.1, 0.03, 0.01, 0.005, 0.002, 0.001] {
        let a: Vec<f32> = audio.iter().map(|s| s * k).collect();
        let raw = engine.transcribe(&a).unwrap();
        let fin = default_run(&raw, &[], &[]).text;
        println!("   crudo: {raw:?}");
        println!("×{k}: pico {:.4} (umbral mudo 0.0005) -> {} palabras: {:?}", pico0 * k, fin.split_whitespace().count(), &fin[..fin.len().min(70)]);
    }
}

/// Regla 3: la tubería de texto sobre el dictado de 15 s (lo que oye Parakeet del WAV de
/// `tests/no-se-negocia/datos`), con un diccionario realista (20 palabras con variantes) y 10
/// snippets, en caliente. Perfil test (debug): orientativo.
#[test]
fn rompedor_dictado_b_tuberia_15s_coste() {
    let raw = "Hola equipo. La reunión del jueves se mueve al viernes a las 10 de la mañana. Por favor, revisad el presupuesto del proyecto antes de entonces y traid las dudas apuntadas. Si alguien no puede venir, que me avise hoy.";
    let words: Vec<Word> = ["Txonso", "Ollama", "BeltranLabs", "Parakeet", "Tauri", "Claude", "Wispr", "Begoña", "Kubernetes", "Vercel", "Figma", "Notion", "Supabase", "Stripe", "Hacienda", "Movistar", "Bizum", "Cabify", "Glovo", "Idealista"]
        .iter()
        .enumerate()
        .map(|(i, w)| word(w, None, &[&format!("variante {i}")]))
        .collect();
    let snippets: Vec<Snippet> = (0..10).map(|i| snip(&format!("atajo número {i}"), &format!("Expansión {i}"))).collect();
    for (nombre, w, s) in [("sin diccionario", &[][..], &[][..]), ("20 palabras", &words[..], &[][..]), ("10 snippets", &[][..], &snippets[..]), ("20 palabras + 10 snippets", &words[..], &snippets[..])] {
        let _ = default_run(raw, w, s);
        let mut ts: Vec<f64> = (0..21)
            .map(|_| {
                let t0 = Instant::now();
                let _ = default_run(raw, w, s);
                t0.elapsed().as_secs_f64() * 1000.0
            })
            .collect();
        ts.sort_by(f64::total_cmp);
        println!("tubería 15 s, {nombre} (debug): p50 {:.2} ms, p95 {:.2} ms", ts[10], ts[19]);
    }
}
