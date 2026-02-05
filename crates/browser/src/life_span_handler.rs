//! CEF Life Span Handler
//!
//! Handles browser lifecycle events: popup requests, browser creation,
//! and browser close. Popups are cancelled and their URL is sent as an
//! event so the tab can navigate to it instead.

use crate::events::{BrowserEvent, EventSender};
use cef::{
    rc::Rc as _, wrap_life_span_handler, Browser, ImplLifeSpanHandler, LifeSpanHandler,
    WrapLifeSpanHandler,
};

#[derive(Clone)]
pub struct OsrLifeSpanHandler {
    sender: EventSender,
}

impl OsrLifeSpanHandler {
    pub fn new(sender: EventSender) -> Self {
        log::info!("[browser::life_span_handler] OsrLifeSpanHandler::new()");
        Self { sender }
    }
}

wrap_life_span_handler! {
    pub struct LifeSpanHandlerBuilder {
        handler: OsrLifeSpanHandler,
    }

    impl LifeSpanHandler {
        fn on_before_popup(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut cef::Frame>,
            _popup_id: ::std::os::raw::c_int,
            target_url: Option<&cef::CefString>,
            _target_frame_name: Option<&cef::CefString>,
            _target_disposition: cef::WindowOpenDisposition,
            _user_gesture: ::std::os::raw::c_int,
            _popup_features: Option<&cef::PopupFeatures>,
            _window_info: Option<&mut cef::WindowInfo>,
            _client: Option<&mut Option<cef::Client>>,
            _settings: Option<&mut cef::BrowserSettings>,
            _extra_info: Option<&mut Option<cef::DictionaryValue>>,
            _no_javascript_access: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            // Cancel popup, navigate in current tab instead
            if let Some(url) = target_url {
                let url_str = url.to_string();
                log::info!("[browser::life_span_handler] on_before_popup(url={})", url_str);
                if !url_str.is_empty() {
                    let _ = self.handler.sender.send(BrowserEvent::PopupRequested(url_str));
                }
            } else {
                log::info!("[browser::life_span_handler] on_before_popup(url=None)");
            }
            1 // Return 1 to cancel the popup
        }

        fn on_after_created(&self, _browser: Option<&mut Browser>) {
            log::info!("[browser::life_span_handler] on_after_created()");
            let _ = self.handler.sender.send(BrowserEvent::BrowserCreated);
        }

        fn do_close(&self, _browser: Option<&mut Browser>) -> ::std::os::raw::c_int {
            log::info!("[browser::life_span_handler] do_close()");
            0 // Allow close
        }

        fn on_before_close(&self, _browser: Option<&mut Browser>) {
            log::info!("[browser::life_span_handler] on_before_close()");
        }
    }
}

impl LifeSpanHandlerBuilder {
    pub fn build(handler: OsrLifeSpanHandler) -> cef::LifeSpanHandler {
        log::info!("[browser::life_span_handler] LifeSpanHandlerBuilder::build()");
        Self::new(handler)
    }
}
