//! Tubería de texto: limpieza -> formatos (correos, teléfonos, horas…) -> diccionario -> snippets -> estilo.
//!
//! Todo es determinista y local. Cada paso es una función pública para poder probarlo suelto;
//! `process` los encadena en el orden del contrato.

use crate::models::{CleanupLevel, Snippet, Tone, Word};
use regex::{Regex, RegexBuilder};

/// Categoría de app para elegir el estilo (Mensajes personales, trabajo, correo, otros).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Personal,
    Work,
    Email,
    Other,
}

/// Todo lo que la tubería necesita, ya extraído de ajustes y base de datos.
pub struct Options<'a> {
    pub cleanup: CleanupLevel,
    pub press_enter: bool,
    pub tone: Tone,
    pub words: &'a [Word],
    pub snippets: &'a [Snippet],
}

/// Resultado de la tubería, con el recuento de arreglos para las estadísticas.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Processed {
    pub text: String,
    /// El dictado terminaba con «pulsa enter»: hay que pulsar Enter tras pegar.
    pub press_enter: bool,
    pub fixes_dictionary: usize,
    pub fixes_snippets: usize,
    pub fixes_fillers: usize,
}

// Marcadores privados para proteger reemplazos del diccionario y expansiones de snippets:
// el estilo (p. ej. «muy casual» pasa todo a minúsculas) no debe tocar lo que el usuario escribió.
const PH_OPEN: char = '\u{E000}';
const PH_CLOSE: char = '\u{E001}';

/// Los caracteres que usan los marcadores (`PH_OPEN`, `PH_CLOSE` y las cifras `U+E100..U+E109`).
fn is_marker_char(c: char) -> bool {
    c == PH_OPEN || c == PH_CLOSE || ('\u{E100}'..='\u{E109}').contains(&c)
}

/// Un carácter de marcador que ya venía en el texto del usuario se guarda como texto protegido
/// (y vuelve tal cual al final): nunca se toma por un marcador abierto que se coma lo que sigue
/// (rompedor 22-oct-c, BAJA 7).
fn escape_markers(text: &str, protected: &mut Vec<String>) -> String {
    if !text.chars().any(is_marker_char) {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if is_marker_char(c) {
            protected.push(c.to_string());
            out.push_str(&placeholder(protected.len() - 1));
        } else {
            out.push(c);
        }
    }
    out
}

pub fn process(raw: &str, o: &Options) -> Processed {
    let mut out = Processed::default();
    let (mut text, fillers) = clean_counted(raw, o.cleanup);
    out.fixes_fillers = fillers;
    // El retroceso («quiero decir», «mejor dicho», «borra eso»…) ya no se aplica: borraba la frase
    // entera hasta el punto anterior y podía confundir al usuario. Txonso lo quitó el 23-oct-2026
    // («si se equivocan, que lo arreglen»). `apply_backtrack` se conserva por si vuelve.
    if o.press_enter {
        let (t, pressed) = strip_press_enter(&text);
        text = t;
        out.press_enter = pressed;
    }
    // Correos, teléfonos, horas, euros, % y símbolos dictados → escritos (BELTR-PRT-15). Antes del
    // diccionario (que no toca lo pegado a un enlace o un correo) y antes del pulido (regla 7).
    text = format_spoken(&text);
    text = fix_domains(&text, o.words);
    let mut protected: Vec<String> = Vec::new();
    text = escape_markers(&text, &mut protected);
    text = dictionary_inner(&text, o.words, &mut protected, &mut out.fixes_dictionary);
    let before = protected.len();
    text = snippets_inner(&text, o.snippets, &mut protected, &mut out.fixes_snippets);
    // Un snippet que es la frase entera sale tal cual: sin punto ni «!» del estilo (FR-02 R7).
    if !(protected.len() == before + 1 && text == placeholder(before)) {
        text = apply_style(&text, o.tone);
    }
    out.text = restore_placeholders(&text, &protected);
    out
}

// ---------------------------------------------------------------- 1. limpieza

/// Limpieza según el nivel: `None` deja el texto tal cual; `Light` quita muletillas;
/// `Medium` añade palabras repetidas; `High` añade arranques en falso al inicio.
/// (El retroceso del nivel Medium se aplica aparte, en `process`.)
#[allow(dead_code)]
pub fn clean(text: &str, level: CleanupLevel) -> String {
    clean_counted(text, level).0
}

fn clean_counted(text: &str, level: CleanupLevel) -> (String, usize) {
    let mut t = text.to_string();
    let mut fixes = 0;
    if level >= CleanupLevel::Light {
        let (r, n) = remove_fillers_counted(&t);
        t = r;
        fixes += n;
    }
    if level >= CleanupLevel::Medium {
        let (r, n) = collapse_repeats(&t);
        t = r;
        fixes += n;
    }
    t = normalize_spacing(&t);
    if level >= CleanupLevel::High {
        let (r, n) = strip_false_starts(&t);
        t = r;
        fixes += n;
    }
    (t, fixes)
}

fn filler_token_re() -> &'static Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)^(?:e+h+m*|e+m+|m{2,}|mhm+|hmm+|u+m+|u+h+m*|erm+)$").expect("regex muletillas")
    })
}

fn leading_o_sea_re() -> &'static Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)^\s*o\s+sea\b[\s,;:.]*").expect("regex o sea"))
}

const TERMINATORS: &[char] = &['.', '!', '?', '…'];

#[allow(dead_code)]
pub fn remove_fillers(text: &str) -> String {
    remove_fillers_counted(text).0
}

fn remove_fillers_counted(text: &str) -> (String, usize) {
    let mut count = 0;
    // «o sea» solo al inicio de la frase dictada.
    if leading_o_sea_re().is_match(text) {
        count += 1;
    }
    let text = leading_o_sea_re().replace(text, "").into_owned();
    let mut out: Vec<String> = Vec::new();
    let mut capitalize_next = false;
    for tok in text.split_whitespace() {
        let core = tok.trim_matches(|c: char| !c.is_alphanumeric());
        if !core.is_empty() && filler_token_re().is_match(core) {
            count += 1;
            // Conservamos el cierre de frase que acompañara a la muletilla. Si iba entre signos
            // («¿eh?», «¡eh!», que es como la escribe Parakeet) era una interjección: su «?» no es
            // de la frase anterior, así que el cierre que se hereda es un punto.
            if let Some(last) = tok.chars().last() {
                if TERMINATORS.contains(&last) {
                    let wrapped = tok.starts_with(['¿', '¡']);
                    let end = if wrapped { '.' } else { last };
                    if let Some(prev) = out.last_mut() {
                        if !prev.ends_with(TERMINATORS) {
                            // «Marta, ¿eh? Te…» -> «Marta. Te…»: la coma colgante se sustituye.
                            let trimmed = prev.trim_end_matches([',', ';', ':']).to_string();
                            *prev = trimmed;
                            prev.push(end);
                        }
                    }
                }
            }
            if tok.chars().next().is_some_and(|c| c.is_uppercase()) {
                capitalize_next = true;
            }
            continue;
        }
        if capitalize_next {
            out.push(capitalize_first(tok));
            capitalize_next = false;
        } else {
            out.push(tok.to_string());
        }
    }
    (out.join(" "), count)
}

/// Palabras que se repiten a propósito («muy muy», «no no»): no se tocan.
const REPEAT_OK: &[&str] = &["muy", "very", "no", "ja", "jaja", "ha", "hahaha", "so", "bye", "tal", "poco", "yes", "sí", "nunca", "bla"];

/// «que que» -> «que». Solo repeticiones inmediatas de una palabra (con coma opcional entre ellas).
fn collapse_repeats(text: &str) -> (String, usize) {
    let mut out: Vec<String> = Vec::new();
    let mut count = 0;
    let core = |t: &str| t.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
    for tok in text.split_whitespace() {
        if let Some(prev) = out.last_mut() {
            let (pc, tc) = (core(prev), core(tok));
            // La primera no puede cerrar frase («Vale. Vale» son dos frases).
            let prev_ok = prev.ends_with(',') || prev.chars().last().is_some_and(|c| c.is_alphanumeric());
            if !tc.is_empty()
                && pc == tc
                && prev_ok
                && !REPEAT_OK.contains(&tc.as_str())
                && !tc.chars().all(|c| c.is_ascii_digit())
            {
                // Se conserva la primera aparición (con su mayúscula) y la puntuación de la segunda.
                let trailing: String = tok.chars().rev().take_while(|c| !c.is_alphanumeric()).collect::<Vec<_>>().into_iter().rev().collect();
                let base = prev.trim_end_matches(|c: char| !c.is_alphanumeric()).to_string();
                *prev = format!("{base}{trailing}");
                count += 1;
                continue;
            }
        }
        out.push(tok.to_string());
    }
    (out.join(" "), count)
}

fn false_start_res() -> &'static [Regex; 2] {
    use std::sync::OnceLock;
    static RE: OnceLock<[Regex; 2]> = OnceLock::new();
    RE.get_or_init(|| {
        [
            Regex::new(r"(?i)^\s*(?:(?:bueno|vale),?\s+pues|okay,?\s+so)\b[\s,;:]*").expect("regex arranque 1"),
            // Las palabras sueltas solo cuentan si van seguidas de coma (pausa).
            Regex::new(r"(?i)^\s*(?:a\s+ver|bueno|pues|well|so)\s*,\s*").expect("regex arranque 2"),
        ]
    })
}

/// Arranques en falso cortos al inicio: «bueno, pues», «a ver,».
fn strip_false_starts(text: &str) -> (String, usize) {
    let mut t = text.to_string();
    let mut count = 0;
    for _ in 0..2 {
        for re in false_start_res() {
            if re.is_match(&t) {
                t = capitalize_first(&re.replace(&t, ""));
                count += 1;
            }
        }
    }
    (t, count)
}

fn press_enter_re() -> &'static Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)[\s,;:.]*\b(?:pulsa|presiona|pulse|aprieta|press)\s+(?:enter|intro|return)\b[\s.!?]*$").expect("regex enter")
    })
}

/// «…pulsa enter» / «press enter» al final: se quita la orden y se avisa para pulsar Enter.
/// Una orden negativa («no pulse enter», «don't press enter», «nunca pulses intro») no es la orden.
pub fn strip_press_enter(text: &str) -> (String, bool) {
    let Some(m) = press_enter_re().find(text) else {
        return (text.to_string(), false);
    };
    let prev = text[..m.start()].split_whitespace().next_back().unwrap_or("");
    let prev = fold(prev.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'' && c != '’'));
    // La negación tiene que ir en la misma frase: «¿Lo mando así? No. Pulsa enter.» es una
    // respuesta y luego la orden (rompedor 23-oct, dictado BAJA 11).
    let new_sentence = text[m.start()..].chars().take_while(|c| !c.is_alphanumeric()).any(|c| TERMINATORS.contains(&c));
    if !new_sentence && matches!(prev.as_str(), "no" | "nunca" | "jamas" | "ni" | "not" | "never" | "don't" | "don’t" | "dont") {
        return (text.to_string(), false);
    }
    (press_enter_re().replace(text, "").trim().to_string(), true)
}

pub fn normalize_spacing(text: &str) -> String {
    use std::sync::OnceLock;
    static BEFORE: OnceLock<Regex> = OnceLock::new();
    static COMMAS: OnceLock<Regex> = OnceLock::new();
    let before = BEFORE.get_or_init(|| Regex::new(r"\s+([,.;:!?…)\]])").expect("regex espacios"));
    let commas = COMMAS.get_or_init(|| Regex::new(r",\s*,+").expect("regex comas"));
    let joined = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let t = before.replace_all(&joined, "$1");
    // Una muletilla entre comas deja «hola, , mundo».
    let t = commas.replace_all(&t, ",");
    t.trim().trim_start_matches([',', ';', ':']).trim_start().to_string()
}

// ---------------------------------------------------------------- 2. retroceso

fn backtrack_re() -> &'static Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        RegexBuilder::new(
            // «no, mejor dicho» va entero y antes que «no, mejor»: si no, gana «no, mejor» y lo
            // que queda empieza por «Dicho,» (rompedor 23-oct, dictado MEDIA 2).
            r"\b(?P<t>no,?\s+mejor\s+dicho|no,?\s+mejor|mejor\s+dicho|no,?\s+quiero\s+decir|quiero\s+decir|scratch\s+that|borra\s+eso|no,?\s+wait)\b[\s,.;:]*",
        )
        .case_insensitive(true)
        .build()
        .expect("regex retroceso")
    })
}

/// Palabras que, justo delante de «quiero decir» / «mejor dicho» sin pausa, los hacen parte de la
/// frase y no una corrección: «lo que quiero decir», «te quiero decir», «está mejor dicho así». Sin
/// los pronombres tónicos («yo», «tú»): no pueden ir delante de «quiero decir» como complemento y
/// tapaban correcciones como «Cómpralo tú quiero decir yo» (rompedor 23-oct, dictado BAJA 9).
const BACKTRACK_CONTEXT_BEFORE: &[&str] = &[
    "que", "lo", "te", "le", "me", "se", "nos", "os", "les", "es", "esta", "estaba", "estan", "era", "fue", "seria", "sera", "queda",
    "quedaria", "quedo", "muy", "mas", "tan", "bien", "algo", "eso", "esto", "ya", "solo", "no", "nunca", "ni",
];

/// Palabras de función: un reinicio de la frase («Compra pan quiero decir compra leche») tiene que
/// repetir una palabra con contenido, no «a», «la» o «de» («A las cinco quiero decir a todos…»).
const FUNCTION_WORDS: &[&str] = &[
    "a", "al", "de", "del", "el", "la", "los", "las", "lo", "un", "una", "unos", "unas", "en", "con", "por", "para", "y", "o", "e", "u", "que",
    "se", "me", "te", "le", "les", "nos", "os", "mi", "tu", "su", "the", "an", "to", "at", "in", "on", "of", "and", "or", "for",
];

const NUMBER_WORDS: &[&str] = &[
    "cero", "uno", "dos", "tres", "cuatro", "cinco", "seis", "siete", "ocho", "nueve", "diez", "once", "doce", "trece", "catorce", "quince",
    "dieciseis", "diecisiete", "dieciocho", "diecinueve", "veinte", "treinta", "cuarenta", "cincuenta", "sesenta", "setenta", "ochenta",
    "noventa", "cien", "ciento", "doscientos", "quinientos", "mil", "zero", "one", "two", "three", "four", "five", "six", "seven", "eight",
    "nine", "ten", "eleven", "twelve", "fifteen", "twenty", "thirty", "fifty", "hundred", "thousand",
];
const DAY_WORDS: &[&str] = &[
    "lunes", "martes", "miercoles", "jueves", "viernes", "sabado", "domingo", "monday", "tuesday", "wednesday", "thursday", "friday",
    "saturday", "sunday",
];
const MONTH_WORDS: &[&str] = &[
    "enero", "febrero", "marzo", "abril", "mayo", "junio", "julio", "agosto", "septiembre", "setiembre", "octubre", "noviembre", "diciembre",
    "january", "february", "march", "april", "june", "july", "august", "september", "october", "november", "december",
];
const WHEN_WORDS: &[&str] = &["hoy", "manana", "ayer", "anoche", "today", "tomorrow", "yesterday", "tonight"];
/// Con tilde: «tú» es pronombre y «tu», posesivo («Yo quiero decir tu nombre» no es corrección).
const PRONOUNS: &[&str] = &[
    "yo", "tú", "él", "ella", "ello", "nosotros", "nosotras", "vosotros", "vosotras", "ellos", "ellas", "usted", "ustedes", "mí", "ti", "i",
    "you", "he", "she", "we", "they", "him", "her", "them",
];

/// Clase cerrada de una palabra (en minúsculas, con tildes): número, día, mes, «hoy/mañana» o
/// pronombre. Dos palabras de la misma clase se pueden cambiar una por otra en una corrección.
fn closed_class(w: &str) -> Option<u8> {
    if !w.is_empty() && w.chars().all(|c| c.is_ascii_digit() || matches!(c, ':' | '.' | ',')) {
        return Some(0);
    }
    if PRONOUNS.contains(&w) {
        return Some(4);
    }
    let f = fold(w);
    [NUMBER_WORDS, DAY_WORDS, MONTH_WORDS, WHEN_WORDS].iter().position(|list| list.contains(&f.as_str())).map(|i| i as u8)
}

/// Palabras de un tramo, sin signos y en minúsculas (con sus tildes).
fn clause_words(s: &str) -> Vec<String> {
    s.split_whitespace()
        .map(|t| t.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
        .filter(|t| !t.is_empty())
        .collect()
}

/// ¿Lo que sigue (`b`) es del mismo tipo que lo que corrige (el final de `a`)? Es lo único que
/// distingue sin pausas una corrección de «quiero decir» como verbo: o se repite la frase desde su
/// primera palabra con contenido («Compra pan quiero decir compra leche»), o se repite el final
/// cambiando solo su última palabra por otra de la misma clase cerrada («a las cinco quiero decir a
/// las seis», «Cómpralo tú quiero decir yo», «el lunes no, quiero decir el martes»). En la duda, no.
fn parallel_correction(a: &[String], b: &[String]) -> bool {
    // Una sola palabra delante es el sujeto o un adverbio del verbo «quiero» («Hoy quiero decir…»).
    if a.len() < 2 || b.is_empty() || b.len() > 6 {
        return false;
    }
    let content = |w: &str| !FUNCTION_WORDS.contains(&fold(w).as_str());
    if b.len() >= 2 && b[0] == a[0] && content(&b[0]) {
        return true;
    }
    (1..=a.len().min(b.len()).min(3)).any(|k| {
        let tail = &a[a.len() - k..];
        tail[..k - 1] == b[..k - 1] && tail[k - 1] != b[k - 1] && closed_class(&tail[k - 1]).is_some_and(|c| closed_class(&b[k - 1]) == Some(c))
    })
}

/// Las órdenes de borrar («borra eso», «scratch that») borran aunque no venga nada detrás; el resto
/// de disparadores son correcciones y solo cuentan si les sigue la corrección.
fn is_erase_command(trigger: &str) -> bool {
    let t = fold(trigger);
    t.starts_with("borra") || t.starts_with("scratch")
}

/// ¿El disparador es un retroceso de verdad? `start..core_end` es el disparador y `core_end..end`
/// las pausas que lo siguen. Rompedor 23-oct, dictado ALTA 1: la regla era «cuenta salvo que la
/// palabra de delante esté en una lista», y se comía frases normales («Hoy quiero decir algo
/// importante» -> «Algo importante»). Ahora es al revés: solo cuenta si se ve que el usuario se
/// corrige, y en la duda se deja el texto tal cual (un dictado mal corregido es peor que una
/// corrección que no salta; lo oído queda además en el historial).
/// - «no quiero decir», «no mejor», «no wait» sin coma son negaciones normales: nunca.
/// - Orden de borrar: al principio o tras una pausa, y al final del dictado o seguida de pausa
///   («Te mando el informe. Borra eso.»); nunca con sujeto delante («Mi hijo borra eso de la
///   pizarra», «to scratch that») ni en una pregunta («¿Borra eso?», BAJA 10).
/// - Corrección: necesita algo que corregir delante (que no sea una pregunta) y la corrección
///   detrás, en la misma frase y sin empezar por «que» («quiero decir que…» explica, no corrige).
///   Cuenta sola si va aislada: dos de pausa delante, pausa detrás y «no,» («lunes, quiero decir,
///   el martes», «a ti, no, quiero decir a él», «mañana no, mejor dicho, el viernes»). Si no, solo
///   si lo que sigue es del mismo tipo que lo que reemplaza (`parallel_correction`).
fn is_real_backtrack(text: &str, start: usize, core_end: usize, end: usize) -> bool {
    let core = fold(&text[start..core_end]);
    let tail = &text[core_end..end];
    let marker_no = core.starts_with("no") && core.contains(',');
    if core.starts_with("no") && !marker_no {
        return false;
    }
    let before = text[..start].trim_end();
    let rest = text[end..].trim_start();
    let tail_pause = tail.contains([',', ';', ':']);
    let tail_end = tail.contains(TERMINATORS);
    if is_erase_command(&core) {
        let start_ok = before.is_empty() || before.ends_with([',', ';', ':']) || before.ends_with(TERMINATORS);
        let end_ok = rest.is_empty() || tail_pause || tail_end;
        return start_ok && end_ok && !rest.starts_with('?');
    }
    if before.is_empty() || before.ends_with('?') || tail_end {
        return false;
    }
    let b = clause_words(rest.split(|c: char| TERMINATORS.contains(&c) || matches!(c, ',' | ';' | ':')).next().unwrap_or(""));
    if b.first().is_none_or(|w| w == "que") {
        return false;
    }
    let pause_before = before.ends_with([',', ';', ':']) || before.ends_with(TERMINATORS);
    if [pause_before, tail_pause, marker_no].iter().filter(|x| **x).count() >= 2 {
        return true;
    }
    // «no, mejor» suelto y sin pausas es una respuesta («Hoy no, mejor mañana»), no una corrección.
    if marker_no && !core.contains("dicho") && !core.contains("decir") {
        return false;
    }
    let prev_word = fold(before.rsplit(char::is_whitespace).next().unwrap_or("").trim_matches(|c: char| !c.is_alphanumeric()));
    if !pause_before && !marker_no && BACKTRACK_CONTEXT_BEFORE.contains(&prev_word.as_str()) {
        return false;
    }
    // Lo que se corrige: la frase o el inciso que acaba justo antes del disparador.
    let body = before.trim_end_matches(|c: char| c.is_whitespace() || TERMINATORS.contains(&c) || matches!(c, ',' | ';' | ':'));
    let clause = body.rsplit(|c: char| TERMINATORS.contains(&c) || matches!(c, ',' | ';' | ':' | '¿' | '¡')).next().unwrap_or("");
    parallel_correction(&clause_words(clause), &b)
}

/// «No, mejor…» / «quiero decir» / «scratch that» descartan la frase anterior (desde el último
/// signo de fin de frase) y el texto continúa con lo que viene después. Las correcciones («quiero
/// decir», «mejor dicho», «no, mejor…») solo actúan si les sigue la corrección y no dejan nunca
/// vacío un dictado (rompedor 22-oct-c, ALTA 1: «Eso es justo lo que quiero decir.» se borraba
/// entero). Solo una orden de borrar explícita («borra eso», «scratch that») puede vaciarlo. Qué
/// cuenta como retroceso: `is_real_backtrack`. Fuera de la tubería desde el 23-oct-2026 (ver `process`).
#[allow(dead_code)]
pub fn apply_backtrack(text: &str) -> String {
    let mut current = text.to_string();
    // Desde dónde buscar: los disparadores que no son retroceso se saltan sin tocarlos.
    let mut from = 0;
    let mut applied = 0;
    // Cada vuelta aplica o salta un disparador, y aplicar uno acorta el texto: el tope solo está
    // contra bucles. Antes era un 16 fijo y en manos libres (hasta 20 min) las correcciones de más
    // se quedaban tal cual (rompedor 23-oct, dictado BAJA 8).
    let mut rounds = 2 * backtrack_re().find_iter(text).count() + 2;
    while rounds > 0 {
        rounds -= 1;
        let Some(caps) = backtrack_re().captures_at(&current, from) else {
            break;
        };
        let (Some(m), Some(core)) = (caps.get(0), caps.name("t")) else { break };
        let (start, end) = (m.start(), m.end());
        if !is_real_backtrack(&current, start, core.end(), end) {
            from = end;
            continue;
        }
        applied += 1;
        let before = current[..start].trim_end().trim_end_matches([',', ';', ':']).trim_end().to_string();
        let after = current[end..].trim().to_string();

        if before.is_empty() {
            current = capitalize_first(&after);
            from = 0;
            continue;
        }
        // Se descarta la frase en curso: se corta tras el último fin de frase previo.
        let body = before.trim_end_matches(TERMINATORS);
        let keep = match body.rfind(TERMINATORS) {
            Some(i) => {
                let len = body[i..].chars().next().map_or(1, char::len_utf8);
                body[..i + len].to_string()
            }
            None => String::new(),
        };
        // Lo que ya queda antes no se vuelve a mirar: sus disparadores saltados siguen saltados.
        from = keep.len();
        current = if keep.is_empty() {
            capitalize_first(&after)
        } else if after.is_empty() {
            keep
        } else {
            format!("{} {}", keep, capitalize_first(&after))
        };
        from = from.min(current.len());
    }
    // Lo que queda sin letras ni cifras («?» de «¿…? Borra eso.») es un dictado vacío: así va por
    // «Se borró todo el dictado» y queda en el historial (rompedor 23-oct, dictado BAJA 10).
    if applied > 0 && !current.chars().any(char::is_alphanumeric) {
        return String::new();
    }
    current.trim().to_string()
}

// ---------------------------------------------------------------- 3. diccionario

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Expresión «palabra completa» e insensible a mayúsculas para `phrase`.
fn whole_word_re(phrase: &str) -> Option<std::sync::Arc<Regex>> {
    cached_phrase_re(phrase, false, false)
}

/// `phrase_re` con caché por patrón: tras un cambio del diccionario solo se compilan las entradas
/// nuevas, y los disparadores de los snippets no se recompilan en cada dictado (rompedor 23-oct,
/// dictado BAJA 12: grabar una pronunciación con 1000 palabras costaba ~4 s el dictado siguiente).
fn cached_phrase_re(phrase: &str, loose: bool, fold: bool) -> Option<std::sync::Arc<Regex>> {
    use parking_lot::Mutex;
    use std::collections::HashMap;
    use std::sync::{Arc, OnceLock};
    type Cache = Mutex<HashMap<(String, bool, bool), Arc<Regex>>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = (phrase.to_string(), loose, fold);
    if let Some(re) = cache.lock().get(&key) {
        return Some(re.clone());
    }
    let re = Arc::new(phrase_re(phrase, loose, fold)?);
    let mut cache = cache.lock();
    // Tope de memoria: patrones de entradas ya borradas no se acumulan sin fin.
    if cache.len() >= 50_000 {
        cache.clear();
    }
    cache.insert(key, re.clone());
    Some(re)
}

/// Como `whole_word_re`, con dos holguras opcionales. `loose`: entre palabras admite una coma o
/// punto y coma (Parakeet puntúa a su aire: la variante «Beltrán Labs» debe casar con «Beltrán,
/// Labs»). `fold`: las vocales, la ñ y la ç casan con o sin tilde.
fn phrase_re(phrase: &str, loose: bool, fold: bool) -> Option<Regex> {
    let parts: Vec<String> = phrase
        .split_whitespace()
        .map(|p| if fold { fold_pattern(p) } else { regex::escape(p) })
        .collect();
    if parts.is_empty() {
        return None;
    }
    let first_word = phrase.chars().next().is_some_and(is_word_char);
    let last_word = phrase.chars().last().is_some_and(is_word_char);
    let pat = format!(
        "{}{}{}",
        if first_word { r"\b" } else { "" },
        parts.join(if loose { r"[,;:]?\s+" } else { r"\s+" }),
        if last_word { r"\b" } else { "" }
    );
    RegexBuilder::new(&pat).case_insensitive(true).build().ok()
}

/// Letra base en minúscula: «Á» -> «a», «ñ» -> «n». El resto, en minúscula tal cual.
fn base_letter(c: char) -> char {
    let lower = c.to_lowercase().next().unwrap_or(c);
    match lower {
        'á' | 'à' | 'â' | 'ä' | 'ã' => 'a',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'ó' | 'ò' | 'ô' | 'ö' | 'õ' => 'o',
        'ú' | 'ù' | 'û' | 'ü' => 'u',
        'ñ' => 'n',
        'ç' => 'c',
        other => other,
    }
}

/// Minúsculas y sin tildes, para comparar.
pub(crate) fn fold(s: &str) -> String {
    s.chars().map(base_letter).collect()
}

fn fold_pattern(token: &str) -> String {
    token
        .chars()
        .map(|c| match base_letter(c) {
            'a' => "[aáàâäã]".to_string(),
            'e' => "[eéèêë]".to_string(),
            'i' => "[iíìîï]".to_string(),
            'o' => "[oóòôöõ]".to_string(),
            'u' => "[uúùûü]".to_string(),
            'n' => "[nñ]".to_string(),
            'c' => "[cç]".to_string(),
            _ => regex::escape(&c.to_string()),
        })
        .collect()
}

/// Palabras corrientes (minúsculas y sin tildes): una palabra suelta del diccionario que sea una de
/// estas no ajusta mayúsculas ni tildes (si alguien guarda «Mar» o «Rosa» como nombre, «el mar» y
/// «una rosa» no deben cambiar; tampoco «esta»/«está» o «mas»/«más»). Incluye nombres de pila y
/// apellidos que también son palabras, y las que cambian de sentido con la tilde.
const COMMON_WORDS: &[&str] = &[
    // español: lo más frecuente
    "a", "al", "algo", "alguien", "algun", "alguna", "alguno", "algunas", "algunos", "alla", "alli", "ahi", "ahora", "antes", "ante", "aqui", "asi",
    "aun", "aunque", "bajo", "bien", "bueno", "buena", "cada", "casi", "cerca", "como", "con", "contra", "cual", "cuales", "cuando", "cuanto",
    "cuanta", "de", "del", "desde", "despues", "donde", "dos", "durante", "el", "ella", "ellas", "ello", "ellos", "en", "entre", "era", "eres",
    "es", "esa", "esas", "ese", "eso", "esos", "esta", "estas", "este", "esto", "estos", "estoy", "estan", "estamos", "fue", "fui", "ha", "habia",
    "han", "has", "hasta", "hay", "he", "hemos", "hoy", "la", "las", "le", "les", "lo", "los", "luego", "mal", "mas", "me", "menos", "mi", "mia",
    "mias", "mio", "mios", "mis", "mismo", "misma", "mucho", "mucha", "muchos", "muchas", "muy", "nada", "nadie", "ni", "ningun", "ninguna",
    "ninguno", "no", "nos", "nosotros", "nosotras", "nuestro", "nuestra", "nuestros", "nuestras", "nunca", "o", "os", "otra", "otras", "otro",
    "otros", "para", "pero", "poco", "poca", "pocos", "pocas", "por", "porque", "pues", "que", "quien", "quienes", "se", "sea", "segun", "ser",
    "si", "sido", "siempre", "sin", "sobre", "solo", "sola", "son", "soy", "su", "sus", "suyo", "suya", "tal", "tambien", "tampoco", "tan",
    "tanto", "tanta", "te", "ti", "tiene", "tienen", "tengo", "todo", "toda", "todos", "todas", "tras", "tu", "tus", "tuyo", "tuya", "un", "una",
    "unas", "uno", "unos", "usted", "ustedes", "va", "vamos", "van", "vaya", "voy", "vosotros", "vuestro", "vuestra", "y", "ya", "yo", "e", "u",
    "dar", "decir", "dice", "dijo", "hacer", "hace", "hizo", "ir", "poder", "puede", "pueden", "puedo", "querer", "quiero", "saber", "sabe",
    "ver", "veo", "visto", "venir", "viene", "llegar", "pasar", "pasa", "deber", "debe", "poner", "parecer", "quedar", "creer", "creo", "hablar",
    "llevar", "dejar", "seguir", "encontrar", "llamar", "pensar", "salir", "volver", "tomar", "conocer", "vivir", "sentir", "tratar", "mirar",
    "contar", "empezar", "esperar", "buscar", "existir", "entrar", "trabajar", "escribir", "perder", "producir", "ocurrir", "entender", "pedir",
    "recibir", "recordar", "terminar", "permitir", "aparecer", "conseguir", "comenzar", "servir", "sacar", "necesitar", "mantener", "resultar",
    "leer", "caer", "cambiar", "presentar", "crear", "abrir", "considerar", "oir", "acabar", "convertir", "ganar", "formar", "traer", "partir",
    "morir", "aceptar", "realizar", "suponer", "comprender", "lograr", "explicar", "preguntar", "tocar", "reconocer", "estudiar", "alcanzar",
    "nacer", "dirigir", "correr", "utilizar", "pagar", "ayudar", "gustar", "jugar", "escuchar", "cumplir", "ofrecer", "descubrir", "levantar",
    "intentar", "usar", "decidir", "repetir", "olvidar", "valer", "comer", "mostrar", "ocupar", "mover", "continuar", "suceder", "fijar",
    "referir", "acercar", "dedicar", "aprender", "comprar", "subir", "evitar", "interesar", "cerrar", "echar", "responder", "sufrir", "importar",
    "obtener", "observar", "indicar", "imaginar", "soler", "detener", "desarrollar", "senalar", "elegir", "preparar", "proponer", "demostrar",
    "significar", "reunir", "faltar", "acompanar", "desear", "enviar", "vender", "construir", "guardar", "publico", "practico", "calculo",
    "termino", "ultimo", "ultima", "medico", "musica", "numero", "pagina", "telefono", "rapido", "facil", "dificil", "util", "cosa", "cosas",
    "casa", "dia", "dias", "vez", "veces", "vida", "tiempo", "ano", "anos", "mundo", "parte", "forma", "caso", "lugar", "trabajo", "hombre",
    "mujer", "persona", "gente", "momento", "manera", "hora", "horas", "agua", "noche", "tarde", "manana", "semana", "mes", "nombre", "punto",
    "lado", "pais", "ciudad", "grupo", "problema", "fin", "hecho", "historia", "idea", "mano", "ojo", "padre", "madre", "hijo", "hija", "papa",
    "mama", "amigo", "amiga", "nino", "nina", "cliente", "empresa", "equipo", "proyecto", "reunion", "correo", "mensaje", "llamada", "texto",
    "palabra", "voz", "nuevo", "nueva", "grande", "gran", "mejor", "peor", "mayor", "menor", "primero", "primera", "segundo", "segunda", "tres",
    "cuatro", "cinco", "seis", "siete", "ocho", "nueve", "diez", "cien", "mil", "claro", "vale", "gracias", "hola", "adios", "senor", "senora",
    "don", "dona", "santo", "santa", "san", "real", "rey", "reina", "cruz", "rio", "monte", "sierra", "vega", "campo", "campos", "castillo",
    "torre", "puerto", "playa", "costa", "isla", "valle", "fuente", "fuentes", "iglesia",
    // nombres de pila y apellidos que también son palabras
    "mar", "sol", "luz", "paz", "rosa", "flor", "cielo", "rocio", "pilar", "angeles", "dolores", "mercedes", "victoria", "esperanza", "consuelo",
    "amparo", "soledad", "salud", "nieves", "reyes", "rosario", "remedios", "milagros", "estrella", "aurora", "alba", "blanca", "clara", "gloria",
    "leon", "lobo", "moreno", "blanco", "rubio", "delgado", "bravo", "leal", "prieto", "calvo", "pastor", "herrero", "molina", "nieto", "rico",
    "serrano", "cano", "marques", "conde", "duque", "guerra", "iglesias", "mora", "pena", "caballero", "soler", "romero", "medina", "carmen",
    "angel", "jesus", "salvador", "cristo", "domingo", "lunes", "martes", "miercoles", "jueves", "viernes", "sabado", "verano", "invierno",
    "primavera", "otono", "enero", "febrero", "marzo", "abril", "mayo", "junio", "julio", "agosto", "septiembre", "octubre", "noviembre",
    "diciembre",
    // inglés: lo más frecuente
    "the", "be", "to", "of", "and", "in", "that", "have", "it", "for", "not", "on", "with", "as", "you", "do", "at", "this", "but", "his",
    "by", "from", "they", "we", "say", "her", "she", "or", "an", "will", "my", "one", "all", "would", "there", "their", "what", "so", "up",
    "out", "if", "about", "who", "get", "which", "go", "when", "make", "can", "like", "time", "just", "him", "know", "take", "people",
    "into", "year", "your", "good", "some", "could", "them", "see", "other", "than", "then", "now", "look", "only", "come", "its", "over",
    "think", "also", "back", "after", "use", "two", "how", "our", "work", "first", "well", "way", "even", "new", "want", "because", "any",
    "these", "give", "day", "most", "us", "is", "are", "was", "were", "has", "had", "been", "am", "i", "yes", "ok", "okay", "hi", "hello",
    "thanks", "please", "here", "very", "much", "more", "less", "why", "where", "apple", "windows", "word", "office", "teams", "slack",
    "chrome", "edge", "code", "notion", "zoom", "amazon", "google", "meta", "visa", "shell", "orange", "jaguar", "puma", "corona", "may",
    "june", "april", "august", "grace", "hope", "faith", "joy", "rose", "lily", "ruby", "amber", "dawn", "summer", "victor", "mark", "bill",
    "frank", "rob", "pat", "sue", "jack", "smith", "brown", "black", "white", "green", "young", "king", "wood", "hill",
];

fn common_words() -> &'static std::collections::HashSet<&'static str> {
    use std::sync::OnceLock;
    static SET: OnceLock<std::collections::HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| COMMON_WORDS.iter().copied().collect())
}

/// Todas las palabras de `phrase` son corrientes (ver `COMMON_WORDS`).
pub fn is_common_phrase(phrase: &str) -> bool {
    let tokens: Vec<String> = phrase
        .split_whitespace()
        .map(|t| fold(t.trim_matches(|c: char| !c.is_alphanumeric())))
        .filter(|t| !t.is_empty())
        .collect();
    // También los plurales de las corrientes («casas», «torres»).
    let common = |t: &str| {
        let set = common_words();
        set.contains(t) || t.strip_suffix('s').is_some_and(|b| set.contains(b)) || t.strip_suffix("es").is_some_and(|b| set.contains(b))
    };
    !tokens.is_empty() && tokens.iter().all(|t| common(t))
}

/// Léxico pequeño embebido (`lexico_es.txt`). Sin tildes y en minúsculas: palabras corrientes y
/// nombres de pila (los de las secciones cuyo comentario empieza por «# nombres»); y las palabras
/// corrientes tal cual se escriben, en minúsculas y con sus tildes («verá» no es «Vera»).
struct Lexicon {
    words: std::collections::HashSet<String>,
    names: std::collections::HashSet<String>,
    words_exact: std::collections::HashSet<String>,
}

fn lexicon() -> &'static Lexicon {
    use std::sync::OnceLock;
    static LEX: OnceLock<Lexicon> = OnceLock::new();
    LEX.get_or_init(|| {
        let mut lex = Lexicon { words: Default::default(), names: Default::default(), words_exact: Default::default() };
        let mut in_names = false;
        for line in include_str!("lexico_es.txt").lines() {
            if let Some(comment) = line.trim_start().strip_prefix('#') {
                in_names = fold(comment.trim()).starts_with("nombres");
                continue;
            }
            for w in line.split('#').next().unwrap_or("").split_whitespace() {
                if in_names {
                    lex.names.insert(fold(w));
                } else {
                    lex.words.insert(fold(w));
                    lex.words_exact.insert(w.to_lowercase());
                }
            }
        }
        lex
    })
}

fn known_form(t: &str) -> bool {
    let lex = lexicon();
    common_words().contains(t) || lex.words.contains(t) || lex.names.contains(t)
}

/// ¿`token` es una palabra corriente del léxico escrita igual, tildes incluidas (no un nombre de
/// pila)? Su plural o una forma regular de un verbo del léxico (sin tildes) también cuentan.
fn is_ordinary_word(token: &str) -> bool {
    let t = token.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
    if t.is_empty() {
        return false;
    }
    let exact = |w: &str| lexicon().words_exact.contains(w);
    let plural = |suffix: &str| t.strip_suffix(suffix).is_some_and(exact);
    exact(&t) || plural("s") || plural("es") || (t.is_ascii() && verb_form(&t))
}

/// Terminaciones regulares de los verbos en -ar y en -er/-ir (sin tildes): «llama» -> «llam» +
/// «ar» = «llamar», que está en las listas.
const AR_ENDINGS: &[&str] = &[
    "o", "as", "a", "amos", "ais", "an", "e", "es", "emos", "eis", "en", "aste", "asteis", "aron", "aba", "abas", "abamos", "abais", "aban", "are",
    "aras", "ara", "aremos", "areis", "aran", "aria", "arias", "ariamos", "ariais", "arian", "ase", "ases", "asen", "ando", "ado", "ada", "ados",
    "adas", "ad", "arlo", "arla", "arlos", "arlas", "arle", "arles", "arme", "arte", "arse", "arnos", "ame", "alo", "ala",
];
const ER_IR_ENDINGS: &[&str] = &[
    "o", "es", "e", "emos", "eis", "en", "imos", "is", "i", "iste", "io", "isteis", "ieron", "ia", "ias", "iamos", "iais", "ian", "ere", "eras",
    "era", "eremos", "eran", "eria", "erias", "erian", "ire", "iras", "ira", "iremos", "iran", "iria", "irian", "a", "as", "amos", "an", "iera",
    "ieras", "ieran", "iese", "iendo", "ido", "ida", "idos", "idas", "ed", "id", "erlo", "erla", "erle", "erme", "erte", "erse", "irlo", "irla",
    "irle", "irme", "irte", "irse",
];

/// ¿`t` (sin tildes, minúsculas) es una forma regular de un verbo de las listas?
fn verb_form(t: &str) -> bool {
    let stem_ok = |stem: &str| stem.chars().count() >= 2;
    AR_ENDINGS.iter().any(|e| t.strip_suffix(e).is_some_and(|s| stem_ok(s) && known_form(&format!("{s}ar"))))
        || ER_IR_ENDINGS
            .iter()
            .any(|e| t.strip_suffix(e).is_some_and(|s| stem_ok(s) && (known_form(&format!("{s}er")) || known_form(&format!("{s}ir")))))
}

/// ¿`token` es una palabra que existe (corriente, del léxico, forma regular de un verbo conocido o
/// nombre de pila)? «Suena igual» nunca la cambia por una palabra del diccionario (rompedor
/// 22-oct-c, MEDIA 2: «llama» -> «Ollama», «Sara» -> «Zara», «vaca» -> «Baca»).
pub(crate) fn is_real_word(token: &str) -> bool {
    let t = fold(token.trim_matches(|c: char| !c.is_alphanumeric()));
    if t.is_empty() {
        return false;
    }
    let plural = |suffix: &str| t.strip_suffix(suffix).is_some_and(known_form);
    known_form(&t) || plural("s") || plural("es") || verb_form(&t)
}

/// Todas las palabras de `phrase` existen (`is_real_word`). La huella de voz no cambia un tramo así
/// por una palabra del diccionario (rompedor 23-oct, dictado MEDIA 5: «está sana» -> «Asana»).
pub(crate) fn is_real_phrase(phrase: &str) -> bool {
    let mut tokens = phrase.split_whitespace().filter(|t| t.chars().any(char::is_alphanumeric)).peekable();
    tokens.peek().is_some() && tokens.all(is_real_word)
}

/// ¿Una palabra suelta (sin reemplazo) puede ajustar mayúsculas y tildes cuando el texto ya la
/// contiene? Solo si hay algo que restaurar (una mayúscula, una tilde, una ñ: «kubernetes» en
/// minúsculas solo podría quitar la mayúscula de inicio de frase) y no es una palabra corriente.
/// Tampoco si es una palabra del léxico (rompedor 23-oct, dictado MEDIA 3: con «Pronto» en el
/// diccionario, «hasta pronto» se pegaba «hasta Pronto»; igual «llama», «luna», «cursor»). Los
/// nombres de pila sí la ajustan: «begona» -> «Begoña».
fn case_rule_ok(word: &str) -> bool {
    let w = word.trim();
    if w.chars().filter(|c| c.is_alphanumeric()).count() < 2 {
        return false;
    }
    let restores = w.chars().any(|c| c.is_uppercase() || (c.is_alphabetic() && !c.is_ascii()));
    let ordinary = w.split_whitespace().all(is_ordinary_word);
    restores && !is_common_phrase(w) && !ordinary
}

/// La forma del diccionario; si es toda minúsculas, con la mayúscula inicial de lo encontrado
/// («pádel» al empezar la frase queda «Pádel»).
fn with_case_of(canonical: &str, found: &str) -> String {
    let found_upper = found.chars().find(|c| c.is_alphabetic()).is_some_and(char::is_uppercase);
    if canonical.chars().any(char::is_uppercase) || !found_upper {
        return canonical.to_string();
    }
    capitalize_first(canonical)
}

/// `found` lleva una tilde (o ñ) donde `canonical` no: son palabras distintas que solo se parecen.
fn drops_accent(found: &str, canonical: &str) -> bool {
    let letters = |s: &str| s.chars().filter(|c| !c.is_whitespace()).flat_map(char::to_lowercase).collect::<Vec<_>>();
    letters(found).iter().zip(letters(canonical)).any(|(f, c)| base_letter(*f) != *f && *f != c)
}

/// Lo que oye el motor, listo para guardarse como variante: sin signos alrededor de cada palabra
/// («¿Chonso?» -> «Chonso», «Beltrán, Labs.» -> «Beltrán Labs») y con espacios simples.
pub fn normalize_heard(s: &str) -> String {
    s.split_whitespace()
        .map(|t| t.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Veredicto de una pronunciación grabada para `word` (y su `replacement`, si lo tiene):
/// `ok` = Pronto ya escribe la palabra buena; `covered` = lo oído ya se corrige con la entrada tal
/// cual (es el «error» del reemplazo, o la palabra con otras mayúsculas/tildes); `new` = hay que
/// guardarlo como variante; `empty` = no se entendió nada.
pub fn pronounce_status(word: &str, replacement: Option<&str>, heard: &str) -> &'static str {
    let heard = normalize_heard(heard);
    if heard.is_empty() {
        return "empty";
    }
    let same = |a: &str| normalize_heard(a).to_lowercase() == heard.to_lowercase();
    let repl = replacement.map(str::trim).filter(|r| !r.is_empty());
    if same(repl.unwrap_or(word)) {
        return "ok";
    }
    if repl.is_some() && same(word) {
        return "covered";
    }
    if repl.is_none() && case_rule_ok(word) && fold(&normalize_heard(word)) == fold(&heard) {
        return "covered";
    }
    "new"
}

/// El índice se codifica con caracteres privados (no con dígitos) para que ninguna entrada del
/// diccionario, por rara que sea, pueda casar dentro de un marcador.
fn placeholder(idx: usize) -> String {
    let digits: String = idx.to_string().chars().filter_map(|d| d.to_digit(10)).filter_map(|d| char::from_u32(0xE100 + d)).collect();
    format!("{PH_OPEN}{digits}{PH_CLOSE}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuleKind {
    /// Par «cómo lo transcribe → cómo se escribe» escrito a mano.
    Fix,
    /// Lo que oye Parakeet al pronunciar la palabra (grabado en el hub) -> la palabra.
    Variant,
    /// Palabra suelta: solo ajusta mayúsculas y tildes de la propia palabra.
    Case,
}

struct Rule<'a> {
    from: &'a str,
    to: &'a str,
    kind: RuleKind,
}

fn dictionary_rules(words: &[Word]) -> Vec<Rule<'_>> {
    let mut rules = Vec::new();
    for w in words {
        let word = w.word.trim();
        if word.is_empty() {
            continue;
        }
        let repl = w.replacement.as_deref().map(str::trim).filter(|r| !r.is_empty());
        match repl {
            Some(r) => {
                rules.push(Rule { from: word, to: r, kind: RuleKind::Fix });
                // La palabra buena también ajusta sus mayúsculas: «github» → «GitHub» (BELTR-PRT-11).
                if case_rule_ok(r) && !r.eq_ignore_ascii_case(word) {
                    rules.push(Rule { from: r, to: r, kind: RuleKind::Case });
                }
            }
            None if case_rule_ok(word) => rules.push(Rule { from: word, to: word, kind: RuleKind::Case }),
            None => {}
        }
        // Las variantes van a la palabra buena: el reemplazo si lo hay, si no la propia palabra.
        let target = repl.unwrap_or(word);
        for v in &w.variants {
            let heard = v.heard.trim();
            if !heard.is_empty() {
                rules.push(Rule { from: heard, to: target, kind: RuleKind::Variant });
            }
        }
    }
    // Las frases más largas primero para que «san francisco» gane a «san».
    rules.sort_by_key(|r| std::cmp::Reverse(r.from.chars().count()));
    rules
}

/// Una regla del diccionario ya compilada.
struct CompiledRule {
    re: std::sync::Arc<Regex>,
    to: String,
    kind: RuleKind,
}

/// El diccionario listo para aplicar: reglas compiladas y mapa «suena igual».
struct CompiledDictionary {
    rules: Vec<CompiledRule>,
    sound: std::collections::HashMap<String, String>,
    near: Vec<NearEntry>,
}

/// Lo que define un diccionario (palabra, reemplazo y variantes de cada entrada): su «versión».
type DictionaryKey = Vec<(String, Option<String>, Vec<String>)>;

fn dictionary_key_matches(key: &DictionaryKey, words: &[Word]) -> bool {
    key.len() == words.len()
        && key.iter().zip(words).all(|((w, r, v), word)| {
            *w == word.word && *r == word.replacement && v.len() == word.variants.len() && v.iter().zip(&word.variants).all(|(a, b)| *a == b.heard)
        })
}

fn compile_dictionary(words: &[Word]) -> CompiledDictionary {
    let rules = dictionary_rules(words)
        .into_iter()
        .filter_map(|rule| {
            let re = match rule.kind {
                RuleKind::Fix => whole_word_re(rule.from),
                RuleKind::Variant => cached_phrase_re(rule.from, true, false),
                RuleKind::Case => cached_phrase_re(rule.from, false, true),
            }?;
            Some(CompiledRule { re, to: rule.to.to_string(), kind: rule.kind })
        })
        .collect();
    CompiledDictionary { rules, sound: sound_map(words), near: near_entries(words) }
}

/// Las regex del diccionario se compilan una vez por versión del diccionario, no en cada dictado
/// (rompedor 22-oct-c, BAJA 9: con 1000 palabras costaba segundos por dictado).
fn compiled_dictionary(words: &[Word]) -> std::sync::Arc<CompiledDictionary> {
    use parking_lot::Mutex;
    use std::sync::Arc;
    static CACHE: Mutex<Option<(DictionaryKey, Arc<CompiledDictionary>)>> = Mutex::new(None);
    if let Some((key, compiled)) = CACHE.lock().as_ref() {
        if dictionary_key_matches(key, words) {
            return compiled.clone();
        }
    }
    let compiled = Arc::new(compile_dictionary(words));
    let key = words.iter().map(|w| (w.word.clone(), w.replacement.clone(), w.variants.iter().map(|v| v.heard.clone()).collect())).collect();
    *CACHE.lock() = Some((key, compiled.clone()));
    compiled
}

fn dictionary_inner(text: &str, words: &[Word], protected: &mut Vec<String>, fixes: &mut usize) -> String {
    let mut out = text.to_string();
    if words.is_empty() {
        return out;
    }
    let dict = compiled_dictionary(words);
    for rule in &dict.rules {
        let re = &rule.re;
        if !re.is_match(&out) {
            continue;
        }
        let (to, kind) = (rule.to.as_str(), rule.kind);
        // Una forma toda en minúsculas que solo ajusta tildes no se protege: así el estilo puede
        // ponerle la mayúscula de inicio de frase.
        let protect = kind != RuleKind::Case || to.chars().any(char::is_uppercase);
        let mut slots: Vec<(String, String)> = Vec::new();
        let mut n = 0;
        out = re
            .replace_all(&out, |caps: &regex::Captures| {
                let found = &caps[0];
                // Dentro de un correo, un dominio o una ruta no se toca nada («odoo@clientes.es»;
                // revisión BELTR-PRT-11).
                let m = caps.get(0).expect("coincidencia");
                if glued_to_link(&out, m.start(), m.end()) {
                    return found.to_string();
                }
                // «verá» no es «Vera»: el ajuste pone tildes, nunca las quita.
                if kind == RuleKind::Case && drops_accent(found, to) {
                    return found.to_string();
                }
                let text = if kind == RuleKind::Case { with_case_of(to, found) } else { to.to_string() };
                // Los pares escritos a mano cuentan siempre (como antes); variantes y ajustes,
                // solo si cambian algo.
                if kind == RuleKind::Fix || found != text {
                    n += 1;
                }
                if !protect {
                    return text;
                }
                if let Some((_, ph)) = slots.iter().find(|(t, _)| *t == text) {
                    return ph.clone();
                }
                protected.push(text.clone());
                let ph = placeholder(protected.len() - 1);
                slots.push((text, ph.clone()));
                ph
            })
            .into_owned();
        *fixes += n;
    }
    let out = sound_pass(&out, &dict.sound, protected, fixes);
    near_pass(&out, &dict.near, protected, fixes)
}

/// Clave «suena igual» en español, sobre la forma sin tildes: tx y ch -> ch, ll -> y, v -> b,
/// z y c+e/i -> s, qu, k y c+a/o/u -> k, g+e/i -> j, ph -> f, la h muda fuera y las letras dobles
/// cuentan una. «Txonso» y «Chonzo» dan «chonso»; «Ollama» y «Oyama», «oyama».
fn sound_key(token: &str) -> String {
    let s: Vec<char> = fold(token).chars().filter(|c| c.is_alphanumeric()).collect();
    let mut out: Vec<char> = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        let next = s.get(i + 1).copied();
        let after = s.get(i + 2).copied();
        let (piece, skip): (&str, usize) = match (s[i], next) {
            ('t', Some('x')) | ('c', Some('h')) => ("ch", 1),
            ('l', Some('l')) => ("y", 1),
            ('q', Some('u')) => ("k", 1),
            ('p', Some('h')) => ("f", 1),
            ('g', Some('u')) if matches!(after, Some('e' | 'i')) => ("g", 1),
            ('g', Some('e' | 'i')) => ("j", 0),
            ('c', Some('e' | 'i')) | ('z', _) => ("s", 0),
            ('c' | 'k' | 'q', _) => ("k", 0),
            ('v', _) => ("b", 0),
            ('h', _) => ("", 0),
            _ => ("", 0),
        };
        if piece.is_empty() && s[i] != 'h' {
            out.push(s[i]);
        } else {
            out.extend(piece.chars());
        }
        i += 1 + skip;
    }
    out.dedup();
    out.into_iter().collect()
}

/// Formas de una sola palabra que «suenan» a cada palabra del diccionario: la palabra buena, el
/// «error» de un reemplazo y las variantes grabadas. Solo palabras con clave de 4+ letras y que no
/// sean corrientes; una clave que suene a dos palabras distintas no se usa.
fn sound_map(words: &[Word]) -> std::collections::HashMap<String, String> {
    use std::collections::HashMap;
    let mut map: HashMap<String, Option<&str>> = HashMap::new();
    for w in words {
        let word = w.word.trim();
        if word.is_empty() {
            continue;
        }
        let repl = w.replacement.as_deref().map(str::trim).filter(|r| !r.is_empty());
        let target = repl.unwrap_or(word);
        if target.split_whitespace().count() != 1 || is_common_phrase(target) {
            continue;
        }
        let forms = std::iter::once(target).chain(repl.map(|_| word)).chain(w.variants.iter().map(|v| v.heard.trim()));
        for form in forms.filter(|f| f.split_whitespace().count() == 1 && !is_common_phrase(f)) {
            let key = sound_key(form);
            if key.chars().count() < 4 {
                continue;
            }
            let slot = map.entry(key).or_insert(Some(target));
            if slot.is_some_and(|t| t != target) {
                *slot = None;
            }
        }
    }
    map.into_iter().filter_map(|(k, t)| t.map(|t| (k, t.to_string()))).collect()
}

/// Último paso del diccionario: una palabra que suena igual que una del diccionario (o que una de
/// sus variantes) se escribe como la del diccionario. Parakeet no oye igual una palabra suelta
/// que dentro de una frase («Cholso» al grabarla, «Chonzo» al dictar): esto cubre esa holgura.
/// Una palabra que existe («llama», «vaca») o un nombre de pila distinto («Sara») no se toca nunca:
/// solo se corrige lo que no es una palabra real (ver `is_real_word`).
fn sound_pass(text: &str, map: &std::collections::HashMap<String, String>, protected: &mut Vec<String>, fixes: &mut usize) -> String {
    use std::sync::OnceLock;
    static TOKEN: OnceLock<Regex> = OnceLock::new();
    if map.is_empty() {
        return text.to_string();
    }
    let token = TOKEN.get_or_init(|| Regex::new(r"[\p{L}\p{N}]+").expect("regex palabra"));
    token
        .replace_all(text, |caps: &regex::Captures| {
            let found = &caps[0];
            let m = caps.get(0).expect("coincidencia");
            // Un dominio o una ruta no se tocan («sonet.es» no es «Sonnet.es»; revisión BELTR-PRT-11).
            if glued_to_link(text, m.start(), m.end()) {
                return found.to_string();
            }
            let key = sound_key(found);
            let target = match map.get(&key).map(String::as_str).or_else(|| near_key(map, &key)) {
                Some(t) if fold(found) != fold(t) && !is_common_phrase(found) && !is_real_word(found) => t,
                _ => return found.to_string(),
            };
            *fixes += 1;
            let text = with_case_of(target, found);
            if !target.chars().any(char::is_uppercase) {
                return text;
            }
            protected.push(text);
            placeholder(protected.len() - 1)
        })
        .into_owned()
}

/// Holgura de la vocal inicial: Parakeet a veces se la come o la añade dentro de una frase
/// («Oyama» al grabar, «Yama» al dictar). Solo esa diferencia, solo si la otra parte coincide
/// entera (4+ letras) y solo si apunta a una única palabra del diccionario. `sound_pass` solo la
/// usa con palabras que no existen («Yama» sí, «llama» no). Y solo cuando lo que queda tras la
/// vocal empieza por «y»/«ll», que es lo que se ha visto que Parakeet hace («Oyama» -> «Yama»):
/// con cualquier vocal pisaba palabras fuera del léxico (rompedor 23-oct, dictado MEDIA 4: «sana»
/// -> «Asana», «lisa» -> «Elisa», «riel» -> «Ariel», «cana» -> «Ocaña»).
fn near_key<'a>(map: &'a std::collections::HashMap<String, String>, key: &str) -> Option<&'a str> {
    let vowel_head = |k: &str| k.chars().next().is_some_and(|c| "aeiou".contains(c));
    let y_head = |k: &str| k.starts_with('y');
    if key.chars().count() < 4 {
        return None;
    }
    let mut hits = map.iter().filter(|(k, _)| {
        (vowel_head(k) && y_head(key) && k.chars().skip(1).eq(key.chars())) || (vowel_head(key) && y_head(k) && key.chars().skip(1).eq(k.chars()))
    });
    match (hits.next(), hits.next()) {
        (Some((_, t)), None) => Some(t.as_str()),
        _ => None,
    }
}

// ---------------------------------------------------------------- 3b. diccionario aproximado

/// Una palabra del diccionario para la pasada aproximada (BELTR-PRT-11): la forma buena, sus claves
/// «suena como» (la palabra, el «error» de un reemplazo, las variantes grabadas y cómo se lee en
/// español una palabra inglesa).
struct NearEntry {
    target: String,
    keys: Vec<String>,
}

/// Lecturas en español de una palabra escrita en inglés (sin tildes, minúsculas): «whisper» →
/// «guisper», «parakeet» → «parakit», «sonnet» → «sonit», «claude» → «claud». Incluye la propia.
fn english_readings(s: &str) -> Vec<String> {
    type Lectura = fn(&str) -> Option<String>;
    let rules: [Lectura; 10] = [
        |w| w.contains("ee").then(|| w.replace("ee", "i")),
        |w| w.contains("ea").then(|| w.replace("ea", "i")),
        |w| w.contains("oo").then(|| w.replace("oo", "u")),
        |w| w.contains('w').then(|| w.replace('w', "gu")),
        |w| w.contains('w').then(|| w.replace('w', "u")),
        |w| w.contains("sh").then(|| w.replace("sh", "ch")),
        |w| w.contains("au").then(|| w.replace("au", "o")),
        |w| w.strip_suffix('y').map(|b| format!("{b}i")),
        |w| w.strip_suffix("et").map(|b| format!("{b}it")),
        |w| w.strip_suffix('e').filter(|b| b.chars().last().is_some_and(|c| !"aeiou".contains(c))).map(str::to_string),
    ];
    let mut out = vec![s.to_string()];
    for rule in rules {
        for i in 0..out.len() {
            if out.len() >= 12 {
                break;
            }
            if let Some(v) = rule(&out[i]) {
                if !out.contains(&v) {
                    out.push(v);
                }
            }
        }
    }
    out
}

fn near_entries(words: &[Word]) -> Vec<NearEntry> {
    let mut out: Vec<NearEntry> = Vec::new();
    for w in words {
        let word = w.word.trim();
        if word.is_empty() {
            continue;
        }
        let repl = w.replacement.as_deref().map(str::trim).filter(|r| !r.is_empty());
        let target = repl.unwrap_or(word);
        // Nunca una palabra corriente («Pronto», «Mar») ni algo con cifras.
        if is_common_phrase(target) || target.chars().any(char::is_numeric) {
            continue;
        }
        let mut forms = vec![target.to_string()];
        if repl.is_some() {
            forms.push(word.to_string());
        }
        forms.extend(w.variants.iter().map(|v| v.heard.trim().to_string()).filter(|h| !h.is_empty()));
        let mut keys: Vec<String> = Vec::new();
        for f in &forms {
            let joined: String = fold(&f.split_whitespace().collect::<String>()).chars().filter(|c| c.is_alphanumeric()).collect();
            for spelled in english_readings(&joined) {
                let k = sound_key(&spelled);
                if k.chars().count() >= 3 && !keys.contains(&k) {
                    keys.push(k);
                }
            }
        }
        if keys.is_empty() {
            continue;
        }
        match out.iter_mut().find(|e| e.target == target) {
            Some(e) => {
                for k in keys {
                    if !e.keys.contains(&k) {
                        e.keys.push(k);
                    }
                }
            }
            None => out.push(NearEntry { target: target.to_string(), keys }),
        }
    }
    out
}

/// Distancia de edición entre dos claves (letra a letra).
fn key_distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            cur[j] = (prev[j - 1] + usize::from(a[i - 1] != b[j - 1])).min(prev[j] + 1).min(cur[j - 1] + 1);
        }
        prev = cur;
    }
    prev[b.len()]
}

/// Holgura según lo larga que es la clave: 0 con menos de 5 letras, 1 con 5-7, 2 con 8 o más.
fn near_allowance(key: &str) -> usize {
    match key.chars().count() {
        0..=4 => 0,
        5..=7 => 1,
        _ => 2,
    }
}

/// ¿Vale `key` por la clave `k` del diccionario a distancia `d`? Igual, siempre. En claves largas (8+
/// letras las dos), hasta la holgura (`near_allowance`). En claves de 5-7 letras, solo si lo único que
/// cambia es una vocal: es lo que Parakeet confunde al oír palabras inglesas («Perakit», «Wispar»,
/// «Sanit»), y deja fuera palabras de verdad a las que les falta o cambia una consonante («parquet»
/// no es «Parakeet», «Obama» no es «Ollama», «Sonic» no es «Sonnet»; revisión BELTR-PRT-11).
fn near_ok(key: &str, k: &str, d: usize) -> bool {
    let (a, b): (Vec<char>, Vec<char>) = (key.chars().collect(), k.chars().collect());
    let vowel = |c: char| "aeiou".contains(c);
    d == 0
        || (a.len() >= 8 && b.len() >= 8 && d <= near_allowance(key).min(near_allowance(k)))
        || (d == 1 && a.len() == b.len() && a.len() >= 5 && a.iter().zip(&b).all(|(x, y)| x == y || (vowel(*x) && vowel(*y))))
}

/// La palabra del diccionario que suena como `key`: la más cercana dentro de la holgura de las dos
/// claves (la más corta manda), que empiece por el mismo sonido y que gane a la segunda por al menos
/// 1. Si dos empatan, ninguna.
fn near_match<'a>(entries: &'a [NearEntry], key: &str) -> Option<(&'a NearEntry, usize)> {
    let first = key.chars().next()?;
    let mut scored: Vec<(&NearEntry, usize)> = entries
        .iter()
        .filter_map(|e| {
            e.keys
                .iter()
                .filter(|k| k.starts_with(first))
                .map(|k| (key_distance(key, k), k))
                .filter(|(d, k)| near_ok(key, k, *d))
                .map(|(d, _)| d)
                .min()
                .map(|d| (e, d))
        })
        .collect();
    scored.sort_by_key(|(_, d)| *d);
    match scored.as_slice() {
        [] => None,
        [one] => Some(*one),
        [a, b, ..] if b.1 > a.1 => Some(*a),
        _ => None,
    }
}

/// ¿La palabra `text[a..b]` va pegada a un enlace, un correo, una ruta o un marcador («sonet.es»,
/// «ana@x.com», «src/odoo»)? Entonces el diccionario no la toca.
fn glued_to_link(text: &str, a: usize, b: usize) -> bool {
    // Los dos puntos no: «Estimado Chonso: te escribo» sí se corrige (las URL ya llevan «/»).
    let sticky = |c: char| "@/\\_#".contains(c) || is_marker_char(c);
    let mut before = text[..a].chars().rev();
    let mut after = text[b..].chars();
    let (b1, b2) = (before.next(), before.next());
    let (a1, a2) = (after.next(), after.next());
    b1.is_some_and(sticky)
        || a1.is_some_and(sticky)
        || (a1 == Some('.') && a2.is_some_and(char::is_alphanumeric))
        || (b1 == Some('.') && b2.is_some_and(char::is_alphanumeric))
}

/// Último paso del diccionario (BELTR-PRT-11): Parakeet parte una palabra guardada en dos o tres
/// («Beltran Labs», «su pavase» → «Supabase») o la oye casi igual («Perakit» → «Parakeet», «Wispar»
/// → «Whisper»). Se miran ventanas de 1 a 3 palabras seguidas (sin signos en medio) y se cambian por
/// la del diccionario que suene igual o casi (`near_match`). Guardas contra inventar:
/// - una palabra suelta que existe («olla», «sonata») nunca se toca;
/// - al juntar palabras, la última no existe y tiene 3 letras o más; si alguna existe, tiene que sonar
///   igual; con 2 palabras, una letra de diferencia como mucho; con 3, ninguna;
/// - nada con cifras, ni pegado a un enlace, un correo, una ruta o lo ya protegido.
fn near_pass(text: &str, entries: &[NearEntry], protected: &mut Vec<String>, fixes: &mut usize) -> String {
    use std::sync::OnceLock;
    static TOKEN: OnceLock<Regex> = OnceLock::new();
    if entries.is_empty() {
        return text.to_string();
    }
    let token = TOKEN.get_or_init(|| Regex::new(r"[\p{L}\p{N}]+").expect("regex palabra"));
    let toks: Vec<(usize, usize)> = token.find_iter(text).map(|m| (m.start(), m.end())).collect();
    let glued = |a: usize, b: usize| glued_to_link(text, a, b);
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    let mut i = 0;
    while i < toks.len() {
        let mut best: Option<(usize, &NearEntry, usize)> = None;
        for n in 1..=3 {
            if i + n > toks.len() {
                break;
            }
            let span = &toks[i..i + n];
            if n > 1 && !text[span[n - 2].1..span[n - 1].0].chars().all(char::is_whitespace) {
                break;
            }
            let words: Vec<&str> = span.iter().map(|&(a, b)| &text[a..b]).collect();
            if words.iter().any(|w| w.chars().any(char::is_numeric)) || glued(span[0].0, span[n - 1].1) {
                break;
            }
            let joined: String = words.concat();
            let key = sound_key(&joined);
            let Some((e, d)) = near_match(entries, &key) else { continue };
            let (folded, target) = (fold(&joined), fold(&e.target.split_whitespace().collect::<String>()));
            // Ya está bien escrita (las mayúsculas las ajusta otra regla).
            if n == 1 && folded == target {
                continue;
            }
            // La palabra con letras de más es otra palabra: «chonsos», «beltranlabsx» (palabra completa).
            let main = sound_key(&target);
            if (folded != target && folded.contains(&target)) || (key.chars().count() > main.chars().count() && key.starts_with(main.as_str())) {
                continue;
            }
            let real = |w: &&str| is_real_word(w) || is_common_phrase(w);
            let ok = if n == 1 {
                !real(&words[0])
            } else {
                // Al juntar palabras, la última es la que no existe y tiene 3 letras o más («Beltran Labs»,
                // «su pavase»): así no se come una palabra dicha («Beltrán las veces», «Claquet a veces»,
                // «para Keith»). Si alguna existe, tiene que sonar igual; con 2 palabras, una letra de
                // diferencia como mucho; con 3, ninguna («Su pavo de» no es «Supabase»).
                let last = words[n - 1];
                !real(&last) && last.chars().count() >= 3 && d <= if words.iter().any(real) { 0 } else { [0, 1, 0][n - 1] }
            };
            if ok && best.is_none_or(|(_, _, bd)| d < bd) {
                best = Some((n, e, d));
            }
        }
        match best {
            Some((n, e, _)) => {
                let (a, b) = (toks[i].0, toks[i + n - 1].1);
                out.push_str(&text[last..a]);
                *fixes += 1;
                let fixed = with_case_of(&e.target, &text[a..b]);
                if e.target.chars().any(char::is_uppercase) {
                    protected.push(fixed);
                    out.push_str(&placeholder(protected.len() - 1));
                } else {
                    out.push_str(&fixed);
                }
                last = b;
                i += n;
            }
            None => i += 1,
        }
    }
    out.push_str(&text[last..]);
    out
}

/// Reemplazos del diccionario (palabra completa, insensible a mayúsculas): la vista previa del texto
/// en vivo y el texto ya pulido pasan por aquí (BELTR-PRT-11).
pub fn apply_dictionary(text: &str, words: &[Word]) -> String {
    let mut protected = Vec::new();
    let t = escape_markers(text, &mut protected);
    let t = dictionary_inner(&t, words, &mut protected, &mut 0);
    restore_placeholders(&t, &protected)
}

// ---------------------------------------------------------------- 4. snippets

/// Minúsculas, sin signos de cierre ni de apertura, espacios simples.
fn normalize_trigger(s: &str) -> String {
    s.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches(|c: char| matches!(c, '.' | ',' | ';' | ':' | '!' | '?' | '¿' | '¡' | '…'))
        .trim()
        .to_string()
}

/// Un tramo de `marker_view`: dónde está en la vista, dónde en el texto con marcadores y si es un
/// marcador (entonces solo se puede cortar en sus bordes).
struct ViewSeg {
    view: std::ops::Range<usize>,
    orig: std::ops::Range<usize>,
    marker: bool,
}

/// El texto con los marcadores de índice `< visible` (los reemplazos del diccionario) cambiados por
/// lo que protegen, y los tramos para volver de la vista al texto con marcadores. Los marcadores de
/// snippets ya expandidos (índice `>= visible`) se quedan opacos: una expansión nunca dispara otro
/// snippet.
fn marker_view(text: &str, protected: &[String], visible: usize) -> (String, Vec<ViewSeg>) {
    let mut view = String::with_capacity(text.len());
    let mut segs: Vec<ViewSeg> = Vec::new();
    let mut push = |view: &mut String, content: &str, orig: std::ops::Range<usize>, marker: bool| {
        if orig.is_empty() {
            return;
        }
        let start = view.len();
        view.push_str(content);
        segs.push(ViewSeg { view: start..view.len(), orig, marker });
    };
    let mut plain_start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((pos, c)) = chars.next() {
        if c != PH_OPEN {
            continue;
        }
        let mut end = pos + c.len_utf8();
        let mut num = String::new();
        let mut closed = false;
        while let Some(&(p, d)) = chars.peek() {
            if d == PH_CLOSE {
                chars.next();
                end = p + d.len_utf8();
                closed = true;
                break;
            }
            match (d as u32).checked_sub(0xE100).filter(|v| *v < 10).and_then(|v| char::from_digit(v, 10)) {
                Some(v) => {
                    num.push(v);
                    chars.next();
                    end = p + d.len_utf8();
                }
                None => break,
            }
        }
        let Some(idx) = num.parse::<usize>().ok().filter(|&i| closed && i < visible && i < protected.len()) else { continue };
        push(&mut view, &text[plain_start..pos], plain_start..pos, false);
        push(&mut view, &protected[idx], pos..end, true);
        plain_start = end;
    }
    push(&mut view, &text[plain_start..], plain_start..text.len(), false);
    (view, segs)
}

/// De un rango de la vista al rango del texto con marcadores; `None` si corta un marcador por dentro.
fn view_to_orig(segs: &[ViewSeg], r: std::ops::Range<usize>) -> Option<std::ops::Range<usize>> {
    // Los tramos van en orden y sin huecos: búsqueda binaria.
    let start = segs.get(segs.partition_point(|s| s.view.end <= r.start)).filter(|s| s.view.start <= r.start)?;
    let end = segs.get(segs.partition_point(|s| s.view.end < r.end)).filter(|s| s.view.start < r.end)?;
    let a = if start.marker { (r.start == start.view.start).then_some(start.orig.start)? } else { start.orig.start + (r.start - start.view.start) };
    let b = if end.marker { (r.end == end.view.end).then_some(end.orig.end)? } else { end.orig.start + (r.end - end.view.start) };
    Some(a..b)
}

/// Los disparadores se buscan sobre el texto con los reemplazos del diccionario a la vista: «firma
/// txonso» se expande aunque «Txonso» esté en el diccionario (rompedor 22-oct-c, MEDIA 4).
fn snippets_inner(text: &str, snippets: &[Snippet], protected: &mut Vec<String>, fixes: &mut usize) -> String {
    let valid: Vec<&Snippet> = snippets
        .iter()
        .filter(|s| !normalize_trigger(&s.trigger).is_empty() && !s.expansion.is_empty())
        .collect();
    let visible = protected.len();
    // Frase completa: «mi correo.» -> expansión exacta, sin punto.
    let whole = normalize_trigger(&marker_view(text, protected, visible).0);
    if let Some(s) = valid.iter().find(|s| normalize_trigger(&s.trigger) == whole) {
        *fixes += 1;
        protected.push(s.expansion.clone());
        return placeholder(protected.len() - 1);
    }
    // Dentro de la frase, por palabra completa. Disparadores largos primero.
    let mut sorted = valid;
    sorted.sort_by_key(|s| std::cmp::Reverse(s.trigger.chars().count()));
    let mut out = text.to_string();
    for s in sorted {
        let trigger = s.trigger.trim().trim_matches(|c: char| matches!(c, '.' | ',' | '!' | '?'));
        let Some(re) = whole_word_re(trigger) else { continue };
        let (view, segs) = marker_view(&out, protected, visible);
        let hits: Vec<std::ops::Range<usize>> = re.find_iter(&view).filter_map(|m| view_to_orig(&segs, m.range())).collect();
        if hits.is_empty() {
            continue;
        }
        *fixes += hits.len();
        protected.push(s.expansion.clone());
        let ph = placeholder(protected.len() - 1);
        let mut next = String::with_capacity(out.len());
        let mut last = 0;
        for r in hits {
            next.push_str(&out[last..r.start]);
            next.push_str(&ph);
            last = r.end;
        }
        next.push_str(&out[last..]);
        out = next;
    }
    out
}

/// Sustituye disparadores de snippets por su expansión. Paso suelto, para tests.
#[allow(dead_code)]
pub fn apply_snippets(text: &str, snippets: &[Snippet]) -> String {
    let mut protected = Vec::new();
    let t = escape_markers(text, &mut protected);
    let t = snippets_inner(&t, snippets, &mut protected, &mut 0);
    restore_placeholders(&t, &protected)
}

fn restore_placeholders(text: &str, protected: &[String]) -> String {
    if protected.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != PH_OPEN {
            out.push(c);
            continue;
        }
        let mut num = String::new();
        let mut closed = false;
        for d in chars.by_ref() {
            if d == PH_CLOSE {
                closed = true;
                break;
            }
            if let Some(v) = (d as u32).checked_sub(0xE100).filter(|v| *v < 10).and_then(|v| char::from_digit(v, 10)) {
                num.push(v);
            }
        }
        match (closed, num.parse::<usize>().ok().and_then(|i| protected.get(i))) {
            (true, Some(s)) => out.push_str(s),
            _ => {}
        }
    }
    out
}

// ---------------------------------------------------------------- 5. estilo

fn capitalize_first(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut done = false;
    for c in s.chars() {
        if !done && (c == PH_OPEN) {
            // El primer elemento es un reemplazo protegido: se respeta tal cual.
            done = true;
        }
        if !done && c.is_alphabetic() {
            out.extend(c.to_uppercase());
            done = true;
        } else {
            out.push(c);
            // Los signos de apertura («¿», comillas) no consumen la mayúscula.
            if !done && c.is_alphanumeric() {
                done = true;
            }
        }
    }
    out
}

fn lowercase_unprotected(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut inside = false;
    for c in s.chars() {
        match c {
            PH_OPEN => {
                inside = true;
                out.push(c);
            }
            PH_CLOSE => {
                inside = false;
                out.push(c);
            }
            _ if inside => out.push(c),
            _ => out.extend(c.to_lowercase()),
        }
    }
    out
}

/// Cuántos finales de frase hay antes del último carácter.
fn has_inner_sentence_end(s: &str) -> bool {
    let trimmed = s.trim_end_matches(TERMINATORS);
    trimmed.contains(TERMINATORS)
}

/// Frases afirmativas cortas (hasta 10 palabras) terminan en «!»; las largas conservan su punto,
/// también la última (FR-02 R9).
fn exclaim(s: &str) -> String {
    fn flush(sentence: &mut String, out: &mut String, last: bool) {
        let trimmed = sentence.trim().to_string();
        sentence.clear();
        if trimmed.is_empty() {
            return;
        }
        let words = trimmed.split_whitespace().count();
        let body = match trimmed.strip_suffix('.').filter(|b| !b.ends_with('.')) {
            Some(b) if words <= 10 => format!("{b}!"),
            Some(_) => trimmed.clone(),
            None if last && words <= 10 && trimmed.chars().last().is_some_and(|c| c.is_alphanumeric() || c == PH_CLOSE) => format!("{trimmed}!"),
            None => trimmed.clone(),
        };
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&body);
    }
    let mut out = String::new();
    let mut sentence = String::new();
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        sentence.push(c);
        let ends = matches!(c, '.' | '!' | '?') && chars.get(i + 1).is_none_or(|n| n.is_whitespace());
        if ends {
            flush(&mut sentence, &mut out, i + 1 == chars.len());
        }
    }
    flush(&mut sentence, &mut out, true);
    out
}

pub fn apply_style(text: &str, tone: Tone) -> String {
    let t = text.trim();
    if t.is_empty() {
        return String::new();
    }
    match tone {
        Tone::Formal => {
            let mut s = capitalize_first(t);
            if s.chars().last().is_some_and(|c| c.is_alphanumeric() || c == PH_CLOSE || c == ')') {
                s.push('.');
            }
            s
        }
        Tone::Casual => {
            let mut s = capitalize_first(t);
            // Sin punto final en frases sueltas; «?» y «!» se conservan siempre.
            if s.ends_with('.') && !s.ends_with("..") && !has_inner_sentence_end(&s) {
                s.pop();
            }
            s
        }
        Tone::Excited => exclaim(&capitalize_first(t)),
        Tone::VeryCasual => {
            let mut s = lowercase_unprotected(t);
            while s.ends_with('.') || s.ends_with(',') || s.ends_with(';') {
                s.pop();
            }
            s
        }
    }
}

// ---------------------------------------------------------------- categoría de app

/// `exe` es el nombre del ejecutable (con o sin `.exe`); `title`, el título de la ventana.
pub fn category_for_app(exe: &str, title: &str) -> Category {
    let exe = exe.to_lowercase();
    let exe = exe.trim_end_matches(".exe");
    let title = title.to_lowercase();
    match exe {
        "whatsapp" | "whatsapp.root" | "telegram" | "discord" | "signal" | "messenger" => Category::Personal,
        "slack" | "teams" | "ms-teams" | "msteams" | "zoom" => Category::Work,
        "outlook" | "olk" | "thunderbird" | "hxoutlook" => Category::Email,
        _ if title.contains("gmail") || title.contains("outlook") => Category::Email,
        _ => Category::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::WordVariant;

    fn word(w: &str, r: Option<&str>) -> Word {
        word_v(w, r, &[])
    }
    fn word_v(w: &str, r: Option<&str>, heard: &[&str]) -> Word {
        let variants = heard.iter().enumerate().map(|(i, h)| WordVariant { id: i as i64, heard: (*h).into() }).collect();
        Word { id: 0, word: w.into(), replacement: r.map(String::from), created_at: String::new(), starred: false, source: "manual".into(), variants, samples: Vec::new() }
    }
    fn snip(t: &str, e: &str) -> Snippet {
        Snippet { id: 0, trigger: t.into(), expansion: e.into(), created_at: String::new() }
    }

    #[test]
    fn muletillas_es_en() {
        assert_eq!(clean("Eh, hola, em, ¿qué tal?", CleanupLevel::Light), "Hola, ¿qué tal?");
        assert_eq!(clean("Um, I think, uh, we should go", CleanupLevel::Light), "I think, we should go");
        assert_eq!(clean("mmm vale", CleanupLevel::Light), "vale");
        assert_eq!(clean("Hola eh. Adiós", CleanupLevel::Light), "Hola. Adiós");
        // Así la escribe Parakeet (caso real): la interjección entre signos no deja «,?».
        assert_eq!(
            clean("Hola Marta, ¿eh? Te confirmo la reunión.", CleanupLevel::Light),
            "Hola Marta. Te confirmo la reunión."
        );
        assert_eq!(clean("Vale, ¡eh! Nos vemos.", CleanupLevel::Light), "Vale. Nos vemos.");
    }

    #[test]
    fn o_sea_solo_al_inicio() {
        assert_eq!(clean("O sea, vamos a ir", CleanupLevel::Light), "vamos a ir");
        assert_eq!(clean("Vamos a ir, o sea, mañana", CleanupLevel::Light), "Vamos a ir, o sea, mañana");
    }

    #[test]
    fn no_toca_palabras_normales() {
        assert_eq!(clean("Vamos a mirar el helado de menta", CleanupLevel::Light), "Vamos a mirar el helado de menta");
        assert_eq!(clean("Eh, hola", CleanupLevel::None), "Eh, hola");
    }

    #[test]
    fn espacios_y_signos() {
        assert_eq!(normalize_spacing("hola  ,  mundo !"), "hola, mundo!");
        assert_eq!(normalize_spacing("  uno   dos  "), "uno dos");
    }

    #[test]
    fn retroceso_es() {
        assert_eq!(
            apply_backtrack("Quedamos a las dos. No, mejor a las tres."),
            "A las tres."
        );
        assert_eq!(
            apply_backtrack("Hola a todos. Quedamos a las dos. No, mejor a las tres."),
            "Hola a todos. A las tres."
        );
        assert_eq!(apply_backtrack("Compra pan quiero decir compra leche"), "Compra leche");
    }

    #[test]
    fn retroceso_en() {
        assert_eq!(apply_backtrack("Send it on Monday. Scratch that, send it on Friday."), "Send it on Friday.");
        assert_eq!(apply_backtrack("Hello there. Scratch that."), "");
        assert_eq!(apply_backtrack("Hi all. Meet at noon. Scratch that."), "Hi all.");
    }

    #[test]
    fn retroceso_no_mejor_al_inicio_es_respuesta() {
        assert_eq!(apply_backtrack("No, mejor no."), "No, mejor no.");
    }

    #[test]
    fn diccionario_palabra_completa_e_insensible() {
        let w = [word("wispr", Some("Wispr")), word("de", Some("a"))];
        assert_eq!(apply_dictionary("Usa WISPR de verdad", &w), "Usa Wispr a verdad");
        // «desde» no contiene la palabra «de» completa.
        assert_eq!(apply_dictionary("desde aquí", &w), "desde aquí");
    }

    #[test]
    fn diccionario_palabra_suelta_ajusta_mayusculas_y_tildes() {
        let w = [word("Kubernetes", None), word("BeltranLabs", None), word("Begoña", None), word("pádel", None)];
        assert_eq!(apply_dictionary("kubernetes y beltranlabs", &w), "Kubernetes y BeltranLabs");
        assert_eq!(apply_dictionary("Hola, Begona", &w), "Hola, Begoña");
        assert_eq!(apply_dictionary("jugamos al padel", &w), "jugamos al pádel");
        // Al empezar la frase, la forma en minúsculas conserva la mayúscula que traía.
        assert_eq!(apply_dictionary("Padel mañana", &w), "Pádel mañana");
        // Palabra completa: no toca trozos de otras.
        assert_eq!(apply_dictionary("beltranlabsx", &w), "beltranlabsx");
        // Toda en minúsculas y sin tildes: no hay nada que ajustar (ni se quita la mayúscula).
        let lower = [word("kubernetes", None)];
        assert_eq!(apply_dictionary("Kubernetes va", &lower), "Kubernetes va");
    }

    #[test]
    fn diccionario_no_estropea_palabras_corrientes() {
        let w = [word("Mar", None), word("Rosa", None), word("Está", None), word("Más", None), word("La Paz", None)];
        assert_eq!(apply_dictionary("el mar está en calma", &w), "el mar está en calma");
        assert_eq!(apply_dictionary("una rosa roja", &w), "una rosa roja");
        assert_eq!(apply_dictionary("esta casa es mas grande", &w), "esta casa es mas grande");
        assert_eq!(apply_dictionary("firmaron la paz", &w), "firmaron la paz");
        assert!(is_common_phrase("Mar") && is_common_phrase("la paz") && !is_common_phrase("San Sebastián"));
        assert!(!is_common_phrase("Txonso") && !is_common_phrase("¿?"));
    }

    #[test]
    fn diccionario_variantes_van_a_la_palabra() {
        let w = [word_v("Txonso", None, &["Chonso", "Tonso"])];
        assert_eq!(apply_dictionary("Hola chonso, ¿qué tal?", &w), "Hola Txonso, ¿qué tal?");
        assert_eq!(apply_dictionary("CHONSO y Tonso", &w), "Txonso y Txonso");
        // Palabra completa.
        assert_eq!(apply_dictionary("chonsos", &w), "chonsos");
    }

    #[test]
    fn diccionario_variantes_van_al_reemplazo_si_lo_hay() {
        let w = [word_v("olama", Some("Ollama"), &["o llama"])];
        assert_eq!(apply_dictionary("abre o llama y olama", &w), "abre Ollama y Ollama");
        // Entre las palabras de una variante se admite la coma que Parakeet ponga.
        let b = [word_v("BeltranLabs", None, &["Beltrán Labs"])];
        assert_eq!(apply_dictionary("Soy de Beltrán, labs.", &b), "Soy de BeltranLabs.");
    }

    #[test]
    fn diccionario_lo_mas_largo_primero_entre_variantes_y_pares() {
        // La variante de dos palabras gana al par «labs» -> «LABS».
        let w = [word("labs", Some("LABS")), word_v("BeltranLabs", None, &["beltran labs"])];
        assert_eq!(apply_dictionary("beltran labs y labs", &w), "BeltranLabs y LABS");
    }

    #[test]
    fn diccionario_cuenta_arreglos() {
        let words = [word_v("Txonso", None, &["chonso"]), word("BeltranLabs", None), word("wispr", Some("Wispr"))];
        let o = Options { cleanup: CleanupLevel::Medium, press_enter: false, tone: Tone::Casual, words: &words, snippets: &[] };
        let r = process("chonso y chonso usan beltranlabs y wispr", &o);
        assert_eq!(r.text, "Txonso y Txonso usan BeltranLabs y Wispr");
        assert_eq!(r.fixes_dictionary, 4);
        // Lo que ya estaba bien escrito no cuenta como arreglo.
        let r = process("Txonso usa BeltranLabs", &o);
        assert_eq!((r.text.as_str(), r.fixes_dictionary), ("Txonso usa BeltranLabs", 0));
        // Muy casual baja todo a minúsculas salvo lo del diccionario.
        let vc = Options { tone: Tone::VeryCasual, ..o };
        assert_eq!(process("Hola chonso.", &vc).text, "hola Txonso");
        // Formal pone la mayúscula de inicio aunque la palabra del diccionario sea minúscula.
        let p = [word("pádel", None)];
        let f = Options { tone: Tone::Formal, words: &p, ..o };
        assert_eq!(process("padel el lunes", &f).text, "Pádel el lunes.");
    }

    #[test]
    fn el_ajuste_pone_tildes_pero_no_las_quita() {
        let w = [word("Vera", None)];
        assert_eq!(apply_dictionary("Lo verá mañana con vera", &w), "Lo verá mañana con Vera");
    }

    #[test]
    fn clave_de_sonido() {
        assert_eq!(sound_key("Txonso"), sound_key("Chonzo"));
        assert_eq!(sound_key("Ollama"), sound_key("Oyama"));
        assert_eq!(sound_key("Kubernetes"), sound_key("Cubernetes"));
        assert_eq!(sound_key("Sergio"), sound_key("Serjio"));
        assert_eq!(sound_key("Guille"), "giye");
        assert_ne!(sound_key("Txonso"), sound_key("Cholso"));
    }

    #[test]
    fn suena_igual_que_la_palabra_o_una_variante() {
        let w = [word_v("Txonso", None, &["Cholso"]), word("Kasa", None), word("olama", Some("Ollama"))];
        assert_eq!(apply_dictionary("Hola Chonzo y cholso", &w), "Hola Txonso y Txonso");
        // «Oyama» suena a «Ollama», el reemplazo.
        assert_eq!(apply_dictionary("abre Oyama", &w), "abre Ollama");
        // «casa» suena a «Kasa», pero es corriente: no se toca.
        assert_eq!(apply_dictionary("en casa", &w), "en casa");
        // Una clave que suena a dos palabras del diccionario no se usa.
        let dos = [word("Chonso", None), word("Txonzo", None)];
        assert_eq!(apply_dictionary("dile a Txonso", &dos), "dile a Txonso");
    }

    /// Caso real: WAV con la voz española de Windows (Helena) diciendo cada palabra suelta y luego
    /// una frase; esto es lo que transcribió Parakeet en cada caso.
    #[test]
    fn caso_real_variantes_grabadas_y_frase_dictada() {
        let w = [
            word_v("Txonso", None, &["Cholso"]),
            word_v("BeltranLabs", None, &["Meltranabs"]),
            word_v("Parakeet", None, &["Perakit"]),
            word_v("Ollama", None, &["Oyama"]),
        ];
        let o = Options { cleanup: CleanupLevel::Medium, press_enter: false, tone: Tone::Formal, words: &w, snippets: &[] };
        let r = process("Hola Chonzo, mañana hablamos con el equipo de Beltranab sobre Perakit.", &o);
        // «Beltranab» no coincide con nada grabado, pero suena casi igual que «BeltranLabs» (pasada
        // aproximada, BELTR-PRT-11; antes había que grabarla otra vez).
        assert_eq!(r.text, "Hola Txonso, mañana hablamos con el equipo de BeltranLabs sobre Parakeet.");
        assert_eq!(r.fixes_dictionary, 3);
        assert_eq!(process("Lo instalé con Oyama.", &o).text, "Lo instalé con Ollama.");
        // Dentro de una frase Parakeet se come la vocal inicial: «Oyama» grabado, «Yama» dictado.
        assert_eq!(process("He instalado Yama y Kubernetes en el servidor.", &o).text, "He instalado Ollama y Kubernetes en el servidor.");
    }

    #[test]
    fn normalizar_lo_oido() {
        assert_eq!(normalize_heard("  ¿Chonso?  "), "Chonso");
        assert_eq!(normalize_heard("Beltrán, Labs."), "Beltrán Labs");
        assert_eq!(normalize_heard("sherpa-onnx."), "sherpa-onnx");
        assert_eq!(normalize_heard(" ... "), "");
    }

    #[test]
    fn veredicto_de_la_pronunciacion() {
        assert_eq!(pronounce_status("Txonso", None, "Chonso."), "new");
        assert_eq!(pronounce_status("Txonso", None, "Txonso."), "ok");
        assert_eq!(pronounce_status("Txonso", None, "txonso"), "ok");
        assert_eq!(pronounce_status("Begoña", None, "Begona"), "covered");
        assert_eq!(pronounce_status("olama", Some("Ollama"), "Ollama"), "ok");
        assert_eq!(pronounce_status("olama", Some("Ollama"), "Olama."), "covered");
        assert_eq!(pronounce_status("olama", Some("Ollama"), "o llama"), "new");
        assert_eq!(pronounce_status("Txonso", None, "..."), "empty");
        // «Mar» es corriente: no se ajusta solo, así que «mar» oído es la palabra tal cual.
        assert_eq!(pronounce_status("Mar", None, "mar"), "ok");
    }

    #[test]
    fn diccionario_frase_larga_gana() {
        let w = [word("san", Some("San")), word("san francisco", Some("SF"))];
        assert_eq!(apply_dictionary("voy a san francisco", &w), "voy a SF");
    }

    #[test]
    fn snippets_frase_completa() {
        let s = [snip("mi correo", "ana@ejemplo.com")];
        assert_eq!(apply_snippets("Mi correo.", &s), "ana@ejemplo.com");
        assert_eq!(apply_snippets("mi correo", &s), "ana@ejemplo.com");
    }

    #[test]
    fn snippets_dentro_de_la_frase() {
        let s = [snip("mi correo", "ana@ejemplo.com")];
        assert_eq!(apply_snippets("Escribe a mi correo, gracias", &s), "Escribe a ana@ejemplo.com, gracias");
        assert_eq!(apply_snippets("nada que expandir", &s), "nada que expandir");
    }

    #[test]
    fn estilo_formal() {
        assert_eq!(apply_style("hola que tal", Tone::Formal), "Hola que tal.");
        assert_eq!(apply_style("¿qué tal?", Tone::Formal), "¿Qué tal?");
        assert_eq!(apply_style("Hola. Adiós", Tone::Formal), "Hola. Adiós.");
    }

    #[test]
    fn estilo_casual() {
        assert_eq!(apply_style("hola que tal.", Tone::Casual), "Hola que tal");
        assert_eq!(apply_style("Hola. Qué tal.", Tone::Casual), "Hola. Qué tal.");
        assert_eq!(apply_style("¿qué tal?", Tone::Casual), "¿Qué tal?");
    }

    #[test]
    fn estilo_muy_casual() {
        assert_eq!(apply_style("Hola, ¿Qué Tal?", Tone::VeryCasual), "hola, ¿qué tal?");
        assert_eq!(apply_style("Vale perfecto.", Tone::VeryCasual), "vale perfecto");
    }

    #[test]
    fn categorias() {
        assert_eq!(category_for_app("WhatsApp.exe", ""), Category::Personal);
        assert_eq!(category_for_app("slack.exe", ""), Category::Work);
        assert_eq!(category_for_app("OUTLOOK.EXE", ""), Category::Email);
        assert_eq!(category_for_app("chrome.exe", "Bandeja - Gmail"), Category::Email);
        assert_eq!(category_for_app("notepad.exe", "sin título"), Category::Other);
    }


    #[test]
    fn tuberia_completa() {
        let words = [word("pronto", Some("Pronto"))];
        let snippets = [snip("mi firma", "Saludos,\nTxonso")];
        let o = Options { cleanup: CleanupLevel::Medium, press_enter: false, tone: Tone::VeryCasual, words: &words, snippets: &snippets };
        // Muy casual baja a minúsculas pero respeta el diccionario y los snippets.
        let r = process("Eh, abre pronto.", &o);
        assert_eq!(r.text, "abre Pronto");
        assert_eq!((r.fixes_fillers, r.fixes_dictionary, r.fixes_snippets), (1, 1, 0));
        let r = process("Mi firma.", &o);
        assert_eq!(r.text, "Saludos,\nTxonso");
        assert_eq!(r.fixes_snippets, 1);
        let f = Options { tone: Tone::Formal, ..o };
        // Retroceso quitado por Txonso el 23-oct-2026: la tubería ya no borra lo que se corrige.
        assert_eq!(process("quedamos a las dos. no, mejor a las tres", &f).text, "Quedamos a las dos. no, mejor a las tres.");
    }

    #[test]
    fn nivel_none_deja_el_texto() {
        let o = Options { cleanup: CleanupLevel::None, press_enter: false, tone: Tone::Casual, words: &[], snippets: &[] };
        // Sin limpieza ni retroceso: solo estilo.
        assert_eq!(process("Eh, que que no, mejor otra", &o).text, "Eh, que que no, mejor otra");
    }

    #[test]
    fn nivel_light_solo_muletillas() {
        let o = Options { cleanup: CleanupLevel::Light, press_enter: false, tone: Tone::Casual, words: &[], snippets: &[] };
        // Sin repetidas ni retroceso.
        assert_eq!(process("Eh, que que no, mejor otra", &o).text, "Que que no, mejor otra");
    }

    #[test]
    fn nivel_medium_repetidas_y_retroceso() {
        assert_eq!(collapse_repeats("Creo que que vamos").0, "Creo que vamos");
        assert_eq!(collapse_repeats("es que, que no").0, "es que no");
        assert_eq!(collapse_repeats("muy muy bueno").0, "muy muy bueno");
        assert_eq!(collapse_repeats("Vale. Vale").0, "Vale. Vale");
        let o = Options { cleanup: CleanupLevel::Medium, press_enter: false, tone: Tone::Casual, words: &[], snippets: &[] };
        // Retroceso quitado por Txonso el 23-oct-2026: la tubería ya no borra lo que se corrige.
        assert_eq!(process("Hola. Quedamos el lunes. No, mejor el martes", &o).text, "Hola. Quedamos el lunes. No, mejor el martes");
        // Arranque en falso: solo a nivel alto.
        assert_eq!(process("Bueno, pues vamos", &o).text, "Bueno, pues vamos");
    }

    #[test]
    fn nivel_high_quita_arranques_en_falso() {
        let o = Options { cleanup: CleanupLevel::High, press_enter: false, tone: Tone::Casual, words: &[], snippets: &[] };
        assert_eq!(process("Bueno, pues vamos a ello", &o).text, "Vamos a ello");
        assert_eq!(process("A ver, necesito un café", &o).text, "Necesito un café");
        assert_eq!(process("Bueno ya está", &o).text, "Bueno ya está");
    }

    #[test]
    fn estilo_excited() {
        assert_eq!(apply_style("gracias por todo.", Tone::Excited), "Gracias por todo!");
        assert_eq!(apply_style("hola que tal", Tone::Excited), "Hola que tal!");
        assert_eq!(apply_style("¿vienes?", Tone::Excited), "¿Vienes?");
        assert_eq!(apply_style("Genial. Nos vemos mañana", Tone::Excited), "Genial! Nos vemos mañana!");
    }

    #[test]
    fn snippet_de_frase_completa_sale_tal_cual_en_cualquier_estilo() {
        // FR-02 R7: la expansión exacta, sin punto ni «!» añadidos.
        let s = [snip("mi correo", "ana@ejemplo.com")];
        for tone in [Tone::Formal, Tone::Casual, Tone::Excited, Tone::VeryCasual] {
            let o = Options { cleanup: CleanupLevel::Medium, press_enter: false, tone, words: &[], snippets: &s };
            assert_eq!(process("Mi correo.", &o).text, "ana@ejemplo.com", "{tone:?}");
        }
        // Dentro de una frase el estilo sí cierra la frase.
        let o = Options { cleanup: CleanupLevel::Medium, press_enter: false, tone: Tone::Formal, words: &[], snippets: &s };
        assert_eq!(process("escribe a mi correo", &o).text, "Escribe a ana@ejemplo.com.");
    }

    #[test]
    fn excited_no_exclama_frases_largas_aunque_sean_la_ultima() {
        // FR-02 R9: el «!» solo en frases de hasta 10 palabras.
        let larga = "Mañana nos vemos todos en la oficina de la calle mayor a las diez";
        assert_eq!(apply_style(&format!("Genial. {larga}."), Tone::Excited), format!("Genial! {larga}."));
        assert_eq!(apply_style(larga, Tone::Excited), larga);
        assert_eq!(apply_style("Nos vemos mañana en la oficina a las diez.", Tone::Excited), "Nos vemos mañana en la oficina a las diez!");
    }

    #[test]
    fn pulsa_enter_al_final() {
        assert_eq!(strip_press_enter("Envía el mensaje, pulsa enter."), ("Envía el mensaje".to_string(), true));
        assert_eq!(strip_press_enter("Send it. Press enter"), ("Send it".to_string(), true));
        assert_eq!(strip_press_enter("Pulsa enter para seguir"), ("Pulsa enter para seguir".to_string(), false));
        let o = Options { cleanup: CleanupLevel::Medium, press_enter: true, tone: Tone::Casual, words: &[], snippets: &[] };
        let r = process("Hola a todos pulsa enter", &o);
        assert!(r.press_enter);
        assert_eq!(r.text, "Hola a todos");
        let off = Options { press_enter: false, ..o };
        assert!(!process("Hola a todos pulsa enter", &off).press_enter);
    }

    /// Rompedor 22-oct-c (ALTA 1): «quiero decir» / «mejor dicho» dentro de una frase normal no son
    /// un retroceso; una corrección de verdad sí, y «borra eso» sigue borrando.
    #[test]
    fn retroceso_no_se_come_frases_normales() {
        for t in [
            "Eso es justo lo que quiero decir.",
            "Lo que te quiero decir es que mañana no puedo.",
            "Creo que eso está mejor dicho así.",
            "Quiero decir que mañana no vengo.",
            "Hoy estoy cansado. No quiero decir nada más.",
            "El agua es lo que borra eso.",
        ] {
            assert_eq!(apply_backtrack(t), t);
        }
        assert_eq!(apply_backtrack("Quedamos el lunes, quiero decir, el martes."), "El martes.");
        assert_eq!(apply_backtrack("Hola. Quedamos el lunes no, quiero decir el martes."), "Hola. El martes.");
        assert_eq!(apply_backtrack("Hola. Te mando el informe. Borra eso."), "Hola.");
        // Un disparador saltado no impide ver el siguiente.
        assert_eq!(apply_backtrack("Es lo que quiero decir. Compra pan quiero decir compra leche"), "Es lo que quiero decir. Compra leche");
    }

    /// Rompedor 23-oct, dictado ALTA 1: el retroceso solo salta cuando el usuario se corrige de
    /// verdad (pausas o «mismo tipo»); en la duda, el texto se queda tal cual.
    #[test]
    fn retroceso_solo_si_el_usuario_se_corrige() {
        for t in [
            "Hoy quiero decir algo importante.",
            "Antes de empezar quiero decir que estoy muy contento.",
            "Yo también quiero decir una cosa.",
            "Os cuento una cosa. Quiero decir que mañana no vengo.",
            "Gracias por venir. Quiero decir unas palabras.",
            "Mira, no quiero decir adiós todavía.",
            "Yo no quiero decir su nombre.",
            "Yo quiero decir tu nombre.",
            "A las cinco quiero decir a todos que llego tarde.",
            "Al final de la reunión quiero decir la fecha.",
            "Esto suena mejor dicho en voz alta.",
            "Mi hijo siempre borra eso de la pizarra.",
            "¿Juan borra eso?",
            "¿Borra eso?",
            "Hoy no, mejor mañana.",
            "¿Vamos al cine? No, mejor dicho, al teatro.",
            "There is no wait at the restaurant.",
            "My cat likes to scratch that old sofa.",
        ] {
            assert_eq!(apply_backtrack(t), t);
        }
        for (t, want) in [
            ("Quedamos el lunes, quiero decir, el martes.", "El martes."),
            ("Llego a las cinco quiero decir a las seis.", "A las seis."),
            ("Quedamos el lunes, perdón, quiero decir, el martes.", "El martes."),
            ("Quedamos el lunes. Quiero decir, el martes.", "El martes."),
            ("Te lo mando a ti, no, quiero decir a él.", "A él."),
            ("Quedamos el lunes, quiero decir el martes.", "El martes."),
            ("Cómpralo tú quiero decir yo.", "Yo."),
            ("Dáselo a ella y a ti quiero decir a él.", "A él."),
            ("Quedamos el lunes, no, mejor dicho, el martes.", "El martes."),
            ("Es para mañana no, mejor dicho, para el viernes.", "Para el viernes."),
            ("Hola. Llámame a las cinco. No, mejor dicho, a las seis.", "Hola. A las seis."),
            ("Meet at noon, no, wait, at one.", "At one."),
            ("¿Lo mando? Borra eso.", ""),
            ("Hola, borra eso.", ""),
        ] {
            assert_eq!(apply_backtrack(t), want, "{t:?}");
        }
        // Más de 16 correcciones en un dictado largo (manos libres): todas se aplican (BAJA 8).
        let largo = "Quedamos el lunes, quiero decir, el martes. ".repeat(20);
        assert_eq!(apply_backtrack(&largo), "El martes. ".repeat(20).trim());
    }

    #[test]
    fn pulsa_enter_tras_un_no_de_otra_frase() {
        assert_eq!(strip_press_enter("¿Lo mando así? No. Pulsa enter."), ("¿Lo mando así? No".to_string(), true));
        assert_eq!(strip_press_enter("Eso no pulse enter"), ("Eso no pulse enter".to_string(), false));
    }

    /// Rompedor 23-oct, dictado MEDIA 3-4: el diccionario no pisa palabras del léxico.
    #[test]
    fn diccionario_no_pisa_palabras_del_lexico() {
        for (w, t) in [("Pronto", "hasta pronto"), ("Luna", "hay luna llena"), ("Cursor", "mueve el cursor"), ("Asana", "ya está sana"), ("Elisa", "es lisa"), ("Zara", "I met Sarah")] {
            assert_eq!(apply_dictionary(t, &[word(w, None)]), t, "{w}");
        }
        // Un nombre de pila sí se ajusta, y lo que no es una palabra se sigue corrigiendo.
        assert_eq!(apply_dictionary("hola, begona", &[word("Begoña", None)]), "hola, Begoña");
        assert_eq!(apply_dictionary("dile a txonso", &[word("Txonso", None)]), "dile a Txonso");
        assert!(is_real_phrase("está sana") && !is_real_phrase("está Chonzo") && !is_real_phrase("¿?"));
    }

    #[test]
    fn suena_igual_no_pisa_palabras_que_existen() {
        let w = [word_v("Ollama", None, &["Oyama"]), word("Zara", None), word("Baca", None)];
        assert_eq!(apply_dictionary("me llama y le llamaron, Sara y la vaca", &w), "me llama y le llamaron, Sara y la vaca");
        // Lo que no es una palabra sí se corrige.
        assert_eq!(apply_dictionary("instalé Yama", &w), "instalé Ollama");
        assert!(is_real_word("llamamos") && is_real_word("Sara") && is_real_word("vacas") && !is_real_word("Yama") && !is_real_word("Chonzo"));
    }

    #[test]
    fn marcadores_del_usuario_no_se_comen_texto() {
        let w = [word("Txonso", None)];
        let raw = "a \u{E000} b txonso c \u{E001}\u{E105}";
        assert_eq!(apply_dictionary(raw, &w), "a \u{E000} b Txonso c \u{E001}\u{E105}");
    }
}

// ---------------------------------------------------------------- formatos (BELTR-PRT-15)
//
// Correos, webs, teléfonos, símbolos, horas, euros y porcentajes dictados → escritos, con reglas
// locales y deterministas (ni red ni IA). Van ANTES del pulido: así la regla 7 (el pulido no añade
// cifras, enlaces ni cuentas) ve «ana@gmail.com» ya en el texto local y no lo toma por un invento.
// Trabajan sobre lo que escribe Parakeet de verdad, que ya pone muchas cifras («600123456», «15%»,
// «gmail.com») pero deja «arroba», «barra», «y media» o «asterisco» en palabras. Cada regla pide un
// contexto claro y deja el texto como estaba si duda («un asterisco», «la almohadilla nueva», «a las
// cinco»). Las cifras sueltas de una palabra («veintiuna líneas») se quedan en letra, como pide la RAE.
// Investigación (Wispr Flow, SuperWhisper, Dragon, RAE): `specs/features/pulido-en-directo.md`.

/// Dominios de primer nivel que se aceptan al juntar «punto com», «punto es»… (sin ellos, «punto» es
/// la palabra de siempre).
const TLDS: &[&str] = &[
    "com", "es", "net", "org", "app", "io", "dev", "eu", "info", "cat", "gal", "eus", "me", "co", "ai", "mx", "ar", "cl", "pe", "uy", "fr", "de",
    "it", "pt", "uk", "us", "edu", "gob", "gov", "tv", "xyz", "biz", "online", "shop", "store", "tech", "ws", "ly", "to", "so",
];

/// Lo que va delante de «la»/«las» cuando sigue una hora («a las cinco», «son las diez»).
const HOUR_LEAD: &[&str] = &["a", "hasta", "desde", "sobre", "de", "para", "hacia", "entre", "son", "es", "era", "eran", "y", "o", "antes", "después", "despues"];

/// TLD que también son palabras corrientes.
const AMBIGUOUS_TLDS: &[&str] = &["es", "de", "me", "co", "to", "so", "it", "eu", "tv", "ar", "cl", "pe"];

/// Lo que va delante de «almohadilla» o «asterisco» cuando son la palabra de siempre («una
/// almohadilla», «sin asterisco»): entonces no se cambian por el símbolo.
const DETERMINERS: &[&str] = &[
    "un", "una", "el", "la", "los", "las", "mi", "tu", "su", "mis", "tus", "sus", "esta", "este", "esa", "ese", "aquella", "aquel", "otra", "otro",
    "nueva", "nuevo", "sin", "con", "cada", "la", "lleva", "llevan", "tiene", "tienen", "poner", "pon", "quita", "quitar", "nuestra", "nuestro", "vuestra",
];

/// ¿Palabra corriente del castellano (léxico, palabras de función o determinantes)?
fn spanish_word(w: &str) -> bool {
    let k = fold(w);
    is_ordinary_word(w) || FUNCTION_WORDS.contains(&k.as_str()) || DETERMINERS.contains(&k.as_str()) || common_words().contains(k.as_str())
}

#[derive(Debug, Clone)]
struct Tok {
    pre: String,
    core: String,
    post: String,
}

impl Tok {
    fn new(raw: &str) -> Self {
        let lead: String = raw.chars().take_while(|c| "¿¡(«\"'“".contains(*c)).collect();
        let rest = &raw[lead.len()..];
        let tail_len: usize = rest.chars().rev().take_while(|c| ",;:!?)»\"'”….".contains(*c)).map(char::len_utf8).sum();
        let (core, post) = rest.split_at(rest.len() - tail_len);
        // «2.barra», «17.45», «gmail.com»: el punto de dentro es del token; el del final es puntuación.
        Self { pre: lead, core: core.to_string(), post: post.to_string() }
    }
    fn key(&self) -> String {
        fold(&self.core)
    }
    fn bare(&self) -> bool {
        self.pre.is_empty() && self.post.is_empty()
    }
    fn render(&self) -> String {
        format!("{}{}{}", self.pre, self.core, self.post)
    }
}

fn digits_only(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

fn is_tld(s: &str) -> bool {
    TLDS.contains(&fold(s).as_str())
}

/// ¿Parece un dominio («gmail.com», «elpaís.es», «www.x.com»)? Partes de letras/cifras/guiones
/// separadas por puntos y la última un TLD conocido.
fn looks_like_domain(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() >= 2
        && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_alphanumeric() || c == '-'))
        && is_tld(parts[parts.len() - 1])
        && !parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit()))
}

/// Un trozo de usuario o de dominio («maria», «lopez», «info-ventas», «maria.lopez»).
fn addr_piece(t: &Tok) -> bool {
    !t.core.is_empty() && t.core.chars().all(|c| c.is_alphanumeric() || "._-".contains(c)) && t.core.chars().any(char::is_alphanumeric)
}

/// Enlace o correo en minúsculas y sin tildes («Google.es» → «google.es», «elpaís.com» → «elpais.com»).
fn addr_case(s: &str) -> String {
    fold(s)
}

const UNITS: &[(&str, u64)] = &[
    ("cero", 0), ("un", 1), ("uno", 1), ("una", 1), ("dos", 2), ("tres", 3), ("cuatro", 4), ("cinco", 5), ("seis", 6), ("siete", 7), ("ocho", 8),
    ("nueve", 9), ("diez", 10), ("once", 11), ("doce", 12), ("trece", 13), ("catorce", 14), ("quince", 15), ("dieciseis", 16), ("diecisiete", 17),
    ("dieciocho", 18), ("diecinueve", 19), ("veinte", 20), ("veintiun", 21), ("veintiuno", 21), ("veintiuna", 21), ("veintidos", 22), ("veintitres", 23),
    ("veinticuatro", 24), ("veinticinco", 25), ("veintiseis", 26), ("veintisiete", 27), ("veintiocho", 28), ("veintinueve", 29),
];
const TENS: &[(&str, u64)] = &[("treinta", 30), ("cuarenta", 40), ("cincuenta", 50), ("sesenta", 60), ("setenta", 70), ("ochenta", 80), ("noventa", 90)];
const HUNDREDS: &[(&str, u64)] = &[
    ("cien", 100), ("ciento", 100), ("doscientos", 200), ("doscientas", 200), ("trescientos", 300), ("trescientas", 300), ("cuatrocientos", 400),
    ("cuatrocientas", 400), ("quinientos", 500), ("quinientas", 500), ("seiscientos", 600), ("seiscientas", 600), ("setecientos", 700),
    ("setecientas", 700), ("ochocientos", 800), ("ochocientas", 800), ("novecientos", 900), ("novecientas", 900),
];

fn lookup(t: &[(&str, u64)], k: &str) -> Option<u64> {
    t.iter().find(|(w, _)| *w == k).map(|(_, v)| *v)
}

/// Una cifra de una sola palabra del 0 al 9 («seis», «6»).
fn single_digit(k: &str) -> Option<u64> {
    if k.len() == 1 && digits_only(k) {
        return k.parse().ok();
    }
    lookup(UNITS, k).filter(|v| *v <= 9).filter(|_| k != "un" && k != "una")
}

/// Número en palabras o cifras desde `toks[i]` (claves ya en minúsculas y sin tildes). Para en la
/// primera palabra que no encaja y devuelve (valor, tokens usados). Solo mira el núcleo: una coma o
/// un punto pegados al último token lo cierran.
fn parse_number(toks: &[Tok], i: usize) -> Option<(u64, usize)> {
    let key = |j: usize| toks.get(j).map(Tok::key);
    let open = |j: usize| toks.get(j).is_some_and(|t| t.post.is_empty());
    let first = key(i)?;
    if digits_only(&first) && first.len() <= 6 {
        return Some((first.parse().ok()?, 1));
    }
    // Hasta 999.
    let below_1000 = |j: usize| -> Option<(u64, usize)> {
        let mut v = 0;
        let mut n = 0;
        let k = key(j)?;
        if let Some(h) = lookup(HUNDREDS, &k) {
            v += h;
            n += 1;
            if k == "cien" || !open(j) {
                return Some((v, n));
            }
        }
        let k = match key(j + n) {
            Some(k) => k,
            None => return (n > 0).then_some((v, n)),
        };
        if let Some(t) = lookup(TENS, &k) {
            v += t;
            n += 1;
            if open(j + n - 1) && key(j + n).as_deref() == Some("y") && open(j + n) {
                if let Some(u) = key(j + n + 1).and_then(|k| lookup(UNITS, &k)).filter(|u| (1..=9).contains(u)) {
                    v += u;
                    n += 2;
                }
            }
        } else if let Some(u) = lookup(UNITS, &k) {
            v += u;
            n += 1;
        }
        (n > 0).then_some((v, n))
    };
    let (mut v, mut n) = if first == "mil" { (1, 0) } else { below_1000(i)? };
    if key(i + n).as_deref() == Some("mil") && (n == 0 || open(i + n - 1)) {
        v *= 1000;
        n += 1;
        if open(i + n - 1) {
            if let Some((r, m)) = below_1000(i + n) {
                v += r;
                n += m;
            }
        }
    }
    (n > 0).then_some((v, n))
}

/// Pasa la lista a texto otra vez.
fn join_toks(toks: &[Tok]) -> String {
    toks.iter().map(Tok::render).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ")
}

/// Sustituye `toks[a..b]` por un token con `core`, el `pre` del primero y el `post` del último.
fn splice(toks: &mut Vec<Tok>, a: usize, b: usize, core: String) {
    let pre = toks[a].pre.clone();
    let post = toks[b - 1].post.clone();
    toks.splice(a..b, [Tok { pre, core, post }]);
}

/// Correos, webs, teléfonos, símbolos, horas, euros y % dictados → escritos (ver arriba).
pub fn format_spoken(text: &str) -> String {
    if text.trim().is_empty() {
        return text.to_string();
    }
    let mut toks: Vec<Tok> = text.split_whitespace().map(Tok::new).collect();
    plus_34(&mut toks);
    spelled_digits(&mut toks);
    urls(&mut toks);
    emails(&mut toks);
    phones(&mut toks);
    symbols(&mut toks);
    times(&mut toks);
    money_and_percent(&mut toks);
    join_toks(&toks)
}

/// «seis uno ocho cero» (4 o más cifras dichas una a una; con comas en medio, 7 o más) → «6180». Con
/// menos se queda en letra: «uno, dos, tres» es contar.
fn spelled_digits(toks: &mut Vec<Tok>) {
    let mut i = 0;
    while i < toks.len() {
        let mut j = i;
        let mut digits = String::new();
        while j < toks.len() && toks[j].pre.is_empty() && (j == i || toks[j - 1].post.is_empty() || toks[j - 1].post == ",") {
            let k = toks[j].key();
            let Some(d) = single_digit(&k).filter(|_| !digits_only(&k)) else { break };
            digits.push(char::from_digit(d as u32, 10).unwrap_or('0'));
            j += 1;
        }
        let commas = j > i && toks[i..j - 1].iter().any(|t| t.post == ",");
        if (j - i >= 4 && !commas) || j - i >= 7 {
            // Las comas de en medio se pierden: las agrupa luego `phones`.
            let post = toks[j - 1].post.clone();
            toks.splice(i..j, [Tok { pre: String::new(), core: digits, post }]);
        }
        i += 1;
    }
}

/// Webs: «https dos puntos barra barra», «www punto x punto com», «x punto es», «x.com barra y».
fn urls(toks: &mut Vec<Tok>) {
    // 1. «x punto y punto com» → «x.y.com» si acaba en un TLD conocido.
    let mut i = 0;
    while i + 2 < toks.len() {
        if toks[i].post.is_empty() && addr_piece(&toks[i]) && !digits_only(&toks[i].core) && toks[i + 1].key() == "punto" && toks[i + 1].bare() {
            let mut j = i;
            let mut parts = vec![toks[i].core.clone()];
            while j + 2 < toks.len() && toks[j + 1].key() == "punto" && toks[j + 1].bare() && toks[j].post.is_empty() && addr_piece(&toks[j + 2]) && toks[j + 2].pre.is_empty() {
                parts.push(toks[j + 2].core.clone());
                j += 2;
            }
            let cand = parts.join(".");
            // «es», «de», «me»… también son palabras: «este punto es», «el punto de venta». Con solo dos
            // trozos y uno de esos, el de delante tiene que ser un nombre raro («google punto es»).
            let ambiguous = j == i + 2 && AMBIGUOUS_TLDS.contains(&fold(&parts[1]).as_str()) && spanish_word(&parts[0]);
            if j > i && looks_like_domain(&cand) && !ambiguous {
                splice(toks, i, j + 1, addr_case(&cand));
            }
        }
        i += 1;
    }
    // 2. «https» + «dos puntos barra barra» (o lo que escribe Parakeet: «2.barra barra», «: //»).
    let mut i = 0;
    while i < toks.len() {
        let k = toks[i].key();
        if (k == "https" || k == "http") && toks[i].post.is_empty() {
            let mut joined = String::new();
            for n in 1..=4 {
                let Some(t) = toks.get(i + n) else { break };
                joined.push_str(&fold(&format!("{}{}", t.core, t.post)));
                let ok = ["dospuntosbarrabarra", "2.barrabarra", "2puntosbarrabarra", ":barrabarra", "://", "dospuntos//", ":/barra"].contains(&joined.as_str());
                if ok {
                    let scheme = format!("{k}://");
                    // Se pega al dominio que sigue.
                    if let Some(next) = toks.get(i + n + 1).filter(|t| t.pre.is_empty()) {
                        let core = format!("{scheme}{}", next.core);
                        let post = next.post.clone();
                        let pre = toks[i].pre.clone();
                        toks.splice(i..i + n + 2, [Tok { pre, core, post }]);
                    } else {
                        splice(toks, i, i + n + 1, scheme);
                    }
                    break;
                }
            }
        }
        i += 1;
    }
    // 3. Un dominio ya escrito va en minúsculas y sin tildes; «barra x» detrás → «/x».
    let mut i = 0;
    while i < toks.len() {
        let core = toks[i].core.clone();
        let (scheme, host) = core.split_once("://").map(|(s, h)| (format!("{s}://"), h.to_string())).unwrap_or((String::new(), core.clone()));
        let (host_only, path) = host.split_once('/').map(|(h, p)| (h.to_string(), format!("/{p}"))).unwrap_or((host.clone(), String::new()));
        if looks_like_domain(&host_only) && !core.contains('@') {
            let mut out = format!("{scheme}{}{path}", addr_case(&host_only));
            let mut j = i;
            // «barra economía» o «barraeconomía» pegado.
            while toks[j].post.is_empty() {
                let next = toks.get(j + 1);
                let Some(n) = next.filter(|t| t.pre.is_empty()) else { break };
                let nk = n.key();
                if nk == "barra" && n.post.is_empty() {
                    if let Some(seg) = toks.get(j + 2).filter(|t| t.pre.is_empty() && addr_piece(t)) {
                        out.push('/');
                        out.push_str(&addr_case(&seg.core));
                        j += 2;
                        continue;
                    }
                    break;
                } else if let Some(rest) = nk.strip_prefix("barra").filter(|r| r.len() >= 2 && r.chars().all(char::is_alphanumeric)) {
                    out.push('/');
                    out.push_str(rest);
                    j += 1;
                    continue;
                }
                break;
            }
            splice(toks, i, j + 1, out);
        }
        i += 1;
    }
}

/// Correos: «ana punto lopez arroba gmail punto com», «info guion ventas arroba x.es», «x arrobay.es».
fn emails(toks: &mut Vec<Tok>) {
    // Whisper pega la arroba con puntos: «Marta.ruiz.arroba.novalumen.com» (BELTR-PRT-30, voz de
    // Txonso). Si a la derecha hay un dominio, es un correo.
    for t in toks.iter_mut() {
        let low = t.core.to_ascii_lowercase();
        if let Some((p, n)) = [".arroba.", ".aroba."].iter().find_map(|a| low.find(a).map(|p| (p, a.len()))) {
            let (user, domain) = (&t.core[..p], &t.core[p + n..]);
            if !user.is_empty() && user.chars().all(|c| c.is_alphanumeric() || "._-".contains(c)) && looks_like_domain(&fold(domain)) {
                t.core = format!("{}@{}", addr_case(user), addr_case(domain));
            }
        }
    }
    // Whisper con «arroba» en el diccionario la oye, pero entre comas: «marta.ruiz, arroba,
    // novalumen.es» (BELTR-PRT-30). Si detrás viene un dominio (o «x punto y»), las comas sobran. «aroba»,
    // como la escribe a veces, vale igual.
    for i in 1..toks.len().saturating_sub(1) {
        let k = toks[i].key();
        if !(k == "arroba" || k == "aroba") || !toks[i].pre.is_empty() || !matches!(toks[i].post.as_str(), "" | ",") || !matches!(toks[i - 1].post.as_str(), "" | ",") {
            continue;
        }
        let sigue_dominio = addr_piece(&toks[i + 1])
            && toks[i + 1].pre.is_empty()
            && (looks_like_domain(&fold(&toks[i + 1].core)) || toks.get(i + 2).is_some_and(|t| t.key() == "punto"));
        if sigue_dominio && addr_piece(&toks[i - 1]) {
            toks[i].core = "arroba".into();
            toks[i].post.clear();
            toks[i - 1].post.clear();
        }
    }
    let mut i = 0;
    while i < toks.len() {
        let k = toks[i].key();
        // «arroba» suelto, «@» suelto o pegado delante del dominio («arrobaempresa.es»).
        let (is_at, glued) = if (k == "arroba" || k == "@") && toks[i].bare() {
            (true, None)
        } else if let Some(rest) = k.strip_prefix("arroba").filter(|r| looks_like_domain(r)).filter(|_| toks[i].pre.is_empty()) {
            (true, Some(rest.to_string()))
        } else {
            (false, None)
        };
        if !is_at || i == 0 {
            i += 1;
            continue;
        }
        // Usuario hacia atrás: palabra (conector palabra)*.
        let mut a = i - 1;
        if !(addr_piece(&toks[a]) && toks[a].post.is_empty()) {
            i += 1;
            continue;
        }
        let mut user = toks[a].core.clone();
        loop {
            if a >= 2 && toks[a - 1].bare() && toks[a - 2].post.is_empty() && addr_piece(&toks[a - 2]) {
                let c = toks[a - 1].key();
                let sep = match c.as_str() {
                    "punto" => Some("."),
                    "guion" => Some("-"),
                    _ => None,
                };
                if let Some(sep) = sep {
                    user = format!("{}{sep}{user}", toks[a - 2].core);
                    a -= 2;
                    continue;
                }
                // «guion bajo» / «barra baja».
                if a >= 3 && (c == "bajo" || c == "baja") && toks[a - 2].bare() && ["guion", "barra"].contains(&toks[a - 2].key().as_str()) && addr_piece(&toks[a - 3]) && toks[a - 3].post.is_empty() {
                    user = format!("{}_{user}", toks[a - 3].core);
                    a -= 3;
                    continue;
                }
            }
            break;
        }
        // Dominio hacia delante.
        let (mut domain, mut b) = match &glued {
            Some(d) => (d.clone(), i + 1),
            None => match toks.get(i + 1).filter(|t| t.pre.is_empty() && addr_piece(t)) {
                Some(t) => (t.core.clone(), i + 2),
                None => {
                    i += 1;
                    continue;
                }
            },
        };
        while toks[b - 1].post.is_empty() && b + 1 < toks.len() + 1 {
            let Some(c) = toks.get(b).filter(|t| t.bare()) else { break };
            let sep = match c.key().as_str() {
                "punto" => ".",
                "guion" => "-",
                _ => break,
            };
            let Some(n) = toks.get(b + 1).filter(|t| t.pre.is_empty() && addr_piece(t)) else { break };
            domain = format!("{domain}{sep}{}", n.core);
            b += 2;
        }
        // Dominio partido en dos palabras: «arroba nova lumen.com» → «novalumen.com» (BELTR-PRT-30).
        if !looks_like_domain(&domain) && toks[b - 1].post.is_empty() {
            if let Some(n) = toks.get(b).filter(|t| t.pre.is_empty() && addr_piece(t)).filter(|t| looks_like_domain(&fold(&format!("{domain}{}", t.core)))) {
                domain = format!("{domain}{}", n.core);
                b += 1;
            }
        }
        if looks_like_domain(&domain) {
            let core = format!("{}@{}", addr_case(&user), addr_case(&domain));
            splice(toks, a, b, core);
            i = a + 1;
        } else {
            i += 1;
        }
    }
}

/// «más treinta y cuatro» / «más 34» → «+34» (antes de juntar cifras dichas, que se comerían el 4).
fn plus_34(toks: &mut Vec<Tok>) {
    let mut i = 0;
    while i + 1 < toks.len() {
        if toks[i].key() == "mas" && toks[i].post.is_empty() {
            if let Some((34, n)) = parse_number(toks, i + 1) {
                // Solo si detrás viene el número (cifras o cifras dichas): «10 más 34» es una suma.
                let sigue = toks.get(i + 1 + n).is_some_and(|t| digits_only(&t.core.replace('-', "")) || single_digit(&t.key()).is_some());
                if sigue {
                    splice(toks, i, i + 1 + n, "+34".into());
                }
            }
        }
        i += 1;
    }
}

/// ¿Es un token que escribieron las reglas de formatos (correo, enlace, hora, «*101#», «#verano»)?
/// El pulido tiene que dejarlo tal cual (`polish::formats_kept`).
pub fn is_formatted_token(t: &str) -> bool {
    let time = t.split_once(':').is_some_and(|(h, m)| digits_only(h) && h.len() <= 2 && digits_only(m) && m.len() == 2);
    (t.contains('@') && t.contains('.')) || t.contains("://") || time || ((t.starts_with('*') || t.starts_with('#') || t.ends_with('#')) && t.chars().count() > 1)
}

/// Proveedores de correo conocidos: si Parakeet oye «otmail» o «jimeil» tras la arroba, se escriben bien.
const MAIL_PROVIDERS: &[&str] = &["gmail", "hotmail", "outlook", "yahoo", "icloud", "protonmail", "gmx", "live"];

/// Dominios casi iguales a uno conocido o a una palabra del diccionario («otmail.es» → «hotmail.es»,
/// «www.beltranabs.com» → «www.beltranlabs.com» si «BeltranLabs» está guardada). Solo dentro de un correo
/// o un enlace ya escrito, solo la parte de antes del TLD, y solo con una candidata clara: una letra de
/// diferencia, o dos que faltan («gitu» → «github»; «gitlab» no se toca).
pub fn fix_domains(text: &str, words: &[Word]) -> String {
    let proveedores: Vec<String> = MAIL_PROVIDERS.iter().map(|s| s.to_string()).collect();
    let mut cands: Vec<String> = proveedores.clone();
    for w in words {
        let v = w.replacement.clone().filter(|r| !r.trim().is_empty()).unwrap_or_else(|| w.word.clone());
        let k: String = fold(&v).chars().filter(|c| c.is_alphanumeric()).collect();
        if k.chars().count() >= 5 && !cands.contains(&k) {
            cands.push(k);
        }
    }
    let fix_label = |label: &str| -> Option<String> {
        if label.chars().count() < 3 || cands.iter().any(|c| c == label) {
            return None;
        }
        // Solo letras que faltan («otmail» → «hotmail», «beltranabs» → «beltranlabs»): un dominio real que
        // se parece («mail.com», «email.com», «lime.com») no se toca. Palabras guardadas: también una letra
        // cambiada si son largas. 5 letras o más (4 para las guardadas).
        let (n, lista_propia) = (label.chars().count(), |c: &String| !proveedores.contains(c));
        let mut best: Vec<(&String, usize)> = cands
            .iter()
            .map(|c| (c, key_distance(label, c)))
            .filter(|(c, d)| {
                let faltan = c.chars().count().saturating_sub(n);
                let solo_faltan = *d == faltan && (1..=2).contains(d);
                let largo = if lista_propia(c) { n >= 4 } else { n >= 5 };
                largo && (solo_faltan || (lista_propia(c) && *d == 1 && n >= 7))
            })
            .collect();
        best.sort_by_key(|x| x.1);
        match best.as_slice() {
            [one] => Some(one.0.clone()),
            [a, b, ..] if b.1 > a.1 => Some(a.0.clone()),
            _ => None,
        }
    };
    text.split(' ')
        .map(|tok| {
            let t = Tok::new(tok);
            let (head, host) = match t.core.split_once('@') {
                Some((u, h)) => (format!("{u}@"), h.to_string()),
                None => match t.core.split_once("://") {
                    Some((s, h)) => (format!("{s}://"), h.to_string()),
                    None => (String::new(), t.core.clone()),
                },
            };
            let (host_only, path) = host.split_once('/').map(|(h, p)| (h.to_string(), format!("/{p}"))).unwrap_or((host.clone(), String::new()));
            if !looks_like_domain(&host_only) || !host_only.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || ".-".contains(c)) {
                return tok.to_string();
            }
            let mut parts: Vec<String> = host_only.split('.').map(str::to_string).collect();
            let n = parts.len();
            let li = if n >= 3 && parts[0] == "www" { 1 } else { n - 2 };
            match fix_label(&parts[li]) {
                Some(f) => {
                    parts[li] = f;
                    format!("{}{head}{}{path}{}", t.pre, parts.join("."), t.post)
                }
                None => tok.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Teléfonos de 9 cifras (con o sin +34): «618-785-522», «91 456 7890», «600123456» → «618 785 522».
fn phones(toks: &mut Vec<Tok>) {
    let digit_tok = |t: &Tok| t.pre.is_empty() && !t.core.is_empty() && t.core.chars().all(|c| c.is_ascii_digit() || c == '-') && t.core.chars().any(|c| c.is_ascii_digit());
    let mut i = 0;
    while i < toks.len() {
        if !digit_tok(&toks[i]) {
            i += 1;
            continue;
        }
        let mut j = i;
        let mut digits = String::new();
        while j < toks.len() && digit_tok(&toks[j]) && (j == i || toks[j - 1].post.is_empty() || toks[j - 1].post == ",") {
            digits.extend(toks[j].core.chars().filter(char::is_ascii_digit));
            j += 1;
            if digits.len() >= 9 {
                break;
            }
        }
        let starts_ok = digits.starts_with(['6', '7', '8', '9']);
        if digits.len() == 9 && starts_ok && (j - i > 1 || toks[i].core.len() >= 9) {
            let g = format!("{} {} {}", &digits[0..3], &digits[3..6], &digits[6..9]);
            let prefixed = i > 0 && toks[i - 1].core == "+34" && toks[i - 1].post.is_empty() || i > 0 && toks[i - 1].core == "+34" && toks[i - 1].post == ",";
            if prefixed {
                splice(toks, i - 1, j, format!("+34 {g}"));
                i = j - 1;
            } else {
                splice(toks, i, j, g);
                i += 1;
            }
            continue;
        }
        i = j.max(i + 1);
    }
}

/// «asterisco 101 almohadilla» → «*101#», «almohadilla 2» → «#2», «la etiqueta almohadilla verano»
/// → «#verano». «Un asterisco» o «no lleva almohadilla.» se quedan en palabra.
fn symbols(toks: &mut Vec<Tok>) {
    let mut i = 0;
    while i < toks.len() {
        let k = toks[i].key();
        let prev_det = i > 0 && DETERMINERS.contains(&toks[i - 1].key().as_str());
        if k == "asterisco" && toks[i].post.is_empty() && !prev_det {
            if let Some(n) = toks.get(i + 1).filter(|t| t.pre.is_empty() && digits_only(&t.core)) {
                let mut core = format!("*{}", n.core);
                let mut end = i + 2;
                if n.post.is_empty() && toks.get(i + 2).is_some_and(|t| t.key() == "almohadilla" && t.pre.is_empty()) {
                    core.push('#');
                    end = i + 3;
                }
                splice(toks, i, end, core);
            }
        } else if k == "almohadilla" && toks[i].pre.is_empty() && !prev_det {
            // Detrás de cifras: «33 almohadilla» → «33#».
            if i > 0 && toks[i - 1].post.is_empty() && digits_only(toks[i - 1].core.trim_start_matches('*')) && !toks[i - 1].core.is_empty() {
                let core = format!("{}#", toks[i - 1].core);
                splice(toks, i - 1, i + 1, core);
                continue;
            }
            if toks[i].post.is_empty() {
                if let Some(n) = toks.get(i + 1).filter(|t| t.pre.is_empty()) {
                    let nk = n.key();
                    let word = n.core.chars().all(char::is_alphabetic) && n.core.chars().count() >= 3 && !FUNCTION_WORDS.contains(&nk.as_str());
                    if digits_only(&n.core) {
                        let core = format!("#{}", n.core);
                        splice(toks, i, i + 2, core);
                    } else if word {
                        let core = format!("#{}", n.core.to_lowercase());
                        splice(toks, i, i + 2, core);
                    }
                }
            }
        }
        i += 1;
    }
}

/// Horas: «a las cinco y media» → «a las 5:30», «a las 17.45» → «a las 17:45», «a las diez menos
/// cuarto» → «a las 9:45», «a las doce y diez» → «a las 12:10». Sin minutos («a las cinco») no cambia.
fn times(toks: &mut Vec<Tok>) {
    let mut i = 0;
    while i + 1 < toks.len() {
        let k = toks[i].key();
        // Solo «a las», «son las», «hasta las»…: «revisa la 1 y 2» o «las 3 y 4 son iguales» no son horas.
        let antes = if i > 0 && toks[i - 1].post.is_empty() { toks[i - 1].key() } else { String::new() };
        if !((k == "las" || k == "la") && toks[i].post.is_empty() && HOUR_LEAD.contains(&antes.as_str())) {
            i += 1;
            continue;
        }
        let h_at = i + 1;
        // «17.45» / «17:45» de una vez.
        let hc = toks[h_at].core.clone();
        if let Some((h, m)) = hc.split_once(['.', ':']) {
            if digits_only(h) && digits_only(m) && m.len() == 2 && h.len() <= 2 {
                let (hv, mv): (u32, u32) = (h.parse().unwrap_or(99), m.parse().unwrap_or(99));
                if hv <= 24 && mv <= 59 {
                    toks[h_at].core = format!("{hv}:{m}");
                }
            }
            i += 2;
            continue;
        }
        let Some((h, hn)) = parse_number(toks, h_at).filter(|(h, _)| (1..=24).contains(h)) else {
            i += 1;
            continue;
        };
        let after = h_at + hn;
        if !toks[after - 1].post.is_empty() || after >= toks.len() {
            i += 1;
            continue;
        }
        let key = |j: usize| toks.get(j).map(Tok::key).unwrap_or_default();
        let open = |j: usize| toks.get(j).is_some_and(|t| t.post.is_empty() && t.pre.is_empty());
        let mut found: Option<(u64, u64, usize)> = None; // (hora, minutos, fin)
        match key(after).as_str() {
            "y" if open(after) => match key(after + 1).as_str() {
                "media" => found = Some((h, 30, after + 2)),
                "cuarto" => found = Some((h, 15, after + 2)),
                _ => {
                    // «y diez», «y veinte», «y cuarenta y cinco»: minutos de reloj (de 5 en 5, o de dos cifras).
                    if let Some((m, mn)) = parse_number(toks, after + 1).filter(|(m, _)| (1..=59).contains(m) && (m % 5 == 0 || *m >= 10)) {
                        found = Some((h, m, after + 1 + mn));
                    }
                }
            },
            "menos" if open(after) => {
                let prev = if h == 1 { 12 } else { h - 1 };
                if key(after + 1) == "cuarto" {
                    found = Some((prev, 45, after + 2));
                } else if let Some((m, mn)) = parse_number(toks, after + 1).filter(|(m, _)| (1..=30).contains(m)) {
                    found = Some((prev, 60 - m, after + 1 + mn));
                }
            }
            "en" if open(after) && key(after + 1) == "punto" => found = Some((h, 0, after + 2)),
            _ => {
                // «diecisiete cuarenta y cinco»: minutos de dos cifras justo detrás (10..59).
                if let Some((m, mn)) = parse_number(toks, after).filter(|(m, _)| (10..=59).contains(m)) {
                    if !digits_only(&toks[after].core) || toks[after].core.len() == 2 {
                        found = Some((h, m, after + mn));
                    }
                }
            }
        }
        if let Some((hh, mm, end)) = found.filter(|(_, _, end)| *end <= toks.len()) {
            splice(toks, h_at, end, format!("{hh}:{mm:02}"));
            i = h_at + 1;
            continue;
        }
        i += 1;
    }
}

/// «3 euros con 50» → «3,50 euros», «tres coma cinco por ciento» → «3,5%», «quince por ciento» → «15%».
fn money_and_percent(toks: &mut Vec<Tok>) {
    let mut i = 0;
    while i < toks.len() {
        let Some((n, nn)) = parse_number(toks, i) else {
            i += 1;
            continue;
        };
        let after = i + nn;
        let key = |j: usize| toks.get(j).map(Tok::key).unwrap_or_default();
        let open = |j: usize| toks.get(j).is_some_and(|t| t.post.is_empty());
        if !open(after - 1) {
            i += 1;
            continue;
        }
        // Decimal dicho: «tres coma cinco».
        let mut value = n.to_string();
        let mut end = after;
        if key(after) == "coma" && open(after) {
            if let Some((d, dn)) = parse_number(toks, after + 1) {
                value = format!("{n},{d}");
                end = after + 1 + dn;
            }
        }
        let k1 = key(end);
        if (k1 == "euros" || k1 == "euro") && open(end) && key(end + 1) == "con" && open(end + 1) {
            if let Some((c, cn)) = parse_number(toks, end + 2).filter(|(c, _)| *c <= 99) {
                splice(toks, i, end + 2 + cn, format!("{n},{c:02} euros"));
                i += 1;
                continue;
            }
        }
        if k1 == "con" && open(end) && !value.contains(',') {
            if let Some((c, cn)) = parse_number(toks, end + 1).filter(|(c, _)| *c <= 99) {
                let e = end + 1 + cn;
                if (key(e) == "euros" || key(e) == "euro") && open(e - 1) {
                    splice(toks, i, e + 1, format!("{n},{c:02} euros"));
                    i += 1;
                    continue;
                }
            }
        }
        if k1 == "por" && open(end) && key(end + 1) == "ciento" {
            splice(toks, i, end + 2, format!("{value}%"));
            i += 1;
            continue;
        }
        if end > after && value.contains(',') && (k1 == "euros" || k1 == "%") {
            splice(toks, i, end, value);
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests_formatos {
    use super::format_spoken as f;

    /// Lo que escribe Parakeet en el banco de formatos (tests/banco-formatos) y lo que debe salir.
    #[test]
    fn escribe_correos_webs_telefonos_simbolos_horas_euros_y_porcentajes() {
        for (oido, bien) in [
            ("Escríbeme alfonso arroba gmail.com", "Escríbeme alfonso@gmail.com"),
            ("Mi correo es maria punto lopez arroba hotmail punto es.", "Mi correo es maria.lopez@hotmail.es."),
            ("Mi correo es maria.lopez arroba hotmail.es", "Mi correo es maria.lopez@hotmail.es"),
            ("El correo es info guion ventas arroba empresa punto es.", "El correo es info-ventas@empresa.es."),
            ("El correo es info-ventas arrobaempresa.es.", "El correo es info-ventas@empresa.es."),
            ("Contesta a juan guion bajo perez arroba yahoo punto es", "Contesta a juan_perez@yahoo.es"),
            ("Pon soporte arroba pronto punto app, gracias", "Pon soporte@pronto.app, gracias"),
            // Voz de Txonso con Whisper y Parakeet (BELTR-PRT-30, 20-nov-2026).
            ("Marta.ruiz.arroba.novalumen.com", "marta.ruiz@novalumen.com"),
            ("Marta.arroba.gmail.com", "marta@gmail.com"),
            ("Marta punto Ruiz arroba nova lumen.com", "marta.ruiz@novalumen.com"),
            ("Escríbeme a ana arroba mi empresa.es, gracias", "Escríbeme a ana@miempresa.es, gracias"),
            ("Marta.ruiz, arroba, novalumen.es", "marta.ruiz@novalumen.es"),
            ("Marta.ruiz.aroba.novalumen.es y marta.aroba.gmail.com", "marta.ruiz@novalumen.es y marta@gmail.com"),
            ("Escribe a ana, arroba, gmail punto com.", "Escribe a ana@gmail.com."),
            ("Mi teléfono es 618-785-522", "Mi teléfono es 618 785 522"),
            ("Llámame al 600123456", "Llámame al 600 123 456"),
            ("El fijo es 91 456 7890", "El fijo es 914 567 890"),
            ("Llámame al seis cero cero uno dos tres cuatro cinco seis.", "Llámame al 600 123 456."),
            ("Mi móvil es 618, 785, 522.", "Mi móvil es 618 785 522."),
            ("El número es más treinta y cuatro 618 785 522", "El número es +34 618 785 522"),
            ("El código postal es cuatro ocho cero cero uno.", "El código postal es 48001."),
            ("Para ver el saldo marca asterisco 101 almohadilla", "Para ver el saldo marca *101#"),
            ("Marca asterisco 33 Almohadilla.", "Marca *33#."),
            ("Después pulsa Almohadilla 2", "Después pulsa #2"),
            ("Usa la etiqueta Almohadilla Verano", "Usa la etiqueta #verano"),
            ("Entra en www punto beltranlabs punto com", "Entra en www.beltranlabs.com"),
            ("La web es pronto.app barra descargas.", "La web es pronto.app/descargas."),
            ("Mira Google.es", "Mira google.es"),
            ("Visita elpaís.com barraeconomia.", "Visita elpais.com/economia."),
            ("Abre https 2.barra barra github.com", "Abre https://github.com"),
            ("Abre https dos puntos barra barra github punto com", "Abre https://github.com"),
            ("La reunión es a las 5 y media", "La reunión es a las 5:30"),
            ("La reunión es a las cinco y media.", "La reunión es a las 5:30."),
            ("El tren sale a las 17.45.", "El tren sale a las 17:45."),
            ("El tren sale a las diecisiete cuarenta y cinco", "El tren sale a las 17:45"),
            ("Te llamo a las nueve y cuarto", "Te llamo a las 9:15"),
            ("Abrimos a las 10 menos cuarto", "Abrimos a las 9:45"),
            ("La cita es a las doce y diez", "La cita es a las 12:10"),
            ("Quedamos a la una y media", "Quedamos a la 1:30"),
            ("Llego a las 8 en punto", "Llego a las 8:00"),
            ("Cuesta 3 euros con 50", "Cuesta 3,50 euros"),
            ("Cuesta tres euros con cincuenta.", "Cuesta 3,50 euros."),
            ("La cuota es de cuarenta y nueve con noventa y nueve euros", "La cuota es de 49,99 euros"),
            ("Tenemos un descuento del quince por ciento", "Tenemos un descuento del 15%"),
            ("Ha subido un tres coma cinco por ciento.", "Ha subido un 3,5%."),
            ("El presupuesto es de mil doscientos cincuenta por ciento", "El presupuesto es de 1250%"),
        ] {
            assert_eq!(f(oido), bien, "entrada: {oido}");
        }
    }

    /// Trampas: texto corriente que no debe cambiar (la regla duda → lo deja).
    #[test]
    fn no_toca_texto_corriente() {
        for t in [
            "La nota lleva un asterisco al final.",
            "Mi contraseña no lleva almohadilla.",
            "Compré una almohadilla nueva para el sofá.",
            "Pon el asterisco donde quieras.",
            "Quedamos a las cinco en la plaza.",
            "Wow, Matt Pocock, veintiuna líneas with Matt skills.",
            "Uno, dos, tres, probando.",
            "Tengo 3 hijos y 2 perros.",
            "En este punto es donde fallamos.",
            "El punto de venta abre a las diez.",
            "Lo hicimos en 2024 con 15 personas.",
            "Vamos a ver cómo funciona ahora. Me cago en diez, funciona de locos.",
            "Ese es un punto importante, la verdad.",
            "La barra del bar estaba llena.",
            "Es la una.",
            "Envíalo con arroba al final, no sé.",
            "Su DNI es 12345678.",
            "Pásate a las dos o a las tres.",
            "Cuesta mil euros.",
            "Te lo digo por ciento y una vez.",
            "Llego a las cinco y diez minutos después empezamos.",
            "Las 3 y 4 son iguales.",
            "Revisa la 1 y 2 antes de enviar.",
            "Uno, dos, tres, cuatro, cinco, seis.",
            "Son 10 más 34 en total.",
            "Escríbeme a ana@mail.com o a info@email.com.",
            "Mira lime.com y live.com.",
        ] {
            let fuera = f(t);
            // La última trampa sí lleva hora: lo que se comprueba es que no se coma «minutos».
            if t.starts_with("Llego a las cinco y diez") {
                assert_eq!(fuera, "Llego a las 5:10 minutos después empezamos.");
                continue;
            }
            assert_eq!(fuera, t, "cambió texto corriente");
        }
    }

    #[test]
    fn arregla_dominios_casi_iguales_a_uno_conocido() {
        use crate::banco_entender::leer_diccionario;
        let w = leer_diccionario("BeltranLabs
Gihad => GitHub
");
        let g = |t: &str| super::fix_domains(t, &w);
        assert_eq!(g("Mi correo es maria.lopez@otmail.es"), "Mi correo es maria.lopez@hotmail.es");
        assert_eq!(g("Entra en www.beltranabs.com."), "Entra en www.beltranlabs.com.");
        assert_eq!(g("Manda a facturas@beltralabs.com"), "Manda a facturas@beltranlabs.com");
        assert_eq!(g("Abre https://gitu.com"), "Abre https://github.com");
        // Sin parecido claro o ya conocido: igual.
        for t in ["Mira google.es", "Escribe a ana@gmail.com", "Abre gitlab.com", "La casa de beltran es grande", "Visita elpais.com/economia"] {
            assert_eq!(g(t), t);
        }
    }

    #[test]
    fn respeta_signos_de_alrededor() {
        assert_eq!(f("¿Me escribes a ana arroba gmail punto com?"), "¿Me escribes a ana@gmail.com?");
        assert_eq!(f("(llama al 600 123 456)"), "(llama al 600 123 456)");
        assert_eq!(f(""), "");
    }
}
