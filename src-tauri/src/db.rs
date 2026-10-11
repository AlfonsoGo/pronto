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
    pub(crate) conn: Mutex<Connection>,
}

fn data_dir() -> Result<PathBuf> {
    let dir = dirs::data_dir().context("no hay carpeta de datos de usuario")?.join("Pronto2");
    std::fs::create_dir_all(&dir).with_context(|| format!("no se pudo crear {}", dir.display()))?;
    Ok(dir)
}

pub fn db_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("pronto2.db"))
}

/// Carpeta de los WAV de cada dictado (`%APPDATA%\Pronto2\audio`).
pub fn audio_dir() -> Option<PathBuf> {
    let dir = data_dir().ok()?.join("audio");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

pub fn audio_path(id: i64) -> Option<PathBuf> {
    Some(audio_dir()?.join(format!("{id}.wav")))
}

fn remove_audio_file(id: i64) {
    if let Some(p) = audio_path(id) {
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

pub const POLISH_PROMPT: &str = "Mejora la claridad y la concisión del texto siguiente. Conserva el significado, el tono y el idioma original; corrige gramática y puntuación. Devuelve únicamente el texto resultante, sin comentarios ni comillas.";
pub const PROMPT_ENGINEER_PROMPT: &str = "Convierte el texto siguiente en un prompt claro y bien estructurado para un asistente de IA: define el rol, el objetivo, el contexto, las restricciones y el formato de salida esperado. Mantén el idioma original. Devuelve únicamente el prompt.";

fn migrate(conn: &mut Connection) -> Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 1 {
        let tx = conn.transaction()?;
        tx.execute_batch(SCHEMA_V1)?;
        tx.execute_batch("PRAGMA user_version = 1")?;
        tx.commit()?;
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
        tx.commit()?;
    }
    if version < 3 {
        let tx = conn.transaction()?;
        tx.execute_batch(MIGRATION_V3)?;
        tx.execute_batch("PRAGMA user_version = 3")?;
        tx.commit()?;
    }
    if version < 4 {
        let tx = conn.transaction()?;
        tx.execute_batch(MIGRATION_V4)?;
        tx.execute_batch("PRAGMA user_version = 4")?;
        tx.commit()?;
    }
    if version < 5 {
        let tx = conn.transaction()?;
        tx.execute_batch(MIGRATION_V5)?;
        tx.execute_batch("PRAGMA user_version = 5")?;
        tx.commit()?;
    }
    Ok(())
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
    })
}

fn transform_row(r: &Row) -> rusqlite::Result<Transform> {
    Ok(Transform { id: r.get(0)?, name: r.get(1)?, prompt: r.get(2)?, builtin: r.get::<_, i64>(3)? != 0, hotkey_index: r.get(4)? })
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
        let path = db_path()?;
        let conn = Connection::open(&path).with_context(|| format!("no se pudo abrir {}", path.display()))?;
        Self::init(conn)
    }

    #[cfg(test)]
    pub fn open_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Self> {
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")?;
        migrate(&mut conn)?;
        Ok(Self { conn: Mutex::new(conn) })
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
        let mut settings: Settings = serde_json::from_value(value.clone()).unwrap_or_default();
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

    pub fn save_settings(&self, s: &Settings) -> Result<()> {
        let json = serde_json::to_string(s)?;
        self.conn.lock().execute(
            "INSERT INTO settings (id, json) VALUES (1, ?1) ON CONFLICT(id) DO UPDATE SET json = excluded.json",
            params![json],
        )?;
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
        let like = query
            .map(str::trim)
            .filter(|q| !q.is_empty())
            .map(|q| format!("%{}%", q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")));
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {HISTORY_COLS} FROM history
             WHERE (?1 IS NULL OR text LIKE ?1 ESCAPE '\\' OR app LIKE ?1 ESCAPE '\\')
             ORDER BY id DESC LIMIT ?2 OFFSET ?3"
        ))?;
        let rows = stmt.query_map(params![like, limit.clamp(1, 1000), offset.max(0)], history_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Último dictado (no transform) para «pegar/copiar último».
    pub fn last_history_text(&self) -> Option<String> {
        self.conn
            .lock()
            .query_row("SELECT text FROM history ORDER BY id DESC LIMIT 1", [], |r| r.get(0))
            .optional()
            .unwrap_or(None)
    }

    pub fn delete_history(&self, id: i64) -> Result<()> {
        self.conn.lock().execute("DELETE FROM history WHERE id = ?1", params![id])?;
        remove_audio_file(id);
        Ok(())
    }

    pub fn clear_history(&self) -> Result<()> {
        let ids: Vec<i64> = {
            let conn = self.conn.lock();
            let mut stmt = conn.prepare("SELECT id FROM history WHERE has_audio = 1")?;
            let ids = stmt.query_map([], |r| r.get(0))?.filter_map(|r| r.ok()).collect();
            conn.execute("DELETE FROM history", [])?;
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
            let conn = self.conn.lock();
            let mut stmt = conn.prepare("SELECT id FROM history WHERE has_audio = 1 AND datetime(created_at) < datetime('now', ?1)")?;
            let ids: Vec<i64> = stmt.query_map(params![modifier], |r| r.get(0))?.filter_map(|r| r.ok()).collect();
            let n = conn.execute("DELETE FROM history WHERE datetime(created_at) < datetime('now', ?1)", params![modifier])?;
            (ids, n)
        };
        ids.into_iter().for_each(remove_audio_file);
        Ok(n)
    }

    /// Borra el audio guardado con más de `days` días. 0 = no se guarda audio: se borra todo.
    pub fn purge_audio(&self, days: u32) -> Result<usize> {
        let modifier = format!("-{days} days");
        let ids: Vec<i64> = {
            let conn = self.conn.lock();
            let mut stmt = conn.prepare(
                "SELECT id FROM history WHERE has_audio = 1 AND (?2 = 0 OR datetime(created_at) < datetime('now', ?1))",
            )?;
            let ids: Vec<i64> = stmt.query_map(params![modifier, days], |r| r.get(0))?.filter_map(|r| r.ok()).collect();
            for id in &ids {
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

    pub fn list_words(&self) -> Result<Vec<Word>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT id, word, replacement, created_at, starred, source FROM words ORDER BY word COLLATE NOCASE")?;
        let rows = stmt.query_map([], word_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn add_word(&self, word: &str, replacement: Option<&str>) -> Result<Word> {
        let replacement = replacement.map(str::trim).filter(|r| !r.is_empty());
        let created_at = now_iso();
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO words (word, replacement, created_at, starred, source) VALUES (?1, ?2, ?3, 0, 'manual')",
            params![word.trim(), replacement, created_at],
        )?;
        Ok(Word {
            id: conn.last_insert_rowid(),
            word: word.trim().into(),
            replacement: replacement.map(String::from),
            created_at,
            starred: false,
            source: "manual".into(),
        })
    }

    pub fn update_word(&self, id: i64, word: &str, replacement: Option<&str>) -> Result<()> {
        let replacement = replacement.map(str::trim).filter(|r| !r.is_empty());
        self.conn.lock().execute(
            "UPDATE words SET word = ?2, replacement = ?3 WHERE id = ?1",
            params![id, word.trim(), replacement],
        )?;
        Ok(())
    }

    pub fn toggle_word_star(&self, id: i64) -> Result<()> {
        self.conn.lock().execute("UPDATE words SET starred = 1 - starred WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn delete_word(&self, id: i64) -> Result<()> {
        self.conn.lock().execute("DELETE FROM words WHERE id = ?1", params![id])?;
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
    fn diccionario_snippets_notas() {
        let db = Db::open_memory().unwrap();
        let w = db.add_word("wispr", Some("Wispr")).unwrap();
        assert!(!w.starred && w.source == "manual");
        db.update_word(w.id, "wispr", Some("")).unwrap();
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
        // Una base v4 con datos pasa a v5 conservándolos.
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA_V1).unwrap();
        conn.execute_batch(MIGRATION_V2).unwrap();
        conn.execute_batch(MIGRATION_V3).unwrap();
        conn.execute_batch(MIGRATION_V4).unwrap();
        conn.execute_batch("PRAGMA user_version = 4").unwrap();
        conn.execute("INSERT INTO history (created_at, text, raw_text, app, duration_ms, words) VALUES ('2025-01-01T10:00:00+01:00','hola','hola','Code',1000,1)", []).unwrap();
        migrate(&mut conn).unwrap();
        let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, 5);
        let db = Db { conn: Mutex::new(conn) };
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
        assert_eq!(v, 5);
        let db = Db { conn: Mutex::new(conn) };
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
        assert_eq!(v, 5);
        let db = Db { conn: Mutex::new(conn) };
        assert_eq!(db.list_history(None, 10, 0).unwrap()[0].text, "adiós");
        assert_eq!(db.list_transforms().unwrap()[0].name, "Mío");
        // Las tablas nuevas existen y funcionan, incluida la búsqueda de texto.
        let m = db.create_meeting("Prueba", None).unwrap();
        db.add_segment(&Segment { meeting_id: m.id, speaker: "me".into(), start_ms: 0, end_ms: 1500, text: "hablamos del presupuesto".into() }).unwrap();
        assert_eq!(db.list_meetings(Some("presupuesto"), 10, 0).unwrap().len(), 1);
        assert!(db.list_meetings(Some("jirafa"), 10, 0).unwrap().is_empty());
        assert!(!db.get_meeting(m.id).unwrap().unwrap().auto);
        // Idempotente.
        migrate(&mut db.conn.lock()).unwrap();
    }
}
