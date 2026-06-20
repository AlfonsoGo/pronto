// Implementación Windows del atajo global (push-to-talk con mantener-pulsado).
//
// ─────────────────────────────────────────────────────────────────────────────
// POR QUÉ ES COMPLEJO (lee esto antes de tocar nada)
// ─────────────────────────────────────────────────────────────────────────────
// Para detectar la PULSACIÓN (down) y la SOLTADA (up) de un atajo AUNQUE la app
// no tenga el foco, en Windows hay dos caminos:
//
//  1) RegisterHotKey: sencillo, pero SOLO notifica una vez por pulsación
//     (mensaje WM_HOTKEY). No da el evento de "soltar la tecla", así que es
//     imposible implementar "mantener-pulsado" con él. Solo sirve para toggle.
//
//  2) Hook de teclado de bajo nivel WH_KEYBOARD_LL (SetWindowsHookEx): recibe
//     CADA WM_KEYDOWN/WM_KEYUP del sistema entero, con foco o sin él. Esto sí
//     permite distinguir down/up y por tanto el "mantener-pulsado".
//
// Elegimos el camino (2). Pero el hook de bajo nivel tiene un requisito duro:
// el hilo que lo instala DEBE tener un bucle de mensajes Win32
// (GetMessage/TranslateMessage/DispatchMessage) corriendo continuamente, porque
// el sistema entrega el evento "enviando un mensaje" a ese hilo. Si bloqueamos
// el hilo de UI de Flutter con GetMessage, congelamos la app. Por eso el hook
// vive en un ISOLATE DEDICADO con su propio bucle de mensajes.
//
// El callback del hook es código nativo que Windows invoca. Para que apunte a
// una función Dart usamos NativeCallable. Como el hook se ejecuta SIEMPRE en el
// mismo hilo que lo instaló (el del isolate que bombea mensajes), podemos usar
// NativeCallable.isolateLocal (el más rápido; solo es válido si la invocación
// nativa ocurre en el hilo que lo creó, que es justo nuestro caso).
//
// El callback inspecciona la tecla y los modificadores y, cuando se cumple el
// combo, manda 'down'/'up' al isolate principal por un SendPort. El servicio
// principal reexpone esos eventos como Stream<HotkeyEvent> (broadcast).
//
// Parada limpia: NO se puede parar mandando un mensaje Dart por un puerto,
// porque el hilo del isolate está BLOQUEADO dentro de GetMessage (llamada
// nativa síncrona) y su bucle de eventos Dart no corre mientras tanto. En su
// lugar, el isolate del hook nos comunica al arrancar el id de su hilo nativo;
// para pararlo, el isolate principal le envía WM_QUIT con PostThreadMessage
// (seguro entre hilos). Eso despierta GetMessage, que devuelve 0 y rompe el
// bucle; en el finally se hace UnhookWindowsHookEx y se cierra el NativeCallable.
//
// PRUEBA ON-DEVICE: este enfoque depende de que NativeCallable.isolateLocal
// funcione invocado desde el hook en el hilo del isolate. Es la práctica
// documentada, pero debe verificarse en una máquina Windows real (ver 'todos').
// ─────────────────────────────────────────────────────────────────────────────

import 'dart:async';
import 'dart:ffi';
import 'dart:isolate';

import 'package:ffi/ffi.dart';
import 'package:win32/win32.dart';

import '../global_hotkey_service.dart';

/// Tipos de mensaje que el isolate del hook envía al isolate principal.
const String _kMsgReady = 'ready'; // hook instalado correctamente
const String _kMsgError = 'error'; // fallo al instalar el hook
const String _kMsgDown = 'down'; // el combo se acaba de pulsar
const String _kMsgUp = 'up'; // el combo se acaba de soltar
const String _kMsgThreadId = 'tid'; // id del hilo nativo del isolate del hook
const String _kMsgCopy = 'copy'; // el combo de copiar se acaba de pulsar

/// Parámetros con los que arrancamos el isolate del hook.
///
/// Se pasan por valor (son tipos simples + un SendPort), no se comparten
/// objetos mutables, así evitamos problemas de concurrencia.
class _HookConfig {
  final SendPort toMain; // canal isolate-hook -> principal
  final int virtualKey;
  final bool ctrl;
  final bool alt;
  final bool shift;
  final bool win;

  /// 0 = hold, 1 = toggle. Usamos int para que sea trivial de serializar.
  final int mode;

  /// Combo opcional de COPIAR la última transcripción. [hasCopy] indica si está
  /// configurado; si lo está, se vigila también esta tecla + modificadores.
  final bool hasCopy;
  final int copyVk;
  final bool copyCtrl;
  final bool copyAlt;
  final bool copyShift;
  final bool copyWin;

  const _HookConfig({
    required this.toMain,
    required this.virtualKey,
    required this.ctrl,
    required this.alt,
    required this.shift,
    required this.win,
    required this.mode,
    this.hasCopy = false,
    this.copyVk = 0,
    this.copyCtrl = false,
    this.copyAlt = false,
    this.copyShift = false,
    this.copyWin = false,
  });
}

/// Servicio de atajo global para Windows basado en un hook de teclado de bajo
/// nivel ejecutado en un isolate dedicado con bucle de mensajes propio.
class WindowsHotkeyService implements GlobalHotkeyService {
  final StreamController<HotkeyEvent> _controller =
      StreamController<HotkeyEvent>.broadcast();

  /// Isolate que aloja el hook y el bucle de mensajes Win32.
  Isolate? _isolate;

  /// Puerto por el que recibimos eventos/avisos del isolate del hook.
  ReceivePort? _fromHook;
  StreamSubscription<dynamic>? _fromHookSub;

  /// Id del hilo NATIVO del isolate del hook. Lo necesitamos para mandarle
  /// WM_QUIT con PostThreadMessage y así despertar/terminar su bucle GetMessage.
  ///
  /// IMPORTANTE: no podemos parar el hook mandando un mensaje Dart por un
  /// SendPort, porque el hilo del isolate está BLOQUEADO dentro de GetMessage
  /// (llamada nativa síncrona) y su bucle de eventos Dart no corre. En cambio,
  /// PostThreadMessage es seguro entre hilos y SÍ despierta a GetMessage.
  int _hookThreadId = 0;

  bool _disposed = false;

  @override
  Stream<HotkeyEvent> get events => _controller.stream;

  @override
  Future<void> register(
    HotkeyCombo combo, {
    TriggerMode mode = TriggerMode.hold,
    HotkeyCombo? copyCombo,
  }) async {
    if (_disposed) {
      throw StateError('WindowsHotkeyService ya está liberado.');
    }
    // Si ya hay un atajo activo, lo retiramos antes de registrar el nuevo.
    if (_isolate != null) {
      await unregister();
    }

    final fromHook = ReceivePort();
    _fromHook = fromHook;

    // Future que se completa cuando el hook confirma que está listo (o falla).
    final readyCompleter = Completer<void>();

    _fromHookSub = fromHook.listen((dynamic message) {
      if (message is! List || message.isEmpty) return;
      final tag = message[0];

      switch (tag) {
        case _kMsgThreadId:
          // El hook nos comunica el id de su hilo nativo para poder pararlo.
          _hookThreadId = message.length > 1 ? message[1] as int : 0;
          break;
        case _kMsgReady:
          if (!readyCompleter.isCompleted) readyCompleter.complete();
          break;
        case _kMsgError:
          final detalle =
              message.length > 1 ? message[1].toString() : 'desconocido';
          if (!readyCompleter.isCompleted) {
            readyCompleter.completeError(
              StateError('No se pudo instalar el atajo global: $detalle'),
            );
          }
          break;
        case _kMsgDown:
          if (!_controller.isClosed) {
            _controller.add(const HotkeyEvent(HotkeyEventType.down));
          }
          break;
        case _kMsgUp:
          if (!_controller.isClosed) {
            _controller.add(const HotkeyEvent(HotkeyEventType.up));
          }
          break;
        case _kMsgCopy:
          if (!_controller.isClosed) {
            _controller.add(const HotkeyEvent(HotkeyEventType.copy));
          }
          break;
      }
    });

    final config = _HookConfig(
      toMain: fromHook.sendPort,
      virtualKey: combo.virtualKey,
      ctrl: combo.ctrl,
      alt: combo.alt,
      shift: combo.shift,
      win: combo.win,
      mode: mode == TriggerMode.toggle ? 1 : 0,
      hasCopy: copyCombo != null,
      copyVk: copyCombo?.virtualKey ?? 0,
      copyCtrl: copyCombo?.ctrl ?? false,
      copyAlt: copyCombo?.alt ?? false,
      copyShift: copyCombo?.shift ?? false,
      copyWin: copyCombo?.win ?? false,
    );

    _isolate = await Isolate.spawn<_HookConfig>(
      _hookIsolateEntry,
      config,
      debugName: 'Pronto-HotkeyHook',
      errorsAreFatal: true,
    );

    try {
      // Esperamos confirmación del hook con un tope de tiempo razonable.
      await readyCompleter.future.timeout(
        const Duration(seconds: 5),
        onTimeout: () => throw TimeoutException(
          'El atajo global no respondió a tiempo al instalarse.',
        ),
      );
    } catch (_) {
      // Si algo falló, dejamos el servicio en un estado limpio.
      await unregister();
      rethrow;
    }
  }

  @override
  Future<void> unregister() async {
    // Pedimos al hook que pare con elegancia: WM_QUIT a su hilo despierta
    // GetMessage, que devuelve 0 y rompe el bucle; el isolate deshace el hook
    // (UnhookWindowsHookEx) en su bloque finally y termina por sí solo.
    if (_hookThreadId != 0) {
      try {
        PostThreadMessage(_hookThreadId, WM_QUIT, 0, 0);
      } catch (_) {
        // El hilo puede haber muerto ya; lo ignoramos y matamos el isolate abajo.
      }
    }
    final tidEnviado = _hookThreadId != 0;
    _hookThreadId = 0;

    await _fromHookSub?.cancel();
    _fromHookSub = null;

    // Damos un margen breve para que el isolate procese WM_QUIT y ejecute su
    // bloque finally (UnhookWindowsHookEx + cerrar el NativeCallable). Solo
    // tiene sentido esperar si llegamos a enviar el WM_QUIT.
    if (tidEnviado) {
      await Future<void>.delayed(const Duration(milliseconds: 50));
    }

    // Garantía dura: si por lo que sea no terminó solo, lo matamos. Matar el
    // isolate NO ejecuta su finally, por eso intentamos primero el WM_QUIT y
    // esperamos un poco; esto es el último recurso.
    _isolate?.kill(priority: Isolate.beforeNextEvent);
    _isolate = null;

    _fromHook?.close();
    _fromHook = null;
  }

  @override
  Future<void> dispose() async {
    if (_disposed) return;
    _disposed = true;
    await unregister();
    if (!_controller.isClosed) {
      await _controller.close();
    }
  }
}

// ═════════════════════════════════════════════════════════════════════════════
// CÓDIGO QUE CORRE EN EL ISOLATE DEL HOOK
// ═════════════════════════════════════════════════════════════════════════════
// Todo lo que sigue se ejecuta en el isolate dedicado. No comparte estado con
// el principal salvo a través de SendPort/ReceivePort.

/// Estado vivo del isolate del hook. Lo guardamos en variables de nivel de
/// isolate para que el callback nativo (que no recibe contexto) pueda accederlo.
SendPort? _isoToMain;
int _isoHookHandle = 0;
NativeCallable<_HookProcNative>? _isoCallable;
late _HookConfig _isoConfig;

/// Estado para distinguir down/up y evitar repeticiones por autorrepetición.
bool _comboActive = false; // ¿el combo está actualmente "pulsado"?
bool _toggleOn = false; // estado interno del modo toggle.
bool _copyActive = false; // flanco del combo de copiar (una vez por pulsación).

/// Firma NATIVA del callback del hook (LowLevelKeyboardProc / HOOKPROC):
/// LRESULT CALLBACK proc(int nCode, WPARAM wParam, LPARAM lParam).
/// LRESULT/WPARAM/LPARAM son enteros del tamaño del puntero -> IntPtr.
typedef _HookProcNative = IntPtr Function(Int32 nCode, IntPtr wParam, IntPtr lParam);

/// Punto de entrada del isolate del hook.
void _hookIsolateEntry(_HookConfig config) {
  _isoConfig = config;
  _isoToMain = config.toMain;
  _comboActive = false;
  _toggleOn = false;
  _copyActive = false;

  // Creamos el callback nativo enlazado a este isolate/hilo. Es válido porque
  // Windows invocará el hook en este mismo hilo (el que bombea mensajes).
  final callable = NativeCallable<_HookProcNative>.isolateLocal(
    _lowLevelKeyboardProc,
    exceptionalReturn: 0,
  );
  _isoCallable = callable;

  // El hilo del hook DEBE poseer un bucle de mensajes. La cola de mensajes del
  // hilo se crea de forma perezosa en la primera llamada a una función de
  // mensajes; al instalar el hook y entrar en GetMessage queda garantizada.
  final hMod = GetModuleHandle(nullptr); // módulo del proceso actual.
  final hook = SetWindowsHookEx(
    WH_KEYBOARD_LL,
    callable.nativeFunction,
    hMod,
    0, // 0 = hook global (todos los hilos del escritorio actual).
  );

  if (hook == 0) {
    final err = GetLastError();
    config.toMain.send([
      _kMsgError,
      'SetWindowsHookEx falló (GetLastError=$err).',
    ]);
    callable.close();
    _isoCallable = null;
    return;
  }
  _isoHookHandle = hook;

  // Comunicamos al principal el id de NUESTRO hilo nativo: lo usará para
  // mandarnos WM_QUIT y terminar el bucle de mensajes de forma ordenada.
  config.toMain.send([_kMsgThreadId, GetCurrentThreadId()]);

  // Avisamos al principal de que ya estamos operativos.
  config.toMain.send([_kMsgReady]);

  // Bucle de mensajes Win32. Sin esto, el sistema NO entrega los eventos del
  // hook de bajo nivel. GetMessage devuelve 0 al recibir WM_QUIT y -1 si hay
  // un error real; en ambos casos salimos. Nota: esta llamada nativa BLOQUEA
  // el hilo del isolate, por eso la parada se hace con PostThreadMessage(WM_QUIT)
  // desde el isolate principal, no con un mensaje Dart por un puerto.
  final msg = calloc<MSG>();
  try {
    while (true) {
      final ret = GetMessage(msg, NULL, 0, 0);
      if (ret == 0 || ret == -1) break; // WM_QUIT o error -> fin.
      TranslateMessage(msg);
      DispatchMessage(msg);
    }
  } finally {
    calloc.free(msg);
    _teardownHook();
  }
}

/// Deshace el hook y libera el NativeCallable. Idempotente.
void _teardownHook() {
  if (_isoHookHandle != 0) {
    UnhookWindowsHookEx(_isoHookHandle);
    _isoHookHandle = 0;
  }
  _isoCallable?.close();
  _isoCallable = null;
}

/// Callback del hook de bajo nivel. Lo invoca Windows en este mismo hilo por
/// cada evento de teclado del sistema. Debe ser RÁPIDO: si tarda demasiado,
/// Windows lo elimina silenciosamente (timeout de LowLevelHooks, máx. 1 s).
int _lowLevelKeyboardProc(int nCode, int wParam, int lParam) {
  // Si nCode < 0 debemos reenviar sin procesar (regla de la API).
  if (nCode == HC_ACTION) {
    final kb = Pointer<KBDLLHOOKSTRUCT>.fromAddress(lParam).ref;
    final vk = kb.vkCode;

    final isDown = wParam == WM_KEYDOWN || wParam == WM_SYSKEYDOWN;
    final isUp = wParam == WM_KEYUP || wParam == WM_SYSKEYUP;

    // La tecla principal del combo de DICTAR.
    if (vk == _isoConfig.virtualKey) {
      if (isDown) {
        _onTriggerKeyDown();
      } else if (isUp) {
        _onTriggerKeyUp();
      }
    }

    // La tecla del combo de COPIAR (si está configurado).
    if (_isoConfig.hasCopy && vk == _isoConfig.copyVk) {
      if (isDown) {
        _onCopyKeyDown();
      } else if (isUp) {
        _onCopyKeyUp();
      }
    }
  }

  // Reenviamos SIEMPRE al siguiente hook de la cadena. No consumimos la tecla
  // (devolver no-cero la "tragaría"): queremos que el atajo siga llegando a la
  // app con foco, igual que lo haría cualquier combinación normal.
  return CallNextHookEx(0, nCode, wParam, lParam);
}

/// Gestiona la pulsación de la tecla principal del combo.
void _onTriggerKeyDown() {
  // Comprobamos los modificadores requeridos. Para los modificadores
  // (Ctrl/Alt/Shift/Win) GetAsyncKeyState SÍ es fiable dentro del hook, porque
  // la limitación documentada ("el estado asíncrono aún no está actualizado")
  // solo afecta a la tecla que está cambiando, no a las demás.
  if (!_modsMatch(_isoConfig.ctrl, _isoConfig.alt, _isoConfig.shift,
      _isoConfig.win,)) {
    return;
  }

  if (_isoConfig.mode == 1) {
    // TOGGLE: cada pulsación válida alterna. Emitimos down al activar y up al
    // desactivar. Ignoramos la autorrepetición usando el flanco.
    if (_comboActive) return; // ya estamos dentro de una pulsación física.
    _comboActive = true;
    _toggleOn = !_toggleOn;
    _send(_toggleOn ? _kMsgDown : _kMsgUp);
  } else {
    // HOLD: emitimos down una sola vez por pulsación física; la autorrepetición
    // del teclado genera más WM_KEYDOWN que ignoramos mientras siga activo.
    if (_comboActive) return;
    _comboActive = true;
    _send(_kMsgDown);
  }
}

/// Gestiona la soltada de la tecla principal del combo.
void _onTriggerKeyUp() {
  if (_isoConfig.mode == 1) {
    // TOGGLE: al soltar solo cerramos el flanco; el up/down real ya se emitió
    // en la pulsación. Así una pulsación = un cambio de estado.
    _comboActive = false;
  } else {
    // HOLD: si estábamos activos, emitimos up (fin del dictado).
    if (!_comboActive) return;
    _comboActive = false;
    _send(_kMsgUp);
  }
}

/// ¿Están pulsados exactamente los modificadores que pide el combo?
///
/// Exigimos que los modificadores requeridos estén DOWN. No forzamos que los no
/// requeridos estén UP, para no romper el dictado si el usuario tiene otra tecla
/// pulsada por casualidad; aun así, si quisiéramos un combo estricto bastaría
/// con comprobar también que los no marcados están sueltos.
bool _modsMatch(bool ctrl, bool alt, bool shift, bool win) {
  if (ctrl && !_isKeyDown(VK_CONTROL)) return false;
  if (alt && !_isKeyDown(VK_MENU)) return false;
  if (shift && !_isKeyDown(VK_SHIFT)) return false;
  if (win && !(_isKeyDown(VK_LWIN) || _isKeyDown(VK_RWIN))) return false;
  return true;
}

/// Pulsación del combo de COPIAR: emite 'copy' una sola vez por pulsación
/// física (flanco), si los modificadores coinciden.
void _onCopyKeyDown() {
  if (!_modsMatch(_isoConfig.copyCtrl, _isoConfig.copyAlt,
      _isoConfig.copyShift, _isoConfig.copyWin,)) {
    return;
  }
  if (_copyActive) return;
  _copyActive = true;
  _send(_kMsgCopy);
}

void _onCopyKeyUp() {
  _copyActive = false;
}

/// Devuelve true si la tecla [vk] está actualmente pulsada.
/// El bit alto (0x8000) de GetAsyncKeyState indica "tecla abajo ahora".
bool _isKeyDown(int vk) => (GetAsyncKeyState(vk) & 0x8000) != 0;

/// Envía un evento simple al isolate principal.
void _send(String tag) {
  _isoToMain?.send([tag]);
}
