//! «Iniciar con Windows»: valor en `HKCU\...\Run`. Arranca con `--hidden` para no abrir el hub.
//! En Mac (decisión D11), un LaunchAgent en `~/Library/LaunchAgents` (`mac/autostart.rs`).

#[cfg(windows)]
use windows::core::{w, PCWSTR};
#[cfg(windows)]
use windows::Win32::System::Registry::{RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_SZ};

pub const HIDDEN_FLAG: &str = "--hidden";

/// Etiqueta (y nombre del fichero) del LaunchAgent de macOS.
#[cfg_attr(windows, allow(dead_code))]
pub const LAUNCH_AGENT_LABEL: &str = "com.beltranlabs.pronto2";

#[cfg(target_os = "macos")]
pub use crate::mac::autostart::set;

#[cfg(windows)]
pub fn set(enabled: bool) -> Result<(), String> {
    // La versión de pruebas (`PRONTO_DATOS`) no toca el arranque con Windows: es del Pronto instalado.
    if crate::db::test_build() {
        return Ok(());
    }
    let mut key = HKEY::default();
    // SAFETY: llamadas al registro con punteros a datos locales que viven durante la llamada.
    unsafe {
        RegOpenKeyExW(HKEY_CURRENT_USER, w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run"), None, KEY_SET_VALUE, &mut key)
            .ok()
            .map_err(|e| e.to_string())?;
        let result = if enabled {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let cmd = format!("\"{}\" {}", exe.display(), HIDDEN_FLAG);
            let wide: Vec<u16> = cmd.encode_utf16().chain(std::iter::once(0)).collect();
            let bytes = std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);
            RegSetValueExW(key, w!("Pronto2"), None, REG_SZ, Some(bytes)).ok().map_err(|e| e.to_string())
        } else {
            // Si el valor no existe, ya está como queremos.
            let _ = RegDeleteValueW(key, PCWSTR(w!("Pronto2").0));
            Ok(())
        };
        let _ = RegCloseKey(key);
        result
    }
}

/// Escapa texto para un `<string>` de un plist XML.
#[cfg_attr(windows, allow(dead_code))]
fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // XML 1.0 no admite controles (salvo tab y saltos): fuera, no hay ruta legítima con ellos.
            c if c.is_control() && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

/// Plist del LaunchAgent de macOS que abre `exe` (el binario dentro de `Pronto.app/Contents/MacOS`)
/// con `--hidden` al iniciar sesión: `RunAtLoad`, proceso interactivo y solo en la sesión gráfica
/// (`Aqua`). Pura para probarla también en Windows.
#[cfg_attr(windows, allow(dead_code))]
pub fn launch_agent_plist(exe: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>{label}</string>
	<key>ProgramArguments</key>
	<array>
		<string>{exe}</string>
		<string>{hidden}</string>
	</array>
	<key>RunAtLoad</key>
	<true/>
	<key>ProcessType</key>
	<string>Interactive</string>
	<key>LimitLoadToSessionType</key>
	<string>Aqua</string>
</dict>
</plist>
"#,
        label = xml_escape(LAUNCH_AGENT_LABEL),
        exe = xml_escape(exe),
        hidden = xml_escape(HIDDEN_FLAG),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plist_del_launch_agent() {
        let p = launch_agent_plist("/Applications/Pronto.app/Contents/MacOS/pronto");
        assert!(p.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\">"));
        assert!(p.contains("<key>Label</key>\n\t<string>com.beltranlabs.pronto2</string>"));
        assert!(p.contains("<array>\n\t\t<string>/Applications/Pronto.app/Contents/MacOS/pronto</string>\n\t\t<string>--hidden</string>\n\t</array>"));
        assert!(p.contains("<key>RunAtLoad</key>\n\t<true/>"));
        assert!(p.contains("<key>ProcessType</key>\n\t<string>Interactive</string>"));
        assert!(p.contains("<key>LimitLoadToSessionType</key>\n\t<string>Aqua</string>"));
        assert!(p.trim_end().ends_with("</plist>"));
    }

    #[test]
    fn plist_escapa_la_ruta() {
        let p = launch_agent_plist("/Users/a&b/<x>\"q'/Pronto.app/Contents/MacOS/pronto\u{0}\u{7}");
        assert!(p.contains("<string>/Users/a&amp;b/&lt;x&gt;&quot;q&apos;/Pronto.app/Contents/MacOS/pronto</string>"), "{p}");
        assert_eq!(p.matches("<string>").count(), 5, "ni una etiqueta de más: {p}");
    }
}
