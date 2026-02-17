//! CEF Find Handler
//!
//! Receives in-page find results from CEF and forwards them to BrowserTab.

use crate::events::{BrowserEvent, EventSender, FindResultEvent};
use cef::{Browser, FindHandler, ImplFindHandler, WrapFindHandler, rc::Rc as _, wrap_find_handler};

#[derive(Clone)]
pub struct OsrFindHandler {
    sender: EventSender,
}

impl OsrFindHandler {
    pub fn new(sender: EventSender) -> Self {
        Self { sender }
    }
}

wrap_find_handler! {
    pub struct FindHandlerBuilder {
        handler: OsrFindHandler,
    }

    impl FindHandler {
        fn on_find_result(
            &self,
            _browser: Option<&mut Browser>,
            identifier: ::std::os::raw::c_int,
            count: ::std::os::raw::c_int,
            _selection_rect: Option<&cef::Rect>,
            active_match_ordinal: ::std::os::raw::c_int,
            final_update: ::std::os::raw::c_int,
        ) {
            if let Err(error) = self
                .handler
                .sender
                .send(BrowserEvent::FindResult(FindResultEvent {
                identifier,
                count,
                active_match_ordinal,
                final_update: final_update != 0,
            }))
            {
                log::debug!("[browser] failed to send find result event: {}", error);
            }
        }
    }
}

impl FindHandlerBuilder {
    pub fn build(handler: OsrFindHandler) -> cef::FindHandler {
        Self::new(handler)
    }
}
