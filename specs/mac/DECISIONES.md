# Pronto 2 para Mac — decisiones (20-nov-2026)

Consultadas con un asesor (Fable) antes de empezar. Decide el responsable del encargo; Txonso puede cambiarlas.

| # | Decisión | Porqué |
|---|---|---|
| D1 | **Un solo código.** Cada función que toca el sistema lleva `#[cfg(windows)]` y su hermana Mac vive en `src-tauri/src/mac/<área>.rs` con la misma firma, traída con `#[cfg(target_os = "macos")] pub use crate::mac::<área>::...`. | Lo común (motores, pulido, historial, reuniones, panel) se escribe una vez: cada mejora de Windows llega sola al Mac. Los archivos comunes solo ganan anotaciones, así la unión con otras ramas es fácil. |
| D2 | **Guarda:** `npm test` corre `cargo check` para `aarch64-apple-darwin` (`tests/mac.test.mjs`). Si alguien añade código Windows sin su hermano Mac, falla en Windows. `compile_error!` en cualquier otro sistema. | Sin Mac a mano, la única forma de no romper el Mac en silencio. No enlaza: los fallos de enlace solo salen en un Mac (CI o el amigo). |
| D3 | **Pulido con Ollama** también en Mac. Apple Foundation Models (macOS 26) queda como opción futura. | Mismos prompts y mismas guardas de la regla 7. FM exige macOS 26, Apple Intelligence activado y un puente Swift imposible de probar desde aquí. |
| D4 | **Solo Apple Silicon (arm64)**, macOS 14 o superior. Audio del sistema en reuniones desde macOS 14.6 (por debajo, la reunión graba solo el micro). | Intel deja de recibir macOS después de 26; Ollama y la regla 3 no están garantizados allí. Ollama pide macOS 14. |
| D5 | **Se construye en GitHub Actions** (`macos-15`, gratis en repos públicos) → `.dmg` como artefacto. Necesita el «sube» de Txonso. | Solo un Mac puede enlazar y empaquetar. Así el amigo solo prueba. |
| D6 | **Firma autofirmada estable** (certificado propio, gratis, guardado como secreto de GitHub). Sin secreto, firma ad-hoc. Sin notarizar. Sin «hardened runtime». | Con ad-hoc macOS olvida los permisos (Accesibilidad, Monitorización de entrada) en cada versión. Sin notarizar, el primer arranque pide «Abrir igualmente» en Ajustes. |
| D7 | **Atajo Fn/Globo** con un CGEventTap activo (keycode 63, `flagsChanged`), igual que el gancho de Windows: solo traga Esc/Enter con la vista previa abierta. | Como Wispr Flow. Se avisa si la tecla Globo está configurada para emojis o dictado de Apple (`AppleFnUsageType` ≠ 0). |
| D8 | **Pegado:** Cmd+V sintético (keycode 9 con solo la marca Cmd), tras soltar Fn; se guarda y devuelve el portapapeles; antes de pegar se reactiva la app que tenía el foco. | La Flow bar no es un NSPanel: si el usuario la pulsa, Pronto se activa y el pegado caería en Pronto. |
| D9 | **Actualizador en Mac:** avisa de la versión nueva y abre la página del release. Sin instalación automática en esta versión. | Sin notarizar y sin Mac para probarlo, cambiar el .app en caliente es arriesgado. |
| D10 | **Barra de menús:** icono plantilla, menú al pulsar; Pronto sin icono en el Dock salvo con el panel abierto. | Lo que espera un usuario de Mac de una utilidad como esta. |
| D11 | **Arranque al iniciar sesión** con un LaunchAgent en `~/Library/LaunchAgents`. | Funciona sin cuenta de Apple de pago. |
| D12 | **Regla 3 en Mac:** el kit mide 15 s de audio contra 1068 ms y lo informa. Si un Mac no llega, cambiar el umbral es decisión de Txonso. | Es «NO SE NEGOCIA»: no se toca sin él. |
