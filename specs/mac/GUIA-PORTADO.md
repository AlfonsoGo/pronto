# Guía para portar una parte de Pronto a macOS

Lee antes `specs/mac/DECISIONES.md` (D1-D12). Resumen de las normas:

1. **No muevas ni reescribas código Windows.** En el archivo común (`src-tauri/src/<area>.rs`) solo:
   - `#[cfg(windows)]` delante de cada función, `use`, `const`, `static` o bloque que toca Win32,
     PowerShell, `.exe`, registro, `std::os::windows`, etc.
   - Al lado, `#[cfg(target_os = "macos")] pub(crate) use crate::mac::<area>::{nombre, ...};` (o `use`
     privado si la función era privada) para que el resto del archivo llame igual en los dos sistemas.
   - Si una función mezcla lógica pura con llamadas al sistema, separa SOLO la llamada al sistema en una
     función pequeña `#[cfg(windows)]` + su hermana Mac. La lógica pura sigue común y con sus tests.
2. **El código Mac va en `src-tauri/src/mac/<area>.rs`** con la MISMA firma que la función Windows a la
   que sustituye. Doc-comment en castellano diciendo qué API de Apple usa y por qué.
3. **Crates disponibles en Mac** (ya en `Cargo.toml`, todas las features por defecto): `objc2 0.6`,
   `objc2-foundation`, `objc2-app-kit`, `objc2-core-foundation`, `objc2-core-graphics`,
   `objc2-core-audio`, `objc2-application-services` (0.3), `block2 0.6`. No añadas más crates sin
   motivo fuerte; si hace falta, dilo en tu informe. Busca las firmas exactas en
   `~/.cargo/registry/src/index.crates.io-*/objc2-*-0.3.2/src/generated/`. Prefiere estas crates a
   `extern "C"` a mano (las firmas generadas no se equivocan). Procesos de sistema (`/usr/bin/open`,
   `/usr/bin/afplay`, `/bin/launchctl`, `/usr/bin/codesign`) valen cuando son lo más simple y fiable.
4. **Todo lo de AppKit (ventanas, NSApp)** en el hilo principal (`app.run_on_main_thread`).
5. **Nada de red nueva** (regla 1): ningún host fuera de `specs/PRD.md` §5.2. Toda petición con timeout.
6. **Tests:** los que dependen de Windows llevan `#[cfg(windows)]`. Si la lógica pura de Mac se puede
   probar (p. ej. generar un plist, elegir un asset), añade un test `#[cfg(target_os = "macos")]` o, mejor,
   haz la función pura sin cfg y pruébala en Windows.
7. **Comprueba** con `node scripts/mac/check.mjs` (tipa todo para `aarch64-apple-darwin` desde Windows,
   también los tests) y con `cargo check --tests` normal en Windows (`CARGO_TARGET_DIR` = la ruta
   corta de una carpeta corta, p. ej. `C:\dev\w`). Otras personas trabajan a la vez en otros archivos: fíjate solo
   en los errores de TUS archivos. No ejecutes `cargo test` completo (lo hace quien dirige).
8. **No toques** archivos de otras áreas, `Cargo.toml`, `main.rs`, `mac/mod.rs`, `text.rs`, `polish.rs`,
   `state.rs` (salvo una línea imprescindible, y dilo). No hagas commits. No uses git stash.
9. **Lo que no se puede hacer bien en Mac**, escríbelo en tu informe con su motivo (irá a
   `specs/mac/ESTADO.md`).
