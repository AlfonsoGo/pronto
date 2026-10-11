//! Salida del texto: portapapeles + `SendInput` Ctrl+V, y detección de la app con foco.

use parking_lot::Mutex;
use std::time::{Duration, Instant};
use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HWND};
use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId};

/// Marca (`dwExtraInfo`) de las teclas que inyectamos, para que el hook las ignore.
pub const MARKER: usize = 0x5052_4F32; // «PRO2»

/// Cuánto esperamos antes de devolver el portapapeles original.
const RESTORE_AFTER: Duration = Duration::from_millis(600);

#[derive(Debug, Clone, Default)]
pub struct AppInfo {
    /// Nombre del ejecutable, p. ej. `chrome.exe` (vacío si no se pudo leer).
    pub exe: String,
    pub title: String,
}

impl AppInfo {
    /// Nombre legible para el historial: `chrome.exe` -> `Chrome`.
    pub fn display_name(&self) -> String {
        let stem = self.exe.trim_end_matches(".exe").trim_end_matches(".EXE");
        let mut chars = stem.chars();
        match chars.next() {
            Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
            None => String::new(),
        }
    }
}

/// Ejecutable y título de la ventana con foco. Nunca falla: devuelve vacíos si no puede.
pub fn foreground_app() -> AppInfo {
    // SAFETY: GetForegroundWindow no tiene precondiciones.
    app_info_of(unsafe { GetForegroundWindow() })
}

/// Ejecutable y título de una ventana cualquiera (vacíos si no se puede leer).
pub fn app_info_of(hwnd: HWND) -> AppInfo {
    // SAFETY: llamadas Win32 sin precondiciones; los búferes viven durante la llamada.
    unsafe {
        if hwnd.0.is_null() {
            return AppInfo::default();
        }
        let mut title_buf = [0u16; 512];
        let n = GetWindowTextW(hwnd, &mut title_buf);
        let title = String::from_utf16_lossy(&title_buf[..n.max(0) as usize]);

        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let mut exe = String::new();
        if pid != 0 {
            if let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                let mut buf = [0u16; 1024];
                let mut len = buf.len() as u32;
                if QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok() {
                    let full = String::from_utf16_lossy(&buf[..len as usize]);
                    exe = full.rsplit(['\\', '/']).next().unwrap_or("").to_string();
                }
                let _ = CloseHandle(h);
            }
        }
        AppInfo { exe, title }
    }
}

/// Una pulsación a inyectar. `scan`/`extended` importan al reinyectar una tecla real (p. ej. el
/// Alt derecho es «extendido»); para teclas sintéticas basta el código virtual.
#[derive(Debug, Clone, Copy)]
pub struct KeyEvent {
    pub vk: u16,
    pub scan: u16,
    pub extended: bool,
    pub up: bool,
}

impl KeyEvent {
    pub fn vk(vk: u16, up: bool) -> Self {
        Self { vk, scan: 0, extended: false, up }
    }
}

/// Inyecta eventos marcados como propios, en un solo `SendInput` (el orden queda garantizado).
pub fn send_key_events(keys: &[KeyEvent]) {
    let inputs: Vec<INPUT> = keys
        .iter()
        .map(|k| {
            let mut flags = KEYBD_EVENT_FLAGS(0);
            if k.up {
                flags |= KEYEVENTF_KEYUP;
            }
            if k.extended {
                flags |= KEYEVENTF_EXTENDEDKEY;
            }
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT { wVk: VIRTUAL_KEY(k.vk), wScan: k.scan, dwFlags: flags, time: 0, dwExtraInfo: MARKER },
                },
            }
        })
        .collect();
    // SAFETY: `inputs` es un slice válido de INPUT y el tamaño es el de la estructura.
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize != inputs.len() {
        eprintln!("[inject] SendInput envió {sent} de {} eventos", inputs.len());
    }
}

/// Inyecta una secuencia de teclas `(vk, soltar)` marcadas como propias.
pub fn send_keys(keys: &[(u16, bool)]) {
    let inputs: Vec<INPUT> = keys
        .iter()
        .map(|&(vk, up)| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(vk),
                    wScan: 0,
                    dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                    time: 0,
                    dwExtraInfo: MARKER,
                },
            },
        })
        .collect();
    // SAFETY: `inputs` es un slice válido de INPUT y el tamaño es el de la estructura.
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize != inputs.len() {
        eprintln!("[inject] SendInput envió {sent} de {} eventos", inputs.len());
    }
}

fn key_down(vk: u32) -> bool {
    // SAFETY: GetAsyncKeyState no tiene precondiciones.
    unsafe { (GetAsyncKeyState(vk as i32) as u16) & 0x8000 != 0 }
}

/// Espera a que se suelten Win, Alt, Shift y Ctrl: si siguieran pulsadas, el Ctrl+V llegaría
/// a la app como Win+Alt+Ctrl+V (p. ej. al parar el modo manos libres sin soltar el atajo).
fn wait_modifiers_released(max: Duration) {
    const MODS: [u32; 8] = [0x5B, 0x5C, 0xA4, 0xA5, 0xA0, 0xA1, 0xA2, 0xA3];
    let t0 = Instant::now();
    while MODS.iter().any(|&v| key_down(v)) && t0.elapsed() < max {
        std::thread::sleep(Duration::from_millis(15));
    }
}

enum Saved {
    Text(String),
    Image(arboard::ImageData<'static>),
    Empty,
}

struct Pending {
    original: Saved,
    token: u64,
}

/// Restauración pendiente: si llegan dos pegados seguidos hay que devolver el portapapeles del
/// usuario, no el texto del primer pegado.
static PENDING: Mutex<Option<Pending>> = Mutex::new(None);
static TOKEN: Mutex<u64> = Mutex::new(0);

fn read_clipboard(cb: &mut arboard::Clipboard) -> Saved {
    if let Ok(t) = cb.get_text() {
        return Saved::Text(t);
    }
    if let Ok(img) = cb.get_image() {
        return Saved::Image(arboard::ImageData { width: img.width, height: img.height, bytes: img.bytes.into_owned().into() });
    }
    Saved::Empty
}

fn restore_clipboard(saved: Saved) {
    let Ok(mut cb) = arboard::Clipboard::new() else { return };
    let r = match saved {
        Saved::Text(t) => cb.set_text(t),
        Saved::Image(i) => cb.set_image(i),
        Saved::Empty => cb.clear(),
    };
    if let Err(e) = r {
        eprintln!("[inject] no se pudo restaurar el portapapeles: {e}");
    }
}

/// Pone `text` en el portapapeles (sin pegar).
pub fn copy_text(text: &str) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    cb.set_text(text.to_string()).map_err(|e| e.to_string())
}

/// Pega `text` en la app con foco: guarda el portapapeles, escribe el texto, Ctrl+V y restaura a
/// los 600 ms (en un hilo aparte para no retrasar el resto del flujo).
pub fn paste_text(text: &str) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| format!("portapapeles no disponible: {e}"))?;
    let token = {
        let mut t = TOKEN.lock();
        *t += 1;
        *t
    };
    {
        let mut pending = PENDING.lock();
        match pending.as_mut() {
            Some(p) => p.token = token, // ya hay un original guardado: lo conservamos
            None => *pending = Some(Pending { original: read_clipboard(&mut cb), token }),
        }
    }
    cb.set_text(text.to_string()).map_err(|e| format!("no se pudo escribir en el portapapeles: {e}"))?;
    drop(cb);

    wait_modifiers_released(Duration::from_millis(2000));
    if cfg!(debug_assertions) {
        let fg = foreground_app();
        let held: Vec<String> = [0x5Bu32, 0x5C, 0xA4, 0xA5, 0xA0, 0xA1, 0xA2, 0xA3, 0x12, 0x11, 0x10]
            .iter()
            .filter(|&&v| key_down(v))
            .map(|v| format!("{v:#X}"))
            .collect();
        eprintln!("[inject] Ctrl+V -> {} «{}»; teclas abajo: {held:?}", fg.exe, fg.title);
    }
    send_keys(&[(0x11, false), (0x56, false), (0x56, true), (0x11, true)]);

    std::thread::spawn(move || {
        std::thread::sleep(RESTORE_AFTER);
        let mut pending = PENDING.lock();
        if pending.as_ref().is_some_and(|p| p.token == token) {
            if let Some(p) = pending.take() {
                restore_clipboard(p.original);
            }
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nombre_legible() {
        let a = AppInfo { exe: "chrome.exe".into(), title: String::new() };
        assert_eq!(a.display_name(), "Chrome");
        assert_eq!(AppInfo::default().display_name(), "");
    }
}

// ------------------------------------------------------------------ transforms

/// Pulsa Enter (para «pulsa enter» al final de un dictado).
pub fn press_enter() {
    wait_modifiers_released(Duration::from_millis(1000));
    send_keys(&[(0x0D, false), (0x0D, true)]);
}

/// Devuelve el portapapeles del usuario si quedó un original pendiente de restaurar.
pub fn restore_pending() {
    if let Some(p) = PENDING.lock().take() {
        restore_clipboard(p.original);
    }
}

/// Copia la selección actual de la app con foco (Ctrl+C) y devuelve su texto. Guarda antes el
/// portapapeles del usuario como «original»: lo restaura `paste_text` tras pegar, o
/// `restore_pending` si el flujo se aborta.
pub fn capture_selection() -> Result<String, String> {
    use windows::Win32::System::DataExchange::GetClipboardSequenceNumber;
    let mut cb = arboard::Clipboard::new().map_err(|e| format!("portapapeles no disponible: {e}"))?;
    let seeded = {
        let mut pending = PENDING.lock();
        if pending.is_none() {
            let mut t = TOKEN.lock();
            *t += 1;
            *pending = Some(Pending { original: read_clipboard(&mut cb), token: *t });
            true
        } else {
            false
        }
    };
    // SAFETY: sin precondiciones.
    let before = unsafe { GetClipboardSequenceNumber() };
    wait_modifiers_released(Duration::from_millis(2000));
    send_keys(&[(0x11, false), (0x43, false), (0x43, true), (0x11, true)]);

    // Esperamos a que la app publique la selección: lo sabemos porque cambia el número de secuencia.
    let t0 = Instant::now();
    let mut changed = false;
    while t0.elapsed() < Duration::from_millis(900) {
        // SAFETY: sin precondiciones.
        if unsafe { GetClipboardSequenceNumber() } != before {
            changed = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    let text = if changed {
        std::thread::sleep(Duration::from_millis(30));
        cb.get_text().ok().filter(|t| !t.trim().is_empty())
    } else {
        None
    };
    match text {
        Some(t) => Ok(t),
        None => {
            if seeded {
                restore_pending();
            }
            Err("Selecciona texto primero".into())
        }
    }
}
