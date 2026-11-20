# Repartir Pronto para Mac

Pronto para Mac se descarga desde GitHub (no se publica en la App Store). Este documento dice qué hay
hoy, qué ve quien lo instala y qué haría falta para que lo vea como cualquier app de Mac.

## Hoy (gratis)

| Pieza | Cómo está |
|---|---|
| Dónde se construye | En un Mac de GitHub (`.github/workflows/mac.yml`), gratis en repos públicos. |
| Qué sale | `Pronto-<versión>-mac-arm64.dmg`, solo para chips M (decisión D4). |
| Cuándo | A mano (Actions → «Pronto para Mac» → Run workflow) o al publicar una release, junto al `.exe`. |
| Firma | Certificado propio autofirmado (`scripts/mac/crear-certificado.sh`), guardado como secreto de GitHub. Sin secreto, firma ad-hoc. |
| Notarización | No. |

**Lo que ve quien lo instala la primera vez:** macOS dice que no puede comprobar que Pronto no tenga
software malicioso y no lo abre. Hay que ir a **Ajustes del Sistema → Privacidad y seguridad**, bajar
hasta «Pronto se bloqueó…» y pulsar **Abrir igualmente** (desde macOS 15 ya no vale el clic derecho →
Abrir). Pasa una sola vez por instalación.

**Actualizar:** Pronto avisa de la versión nueva y abre la página de descarga (decisión D9). Con el
certificado propio, macOS mantiene los permisos al instalar la versión nueva encima. Sin él (ad-hoc),
habría que volver a dar Accesibilidad y Micrófono en cada versión.

## Para repartirlo a mucha gente sin avisos (de pago)

| Paso | Coste | Qué cambia |
|---|---|---|
| 1. Cuenta **Apple Developer Program** (developer.apple.com/programs) | 99 $ al año | Da un certificado «Developer ID Application» y permiso para notarizar. Para empresa hace falta número D-U-N-S (gratis, tarda días). |
| 2. Firmar con Developer ID | incluido | En GitHub: secretos `APPLE_CERTIFICATE` (+ contraseña) en vez del autofirmado; `signingIdentity` = «Developer ID Application: …». |
| 3. Activar «hardened runtime» | incluido | `hardenedRuntime: true` en `src-tauri/tauri.macos.conf.json`. Las excepciones ya están en `src-tauri/Entitlements.plist` (micrófono y librerías propias). |
| 4. Notarizar | incluido | Tauri lo hace solo con `APPLE_ID`, `APPLE_PASSWORD` (contraseña de app) y `APPLE_TEAM_ID` como secretos. |
| 5. Actualizar sin abrir la web | 0 € | Con la app notarizada, el actualizador puede instalar el `.app` solo (bajar, comprobar firma con `codesign`, cambiar y reabrir), como en Windows. |

Con esto, el `.dmg` se abre con doble clic, sin pasar por Ajustes, y macOS no muestra ningún aviso.

## Lo que no se hace

- **App Store:** no. Pide la «sandbox» de Apple, que no deja escuchar el teclado de otras apps ni pegar
  en ellas, que es lo que hace Pronto.
- **Intel:** no. Ver decisión D4 en `specs/mac/DECISIONES.md`.
