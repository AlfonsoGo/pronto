//! Tabla de teclas del Mac (códigos `kVK_*` de `Events.h`, teclado ANSI) ↔ códigos virtuales de
//! Windows, que son los que entiende toda la lógica común de atajos (`hotkey.rs`) y de pegado
//! (`inject.rs`). Es pura y sin `cfg`: se prueba en Windows dentro de `cargo test`.
//!
//! Los códigos del Mac son de posición (la tecla física): con teclado español la tecla de la «Z»
//! sigue siendo `0x06`, como con el inglés. Para atajos y para Cmd+V/Cmd+C es lo que queremos.

use crate::hotkey::VK_FN;

/// (código de tecla del Mac, código virtual de Windows). Izquierda y derecha de cada modificador por
/// separado, como en Windows (`0xA0`/`0xA1`…).
const TABLA: &[(u16, u32)] = &[
    // Letras
    (0x00, 0x41), // A
    (0x0B, 0x42), // B
    (0x08, 0x43), // C
    (0x02, 0x44), // D
    (0x0E, 0x45), // E
    (0x03, 0x46), // F
    (0x05, 0x47), // G
    (0x04, 0x48), // H
    (0x22, 0x49), // I
    (0x26, 0x4A), // J
    (0x28, 0x4B), // K
    (0x25, 0x4C), // L
    (0x2E, 0x4D), // M
    (0x2D, 0x4E), // N
    (0x1F, 0x4F), // O
    (0x23, 0x50), // P
    (0x0C, 0x51), // Q
    (0x0F, 0x52), // R
    (0x01, 0x53), // S
    (0x11, 0x54), // T
    (0x20, 0x55), // U
    (0x09, 0x56), // V
    (0x0D, 0x57), // W
    (0x07, 0x58), // X
    (0x10, 0x59), // Y
    (0x06, 0x5A), // Z
    // Números de la fila de arriba
    (0x1D, 0x30), // 0
    (0x12, 0x31), // 1
    (0x13, 0x32), // 2
    (0x14, 0x33), // 3
    (0x15, 0x34), // 4
    (0x17, 0x35), // 5
    (0x16, 0x36), // 6
    (0x1A, 0x37), // 7
    (0x1C, 0x38), // 8
    (0x19, 0x39), // 9
    // Teclas de función
    (0x7A, 0x70), // F1
    (0x78, 0x71), // F2
    (0x63, 0x72), // F3
    (0x76, 0x73), // F4
    (0x60, 0x74), // F5
    (0x61, 0x75), // F6
    (0x62, 0x76), // F7
    (0x64, 0x77), // F8
    (0x65, 0x78), // F9
    (0x6D, 0x79), // F10
    (0x67, 0x7A), // F11
    (0x6F, 0x7B), // F12
    (0x69, 0x7C), // F13
    (0x6B, 0x7D), // F14
    (0x71, 0x7E), // F15
    (0x6A, 0x7F), // F16
    (0x40, 0x80), // F17
    (0x4F, 0x81), // F18
    (0x50, 0x82), // F19
    (0x5A, 0x83), // F20
    // Edición y movimiento
    (0x31, 0x20), // Espacio
    (0x24, 0x0D), // Return
    (0x4C, 0x0D), // Enter del teclado numérico (solo de ida: Return es el canónico)
    (0x30, 0x09), // Tab
    (0x33, 0x08), // Borrar (Backspace)
    (0x75, 0x2E), // Suprimir
    (0x35, 0x1B), // Esc
    (0x73, 0x24), // Inicio
    (0x77, 0x23), // Fin
    (0x74, 0x21), // RePág
    (0x79, 0x22), // AvPág
    (0x7B, 0x25), // ←
    (0x7E, 0x26), // ↑
    (0x7C, 0x27), // →
    (0x7D, 0x28), // ↓
    // Puntuación (posición ANSI)
    (0x32, 0xC0), // `
    (0x1B, 0xBD), // -
    (0x18, 0xBB), // =
    (0x2B, 0xBC), // ,
    (0x2F, 0xBE), // .
    (0x2C, 0xBF), // /
    (0x29, 0xBA), // ;
    (0x27, 0xDE), // '
    (0x21, 0xDB), // [
    (0x1E, 0xDD), // ]
    (0x2A, 0xDC), // \
    // Modificadores: Cmd ↔ Win, Opción ↔ Alt, Control ↔ Ctrl, Mayúsculas ↔ Shift
    (0x37, 0x5B), // Cmd izquierda
    (0x36, 0x5C), // Cmd derecha
    (0x3A, 0xA4), // Opción izquierda
    (0x3D, 0xA5), // Opción derecha
    (0x3B, 0xA2), // Control izquierda
    (0x3E, 0xA3), // Control derecha
    (0x38, 0xA0), // Mayúsculas izquierda
    (0x3C, 0xA1), // Mayúsculas derecha
];

/// Tecla Fn/Globo (`kVK_Function`).
pub const MAC_FN: u16 = 0x3F;

/// Código virtual de Windows de una tecla del Mac (`None` = tecla que Pronto no usa).
pub fn vk_from_mac(keycode: u16) -> Option<u32> {
    if keycode == MAC_FN {
        return Some(VK_FN);
    }
    TABLA.iter().find(|(m, _)| *m == keycode).map(|(_, v)| *v)
}

/// Tecla del Mac para un código virtual de Windows. Los genéricos de Windows (`0x10` Shift, `0x11`
/// Ctrl, `0x12` Alt) van a la tecla izquierda.
pub fn mac_from_vk(vk: u32) -> Option<u16> {
    let vk = match vk {
        0x10 => 0xA0,
        0x11 => 0xA2,
        0x12 => 0xA4,
        v => v,
    };
    if vk == VK_FN {
        return Some(MAC_FN);
    }
    TABLA.iter().find(|(m, v)| *v == vk && *m != 0x4C).map(|(m, _)| *m)
}

/// Bits de `CGEventFlags` (`IOLLEvent.h`): generales y de cada lado.
pub mod flags {
    pub const SHIFT: u64 = 0x0002_0000;
    pub const CONTROL: u64 = 0x0004_0000;
    pub const OPTION: u64 = 0x0008_0000;
    pub const COMMAND: u64 = 0x0010_0000;
    pub const FN: u64 = 0x0080_0000;
    pub const L_CONTROL: u64 = 0x0000_0001;
    pub const L_SHIFT: u64 = 0x0000_0002;
    pub const R_SHIFT: u64 = 0x0000_0004;
    pub const L_COMMAND: u64 = 0x0000_0008;
    pub const R_COMMAND: u64 = 0x0000_0010;
    pub const L_OPTION: u64 = 0x0000_0020;
    pub const R_OPTION: u64 = 0x0000_0040;
    pub const R_CONTROL: u64 = 0x0000_2000;
}

/// Para una tecla modificadora del Mac: (bit general, bit de su lado). `None` si no es modificadora.
pub fn modifier_bits(keycode: u16) -> Option<(u64, u64)> {
    use flags::*;
    Some(match keycode {
        0x37 => (COMMAND, L_COMMAND),
        0x36 => (COMMAND, R_COMMAND),
        0x3A => (OPTION, L_OPTION),
        0x3D => (OPTION, R_OPTION),
        0x3B => (CONTROL, L_CONTROL),
        0x3E => (CONTROL, R_CONTROL),
        0x38 => (SHIFT, L_SHIFT),
        0x3C => (SHIFT, R_SHIFT),
        MAC_FN => (FN, 0),
        _ => return None,
    })
}

/// En un evento `flagsChanged`, ¿la tecla modificadora `keycode` acaba de bajar? Se mira el bit de su
/// lado (así soltar la Cmd izquierda con la derecha pulsada cuenta como «soltar»); si el teclado no
/// da bits de lado, el general. La Fn solo tiene el general (y las flechas también lo encienden, por
/// eso se decide por el código de tecla 63 y no por el bit).
pub fn modifier_down(keycode: u16, event_flags: u64) -> Option<bool> {
    let (general, side) = modifier_bits(keycode)?;
    const SIDES: u64 = flags::L_CONTROL | flags::L_SHIFT | flags::R_SHIFT | flags::L_COMMAND | flags::R_COMMAND | flags::L_OPTION | flags::R_OPTION | flags::R_CONTROL;
    if side != 0 && event_flags & SIDES != 0 {
        Some(event_flags & side != 0)
    } else {
        Some(event_flags & general != 0)
    }
}

/// Bits de modificadores para un Cmd+V sintético, a partir de las teclas modificadoras de Windows que
/// estén pulsadas en la secuencia (`0x5B` → Cmd…). Se ponen explícitos en cada evento: así una Fn
/// que el usuario aún tenga pulsada no convierte el Cmd+V en Fn+Cmd+V.
pub fn flags_for(held_vks: &[u32]) -> u64 {
    let mut f = 0;
    for &vk in held_vks {
        f |= match vk {
            0x5B | 0x5C => flags::COMMAND,
            0xA4 | 0xA5 | 0x12 => flags::OPTION,
            0xA2 | 0xA3 | 0x11 => flags::CONTROL,
            0xA0 | 0xA1 | 0x10 => flags::SHIFT,
            _ => 0,
        };
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_tabla_no_repite_teclas_y_va_y_vuelve() {
        for (i, (m, v)) in TABLA.iter().enumerate() {
            assert!(!TABLA[..i].iter().any(|(m2, _)| m2 == m), "código del Mac repetido: {m:#X}");
            if *m != 0x4C {
                assert_eq!(mac_from_vk(*v), Some(*m), "ida y vuelta de {v:#X}");
            }
            assert_eq!(vk_from_mac(*m), Some(*v));
        }
    }

    #[test]
    fn todas_las_teclas_con_nombre_de_pronto_existen_en_el_mac() {
        // Todo lo que se puede elegir como atajo en Windows tiene tecla en el Mac.
        for vk in (0x41..=0x5A).chain(0x30..=0x39).chain(0x70..=0x83) {
            assert!(mac_from_vk(vk).is_some(), "falta {vk:#X}");
        }
        for name in ["Space", "Enter", "Tab", "Backspace", "Delete", "Home", "End", "PageUp", "PageDown", "ArrowLeft", "ArrowUp", "ArrowRight", "ArrowDown", "`", "-", "=", ",", ".", "/", ";", "'", "[", "]", "\\"] {
            let vk = crate::hotkey::vk_from_name(name).unwrap();
            assert!(mac_from_vk(vk).is_some(), "falta {name}");
        }
    }

    #[test]
    fn cmd_v_y_cmd_c_son_las_teclas_de_siempre() {
        assert_eq!(mac_from_vk(0x56), Some(0x09)); // V
        assert_eq!(mac_from_vk(0x43), Some(0x08)); // C
        assert_eq!(mac_from_vk(0x5B), Some(0x37)); // Cmd
        assert_eq!(mac_from_vk(0x0D), Some(0x24)); // Return, no el Enter numérico
        assert_eq!(flags_for(&[0x5B]), flags::COMMAND);
        assert_eq!(flags_for(&[]), 0);
    }

    #[test]
    fn la_fn_es_la_63_y_se_decide_por_su_codigo() {
        assert_eq!(vk_from_mac(63), Some(VK_FN));
        assert_eq!(mac_from_vk(VK_FN), Some(63));
        assert_eq!(modifier_down(63, flags::FN), Some(true));
        assert_eq!(modifier_down(63, 0), Some(false));
        // Una flecha enciende el bit de Fn pero no es la tecla Fn.
        assert_eq!(modifier_down(0x7B, flags::FN), None);
    }

    #[test]
    fn soltar_un_lado_con_el_otro_pulsado_cuenta_como_soltar() {
        // Cmd izquierda suelta, Cmd derecha sigue: el general sigue encendido, el de su lado no.
        assert_eq!(modifier_down(0x37, flags::COMMAND | flags::R_COMMAND), Some(false));
        assert_eq!(modifier_down(0x36, flags::COMMAND | flags::R_COMMAND), Some(true));
        // Teclado sin bits de lado: vale el general.
        assert_eq!(modifier_down(0x3A, flags::OPTION), Some(true));
        assert_eq!(modifier_down(0x3A, 0), Some(false));
    }
}
