//! «Iniciar con Windows»: valor en `HKCU\...\Run`. Arranca con `--hidden` para no abrir el hub.

use windows::core::{w, PCWSTR};
use windows::Win32::System::Registry::{RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_SZ};

pub const HIDDEN_FLAG: &str = "--hidden";

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
