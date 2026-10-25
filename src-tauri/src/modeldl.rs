//! Descarga del modelo de voz (Parakeet TDT 0.6B v3 int8) en el primer arranque.
//!
//! Cuatro ficheros desde Hugging Face a `<fichero>.part`; se verifica el SHA-256 y solo entonces se
//! renombra. Un único hilo en segundo plano y una sola descarga a la vez.

use crate::models::{ModelFileStatus, ModelProgress, ModelStatus};
use crate::state::AppState;
use crate::updater::{copy_limited, CopyError, GetError, Limits};
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

/// Tamaño justo y, si ya se hasheó con este tamaño y fecha, SHA-256 bueno. No hashea: es barato y
/// lo pregunta el hub a menudo. La verificación completa es `verify`.
pub fn is_complete(dir: &Path, files: &[ModelFile]) -> bool {
    files.iter().all(|f| file_ok(dir, f) && !known_bad(dir, f))
}

// ------------------------------------------------------------ verificación (SHA-256 con caché)

/// Marca junto al modelo con los hashes ya calculados: una línea
/// `nombre\ttamaño\tmtime_ns\tv2:<ChangeTime>:<creación>:<volumen>:<id de fichero>\tsha256` por
/// fichero. Así no se hashean 650 MB en cada arranque: solo cuando cambia el fichero. Tamaño y fecha
/// de modificación no bastan (rompedor 22-oct-b, BAJA 7): reescribir el fichero con el mismo tamaño y
/// devolverle la fecha envenenaba la marca. El `ChangeTime` de NTFS cambia con cualquier escritura o
/// cambio de metadatos (también al poner la fecha de modificación a mano) y el id de fichero cambia
/// si se sustituye por otro. Las líneas antiguas (sin `v2:`) no valen: se rehashea una vez.
const MARK: &str = ".verificado";

/// Huella de un fichero tal como está: (tamaño, mtime en ns, `v2:…` con ChangeTime, creación,
/// volumen e id de fichero).
type Stamp = (u64, u128, String);

/// Caché en memoria (por ruta) por si la carpeta no deja escribir la marca.
static HASHES: std::sync::LazyLock<Mutex<std::collections::HashMap<PathBuf, (Stamp, String)>>> = std::sync::LazyLock::new(Default::default);

fn stamp(path: &Path) -> Option<Stamp> {
    let m = fs::metadata(path).ok().filter(|m| m.is_file())?;
    let mtime = m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos();
    Some((m.len(), mtime, file_identity(path)?))
}

/// `v2:<ChangeTime>:<creación>:<volumen>:<id de fichero>` (Windows, `GetFileInformationByHandle[Ex]`).
#[cfg(windows)]
fn file_identity(path: &Path) -> Option<String> {
    use std::os::windows::io::AsRawHandle;
    #[repr(C)]
    #[derive(Default)]
    struct FileBasicInfo {
        creation: i64,
        access: i64,
        write: i64,
        change: i64,
        attributes: u32,
    }
    #[repr(C)]
    #[derive(Default)]
    struct ByHandleFileInformation {
        attributes: u32,
        creation: [u32; 2],
        access: [u32; 2],
        write: [u32; 2],
        volume_serial: u32,
        size_high: u32,
        size_low: u32,
        links: u32,
        index_high: u32,
        index_low: u32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFileInformationByHandleEx(file: *mut std::ffi::c_void, class: i32, info: *mut std::ffi::c_void, size: u32) -> i32;
        fn GetFileInformationByHandle(file: *mut std::ffi::c_void, info: *mut ByHandleFileInformation) -> i32;
    }
    const FILE_BASIC_INFO_CLASS: i32 = 0;
    let f = File::open(path).ok()?;
    let h = f.as_raw_handle();
    let mut basic = FileBasicInfo::default();
    let mut by = ByHandleFileInformation::default();
    // SAFETY: `h` es un handle abierto de `f` (vive hasta el final) y los punteros apuntan a
    // estructuras `repr(C)` del tamaño que se declara.
    let ok = unsafe {
        GetFileInformationByHandleEx(h, FILE_BASIC_INFO_CLASS, (&raw mut basic).cast(), std::mem::size_of::<FileBasicInfo>() as u32) != 0
            && GetFileInformationByHandle(h, &raw mut by) != 0
    };
    let index = ((by.index_high as u64) << 32) | by.index_low as u64;
    ok.then(|| format!("v2:{}:{}:{}:{index}", basic.change, basic.creation, by.volume_serial))
}

#[cfg(not(windows))]
fn file_identity(path: &Path) -> Option<String> {
    use std::os::unix::fs::MetadataExt;
    let m = fs::metadata(path).ok()?;
    Some(format!("v2:{}.{}:0:{}:{}", m.ctime(), m.ctime_nsec(), m.dev(), m.ino()))
}

fn read_mark(dir: &Path) -> Vec<(String, Stamp, String)> {
    let Ok(text) = fs::read_to_string(dir.join(MARK)) else { return Vec::new() };
    text.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            let [name, len, mtime, id, hash] = f[..] else { return None };
            id.starts_with("v2:").then_some(())?;
            Some((name.to_string(), (len.parse().ok()?, mtime.parse().ok()?, id.to_string()), hash.to_string()))
        })
        .collect()
}

/// Hash ya calculado para este fichero tal como está ahora (misma huella, `Stamp`), si lo hay.
fn cached_hash(dir: &Path, name: &str, st: &Stamp) -> Option<String> {
    let path = dir.join(name);
    if let Some((s, h)) = HASHES.lock().get(&path) {
        if s == st {
            return Some(h.clone());
        }
    }
    read_mark(dir).into_iter().find(|(n, s, _)| n == name && s == st).map(|(_, _, h)| h)
}

fn remember(dir: &Path, name: &str, st: Stamp, hash: &str, persist: bool) {
    HASHES.lock().insert(dir.join(name), (st.clone(), hash.to_string()));
    if persist {
        let mut lines: Vec<String> = read_mark(dir)
            .into_iter()
            .filter(|(n, _, _)| n != name)
            .map(|(n, (len, mtime, id), h)| format!("{n}\t{len}\t{mtime}\t{id}\t{h}"))
            .collect();
        lines.push(format!("{name}\t{}\t{}\t{}\t{hash}", st.0, st.1, st.2));
        // Si la carpeta no deja escribir (instalación de Pronto 1), queda la caché en memoria.
        let _ = fs::write(dir.join(MARK), lines.join("\n") + "\n");
    }
}

/// `true` si ya se sabe que el fichero, tal como está ahora, no tiene el SHA-256 esperado.
fn known_bad(dir: &Path, f: &ModelFile) -> bool {
    stamp(&dir.join(f.name)).and_then(|st| cached_hash(dir, f.name, &st)).is_some_and(|h| h != f.sha256)
}

/// `known_bad` para un fichero suelto (modelos de Whisper): no hashea, solo mira la caché.
pub fn file_known_bad(path: &Path, sha256: &str) -> bool {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name().and_then(|n| n.to_str())) else { return false };
    stamp(path).and_then(|st| cached_hash(dir, name, &st)).is_some_and(|h| h != sha256)
}

/// SHA-256 del fichero, de la caché si no ha cambiado (`Stamp`).
fn content_hash(dir: &Path, name: &str, persist: bool) -> std::io::Result<String> {
    let path = dir.join(name);
    let st = stamp(&path).ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no está"))?;
    if let Some(h) = cached_hash(dir, name, &st) {
        return Ok(h);
    }
    let h = hash_file(&path)?;
    // Si cambió mientras se leía, no se guarda: la próxima vez se vuelve a hashear.
    if stamp(&path).as_ref() == Some(&st) {
        remember(dir, name, st, &h, persist);
    }
    Ok(h)
}

/// `tokens.txt` de sherpa-onnx: UTF-8 y una línea `<token> <id>` por token. sherpa-onnx hace
/// `exit(-1)` con uno mal formado (rompedor, ALTA 2): se comprueba antes de dárselo.
pub fn validate_tokens(path: &Path) -> Result<(), String> {
    let bytes = fs::read(path).map_err(|e| format!("No se pudo leer {}: {e}", path.display()))?;
    let text = std::str::from_utf8(&bytes).map_err(|_| format!("{} no es texto UTF-8", path.display()))?;
    let mut n = 0usize;
    for (i, line) in text.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let ok = line.rsplit_once(' ').is_some_and(|(tok, id)| !tok.is_empty() && id.trim_end_matches('\r').parse::<u32>().is_ok());
        if !ok {
            return Err(format!("{} está dañado (línea {}: no es «<token> <id>»)", path.display(), i + 1));
        }
        n += 1;
    }
    if n == 0 {
        return Err(format!("{} está vacío", path.display()));
    }
    Ok(())
}

/// Comprueba que `dir` tiene el modelo entero y sin daños antes de dárselo a sherpa-onnx, que ante un
/// fichero malo no devuelve error sino que tumba el proceso (`exit(-1)` o excepción C++). Tamaño y
/// SHA-256 de cada fichero (con caché, `MARK`) y `tokens.txt` legible.
/// `Err` con el motivo, para el hub.
pub fn verify(dir: &Path, files: &[ModelFile]) -> Result<(), String> {
    // Las pruebas cargan el modelo real de `%APPDATA%` en solo lectura: nunca escriben la marca.
    verify_with(dir, files, !cfg!(test))
}

fn verify_with(dir: &Path, files: &[ModelFile], persist: bool) -> Result<(), String> {
    for f in files {
        verify_one(dir, f.name, f.sha256, f.size, persist)?;
        if f.name == "tokens.txt" {
            validate_tokens(&dir.join(f.name))?;
        }
    }
    Ok(())
}

/// Tamaño y SHA-256 (con la caché de `MARK`) de `dir\name`.
fn verify_one(dir: &Path, name: &str, sha256: &str, size: u64, persist: bool) -> Result<(), String> {
    let at = dir.display();
    let path = dir.join(name);
    let len = fs::metadata(&path).ok().filter(|m| m.is_file()).map(|m| m.len()).ok_or_else(|| format!("Falta {name} en {at}"))?;
    if len != size {
        return Err(format!("{name} en {at} está incompleto o no es el de Pronto ({len} de {size} bytes)"));
    }
    let got = content_hash(dir, name, persist).map_err(|e| format!("No se pudo leer {name} en {at}: {e}"))?;
    if got != sha256 {
        return Err(format!("{name} en {at} está dañado (el SHA-256 no coincide)"));
    }
    Ok(())
}

/// `verify` para un fichero suelto (el `.bin` de un modelo de Whisper): whisper.cpp también aborta el
/// proceso con algunos ficheros dañados (`GGML_ASSERT`, rompedor 22-oct-b, MEDIA 5). Misma caché.
pub fn verify_file(path: &Path, sha256: &str, size: u64) -> Result<(), String> {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name().and_then(|n| n.to_str())) else {
        return Err(format!("Ruta de modelo no válida: {}", path.display()));
    };
    verify_one(dir, name, sha256, size, !cfg!(test))
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
            let ok = file_ok(dir, f) && !known_bad(dir, f);
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
    // Sin redirecciones automáticas: las sigue `updater::get_following` (Hugging Face redirige a su CDN).
    let agent = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(15)).timeout_read(Duration::from_secs(30)).redirects(0).build();
    let mut before = 0u64;
    for f in files {
        // Del tamaño justo no basta: uno dañado con el mismo tamaño se vuelve a bajar (rompedor, BAJA 10).
        let good = file_ok(dir, f) && content_hash(dir, f.name, !cfg!(test)).is_ok_and(|h| h == f.sha256);
        if !good {
            download_one(&agent, base_url, dir, f, before, total, emit)?;
        }
        before += f.size;
    }
    // `file` = el último fichero: con un solo fichero (Whisper) el hub sabe qué modelo ya está (fallo del 18-oct:
    // con `file` vacío no lo encontraba y seguía pintando «Descargar»).
    let last = files.last().map(|f| f.name.to_string()).unwrap_or_default();
    emit(ModelProgress { file: last, done_bytes: total, total_bytes: total, overall_percent: 100, state: "done", error: None });
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
        let dest = dir.join(f.name);
        fs::rename(&part, &dest).map_err(|e| format!("{}: no se pudo guardar: {e}", f.name))?;
        // Recién verificado: el primer arranque no tiene que volver a hashearlo.
        if let Some(st) = stamp(&dest) {
            remember(dir, f.name, st, &got, !cfg!(test));
        }
        Ok(())
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
    // Plazo para las cabeceras y plazo total de la petición: sin ellos, un servidor que las manda a
    // goteo dejaba `RUNNING` en `true` para siempre (rompedor 23-oct, descargas MEDIA 1).
    let resp = crate::updater::get_following_timed(agent, &url, crate::updater::HEADERS_DEADLINE, MODEL_DEADLINE + Duration::from_secs(10), model_hop).map_err(|e| match e {
        GetError::Status(code) => format!("{}: el servidor respondió {code}", f.name),
        GetError::Transport(e) => format!("{}: no se pudo conectar para descargar el modelo: {e}", f.name),
        GetError::Redirect(e) => format!("{}: no se descarga el modelo: {e}", f.name),
    })?;
    if let Some(n) = resp.header("Content-Length").and_then(|v| v.trim().parse::<u64>().ok()).filter(|n| *n > f.size) {
        return Err(format!("{}: el servidor anuncia {n} bytes y el modelo mide {}; no se descarga", f.name, f.size));
    }
    let mut reader = resp.into_reader();
    let mut out = File::create(part).map_err(|e| format!("{}: no se pudo crear el fichero: {e}", f.name))?;
    let mut last_emit = Instant::now() - EMIT_EVERY;
    // Tope: el tamaño conocido del fichero. Se corta en cuanto llega un byte de más (PRNT-41).
    // Más un plazo total y una ventana de avance mínimo, como el instalador (rompedor 23-oct, descargas MEDIA 1).
    let limits = Limits { max_bytes: f.size, deadline: Some(MODEL_DEADLINE), window: Some((MODEL_WINDOW, MODEL_MIN_BYTES)) };
    let got = copy_limited(&mut reader, &mut out, limits, &mut |got| {
        if last_emit.elapsed() >= EMIT_EVERY {
            last_emit = Instant::now();
            emit(progress(f, before + got, total, "downloading"));
        }
    })
    .map_err(|e| match e {
        CopyError::TooBig(max) => format!("{}: el servidor manda más de lo que mide el modelo ({max} bytes); se ha cortado la descarga", f.name),
        CopyError::Read(e) => format!("{}: se cortó la descarga: {e}", f.name),
        CopyError::Write(e) => format!("{}: no se pudo escribir en disco: {e}", f.name),
        CopyError::Deadline(d) => format!("{}: la descarga tarda más de {} minutos; se ha cancelado. Inténtalo más tarde.", f.name, d.as_secs() / 60),
        CopyError::Stalled(w, min) => format!("{}: la descarga va demasiado lenta (menos de {} KB en {} s); se ha cancelado. Inténtalo más tarde.", f.name, min / 1024, w.as_secs()),
    })?;
    out.flush().map_err(|e| format!("{}: no se pudo escribir en disco: {e}", f.name))?;
    if got != f.size {
        return Err(format!("{}: descarga incompleta ({got} de {} bytes)", f.name, f.size));
    }
    Ok(())
}

/// Plazo total de cada fichero de modelo (cabeceras + cuerpo): holgado para conexiones lentas
/// (≈ 0,1 MB/s para el fichero más grande), pero no infinito.
const MODEL_DEADLINE: Duration = Duration::from_secs(2 * 60 * 60);
/// Avance mínimo: al menos `MODEL_MIN_BYTES` cada `MODEL_WINDOW`.
const MODEL_WINDOW: Duration = Duration::from_secs(30);
const MODEL_MIN_BYTES: u64 = 64 * 1024;

/// Cada salto de una redirección de los modelos: https (o este mismo PC, en las pruebas). El host no
/// se limita: Hugging Face sirve los ficheros desde su CDN, que tiene otro dominio (fuera de la lista de
/// PRD §5.2, pendiente de visto bueno), y cada fichero se comprueba con su SHA-256.
fn model_hop(u: &tauri::Url) -> bool {
    u.scheme() == "https" || crate::updater::is_loopback_host(u.host_str().unwrap_or(""))
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
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| download_all(Path::new(&dir), BASE_URL, &FILES, &mut |p| publish(&app, p))))
                .unwrap_or_else(|_| Err("Fallo interno al descargar el modelo. Inténtalo de nuevo.".into()));
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

// ------------------------------------------------------------ Whisper (Contrato v13)

const WHISPER_BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";
const WHISPER_EVENT: &str = "whisper://progress";
/// Id del modelo de Whisper que se está bajando (uno a la vez).
static WHISPER_DL: Mutex<Option<String>> = Mutex::new(None);

pub fn whisper_downloading() -> Option<String> {
    WHISPER_DL.lock().clone()
}

/// Baja un modelo de Whisper (`whisper::MODELS`, SHA-256 comprobado) en un hilo aparte y avisa con
/// `whisper://progress` (`file` = su fichero). Si es el modelo elegido, lo carga al terminar.
pub fn whisper_start(app: &AppHandle, st: &Arc<AppState>, id: &str) -> Result<(), String> {
    let m = crate::whisper::MODELS.iter().find(|m| m.id == id).ok_or_else(|| format!("Modelo de Whisper desconocido: {id}"))?;
    {
        let mut dl = WHISPER_DL.lock();
        if dl.is_some() {
            return Err("Ya se está descargando otro modelo; espera a que termine".into());
        }
        *dl = Some(m.id.to_string());
    }
    // Se libera aunque el hilo no arranque o termine por un pánico: si no, «Ya se está descargando
    // otro modelo» hasta reiniciar Pronto (rompedor 22-oct-c, descargas MEDIA 1).
    let guard = WhisperDl;
    let files = [ModelFile { name: m.file, sha256: m.sha256, size: m.bytes }];
    let id = m.id;
    let (app, st) = (app.clone(), st.clone());
    std::thread::Builder::new()
        .name("pronto-whisperdl".into())
        .spawn(move || {
            let guard = guard;
            let mut emit = |p: ModelProgress| {
                let _ = app.emit(WHISPER_EVENT, p);
            };
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| download_all(&crate::whisper::model_dir(), WHISPER_BASE_URL, &files, &mut emit)))
                .unwrap_or_else(|_| Err("Fallo interno al descargar el modelo. Inténtalo de nuevo.".into()));
            drop(guard);
            match result {
                Ok(()) => {
                    let choice = st.engine.choice();
                    if choice.kind == "whisper" && crate::whisper::model(&choice.model).id == id {
                        let dir = st.settings.read().model_dir.clone();
                        st.engine.load(&app, dir);
                    }
                }
                Err(e) => {
                    eprintln!("[whisperdl] {e}");
                    let total = files[0].size;
                    let _ = app.emit(WHISPER_EVENT, ModelProgress { file: files[0].name.into(), done_bytes: 0, total_bytes: total, overall_percent: 0, state: "error", error: Some(e) });
                }
            }
        })
        .map(|_| ())
        .map_err(|e| format!("No se pudo lanzar la descarga: {e}"))
}

/// Libera `WHISPER_DL` aunque el hilo termine por un pánico.
struct WhisperDl;

impl Drop for WhisperDl {
    fn drop(&mut self) {
        *WHISPER_DL.lock() = None;
    }
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
        assert_eq!(last.file, "b.txt", "el `done` nombra el fichero para que el hub marque el modelo como presente");
        assert!(events.windows(2).all(|w| w[0].done_bytes <= w[1].done_bytes));

        let mut again = Vec::new();
        download_all(&dir, &url, &TEST_FILES, &mut |p| again.push(p)).unwrap();
        assert_eq!(hits.load(Ordering::SeqCst), 2, "lo que ya está entero no se vuelve a pedir");
        assert_eq!(again.last().map(|p| (p.state, p.file.as_str())), Some(("done", "b.txt")));
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

    const TOKENS_OK: &[u8] = b"<unk> 0\n<blk> 1\n\xe2\x96\x81de 2\n";

    fn tok_files() -> [ModelFile; 2] {
        [
            ModelFile { name: "a.bin", sha256: TEST_FILES[0].sha256, size: 3 },
            ModelFile { name: "tokens.txt", sha256: Box::leak(sha256_hex(TOKENS_OK).unwrap().into_boxed_str()), size: TOKENS_OK.len() as u64 },
        ]
    }

    /// Rompedor 22-oct, ALTA 2: lo que llega a sherpa-onnx tiene que estar entero y verificado.
    #[test]
    fn verificar_rechaza_falta_tamano_hash_y_tokens() {
        let files = tok_files();
        let dir = tmp("verify");
        assert!(verify_with(&dir, &files, true).unwrap_err().contains("Falta a.bin"));
        put(&dir, "a.bin", b"abcd");
        put(&dir, "tokens.txt", TOKENS_OK);
        assert!(verify_with(&dir, &files, true).unwrap_err().contains("incompleto"));
        put(&dir, "a.bin", b"abd"); // mismo tamaño, bytes dañados
        assert!(verify_with(&dir, &files, true).unwrap_err().contains("dañado"));
        assert!(!is_complete(&dir, &files), "un fichero ya visto dañado no cuenta como completo aunque mida lo justo");
        assert!(!status_for(&dir, &files, false, None).files[0].ok, "el hub ofrece bajarlo de nuevo");
        put(&dir, "a.bin", b"abc");
        verify_with(&dir, &files, true).unwrap();
        assert!(is_complete(&dir, &files));
        let _ = fs::remove_dir_all(dir);

        // tokens.txt con el hash de la tabla pero mal formado (otra tabla de hashes, carpeta a mano).
        for (bad, why) in [(&b"\xff\xfe basura"[..], "UTF-8"), (b"hola\nadios 3\n", "línea 1"), (b"\n\n", "vacío"), (b"<unk> x\n", "línea 1")] {
            let p = std::env::temp_dir().join(format!("pronto-modeldl-{}-tokens.txt", std::process::id()));
            fs::write(&p, bad).unwrap();
            let e = validate_tokens(&p).unwrap_err();
            assert!(e.contains(why), "{why}: {e}");
            let _ = fs::remove_file(p);
        }
    }

    /// El SHA-256 se calcula una vez por fichero y se guarda en `.verificado`: el arranque no
    /// rehashea 650 MB. Si el fichero cambia, aunque conserve tamaño y fecha (rompedor 22-oct-b,
    /// BAJA 7: la marca se envenenaba así), se vuelve a comprobar.
    #[test]
    fn verificar_cachea_y_no_se_fia_de_tamano_y_fecha() {
        let files = tok_files();
        let dir = tmp("verify-cache");
        put(&dir, "a.bin", b"abc");
        put(&dir, "tokens.txt", TOKENS_OK);
        verify_with(&dir, &files, true).unwrap();
        let mark = fs::read_to_string(dir.join(MARK)).unwrap();
        assert!(mark.contains("a.bin\t3\t") && mark.contains("\tv2:") && mark.contains(TEST_FILES[0].sha256), "{mark}");
        // Otro proceso (caché en memoria vacía) con el fichero intacto: se fía de la marca y no
        // hashea. Se demuestra con una marca que miente sobre el hash: si la lee, da «dañado».
        HASHES.lock().clear();
        fs::write(dir.join(MARK), mark.replace(TEST_FILES[0].sha256, &"0".repeat(64))).unwrap();
        assert!(verify_with(&dir, &files, true).unwrap_err().contains("dañado"), "no usó la marca");
        HASHES.lock().clear();
        fs::write(dir.join(MARK), &mark).unwrap();
        verify_with(&dir, &files, true).expect("fichero intacto: la marca vale");
        // Mismo tamaño y misma fecha con otro contenido: la marca ya no vale, se rehashea.
        let mtime = fs::metadata(dir.join("a.bin")).unwrap().modified().unwrap();
        HASHES.lock().clear();
        fs::write(dir.join("a.bin"), b"xyz").unwrap();
        fs::OpenOptions::new().write(true).open(dir.join("a.bin")).unwrap().set_modified(mtime).unwrap();
        assert_eq!(fs::metadata(dir.join("a.bin")).unwrap().modified().unwrap(), mtime);
        assert!(verify_with(&dir, &files, true).unwrap_err().contains("dañado"), "se fió de una marca envenenada");
        // Una marca del formato antiguo (tamaño y fecha) no se usa.
        put(&dir, "a.bin", b"abc");
        HASHES.lock().clear();
        let m = fs::metadata(dir.join("a.bin")).unwrap();
        let ns = m.modified().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        fs::write(dir.join(MARK), format!("a.bin\t3\t{ns}\t{}\n", "0".repeat(64))).unwrap();
        verify_with(&dir, &files, true).expect("la marca antigua se ignora y se rehashea");
        let _ = fs::remove_dir_all(dir);
    }

    /// Servidor con respuestas crudas por ruta.
    fn serve_raw(routes: Vec<(&'static str, Vec<u8>)>) -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = hits.clone();
        std::thread::spawn(move || {
            for conn in listener.incoming().flatten() {
                counter.fetch_add(1, Ordering::SeqCst);
                let mut reader = BufReader::new(conn.try_clone().unwrap());
                let mut line = String::new();
                let _ = reader.read_line(&mut line);
                loop {
                    let mut h = String::new();
                    if reader.read_line(&mut h).is_err() || h.trim().is_empty() {
                        break;
                    }
                }
                let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
                let mut conn = conn;
                let resp = routes.iter().find(|(p, _)| *p == path).map(|(_, r)| r.clone());
                let _ = conn.write_all(&resp.unwrap_or_else(|| b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()));
            }
        });
        (url, hits)
    }

    fn redirect(to: &str) -> Vec<u8> {
        format!("HTTP/1.1 302 Found\r\nLocation: {to}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").into_bytes()
    }

    /// Rompedor 22-oct-c, descargas MEDIA 1: las redirecciones (Hugging Face -> su CDN) se siguen a
    /// mano; a una URL sin host o de otro esquema, error y no pánico. PRNT-41: tope en el tamaño.
    #[test]
    fn redirecciones_y_tope_del_modelo() {
        let dir = tmp("dl-redir");
        let one = [ModelFile { name: "a.bin", sha256: TEST_FILES[0].sha256, size: 3 }];
        let mut chorro = b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n".to_vec();
        chorro.extend(std::iter::repeat_n(b'x', 300_000));
        let (url, hits) = serve_raw(vec![
            ("/ok/a.bin", redirect("/cdn/a.bin")),
            ("/cdn/a.bin", b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc".to_vec()),
            ("/file/a.bin", redirect("file:///C:/Windows/win.ini")),
            ("/data/a.bin", redirect("data:,abc")),
            ("/ftp/a.bin", redirect("ftp://127.0.0.1/a.bin")),
            ("/fuera/a.bin", redirect("http://evil.example/a.bin")),
            ("/bucle/a.bin", redirect("/bucle/a.bin")),
            ("/grande/a.bin", chorro),
            ("/anuncia/a.bin", b"HTTP/1.1 200 OK\r\nContent-Length: 9999999\r\nConnection: close\r\n\r\nabc".to_vec()),
        ]);
        download_all(&dir, &format!("{url}/ok"), &one, &mut |_| {}).unwrap();
        assert_eq!(fs::read(dir.join("a.bin")).unwrap(), b"abc");
        assert_eq!(hits.load(Ordering::SeqCst), 2);
        fs::remove_file(dir.join("a.bin")).unwrap();
        for (ruta, esperado) in [
            ("file", "no es http"),
            ("data", "no es http"),
            ("ftp", "no es http"),
            ("fuera", "no permitido"),
            ("bucle", "redirecciones"),
            ("grande", "más de lo que mide"),
            ("anuncia", "anuncia"),
        ] {
            let mut max = 0u64;
            let e = download_all(&dir, &format!("{url}/{ruta}"), &one, &mut |p| max = max.max(p.done_bytes)).unwrap_err();
            assert!(e.contains(esperado), "{ruta}: {e}");
            assert!(max <= 3, "{ruta}: {max} bytes");
            assert!(!dir.join("a.bin.part").exists() && !dir.join("a.bin").exists(), "{ruta}");
        }
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
