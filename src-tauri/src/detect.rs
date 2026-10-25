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

/// Ejecutables (normalizados con `normalize_app`) de las apps de reuniones, con su nombre. Nombre
/// EXACTO, no subcadena: `TeamSpeak.exe` no es Teams ni `ZoomIt64.exe` es Zoom (rompedor
/// 22-oct-c, MEDIA 2; PRD §1: no se graban Discord ni juegos).
const NATIVE_APPS: &[(&str, &str)] = &[
    ("zoom", "Zoom"),
    ("ms-teams", "Microsoft Teams"),        // Teams nuevo (exe)
    ("msteams", "Microsoft Teams"),         // Teams nuevo (paquete MSTeams_8wekyb3d8bbwe)
    ("microsoftteams", "Microsoft Teams"),  // Teams de Windows 11 (paquete MicrosoftTeams_…)
    ("teams", "Microsoft Teams"),           // Teams clásico
    ("webex", "Webex"),
    ("ciscocollabhost", "Webex"),
    ("webexhost", "Webex"),
    ("atmgr", "Webex"),                     // Webex Meetings clásico
    ("slack", "Slack (huddle)"),
];

/// `C:#Program Files#Zoom#bin#Zoom.exe` -> `zoom`; `MSTeams_8wekyb3d8bbwe` -> `msteams`.
pub fn normalize_app(raw: &str) -> String {
    let last = raw.rsplit(['#', '\\', '/']).next().unwrap_or(raw);
    let last = last.split('_').next().unwrap_or(last);
    last.to_lowercase().trim_end_matches(".exe").to_string()
}

/// ¿Es el código de una sala de Meet («abc-defg-hij»)?
fn is_meet_code(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    parts.len() == 3 && parts.iter().zip([3, 4, 3]).all(|(p, n)| p.len() == n && p.chars().all(|c| c.is_ascii_lowercase()))
}

/// Primera palabra del título de Teams que delata una llamada o reunión EN CURSO («Reunión con Ana |
/// Microsoft Teams»). «Chat», «Calendario», «Llamadas», «Reuniones» o la página del producto también
/// acaban en «| Microsoft Teams» y no son una llamada (rompedor 23-oct, reuniones MEDIA 3).
const TEAMS_CALL_WORDS: &[&str] = &["reunión", "reunion", "meeting", "llamada", "videollamada", "call", "videocall"];

/// Lo que queda tras un título de servicio («Zoom Meeting» + « - Google Chrome»): solo vale si es
/// nada o el sufijo del navegador, sin más texto propio de la página («: tutorial…», « - YouTube…»).
fn only_browser_suffix(rest: &str) -> bool {
    let rest = rest.trim();
    if rest.is_empty() {
        return true;
    }
    let seps = ["- ", "– ", "— ", "| "];
    let Some(tail) = seps.iter().find_map(|sep| rest.strip_prefix(sep)) else { return false };
    !seps.iter().any(|sep| tail.contains(&format!(" {sep}")))
}

/// El servicio de videollamada cuyo título PROPIO lleva la pestaña: «Meet - abc-defg-hij»,
/// «Reunión | Microsoft Teams», «Zoom Meeting», «Sala | Jitsi Meet», «… | Whereby». Una palabra
/// suelta («Cómo hacer zoom en fotos», «#teams | Mi servidor») no basta, ni una página de Teams que
/// no es una llamada, ni un artículo que empieza como el título de Zoom o Jitsi; y Discord nunca
/// cuenta.
fn title_service(title: &str) -> Option<&'static str> {
    let low = title.trim().to_lowercase();
    if low.contains("discord") {
        return None;
    }
    // Avisos que el sitio antepone al título: «(2) Meet - …».
    let low = match low.strip_prefix('(').and_then(|r| r.split_once(") ")) {
        Some((n, rest)) if n.chars().all(|c| c.is_ascii_digit()) => rest.to_string(),
        _ => low,
    };
    let meet = ["meet - ", "meet – ", "meet — "].iter().any(|p| low.starts_with(p))
        && low.split(|c: char| c.is_whitespace()).any(is_meet_code);
    if meet || low.starts_with("meet.google.com") {
        return Some("Google Meet");
    }
    if let Some(i) = low.find("| microsoft teams") {
        let first = low[..i].split(|c: char| !c.is_alphanumeric()).find(|w| !w.is_empty()).unwrap_or("");
        return TEAMS_CALL_WORDS.contains(&first).then_some("Microsoft Teams");
    }
    let after = |prefixes: &[&str]| prefixes.iter().find_map(|p| low.strip_prefix(p)).is_some_and(only_browser_suffix);
    if after(&["zoom meeting", "zoom webinar", "reunión de zoom", "zoom workplace"]) {
        return Some("Zoom");
    }
    if after(&["jitsi meet"]) || low.contains("| jitsi meet") || low.starts_with("meet.jit.si") {
        return Some("Jitsi");
    }
    if low.contains("| whereby") {
        return Some("Whereby");
    }
    None
}

/// ¿Es el título de una ventana de Discord o WhatsApp (llamadas privadas, fuera del PRD §1)?
fn is_private_call_window(title: &str) -> bool {
    let low = title.to_lowercase();
    low.contains("discord") || low.starts_with("whatsapp") || low.contains("- whatsapp")
}

/// Servicios que la detección AUTOMÁTICA puede grabar sola: PRD §1 «decidido», «la detección
/// automática se limita a Teams, Zoom y Meet». Slack, Webex, Jitsi y Whereby se detectan (el modo
/// «ask» los puede avisar) pero no se graban solos (rompedor 23-oct, reuniones MEDIA 3).
pub fn is_auto_service(label: &str) -> bool {
    ["Zoom", "Microsoft Teams", "Google Meet"].iter().any(|s| label == *s || label.starts_with(&format!("{s} (")))
}

/// Resultado de mirar el sistema una vez (`scan`).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Scan {
    /// Reuniones en curso (una por app con el micro en uso), en el orden del registro.
    pub meetings: Vec<String>,
    /// De `meetings`, las de navegador que tienen además OTRA ventana de Discord o WhatsApp en el
    /// mismo navegador: Windows no dice qué pestaña tiene el micro, así que puede ser esa llamada
    /// privada (PRD §1: no se graban). No se disparan solas: se avisan como en «ask».
    pub ambiguous: Vec<String>,
    /// Navegadores (`"Chrome"`, `"Edge"`…) que tienen el micro en uso ahora, sea cual sea su pestaña:
    /// la reunión automática de navegador se sigue por esto y no por el título, que cambia al
    /// cambiar de pestaña.
    pub browsers: Vec<String>,
}

/// Todas las reuniones en curso (una por app con el micro en uso), en el orden del registro.
///
/// - `mic_apps`: `(app, en_uso)` tal como salen del registro (se normalizan aquí).
/// - `titles`: `(exe, título)` de las ventanas visibles, solo se miran las de navegadores.
///
/// Devuelve nombres legibles (para el aviso y para seguir a la app). Ignora Pronto y lo que no
/// usa el micro.
pub fn detect_all(mic_apps: &[(String, bool)], titles: &[(String, String)]) -> Vec<String> {
    scan(mic_apps, titles).meetings
}

/// Como `detect_all`, con lo que la máquina del modo automático necesita saber además.
pub fn scan(mic_apps: &[(String, bool)], titles: &[(String, String)]) -> Scan {
    let mut out = Scan::default();
    for (raw, in_use) in mic_apps {
        if !in_use {
            continue;
        }
        let app = normalize_app(raw);
        if app.is_empty() || app.contains("pronto") {
            continue;
        }
        let native = NATIVE_APPS.iter().find(|(exe, _)| app == *exe).map(|(_, label)| *label);
        let mut private_window = false;
        let label = if let Some(label) = native {
            Some(label.to_string())
        } else if let Some((_, browser)) = BROWSERS.iter().find(|(exe, _)| app == *exe) {
            if !out.browsers.iter().any(|b| b == browser) {
                out.browsers.push(browser.to_string());
            }
            // Solo las ventanas de ESE navegador (el que usa el micro) y solo con el título propio
            // de la videollamada. Windows no dice qué pestaña tiene el micro: el título estricto
            // es lo que evita que otra ventana con «zoom» o «teams» en el título dispare.
            let own: Vec<&str> = titles.iter().filter(|(exe, _)| normalize_app(exe) == app).map(|(_, t)| t.as_str()).collect();
            private_window = own.iter().any(|t| is_private_call_window(t));
            own.iter().find_map(|t| title_service(t)).map(|service| format!("{service} ({browser})"))
        } else {
            None
        };
        if let Some(l) = label {
            if private_window && !out.ambiguous.contains(&l) {
                out.ambiguous.push(l.clone());
            }
            if !out.meetings.contains(&l) {
                out.meetings.push(l);
            }
        }
    }
    out
}

/// Las reuniones que el detector entrega a la máquina de `auto.rs`. En modo «Automático»
/// (`auto_mode`) solo las que se pueden grabar solas: Teams, Zoom y Meet (PRD §1) y no dudosas
/// (`Scan::ambiguous`); en «ask», todas, porque solo se avisa.
pub fn candidates(scan: &Scan, auto_mode: bool) -> Vec<String> {
    if auto_mode {
        scan.meetings.iter().filter(|l| is_auto_service(l) && !scan.ambiguous.contains(l)).cloned().collect()
    } else {
        scan.meetings.clone()
    }
}

/// Navegador de una etiqueta de reunión de navegador («Microsoft Teams (Edge)» -> `Edge`).
pub fn browser_of(label: &str) -> Option<&str> {
    let (_, rest) = label.rsplit_once(" (")?;
    let name = rest.strip_suffix(')')?;
    BROWSERS.iter().any(|(_, b)| *b == name).then_some(name)
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

    /// Rompedor 22-oct-c, MEDIA 2 (PRD §1: ni Discord ni juegos): exe por nombre exacto y, en el
    /// navegador, solo el título propio de la videollamada.
    #[test]
    fn sin_falsos_positivos_y_con_los_legitimos() {
        let chrome = apps(&[("C:#Program Files#Google#Chrome#Application#chrome.exe", true)]);
        // Los cuatro casos del rompedor.
        assert_eq!(detect_all(&apps(&[("C:#Program Files#TeamSpeak#TeamSpeak.exe", true)]), &[]), Vec::<String>::new());
        assert_eq!(detect_all(&apps(&[("C:#Tools#ZoomIt64.exe", true)]), &[]), Vec::<String>::new());
        assert_eq!(detect_all(&chrome, &titles(&[("chrome.exe", "#teams | Mi servidor - Discord - Google Chrome")])), Vec::<String>::new());
        assert_eq!(
            detect_all(&chrome, &titles(&[("chrome.exe", "WhatsApp - Google Chrome"), ("chrome.exe", "Cómo hacer zoom en fotos - Google Chrome")])),
            Vec::<String>::new()
        );
        // Más de lo mismo: Discord nativo, palabras sueltas y Discord aunque copie el título de Teams.
        for t in ["Zoom - Wikipedia - Google Chrome", "teams de fútbol - Google Chrome", "meet the press - Google Chrome", "Reunión | Microsoft Teams - Discord"] {
            assert_eq!(detect_all(&chrome, &titles(&[("chrome.exe", t)])), Vec::<String>::new(), "{t}");
        }
        for exe in ["C:#x#Discord.exe", "C:#x#steam.exe", "C:#x#zoomit.exe", "C:#x#teamspeak3.exe", "C:#x#teamviewer.exe"] {
            assert_eq!(detect_all(&apps(&[(exe, true)]), &[]), Vec::<String>::new(), "{exe}");
        }
        // Los legítimos.
        assert_eq!(detect_all(&apps(&[("C:#Program Files#Zoom#bin#Zoom.exe", true)]), &[]), ["Zoom"]);
        assert_eq!(detect_all(&apps(&[("C:#x#Teams.exe", true)]), &[]), ["Microsoft Teams"]);
        assert_eq!(detect_all(&apps(&[("MicrosoftTeams_8wekyb3d8bbwe", true)]), &[]), ["Microsoft Teams"]);
        for (t, want) in [
            ("Meet - abc-defg-hij - Google Chrome", "Google Meet (Chrome)"),
            ("(1) Meet – abc-defg-hij - Google Chrome", "Google Meet (Chrome)"),
            ("Reunión con Ana | Microsoft Teams - Google Chrome", "Microsoft Teams (Chrome)"),
            ("Zoom Meeting - Google Chrome", "Zoom (Chrome)"),
            ("Diario | Jitsi Meet - Google Chrome", "Jitsi (Chrome)"),
            ("Sala de Ana | Whereby - Google Chrome", "Whereby (Chrome)"),
        ] {
            assert_eq!(detect_all(&chrome, &titles(&[("chrome.exe", "WhatsApp - Google Chrome"), ("chrome.exe", t)])), [want], "{t}");
        }
    }

    /// Rompedor 23-oct, reuniones MEDIA 3: páginas de Teams que no son una llamada, artículos que
    /// empiezan como el título de Zoom o Jitsi, y otra ventana de Discord o WhatsApp.
    #[test]
    fn titulos_genericos_no_bastan() {
        let chrome = apps(&[("C:#x#chrome.exe", true)]);
        for t in [
            "Chat | Microsoft Teams - Google Chrome",
            "Calendario | Microsoft Teams - Google Chrome",
            "Llamadas | Microsoft Teams - Google Chrome",
            "Reuniones | Microsoft Teams - Google Chrome",
            "Videoconferencias, reuniones y llamadas | Microsoft Teams - Google Chrome",
            "Zoom Meeting: tutorial para principiantes - YouTube - Google Chrome",
            "Zoom Meeting - Cómo usarlo - Xataka - Google Chrome",
            "Jitsi Meet, la alternativa gratis a Zoom - Xataka - Google Chrome",
        ] {
            assert_eq!(detect_all(&chrome, &titles(&[("chrome.exe", t)])), Vec::<String>::new(), "{t}");
        }
        for (t, want) in [
            ("Reunión semanal | Microsoft Teams - Google Chrome", "Microsoft Teams (Chrome)"),
            ("Meeting with Ana | Microsoft Teams - Google Chrome", "Microsoft Teams (Chrome)"),
            ("Zoom Meeting - Google Chrome", "Zoom (Chrome)"),
            ("Jitsi Meet - Google Chrome", "Jitsi (Chrome)"),
        ] {
            assert_eq!(detect_all(&chrome, &titles(&[("chrome.exe", t)])), [want], "{t}");
        }
        // Una llamada de Teams con otra ventana de WhatsApp o Discord del mismo navegador: dudosa.
        let s = scan(&chrome, &titles(&[("chrome.exe", "Reunión | Microsoft Teams - Google Chrome"), ("chrome.exe", "WhatsApp - Google Chrome")]));
        assert_eq!((s.meetings.as_slice(), s.ambiguous.as_slice(), s.browsers.as_slice()), (&["Microsoft Teams (Chrome)".to_string()][..], &["Microsoft Teams (Chrome)".to_string()][..], &["Chrome".to_string()][..]));
        assert!(candidates(&s, true).is_empty(), "no se graba sola");
        assert_eq!(candidates(&s, false), ["Microsoft Teams (Chrome)"], "en «ask» se avisa");
        // PRD §1: Slack, Webex, Jitsi y Whereby no se graban solos, solo se avisan.
        let s = scan(&apps(&[("C:#x#slack.exe", true), ("C:#x#CiscoCollabHost.exe", true), ("C:#x#zoom.exe", true)]), &[]);
        assert_eq!(candidates(&s, true), ["Zoom"]);
        assert_eq!(candidates(&s, false), ["Slack (huddle)", "Webex", "Zoom"]);
        assert_eq!(browser_of("Microsoft Teams (Edge)"), Some("Edge"));
        assert_eq!(browser_of("Slack (huddle)"), None);
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
