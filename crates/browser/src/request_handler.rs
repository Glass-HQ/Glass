//! CEF Request Handler
//!
//! Allows CEF to handle non-current-tab dispositions (new tab/window) so
//! popup-based auth flows can use native opener semantics.

use cef::{
    Browser, ImplRequestHandler, RequestHandler, WindowOpenDisposition, WrapRequestHandler,
    rc::Rc as _, wrap_request_handler,
};

#[derive(Clone)]
pub struct OsrRequestHandler;

impl OsrRequestHandler {
    pub fn new() -> Self {
        Self
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
            _target_url: Option<&cef::CefString>,
            _target_disposition: WindowOpenDisposition,
            _user_gesture: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            0 // Allow CEF default handling for current and non-current dispositions.
        }
    }
}

impl RequestHandlerBuilder {
    pub fn build(handler: OsrRequestHandler) -> cef::RequestHandler {
        Self::new(handler)
    }
}
