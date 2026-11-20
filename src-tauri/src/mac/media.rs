//! macOS: silenciar la salida de audio mientras se dicta y los sonidos de inicio, fin y bloqueo.
//! Hermana de las funciones `#[cfg(windows)]` de `crate::media`.
//!
//! - Silencio: Core Audio (`AudioObjectGetPropertyData`/`AudioObjectSetPropertyData`) sobre la salida
//!   por defecto (`kAudioHardwarePropertyDefaultOutputDevice`). Primero el interruptor de silencio
//!   (`kAudioDevicePropertyMute`, ámbito salida, elemento principal o, si no lo tiene, por canal); si el
//!   dispositivo no tiene silencio (algunos HDMI/USB), el volumen (`kAudioDevicePropertyVolumeScalar`) a 0
//!   y luego el valor exacto de antes. Igual que en Windows: si ya estaba silenciado no se toca, y al
//!   restaurar solo se deshace lo que cambió Pronto, en el mismo dispositivo aunque el usuario cambie
//!   de salida mientras dicta.
//! - Sonidos: `/usr/bin/afplay` con los mismos WAV incrustados que Windows (se escriben una vez en la
//!   carpeta temporal del usuario). En un hilo propio: nunca bloquea al que llama.
//!
//! Todo fallo es un apunte en `errlog`, nunca un pánico: sin silencio o sin pitido, el dictado sigue.

use crate::errlog;
use objc2_core_audio::{
    kAudioDevicePropertyMute, kAudioDevicePropertyVolumeScalar, kAudioHardwarePropertyDefaultOutputDevice,
    kAudioObjectPropertyElementMain, kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyScopeOutput, kAudioObjectSystemObject,
    kAudioObjectUnknown, AudioObjectGetPropertyData, AudioObjectHasProperty, AudioObjectID, AudioObjectIsPropertySettable,
    AudioObjectPropertyAddress, AudioObjectPropertyElement, AudioObjectPropertyScope, AudioObjectPropertySelector,
    AudioObjectSetPropertyData,
};
use parking_lot::Mutex;
use std::ffi::c_void;
use std::path::PathBuf;
use std::ptr::NonNull;
use std::time::{Duration, Instant};

/// Canales que se prueban cuando el dispositivo no tiene control en el elemento principal.
const MAX_CHANNELS: u32 = 16;
/// Lo que tarda `afplay` en arrancar y empezar a sonar (margen generoso).
const AFPLAY_START: Duration = Duration::from_millis(250);
/// Tope de espera extra antes de silenciar para que el pitido de inicio se oiga entero.
const MAX_EXTRA_WAIT: Duration = Duration::from_millis(900);
/// Un `afplay` que no acaba en este tiempo se mata (los pitidos duran menos de un segundo).
const AFPLAY_TIMEOUT: Duration = Duration::from_secs(5);

/// Qué cambió Pronto para silenciar, para deshacer exactamente eso.
enum How {
    /// Elementos a los que se puso el silencio (estaban sin silenciar).
    Mute(Vec<AudioObjectPropertyElement>),
    /// Elementos a los que se bajó el volumen a 0, con el volumen exacto que tenían.
    Volume(Vec<(AudioObjectPropertyElement, f32)>),
}

struct Muted {
    device: AudioObjectID,
    how: How,
}

/// `Some` mientras la app es la responsable de un silencio que debe deshacer.
static ACTIVE: Mutex<Option<Muted>> = Mutex::new(None);
/// Si es `false` cuando llega el silencio retardado, ya se paró la grabación: no se silencia.
static WANT_MUTED: Mutex<bool> = Mutex::new(false);
/// Hasta cuándo suena el último pitido lanzado (para no cortarlo al silenciar).
static SOUND_UNTIL: Mutex<Option<Instant>> = Mutex::new(None);

/// Pide silenciar dentro de `delay` (el pitido de inicio debe oírse antes). Si mientras tanto se
/// llama a `restore`, no se silencia nada. En Mac `afplay` tarda en arrancar, así que además se
/// espera a que acabe el pitido en curso (con tope).
pub fn mute_after(delay: Duration) {
    *WANT_MUTED.lock() = true;
    let spawned = std::thread::Builder::new().name("pronto-mute".into()).spawn(move || {
        std::thread::sleep(delay);
        let until = *SOUND_UNTIL.lock();
        if let Some(extra) = until.and_then(|u| u.checked_duration_since(Instant::now())) {
            std::thread::sleep(extra.min(MAX_EXTRA_WAIT));
        }
        let mut active = ACTIVE.lock();
        if active.is_some() || !*WANT_MUTED.lock() {
            return;
        }
        match do_mute() {
            Ok(Some(m)) => *active = Some(m),
            Ok(None) => {} // ya estaba silenciado: no es cosa nuestra deshacerlo
            Err(e) => errlog::error("media", format!("no se pudo silenciar: {e}")),
        }
    });
    if let Err(e) = spawned {
        errlog::error("media", format!("no se pudo lanzar el hilo de silencio: {e}"));
    }
}

/// Devuelve el audio a como estaba. Idempotente.
pub fn restore() {
    *WANT_MUTED.lock() = false;
    let mut active = ACTIVE.lock();
    if let Some(m) = active.take() {
        if let Err(e) = do_unmute(&m) {
            errlog::error("media", format!("no se pudo restaurar el audio: {e}"));
        }
    }
}

// ------------------------------------------------------------------ Core Audio

pub(super) fn address(selector: AudioObjectPropertySelector, scope: AudioObjectPropertyScope, element: AudioObjectPropertyElement) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress { mSelector: selector, mScope: scope, mElement: element }
}

/// Lee una propiedad de tamaño fijo (`u32`, `f32`, `AudioObjectID`…).
pub(super) fn get<T: Copy + Default>(object: AudioObjectID, addr: AudioObjectPropertyAddress) -> Result<T, String> {
    let mut value = T::default();
    let mut size = std::mem::size_of::<T>() as u32;
    // SAFETY: `addr`, `size` y `value` son locales vivos; `size` anuncia exactamente el búfer de `value`.
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&addr),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
            NonNull::from(&mut value).cast::<c_void>(),
        )
    };
    if status != 0 {
        return Err(format!("lectura de '{}' en {object}: OSStatus {status}", fourcc(addr.mSelector)));
    }
    if size as usize != std::mem::size_of::<T>() {
        return Err(format!("lectura de '{}' en {object}: tamaño {size}", fourcc(addr.mSelector)));
    }
    Ok(value)
}

/// Escribe una propiedad de tamaño fijo.
fn set<T: Copy>(object: AudioObjectID, addr: AudioObjectPropertyAddress, value: T) -> Result<(), String> {
    let mut value = value;
    // SAFETY: `addr` y `value` son locales vivos; el tamaño es el de `value`.
    let status = unsafe {
        AudioObjectSetPropertyData(
            object,
            NonNull::from(&addr),
            0,
            std::ptr::null(),
            std::mem::size_of::<T>() as u32,
            NonNull::from(&mut value).cast::<c_void>(),
        )
    };
    if status != 0 {
        return Err(format!("escritura de '{}' en {object}: OSStatus {status}", fourcc(addr.mSelector)));
    }
    Ok(())
}

/// ¿Tiene el objeto esa propiedad y se puede cambiar?
fn writable(object: AudioObjectID, addr: AudioObjectPropertyAddress) -> bool {
    // SAFETY: `addr` es un local vivo.
    if !unsafe { AudioObjectHasProperty(object, NonNull::from(&addr)) } {
        return false;
    }
    let mut settable: u8 = 0;
    // SAFETY: `addr` y `settable` son locales vivos.
    let status = unsafe { AudioObjectIsPropertySettable(object, NonNull::from(&addr), NonNull::from(&mut settable)) };
    status == 0 && settable != 0
}

/// Selector de Core Audio legible en los apuntes (`0x6d757465` -> `mute`).
fn fourcc(v: u32) -> String {
    v.to_be_bytes().iter().map(|&b| if b.is_ascii_graphic() || b == b' ' { b as char } else { '?' }).collect()
}

fn default_output() -> Result<AudioObjectID, String> {
    let addr = address(kAudioHardwarePropertyDefaultOutputDevice, kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyElementMain);
    let id: AudioObjectID = get(kAudioObjectSystemObject as AudioObjectID, addr)?;
    if id == kAudioObjectUnknown {
        return Err("no hay salida de audio por defecto".into());
    }
    Ok(id)
}

/// Elementos del dispositivo donde se puede cambiar `selector` en la salida: el principal si lo
/// admite; si no, cada canal que lo admita (muchos dispositivos solo tienen control por canal).
fn elements(device: AudioObjectID, selector: AudioObjectPropertySelector) -> Vec<AudioObjectPropertyElement> {
    let main = address(selector, kAudioObjectPropertyScopeOutput, kAudioObjectPropertyElementMain);
    if writable(device, main) {
        return vec![kAudioObjectPropertyElementMain];
    }
    (1..=MAX_CHANNELS).filter(|&ch| writable(device, address(selector, kAudioObjectPropertyScopeOutput, ch))).collect()
}

fn do_mute() -> Result<Option<Muted>, String> {
    let device = default_output()?;

    // 1) Interruptor de silencio: solo se silencian los elementos que no lo estaban.
    let mute_elems = elements(device, kAudioDevicePropertyMute);
    if !mute_elems.is_empty() {
        let mut changed = Vec::new();
        for el in mute_elems {
            let addr = address(kAudioDevicePropertyMute, kAudioObjectPropertyScopeOutput, el);
            let now: u32 = match get(device, addr) {
                Ok(v) => v,
                Err(e) => {
                    undo_mute(device, &changed);
                    return Err(e);
                }
            };
            if now != 0 {
                continue; // ya silenciado por el usuario: se respeta
            }
            if let Err(e) = set(device, addr, 1u32) {
                undo_mute(device, &changed);
                return Err(e);
            }
            changed.push(el);
        }
        return Ok((!changed.is_empty()).then_some(Muted { device, how: How::Mute(changed) }));
    }

    // 2) Sin interruptor: volumen a 0 y se guarda el exacto de cada elemento.
    let vol_elems = elements(device, kAudioDevicePropertyVolumeScalar);
    if vol_elems.is_empty() {
        return Err(format!("la salida {device} no tiene silencio ni volumen ajustable"));
    }
    let mut changed: Vec<(AudioObjectPropertyElement, f32)> = Vec::new();
    for el in vol_elems {
        let addr = address(kAudioDevicePropertyVolumeScalar, kAudioObjectPropertyScopeOutput, el);
        let before: f32 = match get(device, addr) {
            Ok(v) => v,
            Err(e) => {
                undo_volume(device, &changed);
                return Err(e);
            }
        };
        if !(before > 0.0) {
            continue; // ya a 0 (o valor raro): no se toca
        }
        if let Err(e) = set(device, addr, 0.0f32) {
            undo_volume(device, &changed);
            return Err(e);
        }
        changed.push((el, before));
    }
    Ok((!changed.is_empty()).then_some(Muted { device, how: How::Volume(changed) }))
}

fn undo_mute(device: AudioObjectID, elems: &[AudioObjectPropertyElement]) -> Vec<String> {
    elems
        .iter()
        .filter_map(|&el| set(device, address(kAudioDevicePropertyMute, kAudioObjectPropertyScopeOutput, el), 0u32).err())
        .collect()
}

fn undo_volume(device: AudioObjectID, saved: &[(AudioObjectPropertyElement, f32)]) -> Vec<String> {
    saved
        .iter()
        .filter_map(|&(el, v)| set(device, address(kAudioDevicePropertyVolumeScalar, kAudioObjectPropertyScopeOutput, el), v).err())
        .collect()
}

fn do_unmute(m: &Muted) -> Result<(), String> {
    let errors = match &m.how {
        How::Mute(elems) => undo_mute(m.device, elems),
        How::Volume(saved) => undo_volume(m.device, saved),
    };
    if errors.is_empty() { Ok(()) } else { Err(errors.join("; ")) }
}

// ------------------------------------------------------------------ sonidos

/// Ruta en disco del WAV (afplay necesita un archivo). Se escribe una vez por arranque en la
/// carpeta temporal del usuario (`$TMPDIR`, privada en macOS); `None` si no se pudo.
fn sound_file(wav: &'static [u8]) -> Option<PathBuf> {
    static FILES: Mutex<Vec<(usize, Option<PathBuf>)>> = Mutex::new(Vec::new());
    let key = wav.as_ptr() as usize;
    let mut files = FILES.lock();
    if let Some((_, p)) = files.iter().find(|(k, _)| *k == key) {
        return p.clone();
    }
    let name = if std::ptr::eq(wav, crate::media::START_WAV) {
        "start.wav"
    } else if std::ptr::eq(wav, crate::media::STOP_WAV) {
        "stop.wav"
    } else {
        "lock.wav"
    };
    let dir = std::env::temp_dir().join("pronto2-sonidos");
    let path = dir.join(name);
    let ok = std::fs::create_dir_all(&dir)
        .and_then(|_| match std::fs::metadata(&path) {
            Ok(m) if m.len() == wav.len() as u64 => Ok(()),
            _ => std::fs::write(&path, wav),
        })
        .map_err(|e| errlog::error("media", format!("no se pudo preparar el sonido {name}: {e}")))
        .is_ok();
    let out = ok.then_some(path);
    files.push((key, out.clone()));
    out
}

/// Reproduce un WAV incrustado con `/usr/bin/afplay`, sin bloquear. Hermana de `PlaySoundW`.
pub fn play(wav: &'static [u8]) {
    let length = crate::media::wav_duration(wav).unwrap_or(Duration::from_millis(500));
    *SOUND_UNTIL.lock() = Some(Instant::now() + AFPLAY_START + length);
    let spawned = std::thread::Builder::new().name("pronto-sonido".into()).spawn(move || {
        let Some(path) = sound_file(wav) else { return };
        let child = std::process::Command::new("/usr/bin/afplay")
            .arg(&path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(e) => {
                errlog::error("media", format!("afplay no arrancó: {e}"));
                return;
            }
        };
        // Se espera para no dejar procesos zombi; si se cuelga, se mata.
        let deadline = Instant::now() + AFPLAY_TIMEOUT;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    errlog::error("media", "afplay no terminó a tiempo");
                    return;
                }
                Err(e) => {
                    errlog::error("media", format!("afplay: {e}"));
                    return;
                }
            }
        }
    });
    if let Err(e) = spawned {
        errlog::error("media", format!("no se pudo lanzar el hilo del sonido: {e}"));
    }
}
