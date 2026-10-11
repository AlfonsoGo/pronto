//! Detección de reuniones: qué apps tienen el micrófono en uso ahora mismo.
//!
//! Windows lo apunta en el registro (`CapabilityAccessManager\ConsentStore\microphone`): cada app
//! (empaquetada, o en `NonPackaged` con la ruta del exe) tiene `LastUsedTimeStart` y
//! `LastUsedTimeStop`; con `Stop == 0` el micro está abierto en este momento. La decisión de si
//! eso es una reunión es una función pura (`detect_meeting`), separada de la lectura del sistema.

use crate::inject;
use windows::core::{w, BOOL, PCWSTR, PWSTR};
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::System::Registry::{
    RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ, REG_VALUE_TYPE,
};
use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, IsWindowVisible};

const MIC_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\microphone");

/// Navegadores en los que una pestaña puede ser una reunión.
const BROWSERS: &[(&str, &str)] = &[
    ("chrome", "Chrome"),
    ("msedge", "Edge"),
    ("firefox", "Firefox"),
    ("brave", "Brave"),
    ("opera", "Opera"),
    ("vivaldi", "Vivaldi"),
    ("arc", "Arc"),
];

/// Palabras del título de una pestaña que delatan una videollamada.
const TITLE_KEYWORDS: &[(&str, &str)] = &[
    ("meet", "Google Meet"),
    ("zoom", "Zoom"),
    ("teams", "Microsoft Teams"),
    ("jitsi", "Jitsi"),
    ("whereby", "Whereby"),
];

/// `C:#Program Files#Zoom#bin#Zoom.exe` -> `zoom`; `MSTeams_8wekyb3d8bbwe` -> `msteams`.
pub fn normalize_app(raw: &str) -> String {
    let last = raw.rsplit(['#', '\\', '/']).next().unwrap_or(raw);
    let last = last.split('_').next().unwrap_or(last);
    last.to_lowercase().trim_end_matches(".exe").to_string()
}

fn title_has_keyword(title: &str) -> Option<&'static str> {
    let low = title.to_lowercase();
    let tokens: Vec<&str> = low.split(|c: char| !c.is_alphanumeric()).collect();
    TITLE_KEYWORDS.iter().find(|(k, _)| tokens.contains(k)).map(|(_, label)| *label)
}

/// Todas las reuniones en curso (una por app con el micro en uso), en el orden del registro.
///
/// - `mic_apps`: `(app, en_uso)` tal como salen del registro (se normalizan aquí).
/// - `titles`: `(exe, título)` de las ventanas visibles, solo se miran las de navegadores.
///
/// Devuelve nombres legibles (para el aviso y para seguir a la app). Ignora Pronto y lo que no
/// usa el micro.
pub fn detect_all(mic_apps: &[(String, bool)], titles: &[(String, String)]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (raw, in_use) in mic_apps {
        if !in_use {
            continue;
        }
        let app = normalize_app(raw);
        if app.is_empty() || app.contains("pronto") {
            continue;
        }
        let native = if app.contains("zoom") {
            Some("Zoom")
        } else if app.contains("teams") {
            Some("Microsoft Teams")
        } else if app.contains("webex") || app.contains("ciscocollab") {
            Some("Webex")
        } else if app.contains("slack") {
            Some("Slack (huddle)")
        } else {
            None
        };
        let label = if let Some(label) = native {
            Some(label.to_string())
        } else if let Some((_, browser)) = BROWSERS.iter().find(|(exe, _)| app == *exe) {
            titles
                .iter()
                .filter(|(exe, _)| normalize_app(exe) == app)
                .find_map(|(_, title)| title_has_keyword(title))
                .map(|service| format!("{service} ({browser})"))
        } else {
            None
        };
        if let Some(l) = label {
            if !out.contains(&l) {
                out.push(l);
            }
        }
    }
    out
}

/// La primera reunión en curso, si la hay.
#[cfg(test)]
pub fn detect_meeting(mic_apps: &[(String, bool)], titles: &[(String, String)]) -> Option<String> {
    detect_all(mic_apps, titles).into_iter().next()
}

/// ¿Alguna de las apps con el micro en uso es un navegador? (para saber si hace falta mirar títulos)
pub fn any_browser_in_use(mic_apps: &[(String, bool)]) -> bool {
    mic_apps.iter().any(|(raw, in_use)| *in_use && BROWSERS.iter().any(|(exe, _)| normalize_app(raw) == *exe))
}

// ------------------------------------------------------------------ lectura del sistema

fn enum_subkeys(key: HKEY) -> Vec<String> {
    let mut out = Vec::new();
    let mut index = 0u32;
    loop {
        let mut buf = [0u16; 512];
        let mut len = buf.len() as u32;
        // SAFETY: búfer local con su longitud; la clave está abierta.
        let r = unsafe { RegEnumKeyExW(key, index, Some(PWSTR(buf.as_mut_ptr())), &mut len, None, None, None, None) };
        if r.0 != 0 {
            break;
        }
        out.push(String::from_utf16_lossy(&buf[..len as usize]));
        index += 1;
    }
    out
}

fn read_qword(key: HKEY, name: PCWSTR) -> Option<u64> {
    let mut data = [0u8; 8];
    let mut size = 8u32;
    let mut kind = REG_VALUE_TYPE(0);
    // SAFETY: búfer local de 8 bytes.
    let r = unsafe { RegQueryValueExW(key, name, None, Some(&mut kind), Some(data.as_mut_ptr()), Some(&mut size)) };
    (r.0 == 0 && size == 8).then(|| u64::from_le_bytes(data))
}

fn open(parent: HKEY, sub: PCWSTR) -> Option<HKEY> {
    let mut key = HKEY::default();
    // SAFETY: `key` es un destino válido.
    let r = unsafe { RegOpenKeyExW(parent, sub, None, KEY_READ, &mut key) };
    (r.0 == 0).then_some(key)
}

fn in_use(key: HKEY) -> bool {
    let start = read_qword(key, w!("LastUsedTimeStart")).unwrap_or(0);
    let stop = read_qword(key, w!("LastUsedTimeStop")).unwrap_or(1);
    start != 0 && stop == 0
}

/// Apps (paquete o exe) con su estado de uso del micrófono, según el registro.
pub fn read_mic_consent() -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let Some(root) = open(HKEY_CURRENT_USER, MIC_KEY) else { return out };
    for name in enum_subkeys(root) {
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        let Some(sub) = open(root, PCWSTR(wide.as_ptr())) else { continue };
        if name.eq_ignore_ascii_case("NonPackaged") {
            for exe in enum_subkeys(sub) {
                let wide: Vec<u16> = exe.encode_utf16().chain(std::iter::once(0)).collect();
                if let Some(k) = open(sub, PCWSTR(wide.as_ptr())) {
                    out.push((exe, in_use(k)));
                    // SAFETY: clave abierta arriba.
                    unsafe {
                        let _ = RegCloseKey(k);
                    }
                }
            }
        } else {
            out.push((name, in_use(sub)));
        }
        // SAFETY: clave abierta arriba.
        unsafe {
            let _ = RegCloseKey(sub);
        }
    }
    // SAFETY: clave abierta arriba.
    unsafe {
        let _ = RegCloseKey(root);
    }
    out
}

unsafe extern "system" fn collect_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: lparam es el puntero al Vec que nos pasó `window_titles`, vivo durante EnumWindows.
    let list = unsafe { &mut *(lparam.0 as *mut Vec<(String, String)>) };
    if unsafe { IsWindowVisible(hwnd) }.as_bool() {
        let info = inject::app_info_of(hwnd);
        if !info.title.is_empty() && !info.exe.is_empty() {
            list.push((info.exe, info.title));
        }
    }
    BOOL(1)
}

/// `(exe, título)` de las ventanas visibles con título.
pub fn window_titles() -> Vec<(String, String)> {
    let mut list: Vec<(String, String)> = Vec::new();
    // SAFETY: el puntero a `list` vive hasta que EnumWindows termina.
    unsafe {
        let _ = EnumWindows(Some(collect_window), LPARAM(&mut list as *mut _ as isize));
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apps(list: &[(&str, bool)]) -> Vec<(String, bool)> {
        list.iter().map(|(a, u)| (a.to_string(), *u)).collect()
    }
    fn titles(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter().map(|(a, t)| (a.to_string(), t.to_string())).collect()
    }

    #[test]
    fn nombres_del_registro() {
        assert_eq!(normalize_app("C:#Program Files#Zoom#bin#Zoom.exe"), "zoom");
        assert_eq!(normalize_app("MSTeams_8wekyb3d8bbwe"), "msteams");
        assert_eq!(normalize_app("C:#Users#x#AppData#Local#Programs#pronto.exe"), "pronto");
        assert_eq!(normalize_app("chrome.exe"), "chrome");
    }

    #[test]
    fn apps_de_reuniones_nativas() {
        assert_eq!(detect_meeting(&apps(&[("C:#Program Files#Zoom#bin#Zoom.exe", true)]), &[]).as_deref(), Some("Zoom"));
        assert_eq!(detect_meeting(&apps(&[("MSTeams_8wekyb3d8bbwe", true)]), &[]).as_deref(), Some("Microsoft Teams"));
        assert_eq!(detect_meeting(&apps(&[("C:#x#ms-teams.exe", true)]), &[]).as_deref(), Some("Microsoft Teams"));
        assert_eq!(detect_meeting(&apps(&[("C:#x#CiscoCollabHost.exe", true)]), &[]).as_deref(), Some("Webex"));
        assert_eq!(detect_meeting(&apps(&[("C:#x#slack.exe", true)]), &[]).as_deref(), Some("Slack (huddle)"));
    }

    #[test]
    fn solo_cuenta_lo_que_esta_en_uso_ahora() {
        assert_eq!(detect_meeting(&apps(&[("C:#x#Zoom.exe", false)]), &[]), None);
        assert_eq!(detect_meeting(&[], &[]), None);
    }

    #[test]
    fn ignora_a_pronto_y_a_otras_apps() {
        assert_eq!(detect_meeting(&apps(&[("C:#x#pronto.exe", true)]), &[]), None);
        assert_eq!(detect_meeting(&apps(&[("C:#x#Pronto.exe", true), ("C:#x#obs64.exe", true), ("Microsoft.WindowsCamera_8wekyb3d8bbwe", true)]), &[]), None);
        // Pronto usando el micro no tapa una reunión real.
        assert_eq!(detect_meeting(&apps(&[("C:#x#pronto.exe", true), ("C:#x#zoom.exe", true)]), &[]).as_deref(), Some("Zoom"));
    }

    #[test]
    fn navegador_solo_si_el_titulo_delata_una_videollamada() {
        let mic = apps(&[("C:#Program Files#Google#Chrome#Application#chrome.exe", true)]);
        let meet = titles(&[("chrome.exe", "Meet - abc-defg-hij - Google Chrome"), ("code.exe", "main.rs")]);
        assert_eq!(detect_meeting(&mic, &meet).as_deref(), Some("Google Meet (Chrome)"));
        let yt = titles(&[("chrome.exe", "Vídeo gracioso - YouTube")]);
        assert_eq!(detect_meeting(&mic, &yt), None);
        // «Meetup» no es «Meet».
        assert_eq!(detect_meeting(&mic, &titles(&[("chrome.exe", "Meetup Madrid - Chrome")])), None);
        // El título es de otro navegador distinto al que usa el micro.
        assert_eq!(detect_meeting(&mic, &titles(&[("msedge.exe", "Meet - abc")])), None);
        // Micro de Edge con pestaña de Teams.
        let edge = apps(&[("C:#x#msedge.exe", true)]);
        assert_eq!(detect_meeting(&edge, &titles(&[("msedge.exe", "Reunión | Microsoft Teams")])).as_deref(), Some("Microsoft Teams (Edge)"));
        // Con el micro cerrado, aunque el título diga Meet, no.
        assert_eq!(detect_meeting(&apps(&[("C:#x#chrome.exe", false)]), &meet), None);
    }

    #[test]
    fn varias_reuniones_a_la_vez_salen_todas() {
        let mic = apps(&[("C:#x#zoom.exe", true), ("MSTeams_8wekyb3d8bbwe", true), ("C:#x#zoom2.exe", true)]);
        assert_eq!(detect_all(&mic, &[]), ["Zoom", "Microsoft Teams"]);
    }

    #[test]
    fn necesita_mirar_titulos_solo_con_navegador() {
        assert!(any_browser_in_use(&apps(&[("C:#x#chrome.exe", true)])));
        assert!(!any_browser_in_use(&apps(&[("C:#x#chrome.exe", false), ("C:#x#zoom.exe", true)])));
    }

    #[test]
    fn lectura_del_registro_no_falla() {
        // No asumimos qué hay, solo que leerlo no revienta y los nombres son legibles.
        for (name, _) in read_mic_consent() {
            assert!(!name.is_empty());
        }
        let _ = window_titles();
    }
}
