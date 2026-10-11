//! Descarga del modelo de voz (Parakeet TDT 0.6B v3 int8) en el primer arranque.
//!
//! Cuatro ficheros desde Hugging Face a `<fichero>.part`; se verifica el SHA-256 y solo entonces se
//! renombra. Un único hilo en segundo plano y una sola descarga a la vez.

use crate::models::{ModelFileStatus, ModelProgress, ModelStatus};
use crate::state::AppState;
use parking_lot::Mutex;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

pub const BASE_URL: &str = "https://huggingface.co/csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/resolve/main";

pub struct ModelFile {
    pub name: &'static str,
    pub sha256: &'static str,
    pub size: u64,
}

/// Hashes de `parakeetModelSha256` (Pronto 1, `core/config.dart`); tamaños medidos sobre esos ficheros.
pub const FILES: [ModelFile; 4] = [
    ModelFile { name: "encoder.int8.onnx", sha256: "acfc2b4456377e15d04f0243af540b7fe7c992f8d898d751cf134c3a55fd2247", size: 652_184_281 },
    ModelFile { name: "decoder.int8.onnx", sha256: "179e50c43d1a9de79c8a24149a2f9bac6eb5981823f2a2ed88d655b24248db4e", size: 11_845_275 },
    ModelFile { name: "joiner.int8.onnx", sha256: "3164c13fc2821009440d20fcb5fdc78bff28b4db2f8d0f0b329101719c0948b3", size: 6_355_277 },
    ModelFile { name: "tokens.txt", sha256: "d58544679ea4bc6ac563d1f545eb7d474bd6cfa467f0a6e2c1dc1c7d37e3c35d", size: 93_939 },
];

/// Aviso a las ventanas cada vez que avanza la descarga.
const EVENT: &str = "model://progress";
const EMIT_EVERY: Duration = Duration::from_millis(120);

static RUNNING: AtomicBool = AtomicBool::new(false);
static LAST: Mutex<Option<ModelProgress>> = Mutex::new(None);

// ------------------------------------------------------------ rutas

/// Carpetas donde Pronto 1 deja el modelo, por orden: `%APPDATA%\Pronto\Pronto\models\parakeet`,
/// `<carpeta del exe>\models\parakeet` y la del instalador de Pronto 1
/// (`%LOCALAPPDATA%\Programs\Pronto\models\parakeet`).
fn existing_dirs() -> Vec<PathBuf> {
    let model = |base: PathBuf| base.join("models").join("parakeet");
    let mut dirs = vec![model(dirs::data_dir().unwrap_or_default().join("Pronto").join("Pronto"))];
    if let Some(exe) = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)) {
        dirs.push(model(exe));
    }
    if let Some(local) = dirs::data_local_dir() {
        dirs.push(model(local.join("Programs").join("Pronto")));
    }
    dirs
}

/// Carpeta propia de Pronto 2: `%APPDATA%\Pronto2\models\parakeet`.
fn own_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_default().join("Pronto2").join("models").join("parakeet")
}

/// Valor por defecto de `Settings.model_dir`: el primer modelo completo de Pronto 1; si no hay, el propio.
pub fn default_dir() -> String {
    pick_default(&existing_dirs(), &own_dir(), &FILES).to_string_lossy().into_owned()
}

/// Ajuste ya guardado -> carpeta efectiva. Solo se cambia lo que era un valor de fábrica (una ruta
/// de Pronto 1 o la propia) y no tiene el modelo completo; una ruta elegida a mano nunca se toca.
pub fn resolve_dir(saved: &str) -> String {
    resolve(saved, &existing_dirs(), &own_dir(), &FILES)
}

fn pick_default(existing: &[PathBuf], own: &Path, files: &[ModelFile]) -> PathBuf {
    existing.iter().find(|d| is_complete(d, files)).map_or_else(|| own.to_path_buf(), |d| d.clone())
}

fn resolve(saved: &str, existing: &[PathBuf], own: &Path, files: &[ModelFile]) -> String {
    let default = || pick_default(existing, own, files).to_string_lossy().into_owned();
    if saved.trim().is_empty() {
        return default();
    }
    let was_factory = existing.iter().map(PathBuf::as_path).chain([own]).any(|d| saved.eq_ignore_ascii_case(&d.to_string_lossy()));
    if was_factory && !is_complete(Path::new(saved), files) {
        return default();
    }
    saved.to_string()
}

// ------------------------------------------------------------ estado en disco

fn file_len(path: &Path) -> u64 {
    fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

fn file_ok(dir: &Path, f: &ModelFile) -> bool {
    fs::metadata(dir.join(f.name)).is_ok_and(|m| m.is_file() && m.len() == f.size)
}

pub fn is_complete(dir: &Path, files: &[ModelFile]) -> bool {
    files.iter().all(|f| file_ok(dir, f))
}

pub fn model_present(dir: &str) -> bool {
    is_complete(Path::new(dir), &FILES)
}

pub fn percent(done: u64, total: u64) -> u32 {
    if total == 0 { 0 } else { (done.min(total) * 100 / total) as u32 }
}

pub fn status(dir: &str) -> ModelStatus {
    status_for(Path::new(dir), &FILES, RUNNING.load(Ordering::SeqCst), LAST.lock().as_ref())
}

fn status_for(dir: &Path, files: &[ModelFile], running: bool, last: Option<&ModelProgress>) -> ModelStatus {
    let list: Vec<ModelFileStatus> = files
        .iter()
        .map(|f| {
            let ok = file_ok(dir, f);
            let bytes = if ok { f.size } else { file_len(&dir.join(f.name)).max(file_len(&dir.join(format!("{}.part", f.name)))) };
            ModelFileStatus { name: f.name.into(), ok, bytes, size: f.size }
        })
        .collect();
    let complete = list.iter().all(|f| f.ok);
    let done_bytes: u64 = list.iter().map(|f| f.bytes.min(f.size)).sum();
    let total_bytes: u64 = list.iter().map(|f| f.size).sum();
    let (state, error) = match last {
        _ if complete => ("done", None),
        Some(p) if running => (p.state, None),
        None if running => ("downloading", None),
        Some(p) if p.state == "error" => ("error", p.error.clone()),
        _ => ("idle", None),
    };
    ModelStatus {
        dir: dir.to_string_lossy().into_owned(),
        complete,
        downloading: running,
        state,
        done_bytes,
        total_bytes,
        overall_percent: percent(done_bytes, total_bytes),
        error,
        files: list,
    }
}

// ------------------------------------------------------------ descarga

pub fn sha256_hex(mut reader: impl Read) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

pub fn hash_file(path: &Path) -> std::io::Result<String> {
    sha256_hex(File::open(path)?)
}

/// Baja los ficheros que falten a `dir` y emite el progreso. Los que ya están enteros se saltan.
pub fn download_all(dir: &Path, base_url: &str, files: &[ModelFile], emit: &mut dyn FnMut(ModelProgress)) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| format!("No se pudo crear {}: {e}", dir.display()))?;
    let total: u64 = files.iter().map(|f| f.size).sum();
    let agent = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(15)).timeout_read(Duration::from_secs(30)).build();
    let mut before = 0u64;
    for f in files {
        if !file_ok(dir, f) {
            download_one(&agent, base_url, dir, f, before, total, emit)?;
        }
        before += f.size;
    }
    emit(ModelProgress { file: String::new(), done_bytes: total, total_bytes: total, overall_percent: 100, state: "done", error: None });
    Ok(())
}

fn download_one(
    agent: &ureq::Agent,
    base_url: &str,
    dir: &Path,
    f: &ModelFile,
    before: u64,
    total: u64,
    emit: &mut dyn FnMut(ModelProgress),
) -> Result<(), String> {
    let part = dir.join(format!("{}.part", f.name));
    let result = fetch_to_part(agent, base_url, &part, f, before, total, emit).and_then(|()| {
        emit(progress(f, before + f.size, total, "verifying"));
        let got = hash_file(&part).map_err(|e| format!("{}: no se pudo verificar: {e}", f.name))?;
        if got != f.sha256 {
            return Err(format!("{}: la descarga está dañada (el SHA-256 no coincide). Inténtalo de nuevo.", f.name));
        }
        fs::rename(&part, dir.join(f.name)).map_err(|e| format!("{}: no se pudo guardar: {e}", f.name))
    });
    if result.is_err() {
        let _ = fs::remove_file(&part);
    }
    result
}

fn fetch_to_part(
    agent: &ureq::Agent,
    base_url: &str,
    part: &Path,
    f: &ModelFile,
    before: u64,
    total: u64,
    emit: &mut dyn FnMut(ModelProgress),
) -> Result<(), String> {
    let url = format!("{}/{}", base_url.trim_end_matches('/'), f.name);
    let resp = agent.get(&url).call().map_err(|e| match e {
        ureq::Error::Status(code, _) => format!("{}: el servidor respondió {code}", f.name),
        e => format!("{}: no se pudo conectar para descargar el modelo: {e}", f.name),
    })?;
    let mut reader = resp.into_reader();
    let mut out = File::create(part).map_err(|e| format!("{}: no se pudo crear el fichero: {e}", f.name))?;
    let mut buf = vec![0u8; 256 * 1024];
    let (mut got, mut last_emit) = (0u64, Instant::now() - EMIT_EVERY);
    loop {
        let n = reader.read(&mut buf).map_err(|e| format!("{}: se cortó la descarga: {e}", f.name))?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n]).map_err(|e| format!("{}: no se pudo escribir en disco: {e}", f.name))?;
        got += n as u64;
        if last_emit.elapsed() >= EMIT_EVERY {
            last_emit = Instant::now();
            emit(progress(f, before + got, total, "downloading"));
        }
    }
    out.flush().map_err(|e| format!("{}: no se pudo escribir en disco: {e}", f.name))?;
    if got != f.size {
        return Err(format!("{}: descarga incompleta ({got} de {} bytes)", f.name, f.size));
    }
    Ok(())
}

fn progress(f: &ModelFile, done: u64, total: u64, state: &'static str) -> ModelProgress {
    ModelProgress { file: f.name.into(), done_bytes: done, total_bytes: total, overall_percent: percent(done, total), state, error: None }
}

fn publish(app: &AppHandle, p: ModelProgress) {
    *LAST.lock() = Some(p.clone());
    let _ = app.emit(EVENT, p);
}

/// Libera `RUNNING` aunque el hilo termine por un pánico.
struct Running;

impl Drop for Running {
    fn drop(&mut self) {
        RUNNING.store(false, Ordering::SeqCst);
    }
}

/// Arranca la descarga en un hilo aparte (no hace nada si ya hay una en marcha). Al terminar bien,
/// carga el motor para poder dictar sin reiniciar.
pub fn start(app: &AppHandle, st: &Arc<AppState>) -> Result<(), String> {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let guard = Running;
    let dir = st.settings.read().model_dir.clone();
    let (app, st) = (app.clone(), st.clone());
    std::thread::Builder::new()
        .name("pronto-modeldl".into())
        .spawn(move || {
            let _guard = guard;
            let result = download_all(Path::new(&dir), BASE_URL, &FILES, &mut |p| publish(&app, p));
            match result {
                Ok(()) => st.engine.load(&app, dir),
                Err(e) => {
                    eprintln!("[modeldl] {e}");
                    let (done, total) = (status(&dir).done_bytes, FILES.iter().map(|f| f.size).sum());
                    publish(&app, ModelProgress { file: String::new(), done_bytes: done, total_bytes: total, overall_percent: percent(done, total), state: "error", error: Some(e) });
                }
            }
        })
        .map(|_| ())
        .map_err(|e| {
            RUNNING.store(false, Ordering::SeqCst);
            format!("No se pudo lanzar la descarga: {e}")
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;
    use std::sync::atomic::AtomicUsize;

    const TEST_FILES: [ModelFile; 2] = [
        ModelFile { name: "a.bin", sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad", size: 3 },
        ModelFile { name: "b.txt", sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", size: 0 },
    ];

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pronto-modeldl-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn put(dir: &Path, name: &str, bytes: &[u8]) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join(name), bytes).unwrap();
    }

    /// Sirve `routes` (ruta -> cuerpo) por HTTP en 127.0.0.1 y cuenta las peticiones.
    fn serve(routes: Vec<(&'static str, Vec<u8>)>) -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = hits.clone();
        std::thread::spawn(move || {
            for conn in listener.incoming().flatten() {
                counter.fetch_add(1, Ordering::SeqCst);
                let mut reader = BufReader::new(conn.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                loop {
                    let mut h = String::new();
                    reader.read_line(&mut h).unwrap();
                    if h.trim().is_empty() {
                        break;
                    }
                }
                let path = line.split_whitespace().nth(1).unwrap_or("/");
                let mut conn = conn;
                match routes.iter().find(|(p, _)| *p == path) {
                    Some((_, body)) => {
                        let _ = write!(conn, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                        let _ = conn.write_all(body);
                    }
                    None => {
                        let _ = write!(conn, "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                    }
                }
            }
        });
        (url, hits)
    }

    #[test]
    fn sha256_de_vectores_conocidos() {
        assert_eq!(sha256_hex(&b"abc"[..]).unwrap(), TEST_FILES[0].sha256);
        assert_eq!(sha256_hex(&b""[..]).unwrap(), TEST_FILES[1].sha256);
    }

    #[test]
    fn tabla_de_ficheros_es_coherente() {
        let names: Vec<_> = FILES.iter().map(|f| f.name).collect();
        assert_eq!(names, ["encoder.int8.onnx", "decoder.int8.onnx", "joiner.int8.onnx", "tokens.txt"]);
        for f in &FILES {
            assert_eq!(f.sha256.len(), 64, "{}", f.name);
            assert!(f.sha256.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)), "{}", f.name);
            assert!(f.size > 0);
        }
        assert!(BASE_URL.starts_with("https://huggingface.co/") && BASE_URL.ends_with("/resolve/main"));
    }

    #[test]
    fn rutas_de_pronto1_y_propia() {
        let dirs = existing_dirs();
        assert!(dirs[0].ends_with(Path::new("Pronto").join("Pronto").join("models").join("parakeet")));
        assert!(dirs.iter().all(|d| d.ends_with(Path::new("models").join("parakeet"))));
        assert!(dirs.iter().any(|d| d.ends_with(Path::new("Programs").join("Pronto").join("models").join("parakeet"))));
        assert!(own_dir().ends_with(Path::new("Pronto2").join("models").join("parakeet")));
        assert!(!dirs.contains(&own_dir()));
    }

    #[test]
    fn porcentaje_acotado() {
        assert_eq!(percent(0, 0), 0);
        assert_eq!(percent(50, 200), 25);
        assert_eq!(percent(999, 1000), 99);
        assert_eq!(percent(2000, 1000), 100);
    }

    fn put_model(dir: &Path) {
        put(dir, "a.bin", b"abc");
        put(dir, "b.txt", b"");
    }

    #[test]
    fn carpeta_por_defecto_usa_el_primer_modelo_completo_de_pronto1() {
        let root = tmp("default");
        let (appdata, exe, own) = (root.join("appdata"), root.join("exe"), root.join("p2"));
        let existing = [appdata.clone(), exe.clone()];
        assert_eq!(pick_default(&existing, &own, &TEST_FILES), own);
        put(&exe, "a.bin", b"abc");
        assert_eq!(pick_default(&existing, &own, &TEST_FILES), own, "incompleto");
        put_model(&exe);
        assert_eq!(pick_default(&existing, &own, &TEST_FILES), exe);
        put_model(&appdata);
        assert_eq!(pick_default(&existing, &own, &TEST_FILES), appdata, "manda el orden");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ajustes_guardados_se_respetan() {
        let root = tmp("resolve");
        let (appdata, exe, own, custom) = (root.join("appdata"), root.join("exe"), root.join("p2"), root.join("mio"));
        let existing = [appdata.clone(), exe.clone()];
        let r = |saved: &Path| resolve(&saved.to_string_lossy(), &existing, &own, &TEST_FILES);
        let s = |p: &Path| p.to_string_lossy().into_owned();

        assert_eq!(r(&appdata), s(&own), "Pronto 1 sin modelo: pasa a la carpeta propia");
        put_model(&appdata);
        assert_eq!(r(&appdata), s(&appdata), "Pronto 1 con modelo completo: se queda");
        assert_eq!(r(&own), s(&appdata), "la propia vacía cede ante un modelo de Pronto 1");
        assert_eq!(r(&custom), s(&custom), "ruta elegida a mano, aunque esté vacía");
        put_model(&custom);
        put_model(&own);
        assert_eq!(r(&own), s(&own), "la propia completa se queda");
        assert_eq!(resolve("  ", &existing, &own, &TEST_FILES), s(&appdata), "vacío = valor por defecto");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn estado_cuenta_ficheros_y_parciales() {
        let dir = tmp("status");
        let none = status_for(&dir, &TEST_FILES, false, None);
        assert!(!none.complete && none.state == "idle" && none.done_bytes == 0 && none.total_bytes == 3);
        assert_eq!(none.files.iter().map(|f| f.ok).collect::<Vec<_>>(), [false, false]);

        put(&dir, "a.bin.part", b"a");
        let running = status_for(&dir, &TEST_FILES, true, None);
        assert_eq!((running.state, running.done_bytes, running.overall_percent), ("downloading", 1, 33));
        assert_eq!(running.files[0].bytes, 1);

        put(&dir, "a.bin", b"abc");
        put(&dir, "b.txt", b"");
        let done = status_for(&dir, &TEST_FILES, false, None);
        assert!(done.complete && done.state == "done" && done.overall_percent == 100);

        let failed = ModelProgress { file: String::new(), done_bytes: 0, total_bytes: 3, overall_percent: 0, state: "error", error: Some("sin red".into()) };
        let dir2 = tmp("status2");
        let err = status_for(&dir2, &TEST_FILES, false, Some(&failed));
        assert_eq!((err.state, err.error.as_deref()), ("error", Some("sin red")));
        let _ = fs::remove_dir_all(dir);
        let _ = fs::remove_dir_all(dir2);
    }

    #[test]
    fn json_del_evento_y_del_estado() {
        let p = ModelProgress { file: "x".into(), done_bytes: 1, total_bytes: 2, overall_percent: 50, state: "downloading", error: None };
        assert_eq!(
            serde_json::to_value(&p).unwrap(),
            serde_json::json!({ "file": "x", "doneBytes": 1, "totalBytes": 2, "overallPercent": 50, "state": "downloading" })
        );
        let s = serde_json::to_value(status_for(Path::new("no-existe"), &TEST_FILES, false, None)).unwrap();
        assert_eq!(s["complete"], false);
        assert_eq!(s["files"][0]["name"], "a.bin");
    }

    #[test]
    fn descarga_verifica_renombra_y_salta_lo_que_ya_esta() {
        let dir = tmp("dl-ok");
        let (url, hits) = serve(vec![("/a.bin", b"abc".to_vec()), ("/b.txt", Vec::new())]);
        let mut events = Vec::new();
        download_all(&dir, &url, &TEST_FILES, &mut |p| events.push(p)).unwrap();
        assert_eq!(fs::read(dir.join("a.bin")).unwrap(), b"abc");
        assert!(dir.join("b.txt").is_file());
        assert!(!dir.join("a.bin.part").exists());
        assert_eq!(hits.load(Ordering::SeqCst), 2);
        assert!(events.iter().any(|e| e.state == "verifying" && e.file == "a.bin"));
        let last = events.last().unwrap();
        assert_eq!((last.state, last.overall_percent), ("done", 100));
        assert!(events.windows(2).all(|w| w[0].done_bytes <= w[1].done_bytes));

        download_all(&dir, &url, &TEST_FILES, &mut |_| {}).unwrap();
        assert_eq!(hits.load(Ordering::SeqCst), 2, "lo que ya está entero no se vuelve a pedir");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn hash_distinto_borra_el_parcial_y_da_error_claro() {
        let dir = tmp("dl-hash");
        let (url, _) = serve(vec![("/a.bin", b"xyz".to_vec()), ("/b.txt", Vec::new())]);
        let err = download_all(&dir, &url, &TEST_FILES, &mut |_| {}).unwrap_err();
        assert!(err.contains("a.bin") && err.contains("SHA-256"), "{err}");
        assert!(!dir.join("a.bin.part").exists() && !dir.join("a.bin").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn error_http_y_descarga_corta_no_dejan_basura() {
        let dir = tmp("dl-http");
        let (url, _) = serve(vec![]);
        let err = download_all(&dir, &url, &TEST_FILES, &mut |_| {}).unwrap_err();
        assert!(err.contains("404"), "{err}");

        let (url, _) = serve(vec![("/a.bin", b"ab".to_vec())]);
        let err = download_all(&dir, &url, &TEST_FILES, &mut |_| {}).unwrap_err();
        assert!(err.contains("incompleta"), "{err}");
        assert!(!dir.join("a.bin.part").exists());
        let _ = fs::remove_dir_all(dir);
    }
}
