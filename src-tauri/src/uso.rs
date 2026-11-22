//! Estadísticas de uso voluntarias (PRNT-43, decisión de Txonso, 31-oct-2026; regla 1 de CLAUDE.md).
//!
//! Solo con un «sí» expreso (`Settings::usage_stats == Some(true)`), un hilo de fondo manda a Umami, una
//! vez al día, las cifras de los días ya terminados. Reglas:
//! - Lista cerrada: `payload` es el ÚNICO sitio donde se escribe lo que sale, y solo lleva recuentos,
//!   el día, la versión y el programa principal del día, y este solo si es uno muy conocido de
//!   `known_app`: cualquier otro sale como `otro`. Nunca voz, texto, títulos de ventana ni rutas.
//! - El código de instalación es aleatorio, nace al aceptar y se borra al retirar el permiso.
//! - Lo anterior al «sí» no se envía nunca: el primer día que cuenta es el siguiente a aceptar.
//! - Umami no recibe datos para situar a nadie: el campo `ip` lleva loopback para que no calcule país,
//!   y la marca de tiempo es el mediodía del día contado, no la hora a la que se encendió el PC.
//! - Si falla la red no pasa nada: el día sigue pendiente y se reintenta en la hora siguiente.

use crate::db::{Db, DayUsage};
use crate::state::AppState;
use chrono::{Duration as Days, Local, NaiveDate};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

const ENDPOINT: &str = "https://stats.beltranlabs.com/api/send";
/// Identificador de la app en el panel de Umami. Vacío = no se envía nada (`tests/uso.test.mjs` no deja publicar así).
const WEBSITE_ID: &str = "7ac19748-283a-4d39-bfa0-230d5fa8f73b";
const HOSTNAME: &str = "app.pronto";
const EVENT: &str = "uso_diario";
const NO_LOCATION_IP: &str = "127.0.0.1";
const MAX_BACKLOG_DAYS: i64 = 7;
const FIRST_DELAY: Duration = Duration::from_secs(90);
const CHECK_EVERY: Duration = Duration::from_secs(3600);

/// Programas conocidos. Un nombre de esta lista no dice nada de nadie; uno suelto podría decirlo
/// (`ClienteGarciaERP.exe`), así que lo que no está aquí sale como `otro`.
fn known_app(name: &str) -> &'static str {
    match name.to_lowercase().as_str() {
        "chrome" => "Chrome",
        "msedge" => "Edge",
        "firefox" => "Firefox",
        "brave" => "Brave",
        "opera" => "Opera",
        "winword" => "Word",
        "excel" => "Excel",
        "powerpnt" => "PowerPoint",
        "onenote" => "OneNote",
        "outlook" | "olk" | "hxoutlook" => "Outlook",
        "thunderbird" => "Thunderbird",
        "teams" | "ms-teams" | "msteams" => "Teams",
        "slack" => "Slack",
        "zoom" => "Zoom",
        "whatsapp" | "whatsapp.root" => "WhatsApp",
        "telegram" => "Telegram",
        "discord" => "Discord",
        "signal" => "Signal",
        "notepad" => "Bloc de notas",
        "notion" => "Notion",
        "obsidian" => "Obsidian",
        "code" => "VS Code",
        "cursor" => "Cursor",
        "claude" => "Claude",
        "chatgpt" => "ChatGPT",
        "windowsterminal" => "Terminal",
        _ => "otro",
    }
}

/// El programa con más palabras del día, ya pasado por la lista cerrada.
fn main_app(apps: &[(String, i64)]) -> &'static str {
    let mut totals: BTreeMap<&'static str, i64> = BTreeMap::new();
    for (name, words) in apps {
        *totals.entry(known_app(name)).or_default() += words;
    }
    totals.into_iter().max_by_key(|(_, words)| *words).map_or("ninguno", |(app, _)| app)
}

/// Días terminados aún sin enviar, del más antiguo al más nuevo (como mucho, la última semana).
fn pending_days(sent_until: NaiveDate, today: NaiveDate) -> Vec<NaiveDate> {
    let first = (sent_until + Days::days(1)).max(today - Days::days(MAX_BACKLOG_DAYS));
    first.iter_days().take_while(|d| *d < today).collect()
}

fn minutes(ms: i64) -> i64 {
    (ms + 30_000) / 60_000
}

/// Lo ÚNICO que sale de Pronto en este módulo.
fn payload(website: &str, id: &str, day: NaiveDate, u: &DayUsage) -> Value {
    json!({
        "type": "event",
        "payload": {
            "website": website,
            "hostname": HOSTNAME,
            "url": "/app",
            "name": EVENT,
            "id": id,
            "ip": NO_LOCATION_IP,
            "timestamp": day.and_hms_opt(12, 0, 0).map_or(0, |t| t.and_utc().timestamp()),
            "data": {
                "dia": day.to_string(),
                "version": crate::updater::CARGO_VERSION,
                "palabras": u.words,
                "segundos_dictado": u.dictation_ms / 1000,
                "dictados": u.dictations,
                "reuniones": u.meetings,
                "minutos_reunion": minutes(u.meeting_ms),
                "app_principal": main_app(&u.apps),
            },
        },
    })
}

/// Umami descarta sin error lo que parece un robot, y `Nombre/1.2.3` lo parece (comprobado en su filtro, v3.4.1).
fn user_agent() -> String {
    format!("Mozilla/5.0 (Windows NT 10.0; Win64; x64) Pronto/{}", crate::updater::CARGO_VERSION)
}

fn post(url: &str, body: &Value) -> Result<(), String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(20))
        .redirects(0)
        .user_agent(&user_agent())
        .build();
    match agent.post(url).timeout(Duration::from_secs(30)).send_json(body) {
        Ok(r) if (200..300).contains(&r.status()) => match r.into_string() {
            Ok(text) if text.contains("beep") => Err("descartado por Umami".into()),
            _ => Ok(()),
        },
        Ok(r) => Err(format!("respuesta {}", r.status())),
        Err(ureq::Error::Status(code, _)) => Err(format!("respuesta {code}")),
        Err(ureq::Error::Transport(_)) => Err("sin conexión".into()),
    }
}

/// Envía lo pendiente y devuelve cuántos días salieron. `allowed` se vuelve a mirar antes de cada envío:
/// retirar el permiso corta el lote. Los días sin uso no se envían; si no hubo ninguno con uso, sale uno
/// a cero para saber que esa instalación sigue viva.
pub fn send_pending(db: &Db, url: &str, website: &str, today: NaiveDate, allowed: &dyn Fn() -> bool) -> Result<usize, String> {
    if !allowed() {
        return Ok(0);
    }
    let Some((id, sent_until)) = db.usage_state().map_err(|e| e.to_string())? else { return Ok(0) };
    let days = pending_days(sent_until, today);
    let Some(&last) = days.last() else { return Ok(0) };
    let mut reports = Vec::new();
    for day in days {
        let usage = db.usage_day(day).map_err(|e| e.to_string())?;
        if usage.has_use() {
            reports.push((day, usage));
        }
    }
    if reports.is_empty() {
        reports.push((last, DayUsage::default()));
    }
    let mut sent = 0;
    for (day, usage) in reports {
        if !allowed() {
            return Ok(sent);
        }
        post(url, &payload(website, &id, day, &usage))?;
        db.usage_mark_sent(&id, day).map_err(|e| e.to_string())?;
        sent += 1;
    }
    if allowed() {
        db.usage_mark_sent(&id, last).map_err(|e| e.to_string())?;
    }
    Ok(sent)
}

/// Arranca el hilo. Las compilaciones de desarrollo y de pruebas no envían nunca.
pub fn spawn_background(st: Arc<AppState>) {
    if cfg!(debug_assertions) || crate::db::test_build() || WEBSITE_ID.is_empty() {
        return;
    }
    let _ = std::thread::Builder::new().name("pronto-uso".into()).spawn(move || {
        std::thread::sleep(FIRST_DELAY);
        loop {
            let allowed = || st.settings.read().usage_stats == Some(true);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                send_pending(&st.db, ENDPOINT, WEBSITE_ID, Local::now().date_naive(), &allowed)
            }));
            if let Ok(Err(e)) = result.as_ref().map(|r| r.as_ref().map(|_| ())) {
                if e != "sin conexión" {
                    crate::errlog::error("uso", e);
                }
            }
            std::thread::sleep(CHECK_EVERY);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::NewHistory;
    use crate::models::Fixes;
    use std::collections::BTreeSet;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    const WEB: &str = "00000000-0000-4000-8000-000000000000";
    const SECRET: &str = "secreto-del-usuario";

    fn day(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    /// Servidor local que apunta cada cuerpo recibido y responde `status`.
    fn server(status: u16) -> (String, Arc<Mutex<Vec<String>>>) {
        server_with(status, "", "")
    }

    fn server_with(status: u16, extra: &str, reply: &str) -> (String, Arc<Mutex<Vec<String>>>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/api/send", l.local_addr().unwrap());
        let bodies = Arc::new(Mutex::new(Vec::new()));
        let out = bodies.clone();
        let (extra, reply) = (extra.to_string(), reply.to_string());
        std::thread::spawn(move || {
            for conn in l.incoming().flatten() {
                let mut r = BufReader::new(conn.try_clone().unwrap());
                let mut len = 0;
                loop {
                    let mut h = String::new();
                    r.read_line(&mut h).unwrap();
                    if h.trim().is_empty() {
                        break;
                    }
                    if let Some(v) = h.to_lowercase().strip_prefix("content-length:") {
                        len = v.trim().parse().unwrap();
                    }
                }
                let mut b = vec![0; len];
                r.read_exact(&mut b).unwrap();
                out.lock().unwrap().push(String::from_utf8(b).unwrap());
                let _ = (&conn).write_all(format!("HTTP/1.1 {status} X\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{reply}", reply.len()).as_bytes());
            }
        });
        (url, bodies)
    }

    fn dictate(db: &Db, text: &str, words: i64, ms: i64, app: &str) {
        db.add_history(&NewHistory { polished: false, text, raw: text, app, duration_ms: ms, words, fixes: Fixes::default(), transform: None }).unwrap();
    }

    fn set_day(db: &Db, day: NaiveDate) {
        let c = db.conn.lock();
        c.execute("UPDATE history SET created_at = ?1 || substr(created_at, 11)", [day.to_string()]).unwrap();
        c.execute("UPDATE meetings SET started_at = ?1 || substr(started_at, 11)", [day.to_string()]).unwrap();
    }

    #[test]
    fn pendientes_solo_dias_terminados_y_como_mucho_una_semana() {
        assert_eq!(pending_days(day("2026-11-03"), day("2026-11-04")), Vec::<NaiveDate>::new());
        assert_eq!(pending_days(day("2026-11-02"), day("2026-11-04")), vec![day("2026-11-03")]);
        assert_eq!(pending_days(day("2026-11-09"), day("2026-11-04")), Vec::<NaiveDate>::new(), "reloj atrasado");
        let long = pending_days(day("2026-01-01"), day("2026-11-10"));
        assert_eq!((long.len(), long[0], *long.last().unwrap()), (7, day("2026-11-03"), day("2026-11-09")));
    }

    #[test]
    fn nsn_privacidad_uso_solo_sale_la_lista_cerrada() {
        let db = Db::open_memory().unwrap();
        db.usage_start(day("2026-10-31")).unwrap();
        dictate(&db, SECRET, 120, 90_000, "secreto-app.exe");
        let m = db.create_meeting(SECRET, Some("secreto-reunion")).unwrap();
        db.finish_meeting(m.id, 600_000).unwrap();
        set_day(&db, day("2026-11-01"));
        let (url, bodies) = server(200);
        assert_eq!(send_pending(&db, &url, WEB, day("2026-11-02"), &|| true), Ok(1));
        let body = bodies.lock().unwrap()[0].clone();
        assert!(!body.to_lowercase().contains("secreto"), "salió texto del usuario: {body}");
        let v: Value = serde_json::from_str(&body).unwrap();
        let p = &v["payload"];
        let keys = |o: &Value| o.as_object().unwrap().keys().cloned().collect::<BTreeSet<_>>();
        let want = |l: &[&str]| l.iter().map(|s| s.to_string()).collect::<BTreeSet<_>>();
        assert_eq!(keys(&v), want(&["type", "payload"]));
        assert_eq!(keys(p), want(&["website", "hostname", "url", "name", "id", "ip", "timestamp", "data"]));
        assert_eq!(keys(&p["data"]), want(&["dia", "version", "palabras", "segundos_dictado", "dictados", "reuniones", "minutos_reunion", "app_principal"]));
        assert_eq!(p["ip"], NO_LOCATION_IP);
        assert_eq!(p["timestamp"], 1793534400, "mediodía del día contado, no la hora del envío");
        let d = &p["data"];
        assert_eq!((d["dia"].as_str(), d["palabras"].as_i64(), d["segundos_dictado"].as_i64(), d["dictados"].as_i64(), d["reuniones"].as_i64(), d["minutos_reunion"].as_i64()), (Some("2026-11-01"), Some(120), Some(90), Some(1), Some(1), Some(10)));
        assert_eq!(d["app_principal"], "otro", "un programa fuera de la lista no dice su nombre");
    }

    #[test]
    fn nsn_privacidad_uso_programa_principal_solo_de_la_lista() {
        let apps = |l: &[(&str, i64)]| l.iter().map(|(a, w)| (a.to_string(), *w)).collect::<Vec<_>>();
        assert_eq!(main_app(&apps(&[("Chrome", 10), ("Winword", 30)])), "Word");
        assert_eq!(main_app(&apps(&[("Teams", 10), ("Ms-teams", 15), ("Chrome", 20)])), "Teams", "dos nombres del mismo programa suman");
        assert_eq!(main_app(&apps(&[("ClienteGarciaERP", 90), ("Chrome", 10)])), "otro");
        assert_eq!(main_app(&apps(&[("Mi-factura-2026", 1), ("Otro-privado", 1)])), "otro", "lo desconocido suma junto y nunca sale su nombre");
        assert_eq!(main_app(&[]), "ninguno");
        for raro in [r"C:\Users\ana\Documents\chrome", "Chrome - Factura García.pdf", "chrome\u{200b}", "ｃｈｒｏｍｅ", "chrome ", "chrome.exe", ""] {
            assert_eq!(known_app(raro), "otro", "{raro:?}");
        }
    }

    #[test]
    fn nsn_privacidad_uso_sin_si_no_hay_codigo_ni_envio() {
        let db = Db::open_memory().unwrap();
        dictate(&db, "hola", 1, 1000, "");
        let (url, bodies) = server(200);
        assert_eq!(send_pending(&db, &url, WEB, day("2026-12-01"), &|| true), Ok(0), "sin código no se envía aunque el ajuste dijera sí");
        assert_eq!(db.usage_id().unwrap(), None);
        db.usage_start(day("2026-10-31")).unwrap();
        assert_eq!(send_pending(&db, &url, WEB, day("2026-11-05"), &|| false), Ok(0), "permiso retirado");
        assert!(bodies.lock().unwrap().is_empty());
    }

    #[test]
    fn nsn_privacidad_uso_lo_anterior_al_si_no_sale_ni_el_mismo_dia() {
        let db = Db::open_memory().unwrap();
        dictate(&db, "de ayer", 500, 60_000, "");
        set_day(&db, day("2026-10-31"));
        dictate(&db, "de esta mañana", 700, 60_000, "");
        db.conn.lock().execute("UPDATE history SET created_at = '2026-11-01' || substr(created_at, 11) WHERE words = 700", []).unwrap();
        db.usage_start(day("2026-11-01")).unwrap();
        let (url, bodies) = server(200);
        assert_eq!(send_pending(&db, &url, WEB, day("2026-11-02"), &|| true), Ok(0), "el día de aceptar no cuenta");
        assert!(bodies.lock().unwrap().is_empty());
        assert_eq!(send_pending(&db, &url, WEB, day("2026-11-03"), &|| true), Ok(1));
        let v: Value = serde_json::from_str(&bodies.lock().unwrap()[0]).unwrap();
        assert_eq!((v["payload"]["data"]["palabras"].as_i64(), v["payload"]["data"]["dia"].as_str()), (Some(0), Some("2026-11-02")));
    }

    #[test]
    fn nsn_privacidad_uso_retirar_borra_el_codigo_y_volver_da_otro() {
        let db = Db::open_memory().unwrap();
        let hoy = day("2026-11-01");
        db.usage_sync(None, Some(true), hoy).unwrap();
        let first = db.usage_id().unwrap().unwrap();
        db.usage_sync(Some(true), Some(true), day("2026-11-02")).unwrap();
        assert_eq!(db.usage_id().unwrap().as_deref(), Some(first.as_str()), "repetir el sí no cambia nada");
        assert_eq!(db.usage_state().unwrap().unwrap().1, hoy);
        db.usage_sync(Some(true), Some(false), hoy).unwrap();
        assert_eq!(db.usage_id().unwrap(), None);
        assert_eq!(db.usage_state().unwrap(), None);
        db.usage_sync(Some(false), Some(true), hoy).unwrap();
        assert_ne!(db.usage_id().unwrap().unwrap(), first);
    }

    #[test]
    fn nsn_privacidad_uso_un_no_con_codigo_viejo_y_un_si_sin_codigo_se_arreglan_al_arrancar() {
        let db = Db::open_memory().unwrap();
        assert_eq!(db.usage_reconcile(Some(true)).unwrap(), None, "sí sin código: se vuelve a preguntar");
        assert_eq!(db.usage_reconcile(None).unwrap(), None);
        db.usage_start(day("2026-11-01")).unwrap();
        assert_eq!(db.usage_reconcile(Some(true)).unwrap(), Some(true), "lo sano no se toca");
        assert!(db.usage_id().unwrap().is_some());
        assert_eq!(db.usage_reconcile(Some(false)).unwrap(), Some(false));
        assert_eq!(db.usage_id().unwrap(), None, "código sin sí: fuera");
        db.usage_start(day("2026-11-01")).unwrap();
        db.conn.lock().execute("UPDATE meta SET value = 'roto' WHERE key = 'uso_hasta'", []).unwrap();
        assert_eq!(db.usage_reconcile(Some(true)).unwrap(), None, "fecha dañada: se vuelve a preguntar");
        assert_eq!(db.usage_id().unwrap(), None);
    }

    #[test]
    fn nsn_privacidad_uso_las_copias_no_guardan_el_codigo() {
        let dir = std::env::temp_dir().join(format!("pronto-uso-copia-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = Db::open_memory().unwrap();
        dictate(&db, "x", 1, 1000, "");
        db.usage_start(day("2026-11-01")).unwrap();
        let copy = db.daily_backup(&dir, 2).unwrap().unwrap();
        let restored = Db::open_at(&copy).unwrap();
        assert_eq!(restored.usage_id().unwrap(), None);
        assert_eq!(restored.usage_reconcile(Some(true)).unwrap(), None, "una copia restaurada no revive el sí");
        assert!(db.usage_id().unwrap().is_some(), "la base viva no se toca");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nsn_privacidad_uso_retirar_con_un_envio_en_vuelo_no_deja_restos() {
        let db = Db::open_memory().unwrap();
        db.usage_start(day("2026-10-31")).unwrap();
        let old = db.usage_id().unwrap().unwrap();
        db.usage_stop().unwrap();
        db.usage_start(day("2026-11-05")).unwrap();
        db.usage_mark_sent(&old, day("2026-11-01")).unwrap();
        assert_eq!(db.usage_state().unwrap().unwrap().1, day("2026-11-05"), "un envío del código viejo no mueve la fecha nueva");
    }

    #[test]
    fn nsn_privacidad_uso_no_parece_un_robot_y_lo_descartado_sigue_pendiente() {
        assert!(user_agent().starts_with("Mozilla/5.0 (Windows NT 10.0;"), "Umami tira sin avisar los `Nombre/1.2.3`");
        let db = Db::open_memory().unwrap();
        db.usage_start(day("2026-10-31")).unwrap();
        let (url, _) = server_with(200, "", "{\"beep\":\"boop\"}");
        assert_eq!(send_pending(&db, &url, WEB, day("2026-11-02"), &|| true), Err("descartado por Umami".into()));
        assert_eq!(db.usage_state().unwrap().unwrap().1, day("2026-10-31"));
    }

    #[test]
    fn nsn_privacidad_uso_retirar_a_mitad_de_lote_corta_lo_que_falta() {
        let db = Db::open_memory().unwrap();
        db.usage_start(day("2026-10-31")).unwrap();
        for (n, d) in [(1, "2026-11-01"), (2, "2026-11-02")] {
            dictate(&db, "x", n, 1000, "");
            db.conn.lock().execute("UPDATE history SET created_at = ?1 || substr(created_at, 11) WHERE words = ?2", rusqlite::params![d, n]).unwrap();
        }
        let (url, bodies) = server(200);
        let calls = Mutex::new(0);
        let allowed = || { let mut c = calls.lock().unwrap(); *c += 1; *c <= 2 };
        assert_eq!(send_pending(&db, &url, WEB, day("2026-11-03"), &allowed), Ok(1));
        assert_eq!(bodies.lock().unwrap().len(), 1);
    }

    #[test]
    fn un_fallo_deja_el_dia_pendiente_y_no_se_repite_lo_enviado() {
        let db = Db::open_memory().unwrap();
        db.usage_start(day("2026-10-31")).unwrap();
        dictate(&db, "x", 3, 1000, "");
        set_day(&db, day("2026-11-01"));
        let (bad, _) = server(500);
        assert_eq!(send_pending(&db, &bad, WEB, day("2026-11-02"), &|| true), Err("respuesta 500".into()));
        let (good, bodies) = server(200);
        assert_eq!(send_pending(&db, &good, WEB, day("2026-11-02"), &|| true), Ok(1));
        assert_eq!(send_pending(&db, &good, WEB, day("2026-11-02"), &|| true), Ok(0));
        assert_eq!(bodies.lock().unwrap().len(), 1);
        assert_eq!(send_pending(&db, "http://127.0.0.1:9/api/send", WEB, day("2026-11-03"), &|| true), Err("sin conexión".into()));
    }

    #[test]
    fn varios_dias_sin_uso_salen_en_un_solo_envio_a_cero() {
        let db = Db::open_memory().unwrap();
        db.usage_start(day("2026-10-31")).unwrap();
        let (url, bodies) = server(200);
        assert_eq!(send_pending(&db, &url, WEB, day("2026-11-05"), &|| true), Ok(1));
        assert_eq!(bodies.lock().unwrap().len(), 1);
        assert_eq!(db.usage_state().unwrap().unwrap().1, day("2026-11-04"));
    }

    #[test]
    fn nsn_privacidad_uso_una_redireccion_no_se_sigue() {
        let db = Db::open_memory().unwrap();
        db.usage_start(day("2026-10-31")).unwrap();
        let (other, reached) = server(200);
        let (url, _) = server_with(302, &format!("Location: {other}\r\n"), "");
        assert_eq!(send_pending(&db, &url, WEB, day("2026-11-02"), &|| true), Err("respuesta 302".into()));
        assert!(reached.lock().unwrap().is_empty(), "las cifras solo van al destino escrito en el código");
    }
}
