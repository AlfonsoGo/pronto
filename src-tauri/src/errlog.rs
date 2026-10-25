//! Registro local de errores (decisión de Txonso, 23-oct-2026).
//!
//! Pronto sin consola (`windows_subsystem = "windows"`) tira a la basura todo `eprintln!`: cuando
//! algo falla en casa de un cliente no queda rastro. Este módulo deja una línea por error en
//! `%APPDATA%\Pronto2\logs\errores.log` (misma carpeta base que `db.rs`/`modeldl.rs`) y recoge los
//! pánicos con un `panic::set_hook` que encadena el anterior.
//!
//! Reglas:
//! - Regla 1 (privacidad): el fichero NUNCA sale del PC. Aquí no hay ninguna petición de red; el
//!   único «envío» es que el usuario abra la carpeta con el botón del hub.
//! - Sin texto del usuario: quien llama pasa solo mensajes de fallo (nunca dictados, transcripciones
//!   ni notas). Por si se cuela algo, cada línea pasa por `sanitize`: `C:\Users\<nombre>` se queda en
//!   `C:\Users\…`, el nombre de usuario de Windows suelto se tapa y lo que parece una frase entre
//!   comillas se sustituye por `«…»`. El mensaje de un pánico no se copia si es dinámico (los
//!   `unwrap` sobre un `Err` o un corte de cadena meten datos del usuario en el texto).
//! - Una línea por error: `2026-10-23T10:15:00+02:00 v2.2.0 [modulo] mensaje`.
//! - Rotación: al pasar de 1 MB se renombra `errores.log` → `errores.1.log` → `errores.2.log` y se
//!   borra el más viejo (3 ficheros como máximo).
//! - Escribir en el registro nunca falla hacia fuera ni entra en pánico: si no se puede, se pierde
//!   la línea (sigue saliendo por stderr).

use parking_lot::Mutex;
use regex::Regex;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

/// Nombre del fichero activo.
pub const FILE_NAME: &str = "errores.log";
/// Tamaño a partir del cual se rota.
pub const MAX_BYTES: u64 = 1024 * 1024;
/// Ficheros que se guardan (el activo y los rotados).
pub const KEEP: usize = 3;
/// Una línea más larga se corta (un error no necesita más; evita volcar un cuerpo de respuesta).
const MAX_LINE_CHARS: usize = 400;

/// Carpeta del registro: `%APPDATA%\Pronto2\logs`. En las pruebas, una carpeta temporal propia:
/// nunca se toca la del usuario.
pub fn dir() -> PathBuf {
    if cfg!(test) {
        return std::env::temp_dir().join(format!("pronto2-pruebas-logs-{}", std::process::id()));
    }
    crate::db::data_root().unwrap_or_else(|| std::env::temp_dir().join("Pronto2")).join("logs")
}

// ------------------------------------------------------------ limpieza del texto

fn re_user_path() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // `C:\Users\Nombre`, `c:/users/Nombre`, `\\?\C:\Users\Nombre` y la forma con barras dobles del
    // `{:?}` de Rust (`C:\\Users\\Nombre`). El nombre llega hasta la barra siguiente o un espacio.
    RE.get_or_init(|| Regex::new(r#"(?i)([a-z]):[\\/]+users[\\/]+[^\\/\s"'<>|:*?«»]+"#).expect("regex de ruta de usuario"))
}

fn re_quoted() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"«[^»]*»|“[^”]*”|"[^"]*""#).expect("regex de comillas"))
}

/// Nombre de usuario de Windows (y su carpeta de perfil), para taparlos aunque no vayan en una ruta.
fn identity() -> &'static (Vec<String>, Vec<String>) {
    static ID: OnceLock<(Vec<String>, Vec<String>)> = OnceLock::new();
    ID.get_or_init(|| {
        let mut names = Vec::new();
        for var in ["USERNAME", "USER"] {
            if let Ok(v) = std::env::var(var) {
                let v = v.trim().to_string();
                // Nombres de 1-2 letras destrozarían el texto; el patrón de ruta ya los cubre.
                if v.chars().count() >= 3 && !names.iter().any(|n: &String| n.eq_ignore_ascii_case(&v)) {
                    names.push(v);
                }
            }
        }
        let mut profiles = Vec::new();
        if let Some(h) = dirs::home_dir() {
            profiles.push(h.to_string_lossy().to_string());
        }
        (names, profiles)
    })
}

fn replace_ci(text: &str, needle: &str, with: &str) -> String {
    match Regex::new(&format!("(?i){}", regex::escape(needle))) {
        Ok(re) => re.replace_all(text, regex::NoExpand(with)).into_owned(),
        Err(_) => text.to_string(),
    }
}

/// Quita del texto lo que identifica al usuario. Ver el comentario del módulo.
pub fn sanitize(msg: &str) -> String {
    sanitize_with(msg, &identity().0, &identity().1)
}

fn sanitize_with(msg: &str, names: &[String], profiles: &[String]) -> String {
    // Una sola línea.
    let mut s: String = msg.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    // Carpeta de perfil completa (también con `/` y con barras dobles), antes que el patrón general:
    // cubre un perfil que no se llama como el usuario o con espacios en el nombre.
    for p in profiles.iter().filter(|p| !p.is_empty()) {
        for variant in [p.clone(), p.replace('\\', "/"), p.replace('\\', "\\\\")] {
            s = replace_ci(&s, &variant, "C:\\Users\\…");
        }
    }
    s = re_user_path().replace_all(&s, "${1}:\\Users\\…").into_owned();
    // Frases entre comillas (parecen texto del usuario); las rutas y los identificadores se quedan.
    s = re_quoted()
        .replace_all(&s, |c: &regex::Captures| {
            let q = &c[0];
            let inner: String = q.chars().skip(1).take(q.chars().count().saturating_sub(2)).collect();
            let looks_like_prose = inner.split_whitespace().count() >= 4 && !inner.contains(['\\', '/', ':']);
            if looks_like_prose { "«…»".to_string() } else { q.to_string() }
        })
        .into_owned();
    for n in names {
        s = replace_ci(&s, n, "…");
    }
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() > MAX_LINE_CHARS {
        let cut: String = s.chars().take(MAX_LINE_CHARS).collect();
        format!("{cut}… (cortado)")
    } else {
        s
    }
}

/// El módulo va entre corchetes: solo letras, cifras, `_` y `-`, y corto.
fn clean_module(module: &str) -> String {
    let m: String = module.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-')).take(24).collect();
    if m.is_empty() { "?".into() } else { m }
}

/// La línea completa (sin salto de línea) con la fecha que se le da.
fn format_line(stamp: &str, version: &str, module: &str, msg: &str) -> String {
    format!("{stamp} v{version} [{}] {}", clean_module(module), sanitize(msg))
}

// ------------------------------------------------------------ fichero con rotación

/// Fichero de registro con rotación. Sin estado propio: cada escritura mira el tamaño actual.
pub struct Log {
    dir: PathBuf,
    max_bytes: u64,
    keep: usize,
    lock: Mutex<()>,
}

impl Log {
    pub fn new(dir: PathBuf, max_bytes: u64, keep: usize) -> Self {
        Self { dir, max_bytes, keep: keep.max(1), lock: Mutex::new(()) }
    }

    /// Nombre del fichero rotado `n` (`errores.1.log`…); el 0 es el activo.
    fn path_of(&self, n: usize) -> PathBuf {
        if n == 0 { self.dir.join(FILE_NAME) } else { self.dir.join(format!("errores.{n}.log")) }
    }

    fn rotate(&self) {
        let _ = fs::remove_file(self.path_of(self.keep - 1));
        for n in (0..self.keep - 1).rev() {
            let from = self.path_of(n);
            if from.exists() {
                let _ = fs::rename(&from, self.path_of(n + 1));
            }
        }
    }

    /// Añade una línea ya formada. `false` si no se pudo escribir (nunca entra en pánico).
    pub fn append(&self, line: &str) -> bool {
        // Con plazo: un pánico mientras otro hilo escribe no debe colgar el gancho de pánicos.
        let Some(_guard) = self.lock.try_lock_for(Duration::from_millis(500)) else { return false };
        if fs::create_dir_all(&self.dir).is_err() {
            return false;
        }
        let size = fs::metadata(self.path_of(0)).map(|m| m.len()).unwrap_or(0);
        if size > 0 && size + line.len() as u64 + 1 > self.max_bytes {
            self.rotate();
        }
        match OpenOptions::new().create(true).append(true).open(self.path_of(0)) {
            Ok(mut f) => writeln!(f, "{line}").is_ok(),
            Err(_) => false,
        }
    }
}

fn global() -> &'static Log {
    static LOG: OnceLock<Log> = OnceLock::new();
    LOG.get_or_init(|| Log::new(dir(), MAX_BYTES, KEEP))
}

fn now_stamp() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%:z").to_string()
}

/// Registra un error de `module` y lo deja también en stderr (como el `eprintln!` que sustituye).
/// `msg` no debe llevar texto del usuario (dictados, transcripciones, notas): solo el fallo.
pub fn error(module: &str, msg: impl AsRef<str>) {
    let line = format_line(&now_stamp(), env!("CARGO_PKG_VERSION"), module, msg.as_ref());
    eprintln!("{line}");
    global().append(&line);
}

// ------------------------------------------------------------ pánicos

/// Texto de un pánico que es seguro copiar: solo los literales (`panic!("…")`, `expect`). Un mensaje
/// formateado (`unwrap` sobre `Err(e)`, un corte de cadena fuera de límite…) lleva datos de dentro.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    match payload.downcast_ref::<&'static str>() {
        Some(s) => (*s).to_string(),
        None => match payload.downcast_ref::<String>() {
            Some(s) => format!("mensaje con datos omitido ({} caracteres)", s.chars().count()),
            None => "sin mensaje".into(),
        },
    }
}

fn panic_text(info: &std::panic::PanicHookInfo<'_>) -> String {
    let at = info.location().map_or_else(|| "?".to_string(), |l| format!("{}:{}", l.file(), l.line()));
    let thread = std::thread::current();
    format!("pánico en {at} (hilo {}): {}", thread.name().unwrap_or("sin nombre"), panic_message(info.payload()))
}

/// Instala el gancho de pánicos: registra el pánico y llama al gancho anterior (el mensaje de
/// siempre por stderr, o el de las pruebas).
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let line = format_line(&now_stamp(), env!("CARGO_PKG_VERSION"), "panic", &panic_text(info));
        global().append(&line);
        previous(info);
    }));
}

/// Primer paso de `main`: gancho de pánicos y una línea de arranque (para ver desde cuándo hay registro).
pub fn init() {
    install_panic_hook();
}

/// Carpeta del registro, creada si no existe, lista para abrirla en el Explorador.
pub fn ensure_dir() -> Result<PathBuf, String> {
    let d = dir();
    fs::create_dir_all(&d).map_err(|e| format!("No se pudo crear la carpeta del registro: {e}"))?;
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pronto2-errlog-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    fn san(s: &str) -> String {
        sanitize_with(s, &[], &[])
    }

    #[test]
    fn filtra_la_carpeta_de_usuario() {
        assert_eq!(san(r"no se pudo abrir C:\Users\usuario\AppData\Roaming\Pronto2\pronto2.db"), r"no se pudo abrir C:\Users\…\AppData\Roaming\Pronto2\pronto2.db");
        // Sin conocer el nombre, el de la ruta llega hasta el primer espacio (el real se tapa con `identity`).
        assert_eq!(san(r"c:/users/Ana Maria/Documents/x.wav"), r"c:\Users\… Maria/Documents/x.wav");
        assert_eq!(san(r"\\?\D:\USERS\pepe\x"), r"\\?\D:\Users\…\x");
        // Forma con barras dobles del `{:?}` de Rust.
        assert_eq!(san(r"Os { path: C:\\Users\\usuario\\x }"), r"Os { path: C:\Users\…\\x }");
        assert!(!san(r"C:\Users\usuario").contains("Txonso"));
        // Las rutas que no son de usuario se conservan.
        assert_eq!(san(r"C:\Program Files\Ollama\ollama.exe"), r"C:\Program Files\Ollama\ollama.exe");
    }

    #[test]
    fn filtra_el_perfil_y_el_nombre_de_usuario_aunque_no_vayan_en_una_ruta() {
        let names = vec!["Juan Perez".to_string()];
        let profiles = vec![r"C:\Users\Juan Perez".to_string()];
        let s = sanitize_with(r"fallo en C:\Users\Juan Perez\Documents\a.wav de Juan Perez", &names, &profiles);
        assert_eq!(s, r"fallo en C:\Users\…\Documents\a.wav de …");
        // Con barras normales y con barras dobles.
        assert!(!sanitize_with("C:/users/juan perez/x", &names, &profiles).to_lowercase().contains("juan"));
        assert!(!sanitize_with(r"C:\\Users\\Juan Perez\\x", &names, &profiles).contains("Juan"));
    }

    #[test]
    fn el_usuario_real_de_esta_maquina_no_sale() {
        // Lo que de verdad hará `error()` aquí: ni el nombre de usuario ni la carpeta de perfil.
        let home = dirs::home_dir().map(|h| h.to_string_lossy().to_string()).unwrap_or_default();
        if home.is_empty() {
            return;
        }
        let s = sanitize(&format!("no se pudo crear {home}\\AppData\\x"));
        assert!(!s.contains(&home), "{s}");
        assert!(s.contains(r"C:\Users\…") || !home.to_lowercase().contains(r":\users\"), "{s}");
    }

    #[test]
    fn tapa_frases_entre_comillas_y_deja_rutas_e_identificadores() {
        assert_eq!(san("falló «mañana a las diez tenemos que revisar el informe»"), "falló «…»");
        assert_eq!(san(r#"transcripción "hola que tal estas hoy" rota"#), "transcripción «…» rota");
        assert_eq!(san(r#"no existe "C:\Program Files\Mi App\x.exe""#), r#"no existe "C:\Program Files\Mi App\x.exe""#);
        assert_eq!(san("modelo «qwen3:1.7b» no encontrado"), "modelo «qwen3:1.7b» no encontrado");
        assert_eq!(san(r#"status "403 Forbidden""#), r#"status "403 Forbidden""#);
    }

    #[test]
    fn una_linea_y_corta() {
        let s = san("uno\ndos\r\ntres\t cuatro");
        assert_eq!(s, "uno dos tres cuatro");
        let largo = san(&"x".repeat(5000));
        assert!(largo.chars().count() < MAX_LINE_CHARS + 20, "{}", largo.len());
        assert!(largo.ends_with("(cortado)"));
    }

    #[test]
    fn el_modulo_no_puede_romper_la_linea() {
        let l = format_line("2026-10-23T10:00:00+02:00", "2.2.0", "me]\nmal [x", "m");
        assert_eq!(l, "2026-10-23T10:00:00+02:00 v2.2.0 [memalx] m");
        assert_eq!(format_line("t", "1", "", "m"), "t v1 [?] m");
    }

    #[test]
    fn escribe_una_linea_por_error_con_fecha_version_y_modulo() {
        let d = tmp("linea");
        let log = Log::new(d.clone(), MAX_BYTES, KEEP);
        assert!(log.append(&format_line(&now_stamp(), "2.2.0", "pegado", "no se pudo pegar: acceso denegado")));
        assert!(log.append(&format_line(&now_stamp(), "2.2.0", "ollama", r"sin respuesta de C:\Users\usuario\x")));
        let t = fs::read_to_string(d.join(FILE_NAME)).unwrap();
        let lines: Vec<&str> = t.lines().collect();
        assert_eq!(lines.len(), 2);
        let re = Regex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}[+-]\d{2}:\d{2} v2\.2\.0 \[pegado\] no se pudo pegar: acceso denegado$").unwrap();
        assert!(re.is_match(lines[0]), "{}", lines[0]);
        assert!(lines[1].contains("[ollama]") && lines[1].contains(r"C:\Users\…\x") && !lines[1].contains("Txonso"), "{}", lines[1]);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn rota_al_pasar_del_maximo_y_guarda_tres_ficheros() {
        let d = tmp("rota");
        let log = Log::new(d.clone(), 100, 3);
        let line = |n: usize| format!("{n:02} {}", "x".repeat(37)); // 40 bytes con el salto
        for n in 0..12 {
            assert!(log.append(&line(n)));
        }
        let mut names: Vec<String> = fs::read_dir(&d).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
        names.sort();
        assert_eq!(names, ["errores.1.log", "errores.2.log", "errores.log"], "3 ficheros como mucho");
        for n in ["errores.log", "errores.1.log", "errores.2.log"] {
            assert!(fs::metadata(d.join(n)).unwrap().len() <= 100, "{n} pasa del máximo");
        }
        // Lo más nuevo está en el activo; lo más viejo se ha ido.
        let activo = fs::read_to_string(d.join(FILE_NAME)).unwrap();
        assert!(activo.contains("11 x"), "{activo}");
        let todo: String = ["errores.2.log", "errores.1.log", FILE_NAME].iter().map(|n| fs::read_to_string(d.join(n)).unwrap()).collect();
        assert!(!todo.contains("00 x"), "la línea más vieja debía haberse borrado");
        // El orden entre ficheros: el 2 es el más viejo.
        let viejo = fs::read_to_string(d.join("errores.2.log")).unwrap();
        let medio = fs::read_to_string(d.join("errores.1.log")).unwrap();
        assert!(viejo.lines().last().unwrap() < medio.lines().next().unwrap());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn rota_con_el_maximo_real_de_1_mb() {
        let d = tmp("mb");
        let log = Log::new(d.clone(), MAX_BYTES, KEEP);
        let line = "y".repeat(999);
        for _ in 0..1100 {
            log.append(&line);
        }
        assert!(d.join("errores.1.log").exists(), "pasado 1 MB debía haber rotado");
        assert!(fs::metadata(d.join(FILE_NAME)).unwrap().len() <= MAX_BYTES);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn la_carpeta_de_las_pruebas_no_es_la_del_usuario() {
        let d = dir().to_string_lossy().to_lowercase();
        assert!(d.contains("pronto2-pruebas-logs"), "{d}");
        assert!(!d.contains("roaming"), "{d}");
    }

    #[test]
    fn el_gancho_de_panicos_escribe_y_encadena_el_anterior() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        let old = std::panic::take_hook();
        let called = Arc::new(AtomicBool::new(false));
        let c2 = called.clone();
        std::panic::set_hook(Box::new(move |_| c2.store(true, Ordering::SeqCst)));
        install_panic_hook(); // encadena el de arriba
        let r = std::thread::Builder::new()
            .name("hilo-de-prueba".into())
            .spawn(|| {
                if std::env::var_os("PRONTO_NO_EXISTE_NUNCA").is_none() {
                    panic!("fallo de prueba del gancho");
                }
            })
            .unwrap()
            .join();
        // Un pánico con texto dinámico (como el de un `unwrap` sobre `Err`): el texto no se copia.
        let secreto = "frase secreta del usuario";
        let r2 = std::thread::spawn(move || panic!("dictado: {secreto}")).join();
        let _ = std::panic::take_hook();
        std::panic::set_hook(old);
        assert!(r.is_err() && r2.is_err());
        assert!(called.load(Ordering::SeqCst), "el gancho anterior debe seguir llamándose");
        let t = fs::read_to_string(dir().join(FILE_NAME)).expect("el pánico debía escribir el registro");
        assert!(t.contains("[panic]") && t.contains("pánico en ") && t.contains("errlog.rs:"), "{t}");
        assert!(t.contains("hilo-de-prueba") && t.contains("fallo de prueba del gancho"), "{t}");
        assert!(!t.contains("secreta"), "el texto de un pánico dinámico no debe copiarse: {t}");
        assert!(t.contains("mensaje con datos omitido"), "{t}");
    }

    #[test]
    fn error_escribe_en_el_registro_de_pruebas() {
        error("prueba", r"fallo en C:\Users\Alguien\x.wav");
        let t = fs::read_to_string(dir().join(FILE_NAME)).unwrap();
        assert!(t.contains("[prueba] fallo en C:\\Users\\…\\x.wav"), "{t}");
        assert!(t.lines().all(|l| !l.contains("Alguien")));
    }
}
