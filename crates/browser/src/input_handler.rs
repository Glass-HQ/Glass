//! Input Handler
//!
//! Converts GPUI input events to CEF events for browser interaction.
//! GPUI provides coordinates in logical pixels, but CEF's view_rect
//! expects logical pixels and screen_info provides the scale factor.

use crate::cef_browser::{CefBrowser, CefKeyEvent, MouseButton as CefMouseButton};
use cef::KeyEventType;
use gpui::{
    KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, ScrollDelta, ScrollWheelEvent,
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

pub fn handle_mouse_exit(browser: &CefBrowser) {
    browser.send_mouse_move(0, 0, true);
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

pub fn handle_key_down(browser: &CefBrowser, event: &KeyDownEvent) {
    let cef_event = convert_key_event(&event.keystroke, true, event.is_held);
    browser.send_key_event(&cef_event);

    if let Some(key_char) = &event.keystroke.key_char {
        for ch in key_char.chars() {
            let char_event = create_char_event(ch, &event.keystroke.modifiers);
            browser.send_key_event(&char_event);
        }
    }
}

pub fn handle_key_up(browser: &CefBrowser, event: &KeyUpEvent) {
    let cef_event = convert_key_event(&event.keystroke, false, false);
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

    let event_type = if is_down {
        KeyEventType::RAWKEYDOWN
    } else {
        KeyEventType::KEYUP
    };

    CefKeyEvent {
        event_type,
        modifiers,
        windows_key_code,
        native_key_code: 0,
        is_system_key: 0,
        character: 0,
        unmodified_character: 0,
        focus_on_editable_field: 0,
    }
}

fn create_char_event(ch: char, modifiers: &Modifiers) -> CefKeyEvent {
    let mods = convert_modifiers(modifiers);

    CefKeyEvent {
        event_type: KeyEventType::CHAR,
        modifiers: mods,
        windows_key_code: ch as i32,
        native_key_code: 0,
        is_system_key: 0,
        character: ch as u16,
        unmodified_character: ch as u16,
        focus_on_editable_field: 0,
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
