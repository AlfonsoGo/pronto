fn main() {
    // vulkan-1.dll con carga diferida: sin controlador Vulkan Pronto arranca igual (usa Parakeet);
    // `whisper::gpu` comprueba que la DLL exista antes de llamar a Vulkan.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-arg-bins=/DELAYLOAD:vulkan-1.dll");
        println!("cargo:rustc-link-arg-bins=delayimp.lib");
    }
    tauri_build::build()
}
