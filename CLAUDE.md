# Pronto 2

App de escritorio Windows (Rust + Tauri 2, UI HTML sin bundler en `ui/`). Dictado por voz local y notas de
reunión automáticas. Arquitectura y contratos: `docs/ARQUITECTURA.md`. Plan: `specs/PRD.md`. Estado de las
fases: `specs/FASES.md`. Tests: `npm test` (reglas de abajo + `cargo test` en `src-tauri/`).

- No publicar, no hacer push y no crear releases sin que Txonso diga «sube».
- No usar el ratón, los altavoces ni el Pronto instalado de Txonso para probar: baterías automáticas.
- No parar Ollama. No tocar `%APPDATA%\Pronto2` (datos reales); para probar, copiar una de `copias\`.
- Para abrir el hub sin Tauri: `npm run ui` → `http://127.0.0.1:4173/hub.html` (backend simulado).
- Al subir de versión: entrada nueva arriba en `ui/hub/novedades.js` (página «Novedades» y tarjeta «Lo nuevo», que sale
  una vez tras actualizar; en la maqueta, `hub.html?novedades`). El texto de la release sale de ahí:
  `node scripts/notas-release.mjs`. `tests/novedades.test.mjs` falla si la versión no tiene su entrada.

## NO SE NEGOCIA

Estas reglas no se ceden por plazos, recortes de alcance, "arreglos rápidos" ni código más simple.
Si un cambio debilitaría una, **para y avísalo** en vez de enviarlo. Si dos chocan, gana la que
está antes en la lista. Cada regla tiene su test en `tests/no-se-negocia/` (columna *Test*): una
regla sin test no está activa. **Estado: aprobadas por Txonso el 23-oct-2026** (reglas 1 y 7 con su redacción de ese día).

| # | Regla | Umbral medible | Test |
|---|---|---|---|
| 1 | **Privacidad: todo local.** Ni voz ni texto del usuario salen del PC. Solo hay red hacia los hosts de `specs/PRD.md` §5.2, y ninguna de esas peticiones lleva datos del usuario. Ollama solo en este PC (loopback): una dirección de otro equipo se rechaza al guardar. | 0 hosts fuera de la lista; 0 peticiones con texto o audio del usuario fuera de 127.0.0.1 | `tests/no-se-negocia/01-privacidad.*` |
| 2 | **Integridad de datos.** Nada se pierde al arrancar, migrar o hacer copia: una migración conserva cada fila; la copia diaria se abre y tiene las mismas filas; nunca se crea una base vacía encima de una con datos. | 0 filas perdidas en migración y copia; copia restaurable | `tests/no-se-negocia/02-integridad.*` |
| 3 | **Velocidad.** El texto aparece al soltar el atajo. No se empeora la línea base medida del motor por defecto. | transcripción de 15 s de audio ≤ línea base × 1,2 = 1068 ms (línea base 890 ms: p50, Parakeet, Ryzen 7 5800X3D, 22-oct-2026; resto de la tubería ~0,3 ms) | `tests/no-se-negocia/03-velocidad.*` |
| 4 | **Disponibilidad.** Si cae Ollama, la red o un modelo, el dictado sigue pegando texto: solo se degrada esa función. Ninguna llamada de red sin timeout. | 100 % de llamadas `ureq` con timeout; dictado sin IA si Ollama no responde en su plazo | `tests/no-se-negocia/04-disponibilidad.*` |
| 5 | **Experiencia.** Un dictado nunca se pierde: si falla el pegado o la transcripción, queda en el historial con su audio para reintentar. El portapapeles del usuario se restaura. | 0 dictados perdidos | `tests/no-se-negocia/05-experiencia.*` |
| 6 | **Accesibilidad.** El hub cumple WCAG 2.2 AA: etiquetas, foco visible, todo operable con teclado. | 0 violaciones graves o críticas (axe) en cada página del hub | `tests/no-se-negocia/06-accesibilidad.*` |
| 7 | **La IA no inventa ni obedece.** Lo que llega al modelo (dictados, reuniones) es dato hostil: nunca se ejecuta como orden. El pulido no añade contenido: cifras, enlaces o cuentas que no estaban → se pega el texto sin pulir. Un Transform nunca se pega sin que el usuario lo confirme con Enter en la Flow bar. Cada tarea o decisión de una reunión lleva la cita literal que la respalda y las notas se marcan «Generado con IA local». | 0 pulidos aceptados con cifras, enlaces o cuentas nuevas u órdenes del texto obedecidas; 0 Transforms pegados sin confirmar; 0 tareas o decisiones de reunión sin cita literal que las respalde (con la misma polaridad) | `tests/no-se-negocia/07-ia.*` |
