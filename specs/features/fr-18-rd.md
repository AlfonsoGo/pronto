# FR-18 · Novedades dentro de la app

> Origen: observado-en-código (commit f148361, 25-oct-2026; pedido por Txonso) · Objetivo al que sirve: G1 (adopción de lo nuevo)

## Qué hace (lenguaje del usuario)
Al actualizar, Pronto me enseña una vez lo nuevo con un botón «Probar» para cada cosa, y siempre puedo volver a verlo en «Novedades».

## Requisitos
- R1. Fuente única `ui/hub/novedades.js` (castellano e inglés); la primera entrada es la versión de `tauri.conf.json` (test `tests/novedades.test.mjs`).
- R2. Página «Novedades» en el menú lateral con punto mientras la versión instalada traiga destacados sin ver: destacados, mejoras y arreglos; versiones anteriores plegadas con su recuento.
- R3. Tarjeta «Lo nuevo en Pronto X» una sola vez por versión, solo si ya hay dictados o reuniones (una instalación nueva no la ve); enseña los destacados de todas las versiones sin ver, de la más nueva a la más vieja (quien no tiene nada guardado ve desde la 2.3.0, la primera con Novedades) y si se puede guardar que se vio (`localStorage`); no tapa una reunión o Configuración abiertas desde la bandeja.
- R4. «Probar» lleva a la pantalla de la función, abre lo que haga falta, resalta el control y explica el paso siguiente.
- R5. Sin red (regla 1): la web `pronto.beltranlabs.com` solo aparece escrita, sin enlace, hasta que Txonso decida añadir ese host.
- R6. El texto de la release de GitHub sale de la misma fuente (`node scripts/notas-release.mjs`), resume en una línea «Incluye la X» las versiones anteriores de la misma serie y cabe en el aviso del actualizador (600 caracteres, 12 líneas).

## Dónde vive
- `ui/hub/novedades.js`, `ui/hub/pages/novedades.js`, `ui/hub.js` (`novedadesNavHtml`, `maybeShowWhatsNew`), `ui/hub.css` (`.nv-*`), `scripts/notas-release.mjs`, `tests/novedades.test.mjs`.
