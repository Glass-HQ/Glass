//! CEF Keyboard Handler
//!
//! Handles keyboard events for the CEF browser.
//! Now that pump_messages() is called outside of cx.update(), we can let CEF
//! process all key events normally without re-entrant borrow issues.

use cef::{
    rc::Rc as _, wrap_keyboard_handler, Browser, ImplKeyboardHandler, KeyEvent, KeyboardHandler,
    WrapKeyboardHandler,
};

#[derive(Clone)]
pub struct OsrKeyboardHandler;

impl OsrKeyboardHandler {
    pub fn new() -> Self {
        Self
    }
}

wrap_keyboard_handler! {
    pub struct KeyboardHandlerBuilder {
        handler: OsrKeyboardHandler,
    }

    impl KeyboardHandler {
        fn on_pre_key_event(
            &self,
            _browser: Option<&mut Browser>,
            _event: Option<&KeyEvent>,
            _os_event: *mut u8,
            _is_keyboard_shortcut: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            // Return 0 to let CEF handle all key events normally.
            // The re-entrant borrow issue was fixed by moving pump_messages()
            // outside of cx.update() in browser_view.rs
            0
        }
    }
}

impl KeyboardHandlerBuilder {
    pub fn build(handler: OsrKeyboardHandler) -> cef::KeyboardHandler {
        Self::new(handler)
    }
}
