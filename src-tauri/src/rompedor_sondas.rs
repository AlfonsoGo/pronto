//! Sondas del rompedor (informe `specs/rompedor/2026-10-22.md`). Siempre sobre bases y carpetas
//! temporales y servidores TCP propios en 127.0.0.1: nunca `%APPDATA%\Pronto2` ni la red.
//!
//! Las que demuestran una rotura FALLAN hoy y van con `#[ignore = "rompedor: …"]` para no romper la
//! suite; se quita el `ignore` al arreglarla. Se lanzan con
//! `cargo test rompedor_ -- --ignored --nocapture --test-threads=1`.
//! Las de carga y del motor real solo corren con `PRONTO_ROMPEDOR_CARGA=1` /
//! `PRONTO_ROMPEDOR_MODELO=<carpeta de Parakeet>` + `PRONTO_ROMPEDOR_WAV=<wav 16 kHz>` (solo lectura).

use crate::db::{self, Db, NewHistory};
use crate::models::*;
use rusqlite::{params, Connection};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

// ------------------------------------------------------------ utilidades

/// Carpeta temporal propia que se borra al salir de la sonda.
struct Tmp(PathBuf);

impl Tmp {
    fn new(name: &str) -> Self {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("pronto2-rompedor-{name}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Tmp(dir)
    }
    fn join(&self, p: &str) -> PathBuf {
        self.0.join(p)
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn wal(p: &Path) -> PathBuf {
    PathBuf::from(format!("{}-wal", p.display()))
}

fn shm(p: &Path) -> PathBuf {
    PathBuf::from(format!("{}-shm", p.display()))
}

fn add(db: &Db, text: &str) -> anyhow::Result<HistoryItem> {
    db.add_history(&NewHistory { polished: false, text, raw: text, app: "Code", duration_ms: 1200, words: 3, fixes: Fixes::default(), transform: None })
}

/// 20 dictados, una palabra, un snippet, una nota, una reunión con un turno y ajustes propios.
fn fill(db: &Db) {
    for i in 0..20 {
        add(db, &format!("dictado bueno {i}: presupuesto de camión")).unwrap();
    }
    db.add_word("Txonso", None, &["chonso".to_string()]).unwrap();
    db.add_snippet("mi correo", "alguien@example.com").unwrap();
    db.save_note(None, "nota importante").unwrap();
    let m = db.create_meeting("Reunión buena", Some("Teams")).unwrap();
    db.add_segment(&Segment { meeting_id: m.id, speaker: "me".into(), start_ms: 0, end_ms: 900, text: "cerramos el presupuesto".into(), ..Default::default() }).unwrap();
    let mut s = db.load_settings();
    s.sounds = false;
    db.save_settings(&s).unwrap();
}

fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap_or(-1)
}

fn n_hist(db: &Db) -> i64 {
    count(&db.conn.lock(), "history")
}

fn pct(v: &mut [f64], p: f64) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    if v.is_empty() {
        return 0.0;
    }
    v[((v.len() as f64 - 1.0) * p).round() as usize]
}

/// Servidor HTTP mínimo en 127.0.0.1 (puerto libre). `handler(stream, petición)` por conexión.
fn servir(handler: impl Fn(TcpStream, String) + Send + Sync + 'static) -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let h = Arc::new(handler);
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            let h = h.clone();
            std::thread::spawn(move || {
                let mut s = s;
                let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
                let mut req = Vec::new();
                let mut b = [0u8; 1];
                while !req.ends_with(b"\r\n\r\n") && req.len() < 64 * 1024 {
                    match s.read(&mut b) {
                        Ok(1) => req.push(b[0]),
                        _ => break,
                    }
                }
                h(s, String::from_utf8_lossy(&req).into_owned());
            });
        }
    });
    port
}

/// Escribe `total` bytes (sin `Content-Length`, cierre al final) y cuenta lo que el cliente aceptó.
fn chorro(mut s: TcpStream, total: u64, sent: &AtomicU64) {
    let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\n");
    let mut block = vec![0u8; 64 * 1024];
    block[0] = b'M';
    block[1] = b'Z';
    let mut done = 0u64;
    while done < total {
        let n = (total - done).min(block.len() as u64) as usize;
        if s.write_all(&block[..n]).is_err() {
            return;
        }
        done += n as u64;
        sent.fetch_add(n as u64, Ordering::SeqCst);
        block[0] = 0;
        block[1] = 0;
    }
}

fn hoy() -> String {
    chrono::Local::now().format("%Y%m%d").to_string()
}

// ============================================================ G5 · integridad de la base

/// Incidente del 13-oct: el `-wal` llevaba una base recién creada (páginas con user_version 1→5,
/// change_counter 2, ckseq 0) y el `pronto2.db` tenía los datos buenos. SQLite aplica un `-wal` que
/// no es de esa base (no hay identificador de base en la cabecera del WAL): Pronto arranca «vacío»
/// sin ninguna alarma y, al cerrar, el checkpoint vuelca la base vacía sobre la buena.
#[test]
fn rompedor_wal_ajeno_de_base_vacia_tapa_la_buena() {
    let t = Tmp::new("wal-ajeno");
    std::fs::create_dir_all(t.join("buena")).unwrap();
    std::fs::create_dir_all(t.join("vacia")).unwrap();
    let good = t.join("buena").join("pronto2.db");
    let antes = {
        let db = Db::open_at(&good).unwrap();
        fill(&db);
        n_hist(&db)
    };
    assert!(!wal(&good).exists(), "al cerrar se vuelca y se borra el -wal");
    // Otro arranque crea una base nueva (vacía) y su -wal queda junto a la buena sin volcar.
    let empty = t.join("vacia").join("pronto2.db");
    let db_vacia = Db::open_at(&empty).unwrap();
    std::fs::copy(wal(&empty), wal(&good)).unwrap();
    drop(db_vacia);

    // Arranque normal de Pronto sobre la buena + ese -wal.
    let db = Db::open_at(&good).unwrap();
    let visto = n_hist(&db);
    let ajustes_vistos = db.load_settings().sounds;
    drop(db); // cierre limpio (actualizar, apagar): checkpoint del -wal ajeno sobre la buena
    let tras_cerrar = {
        let c = Connection::open(&good).unwrap();
        count(&c, "history")
    };
    eprintln!("ROMPEDOR wal_ajeno: dictados antes={antes} vistos_al_abrir={visto} sounds_visto={ajustes_vistos} tras_cerrar(main)={tras_cerrar}");
    assert_eq!(visto, antes, "Pronto abrió la base buena y vio {visto} dictados de {antes}: el -wal ajeno la tapa sin aviso");
    assert_eq!(tras_cerrar, antes, "tras cerrar, el pronto2.db quedó con {tras_cerrar} dictados de {antes}: pérdida definitiva");
}

/// La guarda de `daily_backup` cuenta la fila de ajustes como «datos»: una base que solo tiene
/// ajustes (base vacía tras un incidente + el usuario toca un ajuste) se copia cada día y rota
/// las copias buenas. En 7 días no queda ninguna.
#[test]
fn rompedor_copia_diaria_de_base_solo_ajustes_borra_las_buenas() {
    let t = Tmp::new("copias");
    let copias = t.join("copias");
    std::fs::create_dir_all(&copias).unwrap();
    let buena = Db::open_at(&t.join("buena.db")).unwrap();
    fill(&buena);
    let mut buenas = Vec::new();
    for d in 1..=7 {
        let p = copias.join(format!("pronto2-2026090{d}.db"));
        buena.conn.lock().execute("VACUUM INTO ?1", params![p.to_string_lossy()]).unwrap();
        buenas.push(p);
    }
    drop(buena);
    // Base vacía (incidente) en la que el usuario solo ha cambiado un ajuste.
    let vacia = Db::open_at(&t.join("vacia.db")).unwrap();
    vacia.save_settings(&Settings::default()).unwrap();
    // 7 arranques en 7 días (la copia del día se renombra para simular el cambio de fecha).
    for dia in 1..=7 {
        let hecha = vacia.daily_backup(&copias, 7).unwrap();
        if let Some(p) = hecha {
            std::fs::rename(&p, copias.join(format!("pronto2-{}-d{dia}.db", hoy()))).unwrap();
        }
    }
    let quedan = buenas.iter().filter(|p| p.exists()).count();
    eprintln!("ROMPEDOR copias: copias buenas que quedan tras 7 días con base solo-ajustes = {quedan} de 7");
    assert!(quedan > 0, "las 7 copias buenas desaparecieron, sustituidas por copias de una base sin dictados");
}

/// Una copia del día a medio escribir (corte de luz o disco lleno durante `VACUUM INTO`) ya
/// «existe»: no se rehace ni se comprueba, y ocupa un hueco de los 7.
#[test]
fn rompedor_copia_del_dia_danada_no_se_rehace() {
    let t = Tmp::new("copia-danada");
    let copias = t.join("copias");
    std::fs::create_dir_all(&copias).unwrap();
    let db = Db::open_at(&t.join("pronto2.db")).unwrap();
    fill(&db);
    let hoy_p = copias.join(format!("pronto2-{}.db", hoy()));
    std::fs::write(&hoy_p, b"SQLite format 3\0").unwrap(); // VACUUM INTO cortado
    let r = db.daily_backup(&copias, 7).unwrap();
    let abre = Connection::open(&hoy_p).and_then(|c| c.query_row("SELECT COUNT(*) FROM history", [], |r| r.get::<_, i64>(0)));
    eprintln!("ROMPEDOR copia_danada: daily_backup={r:?} copia_de_hoy_abre={abre:?}");
    assert!(matches!(abre, Ok(20)), "la copia del día no se puede usar para restaurar: {abre:?}");
}

/// La rotación deja exactamente 7 copias (las más recientes).
#[test]
fn rompedor_rotacion_conserva_siete() {
    let t = Tmp::new("rotacion");
    let copias = t.join("copias");
    std::fs::create_dir_all(&copias).unwrap();
    for d in 1..=9 {
        std::fs::write(copias.join(format!("pronto2-2026090{d}.db")), b"x").unwrap();
    }
    let db = Db::open_at(&t.join("pronto2.db")).unwrap();
    fill(&db);
    assert!(db.daily_backup(&copias, 7).unwrap().is_some());
    let n = std::fs::read_dir(&copias).unwrap().count();
    assert_eq!(n, 7);
    assert!(!copias.join("pronto2-20260901.db").exists() && copias.join(format!("pronto2-{}.db", hoy())).exists());
}

/// Otro proceso tiene la base ocupada 500 ms (DB Browser, un script de copia): guardar el dictado
/// espera (rusqlite pone `busy_timeout` de 5 s al abrir) y se guarda.
#[test]
fn rompedor_guardar_con_base_ocupada_espera() {
    let t = Tmp::new("ocupada");
    let path = t.join("pronto2.db");
    let db = Db::open_at(&path).unwrap();
    fill(&db);
    let otro = Connection::open(&path).unwrap();
    otro.execute_batch("BEGIN IMMEDIATE; INSERT INTO notes (text, created_at, updated_at) VALUES ('otro proceso', 'a', 'a');").unwrap();
    let suelta = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(500));
        otro.execute_batch("COMMIT").unwrap();
    });
    let t0 = Instant::now();
    let r = add(&db, "dictado mientras otro proceso escribe");
    let ms = t0.elapsed().as_millis();
    suelta.join().unwrap();
    eprintln!("ROMPEDOR ocupada: add_history={:?} en {ms} ms (la base estuvo ocupada 500 ms)", r.as_ref().map(|i| i.id).map_err(|e| e.to_string()));
    assert!(r.is_ok(), "el dictado no se guardó ({ms} ms): {}", r.unwrap_err());
}

/// Arranque con la base cogida 1 s por otro proceso (fichero abierto sin compartir, o conexión con
/// `locking_mode=EXCLUSIVE`): `Db::open` espera y abre.
#[test]
fn rompedor_arranque_con_base_cogida_espera() {
    use std::os::windows::fs::OpenOptionsExt;
    let t = Tmp::new("arranque-cogida");
    let path = t.join("pronto2.db");
    {
        let db = Db::open_at(&path).unwrap();
        fill(&db);
    }
    // (a) fichero abierto sin compartir durante 1 s.
    let h = std::fs::OpenOptions::new().read(true).share_mode(0).open(&path).unwrap();
    let suelta = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(1));
        drop(h);
    });
    let t0 = Instant::now();
    let a = Db::open_at(&path).map(|_| ()).map_err(|e| format!("{e:#}"));
    let ms_a = t0.elapsed().as_millis();
    suelta.join().unwrap();
    // (b) otra conexión con locking_mode=EXCLUSIVE durante 1 s.
    let otro = Connection::open(&path).unwrap();
    otro.execute_batch("PRAGMA locking_mode = EXCLUSIVE; BEGIN EXCLUSIVE; UPDATE settings SET json = json WHERE id = 1;").unwrap();
    let suelta = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(1));
        otro.execute_batch("COMMIT").unwrap();
        drop(otro);
    });
    let t0 = Instant::now();
    let b = Db::open_at(&path).map(|_| ()).map_err(|e| format!("{e:#}"));
    let ms_b = t0.elapsed().as_millis();
    suelta.join().unwrap();
    eprintln!("ROMPEDOR arranque_cogida: (a) sin compartir 1 s -> {a:?} en {ms_a} ms; (b) EXCLUSIVE 1 s -> {b:?} en {ms_b} ms");
    assert!(a.is_ok() && b.is_ok(), "Db::open falló al instante con la base cogida 1 s: a={a:?} b={b:?}");
}

/// Dos arranques a la vez sobre una base que aún no existe (primer arranque, o dos procesos sin el
/// candado de instancia única, como la 2.0.2): la migración no es atómica en conjunto.
#[test]
fn rompedor_dos_arranques_a_la_vez_sobre_base_nueva() {
    let t = Tmp::new("dos-arranques");
    let (mut errores, mut rotas, mut versiones_raras) = (0, 0, 0);
    let mut ejemplo = String::new();
    for i in 0..60 {
        let path = t.join(&format!("p{i}.db"));
        let barrier = Arc::new(Barrier::new(2));
        let hilos: Vec<_> = (0..2)
            .map(|_| {
                let (p, b) = (path.clone(), barrier.clone());
                std::thread::spawn(move || {
                    b.wait();
                    Db::open_at(&p).map(|_| ()).map_err(|e| format!("{e:#}"))
                })
            })
            .collect();
        for h in hilos {
            if let Err(e) = h.join().unwrap() {
                errores += 1;
                if ejemplo.is_empty() {
                    ejemplo = e;
                }
            }
        }
        match Db::open_at(&path) {
            Ok(db) => {
                let v: i64 = db.conn.lock().query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
                let tr = count(&db.conn.lock(), "transforms");
                if v != 10 || tr != 2 { // v10: citas de las notas (decisión A, 23-oct)
                    versiones_raras += 1;
                }
            }
            Err(_) => rotas += 1,
        }
    }
    eprintln!("ROMPEDOR dos_arranques: 60 rondas; aperturas fallidas={errores} (p. ej. {ejemplo:?}); bases que ya no abren={rotas}; versión/transforms raros={versiones_raras}");
    assert_eq!(rotas, 0, "{rotas} bases quedaron sin poder abrirse tras dos arranques simultáneos");
    assert_eq!(versiones_raras, 0);
}

/// `-wal` y `-shm` basura junto a una base buena (restos de otra cosa): se ignoran sin perder nada.
/// Y un `-wal` cortado a la mitad (corte de luz): se abre con un prefijo coherente.
#[test]
fn rompedor_wal_shm_basura_y_wal_cortado() {
    let t = Tmp::new("wal-basura");
    let path = t.join("pronto2.db");
    {
        let db = Db::open_at(&path).unwrap();
        fill(&db);
    }
    let basura: Vec<u8> = (0..65_536u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8).collect();
    std::fs::write(wal(&path), &basura).unwrap();
    std::fs::write(shm(&path), &basura[..32_768]).unwrap();
    let db = Db::open_at(&path).unwrap();
    assert_eq!(n_hist(&db), 20, "un -wal basura cambió lo que se ve");
    let ok: String = db.conn.lock().query_row("PRAGMA integrity_check", [], |r| r.get(0)).unwrap();
    assert_eq!(ok, "ok");
    // WAL vivo copiado a mitad (simula apagón con dictados sin volcar).
    for i in 0..200 {
        add(&db, &format!("sin volcar {i}")).unwrap();
    }
    let copia = t.join("apagon");
    std::fs::create_dir_all(&copia).unwrap();
    let main2 = copia.join("pronto2.db");
    std::fs::copy(&path, &main2).unwrap();
    let w = std::fs::read(wal(&path)).unwrap();
    std::fs::write(wal(&main2), &w[..w.len() / 2]).unwrap();
    drop(db);
    let db2 = Db::open_at(&main2).unwrap();
    let n = n_hist(&db2);
    let ok: String = db2.conn.lock().query_row("PRAGMA integrity_check", [], |r| r.get(0)).unwrap();
    eprintln!("ROMPEDOR wal_cortado: dictados visibles tras apagón con -wal a medias = {n} (20 volcados + hasta 200 sin volcar)");
    assert_eq!(ok, "ok");
    assert!((20..=220).contains(&n));
}

/// Disco lleno simulado (`max_page_count`): la escritura falla limpia y la base sigue íntegra.
#[test]
fn rompedor_disco_lleno_falla_limpio() {
    let t = Tmp::new("lleno");
    let db = Db::open_at(&t.join("pronto2.db")).unwrap();
    fill(&db);
    let pages: i64 = db.conn.lock().query_row("PRAGMA page_count", [], |r| r.get(0)).unwrap();
    db.conn.lock().execute_batch(&format!("PRAGMA max_page_count = {pages}")).unwrap();
    let grande = "x".repeat(2_000_000);
    let r = add(&db, &grande);
    eprintln!("ROMPEDOR disco_lleno: add_history -> {:?}", r.as_ref().map(|i| i.id).map_err(|e| e.to_string()));
    assert!(r.is_err());
    db.conn.lock().execute_batch("PRAGMA max_page_count = 1073741823").unwrap();
    assert_eq!(n_hist(&db), 20);
    let ok: String = db.conn.lock().query_row("PRAGMA integrity_check", [], |r| r.get(0)).unwrap();
    assert_eq!(ok, "ok");
}

/// Base de una versión más nueva (user_version 10, columna extra): una vuelta atrás abre y escribe.
#[test]
fn rompedor_base_de_version_futura_abre() {
    let t = Tmp::new("futura");
    let path = t.join("pronto2.db");
    {
        let db = Db::open_at(&path).unwrap();
        fill(&db);
        db.conn.lock().execute_batch("ALTER TABLE history ADD COLUMN futuro TEXT; PRAGMA user_version = 10;").unwrap();
    }
    let db = Db::open_at(&path).unwrap();
    assert_eq!(n_hist(&db), 20);
    add(&db, "después de volver a la 2.2").unwrap();
    assert_eq!(n_hist(&db), 21);
}

/// Tras una base vacía (incidente del 13-oct) los id vuelven a empezar en 1. Hasta la 2.2.0 el WAV
/// de un dictado era `audio\{id}.wav` (y el de una reunión `meeting_{id}_me.wav`): el dictado 1 de
/// la base nueva sobrescribía el audio del dictado 1 de la buena (`fs::write`; las reuniones,
/// `File::create`). Al restaurar la buena, «Reintentar transcripción» transcribía audio ajeno.
/// Ahora: cada fila tiene su fichero, nunca se sobrescribe uno existente y el historial lee el suyo.
#[test]
fn rompedor_base_nueva_reutiliza_nombres_de_wav() {
    let t = Tmp::new("ids");
    let buena = Db::open_at(&t.join("buena.db")).unwrap();
    fill(&buena);
    let bueno = buena.list_history(None, 1000, 0).unwrap().last().unwrap().clone();
    let reunion_buena = buena.list_meetings(None, 10, 0).unwrap()[0].meeting.clone();
    let audio_bueno = vec![0.25f32; 1600];
    assert!(crate::state::save_audio(&buena, 14, &bueno, &audio_bueno), "no se guardó el audio bueno");
    let vacia = Db::open_at(&t.join("vacia.db")).unwrap();
    let nuevo = add(&vacia, "primer dictado tras el incidente").unwrap();
    assert_eq!(nuevo.id, bueno.id, "precondición: la base nueva reutiliza el id");
    assert!(crate::state::save_audio(&vacia, 14, &nuevo, &vec![-0.5f32; 800]), "no se guardó el audio nuevo");
    let reunion_nueva = vacia.create_meeting("", None).unwrap();
    assert_eq!(reunion_nueva.id, reunion_buena.id, "precondición: la reunión nueva reutiliza el id");
    let p_bueno = db::existing_audio_path(bueno.id, &bueno.created_at).expect("el dictado bueno perdió su WAV");
    let p_nuevo = db::existing_audio_path(nuevo.id, &nuevo.created_at).expect("el dictado nuevo no tiene WAV");
    let leido = crate::audio::decode_wav(&std::fs::read(&p_bueno).unwrap()).unwrap();
    let (m_bueno, m_nuevo) = (crate::meeting::audio_file(&reunion_buena, "me").unwrap(), crate::meeting::audio_file(&reunion_nueva, "me").unwrap());
    eprintln!("ROMPEDOR ids: dictado bueno {} -> {}; dictado nuevo {} -> {}; reunión {} -> {} / {}", bueno.id, p_bueno.display(), nuevo.id, p_nuevo.display(), reunion_nueva.id, m_bueno.display(), m_nuevo.display());
    assert_ne!(p_nuevo, p_bueno, "el WAV del dictado nuevo es el del dictado {} de la base buena", bueno.id);
    assert_eq!(leido.len(), audio_bueno.len(), "el audio del dictado bueno fue sobrescrito");
    assert_ne!(m_nuevo, m_bueno, "la reunión nueva graba en el WAV de la reunión {} de la base buena", reunion_buena.id);
    // Aunque el nombre coincidiera (otra copia de la misma base), nunca se sobrescribe: se aparta.
    std::fs::write(&m_nuevo, b"audio de otra reunion").unwrap();
    drop(crate::audio::WavStream::create_new(&m_nuevo).unwrap());
    let apartado = std::fs::read_dir(m_nuevo.parent().unwrap())
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("apartado-"))
        .flat_map(|e| std::fs::read_dir(e.path()).unwrap().flatten())
        .any(|f| std::fs::read(f.path()).is_ok_and(|b| b == b"audio de otra reunion"));
    assert!(apartado, "el WAV que ya existía se perdió en vez de apartarse");
    let _ = std::fs::remove_file(&m_nuevo);
}

/// Ya conocido (riesgo de `load_settings`), confirmado: UN campo con un valor que esta versión no
/// entiende (p. ej. de una versión más nueva) devuelve TODOS los ajustes de fábrica, y el siguiente
/// guardado los machaca.
#[test]
fn rompedor_ajustes_un_campo_raro_resetea_todo() {
    let db = Db::open_memory().unwrap();
    let mut s = Settings::default();
    s.sounds = false;
    s.keep_audio_days = 3;
    let mut v = serde_json::to_value(&s).unwrap();
    v["cleanupLevel"] = serde_json::json!("extremo");
    db.conn.lock().execute("INSERT INTO settings (id, json) VALUES (1, ?1)", params![v.to_string()]).unwrap();
    let leidos = db.load_settings();
    db.save_settings(&leidos).unwrap(); // cualquier cambio en el hub
    let guardado: String = db.conn.lock().query_row("SELECT json FROM settings", [], |r| r.get(0)).unwrap();
    eprintln!("ROMPEDOR ajustes: guardados sounds=false keepAudioDays=3; leídos sounds={} keepAudioDays={}; tras guardar, ¿sigue sounds=false en la base? {}", leidos.sounds, leidos.keep_audio_days, guardado.contains("\"sounds\":false"));
    assert!(!leidos.sounds && leidos.keep_audio_days == 3, "un campo desconocido devolvió todos los ajustes de fábrica");
}

// ============================================================ entradas hostiles y carga

/// Catálogo §3: lo que se guarda se relee igual; las búsquedas no fallan con nada.
#[test]
fn rompedor_textos_hostiles_se_guardan_y_buscan() {
    let db = Db::open_memory().unwrap();
    fill(&db);
    let raros = [
        "".to_string(),
        " ".to_string(),
        "con\0nulo".to_string(),
        "'; DROP TABLE history; --".to_string(),
        "%_\\ comodines".to_string(),
        "مرحبا \u{202e}txet\u{202c} 👩‍👩‍👧 ñandú".to_string(),
        "<img src=x onerror=alert(1)>".to_string(),
        "x".repeat(1_000_000),
    ];
    for r in &raros {
        let it = add(&db, r).unwrap();
        assert_eq!(&db.get_history(it.id).unwrap().unwrap().text, r, "no se releyó igual");
        if r.len() < 50_000 {
            assert!(db.list_history(Some(r), 50, 0).is_ok(), "list_history falló con {:?}", r.chars().take(40).collect::<String>());
        }
        let m = db.create_meeting(r, Some(r)).unwrap();
        assert!(db.get_meeting(m.id).unwrap().is_some());
    }
    let largo = "palabra ".repeat(5000);
    for q in ["\"", "*", "NEAR(", "^a", "a OR", ")", "\0", "👩‍👩‍👧", "-", ":", "'", "a\"b*c(d)", largo.as_str()] {
        assert!(db.list_meetings(Some(q), 20, 0).is_ok(), "list_meetings falló con {q:?}");
        assert!(db.rank_meetings(q, 5).is_ok(), "rank_meetings falló con {q:?}");
        assert!(db.list_history(Some(q), 20, -5).is_ok());
    }
    assert!(db.list_history(None, i64::MAX, i64::MAX).is_ok());
    assert!(db.purge_history(u32::MAX).is_ok(), "purge_history con u32::MAX días");
    assert!(db.purge_audio(u32::MAX).is_ok());
    assert!(db.add_word_variant(999_999, "x").is_err());
    assert!(db.add_word_sample(1, "..\\..\\pronto2.db").is_err());
    assert!(db.add_word_sample(1, "pendiente-..wav").is_err());
}

/// Buscar en el historial un texto de más de 50 000 bytes (pegar un documento en el buscador del
/// hub): el `LIKE` de SQLite lo rechaza («LIKE or GLOB pattern too complex») y el hub muestra error.
#[test]
fn rompedor_buscar_texto_largo_en_historial() {
    let db = Db::open_memory().unwrap();
    fill(&db);
    let q = "presupuesto ".repeat(5_000);
    let r = db.list_history(Some(&q), 50, 0);
    eprintln!("ROMPEDOR busqueda_larga: {} bytes -> {:?}", q.len(), r.as_ref().map(|v| v.len()).map_err(|e| e.to_string()));
    assert!(r.is_ok(), "la búsqueda falló: {}", r.unwrap_err());
}

/// WAV dañados (dictado o reunión cortados, ficheros truncados): ni pánico ni bucle.
#[test]
fn rompedor_wav_danados_no_cuelgan() {
    let t = Tmp::new("wav");
    let bueno = crate::audio::encode_wav(&vec![0.1f32; 16_000]);
    let mut casos: Vec<(String, Vec<u8>)> = vec![
        ("vacio".into(), vec![]),
        ("solo_riff".into(), b"RIFF\0\0\0\0WAVE".to_vec()),
        ("cabecera".into(), bueno[..44].to_vec()),
        ("cortado_impar".into(), bueno[..1001].to_vec()),
        ("data_enorme".into(), {
            let mut b = bueno.clone();
            b[40..44].copy_from_slice(&u32::MAX.to_le_bytes());
            b
        }),
        ("chunk_enorme".into(), {
            let mut b = bueno[..12].to_vec();
            b.extend_from_slice(b"LIST");
            b.extend_from_slice(&u32::MAX.to_le_bytes());
            b
        }),
        ("fmt_corto".into(), {
            let mut b = bueno[..12].to_vec();
            b.extend_from_slice(b"fmt ");
            b.extend_from_slice(&2u32.to_le_bytes());
            b.extend_from_slice(&[1, 0]);
            b.extend_from_slice(b"data");
            b.extend_from_slice(&4u32.to_le_bytes());
            b.extend_from_slice(&[1, 2, 3, 4]);
            b
        }),
        ("rate_cero".into(), {
            let mut b = bueno.clone();
            b[24..28].copy_from_slice(&0u32.to_le_bytes());
            b
        }),
    ];
    casos.push(("basura".into(), (0..5000u32).map(|i| (i * 7919 % 251) as u8).collect()));
    for (name, bytes) in casos {
        let p = t.join(&format!("{name}.wav"));
        std::fs::write(&p, &bytes).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let p2 = p.clone();
        std::thread::spawn(move || {
            let d = std::panic::catch_unwind(|| crate::audio::decode_wav(&bytes).map(|s| s.len()));
            let r = std::panic::catch_unwind(|| {
                crate::audio::WavReader::open(&p2).map(|mut r| {
                    let mut n = 0usize;
                    for _ in 0..1000 {
                        let b = r.read_block(4096).unwrap_or_default();
                        if b.is_empty() {
                            break;
                        }
                        n += b.len();
                    }
                    n
                })
            });
            let _ = tx.send((d.is_ok(), r.is_ok()));
        });
        let (d_ok, r_ok) = rx.recv_timeout(Duration::from_secs(5)).unwrap_or_else(|_| panic!("{name}: se colgó"));
        assert!(d_ok && r_ok, "{name}: pánico (decode_wav ok={d_ok}, WavReader ok={r_ok})");
    }
}

/// Historial de 100 000 dictados: tiempos del hub y de guardar un dictado mientras el hub busca.
#[test]
fn rompedor_carga_historial_100k() {
    if std::env::var("PRONTO_ROMPEDOR_CARGA").is_err() {
        println!("ROMPEDOR_OMITIDO: falta PRONTO_ROMPEDOR_CARGA=1");
        return;
    }
    let t = Tmp::new("carga");
    let db = Arc::new(Db::open_at(&t.join("pronto2.db")).unwrap());
    fill(&db);
    let t0 = Instant::now();
    let mut ins = Vec::new();
    for i in 0..100_000 {
        let a = Instant::now();
        add(&db, &format!("dictado {i} sobre el presupuesto del proyecto número {} con varias palabras más para que pese", i % 977)).unwrap();
        ins.push(a.elapsed().as_secs_f64() * 1000.0);
    }
    let total_ins = t0.elapsed().as_secs_f64();
    let medir = |f: &dyn Fn()| {
        let mut v = Vec::new();
        for _ in 0..15 {
            let a = Instant::now();
            f();
            v.push(a.elapsed().as_secs_f64() * 1000.0);
        }
        (pct(&mut v.clone(), 0.5), pct(&mut v, 0.95))
    };
    let pagina = medir(&|| {
        db.list_history(None, 50, 0).unwrap();
    });
    let busca = medir(&|| {
        db.list_history(Some("zzz-no-está"), 50, 0).unwrap();
    });
    let stats = medir(&|| {
        db.stats().unwrap();
    });
    let insights = medir(&|| {
        db.insights().unwrap();
    });
    let copias = t.join("copias");
    std::fs::create_dir_all(&copias).unwrap();
    let a = Instant::now();
    db.daily_backup(&copias, 7).unwrap();
    let backup_ms = a.elapsed().as_secs_f64() * 1000.0;
    // Guardar dictados mientras el hub busca sin parar (mismo mutex de conexión).
    let parar = Arc::new(AtomicBool::new(false));
    let (d2, p2) = (db.clone(), parar.clone());
    let hub = std::thread::spawn(move || {
        let mut n = 0;
        while !p2.load(Ordering::SeqCst) {
            d2.list_history(Some("presupuesto 9"), 50, 0).unwrap();
            d2.insights().unwrap();
            n += 1;
        }
        n
    });
    let mut bajo_carga = Vec::new();
    for i in 0..40 {
        std::thread::sleep(Duration::from_millis(37));
        let a = Instant::now();
        add(&db, &format!("dictado bajo carga {i}")).unwrap();
        bajo_carga.push(a.elapsed().as_secs_f64() * 1000.0);
    }
    parar.store(true, Ordering::SeqCst);
    let vueltas = hub.join().unwrap();
    let a = Instant::now();
    db.purge_audio(14).unwrap();
    let purge_ms = a.elapsed().as_secs_f64() * 1000.0;
    println!(
        "ROMPEDOR_CARGA insertar 100k: {total_ins:.1} s (p50 {:.2} ms, p95 {:.2} ms) | página hub p50/p95 {:.1}/{:.1} ms | búsqueda sin resultado {:.1}/{:.1} ms | stats {:.1}/{:.1} ms | insights {:.1}/{:.1} ms | copia diaria {backup_ms:.0} ms | purge_audio {purge_ms:.1} ms | guardar dictado con hub buscando p50/p95/max {:.1}/{:.1}/{:.1} ms ({vueltas} vueltas del hub)",
        pct(&mut ins.clone(), 0.5),
        pct(&mut ins, 0.95),
        pagina.0, pagina.1, busca.0, busca.1, stats.0, stats.1, insights.0, insights.1,
        pct(&mut bajo_carga.clone(), 0.5),
        pct(&mut bajo_carga.clone(), 0.95),
        bajo_carga.iter().cloned().fold(0.0, f64::max),
    );
}

// ============================================================ actualizador contra servidor hostil

fn agente_como_el_actualizador() -> ureq::Agent {
    // Misma configuración que `updater::agent()` (privada): conexión 15 s, lectura 30 s.
    ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(15)).timeout_read(Duration::from_secs(30)).build()
}

/// Release con asset de 1 KB; el servidor manda 48 MB. `fetch_installer` no corta en el tamaño
/// publicado: lo baja todo a `%TEMP%` y solo después compara (disco lleno con un servidor hostil).
#[test]
fn rompedor_actualizador_descarga_sin_tope() {
    let t = Tmp::new("upd-tope");
    let sent = Arc::new(AtomicU64::new(0));
    let s2 = sent.clone();
    let port = servir(move |s, _| chorro(s, 48 * 1024 * 1024, &s2));
    let asset = crate::updater::Asset { name: "Pronto-Setup.exe".into(), url: format!("http://127.0.0.1:{port}/a.exe"), size: 1024, sha256: None };
    let mut max_part = 0u64;
    let r = crate::updater::fetch_installer(&agente_como_el_actualizador(), &asset, &t.0, &mut |_, done, _| max_part = max_part.max(done));
    let enviado = sent.load(Ordering::SeqCst);
    eprintln!("ROMPEDOR upd_tope: asset 1024 B; aceptados {enviado} B; escritos en .part {max_part} B; resultado {r:?}");
    assert!(r.is_err());
    assert!(max_part <= 1024 + (1 << 20), "escribió {max_part} B en disco para un asset de 1 KB");
}

/// Servidor que gotea 1 byte/s: no hay plazo total, la descarga queda «Descargando…» para siempre
/// (INSTALLING bloquea reintentos y comprobaciones).
#[test]
fn rompedor_actualizador_goteo_sin_plazo() {
    let t = Tmp::new("upd-goteo");
    let parar = Arc::new(AtomicBool::new(false));
    let p2 = parar.clone();
    let port = servir(move |mut s, _| {
        let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10000000\r\n\r\nMZ");
        while !p2.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_secs(1));
            if s.write_all(b"\0").is_err() {
                return;
            }
        }
    });
    let asset = crate::updater::Asset { name: "Pronto-Setup.exe".into(), url: format!("http://127.0.0.1:{port}/a.exe"), size: 10_000_000, sha256: None };
    let dir = t.0.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let r = crate::updater::fetch_installer(&agente_como_el_actualizador(), &asset, &dir, &mut |_, _, _| {});
        let _ = tx.send(r.is_ok());
    });
    let fin = rx.recv_timeout(Duration::from_secs(45));
    parar.store(true, Ordering::SeqCst);
    let _ = rx.recv_timeout(Duration::from_secs(5));
    eprintln!("ROMPEDOR upd_goteo: ¿terminó en 45 s? {fin:?}");
    assert!(fin.is_ok(), "fetch_installer sigue descargando tras 45 s a 1 byte/s (sin plazo total)");
}

/// `fetch_latest` contra respuestas hostiles: JSON de 5 MB, servidor mudo, tags y nombres raros.
#[test]
fn rompedor_actualizador_respuestas_hostiles() {
    use crate::updater::{fetch_latest, parse_release, parse_version, url_allowed};
    // Servidor mudo: acepta y no contesta.
    let mudo = servir(|s, _| {
        std::thread::sleep(Duration::from_secs(40));
        drop(s);
    });
    let h = std::thread::spawn(move || {
        let a = Instant::now();
        let r = fetch_latest(&format!("http://127.0.0.1:{mudo}/latest"));
        (a.elapsed(), r.is_err())
    });
    // 5 MB de JSON.
    let gordo = servir(|mut s, _| {
        let _ = s.write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{\"tag_name\":\"v9.9.9\",\"body\":\"");
        let _ = s.write_all(&vec![b'a'; 5 * 1024 * 1024]);
        let _ = s.write_all(b"\"}");
    });
    assert!(fetch_latest(&format!("http://127.0.0.1:{gordo}/latest")).is_err());
    // Tags y assets raros.
    assert!(parse_version("v99999999999999999999999.0.0").is_none());
    let j = |name: &str, url: &str| serde_json::json!({"tag_name": "v9.0.0", "assets": [{"name": name, "size": 10, "browser_download_url": url}]}).to_string();
    let r = parse_release(&j("..\\..\\..\\Windows\\evil.exe", "https://github.com/x")).unwrap().unwrap();
    assert_eq!(r.asset.name, "evil.exe");
    let r = parse_release(&j("a.exe\0.txt.exe", "https://github.com/x")).unwrap().unwrap();
    assert!(!r.asset.name.contains('\0'));
    for u in ["https://github.com.evil.test/x", "https://evil.test\\@github.com/x", "https://github.com@evil.test/x", "http://github.com/x", "file:///C:/x.exe", "https://evil.test/github.com", "HTTPS://github.com/x"] {
        assert!(!url_allowed(u, false) || u.starts_with("HTTPS"), "url_allowed aceptó {u}");
    }
    let (t, err) = h.join().unwrap();
    eprintln!("ROMPEDOR upd_hostil: servidor mudo -> error={err} en {:.1} s", t.as_secs_f32());
    assert!(err && t < Duration::from_secs(25), "fetch_latest no respetó su plazo con un servidor mudo");
}

// ============================================================ descarga del modelo (servidor local)

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

fn modelo(name: &str, bytes: &[u8]) -> crate::modeldl::ModelFile {
    crate::modeldl::ModelFile { name: leak(name.into()), sha256: leak(crate::modeldl::sha256_hex(bytes).unwrap()), size: bytes.len() as u64 }
}

fn contenido(n: usize) -> Vec<u8> {
    (0..n as u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 11) as u8).collect()
}

/// Corte a mitad, bytes dañados del mismo tamaño y 404: error, sin fichero final ni `.part`.
#[test]
fn rompedor_modelo_corte_y_dano_no_dejan_basura() {
    let t = Tmp::new("modelo");
    let bueno = Arc::new(contenido(300_000));
    let f = modelo("encoder.int8.onnx", &bueno);
    for modo in ["corte", "dano", "404"] {
        let b = bueno.clone();
        let m = modo.to_string();
        let port = servir(move |mut s, _| match m.as_str() {
            "corte" => {
                let _ = s.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", b.len()).as_bytes());
                let _ = s.write_all(&b[..b.len() / 2]);
            }
            "dano" => {
                let mut d = b.to_vec();
                d[1234] ^= 0xFF;
                let _ = s.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", d.len()).as_bytes());
                let _ = s.write_all(&d);
            }
            _ => {
                let _ = s.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
            }
        });
        let dir = t.join(modo);
        let r = crate::modeldl::download_all(&dir, &format!("http://127.0.0.1:{port}"), std::slice::from_ref(&f), &mut |_| {});
        assert!(r.is_err(), "{modo}: dio por buena la descarga");
        assert!(!dir.join(f.name).exists(), "{modo}: dejó el fichero final");
        assert!(!dir.join(format!("{}.part", f.name)).exists(), "{modo}: dejó el .part");
    }
}

/// Modelo de 1000 B; el servidor manda 48 MB sin `Content-Length`: se escriben todos antes de fallar.
#[test]
fn rompedor_modelo_descarga_sin_tope() {
    let t = Tmp::new("modelo-tope");
    let f = modelo("tokens.txt", &contenido(1000));
    let sent = Arc::new(AtomicU64::new(0));
    let s2 = sent.clone();
    let port = servir(move |s, _| chorro(s, 48 * 1024 * 1024, &s2));
    let mut max = 0u64;
    let r = crate::modeldl::download_all(&t.0, &format!("http://127.0.0.1:{port}"), std::slice::from_ref(&f), &mut |p| max = max.max(p.done_bytes));
    let enviado = sent.load(Ordering::SeqCst);
    eprintln!("ROMPEDOR modelo_tope: esperado 1000 B; aceptados {enviado} B; progreso máx {max} B; {r:?}");
    assert!(enviado <= 1000 + (1 << 20), "aceptó {enviado} B para un fichero de 1000 B");
}

/// Un fichero del modelo con el tamaño correcto pero bytes dañados se da por bueno para siempre:
/// `file_ok` solo mira el tamaño, ni se re-verifica ni se vuelve a bajar.
#[test]
fn rompedor_modelo_danado_mismo_tamano_se_da_por_bueno() {
    let t = Tmp::new("modelo-mismo");
    let bueno = contenido(50_000);
    let f = modelo("joiner.int8.onnx", &bueno);
    let mut malo = bueno.clone();
    malo[777] ^= 0x55;
    std::fs::write(t.join(f.name), &malo).unwrap();
    let peticiones = Arc::new(AtomicUsize::new(0));
    let (p2, b2) = (peticiones.clone(), bueno.clone());
    let port = servir(move |mut s, _| {
        p2.fetch_add(1, Ordering::SeqCst);
        let _ = s.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", b2.len()).as_bytes());
        let _ = s.write_all(&b2);
    });
    let completo_antes = crate::modeldl::is_complete(&t.0, std::slice::from_ref(&f));
    let r = crate::modeldl::download_all(&t.0, &format!("http://127.0.0.1:{port}"), std::slice::from_ref(&f), &mut |_| {});
    let hash = crate::modeldl::hash_file(&t.join(f.name)).unwrap();
    eprintln!("ROMPEDOR modelo_mismo_tamano: is_complete={completo_antes} download_all={r:?} peticiones={} hash_ok={}", peticiones.load(Ordering::SeqCst), hash == f.sha256);
    assert_eq!(hash, f.sha256, "el fichero dañado sigue ahí y la app lo da por bueno");
}

/// Ficheros del modelo presentes pero dañados en la carpeta del modelo (copia a medias, carpeta
/// elegida a mano, antivirus, disco): cargar el motor debe dar `Err` («¿modelo dañado?»), no tumbar
/// el proceso. sherpa-onnx llama a `exit(-1)` y `catch_unwind` no lo para: como `main.rs` carga el
/// motor en cada arranque (aunque `model_present` sea falso), Pronto muere al iniciar, siempre.
/// Variante con `PRONTO_ROMPEDOR_VARIANTE`: `tokens` (tokens.txt basura, por defecto), `onnx`
/// (tokens reales, onnx basura) o `encoder` (reales salvo el encoder cortado a 8 MB); las dos últimas
/// copian de `PRONTO_ROMPEDOR_MODELO` (solo lectura). Lánzala sola: si aborta, se lleva el proceso.
#[test]
fn rompedor_motor_con_modelo_danado() {
    let t = Tmp::new("motor-danado");
    let variante = std::env::var("PRONTO_ROMPEDOR_VARIANTE").unwrap_or_else(|_| "tokens".into());
    let real = std::env::var("PRONTO_ROMPEDOR_MODELO").ok();
    let copia_real = |n: &str, max: Option<usize>| {
        let src = Path::new(real.as_deref().expect("PRONTO_ROMPEDOR_MODELO")).join(n);
        let mut f = std::fs::File::open(src).unwrap();
        let mut buf = Vec::new();
        match max {
            Some(m) => {
                buf.resize(m, 0);
                f.read_exact(&mut buf).unwrap();
            }
            None => {
                f.read_to_end(&mut buf).unwrap();
            }
        }
        std::fs::write(t.join(n), buf).unwrap();
    };
    match variante.as_str() {
        "onnx" => {
            copia_real("tokens.txt", None);
            for n in ["encoder.int8.onnx", "decoder.int8.onnx", "joiner.int8.onnx"] {
                std::fs::write(t.join(n), contenido(4096)).unwrap();
            }
        }
        "encoder" => {
            for n in ["tokens.txt", "decoder.int8.onnx", "joiner.int8.onnx"] {
                copia_real(n, None);
            }
            copia_real("encoder.int8.onnx", Some(8 << 20));
        }
        _ => {
            for (n, len) in [("encoder.int8.onnx", 4096), ("decoder.int8.onnx", 4096), ("joiner.int8.onnx", 4096), ("tokens.txt", 64)] {
                std::fs::write(t.join(n), contenido(len)).unwrap();
            }
        }
    }
    eprintln!("ROMPEDOR motor_danado[{variante}]: model_present={} · cargando…", crate::modeldl::model_present(&t.0.to_string_lossy()));
    let r = crate::asr::Engine::load_for_tests(&t.0.to_string_lossy());
    eprintln!("ROMPEDOR motor_danado[{variante}]: load -> {:?}", r.as_ref().err());
    assert!(r.is_err());
}

// ============================================================ motor compartido: dictado + reunión

/// Dictado de 15 s mientras la fase 2 de una reunión transcribe trozos de 30 s (`segment::FINAL`,
/// el máximo) sin parar: el dictado espera a que acabe el trozo en curso (`asr.rs` `busy`). Mide la
/// latencia (regla 3: ≤ 1068 ms) y que la reunión avanza (sin bloqueos ni hambre).
#[test]
fn rompedor_motor_dictado_durante_fase2_de_reunion() {
    let (Ok(dir), Ok(wav)) = (std::env::var("PRONTO_ROMPEDOR_MODELO"), std::env::var("PRONTO_ROMPEDOR_WAV")) else {
        println!("ROMPEDOR_OMITIDO: faltan PRONTO_ROMPEDOR_MODELO y PRONTO_ROMPEDOR_WAV");
        return;
    };
    let dictado = crate::audio::decode_wav(&std::fs::read(&wav).unwrap()).unwrap();
    let reunion: Vec<f32> = dictado.iter().cycle().take(30 * 16_000).cloned().collect();
    let engine = Arc::new(crate::asr::Engine::load_for_tests(&dir).expect("cargar Parakeet"));
    // Como en la app: la marca de «dictado en marcha» (`AppState.active`) se pone al pulsar el atajo.
    let pulsado = Arc::new(AtomicBool::new(false));
    engine.share_dictation_flag(pulsado.clone());
    let _ = engine.transcribe_timed(&dictado).unwrap();
    let mut solo = Vec::new();
    for _ in 0..5 {
        let a = Instant::now();
        engine.transcribe_timed(&dictado).unwrap();
        solo.push(a.elapsed().as_secs_f64() * 1000.0);
    }
    let a = Instant::now();
    engine.transcribe_background_timed(&reunion).unwrap();
    let trozo_ms = a.elapsed().as_secs_f64() * 1000.0;
    let parar = Arc::new(AtomicBool::new(false));
    let trozos = Arc::new(AtomicUsize::new(0));
    let (e2, p2, t2) = (engine.clone(), parar.clone(), trozos.clone());
    let fondo = std::thread::spawn(move || {
        while !p2.load(Ordering::SeqCst) {
            e2.transcribe_background_timed(&reunion).unwrap();
            t2.fetch_add(1, Ordering::SeqCst);
        }
    });
    let mut con = Vec::new();
    // Habla `PRONTO_ROMPEDOR_HABLA_MS` (2 s por defecto, para no tardar; el WAV es de 15 s) con el
    // atajo pulsado y suelta: se mide desde que se suelta.
    let habla = std::env::var("PRONTO_ROMPEDOR_HABLA_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(2000u64);
    for i in 0..10 {
        std::thread::sleep(Duration::from_millis(300 + (i * 397 % 1500) as u64));
        pulsado.store(true, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(habla));
        let a = Instant::now();
        engine.transcribe_timed(&dictado).unwrap();
        con.push(a.elapsed().as_secs_f64() * 1000.0);
        pulsado.store(false, Ordering::SeqCst);
    }
    parar.store(true, Ordering::SeqCst);
    fondo.join().unwrap();
    let (s50, c50, c95) = (pct(&mut solo.clone(), 0.5), pct(&mut con.clone(), 0.5), pct(&mut con.clone(), 0.95));
    let cmax = con.iter().cloned().fold(0.0, f64::max);
    println!("ROMPEDOR_MOTOR dictado 15 s solo p50 {s50:.0} ms | trozo de reunión de 30 s {trozo_ms:.0} ms | dictado con fase 2 en marcha p50/p95/max {c50:.0}/{c95:.0}/{cmax:.0} ms | trozos de reunión hechos {}", trozos.load(Ordering::SeqCst));
    assert!(trozos.load(Ordering::SeqCst) > 0, "la reunión no avanzó (hambre)");
    assert!(c95 <= 1068.0, "dictado con la fase 2 en marcha: p95 {c95:.0} ms > 1068 ms (regla 3)");
}

/// Audio vacío, de una muestra, con NaN/infinitos o saturado (micro que falla, WAV dañado): el motor
/// real devuelve texto (vacío o no) sin pánico ni aborto.
#[test]
fn rompedor_motor_audios_raros() {
    let Ok(dir) = std::env::var("PRONTO_ROMPEDOR_MODELO") else {
        println!("ROMPEDOR_OMITIDO: falta PRONTO_ROMPEDOR_MODELO");
        return;
    };
    let engine = crate::asr::Engine::load_for_tests(&dir).expect("cargar Parakeet");
    let casos: Vec<(&str, Vec<f32>)> = vec![
        ("vacio", vec![]),
        ("una", vec![0.5]),
        ("nan", vec![f32::NAN; 16_000]),
        ("inf", (0..16_000).map(|i| if i % 2 == 0 { f32::INFINITY } else { f32::NEG_INFINITY }).collect()),
        ("saturado", vec![1.0; 32_000]),
        ("silencio_61s", vec![0.0; 61 * 16_000]),
    ];
    for (n, s) in casos {
        let a = Instant::now();
        let r = engine.transcribe_timed(&s);
        eprintln!("ROMPEDOR motor_raro[{n}]: {:?} en {} ms", r.as_ref().map(|t| t.text.chars().take(40).collect::<String>()), a.elapsed().as_millis());
        assert!(r.is_ok(), "{n}: {:?}", r.err());
    }
}

// ============================================================ segunda pasada (informe 2026-10-22-b)
//
// Contra los arreglos de 7928069: guarda del -wal ajeno, marca `.verificado`, `set_aside`/`write_new`,
// copia diaria con `-sospechosa`, ajustes campo a campo y la fase 2 que cede el motor al dictado.

/// Copia `src` (y su `-wal`/`-shm` si existen) a `dir` mientras la conexión sigue abierta: es lo que
/// queda en disco si el proceso muere (apagado, actualización) sin cerrar la base.
fn copia_en_caliente(src: &Path, dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let dst = dir.join("pronto2.db");
    std::fs::copy(src, &dst).unwrap();
    for (a, b) in [(wal(src), wal(&dst)), (shm(src), shm(&dst))] {
        if a.exists() {
            std::fs::copy(a, b).unwrap();
        }
    }
    dst
}

fn apartados(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("apartado-"))).collect())
        .unwrap_or_default()
}

fn solo_lectura(f: &Path, v: bool) {
    if let Ok(m) = std::fs::metadata(f) {
        let mut perm = m.permissions();
        perm.set_readonly(v);
        let _ = std::fs::set_permissions(f, perm);
    }
}

/// El incidente del 13-oct tal cual: la base vacía del `-wal` ajeno YA tenía 4 dictados (ids 1-4,
/// `audio\1.wav`…`4.wav` de las 21:49) cuando se cerró. La guarda solo aparta un `-wal` que enseñe
/// 0 filas (`db.rs` `guard_foreign_wal`: `view_rows > 0 && view_version >= main_version` → se acepta):
/// con un solo dictado en la base ajena, la buena vuelve a quedar tapada y el cierre la machaca.
#[test]
fn rompedor_b_wal_ajeno_con_dictados_pasa_la_guarda() {
    let t = Tmp::new("b-wal-ajeno-dictados");
    let good = t.join("buena").join("pronto2.db");
    std::fs::create_dir_all(good.parent().unwrap()).unwrap();
    let antes = {
        let db = Db::open_at(&good).unwrap();
        fill(&db);
        n_hist(&db)
    };
    let empty = t.join("vacia").join("pronto2.db");
    std::fs::create_dir_all(empty.parent().unwrap()).unwrap();
    let db_vacia = Db::open_at(&empty).unwrap();
    for i in 1..=4 {
        add(&db_vacia, &format!("dictado {i} hecho en la base vacía")).unwrap();
    }
    std::fs::copy(wal(&empty), wal(&good)).unwrap();
    drop(db_vacia);

    let db = Db::open_at(&good).unwrap();
    let visto = n_hist(&db);
    let aviso = db.aviso().map(String::from);
    let sounds = db.load_settings().sounds;
    drop(db);
    let tras_cerrar = count(&Connection::open(&good).unwrap(), "history");
    eprintln!(
        "ROMPEDOR_B wal_ajeno_con_dictados: dictados buenos={antes} vistos_al_abrir={visto} aviso={} sounds_visto={sounds} apartados={} tras_cerrar(main)={tras_cerrar}",
        aviso.is_some(),
        apartados(good.parent().unwrap()).len()
    );
    assert_eq!(visto, antes, "con 4 dictados en la base ajena la guarda no salta: se ven {visto} de {antes}");
    assert_eq!(tras_cerrar, antes, "tras cerrar, pronto2.db quedó con {tras_cerrar} dictados de {antes}");
}

/// Restaurar una copia como dice el aviso de Pronto («restáuralo desde ahí»): con Pronto cerrado, se
/// copia `copias\pronto2-AAAAMMDD.db` encima de `pronto2.db`. Pronto no vuelca al salir, así que el
/// `-wal` de la base que se quiere dejar atrás sigue ahí. (R1) Tras un incidente como el del 13-oct
/// (base nueva en el `-wal`, con 4 dictados): ¿se ve la copia restaurada? (R2) Base normal con
/// cambios sin volcar y se vuelve a la copia de ayer: ¿qué se ve?
#[test]
#[ignore = "rompedor: restaurar una copia encima de pronto2.db con el -wal viejo al lado no restaura nada"]
fn rompedor_b_restaurar_copia_con_wal_viejo_al_lado() {
    let t = Tmp::new("b-restaurar");
    // Copia buena de ayer.
    let copia = t.join("copia.db");
    {
        let db = Db::open_at(&copia).unwrap();
        fill(&db);
    }
    let mut fallos = Vec::new();
    for caso in ["R1_incidente", "R2_base_normal"] {
        let dir = t.join(caso);
        std::fs::create_dir_all(&dir).unwrap();
        let vivo = dir.join("pronto2.db");
        let db = if caso == "R1_incidente" {
            let db = Db::open_at(&vivo).unwrap();
            for i in 1..=4 {
                add(&db, &format!("dictado {i} de la base vacía")).unwrap();
            }
            db
        } else {
            std::fs::copy(&copia, &vivo).unwrap();
            let db = Db::open_at(&vivo).unwrap();
            for i in 0..30 {
                add(&db, &format!("dictado de hoy {i}")).unwrap();
            }
            db
        };
        // Pronto muere sin volcar: queda su -wal. El usuario copia la copia buena encima.
        let quedan = t.join(&format!("{caso}-apagado"));
        let p = copia_en_caliente(&vivo, &quedan);
        drop(db);
        std::fs::copy(&copia, &p).unwrap();
        let r = Db::open_at(&p).map(|d| (n_hist(&d), d.aviso().is_some())).map_err(|e| format!("{e:#}"));
        let tras_cerrar = Connection::open(&p).map(|c| count(&c, "history"));
        eprintln!("ROMPEDOR_B restaurar[{caso}]: copia restaurada con 20 dictados; Pronto ve {r:?}; pronto2.db tras cerrar = {tras_cerrar:?}");
        if r.as_ref().map(|x| x.0) != Ok(20) {
            fallos.push(format!("{caso}: se ve {r:?} en vez de los 20 dictados de la copia"));
        }
    }
    assert!(fallos.is_empty(), "{fallos:?}");
}

/// Un `-wal` BUENO apartado: el usuario solo dicta (sin reuniones, palabras, snippets ni notas),
/// pulsa «Borrar historial» y cambia un ajuste; Pronto se cierra sin volcar (en el PC de Txonso el
/// `pronto2.db` no se vuelca desde el 15-oct: todo lo posterior vive en el `-wal`). Al arrancar, la
/// guarda ve «0 filas» con el `-wal` y lo aparta como ajeno: los dictados borrados vuelven, el ajuste
/// se pierde y el aviso dice «Tus datos están intactos».
#[test]
fn rompedor_b_wal_bueno_con_borrado_legitimo_se_aparta() {
    let t = Tmp::new("b-wal-borrado");
    let path = t.join("vivo").join("pronto2.db");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let db = Db::open_at(&path).unwrap();
    for i in 0..20 {
        add(&db, &format!("dictado privado {i}")).unwrap();
    }
    db.conn.lock().execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").unwrap(); // último volcado
    let mut s = db.load_settings();
    s.sounds = false;
    db.save_settings(&s).unwrap();
    db.clear_history().unwrap(); // «Borrar historial» del hub
    assert!(std::fs::metadata(wal(&path)).unwrap().len() > 0, "precondición: hay cambios sin volcar");
    let dir = t.join("tras-apagar");
    let p2 = copia_en_caliente(&path, &dir);
    drop(db);

    let db2 = Db::open_at(&p2).unwrap();
    let visto = n_hist(&db2);
    let sounds = db2.load_settings().sounds;
    let aviso = db2.aviso().map(String::from);
    eprintln!(
        "ROMPEDOR_B wal_bueno_borrado: dictados tras «Borrar historial»=0 vistos_al_arrancar={visto} sounds guardado=false visto={sounds} aviso={:?} apartados={}",
        aviso.as_deref().map(|a| a.chars().take(100).collect::<String>()),
        apartados(&dir).len()
    );
    assert!(aviso.is_none(), "el -wal del propio usuario se apartó como «ajeno»");
    assert_eq!(visto, 0, "volvieron {visto} dictados que el usuario había borrado");
    assert!(!sounds, "el ajuste guardado se perdió");
}

/// Un `-wal` bueno con dictados sin volcar y algo raro en sus ficheros (atributo de solo lectura de una
/// restauración o sincronización, o un programa que los tiene abiertos). `guard_foreign_wal` trata
/// cualquier error que no sea BUSY/LOCKED al leer «la vista con el -wal» como `(0, 0)`, es decir,
/// como `-wal` ajeno. Se mira qué variantes apartan un `-wal` bueno (datos recientes fuera).
#[test]
fn rompedor_b_wal_bueno_con_ficheros_raros() {
    use std::os::windows::fs::OpenOptionsExt;
    const SHARE_READ: u32 = 1;
    const SHARE_DELETE: u32 = 4;
    let t = Tmp::new("b-wal-raro");
    let path = t.join("vivo").join("pronto2.db");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let db = Db::open_at(&path).unwrap();
    fill(&db); // 20 dictados + palabra, snippet, nota, reunión
    db.conn.lock().execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
    for i in 0..30 {
        add(&db, &format!("dictado reciente sin volcar {i}")).unwrap();
    }
    let mut malos = Vec::new();
    for variante in ["shm_solo_lectura", "wal_solo_lectura", "wal_abierto_lectura+borrar", "wal_abierto_sin_compartir", "shm_abierto_sin_compartir", "shm_abierto_lectura+borrar", "shm_abierto_compartiendo_lectura", "wal_abierto_compartiendo_lectura"] {
        let dir = t.join(variante);
        let p = copia_en_caliente(&path, &dir);
        let mut handle = None;
        match variante {
            "shm_solo_lectura" => solo_lectura(&shm(&p), true),
            "wal_solo_lectura" => solo_lectura(&wal(&p), true),
            "wal_abierto_lectura+borrar" => handle = Some(std::fs::OpenOptions::new().read(true).share_mode(SHARE_READ | SHARE_DELETE).open(wal(&p)).unwrap()),
            "wal_abierto_sin_compartir" => handle = Some(std::fs::OpenOptions::new().read(true).share_mode(0).open(wal(&p)).unwrap()),
            "shm_abierto_sin_compartir" => handle = Some(std::fs::OpenOptions::new().read(true).share_mode(0).open(shm(&p)).unwrap()),
            "shm_abierto_compartiendo_lectura" => handle = Some(std::fs::OpenOptions::new().read(true).share_mode(SHARE_READ).open(shm(&p)).unwrap()),
            "wal_abierto_compartiendo_lectura" => handle = Some(std::fs::OpenOptions::new().read(true).share_mode(SHARE_READ).open(wal(&p)).unwrap()),
            _ => handle = Some(std::fs::OpenOptions::new().read(true).share_mode(SHARE_READ | SHARE_DELETE).open(shm(&p)).unwrap()),
        }
        let r = Db::open_at(&p).map(|d| (n_hist(&d), d.aviso().is_some())).map_err(|e| format!("{e:#}").chars().take(140).collect::<String>());
        drop(handle);
        let aparto = apartados(&dir).iter().any(|a| a.join("pronto2.db-wal").exists());
        for f in [wal(&p), shm(&p)] {
            solo_lectura(&f, false);
        }
        for a in apartados(&dir) {
            for f in std::fs::read_dir(&a).unwrap().flatten() {
                solo_lectura(&f.path(), false);
            }
        }
        let despues = Db::open_at(&p).map(|d| n_hist(&d)).map_err(|e| format!("{e:#}"));
        eprintln!("ROMPEDOR_B wal_raro[{variante}]: open_at={r:?} -wal_apartado={aparto} al_reabrir_limpio={despues:?} (esperado 50)");
        if aparto {
            malos.push(format!("{variante}: apartó el -wal bueno, quedan {despues:?} de 50 dictados"));
        }
    }
    drop(db);
    assert!(malos.is_empty(), "la guarda apartó un -wal bueno: {malos:?}");
}

/// Control: `-wal` bueno con más filas (uso normal entre volcados) y rutas con caracteres raros
/// (`%`, `#`, espacios, `&`, `[`, `;`, `'`, acentos) en `file_uri`: la guarda no salta con el bueno y
/// sí con el ajeno vacío.
#[test]
fn rompedor_b_guarda_rutas_raras_y_wal_con_mas_filas() {
    let t = Tmp::new("b-guarda-rutas");
    for nombre in ["normal", "José %41 #1 & [x] ñ", "100% seguro #2", "a b;c=d,e'f"] {
        let dir = t.join(nombre);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pronto2.db");
        let db = Db::open_at(&path).unwrap();
        fill(&db);
        db.conn.lock().execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
        for i in 0..15 {
            add(&db, &format!("más {i}")).unwrap();
        }
        let p2 = copia_en_caliente(&path, &dir.join("apagado"));
        drop(db);
        let (n, aviso) = Db::open_at(&p2).map(|d| (n_hist(&d), d.aviso().is_some())).unwrap();
        let vacia = Db::open_at(&dir.join("vacia.db")).unwrap();
        std::fs::copy(wal(&dir.join("vacia.db")), wal(&path)).unwrap();
        drop(vacia);
        let (n2, aviso2) = Db::open_at(&path).map(|d| (n_hist(&d), d.aviso().is_some())).unwrap();
        eprintln!("ROMPEDOR_B guarda_rutas[{nombre}]: bueno+15 -> {n} filas aviso={aviso}; ajeno vacío -> {n2} filas aviso={aviso2}");
        assert_eq!((n, aviso), (35, false), "{nombre}: la guarda tocó un -wal bueno");
        // Al cerrar la de arriba, sus 35 dictados quedaron volcados en el fichero principal.
        assert_eq!((n2, aviso2), (35, true), "{nombre}: la guarda no saltó con un -wal ajeno vacío");
    }
}

/// Caché `.verificado` envenenada: mismo tamaño y misma fecha con otro contenido (sector dañado,
/// restaurar con una herramienta que conserva fechas, o la marca copiada junto a otro fichero). La
/// marca manda: `verify` dice «bueno» sin leer el fichero y `is_complete` también.
#[test]
fn rompedor_b_marca_verificado_envenenada() {
    let t = Tmp::new("b-marca");
    let bueno = contenido(200_000);
    let f = modelo("decoder.int8.onnx", &bueno);
    let p = t.join(f.name);
    std::fs::write(&p, &bueno).unwrap();
    let m = std::fs::metadata(&p).unwrap();
    let mtime = m.modified().unwrap();
    let ns = mtime.duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    // La marca que escribió un arranque anterior (formato de `modeldl::remember`).
    std::fs::write(t.join(".verificado"), format!("{}\t{}\t{ns}\t{}\n", f.name, m.len(), f.sha256)).unwrap();
    let mut malo = bueno.clone();
    for b in &mut malo[..4096] {
        *b ^= 0xA5;
    }
    std::fs::write(&p, &malo).unwrap();
    std::fs::OpenOptions::new().write(true).open(&p).unwrap().set_modified(mtime).unwrap();
    let mismo = std::fs::metadata(&p).unwrap();
    assert_eq!((mismo.len(), mismo.modified().unwrap()), (m.len(), mtime), "precondición: mismo tamaño y fecha");
    let v = crate::modeldl::verify(&t.0, std::slice::from_ref(&f));
    let c = crate::modeldl::is_complete(&t.0, std::slice::from_ref(&f));
    let real_ok = crate::modeldl::hash_file(&p).unwrap() == f.sha256;
    eprintln!("ROMPEDOR_B marca_envenenada: hash real bueno={real_ok}; verify={v:?}; is_complete={c}");
    assert!(v.is_err(), "verify dio por bueno un fichero dañado (lo dijo la marca)");
}

/// Consecuencia con el motor real: modelo de Parakeet copiado de `PRONTO_ROMPEDOR_MODELO` (solo
/// lectura), `decoder.int8.onnx` dañado en sus primeros 64 KB con el mismo tamaño y fecha, y la marca
/// `.verificado` que dejó un arranque anterior. Lánzala sola con `PRONTO_ROMPEDOR_MARCA=1`: si
/// sherpa-onnx aborta, se lleva el proceso (como al arrancar Pronto, en cada arranque).
#[test]
fn rompedor_b_motor_con_marca_envenenada() {
    let (Ok(_), Ok(src)) = (std::env::var("PRONTO_ROMPEDOR_MARCA"), std::env::var("PRONTO_ROMPEDOR_MODELO")) else {
        println!("ROMPEDOR_OMITIDO: faltan PRONTO_ROMPEDOR_MARCA=1 y PRONTO_ROMPEDOR_MODELO");
        return;
    };
    let t = Tmp::new("b-motor-marca");
    let mut marca = String::new();
    for f in crate::modeldl::FILES.iter() {
        let dst = t.join(f.name);
        std::fs::copy(Path::new(&src).join(f.name), &dst).unwrap();
        let m = std::fs::metadata(&dst).unwrap();
        let mtime = m.modified().unwrap();
        if f.name == "decoder.int8.onnx" {
            let mut b = std::fs::read(&dst).unwrap();
            for x in &mut b[..65_536] {
                *x ^= 0x5A;
            }
            std::fs::write(&dst, &b).unwrap();
            std::fs::OpenOptions::new().write(true).open(&dst).unwrap().set_modified(mtime).unwrap();
        }
        let ns = mtime.duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        marca.push_str(&format!("{}\t{}\t{ns}\t{}\n", f.name, m.len(), f.sha256));
    }
    std::fs::write(t.join(".verificado"), marca).unwrap();
    let dir = t.0.to_string_lossy().into_owned();
    eprintln!("ROMPEDOR_B motor_marca: model_present={} verify={:?} · cargando…", crate::modeldl::model_present(&dir), crate::modeldl::verify(&t.0, &crate::modeldl::FILES));
    let r = crate::asr::Engine::load_for_tests(&dir);
    eprintln!("ROMPEDOR_B motor_marca: load -> {:?}", r.as_ref().err());
    assert!(r.is_err());
}

/// Whisper con un `.bin` dañado del mismo tamaño (`WhisperModel::present` solo mira el tamaño y
/// `asr::create_any` lo carga sin SHA-256). Se carga con `whisper::Whisper::load` (el de la app) una
/// copia en `%TEMP%` de `PRONTO_ROMPEDOR_WHISPER` (solo lectura). Variantes (`PRONTO_ROMPEDOR_VARIANTE`):
/// `magia` (4 primeros bytes), `hparams` (bytes 4-48), `ftype_un_byte` (un bit del byte 44),
/// `tensor_tipo` / `tensor_dims` (cabecera del
/// primer tensor) y `datos` (16 MB a mitad). Lánzala sola: si whisper.cpp aborta, se lleva el proceso.
#[test]
fn rompedor_b_whisper_bin_danado() {
    let Ok(src) = std::env::var("PRONTO_ROMPEDOR_WHISPER") else {
        println!("ROMPEDOR_OMITIDO: falta PRONTO_ROMPEDOR_WHISPER");
        return;
    };
    let variante = std::env::var("PRONTO_ROMPEDOR_VARIANTE").unwrap_or_else(|_| "hparams".into());
    let t = Tmp::new("b-whisper");
    let dst = t.join("ggml-danado.bin");
    std::fs::copy(&src, &dst).unwrap();
    let mut b = std::fs::read(&dst).unwrap();
    let len = b.len();
    let i32_at = |b: &[u8], o: usize| i32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    // Primer tensor: magia + 11 hparams, filtros mel (n_mel, n_fft, datos f32), vocabulario.
    let mut o = 4 + 11 * 4;
    let (n_mel, n_fft) = (i32_at(&b, o) as usize, i32_at(&b, o + 4) as usize);
    o += 8 + n_mel * n_fft * 4;
    let n_vocab = i32_at(&b, o) as usize;
    o += 4;
    for _ in 0..n_vocab {
        o += 4 + u32::from_le_bytes(b[o..o + 4].try_into().unwrap()) as usize;
    }
    let tensor = o;
    let (n_dims, name_len) = (i32_at(&b, tensor), i32_at(&b, tensor + 4));
    match variante.as_str() {
        "magia" => b[..4].copy_from_slice(b"XXXX"),
        "hparams" => b[4..48].fill(0x7F),
        "ftype_un_byte" => b[44] ^= 0x40, // un solo bit en `ftype` (q5_0 = 8 → 72)
        "tensor_tipo" => b[tensor + 8..tensor + 12].copy_from_slice(&999i32.to_le_bytes()),
        "tensor_dims" => b[tensor + 12..tensor + 16].copy_from_slice(&i32::MAX.to_le_bytes()),
        _ => {
            let mid = len / 2;
            for (k, x) in b[mid..mid + (16 << 20)].iter_mut().enumerate() {
                *x = (k as u32).wrapping_mul(2_654_435_761).to_le_bytes()[1];
            }
        }
    }
    std::fs::write(&dst, &b).unwrap();
    drop(b);
    // Premisa ajustada tras el arreglo (MEDIA 5): `Whisper::load` verifica el SHA-256, así que el
    // modelo de prueba lleva el hash real del de la tabla con ese tamaño (con "" fallaría siempre).
    let sha256 = crate::whisper::MODELS.iter().find(|x| x.bytes == len as u64).map_or("", |x| x.sha256);
    let m: &'static crate::whisper::WhisperModel = Box::leak(Box::new(crate::whisper::WhisperModel {
        id: "large-v3-turbo",
        name: "dañado",
        desc: "",
        file: leak(dst.to_string_lossy().into_owned()),
        sha256,
        bytes: len as u64,
        min_vram_gb: 0.0,
        pair_vram_gb: 0.0,
        wer: None,
        speed: 0,
    }));
    eprintln!("ROMPEDOR_B whisper[{variante}]: {len} bytes, n_mel={n_mel} n_fft={n_fft} n_vocab={n_vocab}, primer tensor en {tensor} (n_dims={n_dims}, name_len={name_len}), present={} · cargando…", m.present());
    let a = Instant::now();
    let r = crate::whisper::Whisper::load(m, true);
    eprintln!("ROMPEDOR_B whisper[{variante}]: load -> {:?} en {:.1} s", r.as_ref().err(), a.elapsed().as_secs_f32());
    if let Ok(w) = &r {
        let s = w.decode_timed(&vec![0.0f32; 16_000 * 3], "es");
        eprintln!("ROMPEDOR_B whisper[{variante}]: decode 3 s de silencio -> {:?}", s.text.chars().take(60).collect::<String>());
    }
    assert!(r.is_err(), "un .bin dañado del mismo tamaño se cargó sin verificar");
}

/// Un ajuste de una versión más nueva (campo que esta no conoce): serde lo ignora sin error, así que
/// `settings_from_value` no lo cuenta como «no entendido», no va a `settings_backup` y el primer
/// guardado lo borra. El doc de `save_settings` promete lo contrario.
#[test]
fn rompedor_b_ajuste_de_version_nueva_se_pierde_sin_copia() {
    let db = Db::open_memory().unwrap();
    let mut s = Settings::default();
    s.sounds = false;
    let mut v = serde_json::to_value(&s).unwrap();
    v["dictadoEnVozAlta"] = serde_json::json!({"activo": true, "voz": "Lucía"});
    db.conn.lock().execute("INSERT INTO settings (id, json) VALUES (1, ?1)", params![v.to_string()]).unwrap();
    let leidos = db.load_settings();
    db.save_settings(&leidos).unwrap();
    let c = db.conn.lock();
    let guardado: String = c.query_row("SELECT json FROM settings", [], |r| r.get(0)).unwrap();
    let copias: i64 = c.query_row("SELECT COUNT(*) FROM settings_backup WHERE json LIKE '%dictadoEnVozAlta%'", [], |r| r.get(0)).unwrap_or(-1);
    eprintln!("ROMPEDOR_B ajuste_nuevo: sounds={} ¿sigue en settings? {} copias en settings_backup={copias}", leidos.sounds, guardado.contains("dictadoEnVozAlta"));
    assert!(guardado.contains("dictadoEnVozAlta") || copias > 0, "el ajuste de la versión nueva desapareció sin copia");
}

/// Dos guardados de ajustes a la vez (hub + `adopt_model_dir`), con un JSON viejo que esta versión no
/// entiende: una sola copia en `settings_backup`, sin errores ni ajustes mezclados.
#[test]
fn rompedor_b_ajustes_guardados_a_la_vez() {
    let t = Tmp::new("b-ajustes-carrera");
    let db = Arc::new(Db::open_at(&t.join("pronto2.db")).unwrap());
    let mut v = serde_json::to_value(Settings::default()).unwrap();
    v["cleanupLevel"] = serde_json::json!("extremo");
    db.conn.lock().execute("INSERT INTO settings (id, json) VALUES (1, ?1) ON CONFLICT(id) DO UPDATE SET json = excluded.json", params![v.to_string()]).unwrap();
    let barrier = Arc::new(Barrier::new(8));
    let hilos: Vec<_> = (0..8)
        .map(|i| {
            let (db, b) = (db.clone(), barrier.clone());
            std::thread::spawn(move || {
                b.wait();
                let mut errs = 0;
                for k in 0..50 {
                    let mut s = db.load_settings();
                    s.keep_audio_days = (i * 100 + k) as u32;
                    if db.save_settings(&s).is_err() {
                        errs += 1;
                    }
                }
                errs
            })
        })
        .collect();
    let errs: i32 = hilos.into_iter().map(|h| h.join().unwrap()).sum();
    let c = db.conn.lock();
    let copias: i64 = c.query_row("SELECT COUNT(*) FROM settings_backup", [], |r| r.get(0)).unwrap();
    let json: String = c.query_row("SELECT json FROM settings WHERE id = 1", [], |r| r.get(0)).unwrap();
    let ok = serde_json::from_str::<Settings>(&json).is_ok();
    eprintln!("ROMPEDOR_B ajustes_carrera: 400 guardados en 8 hilos, errores={errs}, copias en settings_backup={copias}, JSON final válido={ok}");
    assert_eq!((errs, copias, ok), (0, 1, true));
}

/// Borrado legítimo grande (vaciar el historial, o bajar la retención de «siempre» a 7 días): la
/// copia diaria lo toma por un incidente cada día, avisa cada día y no rota nunca las copias buenas,
/// que siguen guardando los textos que el usuario borró (antes duraban como mucho 7 días).
#[test]
fn rompedor_b_copia_diaria_tras_borrado_legitimo() {
    let t = Tmp::new("b-copia-borrado");
    let copias = t.join("copias");
    std::fs::create_dir_all(&copias).unwrap();
    let db = Db::open_at(&t.join("pronto2.db")).unwrap();
    for i in 0..200 {
        add(&db, &format!("texto que el usuario borrará {i}")).unwrap();
    }
    for d in 1..=7 {
        db.conn.lock().execute("VACUUM INTO ?1", params![copias.join(format!("pronto2-2026090{d}.db")).to_string_lossy()]).unwrap();
    }
    db.clear_history().unwrap();
    let (mut sospechosas, mut normales) = (0, 0);
    for dia in 1..=14 {
        for i in 0..3 {
            add(&db, &format!("día {dia} dictado {i}")).unwrap();
        }
        if let Some(p) = db.daily_backup(&copias, 7).unwrap() {
            if p.to_string_lossy().contains("sospechosa") {
                sospechosas += 1;
                std::fs::rename(&p, copias.join(format!("pronto2-{}-d{dia:02}-sospechosa.db", hoy()))).unwrap();
            } else {
                normales += 1;
                std::fs::rename(&p, copias.join(format!("pronto2-202610{dia:02}.db"))).unwrap();
            }
        }
    }
    let con_borrado = std::fs::read_dir(&copias)
        .unwrap()
        .flatten()
        .filter(|e| {
            Connection::open_with_flags(e.path(), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                .and_then(|c| c.query_row("SELECT COUNT(*) FROM history WHERE text LIKE 'texto que el usuario borrará%'", [], |r| r.get::<_, i64>(0)))
                .is_ok_and(|n| n > 0)
        })
        .count();
    eprintln!(
        "ROMPEDOR_B copia_tras_borrado: 14 días tras vaciar el historial (200 -> 3..42 filas): días con copia «sospechosa» y aviso={sospechosas}, copias normales={normales}, copias que aún guardan lo borrado={con_borrado}"
    );
    assert!(con_borrado == 0 || sospechosas < 8, "14 días después: {sospechosas} avisos y {con_borrado} copias con los textos borrados");
}

/// `set_aside`/`write_new` con colisiones en el mismo segundo, nombres raros y un fichero de solo
/// lectura: nunca se pierde un WAV. Y con la carpeta `apartado-<fecha-hora>` ocupada por un fichero.
#[test]
fn rompedor_b_set_aside_colisiones_y_nombres_raros() {
    let t = Tmp::new("b-set-aside");
    let mut perdidos = Vec::new();
    for nombre in ["7-20261022210000.wav", "José ñ #1 %20 & [x].wav", "con espacio .wav"] {
        let p = t.join(nombre);
        for k in 0..5u8 {
            if k == 3 {
                solo_lectura(&p, true);
            }
            if let Err(e) = db::write_new(&p, &[k; 64]) {
                perdidos.push(format!("{nombre} vuelta {k}: {e}"));
            }
        }
        let mut vistas: Vec<u8> = apartados(&t.0)
            .iter()
            .flat_map(|a| std::fs::read_dir(a).unwrap().flatten().map(|e| e.path()))
            .chain([p.clone()])
            .filter(|f| f.file_name().unwrap().to_string_lossy().ends_with(nombre))
            .filter_map(|f| std::fs::read(f).ok().map(|b| b[0]))
            .collect();
        vistas.sort();
        eprintln!("ROMPEDOR_B set_aside[{nombre}]: versiones conservadas {vistas:?}");
        if vistas != vec![0, 1, 2, 3, 4] {
            perdidos.push(format!("{nombre}: quedan {vistas:?}"));
        }
    }
    let p = t.join("8-1.wav");
    std::fs::write(&p, b"viejo").unwrap();
    for s in 0..3 {
        let stamp = (chrono::Local::now() + chrono::Duration::seconds(s)).format("%Y%m%d-%H%M%S").to_string();
        let _ = std::fs::write(t.join(&format!("apartado-{stamp}")), b"x");
    }
    let r = db::write_new(&p, b"nuevo");
    let viejo_apartado = apartados(&t.0).iter().filter(|a| a.is_dir()).flat_map(|a| std::fs::read_dir(a).unwrap().flatten()).any(|f| std::fs::read(f.path()).is_ok_and(|b| b == b"viejo"));
    let viejo_en_su_sitio = std::fs::read(&p).is_ok_and(|b| b == b"viejo");
    eprintln!("ROMPEDOR_B set_aside[apartado-<fecha-hora> ocupado por un fichero]: write_new -> {r:?}; el viejo sigue en su sitio={viejo_en_su_sitio} o apartado={viejo_apartado}");
    if !(viejo_apartado || viejo_en_su_sitio) {
        perdidos.push("8-1.wav: el WAV viejo desapareció".into());
    }
    for a in apartados(&t.0) {
        for f in std::fs::read_dir(&a).into_iter().flatten().flatten() {
            solo_lectura(&f.path(), false);
        }
    }
    solo_lectura(&t.join("7-20261022210000.wav"), false);
    assert!(perdidos.is_empty(), "{perdidos:?}");
}

/// Hueco del primer informe: base dañada o vacía al arrancar. Se mira qué devuelve `Db::open_at` (lo
/// que hace `main.rs` en `setup`, con `?`) y si queda algún aviso. Variantes: cabecera basura, base
/// cortada a la mitad, una página interior dañada y `pronto2.db` de 0 bytes con copias buenas en
/// `copias\` (la firma del 13-oct: «el proceso vio pronto2.db vacío»).
#[test]
fn rompedor_b_base_danada_o_vacia_al_arrancar() {
    let t = Tmp::new("b-base-danada");
    let buena = t.join("buena.db");
    {
        let db = Db::open_at(&buena).unwrap();
        fill(&db);
    }
    let bytes = std::fs::read(&buena).unwrap();
    let copias = t.join("copias");
    std::fs::create_dir_all(&copias).unwrap();
    std::fs::copy(&buena, copias.join("pronto2-20261021.db")).unwrap();
    let mut silencioso = false;
    for variante in ["cabecera_basura", "cortada_mitad", "pagina_interior", "cero_bytes"] {
        let dir = t.join(variante);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("pronto2.db");
        let mut b = bytes.clone();
        match variante {
            "cabecera_basura" => b[16..100].fill(0xEE),
            "cortada_mitad" => b.truncate(b.len() / 2),
            "pagina_interior" => {
                let ps = 4096;
                let n = b.len() / ps;
                b[(n / 2) * ps..(n / 2) * ps + ps].fill(0x33);
            }
            _ => b.clear(),
        }
        std::fs::write(&p, &b).unwrap();
        match Db::open_at(&p) {
            Err(e) => eprintln!("ROMPEDOR_B base[{variante}]: Db::open_at -> Err({}) => main.rs:171 `?` y :233 `.expect` (sin ventana ni aviso)", format!("{e:#}").chars().take(120).collect::<String>()),
            Ok(db) => {
                let n = db.conn.lock().query_row("SELECT COUNT(*) FROM history", [], |r| r.get::<_, i64>(0)).map_err(|e| e.to_string());
                let ok: Result<String, String> = db.conn.lock().query_row("PRAGMA quick_check", [], |r| r.get(0)).map_err(|e| e.to_string());
                let copia = db.daily_backup(&copias, 7).map_err(|e| e.to_string());
                eprintln!("ROMPEDOR_B base[{variante}]: abre; dictados={n:?} quick_check={ok:?} aviso={:?} daily_backup={copia:?}", db.aviso());
                if variante == "cero_bytes" && db.aviso().is_none() && matches!(copia, Ok(None)) && n == Ok(0) {
                    silencioso = true;
                }
            }
        }
    }
    assert!(!silencioso, "pronto2.db de 0 bytes: arranca con una base nueva vacía, sin aviso y sin mirar copias\\ (20 dictados en la copia)");
}

/// Compatibilidad con los WAV de reunión de antes (`meeting_{id}_{who}.wav`): solo se aceptan si su
/// fecha cae entre `started_at − 1 min` y `ended_at + 10 min`. Una reunión que quedó grabando cuando
/// Pronto se cerró (el caso en que más importa su audio) se cierra con `ended_at = started_at`
/// (`meetingdb.rs` `close_orphan_meetings`): su WAV, escrito hasta el final, cae fuera y deja de
/// existir para Pronto (ni reintentar ni borrarlo).
#[test]
#[ignore = "rompedor: el WAV antiguo de una reunión huérfana de más de 10 min no se encuentra"]
fn rompedor_b_reunion_huerfana_antigua_pierde_su_audio() {
    let db = Db::open_memory().unwrap();
    let m = db.create_meeting("Reunión que se cortó", Some("Teams")).unwrap();
    let inicio = chrono::Local::now() - chrono::Duration::minutes(90);
    db.conn.lock().execute("UPDATE meetings SET started_at = ?2, ended_at = NULL WHERE id = ?1", params![m.id, inicio.to_rfc3339()]).unwrap();
    let wav = db::audio_dir().unwrap().join(format!("meeting_{}_me.wav", m.id));
    std::fs::write(&wav, crate::audio::encode_wav(&vec![0.1f32; 16_000])).unwrap();
    // Grabó 60 min: la última escritura del WAV fue una hora después de empezar.
    let fin: std::time::SystemTime = (inicio + chrono::Duration::minutes(60)).into();
    std::fs::OpenOptions::new().write(true).open(&wav).unwrap().set_modified(fin).unwrap();
    db.close_orphan_meetings().unwrap();
    let m = db.get_meeting(m.id).unwrap().expect("la reunión");
    crate::meeting::remove_audio_files(&m); // usa `existing_audio_file`, como reintentar y la fase 2
    let sigue = wav.exists();
    eprintln!("ROMPEDOR_B reunion_huerfana: started_at={} ended_at={:?}; WAV antiguo con fecha +60 min; ¿lo encontró (borrar)? {}", m.started_at, m.ended_at, !sigue);
    let _ = std::fs::remove_file(&wav);
    assert!(!sigue, "el WAV de la reunión huérfana no se encuentra: ni se reintenta ni se borra");
}

/// La marca `active` (dictado en marcha) atascada: `wait_dictation_released` tiene tope por TROZO,
/// no por reunión, y se llama al empezar cada trozo. Con la marca atascada, cada trozo espera
/// `HOLD_CAP` (21 min). Para medirlo sin esperar 21 min se baja el tope del motor a 150 ms buscando
/// el `Duration` de 1260 s en el propio struct (sonda; si no se encuentra exactamente uno, se omite).
#[test]
#[ignore = "rompedor: con la marca active atascada, cada trozo de la fase 2 espera el tope entero (21 min)"]
fn rompedor_b_marca_atascada_espera_por_trozo() {
    let mut e = crate::asr::Engine::new("");
    let size = std::mem::size_of::<crate::asr::Engine>();
    // SAFETY: lectura/escritura de bytes dentro del propio struct, solo en esta sonda.
    let bytes = unsafe { std::slice::from_raw_parts_mut(&mut e as *mut crate::asr::Engine as *mut u8, size) };
    let hits: Vec<usize> = (0..size.saturating_sub(12))
        .step_by(8)
        .filter(|&o| u64::from_le_bytes(bytes[o..o + 8].try_into().unwrap()) == 21 * 60 && u32::from_le_bytes(bytes[o + 8..o + 12].try_into().unwrap()) == 0)
        .collect();
    if hits.len() != 1 {
        println!("ROMPEDOR_OMITIDO: no se encontró el tope en el struct ({hits:?})");
        return;
    }
    bytes[hits[0]..hits[0] + 8].copy_from_slice(&0u64.to_le_bytes());
    bytes[hits[0] + 8..hits[0] + 12].copy_from_slice(&150_000_000u32.to_le_bytes());
    let atascada = Arc::new(AtomicBool::new(true));
    e.share_dictation_flag(atascada.clone());
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let a = Instant::now();
        for _ in 0..10 {
            let _ = e.transcribe_background_timed(&[0.0; 160]); // sin modelo: Err nada más pasar la espera
        }
        let _ = tx.send(a.elapsed());
    });
    let Ok(dt) = rx.recv_timeout(Duration::from_secs(20)) else {
        println!("ROMPEDOR_OMITIDO: el tope no cambió (sigue en 21 min)");
        return;
    };
    eprintln!(
        "ROMPEDOR_B marca_atascada: 10 trozos con la marca puesta y tope 150 ms -> {:.2} s ({:.0} ms por trozo). Con HOLD_CAP real (21 min) y 120 trozos (1 h de reunión en trozos de 30 s): {:.0} h de fase 2",
        dt.as_secs_f64(),
        dt.as_secs_f64() * 100.0,
        120.0 * 21.0 / 60.0
    );
    assert!(dt < Duration::from_millis(1000), "el tope es por trozo: {dt:?} para 10 trozos");
}

/// MEDIA 5 tras el arreglo: la fase 2 no empieza un trozo mientras se graba, pero el trozo que ya
/// estaba en marcha al pulsar sigue (30 s de audio ≈ 1,8 s de motor). Un dictado CORTO (se suelta
/// antes de que acabe ese trozo) sigue esperando. Se mide desde que se suelta, con el audio de esa
/// duración, contra el mismo dictado con el motor libre.
#[test]
#[ignore = "rompedor: un dictado corto aún espera al trozo de la fase 2 que ya estaba en marcha"]
fn rompedor_b_dictado_corto_durante_fase2() {
    let (Ok(dir), Ok(wav)) = (std::env::var("PRONTO_ROMPEDOR_MODELO"), std::env::var("PRONTO_ROMPEDOR_WAV")) else {
        println!("ROMPEDOR_OMITIDO: faltan PRONTO_ROMPEDOR_MODELO y PRONTO_ROMPEDOR_WAV");
        return;
    };
    let voz = crate::audio::decode_wav(&std::fs::read(&wav).unwrap()).unwrap();
    let reunion: Vec<f32> = voz.iter().cycle().take(30 * 16_000).cloned().collect();
    let engine = Arc::new(crate::asr::Engine::load_for_tests(&dir).expect("cargar Parakeet"));
    let pulsado = Arc::new(AtomicBool::new(false));
    engine.share_dictation_flag(pulsado.clone());
    let _ = engine.transcribe_timed(&voz[..16_000]).unwrap();
    let parar = Arc::new(AtomicBool::new(false));
    let trozos = Arc::new(AtomicUsize::new(0));
    let (e2, p2, t2) = (engine.clone(), parar.clone(), trozos.clone());
    let fondo = std::thread::spawn(move || {
        while !p2.load(Ordering::SeqCst) {
            e2.transcribe_background_timed(&reunion).unwrap();
            t2.fetch_add(1, Ordering::SeqCst);
        }
    });
    let mut linea = String::new();
    let mut peor = 0.0f64;
    for habla in [600u64, 1000, 1500] {
        let audio = &voz[..(habla as usize * 16)];
        let mut solo = Vec::new();
        let mut con = Vec::new();
        for i in 0..12 {
            std::thread::sleep(Duration::from_millis(200 + (i * 523 % 1700) as u64));
            pulsado.store(true, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(habla));
            let a = Instant::now();
            engine.transcribe_timed(audio).unwrap();
            con.push(a.elapsed().as_secs_f64() * 1000.0);
            // El mismo audio con el motor libre (la marca sigue puesta: la fase 2 no empieza trozo).
            let a = Instant::now();
            engine.transcribe_timed(audio).unwrap();
            solo.push(a.elapsed().as_secs_f64() * 1000.0);
            pulsado.store(false, Ordering::SeqCst);
        }
        let (s50, c50, c95) = (pct(&mut solo.clone(), 0.5), pct(&mut con.clone(), 0.5), pct(&mut con.clone(), 0.95));
        let cmax = con.iter().cloned().fold(0.0, f64::max);
        peor = peor.max(c95);
        linea.push_str(&format!(" | habla {habla} ms: motor libre p50 {s50:.0} ms; con fase 2 p50/p95/máx {c50:.0}/{c95:.0}/{cmax:.0} ms"));
    }
    parar.store(true, Ordering::SeqCst);
    fondo.join().unwrap();
    println!("ROMPEDOR_B motor_corto:{linea} | trozos de reunión hechos {}", trozos.load(Ordering::SeqCst));
    assert!(peor <= 1068.0, "dictado corto con la fase 2 en marcha: p95 {peor:.0} ms > 1068 ms");
}

// ===== hub 23-oct
// Segunda pasada del hub (informe `specs/rompedor/2026-10-23/hub.md`). Los comandos IPC piden
// `tauri::State` y sin la feature `test` de tauri no se pueden llamar desde aquí: se ataca la
// función de `Db` a la que cada comando delega tal cual (`list_meetings` → `Db::list_meetings`).

/// El buscador de reuniones del hub (`list_meetings`) encuentra por título con y sin tildes, como el
/// del historial desde el arreglo de dictado (68707a4, «búsqueda con tildes»).
#[test]
#[ignore = "rompedor: el buscador de reuniones no encuentra el título si cambian tildes o mayúsculas no ASCII"]
fn rompedor_hub_buscar_reunion_por_titulo_con_tildes() {
    let tmp = Tmp::new("hub-titulo");
    let db = Db::open_at(&tmp.join("pronto2.db")).unwrap();
    // Solo título: sin turnos ni resumen (reunión manual que aún no tiene notas, o con la fase 2 fallida).
    let m = db.create_meeting("Revisión del presupuesto con Óscar", Some("Teams")).unwrap();
    // Control: el historial sí pliega tildes y mayúsculas.
    add(&db, "Llamar a Óscar por la revisión").unwrap();
    let hist = |q: &str| db.list_history(Some(q), 50, 0).unwrap().len();
    let reun = |q: &str| db.list_meetings(Some(q), 50, 0).unwrap().iter().filter(|x| x.meeting.id == m.id).count();
    let consultas = ["Óscar", "óscar", "oscar", "OSCAR", "revision", "Revisión", "REVISIÓN", "presupuesto"];
    let mut fallan = Vec::new();
    let mut tabla = String::new();
    for q in consultas {
        let (h, r) = (hist(q), reun(q));
        tabla.push_str(&format!(" «{q}»: historial {h}, reuniones {r} |"));
        if h > 0 && r == 0 {
            fallan.push(q);
        }
    }
    println!("ROMPEDOR_HUB titulo:{tabla}");
    assert!(fallan.is_empty(), "el historial encuentra y el buscador de reuniones no: {fallan:?} ({tabla})");
}
