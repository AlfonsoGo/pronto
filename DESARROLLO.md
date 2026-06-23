# Desarrollo: local (dev) → producción (prod)

Pronto tiene **dos canales** para separar lo que pruebas de lo que se publica:

| Canal | Rama | Qué es | Badge |
|---|---|---|---|
| **prod** | `main` | Lo que se publica en GitHub Releases (lo que instala la gente). | — |
| **dev** | `dev` | Build **local de pruebas**. Solo en tu PC, no se publica. | naranja **DEV** |

La app muestra arriba la **versión** y, si es un build de pruebas, un badge naranja **DEV · `<sha>`**, para que sepas siempre qué estás ejecutando.

## Flujo de trabajo

1. **Implementar en local.** El trabajo nuevo va a la rama `dev`:
   ```
   git switch dev
   ```
   (se implementan los cambios)

2. **Probar en local** (compila y abre Pronto marcado como DEV, sin publicar nada):
   ```
   powershell -ExecutionPolicy Bypass -File tools\pronto-dev.ps1
   ```
   > Instancia única: cierra el Pronto de producción si lo tienes abierto; el script lo hace por ti.

3. **Iterar.** Si algo no va, se ajusta en `dev` y se repite el paso 2.

4. **Promover a producción** (SOLO cuando das el OK):
   ```
   git switch main
   git merge dev
   powershell -ExecutionPolicy Bypass -File tools\publicar.ps1 -Version X.Y.Z [-Notas notas.md]
   ```
   `publicar.ps1` sube la versión, compila el instalador de producción (sin badge DEV), commitea, hace push y publica la release en GitHub.

## Notas
- El badge **DEV** solo aparece en builds hechos con `--dart-define=PRONTO_CHANNEL=dev` (lo pone `pronto-dev.ps1`). Los builds de producción nunca lo muestran.
- Por ahora `dev` y `prod` **comparten** la carpeta de datos (`%APPDATA%\Pronto\Pronto`) y el mismo *instancia única*: hay que cerrar uno para abrir el otro. Aislar del todo dev/prod (carpeta de datos y nombre propios, para correrlos a la vez) es una mejora futura.
- `tools\pronto-dev.ps1` y `tools\publicar.ps1` detectan flutter/cmake/gh sin rutas personales (igual que `installer\build_installer.ps1`).
