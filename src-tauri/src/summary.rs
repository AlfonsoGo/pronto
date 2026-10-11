//! Resumen y preguntas sobre reuniones: prompts, parseo tolerante del JSON de Ollama, idioma de
//! la reunión, contexto para «Ask» y exportación a Markdown. Todo son funciones puras (el
//! contacto con Ollama lo hace `meeting.rs`), por eso es fácil de probar.

use crate::models::{ActionItem, MeetingDetail, Segment};
use regex::Regex;
use serde_json::Value;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedAction {
    pub text: String,
    pub owner: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedSummary {
    pub title: Option<String>,
    pub summary: String,
    pub action_items: Vec<ParsedAction>,
    pub decisions: Vec<String>,
    pub topics: Vec<String>,
}

// ------------------------------------------------------------------ idioma

const ES_WORDS: &[&str] = &[
    "de", "la", "que", "el", "en", "y", "los", "se", "del", "las", "un", "por", "con", "para", "una", "su", "al", "lo", "como", "más",
    "pero", "ya", "este", "sí", "porque", "esta", "muy", "sin", "sobre", "también", "hay", "todo", "nos", "entonces", "vale", "bueno",
];
const EN_WORDS: &[&str] = &[
    "the", "and", "to", "of", "a", "in", "that", "is", "it", "for", "was", "on", "with", "as", "be", "at", "this", "have", "from",
    "or", "an", "they", "you", "we", "are", "so", "but", "not", "what", "all", "can", "will", "just", "okay", "yeah", "about",
];

/// `"es"` o `"en"` según qué palabras vacías abunden más (por defecto, español).
pub fn detect_language(text: &str) -> &'static str {
    let (mut es, mut en) = (0, 0);
    for w in text.split(|c: char| !c.is_alphabetic()) {
        let w = w.to_lowercase();
        if ES_WORDS.contains(&w.as_str()) {
            es += 1;
        }
        if EN_WORDS.contains(&w.as_str()) {
            en += 1;
        }
    }
    if en > es { "en" } else { "es" }
}

// ------------------------------------------------------------------ transcripción

pub fn format_clock(ms: i64) -> String {
    let s = (ms / 1000).max(0);
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

pub fn speaker_label(speaker: &str, lang: &str) -> &'static str {
    match (speaker, lang) {
        ("me", "en") => "Me",
        ("me", _) => "Tú",
        (_, "en") => "Them",
        _ => "Ellos",
    }
}

/// `[01:02] Tú: texto`, una línea por segmento.
pub fn transcript_lines(segments: &[Segment], lang: &str) -> Vec<String> {
    segments.iter().map(|s| format!("[{}] {}: {}", format_clock(s.start_ms), speaker_label(&s.speaker, lang), s.text)).collect()
}

/// Líneas para el modelo: el usuario va con su nombre y el resto como «Otro participante». Con
/// «Tú»/«Ellos» el modelo escribía «la realice Tú» y perdía las tareas de la otra parte (medido
/// con qwen2.5:7b en una reunión real).
pub fn prompt_lines(segments: &[Segment], lang: &str, me: &str) -> Vec<String> {
    let other = if lang == "en" { "Other participant" } else { "Otro participante" };
    segments
        .iter()
        .map(|s| format!("[{}] {}: {}", format_clock(s.start_ms), if s.speaker == "me" { me } else { other }, s.text))
        .collect()
}

/// Agrupa líneas en trozos de como mucho `max_chars` (sin partir una línea).
pub fn chunk_lines(lines: &[String], max_chars: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut cur = String::new();
    for l in lines {
        if !cur.is_empty() && cur.len() + l.len() + 1 > max_chars {
            chunks.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push('\n');
        }
        cur.push_str(l);
    }
    if !cur.is_empty() {
        chunks.push(cur);
    }
    chunks
}

// ------------------------------------------------------------------ prompts

/// Prompt del resumen estructurado, en el idioma de la reunión. `me` es el nombre del usuario.
pub fn summary_prompt(lang: &str, transcript: &str, partial_notes: bool, me: &str) -> String {
    if lang == "en" {
        format!(
            "You take notes for meetings. Below is {what}. \"{me}\" is the user (the person taking these notes) and \"Other participant\" is everyone else.
             Reply ONLY with a JSON object of exactly this shape: {{\"title\": \"short title\", \"summary\": \"3 to 5 sentence summary\",              \"action_items\": [{{\"text\": \"task\", \"owner\": \"person responsible or null\"}}], \"decisions\": [\"decision\"], \"topics\": [\"topic\"]}}.
             Rules: write everything in English; refer to the user as \"{me}\", never as \"me\" or \"you\".              action_items must contain EVERY commitment made by ANY participant (\"I'll send\", \"you take care of\", \"I'll prepare\"...),              with the owner's name if it is said, \"{me}\" if it is the user's, or \"Other participant\".              Do not say in the summary that there are no tasks if action_items is not empty. Do not invent anything; use empty lists if there is nothing.

{transcript}",
            what = if partial_notes { "a set of partial notes of a long meeting" } else { "the transcript of a meeting" },
        )
    } else {
        format!(
            "Eres un asistente que toma notas de reuniones. A continuación tienes {what}. \"{me}\" es el usuario (quien toma estas notas) y \"Otro participante\" es el resto.
             Responde SOLO con un objeto JSON con esta forma exacta: {{\"title\": \"título corto\", \"summary\": \"resumen de 3 a 5 frases\",              \"action_items\": [{{\"text\": \"tarea\", \"owner\": \"responsable o null\"}}], \"decisions\": [\"decisión\"], \"topics\": [\"tema\"]}}.
             Reglas: escribe todo en español; refiérete al usuario como \"{me}\", nunca como \"Tú\".              action_items debe recoger TODOS los compromisos de CUALQUIER participante (\"yo mando\", \"te encargas tú\", \"preparo\"...),              con el nombre del responsable si se dice, \"{me}\" si es del usuario o \"Otro participante\".              No digas en el resumen que no hay tareas si action_items no está vacío. No inventes nada; usa listas vacías si no hay.

{transcript}",
            what = if partial_notes { "un conjunto de notas parciales de una reunión larga" } else { "la transcripción de una reunión" },
        )
    }
}

/// Prompt para resumir un trozo de una reunión muy larga (paso intermedio, texto libre).
pub fn chunk_prompt(lang: &str, chunk: &str) -> String {
    if lang == "en" {
        format!("Summarize this part of a meeting as concise bullet points: key points, tasks (with owner if mentioned) and decisions. Write in English and do not invent anything.\n\n{chunk}")
    } else {
        format!("Resume este fragmento de una reunión en viñetas concisas: puntos clave, tareas (con responsable si se dice) y decisiones. Escribe en español y no inventes nada.\n\n{chunk}")
    }
}

pub fn ask_prompt(question: &str, context: &str) -> String {
    format!(
        "Eres el asistente de notas de reuniones del usuario. Responde a la pregunta usando SOLO la información de las reuniones de abajo. \
         Responde en el mismo idioma de la pregunta, de forma breve y directa. Si la respuesta no está en las reuniones, dilo claramente.\n\n\
         === REUNIONES ===\n{context}\n=== FIN ===\n\nPregunta: {question}"
    )
}

// ------------------------------------------------------------------ parseo tolerante

fn trailing_comma_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r",\s*([}\]])").expect("regex comas"))
}

/// Primer objeto `{…}` del texto; si el texto se corta (respuesta truncada) lo cierra a mano.
fn first_object(text: &str, repair: bool) -> Option<String> {
    let start = text.find('{')?;
    let mut stack: Vec<char> = Vec::new();
    let (mut in_str, mut esc) = (false, false);
    for (i, c) in text[start..].char_indices() {
        if in_str {
            match (esc, c) {
                (true, _) => esc = false,
                (false, '\\') => esc = true,
                (false, '"') => in_str = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' => stack.push('}'),
            '[' => stack.push(']'),
            '}' | ']' => {
                stack.pop();
                if stack.is_empty() {
                    return Some(text[start..start + i + 1].to_string());
                }
            }
            _ => {}
        }
    }
    if !repair {
        return None;
    }
    let mut out = text[start..].trim_end().trim_end_matches(',').to_string();
    if in_str {
        out.push('"');
    }
    while let Some(closer) = stack.pop() {
        out.push(closer);
    }
    Some(out)
}

fn candidates(raw: &str) -> Vec<String> {
    let mut out = vec![raw.trim().to_string()];
    // Bloques ```json … ```
    let mut rest = raw;
    while let Some(i) = rest.find("```") {
        let after = &rest[i + 3..];
        let after = after.strip_prefix("json").or_else(|| after.strip_prefix("JSON")).unwrap_or(after);
        match after.find("```") {
            Some(j) => {
                out.push(after[..j].trim().to_string());
                rest = &after[j + 3..];
            }
            None => {
                out.push(after.trim().to_string());
                break;
            }
        }
    }
    if let Some(o) = first_object(raw, false) {
        out.push(o);
    }
    if let Some(o) = first_object(raw, true) {
        out.push(o);
    }
    out
}

fn text_of(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.trim().to_string()).filter(|s| !s.is_empty()),
        Value::Number(n) => Some(n.to_string()),
        Value::Array(a) => {
            let parts: Vec<String> = a.iter().filter_map(text_of).collect();
            Some(parts.join(" ")).filter(|s| !s.is_empty())
        }
        _ => None,
    }
}

fn pick<'a>(obj: &'a serde_json::Map<String, Value>, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|k| obj.iter().find(|(name, _)| name.eq_ignore_ascii_case(k)).map(|(_, v)| v))
}

fn string_list(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Array(a)) => a.iter().filter_map(text_of).collect(),
        Some(Value::String(s)) => s.lines().map(|l| l.trim().trim_start_matches(['-', '•', '*']).trim().to_string()).filter(|l| !l.is_empty()).collect(),
        _ => Vec::new(),
    }
}

fn action_list(v: Option<&Value>) -> Vec<ParsedAction> {
    let items: Vec<&Value> = match v {
        Some(Value::Array(a)) => a.iter().collect(),
        _ => return Vec::new(),
    };
    items
        .into_iter()
        .filter_map(|item| match item {
            Value::String(s) => Some(ParsedAction { text: s.trim().to_string(), owner: None }).filter(|a| !a.text.is_empty()),
            Value::Object(o) => {
                let text = text_of(pick(o, &["text", "task", "tarea", "description", "descripcion", "descripción", "action"])?)?;
                let owner = pick(o, &["owner", "responsable", "assignee", "assigned_to", "who"])
                    .and_then(text_of)
                    .filter(|s| !matches!(s.to_lowercase().as_str(), "null" | "none" | "n/a" | "ninguno" | "nadie" | "-"));
                Some(ParsedAction { text, owner })
            }
            _ => None,
        })
        .collect()
}

/// Interpreta la respuesta de Ollama aunque venga con texto alrededor, en un bloque de código,
/// con comas sobrantes o cortada a medias. Devuelve `None` si no hay nada aprovechable.
pub fn parse_summary(raw: &str) -> Option<ParsedSummary> {
    for cand in candidates(raw) {
        let cleaned = trailing_comma_re().replace_all(&cand, "$1").into_owned();
        let Ok(Value::Object(obj)) = serde_json::from_str::<Value>(&cleaned) else { continue };
        let summary = pick(&obj, &["summary", "resumen"]).and_then(text_of).unwrap_or_default();
        let parsed = ParsedSummary {
            title: pick(&obj, &["title", "titulo", "título"]).and_then(text_of),
            summary,
            action_items: action_list(pick(&obj, &["action_items", "actionitems", "tareas", "tasks", "actions", "acciones"])),
            decisions: string_list(pick(&obj, &["decisions", "decisiones"])),
            topics: string_list(pick(&obj, &["topics", "temas"])),
        };
        if !parsed.summary.is_empty() || !parsed.action_items.is_empty() || !parsed.decisions.is_empty() || !parsed.topics.is_empty() {
            return Some(parsed);
        }
    }
    None
}

// ------------------------------------------------------------------ contexto de Ask

/// Una reunión lista para entrar en el contexto de una pregunta.
pub struct AskMeeting {
    pub title: String,
    pub date: String,
    pub summary: Option<String>,
    pub lines: Vec<String>,
}

/// Arma el texto de contexto sin pasarse de `budget` caracteres (~4 por token). Con `words`
/// (palabras de la pregunta) solo entran las líneas que las mencionan; sin ellas, la transcripción
/// completa hasta agotar el presupuesto.
pub fn build_context(meetings: &[AskMeeting], words: &[String], budget: usize) -> String {
    let mut out = String::new();
    for m in meetings {
        let mut block = format!("## {} ({})\n", m.title, m.date);
        if let Some(s) = m.summary.as_deref().filter(|s| !s.is_empty()) {
            block.push_str(&format!("Resumen: {s}\n"));
        }
        block.push_str("Transcripción:\n");
        let relevant = m.lines.iter().filter(|l| {
            words.is_empty() || {
                let low = l.to_lowercase();
                words.iter().any(|w| low.contains(w.as_str()))
            }
        });
        for l in relevant {
            if out.len() + block.len() + l.len() + 1 > budget {
                break;
            }
            block.push_str(l);
            block.push('\n');
        }
        if out.len() + block.len() > budget {
            // Ni siquiera cabe la cabecera con el resumen: se recorta el resumen.
            let room = budget.saturating_sub(out.len());
            block = block.chars().take(room).collect();
        }
        out.push_str(&block);
        out.push('\n');
        if out.len() >= budget {
            break;
        }
    }
    out
}

// ------------------------------------------------------------------ Markdown

const MONTHS_ES: [&str; 12] = ["ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic"];

/// `2026-10-11T10:32:00+02:00` -> `11 oct 2026, 10:32`. Si no se puede leer, el texto original.
pub fn pretty_date(iso: &str) -> String {
    match chrono::DateTime::parse_from_rfc3339(iso) {
        Ok(d) => {
            use chrono::Datelike;
            format!("{} {} {}, {}", d.day(), MONTHS_ES[d.month0() as usize], d.year(), d.format("%H:%M"))
        }
        Err(_) => iso.to_string(),
    }
}

pub fn pretty_duration(ms: i64) -> String {
    let s = (ms / 1000).max(0);
    if s >= 3600 {
        format!("{} h {:02} min", s / 3600, (s % 3600) / 60)
    } else if s >= 60 {
        format!("{} min", s / 60)
    } else {
        format!("{s} s")
    }
}

/// Reunión completa en Markdown: título, fecha, resumen, tareas, decisiones, temas y transcripción.
pub fn to_markdown(d: &MeetingDetail) -> String {
    let m = &d.meeting;
    let transcript_text: String = d.segments.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ");
    let lang = detect_language(&format!("{} {}", d.summary.as_deref().unwrap_or(""), transcript_text));
    let mut meta = vec![pretty_date(&m.started_at), pretty_duration(m.duration_ms)];
    if let Some(app) = m.app.as_deref().filter(|a| !a.is_empty()) {
        meta.push(app.to_string());
    }
    let mut out = format!("# {}\n\n*{}*\n", m.title, meta.join(" · "));
    if let Some(s) = d.summary.as_deref().filter(|s| !s.is_empty()) {
        out.push_str(&format!("\n## Resumen\n\n{s}\n"));
    }
    if !d.action_items.is_empty() {
        out.push_str("\n## Tareas\n\n");
        for ActionItem { text, owner, done } in &d.action_items {
            let who = owner.as_deref().map(|o| format!(" ({o})")).unwrap_or_default();
            out.push_str(&format!("- [{}] {text}{who}\n", if *done { "x" } else { " " }));
        }
    }
    if !d.decisions.is_empty() {
        out.push_str("\n## Decisiones\n\n");
        for x in &d.decisions {
            out.push_str(&format!("- {x}\n"));
        }
    }
    if !d.topics.is_empty() {
        out.push_str("\n## Temas\n\n");
        for x in &d.topics {
            out.push_str(&format!("- {x}\n"));
        }
    }
    if !d.segments.is_empty() {
        out.push_str("\n## Transcripción\n\n");
        for s in &d.segments {
            out.push_str(&format!("**[{}] {}:** {}\n\n", format_clock(s.start_ms), speaker_label(&s.speaker, lang), s.text));
        }
    }
    out.trim_end().to_string() + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Meeting;

    #[test]
    fn json_limpio() {
        let raw = r#"{"title":"Presupuesto","summary":"Se cierra el viernes.","action_items":[{"text":"Enviar cifras","owner":"Ana"},{"text":"Revisar","owner":null}],"decisions":["Cerrar el viernes"],"topics":["finanzas"]}"#;
        let p = parse_summary(raw).unwrap();
        assert_eq!(p.title.as_deref(), Some("Presupuesto"));
        assert_eq!(p.summary, "Se cierra el viernes.");
        assert_eq!(p.action_items, [ParsedAction { text: "Enviar cifras".into(), owner: Some("Ana".into()) }, ParsedAction { text: "Revisar".into(), owner: None }]);
        assert_eq!(p.decisions, ["Cerrar el viernes"]);
        assert_eq!(p.topics, ["finanzas"]);
    }

    #[test]
    fn json_con_texto_alrededor_y_bloque_de_codigo() {
        let raw = "Claro, aquí tienes el resumen:\n```json\n{\"title\": \"Daily\", \"summary\": \"Breve.\", \"action_items\": [], \"decisions\": [], \"topics\": [\"sprint\"],}\n```\nEspero que sirva.";
        let p = parse_summary(raw).unwrap();
        assert_eq!(p.title.as_deref(), Some("Daily"));
        assert_eq!(p.topics, ["sprint"]);
        let sin_bloque = "Resultado: {\"summary\": \"Hola {mundo}\", \"decisions\": [\"a\"]} fin";
        let p = parse_summary(sin_bloque).unwrap();
        assert_eq!(p.summary, "Hola {mundo}");
        assert_eq!(p.decisions, ["a"]);
    }

    #[test]
    fn json_variantes_de_claves_y_tipos() {
        let raw = r#"{"Resumen":["Primera frase.","Segunda."],"tareas":["Llamar a Pedro",{"tarea":"Enviar acta","responsable":"Luis"},{"task":"x","owner":"null"}],"decisiones":"- Aprobar\n- Aplazar"}"#;
        let p = parse_summary(raw).unwrap();
        assert_eq!(p.summary, "Primera frase. Segunda.");
        assert_eq!(p.action_items.len(), 3);
        assert_eq!(p.action_items[1].owner.as_deref(), Some("Luis"));
        assert_eq!(p.action_items[2].owner, None);
        assert_eq!(p.decisions, ["Aprobar", "Aplazar"]);
    }

    #[test]
    fn json_cortado_se_repara_y_basura_no() {
        let cortado = r#"{"title": "Hola", "summary": "texto sin cerrar"#;
        let p = parse_summary(cortado).unwrap();
        assert_eq!(p.summary, "texto sin cerrar");
        assert!(parse_summary("esto no es json {{{ ::").is_none());
        assert!(parse_summary("").is_none());
        assert!(parse_summary("[1,2,3]").is_none());
        // Un objeto sin nada aprovechable tampoco vale.
        assert!(parse_summary(r#"{"foo": "bar"}"#).is_none());
    }

    #[test]
    fn idioma() {
        assert_eq!(detect_language("vale entonces creo que tenemos que cerrar el presupuesto de la semana"), "es");
        assert_eq!(detect_language("okay so I think we should close the budget for this week and that is it"), "en");
        assert_eq!(detect_language(""), "es");
    }

    #[test]
    fn lineas_del_prompt_con_nombre() {
        let segs = vec![
            Segment { meeting_id: 1, speaker: "me".into(), start_ms: 1_000, end_ms: 2_000, text: "Me encargo yo".into() },
            Segment { meeting_id: 1, speaker: "them".into(), start_ms: 3_000, end_ms: 4_000, text: "Yo preparo la factura".into() },
        ];
        assert_eq!(prompt_lines(&segs, "es", "Alfonso"), vec!["[00:01] Alfonso: Me encargo yo", "[00:03] Otro participante: Yo preparo la factura"]);
    }

    #[test]
    fn prompts_en_el_idioma() {
        assert!(summary_prompt("es", "T", false, "Ana").contains("escribe todo en español"));
        assert!(summary_prompt("en", "T", true, "Ana").contains("write everything in English"));
        // El usuario va por su nombre y se piden los compromisos de todos (caso real con qwen2.5:7b).
        let p = summary_prompt("es", "T", false, "Ana");
        assert!(p.contains("\"Ana\" es el usuario") && p.contains("CUALQUIER participante") && !p.contains("\"Tú\" es"));
        assert!(summary_prompt("es", "TRANSCRIPCION", false, "Ana").ends_with("TRANSCRIPCION"));
    }

    #[test]
    fn trozos_de_transcripcion() {
        let lines: Vec<String> = (0..10).map(|i| format!("linea {i} {}", "x".repeat(20))).collect();
        let chunks = chunk_lines(&lines, 100);
        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|c| c.len() <= 100));
        assert_eq!(chunks.join("\n").lines().count(), 10);
    }

    #[test]
    fn contexto_respeta_el_presupuesto_y_filtra() {
        let m = |t: &str, lines: &[&str]| AskMeeting { title: t.into(), date: "11 oct 2026".into(), summary: Some("Resumen largo.".into()), lines: lines.iter().map(|s| s.to_string()).collect() };
        let ms = [m("A", &["[00:01] Tú: hablamos de presupuesto", "[00:05] Ellos: nada relevante"]), m("B", &["[00:02] Ellos: otro presupuesto"])];
        let ctx = build_context(&ms, &["presupuesto".to_string()], 10_000);
        assert!(ctx.contains("hablamos de presupuesto") && ctx.contains("otro presupuesto"));
        assert!(!ctx.contains("nada relevante"));
        let corto = build_context(&ms, &[], 120);
        assert!(corto.len() <= 130, "{}", corto.len());
    }

    #[test]
    fn markdown_completo() {
        let seg = |who: &str, a: i64, t: &str| Segment { meeting_id: 1, speaker: who.into(), start_ms: a, end_ms: a + 1000, text: t.into() };
        let d = MeetingDetail {
            meeting: Meeting {
                id: 1,
                title: "Cierre de presupuesto".into(),
                started_at: "2026-10-11T10:32:00+02:00".into(),
                ended_at: Some("2026-10-11T11:37:00+02:00".into()),
                duration_ms: 3_900_000,
                app: Some("Zoom".into()),
                summary_status: "done".into(),
                auto: false,
            },
            segments: vec![seg("me", 5_000, "Hola a todos, vamos con el presupuesto"), seg("them", 3_725_000, "Vale, lo cerramos el viernes")],
            summary: Some("Se cerró el presupuesto.".into()),
            action_items: vec![ActionItem { text: "Enviar cifras".into(), owner: Some("Ana".into()), done: false }, ActionItem { text: "Revisar contrato".into(), owner: None, done: true }],
            decisions: vec!["Cerrar el viernes".into()],
            topics: vec!["presupuesto".into()],
        };
        let md = to_markdown(&d);
        let esperado = "# Cierre de presupuesto\n\n*11 oct 2026, 10:32 · 1 h 05 min · Zoom*\n\n## Resumen\n\nSe cerró el presupuesto.\n\n## Tareas\n\n- [ ] Enviar cifras (Ana)\n- [x] Revisar contrato\n\n## Decisiones\n\n- Cerrar el viernes\n\n## Temas\n\n- presupuesto\n\n## Transcripción\n\n**[00:05] Tú:** Hola a todos, vamos con el presupuesto\n\n**[1:02:05] Ellos:** Vale, lo cerramos el viernes\n";
        assert_eq!(md, esperado);
    }

    #[test]
    fn markdown_minimo_sin_resumen() {
        let d = MeetingDetail {
            meeting: Meeting { id: 2, title: "T".into(), started_at: "no es fecha".into(), ended_at: None, duration_ms: 42_000, app: None, summary_status: "none".into(), auto: false },
            segments: vec![],
            summary: None,
            action_items: vec![],
            decisions: vec![],
            topics: vec![],
        };
        assert_eq!(to_markdown(&d), "# T\n\n*no es fecha · 42 s*\n");
    }
}
