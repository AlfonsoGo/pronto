//! Sondas del rompedor (2026-10-22, tanda paralela): descargas. Solo se compilan en pruebas.
//! Las que demuestran una rotura llevan #[ignore = "rompedor: <título>"].
//! Zona: actualizador (`updater.rs`, `update_helper.ps1`), modelos (`modeldl.rs`, `whisper.rs`) e IA local
//! (`aisetup.rs`). Todo contra servidores TCP propios en 127.0.0.1 y carpetas de `%TEMP%`.
#![allow(dead_code, unused_imports)]

use crate::updater::{self, Asset};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

// ------------------------------------------------------------ utilidades

struct Tmp(PathBuf);

impl Tmp {
    fn new(name: &str) -> Self {
        let d = std::env::temp_dir().join(format!("pronto-rompedor-descargas-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Tmp(d)
    }
    fn join(&self, p: &str) -> PathBuf {
        self.0.join(p)
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        if std::env::var("PRONTO_ROMPEDOR_CONSERVAR").is_err() {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

/// Servidor HTTP mínimo en 127.0.0.1: `responder(ruta)` devuelve la respuesta cruda (cabeceras + cuerpo).
fn servidor(responder: impl Fn(&str) -> Vec<u8> + Send + 'static) -> (String, u16, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let hits = Arc::new(AtomicUsize::new(0));
    let h = hits.clone();
    std::thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            h.fetch_add(1, Ordering::SeqCst);
            let mut reader = BufReader::new(conn.try_clone().unwrap());
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() {
                continue;
            }
            loop {
                let mut hd = String::new();
                if reader.read_line(&mut hd).is_err() || hd.trim().is_empty() {
                    break;
                }
            }
            let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
            let mut conn = conn;
            let _ = conn.write_all(&responder(&path));
            let _ = conn.flush();
        }
    });
    (format!("http://127.0.0.1:{port}"), port, hits)
}

fn ok(body: &[u8]) -> Vec<u8> {
    let mut r = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).into_bytes();
    r.extend_from_slice(body);
    r
}

/// Agente igual que `updater::agent()` (privado): mismos plazos, redirecciones por defecto (5).
fn agente() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(30))
        .user_agent("Pronto/2.2.0 (actualizador; rompedor)")
        .build()
}

fn instalador_falso(n: usize) -> Vec<u8> {
    let mut b = b"MZ".to_vec();
    b.extend((0..n).map(|i| (i % 251) as u8));
    b
}

fn sha(b: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}

fn ficheros(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir).map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
    v.sort();
    v
}

// ============================================================ actualizador: redirecciones

/// `url_allowed` solo se aplica a la URL del asset (`updater.rs:732`); `fetch_installer` (`:576`) deja
/// que ureq siga hasta 5 redirecciones a cualquier sitio. Aquí el asset apunta a una URL permitida y
/// el servidor redirige a `http://127.1:<otro puerto>`, que `url_allowed` rechaza (es 127.0.0.1 con
/// otra escritura: nada sale del PC). Sin `digest`, el .exe de la redirección se da por bueno.
#[test]
fn rompedor_descargas_redireccion_salta_url_allowed() {
    let t = Tmp::new("redir");
    let malo = instalador_falso(4096);
    let malo2 = malo.clone();
    let (_, port_b, hits_b) = servidor(move |_| ok(&malo2));
    let destino = format!("http://127.1:{port_b}/otro-sitio/malo.exe");
    let loc = destino.clone();
    let (url_a, _, hits_a) = servidor(move |_| format!("HTTP/1.1 302 Found\r\nLocation: {loc}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").into_bytes());
    let asset = Asset { name: "Pronto-Setup-9.9.9.exe".into(), url: format!("{url_a}/setup.exe"), size: malo.len() as u64, sha256: None };
    let permitida = updater::url_allowed(&asset.url, true);
    let redirigida = updater::url_allowed(&destino, true);
    let r = updater::fetch_installer(&agente(), &asset, &t.0, &mut |_, _, _| {});
    eprintln!(
        "ROMPEDOR redireccion: url_allowed(asset)={permitida} url_allowed(destino)={redirigida} · peticiones A={} B={} · fetch_installer={:?} · en disco={:?}",
        hits_a.load(Ordering::SeqCst),
        hits_b.load(Ordering::SeqCst),
        r.as_ref().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()),
        ficheros(&t.0)
    );
    // Con digest el SHA-256 lo para; sin él solo queda «tamaño + MZ».
    let mut con_digest = asset.clone();
    con_digest.sha256 = Some(sha(b"otro contenido"));
    let r2 = updater::fetch_installer(&agente(), &con_digest, &t.0, &mut |_, _, _| {});
    eprintln!("ROMPEDOR redireccion: con digest de otro fichero -> {:?}", r2.as_ref().err());
    assert!(permitida && !redirigida);
    assert!(r.is_err(), "se descargó y se dio por bueno un instalador de un host que url_allowed rechaza");
}

/// Redirección a esquemas que no son http(s), bucle de redirecciones y redirección sin Location.
#[test]
fn rompedor_descargas_redirecciones_raras_aguantan() {
    let t = Tmp::new("redir-raras");
    let mut casos: Vec<(&str, String)> = Vec::new();
    for loc in ["ftp://127.0.0.1/x.exe", "\\\\127.0.0.1\\c$\\x.exe", "http://", ""] {
        let l = loc.to_string();
        let (url, _, _) = servidor(move |_| {
            if l.is_empty() {
                b"HTTP/1.1 302 Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()
            } else {
                format!("HTTP/1.1 302 Found\r\nLocation: {l}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").into_bytes()
            }
        });
        let asset = Asset { name: "Pronto-Setup-9.9.9.exe".into(), url: format!("{url}/s.exe"), size: 10, sha256: None };
        let r = updater::fetch_installer(&agente(), &asset, &t.0, &mut |_, _, _| {});
        casos.push((loc, format!("{:?}", r.as_ref().err())));
        assert!(r.is_err(), "{loc}");
    }
    // Bucle: A -> A
    let (_, port, hits) = servidor(move |p| format!("HTTP/1.1 302 Found\r\nLocation: {p}x\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").into_bytes());
    let asset = Asset { name: "Pronto-Setup-9.9.9.exe".into(), url: format!("http://127.0.0.1:{port}/b"), size: 10, sha256: None };
    let r = updater::fetch_installer(&agente(), &asset, &t.0, &mut |_, _, _| {});
    casos.push(("bucle", format!("{:?} peticiones={}", r.as_ref().err(), hits.load(Ordering::SeqCst))));
    assert!(r.is_err());
    for c in &casos {
        eprintln!("ROMPEDOR redireccion_rara: {} -> {}", c.0, c.1);
    }
    assert!(ficheros(&t.0).is_empty(), "{:?}", ficheros(&t.0));
}

// ============================================================ actualizador: nombres de asset

/// Nombres hostiles en `assets[].name` (JSON de la release) -> `safe_file_name` -> `fetch_installer`.
/// Comprueba dónde acaba el fichero y si la descarga es posible.
#[test]
fn rompedor_descargas_nombres_de_asset_hostiles() {
    let t = Tmp::new("nombres");
    let body = instalador_falso(1000);
    let b2 = body.clone();
    let (url, _, _) = servidor(move |_| ok(&b2));
    let largo = format!("{}.exe", "x".repeat(250));
    let nombres: Vec<String> = [
        "..\\..\\evil.exe", "../../evil.exe", "C:\\Windows\\Temp\\evil.exe", "C:evil.exe", "\\\\?\\C:\\evil.exe", "\\\\127.0.0.1\\c$\\evil.exe",
        "CON.exe", "NUL.exe", "nul.EXE", "COM1.exe", "LPT1.exe", "AUX.exe", "COM\u{b9}.exe", "Pronto[x64].exe", "a`b$c'd;e&f.exe",
        "evil.exe::$DATA", "evil.exe:x.exe", "\u{202e}exe.tad.exe", "Pronto\u{0}.exe", "..exe", " .exe", "evil.exe ", "evil.exe.",
        "Pronto-Setup-2.3.0.exe", "ＰＲＯＮＴＯ.exe",
    ]
    .iter()
    .map(|s| s.to_string())
    .chain([largo])
    .collect();
    let mut fuera = Vec::new();
    let mut fallan = Vec::new();
    for (i, n) in nombres.iter().enumerate() {
        let json = serde_json::json!({ "tag_name": "v9.9.9", "assets": [{ "name": n, "size": body.len(), "browser_download_url": format!("{url}/s.exe") }] }).to_string();
        let Some(rel) = updater::parse_release(&json).unwrap() else {
            eprintln!("ROMPEDOR nombre {n:?} -> sin instalador (no acaba en .exe)");
            continue;
        };
        let sub = t.0.join(format!("c{i}"));
        std::fs::create_dir_all(&sub).unwrap();
        let r = updater::fetch_installer(&agente(), &rel.asset, &sub, &mut |_, _, _| {});
        let estado = match &r {
            Ok(p) => {
                let dentro = p.parent() == Some(sub.as_path()) && std::fs::read(p).map(|b| b == body).unwrap_or(false);
                if !dentro {
                    fuera.push(format!("{n:?} -> {}", p.display()));
                }
                format!("Ok {:?} dentro={dentro}", p.file_name().unwrap())
            }
            Err(e) => {
                fallan.push(format!("{n:?} -> {:?}: {e}", rel.asset.name));
                format!("Err {e}")
            }
        };
        eprintln!("ROMPEDOR nombre {n:?} -> {:?} · {estado} · en disco {:?}", rel.asset.name, ficheros(&sub));
    }
    eprintln!("ROMPEDOR nombres: {} fuera de la carpeta, {} que no se pueden descargar: {fallan:#?}", fuera.len(), fallan.len());
    assert!(fuera.is_empty(), "{fuera:#?}");
}

/// Un asset con nombre de 251-255 caracteres cabe como `.exe` pero no como `.exe.part`: la release no
/// se puede instalar nunca (cada intento falla igual).
#[test]
fn rompedor_descargas_nombre_largo_bloquea_la_actualizacion() {
    let t = Tmp::new("largo");
    let body = instalador_falso(100);
    let b2 = body.clone();
    let (url, _, _) = servidor(move |_| ok(&b2));
    let n = format!("{}.exe", "P".repeat(251 - 4));
    let json = serde_json::json!({ "tag_name": "v9.9.9", "assets": [{ "name": n, "size": body.len(), "browser_download_url": format!("{url}/s.exe") }] }).to_string();
    let rel = updater::parse_release(&json).unwrap().unwrap();
    let r = updater::fetch_installer(&agente(), &rel.asset, &t.0, &mut |_, _, _| {});
    eprintln!("ROMPEDOR nombre_largo: nombre {} car. -> {:?}", rel.asset.name.len(), r.as_ref().err());
    assert!(r.is_ok());
}

// ============================================================ actualizador: respuestas raras

#[test]
fn rompedor_descargas_respuestas_http_raras_aguantan() {
    let t = Tmp::new("respuestas");
    let body = instalador_falso(10_000);
    let n = body.len();
    let casos: Vec<(&str, Vec<u8>)> = vec![
        ("206 parcial", {
            let mut r = format!("HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-99/{n}\r\nContent-Length: 100\r\nConnection: close\r\n\r\n").into_bytes();
            r.extend_from_slice(&body[..100]);
            r
        }),
        ("304 sin cuerpo", b"HTTP/1.1 304 Not Modified\r\nConnection: close\r\n\r\n".to_vec()),
        ("500", b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 2\r\nConnection: close\r\n\r\nno".to_vec()),
        ("Content-Length mayor que el cuerpo", {
            let mut r = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", n * 2).into_bytes();
            r.extend_from_slice(&body);
            r
        }),
        ("Content-Length menor que el cuerpo", {
            let mut r = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", n / 2).into_bytes();
            r.extend_from_slice(&body);
            r
        }),
        ("Content-Length basura", {
            let mut r = b"HTTP/1.1 200 OK\r\nContent-Length: -5, 9999999999999999999999\r\nConnection: close\r\n\r\n".to_vec();
            r.extend_from_slice(&body);
            r
        }),
        ("gzip (ureq sin la feature: no descomprime)", {
            let mut r = format!("HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: {n}\r\nConnection: close\r\n\r\n").into_bytes();
            let mut gz = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 0xff];
            gz.resize(n, 0);
            r.extend_from_slice(&gz);
            r
        }),
        ("chunked roto", b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\nZZ\r\nMZ\r\n".to_vec()),
        ("cabecera sin terminar", b"HTTP/1.1 200 OK\r\nContent-Len".to_vec()),
        ("no es HTTP", b"SSH-2.0-OpenSSH\r\n".to_vec()),
    ];
    for (nombre, resp) in casos {
        let (url, _, _) = servidor(move |_| resp.clone());
        let asset = Asset { name: "Pronto-Setup-9.9.9.exe".into(), url: format!("{url}/s.exe"), size: n as u64, sha256: Some(sha(&body)) };
        let t0 = Instant::now();
        let r = updater::fetch_installer(&agente(), &asset, &t.0, &mut |_, _, _| {});
        eprintln!("ROMPEDOR respuesta[{nombre}] -> {:?} en {:?}", r.as_ref().err(), t0.elapsed());
        // «Content-Length basura»: ureq lo ignora y lee hasta el cierre; el cuerpo es el bueno.
        if nombre == "Content-Length basura" {
            assert!(r.is_ok(), "{nombre}");
            let _ = std::fs::remove_dir_all(&t.0);
            std::fs::create_dir_all(&t.0).unwrap();
            continue;
        }
        assert!(r.is_err(), "{nombre}");
        assert!(ficheros(&t.0).is_empty(), "{nombre}: {:?}", ficheros(&t.0));
    }
}

// ============================================================ actualizador: JSON hostil

#[test]
fn rompedor_descargas_json_de_release_hostil() {
    let profundo = format!("{{\"tag_name\":\"v9.0.0\",\"x\":{}1{}}}", "[".repeat(100_000), "]".repeat(100_000));
    let notas_regex = format!(
        "{{\"tag_name\":\"v9.0.0\",\"assets\":[{{\"name\":\"a.exe\",\"size\":1,\"browser_download_url\":\"https://github.com/a.exe\"}}],\"body\":{}}}",
        serde_json::to_string(&"**[![<`*_".repeat(200_000)).unwrap()
    );
    let lineas = format!(
        "{{\"tag_name\":\"v9.0.0\",\"assets\":[{{\"name\":\"a.exe\",\"size\":1,\"browser_download_url\":\"https://github.com/a.exe\"}}],\"body\":{}}}",
        serde_json::to_string(&"- a\n".repeat(450_000)).unwrap()
    );
    let titulo = format!(
        "{{\"tag_name\":\"v9.0.0\",\"name\":{},\"html_url\":\"javascript:alert(1)\",\"assets\":[{{\"name\":\"a.exe\",\"size\":1,\"browser_download_url\":\"https://github.com/a.exe\"}}]}}",
        serde_json::to_string(&"T".repeat(1_900_000)).unwrap()
    );
    let casos: Vec<(&str, String)> = vec![
        ("anidado 100000", profundo),
        ("notas con 1,8 MB de marcas", notas_regex),
        ("notas con 450000 líneas", lineas),
        ("título de 1,9 MB y html_url javascript:", titulo),
        ("claves duplicadas", r#"{"tag_name":"v1.0.0","tag_name":"v99.0.0","assets":[]}"#.into()),
        ("tag con NUL", "{\"tag_name\":\"v9.0.0\\u0000\",\"assets\":[]}".into()),
        ("tag con dígitos árabes", r#"{"tag_name":"v٩.0.0","assets":[]}"#.into()),
        ("tag desbordado", r#"{"tag_name":"v18446744073709551616.0.0","assets":[]}"#.into()),
        ("tag u64::MAX", r#"{"tag_name":"v18446744073709551615.0.0","assets":[{"name":"a.exe","size":1,"browser_download_url":"https://github.com/a.exe"}]}"#.into()),
        ("tag con +", r#"{"tag_name":"+9.0.0","assets":[]}"#.into()),
        ("size negativo", r#"{"tag_name":"v9.0.0","assets":[{"name":"a.exe","size":-1,"browser_download_url":"https://github.com/a.exe"}]}"#.into()),
        ("size flotante", r#"{"tag_name":"v9.0.0","assets":[{"name":"a.exe","size":1e3,"browser_download_url":"https://github.com/a.exe"}]}"#.into()),
        ("assets no es lista", r#"{"tag_name":"v9.0.0","assets":{"name":"a.exe"}}"#.into()),
        ("name null", r#"{"tag_name":"v9.0.0","assets":[{"name":null,"size":1,"browser_download_url":"x"}]}"#.into()),
        ("BOM delante", "\u{feff}{\"tag_name\":\"v9.0.0\",\"assets\":[]}".into()),
    ];
    for (nombre, json) in casos {
        let j = json.clone();
        let (url, _, _) = servidor(move |_| ok(j.as_bytes()));
        let t0 = Instant::now();
        let r = updater::fetch_latest(&format!("{url}/latest"));
        let dt = t0.elapsed();
        let res = match &r {
            Ok(Some(rel)) => format!("Ok v{} título {} car. notas {} car. page_url {:?}", rel.version, rel.title.len(), rel.notes.chars().count(), rel.page_url),
            Ok(None) => "Ok(None)".into(),
            Err(e) => format!("Err {}", e.msg.chars().take(120).collect::<String>()),
        };
        eprintln!("ROMPEDOR json[{nombre}] ({} B) -> {res} en {dt:?}", json.len());
        assert!(dt < Duration::from_secs(5), "{nombre}: {dt:?}");
    }
}

/// `digest` presente pero mal formado (otro prefijo, otra longitud) = sin digest: se instala con solo
/// tamaño y «MZ». Es la variante «mal formado» de lo ya conocido (seguridad M1). No cuenta.
#[test]
#[ignore = "rompedor: (conocido M1) un digest mal formado se trata como ausente"]
fn rompedor_descargas_digest_mal_formado_baja_sin_verificar() {
    let t = Tmp::new("digest");
    let bueno = instalador_falso(500);
    let mut cambiado = bueno.clone();
    cambiado[300] ^= 0xff;
    let c2 = cambiado.clone();
    let (url, _, _) = servidor(move |_| ok(&c2));
    let h = sha(&bueno);
    let mut aceptados = Vec::new();
    for d in [format!("SHA256:{h}"), format!("sha-256:{h}"), format!("sha256:{}", &h[..63]), format!("sha256: {h} x"), format!("sha512:{h}{h}")] {
        let json = serde_json::json!({ "tag_name": "v9.9.9", "assets": [{ "name": "P.exe", "size": bueno.len(), "digest": d, "browser_download_url": format!("{url}/s.exe") }] }).to_string();
        let rel = updater::parse_release(&json).unwrap().unwrap();
        let r = updater::fetch_installer(&agente(), &rel.asset, &t.0, &mut |_, _, _| {});
        eprintln!("ROMPEDOR digest {d:.20}… -> sha256={:?} fetch={:?}", rel.asset.sha256, r.as_ref().err());
        if r.is_ok() {
            aceptados.push(d);
        }
    }
    assert!(aceptados.is_empty(), "instalador cambiado aceptado con {} digests mal formados", aceptados.len());
}

// ============================================================ ayudante (update_helper.ps1) con juguetes

const JUGUETE: &str = r#"#![windows_subsystem = "windows"]
use std::io::Write;
fn main() {
    let exe = std::env::current_exe().unwrap();
    let dir = exe.parent().unwrap().to_path_buf();
    let stem = exe.file_stem().unwrap().to_string_lossy().to_lowercase();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(dir.join(format!("{stem}.marcas.txt"))).unwrap();
    writeln!(f, "pid={}\targs={:?}", std::process::id(), args).unwrap();
    drop(f);
    let ms: u64 = if stem.starts_with("pronto") { 15000 } else if stem.starts_with("dummy") { 2000 } else { 0 };
    std::thread::sleep(std::time::Duration::from_millis(ms));
}
"#;

/// Compila el .exe de juguete (sin ventana): como `pronto.exe` deja una marca y se queda 15 s; como
/// `dummy.exe` vive 2 s (el «Pronto» que se cierra); con otro nombre (el «instalador») marca y sale con 0.
fn juguete(dir: &Path) -> PathBuf {
    let src = dir.join("juguete.rs");
    std::fs::write(&src, JUGUETE).unwrap();
    let exe = dir.join("juguete.exe");
    let st = Command::new("rustc").arg("-O").arg("-o").arg(&exe).arg(&src).status().expect("rustc");
    assert!(st.success());
    exe
}

fn matar_juguetes(dir: &Path) {
    let Ok(t) = std::fs::read_to_string(dir.join("pronto.marcas.txt")) else { return };
    for l in t.lines() {
        if let Some(pid) = l.strip_prefix("pid=").and_then(|r| r.split('\t').next()) {
            let _ = Command::new("taskkill").args(["/F", "/PID", pid]).output();
        }
    }
}

/// `spawn_helper` (pub) + el `update_helper.ps1` real con rutas raras: espacios, comillas, `$`,
/// corchetes (comodines de PowerShell), acento grave (escape de comodines), unicode, `;&,%#()!`.
/// Todo con juguetes en `%TEMP%`. Nunca toca el Pronto instalado. Los corchetes, aparte
/// (`rompedor_descargas_ayudante_corchetes_en_la_carpeta`).
#[test]
fn rompedor_descargas_ayudante_con_rutas_raras() {
    let t = Tmp::new("ayudante");
    let toy = juguete(&t.0);
    let variantes = ["normal", "esp acio", "O'Brien", "José ñ 日本", "dolar $env $x", "grave `x", "coma, y; amp&", "100% #1 (x86) !"];
    let mut fallos = Vec::new();
    for v in variantes {
        let base = t.0.join(v);
        let app = base.join("app");
        let work = base.join("work");
        std::fs::create_dir_all(&app).unwrap();
        std::fs::create_dir_all(&work).unwrap();
        let exe = app.join("pronto.exe");
        let installer = work.join(format!("Setup {v}.exe"));
        let dummy = base.join("dummy.exe");
        for p in [&exe, &installer, &dummy] {
            std::fs::copy(&toy, p).unwrap();
        }
        let mut viejo = Command::new(&dummy).spawn().unwrap();
        let t0 = Instant::now();
        let r = updater::spawn_helper(viejo.id(), &installer, &exe, &work);
        let vivo = t0.elapsed();
        let _ = viejo.wait();
        let limite = Instant::now() + Duration::from_secs(25);
        while Instant::now() < limite && !app.join("pronto.marcas.txt").is_file() {
            std::thread::sleep(Duration::from_millis(250));
        }
        std::thread::sleep(Duration::from_secs(4)); // ¿reabre más de una vez?
        let leer = |p: PathBuf| std::fs::read_to_string(p).unwrap_or_default();
        let inst_stem = installer.file_stem().unwrap().to_string_lossy().to_lowercase();
        let instalador = leer(work.join(format!("{inst_stem}.marcas.txt")));
        let reaperturas = leer(app.join("pronto.marcas.txt")).lines().count();
        let resultado = leer(work.join("result.txt")).trim().to_string();
        let log_ok = work.join("update.log").is_file();
        let log_dbg = format!("{:?}", work.join("setup.log").to_string_lossy());
        let args_ok = instalador.contains("/VERYSILENT") && instalador.contains(&log_dbg[1..log_dbg.len() - 1]);
        eprintln!(
            "ROMPEDOR ayudante[{v}]: spawn={:?} en {vivo:?} · instalador ejecutado={} · result.txt={resultado:?} · reaperturas={reaperturas} · update.log={log_ok} · args={}",
            r.as_ref().err(),
            !instalador.is_empty(),
            instalador.trim()
        );
        matar_juguetes(&app);
        if r.is_err() || instalador.is_empty() || resultado != "ok" || reaperturas != 1 || !args_ok {
            fallos.push(format!("{v}: spawn={:?} instalador={} result={resultado:?} reaperturas={reaperturas} args_ok={args_ok}", r.err(), !instalador.is_empty()));
        }
    }
    eprintln!("ROMPEDOR ayudante: fallos {fallos:#?}");
    assert!(fallos.is_empty(), "{fallos:#?}");
}

// ============================================================ modelos: rutas elegidas a mano

/// Hijo: carga Parakeet desde `PRONTO_ROMPEDOR_HIJO_DIR` (lo lanza la sonda de abajo en otro proceso,
/// porque sherpa-onnx puede matar el proceso).
#[test]
fn rompedor_descargas_hijo_carga_parakeet() {
    let Ok(dir) = std::env::var("PRONTO_ROMPEDOR_HIJO_DIR") else { return };
    eprintln!("HIJO: dir {} car. · model_present={} · verify={:?}", dir.chars().count(), crate::modeldl::model_present(&dir), crate::modeldl::verify(Path::new(&dir), &crate::modeldl::FILES).err());
    let r = crate::asr::Engine::load_for_tests(&dir);
    eprintln!("HIJO: load -> {:?}", r.as_ref().err());
    if let Ok(e) = &r {
        eprintln!("HIJO: transcribe(1 s de silencio) -> {:?}", e.transcribe(&vec![0.0; 16_000]));
    }
    assert!(r.is_ok());
}

/// Hijo: carga un `.bin` de Whisper desde `PRONTO_ROMPEDOR_HIJO_BIN` con la misma llamada que
/// `whisper.rs:217` (`WhisperContext::new_with_params`), en CPU.
#[test]
fn rompedor_descargas_hijo_carga_whisper() {
    let Ok(bin) = std::env::var("PRONTO_ROMPEDOR_HIJO_BIN") else { return };
    let mut cp = whisper_rs::WhisperContextParameters::default();
    cp.use_gpu(false);
    let r = whisper_rs::WhisperContext::new_with_params(&bin, cp);
    eprintln!("HIJO: whisper {} -> {:?}", bin, r.as_ref().err());
    assert!(r.is_ok());
}

fn lanzar_hijo(test: &str, env: (&str, &str)) -> (Option<i32>, String) {
    let out = Command::new(std::env::current_exe().unwrap())
        .args([test, "--exact", "--nocapture", "--test-threads=1"])
        .env(env.0, env.1)
        .output()
        .unwrap();
    let txt = String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    let lineas: Vec<&str> = txt.lines().filter(|l| l.contains("HIJO") || l.contains("rror") || l.contains("fatal") || l.contains("Failed") || l.contains("open")).collect();
    (out.status.code(), lineas.join(" | "))
}

/// Carpeta del modelo con rutas que Rust lee bien (y por tanto `verify` aprueba) pero que la librería
/// nativa quizá no: no ASCII (el perfil de «José» en un Windows con página de códigos 1252), > 260
/// caracteres, prefijo `\\?\` y junta (mklink /J). `PRONTO_ROMPEDOR_MODELO` = carpeta de Parakeet
/// (solo se lee y se copia a `%TEMP%`).
#[test]
fn rompedor_descargas_modelo_en_rutas_raras() {
    let Ok(real) = std::env::var("PRONTO_ROMPEDOR_MODELO") else {
        println!("ROMPEDOR_OMITIDO: falta PRONTO_ROMPEDOR_MODELO");
        return;
    };
    let t = Tmp::new("rutas-modelo");
    let base = t.join("base");
    std::fs::create_dir_all(&base).unwrap();
    for f in &crate::modeldl::FILES {
        std::fs::copy(Path::new(&real).join(f.name), base.join(f.name)).unwrap();
    }
    let enlazar = |dst: &Path| {
        std::fs::create_dir_all(dst).unwrap();
        for f in &crate::modeldl::FILES {
            std::fs::hard_link(base.join(f.name), dst.join(f.name)).unwrap();
        }
    };
    let mut casos: Vec<(String, String)> = Vec::new();
    casos.push(("ascii".into(), base.to_string_lossy().into_owned()));
    let no_ascii = t.join("Usuarios").join("José Muñoz").join("parakeet");
    enlazar(&no_ascii);
    casos.push(("no ASCII (é, ñ)".into(), no_ascii.to_string_lossy().into_owned()));
    let mut largo = t.0.clone();
    while largo.to_string_lossy().len() < 280 {
        largo = largo.join("carpeta-muy-larga-de-modelos");
    }
    enlazar(&largo);
    casos.push((format!("larga ({} car.)", largo.to_string_lossy().len()), largo.to_string_lossy().into_owned()));
    casos.push(("\\\\?\\ verbatim".into(), format!("\\\\?\\{}", base.display())));
    let junta = t.join("junta");
    let _ = Command::new("cmd").args(["/C", "mklink", "/J"]).arg(&junta).arg(&base).output();
    casos.push(("junta".into(), junta.to_string_lossy().into_owned()));
    let mut rotos = Vec::new();
    for (nombre, dir) in casos {
        let (code, lineas) = lanzar_hijo("rompedor_descargas::rompedor_descargas_hijo_carga_parakeet", ("PRONTO_ROMPEDOR_HIJO_DIR", &dir));
        eprintln!("ROMPEDOR modelo[{nombre}]: salida={code:?} · {lineas}");
        if code != Some(0) {
            rotos.push(format!("{nombre}: salida {code:?}"));
        }
    }
    let _ = std::fs::remove_dir(&junta);
    assert!(rotos.is_empty(), "{rotos:#?}");
}

/// Igual con Whisper (`PRONTO_ROMPEDOR_WHISPER` = un `.bin` de `%APPDATA%\Pronto2\models\whisper`,
/// que solo se lee y se copia): `model_dir()` cuelga del perfil, así que un usuario «José» lo tiene
/// en una ruta no ASCII.
#[test]
fn rompedor_descargas_whisper_en_ruta_no_ascii() {
    let Ok(real) = std::env::var("PRONTO_ROMPEDOR_WHISPER") else {
        println!("ROMPEDOR_OMITIDO: falta PRONTO_ROMPEDOR_WHISPER");
        return;
    };
    let t = Tmp::new("whisper-ruta");
    let nombre = Path::new(&real).file_name().unwrap().to_owned();
    let ascii = t.join("ascii");
    std::fs::create_dir_all(&ascii).unwrap();
    std::fs::copy(&real, ascii.join(&nombre)).unwrap();
    let raro = t.join("Users").join("José Muñoz").join("AppData").join("Roaming").join("Pronto2").join("models").join("whisper");
    std::fs::create_dir_all(&raro).unwrap();
    std::fs::hard_link(ascii.join(&nombre), raro.join(&nombre)).unwrap();
    let mut rotos = Vec::new();
    for (n, p) in [("ascii", ascii.join(&nombre)), ("no ASCII", raro.join(&nombre))] {
        let (code, lineas) = lanzar_hijo("rompedor_descargas::rompedor_descargas_hijo_carga_whisper", ("PRONTO_ROMPEDOR_HIJO_BIN", &p.to_string_lossy()));
        eprintln!("ROMPEDOR whisper[{n}]: salida={code:?} · {lineas}");
        if code != Some(0) {
            rotos.push(n);
        }
    }
    assert!(rotos.is_empty(), "{rotos:?}");
}

/// `.verificado` hostil en una carpeta elegida a mano (copiada de otro PC o de un USB): líneas con la
/// huella de otro fichero, sin `v2:`, binario, enorme. `verify` no debe dar por bueno un fichero malo.
#[test]
fn rompedor_descargas_marca_verificado_hostil() {
    let t = Tmp::new("marca");
    let malo = b"esto no es tokens\n".repeat(10);
    let files = [crate::modeldl::ModelFile { name: "tokens.txt", sha256: "d58544679ea4bc6ac563d1f545eb7d474bd6cfa467f0a6e2c1dc1c7d37e3c35d", size: malo.len() as u64 }];
    std::fs::write(t.join("tokens.txt"), &malo).unwrap();
    let marcas: Vec<(&str, Vec<u8>)> = vec![
        ("v1 antigua con el hash bueno", format!("tokens.txt\t{}\t0\t{}\n", malo.len(), files[0].sha256).into_bytes()),
        ("v2 con huella inventada", format!("tokens.txt\t{}\t0\tv2:0:0:0:0\t{}\n", malo.len(), files[0].sha256).into_bytes()),
        ("binaria", vec![0xff, 0xfe, 0, 1, 2, 9, 9, 10]),
        ("campos de más", format!("tokens.txt\t{}\t0\tv2:0:0:0:0\t{}\textra\n", malo.len(), files[0].sha256).into_bytes()),
        ("20 MB", vec![b'\t'; 20 << 20]),
    ];
    for (n, m) in marcas {
        std::fs::write(t.join(".verificado"), &m).unwrap();
        let t0 = Instant::now();
        let ic = crate::modeldl::is_complete(&t.0, &files);
        let v = crate::modeldl::verify(&t.0, &files);
        eprintln!("ROMPEDOR marca[{n}]: is_complete={ic} verify={:?} en {:?}", v.as_ref().err().map(|e| e.chars().take(80).collect::<String>()), t0.elapsed());
        assert!(v.is_err(), "{n}");
    }
}

// ============================================================ IA local: URL de Ollama

/// `is_loopback_url` (`aisetup.rs:143`, el aviso de la regla 1) separa el host a mano y no trata `\`
/// como ureq (WHATWG: `\` = `/` en http). `http://otro\@127.0.0.1:11434` pasa por loopback y ureq va
/// a `otro`. Se demuestra la regla de ureq sin salir de 127.0.0.1: con `http://127.0.0.1:P\@localhost:9`
/// ureq conecta a P, no a 9.
#[test]
fn rompedor_descargas_loopback_con_barra_invertida() {
    let (_, port, hits) = servidor(|_| ok(br#"{"models":[{"name":"qwen3:1.7b"}]}"#));
    let prueba = format!("http://127.0.0.1:{port}\\@localhost:9");
    let st = crate::ollama::status(&prueba);
    eprintln!("ROMPEDOR loopback: ureq con {prueba:?} -> reachable={} peticiones al puerto {port}={}", st.reachable, hits.load(Ordering::SeqCst));
    let mut engañan = Vec::new();
    for u in ["http://evil.example\\@127.0.0.1:11434", "http://evil.example:80\\@localhost:11434", "http://evil.example\\@[::1]:11434"] {
        let host = tauri::Url::parse(u).ok().and_then(|x| x.host_str().map(String::from));
        let loop_ = crate::aisetup::is_loopback_url(u);
        eprintln!("ROMPEDOR loopback: {u:?} -> is_loopback_url={loop_} · host real (url/ureq)={host:?}");
        if loop_ && host.as_deref() == Some("evil.example") {
            engañan.push(u);
        }
    }
    // Conservadoras (no engañan): dan «no loopback» aunque lo sean.
    for u in ["http://[::ffff:127.0.0.1]:11434", "http://127.1:11434", "http://LOCALHOST.:11434", "http://127.0.0.1.evil:11434", "http://localhost@evil:11434"] {
        eprintln!("ROMPEDOR loopback: {u:?} -> is_loopback_url={} host={:?}", crate::aisetup::is_loopback_url(u), tauri::Url::parse(u).ok().and_then(|x| x.host_str().map(String::from)));
    }
    assert!(st.reachable && hits.load(Ordering::SeqCst) >= 1, "ureq trata `\\` como `/`");
    assert!(engañan.is_empty(), "{engañan:?}");
}

/// `verify_signature` (pub) con rutas hostiles: no existe, carpeta, vacío, sin firma, > 260 car., unicode.
#[test]
fn rompedor_descargas_firma_con_rutas_hostiles() {
    let t = Tmp::new("firma");
    std::fs::write(t.join("vacio.exe"), b"").unwrap();
    std::fs::write(t.join("MZ ñ 日本.exe"), instalador_falso(5000)).unwrap();
    let mut largo = t.0.clone();
    while largo.to_string_lossy().len() < 280 {
        largo = largo.join("larguísima");
    }
    std::fs::create_dir_all(&largo).unwrap();
    std::fs::write(largo.join("x.exe"), instalador_falso(5000)).unwrap();
    for p in [t.join("no-existe.exe"), t.0.clone(), t.join("vacio.exe"), t.join("MZ ñ 日本.exe"), largo.join("x.exe")] {
        let r = crate::aisetup::verify_signature(&p);
        eprintln!("ROMPEDOR firma[{}…] -> {r:?}", p.to_string_lossy().chars().rev().take(30).collect::<String>().chars().rev().collect::<String>());
        assert!(r.is_err());
    }
}

// ============================================================ redirección a una URL sin host: pánico

/// ureq 2.12.1 (`unit.rs:252`, `host_str().unwrap()`) entra en pánico si una redirección lleva a una
/// URL sin host (`file:`, `data:`, `javascript:`, `mailto:`). Todas las descargas de Pronto usan ureq
/// sin `catch_unwind`: el pánico sale del hilo. Se prueba con las funciones públicas de cada zona.
#[test]
fn rompedor_descargas_redireccion_sin_host_es_panico() {
    let t = Tmp::new("redir-sin-host");
    let mut panicos = Vec::new();
    for loc in ["file:///C:/Windows/win.ini", "data:,MZ", "javascript:alert(1)", "mailto:a@b.c"] {
        let l = loc.to_string();
        let (url, _, _) = servidor(move |_| format!("HTTP/1.1 302 Found\r\nLocation: {l}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").into_bytes());
        let dir = t.0.clone();
        let u = url.clone();
        let casos: Vec<(&str, Box<dyn Fn() -> String + Send + std::panic::RefUnwindSafe>)> = vec![
            ("updater::fetch_latest (hilo pronto-updater, «Buscar ahora»)", Box::new({ let u = u.clone(); move || format!("{:?}", updater::fetch_latest(&format!("{u}/latest")).err().map(|e| e.msg)) })),
            ("updater::fetch_installer (hilo pronto-update)", Box::new({ let u = u.clone(); let d = dir.clone(); move || {
                let a = Asset { name: "P.exe".into(), url: format!("{u}/s.exe"), size: 10, sha256: None };
                format!("{:?}", updater::fetch_installer(&agente(), &a, &d, &mut |_, _, _| {}).err())
            } })),
            ("modeldl::download_all (Parakeet / Whisper)", Box::new({ let u = u.clone(); let d = dir.clone(); move || {
                let f = [crate::modeldl::ModelFile { name: "ggml-x.bin", sha256: "00", size: 10 }];
                format!("{:?}", crate::modeldl::download_all(&d, &u, &f, &mut |_| {}).err())
            } })),
            ("ollama::status (aisetup::status, comprobaciones de la IA)", Box::new({ let u = u.clone(); move || format!("reachable={}", crate::ollama::status(&u).reachable) })),
        ];
        for (n, f) in casos {
            let r = std::thread::spawn(move || f()).join();
            let s = match &r {
                Ok(v) => format!("sin pánico: {v}"),
                Err(e) => {
                    panicos.push(format!("{loc} -> {n}"));
                    format!("PÁNICO: {:?}", e.downcast_ref::<String>().map(String::as_str).or(e.downcast_ref::<&str>().copied()))
                }
            };
            eprintln!("ROMPEDOR sin_host[{loc}] {n}: {s}");
        }
    }
    assert!(panicos.is_empty(), "{panicos:#?}");
}

// ============================================================ ayudante: corchetes, aislado

struct Ayudante {
    spawn: Result<(), String>,
    instalador: bool,
    resultado: String,
    reaperturas: usize,
    log: String,
}

fn probar_ayudante(toy: &Path, base: &Path, app_dir: &str, work_dir: &str, inst: &str) -> Ayudante {
    let app = app_dir.split('/').fold(base.to_path_buf(), |p, c| p.join(c));
    let work = work_dir.split('/').fold(base.to_path_buf(), |p, c| p.join(c));
    std::fs::create_dir_all(&app).unwrap();
    std::fs::create_dir_all(&work).unwrap();
    let exe = app.join("pronto.exe");
    let installer = work.join(inst);
    let dummy = base.join("dummy.exe");
    for p in [&exe, &installer, &dummy] {
        std::fs::copy(toy, p).unwrap();
    }
    let mut viejo = Command::new(&dummy).spawn().unwrap();
    let spawn = updater::spawn_helper(viejo.id(), &installer, &exe, &work);
    let _ = viejo.wait();
    let limite = Instant::now() + Duration::from_secs(20);
    while Instant::now() < limite && !work.join("result.txt").is_file() {
        std::thread::sleep(Duration::from_millis(250));
    }
    std::thread::sleep(Duration::from_secs(14)); // el bucle de reapertura: 8 intentos × 1,5 s
    let leer = |p: PathBuf| std::fs::read_to_string(p).unwrap_or_default();
    let stem = installer.file_stem().unwrap().to_string_lossy().to_lowercase();
    let a = Ayudante {
        spawn,
        instalador: !leer(work.join(format!("{stem}.marcas.txt"))).is_empty(),
        resultado: leer(work.join("result.txt")).trim().to_string(),
        reaperturas: leer(app.join("pronto.marcas.txt")).lines().count(),
        log: leer(work.join("update.log")),
    };
    matar_juguetes(&app);
    a
}

/// Corchetes en la carpeta de instalación (el instalador muestra la página de carpeta en la primera
/// instalación, `pronto2.iss:79` `DisableDirPage=auto`): tras actualizar, ¿se vuelve a abrir Pronto?
#[test]
fn rompedor_descargas_ayudante_corchetes_en_la_carpeta() {
    let t = Tmp::new("ayudante-corchetes");
    let toy = juguete(&t.0);
    let mut malos = Vec::new();
    for (n, app, work, inst) in [
        ("control", "Programas/Pronto", "work", "Setup.exe"),
        ("exe en «Programas [Alfonso]»", "Programas [Alfonso]/Pronto", "work", "Setup.exe"),
        ("exe en «Apps]»", "Apps]/Pronto", "work", "Setup.exe"),
        ("work en «tmp [1]»", "Programas/Pronto", "tmp [1]", "Setup.exe"),
        ("instalador «Pronto-Setup[x64].exe»", "Programas/Pronto", "work", "Pronto-Setup[x64].exe"),
    ] {
        let base = t.0.join(format!("v{}", malos.len() + n.len()));
        let a = probar_ayudante(&toy, &base, app, work, inst);
        let ultimo = a.log.lines().filter(|l| l.contains("abrir") || l.contains("existe") || l.contains("instalador")).last().unwrap_or("(sin log)").to_string();
        eprintln!(
            "ROMPEDOR corchetes[{n}]: spawn={:?} instalador_ejecutado={} result={:?} reaperturas={} log={} · {}",
            a.spawn.as_ref().err(),
            a.instalador,
            a.resultado,
            a.reaperturas,
            !a.log.is_empty(),
            ultimo.chars().take(200).collect::<String>()
        );
        if a.spawn.is_err() || !a.instalador || a.resultado != "ok" || a.reaperturas != 1 {
            malos.push(n);
        }
    }
    assert!(malos.is_empty(), "{malos:?}");
}

// ============================================================ segunda pasada (2026-10-23)
//
// Contra los arreglos de 022e85c/ca4f00b: `get_following`/`next_hop`, `url_allowed` con el parser
// WHATWG, `copy_limited`, `native_path`, `is_loopback_host`, `.redirects(0)` de Ollama. Todo en
// 127.0.0.0/8 (los «otros hosts» son 127.0.0.2, que también es este PC: nada sale fuera).

use crate::models::{Settings, Tone};
use std::sync::atomic::{AtomicBool, AtomicU64};

/// Agente como los de la app tras el arreglo: sin redirecciones automáticas.
fn agente0() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(5)).timeout_read(Duration::from_secs(30)).redirects(0).build()
}

/// Servidor con un hilo por conexión (para goteos y POST): lee petición y cuerpo y llama a `h`.
fn servidor_hilos(h: impl Fn(std::net::TcpStream, String) + Send + Sync + 'static) -> (String, u16, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let hits = Arc::new(AtomicUsize::new(0));
    let hc = hits.clone();
    let h = Arc::new(h);
    std::thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            hc.fetch_add(1, Ordering::SeqCst);
            let h = h.clone();
            std::thread::spawn(move || {
                let mut reader = BufReader::new(conn.try_clone().unwrap());
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() {
                    return;
                }
                let mut len = 0usize;
                loop {
                    let mut hd = String::new();
                    if reader.read_line(&mut hd).is_err() || hd.trim().is_empty() {
                        break;
                    }
                    if let Some(v) = hd.to_ascii_lowercase().strip_prefix("content-length:") {
                        len = v.trim().parse().unwrap_or(0);
                    }
                }
                let mut body = vec![0u8; len.min(1 << 20)];
                let _ = reader.read_exact(&mut body);
                let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
                h(conn, path);
            });
        }
    });
    (format!("http://127.0.0.1:{port}"), port, hits)
}

fn r3xx(code: u16, extra: &str) -> Vec<u8> {
    format!("HTTP/1.1 {code} X\r\n{extra}Content-Length: 0\r\nConnection: close\r\n\r\n").into_bytes()
}

fn json_ok(body: &str) -> Vec<u8> {
    format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).into_bytes()
}

/// `get_following` + `next_hop` con `Location` hostiles, con las tres reglas de salto de la app:
/// actualizador (`url_allowed(_, true)`, `updater.rs:622`), modelos (`modeldl.rs:473`) e instalador de
/// Ollama (`aisetup.rs:186`, las dos últimas privadas: se copian aquí tal cual). Nada debe entrar en
/// pánico, y el actualizador nunca debe seguir a 127.0.0.2 ni a [::1] (no son 127.0.0.1/localhost).
#[test]
fn rompedor_descargas_saltos_de_redireccion() {
    let (_, b, hits_b) = servidor(|p| ok(format!("B{p}").as_bytes()));
    let loc = |l: &str| r3xx(302, &format!("Location: {l}\r\n"));
    let casos: Vec<(&str, Vec<u8>)> = vec![
        ("relativa /x", loc("/x")),
        ("relativa de ruta x", loc("x")),
        ("solo ?query", loc("?q=1")),
        ("//127.0.0.1:B", loc(&format!("//127.0.0.1:{b}/x"))),
        ("127.1", loc(&format!("http://127.1:{b}/x"))),
        ("0x7f.1", loc(&format!("http://0x7f.1:{b}/x"))),
        ("HTTP://LOCALHOST", loc(&format!("HTTP://LOCALHOST:{b}/x"))),
        ("barras invertidas", loc(&format!("http:\\\\127.0.0.1:{b}\\x"))),
        ("%2e%2e", loc(&format!("http://127.0.0.1:{b}/a/%2e%2e/%2E%2E/x"))),
        ("IDN ⓛocalhost", loc(&format!("http://ⓛocalhost:{b}/x"))),
        ("IDN ①②⑦.0.0.1", loc(&format!("http://①②⑦.0.0.1:{b}/x"))),
        ("espacios alrededor", loc(&format!("   http://127.0.0.1:{b}/x   "))),
        ("tab en el host", loc(&format!("http://127.0.0\t.1:{b}/x"))),
        ("127.0.0.2 (otro host)", loc(&format!("http://127.0.0.2:{b}/x"))),
        ("[::1]", loc(&format!("http://[::1]:{b}/x"))),
        ("[::ffff:127.0.0.1]", loc(&format!("http://[::ffff:127.0.0.1]:{b}/x"))),
        ("usuario@otro host", loc(&format!("http://127.0.0.1:{b}@127.0.0.2:{b}/x"))),
        ("usuario:clave@", loc(&format!("http://u:p@127.0.0.1:{b}/x"))),
        ("#@", loc(&format!("http://127.0.0.2:{b}#@127.0.0.1:{b}/"))),
        ("puerto 99999", loc("http://127.0.0.1:99999/x")),
        ("puerto 0", loc("http://127.0.0.1:0/x")),
        ("https a 127.0.0.1", loc(&format!("https://127.0.0.1:{b}/x"))),
        ("file:", loc("file:///C:/Windows/win.ini")),
        ("data:", loc("data:,MZ")),
        ("javascript:", loc("javascript:alert(1)")),
        ("about:blank", loc("about:blank")),
        ("blob:", loc(&format!("blob:http://127.0.0.1:{b}/x"))),
        ("ws:", loc(&format!("ws://127.0.0.1:{b}/x"))),
        ("ftp:", loc(&format!("ftp://127.0.0.1:{b}/x"))),
        ("Location de 60 KB", loc(&format!("/{}", "a".repeat(60_000)))),
        ("Location duplicada, buena primero", r3xx(302, &format!("Location: http://127.0.0.1:{b}/x\r\nLocation: http://127.0.0.2:{b}/x\r\n"))),
        ("Location duplicada, mala primero", r3xx(302, &format!("Location: http://127.0.0.2:{b}/x\r\nLocation: http://127.0.0.1:{b}/x\r\n"))),
        ("307 con cuerpo", format!("HTTP/1.1 307 X\r\nLocation: http://127.0.0.1:{b}/x\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhola!").into_bytes()),
        ("308", r3xx(308, &format!("Location: http://127.0.0.1:{b}/x\r\n"))),
        ("303", r3xx(303, &format!("Location: http://127.0.0.1:{b}/x\r\n"))),
        ("300", r3xx(300, &format!("Location: http://127.0.0.1:{b}/x\r\n"))),
        ("305", r3xx(305, &format!("Location: http://127.0.0.1:{b}/x\r\n"))),
        ("399", r3xx(399, &format!("Location: http://127.0.0.1:{b}/x\r\n"))),
        ("302 sin Location", r3xx(302, "")),
        ("302 Location en blanco", r3xx(302, "Location:    \r\n")),
        ("302 Location no UTF-8", b"HTTP/1.1 302 X\r\nLocation: /\xff\xfe\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()),
    ];
    let model_hop = |u: &tauri::Url| u.scheme() == "https" || updater::is_loopback_host(u.host_str().unwrap_or(""));
    let setup_hop = |u: &tauri::Url| {
        let host = u.host_str().unwrap_or("");
        (u.scheme() == "https" && (host == "ollama.com" || updater::is_github_host(host))) || updater::is_loopback_host(host)
    };
    let mut malos = Vec::new();
    for (nombre, resp) in casos {
        let (a, _, _) = servidor(move |p| if p == "/d/f" { resp.clone() } else { ok(format!("A{p}").as_bytes()) });
        let mut fila = Vec::new();
        for regla in ["actualizador", "modelos", "ollama-setup"] {
            let antes = hits_b.load(Ordering::SeqCst);
            let url = format!("{a}/d/f");
            let r = std::thread::spawn(move || {
                let hop: Box<dyn Fn(&tauri::Url) -> bool> = match regla {
                    "actualizador" => Box::new(|u: &tauri::Url| updater::url_allowed(u.as_str(), true)),
                    "modelos" => Box::new(model_hop),
                    _ => Box::new(setup_hop),
                };
                match updater::get_following(&agente0(), &url, &|r| r, &*hop) {
                    Ok(resp) => {
                        let st = resp.status();
                        let mut s = String::new();
                        let _ = resp.into_reader().take(80).read_to_string(&mut s);
                        format!("Ok {st} «{}»", s.chars().take(30).collect::<String>())
                    }
                    Err(e) => format!("{e:?}").chars().take(110).collect(),
                }
            })
            .join();
            let b_hits = hits_b.load(Ordering::SeqCst) - antes;
            let txt = match &r {
                Ok(s) => s.clone(),
                Err(_) => {
                    malos.push(format!("{nombre} · {regla}: PÁNICO"));
                    "PÁNICO".into()
                }
            };
            let otro = nombre.contains("127.0.0.2") || nombre.contains("otro host") || nombre.contains("mala primero") || nombre == "[::1]" || nombre == "#@";
            if regla == "actualizador" && otro && !txt.starts_with("Redirect") {
                malos.push(format!("{nombre} · actualizador siguió: {txt}"));
            }
            fila.push(format!("{regla}: {txt} (B+{b_hits})"));
        }
        eprintln!("ROMPEDOR salto[{nombre}] {}", fila.join(" | "));
    }
    assert!(malos.is_empty(), "{malos:#?}");
}

/// PRNT-41 «actualizador sin plazo total» se arregló en el cuerpo (`copy_limited`: ventana de
/// 64 KB/30 s y plazo de 30 min), pero ureq lee las cabeceras antes con solo `timeout_read` (30 s
/// por lectura): un servidor que gotea las cabeceras un byte por segundo no se corta nunca. Los
/// modelos (`modeldl.rs:450`, `deadline: None, window: None`) no tienen ni la ventana.
#[test]
fn rompedor_descargas_goteo_en_cabeceras_sin_plazo() {
    let parar = Arc::new(AtomicBool::new(false));
    let p1 = parar.clone();
    let (cab, _, _) = servidor_hilos(move |mut s, _| {
        let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nX-Goteo: ");
        while !p1.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_secs(1));
            if s.write_all(b"a").is_err() {
                return;
            }
        }
    });
    let p2 = parar.clone();
    let (cuerpo, _, _) = servidor_hilos(move |mut s, _| {
        let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10000000\r\nConnection: close\r\n\r\nMZ");
        while !p2.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_secs(1));
            if s.write_all(b"\0").is_err() {
                return;
            }
        }
    });
    let t = Tmp::new("goteo");
    let (tx, rx) = std::sync::mpsc::channel::<(&'static str, Duration, String)>();
    let t0 = Instant::now();
    let casos: [(&'static str, String, bool); 4] = [
        ("instalador · cabeceras a goteo", cab.clone(), true),
        ("instalador · cuerpo a goteo", cuerpo.clone(), true),
        ("modelo · cabeceras a goteo", cab.clone(), false),
        ("modelo · cuerpo a goteo", cuerpo.clone(), false),
    ];
    for (i, (nombre, base, instalador)) in casos.into_iter().enumerate() {
        let dir = t.join(&format!("c{i}"));
        let tx = tx.clone();
        std::thread::spawn(move || {
            let r = if instalador {
                let asset = Asset { name: "P.exe".into(), url: format!("{base}/s.exe"), size: 10_000_000, sha256: None };
                format!("{:?}", updater::fetch_installer(&agente0(), &asset, &dir, &mut |_, _, _| {}).err())
            } else {
                let f = [crate::modeldl::ModelFile { name: "a.bin", sha256: "0000000000000000000000000000000000000000000000000000000000000000", size: 10_000_000 }];
                format!("{:?}", crate::modeldl::download_all(&dir, &base, &f, &mut |_| {}).err())
            };
            let _ = tx.send((nombre, t0.elapsed(), r));
        });
    }
    let limite = Duration::from_secs(75);
    let mut fin = Vec::new();
    while fin.len() < 4 {
        match rx.recv_timeout(limite.saturating_sub(t0.elapsed())) {
            Ok(x) => fin.push(x),
            Err(_) => break,
        }
    }
    let colgadas: Vec<&str> = ["instalador · cabeceras a goteo", "instalador · cuerpo a goteo", "modelo · cabeceras a goteo", "modelo · cuerpo a goteo"]
        .into_iter()
        .filter(|n| !fin.iter().any(|f| f.0 == *n))
        .collect();
    for (n, d, r) in &fin {
        eprintln!("ROMPEDOR goteo[{n}]: terminó a los {:.1} s -> {}", d.as_secs_f32(), r.chars().take(160).collect::<String>());
    }
    for n in &colgadas {
        eprintln!("ROMPEDOR goteo[{n}]: SIGUE en marcha a los {:.0} s (1 byte/s)", t0.elapsed().as_secs_f32());
    }
    parar.store(true, Ordering::SeqCst);
    let t1 = Instant::now();
    while fin.len() < 4 {
        match rx.recv_timeout(Duration::from_secs(40)) {
            Ok(x) => {
                eprintln!("ROMPEDOR goteo[{}]: tras cortar el servidor, terminó a los {:.1} s -> {}", x.0, x.1.as_secs_f32(), x.2.chars().take(160).collect::<String>());
                fin.push(x);
            }
            Err(_) => break,
        }
    }
    eprintln!("ROMPEDOR goteo: limpieza en {:?}", t1.elapsed());
    assert!(colgadas.is_empty(), "sin plazo con 1 byte/s durante {limite:?}: {colgadas:?}");
}

/// PRNT-41 «descargas sin tope»: el tope del instalador es `size + 64 KB` con el `size` del JSON
/// (`updater.rs:792-796`); el de 512 MB (`INSTALLER_MAX_BYTES`) solo vale si `size` es 0. Un JSON con
/// `size` enorme deja escribir sin tope en `%TEMP%` (el `.part` se borra al final, al fallar el tamaño).
#[test]
fn rompedor_descargas_tope_del_instalador_sale_del_json() {
    let t = Tmp::new("tope-json");
    const N: u64 = updater::INSTALLER_MAX_BYTES + 8 * 1024 * 1024;
    let (url, _, _) = servidor_hilos(move |mut s, _| {
        let _ = s.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {N}\r\nConnection: close\r\n\r\n").as_bytes());
        let mut buf = vec![7u8; 1 << 20];
        buf[0] = b'M';
        buf[1] = b'Z';
        let mut sent = 0u64;
        while sent < N {
            let n = (N - sent).min(buf.len() as u64) as usize;
            if s.write_all(&buf[..n]).is_err() {
                return;
            }
            buf[0] = 7;
            buf[1] = 7;
            sent += n as u64;
        }
    });
    let mut pasan = Vec::new();
    for size in [0u64, 50 * 1024 * 1024 * 1024, u64::MAX] {
        let asset = Asset { name: "P.exe".into(), url: format!("{url}/s.exe"), size, sha256: None };
        let mut max = 0u64;
        let t0 = Instant::now();
        let r = updater::fetch_installer(&agente0(), &asset, &t.0, &mut |_, done, _| max = max.max(done));
        eprintln!(
            "ROMPEDOR tope_json: size={size} · escritos en .part {max} B ({} MB) en {:?} · tope 512 MB = {} B · {:?} · en disco {:?}",
            max >> 20,
            t0.elapsed(),
            updater::INSTALLER_MAX_BYTES,
            r.as_ref().err().map(|e| e.chars().take(120).collect::<String>()),
            ficheros(&t.0)
        );
        if max > updater::INSTALLER_MAX_BYTES {
            pasan.push(size);
        }
    }
    assert!(pasan.is_empty(), "más de {} B escritos con size={pasan:?}", updater::INSTALLER_MAX_BYTES);
}

/// `.redirects(0)` en Ollama (`ollama.rs:18`): un 3xx (también con un cuerpo JSON que parece una
/// respuesta) no puede pulir ni marcar Ollama como «responde», ni llevar a otro servidor.
#[test]
fn rompedor_descargas_ollama_3xx_no_rompe_el_pulido() {
    let (url_b, _, hits_b) = servidor_hilos(|mut s, _| {
        let _ = s.write_all(&json_ok(r#"{"response":"TEXTO INVENTADO POR OTRO SERVIDOR","models":[{"name":"evil:latest"}]}"#));
    });
    let texto = "hola qué tal estás hoy te escribo para quedar mañana por la tarde";
    let mut malos = Vec::new();
    for (code, loc) in [
        (301, format!("{url_b}/api/generate")),
        (302, "file:///C:/Windows/win.ini".to_string()),
        (303, "data:,x".to_string()),
        (307, format!("{url_b}/api/generate")),
        (308, format!("{url_b}/api/tags")),
        (302, String::new()),
        (304, String::new()),
    ] {
        let l = loc.clone();
        let (url_a, _, hits_a) = servidor_hilos(move |mut s, _| {
            let body = format!(r#"{{"response":"{texto} INYECTADO","models":[{{"name":"qwen3:1.7b"}}]}}"#);
            let h = if l.is_empty() { String::new() } else { format!("Location: {l}\r\n") };
            let _ = s.write_all(format!("HTTP/1.1 {code} X\r\n{h}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes());
        });
        let antes_b = hits_b.load(Ordering::SeqCst);
        let ua = url_a.clone();
        let t0 = Instant::now();
        let r = std::thread::spawn(move || {
            let s = Settings { ai_polish: true, ollama_url: ua.clone(), ai_polish_timeout_ms: 3000, ..Settings::default() };
            let pol = crate::polish::apply(&s, texto, Tone::Casual, 0);
            let st = crate::ollama::status(&ua);
            let genera = crate::ollama::generate_timeout(&ua, "m", "Corrige", texto, Duration::from_secs(3));
            let ai = crate::aisetup::status(&s);
            (pol, st.reachable, genera, ai.ollama_running, ai.model_ready)
        })
        .join();
        let dt = t0.elapsed();
        match r {
            Err(_) => malos.push(format!("{code} {loc}: PÁNICO")),
            Ok((pol, reachable, genera, running, ready)) => {
                eprintln!(
                    "ROMPEDOR ollama_3xx[{code} {loc:.40}]: pulido failed={} igual={} · status.reachable={reachable} · generate={:?} · aisetup running={running} ready={ready} · A={} B+{} · {dt:?}",
                    pol.failed,
                    pol.text == texto,
                    genera.as_ref().map_err(|e| e.chars().take(70).collect::<String>()),
                    hits_a.load(Ordering::SeqCst),
                    hits_b.load(Ordering::SeqCst) - antes_b
                );
                if !pol.failed || pol.text != texto || reachable || genera.is_ok() || running || hits_b.load(Ordering::SeqCst) != antes_b {
                    malos.push(format!("{code} {loc}"));
                }
            }
        }
    }
    assert!(malos.is_empty(), "{malos:#?}");
}

/// URL de Ollama escrita sin esquema (`localhost:11434`, como la variable `OLLAMA_HOST` de Ollama).
/// El hub (`settings.js:42`) y `aisetup::url_host` (`aisetup.rs:131`) le ponen `http://` y la dan por
/// local; `ollama.rs` la usa tal cual y ureq no la entiende (esquema «localhost», sin host).
#[test]
fn rompedor_descargas_url_de_ollama_sin_esquema() {
    let texto = "hola qué tal estás hoy te escribo para quedar mañana por la tarde";
    let (_, port, hits) = servidor_hilos(move |mut s, p| {
        let body = if p.contains("/api/tags") { r#"{"models":[{"name":"qwen3:1.7b"}]}"# } else { r#"{"response":"Hola, ¿qué tal estás hoy? Te escribo para quedar mañana por la tarde."}"# };
        let _ = s.write_all(&json_ok(body));
    });
    let mut malas = Vec::new();
    for u in [format!("http://127.0.0.1:{port}"), format!("127.0.0.1:{port}"), format!("localhost:{port}"), format!("LOCALHOST:{port}/")] {
        let antes = hits.load(Ordering::SeqCst);
        let local = crate::aisetup::is_loopback_url(&u);
        let st = crate::ollama::status(&u);
        let s = Settings { ai_polish: true, ollama_url: u.clone(), ai_polish_timeout_ms: 3000, ..Settings::default() };
        let pol = crate::polish::apply(&s, texto, Tone::Casual, 0);
        let genera = crate::ollama::generate_timeout(&u, "m", "Corrige", texto, Duration::from_secs(3));
        let ai = crate::aisetup::status(&s);
        eprintln!(
            "ROMPEDOR sin_esquema[{u}]: is_loopback_url={local} · status.reachable={} · pulido failed={} · generate={:?} · aisetup.ollama_running={} ollama_installed={} · peticiones al puerto {port}: +{}",
            st.reachable,
            pol.failed,
            genera.as_ref().map(|_| "Ok").map_err(|e| e.chars().take(80).collect::<String>()),
            ai.ollama_running,
            ai.ollama_installed,
            hits.load(Ordering::SeqCst) - antes
        );
        if local && !st.reachable {
            malas.push(u);
        }
    }
    assert!(malas.is_empty(), "local para aisetup pero ollama.rs no llega: {malas:?}");
}

/// Ruta relativa de `desde` a `hasta` (las dos absolutas, mismo disco).
fn relativa(desde: &Path, hasta: &Path) -> PathBuf {
    let a: Vec<_> = desde.components().collect();
    let b: Vec<_> = hasta.components().collect();
    let comun = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let mut r = PathBuf::new();
    for _ in comun..a.len() {
        r.push("..");
    }
    for c in &b[comun..] {
        r.push(c.as_os_str());
    }
    r
}

/// `asr::native_path` (`asr.rs:523`) con las formas de escribir una ruta larga (>= 240) que Rust lee
/// (y por tanto `verify` aprueba): si la ruta que se le da a sherpa no existe, el hub dice «listo» y
/// no se puede dictar (lo que arregló descargas BAJA 6).
#[test]
#[ignore = "rompedor: native_path convierte una ruta larga \\\\.\\C:\\… en \\\\?\\UNC\\.\\C:\\…, que no existe"]
fn rompedor_descargas_native_path_formas() {
    let t = Tmp::new("np");
    let mut largo = t.0.clone();
    while largo.to_string_lossy().encode_utf16().count() < 250 {
        largo = largo.join("carpeta-honda");
    }
    std::fs::create_dir_all(&largo).unwrap();
    std::fs::write(largo.join("tokens.txt"), b"x 0\n").unwrap();
    let f = largo.join("tokens.txt");
    let s = f.to_string_lossy().into_owned();
    let cwd = std::env::current_dir().unwrap();
    let rel = relativa(&cwd, &f).to_string_lossy().into_owned();
    let sin_unidad = s[3..].to_string();
    let formas: Vec<(&str, String)> = vec![
        ("normal", s.clone()),
        ("barras /", s.replace('\\', "/")),
        ("con .\\ y x\\..\\", s.replacen("carpeta-honda\\", "carpeta-honda\\.\\x\\..\\", 1)),
        ("unidad en minúscula", s[..1].to_lowercase() + &s[1..]),
        ("\\\\?\\ ya puesto", format!(r"\\?\{s}")),
        ("\\\\.\\ dispositivo", format!(r"\\.\{s}")),
        ("//?/ con barras", format!("//?/{}", s.replace('\\', "/"))),
        ("//./ con barras", format!("//./{}", s.replace('\\', "/"))),
        ("relativa al cwd", rel.clone()),
        ("relativa con barras /", rel.replace('\\', "/")),
        ("\\\\?\\GLOBALROOT\\??\\", format!(r"\\?\GLOBALROOT\??\{s}")),
        ("UNC \\\\localhost\\C$", format!(r"\\localhost\C$\{sin_unidad}")),
        ("\\\\?\\UNC\\localhost\\C$", format!(r"\\?\UNC\localhost\C$\{sin_unidad}")),
        ("\\\\127.0.0.1\\C$", format!(r"\\127.0.0.1\C$\{sin_unidad}")),
        ("carpeta con punto final", s.replace("carpeta-honda\\tokens.txt", "carpeta-honda.\\tokens.txt")),
        ("carpeta con espacio final", s.replace("carpeta-honda\\tokens.txt", "carpeta-honda \\tokens.txt")),
    ];
    let mut rotas = Vec::new();
    for (n, p) in formas {
        let rust = Path::new(&p).is_file();
        let np = crate::asr::native_path(Path::new(&p));
        let ok_np = Path::new(&np).is_file();
        eprintln!("ROMPEDOR native_path[{n}] ({} u16): Rust lee={rust} · nativa existe={ok_np} · «{}…»", p.encode_utf16().count(), np.chars().take(34).collect::<String>());
        if rust && !ok_np {
            rotas.push(format!("{n}: {}", np.chars().take(60).collect::<String>()));
        }
    }
    assert!(rotas.is_empty(), "{rotas:#?}");
}

/// Lo mismo con el Parakeet real (copiado a `%TEMP%`, en un proceso hijo): «Ruta del modelo» escrita
/// `\\.\C:\…` (forma de dispositivo), corta y larga. `PRONTO_ROMPEDOR_MODELO` = carpeta de Parakeet.
#[test]
#[ignore = "rompedor: native_path convierte una ruta larga \\\\.\\C:\\… en \\\\?\\UNC\\.\\C:\\…, que no existe"]
fn rompedor_descargas_modelo_en_ruta_de_dispositivo() {
    let Ok(real) = std::env::var("PRONTO_ROMPEDOR_MODELO") else {
        println!("ROMPEDOR_OMITIDO: falta PRONTO_ROMPEDOR_MODELO");
        return;
    };
    let t = Tmp::new("ruta-dispositivo");
    let base = t.join("base");
    std::fs::create_dir_all(&base).unwrap();
    for f in &crate::modeldl::FILES {
        std::fs::copy(Path::new(&real).join(f.name), base.join(f.name)).unwrap();
    }
    let mut largo = t.0.clone();
    while largo.to_string_lossy().encode_utf16().count() < 245 {
        largo = largo.join("carpeta-muy-larga-de-modelos");
    }
    std::fs::create_dir_all(&largo).unwrap();
    for f in &crate::modeldl::FILES {
        std::fs::hard_link(base.join(f.name), largo.join(f.name)).unwrap();
    }
    let mut rotos = Vec::new();
    for (n, dir) in [
        ("larga normal (control)", largo.to_string_lossy().into_owned()),
        ("\\\\.\\ corta", format!(r"\\.\{}", base.display())),
        ("\\\\.\\ larga", format!(r"\\.\{}", largo.display())),
    ] {
        let (code, lineas) = lanzar_hijo("rompedor_descargas::rompedor_descargas_hijo_carga_parakeet", ("PRONTO_ROMPEDOR_HIJO_DIR", &dir));
        eprintln!("ROMPEDOR dispositivo[{n}] ({} car.): salida={code:?} · {lineas}", dir.chars().count());
        if code != Some(0) {
            rotos.push(n);
        }
    }
    assert!(rotos.is_empty(), "{rotos:?}");
}

/// Lector de pruebas: trozos con espera previa.
struct Lector(std::collections::VecDeque<(Duration, std::io::Result<Vec<u8>>)>);

impl Read for Lector {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let Some((espera, r)) = self.0.pop_front() else { return Ok(0) };
        std::thread::sleep(espera);
        let mut v = r?;
        let n = v.len().min(buf.len());
        buf[..n].copy_from_slice(&v[..n]);
        if v.len() > n {
            let resto = v.split_off(n);
            self.0.push_front((Duration::ZERO, Ok(resto)));
        }
        Ok(n)
    }
}

struct EscritorRoto;
impl Write for EscritorRoto {
    fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("disco lleno"))
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// `copy_limited` (`updater.rs:374`) en los bordes: justo el tope, uno de más, tope 0 y u64::MAX,
/// lectura interrumpida, ventana con goteo y con ráfagas al borde, plazo y escritura fallida.
#[test]
fn rompedor_descargas_copy_limited_bordes() {
    use updater::{copy_limited, CopyError, Limits};
    let ms = Duration::from_millis;
    let l = |max: u64, deadline: Option<Duration>, window: Option<(Duration, u64)>| Limits { max_bytes: max, deadline, window };
    let trozos = |v: Vec<(u64, usize)>| Lector(v.into_iter().map(|(e, n)| (ms(e), Ok(vec![b'x'; n]))).collect());
    let mut malos = Vec::new();
    let mut probar = |nombre: &str, mut r: Lector, lim: Limits, esperado: &str| {
        let mut out = Vec::new();
        let t0 = Instant::now();
        let res = copy_limited(&mut r, &mut out, lim, &mut |_| {});
        let txt = match &res {
            Ok(n) => format!("Ok({n})"),
            Err(CopyError::TooBig(m)) => format!("TooBig({m})"),
            Err(CopyError::Deadline(d)) => format!("Deadline({d:?})"),
            Err(CopyError::Stalled(w, m)) => format!("Stalled({w:?},{m})"),
            Err(CopyError::Read(e)) => format!("Read({:?})", e.kind()),
            Err(CopyError::Write(e)) => format!("Write({e})"),
        };
        let bien = txt.starts_with(esperado) && out.len() as u64 <= lim.max_bytes;
        eprintln!("ROMPEDOR copy_limited[{nombre}]: {txt} · escritos {} · {:?} · esperado {esperado} -> {}", out.len(), t0.elapsed(), if bien { "ok" } else { "MAL" });
        if !bien {
            malos.push(format!("{nombre}: {txt}, escritos {}", out.len()));
        }
    };
    probar("justo el tope", trozos(vec![(0, 1000)]), l(1000, None, None), "Ok(1000)");
    probar("justo el tope en 2 trozos", trozos(vec![(0, 999), (0, 1)]), l(1000, None, None), "Ok(1000)");
    probar("uno de más", trozos(vec![(0, 1001)]), l(1000, None, None), "TooBig");
    probar("uno de más tras el tope", trozos(vec![(0, 1000), (0, 1)]), l(1000, None, None), "TooBig");
    probar("tope 0 y nada", trozos(vec![]), l(0, None, None), "Ok(0)");
    probar("tope 0 y 1 byte", trozos(vec![(0, 1)]), l(0, None, None), "TooBig");
    probar("tope u64::MAX", trozos(vec![(0, 300_000), (0, 10)]), l(u64::MAX, None, None), "Ok(300010)");
    probar("tope mayor que el búfer", trozos(vec![(0, 300_000)]), l(262_145, None, None), "TooBig");
    // Comportamiento actual, solo informativo: `std::io::copy` reintentaría un `Interrupted`;
    // `copy_limited` aborta. Sin disparador real conocido en sockets de Windows (no cuenta).
    probar(
        "lectura Interrupted (std::io::copy la reintenta; aquí aborta)",
        Lector(vec![(ms(0), Ok(vec![1u8; 10])), (ms(0), Err(std::io::ErrorKind::Interrupted.into())), (ms(0), Ok(vec![1u8; 10]))].into()),
        l(100, None, None),
        "Read(Interrupted)",
    );
    probar("goteo 1 B/50 ms, ventana 300 ms/100 B", trozos((0..40).map(|_| (50, 1)).collect()), l(10_000, None, Some((ms(300), 100))), "Stalled");
    probar("ráfaga de 100 B justo antes de cada ventana", trozos((0..8).map(|_| (290, 100)).collect()), l(10_000, None, Some((ms(300), 100))), "Ok(800)");
    probar("ráfaga y luego goteo", trozos([(0u64, 5000usize)].into_iter().chain((0..20).map(|_| (100, 1))).collect()), l(10_000, None, Some((ms(300), 100))), "Stalled");
    probar("plazo 300 ms con datos constantes", trozos((0..20).map(|_| (50, 10)).collect()), l(10_000, Some(ms(300)), None), "Deadline");
    let mut out = EscritorRoto;
    let r = copy_limited(&mut trozos(vec![(0, 10)]), &mut out, l(100, None, None), &mut |_| {});
    eprintln!("ROMPEDOR copy_limited[escritura fallida]: {:?}", r.as_ref().map_err(|e| format!("{e:?}")));
    if !matches!(r, Err(CopyError::Write(_))) {
        malos.push("escritura fallida".to_string());
    }
    assert!(malos.is_empty(), "{malos:#?}");
}

/// `url_allowed` (`updater.rs:238`) contra el parser WHATWG de ureq: generador de URLs con trucos
/// (`@`, `\`, tab, `%2e`, puntos unicode, IDN, puertos, `#`, `?`). Propiedad: si la acepta, el
/// parser ve https, sin usuario y un host de GitHub (o, en pruebas, http a 127.0.0.1/localhost).
#[test]
fn rompedor_descargas_url_allowed_coincide_con_el_parser() {
    let hosts = [
        "github.com", "api.github.com", "objects.githubusercontent.com", "evil.com", "127.0.0.1", "localhost", "github.com.evil.com", "evilgithub.com", "githubusercontent.com", "GITHUB.COM",
        "xn--github-.com", "github.com.",
    ];
    let trucos = [
        "", "@", ":@", "\\", "\t", "\n", "\r", "%2e", ".", "\u{3002}", "\u{ff0e}", "#", "?", ":443", ":0", ":65536", ":-1", "%00", "\u{200b}", "\u{ad}", "@evil.com", ".evil.com", "\\@evil.com",
        "%40evil.com", "#@evil.com", "?@evil.com", " ", "%20", ":@evil.com", "\\\\evil.com", "/..", "\u{0}",
    ];
    let esquemas = ["https://", "http://", "HTTPS://", "https:/", "https:\\\\", " https://", "https:///", "\thttps://", "https:", "hTTps://", "https://user@"];
    let mut total = 0usize;
    let mut aceptadas = 0usize;
    let mut malas = Vec::new();
    let mut casos = Vec::new();
    for e in esquemas {
        for h in hosts {
            for t1 in trucos {
                casos.push(format!("{e}{t1}{h}/x"));
                casos.push(format!("{e}{h}{t1}/x"));
                casos.push(format!("{e}{h}{t1}github.com/x"));
                casos.push(format!("{e}evil.com{t1}{h}/x"));
            }
        }
    }
    for u in &casos {
        for testing in [false, true] {
            total += 1;
            if !updater::url_allowed(u, testing) {
                continue;
            }
            aceptadas += 1;
            let p = tauri::Url::parse(u);
            let bien = p.as_ref().is_ok_and(|p| {
                let host = p.host_str().unwrap_or("");
                p.username().is_empty()
                    && p.password().is_none()
                    && ((p.scheme() == "https" && updater::is_github_host(host)) || (testing && p.scheme() == "http" && (host == "127.0.0.1" || host == "localhost")))
            });
            if !bien {
                malas.push(format!("{u:?} (testing={testing}) -> {:?}", p.map(|p| p.to_string())));
            }
        }
    }
    eprintln!("ROMPEDOR url_allowed: {total} comprobaciones, {aceptadas} aceptadas, {} aceptadas que el parser lee de otra forma: {:#?}", malas.len(), malas.iter().take(20).collect::<Vec<_>>());
    assert!(malas.is_empty());
}
