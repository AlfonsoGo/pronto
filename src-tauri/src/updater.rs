//! Actualizador automático desde las releases de GitHub («Contrato v7 · actualizaciones»).
//!
//! * Un hilo de fondo consulta `releases/latest` poco después de arrancar y cada 4 horas (si el ajuste
//!   `autoUpdate` está activo). Sin red o con el límite de la API agotado no pasa nada: se reintenta luego.
//! * Si la versión del tag es mayor que la de este ejecutable se avisa a la interfaz
//!   (`update://available`) y a la bandeja («Actualizar a X.Y.Z»).
//! * `install` baja el instalador a `%TEMP%\pronto-update`, comprueba tamaño y SHA-256 (el `digest` que
//!   publica la API de GitHub) y lanza un ayudante PowerShell oculto (`update_helper.ps1`) que espera a que
//!   Pronto se cierre, ejecuta el instalador en silencio y vuelve a abrir `pronto.exe`.
//!
//! Solo lectura contra la API de GitHub; nada se instala sin que el usuario pulse «Actualizar ahora».

use crate::state::AppState;
use crate::tray;
use parking_lot::Mutex;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

/// Versión de este ejecutable (la de `Cargo.toml`).
pub const CARGO_VERSION: &str = env!("CARGO_PKG_VERSION");
const LATEST_URL: &str = "https://api.github.com/repos/AlfonsoGo/pronto/releases/latest";
/// Solo para pruebas: otra URL para `releases/latest` (p. ej. un servidor local).
pub const ENV_API: &str = "PRONTO_UPDATE_API";
/// Solo para pruebas: finge que esta es otra versión (p. ej. `2.0.0` para ver el aviso).
pub const ENV_FAKE_VERSION: &str = "PRONTO_FAKE_VERSION";
/// Solo para pruebas: permite instalar desde un ejecutable de `target\release` (en una compilación
/// de depuración nunca se instala).
pub const ENV_ALLOW_DEV: &str = "PRONTO_UPDATE_ALLOW_DEV";

const CHECK_EVERY: Duration = Duration::from_secs(4 * 3600);
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(20);
const RETRY_AFTER_FAILURE: Duration = Duration::from_secs(30 * 60);
const RETRY_AFTER_RATE_LIMIT: Duration = Duration::from_secs(3600);
const TICK: Duration = Duration::from_secs(30);
const EMIT_EVERY: Duration = Duration::from_millis(120);
/// Cuánto esperar a que el ayudante confirme que está vivo antes de cerrar Pronto.
const HELPER_ALIVE_TIMEOUT: Duration = Duration::from_secs(10);
const NOTES_MAX_CHARS: usize = 600;
const NOTES_MAX_LINES: usize = 12;

const EV_AVAILABLE: &str = "update://available";
const EV_PROGRESS: &str = "update://progress";

const HELPER_SCRIPT: &str = include_str!("update_helper.ps1");

// ------------------------------------------------------------ versiones (semver mínimo)

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    /// Identificadores de la parte tras el guion (`rc.1` -> ["rc", "1"]); vacío = versión estable.
    pub pre: Vec<String>,
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.pre.is_empty() {
            write!(f, "-{}", self.pre.join("."))?;
        }
        Ok(())
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater, // la estable gana a su prerelease
                (false, true) => Ordering::Less,
                (false, false) => cmp_pre(&self.pre, &other.pre),
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn cmp_pre(a: &[String], b: &[String]) -> Ordering {
    for (x, y) in a.iter().zip(b) {
        let o = match (x.parse::<u64>(), y.parse::<u64>()) {
            (Ok(x), Ok(y)) => x.cmp(&y),
            (Ok(_), Err(_)) => Ordering::Less, // los numéricos van antes que los alfanuméricos
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(_), Err(_)) => x.cmp(y),
        };
        if o != Ordering::Equal {
            return o;
        }
    }
    a.len().cmp(&b.len())
}

/// `v2.0.2`, `2.0.2`, `2.1`, `2.0.2-rc.1`, `2.0.2+build` -> `Version`. `None` si no es una versión.
pub fn parse_version(s: &str) -> Option<Version> {
    let s = s.trim();
    let s = s.strip_prefix(['v', 'V']).unwrap_or(s);
    let s = s.split('+').next()?;
    let (core, pre) = match s.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (s, None),
    };
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().map_or(Some(0), |x| x.parse().ok())?;
    let patch = parts.next().map_or(Some(0), |x| x.parse().ok())?;
    if parts.next().is_some() {
        return None;
    }
    let pre = match pre {
        None => Vec::new(),
        Some("") => return None,
        Some(p) => {
            let ids: Vec<String> = p.split('.').map(str::to_string).collect();
            if ids.iter().any(|i| i.is_empty() || !i.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')) {
                return None;
            }
            ids
        }
    };
    Some(Version { major, minor, patch, pre })
}

/// Versión efectiva de este ejecutable. `PRONTO_FAKE_VERSION` (solo pruebas) la sustituye.
pub fn current_version() -> Version {
    if let Some(v) = std::env::var(ENV_FAKE_VERSION).ok().and_then(|v| parse_version(&v)) {
        return v;
    }
    parse_version(CARGO_VERSION).unwrap_or(Version { major: 0, minor: 0, patch: 0, pre: Vec::new() })
}

pub fn is_newer(remote: &Version, current: &Version) -> bool {
    remote > current
}

// ------------------------------------------------------------ la release de GitHub

#[derive(Debug, Deserialize)]
struct GhRelease {
    tag_name: String,
    name: Option<String>,
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    html_url: Option<String>,
    published_at: Option<String>,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    name: String,
    #[serde(default)]
    size: u64,
    browser_download_url: String,
    digest: Option<String>,
}

/// El instalador que se descargará.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
    /// SHA-256 en minúsculas, si la API lo publica (`digest: "sha256:..."`).
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    pub tag: String,
    pub title: String,
    pub notes: String,
    pub page_url: String,
    pub published_at: Option<String>,
    pub asset: Asset,
}

/// `sha256:ABC...` -> `abc...` (64 hex). Otros algoritmos o formatos se ignoran.
pub fn parse_digest(digest: Option<&str>) -> Option<String> {
    let hex = digest?.trim().strip_prefix("sha256:")?.trim().to_ascii_lowercase();
    (hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit())).then_some(hex)
}

/// Interpreta el JSON de `releases/latest`. `Ok(None)` = nada que instalar (borrador, prerelease o sin
/// ningún `.exe`); `Err` = JSON ilegible o tag que no es una versión. Toma el PRIMER asset `.exe`.
pub fn parse_release(json: &str) -> Result<Option<Release>, String> {
    let gh: GhRelease = serde_json::from_str(json).map_err(|e| format!("La respuesta de GitHub no se entiende: {e}"))?;
    let version = parse_version(&gh.tag_name).ok_or_else(|| format!("El tag «{}» no es una versión", gh.tag_name))?;
    if gh.draft || gh.prerelease || !version.pre.is_empty() {
        return Ok(None);
    }
    let Some(a) = gh.assets.iter().find(|a| a.name.to_ascii_lowercase().ends_with(".exe")) else {
        return Ok(None);
    };
    let tag = gh.tag_name.trim().to_string();
    Ok(Some(Release {
        title: gh.name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| format!("Pronto {version}")),
        notes: plain_notes(gh.body.as_deref().unwrap_or(""), NOTES_MAX_CHARS, NOTES_MAX_LINES),
        page_url: gh.html_url.unwrap_or_default(),
        published_at: gh.published_at,
        asset: Asset { name: safe_file_name(&a.name), url: a.browser_download_url.clone(), size: a.size, sha256: parse_digest(a.digest.as_deref()) },
        version,
        tag,
    }))
}

/// Nombre de fichero seguro para guardar el instalador en `%TEMP%`.
fn safe_file_name(name: &str) -> String {
    let base = Path::new(name).file_name().and_then(|n| n.to_str()).unwrap_or("");
    let clean: String = base.chars().filter(|c| !matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') && !c.is_control()).collect();
    if clean.to_ascii_lowercase().ends_with(".exe") && clean.len() > 4 { clean } else { "Pronto-Setup.exe".into() }
}

/// Solo se descarga de GitHub por HTTPS (con la URL de pruebas también de 127.0.0.1).
pub fn url_allowed(url: &str, testing: bool) -> bool {
    let (https, rest) = match url.strip_prefix("https://") {
        Some(r) => (true, r),
        None => match url.strip_prefix("http://") {
            Some(r) => (false, r),
            None => return false,
        },
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') || authority.contains('\\') {
        return false;
    }
    let host = authority.split(':').next().unwrap_or("").to_ascii_lowercase();
    // Lo que se lee a mano tiene que coincidir con lo que entiende ureq (el parser WHATWG de `url`):
    // mismo esquema y mismo host, sin usuario. Una escritura que el parser reinterpreta se rechaza.
    let parsed = tauri::Url::parse(url).ok().filter(|u| u.username().is_empty() && u.password().is_none());
    if parsed.as_ref().and_then(|u| u.host_str()) != Some(host.as_str()) || parsed.is_some_and(|u| (u.scheme() == "https") != https) {
        return false;
    }
    if !https {
        return testing && (host == "127.0.0.1" || host == "localhost");
    }
    is_github_host(&host)
}

/// GitHub y sus servidores de ficheros de releases (`*.githubusercontent.com`).
pub(crate) fn is_github_host(host: &str) -> bool {
    host == "github.com" || host.ends_with(".github.com") || host.ends_with(".githubusercontent.com")
}

// ------------------------------------------------------------ red: redirecciones a mano y topes
//
// Todas las descargas (actualizador, modelos de voz, instalador de Ollama) usan agentes con
// `redirects(0)` y siguen las redirecciones aquí. ureq 2 entra en pánico con una `Location` sin host
// (`file:`, `data:`…) y, si las sigue él, va hasta 5 saltos a cualquier host, también de https a http
// (rompedor 22-oct-c, descargas MEDIA 1 y 3).

/// Como mucho tantas redirecciones (lo mismo que hacía ureq).
pub const MAX_REDIRECTS: usize = 5;

/// Por qué falló un GET con redirecciones a mano.
#[derive(Debug)]
pub enum GetError {
    /// Código de error del servidor (>= 400, o un 3xx que no es una redirección).
    Status(u16),
    /// Sin conexión, plazo agotado, respuesta ilegible…
    Transport(String),
    /// Redirección rechazada: sin destino, a una URL rota o sin host, a otro esquema, de https a
    /// http, a un host no permitido o demasiadas.
    Redirect(String),
}

/// ¿`host` (tal como lo da `Url::host_str`) es este PC?
pub(crate) fn is_loopback_host(host: &str) -> bool {
    let h = host.trim_start_matches('[').trim_end_matches(']');
    h.eq_ignore_ascii_case("localhost")
        || h.parse::<std::net::IpAddr>().is_ok_and(|ip| match ip {
            std::net::IpAddr::V4(v4) => v4.is_loopback(),
            std::net::IpAddr::V6(v6) => v6.is_loopback() || v6.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback()),
        })
}

/// GET de `url` siguiendo como mucho `MAX_REDIRECTS` redirecciones (301, 302, 303, 307, 308). Cada
/// `Location` se resuelve con el parser de `url` (el de ureq) contra la URL actual y se valida antes de
/// pedirla: http o https con host, nunca de https a http, y `hop_allowed`. `prepare` añade cabeceras o
/// plazo a cada petición. El agente tiene que tener `redirects(0)`; si no, ureq las sigue él.
pub fn get_following(
    agent: &ureq::Agent,
    url: &str,
    prepare: &dyn Fn(ureq::Request) -> ureq::Request,
    hop_allowed: &dyn Fn(&tauri::Url) -> bool,
) -> Result<ureq::Response, GetError> {
    let mut current = tauri::Url::parse(url).map_err(|e| GetError::Transport(format!("dirección no válida ({e})")))?;
    for hop in 0..=MAX_REDIRECTS {
        let resp = match prepare(agent.get(current.as_str())).call() {
            Ok(r) => r,
            Err(ureq::Error::Status(code, _)) => return Err(GetError::Status(code)),
            Err(e) => return Err(GetError::Transport(e.to_string())),
        };
        let code = resp.status();
        if !(300..400).contains(&code) {
            return Ok(resp);
        }
        if !matches!(code, 301 | 302 | 303 | 307 | 308) {
            return Err(GetError::Status(code));
        }
        if hop == MAX_REDIRECTS {
            return Err(GetError::Redirect(format!("más de {MAX_REDIRECTS} redirecciones")));
        }
        current = next_hop(&current, resp.header("Location"), hop_allowed).map_err(GetError::Redirect)?;
    }
    Err(GetError::Redirect(format!("más de {MAX_REDIRECTS} redirecciones")))
}

/// Plazo para recibir las CABECERAS de la respuesta final (toda la cadena de redirecciones). ureq 2 no
/// distingue cabeceras de cuerpo: `timeout_read` cuenta por lectura y un servidor que manda las
/// cabeceras a un byte por segundo no vencía nunca (rompedor 23-oct, descargas MEDIA 1).
pub const HEADERS_DEADLINE: Duration = Duration::from_secs(60);

/// Como `get_following`, con dos plazos: `headers` para tener la respuesta (cabeceras) y `total` para
/// toda la petición, cuerpo incluido (`Request::timeout` de ureq 2 también corta las lecturas del
/// cuerpo). La petición va en un hilo y quien llama espera `headers`: si vence, el hilo se abandona y
/// ureq lo corta solo a los `total`; así «Descargando…» no queda colgado para siempre (regla 4).
pub fn get_following_timed(
    agent: &ureq::Agent,
    url: &str,
    headers: Duration,
    total: Duration,
    hop_allowed: impl Fn(&tauri::Url) -> bool + Send + 'static,
) -> Result<ureq::Response, GetError> {
    let (tx, rx) = std::sync::mpsc::channel();
    let (agent, url) = (agent.clone(), url.to_string());
    let spawned = std::thread::Builder::new().name("pronto-cabeceras".into()).spawn(move || {
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| get_following(&agent, &url, &|r| r.timeout(total), &hop_allowed)));
        let _ = tx.send(r);
    });
    if let Err(e) = spawned {
        return Err(GetError::Transport(format!("no se pudo lanzar la petición ({e})")));
    }
    match rx.recv_timeout(headers) {
        Ok(Ok(r)) => r,
        Ok(Err(_)) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(GetError::Transport("fallo interno en la petición".into())),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err(GetError::Transport(format!("el servidor no respondió en {}", human(headers)))),
    }
}

/// Destino de una redirección desde `current`, o por qué no se sigue.
fn next_hop(current: &tauri::Url, location: Option<&str>, hop_allowed: &dyn Fn(&tauri::Url) -> bool) -> Result<tauri::Url, String> {
    let location = location.map(str::trim).filter(|l| !l.is_empty()).ok_or("redirección sin destino")?;
    let next = current.join(location).map_err(|_| "redirección a una dirección no válida".to_string())?;
    let host = next.host_str().unwrap_or("").to_string();
    if !matches!(next.scheme(), "http" | "https") {
        return Err(format!("redirección a una dirección «{}:», que no es http ni https", next.scheme()));
    }
    if host.is_empty() {
        return Err("redirección a una dirección sin servidor".into());
    }
    if current.scheme() == "https" && next.scheme() != "https" {
        return Err(format!("redirección de https a http ({host})"));
    }
    if !hop_allowed(&next) {
        return Err(format!("redirección a un servidor no permitido ({host})"));
    }
    Ok(next)
}

/// Topes de una descarga: tamaño máximo, plazo total y avance mínimo por ventana de tiempo.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Más bytes que esto = error en cuanto llega el primero de más (nada de llenar el disco).
    pub max_bytes: u64,
    /// Plazo total de la descarga (`None` = sin plazo; cada lectura ya tiene el suyo).
    pub deadline: Option<Duration>,
    /// En cada ventana de este tiempo tienen que llegar al menos `min_bytes` (servidor que gotea).
    pub window: Option<(Duration, u64)>,
}

/// Fallo de `copy_limited`.
#[derive(Debug)]
pub enum CopyError {
    TooBig(u64),
    Deadline(Duration),
    Stalled(Duration, u64),
    Read(std::io::Error),
    Write(std::io::Error),
}

/// Copia `reader` en `out` respetando `limits`. `progress(bytes)` tras cada bloque. Devuelve los bytes.
pub fn copy_limited(reader: &mut dyn Read, out: &mut dyn Write, limits: Limits, progress: &mut dyn FnMut(u64)) -> Result<u64, CopyError> {
    let mut buf = vec![0u8; 256 * 1024];
    let start = Instant::now();
    let (mut got, mut window_start, mut window_bytes) = (0u64, start, 0u64);
    loop {
        // Se pide como mucho un byte más de lo permitido: así se sabe enseguida si sobra.
        let room = limits.max_bytes.saturating_sub(got).saturating_add(1).min(buf.len() as u64) as usize;
        let n = reader.read(&mut buf[..room]).map_err(CopyError::Read)?;
        if n == 0 {
            return Ok(got);
        }
        if got + n as u64 > limits.max_bytes {
            return Err(CopyError::TooBig(limits.max_bytes));
        }
        out.write_all(&buf[..n]).map_err(CopyError::Write)?;
        got += n as u64;
        window_bytes += n as u64;
        progress(got);
        let now = Instant::now();
        if let Some(d) = limits.deadline.filter(|d| now.duration_since(start) > *d) {
            return Err(CopyError::Deadline(d));
        }
        if let Some((w, min)) = limits.window {
            if now.duration_since(window_start) >= w {
                if window_bytes < min {
                    return Err(CopyError::Stalled(w, min));
                }
                (window_start, window_bytes) = (now, 0);
            }
        }
    }
}

/// Markdown de la release -> texto plano corto y seguro (la interfaz lo escapa de todos modos).
pub fn plain_notes(md: &str, max_chars: usize, max_lines: usize) -> String {
    let img = Regex::new(r"!\[([^\]]*)\]\([^)]*\)").unwrap();
    let link = Regex::new(r"\[([^\]]*)\]\([^)]*\)").unwrap();
    let tag = Regex::new(r"<[^>\n]*>").unwrap();
    let bold = Regex::new(r"(\*\*|__)(.+?)(\*\*|__)").unwrap();
    let em = Regex::new(r"\*([^*\n]+)\*").unwrap();
    let title = Regex::new(r"(?i)^pronto\s+v?\d").unwrap();

    let mut out: Vec<String> = Vec::new();
    let mut in_code = false;
    for raw in md.replace("\r\n", "\n").replace('\r', "\n").lines() {
        let line = raw.trim();
        if line.starts_with("```") {
            in_code = !in_code;
            continue;
        }
        let mut text = line.to_string();
        if !in_code {
            if line.is_empty() {
                if out.last().is_some_and(|l| !l.is_empty()) {
                    out.push(String::new());
                }
                continue;
            }
            if line.chars().all(|c| matches!(c, '-' | '*' | '_' | ' ')) && line.len() >= 3 {
                continue; // regla horizontal
            }
            if line.to_ascii_lowercase().contains(".exe") {
                continue; // líneas de «Descarga: Pronto-Setup-x.y.z.exe»
            }
            let heading = line.starts_with('#');
            if heading {
                text = line.trim_start_matches('#').trim().to_string();
                if out.is_empty() && title.is_match(&text) {
                    continue; // el título repite «Pronto X.Y.Z»
                }
            } else if let Some(r) = line.strip_prefix("> ") {
                text = r.trim().to_string();
            } else if let Some(r) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")).or_else(|| line.strip_prefix("+ ")) {
                text = format!("• {}", r.trim());
            }
            text = img.replace_all(&text, "$1").into_owned();
            text = link.replace_all(&text, "$1").into_owned();
            text = tag.replace_all(&text, "").into_owned();
            text = bold.replace_all(&text, "$2").into_owned();
            text = em.replace_all(&text, "$1").into_owned();
            text = text.replace('`', "");
        }
        let text = text.trim().to_string();
        if !text.is_empty() {
            out.push(text);
        }
    }
    while out.last().is_some_and(String::is_empty) {
        out.pop();
    }
    let truncated_lines = out.len() > max_lines;
    out.truncate(max_lines);
    let mut s = out.join("\n");
    let mut cut = truncated_lines;
    if s.chars().count() > max_chars {
        s = s.chars().take(max_chars).collect();
        if let Some(i) = s.rfind(['\n', ' ']) {
            s.truncate(i);
        }
        cut = true;
    }
    let s = s.trim_end().to_string();
    if cut { format!("{s}…") } else { s }
}

// ------------------------------------------------------------ tipos del contrato (camelCase)

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    /// `2.0.2`
    pub version: String,
    /// `v2.0.2`
    pub tag: String,
    pub title: String,
    /// Notas de la release en texto plano, recortadas.
    pub notes: String,
    pub page_url: String,
    pub published_at: Option<String>,
    pub asset_name: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub current_version: String,
    /// `idle` | `upToDate` | `available` | `downloading` | `verifying` | `installing` | `error`
    pub state: &'static str,
    pub checking: bool,
    pub checked_at: Option<String>,
    pub info: Option<UpdateInfo>,
    pub done_bytes: u64,
    pub total_bytes: u64,
    pub percent: u32,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    /// `downloading` | `verifying` | `installing` | `error`
    pub state: &'static str,
    pub done_bytes: u64,
    pub total_bytes: u64,
    pub percent: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn info_of(r: &Release) -> UpdateInfo {
    UpdateInfo {
        version: r.version.to_string(),
        tag: r.tag.clone(),
        title: r.title.clone(),
        notes: r.notes.clone(),
        page_url: r.page_url.clone(),
        published_at: r.published_at.clone(),
        asset_name: r.asset.name.clone(),
        size: r.asset.size,
    }
}

fn percent(done: u64, total: u64) -> u32 {
    if total == 0 { 0 } else { (done.min(total) * 100 / total) as u32 }
}

// ------------------------------------------------------------ estado en memoria

struct Inner {
    state: &'static str,
    release: Option<Release>,
    checked_at: Option<String>,
    done: u64,
    total: u64,
    error: Option<String>,
}

static INNER: Mutex<Inner> = Mutex::new(Inner { state: "idle", release: None, checked_at: None, done: 0, total: 0, error: None });
static CHECKING: AtomicBool = AtomicBool::new(false);
static INSTALLING: AtomicBool = AtomicBool::new(false);

/// Baja la bandera al salir del ámbito, también si el hilo termina por un pánico.
struct Lowers(&'static AtomicBool);

impl Drop for Lowers {
    fn drop(&mut self) {
        self.0.store(false, AtomicOrdering::SeqCst);
    }
}

fn snapshot(g: &Inner) -> UpdateStatus {
    UpdateStatus {
        current_version: current_version().to_string(),
        state: g.state,
        checking: CHECKING.load(AtomicOrdering::SeqCst),
        checked_at: g.checked_at.clone(),
        info: g.release.as_ref().map(info_of),
        done_bytes: g.done,
        total_bytes: g.total,
        percent: percent(g.done, g.total),
        error: g.error.clone(),
    }
}

pub fn status() -> UpdateStatus {
    snapshot(&INNER.lock())
}

/// Versión pendiente de instalar (para el menú de la bandeja), si la hay.
pub fn pending_version() -> Option<String> {
    let g = INNER.lock();
    matches!(g.state, "available" | "error").then(|| g.release.as_ref().map(|r| r.version.to_string())).flatten()
}

fn busy_state(state: &str) -> bool {
    matches!(state, "downloading" | "verifying" | "installing")
}

fn refresh_tray(app: &AppHandle) {
    let a = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(st) = a.try_state::<Arc<AppState>>() {
            let s = st.settings.read().clone();
            tray::rebuild(&a, &s);
        }
    });
}

// ------------------------------------------------------------ consulta a GitHub

#[derive(Debug)]
pub struct CheckError {
    pub msg: String,
    pub rate_limited: bool,
}

fn user_agent() -> String {
    format!("Pronto/{CARGO_VERSION} (actualizador; +https://github.com/AlfonsoGo/pronto)")
}

/// Sin redirecciones automáticas: las sigue `get_following`, que vuelve a aplicar `url_allowed`.
fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(15)).timeout_read(Duration::from_secs(30)).redirects(0).user_agent(&user_agent()).build()
}

/// Cada salto de una redirección del actualizador tiene que pasar `url_allowed` (GitHub por https).
fn updater_hop(testing: bool) -> impl Fn(&tauri::Url) -> bool {
    move |u: &tauri::Url| url_allowed(u.as_str(), testing)
}

fn api_url() -> String {
    std::env::var(ENV_API).ok().filter(|u| !u.trim().is_empty()).unwrap_or_else(|| LATEST_URL.to_string())
}

fn testing() -> bool {
    std::env::var(ENV_API).is_ok_and(|u| !u.trim().is_empty())
}

/// `releases/latest`. `Ok(None)` = no hay release instalable (404, borrador, sin `.exe`...).
pub fn fetch_latest(url: &str) -> Result<Option<Release>, CheckError> {
    let fail = |msg: String, rate_limited: bool| CheckError { msg, rate_limited };
    let prepare = |r: ureq::Request| r.set("Accept", "application/vnd.github+json").set("X-GitHub-Api-Version", "2022-11-28").timeout(Duration::from_secs(20));
    let resp = match get_following(&agent(), url, &prepare, &updater_hop(testing())) {
        Ok(r) => r,
        Err(GetError::Status(404)) => return Ok(None),
        Err(GetError::Status(code @ (403 | 429))) => {
            return Err(fail(format!("GitHub ha limitado las consultas ({code}). Se reintentará más tarde."), true));
        }
        Err(GetError::Status(code)) => return Err(fail(format!("GitHub respondió {code}."), false)),
        Err(GetError::Transport(e)) => return Err(fail(format!("No se pudo conectar con GitHub: {e}"), false)),
        Err(GetError::Redirect(e)) => return Err(fail(format!("GitHub respondió con una redirección que no se sigue: {e}."), false)),
    };
    let mut body = String::new();
    resp.into_reader().take(2 * 1024 * 1024).read_to_string(&mut body).map_err(|e| fail(format!("No se pudo leer la respuesta de GitHub: {e}"), false))?;
    parse_release(&body).map_err(|e| fail(e, false))
}

/// Consulta y actualiza el estado. Si ya hay una consulta o una instalación en marcha devuelve el estado
/// sin hacer nada. Los errores (sin red, límite de la API) los decide quien llama: el hilo de fondo los
/// calla y reintenta; «Buscar ahora» se los muestra al usuario.
pub fn check(app: &AppHandle) -> Result<UpdateStatus, CheckError> {
    if busy_state(INNER.lock().state) || CHECKING.swap(true, AtomicOrdering::SeqCst) {
        return Ok(status());
    }
    let result = {
        // Se libera aunque la consulta entre en pánico: si no, «Buscar ahora» no volvería a consultar.
        let _checking = Lowers(&CHECKING);
        fetch_latest(&api_url())
    };
    let release = result?;
    let current = current_version();
    let newer = release.filter(|r| is_newer(&r.version, &current));
    let (before, announce) = {
        let mut g = INNER.lock();
        let before = g.release.as_ref().map(|r| r.version.to_string());
        g.checked_at = Some(chrono::Utc::now().to_rfc3339());
        if busy_state(g.state) {
            // Se empezó a instalar mientras consultábamos: no pisar nada.
            return Ok(snapshot(&g));
        }
        match &newer {
            Some(r) => {
                g.state = "available";
                g.release = Some(r.clone());
            }
            None => {
                g.state = "upToDate";
                g.release = None;
                g.error = None;
            }
        }
        (before, newer.as_ref().map(info_of))
    };
    if let Some(info) = &announce {
        eprintln!("[updater] hay una versión nueva: {} (actual {current})", info.version);
        let _ = app.emit(EV_AVAILABLE, info);
    }
    if before != announce.as_ref().map(|i| i.version.clone()) {
        refresh_tray(app);
    }
    Ok(status())
}

// ------------------------------------------------------------ descarga y verificación

fn hash_file(path: &Path) -> std::io::Result<String> {
    let mut f = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Tamaño publicado, firma «MZ» de ejecutable y SHA-256 (si la API lo dio).
pub fn verify_file(path: &Path, size: u64, sha256: Option<&str>) -> Result<(), String> {
    let len = fs::metadata(path).map_err(|e| format!("No se pudo leer el instalador descargado: {e}"))?.len();
    if size > 0 && len != size {
        return Err(format!("El instalador descargado está incompleto ({len} de {size} bytes). Inténtalo de nuevo."));
    }
    let mut magic = [0u8; 2];
    File::open(path).and_then(|mut f| f.read_exact(&mut magic)).map_err(|e| format!("No se pudo leer el instalador descargado: {e}"))?;
    if &magic != b"MZ" {
        return Err("El fichero descargado no es un instalador de Windows.".into());
    }
    if let Some(expected) = sha256 {
        let got = hash_file(path).map_err(|e| format!("No se pudo verificar el instalador: {e}"))?;
        if !got.eq_ignore_ascii_case(expected) {
            return Err("El instalador descargado está dañado (el SHA-256 no coincide con el de GitHub). Inténtalo de nuevo.".into());
        }
    }
    Ok(())
}

/// Carpeta de trabajo: `%TEMP%\pronto-update`.
pub fn work_dir() -> PathBuf {
    std::env::temp_dir().join("pronto-update")
}

/// Plazo total de la descarga del instalador (≈ 11 MB): pasado, se aborta y se borra el `.part`.
pub const INSTALLER_DEADLINE: Duration = Duration::from_secs(30 * 60);
/// Avance mínimo: al menos `INSTALLER_MIN_BYTES` cada `INSTALLER_WINDOW` (un servidor que gotea
/// 1 B/s no deja «Descargando…» colgado media hora).
pub const INSTALLER_WINDOW: Duration = Duration::from_secs(30);
pub const INSTALLER_MIN_BYTES: u64 = 64 * 1024;
/// Margen sobre el tamaño publicado por la API antes de cortar (el fichero tiene que medir justo eso).
pub const INSTALLER_SIZE_MARGIN: u64 = 64 * 1024;
/// Tope si la API no da el tamaño (y el servidor no manda `Content-Length`).
pub const INSTALLER_MAX_BYTES: u64 = 512 * 1024 * 1024;

/// `30 minutos`, `400 ms`…
fn human(d: Duration) -> String {
    match d.as_secs() {
        s if s >= 120 => format!("{} minutos", s / 60),
        s if s >= 1 => format!("{s} s"),
        _ => format!("{} ms", d.as_millis()),
    }
}

/// Nombre del fichero local del instalador: propio y corto (el del asset lo pone quien publica la
/// release y puede no caber con `.part`, rompedor 22-oct-c, descargas BAJA 5).
pub fn installer_file_name(version: Option<&Version>) -> String {
    match version {
        Some(v) => format!("Pronto-Setup-{v}.exe"),
        None => "Pronto-Setup.exe".into(),
    }
}

/// Baja el instalador a `dir\Pronto-Setup.exe` (vía `.part`) y lo verifica. `emit(estado, hecho, total)`.
///
/// El agente de quien llama no se usa: las redirecciones (a mano, con `url_allowed` en cada salto) y
/// los topes son parte de la verificación y no pueden depender de cómo se construyó. Se conserva el
/// parámetro para no cambiar la firma pública.
pub fn fetch_installer(_agent: &ureq::Agent, asset: &Asset, dir: &Path, emit: &mut dyn FnMut(&'static str, u64, u64)) -> Result<PathBuf, String> {
    let limits = Limits { max_bytes: 0, deadline: Some(INSTALLER_DEADLINE), window: Some((INSTALLER_WINDOW, INSTALLER_MIN_BYTES)) };
    download_installer(asset, dir, &installer_file_name(None), limits, testing(), emit)
}

/// `fetch_installer` con nombre local, plazos (`limits.max_bytes` se calcula aquí) y modo de pruebas.
fn download_installer(asset: &Asset, dir: &Path, file_name: &str, limits: Limits, testing: bool, emit: &mut dyn FnMut(&'static str, u64, u64)) -> Result<PathBuf, String> {
    // Máximo absoluto, venga lo que venga en el JSON de la release (rompedor 23-oct, descargas BAJA 3).
    if asset.size > INSTALLER_MAX_BYTES {
        return Err(format!("El instalador publicado anuncia {} bytes, más del máximo admitido ({INSTALLER_MAX_BYTES}); no se descarga.", asset.size));
    }
    fs::create_dir_all(dir).map_err(|e| format!("No se pudo crear {}: {e}", dir.display()))?;
    let dest = dir.join(file_name);
    let part = dir.join(format!("{file_name}.part"));
    let _ = fs::remove_file(&dest);
    let result = (|| {
        // Plazo para las cabeceras y plazo total de la petición (cabeceras + cuerpo).
        // Unos segundos más que el de `copy_limited`, para que éste dé su aviso claro cuando llega antes.
        let total_deadline = limits.deadline.unwrap_or(INSTALLER_DEADLINE) + Duration::from_secs(10);
        let resp = get_following_timed(&agent(), &asset.url, HEADERS_DEADLINE, total_deadline, updater_hop(testing)).map_err(|e| match e {
            GetError::Status(code) => format!("GitHub respondió {code} al descargar el instalador."),
            GetError::Transport(e) => format!("No se pudo descargar el instalador: {e}"),
            GetError::Redirect(e) => format!("No se descarga el instalador: {e}."),
        })?;
        let announced: Option<u64> = resp.header("Content-Length").and_then(|v| v.trim().parse().ok());
        // Tope: el tamaño de la API con un margen pequeño; sin él, el `Content-Length` o 512 MB.
        let max_bytes = match (asset.size, announced) {
            (0, Some(n)) => n.min(INSTALLER_MAX_BYTES),
            (0, None) => INSTALLER_MAX_BYTES,
            (size, _) => size.saturating_add(INSTALLER_SIZE_MARGIN).min(INSTALLER_MAX_BYTES),
        };
        if let Some(n) = announced.filter(|n| *n > max_bytes) {
            return Err(format!("El servidor anuncia un instalador de {n} bytes y el publicado mide {}; no se descarga.", asset.size));
        }
        let total = if asset.size > 0 { asset.size } else { announced.unwrap_or(0) };
        let mut reader = resp.into_reader();
        let mut out = File::create(&part).map_err(|e| format!("No se pudo crear el fichero temporal: {e}"))?;
        let mut last = Instant::now() - EMIT_EVERY;
        let got = copy_limited(&mut reader, &mut out, Limits { max_bytes, ..limits }, &mut |got| {
            if last.elapsed() >= EMIT_EVERY {
                last = Instant::now();
                emit("downloading", got, total);
            }
        })
        .map_err(|e| match e {
            CopyError::TooBig(max) => format!("El servidor manda más de lo que mide el instalador publicado (más de {max} bytes); se ha cortado la descarga."),
            CopyError::Deadline(d) => format!("La descarga tarda más de {}; se ha cancelado. Inténtalo más tarde.", human(d)),
            CopyError::Stalled(w, min) => format!("La descarga va demasiado lenta (menos de {} KB en {} s); se ha cancelado. Inténtalo más tarde.", min / 1024, w.as_secs()),
            CopyError::Read(e) => format!("Se cortó la descarga: {e}"),
            CopyError::Write(e) => format!("No se pudo escribir en disco: {e}"),
        })?;
        out.flush().map_err(|e| format!("No se pudo escribir en disco: {e}"))?;
        drop(out);
        emit("downloading", got, total.max(got));
        emit("verifying", got, total.max(got));
        verify_file(&part, asset.size, asset.sha256.as_deref())?;
        fs::rename(&part, &dest).map_err(|e| format!("No se pudo guardar el instalador: {e}"))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&part);
    }
    result.map(|()| dest)
}

// ------------------------------------------------------------ ayudante e instalación

fn powershell_exe() -> PathBuf {
    let p = PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into())).join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    if p.is_file() { p } else { PathBuf::from("powershell.exe") }
}

/// Lanza el ayudante (PowerShell oculto, sin ventana) y espera a que confirme que está vivo
/// (`helper-started`). Si no lo hace, devuelve error y Pronto NO se cierra.
pub fn spawn_helper(pid: u32, installer: &Path, exe: &Path, dir: &Path) -> Result<(), String> {
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    fs::create_dir_all(dir).map_err(|e| format!("No se pudo crear {}: {e}", dir.display()))?;
    let script = dir.join("pronto_update.ps1");
    // UTF-8 con BOM: así Windows PowerShell 5.1 lo lee bien.
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(HELPER_SCRIPT.as_bytes());
    fs::write(&script, bytes).map_err(|e| format!("No se pudo escribir el ayudante de actualización: {e}"))?;
    let alive = dir.join("helper-started");
    let _ = fs::remove_file(&alive);
    let _ = fs::remove_file(dir.join("result.txt"));

    let mut child = Command::new(powershell_exe())
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden", "-File"])
        .arg(&script)
        .arg("-ProntoPid")
        .arg(pid.to_string())
        .arg("-Installer")
        .arg(installer)
        .arg("-Exe")
        .arg(exe)
        .arg("-WorkDir")
        .arg(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP)
        .spawn()
        .map_err(|e| format!("No se pudo lanzar el ayudante de actualización: {e}"))?;
    let start = Instant::now();
    while start.elapsed() < HELPER_ALIVE_TIMEOUT {
        if alive.is_file() {
            return Ok(());
        }
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!("El ayudante de actualización terminó sin arrancar ({status})."));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    Err("El ayudante de actualización no llegó a arrancar.".into())
}

/// ¿Es un ejecutable compilado con cargo? Por la ruta (`target\debug|release`) o, con otro
/// `target-dir` (el de src-tauri/.cargo/config.toml), por lo que cargo deja a su lado.
fn is_dev_exe(exe: &Path) -> bool {
    let s = exe.to_string_lossy().to_ascii_lowercase().replace('/', "\\");
    let cargo_dir = exe.parent().is_some_and(|d| d.join("deps").is_dir() && d.join(".fingerprint").is_dir());
    s.contains("\\target\\debug\\") || s.contains("\\target\\release\\") || cargo_dir
}

/// ¿Esta copia no se puede autoinstalar? Una compilación de depuración nunca; una de `target`
/// solo con `PRONTO_UPDATE_ALLOW_DEV` (pruebas).
fn install_blocked(exe: &Path, debug: bool, allow_dev: bool) -> bool {
    debug || (is_dev_exe(exe) && !allow_dev)
}

/// Por qué no se puede cerrar Pronto ahora para instalar (reunión grabándose o dictado en curso).
fn busy_reason(st: &AppState) -> Option<&'static str> {
    if st.meeting.is_active() {
        return Some("Hay una reunión grabándose. Termínala y actualiza después.");
    }
    // Cerrar Pronto con la fase 2 en marcha deja esas notas `failed` (rompedor 22-oct-c).
    if st.meeting.any_processing() {
        return Some("Se están preparando las notas de una reunión. Espera a que terminen y actualiza después.");
    }
    if st.active.load(AtomicOrdering::SeqCst) {
        return Some("Estás dictando ahora mismo. Espera a que termine y vuelve a intentarlo.");
    }
    None
}

/// Lanza el ayudante (que cierra Pronto) solo si durante la descarga no se ha empezado a dictar ni
/// a grabar una reunión: se vuelve a mirar justo antes de salir.
fn launch_when_idle(busy: impl Fn() -> Option<&'static str>, launch: impl FnOnce() -> Result<(), String>) -> Result<(), String> {
    match busy() {
        Some(why) => Err(why.into()),
        None => launch(),
    }
}

fn set_error(app: &AppHandle, msg: String) {
    let p = {
        let mut g = INNER.lock();
        if g.release.is_some() {
            g.state = "error";
        }
        g.error = Some(msg.clone());
        UpdateProgress { state: "error", done_bytes: g.done, total_bytes: g.total, percent: percent(g.done, g.total), error: Some(msg) }
    };
    let _ = app.emit(EV_PROGRESS, p);
}

fn publish(app: &AppHandle, state: &'static str, done: u64, total: u64) {
    {
        let mut g = INNER.lock();
        g.state = state;
        g.done = done;
        g.total = total;
        g.error = None;
    }
    let _ = app.emit(EV_PROGRESS, UpdateProgress { state, done_bytes: done, total_bytes: total, percent: percent(done, total), error: None });
}

/// Descarga la versión pendiente, la verifica y cierra Pronto dejando al ayudante instalarla y reabrirla.
/// Vuelve enseguida; el avance llega por `update://progress`. Los errores también salen por ese evento.
pub fn install(app: &AppHandle, st: &Arc<AppState>) -> Result<(), String> {
    let preflight = || -> Result<(Release, PathBuf), String> {
        let exe = std::env::current_exe().map_err(|e| format!("No se pudo localizar Pronto: {e}"))?;
        if install_blocked(&exe, cfg!(debug_assertions), std::env::var(ENV_ALLOW_DEV).is_ok()) {
            return Err("Esta copia de Pronto no está instalada con el instalador (es una compilación de desarrollo): no se actualiza sola.".into());
        }
        if let Some(why) = busy_reason(st) {
            return Err(why.into());
        }
        let release = INNER.lock().release.clone().ok_or("No hay ninguna actualización disponible. Pulsa «Buscar ahora».")?;
        if !url_allowed(&release.asset.url, testing()) {
            return Err("La dirección de descarga de la actualización no es de GitHub; no se descarga.".into());
        }
        Ok((release, exe))
    };
    let (release, exe) = match preflight() {
        Ok(x) => x,
        Err(e) => {
            set_error(app, e.clone());
            return Err(e);
        }
    };
    if INSTALLING.swap(true, AtomicOrdering::SeqCst) {
        return Ok(()); // ya hay una en marcha
    }
    // Si el hilo no llega a arrancar o termina por un pánico, `INSTALLING` se libera igual.
    let installing = Lowers(&INSTALLING);
    let (app2, st2, version) = (app.clone(), st.clone(), release.version.to_string());
    let spawned = std::thread::Builder::new().name("pronto-update".into()).spawn(move || {
        let installing = installing;
        eprintln!("[updater] descargando {version}");
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            publish(&app2, "downloading", 0, release.asset.size);
            let dir = work_dir();
            let limits = Limits { max_bytes: 0, deadline: Some(INSTALLER_DEADLINE), window: Some((INSTALLER_WINDOW, INSTALLER_MIN_BYTES)) };
            let name = installer_file_name(Some(&release.version));
            download_installer(&release.asset, &dir, &name, limits, testing(), &mut |state, done, total| publish(&app2, state, done, total)).and_then(|installer| {
                launch_when_idle(
                    || busy_reason(&st2),
                    || {
                        publish(&app2, "installing", release.asset.size, release.asset.size);
                        spawn_helper(std::process::id(), &installer, &exe, &dir)
                    },
                )
            })
        }))
        .unwrap_or_else(|_| Err("Fallo interno al descargar la actualización. Inténtalo de nuevo.".into()));
        match outcome {
            Ok(()) => {
                eprintln!("[updater] ayudante en marcha; cerrando Pronto");
                // Pronto se cierra: no se libera `INSTALLING` para no lanzar un segundo ayudante.
                std::mem::forget(installing);
                // Un instante para que la interfaz pinte «Instalando…» y salimos limpios.
                std::thread::sleep(Duration::from_millis(600));
                app2.exit(0);
            }
            Err(e) => {
                eprintln!("[updater] {e}");
                drop(installing);
                set_error(&app2, e);
            }
        }
    });
    if let Err(e) = spawned {
        let msg = format!("No se pudo lanzar la actualización: {e}");
        set_error(app, msg.clone());
        return Err(msg);
    }
    Ok(())
}

// ------------------------------------------------------------ hilo de fondo

/// Lee y borra el resultado que dejó el ayudante en la actualización anterior. `Some(error)` si falló.
fn take_last_install_error(dir: &Path) -> Option<String> {
    let file = dir.join("result.txt");
    let text = fs::read_to_string(&file).ok()?;
    let _ = fs::remove_file(&file);
    let text = text.trim();
    let code = text.strip_prefix("error")?.trim();
    Some(format!(
        "La última actualización no se pudo instalar (código {}). Sigues con tu versión de siempre; puedes reintentarlo o instalar a mano desde github.com/AlfonsoGo/pronto/releases.",
        if code.is_empty() { "?" } else { code }
    ))
}

/// Borra instaladores y restos de actualizaciones anteriores (el log y el resultado se conservan).
fn clean_work_dir(dir: &Path) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_ascii_lowercase();
        if name.ends_with(".exe") || name.ends_with(".part") || name == "helper-started" {
            let _ = fs::remove_file(e.path());
        }
    }
}

/// Arranca el hilo que comprueba al abrir (tras unos segundos) y cada 4 horas.
pub fn spawn_background(app: AppHandle, st: Arc<AppState>) {
    let dir = work_dir();
    if let Some(msg) = take_last_install_error(&dir) {
        eprintln!("[updater] {msg}");
        INNER.lock().error = Some(msg);
    }
    clean_work_dir(&dir);
    let _ = std::thread::Builder::new().name("pronto-updater".into()).spawn(move || {
        std::thread::sleep(FIRST_CHECK_DELAY);
        let mut next_due = Instant::now();
        loop {
            let auto = st.settings.read().auto_update;
            if auto && Instant::now() >= next_due && !INSTALLING.load(AtomicOrdering::SeqCst) {
                // Un pánico en la consulta no mata este hilo: se trata como un fallo y se reintenta.
                let checked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| check(&app)))
                    .unwrap_or_else(|_| Err(CheckError { msg: "fallo interno al buscar actualizaciones".into(), rate_limited: false }));
                next_due = Instant::now()
                    + match checked {
                        Ok(_) => CHECK_EVERY,
                        Err(e) => {
                            eprintln!("[updater] {}", e.msg);
                            if e.rate_limited { RETRY_AFTER_RATE_LIMIT } else { RETRY_AFTER_FAILURE }
                        }
                    };
            }
            std::thread::sleep(TICK);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;
    use std::sync::atomic::AtomicUsize;

    fn v(s: &str) -> Version {
        parse_version(s).unwrap_or_else(|| panic!("no es versión: {s}"))
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pronto-updater-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// Release real de v2.0.1 (recortada).
    const REAL_RELEASE: &str = r###"{
      "html_url": "https://github.com/AlfonsoGo/pronto/releases/tag/v2.0.1",
      "tag_name": "v2.0.1", "name": "Pronto 2.0.1", "draft": false, "prerelease": false,
      "published_at": "2026-10-11T17:31:44Z",
      "assets": [{
        "name": "Pronto-Setup-2.0.1.exe", "content_type": "application/x-msdownload", "state": "uploaded", "size": 10800872,
        "digest": "sha256:d30bad128dcb791f68c93f84db65ca5981557c89d45feb9ad7ca6e82de4800aa",
        "browser_download_url": "https://github.com/AlfonsoGo/pronto/releases/download/v2.0.1/Pronto-Setup-2.0.1.exe"
      }],
      "body": "## Pronto 2.0.1\n\n**Descarga:** [Pronto-Setup-2.0.1.exe](https://github.com/AlfonsoGo/pronto/releases/download/v2.0.1/Pronto-Setup-2.0.1.exe) (Windows 10/11, 64 bits)\n\n### Arreglos\n- **Estilo:** al pulsar Formal el cambio se ve al momento.\n- **Instalador:** ya no aparecen dos «Pronto».\n\n### Cómo actualizar\n- **Desde Pronto 1:** te saldrá el aviso."
    }"###;

    // ---------------------------------------------------------- versiones

    #[test]
    fn versiones_se_leen_con_o_sin_v_y_con_partes_que_faltan() {
        assert_eq!(v("v2.0.2").to_string(), "2.0.2");
        assert_eq!(v("2.0.2").to_string(), "2.0.2");
        assert_eq!(v("V2.1").to_string(), "2.1.0");
        assert_eq!(v("3").to_string(), "3.0.0");
        assert_eq!(v(" v2.0.2-rc.1+build.5 ").to_string(), "2.0.2-rc.1");
        for bad in ["", "v", "abc", "1.2.3.4", "1..3", "1.2.x", "2.0.2-", "-1", "1.2.3-a..b"] {
            assert!(parse_version(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn comparacion_semver() {
        assert!(is_newer(&v("2.0.2"), &v("2.0.1")));
        assert!(is_newer(&v("2.1.0"), &v("2.0.9")));
        assert!(is_newer(&v("3.0.0"), &v("2.9.9")));
        assert!(is_newer(&v("2.0.10"), &v("2.0.9")), "numérica, no de texto");
        assert!(!is_newer(&v("2.0.1"), &v("2.0.1")));
        assert!(!is_newer(&v("2.0.0"), &v("2.0.1")));
        assert!(!is_newer(&v("v2.0.1+otro"), &v("2.0.1")), "los metadatos de build no cuentan");
        // prereleases: la estable gana a su rc; el rc gana a la versión anterior
        assert!(is_newer(&v("2.0.2"), &v("2.0.2-rc.1")));
        assert!(!is_newer(&v("2.0.2-rc.1"), &v("2.0.2")));
        assert!(is_newer(&v("2.0.2-rc.1"), &v("2.0.1")));
        assert!(is_newer(&v("2.0.2-rc.2"), &v("2.0.2-rc.1")));
        assert!(is_newer(&v("2.0.2-rc.10"), &v("2.0.2-rc.9")));
        assert!(is_newer(&v("2.0.2-rc"), &v("2.0.2-beta")));
        assert!(is_newer(&v("2.0.2-rc.1"), &v("2.0.2-rc")));
    }

    #[test]
    fn version_actual_sale_de_cargo() {
        assert_eq!(parse_version(CARGO_VERSION).unwrap().to_string(), CARGO_VERSION);
    }

    // ---------------------------------------------------------- release

    #[test]
    fn release_real_se_interpreta() {
        let r = parse_release(REAL_RELEASE).unwrap().expect("instalable");
        assert_eq!(r.version.to_string(), "2.0.1");
        assert_eq!(r.tag, "v2.0.1");
        assert_eq!(r.title, "Pronto 2.0.1");
        assert_eq!(r.asset.name, "Pronto-Setup-2.0.1.exe");
        assert_eq!(r.asset.size, 10_800_872);
        assert_eq!(r.asset.sha256.as_deref(), Some("d30bad128dcb791f68c93f84db65ca5981557c89d45feb9ad7ca6e82de4800aa"));
        assert!(r.asset.url.ends_with("/v2.0.1/Pronto-Setup-2.0.1.exe"));
        assert_eq!(r.page_url, "https://github.com/AlfonsoGo/pronto/releases/tag/v2.0.1");
        assert!(url_allowed(&r.asset.url, false));
        assert!(!is_newer(&r.version, &parse_version("2.0.1").unwrap()));
        assert!(is_newer(&r.version, &parse_version("2.0.0").unwrap()));
    }

    #[test]
    fn toma_el_primer_exe_aunque_haya_otros_assets() {
        let json = r#"{"tag_name":"v2.1.0","assets":[
          {"name":"Pronto-2.1.0.zip","size":1,"browser_download_url":"https://github.com/x/a.zip"},
          {"name":"Pronto-Setup-2.1.0.EXE","size":5,"browser_download_url":"https://github.com/x/primero.exe"},
          {"name":"Otro.exe","size":6,"browser_download_url":"https://github.com/x/segundo.exe","digest":"md5:abc"}]}"#;
        let r = parse_release(json).unwrap().unwrap();
        assert_eq!(r.asset.name, "Pronto-Setup-2.1.0.EXE");
        assert_eq!(r.asset.url, "https://github.com/x/primero.exe");
        assert_eq!(r.asset.sha256, None, "sin digest");
        assert_eq!(r.title, "Pronto 2.1.0", "sin nombre usa «Pronto X.Y.Z»");
    }

    #[test]
    fn borradores_prereleases_y_sin_exe_se_ignoran() {
        let mk = |extra: &str, tag: &str, asset: &str| {
            format!(r#"{{"tag_name":"{tag}",{extra}"assets":[{{"name":"{asset}","size":5,"browser_download_url":"https://github.com/x/{asset}"}}]}}"#)
        };
        assert!(parse_release(&mk(r#""draft":true,"#, "v9.0.0", "a.exe")).unwrap().is_none());
        assert!(parse_release(&mk(r#""prerelease":true,"#, "v9.0.0", "a.exe")).unwrap().is_none());
        assert!(parse_release(&mk("", "v9.0.0-rc.1", "a.exe")).unwrap().is_none(), "tag con prerelease");
        assert!(parse_release(&mk("", "v9.0.0", "a.zip")).unwrap().is_none(), "sin .exe");
        assert!(parse_release(r#"{"tag_name":"v9.0.0"}"#).unwrap().is_none(), "sin assets");
        assert!(parse_release(&mk("", "v9.0.0", "a.exe")).unwrap().is_some());
    }

    #[test]
    fn json_roto_o_tag_raro_dan_error() {
        assert!(parse_release("no es json").is_err());
        assert!(parse_release("{}").is_err());
        assert!(parse_release(r#"{"tag_name":"nightly","assets":[]}"#).is_err());
    }

    #[test]
    fn nombre_local_del_instalador_es_propio_y_corto() {
        assert_eq!(installer_file_name(Some(&v("2.3.0"))), "Pronto-Setup-2.3.0.exe");
        assert_eq!(installer_file_name(None), "Pronto-Setup.exe");
        let max = installer_file_name(Some(&v("18446744073709551615.18446744073709551615.18446744073709551615")));
        assert!(max.len() + ".part".len() < 100, "{max}");
    }

    #[test]
    fn nombre_de_asset_peligroso_se_sanea() {
        assert_eq!(safe_file_name("Pronto-Setup-2.0.2.exe"), "Pronto-Setup-2.0.2.exe");
        assert_eq!(safe_file_name("..\\..\\Windows\\evil.exe"), "evil.exe");
        assert_eq!(safe_file_name("a/b/c.exe"), "c.exe");
        assert_eq!(safe_file_name("raro.txt"), "Pronto-Setup.exe");
        assert_eq!(safe_file_name(".exe"), "Pronto-Setup.exe");
        assert_eq!(safe_file_name("a:b*.exe"), "b.exe", "«a:» es un prefijo de unidad para Windows");
        assert_eq!(safe_file_name("x*y?.exe"), "xy.exe");
    }

    #[test]
    fn solo_se_descarga_de_github() {
        assert!(url_allowed("https://github.com/AlfonsoGo/pronto/releases/download/v2/x.exe", false));
        assert!(url_allowed("https://objects.githubusercontent.com/x", false));
        assert!(url_allowed("https://api.github.com/repos/x", false));
        assert!(!url_allowed("http://github.com/x.exe", false), "sin HTTPS");
        assert!(!url_allowed("https://evil.com/github.com/x.exe", false));
        assert!(!url_allowed("https://github.com.evil.com/x.exe", false));
        assert!(!url_allowed("https://github.com@evil.com/x.exe", false));
        assert!(!url_allowed("https://notgithub.com/x.exe", false));
        assert!(!url_allowed("ftp://github.com/x.exe", false));
        assert!(!url_allowed("http://127.0.0.1:80/x.exe", false), "local solo en pruebas");
        assert!(url_allowed("http://127.0.0.1:8080/x.exe", true));
        assert!(!url_allowed("http://evil.com/x.exe", true));
        // Escrituras que el parser de ureq reinterpreta: se rechazan.
        assert!(!url_allowed("http://127.1:8080/x.exe", true), "127.1 es 127.0.0.1 escrito de otra forma");
        assert!(!url_allowed("https://git%68ub.com/x.exe", false));
        assert!(!url_allowed("https://github.com\\@evil.com/x.exe", false));
        assert!(!url_allowed("https://user@github.com/x.exe", false));
    }

    // ---------------------------------------------------------- notas

    #[test]
    fn notas_en_texto_plano_y_recortadas() {
        let r = parse_release(REAL_RELEASE).unwrap().unwrap();
        assert_eq!(
            r.notes,
            "Arreglos\n• Estilo: al pulsar Formal el cambio se ve al momento.\n• Instalador: ya no aparecen dos «Pronto».\n\nCómo actualizar\n• Desde Pronto 1: te saldrá el aviso."
        );
        let md = "# Pronto 3.0\n\nTexto con [enlace](http://x.y), `código`, *énfasis* y __negrita__ <b>html</b> ![img](a.png).\n\n```\nlinea de codigo\n```\n---\n> cita\n1. uno\n2. dos";
        assert_eq!(plain_notes(md, 600, 12), "Texto con enlace, código, énfasis y negrita html img.\n\nlinea de codigo\ncita\n1. uno\n2. dos");
        // lo peligroso queda como texto inofensivo (la interfaz lo escapa además)
        assert_eq!(plain_notes("<script>alert(1)</script>hola", 600, 12), "alert(1)hola");
        // recorte por líneas y por caracteres, siempre con «…»
        let many = (1..=20).map(|i| format!("- punto {i}")).collect::<Vec<_>>().join("\n");
        let cut = plain_notes(&many, 600, 5);
        assert_eq!(cut.lines().count(), 5);
        assert!(cut.ends_with("punto 5…"), "{cut}");
        let long = "palabra ".repeat(200);
        let cut = plain_notes(&long, 100, 12);
        assert!(cut.chars().count() <= 101 && cut.ends_with('…'), "{cut}");
        assert_eq!(plain_notes("", 600, 12), "");
        assert_eq!(plain_notes("  \n\n  ", 600, 12), "");
    }

    // ---------------------------------------------------------- digest y verificación

    const ABC_SHA: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn digest_de_github() {
        let ok = format!("sha256:{ABC_SHA}");
        assert_eq!(parse_digest(Some(&ok)).as_deref(), Some(ABC_SHA));
        assert_eq!(parse_digest(Some(&ok.to_uppercase().replace("SHA256", "sha256"))).as_deref(), Some(ABC_SHA));
        assert_eq!(parse_digest(None), None);
        assert_eq!(parse_digest(Some("md5:abc")), None);
        assert_eq!(parse_digest(Some("sha256:abc")), None, "longitud incorrecta");
        assert_eq!(parse_digest(Some(&format!("sha256:{}", "z".repeat(64)))), None, "no es hex");
        assert_eq!(parse_digest(Some(ABC_SHA)), None, "sin prefijo");
    }

    fn write_exe(dir: &Path, name: &str, body: &[u8]) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn verificacion_de_tamano_firma_y_sha256() {
        let dir = tmp("verify");
        // "MZabc" no es "abc"; usamos un contenido propio y calculamos su hash
        let body = b"MZ-instalador-de-prueba";
        let f = write_exe(&dir, "ok.exe", body);
        let sha = hash_file(&f).unwrap();
        assert_eq!(sha.len(), 64);
        verify_file(&f, body.len() as u64, Some(&sha)).unwrap();
        verify_file(&f, body.len() as u64, Some(&sha.to_uppercase())).unwrap();
        verify_file(&f, body.len() as u64, None).unwrap();
        verify_file(&f, 0, None).unwrap();

        let e = verify_file(&f, body.len() as u64 + 1, Some(&sha)).unwrap_err();
        assert!(e.contains("incompleto"), "{e}");
        let e = verify_file(&f, body.len() as u64, Some(&"0".repeat(64))).unwrap_err();
        assert!(e.contains("SHA-256"), "{e}");
        let txt = write_exe(&dir, "no.exe", b"hola mundo");
        let e = verify_file(&txt, 10, None).unwrap_err();
        assert!(e.contains("no es un instalador"), "{e}");
        assert!(verify_file(&dir.join("no-existe.exe"), 1, None).is_err());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn sha256_de_vector_conocido() {
        let dir = tmp("sha");
        let f = write_exe(&dir, "abc.bin", b"abc");
        assert_eq!(hash_file(&f).unwrap(), ABC_SHA);
        let _ = fs::remove_dir_all(dir);
    }

    // ---------------------------------------------------------- descarga contra un servidor local

    /// Sirve `routes` (ruta -> cuerpo) por HTTP en 127.0.0.1 y cuenta las peticiones.
    fn serve(routes: Vec<(&'static str, Vec<u8>)>) -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = hits.clone();
        std::thread::spawn(move || {
            for conn in listener.incoming().flatten() {
                counter.fetch_add(1, AtomicOrdering::SeqCst);
                let mut reader = BufReader::new(conn.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let mut ua = String::new();
                loop {
                    let mut h = String::new();
                    reader.read_line(&mut h).unwrap();
                    if h.trim().is_empty() {
                        break;
                    }
                    if h.to_ascii_lowercase().starts_with("user-agent:") {
                        ua = h.trim().to_string();
                    }
                }
                let path = line.split_whitespace().nth(1).unwrap_or("/");
                let mut conn = conn;
                match routes.iter().find(|(p, _)| *p == path) {
                    Some((_, body)) => {
                        // El User-Agent propio llega al servidor (GitHub lo exige).
                        assert!(ua.to_ascii_lowercase().contains("pronto/"), "User-Agent: {ua:?}");
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

    fn asset_for(url: &str, body: &[u8], with_digest: bool) -> Asset {
        let sha = {
            let mut h = Sha256::new();
            h.update(body);
            h.finalize().iter().map(|b| format!("{b:02x}")).collect::<String>()
        };
        Asset { name: "Pronto-Setup-9.9.9.exe".into(), url: format!("{url}/setup.exe"), size: body.len() as u64, sha256: with_digest.then_some(sha) }
    }

    #[test]
    fn descarga_verifica_y_deja_el_instalador() {
        let dir = tmp("dl-ok");
        let mut body = b"MZ".to_vec();
        body.extend((0..900_000u32).map(|i| (i % 251) as u8));
        let (url, hits) = serve(vec![("/setup.exe", body.clone())]);
        let asset = asset_for(&url, &body, true);
        let mut events: Vec<(&str, u64, u64)> = Vec::new();
        let path = fetch_installer(&agent(), &asset, &dir, &mut |s, d, t| events.push((s, d, t))).unwrap();
        assert_eq!(path, dir.join("Pronto-Setup.exe"), "nombre local propio, no el del asset");
        assert_eq!(fs::read(&path).unwrap(), body);
        assert!(!dir.join("Pronto-Setup.exe.part").exists());
        assert_eq!(hits.load(AtomicOrdering::SeqCst), 1);
        assert!(events.iter().any(|e| e.0 == "downloading"));
        assert_eq!(events.last().unwrap(), &("verifying", body.len() as u64, body.len() as u64));
        assert!(events.windows(2).all(|w| w[0].1 <= w[1].1), "el avance no retrocede");
        // sin digest: solo tamaño y firma
        let asset = asset_for(&url, &body, false);
        fetch_installer(&agent(), &asset, &dir, &mut |_, _, _| {}).unwrap();
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn descarga_con_hash_o_tamano_malos_no_deja_nada() {
        let dir = tmp("dl-bad");
        let body = b"MZ-contenido-de-prueba".to_vec();
        let (url, _) = serve(vec![("/setup.exe", body.clone())]);

        let mut asset = asset_for(&url, &body, true);
        asset.sha256 = Some("0".repeat(64));
        let e = fetch_installer(&agent(), &asset, &dir, &mut |_, _, _| {}).unwrap_err();
        assert!(e.contains("SHA-256"), "{e}");

        let mut asset = asset_for(&url, &body, true);
        asset.size += 10;
        let e = fetch_installer(&agent(), &asset, &dir, &mut |_, _, _| {}).unwrap_err();
        assert!(e.contains("incompleto") || e.contains("cortó"), "{e}");

        let mut asset = asset_for(&url, &body, false);
        asset.url = format!("{url}/no-existe.exe");
        let e = fetch_installer(&agent(), &asset, &dir, &mut |_, _, _| {}).unwrap_err();
        assert!(e.contains("404"), "{e}");

        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0, "ni .part ni .exe");
        let _ = fs::remove_dir_all(dir);
    }

    /// Servidor que contesta la respuesta HTTP cruda de `respond(ruta)` y cuenta las peticiones.
    fn serve_raw(respond: impl Fn(&str, &mut std::net::TcpStream) + Send + Sync + 'static) -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = hits.clone();
        let respond = Arc::new(respond);
        std::thread::spawn(move || {
            for conn in listener.incoming().flatten() {
                counter.fetch_add(1, AtomicOrdering::SeqCst);
                let respond = respond.clone();
                std::thread::spawn(move || {
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
                    respond(&path, &mut conn);
                });
            }
        });
        (url, hits)
    }

    fn redirect(to: &str) -> Vec<u8> {
        format!("HTTP/1.1 302 Found\r\nLocation: {to}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").into_bytes()
    }

    const NO_LIMITS: Limits = Limits { max_bytes: 0, deadline: None, window: None };

    /// Rompedor 22-oct-c, descargas MEDIA 1 y 3: las redirecciones se siguen a mano (como mucho 5),
    /// con `url_allowed` en cada salto, y una `Location` sin host es un error, no un pánico.
    #[test]
    fn redirecciones_a_mano_validadas_en_cada_salto() {
        let dir = tmp("redir");
        let body = b"MZ-instalador-redirigido".to_vec();
        let b2 = body.clone();
        let (url, hits) = serve_raw(move |path, conn| {
            let r = match path {
                "/a" => redirect("/b"),
                "/b" => redirect("setup.exe"),
                "/setup.exe" => {
                    let mut r = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", b2.len()).into_bytes();
                    r.extend_from_slice(&b2);
                    r
                }
                "/bucle" => redirect("/bucle"),
                "/fuera" => redirect("https://127.0.0.1:1/x.exe"),
                "/otro" => redirect("http://evil.example/x.exe"),
                "/file" => redirect("file:///C:/Windows/win.ini"),
                "/data" => redirect("data:,MZ"),
                "/js" => redirect("javascript:alert(1)"),
                "/vacio" => redirect("http://"),
                "/sin" => b"HTTP/1.1 302 Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
                "/300" => b"HTTP/1.1 300 Multiple Choices\r\nLocation: /setup.exe\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
                _ => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
            };
            let _ = conn.write_all(&r);
        });
        let mut asset = asset_for(&url, &body, true);
        asset.url = format!("{url}/a");
        let p = download_installer(&asset, &dir, "P.exe", NO_LIMITS, true, &mut |_, _, _| {}).unwrap();
        assert_eq!(fs::read(&p).unwrap(), body);
        assert_eq!(hits.load(AtomicOrdering::SeqCst), 3, "dos redirecciones relativas y el fichero");
        // Fuera del modo de pruebas http://127.0.0.1 no pasa `url_allowed`: el primer salto se rechaza.
        let e = download_installer(&asset, &dir, "P.exe", NO_LIMITS, false, &mut |_, _, _| {}).unwrap_err();
        assert!(e.contains("no permitido"), "{e}");
        for (ruta, esperado) in [
            ("/bucle", "más de 5"),
            ("/fuera", "no permitido"),
            ("/otro", "no permitido (evil.example)"),
            ("/file", "no es http"),
            ("/data", "no es http"),
            ("/js", "no es http"),
            ("/vacio", "no válida"),
            ("/sin", "sin destino"),
            ("/300", "300"),
        ] {
            asset.url = format!("{url}{ruta}");
            let before = hits.load(AtomicOrdering::SeqCst);
            let e = download_installer(&asset, &dir, "P.exe", NO_LIMITS, true, &mut |_, _, _| {}).unwrap_err();
            assert!(e.contains(esperado), "{ruta}: {e}");
            let n = hits.load(AtomicOrdering::SeqCst) - before;
            assert!(n <= MAX_REDIRECTS + 1, "{ruta}: {n} peticiones");
        }
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0, "ni .part ni .exe");
        // La consulta de la release, igual: una redirección sin host es un error, no un pánico.
        let e = fetch_latest(&format!("{url}/file")).unwrap_err();
        assert!(e.msg.contains("redirección"), "{}", e.msg);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn salto_de_https_a_http_o_a_otro_host_se_rechaza() {
        let gh = tauri::Url::parse("https://github.com/AlfonsoGo/pronto/releases/download/v2/x.exe").unwrap();
        let hop = updater_hop(false);
        let ok = next_hop(&gh, Some("https://objects.githubusercontent.com/x?sig=1"), &hop).unwrap();
        assert_eq!(ok.host_str(), Some("objects.githubusercontent.com"));
        assert!(next_hop(&gh, Some("http://objects.githubusercontent.com/x"), &hop).unwrap_err().contains("https a http"));
        assert!(next_hop(&gh, Some("https://evil.example/x.exe"), &hop).unwrap_err().contains("no permitido"));
        assert!(next_hop(&gh, Some("https://github.com.evil.example/x.exe"), &hop).unwrap_err().contains("no permitido"));
        assert!(next_hop(&gh, Some("//evil.example/x.exe"), &hop).unwrap_err().contains("no permitido"), "relativa al esquema");
        assert!(next_hop(&gh, Some("\\\\evil.example\\x.exe"), &hop).unwrap_err().contains("no permitido"), "barras invertidas = //");
        assert!(next_hop(&gh, Some("file:///C:/x.exe"), &hop).unwrap_err().contains("no es http"));
        assert!(next_hop(&gh, Some("  "), &hop).unwrap_err().contains("sin destino"));
        assert!(next_hop(&gh, None, &hop).unwrap_err().contains("sin destino"));
        assert_eq!(next_hop(&gh, Some("/AlfonsoGo/pronto/y.exe"), &hop).unwrap().as_str(), "https://github.com/AlfonsoGo/pronto/y.exe");
    }

    /// PRNT-41: tope de tamaño (el de la API + un margen) en cuanto se pasa, y aviso adelantado si el
    /// servidor anuncia más.
    #[test]
    fn descarga_corta_en_cuanto_pasa_del_tamano_publicado() {
        let dir = tmp("tope");
        let (url, _) = serve_raw(|path, conn| {
            if path == "/anuncia" {
                let _ = conn.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10000000\r\nConnection: close\r\n\r\nMZ");
                return;
            }
            let _ = conn.write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n");
            let mut block = vec![0u8; 64 * 1024];
            block[..2].copy_from_slice(b"MZ");
            for _ in 0..160 {
                if conn.write_all(&block).is_err() {
                    return;
                }
            }
        });
        let asset = Asset { name: "P.exe".into(), url: format!("{url}/chorro"), size: 1000, sha256: None };
        let mut max_done = 0u64;
        let e = download_installer(&asset, &dir, "P.exe", NO_LIMITS, true, &mut |_, d, _| max_done = max_done.max(d)).unwrap_err();
        assert!(e.contains("más de"), "{e}");
        assert!(max_done <= 1000 + INSTALLER_SIZE_MARGIN, "{max_done}");
        let asset = Asset { name: "P.exe".into(), url: format!("{url}/anuncia"), size: 1000, sha256: None };
        let e = download_installer(&asset, &dir, "P.exe", NO_LIMITS, true, &mut |_, _, _| {}).unwrap_err();
        assert!(e.contains("anuncia"), "{e}");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0, "sin .part");
        let _ = fs::remove_dir_all(dir);
    }

    /// Rompedor 23-oct, descargas MEDIA 1: las cabeceras a goteo se cortan con el plazo de cabeceras.
    #[test]
    fn cabeceras_a_goteo_se_cortan_con_el_plazo_de_cabeceras() {
        let (url, _) = serve_raw(|_, conn| {
            let _ = conn.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nX-Goteo: ");
            for _ in 0..30 {
                std::thread::sleep(Duration::from_millis(200));
                if conn.write_all(b"a").is_err() {
                    return;
                }
            }
        });
        let t = Instant::now();
        let agent = agent();
        let e = get_following_timed(&agent, &format!("{url}/x"), Duration::from_millis(700), Duration::from_secs(30), |_| true).err().expect("tenía que fallar");
        assert!(matches!(e, GetError::Transport(ref m) if m.contains("no respondió")), "{e:?}");
        assert!(t.elapsed() < Duration::from_secs(3), "{:?}", t.elapsed());
        // Con cabeceras normales, la respuesta llega entera.
        let (ok, _) = serve(vec![("/b", b"hola".to_vec())]);
        let r = get_following_timed(&agent, &format!("{ok}/b"), Duration::from_secs(5), Duration::from_secs(30), |_| true).unwrap();
        assert_eq!(r.status(), 200);
    }

    /// Rompedor 23-oct, descargas BAJA 3: un `size` enorme en el JSON no sube el tope por encima de 512 MB.
    #[test]
    fn un_size_enorme_en_el_json_no_levanta_el_tope() {
        let dir = tmp("size-enorme");
        for size in [INSTALLER_MAX_BYTES + 1, 50 * 1024 * 1024 * 1024, u64::MAX] {
            let asset = Asset { name: "P.exe".into(), url: "http://127.0.0.1:1/x.exe".into(), size, sha256: None };
            let e = download_installer(&asset, &dir, "P.exe", NO_LIMITS, true, &mut |_, _, _| {}).unwrap_err();
            assert!(e.contains("máximo"), "{e}");
        }
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
        let _ = fs::remove_dir_all(dir);
    }

    /// PRNT-41: plazo total y avance mínimo. Un servidor que gotea aborta y no deja `.part`.
    #[test]
    fn descarga_que_gotea_se_aborta_y_no_deja_part() {
        let dir = tmp("goteo");
        let (url, _) = serve_raw(|_, conn| {
            let _ = conn.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100000\r\nConnection: close\r\n\r\nMZ");
            for _ in 0..200 {
                std::thread::sleep(Duration::from_millis(20));
                if conn.write_all(b"\0").is_err() {
                    return;
                }
            }
        });
        let asset = Asset { name: "P.exe".into(), url: format!("{url}/a.exe"), size: 100_000, sha256: None };
        let t0 = Instant::now();
        let lento = Limits { max_bytes: 0, deadline: None, window: Some((Duration::from_millis(400), 1024)) };
        let e = download_installer(&asset, &dir, "P.exe", lento, true, &mut |_, _, _| {}).unwrap_err();
        assert!(e.contains("lenta"), "{e}");
        let plazo = Limits { max_bytes: 0, deadline: Some(Duration::from_millis(400)), window: None };
        let e = download_installer(&asset, &dir, "P.exe", plazo, true, &mut |_, _, _| {}).unwrap_err();
        assert!(e.contains("tarda"), "{e}");
        assert!(t0.elapsed() < Duration::from_secs(3), "{:?}", t0.elapsed());
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0, "sin .part");
        // Los de verdad: 30 min de plazo y 64 KB cada 30 s.
        assert_eq!((INSTALLER_DEADLINE, INSTALLER_WINDOW, INSTALLER_MIN_BYTES), (Duration::from_secs(1800), Duration::from_secs(30), 65_536));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn hosts_de_este_pc() {
        for h in ["localhost", "LocalHost", "127.0.0.1", "127.9.9.9", "[::1]", "[::ffff:127.0.0.1]"] {
            assert!(is_loopback_host(h), "{h}");
        }
        for h in ["", "evil.example", "localhost.evil", "0.0.0.0", "[::]", "192.168.1.5", "10.0.0.1"] {
            assert!(!is_loopback_host(h), "{h}");
        }
    }

    #[test]
    fn consulta_a_un_servidor_local_y_errores() {
        let (url, _) = serve(vec![("/latest", REAL_RELEASE.as_bytes().to_vec()), ("/roto", b"esto no es json".to_vec())]);
        let r = fetch_latest(&format!("{url}/latest")).unwrap().unwrap();
        assert_eq!(r.version.to_string(), "2.0.1");
        assert!(fetch_latest(&format!("{url}/nada")).unwrap().is_none(), "404 = sin releases");
        let e = fetch_latest(&format!("{url}/roto")).unwrap_err();
        assert!(!e.rate_limited && e.msg.contains("no se entiende"), "{}", e.msg);
        // puerto cerrado: sin red
        let closed = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = closed.local_addr().unwrap();
        drop(closed);
        let e = fetch_latest(&format!("http://{addr}/latest")).unwrap_err();
        assert!(!e.rate_limited && e.msg.contains("No se pudo conectar"), "{}", e.msg);
    }

    // ---------------------------------------------------------- contrato JSON, resultado del ayudante, ayudante

    #[test]
    fn json_del_estado_y_del_progreso() {
        let r = parse_release(REAL_RELEASE).unwrap().unwrap();
        let s = UpdateStatus {
            current_version: "2.0.0".into(), state: "available", checking: false, checked_at: None, info: Some(info_of(&r)),
            done_bytes: 0, total_bytes: 0, percent: 0, error: None,
        };
        let j = serde_json::to_value(&s).unwrap();
        assert_eq!(j["currentVersion"], "2.0.0");
        assert_eq!(j["state"], "available");
        assert_eq!(j["info"]["version"], "2.0.1");
        assert_eq!(j["info"]["assetName"], "Pronto-Setup-2.0.1.exe");
        assert_eq!(j["info"]["size"], 10_800_872);
        assert!(j["info"].get("assetUrl").is_none() && j["info"].get("sha256").is_none(), "la URL y el hash no salen del backend");
        let p = serde_json::to_value(UpdateProgress { state: "downloading", done_bytes: 1, total_bytes: 2, percent: 50, error: None }).unwrap();
        assert_eq!(p, serde_json::json!({ "state": "downloading", "doneBytes": 1, "totalBytes": 2, "percent": 50 }));
    }

    #[test]
    fn resultado_del_ayudante_solo_avisa_si_fallo() {
        let dir = tmp("result");
        assert!(take_last_install_error(&dir).is_none());
        fs::write(dir.join("result.txt"), "ok\r\n").unwrap();
        assert!(take_last_install_error(&dir).is_none());
        assert!(!dir.join("result.txt").exists(), "se consume");
        fs::write(dir.join("result.txt"), "error 1\r\n").unwrap();
        let m = take_last_install_error(&dir).unwrap();
        assert!(m.contains("código 1") && m.contains("github.com"), "{m}");
        assert!(take_last_install_error(&dir).is_none());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn compilaciones_de_desarrollo_no_se_autoinstalan() {
        assert!(is_dev_exe(Path::new(r"C:\x\Prontov2\src-tauri\target\release\pronto.exe")));
        assert!(is_dev_exe(Path::new(r"C:\x\src-tauri\target\debug\pronto.exe")));
        assert!(!is_dev_exe(Path::new(r"C:\Users\a\AppData\Local\Programs\Pronto\pronto.exe")));
        // Carpeta de compilación con otro nombre (`target-dir` de src-tauri/.cargo/config.toml): se
        // reconoce por lo que cargo deja junto al ejecutable.
        let dir = std::env::temp_dir().join(format!("pronto_dev_pt_{}", std::process::id())).join("release");
        fs::create_dir_all(dir.join("deps")).unwrap();
        fs::create_dir_all(dir.join(".fingerprint")).unwrap();
        assert!(is_dev_exe(&dir.join("pronto.exe")));
        let _ = fs::remove_dir_all(dir.parent().unwrap());
        // En depuración nunca, ni con la variable de pruebas; instalada, sí.
        let installed = Path::new(r"C:\Users\a\AppData\Local\Programs\Pronto\pronto.exe");
        assert!(install_blocked(installed, true, true));
        assert!(!install_blocked(installed, false, false));
        assert!(install_blocked(Path::new(r"C:\x\target\release\pronto.exe"), false, false));
        assert!(!install_blocked(Path::new(r"C:\x\target\release\pronto.exe"), false, true));
    }

    #[test]
    fn no_se_cierra_pronto_si_se_empezo_a_dictar_durante_la_descarga() {
        let mut launched = false;
        let r = launch_when_idle(|| Some("Estás dictando ahora mismo."), || {
            launched = true;
            Ok(())
        });
        assert!(r.is_err() && !launched, "el ayudante cierra Pronto: no se lanza con un dictado en curso");
        let mut launched = false;
        assert!(launch_when_idle(|| None, || {
            launched = true;
            Ok(())
        })
        .is_ok());
        assert!(launched);
    }

    #[test]
    fn el_ayudante_hace_lo_prometido() {
        let s = HELPER_SCRIPT;
        for needle in ["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "Wait-Process", "helper-started", "result.txt", "[System.Diagnostics.Process]::Start($psi)"] {
            assert!(s.contains(needle), "falta {needle}");
        }
        assert!(s.is_ascii(), "ASCII puro: PowerShell 5.1 lee el script sin sorpresas");
        assert_eq!(s.matches("::Start(").count() + s.matches("Start-Process -FilePath $Exe").count(), 1, "un solo punto de reapertura");
        // Rompedor 22-oct-c, descargas MEDIA 2: con corchetes en la carpeta («C:\Programas [Alfonso]»)
        // `-WorkingDirectory` y `Out-File -FilePath` los toman como comodín. Nada de parámetros que
        // interpreten comodines con rutas que vienen de fuera.
        for wildcard in ["-WorkingDirectory", "Out-File -FilePath", "Split-Path -Parent $Exe", "Test-Path $", "Get-Item $"] {
            assert!(!s.contains(wildcard), "{wildcard}: trata los corchetes como comodín");
        }
    }

    /// `#[ignore]`: consulta REAL a GitHub (solo lectura). `cargo test ultima_release_real -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn ultima_release_real() {
        let r = fetch_latest(LATEST_URL).map_err(|e| e.msg).unwrap().expect("hay release");
        eprintln!("latest = {} · {} · {} B · digest {:?}", r.tag, r.asset.name, r.asset.size, r.asset.sha256);
        assert!(url_allowed(&r.asset.url, false));
        assert!(r.asset.sha256.is_some() && r.asset.size > 1_000_000);
        assert!(is_newer(&r.version, &v("1.0.0")));
    }

    /// `#[ignore]`: descarga REAL del instalador de la última release a `PRONTO_DL_DIR` y verificación.
    #[test]
    #[ignore]
    fn instalador_real_de_github() {
        let dir = PathBuf::from(std::env::var("PRONTO_DL_DIR").expect("PRONTO_DL_DIR"));
        let r = fetch_latest(LATEST_URL).map_err(|e| e.msg).unwrap().expect("hay release");
        let mut last = 0u32;
        let path = fetch_installer(&agent(), &r.asset, &dir, &mut |s, d, t| {
            let p = percent(d, t);
            if p / 10 != last / 10 || s == "verifying" {
                eprintln!("{s} {p} % ({d}/{t})");
            }
            last = p;
        })
        .unwrap();
        eprintln!("descargado y verificado: {}", path.display());
        assert!(r.asset.sha256.is_some(), "esta prueba exige digest");
    }

    /// `#[ignore]`: ayudante REAL. Instala `PRONTO_E2E_INSTALLER` (un instalador de pruebas /DTestSandbox) y
    /// comprueba que reabre `PRONTO_E2E_EXE` una sola vez. Ver docs/ARQUITECTURA.md «Pruebas».
    #[test]
    #[ignore]
    fn ayudante_real() {
        let installer = PathBuf::from(std::env::var("PRONTO_E2E_INSTALLER").expect("PRONTO_E2E_INSTALLER"));
        let exe = PathBuf::from(std::env::var("PRONTO_E2E_EXE").expect("PRONTO_E2E_EXE"));
        let dir = PathBuf::from(std::env::var("PRONTO_E2E_DIR").expect("PRONTO_E2E_DIR"));
        // «Pronto» ficticio que sigue vivo unos segundos: el ayudante tiene que esperarle. Con
        // PRONTO_E2E_PID se usa un proceso que ya existe (el pronto.exe de pruebas, que alguien cerrará).
        let mut fake = None;
        let pid = match std::env::var("PRONTO_E2E_PID").ok().and_then(|p| p.parse::<u32>().ok()) {
            Some(pid) => pid,
            None => {
                let c = Command::new(powershell_exe()).args(["-NoProfile", "-Command", "Start-Sleep -Seconds 4"]).spawn().unwrap();
                let id = c.id();
                fake = Some(c);
                id
            }
        };
        let t0 = Instant::now();
        spawn_helper(pid, &installer, &exe, &dir).unwrap();
        eprintln!("ayudante vivo tras {:?}", t0.elapsed());
        if let Some(mut c) = fake {
            let _ = c.wait();
        }
        let deadline = Instant::now() + Duration::from_secs(90);
        while Instant::now() < deadline && !dir.join("result.txt").is_file() {
            std::thread::sleep(Duration::from_millis(500));
        }
        let result = fs::read_to_string(dir.join("result.txt")).unwrap_or_default();
        eprintln!("resultado: {}", result.trim());
        assert_eq!(result.trim(), "ok");
    }
}
