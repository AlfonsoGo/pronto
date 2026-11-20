# Pronto 2 para Mac — contrato de «hecho» (BELTR-PRT-20)

Escrito el 20-nov-2026, antes de tocar el código. El trabajo está hecho cuando se cumple todo esto.
Lo que no se pueda cumplir sin un Mac queda escrito con su motivo en `specs/mac/ESTADO.md`.

## Medidas de partida (20-nov-2026, rama `claude/mac-71820c` sobre 29fb7b4)

| Medida | Antes |
|---|---|
| Funciones de Windows con versión Mac (`specs/mac/funciones.json`) | 0 de 72 |
| Archivos de `src-tauri/src` atados a Windows (crate `windows`, sin `cfg`) | 15 (12 de la app + 3 de pruebas) |
| `cargo check --target aarch64-apple-darwin` | no llega al código de Pronto: 16 errores en el crate `windows-future` |
| El mismo, con `windows` y Whisper-Vulkan solo para Windows | 72 errores en 10 archivos (rustc corta en los `use`: hay más detrás) |
| Kit de prueba para Mac / compilación descargable | no existe |

## Condiciones de «hecho»

1. **Funciones:** las 72 de `funciones.json` tienen versión Mac o el motivo escrito de por qué no
   (`specs/mac/ESTADO.md`, una línea cada una). Lo comprueba `tests/mac.test.mjs`.
2. **Compila para Mac:** `npm run mac:check` (desde Windows) da 0 errores en `cargo check` y en
   `cargo check --tests` para `aarch64-apple-darwin`. Lo que Windows no puede comprobar (enlazar,
   compilar C/Objective-C, firmar, ejecutar) queda listado con su motivo.
3. **Windows igual que antes:** `npm test` en verde (reglas 1-7 de «NO SE NEGOCIA» + `cargo test`).
   La parte Windows cambia solo en anotaciones `cfg`; ninguna regla se debilita.
4. **Separación:** lo propio de Mac vive en `src-tauri/src/mac/`; los archivos comunes solo ganan
   `#[cfg]` y llamadas. Una prueba falla si aparece código Windows sin `cfg` en un archivo común.
5. **Lo propio de Apple:** atajo con la tecla Fn/Globo, icono en la barra de menús, permisos de
   Micrófono, Accesibilidad y Monitorización de entrada pedidos con explicación, pegado con Cmd+V
   devolviendo el portapapeles, arranque al iniciar sesión, Whisper con Metal y Parakeet con Core ML
   en chips M, y `.dmg`.
6. **Regla 1 en Mac:** no se añade ningún host nuevo a la lista de `specs/PRD.md` §5.2.
7. **Descargable:** un flujo de GitHub Actions (Mac de GitHub, gratis en repos públicos) construye el
   `.dmg` y lo deja como artefacto. No se sube nada hasta el «sube» de Txonso.
8. **Kit del amigo:** `scripts/mac/probar.sh` en un solo comando instala lo que falte, construye,
   pasa `cargo test` y `npm test`, mide la regla 3 (15 s de audio ≤ 1068 ms) y deja un informe.
   Más `docs/mac/PROBAR.md`: 15 pasos a mano como mucho, en castellano llano.
9. **Repartir:** `docs/mac/REPARTIR.md` explica qué haría falta para firmar con Apple (99 $/año).
10. **Revisión:** un subagente nuevo corre las pruebas y dice APTO.
11. **Rama:** commits limpios, sin rastro de IA, sin push.
