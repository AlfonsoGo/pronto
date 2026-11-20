//! macOS: el gancho de teclado global es un `CGEventTap` de sesión, activo (puede tragarse teclas),
//! en un hilo propio con su `CFRunLoop`. Es la hermana de `start_hook`/`hook_proc` de Windows.
//!
//! Cada tecla se traduce al código virtual de Windows (`teclas_mac`) y se pasa a
//! [`crate::hotkey::on_key`]: la lógica de atajos (mantener, manos libres, Esc, vista previa…) es la
//! misma en los dos sistemas. La tecla Fn/Globo llega como `flagsChanged` con el código 63.
//!
//! Permisos: crear un tap activo pide **Accesibilidad**; si macOS solo deja escuchar, hace falta
//! **Monitorización de entrada**. Si no hay permiso, `CGEventTapCreate` devuelve nulo: se reintenta
//! cada 2 s, sin pánico, hasta que la persona lo conceda (la ventana de permisos la abre `permisos`).

use crate::hotkey::{on_key, VK_FN};
use crate::inject::{KeyEvent, MARKER};
use crate::teclas_mac::{mac_from_vk, modifier_down, vk_from_mac, MAC_FN};
use objc2_core_foundation::{kCFRunLoopCommonModes, CFMachPort, CFRunLoop};
use objc2_core_graphics::{CGEvent, CGEventField, CGEventSource, CGEventSourceStateID, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventTapProxy, CGEventType};
use std::ffi::c_void;
use std::ptr::{null_mut, NonNull};
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::time::Duration;

/// El tap creado (para volver a encenderlo si macOS lo apaga). Vive lo que vive la app.
static TAP: AtomicPtr<CFMachPort> = AtomicPtr::new(null_mut());
/// ¿Está funcionando el gancho? Lo enseña la pantalla de permisos.
static ACTIVE: AtomicBool = AtomicBool::new(false);
/// Fn pulsada ahora (la Fn no tiene bit de lado y las flechas encienden su bit general).
static FN_DOWN: AtomicBool = AtomicBool::new(false);

/// ¿El gancho de teclado está activo? (`false` = falta permiso de Accesibilidad/Monitorización).
pub fn tap_active() -> bool {
    ACTIVE.load(Ordering::SeqCst)
}

/// ¿La Fn está pulsada ahora mismo? (para no pegar hasta que se suelte).
pub fn fn_down() -> bool {
    FN_DOWN.load(Ordering::SeqCst)
}

const MASK: u64 = (1 << CGEventType::KeyDown.0) | (1 << CGEventType::KeyUp.0) | (1 << CGEventType::FlagsChanged.0);

/// Lanza el hilo del gancho. Lo llama `hotkey::start` una sola vez.
pub(crate) fn start_hook() {
    let spawned = std::thread::Builder::new().name("pronto-hook".into()).spawn(|| {
        let mut avisado = false;
        loop {
            // SAFETY: `tap_callback` cumple la firma de CGEventTapCallBack y no usa `user_info`.
            let tap = unsafe { CGEvent::tap_create(CGEventTapLocation::SessionEventTap, CGEventTapPlacement::HeadInsertEventTap, CGEventTapOptions::Default, MASK, Some(tap_callback), null_mut()) };
            let Some(tap) = tap else {
                if !avisado {
                    crate::errlog::error("hotkey", "macOS no deja escuchar el teclado todavía: falta el permiso de Accesibilidad o de Monitorización de entrada");
                    avisado = true;
                }
                ACTIVE.store(false, Ordering::SeqCst);
                std::thread::sleep(Duration::from_secs(2));
                continue;
            };
            let Some(source) = CFMachPort::new_run_loop_source(None, Some(&tap), 0) else {
                crate::errlog::error("hotkey", "no se pudo crear la fuente del gancho de teclado");
                std::thread::sleep(Duration::from_secs(5));
                continue;
            };
            let Some(run_loop) = CFRunLoop::current() else {
                crate::errlog::error("hotkey", "el hilo del gancho no tiene CFRunLoop");
                return;
            };
            // SAFETY: constante de CoreFoundation, válida durante todo el proceso.
            let modes = unsafe { kCFRunLoopCommonModes };
            run_loop.add_source(Some(&source), modes);
            CGEvent::tap_enable(&tap, true);
            // El puntero queda válido: `tap` no se suelta mientras corre el bucle.
            TAP.store(&*tap as *const CFMachPort as *mut CFMachPort, Ordering::SeqCst);
            ACTIVE.store(true, Ordering::SeqCst);
            if avisado {
                crate::errlog::error("hotkey", "permiso concedido: el atajo de teclado ya funciona");
            }
            CFRunLoop::run(); // bloquea mientras viva la app
            // Si el bucle termina (no debería), se suelta el tap y se vuelve a crear.
            TAP.store(null_mut(), Ordering::SeqCst);
            ACTIVE.store(false, Ordering::SeqCst);
            drop(source);
            drop(tap);
            std::thread::sleep(Duration::from_secs(1));
        }
    });
    if let Err(e) = spawned {
        crate::errlog::error("hotkey", format!("no se pudo lanzar el hilo del gancho de teclado: {e}"));
    }
}

/// El callback del tap: lo mínimo (traducir y decidir). macOS apaga el tap si tarda (~1 s).
unsafe extern "C-unwind" fn tap_callback(_proxy: CGEventTapProxy, ty: CGEventType, event: NonNull<CGEvent>, _user: *mut c_void) -> *mut CGEvent {
    let pass = event.as_ptr();
    // macOS apagó el tap (tardamos o la persona pulsó algo especial): se vuelve a encender.
    if ty == CGEventType::TapDisabledByTimeout || ty == CGEventType::TapDisabledByUserInput {
        let tap = TAP.load(Ordering::SeqCst);
        if !tap.is_null() {
            // SAFETY: el puntero es el del tap vivo del hilo del gancho.
            CGEvent::tap_enable(unsafe { &*tap }, true);
        }
        return pass;
    }
    // SAFETY: macOS nos da un evento válido durante la llamada.
    let ev = unsafe { event.as_ref() };
    // Lo que inyecta Pronto (Cmd+V, Enter) lleva la marca y no es del usuario.
    if CGEvent::integer_value_field(Some(ev), CGEventField::EventSourceUserData) == MARKER as i64 {
        return pass;
    }
    let keycode = CGEvent::integer_value_field(Some(ev), CGEventField::KeyboardEventKeycode) as u16;
    let Some(vk) = vk_from_mac(keycode) else { return pass };
    let down = if ty == CGEventType::KeyDown {
        true
    } else if ty == CGEventType::KeyUp {
        false
    } else if ty == CGEventType::FlagsChanged {
        match modifier_down(keycode, CGEvent::flags(Some(ev)).0) {
            Some(d) => d,
            None => return pass, // Bloq Mayús y demás: no son de Pronto
        }
    } else {
        return pass;
    };
    if keycode == MAC_FN {
        FN_DOWN.store(down, Ordering::SeqCst);
    }
    let swallow = on_key(KeyEvent::vk(vk as u16, !down));
    // La Fn nunca se traga: macOS la necesita para sus combinaciones (Fn+F1…) y tragarla no evita
    // el emoji/dictado de la tecla Globo (eso se avisa en la pantalla de permisos).
    if swallow && vk != VK_FN { null_mut() } else { pass }
}

/// ¿Está pulsada la tecla de este código virtual de Windows? Estado real del teclado (sesión).
pub(crate) fn async_down(vk: u32) -> bool {
    if vk == VK_FN {
        return fn_down();
    }
    match mac_from_vk(vk) {
        // Los genéricos de Windows (0x10-0x12) se resuelven a la tecla izquierda en `mac_from_vk`; aquí
        // no se miran para no contar dos veces la misma tecla (el grupo ya incluye la izquierda).
        Some(code) if !matches!(vk, 0x10..=0x12) => CGEventSource::key_state(CGEventSourceStateID::CombinedSessionState, code),
        _ => false,
    }
}

/// En Windows reenvía la liberación de Alt/Win tras una tecla neutra. En Mac no hay menú Inicio ni
/// modo menú: `neutralize` nunca la pide, pero si llegara se deja pasar la tecla tal cual.
pub(crate) fn mask_release(ev: KeyEvent) {
    crate::inject::send_key_events(&[ev]);
}
