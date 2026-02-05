//! CEF Load Handler
//!
//! Tracks loading state changes and load errors.
//! URL and title tracking is handled by the DisplayHandler instead.

use crate::events::{BrowserEvent, EventSender};
use cef::{
    rc::Rc as _, wrap_load_handler, Browser, ImplLoadHandler, LoadHandler, WrapLoadHandler,
};

#[derive(Clone)]
pub struct OsrLoadHandler {
    sender: EventSender,
}

impl OsrLoadHandler {
    pub fn new(sender: EventSender) -> Self {
        log::info!("[browser::load_handler] OsrLoadHandler::new()");
        Self { sender }
    }
}

wrap_load_handler! {
    pub struct LoadHandlerBuilder {
        handler: OsrLoadHandler,
    }

    impl LoadHandler {
        fn on_loading_state_change(
            &self,
            _browser: Option<&mut Browser>,
            is_loading: ::std::os::raw::c_int,
            can_go_back: ::std::os::raw::c_int,
            can_go_forward: ::std::os::raw::c_int,
        ) {
            log::info!("[browser::load_handler] on_loading_state_change(loading={}, back={}, fwd={})",
                is_loading != 0, can_go_back != 0, can_go_forward != 0);
            let _ = self.handler.sender.send(BrowserEvent::LoadingStateChanged {
                is_loading: is_loading != 0,
                can_go_back: can_go_back != 0,
                can_go_forward: can_go_forward != 0,
            });
        }

        fn on_load_error(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut cef::Frame>,
            error_code: cef::Errorcode,
            error_text: Option<&cef::CefString>,
            failed_url: Option<&cef::CefString>,
        ) {
            let url = failed_url
                .map(|u| u.to_string())
                .unwrap_or_default();
            let text = error_text
                .map(|t| t.to_string())
                .unwrap_or_default();

            let code: &cef::sys::cef_errorcode_t = error_code.as_ref();

            log::warn!("[browser::load_handler] on_load_error(url={}, text={}, code={:?})", url, text, code);

            let _ = self.handler.sender.send(BrowserEvent::LoadError {
                url,
                error_code: *code as i32,
                error_text: text,
            });
        }
    }
}

impl LoadHandlerBuilder {
    pub fn build(handler: OsrLoadHandler) -> cef::LoadHandler {
        log::info!("[browser::load_handler] LoadHandlerBuilder::build()");
        Self::new(handler)
    }
}
