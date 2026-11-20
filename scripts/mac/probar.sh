#!/bin/bash
# Pronto 2 para Mac · kit de prueba en UN comando.
#
#   bash scripts/mac/probar.sh
#
# Qué hace (unos 20-40 minutos la primera vez, casi todo esperando a que compile):
#   1. Comprueba el Mac (Apple Silicon, macOS 14 o más).
#   2. Instala lo que falte: herramientas de Xcode, Homebrew, Node, CMake y Rust.
#   3. Baja el modelo de voz Parakeet (unos 650 MB, de huggingface.co) si no lo tienes ya.
#   4. Pasa todas las pruebas: `npm test` (las 7 reglas + cargo test).
#   5. Mide la velocidad (regla 3): 15 s de audio transcritos en 1068 ms o menos.
#   6. Construye la app (Pronto.app y el .dmg) en tu Mac, sin firma de Apple.
#   7. Deja un informe: informe-pronto-mac-FECHA.txt en el Escritorio. Mándaselo a Txonso.
#
# No toca nada tuyo fuera de esta carpeta, salvo instalar herramientas (con tu permiso) y guardar el
# modelo de voz donde lo buscará Pronto (~/Library/Application Support/Pronto2/models/parakeet).
# Opciones: --rapido (sin compilar la app), --sin-instalar (no instala nada, solo comprueba).

set -u
cd "$(dirname "$0")/../.." || exit 1
RAIZ="$(pwd)"
FECHA="$(date +%Y-%m-%d-%H%M)"
INFORME="$HOME/Desktop/informe-pronto-mac-$FECHA.txt"
[ -d "$HOME/Desktop" ] || INFORME="$RAIZ/informe-pronto-mac-$FECHA.txt"
LOGS="$RAIZ/.logs/mac-$FECHA"
mkdir -p "$LOGS"
RAPIDO=0
INSTALAR=1
for a in "$@"; do
  case "$a" in
    --rapido) RAPIDO=1 ;;
    --sin-instalar) INSTALAR=0 ;;
  esac
done

OK=0
FALLOS=0
AVISOS=0
: > "$INFORME"
linea() { echo "$*" | tee -a "$INFORME"; }
paso() { linea ""; linea "== $* =="; }
bien() { OK=$((OK + 1)); linea "  [OK]    $*"; }
mal() { FALLOS=$((FALLOS + 1)); linea "  [FALLO] $*"; }
aviso() { AVISOS=$((AVISOS + 1)); linea "  [AVISO] $*"; }
cola() { # últimas líneas de un log, dentro del informe
  linea "  --- últimas líneas de $(basename "$1") ---"
  tail -n "${2:-40}" "$1" | sed 's/^/  | /' | tee -a "$INFORME" >/dev/null
}

linea "Informe de Pronto 2 para Mac · $FECHA"
linea "Carpeta: $RAIZ"
linea "Versión: $(grep -m1 '"version"' src-tauri/tauri.conf.json | sed 's/[^0-9.]//g') · commit $(cat .commit 2>/dev/null || git rev-parse --short HEAD 2>/dev/null || echo desconocido)"

# ---------------------------------------------------------------- 1. el Mac
paso "1. Tu Mac"
MACOS="$(sw_vers -productVersion 2>/dev/null || echo 0)"
ARCH="$(uname -m)"
CHIP="$(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo desconocido)"
RAM_GB=$(( $(sysctl -n hw.memsize 2>/dev/null || echo 0) / 1073741824 ))
linea "  macOS $MACOS · $ARCH · $CHIP · ${RAM_GB} GB de RAM"
MAYOR="${MACOS%%.*}"
if [ "$ARCH" = "arm64" ]; then bien "Apple Silicon"; else mal "Este Mac es Intel: Pronto para Mac es solo para chips M (decisión D4)"; fi
if [ "${MAYOR:-0}" -ge 14 ] 2>/dev/null; then bien "macOS $MACOS (hace falta 14 o más)"; else mal "macOS $MACOS: hace falta macOS 14 o más"; fi
MENOR="$(echo "$MACOS" | cut -d. -f2)"
if [ "${MAYOR:-0}" -gt 14 ] || { [ "${MAYOR:-0}" -eq 14 ] && [ "${MENOR:-0}" -ge 6 ]; }; then
  bien "Audio del sistema en reuniones disponible (macOS 14.6 o más)"
else
  aviso "macOS por debajo de 14.6: las reuniones grabarán solo tu micrófono"
fi

# ---------------------------------------------------------------- 2. herramientas
paso "2. Herramientas"
if ! xcode-select -p >/dev/null 2>&1; then
  if [ $INSTALAR -eq 1 ]; then
    linea "  Instalando las herramientas de Xcode: acepta la ventana que sale y, cuando acabe, vuelve a lanzar este comando."
    xcode-select --install >/dev/null 2>&1
    exit 2
  fi
  mal "Faltan las herramientas de Xcode (xcode-select --install)"
else
  bien "Herramientas de Xcode en $(xcode-select -p)"
fi

if ! command -v brew >/dev/null 2>&1; then
  for b in /opt/homebrew/bin/brew /usr/local/bin/brew; do [ -x "$b" ] && eval "$("$b" shellenv)"; done
fi
if ! command -v brew >/dev/null 2>&1 && [ $INSTALAR -eq 1 ]; then
  linea "  Instalando Homebrew (te pedirá la contraseña del Mac)…"
  /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)" </dev/tty
  eval "$(/opt/homebrew/bin/brew shellenv 2>/dev/null)"
fi
command -v brew >/dev/null 2>&1 && bien "Homebrew $(brew --version | head -1 | cut -d' ' -f2)" || aviso "Sin Homebrew"

for herramienta in node cmake; do
  if ! command -v "$herramienta" >/dev/null 2>&1 && [ $INSTALAR -eq 1 ] && command -v brew >/dev/null 2>&1; then
    linea "  Instalando $herramienta…"
    brew install "$herramienta" >"$LOGS/brew-$herramienta.log" 2>&1
  fi
  if command -v "$herramienta" >/dev/null 2>&1; then bien "$herramienta $("$herramienta" --version 2>/dev/null | head -1)"; else mal "Falta $herramienta"; fi
done

[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
if ! command -v cargo >/dev/null 2>&1 && [ $INSTALAR -eq 1 ]; then
  linea "  Instalando Rust (rustup)…"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal >"$LOGS/rustup.log" 2>&1
  . "$HOME/.cargo/env"
fi
if command -v cargo >/dev/null 2>&1; then bien "$(rustc --version)"; else mal "Falta Rust"; fi

if [ $FALLOS -gt 0 ]; then
  linea ""
  linea "Faltan cosas básicas: no se puede seguir. Manda este informe a Txonso."
  linea "Informe: $INFORME"
  exit 1
fi

# Entorno de compilación del Mac (el .cargo/config.toml trae rutas de Windows; las variables de aquí mandan).
export CARGO_TARGET_DIR="$RAIZ/src-tauri/target"
export GGML_NATIVE=OFF GGML_AVX=OFF GGML_AVX2=OFF GGML_FMA=OFF GGML_F16C=OFF
for d in "$(xcode-select -p)/Toolchains/XcodeDefault.xctoolchain/usr/lib" /Library/Developer/CommandLineTools/usr/lib; do
  if [ -f "$d/libclang.dylib" ]; then export LIBCLANG_PATH="$d"; break; fi
done
[ -n "${LIBCLANG_PATH:-}" ] && linea "  libclang: $LIBCLANG_PATH" || aviso "No encuentro libclang.dylib: whisper usará sus bindings ya hechos"
[ -z "${LIBCLANG_PATH:-}" ] && export WHISPER_DONT_GENERATE_BINDINGS=1

# ---------------------------------------------------------------- 3. modelo de voz
paso "3. Modelo de voz (Parakeet)"
MODELO="${PRONTO_PARAKEET_DIR:-$HOME/Library/Application Support/Pronto2/models/parakeet}"
BASE="https://huggingface.co/csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/resolve/main"
mkdir -p "$MODELO"
for f in encoder.int8.onnx decoder.int8.onnx joiner.int8.onnx tokens.txt; do
  if [ ! -s "$MODELO/$f" ]; then
    linea "  Bajando $f…"
    curl -fL --retry 3 --max-time 3600 -o "$MODELO/$f.part" "$BASE/$f" 2>>"$LOGS/modelo.log" && mv "$MODELO/$f.part" "$MODELO/$f"
  fi
done
# Comprobación con los SHA-256 de la app (src-tauri/src/modeldl.rs)
MODELO_OK=1
for f in encoder.int8.onnx decoder.int8.onnx joiner.int8.onnx tokens.txt; do
  esperado="$(grep -o "name: \"$f\", sha256: \"[0-9a-f]*\"" src-tauri/src/modeldl.rs | head -1 | sed 's/.*sha256: "//; s/"//')"
  if [ ! -s "$MODELO/$f" ]; then MODELO_OK=0; mal "Falta $f"; continue; fi
  real="$(shasum -a 256 "$MODELO/$f" | cut -d' ' -f1)"
  if [ -n "$esperado" ] && [ "$real" != "$esperado" ]; then MODELO_OK=0; mal "$f no coincide con su huella (bórralo y repite)"; fi
done
[ $MODELO_OK -eq 1 ] && bien "Modelo completo y verificado en $MODELO"
export PRONTO_PARAKEET_DIR="$MODELO"

# ---------------------------------------------------------------- 4. pruebas
paso "4. Pruebas (npm test: 7 reglas + cargo test)"
npm ci --no-audit --no-fund >"$LOGS/npm-ci.log" 2>&1 || npm install --no-audit --no-fund >>"$LOGS/npm-ci.log" 2>&1
T0=$(date +%s)
if npm test >"$LOGS/npm-test.log" 2>&1; then
  bien "npm test en verde ($(( $(date +%s) - T0 )) s)"
else
  mal "npm test falló"
  cola "$LOGS/npm-test.log" 60
fi
grep -E '^# (tests|pass|fail|skipped)' "$LOGS/npm-test.log" | sed 's/^/  /' | tee -a "$INFORME" >/dev/null
grep -E '^test result:' "$LOGS/npm-test.log" | tail -3 | sed 's/^/  cargo: /' | tee -a "$INFORME" >/dev/null
grep -E '^# Subtest|^not ok' "$LOGS/npm-test.log" | grep -B1 '^not ok' | sed 's/^/  /' | tee -a "$INFORME" >/dev/null

# ---------------------------------------------------------------- 5. velocidad
paso "5. Velocidad (regla 3: 15 s de audio en 1068 ms o menos)"
( cd src-tauri && PRONTO_NSN_WAV="$RAIZ/tests/no-se-negocia/datos/dictado-15s.wav" PRONTO_NSN_MODELO="$MODELO" PRONTO_NSN_RUNS=5 \
  cargo test --quiet nsn_velocidad -- --nocapture --color never ) >"$LOGS/velocidad.log" 2>&1
# El test de Rust imprime NSN_TRANSCRIPCION_MS=a,b,c,d,e (ms de cada pasada): la mediana es la regla 3.
VEL=$(grep '^NSN_TRANSCRIPCION_MS=' "$LOGS/velocidad.log" | tail -1 | cut -d= -f2 | tr ',' '\n' | sort -n \
  | awk '{v[NR]=$1} END {if (NR) {m = (NR % 2) ? v[(NR+1)/2] : (v[NR/2]+v[NR/2+1])/2; printf "%d", m + 0.5}}')
linea "  Pasadas (ms): $(grep '^NSN_TRANSCRIPCION_MS=' "$LOGS/velocidad.log" | tail -1 | cut -d= -f2)"
linea "  Texto: $(grep '^NSN_TEXTO=' "$LOGS/velocidad.log" | tail -1 | cut -d= -f2- | cut -c1-160)"
if [ -n "$VEL" ]; then
  if [ "$VEL" -le 1068 ]; then bien "Mediana: $VEL ms (límite 1068 ms)"; else mal "Mediana: $VEL ms, por encima de 1068 ms (cambiar el límite en Mac lo decide Txonso)"; fi
else
  aviso "No salió la medida de velocidad"
  cola "$LOGS/velocidad.log" 30
fi

# ---------------------------------------------------------------- 6. la app
paso "6. La app (Pronto.app y .dmg, sin firma de Apple)"
if [ $RAPIDO -eq 1 ]; then
  aviso "Saltado (--rapido)"
else
  # Firma «ad-hoc» (-): suficiente para abrirla en este Mac. Sin notarizar (ver docs/mac/REPARTIR.md).
  if npx --yes @tauri-apps/cli@2 build --bundles app,dmg --config '{"bundle":{"macOS":{"signingIdentity":"-"}}}' >"$LOGS/build.log" 2>&1; then
    APP="$(ls -d "$CARGO_TARGET_DIR"/release/bundle/macos/*.app 2>/dev/null | head -1)"
    DMG="$(ls "$CARGO_TARGET_DIR"/release/bundle/dmg/*.dmg 2>/dev/null | head -1)"
    bien "App construida: $APP"
    [ -n "$DMG" ] && bien "Instalador: $DMG ($(du -h "$DMG" | cut -f1))" || aviso "No salió el .dmg"
    if [ -n "$APP" ]; then
      codesign --verify --deep --strict "$APP" >>"$LOGS/build.log" 2>&1 && bien "Firma ad-hoc válida" || aviso "codesign --verify no pasó (mira build.log)"
      /usr/libexec/PlistBuddy -c 'Print :NSMicrophoneUsageDescription' "$APP/Contents/Info.plist" >/dev/null 2>&1 && bien "Info.plist trae el texto del micrófono" || mal "Info.plist sin NSMicrophoneUsageDescription"
      otool -L "$APP/Contents/MacOS/"* 2>/dev/null | grep -vE '^\S|/System/|/usr/lib/' | sed 's/^/  dylib externa: /' | tee -a "$INFORME" >/dev/null
      linea "  Para probar a mano: arrastra Pronto.app a Aplicaciones y ábrela DESDE el Finder (no desde la Terminal)."
    fi
  else
    mal "No se pudo construir la app"
    cola "$LOGS/build.log" 60
  fi
fi

# ---------------------------------------------------------------- resumen
paso "Resumen"
linea "  Bien: $OK · Avisos: $AVISOS · Fallos: $FALLOS"
[ $FALLOS -eq 0 ] && linea "  RESULTADO: TODO EN VERDE" || linea "  RESULTADO: HAY FALLOS (los detalles están arriba)"
linea "  Registros completos: $LOGS"
linea ""
linea "Siguiente: sigue la lista a mano de docs/mac/PROBAR.md (15 pasos) y manda a Txonso este informe:"
linea "  $INFORME"
[ $FALLOS -eq 0 ]
