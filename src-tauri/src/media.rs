//! Silenciar el volumen maestro mientras se graba (`IAudioEndpointVolume`) y restaurarlo.
//!
//! Se usa el interruptor de silencio, no el nivel: así el volumen exacto del usuario nunca se toca,
//! y si ya estaba silenciado antes de dictar se respeta. Todo fallo es solo un log.

use parking_lot::Mutex;
use windows::core::PWSTR;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED};

struct Muted {
    device_id: String,
}

/// `Some` mientras la app es la responsable de un silencio que debe deshacer.
static ACTIVE: Mutex<Option<Muted>> = Mutex::new(None);
/// Si es `false` cuando llega el silencio retardado, ya se paró la grabación: no se silencia.
static WANT_MUTED: Mutex<bool> = Mutex::new(false);

/// Pide silenciar dentro de `delay` (el pitido de inicio debe oírse antes). Si mientras tanto se
/// llama a `restore`, no se silencia nada.
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

fn do_unmute(device_id: &str) -> windows::core::Result<()> {
    let (vol, _) = endpoint_volume(Some(device_id))?;
    // SAFETY: vol es una interfaz COM válida.
    unsafe { vol.SetMute(false, std::ptr::null()) }
}

// ------------------------------------------------------------------ sonidos

// Los WAV van incrustados: en la app empaquetada `ui/` no existe como carpeta en disco.
static START_WAV: &[u8] = include_bytes!("../../ui/assets/sounds/start.wav");
static STOP_WAV: &[u8] = include_bytes!("../../ui/assets/sounds/stop.wav");
static LOCK_WAV: &[u8] = include_bytes!("../../ui/assets/sounds/lock.wav");

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
