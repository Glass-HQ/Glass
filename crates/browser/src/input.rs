//! Input Handler
//!
//! Converts GPUI input events to CEF events for browser interaction.
//! GPUI provides coordinates in logical pixels, but CEF's view_rect
//! expects logical pixels and screen_info provides the scale factor.

use crate::keycodes::{key_name_to_windows_vk, macos_keycode_to_windows_vk};
use crate::tab::BrowserTab;
use cef::{KeyEvent, KeyEventType, MouseButtonType};
use gpui::{
    Keystroke, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point,
    ScrollDelta, ScrollWheelEvent,
};

const EVENTFLAG_SHIFT_DOWN: u32 = 1 << 1;
const EVENTFLAG_CONTROL_DOWN: u32 = 1 << 2;
const EVENTFLAG_ALT_DOWN: u32 = 1 << 3;
const EVENTFLAG_LEFT_MOUSE_BUTTON: u32 = 1 << 4;
const EVENTFLAG_MIDDLE_MOUSE_BUTTON: u32 = 1 << 5;
const EVENTFLAG_RIGHT_MOUSE_BUTTON: u32 = 1 << 6;
const EVENTFLAG_COMMAND_DOWN: u32 = 1 << 7;

pub fn handle_mouse_down(browser: &BrowserTab, event: &MouseDownEvent, offset: Point<Pixels>) {
    let position = event.position - offset;
    let x = f32::from(position.x) as i32;
    let y = f32::from(position.y) as i32;
    let button = convert_mouse_button(event.button);
    let click_count = event.click_count as i32;
    let modifiers = convert_modifiers(&event.modifiers);

    browser.send_mouse_click(x, y, button, true, click_count, modifiers);
}

pub fn handle_mouse_up(browser: &BrowserTab, event: &MouseUpEvent, offset: Point<Pixels>) {
    let position = event.position - offset;
    let x = f32::from(position.x) as i32;
    let y = f32::from(position.y) as i32;
    let button = convert_mouse_button(event.button);
    let modifiers = convert_modifiers(&event.modifiers);

    browser.send_mouse_click(x, y, button, false, 1, modifiers);
}

pub fn handle_mouse_move(browser: &BrowserTab, event: &MouseMoveEvent, offset: Point<Pixels>) {
    let position = event.position - offset;
    let x = f32::from(position.x) as i32;
    let y = f32::from(position.y) as i32;
    let mut modifiers = convert_modifiers(&event.modifiers);
    modifiers |= pressed_button_flags(event.pressed_button);

    browser.send_mouse_move(x, y, false, modifiers);
}

pub fn handle_scroll_wheel(browser: &BrowserTab, event: &ScrollWheelEvent, offset: Point<Pixels>) {
    let position = event.position - offset;
    let x = f32::from(position.x) as i32;
    let y = f32::from(position.y) as i32;

    let (delta_x, delta_y) = match event.delta {
        ScrollDelta::Pixels(delta) => (f32::from(delta.x) as i32, f32::from(delta.y) as i32),
        ScrollDelta::Lines(delta) => {
            let line_height = 40;
            (
                (delta.x * line_height as f32) as i32,
                (delta.y * line_height as f32) as i32,
            )
        }
    };

    let modifiers = convert_modifiers(&event.modifiers);

    browser.send_mouse_wheel(x, y, delta_x, delta_y, modifiers);
}

/// Deferred key down handler - called outside the GPUI event handler context
/// to avoid re-entrant borrow panics when CEF triggers macOS menu checking.
pub fn handle_key_down_deferred(browser: &BrowserTab, keystroke: &Keystroke, _is_held: bool) {
    // CEF OSR on macOS doesn't have the Cocoa text input system, so editing
    // commands that macOS normally handles via `interpretKeyEvents:` selectors
    // (e.g. `deleteToBeginningOfLine:` for Cmd+Backspace) don't work.
    // Chromium's renderer-side editing behavior table doesn't map
    // Meta+VK_BACK or Meta+Shift+Arrow either — those also rely on the
    // native text system. We use Selection.modify() via JS injection instead.
    #[cfg(target_os = "macos")]
    if keystroke.modifiers.platform
        && !keystroke.modifiers.control
        && !keystroke.modifiers.alt
    {
        match keystroke.key.as_str() {
            "backspace" => {
                log::trace!("[browser::input] Cmd+Backspace -> JS select-to-line-start + delete");
                browser.execute_javascript(
                    "window.getSelection().modify('extend','backward','lineboundary');\
                     document.execCommand('delete');"
                );
                return;
            }
            "delete" => {
                log::trace!("[browser::input] Cmd+Delete -> JS select-to-line-end + forwardDelete");
                browser.execute_javascript(
                    "window.getSelection().modify('extend','forward','lineboundary');\
                     document.execCommand('forwardDelete');"
                );
                return;
            }
            _ => {}
        }
    }

    let cef_event = convert_key_event(keystroke, true);

    log::trace!(
        "[browser::input] key_down: key={:?} key_char={:?} native_key_code={:?} -> windows_vk=0x{:02X} native=0x{:02X} char=0x{:04X}",
        keystroke.key,
        keystroke.key_char,
        keystroke.native_key_code,
        cef_event.windows_key_code,
        cef_event.native_key_code,
        cef_event.character,
    );

    browser.send_key_event(&cef_event);

    // For text input, send a CHAR event after the KEYDOWN event.
    // Do NOT send CHAR events for non-character keys (enter, backspace, arrows, etc.)
    // or when platform/control modifiers are held (those are shortcuts, not text).
    let char_to_send: Option<u16> = if keystroke.modifiers.platform || keystroke.modifiers.control {
        None
    } else {
        match keystroke.key.as_str() {
            "enter" | "backspace" | "tab" | "delete" | "escape" => None,
            "space" => Some(' ' as u16),
            "left" | "right" | "up" | "down" | "home" | "end" | "pageup" | "pagedown" => None,
            "f1" | "f2" | "f3" | "f4" | "f5" | "f6" | "f7" | "f8" | "f9" | "f10" | "f11"
            | "f12" => None,
            _ => {
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
        log::trace!(
            "[browser::input] sending CHAR event: char=0x{:04X} ('{}')",
            char_code,
            char::from_u32(char_code as u32).unwrap_or('?'),
        );
        let char_event = create_char_event(char_code, &keystroke.modifiers);
        browser.send_key_event(&char_event);
    }
}

/// Deferred key up handler - called outside the GPUI event handler context.
pub fn handle_key_up_deferred(browser: &BrowserTab, keystroke: &Keystroke) {
    let cef_event = convert_key_event(keystroke, false);

    log::trace!(
        "[browser::input] key_up: key={:?} native_key_code={:?} -> windows_vk=0x{:02X}",
        keystroke.key,
        keystroke.native_key_code,
        cef_event.windows_key_code,
    );

    browser.send_key_event(&cef_event);
}

fn pressed_button_flags(pressed_button: Option<MouseButton>) -> u32 {
    match pressed_button {
        Some(MouseButton::Left) | Some(MouseButton::Navigate(_)) => EVENTFLAG_LEFT_MOUSE_BUTTON,
        Some(MouseButton::Middle) => EVENTFLAG_MIDDLE_MOUSE_BUTTON,
        Some(MouseButton::Right) => EVENTFLAG_RIGHT_MOUSE_BUTTON,
        None => 0,
    }
}

fn convert_mouse_button(button: MouseButton) -> MouseButtonType {
    match button {
        MouseButton::Left | MouseButton::Navigate(_) => MouseButtonType::LEFT,
        MouseButton::Middle => MouseButtonType::MIDDLE,
        MouseButton::Right => MouseButtonType::RIGHT,
    }
}

fn convert_key_event(keystroke: &Keystroke, is_down: bool) -> KeyEvent {
    let modifiers = convert_modifiers(&keystroke.modifiers);

    let windows_key_code = if let Some(code) = keystroke.native_key_code {
        macos_keycode_to_windows_vk(code)
    } else {
        key_name_to_windows_vk(&keystroke.key)
    };

    let native_key_code = keystroke.native_key_code.unwrap_or(0) as i32;

    let event_type = if is_down {
        KeyEventType::RAWKEYDOWN
    } else {
        KeyEventType::KEYUP
    };

    // `character`: the character with modifiers applied (what the user typed)
    // `unmodified_character`: the character without modifiers (the base key)
    let character = match keystroke.key.as_str() {
        "enter" => 0x0D,
        "backspace" => 0x08,
        "tab" => 0x09,
        "escape" => 0x1B,
        "space" => ' ' as u16,
        "delete" => 0x7F,
        _ => {
            if let Some(key_char) = &keystroke.key_char {
                key_char.chars().next().map(|c| c as u16).unwrap_or(0)
            } else if keystroke.key.len() == 1 {
                keystroke
                    .key
                    .chars()
                    .next()
                    .filter(|c| c.is_ascii_graphic())
                    .map(|c| c as u16)
                    .unwrap_or(0)
            } else {
                0
            }
        }
    };

    let unmodified_character = match keystroke.key.as_str() {
        "enter" => 0x0D,
        "backspace" => 0x08,
        "tab" => 0x09,
        "escape" => 0x1B,
        "space" => ' ' as u16,
        "delete" => 0x7F,
        key if key.len() == 1 => {
            key.chars()
                .next()
                .filter(|c| c.is_ascii_graphic())
                .map(|c| c as u16)
                .unwrap_or(0)
        }
        _ => 0,
    };

    KeyEvent {
        type_: event_type,
        modifiers,
        windows_key_code,
        native_key_code,
        is_system_key: 0,
        character,
        unmodified_character,
        focus_on_editable_field: 1,
        ..Default::default()
    }
}

fn create_char_event(char_code: u16, modifiers: &Modifiers) -> KeyEvent {
    KeyEvent {
        type_: KeyEventType::CHAR,
        modifiers: convert_modifiers(modifiers),
        windows_key_code: char_code as i32,
        character: char_code,
        unmodified_character: char_code,
        focus_on_editable_field: 1,
        ..Default::default()
    }
}

pub fn convert_modifiers(modifiers: &Modifiers) -> u32 {
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
