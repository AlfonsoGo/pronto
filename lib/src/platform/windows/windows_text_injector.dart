// Inyección de texto en Windows mediante Win32 (FFI).
//
// Implementa [TextInjector] con dos estrategias:
//   - clipboardPaste  (por defecto): guarda el portapapeles, pega el texto con
//     Ctrl+V y restaura el portapapeles previo. Rápido y robusto para Unicode.
//   - unicodeSendInput (fallback): teclea carácter a carácter con
//     KEYEVENTF_UNICODE. No toca el portapapeles; sirve en terminales u otras
//     apps que ignoran Ctrl+V.
//
// ─────────────────────────────────────────────────────────────────────────────
// AVISO DE SEGURIDAD — UIPI (User Interface Privilege Isolation)
// ─────────────────────────────────────────────────────────────────────────────
// SendInput está sujeto a UIPI: un proceso solo puede inyectar entrada en otro
// proceso de nivel de integridad IGUAL o INFERIOR. Si la aplicación con foco se
// ejecuta ELEVADA (como administrador) y Pronto NO, la inyección FALLA EN
// SILENCIO: ni el valor de retorno de SendInput ni GetLastError indican que el
// bloqueo se debió a UIPI (confirmado en la documentación oficial de winuser.h).
// Esto afecta tanto a Ctrl+V (las pulsaciones no llegan a la app elevada) como a
// KEYEVENTF_UNICODE.
//
// En la v1 NO resolvemos esto: no elevamos Pronto ni usamos el truco del
// manifiesto `uiAccess=true` (que exige firma de código y ubicación en
// Program Files). Si el texto "no aparece" en una app concreta, la causa más
// probable es que esa app corre elevada. La mitigación queda anotada en
// ROADMAP.md / 'todos'.
// ─────────────────────────────────────────────────────────────────────────────

import 'dart:ffi';

import 'package:ffi/ffi.dart';
import 'package:win32/win32.dart';

import '../text_injector.dart';

/// Implementación de [TextInjector] para Windows usando Win32 vía FFI.
class WindowsTextInjector implements TextInjector {
  /// Crea un inyector de texto para Windows.
  const WindowsTextInjector();

  /// Tiempo de espera tras enviar Ctrl+V antes de restaurar el portapapeles.
  ///
  /// Da margen a que la app con foco procese el WM_PASTE y lea el contenido
  /// del portapapeles antes de que lo sobreescribamos con el valor previo.
  static const Duration _pasteSettleDelay = Duration(milliseconds: 150);

  @override
  Future<void> insert(
    String text, {
    InjectionMode mode = InjectionMode.clipboardPaste,
  }) async {
    // Nada que insertar: evitamos tocar el portapapeles o el teclado.
    if (text.isEmpty) return;

    switch (mode) {
      case InjectionMode.clipboardPaste:
        await _insertViaClipboard(text);
      case InjectionMode.unicodeSendInput:
        _insertViaUnicodeSendInput(text);
    }
  }

  // ───────────────────────────────────────────────────────────────────────────
  // Estrategia 1: portapapeles + Ctrl+V (por defecto)
  // ───────────────────────────────────────────────────────────────────────────

  /// Coloca [text] en el portapapeles, simula Ctrl+V y restaura el contenido
  /// anterior del portapapeles (solo texto Unicode, CF_UNICODETEXT).
  Future<void> _insertViaClipboard(String text) async {
    // 1) Leemos y guardamos el texto que hubiera en el portapapeles para luego
    //    restaurarlo. Puede ser null si no había texto Unicode (p. ej. una
    //    imagen, o el portapapeles vacío); en ese caso no restauramos nada.
    final String? portapapelesPrevio = _leerTextoDelPortapapeles();

    // 2) Escribimos nuestro texto en el portapapeles.
    final bool escrito = _escribirTextoEnPortapapeles(text);
    if (!escrito) {
      throw const TextInjectionException(
        'No se pudo escribir el texto en el portapapeles de Windows.',
      );
    }

    // 3) Simulamos Ctrl+V para pegar en la app con foco.
    //    Si esto falla en silencio, la causa más probable es UIPI (ver aviso
    //    de cabecera): la app de destino corre elevada.
    _enviarCtrlV();

    // 4) Esperamos a que la app procese el pegado antes de restaurar.
    await Future<void>.delayed(_pasteSettleDelay);

    // 5) Restauramos el portapapeles previo (si lo había). Si no lo había,
    //    dejamos nuestro texto: vaciar el portapapeles aquí podría sorprender
    //    al usuario más que conservar el último texto pegado.
    if (portapapelesPrevio != null) {
      _escribirTextoEnPortapapeles(portapapelesPrevio);
    }
  }

  /// Lee el texto (CF_UNICODETEXT) actual del portapapeles, o `null` si no hay.
  String? _leerTextoDelPortapapeles() {
    // OpenClipboard(0) => sin ventana propietaria asociada.
    // Devuelve 0 (FALSE) si falla; comparamos contra 0 para no depender de
    // que la constante FALSE esté exportada por el paquete win32.
    if (OpenClipboard(NULL) == 0) {
      // No pudimos abrir el portapapeles (otra app lo tiene bloqueado).
      // Lo tratamos como "sin contenido previo recuperable".
      return null;
    }
    try {
      // GetClipboardData devuelve un HANDLE como int (no como Pointer en
      // win32 5.x). Si no hay datos en ese formato, devuelve NULL (0).
      final int hData = GetClipboardData(CF_UNICODETEXT);
      if (hData == NULL) return null;

      // El handle global hay que bloquearlo para obtener el puntero a la
      // cadena UTF-16 terminada en NUL. GetClipboardData devuelve el handle
      // como int, pero GlobalLock/GlobalUnlock esperan un Pointer en la versión
      // de win32 resuelta: convertimos con Pointer.fromAddress.
      final Pointer<Utf16> pData =
          GlobalLock(Pointer.fromAddress(hData)).cast<Utf16>();
      if (pData == nullptr) return null;
      try {
        return pData.toDartString();
      } finally {
        // GlobalUnlock equilibra el GlobalLock; no liberamos el handle porque
        // pertenece al portapapeles, no a nosotros.
        GlobalUnlock(Pointer.fromAddress(hData));
      }
    } finally {
      CloseClipboard();
    }
  }

  /// Escribe [text] como CF_UNICODETEXT en el portapapeles. Devuelve `true` si
  /// tuvo éxito.
  ///
  /// Reserva memoria global movible, copia el texto en UTF-16 (con NUL final) y
  /// cede el handle al portapapeles con SetClipboardData. Tras un
  /// SetClipboardData correcto, la propiedad del bloque pasa al sistema y NO
  /// debemos liberarlo nosotros.
  bool _escribirTextoEnPortapapeles(String text) {
    if (OpenClipboard(NULL) == 0) return false;
    try {
      // Vaciamos el portapapeles: pasamos a ser sus propietarios.
      if (EmptyClipboard() == 0) return false;

      // Unidades de código UTF-16 del texto + 1 para el terminador NUL.
      final List<int> codeUnits = text.codeUnits;
      final int charCount = codeUnits.length + 1;
      final int bytes = charCount * 2; // 2 bytes por unidad UTF-16.

      // GMEM_MOVEABLE es obligatorio para datos del portapapeles.
      // En la versión de win32 resuelta, GlobalAlloc devuelve un Pointer
      // (HGLOBAL) y GlobalLock/Unlock/Free + SetClipboardData operan con él.
      final Pointer<NativeType> hMem = GlobalAlloc(GMEM_MOVEABLE, bytes);
      if (hMem == nullptr) return false;

      // Bloqueamos para escribir; si falla, liberamos el bloque (aún es
      // nuestro porque todavía no lo cedimos a SetClipboardData).
      final Pointer<Uint16> dest = GlobalLock(hMem).cast<Uint16>();
      if (dest == nullptr) {
        GlobalFree(hMem);
        return false;
      }
      try {
        for (int i = 0; i < codeUnits.length; i++) {
          dest[i] = codeUnits[i];
        }
        dest[codeUnits.length] = 0; // Terminador NUL.
      } finally {
        GlobalUnlock(hMem);
      }

      // SetClipboardData espera el handle (HGLOBAL) como int: pasamos la
      // dirección del Pointer. Tras un SetClipboardData correcto la propiedad
      // del bloque pasa al sistema y NO debemos liberarlo.
      final int result = SetClipboardData(CF_UNICODETEXT, hMem.address);
      if (result == NULL) {
        // No se cedió: seguimos siendo dueños del bloque, lo liberamos.
        GlobalFree(hMem);
        return false;
      }

      // Éxito: el sistema es ahora dueño de hMem. No lo liberamos.
      return true;
    } finally {
      CloseClipboard();
    }
  }

  /// Simula la combinación Ctrl+V mediante SendInput (4 eventos:
  /// Ctrl abajo, V abajo, V arriba, Ctrl arriba).
  void _enviarCtrlV() {
    const int vKey = 0x56; // Código virtual de la tecla 'V'.

    // Reservamos 4 estructuras INPUT contiguas.
    final Pointer<INPUT> inputs = calloc<INPUT>(4);
    try {
      // [0] Ctrl abajo.
      inputs[0].type = INPUT_KEYBOARD;
      inputs[0].ki.wVk = VK_CONTROL;

      // [1] V abajo.
      inputs[1].type = INPUT_KEYBOARD;
      inputs[1].ki.wVk = vKey;

      // [2] V arriba.
      inputs[2].type = INPUT_KEYBOARD;
      inputs[2].ki.wVk = vKey;
      inputs[2].ki.dwFlags = KEYEVENTF_KEYUP;

      // [3] Ctrl arriba.
      inputs[3].type = INPUT_KEYBOARD;
      inputs[3].ki.wVk = VK_CONTROL;
      inputs[3].ki.dwFlags = KEYEVENTF_KEYUP;

      // cbSize debe ser exactamente sizeOf<INPUT>() o SendInput falla.
      final int enviados = SendInput(4, inputs, sizeOf<INPUT>());
      // Si enviados != 4 lo más probable es UIPI (app elevada). No lanzamos:
      // dejamos que el usuario lo perciba; lanzar interrumpiría el flujo de
      // restauración del portapapeles que ya hicimos en el llamador.
      if (enviados != 4) {
        // Diagnóstico no fatal; el bloqueo por UIPI no se refleja en
        // GetLastError, así que solo informamos del recuento.
        // ignore: avoid_print
        print(
          'Pronto: SendInput(Ctrl+V) insertó $enviados/4 eventos. '
          'Posible bloqueo UIPI (app con foco elevada).',
        );
      }
    } finally {
      calloc.free(inputs);
    }
  }

  // ───────────────────────────────────────────────────────────────────────────
  // Estrategia 2: SendInput con KEYEVENTF_UNICODE (fallback)
  // ───────────────────────────────────────────────────────────────────────────

  /// Teclea [text] carácter a carácter con KEYEVENTF_UNICODE.
  ///
  /// Por cada unidad de código UTF-16 se envían 2 eventos (down y up) con
  /// wVk = 0 y wScan = unidad de código. Los caracteres fuera del BMP ya vienen
  /// representados en `codeUnits` como su par suplente (high + low surrogate),
  /// por lo que se envían como dos unidades consecutivas, tal y como espera
  /// Windows. Los saltos de línea se envían como VK_RETURN (no como U+000A,
  /// que muchas apps ignorarían).
  void _insertViaUnicodeSendInput(String text) {
    // Construimos la lista de eventos: 2 por unidad de código (down/up).
    // Para '\n' usamos VK_RETURN (2 eventos) en lugar de KEYEVENTF_UNICODE.
    // '\r' se omite para no duplicar saltos en secuencias "\r\n".
    final List<int> codeUnits = text.codeUnits;

    // Pre-cálculo del número de eventos para reservar el array de una vez.
    int eventCount = 0;
    for (final int cu in codeUnits) {
      if (cu == 0x0D) continue; // '\r' ignorado.
      eventCount += 2; // down + up (sea Unicode o VK_RETURN).
    }
    if (eventCount == 0) return;

    final Pointer<INPUT> inputs = calloc<INPUT>(eventCount);
    try {
      int idx = 0;
      for (final int cu in codeUnits) {
        if (cu == 0x0D) continue; // '\r' ignorado.

        if (cu == 0x0A) {
          // Salto de línea -> tecla Intro (VK_RETURN).
          inputs[idx].type = INPUT_KEYBOARD;
          inputs[idx].ki.wVk = VK_RETURN;
          idx++;
          inputs[idx].type = INPUT_KEYBOARD;
          inputs[idx].ki.wVk = VK_RETURN;
          inputs[idx].ki.dwFlags = KEYEVENTF_KEYUP;
          idx++;
        } else {
          // Carácter Unicode (incluye unidades de pares suplentes).
          // wVk = 0, wScan = unidad de código, flag KEYEVENTF_UNICODE.
          inputs[idx].type = INPUT_KEYBOARD;
          inputs[idx].ki.wVk = 0;
          inputs[idx].ki.wScan = cu;
          inputs[idx].ki.dwFlags = KEYEVENTF_UNICODE;
          idx++;
          inputs[idx].type = INPUT_KEYBOARD;
          inputs[idx].ki.wVk = 0;
          inputs[idx].ki.wScan = cu;
          inputs[idx].ki.dwFlags = KEYEVENTF_UNICODE | KEYEVENTF_KEYUP;
          idx++;
        }
      }

      final int enviados = SendInput(eventCount, inputs, sizeOf<INPUT>());
      if (enviados != eventCount) {
        // Igual que en Ctrl+V: el fallo silencioso suele ser UIPI.
        // ignore: avoid_print
        print(
          'Pronto: SendInput(Unicode) insertó $enviados/$eventCount eventos. '
          'Posible bloqueo UIPI (app con foco elevada).',
        );
      }
    } finally {
      calloc.free(inputs);
    }
  }
}

/// Error al inyectar texto en la aplicación con foco.
class TextInjectionException implements Exception {
  /// Crea la excepción con un [mensaje] descriptivo en español.
  const TextInjectionException(this.mensaje);

  /// Descripción legible del fallo.
  final String mensaje;

  @override
  String toString() => 'TextInjectionException: $mensaje';
}
