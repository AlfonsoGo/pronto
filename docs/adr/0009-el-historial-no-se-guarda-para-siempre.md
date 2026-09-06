# El Historial no se guarda para siempre, y sale del equipo avisando

El Historial guarda **90 días** de fábrica (`AppConfig.defaultHistoryRetentionDays`). Lo más viejo
se borra al arrancar Pronto. Guardarlo todo sigue siendo posible —retención «Para siempre», el 0 de
los ajustes— pero hay que elegirlo a mano.

## Por qué no es infinito

En `dictation_log`, dentro de `pronto.db`, está **todo lo que el usuario ha dictado nunca**: correos
enteros, nombres de clientes, direcciones, lo que dijera en voz alta sin pensar que quedaba escrito.
Desde el PRNT-16 está además con quién y dónde: la app de destino y el título de la ventana.

La base **no está cifrada**. Vive en `%APPDATA%\Pronto\Pronto\pronto.db`, la lee cualquier proceso
del usuario, y entra en la copia previa de cada migración (ADR 0008), así que se multiplica sola.

Una retención por defecto infinita no es una decisión: es un olvido con consecuencias que crecen
solas. Tres meses cubren de sobra para qué sirve el historial de verdad —buscar lo de la semana
pasada, sacar el corpus con el que se mide el formateo, ver las estadísticas del mes— y a partir de
ahí lo único que crece es el daño de una fuga.

El cero significa **guardar para siempre**, no «borrar todo»: `purgeOlderThan(Duration.zero)`
devuelve 0 sin tocar nada, y hay un test que lo fija. Un cero mal leído en el sitio equivocado
borraría el historial entero al arrancar.

## Exportar avisa antes, no después

`toCsv` / `toJson` escriben un fichero en claro con el texto completo. La pantalla enseña
`exportWarning(n)` **antes** de escribirlo, diciendo cuántos dictados van y que el fichero no está
cifrado. El aviso va delante porque después ya no sirve de nada.

## El formato de exportación se decide aquí, una vez

El CSV/JSON del historial es **el corpus** con el que se miden el formateo (PRNT-1) y el vocabulario
(PRNT-12/17). Si las columnas cambian más adelante, hay que rehacer todas las medidas tomadas con el
formato anterior. Las columnas están enumeradas en la cabecera de `history_export.dart` y no se
reordenan ni se renombran sin decirlo.

Dos decisiones del CSV que no son estéticas: **BOM UTF-8** y **punto y coma** como separador. Sin
BOM, Excel en español abre el fichero como página de códigos de Windows y «reunión» sale
«reuniÃ³n»; con coma como separador, todo cae en una sola columna porque la coma es el separador
decimal. Las dos cosas tienen su test.

## Lo que no se borra

Lo que Pronto **aprendió** de ti —`corrections` y `vocab_terms`— no depende del historial y no se
va con él. Borrar el historial borra lo que dijiste, no lo que Pronto sabe escribir.
