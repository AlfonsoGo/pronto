import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../settings/settings_controller.dart';
import 'cloud_key.dart';
import 'formatter_provider.dart';
import 'modelos_locales.dart';
import 'ollama_catalog.dart';
import 'text_formatter.dart';

/// El comparador de los tres Caminos de formateo, uno al lado del otro.
///
/// Es la variante C del prototipo 06, la que eligió Txonso: en vez de un
/// desplegable que solo dice el nombre de cada camino, se ponen los tres a la
/// vista con **lo que se paga por cada uno** — espera, dinero, descarga,
/// privacidad y licencia. Se usa en dos sitios (Ajustes y el asistente de
/// instalación) para que la decisión se vea igual en los dos.
///
/// ⚠ Ni un número de esta pantalla está escrito a ojo. La espera sale de
/// [kModelosMedidos] —el banco, con su corpus, su equipo y su fecha— y cuando
/// no la hemos medido **dice que no la hemos medido**, que es justo lo que
/// FluidVoice no hace con sus barras de «Speed 100 % / Acc 96 %».
class AssistantComparator extends ConsumerWidget {
  const AssistantComparator({super.key, this.onElegido});

  /// Se llama al elegir un camino, después de guardarlo. El asistente de
  /// instalación lo usa para marcar su bloque como hecho.
  final VoidCallback? onElegido;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final ajustes = ref.watch(settingsProvider);
    final hayClave = ref.watch(cloudKeyPresenceProvider);
    final catalogo = ref.watch(ollamaCatalogProvider);
    final medida = modeloMedido(ajustes.formatModel);

    Future<void> elegir(FormatPath camino) async {
      await ref.read(settingsProvider.notifier).setFormatPath(camino);
      onElegido?.call();
    }

    // `enLinea` lo decide la anchura TOTAL, no cada tarjeta por su cuenta: una
    // tarjeta que se mide a sí misma necesita un LayoutBuilder dentro, y un
    // LayoutBuilder no deja calcular la altura intrínseca de la fila — que es
    // justo lo que iguala las tres columnas.
    List<Widget> crearTarjetas(bool enLinea) => <Widget>[
      _Via(
        nombre: 'Sin Formateo',
        enLinea: enLinea,
        etiqueta: 'de fábrica',
        nota: 'El texto sale tal y como lo oye el motor. Es el camino veloz.',
        datos: const [
          ('Espera por dictado', 'Ninguna'),
          ('Coste', 'Ninguno'),
          ('Descarga', 'Nada'),
          ('Privacidad', 'Nada sale del equipo'),
          ('Licencia', '—'),
        ],
        seleccionado: ajustes.formatPath == FormatPath.none,
        onTap: () => elegir(FormatPath.none),
        pie: const SizedBox.shrink(),
      ),
      _Via(
        nombre: 'En tu equipo',
        enLinea: enLinea,
        nota: 'Un modelo que corre aquí arregla tildes, comas y muletillas. '
            'Necesita Ollama.',
        datos: [
          (
            'Espera por dictado',
            medida == null
                ? 'Sin medir'
                : '~${_segundos(medida.segundosMediana)}'
          ),
          ('Coste', 'Ninguno'),
          ('Descarga', _descargaLocal(catalogo)),
          ('Privacidad', 'Nada sale del equipo'),
          ('Licencia', medida?.licencia ?? 'La del modelo que uses'),
        ],
        seleccionado: ajustes.formatPath == FormatPath.local,
        onTap: () => elegir(FormatPath.local),
        pie: const _PieLocal(),
      ),
      _Via(
        nombre: 'Nube con tu clave',
        enLinea: enLinea,
        nota: 'Sale el TEXTO ya transcrito, nunca el audio, y con tope de '
            'gasto.',
        datos: [
          (
            'Espera por dictado',
            modeloMedido(ajustes.cloudModel) == null
                ? 'Sin medir'
                : '~${_segundos(modeloMedido(ajustes.cloudModel)!.segundosMediana)}'
          ),
          ('Coste', 'Tope de ${_usd(ajustes.cloudDayCapUsd)} al día'),
          ('Descarga', 'Nada'),
          ('Privacidad', 'Sale el texto dictado'),
          ('Licencia', '—'),
        ],
        seleccionado: ajustes.formatPath == FormatPath.cloud,
        // Sin clave no se puede elegir: encenderla dejaría un camino que falla
        // en el primer dictado.
        onTap: hayClave ? () => elegir(FormatPath.cloud) : null,
        pie: hayClave
            ? const SizedBox.shrink()
            : const _Nota(
                'En gris porque no hay ninguna clave guardada. Se pone en '
                'Ajustes; puedes dejarlo para luego.',
              ),
      ),
    ];

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        LayoutBuilder(
          builder: (context, limites) {
            // Por debajo de esto las tres columnas dejan de poder leerse y se
            // apilan. Compararlas de un vistazo es el sentido de la pantalla:
            // tres columnas ilegibles no comparan nada.
            final enColumnas = limites.maxWidth >= 560;
            final anchoTarjeta =
                enColumnas ? (limites.maxWidth - 20) / 3 : limites.maxWidth;
            // 28 son los dos rellenos de la tarjeta. Por debajo de 200 de
            // contenido, «Espera por dictado» y su valor en la misma fila se
            // quedan en dos letras cada uno: ahí van uno encima del otro.
            final tarjetas = crearTarjetas(anchoTarjeta - 28 >= 200);

            if (!enColumnas) {
              return Column(
                children: [
                  for (final t in tarjetas)
                    Padding(
                      padding: const EdgeInsets.only(bottom: 10),
                      child: t,
                    ),
                ],
              );
            }
            // `IntrinsicHeight` para que las tres columnas midan lo mismo: es
            // un comparador, y tres tarjetas de alturas distintas se leen como
            // si una valiera más que otra. Hace falta además porque esto vive
            // dentro de una lista sin alto acotado, y ahí un `stretch` a secas
            // pide alto infinito.
            return IntrinsicHeight(
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  for (var i = 0; i < tarjetas.length; i++) ...[
                    if (i > 0) const SizedBox(width: 10),
                    Expanded(child: tarjetas[i]),
                  ],
                ],
              ),
            );
          },
        ),
        if (medida != null) ...[
          const SizedBox(height: 10),
          _Nota(
            'La espera de «${medida.tag}» está medida: ${medida.dictados} '
            'dictados en español el ${medida.medidoEl}, en ${medida.equipo}. '
            'En tu equipo puede ser otra.',
          ),
        ],
      ],
    );
  }

  static String _descargaLocal(AsyncValue<OllamaCatalog> catalogo) {
    final valor = catalogo.valueOrNull;
    if (valor == null) return 'Mirando…';
    return switch (valor.reach) {
      OllamaReach.disponible => 'Nada: ya lo tienes',
      // «sinModelos» tiene dos motivos —ninguno sirve, o ninguno cabe— y los
      // dos se explican debajo, en la nota del catálogo. Aquí basta con que la
      // fila no diga «ya lo tienes».
      OllamaReach.sinModelos => 'Ninguno usable',
      OllamaReach.noResponde => 'Falta Ollama',
    };
  }

  static String _segundos(double s) =>
      '${s.toStringAsFixed(1).replaceAll('.', ',')} s';

  static String _usd(double v) =>
      '${v.toStringAsFixed(2).replaceAll('.', ',')} \$';
}

/// El estado real de Ollama debajo de la tarjeta local: qué modelos hay, cuál
/// se usa, y qué hacer si no responde.
///
/// Es el corazón del PRNT-18: hasta la 0.9 el nombre del modelo se escribía a
/// mano en una caja de texto y no se comprobaba nunca. Si no estaba instalado,
/// el primer dictado devolvía un 404 y en pantalla parecía que el Formateo no
/// servía.
class _PieLocal extends ConsumerWidget {
  const _PieLocal();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final scheme = Theme.of(context).colorScheme;
    final catalogo = ref.watch(ollamaCatalogProvider);
    final elegido = ref.watch(settingsProvider.select((s) => s.formatModel));

    return catalogo.when(
      loading: () => const _Nota('Preguntándole a Ollama qué modelos tiene…'),
      // El catálogo NUNCA lanza: si se llega aquí es un fallo de programación
      // nuestro, no del servidor, y se dice sin enseñar la excepción.
      error: (_, __) => const _Nota(
        'No se ha podido leer la lista de modelos. Comprueba la URL del '
        'servidor local en Ajustes.',
      ),
      data: (c) {
        if (c.reach != OllamaReach.disponible) {
          return Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              _Nota(c.notice),
              const SizedBox(height: 6),
              const _BotonMirar(),
            ],
          );
        }

        // El modelo guardado, resuelto contra lo que la máquina ofrece AHORA.
        //
        // 🔴 No vale comparar cadenas: el desplegable exige que su `value` sea
        // exactamente uno de sus items, y hay dos formas de fallar. Una, que el
        // ajuste diga «qwen3» y la máquina lo llame «qwen3:latest» — el mismo
        // modelo escrito distinto. Otra, que esté instalado pero NO quepa en la
        // memoria: entonces no está entre los items, y darle ese `value` al
        // desplegable revienta la pantalla entera de Ajustes.
        final actual =
            c.modelos.where((m) => m.coincideCon(elegido)).firstOrNull;
        final noCabe = actual == null && c.tiene(elegido);

        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            DropdownButton<String>(
              key: const ValueKey('modeloLocal'),
              isExpanded: true,
              value: actual?.tag,
              hint: Text(
                'Elige uno',
                style: TextStyle(fontSize: 12, color: scheme.onSurfaceVariant),
              ),
              style: TextStyle(fontSize: 12, color: scheme.onSurface),
              items: [
                for (final m in c.modelos)
                  DropdownMenuItem(
                    value: m.tag,
                    child: Text(
                      _etiqueta(m),
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
              ],
              onChanged: (v) {
                if (v != null) {
                  ref.read(settingsProvider.notifier).setFormatModel(v);
                }
              },
            ),
            if (actual == null)
              _Nota(
                noCabe
                    ? '«$elegido» está instalado pero no cabe en la memoria de '
                        'este equipo: elige otro de los ${c.modelos.length} '
                        'que sí caben.'
                    : '«$elegido» no está en tu Ollama: elige uno de los '
                        '${c.modelos.length} que sí tienes.',
                aviso: true,
              ),
            // Los demás que no caben. El elegido se salta: su aviso, más
            // concreto, ya está justo arriba y decirlo dos veces seguidas en
            // una tarjeta de cinco filas es ruido.
            for (final m in c.noCaben)
              if (!m.coincideCon(elegido))
                _Nota(
                  'Tienes ${m.tag}, pero no cabe en la memoria de este equipo.',
                ),
            const _BotonMirar(),
          ],
        );
      },
    );
  }

  /// Lo que Ollama dice de él, tal cual. Los modelos que hemos medido llevan
  /// una marca; los demás, nada — no un número inventado.
  static String _etiqueta(OllamaModel m) {
    final partes = <String>[
      m.tag,
      if (m.parametros != null) m.parametros!,
      if (m.medido != null) 'medido',
    ];
    return partes.join(' · ');
  }
}

class _BotonMirar extends ConsumerWidget {
  const _BotonMirar();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return TextButton(
      key: const ValueKey('volverAMirarOllama'),
      onPressed: () => ref.invalidate(ollamaCatalogProvider),
      style: TextButton.styleFrom(
        padding: EdgeInsets.zero,
        minimumSize: const Size(0, 28),
        tapTargetSize: MaterialTapTargetSize.shrinkWrap,
      ),
      child: const Text('Volver a mirar', style: TextStyle(fontSize: 12)),
    );
  }
}

/// Una de las tres columnas.
class _Via extends StatelessWidget {
  const _Via({
    required this.nombre,
    required this.nota,
    required this.datos,
    required this.seleccionado,
    required this.onTap,
    required this.pie,
    required this.enLinea,
    this.etiqueta,
  });

  final String nombre;
  final String? etiqueta;
  final String nota;
  final List<(String, String)> datos;

  /// Clave y valor caben en la misma fila. Lo decide el comparador mirando la
  /// anchura total, no la tarjeta: ver [AssistantComparator].
  final bool enLinea;

  final bool seleccionado;
  final VoidCallback? onTap;
  final Widget pie;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final apagado = onTap == null;
    return InkWell(
      onTap: onTap,
      borderRadius: BorderRadius.circular(12),
      child: Opacity(
        opacity: apagado ? 0.55 : 1,
        child: Container(
          padding: const EdgeInsets.all(14),
          decoration: BoxDecoration(
            color: scheme.surfaceContainerHigh,
            borderRadius: BorderRadius.circular(12),
            border: Border.all(
              color: seleccionado ? scheme.primary : scheme.outlineVariant,
              width: seleccionado ? 1.5 : 1,
            ),
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Flexible(
                    child: Text(
                      nombre,
                      style: TextStyle(
                        color: scheme.onSurface,
                        fontSize: 14,
                        fontWeight: FontWeight.w700,
                      ),
                    ),
                  ),
                  if (etiqueta != null) ...[
                    const SizedBox(width: 6),
                    Container(
                      padding: const EdgeInsets.symmetric(
                        horizontal: 7,
                        vertical: 2,
                      ),
                      decoration: BoxDecoration(
                        color: scheme.onSurfaceVariant.withValues(alpha: 0.14),
                        borderRadius: BorderRadius.circular(99),
                      ),
                      child: Text(
                        etiqueta!,
                        style: TextStyle(
                          color: scheme.onSurfaceVariant,
                          fontSize: 10,
                        ),
                      ),
                    ),
                  ],
                ],
              ),
              const SizedBox(height: 6),
              Text(
                nota,
                style: TextStyle(
                  color: scheme.onSurfaceVariant,
                  fontSize: 11.5,
                  height: 1.35,
                ),
              ),
              const SizedBox(height: 10),
              for (final (clave, valor) in datos)
                Padding(
                  padding: const EdgeInsets.symmetric(vertical: 3),
                  child: _Dato(clave: clave, valor: valor, enLinea: enLinea),
                ),
              const SizedBox(height: 6),
              pie,
            ],
          ),
        ),
      ),
    );
  }
}

/// Una fila «concepto → lo que cuesta» de una tarjeta.
class _Dato extends StatelessWidget {
  const _Dato({
    required this.clave,
    required this.valor,
    required this.enLinea,
  });

  final String clave;
  final String valor;
  final bool enLinea;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final etiqueta = Text(
      clave,
      style: TextStyle(color: scheme.onSurfaceVariant, fontSize: 11),
    );
    final dato = Text(
      valor,
      textAlign: enLinea ? TextAlign.right : TextAlign.left,
      style: TextStyle(
        color: scheme.onSurface,
        fontSize: 11,
        fontWeight: FontWeight.w600,
      ),
    );

    if (!enLinea) {
      return Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [etiqueta, dato],
      );
    }
    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Expanded(child: etiqueta),
        const SizedBox(width: 6),
        Flexible(child: dato),
      ],
    );
  }
}

/// Una línea de texto pequeña. Siempre una cadena NUESTRA.
class _Nota extends StatelessWidget {
  const _Nota(this.texto, {this.aviso = false});

  final String texto;
  final bool aviso;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Padding(
      padding: const EdgeInsets.only(top: 4),
      child: Text(
        texto,
        style: TextStyle(
          color: aviso ? scheme.error : scheme.onSurfaceVariant,
          fontSize: 11,
          height: 1.35,
        ),
      ),
    );
  }
}
