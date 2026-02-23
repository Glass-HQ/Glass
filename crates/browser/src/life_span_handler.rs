//! CEF Life Span Handler
//!
//! Handles browser lifecycle events: popup requests, browser creation,
//! and browser close. Popup-based login flows rely on real popup windows,
//! so we allow popup creation instead of rewriting to tab navigation.

use crate::client::ClientBuilder;
use crate::events::{BrowserEvent, EventSender};
use crate::render_handler::RenderState;
use cef::{
    Browser, ImplLifeSpanHandler, LifeSpanHandler, WrapLifeSpanHandler, rc::Rc as _,
    wrap_life_span_handler,
};
use parking_lot::Mutex;
use std::sync::Arc;

#[derive(Clone)]
pub struct OsrLifeSpanHandler {
    sender: EventSender,
}

impl OsrLifeSpanHandler {
    pub fn new(sender: EventSender) -> Self {
        Self { sender }
    }

    fn popup_client() -> cef::Client {
        let render_state = Arc::new(Mutex::new(RenderState::default()));
        let (popup_sender, _popup_receiver) = crate::events::event_channel();
        ClientBuilder::build(render_state, popup_sender)
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
            window_info: Option<&mut cef::WindowInfo>,
            client: Option<&mut Option<cef::Client>>,
            _settings: Option<&mut cef::BrowserSettings>,
            _extra_info: Option<&mut Option<cef::DictionaryValue>>,
            _no_javascript_access: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            // Ensure popup is hosted as a real window and not as an off-screen
            // rendering child tied to the parent tab's event/render pipeline.
            if let Some(window_info) = window_info {
                window_info.windowless_rendering_enabled = 0;
                window_info.shared_texture_enabled = 0;
            }

            if let Some(client) = client {
                *client = Some(OsrLifeSpanHandler::popup_client());
            }

            if let Some(url) = target_url {
                let url_string = url.to_string();
                if !url_string.is_empty() {
                    log::info!("[browser] allowing popup navigation: {}", url_string);
                }
            }

            0 // Allow popup creation.
        }

        fn on_after_created(&self, _browser: Option<&mut Browser>) {
            let _ = self.handler.sender.send(BrowserEvent::BrowserCreated);
        }

        fn do_close(&self, _browser: Option<&mut Browser>) -> ::std::os::raw::c_int {
            0 // Allow close
        }

        fn on_before_close(&self, _browser: Option<&mut Browser>) {
        }
    }
}

impl LifeSpanHandlerBuilder {
    pub fn build(handler: OsrLifeSpanHandler) -> cef::LifeSpanHandler {
        Self::new(handler)
    }
}
