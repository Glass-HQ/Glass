//! CEF Request Handler
//!
//! Intercepts navigation requests. When the user cmd+clicks a link,
//! CEF calls on_open_urlfrom_tab with a non-CURRENT_TAB disposition.
//! We cancel that navigation and send a PopupRequested event so the
//! browser view opens the URL in a new tab instead.

use crate::events::{BrowserEvent, EventSender};
use cef::{
    rc::Rc as _, wrap_request_handler, Browser, ImplRequestHandler, RequestHandler,
    WindowOpenDisposition, WrapRequestHandler,
};

#[derive(Clone)]
pub struct OsrRequestHandler {
    sender: EventSender,
}

impl OsrRequestHandler {
    pub fn new(sender: EventSender) -> Self {
        Self { sender }
    }
}

wrap_request_handler! {
    pub struct RequestHandlerBuilder {
        handler: OsrRequestHandler,
    }

    impl RequestHandler {
        fn on_open_urlfrom_tab(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut cef::Frame>,
            target_url: Option<&cef::CefString>,
            target_disposition: WindowOpenDisposition,
            _user_gesture: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            if *target_disposition.as_ref() != *WindowOpenDisposition::CURRENT_TAB.as_ref() {
                if let Some(url) = target_url {
                    let url_str = url.to_string();
                    if !url_str.is_empty() {
                        let _ = self.handler.sender.send(BrowserEvent::PopupRequested(url_str));
                    }
                }
                1 // Cancel navigation in current tab
            } else {
                0 // Allow normal navigation
            }
        }
    }
}

impl RequestHandlerBuilder {
    pub fn build(handler: OsrRequestHandler) -> cef::RequestHandler {
        Self::new(handler)
    }
}
