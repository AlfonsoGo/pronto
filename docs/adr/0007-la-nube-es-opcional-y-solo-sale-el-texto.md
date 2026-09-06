# La nube es opcional, la enciende el usuario y solo sale el texto

Pronto prometía en un `README.md` público *«100 % on-device y privado. Sin servidores, sin cuotas,
sin enviar tu audio a la nube»*. El formateo en español, que es donde más se nota la diferencia
entre un modelo pequeño local y uno bueno de nube, chocaba de frente con esa frase.

Se abre la puerta a la nube con cuatro condiciones que **no son configuración, son requisito**:
**solo el texto ya transcrito, jamás el audio** · **apagada por defecto** · **se enciende por Modo,
no globalmente** · **la promesa del `README.md` se corrige en el mismo cambio**.

El proveedor es **OpenRouter**, y el motivo es poder ver el gasto: el vigilante que ya corre en el
VPS cubre a Pronto sin construir nada nuevo.

## La promesa, reescrita para que siga siendo verdad

Lo que se mantiene entero: **la voz no sale del equipo**. El audio se transcribe en el propio
ordenador y no viaja a ningún sitio, ni siquiera con la nube encendida — `TextFormatter.format`
solo recibe un `String`, así que no hay camino por el que el audio pueda salir.

Lo que cambia: **el texto ya transcrito puede salir**, si el usuario elige el camino de nube y pone
su clave. Y el texto dictado es tan sensible como el audio (ahí van correos de clientes y nombres
propios), así que esto es una decisión de producto, no una opción más:

- **Apagado de fábrica.** El Modo por defecto no formatea (ADR 0005), y el camino de nube exige
  además una clave que solo pone el usuario.
- **Se ve mientras pasa.** Mientras el texto viaja, la Píldora se pone naranja y dice **«Saliendo
  del equipo»** — un estado propio (`DictationStatus.sendingToCloud`), no un «pensando» más. Lo
  dispara la PETICIÓN, no la intención: si el tope de gasto la frena o falta la clave, no se
  enciende nada porque no ha salido nada.
- **La clave es del usuario y vive en el Administrador de credenciales de Windows**, nunca en el
  fichero de ajustes, que es un JSON en claro. No se cachea en ningún estado de la app: se lee del
  almacén en el instante de mandar la petición y se olvida.
- **Se le prohíbe al proveedor guardar y entrenar** con lo que se le manda
  (`provider: {data_collection: "deny"}`). Por defecto OpenRouter enruta también a proveedores que
  sí pueden hacerlo, así que esa línea no es un adorno.

## El tope de gasto va DENTRO de Pronto

Con el volumen real medido (161 dictados/mes), un mes entero de formateo en nube cuesta entre
**0,008 $ y 0,04 $** según el modelo. Superwhisper Pro son 8,49 $/mes por el mismo trabajo.

De ahí sale el peligro de verdad: el tope del vigilante del VPS es **2 $/día ≈ cincuenta meses de
uso normal**. Ese tope **no puede detectar nada** — un bucle o un reintento infinito se gasta los
2 $ antes de que salte, y el aviso llega cuando el dinero ya no está. El freno tiene que estar
donde se pulsa el botón:

- Tope por **día** (0,10 $) y por **mes** (1,00 $) por defecto, editables. Unas 25 veces el uso
  normal: no estorban a nadie y cortan un bucle en el sitio.
- Tope por **dictado** (4000 caracteres): sin él, un solo dictado disparatado se come el tope del
  día de una sentada.
- Se comprueba **al ENTRAR**: al gasto ya apuntado se le suma lo que va a costar ESTA llamada, y si
  con eso se pasa, la llamada no sale. Un tope que se comprueba a mitad es un tope que se cruza.
- Lo que se apunta es el coste **real** que devuelve OpenRouter en `usage.cost`. Si no viniera, se
  apunta la **estimación**, nunca un cero: una llamada que se hizo y se apunta como gratis es un
  contador que miente y un tope que no salta. Y se apunta **aunque el guardia anti-parafraseo
  descarte la salida**, porque esa llamada se pagó igual.
- El modelo de nube sale de una **lista blanca con su precio** (`kCloudPrices`). Uno desconocido no
  se puede elegir ni colar por el fichero de ajustes: el tope no sabría con qué precio contar.

## Lo que endureció la auditoría de seguridad

Seis cosas que el diseño de arriba no cubría y que ahora forman parte de él:

- **Mientras el texto está fuera, el dictado está OCUPADO** (`DictationState.isBusy` incluye
  `sendingToCloud`). Si no, pulsar el atajo con el texto en vuelo arrancaba otro dictado: la
  píldora naranja desaparecía con el texto todavía fuera —el aviso que el usuario necesita— y
  quedaban dos formateos volando contra el mismo tope. Es además lo que hace que preguntar al
  guardia y apuntar después sea seguro: sin dos dictados a la vez no hay dos llamadas a la vez.
- **El dinero que se gasta se apunta siempre, no solo cuando la respuesta sirve.** El proveedor
  genera y cobra aunque cortemos a los 12 segundos o no entendamos lo que contesta. Un bucle de
  tiempos agotados gastaba de verdad con el contador a cero. Un 4xx no se apunta: ahí no se generó
  nada.
- **Un coste de cero declarado por el proveedor no se cree.** Se apunta la estimación: si no, un
  proveedor que informe siempre 0 congelaría el tope para siempre.
- **La salida del modelo se sanea antes de pegarse.** Lo que se pega no se lee: se teclea, y el
  inyector convierte un salto de línea en un **Intro de verdad**. Un modelo descarrilado —o un
  proveedor comprometido— podía colar una orden detrás de un salto en la ventana con foco. Se
  colapsan saltos y tabuladores, y se caen los controles y los caracteres invisibles (anchura
  cero, marcas bidireccionales), que sirven para esconder texto dentro de texto.
- **El camino "local" también avisa si el servidor no está en esta máquina.** `llmBaseUrl` es un
  campo libre: apuntarlo a otro ordenador saca el dictado igual, y decir que solo la nube sale
  sería mentira.
- **Con el libro de cuentas sin leer, la nube no sale** (`BudgetVerdict.notReady`), y los topes
  tienen techo (`AppConfig.cloudMaxCapUsd`): teclear `100` donde se quería `1,00` no fija cien
  dólares al día en silencio.

## Consequences

- El camino de nube (`FormatPath.cloud`) es un campo del **Modo** cuando exista (ADR 0006); los
  **topes de gasto son Ajustes de la app**, porque protegen la cartera y la cartera es la misma
  escribas donde escribas.
- Un `formatPath: cloud` escrito a mano en el fichero de ajustes ya no se degrada a «sin formateo»,
  pero tampoco enciende nada: sin clave en el almacén de credenciales no sale ni una petición.
- **Pronto es público y lo instala cualquiera.** Mientras la clave la ponga el usuario y salga
  apagada, Beltran Labs no es encargado de tratamiento de nadie. Si algún día Pronto trajera claves
  nuestras o se ofreciera como servicio, entra el art. 28 del RGPD y el art. 50 del AI Act, y pasa
  por `cumplimiento-ia` antes de publicarse.
- Cambiar el modelo de nube por defecto es cambiar a quién se le manda el texto: se decide aquí, no
  en un ajuste suelto.
