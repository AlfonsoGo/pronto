/// Tipo de evento del atajo global.
enum HotkeyEventType {
  /// La tecla/atajo se acaba de pulsar (empieza el dictado en push-to-talk).
  down,

  /// La tecla/atajo se acaba de soltar (termina el dictado en push-to-talk).
  up,

  /// El atajo de COPIAR (distinto del de dictar) se acaba de pulsar.
  copy,
}

class HotkeyEvent {
  final HotkeyEventType type;
  const HotkeyEvent(this.type);
}

/// Combinación de teclas del atajo global.
///
/// [virtualKey] es un Virtual-Key Code de Windows (p.ej. 0x73 = F4).
class HotkeyCombo {
  final int virtualKey;
  final bool ctrl;
  final bool alt;
  final bool shift;
  final bool win;

  const HotkeyCombo({
    required this.virtualKey,
    this.ctrl = false,
    this.alt = false,
    this.shift = false,
    this.win = false,
  });

  /// Atajo por defecto: Ctrl + Alt + Espacio (VK_SPACE = 0x20).
  static const HotkeyCombo defaultCombo =
      HotkeyCombo(virtualKey: 0x20, ctrl: true, alt: true);

  /// Atajo por defecto para COPIAR la última transcripción: Ctrl + Alt + V
  /// (VK_V = 0x56). Distinto de Ctrl + C a propósito.
  static const HotkeyCombo copyDefault =
      HotkeyCombo(virtualKey: 0x56, ctrl: true, alt: true);

  Map<String, dynamic> toJson() => {
        'vk': virtualKey,
        'ctrl': ctrl,
        'alt': alt,
        'shift': shift,
        'win': win,
      };

  factory HotkeyCombo.fromJson(Map<String, dynamic> j) => HotkeyCombo(
        virtualKey: j['vk'] as int,
        ctrl: j['ctrl'] as bool? ?? false,
        alt: j['alt'] as bool? ?? false,
        shift: j['shift'] as bool? ?? false,
        win: j['win'] as bool? ?? false,
      );

  /// Texto legible del atajo, p. ej. "Ctrl + Alt + Espacio".
  String describe() {
    final parts = <String>[
      if (ctrl) 'Ctrl',
      if (alt) 'Alt',
      if (shift) 'Shift',
      if (win) 'Win',
      vkName(virtualKey),
    ];
    return parts.join(' + ');
  }

  /// Nombre legible de un Virtual-Key Code de Windows.
  static String vkName(int vk) {
    const named = <int, String>{
      0x20: 'Espacio',
      0x0D: 'Intro',
      0x09: 'Tab',
      0x1B: 'Esc',
      0x14: 'BloqMayús',
      0x08: 'Retroceso',
      0x2D: 'Insert',
      0x2E: 'Supr',
      0x21: 'AvPág',
      0x22: 'RePág',
      0x24: 'Inicio',
      0x23: 'Fin',
      0xA0: 'ShiftIzq',
      0xA1: 'ShiftDer',
    };
    if (named.containsKey(vk)) return named[vk]!;
    if (vk >= 0x70 && vk <= 0x87) return 'F${vk - 0x6F}'; // F1..F24
    if (vk >= 0x30 && vk <= 0x5A) return String.fromCharCode(vk); // 0-9, A-Z
    return '0x${vk.toRadixString(16).toUpperCase()}';
  }

  @override
  bool operator ==(Object other) =>
      other is HotkeyCombo &&
      other.virtualKey == virtualKey &&
      other.ctrl == ctrl &&
      other.alt == alt &&
      other.shift == shift &&
      other.win == win;

  @override
  int get hashCode => Object.hash(virtualKey, ctrl, alt, shift, win);
}

/// Modo de disparo del dictado.
enum TriggerMode {
  /// Mantener pulsado para grabar, soltar para transcribir (recomendado MVP).
  hold,

  /// Pulsar para empezar, pulsar de nuevo para terminar.
  toggle,
}

/// Registra un atajo de teclado a nivel de SISTEMA (funciona aunque la app
/// no tenga el foco) y emite eventos down/up.
///
/// Implementación Windows: [WindowsHotkeyService] (low-level keyboard hook
/// en un isolate con bucle de mensajes Win32).
abstract class GlobalHotkeyService {
  /// Registra [combo] (dictado) y, opcionalmente, [copyCombo] (copiar la última
  /// transcripción). Lanza si el atajo ya está en uso por otra app.
  Future<void> register(
    HotkeyCombo combo, {
    TriggerMode mode = TriggerMode.hold,
    HotkeyCombo? copyCombo,
  });

  Future<void> unregister();

  /// Stream de eventos del atajo (down/up).
  Stream<HotkeyEvent> get events;

  Future<void> dispose();
}
