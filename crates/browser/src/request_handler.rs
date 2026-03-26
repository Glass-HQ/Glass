//! CEF Request Handler
//!
//! Allows CEF to handle non-current-tab dispositions (new tab/window) so
//! popup-based auth flows can use native opener semantics.

use crate::events::{BrowserEvent, EventSender, OpenDisposition, OpenTargetRequest};
use cef::{
    Browser, ImplRequestHandler, RequestHandler, WindowOpenDisposition, WrapRequestHandler,
    rc::Rc as _, wrap_request_handler,
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
            user_gesture: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            let disposition = OpenDisposition::from(target_disposition);
            let Some(_tab_target) = disposition.app_tab_target() else {
                return 0;
            };

            let Some(target_url) = target_url.map(ToString::to_string).filter(|url| !url.is_empty()) else {
                return 0;
            };

            let request = OpenTargetRequest {
                url: target_url,
                disposition,
                user_gesture: user_gesture != 0,
                is_popup_request: false,
            };

            if let Err(error) = self
                .handler
                .sender
                .send(BrowserEvent::OpenTargetRequested(request))
            {
                log::debug!("[browser] failed to send open target request: {}", error);
                return 0;
            }

            1
        }
    }
}

impl RequestHandlerBuilder {
    pub fn build(handler: OsrRequestHandler) -> cef::RequestHandler {
        Self::new(handler)
    }
}
