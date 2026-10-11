//! «Activar IA local»: deja Ollama y el modelo pequeño (`qwen3:1.7b`) listos con un solo botón.
//!
//! Un hilo propio, cancelable, que según lo que falte: (1) descarga `OllamaSetup.exe`, (2) comprueba su
//! firma Authenticode (WinVerifyTrust, firmante «Ollama…»), (3) lo ejecuta en silencio (Inno Setup, por
//! usuario, sin administrador), (4) arranca Ollama y espera a su API y (5) baja el modelo con
//! `POST /api/pull`. El avance sale por `ai://progress`.

use crate::models::{Settings, DEFAULT_AI_MODEL};
use crate::state::AppState;
use crate::{commands, ollama};
use parking_lot::Mutex;
use serde::Serialize;
use serde_json::{json, Value};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

pub const SETUP_URL: &str = "https://ollama.com/download/OllamaSetup.exe";
/// Banderas de Inno Setup: todo oculto, sin cuadros de mensaje, sin reiniciar y sin la pantalla de bienvenida.
/// El instalador de Ollama es por usuario (`PrivilegesRequired=lowest`), así que no pide administrador.
const INSTALL_FLAGS: [&str; 4] = ["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/SP-"];
/// Un `OllamaSetup.exe` de verdad pesa más de 1 GB; menos de esto es una página de error o un fichero cortado.
const MIN_SETUP_BYTES: u64 = 50 * 1024 * 1024;
const EVENT: &str = "ai://progress";
const EMIT_EVERY: Duration = Duration::from_millis(150);
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

static RUNNING: AtomicBool = AtomicBool::new(false);
static CANCEL: AtomicBool = AtomicBool::new(false);
static LAST: Mutex<Option<AiProgress>> = Mutex::new(None);

// ------------------------------------------------------------ tipos

/// Fases: `check` · `download-ollama` · `install-ollama` · `start-ollama` · `pull-model` · `done` · `error` · `cancelled`.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AiProgress {
    pub phase: &'static str,
    /// Bytes recibidos en la fase (descarga de Ollama o del modelo); 0 en las demás.
    pub received: u64,
    /// Bytes totales de la fase; 0 = no se sabe.
    pub total: u64,
    /// Texto para mostrar; en `error`, el motivo en español.
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSetupStatus {
    pub running: bool,
    /// Fase en curso o la última (`idle` si no hay nada que contar, `done` si ya está todo).
    pub phase: String,
    pub received: u64,
    pub total: u64,
    pub message: String,
    pub ollama_installed: bool,
    pub ollama_running: bool,
    pub model: String,
    pub model_ready: bool,
    /// Ollama responde y los modelos de los ajustes están descargados.
    pub ready: bool,
}

fn prog(phase: &'static str, received: u64, total: u64, message: &str) -> AiProgress {
    AiProgress { phase, received, total, message: message.to_string() }
}

/// Por qué se corta el proceso: cancelado por el usuario o fallo con motivo.
#[derive(Debug, PartialEq)]
enum Stop {
    Cancelled,
    Failed(String),
}

impl From<String> for Stop {
    fn from(s: String) -> Self {
        Stop::Failed(s)
    }
}

type Emit<'a> = &'a mut dyn FnMut(AiProgress);

fn check(cancel: &AtomicBool) -> Result<(), Stop> {
    if cancel.load(Ordering::SeqCst) { Err(Stop::Cancelled) } else { Ok(()) }
}

// ------------------------------------------------------------ detección

/// Nombre tal como lo lista Ollama (`llama3.2` = `llama3.2:latest`).
fn normalize(model: &str) -> String {
    let m = model.trim().to_lowercase();
    if m.contains(':') { m } else { format!("{m}:latest") }
}

fn has_model(models: &[String], wanted: &str) -> bool {
    !wanted.trim().is_empty() && models.iter().any(|m| normalize(m) == normalize(wanted))
}

/// ¿Faltan modelos? Los de los ajustes (`ollamaModel`, `aiPolishModel`) deben estar en Ollama.
fn settings_models_missing(s: &Settings, models: &[String]) -> bool {
    !has_model(models, &s.ollama_model) || !has_model(models, &s.ai_polish_model)
}

/// `ollama.exe` del equipo: la ruta del instalador por usuario, la de Archivos de programa o el PATH.
pub fn find_ollama() -> Option<PathBuf> {
    let mut cands = Vec::new();
    if let Some(l) = dirs::data_local_dir() {
        cands.push(l.join("Programs").join("Ollama").join("ollama.exe"));
    }
    if let Some(p) = std::env::var_os("ProgramFiles") {
        cands.push(PathBuf::from(p).join("Ollama").join("ollama.exe"));
    }
    if let Some(path) = std::env::var_os("PATH") {
        cands.extend(std::env::split_paths(&path).map(|d| d.join("ollama.exe")));
    }
    cands.into_iter().find(|p| p.is_file())
}

/// ¿`url` apunta a este equipo? Solo entonces tiene sentido instalar o arrancar Ollama.
fn is_local_url(url: &str) -> bool {
    let rest = url.trim().split_once("://").map_or(url.trim(), |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = if host.starts_with('[') { host.split(']').next().unwrap_or("").trim_start_matches('[') } else { host.split(':').next().unwrap_or("") };
    matches!(host.to_lowercase().as_str(), "localhost" | "127.0.0.1" | "::1" | "0.0.0.0")
}

pub fn status(s: &Settings) -> AiSetupStatus {
    let o = ollama::status(&s.ollama_url);
    let running = RUNNING.load(Ordering::SeqCst);
    let last = LAST.lock().clone();
    let model_ready = o.reachable && !settings_models_missing(s, &o.models);
    let ready = model_ready;
    let (phase, received, total, message) = match last {
        Some(p) if running || matches!(p.phase, "error" | "cancelled") && !ready => (p.phase.to_string(), p.received, p.total, p.message),
        _ if ready => ("done".to_string(), 0, 0, String::new()),
        _ => ("idle".to_string(), 0, 0, String::new()),
    };
    AiSetupStatus {
        running,
        phase,
        received,
        total,
        message,
        ollama_installed: o.reachable || find_ollama().is_some(),
        ollama_running: o.reachable,
        model: DEFAULT_AI_MODEL.into(),
        model_ready,
        ready,
    }
}

// ------------------------------------------------------------ descarga del instalador

/// Baja `url` a `dest` (sigue redirecciones: ollama.com -> GitHub) emitiendo los bytes recibidos.
fn download_installer(agent: &ureq::Agent, url: &str, dest: &Path, cancel: &AtomicBool, emit: Emit) -> Result<u64, Stop> {
    let resp = agent.get(url).call().map_err(|e| match e {
        ureq::Error::Status(code, _) => format!("El servidor de Ollama respondió {code} al descargar el instalador."),
        e => format!("No se pudo conectar para descargar Ollama: {e}"),
    })?;
    let total: u64 = resp.header("Content-Length").and_then(|v| v.trim().parse().ok()).unwrap_or(0);
    let mut reader = resp.into_reader();
    let mut out = File::create(dest).map_err(|e| format!("No se pudo crear el fichero temporal: {e}"))?;
    let mut buf = vec![0u8; 256 * 1024];
    let (mut got, mut last_emit) = (0u64, Instant::now() - EMIT_EVERY);
    loop {
        check(cancel)?;
        let n = reader.read(&mut buf).map_err(|e| format!("Se cortó la descarga de Ollama: {e}"))?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n]).map_err(|e| format!("No se pudo escribir en disco (¿falta espacio?): {e}"))?;
        got += n as u64;
        if last_emit.elapsed() >= EMIT_EVERY {
            last_emit = Instant::now();
            emit(prog("download-ollama", got, total, "Descargando Ollama…"));
        }
    }
    out.flush().map_err(|e| format!("No se pudo escribir en disco: {e}"))?;
    if total > 0 && got != total {
        return Err(format!("Descarga de Ollama incompleta ({got} de {total} bytes). Inténtalo de nuevo.").into());
    }
    emit(prog("download-ollama", got, got.max(total), "Descargando Ollama…"));
    Ok(got)
}

// ------------------------------------------------------------ firma Authenticode

fn trust_error(hr: i32) -> String {
    match hr as u32 {
        0x800B_0100 => "no está firmado".into(),
        0x800B_0109 | 0x800B_0101 | 0x800B_010A => "la firma no es de una entidad de confianza".into(),
        0x8009_6010 | 0x8009_6011 => "el fichero está modificado o dañado".into(),
        0x800B_0111 => "la firma no es válida".into(),
        0x800B_0004 => "la firma no es de una entidad de confianza".into(),
        code => format!("la firma no es válida (0x{code:08X})"),
    }
}

/// Comprueba la firma Authenticode de `path` con `WinVerifyTrust`. Devuelve el nombre del firmante
/// (CN del certificado) o el motivo del rechazo.
pub fn verify_signature(path: &Path) -> Result<String, String> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{HANDLE, HWND};
    use windows::Win32::Security::Cryptography::{CertGetNameStringW, CERT_NAME_SIMPLE_DISPLAY_TYPE};
    use windows::Win32::Security::WinTrust::*;

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let mut file = WINTRUST_FILE_INFO {
        cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: PCWSTR(wide.as_ptr()),
        hFile: HANDLE::default(),
        pgKnownSubject: std::ptr::null_mut(),
    };
    let mut data = WINTRUST_DATA::default();
    data.cbStruct = std::mem::size_of::<WINTRUST_DATA>() as u32;
    data.dwUIChoice = WTD_UI_NONE;
    data.fdwRevocationChecks = WTD_REVOKE_NONE;
    data.dwUnionChoice = WTD_CHOICE_FILE;
    data.Anonymous.pFile = &mut file;
    data.dwStateAction = WTD_STATEACTION_VERIFY;
    data.dwProvFlags = WTD_REVOCATION_CHECK_NONE;
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    // INVALID_HANDLE_VALUE = «sin ventana»: nunca se muestra interfaz.
    let hwnd = HWND(usize::MAX as *mut core::ffi::c_void);
    let call = |data: &mut WINTRUST_DATA, action: &mut windows::core::GUID| unsafe {
        WinVerifyTrust(hwnd, action, data as *mut WINTRUST_DATA as *mut core::ffi::c_void)
    };

    let hr = call(&mut data, &mut action);
    let result = if hr == 0 {
        // SAFETY: tras un VERIFY correcto `hWVTStateData` es válido hasta el CLOSE de abajo.
        let name = unsafe {
            let prov = WTHelperProvDataFromStateData(data.hWVTStateData);
            let sgnr = if prov.is_null() { std::ptr::null_mut() } else { WTHelperGetProvSignerFromChain(prov, 0, false, 0) };
            let cert = if sgnr.is_null() { std::ptr::null_mut() } else { WTHelperGetProvCertFromChain(sgnr, 0) };
            if cert.is_null() || (*cert).pCert.is_null() {
                String::new()
            } else {
                let mut buf = [0u16; 256];
                let n = CertGetNameStringW((*cert).pCert, CERT_NAME_SIMPLE_DISPLAY_TYPE, 0, None, Some(&mut buf)) as usize;
                String::from_utf16_lossy(&buf[..n.saturating_sub(1).min(buf.len())])
            }
        };
        if name.is_empty() { Err("no se pudo leer el firmante".to_string()) } else { Ok(name) }
    } else {
        Err(trust_error(hr))
    };
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    call(&mut data, &mut action);
    result
}

/// Firma válida **y** firmante «Ollama…» (el instalador oficial va firmado por «Ollama Inc.»).
fn verify_ollama_setup(path: &Path) -> Result<String, String> {
    let signer = verify_signature(path).map_err(|e| format!("No se instaló Ollama: la firma del instalador descargado no es válida ({e})."))?;
    if !signer.to_lowercase().starts_with("ollama") {
        return Err(format!("No se instaló Ollama: el instalador descargado está firmado por «{signer}», no por Ollama."));
    }
    Ok(signer)
}

// ------------------------------------------------------------ instalar y arrancar

fn run_installer(exe: &Path, timeout: Duration, emit: Emit) -> Result<(), Stop> {
    let mut child = Command::new(exe)
        .args(INSTALL_FLAGS)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("No se pudo ejecutar el instalador de Ollama: {e}"))?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => {
                return Err(format!("El instalador de Ollama terminó con un error (código {}).", status.code().unwrap_or(-1)).into());
            }
            Ok(None) => {}
            Err(e) => return Err(format!("No se pudo vigilar el instalador de Ollama: {e}").into()),
        }
        if started.elapsed() > timeout {
            return Err("El instalador de Ollama tarda demasiado; ciérralo desde el Administrador de tareas e inténtalo de nuevo.".to_string().into());
        }
        emit(prog("install-ollama", 0, 0, "Instalando Ollama…"));
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// Lanza Ollama oculto y sin esperarlo: su aplicación de bandeja si existe (la que arranca el servidor,
/// como tras una instalación normal) o `ollama serve`.
fn start_ollama(exe: &Path) -> Result<(), String> {
    let dir = exe.parent().unwrap_or(Path::new("."));
    let app = dir.join("ollama app.exe");
    if app.is_file() { spawn_hidden(&app, &[], dir, &[]) } else { spawn_hidden(exe, &["serve"], dir, &[]) }.map(|_| ())
}

fn spawn_hidden(prog: &Path, args: &[&str], dir: &Path, env: &[(&str, &str)]) -> Result<std::process::Child, String> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let mut paths = vec![dir.to_path_buf()];
    paths.extend(std::env::split_paths(&path));
    let mut cmd = Command::new(prog);
    cmd.args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP);
    if let Ok(joined) = std::env::join_paths(paths) {
        cmd.env("PATH", joined);
    }
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.spawn().map_err(|e| format!("No se pudo arrancar Ollama ({}): {e}", prog.display()))
}

/// Espera a que la API responda. Si pasados `launch_after` aún no responde, llama una vez a `launch`.
fn wait_reachable(
    url: &str,
    timeout: Duration,
    launch_after: Duration,
    launch: &mut dyn FnMut() -> Result<(), String>,
    cancel: &AtomicBool,
    emit: Emit,
) -> Result<(), Stop> {
    let started = Instant::now();
    let mut launched = false;
    loop {
        check(cancel)?;
        if ollama::status(url).reachable {
            return Ok(());
        }
        if !launched && started.elapsed() >= launch_after {
            launched = true;
            launch()?;
        }
        if started.elapsed() > timeout {
            return Err(format!("Ollama no responde en {url}. Ábrelo desde el menú Inicio e inténtalo de nuevo.").into());
        }
        emit(prog("start-ollama", 0, 0, "Arrancando Ollama…"));
        std::thread::sleep(Duration::from_millis(500));
    }
}

// ------------------------------------------------------------ bajar el modelo

/// Suma el avance de las capas del `pull` (cada una con su `digest`, `total` y `completed`).
#[derive(Default)]
struct PullTracker {
    layers: Vec<(String, u64, u64)>,
}

impl PullTracker {
    /// Procesa una línea NDJSON. `Err` = línea de error de Ollama.
    fn feed(&mut self, line: &Value, model: &str) -> Result<(u64, u64), String> {
        if let Some(e) = line["error"].as_str() {
            return Err(if e.contains("file does not exist") || e.contains("not found") {
                format!("Ollama no encuentra el modelo {model} en su biblioteca.")
            } else {
                format!("Ollama: {e}")
            });
        }
        if let (Some(d), Some(total)) = (line["digest"].as_str(), line["total"].as_u64()) {
            let done = line["completed"].as_u64().unwrap_or(0).min(total);
            match self.layers.iter_mut().find(|l| l.0 == d) {
                Some(l) => (l.1, l.2) = (total, done.max(l.2)),
                None => self.layers.push((d.to_string(), total, done)),
            }
        }
        Ok(self.sum())
    }

    fn sum(&self) -> (u64, u64) {
        (self.layers.iter().map(|l| l.2).sum(), self.layers.iter().map(|l| l.1).sum())
    }
}

fn pull_message(status: &str) -> &'static str {
    match status {
        "pulling manifest" => "Preparando la descarga del modelo…",
        s if s.starts_with("verifying") => "Verificando el modelo…",
        "writing manifest" | "removing any unused layers" | "success" => "Terminando…",
        _ => "Descargando el modelo…",
    }
}

/// `POST /api/pull` en streaming; emite los bytes completados/totales de todas las capas.
fn pull_model(url: &str, model: &str, cancel: &AtomicBool, emit: Emit) -> Result<(), Stop> {
    let agent = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(5)).timeout_read(Duration::from_secs(120)).build();
    let endpoint = format!("{}/api/pull", url.trim().trim_end_matches('/'));
    let resp = agent.post(&endpoint).send_json(json!({ "model": model, "stream": true })).map_err(|e| match e {
        ureq::Error::Status(code, r) => match r.into_json::<Value>().ok().and_then(|v| v["error"].as_str().map(String::from)) {
            Some(d) => format!("Ollama respondió: {d}"),
            None => format!("Ollama respondió con el código {code}"),
        },
        e => format!("No se pudo contactar con Ollama para descargar el modelo: {e}"),
    })?;
    let mut tracker = PullTracker::default();
    let (mut last_emit, mut success) = (Instant::now() - EMIT_EVERY, false);
    emit(prog("pull-model", 0, 0, pull_message("pulling manifest")));
    for line in BufReader::new(resp.into_reader()).lines() {
        check(cancel)?;
        let line = line.map_err(|e| format!("Se cortó la descarga del modelo: {e}"))?;
        if line.trim().is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
        let (done, total) = tracker.feed(&v, model)?;
        let status = v["status"].as_str().unwrap_or("");
        success |= status == "success";
        if last_emit.elapsed() >= EMIT_EVERY || status == "success" {
            last_emit = Instant::now();
            emit(prog("pull-model", done, total, pull_message(status)));
        }
    }
    check(cancel)?;
    if !success {
        return Err("Se cortó la descarga del modelo antes de terminar. Inténtalo de nuevo.".to_string().into());
    }
    let (done, total) = tracker.sum();
    emit(prog("pull-model", done.max(total), total, pull_message("success")));
    Ok(())
}

// ------------------------------------------------------------ orquestación

/// Hace lo que falte, en orden: instalar Ollama, arrancarlo y bajar el modelo.
fn setup(settings: &Settings, cancel: &AtomicBool, emit: Emit) -> Result<(), Stop> {
    let url = settings.ollama_url.clone();
    emit(prog("check", 0, 0, "Comprobando…"));
    if !ollama::status(&url).reachable {
        if !is_local_url(&url) {
            return Err(format!("Tu Ollama está configurado en {url}, que no es este equipo y no responde. Revisa la dirección en Configuración.").into());
        }
        let (exe, fresh) = match find_ollama() {
            Some(exe) => (exe, false),
            None => (install_ollama(cancel, emit)?, true),
        };
        check(cancel)?;
        emit(prog("start-ollama", 0, 0, "Arrancando Ollama…"));
        // Recién instalado, el propio instalador lo lanza: se le da un margen antes de abrirlo nosotros.
        let launch_after = Duration::from_secs(if fresh { 8 } else { 0 });
        wait_reachable(&url, Duration::from_secs(90), launch_after, &mut || start_ollama(&exe), cancel, emit)?;
    }
    check(cancel)?;
    let models = ollama::status(&url).models;
    if settings_models_missing(settings, &models) && !has_model(&models, DEFAULT_AI_MODEL) {
        pull_model(&url, DEFAULT_AI_MODEL, cancel, emit)?;
    }
    Ok(())
}

/// Descarga, verifica y ejecuta `OllamaSetup.exe`. Devuelve la ruta de `ollama.exe` instalado.
fn install_ollama(cancel: &AtomicBool, emit: Emit) -> Result<PathBuf, Stop> {
    let dir = std::env::temp_dir().join(format!("Pronto-IA-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis())));
    fs::create_dir_all(&dir).map_err(|e| format!("No se pudo crear la carpeta temporal: {e}"))?;
    let result = (|| {
        let setup = dir.join("OllamaSetup.exe");
        let agent = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(15)).timeout_read(Duration::from_secs(30)).build();
        emit(prog("download-ollama", 0, 0, "Descargando Ollama…"));
        let size = download_installer(&agent, SETUP_URL, &setup, cancel, emit)?;
        if size < MIN_SETUP_BYTES {
            return Err(Stop::Failed("El instalador de Ollama descargado es demasiado pequeño; no se ejecuta.".into()));
        }
        check(cancel)?;
        emit(prog("install-ollama", 0, 0, "Comprobando la firma del instalador…"));
        verify_ollama_setup(&setup)?;
        check(cancel)?;
        emit(prog("install-ollama", 0, 0, "Instalando Ollama…"));
        run_installer(&setup, Duration::from_secs(20 * 60), emit)?;
        find_ollama().ok_or_else(|| Stop::Failed("El instalador terminó, pero no encuentro Ollama en este equipo.".into()))
    })();
    let _ = fs::remove_dir_all(&dir);
    result
}

/// Deja en los ajustes el modelo pequeño donde el modelo guardado no esté en Ollama. Un modelo elegido
/// a mano que sí está descargado se respeta.
fn adopt_model(s: &mut Settings, models: &[String]) -> bool {
    let mut changed = false;
    for field in [&mut s.ollama_model, &mut s.ai_polish_model] {
        if !has_model(models, field) {
            *field = DEFAULT_AI_MODEL.into();
            changed = true;
        }
    }
    changed
}

fn publish(app: &AppHandle, p: AiProgress) {
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

/// Arranca el proceso en un hilo aparte (no hace nada si ya hay uno en marcha).
pub fn start(app: &AppHandle, st: &Arc<AppState>) -> Result<(), String> {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let guard = Running;
    CANCEL.store(false, Ordering::SeqCst);
    *LAST.lock() = None;
    let (app, st) = (app.clone(), st.clone());
    std::thread::Builder::new()
        .name("pronto-aisetup".into())
        .spawn(move || {
            let _guard = guard;
            let settings = st.settings.read().clone();
            let outcome = setup(&settings, &CANCEL, &mut |p| publish(&app, p)).and_then(|()| {
                // Ollama listo y modelo bajado: deja el modelo en los ajustes que no lo tuvieran.
                // «Pulir cada dictado» (`aiPolish`) no se toca.
                let models = ollama::status(&settings.ollama_url).models;
                if !has_model(&models, DEFAULT_AI_MODEL) && settings_models_missing(&settings, &models) {
                    return Err(Stop::Failed(format!("Ollama no muestra el modelo {DEFAULT_AI_MODEL} tras descargarlo. Inténtalo de nuevo.")));
                }
                let mut new = st.settings.read().clone();
                if adopt_model(&mut new, &models) {
                    commands::apply_settings(&app, &st, new).map_err(Stop::Failed)?;
                }
                Ok(())
            });
            match outcome {
                Ok(()) => publish(&app, prog("done", 0, 0, "IA local lista")),
                Err(Stop::Cancelled) => publish(&app, prog("cancelled", 0, 0, "Cancelado")),
                Err(Stop::Failed(e)) => {
                    eprintln!("[aisetup] {e}");
                    publish(&app, prog("error", 0, 0, &e));
                }
            }
        })
        .map(|_| ())
        .map_err(|e| {
            RUNNING.store(false, Ordering::SeqCst);
            format!("No se pudo lanzar la activación de la IA local: {e}")
        })
}

/// Pide parar. Surte efecto en la descarga y en la espera de Ollama; un instalador ya en marcha termina
/// (cortarlo a medias dejaría Ollama roto) y el proceso se detiene justo después.
pub fn cancel() {
    CANCEL.store(true, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufRead;
    use std::net::TcpListener;
    use std::sync::atomic::AtomicUsize;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pronto-aisetup-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// Servidor HTTP mínimo: `routes` = (ruta, cuerpo). Guarda el cuerpo de la última petición.
    fn serve(routes: Vec<(&'static str, Vec<u8>)>) -> (String, Arc<AtomicUsize>, Arc<Mutex<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let hits = Arc::new(AtomicUsize::new(0));
        let last_body = Arc::new(Mutex::new(String::new()));
        let (counter, body_store) = (hits.clone(), last_body.clone());
        std::thread::spawn(move || {
            for conn in listener.incoming().flatten() {
                counter.fetch_add(1, Ordering::SeqCst);
                let mut reader = BufReader::new(conn.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let mut len = 0usize;
                loop {
                    let mut h = String::new();
                    reader.read_line(&mut h).unwrap();
                    if h.trim().is_empty() {
                        break;
                    }
                    if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                        len = v.trim().parse().unwrap_or(0);
                    }
                }
                let mut body = vec![0u8; len];
                reader.read_exact(&mut body).unwrap();
                *body_store.lock() = String::from_utf8_lossy(&body).into_owned();
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
        (url, hits, last_body)
    }

    fn not_cancelled() -> AtomicBool {
        AtomicBool::new(false)
    }

    #[test]
    fn nombres_de_modelo_y_faltantes() {
        let models = vec!["qwen3:1.7b".to_string(), "llama3.2:latest".to_string()];
        assert!(has_model(&models, "qwen3:1.7b") && has_model(&models, "QWEN3:1.7B"));
        assert!(has_model(&models, "llama3.2"), "sin etiqueta = :latest");
        assert!(!has_model(&models, "qwen2.5:7b") && !has_model(&models, "") && !has_model(&models, "qwen3"));
        let mut s = Settings::default();
        assert!(!settings_models_missing(&s, &models));
        s.ai_polish_model = "gemma2:2b".into();
        assert!(settings_models_missing(&s, &models));
        // El modelo elegido a mano y presente se respeta; el que falta pasa al pequeño.
        s.ollama_model = "llama3.2".into();
        assert!(adopt_model(&mut s, &models));
        assert_eq!((s.ollama_model.as_str(), s.ai_polish_model.as_str()), ("llama3.2", DEFAULT_AI_MODEL));
        assert!(!adopt_model(&mut s, &models));
        assert!(!s.ai_polish, "nunca se activa «Pulir cada dictado»");
    }

    #[test]
    fn direcciones_locales() {
        for u in ["http://localhost:11434", "http://127.0.0.1:11434/", "localhost", "http://[::1]:11434", "HTTP://LocalHost:1"] {
            assert!(is_local_url(u), "{u}");
        }
        for u in ["http://192.168.1.5:11434", "http://mi-servidor:11434", "https://ollama.example.com", "http://localhost.evil.com"] {
            assert!(!is_local_url(u), "{u}");
        }
    }

    #[test]
    fn banderas_del_instalador() {
        assert_eq!(INSTALL_FLAGS, ["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/SP-"]);
        assert_eq!(SETUP_URL, "https://ollama.com/download/OllamaSetup.exe");
    }

    #[test]
    fn json_del_evento() {
        let v = serde_json::to_value(prog("pull-model", 5, 10, "x")).unwrap();
        assert_eq!(v, json!({ "phase": "pull-model", "received": 5, "total": 10, "message": "x" }));
    }

    #[test]
    fn tracker_suma_capas_y_traduce_errores() {
        let mut t = PullTracker::default();
        let l = |s: &str| serde_json::from_str::<Value>(s).unwrap();
        assert_eq!(t.feed(&l(r#"{"status":"pulling manifest"}"#), "m").unwrap(), (0, 0));
        assert_eq!(t.feed(&l(r#"{"status":"pulling aaa","digest":"sha256:aaa","total":100,"completed":10}"#), "m").unwrap(), (10, 100));
        assert_eq!(t.feed(&l(r#"{"status":"pulling aaa","digest":"sha256:aaa","total":100,"completed":60}"#), "m").unwrap(), (60, 100));
        assert_eq!(t.feed(&l(r#"{"status":"pulling bbb","digest":"sha256:bbb","total":50}"#), "m").unwrap(), (60, 150));
        assert_eq!(t.feed(&l(r#"{"digest":"sha256:bbb","total":50,"completed":999}"#), "m").unwrap(), (110, 150), "completed acotado al total");
        assert_eq!(t.feed(&l(r#"{"digest":"sha256:aaa","total":100,"completed":5}"#), "m").unwrap().0, 110, "no retrocede");
        let e = t.feed(&l(r#"{"error":"pull model manifest: file does not exist"}"#), "qwen3:1.7b").unwrap_err();
        assert!(e.contains("qwen3:1.7b") && e.contains("biblioteca"), "{e}");
        assert!(t.feed(&l(r#"{"error":"disco lleno"}"#), "m").unwrap_err().contains("disco lleno"));
    }

    #[test]
    fn pull_lee_el_ndjson_y_emite_progreso() {
        let ndjson = [
            r#"{"status":"pulling manifest"}"#,
            r#"{"status":"pulling aaa","digest":"sha256:aaa","total":200,"completed":50}"#,
            r#"{"status":"pulling aaa","digest":"sha256:aaa","total":200,"completed":200}"#,
            r#"{"status":"pulling bbb","digest":"sha256:bbb","total":40,"completed":40}"#,
            r#"{"status":"verifying sha256 digest"}"#,
            r#"{"status":"writing manifest"}"#,
            r#"{"status":"success"}"#,
        ]
        .join("\n");
        let (url, _, body) = serve(vec![("/api/pull", ndjson.into_bytes())]);
        let mut events = Vec::new();
        pull_model(&url, "qwen3:1.7b", &not_cancelled(), &mut |p| events.push(p)).unwrap();
        let sent: Value = serde_json::from_str(&body.lock()).unwrap();
        assert_eq!(sent, json!({ "model": "qwen3:1.7b", "stream": true }));
        assert!(events.iter().all(|e| e.phase == "pull-model"));
        assert!(events.windows(2).all(|w| w[0].received <= w[1].received), "monótono");
        let last = events.last().unwrap();
        assert_eq!((last.received, last.total), (240, 240));
    }

    #[test]
    fn pull_con_error_corte_o_cancelacion() {
        let (url, _, _) = serve(vec![("/api/pull", br#"{"status":"pulling manifest"}
{"error":"pull model manifest: file does not exist"}"#.to_vec())]);
        let Err(Stop::Failed(e)) = pull_model(&url, "nope", &not_cancelled(), &mut |_| {}) else { panic!("debía fallar") };
        assert!(e.contains("nope"), "{e}");

        let (url, _, _) = serve(vec![("/api/pull", br#"{"status":"pulling manifest"}"#.to_vec())]);
        let Err(Stop::Failed(e)) = pull_model(&url, "m", &not_cancelled(), &mut |_| {}) else { panic!("debía fallar") };
        assert!(e.contains("antes de terminar"), "{e}");

        let (url, _, _) = serve(vec![("/api/pull", br#"{"status":"success"}"#.to_vec())]);
        let cancel = AtomicBool::new(true);
        assert_eq!(pull_model(&url, "m", &cancel, &mut |_| {}), Err(Stop::Cancelled));

        let (url, _, _) = serve(vec![]);
        let Err(Stop::Failed(e)) = pull_model(&url, "m", &not_cancelled(), &mut |_| {}) else { panic!("debía fallar") };
        assert!(e.contains("404") || e.contains("Ollama"), "{e}");
    }

    #[test]
    fn descarga_del_instalador_con_progreso_y_comprobaciones() {
        let dir = tmp("dl");
        let data: Vec<u8> = (0..600_000u32).map(|i| (i % 251) as u8).collect();
        let (url, _, _) = serve(vec![("/setup.exe", data.clone())]);
        let agent = ureq::AgentBuilder::new().build();
        let mut events = Vec::new();
        let dest = dir.join("setup.exe");
        let got = download_installer(&agent, &format!("{url}/setup.exe"), &dest, &not_cancelled(), &mut |p| events.push(p)).unwrap();
        assert_eq!(got, data.len() as u64);
        assert_eq!(fs::read(&dest).unwrap(), data);
        let last = events.last().unwrap();
        assert_eq!((last.phase, last.received, last.total), ("download-ollama", 600_000, 600_000));
        assert!(events.windows(2).all(|w| w[0].received <= w[1].received));

        // Cancelado de antemano: no deja el fichero a medias sin avisar.
        let r = download_installer(&agent, &format!("{url}/setup.exe"), &dir.join("c.exe"), &AtomicBool::new(true), &mut |_| {});
        assert_eq!(r, Err(Stop::Cancelled));
        // 404.
        let Err(Stop::Failed(e)) = download_installer(&agent, &format!("{url}/no"), &dir.join("d.exe"), &not_cancelled(), &mut |_| {}) else { panic!() };
        assert!(e.contains("404"), "{e}");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn firma_rechaza_sin_firma_y_firmante_ajeno() {
        let dir = tmp("sig");
        let fake = dir.join("OllamaSetup.exe");
        fs::write(&fake, b"MZ esto no es un instalador").unwrap();
        let e = verify_ollama_setup(&fake).unwrap_err();
        assert!(e.contains("firma") && e.contains("No se instaló"), "{e}");
        assert!(verify_signature(&dir.join("no-existe.exe")).is_err());
        // Un binario con firma válida de otra empresa (Microsoft) tampoco vale como Ollama.
        let notepad = Path::new(r"C:\Windows\System32\notepad.exe");
        if notepad.exists() {
            if let Err(e) = verify_ollama_setup(notepad) {
                assert!(e.contains("No se instaló"), "{e}");
            } else {
                panic!("notepad.exe no debería pasar por Ollama");
            }
        }
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn firma_valida_del_ollama_instalado_si_existe() {
        if let Some(exe) = find_ollama() {
            let signer = verify_signature(&exe).expect("el ollama.exe instalado va firmado");
            assert!(signer.to_lowercase().starts_with("ollama"), "{signer}");
            assert_eq!(verify_ollama_setup(&exe).unwrap(), signer);
        }
    }

    #[test]
    fn espera_a_la_api_y_lanza_una_vez() {
        // Puerto cerrado: se agota el plazo, `launch` se llama una sola vez.
        let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let url = format!("http://127.0.0.1:{port}");
        let launches = AtomicUsize::new(0);
        let r = wait_reachable(&url, Duration::from_millis(900), Duration::from_millis(0), &mut || { launches.fetch_add(1, Ordering::SeqCst); Ok(()) }, &not_cancelled(), &mut |_| {});
        assert!(matches!(r, Err(Stop::Failed(ref e)) if e.contains("no responde")), "{r:?}");
        assert_eq!(launches.load(Ordering::SeqCst), 1);
        // Cancelación inmediata.
        assert_eq!(wait_reachable(&url, Duration::from_secs(5), Duration::from_secs(5), &mut || Ok(()), &AtomicBool::new(true), &mut |_| {}), Err(Stop::Cancelled));
        // Ya responde: no lanza nada.
        let (ok_url, _, _) = serve(vec![("/api/tags", br#"{"models":[]}"#.to_vec())]);
        let n = AtomicUsize::new(0);
        wait_reachable(&ok_url, Duration::from_secs(5), Duration::from_secs(0), &mut || { n.fetch_add(1, Ordering::SeqCst); Ok(()) }, &not_cancelled(), &mut |_| {}).unwrap();
        assert_eq!(n.load(Ordering::SeqCst), 0);
    }

    /// Real: descarga `OllamaSetup.exe` a una carpeta temporal y comprueba su firma (NO lo ejecuta).
    /// `cargo test firma_real -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn firma_real_del_instalador_descargado() {
        let dir = tmp("real-dl");
        let dest = dir.join("OllamaSetup.exe");
        let agent = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(15)).timeout_read(Duration::from_secs(30)).build();
        let mut last = 0u64;
        let size = download_installer(&agent, SETUP_URL, &dest, &not_cancelled(), &mut |p| {
            if p.received / (100 << 20) != last / (100 << 20) {
                println!("{} MB de {}", p.received >> 20, p.total >> 20);
            }
            last = p.received;
        })
        .unwrap();
        assert!(size > MIN_SETUP_BYTES);
        let signer = verify_ollama_setup(&dest).unwrap();
        println!("firmante: {signer}");
        // Un byte cambiado al final (no toca la cabecera) invalida el hash del Authenticode.
        let mut f = fs::OpenOptions::new().append(true).open(&dest).unwrap();
        f.write_all(b"x").unwrap();
        drop(f);
        println!("tras alterarlo: {:?}", verify_signature(&dest));
        let _ = fs::remove_dir_all(dir);
    }

    /// Real: arranca `ollama serve` oculto en otro puerto y lo espera (no toca el Ollama en uso).
    #[test]
    #[ignore]
    fn arranque_real_en_otro_puerto() {
        let exe = find_ollama().expect("Ollama instalado");
        let mut child = spawn_hidden(&exe, &["serve"], exe.parent().unwrap(), &[("OLLAMA_HOST", "127.0.0.1:11999")]).unwrap();
        let r = wait_reachable("http://127.0.0.1:11999", Duration::from_secs(60), Duration::from_secs(600), &mut || Ok(()), &not_cancelled(), &mut |_| {});
        let tags = ollama::status("http://127.0.0.1:11999");
        let _ = child.kill();
        let _ = child.wait();
        r.unwrap();
        println!("modelos: {:?}", tags.models);
    }

    /// Real: baja un modelo diminuto contra el Ollama local y lo borra.
    /// `PRONTO_PULL_MODEL=all-minilm cargo test pull_real -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn pull_real() {
        let url = "http://localhost:11434";
        let model = std::env::var("PRONTO_PULL_MODEL").unwrap_or_else(|_| "all-minilm".into());
        let mut n = 0;
        pull_model(url, &model, &not_cancelled(), &mut |p| {
            n += 1;
            if n % 10 == 1 || p.received == p.total {
                println!("{} {} / {} {}", p.phase, p.received, p.total, p.message);
            }
        })
        .unwrap();
        assert!(has_model(&ollama::status(url).models, &model));
        let r = ureq::request("DELETE", &format!("{url}/api/delete")).send_json(json!({ "model": model }));
        println!("delete: {:?}", r.map(|r| r.status()));
        assert!(!has_model(&ollama::status(url).models, &model));
    }
}
