//! Lo propio de macOS. Cada archivo es la hermana Mac de una parte atada a Windows y tiene las mismas
//! firmas públicas que la función `#[cfg(windows)]` a la que sustituye (decisión D1 de
//! `specs/mac/DECISIONES.md`). El resto de Pronto es común y llega solo al Mac.
//!
//! Nada de aquí se ha ejecutado en un Mac todavía: se comprueba que compila con `npm run mac:check`
//! y se prueba con `scripts/mac/probar.sh` y `docs/mac/PROBAR.md`.

pub mod aisetup;
pub mod app;
pub mod autostart;
pub mod detect;
pub mod flowbar;
pub mod hotkey;
pub mod inject;
pub mod media;
pub mod permisos;
pub mod whisper;
