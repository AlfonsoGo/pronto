//! La ventana `flowbar`: píldora flotante abajo al centro del monitor de trabajo.
//!
//! Si el usuario la fija con el candado (`flowBarMonitor`), se queda en ese monitor aunque se dicte
//! en otro; si ese monitor no está conectado, vuelve a seguir al cursor sin borrar el ajuste.
//!
//! Igual que la ventana «status» de Wispr: **siempre visible para el sistema** y transparente; lo
//! que se ve o no lo decide la UI (en reposo oculto no pinta nada). Así nunca hay que mostrarla en
//! el momento de dictar, que es justo cuando no se puede robar el foco a la app destino.
//!
//! 🔴 No usar aquí las APIs de ventana de Tauri que cambian estilos (`show`, `hide`,
//! `set_ignore_cursor_events`, `set_focusable`…): tao recalcula entonces todos los estilos desde
//! su estado interno y los reaplica con `ShowWindow(SW_SHOW)`, que puede activar la ventana y
//! quitarle el foco a la app donde se va a pegar. Todo va con Win32 directo.
use crate::models::{MonitorInfo, MonitorRect, Rect};
use crate::state::AppState;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use windows::Win32::Devices::Display::{
    DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QueryDisplayConfig, DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
    DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME, DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO,
    DISPLAYCONFIG_SOURCE_DEVICE_NAME, DISPLAYCONFIG_TARGET_DEVICE_NAME, QDC_ONLY_ACTIVE_PATHS,
};
use windows::Win32::Foundation::{ERROR_SUCCESS, HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, MonitorFromPoint, MonitorFromWindow, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
    MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetWindowLongPtrW, GetWindowRect, IsWindowVisible, SetWindowLongPtrW, SetWindowPos, ShowWindow, GWL_EXSTYLE,
    HWND_TOPMOST, MONITORINFOF_PRIMARY, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SW_SHOWNOACTIVATE,
    WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
};
use windows::core::BOOL;

/// Tamaño fijo (lógico) de la ventana: la píldora y sus tarjetas viven dentro, el resto es hueco
/// transparente que no debe bloquear clics (ver el sondeo de abajo). 300 de alto (antes 150) para que
/// quepa la vista previa de un Transform encima de la píldora (decisión A de Txonso, 23-oct): el
/// resultado se lee entero antes de pulsar Enter. Lo de más es transparente y deja pasar los clics.
/// Tiene que coincidir con `STAGE_W`/`STAGE_H` de `ui/flowbar.js` y `.stage` de `ui/flowbar.css`.
pub const SIZE: (f64, f64) = (360.0, 300.0);

/// Zona pulsable informada por la UI (px CSS relativos a la ventana). `None` = nada pulsable.
static HITBOX: Mutex<Option<Rect>> = Mutex::new(None);
/// Último hover comunicado a la UI.
static HOVER: AtomicBool = AtomicBool::new(false);
/// Cada cuántas vueltas del sondeo (~2 s) se comprueba que la barra fijada sigue en su monitor.
const PIN_CHECK_TICKS: u32 = 60;

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
    /// Monitor donde está el cursor (al empezar a dictar), salvo que la barra esté fijada en un
    /// monitor conectado: entonces manda ese (ver `target_monitor`).
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

/// Monitor donde va la barra: el fijado si sigue conectado; si no, el que pide `anchor`.
fn target_monitor(app: &AppHandle, anchor: Anchor, h: HWND) -> HMONITOR {
    if let Some(mon) = pinned_monitor(app) {
        return mon;
    }
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

/// Rectángulo (px físicos) de la ventana abajo al centro del área de trabajo de `mon` (justo sobre
/// la barra de tareas; el margen inferior de 8 px lo lleva la propia píldora dentro de la ventana).
fn bar_rect(mon: HMONITOR) -> Option<RECT> {
    let (work, scale) = work_area(mon)?;
    let (lw, lh) = SIZE;
    let (w, hgt) = ((lw * scale).round() as i32, (lh * scale).round() as i32);
    let x = work.left + ((work.right - work.left) - w) / 2;
    let y = work.bottom - hgt;
    Some(RECT { left: x, top: y, right: x + w, bottom: y + hgt })
}

fn move_to(h: HWND, r: RECT) {
    // SAFETY: h es el HWND vivo de nuestra ventana.
    unsafe {
        let _ = SetWindowPos(h, Some(HWND_TOPMOST), r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOACTIVATE);
    }
}

/// Coloca la ventana abajo al centro de su monitor (el fijado o el de `anchor`).
pub fn place(app: &AppHandle, anchor: Anchor) {
    let Some(h) = hwnd(app) else { return };
    let Some(r) = bar_rect(target_monitor(app, anchor, h)) else { return };
    move_to(h, r);
}

/// Con la barra fijada, la devuelve a su sitio si no está en él: el monitor volvió a enchufarse,
/// cambió la resolución o la escala, o Windows la movió al desconectarlo. Siguiendo al cursor no
/// hace nada (la barra se queda donde se dictó por última vez, como siempre).
fn keep_pinned(app: &AppHandle, h: HWND) {
    let Some(mon) = pinned_monitor(app) else { return };
    let Some(want) = bar_rect(mon) else { return };
    let mut now = RECT::default();
    // SAFETY: puntero a un local válido; h es el HWND vivo de nuestra ventana.
    if unsafe { GetWindowRect(h, &mut now) }.is_ok() && now != want {
        move_to(h, want);
    }
}

// ------------------------------------------------------------ monitores y barra fija

/// Un monitor conectado según Win32.
struct Monitor {
    handle: HMONITOR,
    device: String,
    rect: MonitorRect,
    primary: bool,
}

fn wide(buf: &[u16]) -> String {
    String::from_utf16_lossy(&buf[..buf.iter().position(|&c| c == 0).unwrap_or(buf.len())])
}

/// Número de la pantalla a partir de su nombre de dispositivo (`\\.\DISPLAY2` -> 2).
fn display_number(device: &str) -> u32 {
    let start = device.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    device[start..].parse().unwrap_or(0)
}

fn monitor_info(mon: HMONITOR) -> Option<Monitor> {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    // SAFETY: MONITORINFOEXW empieza por MONITORINFO y `cbSize` anuncia la versión extendida.
    if !unsafe { GetMonitorInfoW(mon, &mut info as *mut MONITORINFOEXW as *mut MONITORINFO) }.as_bool() {
        return None;
    }
    let r = info.monitorInfo.rcMonitor;
    Some(Monitor {
        handle: mon,
        device: wide(&info.szDevice),
        rect: MonitorRect { left: r.left, top: r.top, right: r.right, bottom: r.bottom },
        primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
    })
}

unsafe extern "system" fn collect_monitor(mon: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
    // SAFETY: `data` apunta al `Vec` de `monitors()`, vivo mientras dura EnumDisplayMonitors.
    let list = unsafe { &mut *(data.0 as *mut Vec<HMONITOR>) };
    list.push(mon);
    true.into()
}

/// Monitores conectados, en el orden de Win32.
fn monitors() -> Vec<Monitor> {
    let mut handles: Vec<HMONITOR> = Vec::new();
    // SAFETY: el callback solo usa el puntero a `handles`, que vive durante toda la llamada.
    let _ = unsafe { EnumDisplayMonitors(None, None, Some(collect_monitor), LPARAM(&mut handles as *mut Vec<HMONITOR> as isize)) };
    handles.into_iter().filter_map(monitor_info).collect()
}

/// Cuál de `monitors` (nombre de dispositivo y rectángulo) es el fijado. Primero el que coincide en
/// las dos cosas; si no, el que ocupa el mismo rectángulo (Windows renumera las pantallas al
/// enchufar y desenchufar); si no, el del mismo nombre (cambió la resolución o la disposición).
/// `None` = ya no está conectado.
pub fn resolve_pin(device: &str, rect: Option<MonitorRect>, monitors: &[(&str, MonitorRect)]) -> Option<usize> {
    let same_dev = |d: &str| d.eq_ignore_ascii_case(device);
    let same_rect = |r: &MonitorRect| rect.as_ref() == Some(r);
    monitors
        .iter()
        .position(|(d, r)| same_dev(d) && same_rect(r))
        .or_else(|| monitors.iter().position(|(_, r)| same_rect(r)))
        .or_else(|| monitors.iter().position(|(d, _)| same_dev(d)))
}

/// El monitor fijado en los ajustes, si hay uno y sigue conectado.
fn pinned_monitor(app: &AppHandle) -> Option<HMONITOR> {
    let (device, rect) = {
        let st = app.try_state::<Arc<AppState>>()?;
        let s = st.settings.read();
        (s.flow_bar_monitor.clone()?, s.flow_bar_monitor_rect)
    };
    let mons = monitors();
    let keys: Vec<(&str, MonitorRect)> = mons.iter().map(|m| (m.device.as_str(), m.rect)).collect();
    resolve_pin(&device, rect, &keys).map(|i| mons[i].handle)
}

/// Monitor donde está ahora la ventana de la barra: nombre de dispositivo y rectángulo.
pub fn current_monitor(app: &AppHandle) -> Option<(String, MonitorRect)> {
    let h = hwnd(app)?;
    // SAFETY: h es el HWND vivo de nuestra ventana.
    let m = monitor_info(unsafe { MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST) })?;
    Some((m.device, m.rect))
}

/// Rectángulo actual del monitor con ese nombre de dispositivo, si está conectado.
pub fn monitor_rect(device: &str) -> Option<MonitorRect> {
    monitors().into_iter().find(|m| m.device.eq_ignore_ascii_case(device)).map(|m| m.rect)
}

/// Nombre comercial (EDID) de cada monitor activo, por nombre de dispositivo GDI. Si Windows no lo
/// da (o falla la consulta) el mapa queda vacío y la UI se queda con «Pantalla N».
fn friendly_names() -> HashMap<String, String> {
    let mut out = HashMap::new();
    let (mut np, mut nm) = (0u32, 0u32);
    // SAFETY: búferes del tamaño que pide Windows; cada paquete lleva su tamaño en la cabecera.
    unsafe {
        if GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut np, &mut nm) != ERROR_SUCCESS {
            return out;
        }
        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); np as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); nm as usize];
        if QueryDisplayConfig(QDC_ONLY_ACTIVE_PATHS, &mut np, paths.as_mut_ptr(), &mut nm, modes.as_mut_ptr(), None) != ERROR_SUCCESS {
            return out;
        }
        paths.truncate(np as usize);
        for p in &paths {
            let mut src = DISPLAYCONFIG_SOURCE_DEVICE_NAME::default();
            src.header = DISPLAYCONFIG_DEVICE_INFO_HEADER {
                r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                size: std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
                adapterId: p.sourceInfo.adapterId,
                id: p.sourceInfo.id,
            };
            if DisplayConfigGetDeviceInfo(&mut src as *mut DISPLAYCONFIG_SOURCE_DEVICE_NAME as *mut DISPLAYCONFIG_DEVICE_INFO_HEADER) != 0 {
                continue;
            }
            let mut tgt = DISPLAYCONFIG_TARGET_DEVICE_NAME::default();
            tgt.header = DISPLAYCONFIG_DEVICE_INFO_HEADER {
                r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
                size: std::mem::size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32,
                adapterId: p.targetInfo.adapterId,
                id: p.targetInfo.id,
            };
            if DisplayConfigGetDeviceInfo(&mut tgt as *mut DISPLAYCONFIG_TARGET_DEVICE_NAME as *mut DISPLAYCONFIG_DEVICE_INFO_HEADER) != 0 {
                continue;
            }
            let name = wide(&tgt.monitorFriendlyDeviceName).trim().to_string();
            if !name.is_empty() {
                // Pantallas duplicadas: una misma fuente con varios monitores; vale el primero.
                out.entry(wide(&src.viewGdiDeviceName)).or_insert(name);
            }
        }
    }
    out
}

/// Monitores conectados para la lista de Ajustes (`list_monitors`), por número de pantalla.
pub fn list_monitors(app: &AppHandle) -> Vec<MonitorInfo> {
    let names = friendly_names();
    let current = current_monitor(app).map(|(d, _)| d);
    let mut list: Vec<MonitorInfo> = monitors()
        .into_iter()
        .map(|m| MonitorInfo {
            number: display_number(&m.device),
            name: names.get(&m.device).cloned().unwrap_or_default(),
            primary: m.primary,
            width: m.rect.right - m.rect.left,
            height: m.rect.bottom - m.rect.top,
            current: current.as_deref() == Some(m.device.as_str()),
            device: m.device,
        })
        .collect();
    number_in_order(&mut list);
    list
}

/// Ordena por el número de Windows y renumera 1, 2…: Windows no numera seguido (`DISPLAY7` y `DISPLAY8` con
/// solo dos pantallas) y en Ajustes salía «Pantalla 7» (fallo del 18-oct). `device` sigue siendo la clave.
fn number_in_order(list: &mut [MonitorInfo]) {
    list.sort_by(|a, b| (a.number, &a.device).cmp(&(b.number, &b.device)));
    for (i, m) in list.iter_mut().enumerate() {
        m.number = i as u32 + 1;
    }
}

/// Sondea el cursor a ~30 Hz: sobre la zona pulsable la ventana recibe el ratón; fuera, los
/// clics la atraviesan. Se mira el estilo real en cada vuelta (no un valor cacheado) porque tao
/// puede reescribir los estilos por su cuenta (p. ej. al cambiar de DPI).
fn spawn_cursor_poller(app: AppHandle) {
    let mut tick = 0u32;
    let spawned = std::thread::Builder::new().name("pronto-flowbar-poll".into()).spawn(move || loop {
        std::thread::sleep(Duration::from_millis(33));
        let Some(h) = hwnd(&app) else { continue };
        tick = tick.wrapping_add(1);
        if tick % PIN_CHECK_TICKS == 0 {
            keep_pinned(&app, h);
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Settings;

    const fn r(left: i32, top: i32, right: i32, bottom: i32) -> MonitorRect {
        MonitorRect { left, top, right, bottom }
    }
    const IZQ: MonitorRect = r(0, 0, 1920, 1080);
    const DER: MonitorRect = r(1920, 0, 4480, 1440);
    const D1: &str = r"\\.\DISPLAY1";
    const D2: &str = r"\\.\DISPLAY2";

    #[test]
    fn el_monitor_fijado_se_reencuentra() {
        let mons = [(D1, IZQ), (D2, DER)];
        // Nombre y rectángulo coinciden.
        assert_eq!(resolve_pin(D2, Some(DER), &mons), Some(1));
        // Windows renumeró las pantallas: manda el rectángulo.
        let swapped = [(D2, IZQ), (D1, DER)];
        assert_eq!(resolve_pin(D2, Some(DER), &swapped), Some(1));
        // Cambió la resolución: manda el nombre (también sin rectángulo guardado).
        let bigger = [(D1, IZQ), (D2, r(1920, 0, 5760, 2160))];
        assert_eq!(resolve_pin(D2, Some(DER), &bigger), Some(1));
        assert_eq!(resolve_pin(r"\\.\display2", None, &mons), Some(1));
        // Portátil desenchufado del monitor externo: no está y la barra sigue al cursor.
        assert_eq!(resolve_pin(D2, Some(DER), &mons[..1]), None);
        assert_eq!(resolve_pin(D2, Some(DER), &[]), None);
    }

    #[test]
    fn pantallas_numeradas_seguidas_aunque_windows_no_lo_haga() {
        let mon = |device: &str| MonitorInfo {
            number: display_number(device), name: String::new(), primary: false, width: 1920, height: 1080,
            current: false, device: device.to_string(),
        };
        let mut list = vec![mon(r"\\.\DISPLAY8"), mon(r"\\.\DISPLAY7")];
        number_in_order(&mut list);
        let got: Vec<(u32, &str)> = list.iter().map(|m| (m.number, m.device.as_str())).collect();
        assert_eq!(got, vec![(1, r"\\.\DISPLAY7"), (2, r"\\.\DISPLAY8")]);
    }

    /// La ventana y la página tienen que medir lo mismo: si no, la píldora no queda abajo al centro y
    /// el rectángulo pulsable (px CSS desde la esquina de la ventana) no cae donde se pinta.
    #[test]
    fn la_ventana_mide_lo_que_la_pagina() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ui");
        let js = std::fs::read_to_string(root.join("flowbar.js")).unwrap();
        let css = std::fs::read_to_string(root.join("flowbar.css")).unwrap();
        let (w, h) = (SIZE.0 as u32, SIZE.1 as u32);
        assert!(js.contains(&format!("const STAGE_W = {w}, STAGE_H = {h};")), "ui/flowbar.js no mide {w}x{h}");
        let stage = css.split(".stage {").nth(1).and_then(|r| r.split('}').next()).expect("falta .stage en flowbar.css");
        assert!(stage.contains(&format!("width: {w}px;")) && stage.contains(&format!("height: {h}px;")), "ui/flowbar.css .stage no mide {w}x{h}");
        // Cabe la vista previa de un Transform encima de la píldora (decisión A, 23-oct).
        assert!(h >= 260, "la vista previa no cabe en {h} px");
    }

    #[test]
    fn numero_de_pantalla() {
        assert_eq!(display_number(D2), 2);
        assert_eq!(display_number(r"\\.\DISPLAY12"), 12);
        assert_eq!(display_number("raro"), 0);
    }

    #[test]
    fn ajustes_sin_monitor_de_la_barra_cargan_igual() {
        // Ajustes de antes de la barra fija (sin los campos nuevos): sigue al cursor.
        let old: Settings = serde_json::from_str(r#"{"showFlowBar":false,"sounds":false}"#).unwrap();
        assert!(!old.show_flow_bar && !old.sounds);
        assert_eq!((old.flow_bar_monitor, old.flow_bar_monitor_rect), (None, None));
        assert_eq!(Settings::default().flow_bar_monitor, None);
        // Los campos nuevos van y vuelven en camelCase.
        let mut s = Settings::default();
        s.flow_bar_monitor = Some(D2.into());
        s.flow_bar_monitor_rect = Some(DER);
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["flowBarMonitor"], D2);
        assert_eq!(json["flowBarMonitorRect"]["right"], 4480);
        let back: Settings = serde_json::from_value(json).unwrap();
        assert_eq!((back.flow_bar_monitor.as_deref(), back.flow_bar_monitor_rect), (Some(D2), Some(DER)));
    }

    /// En el equipo real: hay al menos un monitor, uno es el principal y cada uno se reencuentra
    /// por su nombre y su rectángulo.
    #[test]
    fn monitores_reales() {
        let mons = monitors();
        assert!(!mons.is_empty());
        assert!(mons.iter().any(|m| m.primary));
        let names = friendly_names();
        for m in &mons {
            eprintln!("{} n={} {:?} principal={} nombre={:?}", m.device, display_number(&m.device), m.rect, m.primary, names.get(&m.device));
            assert_eq!(monitor_rect(&m.device), Some(m.rect));
        }
        let keys: Vec<(&str, MonitorRect)> = mons.iter().map(|m| (m.device.as_str(), m.rect)).collect();
        for (i, m) in mons.iter().enumerate() {
            assert_eq!(resolve_pin(&m.device, Some(m.rect), &keys), Some(i));
        }
    }
}
