import 'package:fake_async/fake_async.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:pronto/src/features/overlay/pill_popup_controller.dart';
import 'package:pronto/src/features/translation/language_detect.dart';

// La cajita de la píldora guarda DOS textos (el pegado y el del otro idioma) y
// la traducción de cortesía llega tarde, en segundo plano. Estos tests fijan
// las reglas que evitan los dos fallos feos posibles: que se mezclen dictados
// distintos y que la caja reaparezca sola varios segundos después.

void main() {
  late ProviderContainer container;

  setUp(() => container = ProviderContainer());
  tearDown(() => container.dispose());

  PillPopup get() => container.read(pillPopupProvider);
  PillPopupController ctrl() => container.read(pillPopupProvider.notifier);

  group('estado inicial', () {
    test('empieza oculta y vacía', () {
      expect(get().visible, isFalse);
      expect(get().text, '');
      expect(get().hasAlt, isFalse);
    });
  });

  group('showTranscription', () {
    test('muestra el texto y recorta los espacios', () {
      ctrl().showTranscription('  Hola de nuevo  ', lang: TextLanguage.es);
      expect(get().visible, isTrue);
      expect(get().text, 'Hola de nuevo');
      expect(get().lang, TextLanguage.es);
      expect(get().kind, PillPopupKind.transcription);
    });

    test('un texto vacío no abre la caja', () {
      ctrl().showTranscription('   ');
      expect(get().visible, isFalse);
    });

    test('seq sube en cada aparición (re-dispara la animación)', () {
      ctrl().showTranscription('uno');
      final primero = get().seq;
      ctrl().showTranscription('dos');
      expect(get().seq, primero + 1);
    });

    test('con la traducción pendiente marca hasAlt para pintar el spinner', () {
      ctrl().showTranscription('Hola', lang: TextLanguage.es, altPending: true);
      expect(get().altPending, isTrue);
      expect(get().hasAlt, isTrue);
      expect(get().altText, '');
    });

    test('con la traducción ya hecha trae las dos versiones', () {
      ctrl().showTranscription(
        'Hi there',
        lang: TextLanguage.en,
        altText: 'Hola',
        altLang: TextLanguage.es,
      );
      expect(get().text, 'Hi there');
      expect(get().altText, 'Hola');
      expect(get().altLang, TextLanguage.es);
      expect(get().altPending, isFalse);
    });

    // El fallo que este test impide: dictar algo con traducción, dictar otra
    // cosa sin ella, y ver la traducción del dictado ANTERIOR debajo del nuevo.
    test('un dictado nuevo NO hereda la otra versión del anterior', () {
      ctrl().showTranscription(
        'Hi there',
        lang: TextLanguage.en,
        altText: 'Hola',
        altLang: TextLanguage.es,
      );
      ctrl().showTranscription('Otra cosa distinta', lang: TextLanguage.es);
      expect(get().text, 'Otra cosa distinta');
      expect(get().altText, '');
      expect(get().altLang, isNull);
      expect(get().hasAlt, isFalse);
    });
  });

  group('completeAlt (la traducción de cortesía llega tarde)', () {
    test('rellena la otra versión y quita el spinner', () {
      ctrl().showTranscription('Hola', lang: TextLanguage.es, altPending: true);
      ctrl().completeAlt('Hello', TextLanguage.en);
      expect(get().altText, 'Hello');
      expect(get().altLang, TextLanguage.en);
      expect(get().altPending, isFalse);
      expect(get().visible, isTrue);
    });

    // El fallo que este test impide: la caja ya se había ocultado y reaparece
    // sola cuando el LLM termina, tapando lo que estés haciendo.
    test('si la caja ya se ocultó NO la reabre', () {
      ctrl().showTranscription('Hola', lang: TextLanguage.es, altPending: true);
      ctrl().hide();
      ctrl().completeAlt('Hello', TextLanguage.en);
      expect(get().visible, isFalse);
      expect(get().altPending, isFalse);
    });

    test('si ya entró otro dictado no pisa su contenido', () {
      ctrl().showTranscription('Hola', lang: TextLanguage.es, altPending: true);
      ctrl().showTranscription('Segundo dictado', lang: TextLanguage.es);
      ctrl().completeAlt('Hello', TextLanguage.en);
      expect(get().text, 'Segundo dictado');
      expect(get().altText, '');
    });
  });

  group('failAlt (la traducción falló)', () {
    test('quita el spinner y deja la caja como estaba', () {
      ctrl().showTranscription('Hola', lang: TextLanguage.es, altPending: true);
      ctrl().failAlt();
      expect(get().altPending, isFalse);
      expect(get().text, 'Hola');
      expect(get().visible, isTrue);
      expect(get().hasAlt, isFalse);
    });

    test('sin traducción pendiente no cambia nada', () {
      ctrl().showTranscription('Hola', lang: TextLanguage.es);
      final antes = get();
      ctrl().failAlt();
      expect(get(), same(antes));
    });
  });

  group('showCopied', () {
    test('marca la caja como copiada conservando las dos versiones', () {
      ctrl().showCopied(
        'Hello',
        lang: TextLanguage.en,
        altText: 'Hola',
        altLang: TextLanguage.es,
      );
      expect(get().kind, PillPopupKind.copied);
      expect(get().text, 'Hello');
      expect(get().altText, 'Hola');
    });
  });

  group('showNotice', () {
    test('es un aviso sin otra versión ni acción de copiar', () {
      ctrl().showNotice('Pegar en inglés');
      expect(get().kind, PillPopupKind.notice);
      expect(get().text, 'Pegar en inglés');
      expect(get().hasAlt, isFalse);
    });

    test('borra el contenido del dictado anterior', () {
      ctrl().showTranscription(
        'Hi',
        lang: TextLanguage.en,
        altText: 'Hola',
        altLang: TextLanguage.es,
      );
      ctrl().showNotice('Pegar en español');
      expect(get().altText, '');
      expect(get().lang, isNull);
    });
  });

  group('auto-ocultado', () {
    test('se oculta sola pasado el tiempo de lectura', () {
      fakeAsync((async) {
        ctrl().showTranscription('Hola');
        expect(get().visible, isTrue);
        async.elapse(const Duration(seconds: 5));
        expect(get().visible, isFalse);
      });
    });

    test('con el ratón encima (pauseAutoHide) no se oculta', () {
      fakeAsync((async) {
        ctrl().showTranscription('Hola');
        ctrl().pauseAutoHide();
        async.elapse(const Duration(seconds: 30));
        expect(get().visible, isTrue);
      });
    });

    test('al apartar el ratón (resumeAutoHide) vuelve la cuenta atrás', () {
      fakeAsync((async) {
        ctrl().showTranscription('Hola');
        ctrl().pauseAutoHide();
        async.elapse(const Duration(seconds: 30));
        ctrl().resumeAutoHide();
        expect(get().visible, isTrue);
        async.elapse(const Duration(seconds: 5));
        expect(get().visible, isFalse);
      });
    });

    // Si la traducción tarda 3 s, no vale que la caja se cierre 1 s después:
    // no daría tiempo a leerla.
    test('completeAlt reinicia la cuenta atrás para poder leer la traducción',
        () {
      fakeAsync((async) {
        ctrl().showTranscription('Hola', altPending: true);
        async.elapse(const Duration(seconds: 3));
        ctrl().completeAlt('Hello', TextLanguage.en);
        async.elapse(const Duration(seconds: 3));
        expect(get().visible, isTrue, reason: 'aún dentro de la nueva ventana');
        async.elapse(const Duration(seconds: 2));
        expect(get().visible, isFalse);
      });
    });
  });

  group('el asistente de ESTE dictado (PRNT-18)', () {
    test('se guarda quién lo arregló y cuánto tardó', () {
      ctrl().showTranscription(
        'Hola',
        assistant: 'En tu equipo · qwen2.5:7b · 1,2 s',
      );
      expect(get().assistant, 'En tu equipo · qwen2.5:7b · 1,2 s');
    });

    test('no se hereda del dictado anterior', () {
      // El mismo fallo que ya tenían el aviso y la otra versión: un dictado
      // sin Formateo enseñando el asistente del anterior diría que corrió algo
      // que no corrió.
      ctrl().showTranscription('Uno', assistant: 'Nube · gpt-5-nano · 800 ms');
      ctrl().showTranscription('Dos');
      expect(get().assistant, '');
    });

    test('un aviso del sistema no arrastra asistente', () {
      ctrl().showTranscription('Uno', assistant: 'En tu equipo · x · 1,0 s');
      ctrl().showNotice('Ahora se pega en inglés');
      expect(get().assistant, '');
    });
  });
}
