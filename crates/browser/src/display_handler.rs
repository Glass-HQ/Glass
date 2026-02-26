//! CEF Display Handler
//!
//! Handles URL changes, title changes, loading progress, and other
//! display-related events from CEF. Sends events to the BrowserTab
//! entity via the event channel.

use crate::events::{BrowserEvent, EventSender};
use cef::{
    Browser, CefStringList, DisplayHandler, ImplDisplayHandler, WrapDisplayHandler, rc::Rc as _,
    wrap_display_handler,
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

        fn on_fullscreen_mode_change(
            &self,
            _browser: Option<&mut Browser>,
            fullscreen: ::std::os::raw::c_int,
        ) {
            // Implementing this callback prevents CEF from triggering native
            // window fullscreen when a website calls requestFullscreen().
            // The web content still renders fullscreen within the browser view.
            log::trace!(
                "[browser::display] on_fullscreen_mode_change: fullscreen={}",
                fullscreen != 0,
            );
        }

        fn on_favicon_urlchange(
            &self,
            _browser: Option<&mut Browser>,
            icon_urls: Option<&mut CefStringList>,
        ) {
            if let Some(icon_urls) = icon_urls {
                // CefStringList::clone() is broken for BorrowedMut variants (opaque pointer
                // types): the clone converts to a Borrowed variant which loses the pointer.
                // Use std::mem::take to get the original BorrowedMut, which preserves the
                // pointer and iterates correctly. The original slot gets a new empty list.
                let taken = std::mem::take(icon_urls);
                let urls: Vec<String> = taken.into_iter().collect();
                let _ = self.handler.sender.send(BrowserEvent::FaviconUrlChanged(urls));
            }
        }
    }
}

impl DisplayHandlerBuilder {
    pub fn build(handler: OsrDisplayHandler) -> cef::DisplayHandler {
        Self::new(handler)
    }
}
