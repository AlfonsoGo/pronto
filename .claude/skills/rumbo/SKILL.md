---
name: rumbo
description: ¿Seguimos el plan? Compara el código con el PRD y los RDs de specs/ y detecta el desvío (rumbo 🟢/🟡/🔴).
argument-hint: "[todo | FR-03 FR-07 … | cambios]"
allowed-tools: Read, Grep, Glob, Agent, Bash(git status:*), Bash(git diff:*), Bash(git log:*), Write(specs/rumbo/*)
---

# /rumbo — ¿la construcción sigue el plan?

Alcance pedido: `$ARGUMENTS` (vacío = `todo`; `cambios` = solo lo tocado desde el último commit
con rumbo 🟢 o desde `main`). Aquí no se toca el código: solo se mide y se escribe el informe.

## 1. Leer el plan
`specs/PRD.md` entero (objetivos, **no-objetivos**, datos que se recogen) y los RDs del alcance en
`specs/features/`. Lee también la sección `## NO SE NEGOCIA` del `CLAUDE.md`.

## 2. Inventariar lo construido
Rutas y páginas, acciones de servidor/endpoints, tablas y migraciones, integraciones externas,
textos de cara al usuario que los RDs citan, `README` y `scripts/` (lo que prometen del producto),
y `git status`/`git diff` para lo no commiteado.

## 3. Comparar
**A. Planificado → construido.** Con más de 3 RDs, reparte: un subagente `Explore` (modelo
`sonnet`) por cada 2–3 RDs, en paralelo, con el texto del RD; cada uno devuelve **un veredicto por
requisito** con evidencia `fichero:línea`:
✅ cumple · 🟡 cumple con huecos · ❌ falta o lo contradice · ⬜ sin evidencia (no se pudo comprobar).
**B. Construido → no planificado.** Esto lo haces tú mientras ellos trabajan: todo lo que existe en
el código y no aparece en ningún RD. Marca aparte lo que **cruza un no-objetivo**, recoge **datos
que el PRD no permite** o debilita una regla de `NO SE NEGOCIA`.

## 4. Veredicto
- 🟢 **EN RUMBO**: nada ❌ y ningún trabajo no planificado que cruce no-objetivos o reglas.
- 🟡 **DERIVA LEVE**: huecos 🟡 o trabajo no planificado inocuo.
- 🔴 **FUERA DE RUMBO**: algún ❌, algún no-objetivo cruzado, o una regla de `NO SE NEGOCIA` debilitada.

## 5. Informe (este formato, siempre)
```
Rumbo: 🟢|🟡|🔴 <EN RUMBO|DERIVA LEVE|FUERA DE RUMBO>
<un párrafo: qué dice el plan, hacia dónde va el código, por qué el veredicto>

Marcador
| FR | Título | ✅ | 🟡 | ❌ | ⬜ | Notas |

Trabajo no planificado
1. <qué> — <fichero:línea> — cruza: <no-objetivo / regla / nada>

Siguientes pasos (cada desvío se resuelve MANTENIENDO o QUITANDO)
1. Decidir <X>: mantener → escribir su RD y enmendar PRD §…; quitar → borrar <ficheros/tablas>.
2. …
N. Volver a pasar /rumbo tras decidir, hasta 🟢.

No verificado
- <lo que ningún agente pudo comprobar y por qué>
```
Guarda el informe en `specs/rumbo/AAAA-MM-DD.md` y añade una línea con el veredicto a
`specs/FASES.md` (sección *Registro*). Mantener o quitar lo decide la persona: tú propones.
