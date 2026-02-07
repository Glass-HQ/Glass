//! Browser Mode for Glass
//!
//! This crate provides the browser mode functionality, integrating
//! Chromium Embedded Framework (CEF) for a full browser experience within Glass.

mod bookmarks;
mod browser_view;
mod cef_instance;
mod client;
mod context_menu_handler;
mod display_handler;
mod events;
mod history;
mod input;
mod keycodes;
mod life_span_handler;
mod load_handler;
#[cfg(target_os = "macos")]
mod macos_protocol;
mod new_tab_page;
mod omnibox;
mod permission_handler;
mod render_handler;
mod request_handler;
mod session;
mod tab;
mod toolbar;

pub use browser_view::BrowserView;
pub use cef_instance::CefInstance;
pub use tab::BrowserTab;

/// Handle CEF subprocess execution. This MUST be called very early in main(),
/// before any GUI initialization. See CefInstance::handle_subprocess() for details.
pub fn handle_cef_subprocess() -> anyhow::Result<()> {
    CefInstance::handle_subprocess()
}

use gpui::{App, AppContext as _, Focusable};
use std::sync::Arc;
use workspace_modes::{ModeId, ModeViewRegistry, RegisteredModeView};

pub fn init(cx: &mut App) {
    match CefInstance::initialize(cx) {
        Ok(_) => {}
        Err(e) => {
            log::error!("[browser] init() Failed to initialize CEF: {}. Browser mode will show placeholder.", e);
        }
    }

    ModeViewRegistry::global_mut(cx).register_factory(
        ModeId::BROWSER,
        Arc::new(|cx: &mut App| {
            let browser_view = cx.new(|cx| BrowserView::new(cx));
            let focus_handle = browser_view.focus_handle(cx);
            RegisteredModeView {
                view: browser_view.into(),
                focus_handle,
                titlebar_center_view: None,
            }
        }),
    );
}
