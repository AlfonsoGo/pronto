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

fn insert_segment(conn: &rusqlite::Connection, meeting_id: i64, s: &Segment) -> rusqlite::Result<usize> {
    conn.execute(
        "INSERT INTO meeting_segments (meeting_id, speaker, start_ms, end_ms, text, raw_text, kind, overlap, reply_to)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![meeting_id, s.speaker, s.start_ms, s.end_ms, s.text, s.raw_text, s.kind, s.overlap as i64, s.reply_to],
    )
}

/// Título por defecto: «Reunión 11/10 10:32».
pub fn default_title() -> String {
    format!("Reunión {}", Local::now().format("%d/%m %H:%M"))
}

/// Palabras útiles para preguntar y puntuar: letras/dígitos, en minúsculas, sin repetidas, de ≥ 3
/// caracteres o, más cortas, siglas y códigos («IA», «Q3», «UX»: en mayúsculas o con cifras). Las
/// cortas en minúscula («de», «el») no puntúan: casi todas las reuniones las tienen.
pub fn search_words(text: &str) -> Vec<String> {
    words_of(text, false)
}

/// Palabras de `text`; con `all`, también las de 1-2 letras en minúscula.
fn words_of(text: &str, all: bool) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in text.split(|c: char| !c.is_alphanumeric()) {
        let n = raw.chars().count();
        let acronym = n > 0 && (raw.chars().any(|c| c.is_ascii_digit()) || (n >= 2 && raw.chars().all(|c| !c.is_lowercase())));
        let w = raw.to_lowercase();
        if (n >= 3 || acronym || (all && n > 0)) && !out.contains(&w) {
            out.push(w);
        }
    }
    out
}

fn fts_of(words: Vec<String>, joiner: &str, prefix: bool) -> Option<String> {
    if words.is_empty() {
        return None;
    }
    Some(words.iter().map(|w| format!("\"{w}\"{}", if prefix { "*" } else { "" })).collect::<Vec<_>>().join(joiner))
}

/// Consulta FTS5 con las palabras unidas por `joiner` (`" "` = todas, `" OR "` = cualquiera).
pub fn fts_query(text: &str, joiner: &str, prefix: bool) -> Option<String> {
    fts_of(search_words(text), joiner, prefix)
}

/// Consulta del buscador de reuniones: TODAS las palabras, también las de 1-2 letras («IA», «Q3»,
/// escritas como sea), como prefijo. Antes las cortas se ignoraban: «IA» no encontraba nada y
/// «plan IA» devolvía reuniones sin IA (rompedor 22-oct-c, BAJA 7).
pub fn fts_search_query(text: &str) -> Option<String> {
    fts_of(words_of(text, true), " ", true)
}

/// Forma de una tarea para reconocerla tras «Regenerar»: sin tildes ni mayúsculas, solo letras y
/// cifras separadas por un espacio.
fn task_key(text: &str) -> String {
    crate::anclaje::norm(text).split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ")
}

/// Parecido mínimo (`notes::similarity`) para dar por «la misma tarea» una redactada con otras palabras.
const TASK_SAME: f32 = 0.8;

/// Qué tareas nuevas heredan el «hecho» de las viejas hechas: primero las de texto idéntico y
/// después, emparejando cada vieja una sola vez, las muy parecidas con el mismo responsable (si
/// ambas lo tienen).
fn carry_done(old: &[(String, Option<String>)], new: &[crate::summary::ParsedAction]) -> Vec<bool> {
    let mut used = vec![false; old.len()];
    let mut done = vec![false; new.len()];
    for (i, a) in new.iter().enumerate() {
        let key = task_key(&a.text);
        if let Some(j) = (0..old.len()).find(|&j| !used[j] && task_key(&old[j].0) == key) {
            used[j] = true;
            done[i] = true;
        }
    }
    let owner = |o: &Option<String>| o.as_deref().map(task_key).filter(|o| !o.is_empty());
    for (i, a) in new.iter().enumerate() {
        if done[i] {
            continue;
        }
        let best = (0..old.len())
            .filter(|&j| !used[j])
            .filter(|&j| match (owner(&old[j].1), owner(&a.owner)) {
                (Some(x), Some(y)) => x == y,
                _ => true,
            })
            .map(|j| (j, crate::notes::similarity(&old[j].0, &a.text)))
            .filter(|(_, sim)| *sim >= TASK_SAME)
            .max_by(|x, y| x.1.total_cmp(&y.1));
        if let Some((j, _)) = best {
            used[j] = true;
            done[i] = true;
        }
    }
    done
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

    #[cfg(test)]
    pub fn add_segment(&self, s: &Segment) -> Result<()> {
        // Fila y su entrada FTS en la misma tanda (que lleva `meta`; rompedor 22-oct-c, datos ALTA 1).
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        insert_segment(&tx, s.meeting_id, s)?;
        tx.execute(
            "INSERT INTO meeting_fts (content, meeting_id, kind) VALUES (?1, ?2, 'seg')",
            params![s.text, s.meeting_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Sustituye la transcripción por la de la fase 2 (o por la misma ya pulida y etiquetada).
    /// `complete`: salió entera y queda como final; si no, se reintenta desde el audio. Devuelve
    /// `false` si la reunión ya no existe (se borró mientras se procesaba).
    pub fn replace_segments(&self, meeting_id: i64, segments: &[Segment], complete: bool) -> Result<bool> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        if tx.execute("UPDATE meetings SET transcript_final = ?2 WHERE id = ?1", params![meeting_id, complete as i64])? == 0 {
            return Ok(false);
        }
        tx.execute("DELETE FROM meeting_segments WHERE meeting_id = ?1", params![meeting_id])?;
        tx.execute("DELETE FROM meeting_fts WHERE meeting_id = ?1 AND kind = 'seg'", params![meeting_id])?;
        for s in segments {
            insert_segment(&tx, meeting_id, s)?;
            tx.execute("INSERT INTO meeting_fts (content, meeting_id, kind) VALUES (?1, ?2, 'seg')", params![s.text, meeting_id])?;
        }
        tx.commit()?;
        Ok(true)
    }

    /// ¿La transcripción guardada es ya la final de la fase 2?
    pub fn transcript_final(&self, meeting_id: i64) -> Result<bool> {
        let v: Option<i64> = self
            .conn
            .lock()
            .query_row("SELECT transcript_final FROM meetings WHERE id = ?1", params![meeting_id], |r| r.get(0))
            .optional()?;
        Ok(v.unwrap_or(0) != 0)
    }

    /// Alarga la duración guardada si el audio dice que la reunión duró más (reunión recuperada
    /// tras un cierre de la app, cuya duración salió de la vista previa).
    pub fn extend_duration(&self, meeting_id: i64, duration_ms: i64) -> Result<()> {
        self.conn.lock().execute("UPDATE meetings SET duration_ms = MAX(duration_ms, ?2) WHERE id = ?1", params![meeting_id, duration_ms])?;
        Ok(())
    }

    pub fn segments(&self, meeting_id: i64) -> Result<Vec<Segment>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT meeting_id, speaker, start_ms, end_ms, text, raw_text, kind, overlap, reply_to FROM meeting_segments
             WHERE meeting_id = ?1 ORDER BY start_ms, id",
        )?;
        let rows = stmt.query_map(params![meeting_id], |r| {
            Ok(Segment {
                meeting_id: r.get(0)?,
                speaker: r.get(1)?,
                start_ms: r.get(2)?,
                end_ms: r.get(3)?,
                text: r.get(4)?,
                raw_text: r.get(5)?,
                kind: r.get(6)?,
                overlap: r.get::<_, i64>(7)? != 0,
                reply_to: r.get(8)?,
            })
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

    /// Un intento de generar o regenerar las notas ha fallado. Sin notas previas, `failed`. Con
    /// notas buenas ya guardadas (un «Regenerar» que falla) se queda en `done`: si no, el hub las
    /// esconde y no deja marcar las tareas (rompedor 23-oct, reuniones MEDIA 5). Devuelve `true`
    /// si conservó las notas anteriores.
    pub fn fail_summary_status(&self, id: i64) -> Result<bool> {
        let conn = self.conn.lock();
        let had: bool = conn
            .query_row(
                "SELECT (COALESCE(summary, '') != '' OR EXISTS (SELECT 1 FROM meeting_actions a WHERE a.meeting_id = m.id)) FROM meetings m WHERE m.id = ?1",
                params![id],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(false);
        conn.execute("UPDATE meetings SET summary_status = ?2 WHERE id = ?1", params![id, if had { "done" } else { "failed" }])?;
        Ok(had)
    }

    /// Guarda el resumen estructurado (sustituye al anterior). El título generado solo se usa si
    /// el usuario no lo había puesto él.
    pub fn save_summary(&self, id: i64, s: &ParsedSummary) -> Result<()> {
        // Todo en una transacción: el resumen, sus tareas y su entrada FTS juntos, y la tanda del
        // -wal lleva `meta` aunque la última escritura sea la del índice (rompedor 22-oct-c, datos
        // ALTA 1 (c)).
        let mut guard = self.conn.lock();
        let conn = guard.transaction()?;
        let json = |v: &Vec<String>| serde_json::to_string(v);
        // Cita de cada decisión, alineada con `decisions` (decisión A de Txonso, 23-oct; esquema v10).
        let decision_citas: Vec<Option<&crate::summary::Cita>> = (0..s.decisions.len()).map(|i| s.decision_citas.get(i).and_then(Option::as_ref)).collect();
        conn.execute(
            "UPDATE meetings SET summary = ?2, decisions = ?3, topics = ?4, key_points = ?5, next_steps = ?6, open_questions = ?7,
                 questions = ?8, said_me = ?9, said_them = ?10, decision_citas = ?11, summary_status = 'done' WHERE id = ?1",
            params![
                id,
                s.summary,
                json(&s.decisions)?,
                json(&s.topics)?,
                json(&s.key_points)?,
                json(&s.next_steps)?,
                json(&s.open_questions)?,
                serde_json::to_string(&s.questions)?,
                json(&s.said_me)?,
                json(&s.said_them)?,
                serde_json::to_string(&decision_citas)?
            ],
        )?;
        if let Some(t) = s.title.as_deref().map(crate::summary::one_line_title).filter(|t| !t.is_empty()) {
            conn.execute("UPDATE meetings SET title = ?2 WHERE id = ?1 AND title_auto = 1", params![id, t])?;
        }
        // «Regenerar» no desmarca lo que el usuario ya hizo: una tarea que sigue (mismo texto
        // normalizado, o redactada con otras palabras pero igual de parecida y del mismo
        // responsable) conserva su `done` (rompedor 22-oct-c, BAJA 6; 23-oct, reuniones BAJA 6).
        let done_before: Vec<(String, Option<String>)> = {
            let mut stmt = conn.prepare("SELECT text, owner FROM meeting_actions WHERE meeting_id = ?1 AND done != 0")?;
            let rows = stmt.query_map(params![id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)))?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };
        let done_flags = carry_done(&done_before, &s.action_items);
        conn.execute("DELETE FROM meeting_actions WHERE meeting_id = ?1", params![id])?;
        for (i, a) in s.action_items.iter().enumerate() {
            let done = done_flags[i];
            let cita = s.action_citas.get(i).and_then(Option::as_ref).map(serde_json::to_string).transpose()?;
            conn.execute(
                "INSERT INTO meeting_actions (meeting_id, idx, text, owner, done, due, cita) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![id, i as i64, a.text, a.owner, done as i64, a.due, cita],
            )?;
        }
        conn.execute("DELETE FROM meeting_fts WHERE meeting_id = ?1 AND kind = 'sum'", params![id])?;
        let title: String = conn.query_row("SELECT title FROM meetings WHERE id = ?1", params![id], |r| r.get(0))?;
        let mut text = format!("{title}\n{}", s.summary);
        for d in s.decisions.iter().chain(&s.key_points).chain(&s.topics) {
            text.push('\n');
            text.push_str(d);
        }
        for a in &s.action_items {
            text.push('\n');
            text.push_str(&a.text);
        }
        conn.execute("INSERT INTO meeting_fts (content, meeting_id, kind) VALUES (?1, ?2, 'sum')", params![text, id])?;
        conn.commit()?;
        Ok(())
    }

    pub fn get_meeting_detail(&self, id: i64) -> Result<Option<MeetingDetail>> {
        let Some(meeting) = self.get_meeting(id)? else { return Ok(None) };
        let segments = self.segments(id)?;
        let conn = self.conn.lock();
        let (summary, lists, transcript_final, questions, decision_citas): (Option<String>, [String; 7], i64, String, String) = conn.query_row(
            "SELECT summary, decisions, topics, key_points, next_steps, open_questions, said_me, said_them, transcript_final, questions, decision_citas
             FROM meetings WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, [r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?], r.get(8)?, r.get(9)?, r.get(10)?)),
        )?;
        let mut stmt = conn.prepare("SELECT text, owner, done, due, cita FROM meeting_actions WHERE meeting_id = ?1 ORDER BY idx")?;
        // Una cita ilegible (o la de unas notas anteriores a la v10, que no tienen) se ve como antes: sin cita.
        let action_items = stmt
            .query_map(params![id], |r| {
                Ok(ActionItem {
                    text: r.get(0)?,
                    owner: r.get(1)?,
                    done: r.get::<_, i64>(2)? != 0,
                    due: r.get(3)?,
                    cita: r.get::<_, Option<String>>(4)?.and_then(|j| serde_json::from_str(&j).ok()),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let [decisions, topics, key_points, next_steps, open_questions, said_by_me, said_by_them] =
            lists.map(|j| serde_json::from_str::<Vec<String>>(&j).unwrap_or_default());
        let decision_citas: Vec<Option<crate::summary::Cita>> = serde_json::from_str(&decision_citas).unwrap_or_default();
        Ok(Some(MeetingDetail {
            meeting,
            segments,
            summary,
            action_items,
            decisions,
            topics,
            key_points,
            next_steps,
            open_questions,
            transcript_final: transcript_final != 0,
            questions: serde_json::from_str(&questions).unwrap_or_default(),
            said_by_me,
            said_by_them,
            decision_citas,
        }))
    }

    /// ¿Ya pasó el pulido selectivo de turnos (Contrato v12)?
    pub fn polish_done(&self, id: i64) -> Result<bool> {
        let v: Option<i64> = self.conn.lock().query_row("SELECT polish_done FROM meetings WHERE id = ?1", params![id], |r| r.get(0)).optional()?;
        Ok(v.unwrap_or(0) != 0)
    }

    pub fn set_polish_done(&self, id: i64) -> Result<()> {
        self.conn.lock().execute("UPDATE meetings SET polish_done = 1 WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn list_meetings(&self, query: Option<&str>, limit: i64, offset: i64) -> Result<Vec<MeetingListItem>> {
        let query = query.map(str::trim).filter(|q| !q.is_empty());
        let fts = query.and_then(fts_search_query);
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
        // Una transacción: el índice FTS no se queda sin su reunión a medias, y la tanda lleva `meta`.
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM meeting_fts WHERE meeting_id = ?1", params![id])?;
        tx.execute("DELETE FROM meeting_actions WHERE meeting_id = ?1", params![id])?;
        tx.execute("DELETE FROM meeting_segments WHERE meeting_id = ?1", params![id])?;
        tx.execute("DELETE FROM meetings WHERE id = ?1", params![id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn toggle_action_item(&self, meeting_id: i64, index: i64) -> Result<()> {
        self.conn.lock().execute(
            "UPDATE meeting_actions SET done = 1 - done WHERE meeting_id = ?1 AND idx = ?2",
            params![meeting_id, index],
        )?;
        Ok(())
    }

    /// Reuniones que se quedaron «grabando» o a medio procesar (la app se cerró en la fase 2): se
    /// cierran con la duración que se pueda deducir y el resumen queda en `failed`, listo para
    /// reintentar con su audio. Solo al arrancar: entonces no hay ningún hilo de fase 2 vivo.
    /// Devuelve cuántas.
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
        // Una `pending` que ya tenía notas (un «Regenerar» cortado por el cierre) las conserva en
        // `done`: con `failed` el hub las escondería (rompedor 23-oct, reuniones MEDIA 5).
        let stuck = conn.execute(
            "UPDATE meetings SET summary_status = CASE
                 WHEN COALESCE(summary, '') != '' OR EXISTS (SELECT 1 FROM meeting_actions a WHERE a.meeting_id = meetings.id) THEN 'done'
                 ELSE 'failed' END
             WHERE summary_status = 'pending'",
            [],
        )?;
        Ok(n + stuck)
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
        Segment { meeting_id: id, speaker: who.into(), start_ms: a, end_ms: b, text: t.into(), ..Default::default() }
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
            action_items: vec![
                ParsedAction { text: "Enviar cifras".into(), owner: Some("Ana".into()), due: Some("el lunes".into()) },
                ParsedAction { text: "Revisar contrato".into(), owner: None, due: None },
            ],
            decisions: vec!["Cerrar el viernes".into()],
            topics: vec!["presupuesto".into()],
            key_points: vec!["El proveedor sube un 5 %".into()],
            next_steps: vec!["Revisarlo el martes".into()],
            open_questions: vec!["¿Quién firma?".into()],
            said_me: vec!["Pidió cerrar el presupuesto".into()],
            said_them: vec!["El proveedor sube un 5 %".into()],
            questions: vec![crate::models::Question { text: "¿Cerramos el viernes?".into(), asked_by: "me".into(), at_ms: 1000, answered: true, answer: Some("Sí.".into()) }],
            qa: None,
            ..Default::default()
        };
        db.save_summary(m.id, &summary).unwrap();
        let d = db.get_meeting_detail(m.id).unwrap().unwrap();
        assert_eq!(d.meeting.title, "Cierre de presupuesto"); // título automático sustituido
        assert_eq!(d.meeting.summary_status, "done");
        assert_eq!(d.action_items.len(), 2);
        assert_eq!(d.action_items[0].owner.as_deref(), Some("Ana"));
        assert_eq!((d.action_items[0].due.as_deref(), d.action_items[1].due.as_deref()), (Some("el lunes"), None));
        assert_eq!(d.decisions, ["Cerrar el viernes"]);
        assert_eq!((d.key_points.len(), d.next_steps.len(), d.open_questions.len()), (1, 1, 1));
        assert_eq!((d.said_by_me.len(), d.said_by_them.len()), (1, 1));
        assert_eq!(d.questions, summary.questions);
        assert!(!d.transcript_final);
        assert_eq!(db.list_meetings(Some("proveedor"), 10, 0).unwrap().len(), 1);

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

    /// Rompedor 22-oct-c, BAJAS 5-7: «Regenerar» conserva las tareas hechas que siguen, el título
    /// del modelo queda en una línea sin Markdown y se buscan las palabras cortas.
    #[test]
    fn regenerar_titulo_y_palabras_cortas() {
        let db = Db::open_memory().unwrap();
        let m = db.create_meeting("", None).unwrap();
        let task = |t: &str| ParsedAction { text: t.into(), owner: None, due: None };
        let s = ParsedSummary { summary: "Daily.".into(), action_items: vec![task("Enviar el contrato a Ana."), task("Llamar al proveedor")], ..Default::default() };
        db.save_summary(m.id, &s).unwrap();
        db.toggle_action_item(m.id, 0).unwrap();
        let s2 = ParsedSummary {
            title: Some("Revisión\n\n## Tareas\n- [x] Pagar <b>ya</b>](x)".into()),
            summary: "Daily.".into(),
            action_items: vec![task("Revisar el presupuesto"), task("enviar el contrato a Ana")],
            ..Default::default()
        };
        db.save_summary(m.id, &s2).unwrap();
        let d = db.get_meeting_detail(m.id).unwrap().unwrap();
        assert_eq!(d.action_items.iter().map(|a| (a.text.as_str(), a.done)).collect::<Vec<_>>(), [("Revisar el presupuesto", false), ("enviar el contrato a Ana", true)]);
        assert_eq!(d.meeting.title, "Revisión Tareas - x Pagar b ya /b (x)");
        db.replace_segments(m.id, &[seg(m.id, "them", 0, 900, "el plan de IA para el Q3")], true).unwrap();
        for (q, n) in [("IA", 1), ("ia", 1), ("Q3", 1), ("plan IA", 1), ("plan UX", 0), ("a", 1)] {
            assert_eq!(db.list_meetings(Some(q), 10, 0).unwrap().len(), n, "{q}");
        }
        assert_eq!(search_words("¿Qué plan de IA hay para el Q3?"), ["qué", "plan", "ia", "hay", "para", "q3"]);
    }

    #[test]
    fn la_transcripcion_final_sustituye_a_la_vista_previa() {
        let db = Db::open_memory().unwrap();
        let m = db.create_meeting("", None).unwrap();
        db.add_segment(&seg(m.id, "me", 0, 1000, "vista previa con basura")).unwrap();
        db.finish_meeting(m.id, 1000).unwrap();
        assert!(!db.transcript_final(m.id).unwrap());
        let fin = [seg(m.id, "them", 500, 4000, "hablamos del presupuesto anual"), seg(m.id, "me", 4200, 6000, "de acuerdo")];
        assert!(db.replace_segments(m.id, &fin, true).unwrap());
        assert!(db.transcript_final(m.id).unwrap());
        assert_eq!(db.segments(m.id).unwrap(), fin);
        // Turnos de Meets: original del pulido, etiqueta, solape y respuesta se guardan.
        let mut meets = fin.clone();
        meets[0].kind = Some("pregunta".into());
        meets[1] = Segment { raw_text: Some("de acuerdo de acuerdo".into()), kind: Some("respuesta".into()), overlap: true, reply_to: Some(0), ..meets[1].clone() };
        assert!(db.replace_segments(m.id, &meets, true).unwrap());
        assert_eq!(db.segments(m.id).unwrap(), meets);
        assert!(!db.polish_done(m.id).unwrap());
        db.set_polish_done(m.id).unwrap();
        assert!(db.polish_done(m.id).unwrap());
        // El índice de búsqueda sigue a la transcripción nueva.
        assert!(db.list_meetings(Some("basura"), 10, 0).unwrap().is_empty());
        assert_eq!(db.list_meetings(Some("anual"), 10, 0).unwrap().len(), 1);
        db.extend_duration(m.id, 6000).unwrap();
        db.extend_duration(m.id, 2000).unwrap();
        assert_eq!(db.get_meeting(m.id).unwrap().unwrap().duration_ms, 6000);
        // Una reunión borrada mientras se procesaba no resucita.
        db.delete_meeting(m.id).unwrap();
        assert!(!db.replace_segments(m.id, &fin, true).unwrap());
        assert!(db.segments(m.id).unwrap().is_empty());
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

    #[test]
    fn una_transcripcion_parcial_no_queda_como_final() {
        let db = Db::open_memory().unwrap();
        let m = db.create_meeting("Parcial", None).unwrap();
        assert!(db.replace_segments(m.id, &[seg(m.id, "them", 0, 900, "solo salió una pista")], false).unwrap());
        assert!(!db.transcript_final(m.id).unwrap(), "con una pista o trozos de menos se reintenta desde el audio");
        assert_eq!(db.segments(m.id).unwrap().len(), 1, "lo que salió se guarda");
        assert!(db.replace_segments(m.id, &[seg(m.id, "them", 0, 900, "ahora sí, entera")], true).unwrap());
        assert!(db.transcript_final(m.id).unwrap());
    }

    #[test]
    fn reuniones_a_medio_procesar_quedan_para_reintentar() {
        // La app se cerró en plena fase 2: al arrancar no hay ningún hilo procesándola.
        let db = Db::open_memory().unwrap();
        let m = db.create_meeting("A medias", None).unwrap();
        db.finish_meeting(m.id, 60_000).unwrap();
        db.set_summary_status(m.id, "pending").unwrap();
        let hecha = db.create_meeting("Hecha", None).unwrap();
        db.finish_meeting(hecha.id, 60_000).unwrap();
        db.set_summary_status(hecha.id, "done").unwrap();
        assert_eq!(db.close_orphan_meetings().unwrap(), 1);
        assert_eq!(db.get_meeting(m.id).unwrap().unwrap().summary_status, "failed", "sin hilo vivo, `pending` no se queda para siempre");
        assert_eq!(db.get_meeting(hecha.id).unwrap().unwrap().summary_status, "done");
        assert_eq!(db.close_orphan_meetings().unwrap(), 0);
    }

    /// Rompedor 23-oct, reuniones MEDIA 5: un intento fallido no esconde las notas buenas.
    #[test]
    fn un_regenerar_fallido_conserva_las_notas() {
        let db = Db::open_memory().unwrap();
        let sin = db.create_meeting("Sin notas", None).unwrap();
        db.finish_meeting(sin.id, 60_000).unwrap();
        assert!(!db.fail_summary_status(sin.id).unwrap());
        assert_eq!(db.get_meeting(sin.id).unwrap().unwrap().summary_status, "failed");
        let con = db.create_meeting("Con notas", None).unwrap();
        db.finish_meeting(con.id, 60_000).unwrap();
        db.save_summary(con.id, &ParsedSummary { summary: "Se aprobó.".into(), ..Default::default() }).unwrap();
        db.set_summary_status(con.id, "pending").unwrap();
        assert!(db.fail_summary_status(con.id).unwrap());
        assert_eq!(db.get_meeting(con.id).unwrap().unwrap().summary_status, "done");
        // Y al arrancar, una `pending` con notas tampoco pasa a `failed`.
        db.set_summary_status(con.id, "pending").unwrap();
        db.set_summary_status(sin.id, "pending").unwrap();
        db.close_orphan_meetings().unwrap();
        assert_eq!(db.get_meeting(con.id).unwrap().unwrap().summary_status, "done");
        assert_eq!(db.get_meeting(sin.id).unwrap().unwrap().summary_status, "failed");
    }

    /// Rompedor 23-oct, reuniones BAJA 6: la tarea hecha que el modelo redacta distinto sigue hecha,
    /// cada vieja se empareja una vez y otro responsable es otra tarea.
    #[test]
    fn regenerar_conserva_hechas_las_tareas_parafraseadas() {
        let t = |x: &str, o: &str| ParsedAction { text: x.into(), owner: Some(o.into()), due: None };
        let old = vec![("Enviar el contrato a Ana".to_string(), Some("Txonso".to_string())), ("Revisar el presupuesto del trimestre".to_string(), Some("Marta".to_string()))];
        let new = [t("Enviar contrato a Ana", "Txonso"), t("Enviar contrato a Ana ya", "Txonso"), t("Revisar presupuesto del trimestre", "Pedro")];
        assert_eq!(carry_done(&old, &new), [true, false, false]);
    }
}
