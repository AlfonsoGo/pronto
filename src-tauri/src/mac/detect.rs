//! macOS: qué apps tienen el micrófono abierto ahora (hermana de las lecturas `#[cfg(windows)]` de
//! `crate::detect`). La decisión de si eso es una reunión sigue siendo la función pura común
//! `detect::scan`, que no cambia.
//!
//! - Camino normal (macOS 14+): Core Audio lleva un «objeto proceso» por cada programa que usa audio
//!   (`kAudioHardwarePropertyProcessObjectList`); cada uno dice si está grabando
//!   (`kAudioProcessPropertyIsRunningInput`), su PID y su bundle id. Es el equivalente exacto del
//!   registro `ConsentStore\microphone` de Windows: quién tiene el micro, sin pedir permisos. Pronto se
//!   excluye por su PID (y además `scan` ignora todo lo que se llama «pronto»).
//! - Respaldo (si lo anterior falla): `kAudioDevicePropertyDeviceIsRunningSomewhere` en la entrada por
//!   defecto dice si ALGUIEN graba, y `NSWorkspace.runningApplications` qué apps de reunión están
//!   abiertas. Mientras el propio Pronto graba (`kAudioDevicePropertyDeviceIsRunning`, de este
//!   proceso) no se puede saber si hay otro: se repite la última respuesta.
//! - Títulos de ventana: en Mac exigen el permiso de Grabación de pantalla, que Pronto NO pide. Se
//!   devuelve la marca `detect::NO_TITLES` y `scan` lo trata como «llamada en el navegador» dudosa:
//!   se avisa, nunca se graba sola.

use super::media::{address, get};
use crate::detect;
use crate::errlog;
use objc2_app_kit::NSWorkspace;
use objc2_core_audio::{
    kAudioDevicePropertyDeviceIsRunning, kAudioDevicePropertyDeviceIsRunningSomewhere, kAudioHardwarePropertyDefaultInputDevice,
    kAudioHardwarePropertyProcessObjectList, kAudioObjectPropertyElementMain, kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject,
    kAudioObjectUnknown, kAudioProcessPropertyBundleID, kAudioProcessPropertyIsRunningInput, kAudioProcessPropertyPID,
    AudioObjectGetPropertyData, AudioObjectGetPropertyDataSize, AudioObjectID,
};
use objc2_core_foundation::{CFRetained, CFString};
use parking_lot::Mutex;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};

/// Última respuesta del camino de respaldo (se repite mientras el propio Pronto graba).
static LAST_FALLBACK: Mutex<Vec<(String, bool)>> = Mutex::new(Vec::new());
/// Para apuntar una sola vez que el camino normal no funciona en este Mac.
static FALLBACK_LOGGED: AtomicBool = AtomicBool::new(false);

/// Apps con el micrófono en uso ahora, con el nombre que entiende `detect::scan` (`"zoom"`,
/// `"msteams"`, `"chrome"`…; las desconocidas con su bundle id). Misma forma que la lectura del
/// registro de Windows. Solo se devuelven las que lo están usando.
pub fn read_mic_consent() -> Vec<(String, bool)> {
    match from_process_objects() {
        Ok(list) => list,
        Err(e) => {
            if !FALLBACK_LOGGED.swap(true, Ordering::SeqCst) {
                errlog::error("detect", format!("sin lista de procesos de Core Audio ({e}); se usa el respaldo"));
            }
            fallback()
        }
    }
}

/// Mac no da títulos de ventana sin el permiso de Grabación de pantalla: se marca cada navegador
/// como «sin títulos» para que `scan` lo trate como llamada de navegador dudosa.
pub fn window_titles() -> Vec<(String, String)> {
    detect::titles_unavailable()
}

// ------------------------------------------------------------------ camino normal

const SYSTEM: AudioObjectID = kAudioObjectSystemObject as AudioObjectID;

fn global(selector: u32) -> objc2_core_audio::AudioObjectPropertyAddress {
    address(selector, kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyElementMain)
}

fn process_objects() -> Result<Vec<AudioObjectID>, String> {
    let addr = global(kAudioHardwarePropertyProcessObjectList);
    let mut size = 0u32;
    // SAFETY: `addr` y `size` son locales vivos.
    let status = unsafe { AudioObjectGetPropertyDataSize(SYSTEM, NonNull::from(&addr), 0, std::ptr::null(), NonNull::from(&mut size)) };
    if status != 0 {
        return Err(format!("tamaño de la lista de procesos: OSStatus {status}"));
    }
    let n = size as usize / std::mem::size_of::<AudioObjectID>();
    if n == 0 {
        return Ok(Vec::new());
    }
    let mut ids: Vec<AudioObjectID> = vec![kAudioObjectUnknown; n];
    let mut size = (n * std::mem::size_of::<AudioObjectID>()) as u32;
    let Some(buf) = NonNull::new(ids.as_mut_ptr()) else { return Ok(Vec::new()) };
    // SAFETY: `ids` tiene `size` bytes reservados; Core Audio escribe como mucho eso y dice cuánto.
    let status = unsafe { AudioObjectGetPropertyData(SYSTEM, NonNull::from(&addr), 0, std::ptr::null(), NonNull::from(&mut size), buf.cast::<c_void>()) };
    if status != 0 {
        return Err(format!("lista de procesos: OSStatus {status}"));
    }
    ids.truncate(size as usize / std::mem::size_of::<AudioObjectID>());
    Ok(ids)
}

/// Bundle id del proceso (`kAudioProcessPropertyBundleID`, un CFString que hay que liberar).
fn bundle_id(process: AudioObjectID) -> Option<String> {
    let addr = global(kAudioProcessPropertyBundleID);
    let mut ptr: *const c_void = std::ptr::null();
    let mut size = std::mem::size_of::<*const c_void>() as u32;
    // SAFETY: `ptr` es un hueco del tamaño de un puntero, que es lo que devuelve esta propiedad.
    let status = unsafe {
        AudioObjectGetPropertyData(process, NonNull::from(&addr), 0, std::ptr::null(), NonNull::from(&mut size), NonNull::from(&mut ptr).cast::<c_void>())
    };
    if status != 0 || size as usize != std::mem::size_of::<*const c_void>() {
        return None;
    }
    let ptr = NonNull::new(ptr as *mut CFString)?;
    // SAFETY: la propiedad devuelve un CFString con +1 (regla «Copy» de Core Audio): lo libera CFRetained.
    let s = unsafe { CFRetained::from_raw(ptr) };
    let s = s.to_string();
    (!s.trim().is_empty()).then_some(s)
}

fn from_process_objects() -> Result<Vec<(String, bool)>, String> {
    let me = std::process::id() as i32;
    let mut out: Vec<(String, bool)> = Vec::new();
    for p in process_objects()? {
        let recording: u32 = get(p, global(kAudioProcessPropertyIsRunningInput)).unwrap_or(0);
        if recording == 0 {
            continue;
        }
        let pid: i32 = get(p, global(kAudioProcessPropertyPID)).unwrap_or(-1);
        if pid == me {
            continue; // Pronto dictando o grabando una reunión
        }
        let name = match bundle_id(p) {
            Some(b) => detect::mac_app_key(&b).map(str::to_string).unwrap_or(b),
            None => format!("pid-{pid}"),
        };
        if !out.iter().any(|(n, _)| *n == name) {
            out.push((name, true));
        }
    }
    Ok(out)
}

// ------------------------------------------------------------------ respaldo

fn fallback() -> Vec<(String, bool)> {
    let out = match input_state() {
        Ok((false, _)) => Vec::new(),
        // Pronto también graba: no se distingue si hay otro; se mantiene lo último que se supo.
        Ok((true, true)) => LAST_FALLBACK.lock().clone(),
        Ok((true, false)) => running_meeting_apps(),
        Err(e) => {
            errlog::error("detect", format!("no se pudo leer el micrófono: {e}"));
            Vec::new()
        }
    };
    *LAST_FALLBACK.lock() = out.clone();
    out
}

/// (alguien graba con la entrada por defecto, la graba este proceso).
fn input_state() -> Result<(bool, bool), String> {
    let device: AudioObjectID = get(SYSTEM, global(kAudioHardwarePropertyDefaultInputDevice))?;
    if device == kAudioObjectUnknown {
        return Ok((false, false));
    }
    let somewhere: u32 = get(device, global(kAudioDevicePropertyDeviceIsRunningSomewhere))?;
    let here: u32 = get(device, global(kAudioDevicePropertyDeviceIsRunning)).unwrap_or(0);
    Ok((somewhere != 0, here != 0))
}

/// Apps de reunión y navegadores abiertos (`NSWorkspace.runningApplications`, seguro fuera del hilo
/// principal según Apple), como si todas usaran el micro.
fn running_meeting_apps() -> Vec<(String, bool)> {
    let mut out: Vec<(String, bool)> = Vec::new();
    let apps = NSWorkspace::sharedWorkspace().runningApplications();
    for app in apps.iter() {
        if app.isTerminated() {
            continue;
        }
        let Some(bundle) = app.bundleIdentifier() else { continue };
        let Some(key) = detect::mac_app_key(&bundle.to_string()) else { continue };
        if !out.iter().any(|(n, _)| n == key) {
            out.push((key.to_string(), true));
        }
    }
    out
}
