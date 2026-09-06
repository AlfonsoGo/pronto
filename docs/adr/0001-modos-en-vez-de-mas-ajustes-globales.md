# Modos, no más ajustes globales

Hasta la 0.7.1 Pronto tenía **una** configuración: un atajo, un motor, un camino de formateo, un
idioma de salida. Cada petición nueva («que en el terminal no me capitalice», «que en el correo me
lo escriba de usted») llegaba como un interruptor más en Ajustes. Elegimos parar esa escalera y
convertir la configuración en **N Modos con uno activo**.

El motivo es que esas peticiones no son preferencias del usuario: son propiedades del **sitio
donde está escribiendo**. El mismo usuario quiere cosas opuestas en el terminal, en WhatsApp y en
un correo, y en el mismo minuto. Un interruptor global obliga a elegir una de las tres y a ir a
Ajustes a cambiarla; un Modo con su Atajo hace que la elección sea una tecla.

## Considered Options

- **Más ajustes globales.** Más barato de construir y de probar, y no rompe la migración. Se
  descarta porque el nº de interruptores crece con cada aplicación en la que se dicta, y ninguna
  combinación queda guardada: el usuario paga el coste otra vez cada vez que cambia de ventana.
- **Un prompt libre y nada más.** Deja fuera lo que no es prompt (motor, atajo, idioma, contexto),
  que es justo lo que cambia entre el terminal y el correo.

## Consequences

- El Modo pasa a ser el módulo profundo del proyecto: mucha decisión detrás de una interfaz
  pequeña (`Modo activo → cómo transcribo y cómo formateo`). Si al borrarlo la complejidad
  reaparece repartida entre el controlador, los ajustes y la píldora, el módulo estaba mal hecho.
- `dictation_controller.dart` deja de leer ajustes sueltos: lee el Modo activo.
- Todo lo que hoy es global tiene que decidirse uno por uno — ver [ADR 0006](0006-que-vive-en-el-modo-y-que-sigue-siendo-un-ajuste-de-la-app.md).
