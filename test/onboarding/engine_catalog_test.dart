import 'package:flutter_test/flutter_test.dart';
import 'package:pronto/src/features/onboarding/engine_catalog.dart';
import 'package:pronto/src/features/settings/settings_controller.dart';

// El catálogo de Motores de voz del asistente de instalación (PRNT-13).
//
// Lo que de verdad se prueba aquí no es una ordenación: es la regla de oro #5.
// El asistente enseña barras que comparan modelos, y la trampa que hay que
// evitar —la misma que FluidVoice cayó, con sus «Speed 100 % / Acc 96 %»
// escritos a mano— es que un motor del que NO sabemos nada aparezca por delante
// de uno del que sí, dando a entender una medida que nadie hizo.

void main() {
  const conDato = SpeechEngineInfo(
    engine: SpeechEngine.parakeet,
    nombre: 'Con dato',
    idiomas: 'x',
    bytes: 1,
    nota: 'x',
    werFleursEs: 0.05,
  );
  const sinDato = SpeechEngineInfo(
    engine: SpeechEngine.whisper,
    nombre: 'Sin dato',
    idiomas: 'y',
    bytes: 1,
    nota: 'y',
  );

  group('ordenar', () {
    test('el que NO tiene cifra publicada nunca adelanta al que sí', () {
      // Con el mando en cualquier posición, y venga como venga la lista.
      for (final pref in [0.0, 0.25, 0.5, 0.75, 1.0]) {
        for (final entrada in [
          [conDato, sinDato],
          [sinDato, conDato],
        ]) {
          final r = EngineCatalog.ordenar(entrada, preferencia: pref);
          expect(
            r.first.nombre,
            'Con dato',
            reason: 'preferencia=$pref, entrada=${entrada.map((e) => e.nombre)}',
          );
        }
      }
    });

    test('más preciso (menos WER) primero cuando se pide precisión', () {
      const peor = SpeechEngineInfo(
        engine: SpeechEngine.whisper,
        nombre: 'Peor',
        idiomas: '',
        bytes: 1,
        nota: '',
        werFleursEs: 0.09,
      );
      final r = EngineCatalog.ordenar([peor, conDato], preferencia: 1);
      expect(r.map((e) => e.nombre), ['Con dato', 'Peor']);
    });

    test('con el mando en rapidez pura manda el orden del catálogo', () {
      // Sin una cifra de velocidad publicada y comparable no hay nada que
      // ordenar por rapidez: se respeta el orden de entrada en vez de
      // inventarse un ranking.
      const peor = SpeechEngineInfo(
        engine: SpeechEngine.whisper,
        nombre: 'Peor',
        idiomas: '',
        bytes: 1,
        nota: '',
        werFleursEs: 0.09,
      );
      final r = EngineCatalog.ordenar([peor, conDato], preferencia: 0);
      expect(r.map((e) => e.nombre), ['Peor', 'Con dato']);
    });

    test('preferencia fuera de rango no rompe (se recorta)', () {
      expect(
        EngineCatalog.ordenar([sinDato, conDato], preferencia: 9).first.nombre,
        'Con dato',
      );
      expect(
        EngineCatalog.ordenar([sinDato, conDato], preferencia: -3).first.nombre,
        'Con dato',
      );
    });

    test('lista vacía y lista de uno', () {
      expect(EngineCatalog.ordenar(const [], preferencia: 0.5), isEmpty);
      final uno = EngineCatalog.ordenar([sinDato], preferencia: 1);
      expect(uno.map((e) => e.nombre), ['Sin dato']);
    });

    test('no muta la lista que recibe', () {
      final entrada = [sinDato, conDato];
      EngineCatalog.ordenar(entrada, preferencia: 1);
      expect(entrada.map((e) => e.nombre), ['Sin dato', 'Con dato']);
    });
  });

  group('el catálogo real', () {
    test('Whisper NO es elegible: el pipeline nunca lo instancia', () {
      // `whisperEngineProvider` devuelve siempre ParakeetEngine y el instalador
      // no lleva whisper.dll desde la 0.7.1. Ofrecerlo sería un control inerte.
      expect(EngineCatalog.whisperSmall.seleccionable, isFalse);
      expect(EngineCatalog.whisperSmall.porQueNoSePuedeUsar, isNotNull);
      expect(
        EngineCatalog.elegibles.map((e) => e.engine),
        [SpeechEngine.parakeet],
      );
    });

    test('toda cifra publicada viene con fuente y fecha', () {
      for (final motor in EngineCatalog.todos) {
        if (motor.werFleursEs == null) continue;
        expect(motor.fuente, isNotNull, reason: motor.nombre);
        expect(motor.fuenteConsultada, isNotNull, reason: motor.nombre);
      }
    });

    test('of() encuentra la ficha de cada motor conocido', () {
      for (final e in SpeechEngine.values) {
        expect(EngineCatalog.of(e).engine, e);
      }
    });
  });

  group('tamano', () {
    test('por debajo de 1 GiB se dice en MB', () {
      const m = SpeechEngineInfo(
        engine: SpeechEngine.parakeet,
        nombre: '',
        idiomas: '',
        bytes: 500 * 1024 * 1024,
        nota: '',
      );
      expect(m.tamano, '500 MB');
    });

    test('a partir de 1 GiB se dice en GB con coma decimal (es-ES)', () {
      const m = SpeechEngineInfo(
        engine: SpeechEngine.parakeet,
        nombre: '',
        idiomas: '',
        bytes: 1288490188, // 1,2 GiB
        nota: '',
      );
      expect(m.tamano, '1,2 GB');
    });

    test('cero bytes no revienta', () {
      const m = SpeechEngineInfo(
        engine: SpeechEngine.parakeet,
        nombre: '',
        idiomas: '',
        bytes: 0,
        nota: '',
      );
      expect(m.tamano, '0 MB');
    });
  });
}
