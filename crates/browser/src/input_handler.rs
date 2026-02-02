//! Input Handler
//!
//! Converts GPUI input events to CEF events for browser interaction.
//! GPUI provides coordinates in logical pixels, but CEF's view_rect
//! expects logical pixels and screen_info provides the scale factor.

use crate::cef_browser::{CefBrowser, CefKeyEvent, MouseButton as CefMouseButton};
use cef::KeyEventType;
use gpui::{
    Keystroke, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    Point, ScrollDelta, ScrollWheelEvent,
};

const EVENTFLAG_SHIFT_DOWN: u32 = 1 << 1;
const EVENTFLAG_CONTROL_DOWN: u32 = 1 << 2;
const EVENTFLAG_ALT_DOWN: u32 = 1 << 3;
const EVENTFLAG_COMMAND_DOWN: u32 = 1 << 7;

pub fn handle_mouse_down(browser: &CefBrowser, event: &MouseDownEvent, offset: Point<Pixels>) {
    let position = event.position - offset;
    let x = f32::from(position.x) as i32;
    let y = f32::from(position.y) as i32;
    let button = convert_mouse_button(event.button);
    let click_count = event.click_count as i32;

    browser.send_mouse_click(x, y, button, true, click_count);
}

pub fn handle_mouse_up(browser: &CefBrowser, event: &MouseUpEvent, offset: Point<Pixels>) {
    let position = event.position - offset;
    let x = f32::from(position.x) as i32;
    let y = f32::from(position.y) as i32;
    let button = convert_mouse_button(event.button);

    browser.send_mouse_click(x, y, button, false, 1);
}

pub fn handle_mouse_move(browser: &CefBrowser, event: &MouseMoveEvent, offset: Point<Pixels>) {
    let position = event.position - offset;
    let x = f32::from(position.x) as i32;
    let y = f32::from(position.y) as i32;

    browser.send_mouse_move(x, y, false);
}

pub fn handle_scroll_wheel(browser: &CefBrowser, event: &ScrollWheelEvent, offset: Point<Pixels>) {
    let position = event.position - offset;
    let x = f32::from(position.x) as i32;
    let y = f32::from(position.y) as i32;

    let (delta_x, delta_y) = match event.delta {
        ScrollDelta::Pixels(delta) => {
            (f32::from(delta.x) as i32, f32::from(delta.y) as i32)
        }
        ScrollDelta::Lines(delta) => {
            let line_height = 40;
            ((delta.x * line_height as f32) as i32, (delta.y * line_height as f32) as i32)
        }
    };

    browser.send_mouse_wheel(x, y, delta_x, delta_y);
}

/// Deferred key down handler - called outside the GPUI event handler context
/// to avoid re-entrant borrow panics when CEF triggers macOS menu checking.
pub fn handle_key_down_deferred(browser: &CefBrowser, keystroke: &Keystroke, is_held: bool) {
    // Send the key down event (RAWKEYDOWN)
    let cef_event = convert_key_event(keystroke, true, is_held);
    browser.send_key_event(&cef_event);

    // For text input, send a CHAR event after the KEYDOWN event.
    // IMPORTANT: Do NOT send CHAR events for:
    // - Enter: KEYDOWN with VK_RETURN triggers form submission; CHAR would insert newline
    // - Delete: VK_DELETE is a virtual key, not a character
    // - Backspace: KEYDOWN with VK_BACK is sufficient; CHAR with 0x08 can cause issues
    // - Arrow keys, function keys, etc.: These are navigation/action keys, not text input
    let char_to_send: Option<u16> = if keystroke.modifiers.platform
        || keystroke.modifiers.control
    {
        None
    } else {
        match keystroke.key.as_str() {
            // Enter: KEYDOWN triggers form submission, no CHAR needed
            // (CHAR with 0x0D would insert a newline, which is wrong for input fields)
            "enter" => None,
            // Backspace: KEYDOWN with VK_BACK (0x08) is sufficient
            "backspace" => None,
            // Tab: KEYDOWN handles focus navigation
            "tab" => None,
            // Delete: VK_DELETE (0x2E) is a virtual key, not a character
            "delete" => None,
            // Escape: no CHAR needed
            "escape" => None,
            // Space is a printable character, send as CHAR
            "space" => Some(' ' as u16),
            // Arrow keys and other non-character keys - no CHAR event needed
            "left" | "right" | "up" | "down" | "home" | "end" | "pageup" | "pagedown" => None,
            "f1" | "f2" | "f3" | "f4" | "f5" | "f6" | "f7" | "f8" | "f9" | "f10" | "f11" | "f12" => None,
            _ => {
                // Regular text input - send CHAR event
                if let Some(key_char) = &keystroke.key_char {
                    key_char.chars().next().map(|c| c as u16)
                } else if keystroke.key.len() == 1 {
                    if let Some(ch) = keystroke.key.chars().next() {
                        if ch.is_ascii_graphic() || ch == ' ' {
                            let c = if keystroke.modifiers.shift {
                                ch.to_ascii_uppercase()
                            } else {
                                ch
                            };
                            Some(c as u16)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
        }
    };

    if let Some(char_code) = char_to_send {
        let char_event = create_char_event_from_code(char_code, &keystroke.modifiers);
        browser.send_key_event(&char_event);
    }
}

/// Deferred key up handler - called outside the GPUI event handler context.
pub fn handle_key_up_deferred(browser: &CefBrowser, keystroke: &Keystroke) {
    let cef_event = convert_key_event(keystroke, false, false);
    browser.send_key_event(&cef_event);
}

fn convert_mouse_button(button: MouseButton) -> CefMouseButton {
    match button {
        MouseButton::Left => CefMouseButton::Left,
        MouseButton::Middle => CefMouseButton::Middle,
        MouseButton::Right => CefMouseButton::Right,
        MouseButton::Navigate(_) => CefMouseButton::Left,
    }
}

fn convert_key_event(keystroke: &Keystroke, is_down: bool, _is_held: bool) -> CefKeyEvent {
    let modifiers = convert_modifiers(&keystroke.modifiers);
    let windows_key_code = key_to_windows_keycode(&keystroke.key);
    let native_key_code = key_to_macos_keycode(&keystroke.key);

    // Use RAWKEYDOWN for key press - this is what Chrome uses internally.
    // RAWKEYDOWN triggers JavaScript 'keydown' event handlers.
    let event_type = if is_down {
        KeyEventType::RAWKEYDOWN
    } else {
        KeyEventType::KEYUP
    };

    // For special keys, set the character field
    let character = match keystroke.key.as_str() {
        "enter" => 0x0D,
        "backspace" => 0x08,
        "tab" => 0x09,
        "escape" => 0x1B,
        _ => 0,
    };

    CefKeyEvent {
        event_type,
        modifiers,
        windows_key_code,
        native_key_code,
        is_system_key: 0,
        character,
        unmodified_character: character,
        focus_on_editable_field: 1,
    }
}

/// Convert key name to macOS virtual key code
fn key_to_macos_keycode(key: &str) -> i32 {
    match key {
        "a" => 0x00,
        "s" => 0x01,
        "d" => 0x02,
        "f" => 0x03,
        "h" => 0x04,
        "g" => 0x05,
        "z" => 0x06,
        "x" => 0x07,
        "c" => 0x08,
        "v" => 0x09,
        "b" => 0x0B,
        "q" => 0x0C,
        "w" => 0x0D,
        "e" => 0x0E,
        "r" => 0x0F,
        "y" => 0x10,
        "t" => 0x11,
        "1" => 0x12,
        "2" => 0x13,
        "3" => 0x14,
        "4" => 0x15,
        "6" => 0x16,
        "5" => 0x17,
        "=" => 0x18,
        "9" => 0x19,
        "7" => 0x1A,
        "-" => 0x1B,
        "8" => 0x1C,
        "0" => 0x1D,
        "]" => 0x1E,
        "o" => 0x1F,
        "u" => 0x20,
        "[" => 0x21,
        "i" => 0x22,
        "p" => 0x23,
        "enter" => 0x24,      // kVK_Return
        "l" => 0x25,
        "j" => 0x26,
        "'" => 0x27,
        "k" => 0x28,
        ";" => 0x29,
        "\\" => 0x2A,
        "," => 0x2B,
        "/" => 0x2C,
        "n" => 0x2D,
        "m" => 0x2E,
        "." => 0x2F,
        "tab" => 0x30,        // kVK_Tab
        "space" => 0x31,      // kVK_Space
        "`" => 0x32,
        "backspace" => 0x33,  // kVK_Delete (backspace on Mac)
        "escape" => 0x35,     // kVK_Escape
        "left" => 0x7B,       // kVK_LeftArrow
        "right" => 0x7C,      // kVK_RightArrow
        "down" => 0x7D,       // kVK_DownArrow
        "up" => 0x7E,         // kVK_UpArrow
        "delete" => 0x75,     // kVK_ForwardDelete
        "home" => 0x73,       // kVK_Home
        "end" => 0x77,        // kVK_End
        "pageup" => 0x74,     // kVK_PageUp
        "pagedown" => 0x79,   // kVK_PageDown
        "f1" => 0x7A,
        "f2" => 0x78,
        "f3" => 0x63,
        "f4" => 0x76,
        "f5" => 0x60,
        "f6" => 0x61,
        "f7" => 0x62,
        "f8" => 0x64,
        "f9" => 0x65,
        "f10" => 0x6D,
        "f11" => 0x67,
        "f12" => 0x6F,
        _ => 0,
    }
}

fn create_char_event_from_code(char_code: u16, modifiers: &Modifiers) -> CefKeyEvent {
    let mods = convert_modifiers(modifiers);

    CefKeyEvent {
        event_type: KeyEventType::CHAR,
        modifiers: mods,
        windows_key_code: char_code as i32,
        native_key_code: 0,
        is_system_key: 0,
        character: char_code,
        unmodified_character: char_code,
        focus_on_editable_field: 1,
    }
}

fn convert_modifiers(modifiers: &Modifiers) -> u32 {
    let mut result = 0u32;

    if modifiers.shift {
        result |= EVENTFLAG_SHIFT_DOWN;
    }
    if modifiers.control {
        result |= EVENTFLAG_CONTROL_DOWN;
    }
    if modifiers.alt {
        result |= EVENTFLAG_ALT_DOWN;
    }
    if modifiers.platform {
        #[cfg(target_os = "macos")]
        {
            result |= EVENTFLAG_COMMAND_DOWN;
        }
        #[cfg(not(target_os = "macos"))]
        {
            result |= EVENTFLAG_CONTROL_DOWN;
        }
    }

    result
}

fn key_to_windows_keycode(key: &str) -> i32 {
    match key {
        "backspace" => 0x08,
        "tab" => 0x09,
        "enter" => 0x0D,
        "shift" => 0x10,
        "control" => 0x11,
        "alt" => 0x12,
        "pause" => 0x13,
        "capslock" => 0x14,
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
        "a" => 0x41,
        "b" => 0x42,
        "c" => 0x43,
        "d" => 0x44,
        "e" => 0x45,
        "f" => 0x46,
        "g" => 0x47,
        "h" => 0x48,
        "i" => 0x49,
        "j" => 0x4A,
        "k" => 0x4B,
        "l" => 0x4C,
        "m" => 0x4D,
        "n" => 0x4E,
        "o" => 0x4F,
        "p" => 0x50,
        "q" => 0x51,
        "r" => 0x52,
        "s" => 0x53,
        "t" => 0x54,
        "u" => 0x55,
        "v" => 0x56,
        "w" => 0x57,
        "x" => 0x58,
        "y" => 0x59,
        "z" => 0x5A,
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
        ";" | ":" => 0xBA,
        "=" | "+" => 0xBB,
        "," | "<" => 0xBC,
        "-" | "_" => 0xBD,
        "." | ">" => 0xBE,
        "/" | "?" => 0xBF,
        "`" | "~" => 0xC0,
        "[" | "{" => 0xDB,
        "\\" | "|" => 0xDC,
        "]" | "}" => 0xDD,
        "'" | "\"" => 0xDE,
        _ => {
            if let Some(ch) = key.chars().next() {
                if ch.is_ascii_uppercase() {
                    ch as i32
                } else if ch.is_ascii_lowercase() {
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
