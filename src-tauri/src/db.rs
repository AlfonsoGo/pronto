//! SQLite (`%APPDATA%\Pronto2\pronto2.db`): historial, diccionario, snippets, notas, transforms
//! y ajustes. El esquema se versiona con `PRAGMA user_version` y se migra sin perder datos.

use crate::models::*;
use anyhow::{Context, Result};
use chrono::{Duration, Local, NaiveDate};
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::collections::HashMap;
use std::path::PathBuf;

/// El modelo de IA de fábrica pasó de `qwen2.5:7b` a `qwen3:1.7b`. Solo se migra lo guardado que
/// valga exactamente el antiguo de fábrica; un modelo elegido a mano se respeta.
fn migrate_ai_models(s: &mut Settings) {
    for model in [&mut s.ollama_model, &mut s.ai_polish_model] {
        if model == LEGACY_AI_MODEL {
            *model = DEFAULT_AI_MODEL.into();
        }
    }
}

pub struct Db {
    /// Conexión aparte para buscar en el historial (`list_history` con texto), abierta la primera
    /// vez que se busca. Con 100 000 dictados una búsqueda sin resultados recorre la tabla entera, y
    /// en la conexión de `conn` el dictado que terminaba mientras tanto esperaba ~1,5 s para guardar
    /// antes de pegar (rompedor 23-oct, dictado MEDIA 6; regla 3). En WAL un lector no frena al que
    /// escribe. Va ANTES que `conn` a propósito: los campos se sueltan en orden, así esta se cierra
    /// primero y el volcado al cerrar (o no volcar, tras un apagón simulado) lo decide `conn`.
    search: Mutex<Option<Connection>>,
    pub(crate) conn: Mutex<Connection>,
    /// Lo que pasó al abrir y el usuario debe saber (p. ej. un `-wal` ajeno apartado).
    pub(crate) aviso: Option<String>,
}

/// Carpeta de los datos del usuario: `%APPDATA%\Pronto2`, o la de `PRONTO_DATOS` si está puesta. Esa
/// variable es para la versión de pruebas (`scripts/probar-pronto.ps1`): trabaja sobre una copia y
/// nunca toca los datos reales ni el arranque con Windows. Los modelos siguen en `%APPDATA%`.
pub(crate) fn data_root() -> Option<PathBuf> {
    std::env::var_os("PRONTO_DATOS").filter(|v| !v.is_empty()).map(PathBuf::from).or_else(|| dirs::data_dir().map(|d| d.join("Pronto2")))
}

/// ¿Es la versión de pruebas (`PRONTO_DATOS` puesta)?
pub(crate) fn test_build() -> bool {
    std::env::var_os("PRONTO_DATOS").is_some_and(|v| !v.is_empty())
}

fn data_dir() -> Result<PathBuf> {
    let dir = data_root().context("no hay carpeta de datos de usuario")?;
    std::fs::create_dir_all(&dir).with_context(|| format!("no se pudo crear {}", dir.display()))?;
    Ok(dir)
}

pub fn db_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("pronto2.db"))
}

/// Copias diarias de la base (`%APPDATA%\Pronto2\copias`). El 13-oct-2026 una base vacía tapó a la
/// buena al arrancar con Windows; con estas copias se recupera sin forense.
pub fn backup_dir() -> Option<PathBuf> {
    let dir = data_dir().ok()?.join("copias");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// Carpeta de los WAV de cada dictado (`%APPDATA%\Pronto2\audio`).
pub fn audio_dir() -> Option<PathBuf> {
    // Las pruebas nunca tocan el audio del usuario (como `voice_dir`).
    let dir = if cfg!(test) { std::env::temp_dir().join(format!("pronto2-pruebas-audio-{}", std::process::id())) } else { data_dir().ok()?.join("audio") };
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// Cifras de una fecha RFC 3339 para nombrar ficheros (`2026-10-13T21:49:14.12+02:00` →
/// `20261013214914120200`).
pub fn stamp_digits(ts: &str) -> String {
    ts.chars().filter(char::is_ascii_digit).collect()
}

/// WAV de un dictado: `{id}-{cifras de created_at}.wav`. Solo con el id (`{id}.wav`, hasta la 2.2.0),
/// una base nueva tras un incidente como el del 13-oct reutiliza los id y su audio iba a parar al
/// fichero de los dictados de la base buena (rompedor 22-oct, MEDIA 3). Con la fecha de creación
/// cada fila tiene su fichero; una copia restaurada de la misma base sigue encontrando el suyo.
pub fn audio_path(id: i64, created_at: &str) -> Option<PathBuf> {
    Some(audio_dir()?.join(format!("{id}-{}.wav", stamp_digits(created_at))))
}

/// `true` si la fecha de modificación de `path` cae en `[desde − 1 min, hasta + 10 min]`: el fichero
/// se escribió para esa fila y no para otra con el mismo id de otra base.
pub(crate) fn written_between(path: &std::path::Path, from: &str, to: Option<&str>) -> bool {
    let parse = |s: &str| chrono::DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&chrono::Utc));
    let (Some(from), Ok(mtime)) = (parse(from), std::fs::metadata(path).and_then(|m| m.modified())) else { return false };
    let mtime: chrono::DateTime<chrono::Utc> = mtime.into();
    let to = to.and_then(parse).unwrap_or(from);
    mtime >= from - Duration::minutes(1) && mtime <= to + Duration::minutes(10)
}

/// WAV ya guardado de un dictado: el de su nombre o, para los de antes (`{id}.wav`), el antiguo solo
/// si se escribió cuando se creó el dictado (si no, es de otro dictado con el mismo id).
pub fn existing_audio_path(id: i64, created_at: &str) -> Option<PathBuf> {
    let p = audio_path(id, created_at)?;
    if p.is_file() {
        return Some(p);
    }
    let legacy = audio_dir()?.join(format!("{id}.wav"));
    (legacy.is_file() && written_between(&legacy, created_at, None)).then_some(legacy)
}

/// Escribe `bytes` en `path` sin sobrescribir nunca: si ya hay un fichero, se aparta antes (ver
/// `set_aside`). Así un WAV de otra base nunca se pierde.
pub fn write_new(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    set_aside(path)?;
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(path)?;
    f.write_all(bytes)?;
    f.sync_all()
}

/// Si `path` existe, lo mueve (sin borrarlo) a `apartado-<fecha-hora>\` en su carpeta.
pub fn set_aside(path: &std::path::Path) -> std::io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let parent = path.parent().unwrap_or(std::path::Path::new("."));
    let dir = parent.join(format!("apartado-{}", Local::now().format("%Y%m%d-%H%M%S")));
    std::fs::create_dir_all(&dir)?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut dest = dir.join(&name);
    let mut n = 2;
    while dest.exists() {
        dest = dir.join(format!("{n}-{name}"));
        n += 1;
    }
    eprintln!("[audio] {} ya existía: apartado a {}", path.display(), dest.display());
    std::fs::rename(path, dest)
}

/// Carpeta de las muestras de voz del diccionario (`%APPDATA%\Pronto2\voz`, Contrato v11).
pub fn voice_dir() -> Option<PathBuf> {
    // Las pruebas nunca tocan la carpeta del usuario.
    let dir = if cfg!(test) { std::env::temp_dir().join("pronto2-pruebas-voz") } else { data_dir().ok()?.join("voz") };
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// Nombre de fichero de una muestra: solo letras, cifras, `-` y `_`, con `.wav` (nunca una ruta).
pub fn valid_sample_file(name: &str) -> bool {
    name.strip_suffix(".wav").is_some_and(|b| !b.is_empty() && b.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')))
}

fn remove_sample_file(file: &str) {
    if let (true, Some(dir)) = (valid_sample_file(file), voice_dir()) {
        let _ = std::fs::remove_file(dir.join(file));
    }
}

/// Borra el WAV de un dictado (el antiguo `{id}.wav` solo si es suyo, `existing_audio_path`).
fn remove_audio_file((id, created_at): (i64, String)) {
    if let Some(p) = existing_audio_path(id, &created_at) {
        let _ = std::fs::remove_file(p);
    }
}

fn now_iso() -> String {
    Local::now().to_rfc3339()
}

/// Esquema original (versión 1). Las migraciones posteriores parten de aquí.
const SCHEMA_V1: &str = "
CREATE TABLE IF NOT EXISTS history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at TEXT NOT NULL,
    text TEXT NOT NULL,
    raw_text TEXT NOT NULL,
    app TEXT NOT NULL DEFAULT '',
    duration_ms INTEGER NOT NULL DEFAULT 0,
    words INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_history_created ON history(created_at);
CREATE TABLE IF NOT EXISTS words (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    word TEXT NOT NULL,
    replacement TEXT,
    created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS snippets (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    trigger TEXT NOT NULL,
    expansion TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS notes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    text TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    json TEXT NOT NULL
);
";

/// Versión 2: audio guardado, arreglos por dictado, favoritos y transforms.
const MIGRATION_V2: &str = "
ALTER TABLE history ADD COLUMN has_audio INTEGER NOT NULL DEFAULT 0;
ALTER TABLE history ADD COLUMN transform TEXT;
ALTER TABLE history ADD COLUMN fix_dictionary INTEGER NOT NULL DEFAULT 0;
ALTER TABLE history ADD COLUMN fix_snippets INTEGER NOT NULL DEFAULT 0;
ALTER TABLE history ADD COLUMN fix_fillers INTEGER NOT NULL DEFAULT 0;
ALTER TABLE words ADD COLUMN starred INTEGER NOT NULL DEFAULT 0;
ALTER TABLE words ADD COLUMN source TEXT NOT NULL DEFAULT 'manual';
CREATE TABLE IF NOT EXISTS transforms (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    prompt TEXT NOT NULL,
    builtin INTEGER NOT NULL DEFAULT 0,
    hotkey_index INTEGER,
    created_at TEXT NOT NULL
);
";

/// Versión 3: Notetaker. FTS5 con un trozo de texto por fila (`kind` = `seg` | `sum`) para buscar
/// y para armar el contexto de «Pregunta sobre tus reuniones».
const MIGRATION_V3: &str = "
CREATE TABLE IF NOT EXISTS meetings (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    title TEXT NOT NULL,
    title_auto INTEGER NOT NULL DEFAULT 1,
    started_at TEXT NOT NULL,
    ended_at TEXT,
    duration_ms INTEGER NOT NULL DEFAULT 0,
    app TEXT,
    summary TEXT,
    summary_status TEXT NOT NULL DEFAULT 'none',
    decisions TEXT NOT NULL DEFAULT '[]',
    topics TEXT NOT NULL DEFAULT '[]'
);
CREATE INDEX IF NOT EXISTS idx_meetings_started ON meetings(started_at);
CREATE TABLE IF NOT EXISTS meeting_segments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    meeting_id INTEGER NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    speaker TEXT NOT NULL,
    start_ms INTEGER NOT NULL,
    end_ms INTEGER NOT NULL,
    text TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_segments_meeting ON meeting_segments(meeting_id, start_ms);
CREATE TABLE IF NOT EXISTS meeting_actions (
    meeting_id INTEGER NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    idx INTEGER NOT NULL,
    text TEXT NOT NULL,
    owner TEXT,
    done INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (meeting_id, idx)
);
CREATE VIRTUAL TABLE IF NOT EXISTS meeting_fts USING fts5(
    content,
    meeting_id UNINDEXED,
    kind UNINDEXED,
    tokenize = 'unicode61 remove_diacritics 2'
);
";

/// Versión 4: de dónde salió la reunión (detección automática o a mano).
const MIGRATION_V4: &str = "ALTER TABLE meetings ADD COLUMN auto INTEGER NOT NULL DEFAULT 0;";

/// Versión 5: marca de los dictados pulidos con IA.
const MIGRATION_V5: &str = "ALTER TABLE history ADD COLUMN polished INTEGER NOT NULL DEFAULT 0;";

/// Versión 6: diccionario por voz. Cada fila es una forma en que Parakeet transcribe una palabra
/// del diccionario («chonso» para «Txonso»). La base no activa `foreign_keys`, así que
/// `delete_word` borra sus variantes a mano; el `ON DELETE CASCADE` queda como documentación.
const MIGRATION_V6: &str = "
CREATE TABLE IF NOT EXISTS word_variants (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    word_id INTEGER NOT NULL REFERENCES words(id) ON DELETE CASCADE,
    heard TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_word_variants_word ON word_variants(word_id);
";

/// Versión 7: notas de reunión en dos fases (Contrato v10; la 6 es la del diccionario por voz). Campos nuevos del resumen, plazo de
/// cada tarea y la marca de transcripción final (la de la fase 2, no la vista previa en vivo).
const MIGRATION_V7: &str = "
ALTER TABLE meetings ADD COLUMN key_points TEXT NOT NULL DEFAULT '[]';
ALTER TABLE meetings ADD COLUMN next_steps TEXT NOT NULL DEFAULT '[]';
ALTER TABLE meetings ADD COLUMN open_questions TEXT NOT NULL DEFAULT '[]';
ALTER TABLE meetings ADD COLUMN transcript_final INTEGER NOT NULL DEFAULT 0;
ALTER TABLE meeting_actions ADD COLUMN due TEXT;
";

/// Versión 8: huella de voz (Contrato v11; la 7 es la de reuniones). Cada fila es una grabación de
/// la voz del usuario diciendo una palabra del diccionario; el WAV va en `%APPDATA%\Pronto2\voz\{file}`.
/// Como en la v6, `delete_word` borra a mano sus muestras (filas y ficheros).
const MIGRATION_V8: &str = "
CREATE TABLE IF NOT EXISTS word_samples (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    word_id INTEGER NOT NULL REFERENCES words(id) ON DELETE CASCADE,
    file TEXT NOT NULL,
    duration_ms INTEGER NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_word_samples_word ON word_samples(word_id);
";

/// Versión 9: Meets (Contrato v12; la 8 es la huella de voz). Cada turno guarda su texto original si
/// se pulió con IA, su etiqueta (pregunta, respuesta…), si pisó a la otra persona y a qué pregunta
/// contesta; la reunión guarda las preguntas, lo que dijo cada parte y si ya pasó el pulido.
const MIGRATION_V9: &str = "
ALTER TABLE meeting_segments ADD COLUMN raw_text TEXT;
ALTER TABLE meeting_segments ADD COLUMN kind TEXT;
ALTER TABLE meeting_segments ADD COLUMN overlap INTEGER NOT NULL DEFAULT 0;
ALTER TABLE meeting_segments ADD COLUMN reply_to INTEGER;
ALTER TABLE meetings ADD COLUMN questions TEXT NOT NULL DEFAULT '[]';
ALTER TABLE meetings ADD COLUMN said_me TEXT NOT NULL DEFAULT '[]';
ALTER TABLE meetings ADD COLUMN said_them TEXT NOT NULL DEFAULT '[]';
ALTER TABLE meetings ADD COLUMN polish_done INTEGER NOT NULL DEFAULT 0;
";

/// Versión 10: cita de las notas de reunión (decisión A de Txonso, 23-oct: «cada tarea o decisión
/// lleva la cita literal que la respalda»). Cada tarea guarda su cita (JSON con el texto, quién,
/// el momento y el turno) y la reunión, la de cada decisión en el orden de `decisions`. Las notas
/// anteriores quedan sin cita y se ven como antes; no se toca ni se pierde ninguna fila.
const MIGRATION_V10: &str = "
ALTER TABLE meeting_actions ADD COLUMN cita TEXT;
ALTER TABLE meetings ADD COLUMN decision_citas TEXT NOT NULL DEFAULT '[]';
";

pub const POLISH_PROMPT: &str = "Mejora la claridad y la concisión del texto siguiente. Conserva el significado, el tono y el idioma original; corrige gramática y puntuación. Devuelve únicamente el texto resultante, sin comentarios ni comillas.";
pub const PROMPT_ENGINEER_PROMPT: &str = "Convierte el texto siguiente en un prompt claro y bien estructurado para un asistente de IA: define el rol, el objetivo, el contexto, las restricciones y el formato de salida esperado. Mantén el idioma original. Devuelve únicamente el prompt.";

/// Confirma la transacción de una migración con `meta` tocada dentro (si la base ya la tiene): así
/// esa tanda del `-wal` lleva el `base_id` aunque un apagón llegue justo después de su `COMMIT`. Antes
/// `touch_meta` iba después, en otra transacción (rompedor 23-oct, datos BAJA 5).
fn commit_migration(tx: rusqlite::Transaction) -> Result<()> {
    if table_exists(&tx, "meta")? {
        touch_meta(&tx)?;
    }
    tx.commit()?;
    Ok(())
}

fn migrate(conn: &mut Connection) -> Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 1 {
        let tx = conn.transaction()?;
        tx.execute_batch(SCHEMA_V1)?;
        tx.execute_batch("PRAGMA user_version = 1")?;
        commit_migration(tx)?;
    }
    if version < 2 {
        let tx = conn.transaction()?;
        tx.execute_batch(MIGRATION_V2)?;
        let now = now_iso();
        for (name, prompt, idx) in [("Polish", POLISH_PROMPT, 1), ("Prompt Engineer", PROMPT_ENGINEER_PROMPT, 2)] {
            tx.execute(
                "INSERT INTO transforms (name, prompt, builtin, hotkey_index, created_at) VALUES (?1, ?2, 1, ?3, ?4)",
                params![name, prompt, idx, now],
            )?;
        }
        tx.execute_batch("PRAGMA user_version = 2")?;
        commit_migration(tx)?;
    }
    if version < 3 {
        let tx = conn.transaction()?;
        tx.execute_batch(MIGRATION_V3)?;
        tx.execute_batch("PRAGMA user_version = 3")?;
        commit_migration(tx)?;
    }
    if version < 4 {
        let tx = conn.transaction()?;
        tx.execute_batch(MIGRATION_V4)?;
        tx.execute_batch("PRAGMA user_version = 4")?;
        commit_migration(tx)?;
    }
    if version < 5 {
        let tx = conn.transaction()?;
        tx.execute_batch(MIGRATION_V5)?;
        tx.execute_batch("PRAGMA user_version = 5")?;
        commit_migration(tx)?;
    }
    if version < 6 {
        let tx = conn.transaction()?;
        tx.execute_batch(MIGRATION_V6)?;
        tx.execute_batch("PRAGMA user_version = 6")?;
        commit_migration(tx)?;
    }
    if version < 7 {
        let tx = conn.transaction()?;
        tx.execute_batch(MIGRATION_V7)?;
        tx.execute_batch("PRAGMA user_version = 7")?;
        commit_migration(tx)?;
    }
    if version < 8 {
        let tx = conn.transaction()?;
        tx.execute_batch(MIGRATION_V8)?;
        tx.execute_batch("PRAGMA user_version = 8")?;
        commit_migration(tx)?;
    }
    if version < 9 {
        let tx = conn.transaction()?;
        tx.execute_batch(MIGRATION_V9)?;
        tx.execute_batch("PRAGMA user_version = 9")?;
        commit_migration(tx)?;
    }
    if version < 10 {
        // Solo las columnas que falten: una base que vuelve de una versión posterior (o que bajó de
        // versión a mano) puede tenerlas ya, y `ADD COLUMN` repetido fallaría ("duplicate column").
        let tx = conn.transaction()?;
        if v10_missing(&tx)? == (true, true) {
            tx.execute_batch(MIGRATION_V10)?;
        } else {
            add_v10_columns(&tx)?;
        }
        tx.execute_batch("PRAGMA user_version = 10")?;
        commit_migration(tx)?;
    } else {
        ensure_v10_columns(conn)?;
    }
    Ok(())
}

/// (falta `meeting_actions.cita`, falta `meetings.decision_citas`).
fn v10_missing(conn: &Connection) -> rusqlite::Result<(bool, bool)> {
    let has = |table: &str, col: &str| -> rusqlite::Result<bool> {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let cols = stmt.query_map([], |r| r.get::<_, String>(1))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(cols.iter().any(|c| c == col))
    };
    Ok((!has("meeting_actions", "cita")?, !has("meetings", "decision_citas")?))
}

/// Añade las columnas de la v10 que falten (no toca filas).
fn add_v10_columns(conn: &Connection) -> Result<()> {
    let (actions, meetings) = v10_missing(conn)?;
    if actions {
        conn.execute_batch("ALTER TABLE meeting_actions ADD COLUMN cita TEXT;")?;
    }
    if meetings {
        conn.execute_batch("ALTER TABLE meetings ADD COLUMN decision_citas TEXT NOT NULL DEFAULT '[]';")?;
    }
    Ok(())
}

/// Una base marcada como v10 o posterior que no tiene las columnas de las citas (otra rama, una
/// prueba que simula una versión futura): se completan en vez de fallar al leer las notas. Solo
/// añade columnas; no toca filas.
fn ensure_v10_columns(conn: &mut Connection) -> Result<()> {
    if !table_exists(conn, "meetings")? || !table_exists(conn, "meeting_actions")? || v10_missing(conn)? == (false, false) {
        return Ok(());
    }
    let tx = conn.transaction()?;
    add_v10_columns(&tx)?;
    commit_migration(tx)
}

const HISTORY_COLS: &str = "id, created_at, text, raw_text, app, duration_ms, words, has_audio, transform, polished";

fn history_row(r: &Row) -> rusqlite::Result<HistoryItem> {
    Ok(HistoryItem {
        id: r.get(0)?,
        created_at: r.get(1)?,
        text: r.get(2)?,
        raw_text: r.get(3)?,
        app: r.get(4)?,
        duration_ms: r.get(5)?,
        words: r.get(6)?,
        has_audio: r.get::<_, i64>(7)? != 0,
        transform: r.get(8)?,
        polished: r.get::<_, i64>(9)? != 0,
    })
}

fn word_row(r: &Row) -> rusqlite::Result<Word> {
    Ok(Word {
        id: r.get(0)?,
        word: r.get(1)?,
        replacement: r.get(2)?,
        created_at: r.get(3)?,
        starred: r.get::<_, i64>(4)? != 0,
        source: r.get(5)?,
        variants: Vec::new(),
        samples: Vec::new(),
    })
}

/// Misma variante salvo mayúsculas y espacios (la tubería las trata igual).
fn same_heard(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    norm(a) == norm(b)
}

/// Inserta `heard` (ya normalizado) si la palabra no tiene ya esa variante. Devuelve la variante,
/// nueva o la que ya estaba.
fn insert_variant(conn: &Connection, word_id: i64, heard: &str) -> Result<WordVariant> {
    let mut stmt = conn.prepare("SELECT id, heard FROM word_variants WHERE word_id = ?1")?;
    let existing: Vec<WordVariant> =
        stmt.query_map(params![word_id], |r| Ok(WordVariant { id: r.get(0)?, heard: r.get(1)? }))?.collect::<std::result::Result<_, _>>()?;
    if let Some(v) = existing.into_iter().find(|v| same_heard(&v.heard, heard)) {
        return Ok(v);
    }
    conn.execute(
        "INSERT INTO word_variants (word_id, heard, created_at) VALUES (?1, ?2, ?3)",
        params![word_id, heard, now_iso()],
    )?;
    Ok(WordVariant { id: conn.last_insert_rowid(), heard: heard.into() })
}

/// Normaliza una variante; vacía (solo signos) no vale.
fn clean_variant(heard: &str) -> Result<String> {
    let v = crate::text::normalize_heard(heard);
    if v.is_empty() {
        anyhow::bail!("La variante no puede estar vacía");
    }
    Ok(v)
}

/// Prefijo de las muestras grabadas que aún no son de ninguna palabra (ver `attach_sample`).
pub const PENDING_PREFIX: &str = "pendiente-";

/// Registra una muestra pendiente (`pendiente-*.wav` en `voz`) como muestra de `word_id`: la
/// renombra a `w{word_id}-….wav` y guarda la fila. El fichero tiene que existir.
fn attach_sample(conn: &Connection, word_id: i64, pending: &str) -> Result<VoiceSample> {
    let Some(stem) = pending.strip_prefix(PENDING_PREFIX).filter(|_| valid_sample_file(pending)) else {
        anyhow::bail!("Muestra de voz no válida");
    };
    let dir = voice_dir().context("no hay carpeta de voz")?;
    let from = dir.join(pending);
    let len = std::fs::metadata(&from).context("La grabación ya no está: vuelve a grabarla")?.len();
    let duration_ms = (len.saturating_sub(44) / 2 * 1000 / crate::audio::TARGET_RATE as u64) as i64;
    let file = format!("w{word_id}-{stem}");
    std::fs::rename(&from, dir.join(&file)).with_context(|| format!("no se pudo guardar la muestra {file}"))?;
    let created_at = now_iso();
    conn.execute(
        "INSERT INTO word_samples (word_id, file, duration_ms, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![word_id, file, duration_ms, created_at],
    )?;
    Ok(VoiceSample { id: conn.last_insert_rowid(), duration_ms, created_at, file })
}

fn transform_row(r: &Row) -> rusqlite::Result<Transform> {
    Ok(Transform { id: r.get(0)?, name: r.get(1)?, prompt: r.get(2)?, builtin: r.get::<_, i64>(3)? != 0, hotkey_index: r.get(4)? })
}

/// Ajustes desde el JSON guardado, campo a campo: un campo que esta versión no entiende (de una
/// versión más nueva, o dañado) toma su valor de fábrica y el resto se respeta. Antes, UN campo raro
/// devolvía todos los ajustes de fábrica y el siguiente guardado los perdía. Devuelve también los
/// campos descartados.
fn settings_from_value(value: &serde_json::Value) -> (Settings, Vec<String>) {
    if let Ok(s) = serde_json::from_value::<Settings>(value.clone()) {
        return (s, Vec::new());
    }
    let Some(stored) = value.as_object() else { return (Settings::default(), vec!["(todo)".into()]) };
    let mut merged = serde_json::to_value(Settings::default()).unwrap_or_default();
    let mut rejected = Vec::new();
    for (k, v) in stored {
        let Some(obj) = merged.as_object_mut() else { break };
        let prev = obj.insert(k.clone(), v.clone());
        if serde_json::from_value::<Settings>(merged.clone()).is_err() {
            let obj = merged.as_object_mut().expect("objeto");
            match prev {
                Some(p) => obj.insert(k.clone(), p),
                None => obj.remove(k),
            };
            rejected.push(k.clone());
        }
    }
    (serde_json::from_value(merged).unwrap_or_default(), rejected)
}

/// `true` si el JSON guardado tiene claves que esta versión no conoce (de una versión más nueva):
/// serde las ignora sin error, así que `settings_from_value` no las ve y el primer guardado las
/// perdería sin copia en `settings_backup` (rompedor 22-oct-b, BAJA 9).
fn has_unknown_keys(value: &serde_json::Value) -> bool {
    static KNOWN: std::sync::LazyLock<Vec<String>> = std::sync::LazyLock::new(|| {
        serde_json::to_value(Settings::default()).ok().and_then(|v| v.as_object().map(|o| o.keys().cloned().collect())).unwrap_or_default()
    });
    value.as_object().is_some_and(|o| o.keys().any(|k| !KNOWN.contains(k)))
}

/// `fold(x)`: `x` en minúsculas y sin tildes (`text::fold`), para buscar en el historial. Es solo de
/// esta conexión (como los disparadores TEMP de `touch_triggers`): no se guarda en la base, así que
/// ninguna tabla, índice ni disparador guardado depende de ella y la base se abre igual sin Pronto.
fn register_fold(conn: &Connection) -> rusqlite::Result<()> {
    use rusqlite::functions::FunctionFlags;
    use rusqlite::types::ValueRef;
    conn.create_scalar_function("fold", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
        Ok(match ctx.get_raw(0) {
            ValueRef::Text(t) => Some(crate::text::fold(&String::from_utf8_lossy(t))),
            ValueRef::Integer(i) => Some(i.to_string()),
            ValueRef::Real(f) => Some(f.to_string()),
            ValueRef::Null | ValueRef::Blob(_) => None,
        })
    })?;
    // `fold_has(x, aguja)`: 1 si `fold(x)` contiene `aguja` (ya plegada). Sin devolver a SQLite un
    // texto nuevo por fila ni pasar por `LIKE`, y un texto ASCII se compara en el sitio sin copiarlo:
    // `fold(text) LIKE …` iba ×17 más lento que el `LIKE` de antes (rompedor 23-oct, dictado MEDIA 6).
    conn.create_scalar_function("fold_has", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
        // La aguja es la misma en toda la consulta: se lee una vez (dato auxiliar de SQLite).
        let needle: std::sync::Arc<Option<String>> = ctx.get_or_create_aux(1, |v| -> rusqlite::Result<Option<String>> {
            Ok(match v {
                ValueRef::Text(t) => Some(String::from_utf8_lossy(t).into_owned()),
                _ => None,
            })
        })?;
        let Some(needle) = needle.as_ref() else { return Ok(false) };
        Ok(match ctx.get_raw(0) {
            ValueRef::Text(t) => match std::str::from_utf8(t) {
                Ok(s) => fold_contains(s, &needle),
                Err(_) => fold_contains(&String::from_utf8_lossy(t), &needle),
            },
            ValueRef::Integer(i) => i.to_string().contains(needle.as_str()),
            ValueRef::Real(f) => f.to_string().contains(needle.as_str()),
            ValueRef::Null | ValueRef::Blob(_) => false,
        })
    })
}

/// `fold(hay)` contiene `needle` (que ya viene plegada). Un texto ASCII se pliega en el sitio (sus
/// minúsculas ASCII son su `fold`); el resto pasa por `text::fold`, para no separarse de ella.
fn fold_contains(hay: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    if hay.is_ascii() {
        let (h, n) = (hay.as_bytes(), needle.as_bytes());
        return n.len() <= h.len() && h.windows(n.len()).any(|w| w.iter().zip(n).all(|(a, b)| a.to_ascii_lowercase() == *b));
    }
    crate::text::fold(hay).contains(needle)
}

// ------------------------------------------------------------ identidad de la base (meta)

/// Identidad de la base (rompedor 22-oct-b, ALTA 1). Claves: `base_id`, un UUID v4 escrito al crear
/// la base (o al abrir por primera vez una de antes); `escrituras`, un contador que sube con cada
/// fila escrita (disparadores TEMP, `touch_triggers`) para que todo `-wal` lleve la página de `meta`
/// y con ella el `base_id` de su base; `borrado_masivo`, la fecha del último «Borrar historial» o
/// purga por retención hecha desde Pronto (MEDIA 6).
const META_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT)";

fn table_exists(conn: &Connection, name: &str) -> rusqlite::Result<bool> {
    conn.query_row("SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)", params![name], |r| r.get(0))
}

fn read_meta(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    if !table_exists(conn, "meta")? {
        return Ok(None);
    }
    Ok(conn.query_row("SELECT value FROM meta WHERE key = ?1", params![key], |r| r.get::<_, Option<String>>(0)).optional()?.flatten())
}

fn write_meta(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute("INSERT INTO meta (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value", params![key, value])?;
    Ok(())
}

/// UUID v4 con el generador aleatorio de SQLite (`randomblob`): sin dependencias nuevas.
fn new_base_id(conn: &Connection) -> rusqlite::Result<String> {
    let mut b: Vec<u8> = conn.query_row("SELECT randomblob(16)", [], |r| r.get(0))?;
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    Ok(format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32]))
}

fn is_uuid(s: &[u8]) -> bool {
    s.len() == 36 && s.iter().enumerate().all(|(i, c)| if matches!(i, 8 | 13 | 18 | 23) { *c == b'-' } else { c.is_ascii_digit() || (b'a'..=b'f').contains(c) })
}

/// `meta` con su `base_id` y los disparadores que la tocan en cada escritura. Solo escribe si falta
/// algo (abrir una base ya identificada no cambia ninguna fila).
fn ensure_identity(conn: &Connection) -> rusqlite::Result<()> {
    if !table_exists(conn, "meta")? {
        conn.execute_batch(META_SCHEMA)?;
    }
    match read_meta(conn, "base_id")? {
        None => {
            let id = new_base_id(conn)?;
            conn.execute("INSERT OR IGNORE INTO meta (key, value) VALUES ('base_id', ?1)", params![id])?;
        }
        // Copia de `copias\` restaurada (lleva `copia_de`) que aún no se ha adoptado: identidad nueva.
        // Con el `base_id` de la copia, al restaurar la MISMA copia otra vez su `-wal` pasaba por propio
        // y se aplicaba sobre ella (rompedor 23-oct, datos MEDIA 3). `copia_de` se queda (linaje para
        // comparar copias), `restaurada_de` guarda el id de la copia, `adoptada` el id que se le da
        // (así no se vuelve a adoptar) y `restaurada_en` la fecha: restaurar es decisión del usuario y
        // la copia diaria no la toma por base menguada (MEDIA 4).
        Some(cur) if read_meta(conn, "copia_de")?.is_some() && read_meta(conn, "adoptada")?.as_deref() != Some(cur.as_str()) => {
            let tx = conn.unchecked_transaction()?;
            let id = new_base_id(&tx)?;
            write_meta(&tx, "base_id", &id)?;
            write_meta(&tx, "restaurada_de", &cur)?;
            write_meta(&tx, "adoptada", &id)?;
            write_meta(&tx, "restaurada_en", &now_iso())?;
            tx.commit()?;
        }
        Some(_) => {}
    }
    if read_meta(conn, "escrituras")?.is_none() {
        conn.execute("INSERT OR IGNORE INTO meta (key, value) VALUES ('escrituras', '0')", [])?;
    }
    touch_triggers(conn)
}

/// Tablas sombra de las tablas virtuales que NO llevan disparador: las del índice FTS5 (`_data`,
/// `_idx`) se escriben al confirmar (`xSync`) y en tandas grandes; las de cada fila (`_content`,
/// `_docsize`) cambian en la misma tanda que ellas, así que con esas basta para que la tanda lleve
/// `meta`.
const SHADOW_SIN_TOQUE: [&str; 2] = ["_data", "_idx"];

/// `CREATE TEMP TRIGGER` que sube `meta.escrituras` tras cada `op` en la tabla `t`.
fn touch_trigger_sql(t: &str, op: &str) -> String {
    let t = t.replace('"', "\"\"");
    format!(
        "CREATE TEMP TRIGGER IF NOT EXISTS \"pronto_toque_{op}_{t}\" AFTER {op} ON \"{t}\" BEGIN UPDATE meta SET value = CAST(value AS INTEGER) + 1 WHERE key = 'escrituras'; END;\n"
    )
}

/// Disparadores TEMP (de esta conexión, no se guardan en la base) que suben `meta.escrituras` con
/// cada fila escrita en cualquier tabla. SQLite no guarda en el `-wal` a qué base pertenece y aplica
/// cualquiera que encuentre (incidente del 13-oct): así toda tanda del `-wal` lleva la página de
/// `meta` con el `base_id` y la guarda (`wal_identity`) sabe de qué base es, aunque el `-wal` venga
/// de una base con otra disposición de páginas. También las escrituras en las tablas FTS (por sus
/// tablas sombra de cada fila): un `-wal` de esta conexión siempre lleva la página de `meta`.
///
/// Regla para lo que venga: toda escritura fuera de estos disparadores (una migración, FTS
/// `optimize`, `ANALYZE`, `VACUUM`…) va en una transacción que llame a `touch_meta` antes de su
/// `COMMIT` (las migraciones ya lo hacen, `commit_migration`). Un `-wal` sin `meta` (Pronto 2.0.2,
/// un editor de SQLite) ya no se aparta sin más: lo decide la vista (`foreign_by_view`) y antes se
/// copia la base (rompedor 23-oct, datos MEDIA 1 y BAJA 5).
fn touch_triggers(conn: &Connection) -> rusqlite::Result<()> {
    let names = |sql: &str| -> rusqlite::Result<Vec<String>> { conn.prepare(sql)?.query_map([], |r| r.get(0))?.collect() };
    let vtabs = names("SELECT name FROM main.sqlite_master WHERE type = 'table' AND sql LIKE 'CREATE VIRTUAL%'")?;
    let tables = names("SELECT name FROM main.sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name <> 'meta' AND sql NOT LIKE 'CREATE VIRTUAL%'")?;
    let shadow_sin_toque = |t: &str| vtabs.iter().any(|v| SHADOW_SIN_TOQUE.iter().any(|s| t == format!("{v}{s}")));
    let mut sql = String::new();
    for t in tables.iter().filter(|t| !shadow_sin_toque(t)) {
        for op in ["INSERT", "UPDATE", "DELETE"] {
            sql.push_str(&touch_trigger_sql(t, op));
        }
    }
    conn.execute_batch(&sql)
}

/// Sube `meta.escrituras` una vez: la tanda en curso lleva la página de `meta` aunque sus filas se
/// escriban sin disparadores (`clear_history`, migraciones).
fn touch_meta(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute("UPDATE meta SET value = CAST(value AS INTEGER) + 1 WHERE key = 'escrituras'", [])?;
    Ok(())
}

/// Lee un entero de longitud variable de SQLite (1-9 bytes, big-endian, 7 bits por byte salvo el
/// noveno). `None` si se sale de `b`.
fn varint(b: &[u8], at: usize) -> Option<(u64, usize)> {
    let mut v = 0u64;
    for i in 0..9 {
        let c = *b.get(at + i)?;
        if i == 8 {
            return Some(((v << 8) | c as u64, 9));
        }
        v = (v << 7) | (c & 0x7f) as u64;
        if c & 0x80 == 0 {
            return Some((v, i + 1));
        }
    }
    None
}

/// `base_id` de una página hoja de la tabla `meta` (formato de B-tree de SQLite): recorre sus celdas
/// y decodifica cada registro; vale el de clave TEXT `'base_id'` con valor TEXT de 36 bytes con forma
/// de UUID. Antes se buscaba el texto `base_id` en todos los bytes del `-wal`: una nota que lo
/// contuviera, o los restos de una celda borrada, pasaban por identidad (rompedor 22-oct-c, datos
/// MEDIA 4 y BAJA 6). `None` si la página no es una hoja de tabla o no tiene esa fila.
fn meta_page_id(page: &[u8], usable: usize) -> Option<String> {
    if page.first() != Some(&0x0d) {
        return None;
    }
    let u16_at = |o: usize| -> Option<usize> { Some(u16::from_be_bytes([*page.get(o)?, *page.get(o + 1)?]) as usize) };
    let cells = u16_at(3)?;
    for i in 0..cells {
        let Some(cell) = u16_at(8 + 2 * i) else { break };
        let Some((payload, n1)) = varint(page, cell) else { continue };
        let Some((_rowid, n2)) = varint(page, cell + n1) else { continue };
        // Un registro de `meta` es corto: si desborda a otras páginas, no es el nuestro.
        if payload as usize > usable.saturating_sub(35) {
            continue;
        }
        let start = cell + n1 + n2;
        let Some(rec) = page.get(start..start + payload as usize) else { continue };
        let Some((hdr, mut h)) = varint(rec, 0) else { continue };
        let mut types = Vec::new();
        while h < hdr as usize && types.len() < 2 {
            let Some((t, n)) = varint(rec, h) else { break };
            types.push(t);
            h += n;
        }
        // TEXT de n bytes = tipo 13 + 2n: 'base_id' (7) → 27; un UUID (36) → 85.
        if types != [27, 85] {
            continue;
        }
        let at = hdr as usize;
        if rec.get(at..at + 7) == Some(b"base_id") {
            if let Some(v) = rec.get(at + 7..at + 43).filter(|v| is_uuid(v)) {
                return Some(String::from_utf8_lossy(v).into_owned());
            }
        }
    }
    None
}

/// Lo que `wal_identity` sabe de la página de `meta` en un `-wal`.
#[derive(Debug, PartialEq)]
enum WalMeta {
    /// Ningún marco confirmado lleva la página de `meta` (o el principal no tiene `meta`).
    Missing,
    /// La última página de `meta` confirmada no tiene un `base_id` legible (otra disposición de
    /// páginas tras un `VACUUM`, `meta` vaciada por otro programa…): lo decide la vista.
    Unreadable,
    Id(String),
}

/// Lo que llevan los marcos de un `-wal` que SQLite aplicaría (válidos y hasta el último commit).
#[derive(Debug)]
struct WalScan {
    /// Marcos que SQLite aplicaría.
    frames: usize,
    /// Tamaño de página de la cabecera.
    page: u32,
    /// Cabecera con la suma bien y una versión que SQLite no entiende: SQLite no abre la base
    /// (`SQLITE_CANTOPEN`) mientras ese `-wal` esté al lado.
    unknown_version: Option<u32>,
    meta: WalMeta,
    /// `base_id` de la última página confirmada que parece la hoja de `meta` pero NO está donde la
    /// tiene el principal: el `-wal` es de una base con otra disposición de páginas (la viva, junto a
    /// una copia restaurada sobre ella). Con un solo arreglo de un dictado, la página de `meta` de la
    /// base viva no coincide con la raíz de `meta` de la copia y `meta` salía `Missing`.
    stray: Option<String>,
}

/// Lee el `-wal` tal cual, marco a marco (formato WAL de SQLite: cabecera de 32 bytes, marcos de 24
/// bytes + página, sales y sumas encadenadas) y se queda con la última versión confirmada de la
/// página `meta_root` (la raíz de `meta` en el fichero principal), que decodifica. Con un búfer de
/// una página: un `-wal` de 250 MB ya no pasa entero por memoria en cada arranque (rompedor
/// 22-oct-c, datos BAJA 8).
fn wal_identity(wal: &std::path::Path, meta_root: Option<u32>, usable_reserve: usize) -> std::io::Result<WalScan> {
    use std::io::Read;
    let empty = |page| WalScan { frames: 0, page, unknown_version: None, meta: WalMeta::Missing, stray: None };
    let mut f = std::io::BufReader::with_capacity(1 << 16, std::fs::File::open(wal)?);
    let mut h = [0u8; 32];
    if f.read_exact(&mut h).is_err() {
        return Ok(empty(0));
    }
    let be = |b: &[u8], o: usize| u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
    let big = match be(&h, 0) {
        0x377f_0682 => false,
        0x377f_0683 => true,
        _ => return Ok(empty(0)),
    };
    let page = be(&h, 8) as usize;
    // Mismo orden que SQLite (`walIndexRecover`): página válida, suma de la cabecera y, solo con la
    // suma bien, la versión.
    if !(512..=65_536).contains(&page) || !page.is_power_of_two() {
        return Ok(empty(0));
    }
    let sum = |data: &[u8], (mut s0, mut s1): (u32, u32)| {
        for c in data.chunks_exact(8) {
            let w = |x: &[u8]| {
                let a = [x[0], x[1], x[2], x[3]];
                if big { u32::from_be_bytes(a) } else { u32::from_le_bytes(a) }
            };
            s0 = s0.wrapping_add(w(&c[0..4]).wrapping_add(s1));
            s1 = s1.wrapping_add(w(&c[4..8]).wrapping_add(s0));
        }
        (s0, s1)
    };
    let mut s = sum(&h[0..24], (0, 0));
    if s != (be(&h, 24), be(&h, 28)) {
        return Ok(empty(page as u32));
    }
    if be(&h, 4) != 3_007_000 {
        return Ok(WalScan { unknown_version: Some(be(&h, 4)), ..empty(page as u32) });
    }
    let (mut frames, mut committed) = (0usize, 0usize);
    let (mut pending, mut meta_page): (Option<Vec<u8>>, Option<Vec<u8>>) = (None, None);
    let (mut pending_stray, mut stray): (Option<String>, Option<String>) = (None, None);
    let mut fh = [0u8; 24];
    let mut buf = vec![0u8; page];
    while f.read_exact(&mut fh).is_ok() && f.read_exact(&mut buf).is_ok() {
        let pgno = be(&fh, 0);
        if fh[8..16] != h[16..24] || pgno == 0 {
            break;
        }
        s = sum(&fh[0..8], s);
        s = sum(&buf, s);
        if s != (be(&fh, 16), be(&fh, 20)) {
            break;
        }
        frames += 1;
        if Some(pgno) == meta_root {
            pending = Some(buf.clone());
        } else if let Some(id) = stray_meta_id(&buf, page - usable_reserve) {
            pending_stray = Some(id);
        }
        if be(&fh, 4) != 0 {
            committed = frames;
            if pending.is_some() {
                meta_page = pending.take();
            }
            if pending_stray.is_some() {
                stray = pending_stray.take();
            }
        }
    }
    let meta = match meta_page {
        None => WalMeta::Missing,
        Some(p) => meta_page_id(&p, page - usable_reserve).map_or(WalMeta::Unreadable, WalMeta::Id),
    };
    Ok(WalScan { frames: committed, page: page as u32, unknown_version: None, meta, stray })
}

/// `base_id` de una página que parece la hoja de `meta` (pocas celdas: `meta` tiene unas pocas
/// filas), o `None`. Para `WalScan::stray`.
fn stray_meta_id(page: &[u8], usable: usize) -> Option<String> {
    if page.first() != Some(&0x0d) || u16::from_be_bytes([*page.get(3)?, *page.get(4)?]) > 16 {
        return None;
    }
    meta_page_id(page, usable)
}

// ------------------------------------------------------------ guarda del -wal ajeno (13-oct)

fn sidecar(path: &std::path::Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}{suffix}", path.display()))
}

fn file_len(path: &std::path::Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

/// `file:` URI de una ruta de Windows o Unix, con los caracteres reservados escapados.
fn file_uri(path: &std::path::Path) -> String {
    let mut s = path.to_string_lossy().replace('\\', "/");
    if !s.starts_with('/') {
        s.insert(0, '/');
    }
    let mut out = String::from("file://");
    for c in s.chars() {
        match c {
            '%' | '?' | '#' | ' ' => out.push_str(&format!("%{:02X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Conexión de solo lectura al fichero `path` tal cual: `immutable=1` no lee el `-wal`, no bloquea
/// y no escribe nada.
fn open_immutable(path: &std::path::Path) -> rusqlite::Result<Connection> {
    let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX;
    Connection::open_with_flags(format!("{}?immutable=1", file_uri(path)), flags)
}

/// Datos del usuario en una base: (user_version, filas de dictados, reuniones, diccionario,
/// snippets y notas). Los ajustes y los transforms de fábrica no cuentan: una base recién creada
/// también los tiene.
fn census(conn: &Connection) -> rusqlite::Result<(i64, i64)> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let mut rows = 0i64;
    for t in USER_TABLES {
        if table_exists(conn, t)? {
            rows += conn.query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get::<_, i64>(0))?;
        }
    }
    Ok((version, rows))
}

const USER_TABLES: [&str; 5] = ["history", "meetings", "words", "snippets", "notes"];

/// Lo que se ve de una base al abrirla con una conexión.
struct Look {
    version: i64,
    rows: i64,
    /// Mayor id que llegó a tener alguna tabla del usuario (`sqlite_sequence`): 0 = nunca tuvo datos.
    ever: i64,
    id: Option<String>,
    /// `sqlite_sequence` entera: el mayor id que llegó a tener cada tabla.
    seqs: Vec<(String, i64)>,
    /// Página raíz de `meta`, si la tabla existe.
    meta_root: Option<u32>,
}

impl Look {
    /// Primera tabla cuyo contador `sqlite_sequence` es menor en `self` que en `before`. Un `-wal`
    /// propio nunca lo baja (los id de AUTOINCREMENT no se reutilizan ni tras borrar), con o sin
    /// `base_id`: si lo baja, lo escribió otra base (rompedor 22-oct-c, datos ALTA 1 (a)).
    fn lowered_seq(&self, before: &Look) -> Option<(String, i64, i64)> {
        before.seqs.iter().find_map(|(t, was)| {
            let now = self.seqs.iter().find(|(n, _)| n == t).map_or(0, |(_, s)| *s);
            (now < *was).then(|| (t.clone(), now, *was))
        })
    }
}

fn look(conn: &Connection) -> rusqlite::Result<Look> {
    let (version, rows) = census(conn)?;
    let seqs: Vec<(String, i64)> = if table_exists(conn, "sqlite_sequence")? {
        conn.prepare("SELECT name, seq FROM sqlite_sequence")?.query_map([], |r| Ok((r.get(0)?, r.get::<_, Option<i64>>(1)?.unwrap_or(0))))?.collect::<rusqlite::Result<_>>()?
    } else {
        Vec::new()
    };
    let ever = seqs.iter().filter(|(t, _)| USER_TABLES.contains(&t.as_str())).map(|(_, s)| *s).max().unwrap_or(0);
    let meta_root = conn
        .query_row("SELECT rootpage FROM sqlite_master WHERE type = 'table' AND name = 'meta'", [], |r| r.get::<_, i64>(0))
        .optional()?
        .and_then(|p| u32::try_from(p).ok());
    Ok(Look { version, rows, ever, id: read_meta(conn, "base_id")?, seqs, meta_root })
}

/// Tamaño de página y bytes reservados por página de la cabecera del fichero principal.
fn main_page_size(path: &std::path::Path) -> Option<(u32, usize)> {
    use std::io::Read;
    let mut h = [0u8; 100];
    std::fs::File::open(path).ok()?.read_exact(&mut h).ok()?;
    let p = u16::from_be_bytes([h[16], h[17]]) as u32;
    Some((if p == 1 { 65_536 } else { p }, h[20] as usize))
}

/// Carpeta nueva `<prefix>-<fecha-hora>[-n]\` junto a la base.
fn new_dir_beside(path: &std::path::Path, prefix: &str) -> Result<PathBuf> {
    let stamp = Local::now().format("%Y%m%d-%H%M%S");
    let parent = path.parent().unwrap_or(std::path::Path::new("."));
    let mut dest = parent.join(format!("{prefix}-{stamp}"));
    let mut n = 2;
    while dest.exists() {
        dest = parent.join(format!("{prefix}-{stamp}-{n}"));
        n += 1;
    }
    std::fs::create_dir_all(&dest).with_context(|| format!("no se pudo crear {}", dest.display()))?;
    Ok(dest)
}

fn base_name(path: &std::path::Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "pronto2.db".into())
}

/// Mueve (nunca borra) `path` + cada sufijo que exista a `apartado-<fecha-hora>\` junto a la base.
/// Todo o nada: si uno no se puede mover, los ya movidos vuelven a su sitio y se devuelve `Err`.
fn set_aside_all(path: &std::path::Path, suffixes: &[&str]) -> Result<PathBuf> {
    let dest = new_dir_beside(path, "apartado")?;
    let name = base_name(path);
    let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
    for suffix in suffixes {
        let from = sidecar(path, suffix);
        if !from.exists() {
            continue;
        }
        let to = dest.join(format!("{name}{suffix}"));
        if let Err(e) = std::fs::rename(&from, &to) {
            for (f, t) in moved.iter().rev() {
                let _ = std::fs::rename(t, f);
            }
            let _ = std::fs::remove_dir(&dest);
            return Err(anyhow::Error::new(e).context(format!(
                "no se pudo apartar {} (¿lo tiene abierto otro programa?). No se ha movido nada y la base no se abre para no perder datos",
                from.display()
            )));
        }
        moved.push((from, to));
    }
    Ok(dest)
}

/// Copia (no mueve) la base y su `-wal` a `respaldo-<fecha-hora>\` junto a ella, antes de aplicar un
/// `-wal` que se ha dado por propio sin una identidad que lo pruebe (base de antes sin `base_id`, o
/// un `-wal` sin la página de `meta`: Pronto 2.0.2, un editor de SQLite). Si la decisión fuera
/// errónea en cualquiera de los dos sentidos, aquí está el estado de antes, entero (rompedor 23-oct,
/// datos MEDIA 1 y 2: en el primer arranque de 2.0.3 sobre una base de antes, el `TRUNCATE` de ese
/// mismo arranque machacaba la base si el `-wal` era ajeno con contadores más altos).
fn backup_beside(path: &std::path::Path, why: &str) -> Result<PathBuf> {
    let dest = new_dir_beside(path, "respaldo")?;
    let name = base_name(path);
    for suffix in ["", "-wal"] {
        let from = sidecar(path, suffix);
        if from.exists() {
            std::fs::copy(&from, dest.join(format!("{name}{suffix}"))).with_context(|| format!("no se pudo copiar {} a {}", from.display(), dest.display()))?;
        }
    }
    let leeme = format!(
        "Copia de {name} y de su registro de cambios ({name}-wal) tal y como estaban al arrancar Pronto, antes de aplicar ese registro: {why}.\r\n\
         Pronto lo ha dado por suyo y lo ha aplicado. Si al abrir ves datos que no son tuyos o te faltan los de antes, cierra Pronto (Salir), aparta {name}, {name}-wal y {name}-shm de {} y pon en su sitio el {name} de esta carpeta, sin su -wal: es tu base tal y como estaba guardada. El {name}-wal de aquí es el registro que se aplicó, por si lo necesitas.\r\n",
        path.parent().map(|p| p.display().to_string()).unwrap_or_default()
    );
    let _ = std::fs::write(dest.join("LEEME.txt"), leeme);
    Ok(dest)
}

/// Lo que la guarda sabe del fichero principal al arrancar.
#[derive(Default)]
struct Guard {
    aviso: Option<String>,
    /// El fichero principal es una base con esquema (no una recién creada).
    main_schema: bool,
    /// `base_id` del fichero principal leído solo.
    main_id: Option<String>,
    /// Se aplica un `-wal` dudoso sin haber podido copiar antes la base (`backup_beside`): en este
    /// arranque no se vuelca con `TRUNCATE`.
    no_truncate: bool,
}

/// Guarda del incidente del 13-oct (`specs/rompedor/2026-10-22.md` ALTA 1 y `-b.md` ALTA 1, MEDIA 2 y
/// MEDIA 4; `-c/datos.md` ALTA 1, MEDIA 4 y BAJA 5; `2026-10-23/datos.md` MEDIA 1 y 2). SQLite aplica
/// cualquier `-wal` que encuentre junto a la base: el de otra base enseña esa otra y el primer cierre
/// limpio la vuelca encima de la buena. Se decide por IDENTIDAD, no contando filas (un `-wal` ajeno
/// con 4 dictados pasaba; el propio tras «Borrar historial» se apartaba):
/// - una cabecera con la suma bien y una versión desconocida → SQLite no abriría la base: se aparta;
/// - tamaño de página del `-wal` ≠ el del principal → ajeno (no es de esta base, y con él la base se
///   lee dañada: antes se apartaba la base BUENA entera);
/// - el `base_id` del fichero principal leído solo (`immutable=1`) frente al de la última página de
///   `meta` confirmada en el `-wal` (`wal_identity`, decodificada): distintos → ajeno; iguales → propio;
/// - si no se puede saber así (principal sin `meta`, ningún marco con `meta` o una página de `meta`
///   que no se lee), lo decide lo que se vería con él (`foreign_by_view`). Un `-wal` sin `meta` junto
///   a una base identificada ya no se aparta sin más: lo escriben también Pronto 2.0.2 (quien vuelve
///   a la versión anterior) y un editor de SQLite, y se perdían de la vista esos cambios con un aviso
///   que decía «Tus datos están intactos» (rompedor 23-oct, datos MEDIA 1). Lo que se da por propio
///   así se aplica tras copiar la base y el `-wal` a `respaldo-<fecha-hora>\` (`backup_beside`).
/// Lo ajeno se aparta (nunca se borra), `-wal` y `-shm` juntos o ninguno, a `apartado-<fecha-hora>\`.
/// No poder leer el `-wal` o la vista (p. ej. el `-shm` cogido por otro programa) NO es prueba de
/// ajeno: error claro y no se toca nada.
fn guard_foreign_wal(path: &std::path::Path) -> Result<Guard> {
    let mut g = Guard::default();
    if file_len(path) == 0 {
        return Ok(g);
    }
    let Ok(main) = open_immutable(path).and_then(|c| look(&c)) else { return Ok(g) }; // no legible: lo decide la apertura normal
    g.main_schema = main.version > 0;
    g.main_id = main.id.clone();
    let wal = sidecar(path, "-wal");
    let wal_len = file_len(&wal);
    if wal_len == 0 {
        return Ok(g);
    }
    let (main_page, reserve) = main_page_size(path).unwrap_or((0, 0));
    let scan = wal_identity(&wal, main.meta_root, reserve)
        .with_context(|| format!("no se pudo leer {} (¿lo tiene abierto otro programa?); no se ha tocado nada", wal.display()))?;
    let mut seen_rows = None;
    let foreign = if let Some(v) = scan.unknown_version {
        Some(format!("tiene un formato que esta versión de Pronto no sabe leer (versión {v} del registro), y con él la base no se podría abrir"))
    } else if scan.frames == 0 {
        return Ok(g); // SQLite no aplicará nada de ese -wal
    } else if main_page != 0 && scan.page != main_page {
        Some(format!("es de otra base (páginas de {} bytes; las de tu base son de {main_page})", scan.page))
    } else {
        match (&main.id, &scan.meta) {
            (Some(m), WalMeta::Id(w)) if m != w => Some(format!("es de otra base (identidad {w}; la tuya es {m})")),
            (Some(_), WalMeta::Id(_)) => None,
            // Sin `meta` donde la tiene el principal, pero sí en otra página y con otra identidad: otra
            // disposición de páginas, otra base (la viva junto a una copia restaurada encima). Antes
            // se apartaba por `Missing`; ahora `Missing` lo decide la vista y esta no siempre lo nota.
            (Some(m), _) if scan.stray.as_ref().is_some_and(|w| w != m) => {
                Some(format!("es de otra base (identidad {}; la tuya es {m})", scan.stray.as_deref().unwrap_or_default()))
            }
            _ => {
                let (why, rows) = foreign_by_view(path, &main)?;
                seen_rows = rows;
                if why.is_none() && g.main_schema {
                    let doubt = match &main.id {
                        Some(_) => "el registro no lleva la identidad de la base (lo escribió una versión anterior de Pronto o un editor de SQLite)",
                        None => "la base es de una versión anterior de Pronto y aún no tiene identidad",
                    };
                    match backup_beside(path, doubt) {
                        Ok(d) => eprintln!("[db] -wal dado por propio sin identidad ({doubt}): copia de la base y del -wal en {}", d.display()),
                        Err(e) => {
                            eprintln!("[db] -wal dado por propio sin identidad ({doubt}) y no se pudo copiar antes la base: {e:#}. Este arranque no vuelca el -wal");
                            g.no_truncate = true;
                        }
                    }
                }
                why
            }
        }
    };
    let Some(why) = foreign else { return Ok(g) };
    let dest = set_aside_all(path, &["-wal", "-shm"])?;
    // Nunca «Tus datos están intactos»: lo apartado puede tener cambios del usuario (rompedor 23-oct,
    // datos MEDIA 1). Se dice qué se apartó, dónde, y qué abre Pronto.
    let seen = seen_rows.map(|n| format!(" Con él se veían {n} filas de datos; en el fichero de tu base hay {}.", main.rows)).unwrap_or_default();
    let aviso = format!(
        "Al arrancar había junto a la base un registro de cambios (-wal, {wal_len} bytes) que no era suyo: {why}. Se ha apartado entero, sin borrar nada, a {}.{seen} Pronto abre tu base tal y como estaba guardada en {}; si ese registro tenía cambios tuyos que ahora no ves, siguen en esa carpeta.",
        dest.display(),
        path.display()
    );
    let _ = std::fs::write(dest.join("LEEME.txt"), format!("{aviso}\r\n\r\nBase: {}\r\n", path.display()));
    eprintln!("[db] {aviso}");
    g.aviso = Some(aviso);
    Ok(g)
}

/// Huella de una celda (para comparar sin guardar los textos).
fn cell_hash(v: rusqlite::types::ValueRef) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    match v {
        rusqlite::types::ValueRef::Null => 0u8.hash(&mut h),
        rusqlite::types::ValueRef::Integer(i) => i.hash(&mut h),
        rusqlite::types::ValueRef::Real(f) => f.to_bits().hash(&mut h),
        rusqlite::types::ValueRef::Text(t) | rusqlite::types::ValueRef::Blob(t) => t.hash(&mut h),
    }
    h.finish()
}

/// ¿El `-wal` reescribe los dictados que ya estaban en el fichero principal? Un `-wal` propio añade,
/// borra o corrige alguno (reintento de transcripción, un editor), pero nunca cambia la fecha de un
/// dictado (`created_at` no se reescribe nunca) ni el texto de la mayoría de los que siguen ahí. Uno
/// ajeno de una base con la misma disposición de páginas pone OTROS dictados con los mismos id
/// (rompedor 23-oct, datos MEDIA 2: con contadores iguales o más altos, `lowered_seq` no lo veía).
/// `Some(motivo)` si los reescribe.
fn history_rewritten(main: &Connection, view: &Connection) -> rusqlite::Result<Option<String>> {
    if !table_exists(main, "history")? || !table_exists(view, "history")? {
        return Ok(None);
    }
    let read = |c: &Connection| -> rusqlite::Result<Vec<(i64, u64, u64)>> {
        c.prepare("SELECT id, created_at, text FROM history ORDER BY id")?
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, cell_hash(r.get_ref(1)?), cell_hash(r.get_ref(2)?))))?
            .collect()
    };
    let (a, b) = (read(main)?, read(view)?);
    let (mut i, mut j, mut both, mut text) = (0, 0, 0usize, 0usize);
    while i < a.len() && j < b.len() {
        match a[i].0.cmp(&b[j].0) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                if a[i].1 != b[j].1 {
                    return Ok(Some(format!("enseña otra base (el dictado {} cambia de fecha, y Pronto nunca la cambia)", a[i].0)));
                }
                both += 1;
                text += (a[i].2 != b[j].2) as usize;
                i += 1;
                j += 1;
            }
        }
    }
    Ok((both >= 3 && text * 2 > both).then(|| format!("enseña otra base (de {both} dictados que están en los dos, {text} tienen otro texto)")))
}

/// El `-wal` no dice de qué base es: lo que se vería con él, sin volcarlo al cerrar. (`Some(motivo)`
/// si es ajeno, filas de datos que se verían).
fn foreign_by_view(path: &std::path::Path, main: &Look) -> Result<(Option<String>, Option<i64>)> {
    let view = (|| -> rusqlite::Result<(Look, Option<String>, Option<String>)> {
        let conn = Connection::open(path)?;
        conn.set_db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true)?;
        let v = look(&conn)?;
        let main_conn = open_immutable(path).ok();
        let rewritten = match &main_conn {
            Some(m) => history_rewritten(m, &conn)?,
            None => None,
        };
        // Lo que se da por propio así se va a aplicar: con él la base tiene que leerse sana. Un
        // `-wal` de otra disposición de páginas puede enseñar `meta` y contadores coherentes y aun así
        // mezclar páginas; si la base sola está sana y con él no, el culpable es el `-wal` (antes se
        // apartaba la base entera como «dañada» y Pronto arrancaba vacío).
        let broken = match damage(&conn)? {
            Some(why) if main_conn.as_ref().is_some_and(|m| matches!(damage(m), Ok(None))) => Some(why),
            _ => None,
        };
        Ok((v, rewritten, broken))
    })();
    Ok(match view {
        Ok((v, rewritten, broken)) => {
            let why = match (&main.id, v.lowered_seq(main)) {
                (Some(m), _) if v.id.as_ref() != Some(m) => Some(format!("enseña otra base ({})", v.id.as_deref().unwrap_or("sin identidad"))),
                (_, Some((t, now, was))) => Some(format!("enseña otra base (el contador de «{t}» baja de {was} a {now}, y uno propio nunca baja)")),
                _ if rewritten.is_some() => rewritten,
                _ if broken.is_some() => broken.map(|b| format!("con él tu base se lee dañada; sin él, no (la base {b})")),
                (Some(_), None) => None,
                (None, None) => (main.rows > 0 && v.rows == 0 && v.ever == 0)
                    .then(|| format!("enseña una base recién creada ({} filas, versión {}) en vez de tus datos ({} filas, versión {})", v.rows, v.version, main.rows, main.version)),
            };
            (why, Some(v.rows))
        }
        Err(rusqlite::Error::SqliteFailure(e, _)) if matches!(e.code, rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked) => {
            anyhow::bail!("la base {} está ocupada por otro proceso", path.display())
        }
        Err(rusqlite::Error::SqliteFailure(e, _)) if matches!(e.code, rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase) => {
            (Some("deja la base ilegible".to_string()), None)
        }
        Err(e) => anyhow::bail!(
            "no se pudo leer la base {} con su registro de cambios (-wal/-shm): {e}. ¿Lo tiene abierto otro programa? No se ha tocado nada",
            path.display()
        ),
    })
}

// ------------------------------------------------------------ base vacía o dañada (BAJA 11)

/// `None` si la base pasa `PRAGMA quick_check`; si no, el motivo. Un fichero que no es una base
/// SQLite (`NotADatabase`) da `Err`: no se toca (regla 2).
fn damage(conn: &Connection) -> rusqlite::Result<Option<String>> {
    match conn.query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0)) {
        Ok(s) if s == "ok" => Ok(None),
        Ok(s) => Ok(Some(format!("está dañada (PRAGMA quick_check: {})", s.chars().take(200).collect::<String>()))),
        Err(rusqlite::Error::SqliteFailure(e, m)) if e.code == rusqlite::ErrorCode::DatabaseCorrupt => {
            Ok(Some(format!("está dañada ({})", m.unwrap_or_else(|| e.to_string()))))
        }
        Err(e) => Err(e),
    }
}

/// Copia más reciente con datos en `copias\` junto a la base (las normales, `-anterior` incluidas,
/// antes que las `-sospechosa`), con sus filas de datos; y, si hay una `-sospechosa` más nueva que
/// esa, también esa como alternativa: tras restaurar una copia o empezar otra base, lo último del
/// usuario puede estar ahí (rompedor 23-oct, datos MEDIA 4). Se mira el nombre del fichero, no la
/// ruta entera (una carpeta con «sospechosa» en el nombre lo confundía todo).
fn latest_copy(base: &std::path::Path) -> Option<((PathBuf, i64), Option<(PathBuf, i64)>)> {
    let dir = base.parent()?.join("copias");
    let name = |p: &PathBuf| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("pronto2-") && n.ends_with(".db")))
        .collect();
    v.sort_by_key(|p| name(p));
    let rows = |p: &PathBuf| open_immutable(p).and_then(|c| census(&c)).ok().map(|(_, n)| n).filter(|n| *n > 0);
    let pick = |suspicious: bool| v.iter().rev().filter(|p| name(p).contains("sospechosa") == suspicious).find_map(|p| rows(p).map(|n| (p.clone(), n)));
    match pick(false) {
        Some(best) => {
            let newer = pick(true).filter(|(s, _)| name(s) > name(&best.0));
            Some((best, newer))
        }
        None => pick(true).map(|s| (s, None)),
    }
}

/// Aparta (nunca borra) la base vacía o dañada con su `-wal`/`-shm` y devuelve el aviso.
fn set_aside_base(path: &std::path::Path, why: &str) -> Result<String> {
    let dest = set_aside_all(path, &["", "-wal", "-shm"])?;
    let aviso = format!(
        "La base de datos ({}) {why}. Se ha apartado, sin borrarla, a {} y Pronto ha empezado con una base nueva. {}",
        path.display(),
        dest.display(),
        copy_advice(path)
    );
    let _ = std::fs::write(dest.join("LEEME.txt"), format!("{aviso}\r\n"));
    eprintln!("[db] {aviso}");
    Ok(aviso)
}

/// Qué copia de `copias\` restaurar y cómo. Restaurar sin apartar también el -wal y el -shm de la
/// base viva los mezclaba con la copia (rompedor 22-oct-c, datos MEDIA 2): el aviso lo dice.
fn copy_advice(path: &std::path::Path) -> String {
    let name = base_name(path);
    match latest_copy(path) {
        Some(((p, n), newer)) => {
            let alt = newer
                .map(|(s, m)| format!(" Hay otra más nueva, {} ({m} filas), que Pronto guardó aparte porque no se parecía a la anterior: si lo último que hiciste está ahí, restaura esa.", s.display()))
                .unwrap_or_default();
            format!(
                "La copia más reciente es {} ({n} filas de datos): para recuperarla, cierra Pronto (Salir), aparta también {name}-wal y {name}-shm si están, y cópiala encima de {}.{alt}",
                p.display(),
                path.display()
            )
        }
        None => "No hay ninguna copia con datos en la carpeta copias.".to_string(),
    }
}

/// Borra filas de `history` sin el disparador por fila de `touch_triggers`, dentro de la transacción
/// `tx`, que tiene que escribir `meta` ella misma. Con el disparador, «Borrar historial» de 100 000
/// filas iba ×35 más lento (sumaba 100 000 `UPDATE meta` y desactivaba el borrado rápido de
/// `DELETE` sin `WHERE`) y el -wal crecía a 24 MB (rompedor 22-oct-c, datos BAJA 8). El disparador
/// se quita y se vuelve a poner en la misma transacción: si algo falla, vuelve con el rollback.
fn delete_history_rows<P: rusqlite::Params>(tx: &Connection, sql: &str, p: P) -> rusqlite::Result<usize> {
    tx.execute_batch("DROP TRIGGER IF EXISTS temp.\"pronto_toque_DELETE_history\"")?;
    let n = tx.execute(sql, p)?;
    tx.execute_batch(&touch_trigger_sql("history", "DELETE"))?;
    Ok(n)
}

/// Linaje de una base para comparar copias: `copia_de` si es una copia restaurada (o una copia),
/// si no su `base_id`.
fn lineage(conn: &Connection) -> rusqlite::Result<Option<String>> {
    Ok(read_meta(conn, "copia_de")?.or(read_meta(conn, "base_id")?))
}

/// Datos de un dictado nuevo.
pub struct NewHistory<'a> {
    pub polished: bool,
    pub text: &'a str,
    pub raw: &'a str,
    pub app: &'a str,
    pub duration_ms: i64,
    pub words: i64,
    pub fixes: Fixes,
    pub transform: Option<&'a str>,
}

impl Db {
    pub fn open() -> Result<Self> {
        Self::open_at(&db_path()?)
    }

    /// Abre (o crea) la base en `path` y la migra. `open()` la usa con la ruta de `%APPDATA%`; las
    /// pruebas de integridad (regla 2 de CLAUDE.md), con una base temporal.
    ///
    /// Antes de abrir: una `pronto2.db` de 0 bytes o que no pasa `PRAGMA quick_check` no se abre en
    /// silencio: se aparta con su `-wal`/`-shm`, se empieza una base nueva y queda un aviso con la
    /// copia más reciente de `copias\` (rompedor 22-oct-b, BAJA 11). Un fichero que no es una base
    /// SQLite da `Err` y no se toca. Y la guarda del `-wal` ajeno (`guard_foreign_wal`). Una base sin
    /// `base_id` en el fichero principal (de antes o nueva) sale con él escrito y volcado en él.
    pub fn open_at(path: &std::path::Path) -> Result<Self> {
        let mut avisos = Vec::new();
        if std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.len() == 0) {
            // Con un -wal al lado también: SQLite BORRA el -wal de una base de 0 páginas al abrirla, y
            // ese -wal era lo único que quedaba de los últimos cambios (rompedor 22-oct-c, datos MEDIA 3).
            let wal = file_len(&sidecar(path, "-wal"));
            let why = if wal > 0 { format!("estaba vacía (0 bytes) junto a su registro de cambios (-wal de {wal} bytes)") } else { "estaba vacía (0 bytes)".to_string() };
            avisos.push(set_aside_base(path, &why)?);
        } else if !path.exists() && file_len(&sidecar(path, "-wal")) > 0 {
            // `-wal` con datos y la base no está (antivirus en cuarentena, un sincronizador, el
            // usuario la ha movido para restaurar otra): SQLite lo borraba al crear la base nueva, y
            // eran los últimos cambios. Se aparta con su `-shm` (rompedor 23-oct, datos BAJA 6).
            let wal = file_len(&sidecar(path, "-wal"));
            let dest = set_aside_all(path, &["-wal", "-shm"])?;
            let aviso = format!(
                "No estaba la base de datos ({}), pero sí su registro de cambios (-wal de {wal} bytes, los últimos cambios que se hicieron en ella). Se ha apartado, sin borrarlo, a {} y Pronto ha empezado con una base nueva. Si la base está en otro sitio (la movió un antivirus, un sincronizador o tú), cierra Pronto (Salir), aparta {} y vuelve a poner la base y ese registro juntos en su sitio. {}",
                path.display(),
                dest.display(),
                path.display(),
                copy_advice(path)
            );
            let _ = std::fs::write(dest.join("LEEME.txt"), format!("{aviso}\r\n"));
            eprintln!("[db] {aviso}");
            avisos.push(aviso);
        }
        let existed = file_len(path) > 0;
        let mut guard = guard_foreign_wal(path)?;
        avisos.extend(guard.aviso.take());
        let open = || Connection::open(path).with_context(|| format!("no se pudo abrir {}", path.display()));
        let mut conn = open()?;
        if existed {
            let check = damage(&conn).map_err(|e| {
                let why = match &e {
                    rusqlite::Error::SqliteFailure(f, _) if f.code == rusqlite::ErrorCode::NotADatabase => "no es una base de datos SQLite",
                    _ => "¿la tiene abierta otro programa, o su -wal/-shm?",
                };
                anyhow::Error::new(e).context(format!("no se pudo abrir {} ({why}); no se ha tocado nada", path.display()))
            });
            if let Some(why) = check? {
                // Cerrar sin volcar el -wal en la base dañada: se aparta tal cual estaba.
                conn.set_db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true)?;
                drop(conn);
                avisos.push(set_aside_base(path, &why)?);
                guard = Guard::default();
                conn = open()?;
            }
        }
        let mut db = Self::init(conn)?;
        // Base con esquema cuyo `base_id` no es el del fichero principal (base de antes, o `meta`
        // borrada por otro programa): se vuelca ya, con lo que hubiera en su -wal (que la guarda ha
        // dado por suyo). Si no, un apagón antes del volcado dejaba el id nuevo solo en el -wal y el
        // siguiente arranque lo tomaba por ajeno (rompedor 22-oct-c, datos BAJA 6). Una base recién
        // creada no: su principal no tiene `meta` y la guarda la juzga por la vista.
        // Una copia restaurada que se adopta (`ensure_identity`) también. Un `-wal` dudoso que no se
        // pudo copiar antes (`Guard::no_truncate`) no: en la duda, no se vuelca.
        let id_now = read_meta(&db.conn.lock(), "base_id")?;
        if guard.main_schema && id_now != guard.main_id && !guard.no_truncate {
            db.checkpoint_truncate();
        }
        db.aviso = (!avisos.is_empty()).then(|| avisos.join(" "));
        Ok(db)
    }

    #[cfg(test)]
    pub fn open_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Self> {
        // `journal_size_limit`: tras un volcado el -wal se recorta a 4 MB como mucho; sin él, uno que
        // creció a 24 MB («Borrar historial» de 100 000 filas) se quedaba así para siempre (rompedor
        // 22-oct-c, datos BAJA 8).
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA journal_size_limit = 4194304;")?;
        register_fold(&conn)?;
        let before: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        migrate(&mut conn)?;
        ensure_identity(&conn)?;
        // Una migración que abre una tanda del -wal también lleva la página de `meta` (ver `touch_triggers`).
        if conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))? != before {
            touch_meta(&conn)?;
        }
        Ok(Self::from_conn(conn))
    }

    fn from_conn(conn: Connection) -> Self {
        Self { search: Mutex::new(None), conn: Mutex::new(conn), aviso: None }
    }

    /// Conexión de búsqueda (ver `Db::search`), abierta la primera vez: solo lectura
    /// (`query_only`) y sin volcar al cerrar. `None` si la base no es un fichero (en memoria) o no
    /// se puede abrir: entonces se busca por `conn`, como antes.
    fn search_conn(&self) -> Option<parking_lot::MutexGuard<'_, Option<Connection>>> {
        let mut g = self.search.lock();
        if g.is_none() {
            let path = self.conn.lock().path().filter(|p| !p.is_empty()).map(PathBuf::from)?;
            let c = (|| -> rusqlite::Result<Connection> {
                let c = Connection::open(&path)?;
                c.set_db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true)?;
                c.execute_batch("PRAGMA query_only = 1;")?;
                register_fold(&c)?;
                Ok(c)
            })();
            match c {
                Ok(c) => *g = Some(c),
                Err(e) => {
                    eprintln!("[db] no se pudo abrir la conexión de búsqueda ({e}); se busca por la principal");
                    return None;
                }
            }
        }
        Some(g)
    }

    /// Aviso del arranque para el usuario, si lo hubo (ver `guard_foreign_wal` y `open_at`).
    pub fn aviso(&self) -> Option<&str> {
        self.aviso.as_deref()
    }

    /// Vuelca el `-wal` en el fichero principal sin esperar a nadie (`PASSIVE`: lo que no se pueda,
    /// se queda para la próxima). Pronto no volcaba nunca (en el PC de Txonso, `pronto2.db` del
    /// 15-oct y su `-wal` de 1,8 MB del 22-oct): se llama tras la copia diaria y conviene llamarlo al
    /// salir (bandeja, `RunEvent::Exit`, `WM_ENDSESSION`).
    pub fn checkpoint(&self) {
        if let Err(e) = self.conn.lock().query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |_| Ok(())) {
            eprintln!("[db] volcado del -wal: {e}");
        }
    }

    /// Volcado entero y `-wal` a 0 bytes, sin esperar a otros lectores (si los hay, se queda a medias).
    fn checkpoint_truncate(&self) {
        let conn = self.conn.lock();
        let _ = conn.busy_timeout(std::time::Duration::ZERO);
        match conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get::<_, i64>(0)) {
            Ok(0) => {}
            Ok(_) => eprintln!("[db] el volcado del -wal quedó a medias (otro lector); se reintenta al próximo arranque"),
            Err(e) => eprintln!("[db] volcado del -wal: {e}"),
        }
        let _ = conn.busy_timeout(std::time::Duration::from_secs(5));
    }

    // ------------------------------------------------------------ ajustes

    /// Lee los ajustes; migra las filas antiguas (`removeFillers`/`backtrack` -> `cleanupLevel`).
    pub fn load_settings(&self) -> Settings {
        let json: Option<String> = self
            .conn
            .lock()
            .query_row("SELECT json FROM settings WHERE id = 1", [], |r| r.get(0))
            .optional()
            .unwrap_or(None);
        let Some(json) = json else { return Settings::default() };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&json) else { return Settings::default() };
        let (mut settings, rejected) = settings_from_value(&value);
        if !rejected.is_empty() {
            eprintln!("[db] ajustes que esta versión no entiende (toman su valor de fábrica): {}", rejected.join(", "));
        }
        if value.get("cleanupLevel").is_none() {
            let fillers = value.get("removeFillers").and_then(|v| v.as_bool());
            let backtrack = value.get("backtrack").and_then(|v| v.as_bool());
            if fillers.is_some() || backtrack.is_some() {
                settings.cleanup_level = match (fillers.unwrap_or(true), backtrack.unwrap_or(true)) {
                    (false, false) => CleanupLevel::None,
                    (true, false) => CleanupLevel::Light,
                    _ => CleanupLevel::Medium,
                };
            }
        }
        // `meetingDetect` (booleano) -> `meetingAuto`: true = auto, false = off.
        if value.get("meetingAuto").is_none() {
            if let Some(detect) = value.get("meetingDetect").and_then(|v| v.as_bool()) {
                settings.meeting_auto = if detect { MeetingAuto::Auto } else { MeetingAuto::Off };
            }
        }
        settings.meeting_detect = settings.meeting_auto != MeetingAuto::Off;
        // El atajo de fábrica del Transform cambió (Win+Alt+1 -> Alt+Shift+P). Solo se migra si lo
        // guardado es exactamente el antiguo de fábrica; uno cambiado por el usuario se respeta.
        if settings.hotkey_transform == legacy_transform_hotkey() {
            settings.hotkey_transform = default_transform_hotkey();
        }
        settings.model_dir = crate::modeldl::resolve_dir(&settings.model_dir);
        migrate_ai_models(&mut settings);
        settings
    }

    /// Guarda los ajustes. Si el JSON que hay guardado tiene algo que esta versión no entiende (de
    /// una versión más nueva, o dañado), antes de sobrescribirlo se copia entero a `settings_backup`.
    pub fn save_settings(&self, s: &Settings) -> Result<()> {
        let json = serde_json::to_string(s)?;
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        let old: Option<String> = tx.query_row("SELECT json FROM settings WHERE id = 1", [], |r| r.get(0)).optional()?;
        if let Some(old) = old {
            let lossy = serde_json::from_str::<serde_json::Value>(&old).map_or(true, |v| !settings_from_value(&v).1.is_empty() || has_unknown_keys(&v));
            if lossy {
                tx.execute_batch("CREATE TABLE IF NOT EXISTS settings_backup (id INTEGER PRIMARY KEY AUTOINCREMENT, saved_at TEXT NOT NULL, json TEXT NOT NULL)")?;
                tx.execute(
                    "INSERT INTO settings_backup (saved_at, json) SELECT ?1, ?2 WHERE NOT EXISTS (SELECT 1 FROM settings_backup WHERE json = ?2)",
                    params![now_iso(), old],
                )?;
            }
        }
        tx.execute("INSERT INTO settings (id, json) VALUES (1, ?1) ON CONFLICT(id) DO UPDATE SET json = excluded.json", params![json])?;
        tx.commit()?;
        Ok(())
    }

    // ------------------------------------------------------------ historial

    pub fn add_history(&self, n: &NewHistory) -> Result<HistoryItem> {
        let created_at = now_iso();
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO history (created_at, text, raw_text, app, duration_ms, words, transform, fix_dictionary, fix_snippets, fix_fillers, polished)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![created_at, n.text, n.raw, n.app, n.duration_ms, n.words, n.transform, n.fixes.dictionary, n.fixes.snippets, n.fixes.fillers, n.polished as i64],
        )?;
        Ok(HistoryItem {
            id: conn.last_insert_rowid(),
            created_at,
            text: n.text.into(),
            raw_text: n.raw.into(),
            app: n.app.into(),
            duration_ms: n.duration_ms,
            words: n.words,
            has_audio: false,
            polished: n.polished,
            transform: n.transform.map(String::from),
        })
    }

    pub fn get_history(&self, id: i64) -> Result<Option<HistoryItem>> {
        Ok(self
            .conn
            .lock()
            .query_row(&format!("SELECT {HISTORY_COLS} FROM history WHERE id = ?1"), params![id], history_row)
            .optional()?)
    }

    pub fn set_has_audio(&self, id: i64, has: bool) -> Result<()> {
        self.conn.lock().execute("UPDATE history SET has_audio = ?2 WHERE id = ?1", params![id, has as i64])?;
        Ok(())
    }

    /// Sustituye el texto de un dictado (reintento de transcripción).
    pub fn update_history_text(&self, id: i64, text: &str, raw: &str, words: i64) -> Result<()> {
        self.conn.lock().execute(
            "UPDATE history SET text = ?2, raw_text = ?3, words = ?4, polished = 0 WHERE id = ?1",
            params![id, text, raw, words],
        )?;
        Ok(())
    }

    pub fn list_history(&self, query: Option<&str>, limit: i64, offset: i64) -> Result<Vec<HistoryItem>> {
        // Sin tildes ni mayúsculas a los dos lados (`fold`, registrada en `init`): «óscar» encuentra
        // «Óscar» y «último», «ÚLTIMO» (rompedor 22-oct-c, BAJA 6: el LIKE de SQLite solo pliega ASCII).
        // Con texto, `fold_has` (literal: `%`, `_` y `\` no son comodines) y por la conexión de
        // búsqueda, que no frena al dictado que guarda en `conn` (rompedor 23-oct, dictado MEDIA 6).
        let needle = query.map(str::trim).filter(|q| !q.is_empty()).map(crate::text::fold);
        let run = |conn: &Connection| -> Result<Vec<HistoryItem>> {
            let filter = if needle.is_some() { "fold_has(text, ?1) OR fold_has(app, ?1)" } else { "?1 IS NULL" };
            let mut stmt = conn.prepare_cached(&format!(
                "SELECT {HISTORY_COLS} FROM history
                 WHERE ({filter})
                 ORDER BY id DESC LIMIT ?2 OFFSET ?3"
            ))?;
            let rows = stmt.query_map(params![needle, limit.clamp(1, 1000), offset.max(0)], history_row)?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        };
        if needle.is_some() {
            if let Some(g) = self.search_conn() {
                if let Some(c) = g.as_ref() {
                    return run(c);
                }
            }
        }
        run(&self.conn.lock())
    }

    /// Último dictado con texto (no transform) para «pegar/copiar último». Una transcripción fallida
    /// (fila sin texto, con su audio) no cuenta: si no, «copiar último» vaciaba el portapapeles
    /// (rompedor 22-oct-c, MEDIA 3).
    pub fn last_history_text(&self) -> Option<String> {
        self.conn
            .lock()
            .query_row("SELECT text FROM history WHERE transform IS NULL AND trim(text) <> '' ORDER BY id DESC LIMIT 1", [], |r| r.get(0))
            .optional()
            .unwrap_or(None)
    }

    pub fn delete_history(&self, id: i64) -> Result<()> {
        let created_at: Option<String> = {
            let conn = self.conn.lock();
            let c = conn.query_row("SELECT created_at FROM history WHERE id = ?1", params![id], |r| r.get(0)).optional()?;
            conn.execute("DELETE FROM history WHERE id = ?1", params![id])?;
            c
        };
        if let Some(c) = created_at {
            remove_audio_file((id, c));
        }
        Ok(())
    }

    pub fn clear_history(&self) -> Result<()> {
        let ids: Vec<(i64, String)> = {
            let mut conn = self.conn.lock();
            let tx = conn.transaction()?;
            let ids = {
                let mut stmt = tx.prepare("SELECT id, created_at FROM history WHERE has_audio = 1")?;
                stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.filter_map(|r| r.ok()).collect()
            };
            delete_history_rows(&tx, "DELETE FROM history", [])?;
            // Borrado del usuario, no un incidente: la copia diaria no lo toma por base menguada. Y
            // la tanda lleva `meta` (ver `touch_triggers`).
            write_meta(&tx, "borrado_masivo", &now_iso())?;
            tx.commit()?;
            ids
        };
        ids.into_iter().for_each(remove_audio_file);
        Ok(())
    }

    /// Borra lo anterior a `days` días (y sus WAV). 0 = conservar siempre.
    pub fn purge_history(&self, days: u32) -> Result<usize> {
        if days == 0 {
            return Ok(0);
        }
        let modifier = format!("-{days} days");
        let (ids, n) = {
            let mut conn = self.conn.lock();
            let tx = conn.transaction()?;
            let ids: Vec<(i64, String)> = {
                let mut stmt = tx.prepare("SELECT id, created_at FROM history WHERE has_audio = 1 AND datetime(created_at) < datetime('now', ?1)")?;
                stmt.query_map(params![modifier], |r| Ok((r.get(0)?, r.get(1)?)))?.filter_map(|r| r.ok()).collect()
            };
            let n = delete_history_rows(&tx, "DELETE FROM history WHERE datetime(created_at) < datetime('now', ?1)", params![modifier])?;
            if n > 0 {
                write_meta(&tx, "borrado_masivo", &now_iso())?;
            }
            tx.commit()?;
            (ids, n)
        };
        ids.into_iter().for_each(remove_audio_file);
        Ok(n)
    }

    /// Copia del día (`pronto2-AAAAMMDD.db`) con `VACUUM INTO`, consistente aunque haya escrituras,
    /// y deja solo las `keep` más recientes. Una base sin datos del usuario (los ajustes no cuentan)
    /// no se copia. Se copia aparte, `pronto2-AAAAMMDD-sospechosa.db`, sin rotar nada (así no
    /// desplaza, día a día, a las copias buenas; rompedor 22-oct, MEDIA 4), una base que, frente a la
    /// última copia buena, tiene menos de la mitad de datos sin un borrado del usuario después de esa
    /// copia: uno registrado desde Pronto («Borrar historial» o la retención, `meta.borrado_masivo`),
    /// restaurar una copia (`meta.restaurada_en`), o borrados sueltos, que se reconocen porque ningún
    /// contador de `sqlite_sequence` ha bajado desde la copia (una base tapada o una copia vieja
    /// restaurada sí lo bajan; rompedor 22-oct-c, datos BAJA 7). Tras un borrado del usuario las
    /// copias rotan normal y lo borrado sale de ellas en `keep` días (rompedor 22-oct-b, MEDIA 6).
    ///
    /// OTRA base (otro linaje: la nueva de un incidente, o una copia de 2.0.2 restaurada, que no tiene
    /// identidad) solo es sospechosa si además está menguada. Antes lo era siempre, y como las
    /// sospechosas no rotan, la «última copia buena» ya no cambiaba nunca: todas las copias salían
    /// sospechosas para siempre y el aviso de un incidente recomendaba la de antes de restaurar
    /// (rompedor 23-oct, datos MEDIA 4). Cuando la primera copia normal de un linaje nuevo entra, la
    /// última normal del linaje anterior se guarda aparte como `pronto2-AAAAMMDD-anterior.db`, que no
    /// rota nunca: nada de la base anterior se pierde por rotar.
    ///
    /// Cada copia tiene identidad propia: `base_id` nuevo y `copia_de` = el linaje de la base. Con el
    /// mismo `base_id`, restaurarla junto al `-wal` de la base viva hacía que la guarda lo diera por
    /// suyo y mezclara páginas (rompedor 22-oct-c, datos MEDIA 2); las copias se comparan por linaje.
    /// Se escribe primero como `.db-parcial` y se renombra al final: nunca queda una copia a medias
    /// con nombre de copia. Al terminar vuelca el `-wal` (`checkpoint`).
    pub fn daily_backup(&self, dir: &std::path::Path, keep: usize) -> Result<Option<PathBuf>> {
        let r = self.daily_backup_inner(dir, keep);
        self.checkpoint();
        r
    }

    fn daily_backup_inner(&self, dir: &std::path::Path, keep: usize) -> Result<Option<PathBuf>> {
        let day = chrono::Local::now().format("%Y%m%d").to_string();
        let path = dir.join(format!("pronto2-{day}.db"));
        // Una copia del día a medias (corte de luz o disco lleno de una versión que escribía la copia
        // con su nombre final) «existía» y el día quedaba sin copia útil (PRNT-41). Si no se puede
        // leer, se aparta como `.db-danada` (fuera de la rotación, sin borrarla) y se rehace.
        let set_aside_unreadable = |p: &std::path::Path| {
            if p.exists() && open_immutable(p).and_then(|c| look(&c)).is_err() {
                let _ = std::fs::rename(p, p.with_extension("db-danada"));
            }
        };
        set_aside_unreadable(&path);
        set_aside_unreadable(&dir.join(format!("pronto2-{day}-sospechosa.db")));
        if path.exists() {
            return Ok(None);
        }
        let (live, id, borrado, restaurada) = {
            let conn = self.conn.lock();
            (look(&conn)?, lineage(&conn)?, read_meta(&conn, "borrado_masivo")?, read_meta(&conn, "restaurada_en")?)
        };
        let rows = live.rows;
        if rows == 0 {
            return Ok(None);
        }
        // `-anterior`: la última normal de un linaje anterior, que no rota (ver arriba).
        let is_copy = |n: &str, suspicious: bool| n.starts_with("pronto2-") && n.ends_with(".db") && !n.ends_with("-anterior.db") && n.contains("sospechosa") == suspicious;
        let list = |suspicious: bool| -> Result<Vec<PathBuf>> {
            let mut v: Vec<PathBuf> = std::fs::read_dir(dir)?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| is_copy(n, suspicious)))
                .collect();
            v.sort();
            Ok(v)
        };
        // Lo que se ve, linaje, último borrado y última restauración de la copia normal más reciente
        // que se puede leer.
        let last_good = list(false)?.into_iter().rev().find_map(|p| {
            let c = open_immutable(&p).ok()?;
            let l = look(&c).ok().filter(|l| l.rows > 0)?;
            Some((p, l, lineage(&c).ok()?, read_meta(&c, "borrado_masivo").ok()?, read_meta(&c, "restaurada_en").ok()?))
        });
        let other_base = last_good.as_ref().is_some_and(|(_, _, good_id, _, _)| matches!((good_id, &id), (Some(a), Some(b)) if a != b));
        let suspicious = last_good.as_ref().is_some_and(|(_, good, good_id, good_borrado, good_restaurada)| {
            let same = good_id.is_some() && *good_id == id;
            let user_deleted = same
                && ((borrado.is_some() && borrado != *good_borrado) || (restaurada.is_some() && restaurada != *good_restaurada) || live.lowered_seq(good).is_none());
            rows * 2 < good.rows && !user_deleted
        });
        let target = if suspicious { dir.join(format!("pronto2-{day}-sospechosa.db")) } else { path };
        if target.exists() {
            return Ok(None);
        }
        let partial = target.with_extension("db-parcial");
        let _ = std::fs::remove_file(&partial); // restos de una copia que no terminó
        self.conn.lock().execute("VACUUM INTO ?1", params![partial.to_string_lossy()])?;
        let stamp = (|| -> rusqlite::Result<()> {
            let mut c = Connection::open(&partial)?;
            let tx = c.transaction()?;
            let new_id = new_base_id(&tx)?;
            write_meta(&tx, "base_id", &new_id)?;
            if let Some(l) = &id {
                write_meta(&tx, "copia_de", l)?;
            }
            tx.commit()
        })();
        if let Err(e) = stamp.map_err(anyhow::Error::from).and_then(|_| std::fs::rename(&partial, &target).map_err(anyhow::Error::from)) {
            let _ = std::fs::remove_file(&partial);
            return Err(e.context(format!("no se pudo terminar la copia {}", target.display())));
        }
        if let (true, Some((_, good, good_id, _, _))) = (suspicious, &last_good) {
            let good = good.rows;
            eprintln!(
                "[db] la base ({rows} filas de datos, linaje {id:?}) tiene menos de la mitad que la última copia buena ({good} filas, linaje {good_id:?}): copia aparte ({}) y no se borra ninguna copia buena",
                target.display()
            );
        }
        if let (false, true, Some((good_path, _, good_id, _, _))) = (suspicious, other_base, &last_good) {
            let keep_as = good_path.with_file_name(format!("{}-anterior.db", good_path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()));
            match std::fs::rename(good_path, &keep_as) {
                Ok(()) => eprintln!("[db] la base es de otro linaje ({id:?}) que la última copia buena ({good_id:?}): esa se guarda aparte, sin rotar, como {}", keep_as.display()),
                Err(e) => eprintln!("[db] no se pudo guardar aparte {} ({e}); queda como copia normal", good_path.display()),
            }
        }
        let mut copies = list(suspicious)?;
        while copies.len() > keep {
            let _ = std::fs::remove_file(copies.remove(0));
        }
        Ok(Some(target))
    }

    /// Borra el audio guardado con más de `days` días. 0 = no se guarda audio: se borra todo.
    pub fn purge_audio(&self, days: u32) -> Result<usize> {
        // La fecha límite se calcula aquí, en el mismo formato que `created_at` (RFC 3339 con la hora
        // local): `created_at < ?` usa `idx_history_created`; `datetime(created_at)` recorría la
        // tabla entera en cada arranque (rompedor 22-oct-c, datos BAJA 8). Con días = 0, todo (U+10FFFF
        // va detrás de cualquier fecha); con más días de los que caben en una fecha, nada.
        let cutoff = if days == 0 {
            "\u{10FFFF}".to_string()
        } else {
            match Local::now().checked_sub_signed(Duration::days(days as i64)) {
                Some(d) => d.to_rfc3339(),
                None => return Ok(0),
            }
        };
        let ids: Vec<(i64, String)> = {
            let conn = self.conn.lock();
            let mut stmt = conn.prepare("SELECT id, created_at FROM history WHERE created_at < ?1 AND has_audio = 1")?;
            let ids: Vec<(i64, String)> = stmt.query_map(params![cutoff], |r| Ok((r.get(0)?, r.get(1)?)))?.filter_map(|r| r.ok()).collect();
            for (id, _) in &ids {
                conn.execute("UPDATE history SET has_audio = 0 WHERE id = ?1", params![id])?;
            }
            ids
        };
        let n = ids.len();
        ids.into_iter().for_each(remove_audio_file);
        Ok(n)
    }

    // ------------------------------------------------------------ estadísticas

    fn per_day_words(&self) -> Result<HashMap<String, i64>> {
        let conn = self.conn.lock();
        // created_at se guarda con la hora local, así que los 10 primeros caracteres son el día local.
        let mut stmt = conn.prepare("SELECT substr(created_at,1,10), SUM(words) FROM history WHERE transform IS NULL GROUP BY 1")?;
        let map = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(map)
    }

    fn totals(&self) -> Result<(i64, i64, i64)> {
        Ok(self.conn.lock().query_row(
            "SELECT COALESCE(SUM(words),0), COALESCE(SUM(duration_ms),0), COUNT(*) FROM history WHERE transform IS NULL",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?)
    }

    pub fn stats(&self) -> Result<Stats> {
        let (total_words, total_ms, dictations) = self.totals()?;
        let per_day = self.per_day_words()?;
        let today = Local::now().date_naive();
        Ok(Stats {
            total_words,
            wpm: wpm(total_words, total_ms),
            streak_days: streak(&per_day, today),
            dictations,
            words_this_week: (0..7).map(|i| day_words(&per_day, today - Duration::days(i))).sum(),
            by_day: days_back(&per_day, today, 30),
        })
    }

    pub fn insights(&self) -> Result<Insights> {
        let (total_words, total_ms, dictations) = self.totals()?;
        let per_day = self.per_day_words()?;
        let today = Local::now().date_naive();
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT app, SUM(words), COUNT(*) FROM history WHERE transform IS NULL AND app != ''
             GROUP BY app ORDER BY SUM(words) DESC, COUNT(*) DESC LIMIT 8",
        )?;
        let top_apps = stmt
            .query_map([], |r| Ok(AppUsage { app: r.get(0)?, words: r.get(1)?, count: r.get(2)? }))?
            .filter_map(|r| r.ok())
            .collect();
        let fixes = conn.query_row(
            "SELECT COALESCE(SUM(fix_dictionary),0), COALESCE(SUM(fix_snippets),0), COALESCE(SUM(fix_fillers),0) FROM history",
            [],
            |r| Ok(Fixes { dictionary: r.get(0)?, snippets: r.get(1)?, fillers: r.get(2)? }),
        )?;
        Ok(Insights {
            total_words,
            wpm: wpm(total_words, total_ms),
            streak_days: streak(&per_day, today),
            longest_streak: longest_streak(&per_day),
            dictations,
            heatmap: days_back(&per_day, today, 365),
            top_apps,
            fixes,
        })
    }

    // ------------------------------------------------------------ diccionario

    /// Palabras con sus variantes (en el orden en que se grabaron).
    pub fn list_words(&self) -> Result<Vec<Word>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT id, word, replacement, created_at, starred, source FROM words ORDER BY word COLLATE NOCASE")?;
        let mut words = stmt.query_map([], word_row)?.collect::<std::result::Result<Vec<_>, _>>()?;
        let mut stmt = conn.prepare("SELECT word_id, id, heard FROM word_variants ORDER BY id")?;
        let mut by_word: HashMap<i64, Vec<WordVariant>> = HashMap::new();
        for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, WordVariant { id: r.get(1)?, heard: r.get(2)? })))? {
            let (word_id, v) = row?;
            by_word.entry(word_id).or_default().push(v);
        }
        let mut stmt = conn.prepare("SELECT word_id, id, duration_ms, created_at, file FROM word_samples ORDER BY id")?;
        let mut samples: HashMap<i64, Vec<VoiceSample>> = HashMap::new();
        for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, VoiceSample { id: r.get(1)?, duration_ms: r.get(2)?, created_at: r.get(3)?, file: r.get(4)? })))? {
            let (word_id, v) = row?;
            samples.entry(word_id).or_default().push(v);
        }
        for w in &mut words {
            w.variants = by_word.remove(&w.id).unwrap_or_default();
            w.samples = samples.remove(&w.id).unwrap_or_default();
        }
        Ok(words)
    }

    /// Alta de una palabra con sus variantes iniciales (las grabadas antes de guardar).
    #[cfg(test)]
    pub fn add_word(&self, word: &str, replacement: Option<&str>, variants: &[String]) -> Result<Word> {
        self.add_word_with_samples(word, replacement, variants, &[])
    }

    /// Como `add_word`, y además engancha las muestras de voz pendientes (`sample` de
    /// `pronounce_record`).
    pub fn add_word_with_samples(&self, word: &str, replacement: Option<&str>, variants: &[String], new_samples: &[String]) -> Result<Word> {
        let replacement = replacement.map(str::trim).filter(|r| !r.is_empty());
        let created_at = now_iso();
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO words (word, replacement, created_at, starred, source) VALUES (?1, ?2, ?3, 0, 'manual')",
            params![word.trim(), replacement, created_at],
        )?;
        let id = tx.last_insert_rowid();
        let mut saved = Vec::new();
        for v in variants {
            let v = clean_variant(v)?;
            let added = insert_variant(&tx, id, &v)?;
            if !saved.contains(&added) {
                saved.push(added);
            }
        }
        let mut samples = Vec::new();
        for f in new_samples {
            samples.push(attach_sample(&tx, id, f)?);
        }
        tx.commit()?;
        Ok(Word {
            id,
            word: word.trim().into(),
            replacement: replacement.map(String::from),
            created_at,
            starred: false,
            source: "manual".into(),
            variants: saved,
            samples,
        })
    }

    /// `variants: Some(lista)` deja exactamente esas variantes (conserva las que ya estaban);
    /// `None` no las toca.
    #[cfg(test)]
    pub fn update_word(&self, id: i64, word: &str, replacement: Option<&str>, variants: Option<&[String]>) -> Result<()> {
        self.update_word_with_samples(id, word, replacement, variants, None, &[])
    }

    /// Como `update_word`; `keep_samples: Some(ids)` deja solo esas muestras de voz de las que ya
    /// tenía (las demás se borran, fila y fichero) y `new_samples` engancha muestras pendientes.
    pub fn update_word_with_samples(
        &self,
        id: i64,
        word: &str,
        replacement: Option<&str>,
        variants: Option<&[String]>,
        keep_samples: Option<&[i64]>,
        new_samples: &[String],
    ) -> Result<()> {
        let replacement = replacement.map(str::trim).filter(|r| !r.is_empty());
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE words SET word = ?2, replacement = ?3 WHERE id = ?1",
            params![id, word.trim(), replacement],
        )?;
        if let Some(list) = variants {
            let wanted = list.iter().map(|v| clean_variant(v)).collect::<Result<Vec<_>>>()?;
            let mut keep: Vec<i64> = Vec::new();
            for v in &wanted {
                keep.push(insert_variant(&tx, id, v)?.id);
            }
            let current: Vec<i64> = {
                let mut stmt = tx.prepare("SELECT id FROM word_variants WHERE word_id = ?1")?;
                let ids = stmt.query_map(params![id], |r| r.get(0))?.collect::<std::result::Result<_, _>>()?;
                ids
            };
            for vid in current.into_iter().filter(|v| !keep.contains(v)) {
                tx.execute("DELETE FROM word_variants WHERE id = ?1", params![vid])?;
            }
        }
        let mut gone = Vec::new();
        if let Some(keep) = keep_samples {
            let current: Vec<(i64, String)> = {
                let mut stmt = tx.prepare("SELECT id, file FROM word_samples WHERE word_id = ?1")?;
                let rows = stmt.query_map(params![id], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<std::result::Result<_, _>>()?;
                rows
            };
            for (sid, file) in current.into_iter().filter(|(s, _)| !keep.contains(s)) {
                tx.execute("DELETE FROM word_samples WHERE id = ?1", params![sid])?;
                gone.push(file);
            }
        }
        for f in new_samples {
            attach_sample(&tx, id, f)?;
        }
        tx.commit()?;
        // Los ficheros, solo con la base ya al día.
        gone.iter().for_each(|f| remove_sample_file(f));
        Ok(())
    }

    pub fn toggle_word_star(&self, id: i64) -> Result<()> {
        self.conn.lock().execute("UPDATE words SET starred = 1 - starred WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn delete_word(&self, id: i64) -> Result<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        let files: Vec<String> = {
            let mut stmt = tx.prepare("SELECT file FROM word_samples WHERE word_id = ?1")?;
            let rows = stmt.query_map(params![id], |r| r.get(0))?.collect::<std::result::Result<_, _>>()?;
            rows
        };
        tx.execute("DELETE FROM word_variants WHERE word_id = ?1", params![id])?;
        tx.execute("DELETE FROM word_samples WHERE word_id = ?1", params![id])?;
        tx.execute("DELETE FROM words WHERE id = ?1", params![id])?;
        tx.commit()?;
        files.iter().for_each(|f| remove_sample_file(f));
        Ok(())
    }

    /// Engancha una muestra de voz pendiente a una palabra ya guardada.
    pub fn add_word_sample(&self, word_id: i64, file: &str) -> Result<VoiceSample> {
        let conn = self.conn.lock();
        let exists: Option<i64> = conn.query_row("SELECT id FROM words WHERE id = ?1", params![word_id], |r| r.get(0)).optional()?;
        if exists.is_none() {
            anyhow::bail!("Esa palabra ya no está en el diccionario");
        }
        attach_sample(&conn, word_id, file)
    }

    /// Borra una muestra de voz (fila y fichero).
    pub fn delete_word_sample(&self, id: i64) -> Result<()> {
        let conn = self.conn.lock();
        let file: Option<String> = conn.query_row("SELECT file FROM word_samples WHERE id = ?1", params![id], |r| r.get(0)).optional()?;
        conn.execute("DELETE FROM word_samples WHERE id = ?1", params![id])?;
        if let Some(f) = file {
            remove_sample_file(&f);
        }
        Ok(())
    }

    /// Fichero de una muestra (para oírla).
    pub fn word_sample_file(&self, id: i64) -> Result<Option<String>> {
        Ok(self.conn.lock().query_row("SELECT file FROM word_samples WHERE id = ?1", params![id], |r| r.get(0)).optional()?)
    }

    /// Añade una variante a una palabra que ya existe (normalizada; si ya la tenía, devuelve esa).
    pub fn add_word_variant(&self, word_id: i64, heard: &str) -> Result<WordVariant> {
        let heard = clean_variant(heard)?;
        let conn = self.conn.lock();
        let exists: Option<i64> = conn.query_row("SELECT id FROM words WHERE id = ?1", params![word_id], |r| r.get(0)).optional()?;
        if exists.is_none() {
            anyhow::bail!("Esa palabra ya no está en el diccionario");
        }
        insert_variant(&conn, word_id, &heard)
    }

    pub fn delete_word_variant(&self, id: i64) -> Result<()> {
        self.conn.lock().execute("DELETE FROM word_variants WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ------------------------------------------------------------ snippets

    pub fn list_snippets(&self) -> Result<Vec<Snippet>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT id, trigger, expansion, created_at FROM snippets ORDER BY trigger COLLATE NOCASE")?;
        let rows = stmt.query_map([], |r| {
            Ok(Snippet { id: r.get(0)?, trigger: r.get(1)?, expansion: r.get(2)?, created_at: r.get(3)? })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn add_snippet(&self, trigger: &str, expansion: &str) -> Result<Snippet> {
        let created_at = now_iso();
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO snippets (trigger, expansion, created_at) VALUES (?1, ?2, ?3)",
            params![trigger.trim(), expansion, created_at],
        )?;
        Ok(Snippet { id: conn.last_insert_rowid(), trigger: trigger.trim().into(), expansion: expansion.into(), created_at })
    }

    pub fn update_snippet(&self, id: i64, trigger: &str, expansion: &str) -> Result<()> {
        self.conn.lock().execute(
            "UPDATE snippets SET trigger = ?2, expansion = ?3 WHERE id = ?1",
            params![id, trigger.trim(), expansion],
        )?;
        Ok(())
    }

    pub fn delete_snippet(&self, id: i64) -> Result<()> {
        self.conn.lock().execute("DELETE FROM snippets WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ------------------------------------------------------------ notas

    pub fn list_notes(&self) -> Result<Vec<Note>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT id, text, created_at, updated_at FROM notes ORDER BY updated_at DESC, id DESC")?;
        let rows = stmt.query_map([], |r| {
            Ok(Note { id: r.get(0)?, text: r.get(1)?, created_at: r.get(2)?, updated_at: r.get(3)? })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn save_note(&self, id: Option<i64>, text: &str) -> Result<Note> {
        let now = now_iso();
        let conn = self.conn.lock();
        if let Some(id) = id {
            let n = conn.execute("UPDATE notes SET text = ?2, updated_at = ?3 WHERE id = ?1", params![id, text, now])?;
            if n > 0 {
                return Ok(conn.query_row(
                    "SELECT id, text, created_at, updated_at FROM notes WHERE id = ?1",
                    params![id],
                    |r| Ok(Note { id: r.get(0)?, text: r.get(1)?, created_at: r.get(2)?, updated_at: r.get(3)? }),
                )?);
            }
        }
        conn.execute("INSERT INTO notes (text, created_at, updated_at) VALUES (?1, ?2, ?2)", params![text, now])?;
        Ok(Note { id: conn.last_insert_rowid(), text: text.into(), created_at: now.clone(), updated_at: now })
    }

    pub fn delete_note(&self, id: i64) -> Result<()> {
        self.conn.lock().execute("DELETE FROM notes WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ------------------------------------------------------------ transforms

    pub fn list_transforms(&self) -> Result<Vec<Transform>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT id, name, prompt, builtin, hotkey_index FROM transforms ORDER BY builtin DESC, id")?;
        let rows = stmt.query_map([], transform_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn get_transform(&self, id: i64) -> Result<Option<Transform>> {
        Ok(self
            .conn
            .lock()
            .query_row("SELECT id, name, prompt, builtin, hotkey_index FROM transforms WHERE id = ?1", params![id], transform_row)
            .optional()?)
    }

    pub fn transform_by_hotkey(&self, index: i64) -> Result<Option<Transform>> {
        Ok(self
            .conn
            .lock()
            .query_row(
                "SELECT id, name, prompt, builtin, hotkey_index FROM transforms WHERE hotkey_index = ?1 ORDER BY id LIMIT 1",
                params![index],
                transform_row,
            )
            .optional()?)
    }

    /// Crea (id 0 o inexistente) o actualiza. `builtin` no se puede cambiar desde fuera.
    pub fn save_transform(&self, t: &Transform) -> Result<Transform> {
        let conn = self.conn.lock();
        if t.id > 0 {
            let n = conn.execute(
                "UPDATE transforms SET name = ?2, prompt = ?3, hotkey_index = ?4 WHERE id = ?1",
                params![t.id, t.name.trim(), t.prompt, t.hotkey_index],
            )?;
            if n > 0 {
                return Ok(conn.query_row(
                    "SELECT id, name, prompt, builtin, hotkey_index FROM transforms WHERE id = ?1",
                    params![t.id],
                    transform_row,
                )?);
            }
        }
        conn.execute(
            "INSERT INTO transforms (name, prompt, builtin, hotkey_index, created_at) VALUES (?1, ?2, 0, ?3, ?4)",
            params![t.name.trim(), t.prompt, t.hotkey_index, now_iso()],
        )?;
        Ok(Transform { id: conn.last_insert_rowid(), name: t.name.trim().into(), prompt: t.prompt.clone(), builtin: false, hotkey_index: t.hotkey_index })
    }

    pub fn delete_transform(&self, id: i64) -> Result<()> {
        let n = self.conn.lock().execute("DELETE FROM transforms WHERE id = ?1 AND builtin = 0", params![id])?;
        if n == 0 {
            anyhow::bail!("Los transforms de fábrica no se pueden eliminar");
        }
        Ok(())
    }
}

fn wpm(words: i64, ms: i64) -> f64 {
    if ms > 0 {
        ((words as f64 / (ms as f64 / 60_000.0)) * 10.0).round() / 10.0
    } else {
        0.0
    }
}

fn key(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

fn day_words(per_day: &HashMap<String, i64>, d: NaiveDate) -> i64 {
    per_day.get(&key(d)).copied().unwrap_or(0)
}

/// Últimos `n` días hasta hoy, ascendentes y con ceros.
fn days_back(per_day: &HashMap<String, i64>, today: NaiveDate, n: i64) -> Vec<DayWords> {
    (0..n)
        .rev()
        .map(|i| {
            let d = today - Duration::days(i);
            DayWords { date: key(d), words: day_words(per_day, d) }
        })
        .collect()
}

/// Días seguidos con dictado hasta hoy. Si hoy aún no hay ninguno se cuenta desde ayer, para que la
/// racha no marque 0 cada mañana antes del primer dictado.
fn streak(per_day: &HashMap<String, i64>, today: NaiveDate) -> i64 {
    let mut day = if day_words(per_day, today) > 0 { today } else { today - Duration::days(1) };
    let mut n = 0;
    while day_words(per_day, day) > 0 {
        n += 1;
        day -= Duration::days(1);
    }
    n
}

fn longest_streak(per_day: &HashMap<String, i64>) -> i64 {
    let mut days: Vec<NaiveDate> = per_day
        .iter()
        .filter(|(_, w)| **w > 0)
        .filter_map(|(d, _)| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
        .collect();
    days.sort();
    let (mut best, mut run, mut prev) = (0, 0, None::<NaiveDate>);
    for d in days {
        run = if prev.is_some_and(|p| d - p == Duration::days(1)) { run + 1 } else { 1 };
        best = best.max(run);
        prev = Some(d);
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nuevo<'a>(text: &'a str, app: &'a str, ms: i64, words: i64) -> NewHistory<'a> {
        NewHistory { polished: false, text, raw: text, app, duration_ms: ms, words, fixes: Fixes::default(), transform: None }
    }

    /// WAV de antes de la 2.2.x (`{id}.wav`): se usa solo si se escribió cuando se creó ese dictado;
    /// uno de otra base con el mismo id (más viejo o más nuevo) no se lee ni se borra.
    #[test]
    fn wav_antiguo_solo_si_es_de_ese_dictado() {
        let id = 9_000_000 + std::process::id() as i64;
        let created_at = now_iso();
        let legacy = audio_dir().unwrap().join(format!("{id}.wav"));
        std::fs::write(&legacy, crate::audio::encode_wav(&[0.1; 160])).unwrap();
        assert_eq!(existing_audio_path(id, &created_at), Some(legacy.clone()), "el WAV antiguo del mismo dictado se encuentra");
        let f = std::fs::OpenOptions::new().write(true).open(&legacy).unwrap();
        f.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(2 * 86_400)).unwrap();
        drop(f);
        assert_eq!(existing_audio_path(id, &created_at), None, "un {id}.wav de hace dos días es de otro dictado");
        remove_audio_file((id, created_at.clone()));
        assert!(legacy.exists(), "borrar el dictado no puede borrar el WAV de otro");
        // Con el nombre nuevo: se escribe sin pisar y se encuentra.
        let p = audio_path(id, &created_at).unwrap();
        write_new(&p, b"uno").unwrap();
        write_new(&p, b"dos").unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"dos");
        assert_eq!(existing_audio_path(id, &created_at), Some(p.clone()));
        let _ = std::fs::remove_file(&p);
        let _ = std::fs::remove_file(&legacy);
    }

    #[test]
    fn historial_estadisticas_y_purga() {
        let db = Db::open_memory().unwrap();
        db.add_history(&nuevo("hola mundo", "Chrome", 3000, 2)).unwrap();
        db.add_history(&nuevo("adiós", "Notepad", 1000, 1)).unwrap();
        let s = db.stats().unwrap();
        assert_eq!(s.total_words, 3);
        assert_eq!(s.dictations, 2);
        assert_eq!(s.streak_days, 1);
        assert_eq!(s.words_this_week, 3);
        assert_eq!(s.by_day.len(), 30);
        assert!((s.wpm - 45.0).abs() < 0.2, "wpm = {}", s.wpm);
        assert_eq!(db.list_history(Some("hola"), 10, 0).unwrap().len(), 1);
        assert_eq!(db.last_history_text().as_deref(), Some("adiós"));
        // Nada tan viejo: la purga no borra.
        assert_eq!(db.purge_history(30).unwrap(), 0);
        db.clear_history().unwrap();
        assert!(db.list_history(None, 10, 0).unwrap().is_empty());
    }

    #[test]
    fn insights_apps_arreglos_y_transforms_aparte() {
        let db = Db::open_memory().unwrap();
        let mut a = nuevo("uno dos tres", "Chrome", 1000, 3);
        a.fixes = Fixes { dictionary: 2, snippets: 1, fillers: 4 };
        db.add_history(&a).unwrap();
        db.add_history(&nuevo("cuatro", "Code", 1000, 1)).unwrap();
        db.add_history(&nuevo("cinco seis", "Chrome", 1000, 2)).unwrap();
        // Un transform no cuenta como dictado ni suma palabras.
        let mut t = nuevo("resultado largo del transform", "Chrome", 0, 0);
        t.transform = Some("Polish");
        db.add_history(&t).unwrap();
        let i = db.insights().unwrap();
        assert_eq!((i.total_words, i.dictations), (6, 3));
        assert_eq!(i.heatmap.len(), 365);
        assert_eq!(i.top_apps[0].app, "Chrome");
        assert_eq!((i.top_apps[0].words, i.top_apps[0].count), (5, 2));
        assert_eq!((i.fixes.dictionary, i.fixes.snippets, i.fixes.fillers), (2, 1, 4));
        assert_eq!(i.longest_streak, 1);
        assert_eq!(db.list_history(None, 10, 0).unwrap()[0].transform.as_deref(), Some("Polish"));
    }

    #[test]
    fn racha_mas_larga() {
        let mut m = HashMap::new();
        for d in ["2025-01-01", "2025-01-02", "2025-01-03", "2025-01-10", "2025-01-11"] {
            m.insert(d.to_string(), 5);
        }
        m.insert("2025-01-04".into(), 0);
        assert_eq!(longest_streak(&m), 3);
    }

    #[test]
    fn copia_diaria_solo_con_datos_y_las_ultimas() {
        let dir = std::env::temp_dir().join(format!("pronto-copias-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for d in ["20200101", "20200102", "20200103"] {
            std::fs::write(dir.join(format!("pronto2-{d}.db")), b"x").unwrap();
        }
        let db = Db::open_memory().unwrap();
        assert!(db.daily_backup(&dir, 2).unwrap().is_none(), "una base vacía no se copia");
        db.save_settings(&Settings::default()).unwrap();
        assert!(db.daily_backup(&dir, 2).unwrap().is_none(), "los ajustes solos no son datos: no se copian");
        db.save_note(None, "una nota").unwrap();
        let copy = db.daily_backup(&dir, 2).unwrap().expect("copia del día");
        let c = Connection::open(&copy).unwrap();
        let n: i64 = c.query_row("SELECT COUNT(*) FROM notes", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
        drop(c);
        assert!(db.daily_backup(&dir, 2).unwrap().is_none(), "una sola copia al día");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
        assert!(!dir.join("pronto2-20200101.db").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn diccionario_snippets_notas() {
        let db = Db::open_memory().unwrap();
        let w = db.add_word("wispr", Some("Wispr"), &[]).unwrap();
        assert!(!w.starred && w.source == "manual");
        db.update_word(w.id, "wispr", Some(""), None).unwrap();
        assert_eq!(db.list_words().unwrap()[0].replacement, None);
        db.toggle_word_star(w.id).unwrap();
        assert!(db.list_words().unwrap()[0].starred);
        db.toggle_word_star(w.id).unwrap();
        assert!(!db.list_words().unwrap()[0].starred);
        let s = db.add_snippet("mi correo", "a@b.c").unwrap();
        db.update_snippet(s.id, "mi mail", "a@b.c").unwrap();
        assert_eq!(db.list_snippets().unwrap()[0].trigger, "mi mail");
        let n = db.save_note(None, "uno").unwrap();
        let n2 = db.save_note(Some(n.id), "dos").unwrap();
        assert_eq!(n2.id, n.id);
        assert_eq!(db.list_notes().unwrap()[0].text, "dos");
        db.delete_note(n.id).unwrap();
        assert!(db.list_notes().unwrap().is_empty());
    }

    #[test]
    fn transforms_de_fabrica_y_propios() {
        let db = Db::open_memory().unwrap();
        let list = db.list_transforms().unwrap();
        assert_eq!(list.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["Polish", "Prompt Engineer"]);
        assert!(list.iter().all(|t| t.builtin));
        assert_eq!(db.transform_by_hotkey(1).unwrap().unwrap().name, "Polish");
        // De fábrica: editable, no borrable.
        let mut p = list[0].clone();
        p.prompt = "otro".into();
        assert_eq!(db.save_transform(&p).unwrap().prompt, "otro");
        assert!(db.delete_transform(p.id).is_err());
        let mine = db.save_transform(&Transform { id: 0, name: "Resumen".into(), prompt: "resume".into(), builtin: true, hotkey_index: Some(3) }).unwrap();
        assert!(!mine.builtin);
        db.delete_transform(mine.id).unwrap();
        assert_eq!(db.list_transforms().unwrap().len(), 2);
    }

    #[test]
    fn ajustes_por_defecto_y_guardado() {
        let db = Db::open_memory().unwrap();
        let s = db.load_settings();
        assert!(s.hotkey_hold.win);
        assert_eq!(s.cleanup_level, CleanupLevel::Medium);
        let mut s = Settings::default();
        s.sounds = false;
        db.save_settings(&s).unwrap();
        assert!(!db.load_settings().sounds);
    }

    /// Un campo que esta versión no entiende (de una versión más nueva, o dañado) solo pierde su
    /// valor: el resto de ajustes se respeta. Y antes de guardar encima de ese JSON se copia entero
    /// a `settings_backup` (una vez), para no perder lo que esta versión no sabe leer.
    #[test]
    fn ajustes_campo_raro_no_resetea_el_resto_y_se_copia_antes_de_guardar() {
        let db = Db::open_memory().unwrap();
        let mut s = Settings::default();
        s.sounds = false;
        s.keep_audio_days = 3;
        s.first_name = "Txonso".into();
        let mut v = serde_json::to_value(&s).unwrap();
        v["cleanupLevel"] = serde_json::json!("extremo");
        v["keepAudioDays"] = serde_json::json!("muchos");
        let original = v.to_string();
        db.conn.lock().execute("INSERT INTO settings (id, json) VALUES (1, ?1)", params![original]).unwrap();
        let leidos = db.load_settings();
        assert!(!leidos.sounds && leidos.first_name == "Txonso", "un campo raro reseteó el resto");
        assert_eq!(leidos.cleanup_level, CleanupLevel::Medium, "el campo raro toma su valor de fábrica");
        assert_eq!(leidos.keep_audio_days, Settings::default().keep_audio_days);
        db.save_settings(&leidos).unwrap();
        db.save_settings(&leidos).unwrap();
        let copias: Vec<String> = {
            let conn = db.conn.lock();
            let mut st = conn.prepare("SELECT json FROM settings_backup").unwrap();
            st.query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect()
        };
        assert_eq!(copias, vec![original], "el JSON original se copia una vez antes de sobrescribirlo");
        // Un JSON que se entiende entero no deja copias.
        let db = Db::open_memory().unwrap();
        db.save_settings(&s).unwrap();
        db.save_settings(&s).unwrap();
        let hay: bool = db.conn.lock().query_row("SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE name = 'settings_backup')", [], |r| r.get(0)).unwrap();
        assert!(!hay);
    }

    #[test]
    fn ajustes_antiguos_migran_a_cleanup_level() {
        let db = Db::open_memory().unwrap();
        let old = r#"{"removeFillers":true,"backtrack":false,"sounds":false}"#;
        db.conn.lock().execute("INSERT INTO settings (id, json) VALUES (1, ?1)", params![old]).unwrap();
        let s = db.load_settings();
        assert_eq!(s.cleanup_level, CleanupLevel::Light);
        assert!(!s.sounds);
        assert_eq!(s.hotkey_copy_last.key.as_deref(), Some("X")); // campo nuevo: valor por defecto
    }

    #[test]
    fn atajo_de_transform_migra_solo_si_es_el_de_fabrica_antiguo() {
        let load = |json: &str| {
            let db = Db::open_memory().unwrap();
            db.conn.lock().execute("INSERT INTO settings (id, json) VALUES (1, ?1)", params![json]).unwrap();
            db.load_settings().hotkey_transform
        };
        let nuevo = default_transform_hotkey();
        assert_eq!((nuevo.alt, nuevo.shift, nuevo.win, nuevo.key.as_deref()), (true, true, false, Some("P")));
        // Exactamente el antiguo de fábrica -> el nuevo.
        assert_eq!(load(r#"{"hotkeyTransform":{"ctrl":false,"alt":true,"shift":false,"win":true,"key":"1"}}"#), nuevo);
        // Sin fila de ajustes o sin ese campo -> el nuevo por defecto.
        assert_eq!(Db::open_memory().unwrap().load_settings().hotkey_transform, nuevo);
        assert_eq!(load(r#"{"sounds":false}"#), nuevo);
        // Personalizados: se respetan (incluido uno parecido, con otra tecla o con Ctrl).
        let custom = load(r#"{"hotkeyTransform":{"ctrl":false,"alt":true,"shift":false,"win":true,"key":"2"}}"#);
        assert_eq!(custom.key.as_deref(), Some("2"));
        let ctrl = load(r#"{"hotkeyTransform":{"ctrl":true,"alt":true,"shift":false,"win":true,"key":"1"}}"#);
        assert!(ctrl.ctrl && ctrl.win);
        // Y el nuevo ya guardado no se toca.
        assert_eq!(load(r#"{"hotkeyTransform":{"ctrl":false,"alt":true,"shift":true,"win":false,"key":"P"}}"#), nuevo);
        assert_eq!(legacy_transform_hotkey().key.as_deref(), Some("1"));
    }

    #[test]
    fn el_modelo_de_ia_antiguo_de_fabrica_se_migra() {
        let load = |json: &str| {
            let db = Db::open_memory().unwrap();
            db.conn.lock().execute("INSERT INTO settings (id, json) VALUES (1, ?1)", params![json]).unwrap();
            let s = db.load_settings();
            (s.ollama_model, s.ai_polish_model)
        };
        let nuevo = (DEFAULT_AI_MODEL.to_string(), DEFAULT_AI_MODEL.to_string());
        assert_eq!(DEFAULT_AI_MODEL, "qwen3:1.7b");
        assert_eq!(Db::open_memory().unwrap().load_settings().ollama_model, DEFAULT_AI_MODEL);
        // Exactamente el antiguo de fábrica -> el nuevo (cada campo por separado).
        assert_eq!(load(r#"{"ollamaModel":"qwen2.5:7b","aiPolishModel":"qwen2.5:7b"}"#), nuevo);
        assert_eq!(load(r#"{"ollamaModel":"qwen2.5:7b","aiPolishModel":"llama3.2:3b"}"#), (DEFAULT_AI_MODEL.into(), "llama3.2:3b".into()));
        // Elegidos a mano: se respetan, también uno parecido.
        assert_eq!(load(r#"{"ollamaModel":"gemma2:2b","aiPolishModel":"qwen2.5:7b-instruct"}"#), ("gemma2:2b".into(), "qwen2.5:7b-instruct".into()));
        assert_eq!(load(r#"{"sounds":false}"#), nuevo);
    }

    #[test]
    fn pulir_con_ia_defaults_y_columna_polished() {
        let s = Settings::default();
        assert!(!s.ai_polish);
        assert_eq!((s.ai_polish_model.as_str(), s.ai_polish_timeout_ms), (s.ollama_model.as_str(), 6000));
        let db = Db::open_memory().unwrap();
        let mut n = nuevo("Hola, ¿qué tal?", "Chrome", 1000, 3);
        n.polished = true;
        n.raw = "hola que tal";
        let item = db.add_history(&n).unwrap();
        assert!(item.polished);
        let got = db.get_history(item.id).unwrap().unwrap();
        assert!(got.polished);
        assert_eq!((got.text.as_str(), got.raw_text.as_str()), ("Hola, ¿qué tal?", "hola que tal"));
        // Reintentar la transcripción deja de ser «pulido».
        db.update_history_text(item.id, "otra", "otra", 1).unwrap();
        assert!(!db.get_history(item.id).unwrap().unwrap().polished);
        // Una base v4 con datos pasa a la última versión conservándolos.
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA_V1).unwrap();
        conn.execute_batch(MIGRATION_V2).unwrap();
        conn.execute_batch(MIGRATION_V3).unwrap();
        conn.execute_batch(MIGRATION_V4).unwrap();
        conn.execute_batch("PRAGMA user_version = 4").unwrap();
        conn.execute("INSERT INTO history (created_at, text, raw_text, app, duration_ms, words) VALUES ('2025-01-01T10:00:00+01:00','hola','hola','Code',1000,1)", []).unwrap();
        migrate(&mut conn).unwrap();
        let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, 10);
        let db = Db::from_conn(conn);
        assert!(!db.list_history(None, 10, 0).unwrap()[0].polished);
    }

    #[test]
    fn meeting_detect_migra_a_meeting_auto() {
        let load = |json: &str| {
            let db = Db::open_memory().unwrap();
            db.conn.lock().execute("INSERT INTO settings (id, json) VALUES (1, ?1)", params![json]).unwrap();
            db.load_settings()
        };
        assert_eq!(load(r#"{"meetingDetect":true}"#).meeting_auto, MeetingAuto::Auto);
        let off = load(r#"{"meetingDetect":false}"#);
        assert_eq!(off.meeting_auto, MeetingAuto::Off);
        assert!(!off.meeting_detect);
        // Si ya existe meetingAuto manda sobre el booleano antiguo.
        assert_eq!(load(r#"{"meetingDetect":false,"meetingAuto":"ask"}"#).meeting_auto, MeetingAuto::Ask);
        assert_eq!(load(r#"{"sounds":false}"#).meeting_auto, MeetingAuto::Auto);
        assert_eq!(Db::open_memory().unwrap().load_settings().meeting_auto, MeetingAuto::Auto);
    }

    #[test]
    fn migracion_v1_a_v2_conserva_datos() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA_V1).unwrap();
        conn.execute(
            "INSERT INTO history (created_at, text, raw_text, app, duration_ms, words) VALUES ('2025-01-01T10:00:00+01:00','hola','hola','Chrome',1000,1)",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO words (word, replacement, created_at) VALUES ('x','y','2025-01-01')", []).unwrap();
        migrate(&mut conn).unwrap();
        let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, 10);
        let db = Db::from_conn(conn);
        let h = db.list_history(None, 10, 0).unwrap();
        assert_eq!((h[0].text.as_str(), h[0].has_audio), ("hola", false));
        assert_eq!(db.list_words().unwrap()[0].source, "manual");
        assert_eq!(db.list_transforms().unwrap().len(), 2);
        // Idempotente.
        migrate(&mut db.conn.lock()).unwrap();
        assert_eq!(db.list_transforms().unwrap().len(), 2);
    }

    #[test]
    fn migracion_v2_a_v3_conserva_datos() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA_V1).unwrap();
        conn.execute_batch(MIGRATION_V2).unwrap();
        conn.execute_batch("PRAGMA user_version = 2").unwrap();
        conn.execute(
            "INSERT INTO history (created_at, text, raw_text, app, duration_ms, words, transform) VALUES ('2025-01-01T10:00:00+01:00','adiós','adiós','Code',2000,1,NULL)",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO transforms (name, prompt, builtin, hotkey_index, created_at) VALUES ('Mío','p',0,NULL,'2025-01-01')", []).unwrap();
        migrate(&mut conn).unwrap();
        let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, 10);
        let db = Db::from_conn(conn);
        assert_eq!(db.list_history(None, 10, 0).unwrap()[0].text, "adiós");
        assert_eq!(db.list_transforms().unwrap()[0].name, "Mío");
        // Las tablas nuevas existen y funcionan, incluida la búsqueda de texto.
        let m = db.create_meeting("Prueba", None).unwrap();
        db.add_segment(&Segment { meeting_id: m.id, speaker: "me".into(), start_ms: 0, end_ms: 1500, text: "hablamos del presupuesto".into(), ..Default::default() }).unwrap();
        assert_eq!(db.list_meetings(Some("presupuesto"), 10, 0).unwrap().len(), 1);
        assert!(db.list_meetings(Some("jirafa"), 10, 0).unwrap().is_empty());
        assert!(!db.get_meeting(m.id).unwrap().unwrap().auto);
        // Idempotente.
        migrate(&mut db.conn.lock()).unwrap();
    }

    #[test]
    fn variantes_de_pronunciacion() {
        let db = Db::open_memory().unwrap();
        // Alta con variantes grabadas antes de guardar: se normalizan y no se repiten.
        let w = db.add_word("Txonso", None, &["Chonso.".into(), "chonso".into(), " ¿Chonzo? ".into()]).unwrap();
        assert_eq!(w.variants.iter().map(|v| v.heard.as_str()).collect::<Vec<_>>(), ["Chonso", "Chonzo"]);
        assert!(db.add_word("x", None, &["...".into()]).is_err(), "una variante vacía no vale");
        // Grabar otra desde la lista; la misma salvo mayúsculas devuelve la que ya estaba.
        db.add_word_variant(w.id, "Tchonso").unwrap();
        assert_eq!(db.add_word_variant(w.id, "CHONSO").unwrap().id, w.variants[0].id);
        assert!(db.add_word_variant(9999, "algo").is_err());
        let listed = db.list_words().unwrap();
        assert_eq!(listed[0].variants.len(), 3);
        // Editar con lista: quedan exactamente esas (la conservada mantiene su id).
        db.update_word(w.id, "Txonso", None, Some(&["chonso".into(), "Choncho".into()])).unwrap();
        let vs = db.list_words().unwrap()[0].variants.clone();
        assert_eq!(vs.iter().map(|v| v.heard.as_str()).collect::<Vec<_>>(), ["Chonso", "Choncho"]);
        assert_eq!(vs[0].id, w.variants[0].id);
        // Sin lista no se tocan.
        db.update_word(w.id, "Txonso", Some("Txonso"), None).unwrap();
        assert_eq!(db.list_words().unwrap()[0].variants.len(), 2);
        db.delete_word_variant(vs[1].id).unwrap();
        assert_eq!(db.list_words().unwrap()[0].variants.len(), 1);
        // Borrar la palabra borra sus variantes (la base no tiene `foreign_keys` activado).
        db.delete_word(w.id).unwrap();
        let n: i64 = db.conn.lock().query_row("SELECT COUNT(*) FROM word_variants", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn migracion_v5_a_v6_conserva_el_diccionario() {
        let mut conn = Connection::open_in_memory().unwrap();
        for m in [SCHEMA_V1, MIGRATION_V2, MIGRATION_V3, MIGRATION_V4, MIGRATION_V5] {
            conn.execute_batch(m).unwrap();
        }
        conn.execute_batch("PRAGMA user_version = 5").unwrap();
        conn.execute("INSERT INTO words (word, replacement, created_at) VALUES ('olama','Ollama','2025-01-01')", []).unwrap();
        conn.execute("INSERT INTO words (word, replacement, created_at) VALUES ('BeltranLabs',NULL,'2025-01-01')", []).unwrap();
        migrate(&mut conn).unwrap();
        let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, 10); // la 7 (reuniones), la 8 (huella de voz), la 9 (Meets) y la 10 (citas) van detrás
        let db = Db::from_conn(conn);
        let words = db.list_words().unwrap();
        assert_eq!(words.len(), 2);
        assert!(words.iter().all(|w| w.variants.is_empty()));
        assert_eq!(words.iter().find(|w| w.word == "olama").unwrap().replacement.as_deref(), Some("Ollama"));
        let id = words.iter().find(|w| w.word == "BeltranLabs").unwrap().id;
        db.add_word_variant(id, "Beltrán Labs").unwrap();
        migrate(&mut db.conn.lock()).unwrap(); // idempotente
        assert_eq!(db.list_words().unwrap().iter().find(|w| w.id == id).unwrap().variants[0].heard, "Beltrán Labs");
    }

    /// Deja una muestra pendiente en la carpeta de voz de las pruebas (nombre único por prueba).
    fn pending(tag: &str) -> String {
        let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let file = format!("{PENDING_PREFIX}{tag}{n}.wav");
        std::fs::write(voice_dir().unwrap().join(&file), crate::audio::encode_wav(&vec![0.1; 8000])).unwrap();
        file
    }

    #[test]
    fn muestras_de_voz() {
        let db = Db::open_memory().unwrap();
        let dir = voice_dir().unwrap();
        assert!(dir.starts_with(std::env::temp_dir()), "las pruebas no tocan %APPDATA%");
        // Alta con dos grabaciones pendientes: se renombran y quedan con su duración.
        let (p1, p2) = (pending("a"), pending("b"));
        let w = db.add_word_with_samples("Txonso", None, &["Chonso".into()], &[p1.clone(), p2]).unwrap();
        assert_eq!(w.samples.len(), 2);
        assert_eq!(w.samples[0].duration_ms, 500);
        assert!(!dir.join(&p1).exists() && dir.join(&w.samples[0].file).exists());
        assert!(w.samples[0].file.starts_with(&format!("w{}-", w.id)));
        let listed = db.list_words().unwrap();
        assert_eq!(listed[0].samples, w.samples);
        // Una pendiente que ya no está, o un nombre que no es de muestra: error y nada a medias.
        assert!(db.add_word_with_samples("Otra", None, &[], &["pendiente-no-existe.wav".into()]).is_err());
        assert!(db.add_word_sample(w.id, "..\\pronto2.db").is_err());
        assert!(db.add_word_sample(w.id, "w1-123.wav").is_err(), "solo se enganchan pendientes");
        assert_eq!(db.list_words().unwrap().len(), 1);
        // Grabar otra desde la lista.
        let s3 = db.add_word_sample(w.id, &pending("c")).unwrap();
        let orphan = pending("d");
        assert!(db.add_word_sample(9999, &orphan).is_err());
        let _ = std::fs::remove_file(dir.join(orphan));
        assert_eq!(db.list_words().unwrap()[0].samples.len(), 3);
        // Editar: quedan las que se piden y se añaden las nuevas; las quitadas se borran del disco.
        let gone = w.samples[0].file.clone();
        db.update_word_with_samples(w.id, "Txonso", None, None, Some(&[w.samples[1].id, s3.id]), &[pending("e")]).unwrap();
        let after = db.list_words().unwrap()[0].clone();
        assert_eq!(after.samples.len(), 3);
        assert!(!after.samples.iter().any(|s| s.id == w.samples[0].id));
        assert!(!dir.join(&gone).exists());
        assert_eq!(after.variants.len(), 1, "sin lista, las variantes no se tocan");
        // Sin `keep_samples` no se tocan.
        db.update_word_with_samples(w.id, "Txonso", None, None, None, &[]).unwrap();
        assert_eq!(db.list_words().unwrap()[0].samples.len(), 3);
        // Borrar una muestra y luego la palabra: filas y ficheros fuera.
        db.delete_word_sample(s3.id).unwrap();
        assert!(!dir.join(&s3.file).exists());
        let files: Vec<String> = db.list_words().unwrap()[0].samples.iter().map(|s| s.file.clone()).collect();
        assert_eq!(db.word_sample_file(after.samples[0].id).unwrap().as_deref(), Some(after.samples[0].file.as_str()));
        db.delete_word(w.id).unwrap();
        let n: i64 = db.conn.lock().query_row("SELECT COUNT(*) FROM word_samples", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
        assert!(files.iter().all(|f| !dir.join(f).exists()));
    }

    #[test]
    fn nombres_de_muestra_validos() {
        assert!(valid_sample_file("pendiente-123.wav") && valid_sample_file("w12-123.wav"));
        for bad in ["..\\x.wav", "../x.wav", "x.wav.exe", ".wav", "a b.wav", "c:x.wav", "x"] {
            assert!(!valid_sample_file(bad), "{bad}");
        }
    }

    #[test]
    fn migracion_v7_a_v8_conserva_el_diccionario() {
        let mut conn = Connection::open_in_memory().unwrap();
        for m in [SCHEMA_V1, MIGRATION_V2, MIGRATION_V3, MIGRATION_V4, MIGRATION_V5, MIGRATION_V6, MIGRATION_V7] {
            conn.execute_batch(m).unwrap();
        }
        conn.execute_batch("PRAGMA user_version = 7").unwrap();
        conn.execute("INSERT INTO words (word, replacement, created_at) VALUES ('Txonso',NULL,'2025-01-01')", []).unwrap();
        conn.execute("INSERT INTO word_variants (word_id, heard, created_at) VALUES (1,'Chonso','2025-01-01')", []).unwrap();
        migrate(&mut conn).unwrap();
        let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, 10);
        let db = Db::from_conn(conn);
        let w = &db.list_words().unwrap()[0];
        assert_eq!((w.variants.len(), w.samples.len()), (1, 0));
        migrate(&mut db.conn.lock()).unwrap(); // idempotente
    }

    #[test]
    fn migracion_v8_a_v9_conserva_las_reuniones() {
        let mut conn = Connection::open_in_memory().unwrap();
        for m in [SCHEMA_V1, MIGRATION_V2, MIGRATION_V3, MIGRATION_V4, MIGRATION_V5, MIGRATION_V6, MIGRATION_V7, MIGRATION_V8] {
            conn.execute_batch(m).unwrap();
        }
        conn.execute_batch("PRAGMA user_version = 8").unwrap();
        conn.execute("INSERT INTO meetings (title, started_at, summary, summary_status, transcript_final) VALUES ('Vieja','2026-10-01T10:00:00+02:00','Se habló.','done',1)", []).unwrap();
        conn.execute("INSERT INTO meeting_segments (meeting_id, speaker, start_ms, end_ms, text) VALUES (1,'them',0,1500,'Hola, ¿qué tal?')", []).unwrap();
        migrate(&mut conn).unwrap();
        let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, 10);
        let db = Db::from_conn(conn);
        let d = db.get_meeting_detail(1).unwrap().unwrap();
        assert_eq!((d.summary.as_deref(), d.segments.len()), (Some("Se habló."), 1));
        let s = &d.segments[0];
        assert_eq!((s.raw_text.as_ref(), s.kind.as_ref(), s.overlap, s.reply_to), (None, None, false, None));
        assert!(d.questions.is_empty() && d.said_by_me.is_empty() && d.said_by_them.is_empty());
        assert!(!db.polish_done(1).unwrap());
        migrate(&mut db.conn.lock()).unwrap(); // idempotente
    }
}

/// Compatibilidad con una base real de antes (sin `base_id`) y su `-wal` sin volcar, sobre una COPIA:
/// `PRONTO_BASE_REAL=<carpeta con pronto2.db, -wal y -shm copiados de %APPDATA%\Pronto2>`. La copia
/// se modifica (se le escribe el `base_id` y se vuelca): nunca apuntarla a `%APPDATA%`.
#[cfg(test)]
mod compat_base_real {
    use super::*;

    #[test]
    fn compat_base_real_de_antes_con_wal() {
        let Ok(dir) = std::env::var("PRONTO_BASE_REAL") else {
            println!("OMITIDO: falta PRONTO_BASE_REAL");
            return;
        };
        let path = std::path::Path::new(&dir).join("pronto2.db");
        assert!(!path.starts_with(dirs::data_dir().unwrap()), "solo sobre una copia");
        let main_before = open_immutable(&path).and_then(|c| look(&c)).unwrap();
        let view_before = {
            let c = Connection::open(&path).unwrap();
            c.set_db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true).unwrap();
            look(&c).unwrap()
        };
        let db = Db::open_at(&path).unwrap();
        let (_, rows) = census(&db.conn.lock()).unwrap();
        let id = read_meta(&db.conn.lock(), "base_id").unwrap();
        let main_after = open_immutable(&path).and_then(|c| look(&c)).unwrap();
        let ok: String = db.conn.lock().query_row("PRAGMA quick_check", [], |r| r.get(0)).unwrap();
        eprintln!(
            "COMPAT base real: principal antes id={:?} filas={} · vista con -wal filas={} · abierta: aviso={:?} filas={rows} base_id={id:?} · principal después id={:?} filas={} · quick_check={ok}",
            main_before.id,
            main_before.rows,
            view_before.rows,
            db.aviso(),
            main_after.id,
            main_after.rows
        );
        assert!(db.aviso().is_none(), "apartó el -wal propio de la base real");
        assert_eq!(rows, view_before.rows, "faltan filas de las que había en el -wal");
        assert!(id.is_some() && main_after.id == id, "el base_id no quedó en el fichero principal");
        assert_eq!(main_after.rows, rows, "el -wal no quedó volcado");
        assert_eq!(ok, "ok");
    }
}

/// Regla 2 de CLAUDE.md (integridad). Las lanza `tests/no-se-negocia/02-integridad.test.mjs` con
/// `cargo test nsn_integridad`. Siempre sobre bases temporales: nunca `%APPDATA%\Pronto2`.
#[cfg(test)]
mod nsn_integridad {
    use super::*;
    use std::path::Path;

    fn tmp(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("pronto2-nsn-{name}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Todas las filas de las tablas reales (sin las internas ni las de búsqueda), como texto.
    fn dump(conn: &Connection) -> Vec<String> {
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '%fts%' ORDER BY name")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        let mut out = Vec::new();
        for t in tables {
            out.extend(rows(conn, &format!("SELECT * FROM \"{t}\" ORDER BY rowid")).into_iter().map(|r| format!("{t}|{r}")));
        }
        out
    }

    fn rows(conn: &Connection, sql: &str) -> Vec<String> {
        let mut stmt = conn.prepare(sql).unwrap();
        let n = stmt.column_count();
        let celda = |v: rusqlite::types::ValueRef| match v {
            rusqlite::types::ValueRef::Null => "NULL".to_string(),
            rusqlite::types::ValueRef::Integer(i) => i.to_string(),
            rusqlite::types::ValueRef::Real(f) => f.to_string(),
            rusqlite::types::ValueRef::Text(t) => format!("{:?}", String::from_utf8_lossy(t)),
            rusqlite::types::ValueRef::Blob(b) => format!("blob:{}", b.len()),
        };
        stmt.query_map([], |r| Ok((0..n).map(|i| celda(r.get_ref(i).unwrap())).collect::<Vec<_>>().join("|")))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    }

    /// Las columnas del esquema v1, que toda migración debe conservar tal cual.
    fn v1_rows(conn: &Connection) -> Vec<String> {
        [
            "SELECT id, created_at, text, raw_text, app, duration_ms, words FROM history ORDER BY id",
            "SELECT id, word, replacement, created_at FROM words ORDER BY id",
            "SELECT id, trigger, expansion, created_at FROM snippets ORDER BY id",
            "SELECT id, text, created_at, updated_at FROM notes ORDER BY id",
            "SELECT id, json FROM settings ORDER BY id",
        ]
        .iter()
        .flat_map(|q| rows(conn, q))
        .collect()
    }

    /// Mismas filas; si no, un mensaje corto con el recuento y la primera diferencia.
    fn assert_same(got: &[String], want: &[String], msg: &str) {
        if got == want {
            return;
        }
        let i = got.iter().zip(want).position(|(a, b)| a != b).unwrap_or(got.len().min(want.len()));
        let corta = |s: Option<&String>| s.map(|s| s.chars().take(160).collect::<String>());
        panic!("{msg}: {} filas frente a {} esperadas; primera diferencia en la fila {i}: {:?} ≠ {:?}", got.len(), want.len(), corta(got.get(i)), corta(want.get(i)));
    }

    fn version(conn: &Connection) -> i64 {
        conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn nsn_integridad_migracion_conserva_cada_fila() {
        let dir = tmp("migra");
        let path = dir.join("pronto2.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(SCHEMA_V1).unwrap();
            conn.execute_batch("PRAGMA user_version = 1").unwrap();
            for i in 0..50 {
                conn.execute(
                    "INSERT INTO history (created_at, text, raw_text, app, duration_ms, words) VALUES (?1, ?2, ?3, 'Code', ?4, 3)",
                    params![format!("2025-0{}-1{}T10:00:00+01:00", 1 + i % 9, i % 10), format!("dictado número {i}: «ñandú» ✓"), format!("crudo {i}"), 1000 + i],
                )
                .unwrap();
            }
            for i in 0..10 {
                let rep = if i % 2 == 0 { Some(format!("Reemplazo{i}")) } else { None };
                conn.execute("INSERT INTO words (word, replacement, created_at) VALUES (?1, ?2, '2025-01-01')", params![format!("palabra{i}"), rep]).unwrap();
                conn.execute("INSERT INTO snippets (trigger, expansion, created_at) VALUES (?1, ?2, '2025-01-01')", params![format!("atajo {i}"), format!("expansión {i}")]).unwrap();
                conn.execute("INSERT INTO notes (text, created_at, updated_at) VALUES (?1, '2025-01-01', '2025-01-02')", params![format!("nota {i}")]).unwrap();
            }
            conn.execute("INSERT INTO settings (id, json) VALUES (1, '{\"sounds\":false,\"removeFillers\":false}')", []).unwrap();
        }
        let before = v1_rows(&Connection::open(&path).unwrap());
        assert_eq!(before.len(), 50 + 10 + 10 + 10 + 1);

        // Arranque con la versión actual: migra la base antigua.
        let db = Db::open_at(&path).unwrap();
        assert!(version(&db.conn.lock()) >= 9, "la base no quedó migrada");
        let after = v1_rows(&db.conn.lock());
        assert_same(&after, &before, "la migración perdió o cambió filas");
        assert_eq!(db.list_history(None, 100, 0).unwrap().len(), 50);
        let full = dump(&db.conn.lock());
        drop(db);

        // Segundo arranque: nada cambia (migración idempotente, sin filas duplicadas ni perdidas).
        let db = Db::open_at(&path).unwrap();
        assert_same(&dump(&db.conn.lock()), &full, "reabrir la base cambió sus filas");
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Migración v10 (citas de las notas, decisión A de Txonso, 23-oct): una base v9 con reuniones,
    /// tareas hechas y decisiones sale con las mismas filas, sus notas se ven como antes (sin cita) y
    /// las notas nuevas guardan y devuelven su cita. Reabrir no cambia nada.
    #[test]
    fn nsn_integridad_migracion_v10_conserva_tareas_y_decisiones() {
        let dir = tmp("v10");
        let path = dir.join("pronto2.db");
        {
            let conn = Connection::open(&path).unwrap();
            for m in [SCHEMA_V1, MIGRATION_V2, MIGRATION_V3, MIGRATION_V4, MIGRATION_V5, MIGRATION_V6, MIGRATION_V7, MIGRATION_V8, MIGRATION_V9] {
                conn.execute_batch(m).unwrap();
            }
            conn.execute_batch("PRAGMA user_version = 9").unwrap();
            for m in 1..=3 {
                conn.execute(
                    "INSERT INTO meetings (title, started_at, summary, summary_status, transcript_final, decisions) VALUES (?1, '2026-10-01T10:00:00+02:00', 'Se habló.', 'done', 1, ?2)",
                    params![format!("Reunión {m}"), format!(r#"["Decisión {m}a","Decisión {m}b"]"#)],
                )
                .unwrap();
                for i in 0..4 {
                    conn.execute(
                        "INSERT INTO meeting_actions (meeting_id, idx, text, owner, done, due) VALUES (?1, ?2, ?3, 'Ana', ?4, 'el lunes')",
                        params![m, i, format!("Tarea {m}.{i} «ñ»"), i % 2],
                    )
                    .unwrap();
                }
                conn.execute("INSERT INTO meeting_segments (meeting_id, speaker, start_ms, end_ms, text) VALUES (?1,'them',0,1500,'Ana revisa el contrato el lunes.')", params![m]).unwrap();
            }
        }
        let rows = |conn: &Connection| -> Vec<String> {
            let mut out = Vec::new();
            for q in [
                "SELECT meeting_id || '|' || idx || '|' || text || '|' || owner || '|' || done || '|' || due FROM meeting_actions ORDER BY meeting_id, idx",
                "SELECT id || '|' || title || '|' || summary || '|' || decisions FROM meetings ORDER BY id",
                "SELECT meeting_id || '|' || text FROM meeting_segments ORDER BY id",
            ] {
                let mut st = conn.prepare(q).unwrap();
                out.extend(st.query_map([], |r| r.get::<_, String>(0)).unwrap().map(Result::unwrap));
            }
            out
        };
        let before = rows(&Connection::open(&path).unwrap());
        assert_eq!(before.len(), 12 + 3 + 3);
        let db = Db::open_at(&path).unwrap();
        assert_eq!(version(&db.conn.lock()), 10);
        assert_same(&rows(&db.conn.lock()), &before, "la migración v10 perdió o cambió filas");
        // Las notas viejas se ven como antes: sin cita.
        let d = db.get_meeting_detail(2).unwrap().unwrap();
        assert_eq!(d.action_items.len(), 4);
        assert!(d.action_items.iter().all(|a| a.cita.is_none()) && d.decision_citas.is_empty());
        assert_eq!((d.action_items[1].done, d.decisions.len()), (true, 2));
        // Las nuevas guardan su cita y la devuelven.
        let cita = crate::summary::Cita { text: "Ana revisa el contrato el lunes.".into(), speaker: "them".into(), at_ms: 0, turn: 0 };
        let s = crate::summary::ParsedSummary {
            summary: "Contrato.".into(),
            action_items: vec![crate::summary::ParsedAction { text: "Revisar el contrato".into(), owner: Some("Ana".into()), due: Some("el lunes".into()) }],
            action_citas: vec![Some(cita.clone())],
            decisions: vec!["Ana revisa el contrato".into()],
            decision_citas: vec![Some(cita.clone())],
            ..Default::default()
        };
        db.save_summary(3, &s).unwrap();
        let d = db.get_meeting_detail(3).unwrap().unwrap();
        assert_eq!((d.action_items[0].cita.as_ref(), d.decision_citas.first().cloned().flatten().as_ref()), (Some(&cita), Some(&cita)));
        let full = dump(&db.conn.lock());
        drop(db);
        // Reabrir: idempotente.
        let db = Db::open_at(&path).unwrap();
        assert_same(&dump(&db.conn.lock()), &full, "reabrir la base v10 cambió sus filas");
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn fill(db: &Db) {
        for i in 0..20 {
            db.add_history(&NewHistory {
                polished: i % 3 == 0,
                text: &format!("texto {i} con acentos: camión, pingüino"),
                raw: &format!("crudo {i}"),
                app: "Chrome",
                duration_ms: 1500,
                words: 5,
                fixes: Fixes { dictionary: 1, snippets: 0, fillers: 2 },
                transform: None,
            })
            .unwrap();
        }
        db.add_word("Txonso", None, &["chonso".to_string()]).unwrap();
        db.add_snippet("mi correo", "alguien@example.com").unwrap();
        db.save_note(None, "nota importante").unwrap();
        let m = db.create_meeting("Reunión de prueba", Some("Teams")).unwrap();
        db.add_segment(&Segment { meeting_id: m.id, speaker: "me".into(), start_ms: 0, end_ms: 900, text: "cerramos el presupuesto".into(), ..Default::default() }).unwrap();
        let mut s = db.load_settings();
        s.sounds = false;
        db.save_settings(&s).unwrap();
    }

    #[test]
    fn nsn_integridad_copia_diaria_restaurable() {
        let dir = tmp("copia");
        let db = Db::open_at(&dir.join("pronto2.db")).unwrap();
        fill(&db);
        let live = dump(&db.conn.lock());
        assert!(live.len() > 20);

        let copies = dir.join("copias");
        std::fs::create_dir_all(&copies).unwrap();
        let copy = db.daily_backup(&copies, 7).unwrap().expect("la copia del día debe crearse");
        assert_same(&dump(&db.conn.lock()), &live, "hacer la copia cambió la base viva");
        // La copia se abre con la app (migraciones incluidas) y tiene exactamente las mismas filas,
        // salvo su identidad (`meta`): `base_id` propio y `copia_de` = el de la base (rompedor
        // 22-oct-c, datos MEDIA 2).
        let restored = Db::open_at(&copy).unwrap();
        let sin_meta = |v: Vec<String>| v.into_iter().filter(|r| !r.starts_with("meta|")).collect::<Vec<_>>();
        assert_same(&sin_meta(dump(&restored.conn.lock())), &sin_meta(live.clone()), "la copia no tiene las mismas filas");
        let id = read_meta(&db.conn.lock(), "base_id").unwrap();
        assert!(id.is_some());
        assert_ne!(read_meta(&restored.conn.lock(), "base_id").unwrap(), id, "la copia lleva su propio base_id");
        assert_eq!(read_meta(&restored.conn.lock(), "copia_de").unwrap(), id, "la copia dice de qué base es");
        assert!(!copies.join(copy.file_name().unwrap()).with_extension("db-parcial").exists());
        assert_eq!(restored.list_history(None, 100, 0).unwrap().len(), 20);
        assert_eq!(restored.list_meetings(Some("presupuesto"), 10, 0).unwrap().len(), 1, "la búsqueda de reuniones funciona en la copia");
        drop((db, restored));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn hash(p: &Path) -> Vec<u8> {
        use sha2::{Digest, Sha256};
        Sha256::digest(std::fs::read(p).unwrap()).to_vec()
    }

    #[test]
    fn nsn_integridad_abrir_nunca_vacia_una_base_con_datos() {
        let dir = tmp("abrir");
        let path = dir.join("pronto2.db");
        let db = Db::open_at(&path).unwrap();
        fill(&db);
        let before = dump(&db.conn.lock());
        drop(db);
        // Cada arranque abre la base existente: nunca la sustituye por una vacía.
        for _ in 0..3 {
            let db = Db::open_at(&path).unwrap();
            assert_same(&dump(&db.conn.lock()), &before, "abrir la base perdió datos");
        }
        // Un fichero que no es una base SQLite válida no se pisa con una base nueva y vacía.
        let bad = dir.join("dañada.db");
        std::fs::write(&bad, b"esto no es una base de datos SQLite, pero son datos del usuario. ".repeat(100)).unwrap();
        let h = hash(&bad);
        assert!(Db::open_at(&bad).is_err(), "abrir un fichero dañado debe fallar, no crear una base encima");
        assert_eq!(hash(&bad), h, "el fichero dañado se modificó");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn sidecar(p: &Path, suffix: &str) -> PathBuf {
        PathBuf::from(format!("{}{suffix}", p.display()))
    }

    /// Rompedor 22-oct, MEDIA 4: tras un incidente, una base con solo ajustes (o con unos pocos
    /// dictados) no puede desplazar, día a día, a las copias buenas. Los ajustes no son datos; una
    /// base con mucho menos que la última copia buena se copia aparte (`-sospechosa`) y no rota nada.
    #[test]
    fn nsn_integridad_copia_diaria_no_rota_las_buenas_con_una_base_menguada() {
        let dir = tmp("copias-menguada");
        let copias = dir.join("copias");
        std::fs::create_dir_all(&copias).unwrap();
        let buena = Db::open_at(&dir.join("buena.db")).unwrap();
        fill(&buena);
        let buenas: Vec<PathBuf> = (1..=7).map(|d| copias.join(format!("pronto2-2026090{d}.db"))).collect();
        for p in &buenas {
            buena.conn.lock().execute("VACUUM INTO ?1", params![p.to_string_lossy()]).unwrap();
        }
        let hoy = chrono::Local::now().format("%Y%m%d").to_string();
        // Solo ajustes: no es una base con datos, no se copia.
        let vacia = Db::open_at(&dir.join("vacia.db")).unwrap();
        vacia.save_settings(&Settings::default()).unwrap();
        for _ in 0..3 {
            assert_eq!(vacia.daily_backup(&copias, 7).unwrap(), None, "una base con solo ajustes no se copia");
        }
        // Un dictado (base nueva tras el incidente): copia aparte, sin rotar las buenas, 9 «días».
        vacia
            .add_history(&NewHistory { polished: false, text: "uno", raw: "uno", app: "", duration_ms: 1, words: 1, fixes: Fixes::default(), transform: None })
            .unwrap();
        for d in 0..9 {
            let p = vacia.daily_backup(&copias, 7).unwrap().expect("la base menguada también se guarda, aparte");
            assert!(p.to_string_lossy().contains("sospechosa"), "{}", p.display());
            std::fs::rename(&p, copias.join(format!("pronto2-{hoy}-{d}-sospechosa.db"))).unwrap();
        }
        assert!(buenas.iter().all(|p| p.exists()), "una base menguada rotó y borró copias buenas");
        let sospechosas = std::fs::read_dir(&copias).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().contains("sospechosa")).count();
        assert!(sospechosas <= 7, "las copias sospechosas también tienen tope: {sospechosas}");
        // La base buena sí rota: quedan 7 copias normales, la de hoy incluida.
        let hecha = buena.daily_backup(&copias, 7).unwrap().expect("copia del día de la base buena");
        let normales: Vec<String> = std::fs::read_dir(&copias)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| !n.contains("sospechosa"))
            .collect();
        assert_eq!(normales.len(), 7, "{normales:?}");
        assert!(hecha.exists() && !buenas[0].exists());
        drop((buena, vacia));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Incidente del 13-oct (`specs/rompedor/2026-10-22.md`, ALTA 1): junto a la base buena queda el
    /// `-wal` de una base recién creada. Abrir no puede enseñar la vacía ni, al cerrar, volcarla
    /// encima de la buena: el `-wal` ajeno se aparta (nunca se borra) y queda un aviso. Un `-wal`
    /// propio con dictados sin volcar (apagón) se respeta.
    #[test]
    fn nsn_integridad_wal_ajeno_no_tapa_la_base_buena() {
        let dir = tmp("wal-ajeno");
        std::fs::create_dir_all(dir.join("vacia")).unwrap();
        let good = dir.join("pronto2.db");
        {
            let db = Db::open_at(&good).unwrap();
            fill(&db);
        }
        let before = dump(&Connection::open(&good).unwrap());
        assert!(!sidecar(&good, "-wal").exists());

        // Otro arranque crea una base vacía y su -wal (y -shm) acaban junto a la buena.
        let empty = dir.join("vacia").join("pronto2.db");
        let db_vacia = Db::open_at(&empty).unwrap();
        std::fs::copy(sidecar(&empty, "-wal"), sidecar(&good, "-wal")).unwrap();
        std::fs::copy(sidecar(&empty, "-shm"), sidecar(&good, "-shm")).unwrap();
        drop(db_vacia);
        let wal_ajeno = hash(&sidecar(&good, "-wal"));

        let db = Db::open_at(&good).unwrap();
        assert_same(&dump(&db.conn.lock()), &before, "el -wal ajeno tapó la base buena al abrir");
        assert!(db.aviso().is_some_and(|a| a.contains("apartado-")), "sin aviso de lo apartado: {:?}", db.aviso());
        drop(db); // cierre limpio: el checkpoint no puede volcar la base vacía encima
        assert_same(&dump(&Connection::open(&good).unwrap()), &before, "tras cerrar, la base buena perdió datos");
        // Lo apartado sigue ahí, intacto, en una carpeta `apartado-<fecha-hora>`.
        let apartados: Vec<PathBuf> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("apartado-")))
            .collect();
        assert_eq!(apartados.len(), 1, "una carpeta apartado-<fecha-hora>");
        assert_eq!(hash(&apartados[0].join("pronto2.db-wal")), wal_ajeno, "el -wal apartado no es el original");
        assert!(apartados[0].join("pronto2.db-shm").exists());

        // Un -wal propio (dictados sin volcar por un apagón) no se aparta.
        let db = Db::open_at(&good).unwrap();
        assert!(db.aviso().is_none());
        db.add_history(&NewHistory { polished: false, text: "sin volcar", raw: "sin volcar", app: "", duration_ms: 1, words: 2, fixes: Fixes::default(), transform: None }).unwrap();
        let con_el_nuevo = dump(&db.conn.lock());
        db.conn.lock().set_db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true).unwrap();
        drop(db);
        assert!(sidecar(&good, "-wal").exists(), "el apagón deja el -wal sin volcar");
        let db = Db::open_at(&good).unwrap();
        assert!(db.aviso().is_none(), "apartó un -wal propio: {:?}", db.aviso());
        assert_same(&dump(&db.conn.lock()), &con_el_nuevo, "se perdió el dictado sin volcar");
        drop(db);

        // La guarda también actúa en una carpeta con espacios, `#`, `%`, `?`-like y no ASCII
        // (nombres de usuario de Windows): la lectura sin -wal va por URI `file:`.
        let rara = dir.join("Usuario con espacio #1 100% ñandú");
        std::fs::create_dir_all(&rara).unwrap();
        let good2 = rara.join("pronto2.db");
        {
            let db = Db::open_at(&good2).unwrap();
            fill(&db);
        }
        let before2 = dump(&Connection::open(&good2).unwrap());
        let db_vacia = Db::open_at(&dir.join("vacia").join("otra.db")).unwrap();
        std::fs::copy(sidecar(&dir.join("vacia").join("otra.db"), "-wal"), sidecar(&good2, "-wal")).unwrap();
        drop(db_vacia);
        let db = Db::open_at(&good2).unwrap();
        assert!(db.aviso().is_some(), "la guarda no actuó en una ruta con símbolos");
        drop(db);
        assert_same(&dump(&Connection::open(&good2).unwrap()), &before2, "ruta con símbolos: la base buena perdió datos");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn apartados(dir: &Path) -> Vec<PathBuf> {
        std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("apartado-")))
            .collect()
    }

    /// Base de una versión anterior (sin `meta`): esquema migrado con una conexión cruda, 20
    /// dictados volcados en el fichero principal y `wal_rows` más que se quedan en su `-wal`.
    fn legacy(path: &Path, wal_rows: usize) {
        let mut c = Connection::open(path).unwrap();
        c.execute_batch("PRAGMA journal_mode = WAL;").unwrap();
        migrate(&mut c).unwrap();
        let add = |c: &Connection, i: usize| {
            c.execute("INSERT INTO history (created_at, text, raw_text, app, duration_ms, words) VALUES ('2026-10-01T10:00:00+02:00', ?1, ?1, 'Code', 1000, 2)", params![format!("antiguo {i}")])
                .unwrap()
        };
        (0..20).for_each(|i| {
            add(&c, i);
        });
        c.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
        (0..wal_rows).for_each(|i| {
            add(&c, 100 + i);
        });
        if wal_rows > 0 {
            c.set_db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true).unwrap();
        }
    }

    /// `base_id` del fichero principal leído solo (sin su -wal).
    fn main_id(p: &Path) -> Option<String> {
        read_meta(&open_immutable(p).unwrap(), "base_id").unwrap()
    }

    /// Rompedor 22-oct-b, ALTA 1: el `-wal` ajeno ya tenía dictados (el 13-oct, 4) y viene de una base
    /// con otra disposición de páginas (ya volcada: su `-wal` no lleva la página 1). Se aparta por
    /// identidad, tenga las filas que tenga, junto a una base nueva y junto a una de antes ya abierta
    /// con esta versión.
    #[test]
    fn nsn_integridad_wal_ajeno_con_datos_se_aparta() {
        let dir = tmp("wal-ajeno-datos");
        for caso in ["base_nueva", "base_de_antes"] {
            let d = dir.join(caso);
            std::fs::create_dir_all(d.join("intrusa")).unwrap();
            let good = d.join("pronto2.db");
            if caso == "base_nueva" {
                fill(&Db::open_at(&good).unwrap());
            } else {
                legacy(&good, 0);
                drop(Db::open_at(&good).unwrap());
            }
            assert!(main_id(&good).is_some(), "{caso}: la base buena no tiene base_id en el fichero principal");
            let before = dump(&Connection::open(&good).unwrap());

            let other = d.join("intrusa").join("pronto2.db");
            let intrusa = Db::open_at(&other).unwrap();
            intrusa.conn.lock().execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
            for i in 1..=4 {
                intrusa
                    .add_history(&NewHistory { polished: false, text: &format!("intrusa {i}"), raw: "", app: "", duration_ms: 1, words: 1, fixes: Fixes::default(), transform: None })
                    .unwrap();
            }
            intrusa.conn.lock().set_db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true).unwrap();
            std::fs::copy(sidecar(&other, "-wal"), sidecar(&good, "-wal")).unwrap();
            std::fs::copy(sidecar(&other, "-shm"), sidecar(&good, "-shm")).unwrap();
            drop(intrusa);
            let wal_ajeno = hash(&sidecar(&good, "-wal"));

            let db = Db::open_at(&good).unwrap();
            assert_same(&dump(&db.conn.lock()), &before, &format!("{caso}: el -wal ajeno con 4 dictados tapó la base buena"));
            assert!(db.aviso().is_some_and(|a| a.contains("apartado-")), "{caso}: sin aviso: {:?}", db.aviso());
            drop(db);
            assert_same(&dump(&Connection::open(&good).unwrap()), &before, &format!("{caso}: tras cerrar, la base buena perdió datos"));
            let a = apartados(&d);
            assert_eq!(a.len(), 1, "{caso}");
            assert_eq!(hash(&a[0].join("pronto2.db-wal")), wal_ajeno, "{caso}: el -wal apartado no es el original");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Rompedor 22-oct-c, datos ALTA 1: el `-wal` ajeno lo escribió Pronto ≤ 2.0.2 (sin `meta`: sin
    /// identidad) en una base con la misma disposición de páginas, con 4 dictados (el 13-oct). La
    /// vista con él se ve coherente y `quick_check` pasa, pero no es la base buena: se aparta, junto a
    /// una base ya identificada y junto a una de antes en su primer arranque con esta versión. Y uno
    /// sin identidad con otro tamaño de página se aparta sin apartar la base buena (antes Pronto la
    /// daba por dañada, la apartaba entera y arrancaba vacío).
    #[test]
    fn nsn_integridad_wal_ajeno_sin_identidad_se_aparta() {
        let dir = tmp("wal-ajeno-sin-id");
        let raw = |p: &Path, n: usize, tag: &str, kill: bool| {
            let c = Connection::open(p).unwrap();
            for i in 0..n {
                c.execute("INSERT INTO history (created_at, text, raw_text, app) VALUES ('2026-10-13T21:49:00+02:00', ?1, '', 'Code')", params![format!("{tag} {i}")]).unwrap();
            }
            if kill {
                c.set_db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true).unwrap();
            }
        };
        // Base de 2.0.2 vacía (esquema v9 sin `meta`) de la que salen la buena y la intrusa.
        let plantilla = |p: &Path, page: u32| {
            let mut c = Connection::open(p).unwrap();
            c.execute_batch(&format!("PRAGMA page_size = {page}; PRAGMA journal_mode = WAL;")).unwrap();
            migrate(&mut c).unwrap();
            c.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
        };
        let molde = dir.join("plantilla.db");
        plantilla(&molde, 4096);
        let intrusa = dir.join("intrusa.db");
        std::fs::copy(&molde, &intrusa).unwrap();
        raw(&intrusa, 4, "intrusa", true);
        let molde_1024 = dir.join("plantilla-1024.db");
        plantilla(&molde_1024, 1024);
        raw(&molde_1024, 4, "intrusa 1024", true);
        // Rompedor 23-oct, datos MEDIA 2: una intrusa con MÁS dictados que la buena (30 > 20): ningún
        // contador baja, pero reescribe los dictados que ya estaban.
        let intrusa_mas = dir.join("intrusa-mas.db");
        std::fs::copy(&molde, &intrusa_mas).unwrap();
        raw(&intrusa_mas, 30, "intrusa", true);
        for (caso, wal_de) in [
            ("ya_identificada", &intrusa),
            ("de_antes_primer_arranque", &intrusa),
            ("otra_pagina", &molde_1024),
            ("ya_identificada_intrusa_con_mas_filas", &intrusa_mas),
            ("de_antes_primer_arranque_intrusa_con_mas_filas", &intrusa_mas),
        ] {
            let d = dir.join(caso);
            std::fs::create_dir_all(&d).unwrap();
            let good = d.join("pronto2.db");
            std::fs::copy(&molde, &good).unwrap();
            raw(&good, 20, "buena", false); // cierre limpio: todo volcado
            if !caso.starts_with("de_antes_primer_arranque") {
                drop(Db::open_at(&good).unwrap());
                assert!(main_id(&good).is_some(), "{caso}: precondición: base identificada");
            }
            let before = dump(&Connection::open(&good).unwrap());
            std::fs::copy(sidecar(wal_de, "-wal"), sidecar(&good, "-wal")).unwrap();
            let wal_ajeno = hash(&sidecar(&good, "-wal"));
            assert!(!std::fs::read(sidecar(&good, "-wal")).unwrap().windows(7).any(|w| w == b"base_id"), "{caso}: precondición: -wal sin identidad");

            let db = Db::open_at(&good).unwrap();
            let visto = dump(&db.conn.lock());
            let ids = |v: &[String]| v.iter().filter(|r| !r.starts_with("meta|")).cloned().collect::<Vec<_>>();
            assert_same(&ids(&visto), &ids(&before), &format!("{caso}: el -wal ajeno sin identidad tapó la base buena"));
            assert!(db.aviso().is_some_and(|a| a.contains("apartado-") && !a.contains("Pronto ha empezado con una base nueva")), "{caso}: aviso: {:?}", db.aviso());
            drop(db); // cierre limpio: no puede volcar la intrusa encima
            assert_same(&ids(&dump(&Connection::open(&good).unwrap())), &ids(&before), &format!("{caso}: tras cerrar, la base buena perdió datos"));
            let a = apartados(&d);
            assert_eq!(a.len(), 1, "{caso}");
            assert_eq!(hash(&a[0].join("pronto2.db-wal")), wal_ajeno, "{caso}: el -wal apartado no es el original");
            assert!(!a[0].join("pronto2.db").exists(), "{caso}: se apartó la base buena");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Rompedor 23-oct, datos MEDIA 1: tras usar esta versión, Pronto 2.0.2 (o un editor de SQLite)
    /// escribe sin los disparadores que tocan `meta` y muere sin volcar. Ese `-wal` es del usuario: no
    /// se aparta y se ve todo; antes de aplicarlo queda una copia exacta de la base y del `-wal` en
    /// `respaldo-<fecha-hora>\` (nada que no se pueda deshacer).
    #[test]
    fn nsn_integridad_wal_de_version_anterior_se_respeta_con_respaldo() {
        let dir = tmp("wal-version-anterior");
        let path = dir.join("pronto2.db");
        {
            let db = Db::open_at(&path).unwrap();
            fill(&db);
        }
        assert!(main_id(&path).is_some(), "precondición: base identificada y volcada");
        let c = Connection::open(&path).unwrap(); // sin disparadores: como 2.0.2
        for i in 0..5 {
            c.execute("INSERT INTO history (created_at, text, raw_text, app) VALUES (?1, ?2, '', 'Code')", params![format!("2026-10-23T1{i}:00:00+02:00"), format!("dictado en 2.0.2 {i}")]).unwrap();
        }
        c.execute("UPDATE history SET text = 'corregido a mano' WHERE id = 3", []).unwrap();
        let live = dump(&c);
        c.set_db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true).unwrap();
        drop(c);
        let (main_before, wal_before) = (hash(&path), hash(&sidecar(&path, "-wal")));
        let db = Db::open_at(&path).unwrap();
        assert!(db.aviso().is_none(), "apartó el -wal del usuario: {:?}", db.aviso());
        assert_same(&dump(&db.conn.lock()), &live, "faltan los cambios hechos con la versión anterior");
        drop(db);
        assert!(apartados(&dir).is_empty());
        let respaldos: Vec<PathBuf> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("respaldo-")))
            .collect();
        assert_eq!(respaldos.len(), 1, "una carpeta respaldo-<fecha-hora>");
        assert_eq!(hash(&respaldos[0].join("pronto2.db")), main_before, "el respaldo de la base no es el original");
        assert_eq!(hash(&respaldos[0].join("pronto2.db-wal")), wal_before, "el respaldo del -wal no es el original");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Rompedor 23-oct, datos MEDIA 3: una copia restaurada toma identidad propia al abrirla (sigue
    /// diciendo de qué base viene). Restaurar la MISMA copia otra vez, sin apartar el `-wal` de la
    /// sesión de entre medias, enseña la copia: ese `-wal` ya no lleva su `base_id`.
    #[test]
    fn nsn_integridad_copia_restaurada_dos_veces() {
        let dir = tmp("restaurar-dos-veces");
        let copias = dir.join("copias");
        std::fs::create_dir_all(&copias).unwrap();
        let path = dir.join("pronto2.db");
        let copy = {
            let db = Db::open_at(&path).unwrap();
            fill(&db);
            db.daily_backup(&copias, 7).unwrap().expect("copia del día")
        };
        let sin_meta = |v: Vec<String>| v.into_iter().filter(|r| !r.starts_with("meta|")).collect::<Vec<_>>();
        let of_copy = sin_meta(dump(&open_immutable(&copy).unwrap()));
        let copy_id = main_id(&copy);
        for s in ["-wal", "-shm"] {
            let _ = std::fs::remove_file(sidecar(&path, s));
        }
        std::fs::copy(&copy, &path).unwrap(); // 1.ª restauración, bien hecha
        let db = Db::open_at(&path).unwrap();
        assert!(db.aviso().is_none(), "{:?}", db.aviso());
        let id = read_meta(&db.conn.lock(), "base_id").unwrap();
        assert!(id.is_some() && id != copy_id, "la base restaurada lleva el base_id de la copia");
        assert_eq!(main_id(&path), id, "la identidad nueva no quedó en el fichero principal");
        assert!(read_meta(&db.conn.lock(), "copia_de").unwrap().is_some(), "se perdió de qué base viene");
        for i in 0..30 {
            db.add_history(&NewHistory { polished: false, text: &format!("tras restaurar {i}"), raw: "", app: "", duration_ms: 1, words: 1, fixes: Fixes::default(), transform: None }).unwrap();
        }
        db.conn.lock().set_db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true).unwrap();
        drop(db); // muere sin volcar
        std::fs::copy(&copy, &path).unwrap(); // 2.ª restauración de la misma copia, con el -wal al lado
        let db = Db::open_at(&path).unwrap();
        assert_same(&sin_meta(dump(&db.conn.lock())), &of_copy, "la 2.ª restauración no enseña la copia");
        assert!(db.aviso().is_some_and(|a| a.contains("apartado-")), "sin aviso del -wal apartado: {:?}", db.aviso());
        drop(db);
        assert_same(&sin_meta(dump(&Connection::open(&path).unwrap())), &of_copy, "tras cerrar, la base no es la copia");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Rompedor 22-oct-b, MEDIA 2: el usuario lo borra TODO (historial, diccionario, snippets, notas,
    /// reuniones), cambia un ajuste y Pronto muere sin volcar. Su `-wal` es suyo: no se aparta, no
    /// vuelve nada de lo borrado y el ajuste se conserva.
    #[test]
    fn nsn_integridad_borrado_legitimo_no_aparta_su_wal() {
        let dir = tmp("borrado-legitimo");
        let path = dir.join("pronto2.db");
        let db = Db::open_at(&path).unwrap();
        fill(&db);
        db.conn.lock().execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").unwrap(); // último volcado
        db.clear_history().unwrap();
        for w in db.list_words().unwrap() {
            db.delete_word(w.id).unwrap();
        }
        for s in db.list_snippets().unwrap() {
            db.delete_snippet(s.id).unwrap();
        }
        for n in db.list_notes().unwrap() {
            db.delete_note(n.id).unwrap();
        }
        for m in db.list_meetings(None, 100, 0).unwrap() {
            db.delete_meeting(m.meeting.id).unwrap();
        }
        let mut s = db.load_settings();
        s.sounds = true;
        s.first_name = "tras borrar".into();
        db.save_settings(&s).unwrap();
        assert_eq!(census(&db.conn.lock()).unwrap().1, 0);
        let after = dump(&db.conn.lock());
        db.conn.lock().set_db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true).unwrap();
        drop(db); // muere sin volcar
        assert!(file_len(&sidecar(&path, "-wal")) > 0, "precondición: lo borrado vive en el -wal");
        let db = Db::open_at(&path).unwrap();
        assert!(db.aviso().is_none(), "apartó el -wal del propio usuario: {:?}", db.aviso());
        assert_same(&dump(&db.conn.lock()), &after, "volvió algo borrado o se perdió el ajuste");
        assert_eq!(db.load_settings().first_name, "tras borrar");
        assert!(apartados(&dir).is_empty());
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Rompedor 22-oct-b, MEDIA 4: un programa tiene el `-shm` abierto sin compartir al arrancar. No
    /// es un `-wal` ajeno: `Db::open_at` da un error claro y no mueve nada, ni con el `-wal` propio ni
    /// con uno ajeno (apartar `-wal` y `-shm` es todo o nada). Al soltarlo, todo sigue ahí.
    #[cfg(windows)]
    #[test]
    fn nsn_integridad_shm_bloqueado_no_mueve_nada() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = tmp("shm-bloqueado");
        for caso in ["wal_propio", "wal_ajeno"] {
            let d = dir.join(caso);
            std::fs::create_dir_all(d.join("otra")).unwrap();
            let path = d.join("pronto2.db");
            let db = Db::open_at(&path).unwrap();
            fill(&db);
            db.conn.lock().execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
            for i in 0..30 {
                db.add_history(&NewHistory { polished: false, text: &format!("sin volcar {i}"), raw: "", app: "", duration_ms: 1, words: 1, fixes: Fixes::default(), transform: None })
                    .unwrap();
            }
            let live = dump(&db.conn.lock());
            db.conn.lock().set_db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true).unwrap();
            drop(db);
            if caso == "wal_ajeno" {
                let other = d.join("otra").join("pronto2.db");
                let o = Db::open_at(&other).unwrap();
                o.conn.lock().set_db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true).unwrap();
                std::fs::copy(sidecar(&other, "-wal"), sidecar(&path, "-wal")).unwrap();
                std::fs::copy(sidecar(&other, "-shm"), sidecar(&path, "-shm")).unwrap();
                drop(o);
            }
            let (wal_before, main_before) = (hash(&sidecar(&path, "-wal")), hash(&path));
            let lock = std::fs::OpenOptions::new().read(true).share_mode(0).open(sidecar(&path, "-shm")).unwrap();
            let r = Db::open_at(&path);
            assert!(r.is_err(), "{caso}: abrió con el -shm cogido");
            let e = format!("{:#}", r.err().unwrap());
            assert!(e.contains("otro programa"), "{caso}: error poco claro: {e}");
            assert_eq!(hash(&sidecar(&path, "-wal")), wal_before, "{caso}: el -wal se movió o cambió");
            assert_eq!(hash(&path), main_before, "{caso}: la base cambió");
            assert!(apartados(&d).is_empty(), "{caso}: quedó algo apartado");
            drop(lock);
            let db = Db::open_at(&path).unwrap();
            if caso == "wal_propio" {
                assert!(db.aviso().is_none());
                assert_same(&dump(&db.conn.lock()), &live, "wal_propio: al soltar el -shm faltan datos");
            } else {
                assert!(db.aviso().is_some(), "wal_ajeno: al soltar el -shm la guarda debe actuar");
                assert_eq!(apartados(&d).len(), 1);
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Una base de antes (sin `base_id`) sale de su primera apertura con el `base_id` escrito y
    /// volcado en el fichero principal: sin `-wal`, y con un `-wal` propio con datos sin volcar (que
    /// se respeta y queda volcado). Es estable entre aperturas.
    #[test]
    fn nsn_integridad_base_antigua_queda_con_base_id() {
        let dir = tmp("base-antigua");
        for (caso, wal_rows) in [("sin_wal", 0usize), ("con_wal", 15)] {
            let path = dir.join(format!("{caso}.db"));
            legacy(&path, wal_rows);
            assert_eq!(file_len(&sidecar(&path, "-wal")) > 0, wal_rows > 0, "{caso}: precondición del -wal");
            assert!(main_id(&path).is_none());
            let db = Db::open_at(&path).unwrap();
            assert!(db.aviso().is_none(), "{caso}: {:?}", db.aviso());
            assert_eq!(census(&db.conn.lock()).unwrap().1, 20 + wal_rows as i64, "{caso}: faltan filas");
            let id = read_meta(&db.conn.lock(), "base_id").unwrap().expect("base_id");
            assert_eq!(main_id(&path).as_deref(), Some(id.as_str()), "{caso}: el base_id no quedó en el fichero principal");
            assert_eq!(census(&open_immutable(&path).unwrap()).unwrap().1, 20 + wal_rows as i64, "{caso}: el -wal no quedó volcado");
            drop(db);
            assert_eq!(main_id(&path).as_deref(), Some(id.as_str()), "{caso}: el base_id cambió al reabrir");
            drop(Db::open_at(&path).unwrap());
            assert_eq!(main_id(&path).as_deref(), Some(id.as_str()), "{caso}: el base_id cambió al reabrir");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
