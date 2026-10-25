//! Salida del texto: portapapeles + `SendInput` Ctrl+V, y detección de la app con foco.

use parking_lot::Mutex;
use std::time::{Duration, Instant};
use windows::core::{w, PWSTR};
use windows::Win32::Foundation::{CloseHandle, GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData, OpenClipboard, RegisterClipboardFormatW, SetClipboardData};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE, GMEM_ZEROINIT};
use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId};

/// Marca (`dwExtraInfo`) de las teclas que inyectamos, para que el hook las ignore.
pub const MARKER: usize = 0x5052_4F32; // «PRO2»

/// Cuánto esperamos antes de devolver el portapapeles original, para un texto corto.
const RESTORE_AFTER: Duration = Duration::from_millis(600);
/// A partir de esta longitud el dictado se queda en el portapapeles: el Bloc de notas de Windows 11
/// lee el portapapeles tarde y, si se restauraba a los 600 ms, pegaba lo de antes o nada (16-oct-2026,
/// dictado de 318 palabras). Así, si el pegado falla, basta con Ctrl+V.
pub const KEEP_IN_CLIPBOARD_CHARS: usize = 600;

/// Espera antes de restaurar: crece con el texto (1 ms por cada 4 caracteres), hasta 3 s.
fn restore_delay(chars: usize) -> Duration {
    RESTORE_AFTER.max(Duration::from_millis((400 + chars / 4) as u64)).min(Duration::from_secs(3))
}

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

/// La ventana con foco como número (0 = ninguna). La vista previa de un Transform (decisión A de
/// Txonso, 23-oct) recuerda dónde se seleccionó el texto y solo pega si esa ventana sigue delante:
/// pegar en otra (p. ej. una terminal) con un Enter que era para ella sería peor que no pegar.
pub fn foreground_window() -> isize {
    // SAFETY: GetForegroundWindow no tiene precondiciones.
    unsafe { GetForegroundWindow() }.0 as isize
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

// ------------------------------------------------------------------ portapapeles entero

// Formatos estándar (winuser.h).
const CF_BITMAP: u32 = 2;
const CF_METAFILEPICT: u32 = 3;
const CF_PALETTE: u32 = 9;
const CF_UNICODETEXT: u32 = 13;
const CF_ENHMETAFILE: u32 = 14;
const CF_OWNERDISPLAY: u32 = 0x80;
const CF_DSPBITMAP: u32 = 0x82;
const CF_DSPMETAFILEPICT: u32 = 0x83;
const CF_DSPENHMETAFILE: u32 = 0x8E;

/// Contenido del portapapeles: cada formato con los bytes de su memoria global, en el orden en que
/// Windows los enumera (del más rico al más pobre).
type Clip = Vec<(u32, Vec<u8>)>;

/// ¿El formato se guarda copiando su HGLOBAL? No lo son los de handle GDI o de metarchivo (Windows
/// sintetiza CF_BITMAP y compañía desde CF_DIB, que sí se guarda), ni los privados y de «owner
/// display», que solo entiende la app que los puso.
fn copyable_format(f: u32) -> bool {
    !matches!(
        f,
        CF_BITMAP | CF_METAFILEPICT | CF_PALETTE | CF_ENHMETAFILE | CF_OWNERDISPLAY | CF_DSPBITMAP | CF_DSPMETAFILEPICT | CF_DSPENHMETAFILE | 0x200..=0x3FF
    )
}

/// Guarda del portapapeles cada formato copiable (`read` lee sus bytes; `None` = no se pudo).
fn save_formats(available: impl IntoIterator<Item = u32>, mut read: impl FnMut(u32) -> Option<Vec<u8>>) -> Clip {
    let mut out: Clip = Vec::new();
    for f in available {
        if copyable_format(f) && !out.iter().any(|(g, _)| *g == f) {
            if let Some(bytes) = read(f) {
                out.push((f, bytes));
            }
        }
    }
    out
}

/// Vuelve a poner cada formato guardado (`write` lo escribe). Devuelve los que no se pudieron.
fn restore_formats(saved: &[(u32, Vec<u8>)], mut write: impl FnMut(u32, &[u8]) -> bool) -> Vec<u32> {
    saved.iter().filter(|(f, bytes)| !write(*f, bytes)).map(|(f, _)| *f).collect()
}

/// Lo que Pronto pone en el portapapeles: el texto (UTF-16 con su NUL) y las marcas (`marks`, los
/// formatos registrados de `exclusion_marks`) con un DWORD 0, para que no entre en el historial
/// Win+V, no suba al portapapeles en la nube ni lo procesen los monitores (regla 1, M4 / PRNT-39).
/// Un NUL dentro del texto se quita: CF_UNICODETEXT termina en el primer NUL y la app destino solo
/// pegaría lo de antes (rompedor 22-oct-c, BAJA 8).
fn text_formats(text: &str, marks: &[u32]) -> Clip {
    let mut clip = vec![(CF_UNICODETEXT, text.encode_utf16().filter(|&u| u != 0).chain([0]).flat_map(u16::to_le_bytes).collect())];
    clip.extend(marks.iter().map(|&m| (m, 0u32.to_le_bytes().to_vec())));
    clip
}

/// Formatos registrados de exclusión del historial, la nube y los monitores del portapapeles.
fn exclusion_marks() -> Vec<u32> {
    // SAFETY: cadenas literales terminadas en NUL.
    let ids = unsafe {
        [
            RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing")),
            RegisterClipboardFormatW(w!("CanIncludeInClipboardHistory")),
            RegisterClipboardFormatW(w!("CanUploadToCloudClipboard")),
        ]
    };
    ids.into_iter().filter(|&id| id != 0).collect()
}

/// Portapapeles abierto; se cierra al soltarlo. Reintenta un poco: otra app puede tenerlo abierto
/// un instante (igual que arboard).
struct OpenClip;

impl OpenClip {
    fn new() -> Result<Self, String> {
        for i in 0..10u64 {
            // SAFETY: sin ventana propietaria, como arboard; se cierra en `Drop`.
            if unsafe { OpenClipboard(None) }.is_ok() {
                return Ok(Self);
            }
            std::thread::sleep(Duration::from_millis(5 + 5 * i));
        }
        Err("el portapapeles está ocupado".into())
    }
}

impl Drop for OpenClip {
    fn drop(&mut self) {
        // SAFETY: lo abrimos en `new`.
        let _ = unsafe { CloseClipboard() };
    }
}

/// Bytes de la memoria global de un formato. SAFETY: con el portapapeles abierto y `f` copiable.
unsafe fn read_hglobal(f: u32) -> Option<Vec<u8>> {
    let h = unsafe { GetClipboardData(f) }.ok()?;
    let g = HGLOBAL(h.0);
    let size = unsafe { GlobalSize(g) };
    if size == 0 {
        return Some(Vec::new());
    }
    let p = unsafe { GlobalLock(g) } as *const u8;
    if p.is_null() {
        return None;
    }
    // SAFETY: `p` apunta a `size` bytes bloqueados hasta `GlobalUnlock`.
    let bytes = unsafe { std::slice::from_raw_parts(p, size) }.to_vec();
    let _ = unsafe { GlobalUnlock(g) };
    Some(bytes)
}

/// Pone `bytes` como formato `f` en una memoria global nueva. SAFETY: con el portapapeles abierto y vaciado.
unsafe fn write_hglobal(f: u32, bytes: &[u8]) -> bool {
    // Un formato de 0 bytes se guarda en un bloque de 1 byte (GlobalAlloc no da 0): a cero, no con
    // lo que hubiera en esa memoria (rompedor 22-oct-c, Huecos).
    let Ok(g) = (unsafe { GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, bytes.len().max(1)) }) else { return false };
    let p = unsafe { GlobalLock(g) } as *mut u8;
    if p.is_null() {
        let _ = unsafe { GlobalFree(Some(g)) };
        return false;
    }
    // SAFETY: el bloque tiene al menos `bytes.len()` bytes.
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len()) };
    let _ = unsafe { GlobalUnlock(g) };
    // Si SetClipboardData acepta, la memoria pasa a ser del sistema.
    if unsafe { SetClipboardData(f, Some(HANDLE(g.0))) }.is_ok() {
        return true;
    }
    let _ = unsafe { GlobalFree(Some(g)) };
    false
}

/// Todo lo que hay en el portapapeles (`None` si no se pudo abrir: entonces no se restaura nada).
fn read_clipboard() -> Option<Clip> {
    let _open = OpenClip::new().ok()?;
    let mut formats = Vec::new();
    let mut f = 0;
    loop {
        // SAFETY: con el portapapeles abierto.
        f = unsafe { EnumClipboardFormats(f) };
        if f == 0 {
            break;
        }
        formats.push(f);
    }
    // SAFETY: abierto y solo formatos copiables (`save_formats` filtra).
    Some(save_formats(formats, |f| unsafe { read_hglobal(f) }))
}

/// Sustituye el portapapeles por `clip` (vacío = lo deja vacío). Devuelve los formatos que no entraron.
fn write_clipboard(clip: &[(u32, Vec<u8>)]) -> Result<Vec<u32>, String> {
    let _open = OpenClip::new()?;
    // SAFETY: abierto por nosotros.
    unsafe { EmptyClipboard() }.map_err(|e| e.to_string())?;
    // SAFETY: abierto y vaciado.
    Ok(restore_formats(clip, |f, bytes| unsafe { write_hglobal(f, bytes) }))
}

/// Pone el texto de Pronto con sus marcas de exclusión.
fn write_text(text: &str) -> Result<(), String> {
    let failed = write_clipboard(&text_formats(text, &exclusion_marks()))?;
    if failed.contains(&CF_UNICODETEXT) {
        return Err("no se pudo escribir el texto".into());
    }
    if !failed.is_empty() {
        eprintln!("[inject] marcas de exclusión que no entraron: {failed:?}");
    }
    Ok(())
}

struct Pending {
    /// El portapapeles del usuario (`None` si no se pudo leer).
    original: Option<Clip>,
    token: u64,
}

/// Restauración pendiente: si llegan dos pegados seguidos hay que devolver el portapapeles del
/// usuario, no el texto del primer pegado.
static PENDING: Mutex<Option<Pending>> = Mutex::new(None);
static TOKEN: Mutex<u64> = Mutex::new(0);

/// Devuelve el portapapeles del usuario con todos sus formatos (también las marcas de exclusión de
/// un gestor de contraseñas, M5). Si no se pudo leer, se deja como está antes que vaciarlo.
fn restore_clipboard(saved: Option<Clip>) {
    let Some(clip) = saved else { return };
    match write_clipboard(&clip) {
        Ok(failed) if failed.is_empty() => {}
        Ok(failed) => eprintln!("[inject] formatos del portapapeles que no se pudieron restaurar: {failed:?}"),
        Err(e) => eprintln!("[inject] no se pudo restaurar el portapapeles: {e}"),
    }
}

/// Pone `text` en el portapapeles (sin pegar). Si había un pegado pendiente de devolver el
/// portapapeles del usuario, se cancela: si no, a los 600 ms machacaría lo que se acaba de copiar.
/// Si no se puede escribir, el portapapeles vuelve a como estaba (no se queda vacío).
pub fn copy_text(text: &str) -> Result<(), String> {
    let pending = PENDING.lock().take();
    let original = match pending {
        Some(p) => p.original,
        None => read_clipboard(),
    };
    if let Err(e) = write_text(text) {
        restore_clipboard(original);
        return Err(e);
    }
    Ok(())
}

/// Pega `text` en la app con foco: guarda el portapapeles, escribe el texto, Ctrl+V y restaura a
/// los 600 ms (en un hilo aparte para no retrasar el resto del flujo).
pub fn paste_text(text: &str) -> Result<(), String> {
    let token = {
        let mut t = TOKEN.lock();
        *t += 1;
        *t
    };
    {
        let mut pending = PENDING.lock();
        match pending.as_mut() {
            Some(p) => p.token = token, // ya hay un original guardado: lo conservamos
            None => *pending = Some(Pending { original: read_clipboard(), token }),
        }
    }
    if let Err(e) = write_text(text) {
        // No se copió nada y `write_clipboard` pudo dejarlo vacío: el portapapeles del usuario vuelve
        // ya, sin esperar (rompedor 22-oct-c, Huecos: quedaba vacío y el aviso decía «Copiado»).
        let original = {
            let mut pending = PENDING.lock();
            if pending.as_ref().is_some_and(|p| p.token == token) {
                pending.take().and_then(|p| p.original)
            } else {
                None
            }
        };
        restore_clipboard(original);
        return Err(format!("no se pudo escribir en el portapapeles: {e}"));
    }

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

    let chars = text.chars().count();
    if chars >= KEEP_IN_CLIPBOARD_CHARS {
        // Texto largo: no se restaura; el original guardado se descarta.
        let mut pending = PENDING.lock();
        if pending.as_ref().is_some_and(|p| p.token == token) {
            *pending = None;
        }
        return Ok(());
    }
    let delay = restore_delay(chars);
    std::thread::spawn(move || {
        std::thread::sleep(delay);
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
    fn la_espera_crece_con_el_texto() {
        assert_eq!(restore_delay(20), RESTORE_AFTER);
        assert_eq!(restore_delay(400), Duration::from_millis(600));
        assert_eq!(restore_delay(500), Duration::from_millis(525).max(RESTORE_AFTER));
        assert_eq!(restore_delay(100_000), Duration::from_secs(3));
    }

    // Portapapeles de mentira: lista de (formato, bytes). Los tests no tocan el portapapeles real.
    const CF_TEXT: u32 = 1;
    const CF_OEMTEXT: u32 = 7;
    const CF_DIB: u32 = 8;
    const CF_HDROP: u32 = 15;
    const CF_LOCALE: u32 = 16;
    const CF_DIBV5: u32 = 17;
    const HTML_FORMAT: u32 = 0xC0F1; // «HTML Format» (registrado: el número cambia por sesión)
    const SHELL_IDLIST: u32 = 0xC0A2; // «Shell IDList Array», el que pone el Explorador con los ficheros
    const EXCLUDE_MONITOR: u32 = 0xC1D3; // «ExcludeClipboardContentFromMonitorProcessing» de un gestor de contraseñas

    fn fake(formats: &[u32]) -> Clip {
        formats.iter().map(|&f| (f, format!("datos de {f:#X}").into_bytes())).collect()
    }

    fn save_from(clip: &Clip) -> Clip {
        save_formats(clip.iter().map(|(f, _)| *f), |f| clip.iter().find(|(g, _)| *g == f).map(|(_, b)| b.clone()))
    }

    #[test]
    fn se_guardan_y_restauran_todos_los_formatos_de_memoria() {
        // Ficheros copiados en el Explorador, una página web (HTML) y una contraseña con su marca.
        let user = fake(&[SHELL_IDLIST, CF_HDROP, HTML_FORMAT, CF_UNICODETEXT, CF_LOCALE, CF_TEXT, CF_OEMTEXT, EXCLUDE_MONITOR]);
        let saved = save_from(&user);
        assert_eq!(saved, user, "se pierde algo de lo que el usuario tenía copiado");
        let mut back: Clip = Vec::new();
        assert!(restore_formats(&saved, |f, b| {
            back.push((f, b.to_vec()));
            true
        })
        .is_empty());
        assert_eq!(back, user, "se restaura entero y en el mismo orden");
    }

    #[test]
    fn los_formatos_de_handle_se_saltan_porque_windows_los_sintetiza() {
        // Una imagen: CF_DIB (memoria) y CF_BITMAP / CF_PALETTE / metarchivos (handles). Windows
        // vuelve a sintetizar los de handle desde CF_DIB.
        let user = fake(&[CF_DIB, CF_BITMAP, CF_DIBV5, CF_PALETTE, CF_METAFILEPICT, CF_ENHMETAFILE, CF_OWNERDISPLAY, CF_DSPBITMAP, CF_DSPMETAFILEPICT, CF_DSPENHMETAFILE, 0x200, 0x2FF, 0x300, 0x3FF]);
        let kept: Vec<u32> = save_from(&user).into_iter().map(|(f, _)| f).collect();
        assert_eq!(kept, [CF_DIB, CF_DIBV5]);
        // Un formato que no se puede leer no tira el resto; uno repetido se guarda una vez.
        let saved = save_formats([CF_HDROP, CF_UNICODETEXT, CF_UNICODETEXT], |f| (f == CF_UNICODETEXT).then(|| b"h\0".to_vec()));
        assert_eq!(saved, [(CF_UNICODETEXT, b"h\0".to_vec())]);
    }

    #[test]
    fn un_formato_que_no_se_restaura_se_cuenta() {
        let saved = fake(&[CF_HDROP, CF_UNICODETEXT]);
        assert_eq!(restore_formats(&saved, |f, _| f != CF_HDROP), [CF_HDROP]);
        assert!(restore_formats(&[], |_, _| false).is_empty(), "vacío: el portapapeles queda vacío");
    }

    #[test]
    fn el_texto_de_pronto_no_entra_en_el_historial_ni_en_la_nube() {
        let marks = [0xC101, 0xC102, 0xC103];
        let clip = text_formats("hola ñ", &marks);
        assert_eq!(clip[0], (CF_UNICODETEXT, "hola ñ\0".encode_utf16().flat_map(u16::to_le_bytes).collect::<Vec<u8>>()));
        for m in marks {
            assert_eq!(clip.iter().find(|(f, _)| *f == m).map(|(_, d)| d.as_slice()), Some(&0u32.to_le_bytes()[..]), "{m:#X}");
        }
        assert_eq!(clip.len(), 4);
    }

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
            *pending = Some(Pending { original: read_clipboard(), token: *t });
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
