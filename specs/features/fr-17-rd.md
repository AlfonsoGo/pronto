# FR-17 · Panel claro para quien no sabe de informática

> Origen: observado-en-código (commit b9b76cc, 24-oct-2026; pedido por Txonso) · Objetivo al que sirve: G1 (también regla 6)

## Qué hace (lenguaje del usuario)
El panel enseña solo lo de la primera semana, con nombres en castellano llano, y todo se lee bien en claro y en oscuro.

## Requisitos
- R1. Menú lateral: Dictado, Reuniones, Mis palabras, Atajos de texto, Cómo escribe, Notas; abajo Configuración, «Cómo se usa» y Novedades (FR-18). Estadísticas se abre desde la tarjeta de cifras de Dictado y Reescribir (Transforms) desde Configuración › Limpieza e IA.
- R2. Dictado explica cómo se dicta (tecla, micrófono, idioma) con un botón para cambiar cada cosa; «Cómo se usa» da los tres pasos.
- R3. Configuración: Lo básico, Ventana y sonidos, Reuniones, Privacidad y datos; en «Avanzado», Reconocimiento de voz y Limpieza e IA.
- R4. Sin jerga en pantalla (Meet, Fragmento, Flow Bar, Ollama…); los botones dicen qué cambian.
- R5. Contraste WCAG 2.2 AA en claro y oscuro (regla 6, `tests/no-se-negocia/06-accesibilidad.test.mjs`).

## Dónde vive
- `ui/hub.js` (`NAV`, `helpDialog`), `ui/hub/pages/home.js`, `ui/hub/pages/settings.js` (`TABS`), `ui/hub/pages/style.js`, `ui/hub/pages/cleanup.js`, `ui/shared/cristal.css`.
- Pruebas con primerizos: `specs/primerizo/2026-10-24.md`, `specs/primerizo/2026-10-25.md`.
