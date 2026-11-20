//! Silenciar el volumen maestro mientras se graba (`IAudioEndpointVolume`) y restaurarlo.
//!
//! Se usa el interruptor de silencio, no el nivel: así el volumen exacto del usuario nunca se toca,
//! y si ya estaba silenciado antes de dictar se respeta. Todo fallo es solo un log.
//!
//! En macOS el silencio y la reproducción viven en `crate::mac::media` (Core Audio y `afplay`).

#[cfg(windows)]
use parking_lot::Mutex;
#[cfg(windows)]
use windows::core::PWSTR;
#[cfg(windows)]
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
#[cfg(windows)]
use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
#[cfg(windows)]
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED};

#[cfg(target_os = "macos")]
pub use crate::mac::media::{mute_after, restore};
#[cfg(target_os = "macos")]
use crate::mac::media::play;

#[cfg(windows)]
struct Muted {
    device_id: String,
}

/// `Some` mientras la app es la responsable de un silencio que debe deshacer.
#[cfg(windows)]
static ACTIVE: Mutex<Option<Muted>> = Mutex::new(None);
/// Si es `false` cuando llega el silencio retardado, ya se paró la grabación: no se silencia.
#[cfg(windows)]
static WANT_MUTED: Mutex<bool> = Mutex::new(false);

/// Pide silenciar dentro de `delay` (el pitido de inicio debe oírse antes). Si mientras tanto se
/// llama a `restore`, no se silencia nada.
#[cfg(windows)]
pub fn mute_after(delay: std::time::Duration) {
    *WANT_MUTED.lock() = true;
    let spawned = std::thread::Builder::new().name("pronto-mute".into()).spawn(move || {
        std::thread::sleep(delay);
        run_on_com_thread(|| {
            let mut active = ACTIVE.lock();
            if active.is_some() || !*WANT_MUTED.lock() {
                return;
            }
            match do_mute() {
                Ok(Some(m)) => *active = Some(m),
                Ok(None) => {} // ya estaba silenciado: no es cosa nuestra deshacerlo
                Err(e) => eprintln!("[media] no se pudo silenciar: {e}"),
            }
        });
    });
    if let Err(e) = spawned {
        eprintln!("[media] no se pudo lanzar el hilo: {e}");
    }
}

/// Devuelve el audio a como estaba. Idempotente.
#[cfg(windows)]
pub fn restore() {
    *WANT_MUTED.lock() = false;
    if ACTIVE.lock().is_none() {
        return; // nada que deshacer (si el silencio retardado aún no ha corrido, verá WANT_MUTED = false)
    }
    run_on_com_thread(|| {
        let mut active = ACTIVE.lock();
        if let Some(m) = active.take() {
            if let Err(e) = do_unmute(&m.device_id) {
                eprintln!("[media] no se pudo restaurar el volumen: {e}");
            }
        }
    });
}

/// COM exige inicializar el hilo; usamos uno corto propio para no interferir con los de Tauri.
#[cfg(windows)]
fn run_on_com_thread(f: impl FnOnce() + Send + 'static) {
    let h = std::thread::Builder::new().name("pronto-media".into()).spawn(move || {
        // SAFETY: emparejado con CoUninitialize si la inicialización tuvo éxito.
        let inited = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).is_ok() };
        f();
        if inited {
            unsafe { CoUninitialize() };
        }
    });
    match h {
        Ok(h) => {
            let _ = h.join();
        }
        Err(e) => eprintln!("[media] no se pudo lanzar el hilo: {e}"),
    }
}

#[cfg(windows)]
fn endpoint_volume(id: Option<&str>) -> windows::core::Result<(IAudioEndpointVolume, String)> {
    // SAFETY: COM ya inicializado en este hilo; los punteros devueltos los gestiona windows-rs.
    unsafe {
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = match id {
            Some(id) => {
                let wide: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();
                enumerator.GetDevice(windows::core::PCWSTR(wide.as_ptr()))?
            }
            None => enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?,
        };
        let pw: PWSTR = device.GetId()?;
        let device_id = pw.to_string().unwrap_or_default();
        CoTaskMemFree(Some(pw.0 as *const _));
        let vol: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None)?;
        Ok((vol, device_id))
    }
}

#[cfg(windows)]
fn do_mute() -> windows::core::Result<Option<Muted>> {
    let (vol, device_id) = endpoint_volume(None)?;
    // SAFETY: vol es una interfaz COM válida.
    unsafe {
        if vol.GetMute()?.as_bool() {
            return Ok(None);
        }
        vol.SetMute(true, std::ptr::null())?;
    }
    Ok(Some(Muted { device_id }))
}

#[cfg(windows)]
fn do_unmute(device_id: &str) -> windows::core::Result<()> {
    let (vol, _) = endpoint_volume(Some(device_id))?;
    // SAFETY: vol es una interfaz COM válida.
    unsafe { vol.SetMute(false, std::ptr::null()) }
}

// ------------------------------------------------------------------ sonidos

// Los WAV van incrustados: en la app empaquetada `ui/` no existe como carpeta en disco.
pub(crate) static START_WAV: &[u8] = include_bytes!("../../ui/assets/sounds/start.wav");
pub(crate) static STOP_WAV: &[u8] = include_bytes!("../../ui/assets/sounds/stop.wav");
pub(crate) static LOCK_WAV: &[u8] = include_bytes!("../../ui/assets/sounds/lock.wav");

/// Duración de un WAV PCM según su cabecera (`byte rate` del bloque `fmt ` y tamaño de `data`).
/// En Mac sirve para no silenciar la salida antes de que acabe el pitido de inicio.
#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn wav_duration(wav: &[u8]) -> Option<std::time::Duration> {
    if wav.len() < 12 || &wav[0..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return None;
    }
    let u32_at = |i: usize| wav.get(i..i + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let (mut pos, mut byte_rate, mut data) = (12usize, None, None);
    while let (Some(id), Some(size)) = (wav.get(pos..pos + 4), u32_at(pos + 4)) {
        match id {
            b"fmt " => byte_rate = u32_at(pos + 16),
            b"data" => data = Some(size),
            _ => {}
        }
        // Los bloques van alineados a 2 bytes.
        pos = pos.checked_add(8 + size as usize + (size as usize & 1))?;
    }
    let (rate, data) = (byte_rate.filter(|r| *r > 0)?, data?);
    Some(std::time::Duration::from_secs_f64(data as f64 / rate as f64))
}

#[cfg(windows)]
fn play(wav: &'static [u8]) {
    use windows::core::PCWSTR;
    use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
    // SAFETY: el búfer es 'static, como exige SND_ASYNC | SND_MEMORY (se reproduce tras volver).
    let ok = unsafe { PlaySoundW(PCWSTR(wav.as_ptr() as *const u16), None, SND_MEMORY | SND_ASYNC | SND_NODEFAULT) };
    if !ok.as_bool() {
        eprintln!("[media] PlaySoundW falló");
    }
}

pub fn play_start() {
    play(START_WAV);
}

pub fn play_stop() {
    play(STOP_WAV);
}

/// Sonido de bloqueo al entrar en manos libres.
pub fn play_lock() {
    play(LOCK_WAV);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duracion_de_los_pitidos() {
        for wav in [START_WAV, STOP_WAV, LOCK_WAV] {
            let d = wav_duration(wav).expect("WAV legible");
            assert!(d > std::time::Duration::ZERO && d < std::time::Duration::from_secs(2), "{d:?}");
        }
        assert_eq!(wav_duration(b"no es un wav"), None);
        assert_eq!(wav_duration(&START_WAV[..40]), None); // sin bloque data
    }
}
