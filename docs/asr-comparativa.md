# Comparativa Parakeet vs Whisper (17-oct-2026, RTX 4070 Ti, faster-whisper 1.2.1 / sherpa-onnx)

Parcial: se paró antes de medir turbo en CPU int8. Pistas a y b = voces TTS con guion; dictados = audio real de Txonso.

## Tabla resumen (4 pistas: a/b x me/them; pista completa con los silencios del otro interlocutor)

WER = minusculas, sin puntuacion, respeta tildes. WER s/t = igual sin tildes. CER = sobre caracteres (respeta tildes). s/min = segundos de computo por minuto de audio de la pista.

| Motor | WER | WER s/t | CER | s por min de audio | factor RT | VRAM pico (MiB sobre base) | Carga (s) |
|---|---|---|---|---|---|---|---|
| Parakeet TDT 0.6B v3 int8, CPU 4 hilos (la app) | 7.0% | 6.9% | 5.3% | 5.6 | 0.094 | 0 (CPU) | 2.0 |
| Whisper large-v3-turbo GPU fp16, beam 1 | 4.4% | 4.4% | 3.5% | 1.6 | 0.026 | 4320 | 4.8 |
| Whisper large-v3-turbo GPU fp16, beam 5 | 4.4% | 4.4% | 3.5% | 3.4 | 0.056 | 2535 | 4.8 |
| Whisper large-v3 GPU fp16, beam 5 | 7.6% | 7.4% | 5.9% | 8.1 | 0.135 | 4741 | 19.6 |
| Whisper medium GPU fp16, beam 5 | 4.4% | 4.4% | 3.7% | 7.8 | 0.131 | 2431 | 8.8 |
| Whisper small GPU fp16, beam 5 | 7.7% | 7.3% | 6.0% | 4.4 | 0.074 | 918 | 20.5 |

## Mismo test troceado por turnos (como haria Meets: un trozo por intervencion, +-0,3 s)

| Motor | WER | WER s/t | CER | s por min de HABLA | factor RT sobre habla |
|---|---|---|---|---|---|
| Parakeet TDT 0.6B v3 int8, CPU 4 hilos (la app) | 5.7% | 5.5% | 4.4% | 6.2 | 0.103 |
| Whisper large-v3-turbo GPU fp16, beam 1 | 4.2% | 4.2% | 3.3% | 3.7 | 0.062 |
| Whisper large-v3-turbo GPU fp16, beam 5 | 3.8% | 3.8% | 3.1% | 5.9 | 0.099 |
| Whisper large-v3 GPU fp16, beam 5 | 3.8% | 3.8% | 3.1% | 14.1 | 0.235 |
| Whisper medium GPU fp16, beam 5 | 3.4% | 3.2% | 2.7% | 12.7 | 0.212 |
| Whisper small GPU fp16, beam 5 | 4.0% | 4.0% | 3.0% | 6.0 | 0.100 |

## Latencia de dictado tipico (modelo ya cargado, mediana de 5-7 ejecuciones; audio real del usuario)

| Motor | 5 s (dictado 142) | 30 s (dictado 84) |
|---|---|---|
| Parakeet TDT 0.6B v3 int8, CPU 4 hilos (la app) | 0.63 s | 3.38 s |
| Whisper large-v3-turbo GPU fp16, beam 1 | 0.27 s | 1.18 s |
| Whisper large-v3-turbo GPU fp16, beam 5 | 0.56 s | 3.61 s |
| Whisper large-v3 GPU fp16, beam 5 | 1.11 s | 5.22 s |
| Whisper medium GPU fp16, beam 5 | 0.89 s | 7.79 s |
| Whisper small GPU fp16, beam 5 | 0.42 s | 3.05 s |

## Detalle por pista (WER / CER, pista completa)

| Motor | a_me | a_them | b_me | b_them |
|---|---|---|---|---|
| Parakeet TDT 0.6B v3 int8, CPU 4 hilos (la app) | 4.5% / 3.2% | 2.8% / 2.6% | 15.4% / 11.2% | 5.1% / 5.0% |
| Whisper large-v3-turbo GPU fp16, beam 1 | 3.9% / 2.7% | 2.8% / 2.0% | 6.2% / 5.2% | 5.1% / 5.0% |
| Whisper large-v3-turbo GPU fp16, beam 5 | 3.9% / 2.7% | 2.8% / 2.0% | 6.2% / 5.2% | 5.1% / 5.0% |
| Whisper large-v3 GPU fp16, beam 5 | 11.9% / 8.5% | 2.8% / 2.0% | 5.6% / 4.9% | 5.1% / 5.0% |
| Whisper medium GPU fp16, beam 5 | 2.3% / 2.1% | 2.8% / 2.0% | 8.2% / 6.4% | 6.0% / 6.1% |
| Whisper small GPU fp16, beam 5 | 8.7% / 7.0% | 4.2% / 2.2% | 10.3% / 8.0% | 5.1% / 5.0% |

## Errores de palabra (pista completa) sobre 766 palabras de referencia

- Parakeet TDT 0.6B v3 int8, CPU 4 hilos (la app): sustituciones=33 borrados=21 inserciones=0
- Whisper large-v3-turbo GPU fp16, beam 1: sustituciones=25 borrados=9 inserciones=0
- Whisper large-v3-turbo GPU fp16, beam 5: sustituciones=25 borrados=9 inserciones=0
- Whisper large-v3 GPU fp16, beam 5: sustituciones=25 borrados=15 inserciones=18
- Whisper medium GPU fp16, beam 5: sustituciones=22 borrados=11 inserciones=1
- Whisper small GPU fp16, beam 5: sustituciones=27 borrados=32 inserciones=0
## Alucinaciones y anomalias detectadas (busqueda automatica; revisadas a mano abajo)

### Parakeet TDT 0.6B v3 int8, CPU 4 hilos (la app)
- (nada detectado)

### Whisper large-v3-turbo GPU fp16, beam 1
- [dictado 174] patron tipico: «Hola gente, a todos los que uséis Whisper Flow os presento mi nueva herramienta Pronto 2, que aparte de que es mucho más rápida, en plan es todo en local, podéis echarle un vistazo en la web www.pronto.beltranlas.com»

### Whisper large-v3-turbo GPU fp16, beam 5
- [dictado 174] patron tipico: «Hola gente, a todos los que uséis Whisper Flow os presento mi nueva herramienta Pronto 2, que aparte de que es mucho más rápida, en plan es todo en local, podéis echarle un vistazo en la web www.pronto.beltranlas.com»

### Whisper large-v3 GPU fp16, beam 5
- [tracks a_me] insercion de 18 palabras no presentes en la referencia: «qué pasa qué pasa qué pasa qué pasa qué pasa qué pasa qué pasa qué pasa qué pasa»
- [tracks a_me] repeticion: «qué pasa» x3+

### Whisper medium GPU fp16, beam 5
- [dictado 174] patron tipico: «Hola gente, a todos los que uséis Whisperflow os presento mi nueva herramienta Pronto 2 que aparte de que es mucho más rápida en plan es todo en local, podéis echarle un vistazo en la web www.pronto.beltramlas.com»

### Whisper small GPU fp16, beam 5
- [dictado 174] patron tipico: «Hola gente, a todos los que useis WhisperFlow os presento mi nueva herramienta Pronto 2, que a partir de que es mucho más rápida. En pan es todo en local, podéis echar un vistazo en la web www.pronto.beltranlast.com»

Nota: lo del dictado 174 no es una alucinación (es lo que dijo); la de large-v3 («qué pasa» ×9) sí.

## Los 12 dictados reales lado a lado

Parakeet(app) = `raw_text` de la base (lo que saco la app en su dia); Parakeet(bench) = el mismo audio con el modelo actual via sherpa-onnx Python, 4 hilos. Tiempos entre parentesis.

### Dictado 180 (2.0 s)

- **Parakeet (app, BD)**: It is una prevail.
- **Parakeet (bench)** (0.34 s): It is one prevail.
- **Turbo b5** (0.26 s): Esto es una prueba.
- **Large-v3 b5** (0.53 s): Esta es una prueba.

### Dictado 188 (2.1 s)

- **Parakeet (app, BD)**: Beltrand Laps.
- **Parakeet (bench)** (0.39 s): Beltrand Laps.
- **Turbo b5** (0.24 s): Beltran Labs
- **Large-v3 b5** (0.50 s): Beltran Labs

### Dictado 12 (3.0 s)

- **Parakeet (app, BD)**: Esto es una prueba de whisper, a ver qué está haciendo.
- **Parakeet (bench)** (0.47 s): Esto es una prueba de whisper, a ver qué está haciendo.
- **Turbo b5** (0.36 s): esto es una prueba de wish pero a ver qué está haciendo
- **Large-v3 b5** (0.82 s): Esto es una prueba de Whisper, a ver qué está haciendo

### Dictado 75 (3.8 s)

- **Parakeet (app, BD)**: Usa su agentes en Sone si lo necesitas, aunque Opus dirija.
- **Parakeet (bench)** (0.57 s): Usa sub agentes en Sone si lo necesitas, aunque Opus dirija.
- **Turbo b5** (0.41 s): Usa subagentes en Sone si lo necesitas, aunque Opus dirija.
- **Large-v3 b5** (0.98 s): Usa subagentes en Sony si lo necesitas, aunque Opus dirija.

### Dictado 105 (4.7 s)

- **Parakeet (app, BD)**: donde puedo encontrar toda la información de pronto en que carpeta está pronto 2
- **Parakeet (bench)** (0.64 s): donde puedo encontrar toda la información de pronto en que carpeta está pronto 2
- **Turbo b5** (0.42 s): donde puedo encontrar toda la información de pronto, en qué carpeta está pronto 2.
- **Large-v3 b5** (0.85 s): donde puedo encontrar toda la información de pronto, en qué carpeta está pronto 2.

### Dictado 164 (7.0 s)

- **Parakeet (app, BD)**: Me he registrado con mi correo desde pronto eltranlas.com. Mira a ver si te ha llegado el correo y demás.
- **Parakeet (bench)** (0.66 s): Me he registrado con mi correo desde pronto eltranlas.com. Mira a ver si te ha llegado el correo y demás.
- **Turbo b5** (0.50 s): Me he registrado con mi correo desde pronto, veltranlas.com, mira a ver si te ha llegado el correo y demás.
- **Large-v3 b5** (1.54 s): Me he registrado con mi correo desde pronto eltranlas.com, mira a ver si te ha llegado el correo y demás.

### Dictado 116 (7.6 s)

- **Parakeet (app, BD)**: Pruéámoslos todos en esta de aquí. Además, el mod de reflect está en inglés. Pásamelo al español, por favor.
- **Parakeet (bench)** (0.59 s): Pruéámelos todos en esta de aquí. Además, el mod de reflect está en inglés. Pásamelo al español, por favor.
- **Turbo b5** (0.47 s): Pruébamelos todos en esta de aquí. Además, el mod de Reflet está en inglés. Pásamelo al español, por favor.
- **Large-v3 b5** (1.45 s): Pruébamelos todos en esta de aquí. Además el mod de Reflet está en inglés, pásamelo al español, por favor.

### Dictado 128 (8.7 s)

- **Parakeet (app, BD)**: Creo que sacar ya pronto V2 de pruebas y hacer una carpeta que sea pronto V2 dentro de Beltrand Labs 2.
- **Parakeet (bench)** (0.66 s): Creo que hay que sacar ya pronto V2 de pruebas y hacer una carpeta que sea pronto V2 dentro del Translabs 2.
- **Turbo b5** (0.43 s): Creo que hay que sacar ya Pronto V2 de pruebas y hacer una carpeta que sea Pronto V2 dentro de Beltran Labs 2.
- **Large-v3 b5** (1.39 s): Creo que hay que sacar ya Pronto V2 de pruebas y hacer una carpeta que sea Pronto V2 dentro de Beltran Labs 2.

### Dictado 28 (11.1 s)

- **Parakeet (app, BD)**: In nuevo update al cerebro al Beltrán Labs a iOS business.
- **Parakeet (bench)** (0.95 s): In nuevo update al cerebro al Beltrán.
- **Turbo b5** (0.37 s): Ingiere el nuevo update al cerebro, a Beltrán Labs IOS Business.
- **Large-v3 b5** (1.40 s): Iniciar el nuevo update al cerebro, a Beltran Labs IOS Business.

### Dictado 174 (19.6 s)

- **Parakeet (app, BD)**: Hola gente, a todos los que uséis Whisperflow os presento mi nueva herramienta Pronto 2 que aparte de que es mucho más rápida En plan es todo en local Puedes darle un vistazo en la web wonto Beltranblast.com
- **Parakeet (bench)** (2.02 s): Hola gente, a todos los que uséis Whisperflow os presento mi nueva herramienta Pronto 2 que aparte de que es mucho más rápida en plan es todo en local podéis darle un vistazo en la web w punto pronto punto Beltranlas.com
- **Turbo b5** (1.21 s): Hola gente, a todos los que uséis Whisper Flow os presento mi nueva herramienta Pronto 2, que aparte de que es mucho más rápida, en plan es todo en local, podéis echarle un vistazo en la web www.pronto.beltranlas.com
- **Large-v3 b5** (2.84 s): Hola gente, a todos los que uséis WhisperFlow os presento mi nueva herramienta Pronto 2 que aparte de que es mucho más rápida, en plan es todo en local, podéis echarle un vistazo en la web

### Dictado 84 (29.9 s)

- **Parakeet (app, BD)**: Yo, para tema de trabajo, se lo he dicho a Sunna y va a pagar ChatGPT. Chat GPT entiende mejor los contextos, entiende mejor para trabajar con él. Yo por ejemplo, GPT la uso para cuando quiero modificar algo a nivel. Pues le explico algo y no lo entienden, pues que se aclare GPT a la otra, ¿sabes? Es como más concisa. Pero vamos, que coges la que quieras. Las dos van bien, pero para casos de ese estilo, yo creo que va mejor Chat GPT.
- **Parakeet (bench)** (3.06 s): Yo, para tema de trabajo, se lo he dicho a Sunday y va a pagar ChatGPT. Chat GPT entiende mejor los contextos, entiende mejor para trabajar con él. Yo, por ejemplo, GPT la uso para cuando quiero modificar algo a nivel. Pues le explico algo y no lo entienden, pues que se aclare GPT a la otra, ¿sabes? Es como más concisa. Pero vamos, que coges la que quieras. Las dos van bien, pero para casos de ese estilo, yo creo que va mejor Chat GPT.
- **Turbo b5** (2.69 s): Yo para tema de trabajos se lo he dicho a Sonia y va a pagar ChagPT. ChagPT entiende mejor los contextos, entiende mejor para trabajar con él. Yo por ejemplo, GPT la uso para cuando quiero modificar algo a nivel, pues le explico algo y no lo entiende, pues que se aclare GPT a la otra, ¿sabes? Es como más concisa, pero vamos, que coges la que quieras. Las dos van bien, pero para casos de ese estilo yo creo que va mejor ChagPT.
- **Large-v3 b5** (5.08 s): Yo para tema de trabajos se lo he dicho a Sonia y va a pagar ChagPT. ChagPT entiende mejor los contextos, entiende mejor para trabajar con él. Yo, por ejemplo, ChagPT la uso para cuando quiero modificar algo a nivel, pues le explico algo y no lo entiende, pues que se aclare ChagPT a la otra, ¿sabes? Es como más concisa. Pero vamos, que coges la que quieras. Las dos van bien, pero para casos de ese estilo yo creo que va mejor ChagPT.

### Dictado 102 (51.2 s)

- **Parakeet (app, BD)**: La dirección quiero que sea pronto. Beltranlabs.com. Quiero añadir la lista de correos que entren a algún sitio. Si es en el VPS, pues mejor en el apartado que sea Libs o Contacto. Que ponga que le pida en el contacto el nombre, apellido. El correo, número de teléfono nacional. Y como se dice, y que he puesto o que a que se dedica que le salgan como unas varias un desplegable y que acepte los términos. Por eso de que si quiere que tenga la opción de suscribirse al newsletter o no y que venga eso todo, lo puente hacia el VPS.
- **Parakeet (bench)** (5.11 s): La dirección quiero que sea pronto.beltranlabs.com. Quiero añadir la lista de correos que entren a algún sitio. Si es en el VPS, pues mejor en el apartado que sea Libs o Contacto. Que ponga que le pida en el contacto, el nombre, apellido, el correo, número de teléfono nacional. Y como se dice, y que he puesto o que a que se dedica que le salgan como unas varias un desplegable y que acepte los términos, por eso de que si quiere que tenga la opción de suscribirse al newsletter o no y que venga eso todo, lo puente hacia el VPS.
- **Turbo b5** (2.19 s): La dirección quiero que sea pronto.beltranlabs.com Quiero añadir la lista de correos que entren a algún sitio Si es en el VPS, mejor en un apartado que sea leads o contactos Que le pida en el contacto el nombre, apellido, el correo, número de teléfono nacional Y que he puesto a qué se dedica, que le salgan como unas varias desplegables Y que acepte los términos de que si quiere que tenga la opción de suscribirse a mi newsletter o no Y que venga eso todo lo puente hacia el VPS
- **Large-v3 b5** (6.82 s): La dirección quiero que sea pronto.beltranlabs.com Quiero añadir la lista de correos que entren a algún sitio. Si es en el VPS, mejor en el apartado que sea Lips o Contactos, que le pida en el contacto el nombre, apellido, el correo, número de televisión opcional y... ¿Cómo se dice? Que he puesto a que se dedica, que les hagan como unas varias desplegables y que acepte los términos de que si quiere que tenga la opción de suscribirse al newsletter o no y que venga eso todo lo puente hacia el VPS.
