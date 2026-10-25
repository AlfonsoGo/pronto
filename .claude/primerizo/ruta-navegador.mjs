// Qué navegador arranca el primerizo (`navegador.mjs`). Aparte para poder probarlo sin lanzar nada.
import { existsSync } from 'node:fs';

/** Primer navegador que exista. El 24-oct-2026 el primerizo no arrancaba: solo probaba el Chromium de Playwright
 *  (`npx playwright install chromium`), que no estaba instalado, aunque el PC tenía Chrome. */
export function rutaNavegador(chromium, env = process.env, existe = existsSync) {
  let pw = null;
  try { pw = chromium && chromium.executablePath(); } catch { /* playwright-core sin navegadores */ }
  const candidatos = [
    env.PRIMERIZO_CHROME, pw,
    'C:/Program Files/Google/Chrome/Application/chrome.exe', 'C:/Program Files (x86)/Google/Chrome/Application/chrome.exe',
    'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe', 'C:/Program Files/Microsoft/Edge/Application/msedge.exe',
    '/usr/bin/google-chrome', '/usr/bin/chromium', '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  ];
  return candidatos.find((c) => c && existe(c)) ?? null;
}
