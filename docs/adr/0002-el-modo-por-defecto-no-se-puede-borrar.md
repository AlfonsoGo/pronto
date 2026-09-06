# El Modo por Defecto no se puede borrar

Siempre hay exactamente un Modo activo, porque el Atajo global tiene que saber con qué grabar en
el momento en que se pulsa. Para que ese invariante no dependa de lo que el usuario haya borrado,
Pronto crea un Modo llamado **«Por defecto»** que no se puede eliminar y al que cae todo lo que no
encuentre otro Modo.

La alternativa —permitir borrarlo y recrearlo si hace falta— deja un hueco real: entre el borrado
y la recreación, un Atajo global pulsado no tiene Modo, y eso ocurre fuera de la app, con el foco
en otra ventana y sin nadie mirando. Un estado imposible de recuperar bien es peor que un botón
menos.

## Consequences

- Borrar el Modo por Defecto falla con un mensaje **nuestro**, no con un error de base de datos.
- Sí se puede **editar** y renombrar: lo que no se puede es dejar a Pronto sin él.
- Borrar el Modo activo (cualquier otro) deja activo el Por Defecto, nunca «ninguno».
- La migración desde 0.7.1 es idempotente y crea exactamente uno: abrir dos veces no da dos.
