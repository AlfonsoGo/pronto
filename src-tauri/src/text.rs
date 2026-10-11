//! Tubería de texto: limpieza -> retroceso -> diccionario -> snippets -> estilo.
//!
//! Todo es determinista y local. Cada paso es una función pública para poder probarlo suelto;
//! `process` los encadena en el orden del contrato.

use crate::models::{CleanupLevel, Snippet, Tone, Word};
use regex::{NoExpand, Regex, RegexBuilder};

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

pub fn process(raw: &str, o: &Options) -> Processed {
    let mut out = Processed::default();
    let (mut text, fillers) = clean_counted(raw, o.cleanup);
    out.fixes_fillers = fillers;
    if o.cleanup >= CleanupLevel::Medium {
        text = apply_backtrack(&text);
    }
    if o.press_enter {
        let (t, pressed) = strip_press_enter(&text);
        text = t;
        out.press_enter = pressed;
    }
    let mut protected: Vec<String> = Vec::new();
    text = dictionary_inner(&text, o.words, &mut protected, &mut out.fixes_dictionary);
    text = snippets_inner(&text, o.snippets, &mut protected, &mut out.fixes_snippets);
    text = apply_style(&text, o.tone);
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
pub fn strip_press_enter(text: &str) -> (String, bool) {
    if press_enter_re().is_match(text) {
        (press_enter_re().replace(text, "").trim().to_string(), true)
    } else {
        (text.to_string(), false)
    }
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
            r"\b(?P<t>no,?\s+mejor|mejor\s+dicho|no,?\s+quiero\s+decir|quiero\s+decir|scratch\s+that|borra\s+eso|no,?\s+wait)\b[\s,.;:]*",
        )
        .case_insensitive(true)
        .build()
        .expect("regex retroceso")
    })
}

/// «No, mejor…» / «quiero decir» / «scratch that» descartan la frase anterior (desde el último
/// signo de fin de frase) y el texto continúa con lo que viene después.
pub fn apply_backtrack(text: &str) -> String {
    let mut current = text.to_string();
    // Cada vuelta elimina un disparador; el tope evita bucles si algo raro se repite.
    for _ in 0..16 {
        let Some(m) = backtrack_re().captures(&current).and_then(|c| c.get(0).map(|m| (m.start(), m.end()))) else {
            break;
        };
        let (start, end) = m;
        let before = current[..start].trim_end().trim_end_matches([',', ';', ':']).trim_end().to_string();
        let after = current[end..].trim().to_string();
        let trigger = current[start..end].to_lowercase();

        if before.is_empty() {
            // «No, mejor no» a pelo es una respuesta legítima; el resto de disparadores se quitan.
            if trigger.starts_with("no") && trigger.contains("mejor") {
                break;
            }
            current = capitalize_first(&after);
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
        current = if keep.is_empty() {
            capitalize_first(&after)
        } else if after.is_empty() {
            keep
        } else {
            format!("{} {}", keep, capitalize_first(&after))
        };
    }
    current.trim().to_string()
}

// ---------------------------------------------------------------- 3. diccionario

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Expresión «palabra completa» e insensible a mayúsculas para `phrase`.
fn whole_word_re(phrase: &str) -> Option<Regex> {
    let parts: Vec<String> = phrase.split_whitespace().map(regex::escape).collect();
    if parts.is_empty() {
        return None;
    }
    let first_word = phrase.chars().next().is_some_and(is_word_char);
    let last_word = phrase.chars().last().is_some_and(is_word_char);
    let pat = format!(
        "{}{}{}",
        if first_word { r"\b" } else { "" },
        parts.join(r"\s+"),
        if last_word { r"\b" } else { "" }
    );
    RegexBuilder::new(&pat).case_insensitive(true).build().ok()
}

/// El índice se codifica con caracteres privados (no con dígitos) para que ninguna entrada del
/// diccionario, por rara que sea, pueda casar dentro de un marcador.
fn placeholder(idx: usize) -> String {
    let digits: String = idx.to_string().chars().filter_map(|d| d.to_digit(10)).filter_map(|d| char::from_u32(0xE100 + d)).collect();
    format!("{PH_OPEN}{digits}{PH_CLOSE}")
}

fn dictionary_inner(text: &str, words: &[Word], protected: &mut Vec<String>, fixes: &mut usize) -> String {
    // Las frases más largas primero para que «san francisco» gane a «san».
    let mut entries: Vec<(&str, &str)> = words
        .iter()
        .filter_map(|w| match w.replacement.as_deref() {
            Some(r) if !r.trim().is_empty() && !w.word.trim().is_empty() => Some((w.word.trim(), r)),
            _ => None,
        })
        .collect();
    entries.sort_by_key(|(w, _)| std::cmp::Reverse(w.chars().count()));
    let mut out = text.to_string();
    for (word, repl) in entries {
        if let Some(re) = whole_word_re(word) {
            let n = re.find_iter(&out).count();
            if n > 0 {
                *fixes += n;
                protected.push(repl.to_string());
                let ph = placeholder(protected.len() - 1);
                out = re.replace_all(&out, NoExpand(&ph)).into_owned();
            }
        }
    }
    out
}

/// Reemplazos del diccionario (palabra completa, insensible a mayúsculas). Paso suelto, para tests.
#[allow(dead_code)]
pub fn apply_dictionary(text: &str, words: &[Word]) -> String {
    let mut protected = Vec::new();
    let t = dictionary_inner(text, words, &mut protected, &mut 0);
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

fn snippets_inner(text: &str, snippets: &[Snippet], protected: &mut Vec<String>, fixes: &mut usize) -> String {
    let valid: Vec<&Snippet> = snippets
        .iter()
        .filter(|s| !normalize_trigger(&s.trigger).is_empty() && !s.expansion.is_empty())
        .collect();
    // Frase completa: «mi correo.» -> expansión exacta, sin punto.
    let whole = normalize_trigger(text);
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
        if let Some(re) = whole_word_re(trigger) {
            let n = re.find_iter(&out).count();
            if n > 0 {
                *fixes += n;
                protected.push(s.expansion.clone());
                let ph = placeholder(protected.len() - 1);
                out = re.replace_all(&out, NoExpand(&ph)).into_owned();
            }
        }
    }
    out
}

/// Sustituye disparadores de snippets por su expansión. Paso suelto, para tests.
#[allow(dead_code)]
pub fn apply_snippets(text: &str, snippets: &[Snippet]) -> String {
    let mut protected = Vec::new();
    let t = snippets_inner(text, snippets, &mut protected, &mut 0);
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
/// salvo la última, que también se exclama.
fn exclaim(s: &str) -> String {
    fn flush(sentence: &mut String, out: &mut String, last: bool) {
        let trimmed = sentence.trim().to_string();
        sentence.clear();
        if trimmed.is_empty() {
            return;
        }
        let words = trimmed.split_whitespace().count();
        let body = match trimmed.strip_suffix('.').filter(|b| !b.ends_with('.')) {
            Some(b) if words <= 10 || last => format!("{b}!"),
            Some(_) => trimmed.clone(),
            None if last && trimmed.chars().last().is_some_and(|c| c.is_alphanumeric() || c == PH_CLOSE) => format!("{trimmed}!"),
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

    fn word(w: &str, r: Option<&str>) -> Word {
        Word { id: 0, word: w.into(), replacement: r.map(String::from), created_at: String::new(), starred: false, source: "manual".into() }
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
    fn diccionario_sin_reemplazo_se_ignora() {
        let w = [word("Kubernetes", None)];
        assert_eq!(apply_dictionary("kubernetes", &w), "kubernetes");
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
        assert_eq!(process("quedamos a las dos. no, mejor a las tres", &f).text, "A las tres.");
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
        assert_eq!(process("Hola. Quedamos el lunes. No, mejor el martes", &o).text, "Hola. El martes");
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
}
