//! CEF Display Handler
//!
//! Handles URL changes, title changes, loading progress, and other
//! display-related events from CEF. Sends events to the BrowserTab
//! entity via the event channel.

use crate::events::{BrowserEvent, EventSender};
use cef::{
    rc::Rc as _, wrap_display_handler, Browser, DisplayHandler, ImplDisplayHandler,
    WrapDisplayHandler,
};

#[derive(Clone)]
pub struct OsrDisplayHandler {
    sender: EventSender,
}

impl OsrDisplayHandler {
    pub fn new(sender: EventSender) -> Self {
        Self { sender }
    }
}

wrap_display_handler! {
    pub struct DisplayHandlerBuilder {
        handler: OsrDisplayHandler,
    }

    impl DisplayHandler {
        fn on_address_change(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut cef::Frame>,
            url: Option<&cef::CefString>,
        ) {
            if let Some(url) = url {
                let url_str = url.to_string();
                if !url_str.is_empty() {
                    let _ = self.handler.sender.send(BrowserEvent::AddressChanged(url_str));
                }
            }
        }

        fn on_title_change(
            &self,
            _browser: Option<&mut Browser>,
            title: Option<&cef::CefString>,
        ) {
            if let Some(title) = title {
                let title_str = title.to_string();
                let _ = self.handler.sender.send(BrowserEvent::TitleChanged(title_str));
            }
        }

        fn on_loading_progress_change(
            &self,
            _browser: Option<&mut Browser>,
            progress: f64,
        ) {
            let _ = self.handler.sender.send(BrowserEvent::LoadingProgress(progress));
        }
    }
}

impl DisplayHandlerBuilder {
    pub fn build(handler: OsrDisplayHandler) -> cef::DisplayHandler {
        Self::new(handler)
    }
}
