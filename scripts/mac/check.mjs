// `npm run mac:check`: comprueba desde Windows (o Linux) que el Rust de Pronto compila para macOS.
//
// No enlaza ni construye la app (eso solo se puede hacer en un Mac: `scripts/mac/probar.sh`). Sí tipa
// todo el código propio para `aarch64-apple-darwin`, incluidos los módulos `src/mac/` y los tests:
//   - el C/Objective-C de las dependencias lo «compila» un cc falso (scripts/mac/falso/) que deja .o vacíos;
//   - DOCS_RS=1 hace que sherpa-onnx-sys y whisper-rs-sys no bajen ni compilen nada nativo;
//   - WHISPER_DONT_GENERATE_BINDINGS=1 usa los bindings de whisper.cpp ya incluidos (sin libclang).
// Uso: node scripts/mac/check.mjs [--json] [--solo-app]
// Sale con 0 si no hay errores; imprime el recuento por archivo.
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const raiz = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
const tauri = join(raiz, "src-tauri");
const target = "aarch64-apple-darwin";
const enWindows = process.platform === "win32";
const json = process.argv.includes("--json");
const soloApp = process.argv.includes("--solo-app");

function haceFalta() {
  const r = spawnSync("rustup", ["target", "list", "--installed"], { encoding: "utf8", shell: enWindows });
  if (r.status === 0 && !r.stdout.includes(target)) {
    const a = spawnSync("rustup", ["target", "add", target], { stdio: "inherit", shell: enWindows });
    if (a.status !== 0) throw new Error(`no se pudo instalar el destino ${target}`);
  }
}

export function checkMac({ tests = true } = {}) {
  haceFalta();
  const falso = join(raiz, "scripts", "mac", "falso");
  const ext = enWindows ? ".cmd" : ".sh";
  const env = {
    ...process.env,
    DOCS_RS: "1",
    WHISPER_DONT_GENERATE_BINDINGS: "1",
    // Ruta corta: en Windows las rutas largas de compilación fallan.
    CARGO_TARGET_DIR: process.env.PRONTO_MAC_TARGET_DIR || (enWindows ? join(process.env.USERPROFILE || "C:\\", "dev", "w-mac") : join(raiz, "target-mac-check")),
  };
  const tri = target.replaceAll("-", "_");
  if (process.platform !== "darwin") {
    env[`CC_${tri}`] = join(falso, `cc${ext}`);
    env[`CXX_${tri}`] = join(falso, `cc${ext}`);
    env[`AR_${tri}`] = join(falso, `ar${ext}`);
  }
  const pasadas = tests ? [["check"], ["check", "--tests"]] : [["check"]];
  const errores = {};
  let total = 0;
  let salida = "";
  for (const p of pasadas) {
    const r = spawnSync("cargo", [...p, "--target", target, "--message-format", "short"], { cwd: tauri, env, encoding: "utf8", shell: enWindows, maxBuffer: 64 << 20 });
    salida += r.stdout + r.stderr;
    for (const linea of (r.stdout + r.stderr).split(/\r?\n/)) {
      const m = /^(src[\\/][^:]+):\d+:\d+: error/.exec(linea);
      if (m) {
        const f = m[1].replaceAll("\\", "/");
        errores[f] = (errores[f] || 0) + 1;
        total++;
      }
    }
    // Un fallo que no es de nuestro código (dependencia, build script) también cuenta.
    if (r.status !== 0 && total === 0) {
      const dep = /error: (?:could not compile|failed to run custom build command for) `([^`]+)/.exec(r.stdout + r.stderr);
      errores[dep ? `dependencia ${dep[1]}` : "cargo"] = 1;
      total = 1;
      break;
    }
  }
  return { total, errores, salida };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (!existsSync(tauri)) throw new Error("no encuentro src-tauri");
  const r = checkMac({ tests: !soloApp });
  if (json) console.log(JSON.stringify({ total: r.total, errores: r.errores }));
  else {
    if (r.total) console.log(r.salida.split(/\r?\n/).filter((l) => / error/.test(l)).slice(0, 60).join("\n"));
    console.log(`\ncargo check para ${target}: ${r.total} errores`);
    for (const [f, n] of Object.entries(r.errores)) console.log(`  ${n}\t${f}`);
  }
  process.exit(r.total ? 1 : 0);
}
