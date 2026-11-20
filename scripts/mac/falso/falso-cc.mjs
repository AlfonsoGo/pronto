// Compilador de C «de mentira» para `cargo check --target *-apple-darwin` desde Windows.
// `cargo check` no enlaza: los build scripts solo necesitan que existan los .o y los .a que piden.
// Crea esos archivos vacíos y sale con 0. Nunca construye la app de verdad (eso se hace en un Mac).
import { writeFileSync } from "node:fs";
const args = process.argv.slice(2);
if (process.env.FALSO_MODO === "ar") {
  const lib = args.find((a) => a.endsWith(".a"));
  if (lib) writeFileSync(lib, "!<arch>\n");
} else {
  const i = args.indexOf("-o");
  if (i >= 0 && args[i + 1]) writeFileSync(args[i + 1], "");
  else for (const a of args) if (a.startsWith("-o") && a.length > 2) writeFileSync(a.slice(2), "");
}
process.exit(0);
