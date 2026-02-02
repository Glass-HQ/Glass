//! CEF Load Handler
//!
//! Implements CEF's LoadHandler trait to track navigation state changes
//! like is_loading, can_go_back, and can_go_forward.

use cef::{
    rc::Rc as _, wrap_load_handler, Browser, CefStringUtf16, ImplBrowser, ImplFrame,
    ImplLoadHandler, LoadHandler, WrapLoadHandler,
};
use parking_lot::Mutex;
use std::sync::Arc;

pub struct LoadState {
    pub is_loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub url: String,
    pub title: String,
}

impl Default for LoadState {
    fn default() -> Self {
        Self {
            is_loading: false,
            can_go_back: false,
            can_go_forward: false,
            url: String::from("about:blank"),
            title: String::from("New Tab"),
        }
    }
}

#[derive(Clone)]
pub struct OsrLoadHandler {
    state: Arc<Mutex<LoadState>>,
}

impl OsrLoadHandler {
    pub fn new(state: Arc<Mutex<LoadState>>) -> Self {
        Self { state }
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
            let mut state = self.handler.state.lock();
            state.is_loading = is_loading != 0;
            state.can_go_back = can_go_back != 0;
            state.can_go_forward = can_go_forward != 0;

            log::debug!(
                "Loading state changed: loading={}, back={}, forward={}",
                state.is_loading,
                state.can_go_back,
                state.can_go_forward
            );
        }

        fn on_load_start(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut cef::Frame>,
            _transition_type: cef::TransitionType,
        ) {
            if let Some(browser) = browser {
                if let Some(frame) = browser.main_frame() {
                    let url_userfree = frame.url();
                    let url_cef: CefStringUtf16 = (&url_userfree).into();
                    let url_str = url_cef.to_string();
                    if !url_str.is_empty() {
                        let mut state = self.handler.state.lock();
                        state.url = url_str.clone();
                        log::debug!("Load started: {}", url_str);
                    }
                }
            }
        }

        fn on_load_end(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut cef::Frame>,
            _http_status_code: ::std::os::raw::c_int,
        ) {
            if let Some(browser) = browser {
                if let Some(frame) = browser.main_frame() {
                    let url_userfree = frame.url();
                    let url_cef: CefStringUtf16 = (&url_userfree).into();
                    let url_str = url_cef.to_string();
                    if !url_str.is_empty() {
                        let mut state = self.handler.state.lock();
                        state.url = url_str.clone();
                        log::debug!("Load ended: {}", url_str);
                    }
                }
            }
        }
    }
}

impl LoadHandlerBuilder {
    pub fn build(handler: OsrLoadHandler) -> cef::LoadHandler {
        Self::new(handler)
    }
}
