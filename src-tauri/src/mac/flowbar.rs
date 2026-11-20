//! macOS: la ventana `flowbar` (hermana de las funciones `#[cfg(windows)]` de `crate::flowbar`).
//!
//! Igual que en Windows: siempre visible para el sistema y transparente; lo que se ve lo decide la UI.
//! - Ventana (AppKit, siempre en el hilo principal): `NSWindow` de Tauri (`ns_window()`) con
//!   `collectionBehavior` = todos los Escritorios + auxiliar de pantalla completa + fija + fuera de
//!   Cmd+`, nivel `NSStatusWindowLevel` (encima de las apps a pantalla completa), sin sombra, sin
//!   ocultarse al desactivar Pronto. Se muestra con `orderFrontRegardless`, que NO activa la app ni
//!   hace la ventana principal: nunca se llama a `set_focus` ni `makeKeyAndOrderFront` (la Flow bar no
//!   es un NSPanel y activarla robaría el foco a la app donde se va a pegar).
//! - Sitio: abajo al centro de `NSScreen.visibleFrame` (sin Dock ni barra de menús), justo encima
//!   del Dock como en Windows sobre la barra de tareas.
//! - Monitores: CoreGraphics (`CGGetActiveDisplayList`, `CGDisplayBounds`), que vale desde cualquier
//!   hilo. Clave de cada monitor: `mac-display-<CGDirectDisplayID>` (estable al reconectar el mismo
//!   monitor); rectángulo en puntos globales con origen arriba a la izquierda de la pantalla principal.
//! - Clics: un sondeo a ~30 Hz mira el cursor (`mouseLocationOutsideOfEventStream`) contra la zona
//!   pulsable que manda la UI y cambia `setIgnoresMouseEvents` (lo mismo que hace
//!   `set_ignore_cursor_events` de Tauri, pero ya estando en el hilo principal); cuenta el hover a la UI.
//!
//! Si algo de AppKit falla (ventana no encontrada, hilo principal ocupado), esa parte se salta y se
//! apunta en `errlog`; nunca un pánico.

use crate::errlog;
use crate::flowbar::{number_in_order, resolve_pin, Anchor, HITBOX, HOVER, PIN_CHECK_TICKS, SIZE};
use crate::models::{MonitorInfo, MonitorRect};
use crate::state::AppState;
use objc2::rc::Retained;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSEvent, NSScreen, NSStatusWindowLevel, NSWindow, NSWindowCollectionBehavior};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_core_graphics::{CGDirectDisplayID, CGDisplayBounds, CGDisplayCopyDisplayMode, CGDisplayMode, CGError, CGGetActiveDisplayList, CGMainDisplayID};
use objc2_foundation::{ns_string, NSNumber};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

const LABEL: &str = "flowbar";
/// Prefijo de la clave de monitor en Mac (lo que se guarda en `flowBarMonitor`).
const DEVICE_PREFIX: &str = "mac-display-";
/// Cuánto se espera al hilo principal cuando hace falta una respuesta.
const MAIN_WAIT: Duration = Duration::from_millis(1500);
/// Hay una vuelta del sondeo pendiente en el hilo principal: no se encola otra.
static TICK_PENDING: AtomicBool = AtomicBool::new(false);

// ------------------------------------------------------------------ hilo principal

/// Ejecuta `f` en el hilo principal sin esperar (en orden con las demás). Si ya estamos en él, ahora.
fn on_main(app: &AppHandle, f: impl FnOnce(MainThreadMarker) + Send + 'static) {
    if let Some(mtm) = MainThreadMarker::new() {
        f(mtm);
        return;
    }
    let r = app.run_on_main_thread(move || match MainThreadMarker::new() {
        Some(mtm) => f(mtm),
        None => errlog::error("flowbar", "run_on_main_thread no corrió en el hilo principal"),
    });
    if let Err(e) = r {
        errlog::error("flowbar", format!("no se pudo pasar al hilo principal: {e}"));
    }
}

/// Como `on_main`, esperando el resultado (con tope: si el hilo principal está ocupado, `None`).
fn on_main_wait<R: Send + 'static>(app: &AppHandle, f: impl FnOnce(MainThreadMarker) -> R + Send + 'static) -> Option<R> {
    if let Some(mtm) = MainThreadMarker::new() {
        return Some(f(mtm));
    }
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let r = app.run_on_main_thread(move || {
        if let Some(mtm) = MainThreadMarker::new() {
            let _ = tx.send(f(mtm));
        }
    });
    if let Err(e) = r {
        errlog::error("flowbar", format!("no se pudo pasar al hilo principal: {e}"));
        return None;
    }
    match rx.recv_timeout(MAIN_WAIT) {
        Ok(v) => Some(v),
        Err(e) => {
            errlog::error("flowbar", format!("el hilo principal no respondió: {e}"));
            None
        }
    }
}

/// El `NSWindow` de la barra. Solo en el hilo principal (lo exige el `MainThreadMarker`).
fn with_window<R>(app: &AppHandle, _mtm: MainThreadMarker, f: impl FnOnce(&NSWindow) -> R) -> Option<R> {
    let win = app.get_webview_window(LABEL)?;
    let ptr = match win.ns_window() {
        Ok(p) if !p.is_null() => p,
        Ok(_) => return None,
        Err(e) => {
            errlog::error("flowbar", format!("sin NSWindow: {e}"));
            return None;
        }
    };
    // SAFETY: Tauri devuelve el NSWindow vivo de esta ventana (lo retiene mientras exista la ventana)
    // y estamos en el hilo principal, como pide AppKit.
    let ns: &NSWindow = unsafe { &*(ptr as *const NSWindow) };
    Some(f(ns))
}

// ------------------------------------------------------------------ ventana

/// Comportamiento de la barra: en todos los Escritorios y encima de las apps a pantalla completa,
/// fuera de Cmd+` y de Exposé, sin sombra y sin esconderse al desactivar u ocultar Pronto.
fn configure(ns: &NSWindow) {
    ns.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
    ns.setLevel(NSStatusWindowLevel);
    ns.setHidesOnDeactivate(false);
    ns.setCanHide(false);
    ns.setHasShadow(false);
}

/// Configura la ventana, la coloca, la muestra sin activarla y arranca el sondeo. Una vez al arrancar.
/// `_visible_at_rest` lo resuelve la UI con los ajustes, como en Windows.
pub fn init(app: &AppHandle, _visible_at_rest: bool) {
    let a = app.clone();
    on_main(app, move |mtm| {
        let done = with_window(&a, mtm, |ns| {
            configure(ns);
            // Sin zona pulsable todavía: los clics atraviesan la ventana.
            ns.setIgnoresMouseEvents(true);
        });
        if done.is_none() {
            errlog::error("flowbar", "ventana flowbar no encontrada");
        }
    });
    place(app, Anchor::Cursor);
    show(app);
    spawn_cursor_poller(app.clone());
}

/// Se asegura de que la ventana está visible y encima de todo, sin activarla.
pub fn show(app: &AppHandle) {
    let a = app.clone();
    on_main(app, move |mtm| {
        with_window(&a, mtm, |ns| {
            // Se reaplica por si algo (tao, un cambio de Escritorio) lo hubiera cambiado.
            configure(ns);
            ns.orderFrontRegardless();
        });
    });
}

/// Coloca la ventana abajo al centro de su pantalla (la fijada o la del cursor).
pub fn place(app: &AppHandle, anchor: Anchor) {
    let a = app.clone();
    on_main(app, move |mtm| {
        let Some(screen) = target_screen(&a, mtm, anchor) else { return };
        let want = bar_frame(screen.visibleFrame());
        with_window(&a, mtm, |ns| move_to(ns, want));
    });
}

/// Mueve la ventana (coordenadas de Cocoa) si no está ya ahí. `setFrame:display:` no activa.
fn move_to(ns: &NSWindow, want: CGRect) {
    if !same_frame(ns.frame(), want) {
        ns.setFrame_display(want, true);
    }
}

fn same_frame(a: CGRect, b: CGRect) -> bool {
    let near = |x: f64, y: f64| (x - y).abs() < 0.5;
    near(a.origin.x, b.origin.x) && near(a.origin.y, b.origin.y) && near(a.size.width, b.size.width) && near(a.size.height, b.size.height)
}

/// Marco de la barra (puntos de Cocoa, origen abajo a la izquierda): centrada en horizontal y con el
/// borde inferior en el del área visible (justo encima del Dock). El margen de 8 px lo lleva la página.
fn bar_frame(visible: CGRect) -> CGRect {
    let (w, h) = SIZE;
    let x = (visible.origin.x + (visible.size.width - w) / 2.0).round();
    let y = visible.origin.y.round();
    CGRect { origin: CGPoint { x, y }, size: CGSize { width: w, height: h } }
}

/// Pantalla donde va la barra: la fijada si sigue conectada; si no, la del cursor; si no, la principal.
fn target_screen(app: &AppHandle, mtm: MainThreadMarker, anchor: Anchor) -> Option<Retained<NSScreen>> {
    let screens = NSScreen::screens(mtm);
    if let Some(id) = pinned_display(app) {
        if let Some(s) = screens.iter().find(|s| screen_id(s) == Some(id)) {
            return Some(s);
        }
    }
    match anchor {
        Anchor::Cursor => {
            let p = NSEvent::mouseLocation();
            let inside = |r: CGRect| {
                p.x >= r.origin.x && p.x <= r.origin.x + r.size.width && p.y >= r.origin.y && p.y <= r.origin.y + r.size.height
            };
            screens.iter().find(|s| inside(s.frame())).or_else(|| NSScreen::mainScreen(mtm)).or_else(|| screens.iter().next())
        }
    }
}

/// `CGDirectDisplayID` de una pantalla (`deviceDescription["NSScreenNumber"]`, disponible en todo macOS;
/// `NSScreen.CGDirectDisplayID` es solo de sistemas muy nuevos).
fn screen_id(screen: &NSScreen) -> Option<CGDirectDisplayID> {
    let desc = screen.deviceDescription();
    let n = desc.objectForKey(ns_string!("NSScreenNumber"))?;
    let n = n.downcast::<NSNumber>().ok()?;
    Some(n.unsignedIntValue())
}

/// Con la barra fijada, la devuelve a su sitio si no está en él (se reconectó el monitor, cambió la
/// resolución o macOS la movió al desconectarlo). Siguiendo al cursor no hace nada.
fn keep_pinned(app: &AppHandle, mtm: MainThreadMarker, ns: &NSWindow) {
    let Some(id) = pinned_display(app) else { return };
    let Some(screen) = NSScreen::screens(mtm).iter().find(|s| screen_id(s) == Some(id)) else { return };
    move_to(ns, bar_frame(screen.visibleFrame()));
}

// ------------------------------------------------------------------ monitores y barra fija

/// Un monitor conectado según CoreGraphics.
struct Display {
    id: CGDirectDisplayID,
    rect: MonitorRect,
    primary: bool,
    /// Resolución en px físicos (la del modo actual; en Retina, el doble que los puntos).
    pixels: (i32, i32),
}

fn device_of(id: CGDirectDisplayID) -> String {
    format!("{DEVICE_PREFIX}{id}")
}

fn rect_of(b: CGRect) -> MonitorRect {
    let (l, t) = (b.origin.x.round() as i32, b.origin.y.round() as i32);
    MonitorRect { left: l, top: t, right: l + b.size.width.round() as i32, bottom: t + b.size.height.round() as i32 }
}

/// Monitores activos (CoreGraphics: vale desde cualquier hilo).
fn displays() -> Vec<Display> {
    let mut ids = [0 as CGDirectDisplayID; 32];
    let mut n = 0u32;
    // SAFETY: búfer local de `ids.len()` entradas; CoreGraphics escribe como mucho eso y dice cuántas.
    let err = unsafe { CGGetActiveDisplayList(ids.len() as u32, ids.as_mut_ptr(), &mut n) };
    if err != CGError::Success {
        errlog::error("flowbar", format!("CGGetActiveDisplayList: {}", err.0));
        return Vec::new();
    }
    let main = CGMainDisplayID();
    ids[..(n as usize).min(ids.len())]
        .iter()
        .map(|&id| {
            let rect = rect_of(CGDisplayBounds(id));
            let points = (rect.right - rect.left, rect.bottom - rect.top);
            let pixels = CGDisplayCopyDisplayMode(id)
                .map(|m| (CGDisplayMode::pixel_width(Some(&*m)) as i32, CGDisplayMode::pixel_height(Some(&*m)) as i32))
                .filter(|(w, h)| *w > 0 && *h > 0)
                .unwrap_or(points);
            Display { id, rect, primary: id == main, pixels }
        })
        .collect()
}

/// El monitor fijado en los ajustes, si hay uno y sigue conectado.
fn pinned_display(app: &AppHandle) -> Option<CGDirectDisplayID> {
    let (device, rect) = {
        let st = app.try_state::<Arc<AppState>>()?;
        let s = st.settings.read();
        (s.flow_bar_monitor.clone()?, s.flow_bar_monitor_rect)
    };
    let list = displays();
    let names: Vec<String> = list.iter().map(|d| device_of(d.id)).collect();
    let keys: Vec<(&str, MonitorRect)> = names.iter().map(String::as_str).zip(list.iter().map(|d| d.rect)).collect();
    resolve_pin(&device, rect, &keys).map(|i| list[i].id)
}

/// Monitor donde está ahora la ventana de la barra: clave y rectángulo.
pub fn current_monitor(app: &AppHandle) -> Option<(String, MonitorRect)> {
    let a = app.clone();
    let id = on_main_wait(app, move |mtm| with_window(&a, mtm, |ns| ns.screen().and_then(|s| screen_id(&s))).flatten()).flatten()?;
    let d = displays().into_iter().find(|d| d.id == id)?;
    Some((device_of(d.id), d.rect))
}

/// Rectángulo actual del monitor con esa clave, si está conectado.
pub fn monitor_rect(device: &str) -> Option<MonitorRect> {
    displays().into_iter().find(|d| device_of(d.id).eq_ignore_ascii_case(device)).map(|d| d.rect)
}

/// Nombre de cada pantalla (`NSScreen.localizedName`, macOS 10.15+) por su id. Vacío si el hilo
/// principal no responde: la UI se queda con «Pantalla N».
fn screen_names(app: &AppHandle) -> Vec<(CGDirectDisplayID, String)> {
    on_main_wait(app, |mtm| {
        NSScreen::screens(mtm)
            .iter()
            .filter_map(|s| Some((screen_id(&s)?, s.localizedName().to_string().trim().to_string())))
            .filter(|(_, n)| !n.is_empty())
            .collect()
    })
    .unwrap_or_default()
}

/// Monitores conectados para la lista de Ajustes: la principal primero y luego de izquierda a derecha.
pub fn list_monitors(app: &AppHandle) -> Vec<MonitorInfo> {
    let names = screen_names(app);
    let current = current_monitor(app).map(|(d, _)| d);
    let mut list = displays();
    list.sort_by_key(|d| (!d.primary, d.rect.left, d.rect.top));
    let mut out: Vec<MonitorInfo> = list
        .into_iter()
        .enumerate()
        .map(|(i, d)| {
            let device = device_of(d.id);
            MonitorInfo {
                number: i as u32 + 1,
                name: names.iter().find(|(id, _)| *id == d.id).map(|(_, n)| n.clone()).unwrap_or_default(),
                primary: d.primary,
                width: d.pixels.0,
                height: d.pixels.1,
                current: current.as_deref() == Some(device.as_str()),
                device,
            }
        })
        .collect();
    number_in_order(&mut out);
    out
}

// ------------------------------------------------------------------ sondeo del cursor

/// ¿Está el cursor sobre la zona pulsable? (px CSS = puntos, desde la esquina superior izquierda).
fn cursor_in_hitbox(ns: &NSWindow) -> bool {
    let Some(rect) = *HITBOX.lock() else { return false };
    // Coordenadas de la ventana: origen abajo a la izquierda.
    let p = ns.mouseLocationOutsideOfEventStream();
    let x = p.x;
    let y = ns.frame().size.height - p.y;
    x >= rect.x && x <= rect.x + rect.width && y >= rect.y && y <= rect.y + rect.height
}

/// Una vuelta del sondeo, en el hilo principal.
fn poll_once(app: &AppHandle, mtm: MainThreadMarker, check_pin: bool) {
    let inside = with_window(app, mtm, |ns| {
        if check_pin {
            keep_pinned(app, mtm, ns);
        }
        let inside = cursor_in_hitbox(ns);
        // Sobre la píldora la ventana recibe el ratón; fuera, los clics la atraviesan.
        if ns.ignoresMouseEvents() == inside {
            ns.setIgnoresMouseEvents(!inside);
        }
        inside
    });
    let Some(inside) = inside else { return };
    // Con la ventana transparente a los clics el webview no recibe `mouseleave`: el hover lo decide
    // este sondeo y se lo cuenta a la UI.
    if HOVER.swap(inside, Ordering::SeqCst) != inside {
        let _ = app.emit_to(LABEL, "flowbar://hover", serde_json::json!({ "hover": inside }));
    }
}

/// Deja `TICK_PENDING` libre al acabar la vuelta, también si algo falla por el camino.
struct TickDone;
impl Drop for TickDone {
    fn drop(&mut self) {
        TICK_PENDING.store(false, Ordering::SeqCst);
    }
}

/// Sondea el cursor a ~30 Hz. AppKit solo se toca en el hilo principal: cada vuelta se encola allí
/// (y no se encola otra mientras la anterior no haya corrido).
fn spawn_cursor_poller(app: AppHandle) {
    let mut tick = 0u32;
    let spawned = std::thread::Builder::new().name("pronto-flowbar-poll".into()).spawn(move || loop {
        std::thread::sleep(Duration::from_millis(33));
        if TICK_PENDING.swap(true, Ordering::SeqCst) {
            continue;
        }
        tick = tick.wrapping_add(1);
        let check_pin = tick % PIN_CHECK_TICKS == 0;
        let a = app.clone();
        let r = app.run_on_main_thread(move || {
            let _done = TickDone;
            if let Some(mtm) = MainThreadMarker::new() {
                poll_once(&a, mtm, check_pin);
            }
        });
        if r.is_err() {
            TICK_PENDING.store(false, Ordering::SeqCst);
        }
    });
    if let Err(e) = spawned {
        errlog::error("flowbar", format!("no se pudo lanzar el sondeo del cursor: {e}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_barra_va_abajo_al_centro_del_area_visible() {
        // Pantalla de 1440x900 con barra de menús (25) y Dock abajo (70): visible y=70, alto 805.
        let visible = CGRect { origin: CGPoint { x: 0.0, y: 70.0 }, size: CGSize { width: 1440.0, height: 805.0 } };
        let f = bar_frame(visible);
        assert_eq!((f.size.width, f.size.height), SIZE);
        assert_eq!(f.origin.x, ((1440.0 - SIZE.0) / 2.0_f64).round());
        assert_eq!(f.origin.y, 70.0);
        // Segunda pantalla a la izquierda (x negativa).
        let left = CGRect { origin: CGPoint { x: -1920.0, y: 0.0 }, size: CGSize { width: 1920.0, height: 1055.0 } };
        assert_eq!(bar_frame(left).origin.x, -1920.0 + ((1920.0 - SIZE.0) / 2.0_f64).round());
    }

    #[test]
    fn claves_de_monitor() {
        assert_eq!(device_of(69733378), "mac-display-69733378");
        let r = rect_of(CGRect { origin: CGPoint { x: -1920.0, y: 0.0 }, size: CGSize { width: 1920.0, height: 1080.0 } });
        assert_eq!(r, MonitorRect { left: -1920, top: 0, right: 0, bottom: 1080 });
    }
}
