//! Atajos globales con un hook de teclado de bajo nivel (`WH_KEYBOARD_LL`) en un hilo propio.
//!
//! `Win + Alt` (solo modificadores) no se puede registrar con `RegisterHotKey`, de ahí el hook.
//! El callback hace lo mínimo (decidir y mandar un evento por canal): Windows desinstala los
//! hooks que tardan más de ~300 ms, y la transcripción nunca debe correr aquí. Toda la lógica de
//! tiempos (debounce, doble toque) vive en el controlador (`state.rs`), no en el hook.

use crate::inject::{send_key_events, KeyEvent, MARKER};
use crate::models::{Hotkey, Settings};
use crossbeam_channel::{bounded, Sender};
use parking_lot::{Mutex, RwLock};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage, KBDLLHOOKSTRUCT, LLKHF_EXTENDED, MSG,
    WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    HoldDown,
    HoldUp,
    HandsFree,
    Cancel,
    PasteLast,
    CopyLast,
    /// Empezar / parar notas de reunión.
    Meeting,
    /// Transform N (1-based).
    Transform(u8),
}

/// Evento con la hora a la que ocurrió en el hook (no cuando lo procese el controlador): el
/// debounce de 500 ms se mide con ella, así que un arranque lento del micrófono no lo falsea.
#[derive(Debug, Clone, Copy)]
pub struct TimedEvent {
    pub ev: HotkeyEvent,
    pub at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Mods {
    ctrl: bool,
    alt: bool,
    shift: bool,
    win: bool,
}

impl Mods {
    fn contains(&self, o: &Mods) -> bool {
        (!o.ctrl || self.ctrl) && (!o.alt || self.alt) && (!o.shift || self.shift) && (!o.win || self.win)
    }
    fn any(&self) -> bool {
        self.ctrl || self.alt || self.shift || self.win
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Combo {
    mods: Mods,
    key: Option<u32>,
    /// Configurado pero con una tecla que no sabemos traducir: se desactiva.
    valid: bool,
}

#[derive(Debug, Clone, Copy, Default)]
struct Config {
    hold: Combo,
    hands_free: Combo,
    paste_last: Combo,
    copy_last: Combo,
    transform: Combo,
    meeting: Combo,
}

#[derive(Default)]
struct HookState {
    hold_active: bool,
    /// Un atajo con Alt o Win acaba de dispararse: su liberación hay que «enmascararla».
    mask_pending: bool,
    /// Teclas cuyo «down» tragamos: hay que tragar también sus repeticiones y su «up».
    swallowed: Vec<u32>,
}

/// Modo captura: la siguiente combinación pulsada y soltada se devuelve y se traga.
struct Capture {
    tx: Sender<Result<Hotkey, String>>,
    down: Vec<u32>,
    peak: Mods,
    key: Option<u32>,
}

static CONFIG: RwLock<Option<Config>> = RwLock::new(None);
static STATE: Mutex<Option<HookState>> = Mutex::new(None);
static CAPTURE: Mutex<Option<Capture>> = Mutex::new(None);
static RECORDING: AtomicBool = AtomicBool::new(false);
static TX: OnceLock<Sender<TimedEvent>> = OnceLock::new();

const VK_ESCAPE: u32 = 0x1B;
const VK_NEUTRAL: u16 = 0xE8; // tecla sin asignar: sirve para «interrumpir» Win/Alt

/// Mientras hay una grabación en curso, Esc se traga y cancela; fuera de ella pasa intacta.
pub fn set_recording(on: bool) {
    RECORDING.store(on, Ordering::SeqCst);
}

/// Recarga los atajos desde los ajustes (se llama al arrancar y al guardar).
pub fn set_config(s: &Settings) {
    let cfg = Config {
        hold: combo(&s.hotkey_hold),
        hands_free: combo(&s.hotkey_hands_free),
        paste_last: combo(&s.hotkey_paste_last),
        copy_last: combo(&s.hotkey_copy_last),
        transform: combo(&s.hotkey_transform),
        meeting: combo(&s.hotkey_meeting),
    };
    *CONFIG.write() = Some(cfg);
    // Si un cambio de atajos pilla una pulsación en curso, se suelta limpiamente.
    let mut st = STATE.lock();
    let hs = st.get_or_insert_with(HookState::default);
    if hs.hold_active {
        hs.hold_active = false;
        send(HotkeyEvent::HoldUp);
    }
}

fn combo(h: &Hotkey) -> Combo {
    let mods = Mods { ctrl: h.ctrl, alt: h.alt, shift: h.shift, win: h.win };
    match h.key.as_deref().map(str::trim).filter(|k| !k.is_empty()) {
        None => Combo { mods, key: None, valid: mods.any() },
        Some(name) => match vk_from_name(name) {
            Some(vk) => Combo { mods, key: Some(vk), valid: true },
            None => {
                eprintln!("[hotkey] tecla desconocida «{name}»: atajo desactivado");
                Combo::default()
            }
        },
    }
}

/// Traduce el nombre de tecla del contrato («Space», «Z», «F9»…) a código de tecla virtual.
pub fn vk_from_name(name: &str) -> Option<u32> {
    let n = name.trim();
    let upper = n.to_ascii_uppercase();
    if upper.chars().count() == 1 {
        let c = upper.chars().next()?;
        if c.is_ascii_uppercase() || c.is_ascii_digit() {
            return Some(c as u32);
        }
    }
    if let Some(num) = upper.strip_prefix('F').and_then(|r| r.parse::<u32>().ok()) {
        if (1..=24).contains(&num) {
            return Some(0x6F + num);
        }
    }
    Some(match upper.as_str() {
        "SPACE" | "ESPACIO" | " " => 0x20,
        "ENTER" | "RETURN" => 0x0D,
        "TAB" => 0x09,
        "BACKSPACE" => 0x08,
        "DELETE" | "DEL" => 0x2E,
        "INSERT" | "INS" => 0x2D,
        "HOME" => 0x24,
        "END" => 0x23,
        "PAGEUP" => 0x21,
        "PAGEDOWN" => 0x22,
        "ARROWLEFT" | "LEFT" => 0x25,
        "ARROWUP" | "UP" => 0x26,
        "ARROWRIGHT" | "RIGHT" => 0x27,
        "ARROWDOWN" | "DOWN" => 0x28,
        "`" | "BACKQUOTE" => 0xC0,
        "-" | "MINUS" => 0xBD,
        "=" | "EQUAL" => 0xBB,
        "," | "COMMA" => 0xBC,
        "." | "PERIOD" => 0xBE,
        "/" | "SLASH" => 0xBF,
        ";" | "SEMICOLON" => 0xBA,
        "'" | "QUOTE" => 0xDE,
        "[" | "BRACKETLEFT" => 0xDB,
        "]" | "BRACKETRIGHT" => 0xDD,
        "\\" | "BACKSLASH" => 0xDC,
        _ => return None,
    })
}

/// Inversa de `vk_from_name`, con el nombre canónico del contrato.
pub fn vk_to_name(vk: u32) -> Option<String> {
    Some(match vk {
        0x41..=0x5A | 0x30..=0x39 => char::from_u32(vk)?.to_string(),
        0x70..=0x87 => format!("F{}", vk - 0x6F),
        0x20 => "Space".into(),
        0x0D => "Enter".into(),
        0x09 => "Tab".into(),
        0x08 => "Backspace".into(),
        0x2E => "Delete".into(),
        0x2D => "Insert".into(),
        0x24 => "Home".into(),
        0x23 => "End".into(),
        0x21 => "PageUp".into(),
        0x22 => "PageDown".into(),
        0x25 => "ArrowLeft".into(),
        0x26 => "ArrowUp".into(),
        0x27 => "ArrowRight".into(),
        0x28 => "ArrowDown".into(),
        0xC0 => "`".into(),
        0xBD => "-".into(),
        0xBB => "=".into(),
        0xBC => ",".into(),
        0xBE => ".".into(),
        0xBF => "/".into(),
        0xBA => ";".into(),
        0xDE => "'".into(),
        0xDB => "[".into(),
        0xDD => "]".into(),
        0xDC => "\\".into(),
        _ => return None,
    })
}

fn send(ev: HotkeyEvent) {
    if let Some(tx) = TX.get() {
        let _ = tx.send(TimedEvent { ev, at: Instant::now() });
    }
}

/// Arranca el hilo del hook. Solo la primera llamada tiene efecto.
pub fn start(tx: Sender<TimedEvent>) {
    if TX.set(tx).is_err() {
        return;
    }
    let spawned = std::thread::Builder::new().name("pronto-hook".into()).spawn(|| unsafe {
        let module = GetModuleHandleW(None).map(|m| HINSTANCE(m.0)).ok();
        let hook = match SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), module, 0) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("[hotkey] no se pudo instalar el hook de teclado: {e}");
                return;
            }
        };
        let _keep = hook;
        // El hook solo se despacha si este hilo bombea mensajes.
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
    if let Err(e) = spawned {
        eprintln!("[hotkey] no se pudo lanzar el hilo del hook: {e}");
    }
}

/// Entra en modo captura y bloquea hasta que se pulse y suelte una combinación (la devuelve), se
/// pulse Esc (cancela) o pase `timeout`. Mientras dura, el hook se traga todas las teclas.
pub fn capture(timeout: Duration) -> Result<Hotkey, String> {
    let (tx, rx) = bounded(1);
    {
        let mut cap = CAPTURE.lock();
        if cap.is_some() {
            return Err("Ya hay una captura en curso".into());
        }
        *cap = Some(Capture { tx, down: Vec::new(), peak: Mods::default(), key: None });
    }
    let result = rx.recv_timeout(timeout).unwrap_or_else(|_| Err("Tiempo agotado".into()));
    *CAPTURE.lock() = None;
    result
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == 0 {
        // SAFETY: con HC_ACTION, lparam apunta a un KBDLLHOOKSTRUCT válido durante la llamada.
        let kb = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        // Ignoramos lo que inyectamos nosotros (Ctrl+V, tecla neutra).
        if kb.dwExtraInfo != MARKER {
            let msg = wparam.0 as u32;
            let down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
            let up = msg == WM_KEYUP || msg == WM_SYSKEYUP;
            if down || up {
                let ev = KeyEvent {
                    vk: kb.vkCode as u16,
                    scan: kb.scanCode as u16,
                    extended: kb.flags.0 & LLKHF_EXTENDED.0 != 0,
                    up: !down,
                };
                let swallow = std::panic::catch_unwind(|| {
                    // La captura va primero y no depende de la configuración.
                    capture_event(kb.vkCode, down).unwrap_or_else(|| handle(ev))
                })
                .unwrap_or(false);
                if swallow {
                    return LRESULT(1);
                }
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

const GROUP_WIN: [u32; 2] = [0x5B, 0x5C];
const GROUP_ALT: [u32; 3] = [0xA4, 0xA5, 0x12];
const GROUP_CTRL: [u32; 3] = [0xA2, 0xA3, 0x11];
const GROUP_SHIFT: [u32; 3] = [0xA0, 0xA1, 0x10];

fn is_modifier(vk: u32) -> bool {
    GROUP_WIN.contains(&vk) || GROUP_ALT.contains(&vk) || GROUP_CTRL.contains(&vk) || GROUP_SHIFT.contains(&vk)
}

fn async_down(vk: u32) -> bool {
    // SAFETY: GetAsyncKeyState no tiene precondiciones.
    unsafe { (GetAsyncKeyState(vk as i32) as u16) & 0x8000 != 0 }
}

/// Estado de modificadores con el evento actual aplicado (el sistema aún no lo refleja).
fn current_mods(event_vk: u32, event_down: bool) -> Mods {
    let group = |g: &[u32]| g.iter().any(|&v| if v == event_vk { event_down } else { async_down(v) });
    Mods { ctrl: group(&GROUP_CTRL), alt: group(&GROUP_ALT), shift: group(&GROUP_SHIFT), win: group(&GROUP_WIN) }
}

/// Marca que la próxima liberación de Alt/Win hay que enmascararla (ver [`mask_release`]).
fn neutralize(hs: &mut HookState, mods: &Mods) {
    if mods.win || mods.alt {
        hs.mask_pending = true;
    }
}

/// Windows abre el menú Inicio al soltar Win, y la app activa entra en **modo menú** al soltar
/// Alt, si entre la pulsación y la liberación no llegó ninguna otra tecla. En modo menú la app se
/// come el `Ctrl+V` del pegado (medido: Bloc de notas tras `Win+Alt`, el texto no entraba).
///
/// La tecla neutra (`VK_E8`) tiene que llegar a la app **entre** Alt↓ y Alt↑. Inyectarla desde
/// dentro del hook no lo garantiza: el orden con el evento que se está procesando queda en el
/// aire. Así que la liberación real se traga y otro hilo inyecta, en un solo `SendInput`,
/// `E8↓ E8↑ tecla↑`: el orden queda fijado. Las inyectadas llevan `MARKER` y este hook las ignora.
fn mask_release(ev: KeyEvent) {
    let spawned = std::thread::Builder::new().name("pronto-mask".into()).spawn(move || {
        send_key_events(&[KeyEvent::vk(VK_NEUTRAL, false), KeyEvent::vk(VK_NEUTRAL, true), ev]);
    });
    if let Err(e) = spawned {
        // Sin hilo, al menos que la tecla no se quede pulsada.
        eprintln!("[hotkey] no se pudo enmascarar la liberación: {e}");
        send_key_events(&[ev]);
    }
}

fn mods_from_down(down: &[u32]) -> Mods {
    let has = |g: &[u32]| down.iter().any(|v| g.contains(v));
    Mods { ctrl: has(&GROUP_CTRL), alt: has(&GROUP_ALT), shift: has(&GROUP_SHIFT), win: has(&GROUP_WIN) }
}

/// `Some(tragar)` si hay una captura en curso (y el evento es suyo); `None` si no la hay.
/// Se traga todo lo que se pulsa durante la captura; de lo soltado solo lo que pulsó ella
/// (así una tecla que venía pulsada de antes no se queda «pegada» en la app).
fn capture_event(vk: u32, down: bool) -> Option<bool> {
    let mut guard = CAPTURE.lock();
    let cap = guard.as_mut()?;

    if down {
        if vk == VK_ESCAPE && cap.down.is_empty() {
            let _ = cap.tx.send(Err("Cancelado".into()));
            *guard = None;
            // El «up» del Esc que cancela también se traga.
            STATE.lock().get_or_insert_with(HookState::default).swallowed.push(vk);
            return Some(true);
        }
        if !cap.down.contains(&vk) {
            cap.down.push(vk);
        }
        let m = mods_from_down(&cap.down);
        cap.peak = Mods { ctrl: cap.peak.ctrl || m.ctrl, alt: cap.peak.alt || m.alt, shift: cap.peak.shift || m.shift, win: cap.peak.win || m.win };
        if !is_modifier(vk) {
            cap.key = Some(vk);
        }
        return Some(true);
    }

    let ours = cap.down.contains(&vk);
    cap.down.retain(|k| *k != vk);
    if cap.down.is_empty() && (cap.peak.any() || cap.key.is_some()) {
        let result = match cap.key {
            Some(k) => match vk_to_name(k) {
                Some(name) => Ok(Hotkey { ctrl: cap.peak.ctrl, alt: cap.peak.alt, shift: cap.peak.shift, win: cap.peak.win, key: Some(name) }),
                None => Err("Tecla no admitida".to_string()),
            },
            None => Ok(Hotkey { ctrl: cap.peak.ctrl, alt: cap.peak.alt, shift: cap.peak.shift, win: cap.peak.win, key: None }),
        };
        let _ = cap.tx.send(result);
        *guard = None;
    }
    Some(ours)
}

/// Devuelve `true` si hay que tragarse el evento.
fn handle(ev: KeyEvent) -> bool {
    let (vk, down) = (ev.vk as u32, !ev.up);
    let Some(cfg) = *CONFIG.read() else { return false };
    let mods = current_mods(vk, down);
    let mut guard = STATE.lock();
    let hs = guard.get_or_insert_with(HookState::default);

    // Red de seguridad: si por lo que sea se nos escapó el «up» de un modificador, la pulsación
    // «mantener» se da por terminada en cuanto llega cualquier otro evento.
    if hs.hold_active && cfg.hold.valid && cfg.hold.key.is_none() && !mods.contains(&cfg.hold.mods) {
        hs.hold_active = false;
        send(HotkeyEvent::HoldUp);
    }

    if is_modifier(vk) {
        let hold = cfg.hold;
        if hold.valid && hold.key.is_none() {
            if !hs.hold_active && down && mods == hold.mods {
                hs.hold_active = true;
                send(HotkeyEvent::HoldDown);
                neutralize(hs, &mods);
            }
        } else if hold.valid && hs.hold_active && !mods.contains(&hold.mods) {
            hs.hold_active = false;
            send(HotkeyEvent::HoldUp);
        }
        let is_alt_or_win = GROUP_ALT.contains(&vk) || GROUP_WIN.contains(&vk);
        if !down && hs.mask_pending && is_alt_or_win {
            // Hasta que no quede ni Alt ni Win pulsada, cada liberación se enmascara.
            if !mods.alt && !mods.win {
                hs.mask_pending = false;
            }
            mask_release(ev);
            return true;
        }
        return false;
    }

    // Teclas ya tragadas: repeticiones y liberación.
    if hs.swallowed.contains(&vk) {
        if !down {
            hs.swallowed.retain(|k| *k != vk);
            if cfg.hold.valid && cfg.hold.key == Some(vk) && hs.hold_active {
                hs.hold_active = false;
                send(HotkeyEvent::HoldUp);
            }
        }
        return true;
    }
    if !down {
        return false;
    }

    if vk == VK_ESCAPE && RECORDING.load(Ordering::SeqCst) {
        hs.swallowed.push(vk);
        send(HotkeyEvent::Cancel);
        return true;
    }
    if cfg.hold.valid && cfg.hold.key == Some(vk) && mods == cfg.hold.mods && !hs.hold_active {
        hs.hold_active = true;
        hs.swallowed.push(vk);
        send(HotkeyEvent::HoldDown);
        neutralize(hs, &mods);
        return true;
    }
    for (combo, ev) in [
        (cfg.hands_free, HotkeyEvent::HandsFree),
        (cfg.paste_last, HotkeyEvent::PasteLast),
        (cfg.copy_last, HotkeyEvent::CopyLast),
        (cfg.meeting, HotkeyEvent::Meeting),
    ] {
        if combo.valid && combo.key == Some(vk) && mods == combo.mods {
            hs.swallowed.push(vk);
            send(ev);
            neutralize(hs, &mods);
            return true;
        }
    }
    if let Some(n) = transform_index(&cfg.transform, vk, &mods) {
        hs.swallowed.push(vk);
        send(HotkeyEvent::Transform(n));
        neutralize(hs, &mods);
        return true;
    }
    false
}

/// Si la tecla del Transform es un dígito, ese y los dos siguientes lanzan Transform N, N+1, N+2
/// (Win+Alt+1/2/3 en Wispr). Con otra tecla solo hay Transform 1.
fn transform_index(c: &Combo, vk: u32, mods: &Mods) -> Option<u8> {
    if !c.valid || *mods != c.mods {
        return None;
    }
    let base = c.key?;
    if (0x31..=0x37).contains(&base) {
        if (base..base + 3).contains(&vk) {
            return Some((vk - 0x30) as u8);
        }
        return None;
    }
    (vk == base).then_some(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nombres_de_tecla() {
        assert_eq!(vk_from_name("Space"), Some(0x20));
        assert_eq!(vk_from_name("z"), Some(0x5A));
        assert_eq!(vk_from_name("Z"), Some(0x5A));
        assert_eq!(vk_from_name("7"), Some(0x37));
        assert_eq!(vk_from_name("F1"), Some(0x70));
        assert_eq!(vk_from_name("F12"), Some(0x7B));
        assert_eq!(vk_from_name("F25"), None);
        assert_eq!(vk_from_name("Nada"), None);
    }

    #[test]
    fn nombre_ida_y_vuelta() {
        for name in ["A", "Z", "0", "9", "F1", "F24", "Space", "Enter", "ArrowUp", "`", "\\"] {
            let vk = vk_from_name(name).unwrap();
            assert_eq!(vk_to_name(vk).as_deref(), Some(name), "{name}");
        }
        assert_eq!(vk_to_name(0xE8), None);
    }

    #[test]
    fn combos_por_defecto() {
        let s = Settings::default();
        let hold = combo(&s.hotkey_hold);
        assert!(hold.valid && hold.key.is_none() && hold.mods.win && hold.mods.alt);
        let hf = combo(&s.hotkey_hands_free);
        assert_eq!(hf.key, Some(0x20));
        let pl = combo(&s.hotkey_paste_last);
        assert!(pl.mods.alt && pl.mods.shift && !pl.mods.win && pl.key == Some(0x5A));
        let cl = combo(&s.hotkey_copy_last);
        assert!(cl.mods.alt && cl.mods.shift && cl.key == Some(0x58));
        let tf = combo(&s.hotkey_transform);
        assert!(tf.mods.alt && tf.mods.shift && !tf.mods.win && tf.key == Some(0x50)); // Alt+Shift+P
    }

    #[test]
    fn transform_digitos() {
        // Un atajo personalizado con dígito (Win+Alt+1) sigue lanzando Transform 1..3.
        let tf = combo(&crate::models::legacy_transform_hotkey());
        assert_eq!(transform_index(&tf, 0x31, &tf.mods), Some(1));
        assert_eq!(transform_index(&tf, 0x33, &tf.mods), Some(3));
        assert_eq!(transform_index(&tf, 0x34, &tf.mods), None);
        assert_eq!(transform_index(&tf, 0x31, &Mods::default()), None);
    }

    #[test]
    fn transform_por_defecto_es_solo_el_1() {
        let tf = combo(&Settings::default().hotkey_transform);
        assert_eq!(transform_index(&tf, 0x50, &tf.mods), Some(1));
        assert_eq!(transform_index(&tf, 0x51, &tf.mods), None);
        assert_eq!(transform_index(&tf, 0x32, &tf.mods), None);
    }

    #[test]
    fn contiene_modificadores() {
        let a = Mods { win: true, alt: true, ..Default::default() };
        let b = Mods { win: true, ..Default::default() };
        assert!(a.contains(&b));
        assert!(!b.contains(&a));
    }

    #[test]
    fn modificadores_desde_teclas_pulsadas() {
        let m = mods_from_down(&[0x5B, 0xA4, 0x20]);
        assert!(m.win && m.alt && !m.ctrl && !m.shift);
    }
}
