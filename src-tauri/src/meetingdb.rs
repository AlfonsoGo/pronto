//! Consultas SQLite del Notetaker (tablas `meetings`, `meeting_segments`, `meeting_actions` y el
//! índice FTS5 `meeting_fts`). El esquema y su migración viven en `db.rs`.

use crate::db::Db;
use crate::models::*;
use crate::summary::ParsedSummary;
use anyhow::Result;
use chrono::Local;
use rusqlite::{params, OptionalExtension, Row};

const MEETING_COLS: &str = "id, title, started_at, ended_at, duration_ms, app, summary_status, auto";

fn meeting_row(r: &Row) -> rusqlite::Result<Meeting> {
    Ok(Meeting {
        id: r.get(0)?,
        title: r.get(1)?,
        started_at: r.get(2)?,
        ended_at: r.get(3)?,
        duration_ms: r.get(4)?,
        app: r.get(5)?,
        summary_status: r.get(6)?,
        auto: r.get::<_, i64>(7)? != 0,
    })
}

/// Título por defecto: «Reunión 11/10 10:32».
pub fn default_title() -> String {
    format!("Reunión {}", Local::now().format("%d/%m %H:%M"))
}

/// Palabras útiles para buscar: letras/dígitos, ≥ 3 caracteres, en minúsculas, sin repetidas.
pub fn search_words(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for w in text.split(|c: char| !c.is_alphanumeric()) {
        let w = w.to_lowercase();
        if w.chars().count() >= 3 && !out.contains(&w) {
            out.push(w);
        }
    }
    out
}

/// Consulta FTS5 con las palabras unidas por `joiner` (`" "` = todas, `" OR "` = cualquiera).
pub fn fts_query(text: &str, joiner: &str, prefix: bool) -> Option<String> {
    let words = search_words(text);
    if words.is_empty() {
        return None;
    }
    Some(words.iter().map(|w| format!("\"{w}\"{}", if prefix { "*" } else { "" })).collect::<Vec<_>>().join(joiner))
}

impl Db {
    #[cfg(test)]
    pub fn create_meeting(&self, title: &str, app: Option<&str>) -> Result<Meeting> {
        self.create_meeting_titled(title, app, title.trim().is_empty(), false)
    }

    /// `replaceable`: el título es provisional y el que genere el resumen lo sustituirá.
    /// `auto`: la reunión la arrancó la detección automática.
    pub fn create_meeting_titled(&self, title: &str, app: Option<&str>, replaceable: bool, auto: bool) -> Result<Meeting> {
        let title = if title.trim().is_empty() { default_title() } else { title.trim().to_string() };
        let started_at = Local::now().to_rfc3339();
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO meetings (title, title_auto, started_at, app, summary_status, auto) VALUES (?1, ?2, ?3, ?4, 'none', ?5)",
            params![title, replaceable as i64, started_at, app, auto as i64],
        )?;
        Ok(Meeting { id: conn.last_insert_rowid(), title, started_at, ended_at: None, duration_ms: 0, app: app.map(String::from), summary_status: "none".into(), auto })
    }

    pub fn get_meeting(&self, id: i64) -> Result<Option<Meeting>> {
        Ok(self
            .conn
            .lock()
            .query_row(&format!("SELECT {MEETING_COLS} FROM meetings WHERE id = ?1"), params![id], meeting_row)
            .optional()?)
    }

    pub fn add_segment(&self, s: &Segment) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO meeting_segments (meeting_id, speaker, start_ms, end_ms, text) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![s.meeting_id, s.speaker, s.start_ms, s.end_ms, s.text],
        )?;
        conn.execute(
            "INSERT INTO meeting_fts (content, meeting_id, kind) VALUES (?1, ?2, 'seg')",
            params![s.text, s.meeting_id],
        )?;
        Ok(())
    }

    pub fn segments(&self, meeting_id: i64) -> Result<Vec<Segment>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT meeting_id, speaker, start_ms, end_ms, text FROM meeting_segments WHERE meeting_id = ?1 ORDER BY start_ms, id",
        )?;
        let rows = stmt.query_map(params![meeting_id], |r| {
            Ok(Segment { meeting_id: r.get(0)?, speaker: r.get(1)?, start_ms: r.get(2)?, end_ms: r.get(3)?, text: r.get(4)? })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Cierra la grabación: fija fin y duración y deja el resumen «pendiente».
    pub fn finish_meeting(&self, id: i64, duration_ms: i64) -> Result<()> {
        self.conn.lock().execute(
            "UPDATE meetings SET ended_at = ?2, duration_ms = ?3, summary_status = 'pending' WHERE id = ?1",
            params![id, Local::now().to_rfc3339(), duration_ms],
        )?;
        Ok(())
    }

    pub fn set_summary_status(&self, id: i64, status: &str) -> Result<()> {
        self.conn.lock().execute("UPDATE meetings SET summary_status = ?2 WHERE id = ?1", params![id, status])?;
        Ok(())
    }

    /// Guarda el resumen estructurado (sustituye al anterior). El título generado solo se usa si
    /// el usuario no lo había puesto él.
    pub fn save_summary(&self, id: i64, s: &ParsedSummary) -> Result<()> {
        let conn = self.conn.lock();
        let decisions = serde_json::to_string(&s.decisions)?;
        let topics = serde_json::to_string(&s.topics)?;
        conn.execute(
            "UPDATE meetings SET summary = ?2, decisions = ?3, topics = ?4, summary_status = 'done' WHERE id = ?1",
            params![id, s.summary, decisions, topics],
        )?;
        if let Some(t) = s.title.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
            conn.execute("UPDATE meetings SET title = ?2 WHERE id = ?1 AND title_auto = 1", params![id, t])?;
        }
        conn.execute("DELETE FROM meeting_actions WHERE meeting_id = ?1", params![id])?;
        for (i, a) in s.action_items.iter().enumerate() {
            conn.execute(
                "INSERT INTO meeting_actions (meeting_id, idx, text, owner, done) VALUES (?1, ?2, ?3, ?4, 0)",
                params![id, i as i64, a.text, a.owner],
            )?;
        }
        conn.execute("DELETE FROM meeting_fts WHERE meeting_id = ?1 AND kind = 'sum'", params![id])?;
        let title: String = conn.query_row("SELECT title FROM meetings WHERE id = ?1", params![id], |r| r.get(0))?;
        let mut text = format!("{title}\n{}", s.summary);
        for d in &s.decisions {
            text.push('\n');
            text.push_str(d);
        }
        for a in &s.action_items {
            text.push('\n');
            text.push_str(&a.text);
        }
        conn.execute("INSERT INTO meeting_fts (content, meeting_id, kind) VALUES (?1, ?2, 'sum')", params![text, id])?;
        Ok(())
    }

    pub fn get_meeting_detail(&self, id: i64) -> Result<Option<MeetingDetail>> {
        let Some(meeting) = self.get_meeting(id)? else { return Ok(None) };
        let segments = self.segments(id)?;
        let conn = self.conn.lock();
        let (summary, decisions, topics): (Option<String>, String, String) = conn.query_row(
            "SELECT summary, decisions, topics FROM meetings WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let mut stmt = conn.prepare("SELECT text, owner, done FROM meeting_actions WHERE meeting_id = ?1 ORDER BY idx")?;
        let action_items = stmt
            .query_map(params![id], |r| Ok(ActionItem { text: r.get(0)?, owner: r.get(1)?, done: r.get::<_, i64>(2)? != 0 }))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(Some(MeetingDetail {
            meeting,
            segments,
            summary,
            action_items,
            decisions: serde_json::from_str(&decisions).unwrap_or_default(),
            topics: serde_json::from_str(&topics).unwrap_or_default(),
        }))
    }

    pub fn list_meetings(&self, query: Option<&str>, limit: i64, offset: i64) -> Result<Vec<MeetingListItem>> {
        let query = query.map(str::trim).filter(|q| !q.is_empty());
        let fts = query.and_then(|q| fts_query(q, " ", true));
        let like = query.map(|q| format!("%{}%", q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")));
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT m.id, m.title, m.started_at, m.ended_at, m.duration_ms, m.app, m.summary_status, m.auto,
                    COALESCE(NULLIF(substr(m.summary, 1, 160), ''),
                             (SELECT text FROM meeting_segments s WHERE s.meeting_id = m.id ORDER BY start_ms, id LIMIT 1), ''),
                    (SELECT COUNT(*) FROM meeting_actions a WHERE a.meeting_id = m.id)
             FROM meetings m
             WHERE (?1 IS NULL AND ?2 IS NULL)
                OR (?1 IS NOT NULL AND m.id IN (SELECT CAST(meeting_id AS INTEGER) FROM meeting_fts WHERE meeting_fts MATCH ?1))
                OR (?2 IS NOT NULL AND m.title LIKE ?2 ESCAPE '\\')
             ORDER BY m.started_at DESC, m.id DESC LIMIT ?3 OFFSET ?4",
        )?;
        let rows = stmt.query_map(params![fts, like, limit.clamp(1, 500), offset.max(0)], |r| {
            Ok(MeetingListItem { meeting: meeting_row(r)?, preview: r.get(8)?, action_count: r.get(9)? })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn rename_meeting(&self, id: i64, title: &str) -> Result<()> {
        self.conn.lock().execute("UPDATE meetings SET title = ?2, title_auto = 0 WHERE id = ?1", params![id, title.trim()])?;
        Ok(())
    }

    pub fn delete_meeting(&self, id: i64) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM meeting_fts WHERE meeting_id = ?1", params![id])?;
        conn.execute("DELETE FROM meeting_actions WHERE meeting_id = ?1", params![id])?;
        conn.execute("DELETE FROM meeting_segments WHERE meeting_id = ?1", params![id])?;
        conn.execute("DELETE FROM meetings WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn toggle_action_item(&self, meeting_id: i64, index: i64) -> Result<()> {
        self.conn.lock().execute(
            "UPDATE meeting_actions SET done = 1 - done WHERE meeting_id = ?1 AND idx = ?2",
            params![meeting_id, index],
        )?;
        Ok(())
    }

    /// Reuniones que se quedaron «grabando» (la app se cerró): se cierran con la duración que se
    /// pueda deducir y el resumen queda en `failed`, listo para reintentar. Devuelve cuántas.
    pub fn close_orphan_meetings(&self) -> Result<usize> {
        let conn = self.conn.lock();
        let n = conn.execute(
            "UPDATE meetings SET
                duration_ms = COALESCE((SELECT MAX(end_ms) FROM meeting_segments s WHERE s.meeting_id = meetings.id), 0),
                ended_at = started_at,
                summary_status = 'failed'
             WHERE ended_at IS NULL",
            [],
        )?;
        Ok(n)
    }

    /// Reuniones más relevantes para una pregunta: `(id, puntuación)` mejor primero.
    pub fn rank_meetings(&self, question: &str, limit: i64) -> Result<Vec<i64>> {
        let Some(q) = fts_query(question, " OR ", false) else { return Ok(Vec::new()) };
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT mid, SUM(score) AS total FROM (
                 SELECT CAST(meeting_id AS INTEGER) AS mid, -bm25(meeting_fts) AS score
                 FROM meeting_fts WHERE meeting_fts MATCH ?1 LIMIT 100000
             ) GROUP BY mid ORDER BY total DESC LIMIT ?2",
        )?;
        let ids = stmt.query_map(params![q, limit], |r| r.get::<_, i64>(0))?.collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(ids)
    }

    pub fn recent_meeting_ids(&self, limit: i64) -> Result<Vec<i64>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT id FROM meetings WHERE ended_at IS NOT NULL ORDER BY started_at DESC LIMIT ?1")?;
        let ids = stmt.query_map(params![limit], |r| r.get::<_, i64>(0))?.collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::summary::ParsedAction;

    fn seg(id: i64, who: &str, a: i64, b: i64, t: &str) -> Segment {
        Segment { meeting_id: id, speaker: who.into(), start_ms: a, end_ms: b, text: t.into() }
    }

    #[test]
    fn palabras_y_consulta_fts() {
        assert_eq!(search_words("¿Qué decidimos del presupuesto de Marketing?"), ["qué", "decidimos", "del", "presupuesto", "marketing"]);
        assert_eq!(fts_query("hola mundo", " OR ", false).as_deref(), Some("\"hola\" OR \"mundo\""));
        assert_eq!(fts_query("pres", " ", true).as_deref(), Some("\"pres\"*"));
        assert_eq!(fts_query("a b", " ", true), None);
    }

    #[test]
    fn ciclo_completo_de_una_reunion() {
        let db = Db::open_memory().unwrap();
        let m = db.create_meeting("", Some("Zoom")).unwrap();
        assert!(m.title.starts_with("Reunión"));
        db.add_segment(&seg(m.id, "them", 3000, 5000, "Necesitamos cerrar el presupuesto el viernes")).unwrap();
        db.add_segment(&seg(m.id, "me", 1000, 2500, "Hola a todos, empezamos")).unwrap();
        // Los segmentos salen ordenados por inicio.
        assert_eq!(db.segments(m.id).unwrap()[0].speaker, "me");
        db.finish_meeting(m.id, 6000).unwrap();
        assert_eq!(db.get_meeting(m.id).unwrap().unwrap().summary_status, "pending");

        let summary = ParsedSummary {
            title: Some("Cierre de presupuesto".into()),
            summary: "Se acordó cerrar el presupuesto.".into(),
            action_items: vec![ParsedAction { text: "Enviar cifras".into(), owner: Some("Ana".into()) }, ParsedAction { text: "Revisar contrato".into(), owner: None }],
            decisions: vec!["Cerrar el viernes".into()],
            topics: vec!["presupuesto".into()],
        };
        db.save_summary(m.id, &summary).unwrap();
        let d = db.get_meeting_detail(m.id).unwrap().unwrap();
        assert_eq!(d.meeting.title, "Cierre de presupuesto"); // título automático sustituido
        assert_eq!(d.meeting.summary_status, "done");
        assert_eq!(d.action_items.len(), 2);
        assert_eq!(d.action_items[0].owner.as_deref(), Some("Ana"));
        assert_eq!(d.decisions, ["Cerrar el viernes"]);

        db.toggle_action_item(m.id, 1).unwrap();
        assert!(db.get_meeting_detail(m.id).unwrap().unwrap().action_items[1].done);

        // Búsqueda por segmento, por resumen y por título; y vista previa/recuento.
        assert_eq!(db.list_meetings(Some("viernes"), 10, 0).unwrap().len(), 1);
        assert_eq!(db.list_meetings(Some("cifras"), 10, 0).unwrap().len(), 1);
        assert_eq!(db.list_meetings(Some("Cierre"), 10, 0).unwrap().len(), 1);
        let item = &db.list_meetings(None, 10, 0).unwrap()[0];
        assert_eq!(item.action_count, 2);
        assert!(item.preview.starts_with("Se acordó"));
        assert_eq!(db.rank_meetings("¿Cuándo cerramos el presupuesto?", 5).unwrap(), [m.id]);

        // Un título puesto a mano no lo pisa un resumen posterior.
        db.rename_meeting(m.id, "Mi título").unwrap();
        db.save_summary(m.id, &summary).unwrap();
        assert_eq!(db.get_meeting(m.id).unwrap().unwrap().title, "Mi título");

        db.delete_meeting(m.id).unwrap();
        assert!(db.list_meetings(Some("presupuesto"), 10, 0).unwrap().is_empty());
        assert!(db.get_meeting(m.id).unwrap().is_none());
    }

    #[test]
    fn reuniones_huerfanas_se_cierran() {
        let db = Db::open_memory().unwrap();
        let m = db.create_meeting("Sin cerrar", None).unwrap();
        db.add_segment(&seg(m.id, "me", 0, 4200, "hola")).unwrap();
        assert_eq!(db.close_orphan_meetings().unwrap(), 1);
        let got = db.get_meeting(m.id).unwrap().unwrap();
        assert_eq!((got.duration_ms, got.summary_status.as_str()), (4200, "failed"));
        assert_eq!(db.close_orphan_meetings().unwrap(), 0);
    }
}
