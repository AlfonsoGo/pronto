import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../platform/global_hotkey_service.dart';

/// Teclas que son SOLO modificadores: mientras solo se pulsen estas, seguimos
/// esperando la tecla principal del atajo.
final Set<LogicalKeyboardKey> _modifierKeys = {
  LogicalKeyboardKey.controlLeft,
  LogicalKeyboardKey.controlRight,
  LogicalKeyboardKey.control,
  LogicalKeyboardKey.altLeft,
  LogicalKeyboardKey.altRight,
  LogicalKeyboardKey.alt,
  LogicalKeyboardKey.shiftLeft,
  LogicalKeyboardKey.shiftRight,
  LogicalKeyboardKey.shift,
  LogicalKeyboardKey.metaLeft,
  LogicalKeyboardKey.metaRight,
  LogicalKeyboardKey.meta,
};

/// Traduce una [LogicalKeyboardKey] al Virtual-Key Code de Windows, o null si
/// no sabemos mapearla.
int? vkFromLogicalKey(LogicalKeyboardKey key) {
  // Teclas especiales con nombre.
  final named = <LogicalKeyboardKey, int>{
    LogicalKeyboardKey.space: 0x20,
    LogicalKeyboardKey.enter: 0x0D,
    LogicalKeyboardKey.tab: 0x09,
    LogicalKeyboardKey.escape: 0x1B,
    LogicalKeyboardKey.capsLock: 0x14,
    LogicalKeyboardKey.backspace: 0x08,
    LogicalKeyboardKey.delete: 0x2E,
    LogicalKeyboardKey.insert: 0x2D,
    LogicalKeyboardKey.home: 0x24,
    LogicalKeyboardKey.end: 0x23,
    LogicalKeyboardKey.pageUp: 0x21,
    LogicalKeyboardKey.pageDown: 0x22,
    LogicalKeyboardKey.arrowLeft: 0x25,
    LogicalKeyboardKey.arrowUp: 0x26,
    LogicalKeyboardKey.arrowRight: 0x27,
    LogicalKeyboardKey.arrowDown: 0x28,
    LogicalKeyboardKey.f1: 0x70,
    LogicalKeyboardKey.f2: 0x71,
    LogicalKeyboardKey.f3: 0x72,
    LogicalKeyboardKey.f4: 0x73,
    LogicalKeyboardKey.f5: 0x74,
    LogicalKeyboardKey.f6: 0x75,
    LogicalKeyboardKey.f7: 0x76,
    LogicalKeyboardKey.f8: 0x77,
    LogicalKeyboardKey.f9: 0x78,
    LogicalKeyboardKey.f10: 0x79,
    LogicalKeyboardKey.f11: 0x7A,
    LogicalKeyboardKey.f12: 0x7B,
  };
  final mapped = named[key];
  if (mapped != null) return mapped;

  // Letras A-Z y dígitos 0-9: el VK coincide con el carácter ASCII en mayúscula.
  final label = key.keyLabel;
  if (label.length == 1) {
    final code = label.toUpperCase().codeUnitAt(0);
    final isLetter = code >= 0x41 && code <= 0x5A; // A-Z
    final isDigit = code >= 0x30 && code <= 0x39; // 0-9
    if (isLetter || isDigit) return code;
  }
  return null;
}

/// Construye un [HotkeyCombo] a partir de un evento de teclado de bajada,
/// o null si el evento es solo un modificador o una tecla no soportada.
HotkeyCombo? comboFromKeyEvent(KeyEvent event) {
  if (event is! KeyDownEvent) return null;
  if (_modifierKeys.contains(event.logicalKey)) return null;

  final vk = vkFromLogicalKey(event.logicalKey);
  if (vk == null) return null;

  final hw = HardwareKeyboard.instance;
  return HotkeyCombo(
    virtualKey: vk,
    ctrl: hw.isControlPressed,
    alt: hw.isAltPressed,
    shift: hw.isShiftPressed,
    win: hw.isMetaPressed,
  );
}

/// Campo que graba un atajo: muestra la combinación actual y, al pulsarlo,
/// captura la siguiente combinación que teclees.
class HotkeyRecorderField extends StatefulWidget {
  final HotkeyCombo value;
  final ValueChanged<HotkeyCombo> onChanged;

  const HotkeyRecorderField({
    super.key,
    required this.value,
    required this.onChanged,
  });

  @override
  State<HotkeyRecorderField> createState() => _HotkeyRecorderFieldState();
}

class _HotkeyRecorderFieldState extends State<HotkeyRecorderField> {
  final FocusNode _focus = FocusNode();
  bool _capturing = false;

  @override
  void dispose() {
    _focus.dispose();
    super.dispose();
  }

  void _start() {
    setState(() => _capturing = true);
    _focus.requestFocus();
  }

  KeyEventResult _onKey(FocusNode node, KeyEvent event) {
    // Escape cancela sin cambiar nada.
    if (event is KeyDownEvent &&
        event.logicalKey == LogicalKeyboardKey.escape) {
      setState(() => _capturing = false);
      return KeyEventResult.handled;
    }
    final combo = comboFromKeyEvent(event);
    if (combo != null) {
      widget.onChanged(combo);
      setState(() => _capturing = false);
    }
    // Tragamos todas las teclas mientras capturamos (que no escriban/desplacen).
    return KeyEventResult.handled;
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    if (_capturing) {
      return Focus(
        focusNode: _focus,
        autofocus: true,
        onKeyEvent: _onKey,
        child: Container(
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
          decoration: BoxDecoration(
            border: Border.all(color: theme.colorScheme.primary, width: 1.5),
            borderRadius: BorderRadius.circular(8),
          ),
          child: Text(
            'Pulsa la combinación…  (Esc para cancelar)',
            style: TextStyle(color: theme.colorScheme.primary),
          ),
        ),
      );
    }

    return OutlinedButton.icon(
      icon: const Icon(Icons.keyboard, size: 18),
      label: Text(widget.value.describe()),
      onPressed: _start,
    );
  }
}
