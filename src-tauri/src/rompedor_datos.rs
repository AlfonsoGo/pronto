//! Sondas del rompedor (2026-10-22, tanda paralela): datos (identidad de base, -wal, copias). Solo se compilan en pruebas.
//! Las que demostraban una rotura llevaban #[ignore = "rompedor: <título>"]; arregladas el 23-oct
//! (rompedor 22-oct-c, datos), ya corren siempre como regresión. Solo las medidas siguen ignoradas.
//!
//! `cargo test rompedor_datos_ -- --test-threads=1 --nocapture` (y `--include-ignored` para las rotas).
//! Medidas: `rompedor_datos_rendimiento_disparadores` (PRONTO_ROMPEDOR_N, por defecto 100000) y
//! `rompedor_datos_wal_enorme` (PRONTO_ROMPEDOR_WAL_MB, por defecto 256) van con `--ignored`, solas.
//! Base real: `PRONTO_ROMPEDOR_DATOS_REAL=<carpeta con una COPIA de pronto2.db y su -wal>` (nunca `%APPDATA%`).
#![allow(dead_code, unused_imports)]

use crate::db::{Db, NewHistory};
use crate::models::{Fixes, Segment};
use rusqlite::{params, Connection, OpenFlags};
use std::path::{Path, PathBuf};
use std::time::Instant;

// ------------------------------------------------------------------ utilidades

const NO_CKPT: rusqlite::config::DbConfig = rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE;

fn tmp(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("pronto2-rompedor-datos-{name}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn side(p: &Path, s: &str) -> PathBuf {
    PathBuf::from(format!("{}{s}", p.display()))
}

fn flen(p: &Path) -> u64 {
    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

fn nuevo(text: &str) -> NewHistory<'_> {
    NewHistory { polished: false, text, raw: text, app: "Code", duration_ms: 1000, words: 2, fixes: Fixes::default(), transform: None }
}

fn fill(db: &Db, n: usize, tag: &str) {
    for i in 0..n {
        db.add_history(&nuevo(&format!("{tag} dictado {i}: camión, pingüino ✓"))).unwrap();
    }
}

/// Pronto muere sin volcar el -wal (apagón, taskkill, `app.exit` sin cerrar la conexión).
fn matar(db: Db) {
    db.conn.lock().set_db_config(NO_CKPT, true).unwrap();
    drop(db);
}

fn matar_conn(c: Connection) {
    c.set_db_config(NO_CKPT, true).unwrap();
    drop(c);
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
    stmt.query_map([], |r| Ok((0..n).map(|i| celda(r.get_ref(i).unwrap())).collect::<Vec<_>>().join("|"))).unwrap().map(|r| r.unwrap()).collect()
}

/// Filas del usuario (sin `meta`, internas ni FTS).
fn datos(conn: &Connection) -> Vec<String> {
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '%fts%' AND name <> 'meta' ORDER BY name")
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

fn datos_db(db: &Db) -> Vec<String> {
    datos(&db.conn.lock())
}

fn n_hist(conn: &Connection) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM history", [], |r| r.get(0)).unwrap_or(-1)
}

/// Lectura cruda (con su -wal) sin volcar al cerrar.
fn ver(p: &Path) -> Result<(i64, Vec<String>), String> {
    let c = Connection::open(p).map_err(|e| e.to_string())?;
    c.set_db_config(NO_CKPT, true).map_err(|e| e.to_string())?;
    let n = c.query_row("SELECT COUNT(*) FROM history", [], |r| r.get(0)).map_err(|e| e.to_string())?;
    let d = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| datos(&c))).map_err(|_| "datos() falló".to_string())?;
    Ok((n, d))
}

fn quick(conn: &Connection) -> String {
    conn.query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0)).unwrap_or_else(|e| format!("error: {e}"))
}

fn apartados(dir: &Path) -> usize {
    std::fs::read_dir(dir).map(|d| d.flatten().filter(|e| e.file_name().to_string_lossy().starts_with("apartado-")).count()).unwrap_or(0)
}

fn base_id(conn: &Connection) -> Option<String> {
    conn.query_row("SELECT value FROM meta WHERE key = 'base_id'", [], |r| r.get(0)).ok()
}

fn base_id_principal(p: &Path) -> Option<String> {
    meta_principal(p, "base_id")
}

/// Una clave de `meta` del fichero principal leído solo (sin su -wal).
fn meta_principal(p: &Path, key: &str) -> Option<String> {
    let uri = format!("file:///{}?immutable=1", p.to_string_lossy().replace('\\', "/").replace('%', "%25").replace(' ', "%20").replace('#', "%23"));
    let c = Connection::open_with_flags(uri, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI).ok()?;
    c.query_row("SELECT value FROM meta WHERE key = ?1", params![key], |r| r.get(0)).ok()
}

fn contiene(hay: &[u8], aguja: &[u8]) -> usize {
    hay.windows(aguja.len()).filter(|w| *w == aguja).count()
}

fn primera_diferencia(got: &[String], want: &[String]) -> String {
    let i = got.iter().zip(want).position(|(a, b)| a != b).unwrap_or(got.len().min(want.len()));
    let corta = |s: Option<&String>| s.map(|s| s.chars().take(100).collect::<String>());
    format!("{} filas frente a {}; 1.ª diferencia en {i}: {:?} ≠ {:?}", got.len(), want.len(), corta(got.get(i)), corta(want.get(i)))
}

/// Reabre como Pronto y comprueba que el -wal propio se respetó: sin aviso, sin apartado, mismas filas.
fn comprobar(dir: &Path, p: &Path, vivo: &[String]) -> Result<String, String> {
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Db::open_at(p)));
    let db = match r {
        Err(_) => return Err("open_at hizo panic".into()),
        Ok(Err(e)) => return Err(format!("open_at=Err({e:#})")),
        Ok(Ok(db)) => db,
    };
    let aviso = db.aviso().map(|a| a.chars().take(140).collect::<String>());
    let d = datos_db(&db);
    drop(db);
    let ap = apartados(dir);
    if aviso.is_some() || ap > 0 {
        return Err(format!("apartó el -wal propio: aviso={aviso:?} apartados={ap}; {}", primera_diferencia(&d, vivo)));
    }
    if d != vivo {
        return Err(format!("datos distintos: {}", primera_diferencia(&d, vivo)));
    }
    Ok(format!("ok ({} filas)", d.len()))
}

// ------------------------------------------------------------------ WAL a mano

fn suma(data: &[u8], (mut s0, mut s1): (u32, u32), big: bool) -> (u32, u32) {
    for c in data.chunks_exact(8) {
        let w = |x: &[u8]| {
            let a = [x[0], x[1], x[2], x[3]];
            if big { u32::from_be_bytes(a) } else { u32::from_le_bytes(a) }
        };
        s0 = s0.wrapping_add(w(&c[0..4])).wrapping_add(s1);
        s1 = s1.wrapping_add(w(&c[4..8])).wrapping_add(s0);
    }
    (s0, s1)
}

fn be32(b: &[u8], o: usize) -> u32 {
    u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// Recalcula la suma de la cabecera y de cada marco con las sales vigentes (un -wal «manipulado bien»).
fn resumar(b: &mut [u8]) {
    let big = be32(b, 0) & 1 == 1;
    let page = be32(b, 8) as usize;
    let mut s = suma(&b[0..24], (0, 0), big);
    b[24..28].copy_from_slice(&s.0.to_be_bytes());
    b[28..32].copy_from_slice(&s.1.to_be_bytes());
    let salts: Vec<u8> = b[16..24].to_vec();
    let mut off = 32;
    while off + 24 + page <= b.len() {
        if b[off + 8..off + 16] != salts[..] {
            break;
        }
        s = suma(&b[off..off + 8], s, big);
        s = suma(&b[off + 24..off + 24 + page], s, big);
        b[off + 16..off + 20].copy_from_slice(&s.0.to_be_bytes());
        b[off + 20..off + 24].copy_from_slice(&s.1.to_be_bytes());
        off += 24 + page;
    }
}

/// -wal construido: (magic, versión, página) y marcos (pgno, commit, página).
fn wal_hecho(magic: u32, version: u32, page: u32, frames: &[(u32, u32, Vec<u8>)]) -> Vec<u8> {
    let mut b = Vec::new();
    for v in [magic, version, page, 0, 0x1234_5678, 0x9abc_def0] {
        b.extend_from_slice(&v.to_be_bytes());
    }
    b.extend_from_slice(&[0u8; 8]);
    for (pgno, commit, data) in frames {
        b.extend_from_slice(&pgno.to_be_bytes());
        b.extend_from_slice(&commit.to_be_bytes());
        b.extend_from_slice(&[0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0]);
        b.extend_from_slice(&[0u8; 8]);
        b.extend_from_slice(data);
    }
    resumar(&mut b);
    b
}

// ------------------------------------------------------------------ 1. -wal PROPIO en todas sus formas

#[test]
fn rompedor_datos_wal_propio_no_se_aparta() {
    let raiz = tmp("propio");
    let mut fallos = Vec::new();
    let mut informe = |caso: &str, r: Result<String, String>| {
        println!("[propio] {caso}: {r:?}");
        if let Err(e) = r {
            fallos.push(format!("{caso}: {e}"));
        }
    };

    // a) varias transacciones + volcado PASSIVE parcial (un lector con instantánea vieja)
    {
        let dir = raiz.join("a");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("pronto2.db");
        let db = Db::open_at(&p).unwrap();
        fill(&db, 30, "a1");
        db.checkpoint();
        let lector = Connection::open(&p).unwrap();
        lector.execute_batch("BEGIN").unwrap();
        let _ = n_hist(&lector);
        fill(&db, 30, "a2");
        db.checkpoint(); // solo hasta la instantánea del lector
        fill(&db, 10, "a3");
        let vivo = datos_db(&db);
        matar_conn(lector);
        matar(db);
        informe("varias_tx_volcado_parcial", comprobar(&dir, &p, &vivo));
    }
    // b) RESTART completo y una sola escritura después: sales nuevas, marcos viejos detrás
    {
        let dir = raiz.join("b");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("pronto2.db");
        let db = Db::open_at(&p).unwrap();
        fill(&db, 60, "b1");
        let r: (i64, i64, i64) = db.conn.lock().query_row("PRAGMA wal_checkpoint(RESTART)", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
        db.add_history(&nuevo("b2 tras RESTART")).unwrap();
        let vivo = datos_db(&db);
        matar(db);
        println!("[propio] restart={r:?} wal={} B", flen(&side(&p, "-wal")));
        informe("restart_sales_nuevas", comprobar(&dir, &p, &vivo));
    }
    // c) RESTART a medias (un lector lo bloquea) y escrituras después
    {
        let dir = raiz.join("c");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("pronto2.db");
        let db = Db::open_at(&p).unwrap();
        fill(&db, 40, "c1");
        let lector = Connection::open(&p).unwrap();
        lector.execute_batch("BEGIN").unwrap();
        let _ = n_hist(&lector);
        fill(&db, 5, "c2");
        let r: (i64, i64, i64) = {
            let c = db.conn.lock();
            c.busy_timeout(std::time::Duration::ZERO).unwrap();
            c.query_row("PRAGMA wal_checkpoint(RESTART)", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap()
        };
        fill(&db, 5, "c3");
        let vivo = datos_db(&db);
        matar_conn(lector);
        matar(db);
        println!("[propio] restart_a_medias={r:?}");
        informe("restart_a_medias", comprobar(&dir, &p, &vivo));
    }
    // d) VACUUM en modo WAL (todas las páginas en el -wal, sin disparadores)
    {
        let dir = raiz.join("d");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("pronto2.db");
        let db = Db::open_at(&p).unwrap();
        fill(&db, 40, "d1");
        db.conn.lock().execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
        for h in db.list_history(None, 100, 0).unwrap().iter().filter(|h| h.id % 2 == 0) {
            db.delete_history(h.id).unwrap();
        }
        db.conn.lock().execute_batch("VACUUM").unwrap();
        let vivo = datos_db(&db);
        matar(db);
        informe("vacuum", comprobar(&dir, &p, &vivo));
    }
    // e) apagón: marcos de una transacción sin confirmar (derrame de caché) + medio marco al final
    {
        let dir = raiz.join("e");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("pronto2.db");
        let db = Db::open_at(&p).unwrap();
        fill(&db, 20, "e1");
        db.conn.lock().execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
        fill(&db, 10, "e2");
        let vivo = datos_db(&db);
        let confirmado = flen(&side(&p, "-wal"));
        let foto = {
            let c = db.conn.lock();
            c.execute_batch("PRAGMA cache_size = 1; BEGIN;").unwrap();
            for i in 0..300 {
                c.execute("INSERT INTO history (created_at, text, raw_text) VALUES ('2026-10-22T10:00:00+02:00', ?1, '')", params![format!("sin confirmar {i} {}", "z".repeat(2000))]).unwrap();
            }
            let foto = std::fs::read(side(&p, "-wal")).unwrap();
            c.execute_batch("ROLLBACK; PRAGMA cache_size = -2000;").unwrap();
            foto
        };
        matar(db);
        let mut w = foto.clone();
        w.extend_from_slice(&foto[32..32 + 24 + 2000]); // medio marco
        std::fs::write(side(&p, "-wal"), &w).unwrap();
        println!("[propio] apagón: confirmado={confirmado} B, con marcos sin confirmar={} B", w.len());
        informe("apagon_marcos_sin_confirmar", comprobar(&dir, &p, &vivo));
    }
    // f) base nueva que nunca se volcó, abierta tres veces
    {
        let dir = raiz.join("f");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("pronto2.db");
        let db = Db::open_at(&p).unwrap();
        fill(&db, 5, "f1");
        let vivo = datos_db(&db);
        matar(db);
        println!("[propio] base nueva sin volcar: principal={} B wal={} B id_principal={:?}", flen(&p), flen(&side(&p, "-wal")), base_id_principal(&p));
        let r1 = comprobar(&dir, &p, &vivo);
        let db = Db::open_at(&p).unwrap();
        fill(&db, 5, "f2");
        let vivo = datos_db(&db);
        matar(db);
        let r2 = comprobar(&dir, &p, &vivo);
        informe("base_nueva_sin_volcar", r1.and(r2));
    }
    // g) -wal con solo escrituras en la tabla FTS (sin disparador: sin página de meta)
    {
        let dir = raiz.join("g");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("pronto2.db");
        let db = Db::open_at(&p).unwrap();
        fill(&db, 10, "g1");
        let m = db.create_meeting("Reunión g", Some("Teams")).unwrap();
        db.conn.lock().execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
        db.conn.lock().execute("INSERT INTO meeting_fts (content, meeting_id, kind) VALUES ('solo fts presupuesto', ?1, 'seg')", params![m.id]).unwrap();
        let vivo = datos_db(&db);
        matar(db);
        let w = std::fs::read(side(&p, "-wal")).unwrap();
        println!("[propio] solo_fts: wal={} B, 'base_id' en el -wal: {}", w.len(), contiene(&w, b"base_id"));
        informe("solo_fts", comprobar(&dir, &p, &vivo));
    }
    // h) base v8 de antes: migración a v9 + identidad en el -wal, volcado TRUNCATE bloqueado por un lector
    {
        let dir = raiz.join("h");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("pronto2.db");
        {
            let db = Db::open_at(&p).unwrap();
            fill(&db, 20, "h1");
        }
        let c = Connection::open(&p).unwrap();
        c.execute_batch(
            "DROP TABLE meta;
             ALTER TABLE meeting_segments DROP COLUMN raw_text; ALTER TABLE meeting_segments DROP COLUMN kind;
             ALTER TABLE meeting_segments DROP COLUMN overlap; ALTER TABLE meeting_segments DROP COLUMN reply_to;
             ALTER TABLE meetings DROP COLUMN questions; ALTER TABLE meetings DROP COLUMN said_me;
             ALTER TABLE meetings DROP COLUMN said_them; ALTER TABLE meetings DROP COLUMN polish_done;
             PRAGMA user_version = 8; PRAGMA wal_checkpoint(TRUNCATE);",
        )
        .unwrap();
        for i in 0..10 {
            c.execute("INSERT INTO history (created_at, text, raw_text) VALUES ('2026-10-01T10:00:00+02:00', ?1, '')", params![format!("h2 v8 sin volcar {i}")]).unwrap();
        }
        c.execute_batch("BEGIN").unwrap();
        let _ = n_hist(&c);
        let db = Db::open_at(&p).unwrap();
        let aviso1 = db.aviso().map(String::from);
        fill(&db, 5, "h3");
        let vivo = datos_db(&db);
        let v: i64 = db.conn.lock().query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        matar_conn(c);
        matar(db);
        println!("[propio] v8: aviso1={aviso1:?} versión={v} id_principal={:?} wal={} B", base_id_principal(&p), flen(&side(&p, "-wal")));
        let r = if aviso1.is_some() { Err(format!("primera apertura apartó: {aviso1:?}")) } else { comprobar(&dir, &p, &vivo) };
        informe("migracion_v8_volcado_bloqueado", r);
    }
    // i) ruta con emoji, `&`, `;`, `=` y ñ: -wal propio se respeta y uno ajeno se aparta
    {
        let dir = raiz.join("📁 Pronto & co; ñ=1 %41");
        std::fs::create_dir_all(dir.join("otra")).unwrap();
        let p = dir.join("pronto2.db");
        let db = Db::open_at(&p).unwrap();
        fill(&db, 10, "i1");
        db.conn.lock().execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
        fill(&db, 5, "i2");
        let vivo = datos_db(&db);
        matar(db);
        let r1 = comprobar(&dir, &p, &vivo);
        let o = dir.join("otra").join("pronto2.db");
        let otra = Db::open_at(&o).unwrap();
        otra.add_history(&nuevo("ajena")).unwrap();
        matar(otra);
        std::fs::copy(side(&o, "-wal"), side(&p, "-wal")).unwrap();
        let db = Db::open_at(&p).unwrap();
        let r2 = if db.aviso().is_some() && datos_db(&db) == vivo { Ok("ajeno apartado".to_string()) } else { Err(format!("ruta rara: -wal ajeno no apartado (aviso={:?})", db.aviso())) };
        drop(db);
        informe("ruta_emoji", r1.and(r2));
    }
    let _ = std::fs::remove_dir_all(&raiz);
    assert!(fallos.is_empty(), "-wal propio mal clasificado:\n{}", fallos.join("\n"));
}

// ------------------------------------------------------------------ 2. texto del usuario con «base_id»+UUID

#[test]
fn rompedor_datos_nota_con_base_id_aparta_su_wal() {
    let raiz = tmp("nota-base-id");
    let mut fallos = Vec::new();
    for (caso, relleno) in [("nota_corta", 0usize), ("nota_larga_desbordada", 1200)] {
        let dir = raiz.join(caso);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("pronto2.db");
        let db = Db::open_at(&p).unwrap();
        fill(&db, 20, caso);
        db.conn.lock().execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
        let id = base_id(&db.conn.lock()).unwrap();
        let nota = format!("{}apunte de depuración base_id0f0e0d0c-0b0a-4908-8706-050403020100 fin", "relleno ".repeat(relleno));
        db.add_history(&nuevo("dictado justo antes del apagón")).unwrap();
        db.save_note(None, &nota).unwrap(); // lo último antes del apagón
        let vivo = datos_db(&db);
        matar(db);
        let db = Db::open_at(&p).unwrap();
        let aviso = db.aviso().map(|a| a.chars().take(160).collect::<String>());
        let d = datos_db(&db);
        drop(db);
        println!("[nota] {caso}: base_id real={id} aviso={aviso:?} apartados={} nota_visible={} dictado_visible={}", apartados(&dir), d.iter().any(|r| r.contains("apunte de depuración")), d.iter().any(|r| r.contains("antes del apagón")));
        if aviso.is_some() || d != vivo {
            fallos.push(format!("{caso}: el -wal propio se apartó ({aviso:?}); {}", primera_diferencia(&d, &vivo)));
        }
    }
    let _ = std::fs::remove_dir_all(&raiz);
    assert!(fallos.is_empty(), "{}", fallos.join("\n"));
}

// ------------------------------------------------------------------ 3. copia restaurada (mismo base_id) con el -wal de hoy al lado

#[test]
fn rompedor_datos_copia_restaurada_con_wal_de_la_misma_base() {
    let raiz = tmp("restaurar");
    let mut fallos = Vec::new();
    for (caso, huecos, pocas) in [("con_paginas_libres", true, false), ("sin_paginas_libres", false, false), ("solo_un_cambio_sin_crecer", false, true)] {
        let dir = raiz.join(caso);
        let copias = dir.join("copias");
        std::fs::create_dir_all(&copias).unwrap();
        let p = dir.join("pronto2.db");
        let db = Db::open_at(&p).unwrap();
        for i in 0..400 {
            db.add_history(&nuevo(&format!("viejo {i} {}", "x".repeat(300)))).unwrap();
        }
        if huecos {
            for h in db.list_history(None, 1000, 0).unwrap().iter().filter(|h| h.id % 2 == 0 && h.id < 200) {
                db.delete_history(h.id).unwrap();
            }
        }
        db.conn.lock().execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
        let copia = db.daily_backup(&copias, 7).unwrap().expect("copia del día");
        if pocas {
            // Un solo arreglo de un dictado (misma longitud): sin páginas nuevas ni página 1 en el -wal.
            let h = db.get_history(150).unwrap().unwrap();
            let t = h.text.replacen("viejo", "VIEJO", 1);
            db.update_history_text(150, &t, &t, 2).unwrap();
        }
        for i in 0..(if pocas { 0 } else { 50 }) {
            db.add_history(&nuevo(&format!("de hoy {i} {}", "y".repeat(300)))).unwrap();
        }
        for h in db.list_history(None, 1000, 0).unwrap().iter().filter(|h| !pocas && (201..=260).contains(&h.id)) {
            db.delete_history(h.id).unwrap();
        }
        let vivo = datos_db(&db);
        let wal_hoy = std::fs::read(side(&p, "-wal")).unwrap();
        println!("[restaurar] {caso}: -wal de hoy {} B, marcos≈{}", wal_hoy.len(), wal_hoy.len().saturating_sub(32) / (4096 + 24));
        matar(db); // «Salir» de Pronto no cierra la conexión: el -wal se queda
        let de_la_copia = ver(&copia).unwrap().1;
        // Rompedor 22-oct-c, datos MEDIA 2: la copia tiene identidad propia y dice de qué base es.
        assert_ne!(base_id_principal(&copia), base_id_principal(&p), "la copia lleva su propio base_id");
        assert_eq!(meta_principal(&copia, "copia_de"), base_id_principal(&p), "la copia dice de qué base es");
        std::fs::copy(&copia, &p).unwrap(); // el usuario «restaura» la copia, como dice el aviso
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Db::open_at(&p)));
        let desc = match r {
            Err(_) => {
                fallos.push(format!("{caso}: open_at hizo panic"));
                "panic".to_string()
            }
            Ok(Err(e)) => {
                fallos.push(format!("{caso}: Pronto no arranca: {e:#}"));
                format!("Err({e:#})")
            }
            Ok(Ok(db)) => {
                let q = quick(&db.conn.lock());
                let d = datos_db(&db);
                let aviso = db.aviso().map(|a| a.chars().take(120).collect::<String>());
                drop(db);
                let tras = ver(&p).map(|(n, _)| n);
                let que = if d == de_la_copia {
                    "la copia".to_string()
                } else if d == vivo {
                    "lo de hoy (la restauración no hizo nada)".to_string()
                } else {
                    format!("NI la copia NI lo de hoy: vs copia {}; vs hoy {}", primera_diferencia(&d, &de_la_copia), primera_diferencia(&d, &vivo))
                };
                if d != de_la_copia {
                    fallos.push(format!("{caso}: tras restaurar se ve {que}; aviso={aviso:?} quick_check={q}"));
                }
                format!("aviso={aviso:?} quick_check={q} filas={} ve={que} history tras cerrar={tras:?} apartados={}", d.len(), apartados(&dir))
            }
        };
        println!("[restaurar] {caso}: copia={} filas, hoy={} filas → {desc}", de_la_copia.len(), vivo.len());
    }
    let _ = std::fs::remove_dir_all(&raiz);
    assert!(fallos.is_empty(), "{}", fallos.join("\n"));
}

// ------------------------------------------------------------------ 4. -wal ajeno SIN identidad (escrito por Pronto ≤ 2.0.2)

/// Base «de antes» (v9 sin `meta`) vacía: todas las que salgan de aquí comparten disposición de páginas.
fn plantilla_antigua(p: &Path) {
    drop(Db::open_at(p).unwrap());
    let c = Connection::open(p).unwrap();
    c.execute_batch("DROP TABLE meta; PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
    drop(c);
    assert!(!side(p, "-wal").exists());
}

fn insertar_crudo(p: &Path, n: usize, tag: &str, matar_al_final: bool) {
    let c = Connection::open(p).unwrap();
    for i in 0..n {
        c.execute("INSERT INTO history (created_at, text, raw_text, app) VALUES ('2026-10-13T21:49:00+02:00', ?1, '', 'Code')", params![format!("{tag} {i}")]).unwrap();
    }
    if matar_al_final {
        matar_conn(c);
    }
}

#[test]
fn rompedor_datos_wal_ajeno_sin_identidad() {
    let raiz = tmp("ajeno-sin-id");
    let plantilla = raiz.join("plantilla.db");
    plantilla_antigua(&plantilla);
    // El -wal intruso: una base de 2.0.2 recién creada con 4 dictados (el 13-oct), sin volcar.
    let intrusa_dir = raiz.join("intrusa");
    std::fs::create_dir_all(&intrusa_dir).unwrap();
    let intrusa = intrusa_dir.join("pronto2.db");
    std::fs::copy(&plantilla, &intrusa).unwrap();
    insertar_crudo(&intrusa, 4, "intrusa", true);
    let wal_intruso = std::fs::read(side(&intrusa, "-wal")).unwrap();
    println!("[ajeno-sin-id] -wal intruso: {} B, 'base_id' dentro: {}", wal_intruso.len(), contiene(&wal_intruso, b"base_id"));

    let mut fallos = Vec::new();
    for caso in ["buena_ya_identificada", "buena_de_antes_primer_arranque_2_0_3"] {
        let dir = raiz.join(caso);
        std::fs::create_dir_all(&dir).unwrap();
        let g = dir.join("pronto2.db");
        std::fs::copy(&plantilla, &g).unwrap();
        insertar_crudo(&g, 20, "buena", false); // cierre limpio: todo volcado
        if caso == "buena_ya_identificada" {
            drop(Db::open_at(&g).unwrap());
        }
        let id = base_id_principal(&g);
        let antes = ver(&g).unwrap();
        std::fs::write(side(&g, "-wal"), &wal_intruso).unwrap();
        let db = Db::open_at(&g).unwrap();
        let aviso = db.aviso().map(String::from);
        let visto = n_hist(&db.conn.lock());
        let q = quick(&db.conn.lock());
        drop(db); // cierre limpio: vuelca
        let despues = ver(&g).unwrap();
        println!(
            "[ajeno-sin-id] {caso}: id_principal={id:?} buenas={} visto_al_abrir={visto} quick_check={q} aviso={} tras_cerrar={} apartados={}",
            antes.0,
            aviso.is_some(),
            despues.0,
            apartados(&dir)
        );
        if despues.1 != antes.1 {
            fallos.push(format!("{caso}: la base buena perdió datos: {}", primera_diferencia(&despues.1, &antes.1)));
        }
    }
    let _ = std::fs::remove_dir_all(&raiz);
    assert!(fallos.is_empty(), "{}", fallos.join("\n"));
}

// ------------------------------------------------------------------ 5. wal_identity con -wal manipulados

#[test]
fn rompedor_datos_wal_manipulado() {
    let raiz = tmp("manipulado");
    let molde = raiz.join("molde.db");
    {
        let db = Db::open_at(&molde).unwrap();
        fill(&db, 20, "molde");
    }
    assert!(!side(&molde, "-wal").exists());
    let antes = ver(&molde).unwrap().1;
    let id = base_id_principal(&molde).unwrap();
    let bytes = std::fs::read(&molde).unwrap();
    let pagina2 = bytes[4096..8192].to_vec(); // aplicar esta página no cambia nada
    let npag = (bytes.len() / 4096) as u32;
    // -wal de otra base (con identidad) con 4 dictados, de la misma disposición.
    let otra = raiz.join("otra").join("pronto2.db");
    std::fs::create_dir_all(otra.parent().unwrap()).unwrap();
    {
        let o = Db::open_at(&otra).unwrap();
        fill(&o, 20, "otra");
        o.conn.lock().execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
        for i in 0..4 {
            o.add_history(&nuevo(&format!("intrusa {i}"))).unwrap();
        }
        matar(o);
    }
    let wal_otra = std::fs::read(side(&otra, "-wal")).unwrap();
    let id_otra = base_id_principal(&otra).unwrap();
    let cambiar_id = |w: &[u8]| -> Vec<u8> {
        let mut w = w.to_vec();
        let (a, b) = (id_otra.as_bytes(), id.as_bytes());
        let mut i = 0;
        while i + a.len() <= w.len() {
            if &w[i..i + a.len()] == a {
                w[i..i + a.len()].copy_from_slice(b);
            }
            i += 1;
        }
        w
    };
    let mut aleatorio = vec![0u8; 5000];
    let mut x: u32 = 0x2545_f491;
    for b in aleatorio.iter_mut() {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        *b = x as u8;
    }
    let valida = |v: u32| wal_hecho(0x377f_0682, v, 4096, &[(2, 0, pagina2.clone()), (2, npag, pagina2.clone())]);
    let mut mala_suma = valida(3_007_000);
    mala_suma[32 + 24 + 100] ^= 0xff;
    let mut cero = valida(3_007_000);
    cero[32..36].copy_from_slice(&0u32.to_be_bytes());
    resumar(&mut cero);
    let ids_falsos = { let mut w = cambiar_id(&wal_otra); resumar(&mut w); w };
    let ids_sin_resumar = cambiar_id(&wal_otra);
    let variantes: Vec<(&str, Vec<u8>, &str)> = vec![
        ("0 bytes", vec![], "intacta"),
        ("1 byte", vec![0x37], "intacta"),
        ("31 bytes", aleatorio[..31].to_vec(), "intacta"),
        ("32 bytes aleatorios", aleatorio[..32].to_vec(), "intacta"),
        ("5000 bytes aleatorios", aleatorio.clone(), "intacta"),
        ("cabecera válida sola", valida(3_007_000)[..32].to_vec(), "intacta"),
        ("cabecera válida + 10 bytes", valida(3_007_000)[..42].to_vec(), "intacta"),
        ("marco con suma mala", mala_suma, "intacta"),
        ("marco con pgno 0", cero, "intacta"),
        // Marcos sin la página de `meta` junto a una base identificada: desde el rompedor 23-oct (datos
        // MEDIA 1) lo decide la vista, porque así escriben también Pronto 2.0.2 y un editor de SQLite.
        // Estos no cambian nada: se aplican (tras copiar la base a `respaldo-*`) y la base queda igual.
        // Antes (22-oct-c, ALTA 1 (c)) se apartaban con aviso.
        ("2 marcos válidos (no-op)", valida(3_007_000), "intacta"),
        ("big-endian válido (no-op)", wal_hecho(0x377f_0683, 3_007_000, 4096, &[(2, npag, pagina2.clone())]), "intacta"),
        ("página 1000 (no potencia de 2)", wal_hecho(0x377f_0682, 3_007_000, 1000, &[]), "intacta"),
        ("página 65536 + basura", { let mut w = wal_hecho(0x377f_0682, 3_007_000, 65536, &[]); w.extend_from_slice(&aleatorio); w }, "intacta"),
        // Rompedor 22-oct-c, datos BAJA 5: SQLite no abre la base con él (`SQLITE_CANTOPEN`): se aparta con aviso.
        ("versión 3007001 (suma bien)", valida(3_007_001), "apartado"),
        ("-wal de otra base", wal_otra.clone(), "apartado"),
        ("otra base con base_id cambiado al mío, sumas sin recalcular", ids_sin_resumar, "intacta"),
        // Con las sumas recalculadas SQLite también lo aplicaría: no es una rotura de la guarda (quien puede escribir el -wal puede escribir la base).
        ("otra base con base_id cambiado al mío, sumas recalculadas", ids_falsos, "DATOS CAMBIADOS"),
    ];
    let mut fallos = Vec::new();
    for (k, (caso, wal, espera)) in variantes.into_iter().enumerate() {
        let dir = raiz.join(format!("v{k}"));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("pronto2.db");
        std::fs::copy(&molde, &p).unwrap();
        std::fs::write(side(&p, "-wal"), &wal).unwrap();
        let t = Instant::now();
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Db::open_at(&p)));
        let ms = t.elapsed().as_millis();
        let (desc, ok) = match r {
            Err(_) => ("PANIC".to_string(), false),
            Ok(Err(e)) => (format!("Err({})", format!("{e:#}").chars().take(220).collect::<String>()), false),
            Ok(Ok(db)) => {
                let d = datos_db(&db);
                let aviso = db.aviso().is_some();
                drop(db);
                let d2 = ver(&p).map(|x| x.1).unwrap_or_default();
                let res = if d == antes && d2 == antes { if aviso { "apartado" } else { "intacta" } } else { "DATOS CAMBIADOS" };
                (format!("{res} (aviso={aviso}, filas={})", d.len()), res == espera)
            }
        };
        println!("[manipulado] {caso}: {desc} en {ms} ms (esperado: {espera})");
        if !ok {
            fallos.push(format!("{caso}: {desc} (esperado {espera})"));
        }
    }
    let _ = std::fs::remove_dir_all(&raiz);
    assert!(fallos.is_empty(), "{}", fallos.join("\n"));
}

// ------------------------------------------------------------------ 6. página de tamaño distinto

#[test]
fn rompedor_datos_wal_con_otro_tamano_de_pagina() {
    let raiz = tmp("pagina");
    let g = raiz.join("pronto2.db");
    {
        let db = Db::open_at(&g).unwrap();
        fill(&db, 20, "buena");
    }
    let antes = ver(&g).unwrap().1;
    let mut fallos = Vec::new();
    for (caso, con_id) in [("1024_con_identidad", true), ("1024_sin_identidad", false)] {
        let d = raiz.join(caso);
        std::fs::create_dir_all(&d).unwrap();
        let o = d.join("pronto2.db");
        {
            let c = Connection::open(&o).unwrap();
            c.execute_batch("PRAGMA page_size = 1024; PRAGMA journal_mode = WAL; CREATE TABLE x(a); DROP TABLE x;").unwrap();
        }
        if con_id {
            let db = Db::open_at(&o).unwrap();
            db.conn.lock().execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
            fill(&db, 4, "intrusa");
            matar(db);
        } else {
            drop(Db::open_at(&o).unwrap());
            let c = Connection::open(&o).unwrap();
            c.execute_batch("DROP TABLE meta; PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
            drop(c);
            insertar_crudo(&o, 4, "intrusa", true);
        }
        let p = d.join("buena.db");
        std::fs::copy(&g, &p).unwrap();
        std::fs::copy(side(&o, "-wal"), side(&p, "-wal")).unwrap();
        let r = Db::open_at(&p);
        let desc = match r {
            Err(e) => {
                fallos.push(format!("{caso}: Pronto no arranca: {e:#}"));
                format!("Err({e:#})")
            }
            Ok(db) => {
                let dd = datos_db(&db);
                let aviso = db.aviso().is_some();
                drop(db);
                if dd != antes {
                    fallos.push(format!("{caso}: datos cambiados"));
                }
                format!("aviso={aviso} intacta={}", dd == antes)
            }
        };
        println!("[pagina] {caso}: {desc}");
    }
    let _ = std::fs::remove_dir_all(&raiz);
    assert!(fallos.is_empty(), "{}", fallos.join("\n"));
}

// ------------------------------------------------------------------ 7. base de 0 bytes con su -wal

#[test]
fn rompedor_datos_base_0_bytes_con_wal_propio() {
    let raiz = tmp("cero");
    let p = raiz.join("pronto2.db");
    let db = Db::open_at(&p).unwrap();
    fill(&db, 20, "volcado");
    db.conn.lock().execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
    fill(&db, 10, "en el wal");
    matar(db);
    let wal = std::fs::read(side(&p, "-wal")).unwrap();
    std::fs::File::create(&p).unwrap(); // la base se queda en 0 bytes (sincronizador, disco lleno…)
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Db::open_at(&p)));
    let desc = match r {
        Err(_) => "PANIC".to_string(),
        Ok(Err(e)) => format!("Err({e:#})"),
        Ok(Ok(db)) => {
            let q = quick(&db.conn.lock());
            let n = n_hist(&db.conn.lock());
            let aviso = db.aviso().map(|a| a.chars().take(160).collect::<String>());
            drop(db);
            format!("Ok: history={n} quick_check={q} aviso={aviso:?}")
        }
    };
    let wal_conservado = std::fs::read_dir(&raiz)
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("apartado-"))
        .any(|e| std::fs::read(e.path().join("pronto2.db-wal")).map(|w| w == wal).unwrap_or(false));
    let wal_ahora = std::fs::read(side(&p, "-wal")).unwrap_or_default();
    println!("[cero] {desc}; -wal original ({} B) conservado en apartado: {wal_conservado}; apartados={}; -wal en su sitio ahora: {} B (¿el original? {})", wal.len(), apartados(&raiz), wal_ahora.len(), wal_ahora == wal);
    let _ = std::fs::remove_dir_all(&raiz);
    assert!(wal_conservado || wal_ahora == wal, "el -wal (los últimos cambios del usuario) desapareció: {desc}");
}

// ------------------------------------------------------------------ 8. copias: borrado uno a uno

#[test]
fn rompedor_datos_copias_borrado_uno_a_uno() {
    let raiz = tmp("copias-uno-a-uno");
    let copias = raiz.join("copias");
    std::fs::create_dir_all(&copias).unwrap();
    let db = Db::open_at(&raiz.join("pronto2.db")).unwrap();
    for i in 0..20 {
        db.add_history(&nuevo(&format!("dictado {i} {}", if i < 15 { "PRIVADO" } else { "normal" }))).unwrap();
    }
    let vieja = copias.join("pronto2-20261001.db");
    db.conn.lock().execute("VACUUM INTO ?1", params![vieja.to_string_lossy()]).unwrap();
    for h in db.list_history(None, 100, 0).unwrap().iter().filter(|h| h.text.contains("PRIVADO")) {
        db.delete_history(h.id).unwrap(); // la papelera de cada dictado, uno a uno
    }
    let hecha = db.daily_backup(&copias, 7).unwrap().expect("copia");
    let sospechosa = hecha.to_string_lossy().contains("sospechosa");
    let privados_en_copia = ver(&vieja).unwrap().1.iter().filter(|r| r.contains("PRIVADO")).count();
    println!("[copias] tras borrar 15 de 20 uno a uno: copia de hoy={} sospechosa={sospechosa}; la copia normal del 01-oct sigue con {privados_en_copia} dictados borrados y no rota mientras la base tenga < 10 filas", hecha.display());
    drop(db);
    let _ = std::fs::remove_dir_all(&raiz);
    assert!(!sospechosa, "un borrado del usuario (uno a uno) marca la copia como sospechosa: las normales no rotan y guardan lo borrado");
}

// ------------------------------------------------------------------ 9. base real (copia)

#[test]
fn rompedor_datos_base_real_copiada() {
    let Ok(src) = std::env::var("PRONTO_ROMPEDOR_DATOS_REAL") else {
        println!("[real] sin PRONTO_ROMPEDOR_DATOS_REAL: no se mide");
        return;
    };
    let src = PathBuf::from(src);
    assert!(!src.to_string_lossy().to_lowercase().contains("appdata\\roaming\\pronto2"), "nunca la base de %APPDATA%");
    let raiz = tmp("real");
    let copia = |nombre: &str| -> PathBuf {
        let d = raiz.join(nombre);
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("pronto2.db");
        std::fs::copy(src.join("pronto2.db"), &p).unwrap();
        if src.join("pronto2.db-wal").exists() {
            std::fs::copy(src.join("pronto2.db-wal"), side(&p, "-wal")).unwrap();
        }
        for f in [p.clone(), side(&p, "-wal")] {
            if let Ok(m) = std::fs::metadata(&f) {
                let mut perm = m.permissions();
                #[allow(clippy::permissions_set_readonly_false)]
                perm.set_readonly(false); // el original de la sonda es de solo lectura; la copia de trabajo no
                std::fs::set_permissions(&f, perm).unwrap();
            }
        }
        p
    };
    let a = copia("vista");
    let (n_vista, d_vista) = ver(&a).unwrap();
    let solo = base_id_principal(&a);
    let w = std::fs::read(side(&a, "-wal")).unwrap_or_default();
    let b = copia("pronto");
    let t = Instant::now();
    let db = Db::open_at(&b).unwrap();
    let ms = t.elapsed().as_millis();
    let aviso = db.aviso().map(String::from);
    let d = datos_db(&db);
    let id = base_id(&db.conn.lock());
    drop(db);
    println!(
        "[real] -wal={} B ('base_id' dentro: {}), id_principal_antes={solo:?}; vista cruda: history={n_vista}, filas={}; open_at en {ms} ms: aviso={aviso:?}, filas={}, iguales={}, base_id={id:?}, id_principal_después={:?}, wal_después={} B",
        w.len(),
        contiene(&w, b"base_id"),
        d_vista.len(),
        d.len(),
        d == d_vista,
        base_id_principal(&b),
        flen(&side(&b, "-wal"))
    );
    let _ = std::fs::remove_dir_all(&raiz);
    assert!(aviso.is_none() && d == d_vista, "la base real (copia) no abre igual que su vista con -wal");
}

// ------------------------------------------------------------------ 10. rendimiento de los disparadores

fn insertar_sql(c: &Connection, i: usize) {
    c.execute(
        "INSERT INTO history (created_at, text, raw_text, app, duration_ms, words, transform, fix_dictionary, fix_snippets, fix_fillers, polished)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params!["2026-10-22T10:00:00+02:00", format!("dictado {i} de prueba con unas cuantas palabras más"), "crudo", "Code", 1000, 8, Option::<String>::None, 0, 0, 0, 0],
    )
    .unwrap();
}

fn pct(v: &mut [u128], p: f64) -> f64 {
    v.sort_unstable();
    v[((v.len() as f64 - 1.0) * p) as usize] as f64 / 1000.0
}

#[test]
#[ignore = "medida: coste de los disparadores (lanzar sola)"]
fn rompedor_datos_rendimiento_disparadores() {
    let n: usize = std::env::var("PRONTO_ROMPEDOR_N").ok().and_then(|s| s.parse().ok()).unwrap_or(100_000);
    let raiz = tmp("rendimiento");
    // A: Pronto 19b9574 (con disparadores). B: misma base y SQL de 7b7d46c (sin disparadores).
    let pa = raiz.join("a.db");
    let pb = raiz.join("b.db");
    let db = Db::open_at(&pa).unwrap();
    drop(Db::open_at(&pb).unwrap());
    let raw = Connection::open(&pb).unwrap();
    raw.execute_batch("PRAGMA synchronous = NORMAL;").unwrap();

    let medir = |f: &mut dyn FnMut(usize)| -> (f64, f64, f64) {
        let mut v = Vec::with_capacity(n);
        let t = Instant::now();
        for i in 0..n {
            let t1 = Instant::now();
            f(i);
            v.push(t1.elapsed().as_nanos());
        }
        let total = t.elapsed().as_secs_f64();
        (total, pct(&mut v, 0.5), pct(&mut v, 0.95))
    };
    let a_ins = medir(&mut |i| {
        db.add_history(&nuevo(&format!("dictado {i} de prueba con unas cuantas palabras más"))).unwrap();
    });
    let b_ins = medir(&mut |i| insertar_sql(&raw, i));
    println!("[rend] {n} add_history (autocommit): con disparadores {:.2} s (p50 {:.1} µs, p95 {:.1} µs) | sin {:.2} s (p50 {:.1} µs, p95 {:.1} µs)", a_ins.0, a_ins.1, a_ins.2, b_ins.0, b_ins.1, b_ins.2);
    let wal_a = flen(&side(&pa, "-wal"));

    let t = Instant::now();
    db.clear_history().unwrap();
    let a_clear = t.elapsed().as_secs_f64();
    let wal_a_clear = flen(&side(&pa, "-wal"));
    let t = Instant::now();
    raw.execute("DELETE FROM history", []).unwrap();
    let b_clear = t.elapsed().as_secs_f64();
    println!("[rend] «Borrar historial» de {n} filas: con disparadores {a_clear:.3} s | sin (7b7d46c: DELETE sin transacción ni disparadores) {b_clear:.3} s; -wal A antes {wal_a} B, después {wal_a_clear} B");

    // Inserción en bloque (una transacción): aísla el coste del disparador.
    let t = Instant::now();
    {
        let mut c = db.conn.lock();
        let tx = c.transaction().unwrap();
        for i in 0..n {
            insertar_sql(&tx, i);
        }
        tx.commit().unwrap();
    }
    let a_bulk = t.elapsed().as_secs_f64();
    let t = Instant::now();
    raw.execute_batch("BEGIN").unwrap();
    for i in 0..n {
        insertar_sql(&raw, i);
    }
    raw.execute_batch("COMMIT").unwrap();
    let b_bulk = t.elapsed().as_secs_f64();
    println!("[rend] {n} inserciones en 1 transacción: con disparadores {a_bulk:.2} s | sin {b_bulk:.2} s");
    let escrituras: String = db.conn.lock().query_row("SELECT value FROM meta WHERE key = 'escrituras'", [], |r| r.get(0)).unwrap();

    // Arranque con un -wal grande propio (el de «Borrar historial» + bloque, sin volcar).
    matar(db);
    let wal = flen(&side(&pa, "-wal"));
    let t = Instant::now();
    let db = Db::open_at(&pa).unwrap();
    let a_open = t.elapsed().as_millis();
    let aviso = db.aviso().is_some();
    matar(db);
    let t = Instant::now();
    let c = Connection::open(&pa).unwrap();
    let _ = n_hist(&c);
    matar_conn(c);
    let b_open = t.elapsed().as_millis();
    println!("[rend] escrituras={escrituras}; arranque con -wal propio de {wal} B: open_at {a_open} ms (aviso={aviso}) | apertura cruda + COUNT {b_open} ms");
    drop(raw);
    let _ = std::fs::remove_dir_all(&raiz);
}

// ------------------------------------------------------------------ 11. -wal enorme

fn mem_mb() -> (f64, f64) {
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows::Win32::System::Threading::GetCurrentProcess;
    unsafe {
        let mut c = PROCESS_MEMORY_COUNTERS::default();
        let _ = GetProcessMemoryInfo(GetCurrentProcess(), &mut c, std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32);
        (c.WorkingSetSize as f64 / 1_048_576.0, c.PeakWorkingSetSize as f64 / 1_048_576.0)
    }
}

#[test]
#[ignore = "medida: -wal enorme (lanzar sola)"]
fn rompedor_datos_wal_enorme() {
    let mb: usize = std::env::var("PRONTO_ROMPEDOR_WAL_MB").ok().and_then(|s| s.parse().ok()).unwrap_or(256);
    let raiz = tmp("wal-enorme");
    let mut resultados = Vec::new();
    for caso in ["identificada", "de_antes"] {
        let p = raiz.join(format!("{caso}.db"));
        if caso == "identificada" {
            let db = Db::open_at(&p).unwrap();
            fill(&db, 20, "buena");
            db.conn.lock().execute_batch("PRAGMA wal_checkpoint(TRUNCATE); PRAGMA wal_autocheckpoint = 0;").unwrap();
            let grande = "w".repeat(1_000_000);
            for i in 0..mb {
                db.conn.lock().execute("INSERT INTO notes (text, created_at, updated_at) VALUES (?1, '2026-10-22', '2026-10-22')", params![format!("{i}{grande}")]).unwrap();
            }
            matar(db);
        } else {
            plantilla_antigua(&p);
            let c = Connection::open(&p).unwrap();
            c.execute_batch("PRAGMA wal_autocheckpoint = 0;").unwrap();
            let grande = "w".repeat(1_000_000);
            for i in 0..mb {
                c.execute("INSERT INTO notes (text, created_at, updated_at) VALUES (?1, '2026-10-22', '2026-10-22')", params![format!("{i}{grande}")]).unwrap();
            }
            matar_conn(c);
        }
        let wal = flen(&side(&p, "-wal"));
        let (_, pico0) = mem_mb();
        let t = Instant::now();
        let db = Db::open_at(&p).unwrap();
        let ms = t.elapsed().as_millis();
        let (ws1, pico1) = mem_mb();
        let aviso = db.aviso().is_some();
        let notas: i64 = db.conn.lock().query_row("SELECT COUNT(*) FROM notes", [], |r| r.get(0)).unwrap();
        matar(db);
        let wal_despues = flen(&side(&p, "-wal"));
        let t = Instant::now();
        let c = Connection::open(&p).unwrap();
        let _ = n_hist(&c);
        matar_conn(c);
        let crudo = t.elapsed().as_millis();
        let r = format!(
            "{caso}: -wal {:.0} MB → open_at {ms} ms (aviso={aviso}, notas={notas}); pico de memoria {pico0:.0}→{pico1:.0} MB (ws {ws1:.0}); -wal después {:.0} MB; apertura cruda {crudo} ms",
            wal as f64 / 1_048_576.0,
            wal_despues as f64 / 1_048_576.0
        );
        println!("[enorme] {r}");
        resultados.push(r);
        let _ = std::fs::remove_file(&p);
        let _ = std::fs::remove_file(side(&p, "-wal"));
        let _ = std::fs::remove_file(side(&p, "-shm"));
    }
    let _ = std::fs::remove_dir_all(&raiz);
}

// ------------------------------------------------------------------ 12. meta tocada por otro programa (informativa)

#[test]
fn rompedor_datos_meta_borrada_fuera() {
    let mut fallos = Vec::new();
    let raiz = tmp("meta-fuera");
    for caso in ["delete_meta", "drop_meta"] {
        let dir = raiz.join(caso);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("pronto2.db");
        let db = Db::open_at(&p).unwrap();
        fill(&db, 20, caso);
        db.conn.lock().execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
        drop(db);
        let c = Connection::open(&p).unwrap();
        c.execute_batch(if caso == "delete_meta" { "DELETE FROM meta;" } else { "DROP TABLE meta;" }).unwrap();
        c.execute("INSERT INTO history (created_at, text, raw_text) VALUES ('2026-10-22T10:00:00+02:00', 'escrito con DB Browser', '')", []).unwrap();
        let vivo = datos(&c);
        matar_conn(c);
        let id_antes = base_id_principal(&p);
        let db = Db::open_at(&p).unwrap();
        let aviso = db.aviso().is_some();
        let d = datos_db(&db);
        let id = base_id(&db.conn.lock());
        // Pronto sigue: un dictado y muere sin volcar (el -wal lleva el base_id nuevo y el viejo borrado).
        db.add_history(&nuevo("dictado tras regenerar el base_id")).unwrap();
        let vivo2 = datos_db(&db);
        matar(db);
        let db = Db::open_at(&p).unwrap();
        let aviso2 = db.aviso().map(|a| a.chars().take(150).collect::<String>());
        let d2 = datos_db(&db);
        drop(db);
        println!("[meta-fuera] {caso}: id_principal_antes={id_antes:?} 1.ª apertura: aviso={aviso} conserva_lo_escrito={} base_id_nuevo={id:?}; tras dictado+apagón: aviso={aviso2:?} conserva={} apartados={}", d == vivo, d2 == vivo2, apartados(&dir));
        if aviso2.is_some() || d2 != vivo2 {
            fallos.push(format!("{caso}: tras regenerar el base_id, el -wal propio se apartó"));
        }
    }
    let _ = std::fs::remove_dir_all(&raiz);
    assert!(fallos.is_empty(), "{}", fallos.join("\n"));
}

// ================================================================== SEGUNDA PASADA (rompedor 23-oct, datos)
// Ataques a los arreglos de 0e5c4d2: falsos positivos de «-wal sin la página de meta = ajeno»,
// copias con identidad propia al restaurar/comparar/rotar, y huecos de la regla de contadores.
// `cargo test rompedor_datos_p2_ -- --include-ignored --test-threads=1 --nocapture`.
// Las 6 que reproducían una rotura (MEDIA 1-4, BAJA 5 y 6) se arreglaron el 23-oct por la tarde y ya
// corren siempre, sin `#[ignore]`, como regresión.

/// Conexión «de Pronto 2.0.2 / de un editor externo»: sin los disparadores TEMP que tocan `meta`.
fn conexion_sin_disparadores(p: &Path) -> Connection {
    let c = Connection::open(p).unwrap();
    c.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;").unwrap();
    c
}

fn crudo_relleno(c: &Connection, n: usize, tag: &str, pad: usize) {
    for i in 0..n {
        c.execute(
            "INSERT INTO history (created_at, text, raw_text, app) VALUES ('2026-10-13T21:49:00+02:00', ?1, '', 'Code')",
            params![format!("{tag} {i} {}", "x".repeat(pad))],
        )
        .unwrap();
    }
}

/// Abre como Pronto y cuenta qué se ve: (aviso, datos, quick_check).
fn abrir_como_pronto(p: &Path) -> Result<(Option<String>, Vec<String>, String), String> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Db::open_at(p))) {
        Err(_) => Err("open_at hizo panic".into()),
        Ok(Err(e)) => Err(format!("open_at=Err({e:#})")),
        Ok(Ok(db)) => {
            let a = db.aviso().map(|a| a.chars().take(170).collect::<String>());
            let d = datos_db(&db);
            let q = quick(&db.conn.lock());
            drop(db);
            Ok((a, d, q))
        }
    }
}

// ------------------------------------------------------------------ p2-1. -wal propio escrito sin pasar por los disparadores

#[test]
fn rompedor_datos_p2_wal_sin_meta_de_version_anterior() {
    let raiz = tmp("p2-sin-meta");
    let mut fallos = Vec::new();
    for caso in ["pronto_2_0_2_dicta_5", "pronto_2_0_2_solo_ajustes", "editor_externo_corrige_un_dictado"] {
        let dir = raiz.join(caso);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("pronto2.db");
        {
            let db = Db::open_at(&p).unwrap(); // 2.0.3: base identificada; cierre limpio (todo volcado)
            fill(&db, 20, "en 2.0.3");
        }
        let id = base_id_principal(&p);
        let c = conexion_sin_disparadores(&p);
        match caso {
            "pronto_2_0_2_dicta_5" => crudo_relleno(&c, 5, "dictado en 2.0.2", 0),
            "pronto_2_0_2_solo_ajustes" => {
                c.execute("INSERT INTO settings (id, json) VALUES (1, '{}') ON CONFLICT(id) DO UPDATE SET json = json_set(json, '$.rompedorP2', 1)", []).unwrap();
            }
            _ => {
                c.execute("UPDATE history SET text = 'corregido a mano en DB Browser' WHERE id = 3", []).unwrap();
            }
        }
        let vivo = datos(&c);
        matar_conn(c); // 2.0.2 no vuelca nunca y «Salir» no cierra la conexión
        let wal = flen(&side(&p, "-wal"));
        let r = abrir_como_pronto(&p);
        let desc = match &r {
            Ok((a, d, q)) => format!("aviso={a:?} quick_check={q} se_ve_lo_escrito={} apartados={}", *d == vivo, apartados(&dir)),
            Err(e) => e.clone(),
        };
        println!("[p2-sin-meta] {caso}: id_principal={id:?} -wal={wal} B → {desc}");
        if !matches!(&r, Ok((None, d, _)) if *d == vivo) {
            fallos.push(format!("{caso}: {desc}"));
        }
    }
    let _ = std::fs::remove_dir_all(&raiz);
    assert!(fallos.is_empty(), "{}", fallos.join("\n"));
}

// ------------------------------------------------------------------ p2-2. escrituras de Pronto que no tocan meta (latentes)

#[test]
fn rompedor_datos_p2_escrituras_propias_sin_meta() {
    let raiz = tmp("p2-propias-sin-meta");
    let mut fallos = Vec::new();
    for caso in ["migracion_v10_solo_esquema", "fts_optimize", "analyze"] {
        let dir = raiz.join(caso);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("pronto2.db");
        let db = Db::open_at(&p).unwrap();
        fill(&db, 10, caso);
        let m = db.create_meeting("Reunión", Some("Teams")).unwrap();
        for i in 0..3 {
            // tres tandas → tres segmentos FTS (optimize tiene algo que fusionar)
            db.conn.lock().execute("INSERT INTO meeting_fts (content, meeting_id, kind) VALUES (?1, ?2, 'seg')", params![format!("presupuesto trimestral {i}"), m.id]).unwrap();
        }
        db.conn.lock().execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
        {
            // La MISMA conexión de Pronto (con sus disparadores TEMP).
            let c = db.conn.lock();
            match caso {
                "migracion_v10_solo_esquema" => c
                    .execute_batch("BEGIN; ALTER TABLE meetings ADD COLUMN rompedor_v10 TEXT; CREATE INDEX idx_rompedor_v10 ON meetings(rompedor_v10); PRAGMA user_version = 10; COMMIT;")
                    .unwrap(),
                "fts_optimize" => c.execute_batch("INSERT INTO meeting_fts(meeting_fts) VALUES('optimize');").unwrap(),
                _ => c.execute_batch("ANALYZE;").unwrap(),
            }
        }
        let vivo = datos_db(&db);
        let ver_v: i64 = db.conn.lock().query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        matar(db); // apagón justo después (en la migración: entre su COMMIT y `touch_meta`)
        let wal = flen(&side(&p, "-wal"));
        let r = abrir_como_pronto(&p);
        let desc = match &r {
            Ok((a, d, q)) => format!("aviso={a:?} quick_check={q} iguales={} apartados={}", *d == vivo, apartados(&dir)),
            Err(e) => e.clone(),
        };
        println!("[p2-propias] {caso}: user_version={ver_v} -wal={wal} B → {desc}");
        if !matches!(&r, Ok((None, d, _)) if *d == vivo) {
            fallos.push(format!("{caso}: {desc}"));
        }
    }
    let _ = std::fs::remove_dir_all(&raiz);
    assert!(fallos.is_empty(), "{}", fallos.join("\n"));
}

// ------------------------------------------------------------------ p2-3. restaurar una copia de 2.0.2 (sin identidad) con el -wal de 2.0.3 (aguanta: regresión)

#[test]
fn rompedor_datos_p2_restaurar_copia_de_2_0_2() {
    let raiz = tmp("p2-restaurar-202");
    let mut fallos = Vec::new();
    for (caso, huecos, n, pad) in [("sin_huecos", false, 400usize, 300usize), ("con_huecos", true, 400, 300), ("base_pequena", false, 5, 0)] {
        let dir = raiz.join(caso);
        let copias = dir.join("copias");
        std::fs::create_dir_all(&copias).unwrap();
        let p = dir.join("pronto2.db");
        plantilla_antigua(&p);
        {
            let c = conexion_sin_disparadores(&p); // 2.0.2
            crudo_relleno(&c, n, "viejo", pad);
            if huecos {
                c.execute("DELETE FROM history WHERE id % 2 = 0 AND id < 200", []).unwrap();
            }
        }
        let copia = copias.join("pronto2-20261020.db"); // copia diaria de 2.0.2: VACUUM INTO, sin meta
        Connection::open(&p).unwrap().execute("VACUUM INTO ?1", params![copia.to_string_lossy()]).unwrap();
        {
            let c = conexion_sin_disparadores(&p);
            crudo_relleno(&c, if n > 5 { 30 } else { 2 }, "2.0.2 tras la copia", pad);
        }
        // Primer arranque de 2.0.3: identidad, copia del día, dictados y borrados; muere sin volcar.
        let db = Db::open_at(&p).unwrap();
        assert!(db.aviso().is_none());
        let _ = db.daily_backup(&copias, 7).unwrap();
        for i in 0..(if n > 5 { 50 } else { 2 }) {
            db.add_history(&nuevo(&format!("de hoy {i} {}", "y".repeat(300)))).unwrap();
        }
        for h in db.list_history(None, 1000, 0).unwrap().iter().filter(|h| (201..=260).contains(&h.id)) {
            db.delete_history(h.id).unwrap();
        }
        let vivo = datos_db(&db);
        matar(db);
        let wal = flen(&side(&p, "-wal"));
        let de_la_copia = ver(&copia).unwrap().1;
        std::fs::copy(&copia, &p).unwrap(); // restaura la copia de 2.0.2 sin apartar el -wal
        let r = abrir_como_pronto(&p);
        let tras = ver(&p).map(|(n, _)| n);
        let desc = match &r {
            Ok((a, d, q)) => {
                let que = if *d == de_la_copia {
                    "la copia".to_string()
                } else if *d == vivo {
                    "lo de hoy".to_string()
                } else {
                    format!("NI la copia NI lo de hoy ({}; vs hoy {})", primera_diferencia(d, &de_la_copia), primera_diferencia(d, &vivo))
                };
                format!("aviso={a:?} quick_check={q} filas={} ve={que} history_tras_cerrar={tras:?} apartados={}", d.len(), apartados(&dir))
            }
            Err(e) => e.clone(),
        };
        println!("[p2-restaurar-202] {caso}: -wal de 2.0.3={wal} B, copia={} filas, hoy={} filas → {desc}", de_la_copia.len(), vivo.len());
        if !matches!(&r, Ok((_, d, _)) if *d == de_la_copia) {
            fallos.push(format!("{caso}: {desc}"));
        }
    }
    let _ = std::fs::remove_dir_all(&raiz);
    assert!(fallos.is_empty(), "{}", fallos.join("\n"));
}

// ------------------------------------------------------------------ p2-4. restaurar la MISMA copia (con identidad) dos veces

#[test]
fn rompedor_datos_p2_restaurar_la_misma_copia_dos_veces() {
    let raiz = tmp("p2-dos-veces");
    let mut fallos = Vec::new();
    for (caso, volcado_intermedio) in [("con_volcado_en_la_sesion", true), ("sin_volcado_en_la_sesion", false)] {
        let dir = raiz.join(caso);
        let copias = dir.join("copias");
        std::fs::create_dir_all(&copias).unwrap();
        let p = dir.join("pronto2.db");
        let copia = {
            let db = Db::open_at(&p).unwrap();
            for i in 0..400 {
                db.add_history(&nuevo(&format!("viejo {i} {}", "x".repeat(300)))).unwrap();
            }
            db.daily_backup(&copias, 7).unwrap().expect("copia del día")
        }; // cierre limpio
        let de_la_copia = ver(&copia).unwrap().1;
        // 1.ª restauración, bien hecha (sin -wal al lado).
        for s in ["-wal", "-shm"] {
            let _ = std::fs::remove_file(side(&p, s));
        }
        std::fs::copy(&copia, &p).unwrap();
        let db = Db::open_at(&p).unwrap();
        let aviso1 = db.aviso().map(String::from);
        let _ = db.daily_backup(&copias, 7).unwrap(); // la de hoy ya existe: solo vuelca (PASSIVE)
        for i in 0..50 {
            db.add_history(&nuevo(&format!("tras restaurar {i} {}", "y".repeat(300)))).unwrap();
        }
        for h in db.list_history(None, 1000, 0).unwrap().iter().filter(|h| (101..=160).contains(&h.id)) {
            db.delete_history(h.id).unwrap();
        }
        if volcado_intermedio {
            db.checkpoint(); // lo hace SQLite solo cada 1000 páginas, o Pronto al arrancar
            for i in 0..10 {
                db.add_history(&nuevo(&format!("tras el volcado {i} {}", "z".repeat(300)))).unwrap();
            }
            for h in db.list_history(None, 1000, 0).unwrap().iter().filter(|h| (201..=230).contains(&h.id)) {
                db.delete_history(h.id).unwrap();
            }
        }
        let vivo = datos_db(&db);
        let id_vivo = base_id(&db.conn.lock());
        matar(db);
        let wal = flen(&side(&p, "-wal"));
        // 2.ª restauración de la MISMA copia, esta vez sin apartar el -wal.
        std::fs::copy(&copia, &p).unwrap();
        let r = abrir_como_pronto(&p);
        let desc = match &r {
            Ok((a, d, q)) => {
                let que = if *d == de_la_copia {
                    "la copia".to_string()
                } else if *d == vivo {
                    "lo de antes de restaurar".to_string()
                } else {
                    format!("NI la copia NI lo de antes ({}; vs antes {})", primera_diferencia(d, &de_la_copia), primera_diferencia(d, &vivo))
                };
                format!("aviso={a:?} quick_check={q} ve={que} apartados={}", apartados(&dir))
            }
            Err(e) => e.clone(),
        };
        println!(
            "[p2-dos-veces] {caso}: aviso_1.ª={aviso1:?} base_id de la copia={:?} base_id de la base viva={id_vivo:?} -wal={wal} B → {desc}",
            base_id_principal(&copia)
        );
        if !matches!(&r, Ok((_, d, _)) if *d == de_la_copia) {
            fallos.push(format!("{caso}: {desc}"));
        }
    }
    let _ = std::fs::remove_dir_all(&raiz);
    assert!(fallos.is_empty(), "{}", fallos.join("\n"));
}

// ------------------------------------------------------------------ p2-5. copias tras restaurar una de 2.0.2 o tras una base nueva

#[test]
fn rompedor_datos_p2_copias_sospechosas_para_siempre() {
    let raiz = tmp("p2-copias-tras-restaurar"); // sin «sospechosa» en la ruta: Pronto lo busca en la ruta entera
    let mut fallos = Vec::new();
    for caso in ["restaura_copia_de_2_0_2", "base_nueva_tras_incidente"] {
        let dir = raiz.join(caso);
        let copias = dir.join("copias");
        std::fs::create_dir_all(&copias).unwrap();
        let p = dir.join("pronto2.db");
        plantilla_antigua(&p);
        {
            let c = conexion_sin_disparadores(&p);
            crudo_relleno(&c, 30, "2.0.2", 50);
        }
        let vieja = copias.join("pronto2-20261015.db"); // copia de 2.0.2
        Connection::open(&p).unwrap().execute("VACUUM INTO ?1", params![vieja.to_string_lossy()]).unwrap();
        {
            let db = Db::open_at(&p).unwrap(); // 2.0.3
            fill(&db, 10, "2.0.3");
            for d in ["20261016", "20261017", "20261018"] {
                let c = db.daily_backup(&copias, 7).unwrap().expect("copia");
                std::fs::rename(&c, copias.join(format!("pronto2-{d}.db"))).unwrap(); // pasa el día
                fill(&db, 2, d);
            }
        }
        if caso == "restaura_copia_de_2_0_2" {
            for s in ["-wal", "-shm"] {
                let _ = std::fs::remove_file(side(&p, s));
            }
            std::fs::copy(&vieja, &p).unwrap(); // como dice el aviso, bien hecho
        } else {
            std::fs::File::create(&p).unwrap(); // incidente: la base se queda en 0 B; Pronto empieza otra
        }
        let db = Db::open_at(&p).unwrap();
        let aviso_inicial = db.aviso().is_some();
        if caso == "base_nueva_tras_incidente" {
            fill(&db, 40, "base nueva");
        }
        let mut dias = Vec::new();
        for d in ["20261019", "20261020", "20261021", "20261022"] {
            let c = db.daily_backup(&copias, 7).unwrap().expect("copia");
            let sosp = c.file_name().unwrap().to_string_lossy().contains("sospechosa");
            std::fs::rename(&c, copias.join(if sosp { format!("pronto2-{d}-sospechosa.db") } else { format!("pronto2-{d}.db") })).unwrap();
            dias.push(format!("{d}:{}", if sosp { "SOSPECHOSA" } else { "normal" }));
            fill(&db, 2, d);
        }
        drop(db);
        // Un incidente más tarde: ¿qué copia recomienda el aviso?
        let _ = std::fs::remove_file(side(&p, "-wal"));
        let _ = std::fs::remove_file(side(&p, "-shm"));
        std::fs::File::create(&p).unwrap();
        let db = Db::open_at(&p).unwrap();
        let recomienda = db
            .aviso()
            .and_then(|a| a.split("La copia más reciente es ").nth(1))
            .map(|s| s.split(" (").next().unwrap_or("").rsplit(['\\', '/']).next().unwrap_or("").to_string());
        drop(db);
        let normales: Vec<String> = std::fs::read_dir(&copias).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| !n.contains("sospechosa")).collect();
        println!("[p2-sospechosas] {caso}: aviso al restaurar={aviso_inicial}; días siguientes: {dias:?}; copias normales: {normales:?}; tras otro incidente el aviso recomienda {recomienda:?}");
        if dias.iter().any(|d| d.ends_with("SOSPECHOSA")) {
            fallos.push(format!("{caso}: {dias:?}; recomienda {recomienda:?}"));
        }
    }
    let _ = std::fs::remove_dir_all(&raiz);
    assert!(fallos.is_empty(), "{}", fallos.join("\n"));
}

// ------------------------------------------------------------------ p2-6. -wal ajeno sin identidad con contadores MÁS ALTOS

#[test]
fn rompedor_datos_p2_ajeno_sin_identidad_con_mas_filas() {
    let raiz = tmp("p2-ajeno-mas");
    let plantilla = raiz.join("plantilla.db");
    plantilla_antigua(&plantilla);
    let intrusa_dir = raiz.join("intrusa");
    std::fs::create_dir_all(&intrusa_dir).unwrap();
    let intrusa = intrusa_dir.join("pronto2.db");
    std::fs::copy(&plantilla, &intrusa).unwrap();
    insertar_crudo(&intrusa, 30, "intrusa", true); // 30 > 20: el contador no baja
    let wal_intruso = std::fs::read(side(&intrusa, "-wal")).unwrap();
    let mut fallos = Vec::new();
    for caso in ["buena_ya_identificada", "buena_de_antes_primer_arranque_2_0_3"] {
        let dir = raiz.join(caso);
        std::fs::create_dir_all(&dir).unwrap();
        let g = dir.join("pronto2.db");
        std::fs::copy(&plantilla, &g).unwrap();
        insertar_crudo(&g, 20, "buena", false);
        if caso == "buena_ya_identificada" {
            drop(Db::open_at(&g).unwrap());
        }
        let antes = ver(&g).unwrap();
        std::fs::write(side(&g, "-wal"), &wal_intruso).unwrap();
        let db = Db::open_at(&g).unwrap();
        let aviso = db.aviso().map(|a| a.chars().take(150).collect::<String>());
        let visto = n_hist(&db.conn.lock());
        drop(db);
        let despues = ver(&g).unwrap();
        let quedan_buenas = despues.1.iter().filter(|r| r.contains("\"buena ")).count();
        println!(
            "[p2-ajeno-mas] {caso}: buenas={} visto_al_abrir={visto} aviso={aviso:?} tras_cerrar: history={} filas 'buena' que quedan={quedan_buenas} apartados={}",
            antes.0,
            despues.0,
            apartados(&dir)
        );
        if despues.1 != antes.1 {
            fallos.push(format!("{caso}: la base buena perdió datos ({} filas 'buena' de {})", quedan_buenas, antes.0));
        }
    }
    let _ = std::fs::remove_dir_all(&raiz);
    assert!(fallos.is_empty(), "{}", fallos.join("\n"));
}

// ------------------------------------------------------------------ p2-7. regresión: el disparador vuelve tras un borrado masivo que falla

#[test]
fn rompedor_datos_p2_disparador_vuelve_tras_borrado_fallido() {
    let raiz = tmp("p2-disparador");
    let p = raiz.join("pronto2.db");
    let db = Db::open_at(&p).unwrap();
    fill(&db, 10, "d");
    db.conn.lock().execute("UPDATE history SET created_at = '2020-01-01T00:00:00+01:00' WHERE id = 1", []).unwrap();
    db.conn.lock().execute_batch("CREATE TEMP TRIGGER rompedor_veto BEFORE DELETE ON history BEGIN SELECT RAISE(ABORT, 'veto'); END;").unwrap();
    let r1 = db.clear_history();
    let r2 = db.purge_history(1); // borraría el id 1: falla igual
    let hay: i64 = db.conn.lock().query_row("SELECT COUNT(*) FROM sqlite_temp_master WHERE name = 'pronto_toque_DELETE_history'", [], |r| r.get(0)).unwrap();
    db.conn.lock().execute_batch("DROP TRIGGER rompedor_veto; PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
    db.delete_history(2).unwrap(); // única escritura del -wal
    let vivo = datos_db(&db);
    matar(db);
    let r = comprobar(&raiz, &p, &vivo);
    println!("[p2-disparador] clear_falla={} purge_falla={} disparador_tras_fallos={hay} → {r:?}", r1.is_err(), r2.is_err());
    let _ = std::fs::remove_dir_all(&raiz);
    assert!(r1.is_err() && r2.is_err() && hay == 1 && r.is_ok(), "el disparador no volvió tras el fallo: {r:?}");
}

// ------------------------------------------------------------------ p2-8. regresión: copias con identidad propia rotan y se leen

#[test]
fn rompedor_datos_p2_copias_con_identidad_rotan() {
    let raiz = tmp("p2-rotan");
    let copias = raiz.join("copias");
    std::fs::create_dir_all(&copias).unwrap();
    let p = raiz.join("pronto2.db");
    let db = Db::open_at(&p).unwrap();
    let id = base_id(&db.conn.lock()).unwrap();
    fill(&db, 20, "base");
    let mut sosp = Vec::new();
    for d in 1..=10 {
        let c = db.daily_backup(&copias, 7).unwrap().expect("copia");
        if c.to_string_lossy().contains("sospechosa") {
            sosp.push(d);
        }
        std::fs::rename(&c, copias.join(format!("pronto2-202610{d:02}.db"))).unwrap();
        fill(&db, 1, "día");
    }
    let hoy = db.daily_backup(&copias, 7).unwrap().expect("copia de hoy");
    drop(db);
    let nombres: Vec<String> = std::fs::read_dir(&copias).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    let ids: std::collections::BTreeSet<Option<String>> = nombres.iter().map(|n| base_id_principal(&copias.join(n))).collect();
    let de: std::collections::BTreeSet<Option<String>> = nombres.iter().map(|n| meta_principal(&copias.join(n), "copia_de")).collect();
    let ok_q = nombres.iter().all(|n| quick(&Connection::open_with_flags(copias.join(n), OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap()) == "ok");
    println!("[p2-rotan] copias={nombres:?} sospechosas={sosp:?} ids_distintos={} copia_de={de:?} (base {id}) quick_check_todas={ok_q} hoy={}", ids.len(), hoy.display());
    let _ = std::fs::remove_dir_all(&raiz);
    assert!(sosp.is_empty() && nombres.len() == 7 && nombres.iter().all(|n| n.ends_with(".db")) && ids.len() == 7 && de.len() == 1 && de.contains(&Some(id)) && ok_q);
}

// ------------------------------------------------------------------ p2-9. base real (copia): restaurar su copia de 2.0.2 con el -wal de 2.0.3

#[test]
fn rompedor_datos_p2_real_restaurar_copia_de_2_0_2() {
    let Ok(src) = std::env::var("PRONTO_ROMPEDOR_DATOS_REAL") else {
        println!("[p2-real] sin PRONTO_ROMPEDOR_DATOS_REAL: no se mide");
        return;
    };
    let src = PathBuf::from(src);
    assert!(!src.to_string_lossy().to_lowercase().contains("appdata\\roaming\\pronto2"), "nunca la base de %APPDATA%");
    let Ok(copia_real) = std::env::var("PRONTO_ROMPEDOR_DATOS_REAL_COPIA").map(PathBuf::from) else {
        println!("[p2-real] sin PRONTO_ROMPEDOR_DATOS_REAL_COPIA: no se mide");
        return;
    };
    let raiz = tmp("p2-real");
    let copias = raiz.join("copias");
    std::fs::create_dir_all(&copias).unwrap();
    let p = raiz.join("pronto2.db");
    std::fs::copy(src.join("pronto2.db"), &p).unwrap();
    std::fs::copy(src.join("pronto2.db-wal"), side(&p, "-wal")).unwrap();
    let db = Db::open_at(&p).unwrap(); // primer arranque de 2.0.3 sobre la base real
    let aviso0 = db.aviso().map(String::from);
    let _ = db.daily_backup(&copias, 7);
    for i in 0..15 {
        db.add_history(&nuevo(&format!("dictado de 2.0.3 {i} {}", "y".repeat(200)))).unwrap();
    }
    let vivo = datos_db(&db);
    matar(db);
    let de_la_copia = ver(&copia_real).unwrap().1;
    std::fs::copy(&copia_real, &p).unwrap(); // restaura la copia real de 2.0.2 sin apartar el -wal
    let r = abrir_como_pronto(&p);
    let desc = match &r {
        Ok((a, d, q)) => format!("aviso={a:?} quick_check={q} ve_la_copia={} ve_lo_de_hoy={} filas={} apartados={}", *d == de_la_copia, *d == vivo, d.len(), apartados(&raiz)),
        Err(e) => e.clone(),
    };
    println!("[p2-real] aviso_primer_arranque={aviso0:?}; copia real={} filas, hoy={} filas → {desc}", de_la_copia.len(), vivo.len());
    let _ = std::fs::remove_dir_all(&raiz);
    assert!(matches!(&r, Ok((_, d, _)) if *d == de_la_copia), "{desc}");
}

// ------------------------------------------------------------------ p2-10. regresión: cada escritura de Pronto, sola en el -wal, lleva meta

#[test]
fn rompedor_datos_p2_cada_escritura_lleva_meta() {
    use crate::models::Transform;
    use crate::summary::{ParsedAction, ParsedSummary};
    let raiz = tmp("p2-cada-escritura");
    let p = raiz.join("pronto2.db");
    let (mid, wid) = {
        let db = Db::open_at(&p).unwrap();
        fill(&db, 12, "base");
        db.conn.lock().execute("UPDATE history SET created_at = '2020-01-01T00:00:00+01:00' WHERE id <= 3", []).unwrap();
        db.conn.lock().execute("UPDATE history SET has_audio = 1 WHERE id = 4", []).unwrap();
        let w = db.add_word("Txonso", None, &["chonso".to_string()]).unwrap();
        db.add_snippet("firma", "Un saludo").unwrap();
        db.save_note(None, "nota").unwrap();
        let m = db.create_meeting("Reunión", Some("Teams")).unwrap();
        let seg = |i: i64| Segment { meeting_id: m.id, speaker: "me".into(), start_ms: i * 1000, end_ms: i * 1000 + 900, text: format!("turno {i} presupuesto"), ..Default::default() };
        db.replace_segments(m.id, &[seg(0), seg(1), seg(2)], false).unwrap();
        let s = ParsedSummary { summary: "resumen".into(), action_items: vec![ParsedAction { text: "enviar".into(), owner: None, due: None }], ..Default::default() };
        db.save_summary(m.id, &s).unwrap();
        (m.id, w.id)
    };
    let seg = |i: i64| Segment { meeting_id: mid, speaker: "them".into(), start_ms: i * 1000, end_ms: i * 1000 + 900, text: format!("final {i}"), ..Default::default() };
    type Op<'a> = (&'a str, Box<dyn Fn(&Db) -> anyhow::Result<()> + 'a>);
    let ops: Vec<Op> = vec![
        ("add_history", Box::new(|db| db.add_history(&nuevo("uno")).map(|_| ()))),
        ("update_history_text", Box::new(|db| db.update_history_text(5, "otro", "otro", 1))),
        ("set_has_audio", Box::new(|db| db.set_has_audio(6, true))),
        ("delete_history", Box::new(|db| db.delete_history(7))),
        ("purge_audio", Box::new(|db| db.purge_audio(0).map(|_| ()))),
        ("purge_history", Box::new(|db| db.purge_history(30).map(|_| ()))),
        ("save_settings", Box::new(|db| { let mut s = db.load_settings(); s.keep_audio_days = 3; db.save_settings(&s) })),
        ("add_word", Box::new(|db| db.add_word("Pronto", Some("PRONTO"), &[]).map(|_| ()))),
        ("toggle_word_star", Box::new(move |db| db.toggle_word_star(wid))),
        ("add_word_variant", Box::new(move |db| db.add_word_variant(wid, "chonzo").map(|_| ()))),
        ("update_word", Box::new(move |db| db.update_word(wid, "Txonso", Some("TXONSO"), None))),
        ("add_snippet", Box::new(|db| db.add_snippet("dir", "Calle 1").map(|_| ()))),
        ("delete_snippet", Box::new(|db| db.delete_snippet(1))),
        ("save_note", Box::new(|db| db.save_note(Some(1), "nota editada").map(|_| ()))),
        ("delete_note", Box::new(|db| db.delete_note(1))),
        ("save_transform", Box::new(|db| db.save_transform(&Transform { id: 0, name: "Mío".into(), prompt: "p".into(), builtin: false, hotkey_index: None }).map(|_| ()))),
        ("delete_transform", Box::new(|db| db.delete_transform(3))),
        ("rename_meeting", Box::new(move |db| db.rename_meeting(mid, "Otra"))),
        ("toggle_action_item", Box::new(move |db| db.toggle_action_item(mid, 0))),
        ("extend_duration", Box::new(move |db| db.extend_duration(mid, 99_000))),
        ("set_polish_done", Box::new(move |db| db.set_polish_done(mid))),
        ("set_summary_status", Box::new(move |db| db.set_summary_status(mid, "pending"))),
        ("close_orphan_meetings", Box::new(|db| db.close_orphan_meetings().map(|_| ()))),
        ("replace_segments", Box::new(move |db| db.replace_segments(mid, &[seg(0), seg(1)], true).map(|_| ()))),
        ("save_summary", Box::new(move |db| db.save_summary(mid, &ParsedSummary { summary: "otro resumen".into(), ..Default::default() }))),
        ("delete_word", Box::new(move |db| db.delete_word(wid))),
        ("delete_meeting", Box::new(move |db| db.delete_meeting(mid))),
        ("clear_history", Box::new(|db| db.clear_history())),
    ];
    let mut fallos = Vec::new();
    for (nombre, op) in &ops {
        let db = Db::open_at(&p).unwrap();
        db.conn.lock().execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
        let r = op(&db);
        let vivo = datos_db(&db);
        matar(db);
        let wal = flen(&side(&p, "-wal"));
        let c = comprobar(&raiz, &p, &vivo);
        println!("[p2-cada] {nombre}: op={:?} -wal={wal} B → {c:?}", r.as_ref().err().map(|e| e.to_string()));
        if r.is_err() || c.is_err() {
            fallos.push(format!("{nombre}: op={r:?} → {c:?}"));
        }
    }
    let _ = std::fs::remove_dir_all(&raiz);
    assert!(fallos.is_empty(), "{}", fallos.join("\n"));
}

// ------------------------------------------------------------------ p2-11. -wal con datos y SIN fichero principal

#[test]
fn rompedor_datos_p2_wal_huerfano_sin_base() {
    let raiz = tmp("p2-wal-huerfano");
    let p = raiz.join("pronto2.db");
    let db = Db::open_at(&p).unwrap();
    fill(&db, 20, "volcado");
    db.conn.lock().execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
    fill(&db, 10, "solo en el wal");
    matar(db);
    let wal = std::fs::read(side(&p, "-wal")).unwrap();
    // La base desaparece (antivirus en cuarentena, sincronizador, el usuario la mueve para restaurar otra)
    std::fs::rename(&p, raiz.join("movida.db")).unwrap();
    let r = abrir_como_pronto(&p);
    let conservado = std::fs::read_dir(&raiz)
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("apartado-"))
        .any(|e| std::fs::read(e.path().join("pronto2.db-wal")).map(|w| w == wal).unwrap_or(false));
    let ahora = std::fs::read(side(&p, "-wal")).unwrap_or_default();
    let desc = match &r {
        Ok((a, d, _)) => format!("aviso={a:?} filas={}", d.len()),
        Err(e) => e.clone(),
    };
    println!("[p2-huerfano] -wal original {} B → {desc}; conservado en apartado={conservado}; -wal en su sitio ahora={} B (¿el original? {})", wal.len(), ahora.len(), ahora == wal);
    let _ = std::fs::remove_dir_all(&raiz);
    assert!(conservado || ahora == wal, "el -wal con los últimos cambios desapareció: {desc}");
}
