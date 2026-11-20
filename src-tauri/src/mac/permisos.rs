//! macOS: los tres permisos que necesita Pronto, consultados y pedidos con explicación.
//!
//! | Permiso (Ajustes > Privacidad y seguridad) | Para qué | API |
//! |---|---|---|
//! | Accesibilidad | pegar con Cmd+V y escuchar el atajo (tap activo) | `AXIsProcessTrustedWithOptions` |
//! | Monitorización de entrada | escuchar el atajo si macOS lo pide aparte | `CGPreflightListenEventAccess` / `CGRequestListenEventAccess` |
//! | Micrófono | dictar | `AVCaptureDevice authorizationStatusForMediaType:` |
//!
//! macOS ata los permisos a la firma de la app: con la firma estable de `scripts/mac/` se conservan
//! entre versiones (decisión D6). Además se mira la tecla Globo: si abre los emojis o el dictado de
//! Apple, al mantenerla para dictar aparecerían las dos cosas.

use objc2::rc::autoreleasepool;
use objc2::runtime::{AnyClass, Bool};
use objc2::msg_send;
use objc2_core_foundation::{CFBoolean, CFDictionary, CFString};
use objc2_foundation::NSString;
use serde::Serialize;

// AVCaptureDevice vive en AVFoundation: se enlaza aquí (sin crate propia) y se habla con él por
// mensajes de Objective-C, buscando la clase en tiempo de ejecución (si no estuviera, no hay pánico).
#[link(name = "AVFoundation", kind = "framework")]
unsafe extern "C" {}

/// `AVMediaTypeAudio` vale `@"soun"` (constante pública de AVFoundation).
const AV_MEDIA_TYPE_AUDIO: &str = "soun";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub accessibility: bool,
    pub input_monitoring: bool,
    /// "granted", "denied", "ask" (aún no se ha preguntado) o "unknown".
    pub microphone: &'static str,
    /// El gancho de teclado está funcionando (si no, el atajo no hace nada).
    pub keyboard_hook: bool,
    /// Ajuste «Al pulsar la tecla 🌐» (`AppleFnUsageType`): 0 nada, 1 cambiar teclado, 2 emojis,
    /// 3 dictado de Apple. `None` = no se pudo leer.
    pub globe_key: Option<i64>,
    /// ¿Hay que avisar de la tecla Globo? (hace algo y además el atajo de Pronto usa Fn).
    pub globe_key_conflict: bool,
    pub all_ok: bool,
}

/// ¿Pronto puede pegar (Accesibilidad)? Con `prompt`, macOS enseña su aviso y abre Ajustes.
pub fn accessibility(prompt: bool) -> bool {
    if !prompt {
        // SAFETY: sin precondiciones.
        return unsafe { objc2_application_services::AXIsProcessTrusted() };
    }
    // SAFETY: constante de HIServices, válida todo el proceso.
    let key: &CFString = unsafe { objc2_application_services::kAXTrustedCheckOptionPrompt };
    let dict = CFDictionary::<CFString, CFBoolean>::from_slices(&[key], &[CFBoolean::new(true)]);
    // SAFETY: diccionario CFString → CFBoolean, como pide la API.
    unsafe { objc2_application_services::AXIsProcessTrustedWithOptions(Some(dict.as_ref())) }
}

/// ¿Pronto puede escuchar el teclado (Monitorización de entrada)?
pub fn input_monitoring(request: bool) -> bool {
    if request {
        objc2_core_graphics::CGRequestListenEventAccess()
    } else {
        objc2_core_graphics::CGPreflightListenEventAccess()
    }
}

fn capture_device_class() -> Option<&'static AnyClass> {
    AnyClass::get(c"AVCaptureDevice")
}

/// Estado del permiso de micrófono.
pub fn microphone() -> &'static str {
    let Some(cls) = capture_device_class() else { return "unknown" };
    autoreleasepool(|_| {
        let media = NSString::from_str(AV_MEDIA_TYPE_AUDIO);
        // SAFETY: método de clase documentado; devuelve AVAuthorizationStatus (NSInteger).
        let status: isize = unsafe { msg_send![cls, authorizationStatusForMediaType: &*media] };
        match status {
            3 => "granted",
            0 => "ask",
            1 | 2 => "denied",
            _ => "unknown",
        }
    })
}

/// Pide el micrófono (macOS enseña su ventana con el texto de `NSMicrophoneUsageDescription`).
pub fn request_microphone() {
    let Some(cls) = capture_device_class() else { return };
    autoreleasepool(|_| {
        let media = NSString::from_str(AV_MEDIA_TYPE_AUDIO);
        let block = block2::RcBlock::new(|granted: Bool| {
            if !granted.as_bool() {
                crate::errlog::error("permisos", "la persona no dio permiso de micrófono");
            }
        });
        // SAFETY: método de clase documentado; el bloque vive lo que necesite (lo retiene macOS).
        let _: () = unsafe { msg_send![cls, requestAccessForMediaType: &*media, completionHandler: &*block] };
    });
}

/// Ajuste de la tecla Globo (`defaults read com.apple.HIToolbox AppleFnUsageType`).
pub fn globe_key() -> Option<i64> {
    let out = std::process::Command::new("/usr/bin/defaults").args(["read", "com.apple.HIToolbox", "AppleFnUsageType"]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

/// ¿El ajuste de la tecla Globo choca con un atajo que use Fn? (0 = «No hacer nada» no choca).
pub fn globe_conflict(globe: Option<i64>, uses_fn: bool) -> bool {
    uses_fn && matches!(globe, Some(v) if v != 0)
}

pub fn status(uses_fn: bool) -> Status {
    let accessibility = accessibility(false);
    let input_monitoring = input_monitoring(false);
    let microphone = microphone();
    let keyboard_hook = crate::mac::hotkey::tap_active();
    let globe = globe_key();
    let globe_key_conflict = globe_conflict(globe, uses_fn);
    Status {
        accessibility,
        input_monitoring,
        microphone,
        keyboard_hook,
        globe_key: globe,
        globe_key_conflict,
        all_ok: accessibility && keyboard_hook && microphone != "denied" && !globe_key_conflict,
    }
}

/// Dirección de Ajustes del sistema para cada permiso.
pub fn settings_url(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "accessibility" => "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
        "inputMonitoring" => "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent",
        "microphone" => "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone",
        "keyboard" => "x-apple.systempreferences:com.apple.Keyboard-Settings.extension",
        _ => return None,
    })
}

/// Pide un permiso: primero la ventana propia de macOS (si la hay) y luego Ajustes en su sitio.
pub fn request(kind: &str) -> Result<(), String> {
    match kind {
        "accessibility" => {
            if accessibility(true) {
                return Ok(());
            }
        }
        "inputMonitoring" => {
            if input_monitoring(true) {
                return Ok(());
            }
        }
        "microphone" => {
            if microphone() == "ask" {
                request_microphone();
                return Ok(());
            }
        }
        "keyboard" => {}
        _ => return Err(format!("permiso desconocido: {kind}")),
    }
    open_settings(kind)
}

pub fn open_settings(kind: &str) -> Result<(), String> {
    let url = settings_url(kind).ok_or_else(|| format!("permiso desconocido: {kind}"))?;
    std::process::Command::new("/usr/bin/open").arg(url).spawn().map(|_| ()).map_err(|e| format!("no se pudo abrir Ajustes: {e}"))
}
