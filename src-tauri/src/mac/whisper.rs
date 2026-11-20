//! macOS: GPU y CPU para Whisper (decisión D4), hermanas de `whisper::gpu` y del AVX2 de Windows.
//!
//! En Apple Silicon whisper.cpp va por Metal (feature `metal` de `whisper-rs`) y siempre hay GPU. No
//! hay memoria de vídeo aparte: CPU y GPU comparten la RAM («memoria unificada»). Para filtrar el
//! catálogo (`min_vram_gb`) y decidir el latido (`pair_vram_gb`) se da como «VRAM» lo que Metal deja
//! usar a la GPU sin apretar al sistema: `recommendedMaxWorkingSetSize` ronda 2/3 de la RAM en los Mac
//! de hasta 36 GB, y además se reservan 4 GB para macOS, Pronto (Parakeet) y Ollama. Así un Mac de
//! 8 GB ve 4 GB (turbo sí, sin el contexto doble del latido) y uno de 16 GB ve 10,7 GB.
//!
//! La RAM y el nombre del chip salen de `sysctl` (`/usr/sbin/sysctl -n hw.memsize`), con plazo: `libc`
//! no es dependencia directa y `sysctl` es la vía documentada y estable.

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const SYSCTL: &str = "/usr/sbin/sysctl";
const SYSCTL_TIMEOUT: Duration = Duration::from_secs(2);
const GIB: u64 = 1024 * 1024 * 1024;
/// RAM que se deja a macOS y al resto de Pronto antes de dar memoria a Whisper.
const RESERVED: u64 = 4 * GIB;

/// whisper.cpp para Mac se compila para arm64 (NEON): en Apple Silicon siempre vale (D4).
pub(crate) fn cpu_supported() -> bool {
    cfg!(target_arch = "aarch64")
}

/// «VRAM» de la memoria unificada: el menor de 2/3 de la RAM y la RAM menos `RESERVED`.
pub(crate) fn unified_vram(ram: u64) -> u64 {
    (ram / 3 * 2).min(ram.saturating_sub(RESERVED))
}

/// `sysctl -n <clave>` con plazo; `None` si falla o no responde.
fn sysctl(key: &str) -> Option<String> {
    let mut child = Command::new(SYSCTL).arg("-n").arg(key).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(st)) if st.success() => break,
            Ok(Some(_)) | Err(_) => return None,
            Ok(None) if started.elapsed() > SYSCTL_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
        }
    }
    let mut out = String::new();
    child.stdout.take()?.read_to_string(&mut out).ok()?;
    let out = out.trim().to_string();
    (!out.is_empty()).then_some(out)
}

/// RAM total del Mac en bytes (`hw.memsize`).
pub(crate) fn ram_bytes() -> Option<u64> {
    sysctl("hw.memsize")?.parse().ok()
}

/// La GPU con la que correría Whisper: nombre del chip y su «VRAM» (`unified_vram`). Misma firma que
/// la de Windows. `None` solo si no se puede leer la RAM (se apunta y Whisper no se ofrece). Se mira
/// una vez por arranque.
pub fn gpu() -> Option<(String, u64)> {
    static GPU: std::sync::OnceLock<Option<(String, u64)>> = std::sync::OnceLock::new();
    GPU.get_or_init(|| {
        if !cpu_supported() {
            return None;
        }
        let Some(ram) = ram_bytes() else {
            crate::errlog::error("whisper", "no se pudo leer la memoria del Mac (sysctl hw.memsize): Whisper no se ofrece");
            return None;
        };
        let chip = sysctl("machdep.cpu.brand_string").unwrap_or_else(|| "Apple Silicon".into());
        Some((format!("{chip} (Metal)"), unified_vram(ram)))
    })
    .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vram_de_la_memoria_unificada() {
        assert_eq!(unified_vram(8 * GIB), 4 * GIB);
        assert_eq!(unified_vram(16 * GIB), 16 * GIB / 3 * 2);
        assert_eq!(unified_vram(2 * GIB), 0);
        assert!(gpu().is_some_and(|(_, v)| v > 0), "un Mac de Apple Silicon tiene GPU Metal");
    }
}
