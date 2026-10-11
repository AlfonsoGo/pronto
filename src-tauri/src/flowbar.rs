//! La ventana `flowbar`: píldora flotante abajo al centro del monitor de trabajo.
//!
//! Igual que la ventana «status» de Wispr: **siempre visible para el sistema** y transparente; lo
//! que se ve o no lo decide la UI (en reposo oculto no pinta nada). Así nunca hay que mostrarla en
//! el momento de dictar, que es justo cuando no se puede robar el foco a la app destino.
//!
//! 🔴 No usar aquí las APIs de ventana de Tauri que cambian estilos (`show`, `hide`,
//! `set_ignore_cursor_events`, `set_focusable`…): tao recalcula entonces todos los estilos desde
//! su estado interno y los reaplica con `ShowWindow(SW_SHOW)`, que puede activar la ventana y
//! quitarle el foco a la app donde se va a pegar. Todo va con Win32 directo.
use crate::models::Rect;
use parking_lot::Mutex;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MonitorFromWindow, HMONITOR, MONITORINFO, MONITOR_DEFAULTTONEAREST};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetWindowLongPtrW, GetWindowRect, IsWindowVisible, SetWindowLongPtrW, SetWindowPos, ShowWindow, GWL_EXSTYLE,
    HWND_TOPMOST, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SW_SHOWNOACTIVATE, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
};

/// Tamaño fijo (lógico) de la ventana: la píldora y sus tarjetas viven dentro, el resto es hueco
/// transparente que no debe bloquear clics (ver el sondeo de abajo).
pub const SIZE: (f64, f64) = (360.0, 150.0);

/// Zona pulsable informada por la UI (px CSS relativos a la ventana). `None` = nada pulsable.
static HITBOX: Mutex<Option<Rect>> = Mutex::new(None);
/// Último hover comunicado a la UI.
static HOVER: AtomicBool = AtomicBool::new(false);

pub fn set_hitbox(rect: Option<Rect>) {
    *HITBOX.lock() = rect;
}

/// ¿Debe verse la barra en reposo? (`showFlowBar` y no estar oculta temporalmente).
pub fn rest_visible(s: &crate::models::Settings) -> bool {
    let hidden = s
        .flow_bar_hidden_until
        .as_deref()
        .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
        .is_some_and(|until| until > chrono::Local::now());
    s.show_flow_bar && !hidden
}

#[derive(Clone, Copy)]
pub enum Anchor {
    /// Monitor donde está el cursor (al empezar a dictar).
    Cursor,
}

fn hwnd(app: &AppHandle) -> Option<HWND> {
    let win = app.get_webview_window("flowbar")?;
    let h = win.hwnd().ok()?;
    Some(HWND(h.0 as *mut c_void))
}

/// Estilos que impiden activarse y la ocultan de Alt+Tab, posición y sondeo. Una vez al arrancar.
/// La ventana nace visible y sin foco (`tauri.conf.json`: `visible`, `focus: false`,
/// `focusable: false`); `_visible_at_rest` lo resuelve la UI con los ajustes.
pub fn init(app: &AppHandle, _visible_at_rest: bool) {
    let Some(h) = hwnd(app) else {
        eprintln!("[flowbar] ventana flowbar no encontrada");
        return;
    };
    // SAFETY: h es el HWND vivo de nuestra ventana.
    unsafe {
        let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
        let ex = ex | WS_EX_NOACTIVATE.0 as isize | WS_EX_TOOLWINDOW.0 as isize | click_through_bits();
        SetWindowLongPtrW(h, GWL_EXSTYLE, ex);
        let _ = SetWindowPos(h, None, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED);
    }
    place(app, Anchor::Cursor);
    show(app);
    spawn_cursor_poller(app.clone());
}

/// Se asegura de que la ventana está visible y encima de todo, sin activarla. Lo que se pinta lo
/// decide la UI.
pub fn show(app: &AppHandle) {
    if let Some(h) = hwnd(app) {
        // SAFETY: h es el HWND vivo de nuestra ventana.
        unsafe {
            if !IsWindowVisible(h).as_bool() {
                let _ = ShowWindow(h, SW_SHOWNOACTIVATE);
            }
            let _ = SetWindowPos(h, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }
}

/// No oculta la ventana: con la barra vacía la UI ya no pinta nada y el hitbox es `None`, así que
/// es invisible y transparente a los clics. Se deja para que el controlador exprese la intención.
pub fn hide(_app: &AppHandle) {}

fn click_through_bits() -> isize {
    (WS_EX_TRANSPARENT.0 | WS_EX_LAYERED.0) as isize
}

fn monitor_of(anchor: Anchor, h: HWND) -> HMONITOR {
    // SAFETY: llamadas Win32 con datos locales válidos.
    unsafe {
        match anchor {
            Anchor::Cursor => {
                let mut pt = POINT::default();
                if GetCursorPos(&mut pt).is_err() {
                    return MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST);
                }
                MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST)
            }
        }
    }
}

/// Área de trabajo (sin barra de tareas) del monitor y su factor de escala.
fn work_area(mon: HMONITOR) -> Option<(RECT, f64)> {
    let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    // SAFETY: info está inicializada con su tamaño.
    if !unsafe { GetMonitorInfoW(mon, &mut info) }.as_bool() {
        return None;
    }
    let (mut dx, mut dy) = (96u32, 96u32);
    // SAFETY: punteros a locales válidos.
    let _ = unsafe { GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy) };
    Some((info.rcWork, dx.max(96) as f64 / 96.0))
}

/// Coloca la ventana abajo al centro del área de trabajo (justo sobre la barra de tareas; el
/// margen inferior de 8 px lo lleva la propia píldora dentro de la ventana).
pub fn place(app: &AppHandle, anchor: Anchor) {
    let Some(h) = hwnd(app) else { return };
    let Some((work, scale)) = work_area(monitor_of(anchor, h)) else { return };
    let (lw, lh) = SIZE;
    let (w, hgt) = ((lw * scale).round() as i32, (lh * scale).round() as i32);
    let x = work.left + ((work.right - work.left) - w) / 2;
    let y = work.bottom - hgt;
    // SAFETY: h es el HWND vivo de nuestra ventana.
    unsafe {
        let _ = SetWindowPos(h, Some(HWND_TOPMOST), x, y, w, hgt, SWP_NOACTIVATE);
    }
}

/// Sondea el cursor a ~30 Hz: sobre la zona pulsable la ventana recibe el ratón; fuera, los
/// clics la atraviesan. Se mira el estilo real en cada vuelta (no un valor cacheado) porque tao
/// puede reescribir los estilos por su cuenta (p. ej. al cambiar de DPI).
fn spawn_cursor_poller(app: AppHandle) {
    let spawned = std::thread::Builder::new().name("pronto-flowbar-poll".into()).spawn(move || loop {
        std::thread::sleep(Duration::from_millis(33));
        let Some(h) = hwnd(&app) else { continue };
        let inside = cursor_in_hitbox(&app);
        // SAFETY: h es el HWND vivo de nuestra ventana.
        unsafe {
            let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
            let want = if inside { ex & !click_through_bits() } else { ex | click_through_bits() };
            if want != ex {
                SetWindowLongPtrW(h, GWL_EXSTYLE, want);
            }
        }
        // Con la ventana transparente a los clics el webview no recibe `mouseleave`: el hover lo
        // decide este sondeo y se lo cuenta a la UI.
        if HOVER.swap(inside, Ordering::SeqCst) != inside {
            let _ = app.emit_to("flowbar", "flowbar://hover", serde_json::json!({ "hover": inside }));
        }
    });
    if let Err(e) = spawned {
        eprintln!("[flowbar] no se pudo lanzar el sondeo del cursor: {e}");
    }
}

fn cursor_in_hitbox(app: &AppHandle) -> bool {
    let Some(rect) = *HITBOX.lock() else { return false };
    let Some(h) = hwnd(app) else { return false };
    let mut pt = POINT::default();
    let mut win = RECT::default();
    // SAFETY: punteros a locales válidos; h es el HWND vivo de nuestra ventana.
    let (scale, ok) = unsafe {
        let ok = GetCursorPos(&mut pt).is_ok() && GetWindowRect(h, &mut win).is_ok();
        (GetDpiForWindow(h).max(96) as f64 / 96.0, ok)
    };
    if !ok {
        return false;
    }
    // A px CSS (lógicos) relativos a la esquina superior izquierda de la ventana.
    let x = (pt.x - win.left) as f64 / scale;
    let y = (pt.y - win.top) as f64 / scale;
    x >= rect.x && x <= rect.x + rect.width && y >= rect.y && y <= rect.y + rect.height
}
