# El modelo de formateo local lo dice la máquina, no una constante

Hasta la 0.9, el **Modelo de formateo** local era una **caja de texto** en Ajustes con un valor de
fábrica escrito en `AppConfig.defaultFormatLocalModel`. Pronto no le preguntaba a Ollama qué tenía
—no había ni una llamada a `/api/tags` en todo `lib/`— así que si ese modelo no estaba instalado, la
primera llamada devolvía un **404** y en pantalla se leía como «el Formateo no sirve».

No es hipotético: ya pasó. El defecto de la 0.7.1 era `qwen3:4b`, que **no existía** en el registro
público de Ollama, y está escrito en el comentario de esa misma constante. Se cambió por
`qwen2.5:7b`, que existe — pero eso no arregla nada, porque el problema no era *qué* nombre estaba
escrito: era que **el nombre estaba escrito**.

**Desde el PRNT-18, la lista sale de la máquina.** `OllamaCatalog` pregunta `/api/tags`, y lo que se
puede elegir es lo que hay instalado.

## Consequences

- **Se pregunta cada vez, y no se guarda ninguna marca.** El mismo criterio que el asistente de
  instalación, que mira el disco en vez de fiarse de un «ya lo descargaste»: un modelo se borra con
  un `ollama rm` y una marca seguiría diciendo que está.
- **Un modelo que no cabe en la memoria del equipo no se propone, pero se dice que está.** Un modelo
  que desaparece de la lista sin explicación parece una avería; el usuario sabe que lo tiene.
  El listón es el tamaño del modelo más 1 GB, y si **no se pudo leer** la memoria (`SystemMemory`
  devuelve `null`) **no se filtra nada**: inventarse una memoria para poder decir «no te cabe» es
  peor que no decirlo.
- **No juzgamos los modelos del usuario.** Solo se descarta lo que no puede generar texto (un modelo
  de *embeddings*). Un modelo de visión, o uno que no conocemos, entra en la lista: decidir que «ese
  no vale» sería un juicio que no hemos medido. Lo que sí se marca es lo que **sí** hemos medido.
- **El orden no es una opinión:** delante los que hemos medido nosotros (por su TER, el mejor
  primero) y detrás el resto por tamaño. Un modelo sin medir nunca adelanta a uno medido, igual que
  en la lista de Motores de voz del asistente (PRNT-13).
- **`AppConfig.defaultFormatLocalModel` sobrevive, pero como último recurso**, no como propuesta:
  solo queda en unos ajustes que nadie tocó nunca.
- **Sin Ollama, Pronto sigue funcionando.** La pantalla lo dice con una cadena nuestra, ofrece los
  otros dos Caminos, y el dictado se pega crudo. Cero errores feos.

## La puerta de licencia

Pronto es **MIT y público**, y el PRNT-19 va a **descargar un modelo en nombre del usuario**. Eso
convierte la licencia del modelo en un problema nuestro, no suyo.

**Solo entra en `kModelosMedidos` un modelo con licencia Apache-2.0 o MIT, comprobada en su ficha el
día que se elige** — no de memoria, no de un blog. Hay trampas reales ahí fuera: **Qwen2.5-3B** es
licencia de investigación (sus hermanos 1,5B y 7B sí son Apache), y **Llama 3.2** obliga a poner
«Built with Llama». Un test lo vigila (`format_preset_test.dart`), y la licencia queda escrita en
`NOTICE`.

## Y una tabla que no se escribe a mano

`kModelosMedidos` no es la tabla de FluidVoice. La suya (`SettingsStore.swift`) pinta barras de
«Speed 100 % / Acc 96 %» sacadas de valores escritos a mano: sin corpus, sin métrica y sin fecha.
La nuestra solo se llena **corriendo el banco** (`tools/formateo_bench.dart`) contra el corpus real
en español, y cada fila trae el número de dictados, el día y **la máquina**. Donde no hay medida, la
pantalla dice «Sin medir» — nunca un número a ojo.
