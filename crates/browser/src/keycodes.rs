//! Keycode Mapping Tables
//!
//! Converts macOS virtual keycodes (CGKeyCode) to Windows virtual key codes
//! used by CEF. Based on Chromium's keyboard_code_conversion_mac.mm — a stable
//! 1:1 hardware mapping that is independent of keyboard layout.

/// Convert a macOS virtual keycode (CGKeyCode) to a Windows virtual key code.
///
/// Based on Chromium's `KeyboardCodeFromKeyCode()` in
/// `keyboard_code_conversion_mac.mm`. This is a physical-key mapping and
/// does not depend on the active keyboard layout.
pub fn macos_keycode_to_windows_vk(code: u16) -> i32 {
    match code {
        // Row: letters and numbers (ANSI layout order)
        0x00 => 0x41, // kVK_ANSI_A -> VK_A
        0x01 => 0x53, // kVK_ANSI_S -> VK_S
        0x02 => 0x44, // kVK_ANSI_D -> VK_D
        0x03 => 0x46, // kVK_ANSI_F -> VK_F
        0x04 => 0x48, // kVK_ANSI_H -> VK_H
        0x05 => 0x47, // kVK_ANSI_G -> VK_G
        0x06 => 0x5A, // kVK_ANSI_Z -> VK_Z
        0x07 => 0x58, // kVK_ANSI_X -> VK_X
        0x08 => 0x43, // kVK_ANSI_C -> VK_C
        0x09 => 0x56, // kVK_ANSI_V -> VK_V
        0x0A => 0xC0, // kVK_ISO_Section -> VK_OEM_3 (`)
        0x0B => 0x42, // kVK_ANSI_B -> VK_B
        0x0C => 0x51, // kVK_ANSI_Q -> VK_Q
        0x0D => 0x57, // kVK_ANSI_W -> VK_W
        0x0E => 0x45, // kVK_ANSI_E -> VK_E
        0x0F => 0x52, // kVK_ANSI_R -> VK_R
        0x10 => 0x59, // kVK_ANSI_Y -> VK_Y
        0x11 => 0x54, // kVK_ANSI_T -> VK_T
        0x12 => 0x31, // kVK_ANSI_1 -> VK_1
        0x13 => 0x32, // kVK_ANSI_2 -> VK_2
        0x14 => 0x33, // kVK_ANSI_3 -> VK_3
        0x15 => 0x34, // kVK_ANSI_4 -> VK_4
        0x16 => 0x36, // kVK_ANSI_6 -> VK_6
        0x17 => 0x35, // kVK_ANSI_5 -> VK_5
        0x18 => 0xBB, // kVK_ANSI_Equal -> VK_OEM_PLUS
        0x19 => 0x39, // kVK_ANSI_9 -> VK_9
        0x1A => 0x37, // kVK_ANSI_7 -> VK_7
        0x1B => 0xBD, // kVK_ANSI_Minus -> VK_OEM_MINUS
        0x1C => 0x38, // kVK_ANSI_8 -> VK_8
        0x1D => 0x30, // kVK_ANSI_0 -> VK_0
        0x1E => 0xDD, // kVK_ANSI_RightBracket -> VK_OEM_6
        0x1F => 0x4F, // kVK_ANSI_O -> VK_O
        0x20 => 0x55, // kVK_ANSI_U -> VK_U
        0x21 => 0xDB, // kVK_ANSI_LeftBracket -> VK_OEM_4
        0x22 => 0x49, // kVK_ANSI_I -> VK_I
        0x23 => 0x50, // kVK_ANSI_P -> VK_P
        0x25 => 0x4C, // kVK_ANSI_L -> VK_L
        0x26 => 0x4A, // kVK_ANSI_J -> VK_J
        0x27 => 0xDE, // kVK_ANSI_Quote -> VK_OEM_7
        0x28 => 0x4B, // kVK_ANSI_K -> VK_K
        0x29 => 0xBA, // kVK_ANSI_Semicolon -> VK_OEM_1
        0x2A => 0xDC, // kVK_ANSI_Backslash -> VK_OEM_5
        0x2B => 0xBC, // kVK_ANSI_Comma -> VK_OEM_COMMA
        0x2C => 0xBF, // kVK_ANSI_Slash -> VK_OEM_2
        0x2D => 0x4E, // kVK_ANSI_N -> VK_N
        0x2E => 0x4D, // kVK_ANSI_M -> VK_M
        0x2F => 0xBE, // kVK_ANSI_Period -> VK_OEM_PERIOD
        0x32 => 0xC0, // kVK_ANSI_Grave -> VK_OEM_3

        // Special keys
        0x24 => 0x0D, // kVK_Return -> VK_RETURN
        0x30 => 0x09, // kVK_Tab -> VK_TAB
        0x31 => 0x20, // kVK_Space -> VK_SPACE
        0x33 => 0x08, // kVK_Delete (backspace) -> VK_BACK
        0x35 => 0x1B, // kVK_Escape -> VK_ESCAPE
        0x37 => 0x5B, // kVK_Command -> VK_LWIN
        0x38 => 0x10, // kVK_Shift -> VK_SHIFT
        0x39 => 0x14, // kVK_CapsLock -> VK_CAPITAL
        0x3A => 0x12, // kVK_Option -> VK_MENU
        0x3B => 0x11, // kVK_Control -> VK_CONTROL
        0x3C => 0x10, // kVK_RightShift -> VK_SHIFT
        0x3D => 0x12, // kVK_RightOption -> VK_MENU
        0x3E => 0x11, // kVK_RightControl -> VK_CONTROL
        0x3F => 0x00, // kVK_Function -> (no Windows equivalent)

        // Function keys
        0x7A => 0x70, // kVK_F1 -> VK_F1
        0x78 => 0x71, // kVK_F2 -> VK_F2
        0x63 => 0x72, // kVK_F3 -> VK_F3
        0x76 => 0x73, // kVK_F4 -> VK_F4
        0x60 => 0x74, // kVK_F5 -> VK_F5
        0x61 => 0x75, // kVK_F6 -> VK_F6
        0x62 => 0x76, // kVK_F7 -> VK_F7
        0x64 => 0x77, // kVK_F8 -> VK_F8
        0x65 => 0x78, // kVK_F9 -> VK_F9
        0x6D => 0x79, // kVK_F10 -> VK_F10
        0x67 => 0x7A, // kVK_F11 -> VK_F11
        0x6F => 0x7B, // kVK_F12 -> VK_F12
        0x69 => 0x7C, // kVK_F13 -> VK_F13
        0x6B => 0x7D, // kVK_F14 -> VK_F14
        0x71 => 0x7E, // kVK_F15 -> VK_F15
        0x6A => 0x7F, // kVK_F16 -> VK_F16
        0x40 => 0x80, // kVK_F17 -> VK_F17
        0x4F => 0x81, // kVK_F18 -> VK_F18
        0x50 => 0x82, // kVK_F19 -> VK_F19
        0x5A => 0x83, // kVK_F20 -> VK_F20

        // Navigation
        0x73 => 0x24, // kVK_Home -> VK_HOME
        0x74 => 0x21, // kVK_PageUp -> VK_PRIOR
        0x75 => 0x2E, // kVK_ForwardDelete -> VK_DELETE
        0x77 => 0x23, // kVK_End -> VK_END
        0x79 => 0x22, // kVK_PageDown -> VK_NEXT
        0x7B => 0x25, // kVK_LeftArrow -> VK_LEFT
        0x7C => 0x27, // kVK_RightArrow -> VK_RIGHT
        0x7D => 0x28, // kVK_DownArrow -> VK_DOWN
        0x7E => 0x26, // kVK_UpArrow -> VK_UP

        // Numpad
        0x41 => 0x6E, // kVK_ANSI_KeypadDecimal -> VK_DECIMAL
        0x43 => 0x6A, // kVK_ANSI_KeypadMultiply -> VK_MULTIPLY
        0x45 => 0x6B, // kVK_ANSI_KeypadPlus -> VK_ADD
        0x47 => 0x90, // kVK_ANSI_KeypadClear -> VK_NUMLOCK
        0x4B => 0x6F, // kVK_ANSI_KeypadDivide -> VK_DIVIDE
        0x4C => 0x0D, // kVK_ANSI_KeypadEnter -> VK_RETURN
        0x4E => 0x6D, // kVK_ANSI_KeypadMinus -> VK_SUBTRACT
        0x51 => 0xBB, // kVK_ANSI_KeypadEquals -> VK_OEM_PLUS
        0x52 => 0x60, // kVK_ANSI_Keypad0 -> VK_NUMPAD0
        0x53 => 0x61, // kVK_ANSI_Keypad1 -> VK_NUMPAD1
        0x54 => 0x62, // kVK_ANSI_Keypad2 -> VK_NUMPAD2
        0x55 => 0x63, // kVK_ANSI_Keypad3 -> VK_NUMPAD3
        0x56 => 0x64, // kVK_ANSI_Keypad4 -> VK_NUMPAD4
        0x57 => 0x65, // kVK_ANSI_Keypad5 -> VK_NUMPAD5
        0x58 => 0x66, // kVK_ANSI_Keypad6 -> VK_NUMPAD6
        0x59 => 0x67, // kVK_ANSI_Keypad7 -> VK_NUMPAD7
        0x5B => 0x68, // kVK_ANSI_Keypad8 -> VK_NUMPAD8
        0x5C => 0x69, // kVK_ANSI_Keypad9 -> VK_NUMPAD9

        // Help/Insert
        0x72 => 0x2D, // kVK_Help -> VK_INSERT

        _ => 0,
    }
}

/// Fallback: convert a GPUI key name to a Windows virtual key code.
///
/// Used when `Keystroke::native_key_code` is `None` (synthetic/parsed keystrokes).
pub fn key_name_to_windows_vk(key: &str) -> i32 {
    match key {
        "backspace" => 0x08,
        "tab" => 0x09,
        "enter" => 0x0D,
        "shift" => 0x10,
        "control" => 0x11,
        "alt" => 0x12,
        "escape" => 0x1B,
        "space" => 0x20,
        "pageup" => 0x21,
        "pagedown" => 0x22,
        "end" => 0x23,
        "home" => 0x24,
        "left" => 0x25,
        "up" => 0x26,
        "right" => 0x27,
        "down" => 0x28,
        "insert" => 0x2D,
        "delete" => 0x2E,
        "0" => 0x30,
        "1" => 0x31,
        "2" => 0x32,
        "3" => 0x33,
        "4" => 0x34,
        "5" => 0x35,
        "6" => 0x36,
        "7" => 0x37,
        "8" => 0x38,
        "9" => 0x39,
        "f1" => 0x70,
        "f2" => 0x71,
        "f3" => 0x72,
        "f4" => 0x73,
        "f5" => 0x74,
        "f6" => 0x75,
        "f7" => 0x76,
        "f8" => 0x77,
        "f9" => 0x78,
        "f10" => 0x79,
        "f11" => 0x7A,
        "f12" => 0x7B,
        ";" => 0xBA,
        "=" => 0xBB,
        "," => 0xBC,
        "-" => 0xBD,
        "." => 0xBE,
        "/" => 0xBF,
        "`" => 0xC0,
        "[" => 0xDB,
        "\\" => 0xDC,
        "]" => 0xDD,
        "'" => 0xDE,
        _ => {
            if let Some(ch) = key.chars().next() {
                if ch.is_ascii_alphabetic() {
                    ch.to_ascii_uppercase() as i32
                } else {
                    0
                }
            } else {
                0
            }
        }
    }
}
