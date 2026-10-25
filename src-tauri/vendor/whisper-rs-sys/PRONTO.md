# whisper-rs-sys 0.15.0 con un arreglo de Pronto

Copia de `whisper-rs-sys` 0.15.0 (crates.io, Unlicense; whisper.cpp incluido, MIT) con un único cambio en
`whisper.cpp/src/whisper.cpp`, función `whisper_full_with_state` (busca `[Pronto]`):

Con el idioma en «auto», whisper.cpp detecta el idioma pasando el encoder por el principio del audio y,
acto seguido, el bucle principal vuelve a pasar el encoder por el mismo audio con los mismos parámetros.
El arreglo reutiliza ese primer resultado en la primera vuelta del bucle. El resultado es el mismo
(mismo encoder, misma entrada); solo se ahorra una pasada del encoder por dictado o trozo de reunión.

Medido el 23-oct-2026 (RTX 4070 Ti, large-v3-turbo, juez de G1): ver `specs/velocidad/2026-10-23.md`.

Para actualizar whisper-rs: copiar la versión nueva de `~/.cargo/registry/src/*/whisper-rs-sys-*`
aquí, volver a aplicar el cambio (`diff -u` contra el original) y repetir el banco
`whisper::tests::banco_dictado` comprobando que los textos con idioma «auto» no cambian.
