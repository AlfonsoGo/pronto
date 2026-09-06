# El esquema de `pronto.db` se versiona, y se copia antes de tocarlo

`pronto.db` lleva `PRAGMA user_version` y una lista de migraciones numeradas
(`lib/src/data/schema_migrations.dart`). Antes de aplicar ninguna, Pronto copia la carpeta de
datos a `%APPDATA%\Pronto\copias\copia-<fecha>`.

El motivo es que **Pronto se actualiza solo**. `update_controller.dart` mira los GitHub Releases,
se baja el `.exe` y lo instala con un ayudante de PowerShell en ventana oculta que mata la app si
no se cierra. La versión nueva aterriza en máquinas que no controlamos, de gente que no va a leer
ninguna nota. Publicar deja de ser publicar: es entrar en el equipo de alguien.

Hasta la 0.7.1 el esquema se creaba con tres `CREATE TABLE IF NOT EXISTS` y nada más — ni una
versión, ni una `ALTER TABLE`. Eso aguanta mientras el esquema no cambie. En cuanto una versión
añada una columna (PRNT-16 va a añadir varias al historial), `IF NOT EXISTS` **no la añade a una
tabla que ya existe**: la instalación nueva nace bien y la actualizada revienta. Y los tests, que
crean la base de cero, dicen verde igual — por eso `test/fixtures/pronto-0.7.1.db` es un fichero
real de la 0.7.1 y no una base creada en el `setUp`.

## Las tres reglas

1. **Números consecutivos desde 1, que no se renumeran nunca.** Ese número queda escrito en el
   disco de gente que no conocemos; cambiarlo cambia el significado de lo que ya está escrito.
2. **Cada migración es idempotente y va en su propia transacción**, con el bump de `user_version`
   dentro. Matar el proceso a mitad deja la base entera en la versión vieja o entera en la nueva,
   nunca a medias. Para ampliar una tabla hay que mirar antes `PRAGMA table_info(<tabla>)`: SQLite
   no tiene `ADD COLUMN IF NOT EXISTS` y sin esa comprobación la segunda pasada revienta.
3. **Sin copia no se migra.** Si la copia falla, Pronto arranca con el esquema viejo y lo dice.
   Migrar a ciegas es el único camino sin vuelta atrás.

## Qué NO entra en la copia

`models/`: los ~640 MB del modelo Parakeet. No son datos del usuario — se vuelven a descargar y se
verifican por SHA-256 (`AppConfig.parakeetModelSha256`). Copiarlos duplicaría el disco en cada
actualización para proteger lo único que no se puede perder. Lo que sí se copia —`pronto.db` y
`shared_preferences.json`— son decenas de kilobytes.

## Lo que se le dice al usuario

El informe de apertura no devuelve un puñado de banderas para que la pantalla adivine qué pasó:
devuelve **un caso** (`OpenOutcome`), y cada caso tiene su frase. Existe por un fallo concreto que
se coló en la primera versión de esto: una migración que entra **a medias** —la 1 sí, la 2 no— le
enseñaba al usuario «Pronto no ha tocado nada. Tus datos están como estaban», que es mentira. Con
un caso por situación, una frase falsa se ve al leer el `switch`.

Los casos son: migrada · migrada y luego rota · no tocada tras un error · no tocada porque no se
pudo copiar · base de una versión más nueva · no se pudo ni abrir. El texto crudo de la excepción
se enseña **etiquetado como detalle técnico**, nunca en la voz de Pronto.

## Consequences

- Quien toque el esquema **añade una migración**; no edita la anterior ni cambia el `CREATE TABLE`
  de la 1. La 1 es lo que ya tienen instalado los usuarios de la 0.7.1.
- Una base **más nueva que la app** (el usuario volvió a una versión anterior) no se degrada ni se
  toca: se informa. Degradar borraría columnas con datos dentro.
- La copia se acumula: una carpeta por migración. Son kilobytes y no se borran solas — borrar
  copias del usuario sin que lo pida es justo lo que este ADR viene a evitar. Lo único que sí se
  barre son los restos `.a-medias` de un intento que murió por el camino: no son una copia de
  nadie, y dejarlos ahí empuja el nombre de las buenas a `-2`, `-3`, `-4`.
- La copia **no reemplaza** la carpeta de datos: se restaura ENCIMA. Como no lleva `models/`,
  reemplazar la carpeta dejaría al usuario sin el modelo de voz.
- El aviso de que se ha migrado **se persiste hasta que el usuario lo descarta**. El panel se abre
  solo en el arranque que lo CREÓ: un aviso en una pantalla que nadie mira no es un aviso, pero
  secuestrarle la pantalla en cada arranque hasta que se acuerde de cerrarlo tampoco es avisar.
- La carpeta de datos sigue siendo **compartida entre dev y prod** (`%APPDATA%\Pronto\Pronto`):
  probar migraciones en un build DEV toca la instalación buena. La copia previa también protege de
  eso, pero no lo arregla.
